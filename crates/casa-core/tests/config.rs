use casa_core::config::*;
use casa_core::segreti::{MemoriaSegreti, Segreti};
use std::path::PathBuf;

fn fx(n: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture").join(n)
}

struct Guasto;
impl Segreti for Guasto {
    fn salva(&self, _: &str, _: &str) -> Result<(), String> { Err("rotto".into()) }
    fn leggi(&self, _: &str) -> Result<Option<String>, String> { Err("rotto".into()) }
    fn cancella(&self, _: &str) -> Result<(), String> { Err("rotto".into()) }
}

#[test]
fn import_completo_sposta_i_token() {
    let s = MemoriaSegreti::new();
    let c = importa_v2(&fx("v2-completa.json"), &s).unwrap().unwrap();
    assert_eq!(c.environments.len(), 2);
    assert_eq!(c.active_env_id.as_deref(), Some("env2"));
    assert!(c.importata_da_v2 && c.cameras_enabled && c.launch_at_login);
    assert_eq!(c.favorites, vec!["light.a", "switch.b"]);
    assert_eq!(c.device_order["salotto"], vec!["light.a"]);
    assert_eq!(c.area_icons["salotto"], "sofa");
    let dir = tempfile::tempdir().unwrap();
    let st = ConfigStore::apri(dir.path());
    st.salva(&c).unwrap();
    let json = std::fs::read_to_string(dir.path().join("assistente.json")).unwrap();
    assert!(!json.to_lowercase().contains("token"));
    assert_eq!(s.leggi("env1").unwrap().as_deref(), Some("tok-finto-1"));
}

#[test]
fn import_legacy_haurl_hatoken() {
    let s = MemoriaSegreti::new();
    let c = importa_v2(&fx("v2-legacy.json"), &s).unwrap().unwrap();
    assert_eq!(c.environments.len(), 1);
    assert_eq!(c.environments[0].name, "Home");
    assert_eq!(c.environments[0].ha_url, "http://legacy.finto:8123");
    let id = c.active_env_id.clone().unwrap();
    assert_eq!(s.leggi(&id).unwrap().as_deref(), Some("tok-legacy-finto"));
}

#[test]
fn import_parziale_usa_i_default() {
    let s = MemoriaSegreti::new();
    let c = importa_v2(&fx("v2-parziale.json"), &s).unwrap().unwrap();
    assert_eq!(c.favorites, vec!["light.solo"]);
    assert!(c.environments.is_empty() && c.active_env_id.is_none() && !c.cameras_enabled);
}

#[test]
fn import_corrotto_da_errore_e_non_tocca_il_file() {
    let p = fx("v2-corrotta.json");
    let prima = std::fs::read(&p).unwrap();
    assert!(importa_v2(&p, &MemoriaSegreti::new()).is_err());
    assert_eq!(prima, std::fs::read(&p).unwrap());
}

#[test]
fn file_v2_assente_da_none() {
    let d = tempfile::tempdir().unwrap();
    assert!(importa_v2(&d.path().join("no.json"), &MemoriaSegreti::new()).unwrap().is_none());
}

#[test]
fn salva_e_rileggi_uguale() {
    let d = tempfile::tempdir().unwrap();
    let st = ConfigStore::apri(d.path());
    let mut c = Config {
        favorites: vec!["a".into()],
        ultimo_layout: "isola".into(),
        ..Config::default()
    };
    st.salva(&c).unwrap();
    assert_eq!(st.leggi(), c);
    c.favorites = vec!["b".into(), "c".into()];
    st.salva(&c).unwrap();
    assert_eq!(st.leggi(), c);
}

#[test]
fn icona_nella_barra_spenta_di_serie_e_salvata() {
    let d = tempfile::tempdir().unwrap();
    let st = ConfigStore::apri(d.path());
    // Un file di prima (senza la chiave) la lascia spenta.
    std::fs::write(d.path().join("assistente.json"), r#"{ "favorites": ["a"] }"#).unwrap();
    assert!(!st.leggi().mostra_icona_barra);
    let c = Config { mostra_icona_barra: true, ..Config::default() };
    st.salva(&c).unwrap();
    let testo = std::fs::read_to_string(d.path().join("assistente.json")).unwrap();
    assert!(testo.contains("\"mostraIconaBarra\": true"), "{testo}");
    assert!(st.leggi().mostra_icona_barra);
}

#[test]
fn config_illeggibile_da_default() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("assistente.json"), "{{non json").unwrap();
    assert_eq!(ConfigStore::apri(d.path()).leggi(), Config::default());
}

#[test]
fn portachiavi_guasto_non_scrive_token_in_chiaro() {
    let e = importa_v2(&fx("v2-completa.json"), &Guasto).unwrap_err();
    assert!(e.contains("portachiavi"));
}

/// Riesce al primo `salva`, fallisce al secondo: forza il rollback.
struct FallisceAlSecondo {
    dentro: MemoriaSegreti,
    chiamate: std::sync::atomic::AtomicUsize,
}
impl Segreti for FallisceAlSecondo {
    fn salva(&self, id: &str, t: &str) -> Result<(), String> {
        if self.chiamate.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 {
            return Err("rotto al secondo".into());
        }
        self.dentro.salva(id, t)
    }
    fn leggi(&self, id: &str) -> Result<Option<String>, String> { self.dentro.leggi(id) }
    fn cancella(&self, id: &str) -> Result<(), String> { self.dentro.cancella(id) }
}

#[test]
fn portachiavi_guasto_a_meta_annulla_e_non_scrive_nulla() {
    let s = FallisceAlSecondo { dentro: MemoriaSegreti::new(), chiamate: Default::default() };
    let e = importa_v2(&fx("v2-completa.json"), &s).unwrap_err();
    assert!(e.contains("portachiavi"));
    assert_eq!(s.leggi("env1").unwrap(), None, "rollback non eseguito");
    assert_eq!(s.leggi("env2").unwrap(), None);
    // importa_v2 non scrive file: una cartella di config vuota resta vuota.
    let d = tempfile::tempdir().unwrap();
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    assert!(ConfigStore::apri(d.path()).leggi() == Config::default());
}
/// Task 14f: i collegamenti (preferiti di pagine web) si salvano e si rileggono nell'ordine.
#[test]
fn collegamenti_salvati_e_riletti_nell_ordine() {
    let d = tempfile::tempdir().unwrap();
    let st = ConfigStore::apri(d.path());
    let c = Config {
        collegamenti: vec![
            Collegamento { id: "proxmox".into(), nome: "Proxmox".into(), url: "https://esempio.lan:8006".into() },
            Collegamento { id: "nas".into(), nome: "NAS".into(), url: "http://nas.esempio.lan/admin?x=1".into() },
        ],
        ..Config::default()
    };
    st.salva(&c).unwrap();
    let testo = std::fs::read_to_string(d.path().join("assistente.json")).unwrap();
    assert!(testo.contains("\"collegamenti\""), "{testo}");
    assert!(testo.contains("\"url\": \"https://esempio.lan:8006\""), "{testo}");
    assert_eq!(st.leggi(), c);
}

/// Task 14f: un file di prima (senza `collegamenti`) si legge tutto, con l'elenco vuoto.
#[test]
fn file_senza_collegamenti_si_legge() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("assistente.json"),
        r#"{ "favorites": ["light.a"], "mostraIconaBarra": true, "ultimoLayout": "quadranti" }"#,
    )
    .unwrap();
    let c = ConfigStore::apri(d.path()).leggi();
    assert!(c.collegamenti.is_empty());
    assert_eq!(c.favorites, vec!["light.a"]);
    assert!(c.mostra_icona_barra);
    assert_eq!(c.ultimo_layout, "quadranti");
}

/// M2 Task 4: un file di M1 (senza le chiavi dei lati) si legge con niente / niente.
#[test]
fn config_di_m1_ha_lati_vuoti() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("assistente.json"), r#"{ "favorites": ["light.a"], "ultimoLayout": "quadranti" }"#).unwrap();
    let c = ConfigStore::apri(d.path()).leggi();
    assert_eq!(c.lato_sinistro, "");
    assert_eq!(c.lato_destro, "");
    assert!(c.quadranti.is_empty(), "nessun quadrante scelto: quelli di M1");
    assert!(c.massimi_quadranti.is_empty());
    assert_eq!(c.favorites, vec!["light.a"], "il resto si legge come prima");
}

/// M2 Task 4: lati e quadranti salvati e riletti; i sensori scelti sono quelli col prefisso.
#[test]
fn lati_e_quadranti_salvati_e_sensori_scelti() {
    let d = tempfile::tempdir().unwrap();
    let st = ConfigStore::apri(d.path());
    let mut c = Config {
        lato_sinistro: "cpu".into(),
        lato_destro: "sensore:sensor.sala_t".into(),
        quadranti: vec!["ram".into(), "sensore:sensor.fv".into(), "sensore:".into()],
        ..Config::default()
    };
    c.massimi_quadranti.insert("sensore:sensor.fv".into(), 4500.0);
    st.salva(&c).unwrap();
    assert_eq!(st.leggi(), c);
    let scelti: Vec<String> = c.sensori_scelti().into_iter().collect();
    assert_eq!(scelti, vec!["sensor.fv".to_owned(), "sensor.sala_t".to_owned()], "senza id vuoti, senza doppioni");
}

/// M2 Task 6: un file di prima (senza `schermo`) si legge col monitor principale; la scelta si salva
/// e si rilegge.
#[test]
fn schermo_scelto_salvato_e_file_di_prima_senza() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("assistente.json"), r#"{ "favorites": ["light.a"] }"#).unwrap();
    let st = ConfigStore::apri(d.path());
    let mut c = st.leggi();
    assert_eq!(c.schermo, None, "nessuna scelta: il monitor principale");
    c.schermo = Some(r"\\.\DISPLAY2".into());
    st.salva(&c).unwrap();
    assert_eq!(st.leggi().schermo.as_deref(), Some(r"\\.\DISPLAY2"));
}

/// M2 Task 7 (D2): la rotella del volume sulla pillola è accesa di serie, anche per un file di
/// prima senza la chiave; spenta si salva e si rilegge spenta.
#[test]
fn volume_con_la_rotella_acceso_di_serie() {
    assert!(Config::default().volume_rotella, "configurazione nuova: accesa");
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("assistente.json"), r#"{ "favorites": ["light.a"] }"#).unwrap();
    let st = ConfigStore::apri(d.path());
    let mut c = st.leggi();
    assert!(c.volume_rotella, "file di prima: accesa");
    c.volume_rotella = false;
    st.salva(&c).unwrap();
    assert!(!st.leggi().volume_rotella, "spenta resta spenta");
}
