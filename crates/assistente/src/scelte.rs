//! Le scelte dell'utente che il pannello ricorda tra un avvio e l'altro (oggi: il layout).
//! Passano da qui sole, così il Task 8 le può instradare su `CasaService` in un punto.

use crate::layout::{self, Layout};
use casa_core::config::{cartella_config, ConfigStore};
use std::path::Path;

/// Il layout salvato; Orologio se non c'è o non è valido.
pub fn layout_salvato() -> Layout {
    layout_salvato_in(&cartella_config())
}

/// Salva il layout scelto (errori solo nel log: non è grave perderlo).
pub fn salva_layout(l: Layout) {
    if let Err(e) = salva_layout_in(&cartella_config(), l) {
        eprintln!("layout non salvato: {e}");
    }
}

fn layout_salvato_in(dir: &Path) -> Layout {
    layout::da_str(&ConfigStore::apri(dir).leggi().ultimo_layout)
}

/// Rilegge la configurazione e cambia solo `ultimo_layout`; se è già quello non scrive.
fn salva_layout_in(dir: &Path, l: Layout) -> std::io::Result<()> {
    let store = ConfigStore::apri(dir);
    let mut config = store.leggi();
    if config.ultimo_layout == l.come_str() {
        return Ok(());
    }
    config.ultimo_layout = l.come_str().to_owned();
    store.salva(&config)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn senza_file_parte_dall_orologio() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(layout_salvato_in(dir.path()), Layout::Orologio);
    }

    #[test]
    fn layout_salvato_e_riletto_senza_toccare_il_resto() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::apri(dir.path());
        let mut c = store.leggi();
        c.favorites = vec!["light.cucina".into()];
        c.launch_at_login = true;
        store.salva(&c).unwrap();

        salva_layout_in(dir.path(), Layout::Colori).unwrap();
        assert_eq!(layout_salvato_in(dir.path()), Layout::Colori);
        let dopo = store.leggi();
        assert_eq!(dopo.favorites, ["light.cucina"]);
        assert!(dopo.launch_at_login);
        assert_eq!(dopo.ultimo_layout, "colori");
    }
}
