//! Dove sta la configurazione e, al primo avvio, l'import dalla v2.

use casa_core::config::{cartella_config, file_config_v2, importa_v2, ConfigStore};
use casa_core::segreti::Segreti;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Cartella di configurazione alternativa (prove locali). Quando c'è, l'import dalla v2 è spento:
/// una prova non deve mai leggere il file vero della v2 né toccare i suoi token.
pub const VAR_CARTELLA: &str = "ITSYHOME_ASSISTENTE_DIR";

/// La cartella della configurazione e il file v2 da importare (assente in prova).
pub fn cartelle(var: Option<OsString>) -> (PathBuf, Option<PathBuf>) {
    match var.filter(|v| !v.is_empty()) {
        Some(v) => (PathBuf::from(v), None),
        None => (cartella_config(), file_config_v2()),
    }
}

/// Importa la v2 solo se `assistente.json` non esiste ancora e il file v2 c'è; il risultato si
/// salva subito. Restituisce l'errore da mostrare nelle Impostazioni: in quel caso non si scrive
/// nulla e si parte con la configurazione vuota (al prossimo avvio l'import si ritenta).
pub fn importa_se_serve(dir: &Path, v2: Option<&Path>, segreti: &dyn Segreti) -> Option<String> {
    let v2 = v2?;
    if dir.join("assistente.json").exists() {
        return None;
    }
    match importa_v2(v2, segreti) {
        Ok(Some(config)) => ConfigStore::apri(dir)
            .salva(&config)
            .err()
            .map(|e| format!("configurazione importata ma non salvata: {e}")),
        Ok(None) => None,
        Err(e) => Some(format!("import dalla versione precedente non riuscito: {e}")),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use casa_core::segreti::MemoriaSegreti;

    fn fixture(nome: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../casa-core/tests/fixture").join(nome)
    }

    #[test]
    fn la_variabile_sceglie_la_cartella_e_spegne_l_import() {
        let (dir, v2) = cartelle(Some("C:/prova/assistente".into()));
        assert_eq!(dir, PathBuf::from("C:/prova/assistente"));
        assert_eq!(v2, None);
        let (dir, _) = cartelle(Some("".into()));
        assert_eq!(dir, cartella_config());
    }

    #[test]
    fn primo_avvio_importa_e_salva() {
        let dir = tempfile::tempdir().unwrap();
        let segreti = MemoriaSegreti::new();
        assert_eq!(importa_se_serve(dir.path(), Some(&fixture("v2-completa.json")), &segreti), None);
        let c = ConfigStore::apri(dir.path()).leggi();
        assert!(c.importata_da_v2);
        assert_eq!(c.favorites, ["light.a", "switch.b"]);
        assert_eq!(segreti.leggi("env2").unwrap().as_deref(), Some("tok-finto-2"));
    }

    #[test]
    fn con_la_configurazione_gia_presente_non_importa() {
        let dir = tempfile::tempdir().unwrap();
        ConfigStore::apri(dir.path()).salva(&Default::default()).unwrap();
        let segreti = MemoriaSegreti::new();
        assert_eq!(importa_se_serve(dir.path(), Some(&fixture("v2-completa.json")), &segreti), None);
        assert!(!ConfigStore::apri(dir.path()).leggi().importata_da_v2);
        assert_eq!(segreti.leggi("env2").unwrap(), None);
    }

    #[test]
    fn import_fallito_lascia_l_errore_e_nessun_file() {
        let dir = tempfile::tempdir().unwrap();
        let errore = importa_se_serve(dir.path(), Some(&fixture("v2-corrotta.json")), &MemoriaSegreti::new());
        assert!(errore.unwrap().starts_with("import dalla versione precedente non riuscito"));
        assert!(!dir.path().join("assistente.json").exists());
    }

    #[test]
    fn senza_file_v2_non_fa_nulla() {
        let dir = tempfile::tempdir().unwrap();
        let segreti = MemoriaSegreti::new();
        assert_eq!(importa_se_serve(dir.path(), None, &segreti), None);
        assert_eq!(importa_se_serve(dir.path(), Some(&dir.path().join("manca.json")), &segreti), None);
        assert!(!dir.path().join("assistente.json").exists());
    }
}
