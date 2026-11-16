//! Gli eventi dei registri di Home Assistant e quale rilettura chiedono (Task 14h).
//!
//! Sulla casa vera arrivano `*_registry_updated` ogni pochi minuti che non cambiano nulla di ciò
//! che l'app mostra (un'integrazione che riscrive versioni o capacità dei suoi dispositivi). Prima
//! ognuno faceva rileggere tutto (stati, registri, configurazione) e rifare l'iscrizione: ~3 MB
//! ricevuti per nulla. Qui si legge cosa dice l'evento e si decide.
//!
//! Forma degli eventi (documentazione di HA, «Events», https://www.home-assistant.io/docs/configuration/events/,
//! e il sorgente `homeassistant/helpers/{entity,device,area}_registry.py`):
//! - `entity_registry_updated`: `action` «create» | «remove» | «update», `entity_id`; solo per
//!   «update» `changes` (i valori **di prima** dei soli campi cambiati; HA non manda l'evento se
//!   nulla cambia) e, se l'entity_id è cambiato, `old_entity_id`.
//! - `device_registry_updated`: `action` «create» | «remove» | «update», `device_id`; per «update»
//!   `changes` (valori di prima dei campi cambiati: `sw_version`, `area_id`, `connections`…), per
//!   «remove» `device` (i dati del dispositivo tolto).
//! - `area_registry_updated`: `action` «create» | «remove» | «update» | «reorder», `area_id`
//!   (null per «reorder»); niente `changes`.
//!
//! Dei registri l'app usa (vedi `mapper`): delle entità nome, stanza, dispositivo, disattivata,
//! nascosta, categoria e classe (scelta o originale); dei dispositivi solo la stanza (le entità ne
//! ereditano l'area); delle stanze l'id e il nome.
use serde_json::Value;

use super::mapper::Indici;

/// Cosa fa l'evento sul suo registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Azione {
    Crea,
    Aggiorna,
    Togli,
    /// Solo le stanze: l'ordine delle stanze in HA (l'app ha il suo).
    Riordina,
}

/// Un evento di un registro, letto. Solo ciò che serve a decidere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CambioRegistro {
    /// `campi`: i nomi in `changes` (vuoto se non c'è); `rinominata`: c'è `old_entity_id`.
    Entita { azione: Azione, entity_id: String, campi: Vec<String>, rinominata: bool },
    Dispositivo { azione: Azione, device_id: String, campi: Vec<String> },
    Area { azione: Azione, area_id: Option<String> },
}

/// Quale lettura serve dopo un evento di un registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rilettura {
    /// Nulla che l'app mostri o segua.
    Niente,
    /// Solo i tre registri, poi la mappatura in memoria (gli stati restano quelli seguiti).
    Registri,
    /// La lettura completa: serve lo stato di un'entità che l'app non ha (entità nuova o rinominata).
    Tutto,
}

/// I campi del registro delle entità che decidono se un'entità si mostra.
const CAMPI_VISIBILITA: [&str; 5] = ["disabled_by", "hidden_by", "entity_category", "device_class", "original_device_class"];
/// I campi del registro delle entità che cambiano come un'entità mostrata appare o dove.
const CAMPI_ASPETTO: [&str; 3] = ["name", "area_id", "device_id"];
/// I domini che l'app può mostrare (gli stessi di `mapper`): gli altri non contano mai.
const DOMINI_MOSTRABILI: [&str; 13] = [
    "light", "switch", "input_boolean", "climate", "cover", "lock", "fan", "humidifier", "valve",
    "alarm_control_panel", "camera", "scene", "sensor",
];

fn dominio(entity_id: &str) -> &str {
    entity_id.split('.').next().unwrap_or_default()
}

fn azione(v: &Value) -> Option<Azione> {
    Some(match v.as_str()? {
        "create" => Azione::Crea,
        "update" => Azione::Aggiorna,
        "remove" => Azione::Togli,
        "reorder" => Azione::Riordina,
        _ => return None,
    })
}

fn campi(data: &Value) -> Vec<String> {
    data["changes"].as_object().map(|c| c.keys().cloned().collect()).unwrap_or_default()
}

/// Legge un evento del bus di un registro. `None` se non è di un registro o non si legge (chi
/// chiama allora rilegge tutto, come prima).
pub fn leggi(event_type: &str, data: &Value) -> Option<CambioRegistro> {
    let azione = azione(&data["action"])?;
    match event_type {
        "entity_registry_updated" => {
            let entity_id = data["entity_id"].as_str()?.to_owned();
            // Un aggiornamento senza `changes` non si sa cosa tocchi: non si legge.
            if azione == Azione::Aggiorna && !data["changes"].is_object() {
                return None;
            }
            let rinominata = data.get("old_entity_id").is_some_and(|v| !v.is_null());
            Some(CambioRegistro::Entita { azione, entity_id, campi: campi(data), rinominata })
        }
        "device_registry_updated" => {
            let device_id = data["device_id"].as_str()?.to_owned();
            if azione == Azione::Aggiorna && !data["changes"].is_object() {
                return None;
            }
            Some(CambioRegistro::Dispositivo { azione, device_id, campi: campi(data) })
        }
        "area_registry_updated" => {
            let area_id = data["area_id"].as_str().map(str::to_owned);
            // Solo «reorder» non ha la stanza.
            if area_id.is_none() && azione != Azione::Riordina {
                return None;
            }
            Some(CambioRegistro::Area { azione, area_id })
        }
        _ => None,
    }
}

impl CambioRegistro {
    /// La lettura che serve. `mostrata(entity_id)`: l'app mostra l'entità (anche se nascosta
    /// dall'utente: è nel catalogo delle Impostazioni) o la segue.
    pub fn rilettura(&self, indici: &Indici, mostrata: &dyn Fn(&str) -> bool) -> Rilettura {
        let tocca = |campi: &[String], quali: &[&str]| campi.iter().any(|c| quali.contains(&c.as_str()));
        match self {
            CambioRegistro::Entita { azione, entity_id, campi, rinominata } => {
                let d = dominio(entity_id);
                // Gli `update.*` contano gli aggiornamenti: uno che nasce o si riaccende ha uno
                // stato che l'app non ha (e che l'iscrizione non porta finché non lo segue).
                let aggiornamento = d == "update";
                if !aggiornamento && !DOMINI_MOSTRABILI.contains(&d) {
                    return Rilettura::Niente;
                }
                match azione {
                    Azione::Crea => Rilettura::Tutto,
                    // Il suo stato esce con `r` dall'iscrizione; i registri tolgono la sua voce.
                    Azione::Togli if mostrata(entity_id) => Rilettura::Registri,
                    Azione::Togli | Azione::Riordina => Rilettura::Niente,
                    // Rinominata: l'entity_id nuovo ha uno stato che l'app non ha.
                    Azione::Aggiorna if *rinominata => Rilettura::Tutto,
                    Azione::Aggiorna if aggiornamento => {
                        if tocca(campi, &CAMPI_VISIBILITA) { Rilettura::Tutto } else { Rilettura::Niente }
                    }
                    Azione::Aggiorna => {
                        let conta = tocca(campi, &CAMPI_VISIBILITA)
                            || (tocca(campi, &CAMPI_ASPETTO) && (mostrata(entity_id) || indici.voce(entity_id).is_none()));
                        if conta { Rilettura::Registri } else { Rilettura::Niente }
                    }
                }
            }
            CambioRegistro::Dispositivo { azione, device_id, campi } => {
                // Conta solo se ha entità mostrate che prendono la stanza da lui. Nome, versioni,
                // connessioni non li mostra nessuno; disattivarlo disattiva le sue entità, che
                // mandano il loro evento.
                let usato = || {
                    indici
                        .voci_del_dispositivo(device_id)
                        .any(|e| mostrata(&e.entity_id) && e.area_id.as_deref().is_none_or(str::is_empty))
                };
                match azione {
                    Azione::Aggiorna if tocca(campi, &["area_id"]) && usato() => Rilettura::Registri,
                    Azione::Togli if usato() => Rilettura::Registri,
                    _ => Rilettura::Niente,
                }
            }
            CambioRegistro::Area { azione, area_id } => match (azione, area_id) {
                (Azione::Aggiorna | Azione::Togli, Some(a)) if indici.ha_area(a) => Rilettura::Registri,
                _ => Rilettura::Niente,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ha::models::{HaArea, HaDevice, HaEntityEntry};
    use serde_json::json;

    fn voce(entity_id: &str, device_id: Option<&str>, area_id: Option<&str>) -> HaEntityEntry {
        HaEntityEntry {
            entity_id: entity_id.into(),
            name: None,
            platform: Some("prova".into()),
            device_id: device_id.map(Into::into),
            area_id: area_id.map(Into::into),
            disabled_by: None,
            hidden_by: None,
            icon: None,
            entity_category: None,
            device_class: None,
            original_device_class: None,
        }
    }

    fn dispositivo(id: &str, area: Option<&str>) -> HaDevice {
        HaDevice { id: id.into(), name: "D".into(), name_by_user: None, area_id: area.map(Into::into), disabled_by: None }
    }

    /// Una casa: la luce della cucina (mostrata, sul dispositivo «lampada» in cucina), un sensore
    /// di potenza (non mostrato) sul dispositivo «presa», un segnale wifi in diagnostica sul
    /// dispositivo «router», un'automazione.
    fn indici() -> Indici {
        Indici::new(
            vec![HaArea { area_id: "cucina".into(), name: "Cucina".into() }],
            vec![dispositivo("lampada", Some("cucina")), dispositivo("presa", None), dispositivo("router", None)],
            vec![
                voce("light.cucina", Some("lampada"), None),
                voce("sensor.potenza", Some("presa"), None),
                voce("sensor.wifi", Some("router"), None),
                voce("automation.sera", None, None),
            ],
        )
    }

    fn mostrata(id: &str) -> bool {
        id == "light.cucina"
    }

    fn decidi(tipo: &str, data: Value) -> Rilettura {
        match leggi(tipo, &data) {
            Some(c) => c.rilettura(&indici(), &mostrata),
            None => Rilettura::Tutto,
        }
    }

    const ENTITA: &str = "entity_registry_updated";
    const DISPOSITIVO: &str = "device_registry_updated";
    const AREA: &str = "area_registry_updated";

    #[test]
    fn eventi_letti_come_li_manda_ha() {
        let c = leggi(ENTITA, &json!({ "action": "update", "entity_id": "light.nuovo", "old_entity_id": "light.vecchio",
                                        "changes": { "entity_id": "light.vecchio" } }));
        assert_eq!(
            c,
            Some(CambioRegistro::Entita {
                azione: Azione::Aggiorna,
                entity_id: "light.nuovo".into(),
                campi: vec!["entity_id".into()],
                rinominata: true
            })
        );
        let c = leggi(DISPOSITIVO, &json!({ "action": "update", "device_id": "x", "changes": { "sw_version": "1.0" } }));
        assert_eq!(c, Some(CambioRegistro::Dispositivo { azione: Azione::Aggiorna, device_id: "x".into(), campi: vec!["sw_version".into()] }));
        assert_eq!(leggi(AREA, &json!({ "action": "reorder", "area_id": null })), Some(CambioRegistro::Area { azione: Azione::Riordina, area_id: None }));
        // Illeggibili: si rilegge tutto, come prima.
        for (tipo, data) in [
            (ENTITA, json!({ "action": "update", "entity_id": "light.cucina" })),
            (ENTITA, json!({ "action": "strana", "entity_id": "light.cucina" })),
            (ENTITA, json!({ "action": "create" })),
            (DISPOSITIVO, json!({ "action": "update", "device_id": "x" })),
            (AREA, json!({})),
        ] {
            assert_eq!(leggi(tipo, &data), None, "{tipo} {data}");
            assert_eq!(decidi(tipo, data), Rilettura::Tutto);
        }
    }

    /// I casi visti sulla casa vera: dispositivi e entità aggiornati in campi che l'app non usa.
    #[test]
    fn campi_che_l_app_non_usa_non_fanno_rileggere() {
        for campo in ["sw_version", "hw_version", "connections", "identifiers", "config_entries", "name", "name_by_user", "disabled_by"] {
            let data = json!({ "action": "update", "device_id": "lampada", "changes": { campo: "x" } });
            assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Niente, "dispositivo {campo}");
        }
        for campo in ["options", "capabilities", "supported_features", "original_name", "original_icon", "icon", "unit_of_measurement", "aliases", "labels", "translation_key", "config_entry_id"] {
            let data = json!({ "action": "update", "entity_id": "light.cucina", "changes": { campo: "x" } });
            assert_eq!(decidi(ENTITA, data), Rilettura::Niente, "entità {campo}");
        }
    }

    #[test]
    fn entita_che_non_si_mostrano_non_fanno_rileggere() {
        // Dominio che l'app non mostra mai.
        let data = json!({ "action": "update", "entity_id": "automation.sera", "changes": { "hidden_by": null } });
        assert_eq!(decidi(ENTITA, data), Rilettura::Niente);
        let data = json!({ "action": "create", "entity_id": "binary_sensor.porta" });
        assert_eq!(decidi(ENTITA, data), Rilettura::Niente);
        let data = json!({ "action": "remove", "entity_id": "sensor.potenza" });
        assert_eq!(decidi(ENTITA, data), Rilettura::Niente);
        // Non mostrata: nome e stanza non contano.
        for campo in ["name", "area_id", "device_id"] {
            let data = json!({ "action": "update", "entity_id": "sensor.potenza", "changes": { campo: null } });
            assert_eq!(decidi(ENTITA, data), Rilettura::Niente, "{campo}");
        }
        // La stanza di un dispositivo senza entità mostrate.
        let data = json!({ "action": "update", "device_id": "presa", "changes": { "area_id": null } });
        assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Niente);
        // Un dispositivo nuovo non ha ancora entità (arrivano coi loro eventi); stanze nuove o
        // riordinate in HA nemmeno.
        assert_eq!(decidi(DISPOSITIVO, json!({ "action": "create", "device_id": "nuovo" })), Rilettura::Niente);
        assert_eq!(decidi(AREA, json!({ "action": "create", "area_id": "garage" })), Rilettura::Niente);
        assert_eq!(decidi(AREA, json!({ "action": "reorder", "area_id": null })), Rilettura::Niente);
    }

    #[test]
    fn cio_che_cambia_quel_che_si_vede_rilegge_i_registri() {
        for campo in ["name", "area_id", "device_id", "hidden_by", "disabled_by", "entity_category", "device_class"] {
            let data = json!({ "action": "update", "entity_id": "light.cucina", "changes": { campo: null, "options": {} } });
            assert_eq!(decidi(ENTITA, data), Rilettura::Registri, "{campo}");
        }
        // Un'entità non mostrata che può diventarlo (nascosta, disattivata, classe cambiata).
        for campo in ["hidden_by", "disabled_by", "entity_category", "device_class", "original_device_class"] {
            let data = json!({ "action": "update", "entity_id": "sensor.potenza", "changes": { campo: "x" } });
            assert_eq!(decidi(ENTITA, data), Rilettura::Registri, "{campo}");
        }
        // Tolta un'entità mostrata.
        assert_eq!(decidi(ENTITA, json!({ "action": "remove", "entity_id": "light.cucina" })), Rilettura::Registri);
        // La stanza del dispositivo di un'entità mostrata, e il dispositivo tolto.
        let data = json!({ "action": "update", "device_id": "lampada", "changes": { "area_id": "salotto" } });
        assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Registri);
        let data = json!({ "action": "remove", "device_id": "lampada", "device": {} });
        assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Registri);
        // Una stanza nota rinominata o tolta.
        assert_eq!(decidi(AREA, json!({ "action": "update", "area_id": "cucina" })), Rilettura::Registri);
        assert_eq!(decidi(AREA, json!({ "action": "remove", "area_id": "cucina" })), Rilettura::Registri);
        assert_eq!(decidi(AREA, json!({ "action": "update", "area_id": "sconosciuta" })), Rilettura::Niente);
    }

    #[test]
    fn entita_nuove_o_rinominate_rileggono_tutto() {
        // Serve il loro stato, che l'app non ha (anche un aggiornamento nuovo, `update.*`, conta).
        assert_eq!(decidi(ENTITA, json!({ "action": "create", "entity_id": "light.nuova" })), Rilettura::Tutto);
        assert_eq!(decidi(ENTITA, json!({ "action": "create", "entity_id": "update.core" })), Rilettura::Tutto);
        let data = json!({ "action": "update", "entity_id": "light.studio", "old_entity_id": "light.cucina",
                           "changes": { "entity_id": "light.cucina" } });
        assert_eq!(decidi(ENTITA, data), Rilettura::Tutto);
        // Un aggiornamento acceso o disattivato: il suo stato nasce o sparisce fuori dall'iscrizione.
        let data = json!({ "action": "update", "entity_id": "update.core", "changes": { "disabled_by": "user" } });
        assert_eq!(decidi(ENTITA, data), Rilettura::Tutto);
    }
}
