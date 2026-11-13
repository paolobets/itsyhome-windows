//! Le scelte dell'utente che il pannello ricorda tra un avvio e l'altro (oggi: il layout).
//! Passano dalla configurazione di `CasaService`, unico proprietario della `Config` in memoria:
//! nessuno rilegge o riscrive `assistente.json` alle sue spalle.

use crate::layout::{self, Layout};
use casa_core::servizio::CasaService;

/// Il layout salvato; Orologio se non c'è o non è valido.
pub fn layout_salvato(casa: &CasaService) -> Layout {
    layout::da_str(&casa.config().ultimo_layout)
}

/// Salva il layout scelto (solo `ultimo_layout`; se è già quello non si scrive nulla).
pub fn salva_layout(casa: &CasaService, l: Layout) {
    casa.aggiorna_config(|c| c.ultimo_layout = l.come_str().to_owned());
}

#[cfg(test)]
mod test {
    use super::*;
    use casa_core::config::ConfigStore;
    use casa_core::segreti::MemoriaSegreti;
    use std::sync::Arc;

    fn servizio(dir: &std::path::Path) -> CasaService {
        CasaService::avvia(Arc::new(ConfigStore::apri(dir)), Arc::new(MemoriaSegreti::new()), Arc::new(|_| {}))
    }

    #[test]
    fn senza_file_parte_dall_orologio() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(layout_salvato(&servizio(dir.path())), Layout::Orologio);
    }

    #[test]
    fn layout_salvato_e_riletto_senza_toccare_il_resto() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::apri(dir.path());
        let mut c = store.leggi();
        c.favorites = vec!["light.cucina".into()];
        c.launch_at_login = true;
        store.salva(&c).unwrap();

        let casa = servizio(dir.path());
        salva_layout(&casa, Layout::Quadranti);
        assert_eq!(layout_salvato(&casa), Layout::Quadranti);
        // Il salvataggio avviene sul thread del servizio: chiuderlo lo porta a termine.
        drop(casa);
        let dopo = store.leggi();
        assert_eq!(dopo.favorites, ["light.cucina"]);
        assert!(dopo.launch_at_login);
        assert_eq!(dopo.ultimo_layout, "quadranti");
        assert_eq!(layout_salvato(&servizio(dir.path())), Layout::Quadranti);
    }

    #[test]
    fn layout_tolto_salvato_si_riapre_sull_orologio() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::apri(dir.path());
        let mut c = store.leggi();
        c.ultimo_layout = "colori".into();
        store.salva(&c).unwrap();
        assert_eq!(layout_salvato(&servizio(dir.path())), Layout::Orologio);
    }
}
