//! Il dettaglio delle API nella finestra di gestione (M3, Task 11, spec §6.8): «Dettagli» nella vista
//! Claude apre le schede (Spesa, Token, Chiavi, Workspace e membri, Limiti), in sola lettura.
//!
//! La finestra è il processo `itsyhome-gestione` (nessuna rete, nessun riquadro web per le schede):
//! da lei arrivano solo due cose, ed entrambe si controllano qui.
//! - `aggiorna`: una lettura delle API per la strada di sempre (`api_leggi`: un solo giro alla volta,
//!   guardato dalla generazione); a finestra aperta i dati nuovi la rimpiazzano da soli;
//! - `console:…`: l'id di una pagina della Console (`PaginaConsole::da_id`), che si apre nel browser;
//!   ogni altro id si ignora, e nessun indirizzo passa dalla finestra.
//!
//! In prova (`ITSYHOME_ASSISTENTE_DIR`) la Console non si apre: l'indirizzo scelto va solo nel log.

use crate::claude;
use crate::gestione::Gestione;
use crate::ponte::{ponte, Ponte};
use crate::sistema::browser::PaginaConsole;
use gestione_protocollo::{Messaggio, SchedaDati};

/// Il titolo della finestra delle schede.
const TITOLO: &str = "API Claude";
/// L'azione «Aggiorna» della finestra.
const AZIONE_AGGIORNA: &str = "aggiorna";

#[cfg(test)]
thread_local! {
    /// Le pagine della Console che si sarebbero aperte (nei test non si apre nessun browser).
    static CONSOLE_APERTE: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Quante letture delle API si sono chieste (nei test non si esce in rete).
    static LETTURE_CHIESTE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl Ponte {
    /// «Dettagli» (clic o Invio nella vista Claude): apre la finestra con le schede dell'ultima
    /// lettura. Se non ce n'è ancora una, la chiede.
    pub(crate) fn claude_apri_dettagli(&self) {
        if self.api_ultimo.borrow().is_none() {
            self.dettagli_chiedi_lettura();
        }
        self.mostra_dettagli();
    }

    /// Le schede di adesso: dall'ultima lettura, o la frase del perché non ci sono.
    pub(crate) fn schede_da_mostrare(&self) -> Vec<SchedaDati> {
        match &*self.api_ultimo.borrow() {
            None => claude::schede_vuote("Nessuna lettura ancora: la prima sta partendo"),
            Some((ora, Err(e))) => {
                claude::schede_vuote(&format!("Lettura delle {ora} non riuscita: {e}"))
            }
            Some((ora, Ok(r))) => claude::schede(r, ora),
        }
    }

    fn mostra_dettagli(&self) {
        let schede = self.schede_da_mostrare();
        let mut gestione = self.gestione.borrow_mut();
        let gestione = gestione.get_or_insert_with(|| {
            let (cartella, _) =
                crate::avvio::cartelle(std::env::var_os(crate::avvio::VAR_CARTELLA));
            // Il guardiano gira su un suo thread: ogni messaggio passa all'interfaccia.
            Gestione::new(&cartella, |m| {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(p) = ponte() {
                        p.dettagli_messaggio(m);
                    }
                });
            })
        });
        match gestione.apri_schede(TITOLO, schede) {
            Ok(()) => self.dettagli_aperti.set(true),
            Err(e) => eprintln!("dettagli delle API: {e}"),
        }
    }

    /// Un messaggio della finestra (sul thread dell'interfaccia).
    pub(crate) fn dettagli_messaggio(&self, m: Messaggio) {
        match m {
            Messaggio::Azione { id } => self.dettagli_azione(&id),
            Messaggio::Chiusa => self.dettagli_aperti.set(false),
            _ => {}
        }
    }

    /// Un'azione della finestra: «Aggiorna» o una pagina della Console; il resto si ignora.
    pub(crate) fn dettagli_azione(&self, id: &str) {
        if id == AZIONE_AGGIORNA {
            self.dettagli_chiedi_lettura();
        } else if let Some(pagina) = PaginaConsole::da_id(id) {
            apri_console(pagina);
        }
    }

    /// Una lettura per la strada di sempre (un solo giro alla volta); senza la chiave non c'è nulla
    /// da leggere.
    fn dettagli_chiedi_lettura(&self) {
        if !self.sveglia_api.running() {
            return;
        }
        #[cfg(test)]
        LETTURE_CHIESTE.with(|c| c.set(c.get() + 1));
        #[cfg(not(test))]
        self.api_leggi();
    }

    /// Una lettura nuova è arrivata: a finestra aperta rimpiazza i dati.
    pub(crate) fn dettagli_dati_nuovi(&self) {
        if self.dettagli_aperti.get() {
            self.mostra_dettagli();
        }
    }

    /// La chiave è stata tolta: i dati non ci sono più e la finestra si chiude.
    pub(crate) fn dettagli_chiudi(&self) {
        if let Some(g) = self.gestione.borrow_mut().as_mut() {
            g.chiudi();
        }
        self.dettagli_aperti.set(false);
    }

    /// All'uscita: il processo della finestra si chiude e si raccoglie.
    pub(crate) fn dettagli_spegni(&self) {
        drop(self.gestione.borrow_mut().take());
    }
}

/// Apre la pagina nel browser predefinito (in prova solo nel log, nei test in un elenco).
fn apri_console(pagina: PaginaConsole) {
    let url = pagina.url();
    #[cfg(test)]
    CONSOLE_APERTE.with(|v| v.borrow_mut().push(url));
    #[cfg(not(test))]
    if super::api::in_prova() {
        eprintln!("Console: in prova non si apre, la pagina scelta è {url}");
    } else if let Err(e) = crate::sistema::browser::apri(url) {
        eprintln!("Console: pagina non aperta: {e}");
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ponte::test::ponte_di_prova;

    fn aperte() -> Vec<&'static str> {
        CONSOLE_APERTE.with(|v| std::mem::take(&mut *v.borrow_mut()))
    }

    fn letture() -> u32 {
        LETTURE_CHIESTE.with(|c| c.replace(0))
    }

    #[test]
    fn la_finestra_apre_solo_le_pagine_dell_elenco() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        aperte();
        p.dettagli_azione("console:spesa");
        p.dettagli_azione("console:limiti");
        assert_eq!(
            aperte(),
            [
                "https://platform.claude.com/cost",
                "https://platform.claude.com/usage/limits"
            ]
        );
        for no in [
            "console:altro",
            "https://evil.example/",
            "http://platform.claude.com/cost",
            "",
            "console:",
            "Aggiorna",
            "console:spesa?x=1",
        ] {
            p.dettagli_azione(no);
        }
        assert!(aperte().is_empty(), "ogni altro id si ignora");
        // Anche per il filo del protocollo: solo `azione` conta.
        p.dettagli_messaggio(Messaggio::Azione {
            id: "console:uso".into(),
        });
        p.dettagli_messaggio(Messaggio::Pronta);
        p.dettagli_messaggio(Messaggio::Apri {
            titolo: "x".into(),
            html: "<p>".into(),
        });
        assert_eq!(aperte(), ["https://platform.claude.com/usage"]);
    }

    #[test]
    fn aggiorna_chiede_una_lettura_solo_con_la_chiave() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        letture();
        p.dettagli_azione("aggiorna");
        assert_eq!(letture(), 0, "senza la chiave non c'è nulla da leggere");
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        p.dettagli_azione("aggiorna");
        assert_eq!(letture(), 1);
        p.dettagli_azione("aggiorna ");
        p.dettagli_azione("console:aggiorna");
        assert_eq!(letture(), 0);
    }

    #[test]
    fn le_schede_dicono_perche_non_ci_sono_o_mostrano_la_lettura() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let s = p.schede_da_mostrare();
        assert_eq!(s.len(), 5);
        assert!(s
            .iter()
            .all(|x| x.righe.is_empty() && x.nota.starts_with("Nessuna lettura ancora")));
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        *p.api_ultimo.borrow_mut() =
            Some(("14:05".into(), Err("Il portachiavi non risponde.".into())));
        assert!(p
            .schede_da_mostrare()
            .iter()
            .all(|x| x.nota == "Lettura delle 14:05 non riuscita: Il portachiavi non risponde."));
        let r = api_claude::Riepilogo {
            spesa_mese: api_claude::importo_da("41280"),
            ..Default::default()
        };
        *p.api_ultimo.borrow_mut() = Some(("14:05".into(), Ok(r)));
        let s = p.schede_da_mostrare();
        assert_eq!(s[0].righe[0], ["Mese in corso", "", "412,80 $"]);
    }

    #[test]
    fn senza_l_eseguibile_della_finestra_non_si_apre_e_non_si_blocca() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.claude_apri_dettagli();
        assert!(
            !p.dettagli_aperti.get(),
            "nessun processo: nessuna finestra"
        );
        p.dettagli_dati_nuovi();
        p.dettagli_chiudi();
        p.dettagli_spegni();
    }
}
