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
    let mut c = Config::default();
    c.favorites = vec!["a".into()];
    c.ultimo_layout = "isola".into();
    st.salva(&c).unwrap();
    assert_eq!(st.leggi(), c);
    c.favorites = vec!["b".into(), "c".into()];
    st.salva(&c).unwrap();
    assert_eq!(st.leggi(), c);
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