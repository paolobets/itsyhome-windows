//! I moduli del pannello nel ponte (ridisegno F1): il `Registro` (ordine, riassunti, livelli delle
//! voci) e il modello delle carte, che sopravvive al pannello come gli altri modelli.
//!
//! Nessun timer: un riassunto si ricalcola quando cambia un suo dato (`moduli_aggiorna`, chiamata
//! dai punti in cui il dato cambia), e solo a pannello aperto; da chiuso si segna `moduli_vecchi`
//! e all'apertura si ricalcola tutto una volta. Il modello si riscrive solo se un riassunto è
//! cambiato (`aggiorna_modello` tocca solo le righe diverse) e il tono dell'isola solo se è
//! diverso da quello mostrato.

use super::con_claude::adesso_unix;
use super::{aggiorna_modello, Ponte};
use crate::claude;
use crate::finestra_impostazioni::{apri_alla_sezione, SEZIONE_CASA, SEZIONE_CLAUDE, SEZIONE_COLLEGAMENTI};
use crate::moduli::registro::{tono_ui, Registro};
use crate::moduli::{agenti, casa, collegamenti, pc, Riassunto};
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
        let p = self.pannello.borrow().upgrade();
        self.moduli_scrivi_in(p.as_ref());
    }

    fn moduli_scrivi_in(&self, p: Option<&Pannello>) {
        let (voci, tono) = {
            let r = self.registro.borrow();
            (r.moduli_ui(), tono_ui(r.tono_isola()))
        };
        aggiorna_modello(&self.modello_moduli, voci);
        if let Some(p) = p {
            scrivi_tono_isola(p, tono);
        }
    }

    /// Il pannello appena creato: i riassunti cambiati da chiuso si ricalcolano (una volta), il PC
    /// torna a «—» (i campioni ripartono con il pannello), poi il modello delle carte e il tono
    /// dell'isola, scritti una volta sola.
    pub fn moduli_nel_pannello(&self, p: &Pannello) {
        if self.moduli_vecchi.replace(false) {
            for id in ["agenti", "casa", "collegamenti"] {
                if let Some(r) = self.riassunto_di(id) {
                    self.moduli_accetta(id, r);
                }
            }
            let livelli = self.livelli_delle_voci();
            self.registro.borrow_mut().livelli_voci(&livelli);
        }
        self.moduli_accetta("pc", pc::riassunto(None));
        p.set_moduli(ModelRc::from(self.modello_moduli.clone()));
        self.moduli_scrivi_in(Some(p));
    }

    /// Il riassunto di un modulo dai dati che il ponte tiene (il PC no: i suoi campioni arrivano
    /// dal thread del pannello, `moduli_aggiorna_con`).
    fn riassunto_di(&self, id: &str) -> Option<Riassunto> {
        match id {
            "agenti" => {
                let collegato = self.casa.con_config(|c| c.claude.collegato);
                let (cinque, sette) = self.limiti_a(adesso_unix());
                Some(agenti::riassunto(collegato, &self.claude_sessioni.borrow(), (cinque.as_ref(), sette.as_ref())))
            }
            "casa" => {
                let (stato, menu, dati) = (self.stato.borrow(), self.menu.borrow(), self.dati_delle.borrow());
                Some(casa::riassunto(&stato, menu.as_deref(), dati.as_deref()))
            }
            "collegamenti" => Some(self.casa.con_config(|c| collegamenti::riassunto(&c.collegamenti))),
            _ => None,
        }
    }

    /// Un dato del modulo `id` è cambiato: a pannello aperto se ne ricalcola il riassunto e, se è
    /// cambiato, le carte si riscrivono; a pannello chiuso si segna solo da rifare all'apertura.
    pub fn moduli_aggiorna(&self, id: &'static str) {
        if !self.modelli_vivi.get() {
            self.moduli_vecchi.set(true);
            return;
        }
        if let Some(r) = self.riassunto_di(id) {
            if self.moduli_accetta(id, r) {
                self.moduli_scrivi();
            }
        }
    }

    /// Come `moduli_aggiorna`, col riassunto già calcolato da chi ha il dato (il PC, alla
    /// consegna di un campione). Da chiuso si scarta: all'apertura il PC riparte da «—».
    pub fn moduli_aggiorna_con(&self, id: &'static str, r: Riassunto) {
        if self.modelli_vivi.get() && self.moduli_accetta(id, r) {
            self.moduli_scrivi();
        }
    }

    /// Il riassunto passa al registro; vero se è cambiato. Uno fuori misura resta sullo stderr (il
    /// registro tiene quello di prima).
    fn moduli_accetta(&self, id: &'static str, r: Riassunto) -> bool {
        let esito = self.registro.borrow_mut().aggiorna(id, r);
        esito.unwrap_or_else(|e| {
            eprintln!("riassunto scartato: {e}");
            false
        })
    }

    /// Il livello delle voci «In attesa» per modulo.
    fn livelli_delle_voci(&self) -> Vec<(&'static str, crate::attenzione::Livello)> {
        self.attese.borrow().ordinate().iter().map(|v| (v.fonte.modulo(), v.livello)).collect()
    }

    /// Le voci «In attesa» sono cambiate: alzano (o non alzano più) il tono delle carte.
    pub(super) fn moduli_livelli(&self) {
        if !self.modelli_vivi.get() {
            self.moduli_vecchi.set(true);
            return;
        }
        let livelli = self.livelli_delle_voci();
        if self.registro.borrow_mut().livelli_voci(&livelli) {
            self.moduli_scrivi();
        }
    }

    /// Il bottone d'azione di una carta (`azione-modulo` del pannello): gli id delle azioni sono
    /// quelli dei moduli (`agenti::TERMINALE`…). Va chiamata solo subito dopo il gesto dell'utente.
    pub fn azione_modulo(&self, modulo: &str, azione: &str) {
        match (modulo, azione) {
            ("agenti", agenti::TERMINALE) => {
                // La sessione di cui parla la carta, come il clic sulla sua riga nella vista.
                let impronta = agenti::sessione_dell_azione(&self.claude_sessioni.borrow()).map(|s| claude::id_sessione(&s.id));
                if let Some(impronta) = impronta {
                    self.claude_apri_terminale(&impronta);
                }
            }
            ("agenti", agenti::COLLEGA) => apri_alla_sezione(SEZIONE_CLAUDE, || self.chiedi_impostazioni()),
            ("casa", casa::SPEGNI) => self.spegni_tutte(),
            ("casa", casa::RIPROVA) => self.riprova_casa(),
            ("casa", casa::COLLEGA) => apri_alla_sezione(SEZIONE_CASA, || self.chiedi_impostazioni()),
            ("collegamenti", collegamenti::APRI) => {
                // Il più usato di adesso (lo stesso della carta), aperto come il clic sulla sua
                // card: se il browser parte il pannello si chiude, se no resta aperto con l'errore.
                let id = self.casa.con_config(|c| collegamenti::piu_usato(&c.collegamenti).map(|c| c.id.clone()));
                if id.is_some_and(|id| self.apri_collegamento(&id)) {
                    if let Some(chiudi) = self.chiudi_pannello.borrow().as_ref() {
                        chiudi();
                    }
                }
            }
            ("collegamenti", collegamenti::AGGIUNGI) => apri_alla_sezione(SEZIONE_COLLEGAMENTI, || self.chiedi_impostazioni()),
            _ => eprintln!("azione di modulo sconosciuta: {modulo} {azione}"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::super::con_claude::portati_davanti_prova;
    use crate::attenzione::Livello;
    use crate::moduli::casa;
    use crate::ponte::test::{luce, menu, ponte_di_prova, ponte_di_prova_con};
    use crate::{Pannello, TonoModuloUi};
    use casa_core::eventi::{CasaEvento, StatoConnessione};
    use claude_hook::evento::{EventoHook, TipoEvento};
    use slint::{ComponentHandle, Model};
    use std::sync::Arc;
    use std::time::Instant;

    /// Il pannello nuovo prende le carte nell'ordine del registro e il tono dell'isola; un tono
    /// alzato da una voce arriva al pannello aperto.
    #[test]
    fn moduli_nel_pannello_e_tono_dell_isola() {
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.moduli_nel_pannello(&p);
        let ids: Vec<String> = p.get_moduli().iter().map(|m| m.id.to_string()).collect();
        assert_eq!(ids, ["agenti", "casa", "pc", "collegamenti"]);
        // Claude non collegato: «Da collegare» (vuoto), primo a parità di priorità con la casa calma.
        assert_eq!(p.get_moduli().row_data(0).map(|m| m.titolo.to_string()).as_deref(), Some("Da collegare"));
        assert_eq!(p.get_tono_isola(), TonoModuloUi::Vuoto);

        *ponte.pannello.borrow_mut() = p.as_weak();
        assert!(ponte.registro.borrow_mut().livelli_voci(&[("agenti", Livello::Bloccante)]));
        ponte.moduli_scrivi();
        assert_eq!(p.get_tono_isola(), TonoModuloUi::Aspetta);
        assert_eq!(p.get_moduli().row_data(0).map(|m| m.tono), Some(TonoModuloUi::Aspetta));
    }

    fn titolo(p: &Pannello, i: usize) -> String {
        p.get_moduli().row_data(i).map(|m| m.titolo.to_string()).unwrap_or_default()
    }

    /// I riassunti seguono i dati a pannello aperto: la casa (luci), le voci in attesa (livelli) e
    /// le sessioni di Claude.
    #[test]
    fn le_carte_seguono_i_dati() {
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        assert_eq!(titolo(&p, 1), "Luci spente");
        ponte.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![vec![luce("light.a", true), luce("light.b", true)]]))));
        assert_eq!(titolo(&p, 1), "2 luci");
        assert_eq!(titolo(&p, 2), "—", "il PC aspetta il primo campione");

        // Una voce bloccante della casa alza la carta (livelli collegati in `attese_cambiate`).
        let voce = crate::attenzione::Voce {
            id: "prova.casa".into(),
            livello: Livello::Bloccante,
            fonte: crate::attenzione::Fonte::Casa,
            titolo: "Prova".into(),
            dettaglio: String::new(),
            pulsanti: vec![],
            creata: Instant::now(),
        };
        ponte.metti_voce(voce, false);
        assert_eq!(p.get_moduli().row_data(1).map(|m| m.tono), Some(TonoModuloUi::Aspetta));
        assert_eq!(p.get_tono_isola(), TonoModuloUi::Aspetta);
        ponte.togli_voce("prova.casa");
        assert_eq!(p.get_moduli().row_data(1).map(|m| m.tono), Some(TonoModuloUi::Calmo));

        // Claude collegato, una sessione che lavora.
        ponte.casa().aggiorna_config(|c| c.claude.collegato = true);
        ponte.claude_evento(evento("a", TipoEvento::UserPromptSubmit), 1, Instant::now());
        assert_eq!(titolo(&p, 0), "1 al lavoro");
        assert_eq!(p.get_moduli().row_data(0).map(|m| m.tono), Some(TonoModuloUi::Attivo));
    }

    /// La carta dei Collegamenti segue l'elenco e i lanci; «Apri {nome}» apre il più usato come il
    /// clic sulla sua card, conta il lancio e chiude il pannello solo se il browser è partito.
    #[test]
    fn carta_dei_collegamenti_apre_il_piu_usato() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        assert_eq!((titolo(&p, 3).as_str(), p.get_moduli().row_data(3).map(|m| m.tono)), ("Nessun link", Some(TonoModuloUi::Vuoto)));

        let aperti = Rc::new(RefCell::new(Vec::<String>::new()));
        let a = aperti.clone();
        ponte.su_apri_url(move |url| {
            a.borrow_mut().push(url.to_owned());
            Ok(())
        });
        let chiusure = Rc::new(Cell::new(0));
        let c = chiusure.clone();
        ponte.su_chiudi_pannello(move || c.set(c.get() + 1));
        ponte.casa().aggiorna_config(|c| {
            for (id, nome) in [("nas", "NAS"), ("pve", "Proxmox")] {
                c.collegamenti.push(casa_core::config::Collegamento { id: id.into(), nome: nome.into(), url: format!("https://{id}.esempio.lan/"), ..Default::default() });
            }
        });
        ponte.moduli_aggiorna("collegamenti");
        assert_eq!(titolo(&p, 3), "2 link");
        assert_eq!(p.get_moduli().row_data(3).map(|m| m.narratore.to_string()).as_deref(), Some("Collegamenti, 2 link, Più usato: NAS, modulo 4 di 4, azione: Apri NAS"));

        // Tutti a zero: il primo. Il bottone apre quello e il pannello si chiude.
        ponte.azione_modulo("collegamenti", crate::moduli::collegamenti::APRI);
        assert_eq!(*aperti.borrow(), ["https://nas.esempio.lan/"]);
        assert_eq!(chiusure.get(), 1);
        assert_eq!(ponte.casa().config().collegamenti[0].lanci, 1);
        // Proxmox con più lanci di NAS (1): diventa il più usato e la carta lo dice.
        ponte.casa().aggiorna_config(|c| c.collegamenti[1].lanci = 5);
        ponte.moduli_aggiorna("collegamenti");
        assert!(p.get_moduli().row_data(3).is_some_and(|m| m.narratore.contains("Più usato: Proxmox")));
        // Il browser non parte: il pannello resta aperto e il conteggio non sale. (Un tocco
        // appena fatto blocca i doppi clic fino alla chiusura: qui il pannello è «chiuso».)
        ponte.pannello_chiuso();
        ponte.su_apri_url(|_| Err("ShellExecuteW: 2".into()));
        ponte.azione_modulo("collegamenti", crate::moduli::collegamenti::APRI);
        assert_eq!(chiusure.get(), 1, "nessuna chiusura se il browser non è partito");
        assert_eq!(ponte.casa().config().collegamenti[1].lanci, 5);
    }

    fn evento(id: &str, tipo: TipoEvento) -> EventoHook {
        EventoHook { sessione: id.into(), tipo, cartella: format!("cartella-{id}"), strumento: None, notifica: None, errore: None, riassunto: None }
    }

    /// I bottoni delle carte arrivano ai comandi di sempre: «Spegni tutte» un comando per luce
    /// accesa (al servizio finto: nessuna casa vera), «Apri il terminale» la stessa funzione del
    /// clic su una riga della vista Agenti, sulla sessione di cui la carta parla.
    #[test]
    fn azioni_portano_al_loro_comando() {
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        ponte.ricevi(CasaEvento::Dati(Arc::new(menu(
            vec![],
            vec![vec![luce("light.a", true), luce("light.b", false), luce("light.c", true)]],
        ))));
        ponte.azione_modulo("casa", casa::SPEGNI);
        let numeri: Vec<Option<u64>> = ["light.a", "light.b", "light.c"].iter().map(|id| ponte.ottimismo.borrow().numero(id)).collect();
        assert!(numeri[0].is_some() && numeri[2].is_some(), "un comando per ogni luce accesa: {numeri:?}");
        assert_ne!(numeri[0], numeri[2]);
        assert!(numeri[1].is_none(), "la luce spenta non riceve comandi");

        portati_davanti_prova();
        ponte.claude_evento(evento("a", TipoEvento::UserPromptSubmit), 1, Instant::now());
        ponte.claude_evento(evento("b", TipoEvento::UserPromptSubmit), 2, Instant::now());
        let catena = |n: u32| vec![(n, "claude.exe".to_owned()), (3, "WindowsTerminal.exe".to_owned())];
        ponte.claude_imposta_catena("a", catena(5));
        ponte.claude_imposta_catena("b", catena(6));
        portati_davanti_prova();
        ponte.azione_modulo("agenti", crate::moduli::agenti::TERMINALE);
        let dalla_carta = portati_davanti_prova();
        assert_eq!(dalla_carta, vec![catena(6)], "la sessione più recente che lavora");
        ponte.claude_apri_terminale(&crate::claude::id_sessione("b"));
        assert_eq!(portati_davanti_prova(), dalla_carta, "come il clic sulla riga");
        // Nessuna sessione: niente.
        ponte.claude_evento(evento("a", TipoEvento::SessionEnd), 3, Instant::now());
        ponte.claude_evento(evento("b", TipoEvento::SessionEnd), 4, Instant::now());
        ponte.azione_modulo("agenti", crate::moduli::agenti::TERMINALE);
        assert!(portati_davanti_prova().is_empty());
    }

    /// A pannello chiuso un evento della casa non tocca il registro né scrive `modello_moduli`;
    /// all'apertura il riassunto si rifà coi dati di adesso, una volta.
    #[test]
    fn da_chiuso_non_si_ricalcola() {
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        assert_eq!(titolo(&p, 1), "Luci spente");
        ponte.pannello_chiuso();
        for accese in [1, 3] {
            let luci = (0..accese).map(|i| luce(&format!("light.l{i}"), true)).collect();
            ponte.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![luci]))));
        }
        assert!(ponte.moduli_vecchi.get(), "da rifare all'apertura");
        assert_eq!(ponte.modello_moduli.row_data(1).map(|m| m.titolo.to_string()).as_deref(), Some("Luci spente"), "modello fermo");
        assert_eq!(ponte.registro().nome_narrator(1), "Casa, Luci spente, Nessun sensore, modulo 2 di 4", "registro fermo");
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        // Riapertura: i dati di adesso (l'ultimo), una volta sola.
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        assert_eq!(titolo(&p, 1), "3 luci");
        assert!(!ponte.moduli_vecchi.get(), "rifatti: niente più da rifare");
    }

    /// Da capo a fondo: un `assistente.json` di prima con l'id `claude` arriva al registro e al
    /// dock come `agenti`, nel suo posto (Review Focus 4).
    #[test]
    fn configurazione_con_claude_arriva_al_dock() {
        let config = r#"{ "moduli": [ { "id": "collegamenti" }, { "id": "claude", "fissato": false } ] }"#;
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova_con(Some(config));
        let ordine: Vec<&str> = ponte.registro().ordine().iter().map(|m| m.id).collect();
        assert_eq!(ordine, ["collegamenti", "agenti", "casa", "pc"]);
        let dock: Vec<String> = crate::pannello::voci_app(&ponte.registro()).iter().map(|v| v.id.to_string()).collect();
        assert_eq!(dock, ["panoramica", "collegamenti", "agenti", "casa", "pc"]);
        let p = Pannello::new().expect("pannello");
        ponte.moduli_nel_pannello(&p);
        let carte: Vec<String> = p.get_moduli().iter().map(|m| m.id.to_string()).collect();
        assert_eq!(carte, ["collegamenti", "agenti", "casa", "pc"]);
    }
}
