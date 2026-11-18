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
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Il titolo della finestra delle schede.
const TITOLO: &str = "API Claude";
/// L'azione «Aggiorna» della finestra.
const AZIONE_AGGIORNA: &str = "aggiorna";
/// Fra due «Aggiorna» dell'utente passa almeno tanto: i dati arrivano con 4–24 ore di ritardo,
/// leggere più spesso non dà nulla e le API hanno i loro limiti.
const INTERVALLO_AGGIORNA: Duration = Duration::from_secs(60);

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
        self.mostra_dettagli(false);
    }

    /// Le schede di adesso: dall'ultima lettura, o la frase del perché non ci sono. Un avviso di
    /// «Aggiorna» (una volta sola) o, se una lettura è in corso, «Lettura in corso…» precede la nota.
    pub(crate) fn schede_da_mostrare(&self) -> Vec<SchedaDati> {
        let mut schede = match &*self.api_ultimo.borrow() {
            None => claude::schede_vuote("Nessuna lettura ancora: la prima sta partendo"),
            Some((ora, Err(e))) => claude::schede_vuote(&format!("Lettura delle {ora} non riuscita: {e}")),
            Some((ora, Ok(r))) => claude::schede(r, ora),
        };
        let avviso = self
            .dettagli_avviso
            .borrow_mut()
            .take()
            .or_else(|| self.api_in_corso.borrow().load(Ordering::SeqCst).then(|| "Lettura in corso…".to_owned()));
        if let Some(avviso) = avviso {
            for s in &mut schede {
                s.nota = format!("{avviso} · {}", s.nota);
            }
        }
        schede
    }

    /// Manda le schede alla finestra. `solo_se_viva`: è un aggiornamento che nessuno ha chiesto
    /// (una lettura è finita, o un avviso): va solo a una finestra ancora aperta e non ne fa mai
    /// partire una.
    fn mostra_dettagli(&self, solo_se_viva: bool) {
        let schede = self.schede_da_mostrare();
        let mut gestione = self.gestione.borrow_mut();
        if solo_se_viva && !gestione.as_ref().is_some_and(Gestione::viva) {
            return;
        }
        let gestione = gestione.get_or_insert_with(|| {
            let (cartella, _) = crate::avvio::cartelle(std::env::var_os(crate::avvio::VAR_CARTELLA));
            // Il guardiano gira su un suo thread: ogni messaggio passa all'interfaccia, col numero
            // del processo che l'ha mandato.
            Gestione::new(&cartella, |numero, m| {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(p) = ponte() {
                        p.dettagli_messaggio(numero, m);
                    }
                });
            })
        });
        match gestione.apri_schede(TITOLO, &schede) {
            Ok(numero) => {
                self.dettagli_gen.set(numero);
                self.dettagli_aperti.set(true);
            }
            Err(e) => eprintln!("dettagli delle API: {e}"),
        }
    }

    /// Un messaggio della finestra (sul thread dell'interfaccia). `numero` è la generazione del
    /// processo che l'ha mandato: un messaggio di un processo che non è più la finestra di adesso
    /// (una `chiusa` tardiva della vecchia) non conta.
    pub(crate) fn dettagli_messaggio(&self, numero: u64, m: Messaggio) {
        if numero != self.dettagli_gen.get() {
            return;
        }
        match m {
            Messaggio::Azione { id } => self.dettagli_azione(&id),
            Messaggio::Chiusa => {
                self.dettagli_aperti.set(false);
                // Un avviso non consumato non deve comparire ore dopo, in un'altra finestra.
                *self.dettagli_avviso.borrow_mut() = None;
            }
            _ => {}
        }
    }

    /// Un'azione della finestra: «Aggiorna» o una pagina della Console; il resto si ignora.
    pub(crate) fn dettagli_azione(&self, id: &str) {
        if id == AZIONE_AGGIORNA {
            self.dettagli_aggiorna(Instant::now());
        } else if let Some(pagina) = PaginaConsole::da_id(id) {
            apri_console(pagina);
        }
    }

    /// «Aggiorna» dell'utente a `ora`: una lettura per la strada di sempre, ma non prima di
    /// `INTERVALLO_AGGIORNA` dall'ultima chiesta così; nel frattempo la finestra lo dice.
    pub(crate) fn dettagli_aggiorna(&self, ora: Instant) {
        if let Some(prima) = self.dettagli_ultimo_aggiorna.get() {
            let passato = ora.saturating_duration_since(prima);
            if passato < INTERVALLO_AGGIORNA {
                let mancano = (INTERVALLO_AGGIORNA - passato).as_secs() + 1;
                *self.dettagli_avviso.borrow_mut() = Some(format!("Appena aggiornato: riprova fra {mancano} s"));
                self.dettagli_dati_nuovi();
                return;
            }
        }
        if self.dettagli_chiedi_lettura() {
            self.dettagli_ultimo_aggiorna.set(Some(ora));
        }
        // La finestra mostra subito «Lettura in corso…».
        self.dettagli_dati_nuovi();
    }

    /// Una lettura per la strada di sempre (un solo giro alla volta); senza la chiave non c'è nulla
    /// da leggere. Vero se è stata chiesta.
    fn dettagli_chiedi_lettura(&self) -> bool {
        if !self.sveglia_api.running() {
            return false;
        }
        #[cfg(test)]
        LETTURE_CHIESTE.with(|c| c.set(c.get() + 1));
        #[cfg(not(test))]
        self.api_leggi();
        true
    }

    /// Una lettura nuova è arrivata (o va mostrato un avviso): a una finestra ancora aperta
    /// rimpiazza i dati; una finestra chiusa non si riapre mai da sé.
    pub(crate) fn dettagli_dati_nuovi(&self) {
        if self.dettagli_aperti.get() {
            self.mostra_dettagli(true);
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
        p.dettagli_messaggio(0, Messaggio::Azione {
            id: "console:uso".into(),
        });
        p.dettagli_messaggio(0, Messaggio::Pronta);
        p.dettagli_messaggio(0, Messaggio::Apri {
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
        assert_eq!(s[0].righe[0], ["Mese in corso", "412,80 $"]);
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

    #[test]
    fn aggiorna_ha_un_intervallo_minimo_e_lo_dice() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        letture();
        let t0 = Instant::now();
        p.dettagli_aggiorna(t0);
        assert_eq!(letture(), 1);
        // Dopo 10 s: niente lettura, e l'avviso si legge una volta sola nella nota.
        p.dettagli_aggiorna(t0 + Duration::from_secs(10));
        assert_eq!(letture(), 0, "troppo presto");
        let s = p.schede_da_mostrare();
        assert!(s.iter().all(|x| x.nota.starts_with("Appena aggiornato: riprova fra 51 s · ")), "{:?}", s[0].nota);
        assert!(!p.schede_da_mostrare()[0].nota.contains("Appena aggiornato"), "una volta sola");
        // Allo scadere dei 60 s si legge di nuovo.
        p.dettagli_aggiorna(t0 + Duration::from_secs(60));
        assert_eq!(letture(), 1);
        p.dettagli_aggiorna(t0 + Duration::from_secs(61));
        assert_eq!(letture(), 0, "l'intervallo riparte dall'ultima richiesta");
    }

    #[test]
    fn con_una_lettura_in_corso_la_nota_lo_dice() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        assert!(!p.schede_da_mostrare()[0].nota.starts_with("Lettura in corso"));
        p.api_in_corso.borrow().store(true, Ordering::SeqCst);
        assert!(p.schede_da_mostrare().iter().all(|x| x.nota.starts_with("Lettura in corso… · ")));
        p.api_in_corso.borrow().store(false, Ordering::SeqCst);
        assert!(!p.schede_da_mostrare()[0].nota.starts_with("Lettura in corso"));
    }

    #[test]
    fn una_chiusa_tardiva_di_un_processo_vecchio_non_chiude_la_finestra_nuova() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        // La finestra di adesso è il processo 2; la `chiusa` del processo 1 arriva dopo.
        p.dettagli_gen.set(2);
        p.dettagli_aperti.set(true);
        p.dettagli_messaggio(1, Messaggio::Chiusa);
        assert!(p.dettagli_aperti.get(), "la chiusa del processo vecchio non conta");
        aperte();
        p.dettagli_messaggio(1, Messaggio::Azione { id: "console:spesa".into() });
        assert!(aperte().is_empty(), "nemmeno le sue azioni");
        *p.dettagli_avviso.borrow_mut() = Some("Appena aggiornato: riprova fra 9 s".into());
        p.dettagli_messaggio(2, Messaggio::Chiusa);
        assert!(!p.dettagli_aperti.get());
        assert!(!p.schede_da_mostrare()[0].nota.contains("Appena aggiornato"), "l'avviso non sopravvive alla finestra");
    }

    #[test]
    fn una_lettura_non_riapre_mai_la_finestra() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        // Finestra aperta secondo il ponte ma senza nessun processo vivo (chiusa dall'utente, la
        // `chiusa` ancora in coda): i dati nuovi non ne fanno partire uno.
        p.dettagli_aperti.set(true);
        p.dettagli_dati_nuovi();
        assert!(p.gestione.borrow().is_none(), "nessun processo avviato");
        // E con la finestra chiusa non si manda nulla nemmeno a un `Gestione` che c'è.
        p.dettagli_aperti.set(false);
        p.dettagli_dati_nuovi();
        assert!(p.gestione.borrow().is_none());
    }
}
