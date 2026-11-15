//! Impostazioni › Dispositivi: la gestione di stanze e dispositivi che la v2 aveva nella scheda
//! «Accessories» (`src/renderer/settings/main.ts`, ramo `main`). Qui solo la logica, senza
//! interfaccia: l'ordine mostrato, le righe dell'elenco e le modifiche alla configurazione
//! (ordine delle stanze, ordine dei dispositivi per stanza, nascosti, preferiti e loro ordine,
//! icone delle stanze), con le stesse chiavi della v2.
//!
//! L'ordine si calcola dalla configurazione corrente sopra il catalogo del mapper, così una
//! modifica si vede subito, prima che arrivino i dati rimappati.

use casa_core::config::Config;
use casa_core::types::{StanzaCatalogo, VoceCatalogo, GRUPPO_SCENE};
use std::collections::HashSet;

/// `ids` con prima quelli di `salvato` (nell'ordine salvato, se ci sono) e poi gli altri
/// nell'ordine in cui arrivano.
fn ordina<T>(voci: Vec<T>, chiave: impl Fn(&T) -> &str, salvato: &[String]) -> Vec<T> {
    let mut resto: Vec<Option<T>> = voci.into_iter().map(Some).collect();
    let mut ordinate = Vec::with_capacity(resto.len());
    for id in salvato {
        if let Some(v) = resto.iter_mut().find(|v| v.as_ref().is_some_and(|v| chiave(v) == id)) {
            ordinate.push(v.take().expect("appena trovata"));
        }
    }
    ordinate.extend(resto.into_iter().flatten());
    ordinate
}

/// Una stanza vera (non «Altro», non le scene): si può spostare e le si può cambiare icona.
pub fn stanza_vera(area: &str) -> bool {
    !area.is_empty() && area != GRUPPO_SCENE
}

/// Il catalogo nell'ordine della configurazione corrente: le stanze vere secondo `roomOrder`
/// (le altre dopo, come le mette il mapper), poi «Altro» e le scene; i dispositivi di ogni
/// stanza secondo `deviceOrder`.
pub fn catalogo_ordinato(catalogo: &[StanzaCatalogo], config: &Config) -> Vec<StanzaCatalogo> {
    let (vere, coda): (Vec<StanzaCatalogo>, Vec<StanzaCatalogo>) =
        catalogo.iter().cloned().partition(|s| stanza_vera(&s.area_id));
    let mut stanze = ordina(vere, |s| s.area_id.as_str(), &config.room_order);
    stanze.extend(coda);
    for s in &mut stanze {
        if let Some(salvato) = config.device_order.get(&s.area_id) {
            s.voci = ordina(std::mem::take(&mut s.voci), |v| v.entity_id.as_str(), salvato);
        }
    }
    stanze
}

/// Scambia l'elemento `i` col vicino nella direzione `verso` (-1 su, +1 giù). Falso ai bordi.
fn scambia<T>(v: &mut [T], i: usize, verso: i32) -> bool {
    let j = i as i64 + i64::from(verso.signum());
    if verso == 0 || j < 0 || j as usize >= v.len() {
        return false;
    }
    v.swap(i, j as usize);
    true
}

/// Sposta una stanza vera di un posto fra le stanze vere; salva l'ordine intero (`roomOrder`).
pub fn sposta_stanza(c: &mut Config, catalogo: &[StanzaCatalogo], area: &str, verso: i32) -> bool {
    let mut ordine: Vec<String> = catalogo_ordinato(catalogo, c)
        .into_iter()
        .filter(|s| stanza_vera(&s.area_id))
        .map(|s| s.area_id)
        .collect();
    let Some(i) = ordine.iter().position(|a| a == area) else { return false };
    if !scambia(&mut ordine, i, verso) {
        return false;
    }
    c.room_order = ordine;
    true
}

/// Sposta un dispositivo di un posto nella sua stanza (anche fra quelli nascosti); salva
/// l'ordine intero della stanza (`deviceOrder[area]`, `""` per «Altro»).
pub fn sposta_dispositivo(c: &mut Config, catalogo: &[StanzaCatalogo], area: &str, entity: &str, verso: i32) -> bool {
    let stanze = catalogo_ordinato(catalogo, c);
    let Some(stanza) = stanze.iter().find(|s| s.area_id == area) else { return false };
    let mut ordine: Vec<String> = stanza.voci.iter().map(|v| v.entity_id.clone()).collect();
    let Some(i) = ordine.iter().position(|e| e == entity) else { return false };
    if !scambia(&mut ordine, i, verso) {
        return false;
    }
    c.device_order.insert(area.to_owned(), ordine);
    true
}

/// Nasconde un dispositivo, o lo mostra se era nascosto (`hiddenEntities`).
pub fn commuta_nascosto(c: &mut Config, entity: &str) {
    match c.hidden_entities.iter().position(|e| e == entity) {
        Some(i) => {
            c.hidden_entities.remove(i);
        }
        None => c.hidden_entities.push(entity.to_owned()),
    }
}

/// Tutti i dispositivi di una stanza sono nascosti?
pub fn stanza_nascosta(stanza: &StanzaCatalogo, c: &Config) -> bool {
    !stanza.voci.is_empty() && stanza.voci.iter().all(|v| c.hidden_entities.contains(&v.entity_id))
}

/// L'occhio della stanza, come nella v2: se è tutta nascosta la mostra tutta, altrimenti la
/// nasconde tutta.
pub fn commuta_stanza_nascosta(c: &mut Config, catalogo: &[StanzaCatalogo], area: &str) {
    let Some(stanza) = catalogo.iter().find(|s| s.area_id == area) else { return };
    let nascondi = !stanza_nascosta(stanza, c);
    for v in &stanza.voci {
        let presente = c.hidden_entities.contains(&v.entity_id);
        if nascondi && !presente {
            c.hidden_entities.push(v.entity_id.clone());
        } else if !nascondi && presente {
            c.hidden_entities.retain(|e| e != &v.entity_id);
        }
    }
}

/// La stella: aggiunge il dispositivo in fondo ai preferiti, o lo toglie.
pub fn commuta_preferito(c: &mut Config, entity: &str) {
    match c.favorites.iter().position(|e| e == entity) {
        Some(i) => {
            c.favorites.remove(i);
        }
        None => c.favorites.push(entity.to_owned()),
    }
}

/// I preferiti che ci sono nel catalogo, nell'ordine salvato (quelli di entità sparite restano
/// salvati ma non si mostrano, come nella v2).
pub fn preferiti_presenti<'a>(catalogo: &'a [StanzaCatalogo], c: &Config) -> Vec<&'a VoceCatalogo> {
    c.favorites
        .iter()
        .filter_map(|id| catalogo.iter().flat_map(|s| &s.voci).find(|v| &v.entity_id == id))
        .collect()
}

/// Sposta un preferito di un posto fra quelli presenti; quelli di entità sparite restano in fondo.
pub fn sposta_preferito(c: &mut Config, catalogo: &[StanzaCatalogo], entity: &str, verso: i32) -> bool {
    let mut presenti: Vec<String> = preferiti_presenti(catalogo, c).iter().map(|v| v.entity_id.clone()).collect();
    let Some(i) = presenti.iter().position(|e| e == entity) else { return false };
    if !scambia(&mut presenti, i, verso) {
        return false;
    }
    let assenti: Vec<String> = c.favorites.iter().filter(|e| !presenti.contains(e)).cloned().collect();
    presenti.extend(assenti);
    c.favorites = presenti;
    true
}

/// L'icona di una stanza (l'emoji della v2, vedi `stanze::ICONE`) in `areaIcons`.
pub fn imposta_icona(c: &mut Config, area: &str, emoji: &str) -> bool {
    if !stanza_vera(area) || crate::stanze::indice(emoji).is_none() {
        return false;
    }
    c.area_icons.insert(area.to_owned(), emoji.to_owned());
    true
}

/// Il tipo di dispositivo in italiano (seconda riga di ogni dispositivo).
pub fn nome_tipo(kind: &str) -> &'static str {
    match kind {
        "light" => "Luce",
        "switch" => "Interruttore",
        "climate" => "Termostato",
        "cover" => "Tapparella",
        "lock" => "Serratura",
        "fan" => "Ventilatore",
        "humidifier" => "Umidificatore",
        "valve" => "Valvola",
        "sensor" => "Sensore",
        "alarm" => "Allarme",
        "camera" => "Telecamera",
        "scene" => "Scena",
        _ => "Dispositivo",
    }
}

/// La chiave del gruppo dei preferiti fra le stanze compresse (non è un `area_id` possibile).
pub const CHIAVE_PREFERITI: &str = "__preferiti__";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoRiga {
    TestataPreferiti,
    Preferito,
    TestataStanza,
    Dispositivo,
}

/// Una riga dell'elenco di Impostazioni › Dispositivi.
#[derive(Debug, Clone, PartialEq)]
pub struct Riga {
    pub tipo: TipoRiga,
    /// L'entità (preferito, dispositivo) o l'area (testata di stanza).
    pub chiave: String,
    /// La stanza a cui appartiene la riga.
    pub area: String,
    pub nome: String,
    pub sotto: String,
    /// Indice dell'icona della stanza (`Icone.di-stanza`), solo per le testate di stanza.
    pub icona: i32,
    pub kind: String,
    pub nascosto: bool,
    pub preferito: bool,
    pub su: bool,
    pub giu: bool,
    pub compressa: bool,
}

/// Le righe dell'elenco: i preferiti in cima (come la v2), poi ogni stanza con i suoi
/// dispositivi, nascosti compresi (le stanze `compresse` senza i dispositivi).
pub fn righe(catalogo: &[StanzaCatalogo], c: &Config, compresse: &HashSet<String>) -> Vec<Riga> {
    let stanze = catalogo_ordinato(catalogo, c);
    let preferiti = preferiti_presenti(&stanze, c);
    let mut righe = Vec::new();
    let quanti = |n: usize, una: &str, tante: &str| if n == 1 { format!("1 {una}") } else { format!("{n} {tante}") };
    righe.push(Riga {
        tipo: TipoRiga::TestataPreferiti,
        chiave: CHIAVE_PREFERITI.into(),
        area: String::new(),
        nome: "Preferiti".into(),
        sotto: if preferiti.is_empty() {
            "Nessuno: aggiungili con la stella".into()
        } else {
            quanti(preferiti.len(), "preferito", "preferiti")
        },
        icona: -3,
        kind: String::new(),
        nascosto: false,
        preferito: true,
        su: false,
        giu: false,
        compressa: compresse.contains(CHIAVE_PREFERITI),
    });
    if !compresse.contains(CHIAVE_PREFERITI) {
        let n = preferiti.len();
        for (i, v) in preferiti.iter().enumerate() {
            let nascosto = c.hidden_entities.contains(&v.entity_id);
            righe.push(Riga {
                tipo: TipoRiga::Preferito,
                chiave: v.entity_id.clone(),
                area: String::new(),
                nome: v.name.clone(),
                sotto: format!("{}{}", nome_tipo(&v.kind), if nascosto { " · nascosto" } else { "" }),
                icona: 0,
                kind: v.kind.clone(),
                nascosto,
                preferito: true,
                su: i > 0,
                giu: i + 1 < n,
                compressa: false,
            });
        }
    }
    let vere = stanze.iter().filter(|s| stanza_vera(&s.area_id)).count();
    let mut indice_vera = 0;
    for s in &stanze {
        let vera = stanza_vera(&s.area_id);
        let tutta_nascosta = stanza_nascosta(s, c);
        let compressa = compresse.contains(&s.area_id);
        righe.push(Riga {
            tipo: TipoRiga::TestataStanza,
            chiave: s.area_id.clone(),
            area: s.area_id.clone(),
            nome: s.name.clone(),
            sotto: format!(
                "{}{}",
                quanti(s.voci.len(), "dispositivo", "dispositivi"),
                if tutta_nascosta { " · nascosta" } else { "" }
            ),
            icona: if s.area_id == GRUPPO_SCENE {
                -2
            } else if s.area_id.is_empty() {
                -1
            } else {
                crate::stanze::icona(c.area_icons.get(&s.area_id).map(String::as_str), &s.name) as i32
            },
            kind: String::new(),
            nascosto: tutta_nascosta,
            preferito: false,
            su: vera && indice_vera > 0,
            giu: vera && indice_vera + 1 < vere,
            compressa,
        });
        if vera {
            indice_vera += 1;
        }
        if compressa {
            continue;
        }
        let n = s.voci.len();
        for (i, v) in s.voci.iter().enumerate() {
            let nascosto = c.hidden_entities.contains(&v.entity_id);
            righe.push(Riga {
                tipo: TipoRiga::Dispositivo,
                chiave: v.entity_id.clone(),
                area: s.area_id.clone(),
                nome: v.name.clone(),
                sotto: format!("{}{}", nome_tipo(&v.kind), if nascosto { " · nascosto" } else { "" }),
                icona: 0,
                kind: v.kind.clone(),
                nascosto,
                preferito: c.favorites.contains(&v.entity_id),
                su: i > 0,
                giu: i + 1 < n,
                compressa: false,
            });
        }
    }
    righe
}

#[cfg(test)]
mod test {
    use super::*;
    use casa_core::config::ConfigStore;

    fn voce(id: &str, kind: &str) -> VoceCatalogo {
        VoceCatalogo { entity_id: id.into(), name: format!("Nome {id}"), kind: kind.into() }
    }

    fn stanza(area: &str, nome: &str, voci: &[&str]) -> StanzaCatalogo {
        StanzaCatalogo { area_id: area.into(), name: nome.into(), voci: voci.iter().map(|v| voce(v, "light")).collect() }
    }

    /// Cucina (luce a, luce b), Sala (presa c), Altro (luce z), Scene (sera).
    fn catalogo() -> Vec<StanzaCatalogo> {
        vec![
            stanza("cucina", "Cucina", &["light.a", "light.b"]),
            stanza("sala", "Sala", &["switch.c"]),
            stanza("", "Altro", &["light.z"]),
            StanzaCatalogo { area_id: GRUPPO_SCENE.into(), name: "Scene".into(), voci: vec![voce("scene.sera", "scene")] },
        ]
    }

    fn aree(c: &Config) -> Vec<String> {
        catalogo_ordinato(&catalogo(), c).into_iter().map(|s| s.area_id).collect()
    }

    #[test]
    fn stanze_spostate_su_e_giu_altro_e_scene_restano_in_fondo() {
        let mut c = Config::default();
        assert!(sposta_stanza(&mut c, &catalogo(), "sala", -1));
        assert_eq!(c.room_order, ["sala", "cucina"]);
        assert_eq!(aree(&c), ["sala", "cucina", "", GRUPPO_SCENE]);
        assert!(!sposta_stanza(&mut c, &catalogo(), "sala", -1), "già in cima");
        assert!(!sposta_stanza(&mut c, &catalogo(), "cucina", 1), "ultima stanza vera: Altro e scene non si scavalcano");
        assert!(!sposta_stanza(&mut c, &catalogo(), "", -1), "«Altro» non si sposta");
        assert!(sposta_stanza(&mut c, &catalogo(), "sala", 1));
        assert_eq!(aree(&c), ["cucina", "sala", "", GRUPPO_SCENE]);
    }

    #[test]
    fn dispositivi_spostati_nella_loro_stanza_anche_nascosti() {
        let mut c = Config { hidden_entities: vec!["light.a".into()], ..Config::default() };
        assert!(sposta_dispositivo(&mut c, &catalogo(), "cucina", "light.a", 1));
        assert_eq!(c.device_order["cucina"], ["light.b", "light.a"]);
        assert!(!sposta_dispositivo(&mut c, &catalogo(), "cucina", "light.a", 1), "già in fondo");
        assert!(!sposta_dispositivo(&mut c, &catalogo(), "cucina", "switch.c", -1), "non è di questa stanza");
        let cucina = &catalogo_ordinato(&catalogo(), &c)[0];
        assert_eq!(cucina.voci.iter().map(|v| v.entity_id.as_str()).collect::<Vec<_>>(), ["light.b", "light.a"]);
        // Le scene si ordinano nel loro gruppo.
        let mut cat = catalogo();
        cat[3].voci.push(voce("scene.alba", "scene"));
        assert!(sposta_dispositivo(&mut c, &cat, GRUPPO_SCENE, "scene.alba", -1));
        assert_eq!(c.device_order[GRUPPO_SCENE], ["scene.alba", "scene.sera"]);
    }

    #[test]
    fn nascosti_uno_alla_volta_e_per_stanza() {
        let mut c = Config::default();
        commuta_nascosto(&mut c, "light.a");
        assert_eq!(c.hidden_entities, ["light.a"]);
        // L'occhio della stanza: non tutta nascosta → la nasconde tutta (senza doppioni).
        commuta_stanza_nascosta(&mut c, &catalogo(), "cucina");
        assert_eq!(c.hidden_entities, ["light.a", "light.b"]);
        assert!(stanza_nascosta(&catalogo()[0], &c));
        // Tutta nascosta → la mostra tutta.
        commuta_stanza_nascosta(&mut c, &catalogo(), "cucina");
        assert!(c.hidden_entities.is_empty());
        commuta_nascosto(&mut c, "switch.c");
        commuta_nascosto(&mut c, "switch.c");
        assert!(c.hidden_entities.is_empty());
    }

    #[test]
    fn preferiti_con_la_stella_e_in_ordine() {
        let mut c = Config { favorites: vec!["light.sparita".into()], ..Config::default() };
        commuta_preferito(&mut c, "switch.c");
        commuta_preferito(&mut c, "light.a");
        assert_eq!(c.favorites, ["light.sparita", "switch.c", "light.a"]);
        let cat = catalogo();
        let presenti: Vec<&str> = preferiti_presenti(&cat, &c).iter().map(|v| v.entity_id.as_str()).collect();
        assert_eq!(presenti, ["switch.c", "light.a"]);
        assert!(sposta_preferito(&mut c, &catalogo(), "light.a", -1));
        assert_eq!(c.favorites, ["light.a", "switch.c", "light.sparita"], "le sparite restano, in fondo");
        assert!(!sposta_preferito(&mut c, &catalogo(), "light.a", -1));
        commuta_preferito(&mut c, "light.a");
        assert_eq!(c.favorites, ["switch.c", "light.sparita"]);
    }

    #[test]
    fn icona_solo_dell_insieme_e_solo_per_stanze_vere() {
        let mut c = Config::default();
        assert!(imposta_icona(&mut c, "cucina", "🎮"));
        assert_eq!(c.area_icons["cucina"], "🎮");
        assert!(!imposta_icona(&mut c, "cucina", "🦄"));
        assert!(!imposta_icona(&mut c, "", "🎮"));
        assert!(!imposta_icona(&mut c, GRUPPO_SCENE, "🎮"));
    }

    #[test]
    fn righe_con_preferiti_stanze_e_frecce() {
        let c = Config {
            favorites: vec!["light.b".into()],
            hidden_entities: vec!["switch.c".into()],
            area_icons: [("sala".to_owned(), "🎮".to_owned())].into(),
            ..Config::default()
        };
        let r = righe(&catalogo(), &c, &HashSet::new());
        let tipi: Vec<(TipoRiga, &str)> = r.iter().map(|r| (r.tipo, r.chiave.as_str())).collect();
        assert_eq!(
            tipi,
            [
                (TipoRiga::TestataPreferiti, CHIAVE_PREFERITI),
                (TipoRiga::Preferito, "light.b"),
                (TipoRiga::TestataStanza, "cucina"),
                (TipoRiga::Dispositivo, "light.a"),
                (TipoRiga::Dispositivo, "light.b"),
                (TipoRiga::TestataStanza, "sala"),
                (TipoRiga::Dispositivo, "switch.c"),
                (TipoRiga::TestataStanza, ""),
                (TipoRiga::Dispositivo, "light.z"),
                (TipoRiga::TestataStanza, GRUPPO_SCENE),
                (TipoRiga::Dispositivo, "scene.sera"),
            ]
        );
        assert!(r[4].preferito && !r[3].preferito);
        assert_eq!((r[3].su, r[3].giu, r[4].su, r[4].giu), (false, true, true, false));
        assert_eq!((r[2].su, r[2].giu, r[5].su, r[5].giu), (false, true, true, false), "solo fra stanze vere");
        assert_eq!((r[7].su, r[7].giu), (false, false), "«Altro» fermo");
        assert!(r[5].nascosto && r[6].nascosto, "la sala è tutta nascosta");
        assert_eq!(r[5].sotto, "1 dispositivo · nascosta");
        assert_eq!(r[6].sotto, "Luce · nascosto");
        assert_eq!(r[5].icona as usize, crate::stanze::indice("🎮").unwrap());
        assert_eq!((r[7].icona, r[9].icona), (-1, -2));
        // Una stanza compressa senza i suoi dispositivi.
        let compresse: HashSet<String> = ["cucina".to_owned()].into();
        let r = righe(&catalogo(), &c, &compresse);
        assert_eq!(r[2].chiave, "cucina");
        assert!(r[2].compressa);
        assert_eq!(r[3].chiave, "sala");
    }

    /// Ogni modifica è nella configurazione salvata e ricaricata (le chiavi della v2).
    #[test]
    fn modifiche_salvate_e_ricaricate() {
        let cartella = tempfile::tempdir().unwrap();
        let store = ConfigStore::apri(cartella.path());
        let mut c = Config::default();
        sposta_stanza(&mut c, &catalogo(), "sala", -1);
        sposta_dispositivo(&mut c, &catalogo(), "cucina", "light.a", 1);
        commuta_nascosto(&mut c, "light.z");
        commuta_preferito(&mut c, "switch.c");
        imposta_icona(&mut c, "cucina", "🎮");
        store.salva(&c).unwrap();
        let letta = store.leggi();
        assert_eq!(letta, c);
        let testo = std::fs::read_to_string(cartella.path().join("assistente.json")).unwrap();
        for chiave in ["roomOrder", "deviceOrder", "hiddenEntities", "favorites", "areaIcons"] {
            assert!(testo.contains(chiave), "{chiave} nel file");
        }
        assert_eq!(aree(&letta), ["sala", "cucina", "", GRUPPO_SCENE]);
    }
}
