//! I moduli del pannello nel ponte (ridisegno F1): il `Registro` (ordine, riassunti, livelli delle
//! voci) e il modello delle carte, che sopravvive al pannello come gli altri modelli.
//!
//! Nessun timer: un riassunto si ricalcola quando cambia un suo dato (`moduli_aggiorna`, chiamata
//! dai punti in cui il dato cambia), e solo a pannello aperto; da chiuso si segna `moduli_vecchi`
//! e all'apertura si ricalcola tutto una volta. Il modello si riscrive solo se un riassunto è
//! cambiato (`aggiorna_modello` tocca solo le righe diverse) e il tono dell'isola solo se è
//! diverso da quello mostrato.

use super::{aggiorna_modello, luci_accese_ora, Ponte};
use crate::claude;
use crate::finestra_impostazioni::{apri_alla_sezione, SEZIONE_CASA, SEZIONE_CLAUDE, SEZIONE_COLLEGAMENTI};
use crate::moduli::registro::tono_ui;
use crate::moduli::{agenti, casa, collegamenti, pagina_agenti, pagina_casa, pagina_collegamenti, pc, Riassunto};
use crate::orologio::adesso_unix;
use crate::pannello::schede_ui;
use crate::{Pannello, TonoModuloUi};
use slint::ModelRc;

/// Scrive il tono dell'isola solo se cambia.
fn scrivi_tono_isola(p: &Pannello, tono: TonoModuloUi) {
    if p.get_tono_isola() != tono {
        p.set_tono_isola(tono);
    }
}

impl Ponte {
    /// Il registro dei moduli, per i test (le schede lo leggono da `scrivi_schede`).
    #[cfg(test)]
    pub fn registro(&self) -> std::cell::Ref<'_, crate::moduli::registro::Registro> {
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
            // Il tono di un modulo dà il segno e il nome per Narrator della sua scheda.
            self.scrivi_schede(p);
            // Un modulo che dà il primo riassunto cambia la fascia dell'Orologio («Avvio in corso»).
            self.scrivi_fascia(p);
        }
    }

    /// Riscrive le schede della barra per la vista di adesso (geometria, segni, nomi per Narrator)
    /// e restituisce le righe riscritte: solo quelle cambiate (`aggiorna_modello`). La chiamano il
    /// cambio di vista (`vista-cambiata`) e `moduli_scrivi`.
    pub fn scrivi_schede(&self, p: &Pannello) -> u64 {
        let schede = schede_ui(&self.registro.borrow(), p.get_vista().as_str());
        aggiorna_modello(&self.modello_schede, schede)
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
        p.set_schede(ModelRc::from(self.modello_schede.clone()));
        self.moduli_scrivi_in(Some(p));
    }

    /// Il riassunto di un modulo dai dati che il ponte tiene (il PC no: i suoi campioni arrivano
    /// dal thread del pannello, `moduli_aggiorna_con`).
    fn riassunto_di(&self, id: &str) -> Option<Riassunto> {
        match id {
            "agenti" => {
                let pagina = self.permessi_della_pagina();
                Some(self.riassunto_agenti(&Self::permessi_dal_piu_vecchio(&pagina), adesso_unix()))
            }
            "casa" => {
                let (stato, menu, dati) = (self.stato.borrow(), self.menu.borrow(), self.dati_delle.borrow());
                // Le luci accese come si vedono adesso (con l'ottimismo dei tocchi): la stessa
                // vista della pagina e di «Spegni tutte».
                let accese = menu.as_deref().map_or(0, |m| luci_accese_ora(m, &self.ottimismo.borrow()).len());
                Some(casa::riassunto(&stato, menu.as_deref(), accese, dati.as_deref()))
            }
            "collegamenti" => Some(self.casa.con_config(|c| collegamenti::riassunto(&c.collegamenti))),
            _ => None,
        }
    }

    /// La carta di Agenti (e il suo `r1`, il titolo della sintesi della pagina) a un'ora data.
    /// `permessi`: i permessi che la pagina mostra, **uno per sessione con riga**, dal più vecchio
    /// (`permessi_dal_piu_vecchio`). È la stessa `permessi_della_pagina` della pagina: «{n} ti
    /// aspettano» conta le sessioni con un permesso, non i permessi, perché è il numero di righe
    /// che la pagina porta in alto.
    pub(super) fn riassunto_agenti(&self, permessi: &[agenti::PermessoInAttesa], ora: i64) -> Riassunto {
        let collegato = self.casa.con_config(|c| c.claude.collegato);
        let (cinque, sette) = self.limiti_a(ora);
        agenti::riassunto(collegato, &self.claude_sessioni.borrow(), (cinque.as_ref(), sette.as_ref()), permessi)
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
                // Con un permesso in attesa la carta parla del permesso e non ha azione.
                let pagina = self.permessi_della_pagina();
                let permessi = Self::permessi_dal_piu_vecchio(&pagina);
                let impronta =
                    agenti::sessione_dell_azione(&self.claude_sessioni.borrow(), &permessi).map(|s| claude::id_sessione(&s.id));
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
                if let Some(id) = id {
                    self.lancia_collegamento(&id);
                }
            }
            ("collegamenti", collegamenti::AGGIUNGI) => apri_alla_sezione(SEZIONE_COLLEGAMENTI, || self.chiedi_impostazioni()),
            _ => eprintln!("azione di modulo sconosciuta: {modulo} {azione}"),
        }
    }

    /// Un'azione di una pagina (`azione-pagina` del pannello, ridisegno F2): (id del modulo, id
    /// dell'azione, id della riga o ""). Nella pagina Agenti la riga è l'impronta della sessione:
    /// «Apri il terminale» la porta davanti (la stessa funzione della carta), «Collega» come la
    /// carta, «Dettagli» le schede delle API. Il permesso di una riga non è un'azione della pagina:
    /// lo decide il dettaglio stesso, dalla guardia di `azione-voce` (Task 7). Nella pagina Casa
    /// (ridisegno F3, Task 4b) «Spegni tutte», «Riprova» e «Collega» sono quelle della carta,
    /// «Spegni la stanza» spegne la stanza del filtro scelto. Va chiamata solo subito dopo il gesto
    /// dell'utente. In Collegamenti (Task 5) «Modifica» e «Aggiungi un collegamento» aprono le
    /// Impostazioni sulla scheda Collegamenti, come «Aggiungi» della carta.
    pub fn azione_pagina(&self, modulo: &str, azione: &str, riga: &str) {
        match (modulo, azione) {
            ("agenti", agenti::TERMINALE) => self.claude_apri_terminale(riga),
            ("agenti", agenti::COLLEGA) => self.azione_modulo(modulo, azione),
            ("agenti", pagina_agenti::DETTAGLI) => self.claude_apri_dettagli(),
            ("casa", casa::SPEGNI | casa::RIPROVA | casa::COLLEGA) => self.azione_modulo(modulo, azione),
            ("casa", pagina_casa::SPEGNI_STANZA) => self.spegni_la_stanza_scelta(),
            ("collegamenti", pagina_collegamenti::MODIFICA | collegamenti::AGGIUNGI) => {
                apri_alla_sezione(SEZIONE_COLLEGAMENTI, || self.chiedi_impostazioni());
            }
            _ => eprintln!("azione di pagina sconosciuta: {modulo} {azione}"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::super::con_claude::portati_davanti_prova;
    use crate::attenzione::Livello;
    use crate::moduli::casa;
    use crate::ponte::test::{luce, menu, ponte_di_prova, ponte_di_prova_con};
    use crate::test_ui::{etichetta_col_fuoco, tasto};
    use crate::{Pannello, TonoModuloUi};
    use casa_core::eventi::{CasaEvento, StatoConnessione};
    use claude_hook::evento::{EventoHook, TipoEvento};
    use i_slint_backend_testing::ElementHandle;
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
            permesso: None,
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

    /// Un permesso in attesa di `sessione`, come lo manda Claude Code (il canale non c'è: prova).
    fn chiedi_permesso(ponte: &crate::ponte::Ponte, numero: u64, sessione: &str, sequenza: u64) {
        let e = EventoHook { strumento: Some("Bash".into()), riassunto: Some("npm test".into()), ..evento(sessione, TipoEvento::PermissionRequest) };
        ponte.claude_permesso(numero, e, None, sequenza, Instant::now());
    }

    /// Il pannello aperto con la pagina Agenti viva e Claude collegato.
    fn pagina_agenti_aperta() -> (Pannello, std::rc::Rc<crate::ponte::Ponte>, impl Sized) {
        let (spegni, ponte, pillola, cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        ponte.casa().aggiorna_config(|c| c.claude.collegato = true);
        (p, ponte, (spegni, pillola, cartella))
    }

    /// m2: la carta di Agenti dice il permesso come la pagina. Un permesso in attesa: «1 ti aspetta»
    /// col tono Aspetta e senza azione (il permesso si decide nella pagina); tolto, la carta torna
    /// a parlare della sessione.
    #[test]
    fn carta_agenti_segue_i_permessi() {
        let (p, ponte, _tieni) = pagina_agenti_aperta();
        ponte.claude_evento(evento("a", TipoEvento::UserPromptSubmit), 1, Instant::now());
        assert_eq!(titolo(&p, 0), "1 al lavoro");
        chiedi_permesso(&ponte, 9, "a", 2);
        let carta = p.get_moduli().row_data(0).expect("la carta di Agenti");
        assert_eq!(
            (carta.titolo.as_str(), carta.tono, carta.parola.as_str(), carta.sec.as_str(), carta.con_azione),
            ("1 ti aspetta", TonoModuloUi::Aspetta, "Aspetta", "cartella-a: Comando", false)
        );
        // Un altro permesso, di un'altra sessione: la carta conta le sessioni.
        ponte.claude_evento(evento("b", TipoEvento::UserPromptSubmit), 3, Instant::now());
        chiedi_permesso(&ponte, 10, "b", 4);
        let carta = p.get_moduli().row_data(0).expect("la carta di Agenti");
        assert_eq!((carta.titolo.as_str(), carta.sec.as_str()), ("2 ti aspettano", "2 permessi"));
        // «Apri il terminale» non c'è, e se arriva lo stesso (clic su una carta vecchia) non fa nulla.
        portati_davanti_prova();
        ponte.azione_modulo("agenti", crate::moduli::agenti::TERMINALE);
        assert!(portati_davanti_prova().is_empty(), "con un permesso la carta non ha azione");
        // Tolti i permessi la carta torna a parlare delle sessioni.
        ponte.claude_decidi(9, claude_hook::Decisione::Nega, Instant::now());
        assert_eq!(titolo(&p, 0), "1 ti aspetta");
        ponte.claude_decidi(10, claude_hook::Decisione::Nega, Instant::now());
        assert_eq!(titolo(&p, 0), "2 al lavoro");
        assert_eq!(p.get_moduli().row_data(0).map(|m| (m.tono, m.con_azione)), Some((TonoModuloUi::Attivo, true)));
    }

    /// C14(a): il titolo della sintesi della pagina e `r1.fatto` del registro per «agenti» sono lo
    /// stesso testo, per ogni gradino. Il confronto passa dal ponte (la pagina scrive la sintesi, il
    /// registro tiene il riassunto), con in più il testo atteso: non basta che coincidano.
    #[test]
    fn titolo_della_sintesi_e_r1() {
        let (p, ponte, _tieni) = pagina_agenti_aperta();
        let aggiorna = || {
            ponte.moduli_aggiorna("agenti");
            ponte.aggiorna_vista_claude_a(crate::orologio::adesso_unix());
        };
        let titolo_pagina = || p.get_elenco_sintesi().titolo.to_string();
        let r1 = || ponte.registro().r1("agenti").map(|r| r.fatto.clone()).unwrap_or_default();

        // Scollegata.
        ponte.casa().aggiorna_config(|c| c.claude.collegato = false);
        ponte.vista_claude(true);
        aggiorna();
        let mut visti = Vec::new();
        let mut passo = |atteso: &str| {
            aggiorna();
            assert_eq!(titolo_pagina(), atteso, "la sintesi della pagina");
            assert_eq!(r1(), atteso, "r1 del registro");
            visti.push(atteso.to_owned());
        };
        passo("Da collegare");
        // Collegata, nessuna sessione.
        ponte.casa().aggiorna_config(|c| c.claude.collegato = true);
        passo("A riposo");
        // Due al lavoro.
        ponte.claude_evento(evento("a", TipoEvento::UserPromptSubmit), 1, Instant::now());
        ponte.claude_evento(evento("b", TipoEvento::UserPromptSubmit), 2, Instant::now());
        passo("2 al lavoro");
        // Un permesso, poi due (di due sessioni).
        chiedi_permesso(&ponte, 5, "a", 3);
        passo("1 ti aspetta");
        chiedi_permesso(&ponte, 6, "b", 4);
        passo("2 ti aspettano");
        // Due permessi della stessa sessione: la pagina porta una riga sola, la carta conta una sessione.
        ponte.claude_decidi(6, claude_hook::Decisione::Nega, Instant::now());
        chiedi_permesso(&ponte, 7, "a", 5);
        passo("1 ti aspetta");
        assert_eq!(visti.len(), 6);
    }

    /// C14(b): i permessi della pagina, una mappa senza ordine, arrivano alla carta dal più vecchio
    /// (ora della richiesta, poi numero della voce) sempre allo stesso modo.
    #[test]
    fn permessi_dal_piu_vecchio_in_modo_deterministico() {
        use crate::attenzione::DatiPermesso;
        use std::collections::HashMap;
        let dati = |alle| DatiPermesso { alle, ..Default::default() };
        let mappa: HashMap<String, (String, i32, DatiPermesso)> = [
            ("sessione-c", "claude.permesso.10", 100),
            ("sessione-a", "claude.permesso.9", 100),
            ("sessione-d", "claude.permesso.3", 200),
            ("sessione-b", "claude.permesso.12", 50),
        ]
        .into_iter()
        .map(|(sessione, voce, alle)| (sessione.to_owned(), (voce.to_owned(), 0, dati(alle))))
        .collect();
        let ordine = |m: &HashMap<String, (String, i32, DatiPermesso)>| -> Vec<String> {
            crate::ponte::Ponte::permessi_dal_piu_vecchio(m).iter().map(|(i, _)| (*i).to_owned()).collect()
        };
        // Per ora, poi per numero (9 prima di 10: numerico, non alfabetico).
        assert_eq!(ordine(&mappa), ["sessione-b", "sessione-a", "sessione-c", "sessione-d"]);
        for _ in 0..20 {
            let rifatta: HashMap<_, _> = mappa.clone().into_iter().collect();
            assert_eq!(ordine(&rifatta), ordine(&mappa), "stesso ordine a ogni giro");
        }
        assert!(ordine(&HashMap::new()).is_empty());
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

    /// Rimandato di F1 (Task 5): «Aggiungi» della carta dei Collegamenti (senza collegamenti) apre le
    /// Impostazioni, una volta sola. Lo stesso fanno «Modifica» e «Aggiungi un collegamento» della
    /// pagina (`azione-pagina`), e nient'altro (nessun lancio, nessuna chiusura del pannello).
    #[test]
    fn aggiungi_della_carta_apre_le_impostazioni() {
        use crate::moduli::{collegamenti, pagina_collegamenti};
        use std::cell::Cell;
        use std::rc::Rc;
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let aperte = Rc::new(Cell::new(0));
        let a = aperte.clone();
        ponte.su_apri_impostazioni(move || a.set(a.get() + 1));
        let chiusure = Rc::new(Cell::new(0));
        let c = chiusure.clone();
        ponte.su_chiudi_pannello(move || c.set(c.get() + 1));
        ponte.su_apri_url(|_| panic!("nessun collegamento da aprire"));
        ponte.azione_modulo("collegamenti", collegamenti::AGGIUNGI);
        assert_eq!(aperte.get(), 1, "le Impostazioni chieste una volta");
        // Le azioni della pagina Collegamenti portano alle stesse Impostazioni.
        ponte.azione_pagina("collegamenti", pagina_collegamenti::MODIFICA, "");
        ponte.azione_pagina("collegamenti", collegamenti::AGGIUNGI, "");
        assert_eq!(aperte.get(), 3);
        // Un'azione che non è sua non apre nulla.
        ponte.azione_pagina("collegamenti", "collegamenti.boh", "");
        assert_eq!((aperte.get(), chiusure.get()), (3, 0));
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

    /// Le azioni della pagina Agenti (ridisegno F2, Task 6): «Apri il terminale» della riga è la
    /// stessa funzione della carta, sulla sessione della riga. Dal Task 7 la riga col permesso non
    /// ha più un'azione: il suo dettaglio è il permesso stesso (proprio quello della sua sessione,
    /// anche con un'altra voce davanti nelle attese), col fuoco su «Nega» e «Consenti» riarmato.
    #[test]
    fn azioni_della_pagina_agenti() {
        use crate::moduli::agenti;
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        ponte.claude_evento(evento("a", TipoEvento::UserPromptSubmit), 1, Instant::now());
        ponte.claude_evento(evento("b", TipoEvento::UserPromptSubmit), 2, Instant::now());
        let catena = |n: u32| vec![(n, "claude.exe".to_owned()), (3, "WindowsTerminal.exe".to_owned())];
        ponte.claude_imposta_catena("a", catena(5));
        portati_davanti_prova();
        ponte.azione_pagina("agenti", agenti::TERMINALE, &crate::claude::id_sessione("a"));
        assert_eq!(portati_davanti_prova(), vec![catena(5)], "il terminale della riga");

        p.set_vista("agenti".into());
        // Un permesso di «b», dietro una voce bloccante della casa.
        ponte.metti_voce(
            crate::attenzione::Voce {
                id: "prova.casa".into(),
                livello: Livello::Bloccante,
                fonte: crate::attenzione::Fonte::Casa,
                titolo: "Prova".into(),
                dettaglio: String::new(),
                pulsanti: vec![],
                // Più recente del permesso: gli sta davanti nella Coda.
                creata: Instant::now() + std::time::Duration::from_secs(60),
                permesso: None,
            },
            false,
        );
        let e = EventoHook { strumento: Some("Bash".into()), riassunto: Some("npm test".into()), ..evento("b", TipoEvento::PermissionRequest) };
        ponte.claude_permesso(9, e, None, 3, Instant::now());
        let indice = crate::pannello::indice_voce(&p.get_attese(), &crate::claude::id_voce_permesso(9));
        assert!(indice > 0, "il permesso non è la prima voce: «0» mostrerebbe la voce della casa");
        // La strada vera, da tastiera: la riga di «b» scelta, → nel permesso del dettaglio. Il
        // pannello è mostrato (le parti dentro `if` esistono).
        // Prima che la pagina mostri le righe: la scelta può già stare sulla riga di «b».
        let armamenti = p.get_armamenti();
        ponte.casa().aggiorna_config(|c| c.claude.collegato = true);
        ponte.vista_claude(true);
        p.set_aperto(true);
        p.show().expect("pannello mostrato nel backend di prova");
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        let righe = p.get_elenco_righe();
        let riga_b = (0..righe.row_count()).position(|i| righe.row_data(i).is_some_and(|r| r.id == crate::claude::id_sessione("b"))).expect("la riga di «b»");
        let r = righe.row_data(riga_b).expect("la riga");
        assert_eq!((r.azione_id.as_str(), r.voce.to_string()), ("", crate::claude::id_voce_permesso(9)), "nessuna azione: il dettaglio è il permesso");
        p.set_elenco_fuoco(riga_b as i32);
        tasto(&p, slint::platform::Key::RightArrow);
        slint::platform::update_timers_and_animations();
        assert_eq!(p.get_vista().as_str(), "agenti", "si resta nella pagina");
        // È proprio il permesso di «b» (il suo comando), non la voce della casa davanti a lui, e il
        // fuoco è su «Nega». Il gruppo del permesso dice a Narrator il comando e la sessione.
        let permesso = ElementHandle::find_by_element_id(&p, "Permesso::riquadro").next().is_some();
        assert!(permesso, "il permesso è nel dettaglio");
        let gruppo = ElementHandle::find_by_element_type_name(&p, "Permesso").next().expect("il componente");
        let detto = gruppo.accessible_description().unwrap_or_default().to_string();
        assert!(detto.starts_with("npm test\n"), "il comando di «b», non la voce della casa: {detto:?}");
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        assert!(p.get_armamenti() > armamenti, "la scelta arrivata sul permesso riarma «Consenti»");
    }

    /// Review Focus 5 (ridisegno F3, Task 4a): una carta senza azione non si riscrive se nulla
    /// cambia. L'icona dell'azione di una carta senza azione era un'`Image` vuota, che in Slint non
    /// è mai uguale a se stessa: la riga sembrava sempre cambiata e `aggiorna_modello` la
    /// riscriveva a ogni `moduli_scrivi`. Vale con carte senza azione e con una carta con l'azione.
    #[test]
    fn carta_senza_azione_non_si_riscrive_se_nulla_cambia() {
        use crate::ponte::RIGHE_SCRITTE;
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        let senza_azione = || p.get_moduli().iter().filter(|m| !m.con_azione).count();
        assert!(senza_azione() >= 2, "alla partenza più carte senza azione");
        let righe = || RIGHE_SCRITTE.with(|c| c.get());
        // Nessuna carta ha un'immagine vuota in mano: quella dell'azione è «nessuna».
        let prima = righe();
        ponte.moduli_scrivi();
        ponte.moduli_scrivi();
        assert_eq!(righe(), prima, "carte senza azione: nessuna riga riscritta");
        // Con una carta che ha l'azione («Spegni tutte») e le altre senza, ancora nulla.
        ponte.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![vec![luce("light.a", true)]]))));
        assert!(p.get_moduli().iter().any(|m| m.con_azione), "una carta con l'azione");
        let prima = righe();
        ponte.moduli_scrivi();
        ponte.moduli_scrivi();
        assert_eq!(righe(), prima, "con un'azione e le altre senza: nessuna riga riscritta");
    }

    /// Giro di correzione 1 del Task 4b: a pannello aperto, passando da scollegata a in connessione
    /// (casa-core riprova a ogni tentativo) coi dati di prima, la carta di Casa tiene «Scollegata»
    /// e «Riprova»: il pulsante non compare e sparisce sotto il puntatore.
    #[test]
    fn la_carta_casa_tiene_riprova_durante_la_riconnessione() {
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        let carta = || {
            let c = p.get_moduli().iter().find(|m| m.id == "casa").expect("la carta Casa");
            (c.titolo.to_string(), c.con_azione, c.azione_id.to_string())
        };
        let attesa = ("Scollegata".to_owned(), true, casa::RIPROVA.to_owned());
        for stato in [
            StatoConnessione::Errore("timeout".into()),
            StatoConnessione::InConnessione,
            StatoConnessione::Errore("timeout".into()),
            StatoConnessione::InConnessione,
        ] {
            ponte.ricevi(CasaEvento::Stato(stato.clone()));
            assert_eq!(carta(), attesa, "{stato:?}");
        }
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
        let scritte = crate::ponte::RIGHE_SCRITTE.with(|c| c.get());
        for accese in [1, 3] {
            let luci = (0..accese).map(|i| luce(&format!("light.l{i}"), true)).collect();
            ponte.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![luci]))));
        }
        assert!(ponte.moduli_vecchi.get(), "da rifare all'apertura");
        assert_eq!(ponte.modello_moduli.row_data(1).map(|m| m.titolo.to_string()).as_deref(), Some("Luci spente"), "modello fermo");
        // Il titolo uguale non basta (un modello riscritto con gli stessi valori passerebbe): le
        // scritture vere di `aggiorna_modello` in questo thread sono ferme.
        assert_eq!(crate::ponte::RIGHE_SCRITTE.with(|c| c.get()), scritte, "nessuna riga scritta a pannello chiuso");
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
        let p = Pannello::new().expect("pannello");
        ponte.moduli_nel_pannello(&p);
        let schede: Vec<String> = p.get_schede().iter().map(|v| v.id.to_string()).collect();
        assert_eq!(schede, ["panoramica", "collegamenti", "agenti", "casa", "pc"]);
        let carte: Vec<String> = p.get_moduli().iter().map(|m| m.id.to_string()).collect();
        assert_eq!(carte, ["collegamenti", "agenti", "casa", "pc"]);
    }

    /// Le schede si riscrivono solo se cambiano: due chiamate di fila senza cambi non toccano
    /// nulla; un cambio di vista tocca solo le due schede che cambiano larghezza e quelle che si
    /// spostano; un tono alzato da una voce cambia segno e nome della sua scheda.
    #[test]
    fn schede_riscritte_solo_se_cambiano() {
        let (_spegni, ponte, _pillola, _cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.moduli_nel_pannello(&p);
        assert_eq!(p.get_schede().row_count(), 5);
        assert_eq!(ponte.scrivi_schede(&p), 0, "già scritte da `moduli_nel_pannello`");
        assert_eq!(ponte.scrivi_schede(&p), 0, "di nuovo: nulla");

        p.set_vista("casa".into());
        assert!(ponte.scrivi_schede(&p) > 0, "la vista cambia la geometria");
        assert_eq!(ponte.scrivi_schede(&p), 0);
        let larghezze: Vec<f32> = p.get_schede().iter().map(|s| s.larghezza).collect();
        assert!(larghezze[2] > 36.0 && larghezze[0] == 36.0, "Casa larga, Panoramica 36: {larghezze:?}");

        *ponte.pannello.borrow_mut() = p.as_weak();
        assert!(ponte.registro.borrow_mut().livelli_voci(&[("agenti", Livello::Bloccante)]));
        ponte.moduli_scrivi();
        let agenti = p.get_schede().row_data(1).expect("scheda Agenti");
        assert_eq!((agenti.segno, agenti.narratore.as_str()), (1, "Agenti, ti aspetta, scheda 2 di 5"));
        assert_eq!(ponte.scrivi_schede(&p), 0, "scritte una volta sola");
    }
}
