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

use super::mapper::{Indici, DOMINI_MOSTRABILI};

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
// I campi da cui HA compone il `friendly_name` (`original_name`, `translation_key`,
// `has_entity_name`, e `changes` vuoto quando cambia il nome composto) non rileggono nulla: le
// entità mostrate sono tutte seguite e HA riscrive il loro stato a ogni `update` del registro
// (giro di correzioni 2, N3).

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
        self.decidi(Some(indici), mostrata)
    }

    /// La lettura che serve senza registri affidabili (una lettura dei registri in volo o in coda,
    /// nessun dato ancora): si decide solo da azione, campi e dominio, e ciò che dipenderebbe dai
    /// registri o dalle entità mostrate rilegge i registri (giro di correzioni 1, I2).
    pub fn rilettura_prudente(&self) -> Rilettura {
        self.decidi(None, &|_| true)
    }

    /// `indici` assente: ogni domanda sui registri ha la risposta che fa rileggere.
    fn decidi(&self, indici: Option<&Indici>, mostrata: &dyn Fn(&str) -> bool) -> Rilettura {
        let tocca = |campi: &[String], quali: &[&str]| campi.iter().any(|c| quali.contains(&c.as_str()));
        let si_se = |b: bool| if b { Rilettura::Registri } else { Rilettura::Niente };
        match self {
            CambioRegistro::Entita { azione, entity_id, campi, rinominata } => {
                let d = dominio(entity_id);
                // Gli `update.*` contano gli aggiornamenti: uno che nasce o si riaccende ha uno
                // stato che l'app non ha (e che l'iscrizione non porta finché non lo segue).
                let aggiornamento = d == "update";
                if !aggiornamento && !DOMINI_MOSTRABILI.contains(&d) {
                    return Rilettura::Niente;
                }
                // Mostrata (o seguita), o sconosciuta ai registri in memoria (vecchi?).
                let conta = || mostrata(entity_id) || indici.is_none_or(|i| i.voce(entity_id).is_none());
                match azione {
                    Azione::Crea => Rilettura::Tutto,
                    // Il suo stato esce con `r` dall'iscrizione; i registri tolgono la sua voce.
                    Azione::Togli => si_se(mostrata(entity_id)),
                    Azione::Riordina => Rilettura::Niente,
                    // Rinominata: l'entity_id nuovo ha uno stato che l'app non ha.
                    Azione::Aggiorna if *rinominata => Rilettura::Tutto,
                    Azione::Aggiorna if aggiornamento => {
                        if tocca(campi, &CAMPI_VISIBILITA) { Rilettura::Tutto } else { Rilettura::Niente }
                    }
                    Azione::Aggiorna => si_se(tocca(campi, &CAMPI_VISIBILITA) || (tocca(campi, &CAMPI_ASPETTO) && conta())),
                }
            }
            CambioRegistro::Dispositivo { azione, device_id, campi } => {
                // Le entità mostrate del dispositivo e dei suoi figli (HA 2026.9+), con `stanza`:
                // solo quelle che prendono la stanza da lui (né la loro né quella del figlio).
                let usato = |stanza: bool| {
                    let Some(i) = indici else { return true };
                    let proprie = i.voci_del_dispositivo(device_id).map(|e| (e, None));
                    let dei_figli = i.figli(device_id).flat_map(|f| i.voci_del_dispositivo(&f.id).map(move |e| (e, Some(f))));
                    proprie.chain(dei_figli).any(|(e, figlio)| {
                        let libera = e.area_id.as_deref().is_none_or(str::is_empty)
                            && figlio.is_none_or(|f| f.area_id.as_deref().is_none_or(str::is_empty));
                        mostrata(&e.entity_id) && (!stanza || libera)
                    })
                };
                let usato_dai_figli = || {
                    let Some(i) = indici else { return true };
                    i.figli(device_id).any(|f| i.voci_del_dispositivo(&f.id).any(|e| mostrata(&e.entity_id)))
                };
                match azione {
                    // Stanza propria, o padre cambiato (un figlio senza stanza prende la sua).
                    Azione::Aggiorna if tocca(campi, &["area_id", "parent_device_id"]) && usato(true) => Rilettura::Registri,
                    // Il nome di un padre: HA non riscrive gli stati delle entità dei figli (le
                    // sue, sì: per quelle il nome nuovo arriva con lo stato seguito).
                    Azione::Aggiorna if tocca(campi, &["name", "name_by_user"]) && usato_dai_figli() => Rilettura::Registri,
                    Azione::Togli if usato(false) => Rilettura::Registri,
                    // Versioni, connessioni, `disabled_by` (HA manda gli eventi delle sue entità):
                    // nessuno li mostra. Un dispositivo nuovo non ha ancora entità.
                    _ => Rilettura::Niente,
                }
            }
            CambioRegistro::Area { azione, area_id } => match (azione, area_id) {
                (Azione::Aggiorna | Azione::Togli, Some(a)) => si_se(indici.is_none_or(|i| i.ha_area(a))),
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
        HaDevice {
            id: id.into(),
            name: "D".into(),
            name_by_user: None,
            area_id: area.map(Into::into),
            disabled_by: None,
            parent_device_id: None,
        }
    }

    fn figlio(id: &str, padre: &str, area: Option<&str>) -> HaDevice {
        HaDevice { parent_device_id: Some(padre.into()), ..dispositivo(id, area) }
    }

    /// Una casa: la luce della cucina (mostrata, sul dispositivo «lampada» in cucina), un sensore
    /// di potenza (non mostrato) sul dispositivo «presa», un segnale wifi in diagnostica sul
    /// dispositivo «router», un'automazione; un «hub» con due dispositivi figli (HA 2026.9+): «canale1»
    /// senza stanza (prende quella dell'hub) con la luce mostrata «light.canale1», «canale2» con
    /// la sua stanza e la luce mostrata «light.canale2».
    fn indici() -> Indici {
        Indici::new(
            vec![HaArea { area_id: "cucina".into(), name: "Cucina".into() }],
            vec![
                dispositivo("lampada", Some("cucina")),
                dispositivo("presa", None),
                dispositivo("router", None),
                dispositivo("hub", Some("cucina")),
                figlio("canale1", "hub", None),
                figlio("canale2", "hub", Some("cucina")),
            ],
            vec![
                voce("light.cucina", Some("lampada"), None),
                voce("sensor.potenza", Some("presa"), None),
                voce("sensor.wifi", Some("router"), None),
                voce("automation.sera", None, None),
                voce("light.canale1", Some("canale1"), None),
                voce("light.canale2", Some("canale2"), None),
            ],
        )
    }

    fn mostrata(id: &str) -> bool {
        ["light.cucina", "light.canale1", "light.canale2"].contains(&id)
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
        for campo in ["sw_version", "hw_version", "connections", "identifiers", "config_entries", "disabled_by"] {
            let data = json!({ "action": "update", "device_id": "lampada", "changes": { campo: "x" } });
            assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Niente, "dispositivo {campo}");
        }
        for campo in ["options", "capabilities", "supported_features", "original_icon", "icon", "unit_of_measurement", "aliases", "labels", "config_entry_id"] {
            let data = json!({ "action": "update", "entity_id": "light.cucina", "changes": { campo: "x" } });
            assert_eq!(decidi(ENTITA, data), Rilettura::Niente, "entità {campo}");
        }
    }

    /// Giro di correzioni 2 (N3): i nomi che arrivano con lo stato non rileggono i registri. Le
    /// entità mostrate (anche nascoste dall'utente) sono tutte seguite, e HA riscrive lo stato
    /// (`friendly_name` nuovo) a ogni `update` del registro dell'entità e quando cambia `name` o
    /// `name_by_user` del **suo** dispositivo (`helpers/entity.py`, `_async_registry_updated` e
    /// `_async_device_registry_updated`): così `original_name`, `translation_key`,
    /// `has_entity_name`, `changes` vuoto e il nome del dispositivo proprio. Restano: il nome
    /// scelto (`name`, la mappatura lo legge dal registro) e il nome di un dispositivo **padre**,
    /// che in HA non riscrive gli stati delle entità dei figli (né manda eventi dei figli).
    #[test]
    fn i_nomi_che_arrivano_con_lo_stato_non_rileggono() {
        for campo in ["original_name", "translation_key", "has_entity_name"] {
            let data = json!({ "action": "update", "entity_id": "light.cucina", "changes": { campo: "x" } });
            assert_eq!(decidi(ENTITA, data), Rilettura::Niente, "entità {campo}");
        }
        let data = json!({ "action": "update", "entity_id": "light.cucina", "changes": {} });
        assert_eq!(decidi(ENTITA, data), Rilettura::Niente, "changes vuoto");
        let data = json!({ "action": "update", "entity_id": "light.cucina", "changes": { "name": null } });
        assert_eq!(decidi(ENTITA, data), Rilettura::Registri, "nome scelto: letto dal registro");
        for campo in ["name", "name_by_user"] {
            // Il dispositivo proprio: HA riscrive gli stati delle sue entità.
            let data = json!({ "action": "update", "device_id": "lampada", "changes": { campo: "x" } });
            assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Niente, "dispositivo {campo}");
            // Un padre con figli mostrati: gli stati dei figli non si riscrivono.
            let data = json!({ "action": "update", "device_id": "hub", "changes": { campo: "x" } });
            assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Registri, "hub {campo}");
            // Un dispositivo senza entità mostrate.
            let data = json!({ "action": "update", "device_id": "presa", "changes": { campo: "x" } });
            assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Niente, "presa {campo}");
        }
    }

    /// Giro di correzioni 1: un dispositivo figlio senza stanza prende quella del padre (HA
    /// 2026.9+, `async_get_effective_area_id`): la stanza dell'hub conta per le entità del figlio.
    #[test]
    fn la_stanza_del_padre_conta_per_i_figli() {
        let data = json!({ "action": "update", "device_id": "hub", "changes": { "area_id": "cucina" } });
        assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Registri);
        // Un figlio che cambia padre.
        let data = json!({ "action": "update", "device_id": "canale1", "changes": { "parent_device_id": "altro" } });
        assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Registri);
        let data = json!({ "action": "remove", "device_id": "hub", "device": {} });
        assert_eq!(decidi(DISPOSITIVO, data), Rilettura::Registri);
        // Solo «canale2» (con la sua stanza) sotto un altro padre: la stanza del padre non conta.
        let indici = Indici::new(
            vec![],
            vec![dispositivo("hub", None), figlio("canale2", "hub", Some("cucina"))],
            vec![voce("light.canale2", Some("canale2"), None)],
        );
        let c = leggi(DISPOSITIVO, &json!({ "action": "update", "device_id": "hub", "changes": { "area_id": "x" } })).unwrap();
        assert_eq!(c.rilettura(&indici, &mostrata), Rilettura::Niente);
    }

    /// Giro di correzioni 1, I2: senza registri affidabili (una lettura in volo o in coda) si
    /// decide solo dai campi e dal dominio: ciò che dipenderebbe dai registri rilegge i registri.
    #[test]
    fn decisione_prudente_senza_registri() {
        let prudente = |tipo: &str, data: Value| leggi(tipo, &data).unwrap().rilettura_prudente();
        // Dai soli campi e dominio: come sempre.
        assert_eq!(prudente(DISPOSITIVO, json!({ "action": "update", "device_id": "x", "changes": { "sw_version": "1" } })), Rilettura::Niente);
        assert_eq!(prudente(ENTITA, json!({ "action": "update", "entity_id": "light.x", "changes": { "options": {} } })), Rilettura::Niente);
        assert_eq!(prudente(ENTITA, json!({ "action": "update", "entity_id": "automation.x", "changes": { "name": "x" } })), Rilettura::Niente);
        assert_eq!(prudente(AREA, json!({ "action": "reorder", "area_id": null })), Rilettura::Niente);
        assert_eq!(prudente(ENTITA, json!({ "action": "create", "entity_id": "light.nuova" })), Rilettura::Tutto);
        // Dai registri o dalle entità mostrate: si rileggono i registri.
        assert_eq!(prudente(DISPOSITIVO, json!({ "action": "update", "device_id": "presa", "changes": { "area_id": null } })), Rilettura::Registri);
        assert_eq!(prudente(ENTITA, json!({ "action": "update", "entity_id": "sensor.potenza", "changes": { "name": null } })), Rilettura::Registri);
        assert_eq!(prudente(ENTITA, json!({ "action": "remove", "entity_id": "sensor.potenza" })), Rilettura::Registri);
        assert_eq!(prudente(AREA, json!({ "action": "update", "area_id": "nuova" })), Rilettura::Registri);
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
