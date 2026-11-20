//! I moduli del pannello nel ponte (ridisegno F1): il `Registro` (ordine, riassunti, livelli delle
//! voci) e il modello delle carte, che sopravvive al pannello come gli altri modelli.
//!
//! Nessun timer: il modello si riscrive solo quando qualcosa cambia (`aggiorna_modello` tocca solo
//! le righe diverse) e il tono dell'isola solo se è diverso da quello mostrato.

use super::{aggiorna_modello, Ponte};
use crate::moduli::registro::{tono_ui, Registro};
use crate::{Pannello, TonoModuloUi};
use slint::ModelRc;
use std::cell::Ref;

/// Scrive il tono dell'isola solo se cambia.
fn scrivi_tono_isola(p: &Pannello, tono: TonoModuloUi) {
    if p.get_tono_isola() != tono {
        p.set_tono_isola(tono);
    }
}

impl Ponte {
    /// Il registro dei moduli (il dock ne legge l'ordine).
    pub fn registro(&self) -> Ref<'_, Registro> {
        self.registro.borrow()
    }

    /// Riscrive le carte dal registro (solo le righe cambiate) e il tono dell'isola del pannello
    /// aperto (solo se diverso).
    pub(super) fn moduli_scrivi(&self) {
        let (voci, tono) = {
            let r = self.registro.borrow();
            (r.moduli_ui(), tono_ui(r.tono_isola()))
        };
        aggiorna_modello(&self.modello_moduli, voci);
        if let Some(p) = self.pannello.borrow().upgrade() {
            scrivi_tono_isola(&p, tono);
        }
    }

    /// Il pannello appena creato prende il modello delle carte e il tono dell'isola.
    pub fn moduli_nel_pannello(&self, p: &Pannello) {
        self.moduli_scrivi();
        p.set_moduli(ModelRc::from(self.modello_moduli.clone()));
        scrivi_tono_isola(p, tono_ui(self.registro.borrow().tono_isola()));
    }
}

#[cfg(test)]
mod test {
    use crate::attenzione::Livello;
    use crate::ponte::test::ponte_di_prova;
    use crate::{Pannello, TonoModuloUi};
    use slint::{ComponentHandle, Model};

    /// Il pannello nuovo prende le carte nell'ordine del registro e il tono dell'isola; un tono
    /// alzato da una voce arriva al pannello aperto.
    #[test]
    fn moduli_nel_pannello_e_tono_dell_isola() {
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.moduli_nel_pannello(&p);
        let ids: Vec<String> = p.get_moduli().iter().map(|m| m.id.to_string()).collect();
        assert_eq!(ids, ["agenti", "casa", "pc", "collegamenti"]);
        assert_eq!(p.get_tono_isola(), TonoModuloUi::Vuoto);

        *ponte.pannello.borrow_mut() = p.as_weak();
        assert!(ponte.registro.borrow_mut().livelli_voci(&[("agenti", Livello::Bloccante)]));
        ponte.moduli_scrivi();
        assert_eq!(p.get_tono_isola(), TonoModuloUi::Aspetta);
        assert_eq!(p.get_moduli().row_data(0).map(|m| m.tono), Some(TonoModuloUi::Aspetta));
    }
}
