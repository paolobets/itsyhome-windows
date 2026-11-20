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

/// M2 Task 9: l'avviso delle cuffie Bluetooth è acceso di serie, anche per un file di prima senza
/// le chiavi; le impronte già avvisate (con la data) si salvano e si rileggono.
#[test]
fn avvisi_bt_accesi_di_serie_e_impronte_salvate() {
    assert!(Config::default().avvisi_bt, "configurazione nuova: acceso");
    assert!(Config::default().bt_avvisati.is_empty());
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("assistente.json"), r#"{ "favorites": ["light.a"] }"#).unwrap();
    let st = ConfigStore::apri(d.path());
    let mut c = st.leggi();
    assert!(c.avvisi_bt, "file di prima: acceso");
    assert!(c.bt_avvisati.is_empty(), "file di prima: nessun avviso dato");
    c.avvisi_bt = false;
    c.bt_avvisati.insert("0123456789abcdef".into(), "2026-11-15".into());
    st.salva(&c).unwrap();
    let riletta = st.leggi();
    assert!(!riletta.avvisi_bt, "spento resta spento");
    assert_eq!(riletta.bt_avvisati.get("0123456789abcdef").map(String::as_str), Some("2026-11-15"));
}

/// M3 Task 4: un file di M2, senza la chiave `claude`, si legge scollegato con la soglia di serie;
/// le scelte si salvano e si rileggono.
#[test]
fn config_di_m2_scollegata() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("assistente.json"), r#"{ "favorites": ["light.a"], "avvisiBt": false }"#).unwrap();
    let st = ConfigStore::apri(d.path());
    let mut c = st.leggi();
    assert!(!c.claude.collegato);
    assert_eq!(c.claude.soglia_limite, 80);
    assert_eq!(c, Config { claude: ConfigClaude::default(), ..c.clone() });
    assert_eq!(Config::default().claude, ConfigClaude::default());
    // Una chiave `claude` con solo qualche campo si completa con i valori di serie.
    std::fs::write(d.path().join("assistente.json"), r#"{ "claude": { "collegato": true, "porta": 51234 } }"#).unwrap();
    let parziale = st.leggi();
    assert!(parziale.claude.collegato);
    // La chiave `porta` di prima (Task 7) si ignora: il file si legge lo stesso.
    assert_eq!(parziale.claude.soglia_limite, 80);
    c.claude.collegato = true;
    c.claude.soglia_spesa = Some(50.0);
    c.claude.limiti_avvisati.insert("cinque_ore".into(), 1_700_000_000);
    st.salva(&c).unwrap();
    assert_eq!(st.leggi().claude, c.claude);
}

/// RC-56: l'ultimo valore dei limiti si ricorda fra un riavvio e l'altro; un file di prima, senza
/// le chiavi, si legge senza valori.
#[test]
fn ultimi_limiti_visti_nel_file_di_configurazione() {
    let d = tempfile::tempdir().unwrap();
    let st = ConfigStore::apri(d.path());
    assert_eq!(st.leggi().claude.ultimo_cinque_ore, None);
    assert_eq!(st.leggi().claude.ultimo_sette_giorni, None);
    let mut c = st.leggi();
    c.claude.ultimo_cinque_ore = Some(LimiteVisto { percentuale: 87.0, azzera_alle: 1_700_000_000, visto_alle: 1_699_990_000 });
    st.salva(&c).unwrap();
    let riletto = st.leggi();
    assert_eq!(riletto.claude.ultimo_cinque_ore, c.claude.ultimo_cinque_ore);
    assert_eq!(riletto.claude.ultimo_sette_giorni, None);
}

/// Ridisegno F1, Task 3: l'ordine dei moduli. Un file di prima (senza `moduli`) si legge vuoto
/// (= l'ordine del registro); l'elenco si salva e si rilegge uguale, in camelCase come il resto;
/// una voce senza `id` o senza `fissato` non fa rileggere tutta la configurazione come nuova.
#[test]
fn moduli_salvati_e_file_di_prima() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("assistente.json"), r#"{ "favorites": ["light.a"] }"#).unwrap();
    let st = ConfigStore::apri(d.path());
    let mut c = st.leggi();
    assert!(c.moduli.is_empty(), "file di prima: l'ordine del registro");
    assert!(Config::default().moduli.is_empty());
    c.moduli = vec![
        ConfigModulo { id: "collegamenti".into(), fissato: true },
        ConfigModulo { id: "agenti".into(), fissato: false },
    ];
    st.salva(&c).unwrap();
    assert_eq!(st.leggi(), c);
    let testo = std::fs::read_to_string(d.path().join("assistente.json")).unwrap();
    assert!(testo.contains(r#""moduli""#) && testo.contains(r#""fissato": false"#), "{testo}");

    std::fs::write(
        d.path().join("assistente.json"),
        r#"{ "favorites": ["light.b"], "moduli": [ { "fissato": false }, { "id": "casa" } ] }"#,
    )
    .unwrap();
    let malformato = st.leggi();
    assert_eq!(malformato.favorites, vec!["light.b"], "il resto del file si legge");
    assert_eq!(
        malformato.moduli,
        vec![ConfigModulo { id: String::new(), fissato: false }, ConfigModulo { id: "casa".into(), fissato: true }],
        "senza `fissato`: fissato"
    );
}
