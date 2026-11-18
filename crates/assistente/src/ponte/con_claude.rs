//! Claude Code nel ponte (M3, Task 4): l'ascoltatore degli hook, le voci delle sessioni, il
//! colore degli occhi e la pillola media «attività dal vivo».
//!
//! Nessun timer a riposo: una sveglia singola alla prossima scadenza di una sessione, e il timer
//! da un secondo del testo di attività solo mentre serve (vedi `attivita_serve`).

use super::{ponte, Ponte, MARGINE_SVEGLIA};
use crate::attenzione::{Fonte, Livello, Pulsante, Voce, PULSANTE_IMPOSTAZIONI};
use crate::claude::{self, ID_COLLEGAMENTO};
use crate::pillola;
use crate::umore::Umore;
use crate::LivelloUi;
use casa_core::diagnosi::{Contatore, Diagnosi};
use casa_core::segreti::Segreti;
use claude_hook::evento::{evento_da_json, EventoHook};
use claude_hook::sessioni::Cambio;
use claude_hook::{avvia, Arrivo, AttesaPermesso, CodiceHook, ConfigAscolto, ErroreAvvio, Gestore, ATTESA_MASSIMA};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Dove sta il codice segreto nel portachiavi.
pub const SEGRETO_HOOK: &str = "isola:claude-hook";
/// In prova (`ITSYHOME_ASSISTENTE_DIR`) il codice viene da questa variabile del processo: mai il
/// portachiavi vero.
pub const VARIABILE_PROVA: &str = "ISOLA_HOOK_TOKEN";

/// Il codice dell'ascoltatore: in prova il valore di `ISOLA_HOOK_TOKEN` (passato, i test non leggono
/// l'ambiente), altrimenti il portachiavi. `None` se manca o non è un codice valido. Non lo scrive mai.
fn codice_da(segreti: &dyn Segreti, in_prova: bool, variabile: Option<String>) -> Option<CodiceHook> {
    let testo = if in_prova { variabile } else { segreti.leggi(SEGRETO_HOOK).ok().flatten() };
    testo.as_deref().and_then(CodiceHook::da_testo)
}

/// Chiamata da `main` dopo `ponte::collega`: se Claude Code è collegato, c'è una porta e c'è il
/// codice, si mette in ascolto.
pub fn avvia_se_collegato(p: &Ponte) {
    let in_prova = std::env::var_os(crate::avvio::VAR_CARTELLA).is_some_and(|v| !v.is_empty());
    avvia_se_collegato_con(p, in_prova, std::env::var(VARIABILE_PROVA).ok());
}

/// `avvia_se_collegato` con «in prova» e il valore di `ISOLA_HOOK_TOKEN` passati.
fn avvia_se_collegato_con(p: &Ponte, in_prova: bool, variabile: Option<String>) {
    let (collegato, porta) = p.casa.con_config(|c| (c.claude.collegato, c.claude.porta));
    if !collegato || porta.is_none() {
        return;
    }
    match codice_da(p.segreti.as_ref(), in_prova, variabile) {
        Some(codice) => avvia_con(p, codice),
        None => eprintln!("Claude Code: codice di collegamento non trovato, ascolto non avviato"),
    }
}

/// Mette in ascolto sulla porta della configurazione con questo codice (le Impostazioni, al
/// collegamento, lo chiamano col codice appena creato). Un ascolto già attivo si ferma prima.
pub fn avvia_con(p: &Ponte, codice: CodiceHook) {
    let Some(porta) = p.casa.con_config(|c| c.claude.porta) else { return };
    let vecchio = p.claude_ascolto.borrow_mut().take();
    drop(vecchio); // libera la porta prima di riprenderla
    let config = ConfigAscolto { porta, codice, attesa_massima: ATTESA_MASSIMA };
    match avvia(config, Arc::new(GestoreIsola { diagnosi: p.casa.diagnosi_condivisa() })) {
        Ok(ascolto) => {
            *p.claude_ascolto.borrow_mut() = Some(ascolto);
            p.togli_voce(ID_COLLEGAMENTO);
        }
        Err(e) => {
            let dettaglio = match e {
                ErroreAvvio::PortaOccupata => format!("La porta {porta} è usata da un altro programma."),
                ErroreAvvio::Altro(_) => "Non è stato possibile mettersi in ascolto.".to_owned(),
            };
            p.metti_voce(
                Voce {
                    id: ID_COLLEGAMENTO.into(),
                    livello: Livello::Attenzione,
                    fonte: Fonte::Claude,
                    titolo: "Collegamento con Claude Code non disponibile".into(),
                    dettaglio,
                    pulsanti: vec![Pulsante::new(PULSANTE_IMPOSTAZIONI, "Impostazioni")],
                    creata: Instant::now(),
                },
                true,
            );
        }
    }
}

/// A fine programma, prima di `ponte::spegni`: l'ascolto si ferma (il thread si raccoglie) e le
/// sveglie si fermano.
pub fn spegni() {
    if let Some(p) = ponte() {
        let ascolto = p.claude_ascolto.borrow_mut().take();
        drop(ascolto);
        p.sveglia_claude.stop();
        p.timer_attivita.stop();
    }
}

/// Un pulsante di una voce di Claude Code (i task 5, 10 e 12 ci aggiungono i loro).
pub fn azione(p: &Ponte, voce: &str, pulsante: &str) {
    if voce == ID_COLLEGAMENTO && pulsante == PULSANTE_IMPOSTAZIONI {
        p.chiedi_impostazioni();
    }
}

/// Chi riceve ciò che l'ascoltatore ha accettato. Gira sui thread delle connessioni: legge il
/// corpo e passa al thread dell'interfaccia solo l'evento ridotto (mai il corpo).
struct GestoreIsola {
    /// Il registro condiviso (contatori atomici): i rifiuti si contano sul thread della connessione.
    diagnosi: Arc<Diagnosi>,
}

/// Porta un evento al thread dell'interfaccia. Se il ciclo eventi è finito non c'è più nessuno:
/// si ignora.
fn consegna(e: EventoHook, pid: Option<u32>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.claude_evento(e, pid, Instant::now());
        }
    });
}

impl Gestore for GestoreIsola {
    fn evento(&self, a: Arrivo) {
        if let Ok(e) = evento_da_json(&a.corpo) {
            consegna(e, a.pid_mittente);
        }
    }

    /// Fino al Task 5 un permesso è un evento come gli altri (nessuna risposta: Claude Code
    /// prosegue col suo flusso).
    fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso> {
        self.evento(a);
        None
    }

    fn abbandonato(&self, _id: u64) {}

    /// Anche con nessun evento valido (un codice sbagliato dà solo 401) i rifiuti arrivano al
    /// registro diagnostico, senza dati della richiesta.
    fn rifiutata(&self) {
        self.diagnosi.conta(Contatore::claude_rifiutate);
    }

    /// I dati della barra di stato arrivano col Task 6.
    fn barra(&self, _a: Arrivo) {}
}

impl Ponte {
    /// Un evento di Claude Code, sul thread dell'interfaccia. `pid`: il processo del terminale,
    /// trovato alla prima richiesta di una sessione.
    pub(super) fn claude_evento(&self, e: EventoHook, pid: Option<u32>, ora: Instant) {
        self.casa.diagnosi().conta(Contatore::claude_eventi);
        let cambi = {
            let mut s = self.claude_sessioni.borrow_mut();
            let cambi = s.ricevi(&e, ora);
            if let Some(pid) = pid {
                s.imposta_pid(&e.sessione, pid);
            }
            cambi
        };
        self.claude_cambiate(&cambi, ora);
    }

    /// Le voci, la sveglia e la pillola dopo un cambio delle sessioni.
    fn claude_cambiate(&self, cambi: &[Cambio], ora: Instant) {
        for c in cambi {
            match c {
                Cambio::Tolta(id) => self.togli_voci_sessione(id),
                Cambio::Cambiata(id) => self.voci_sessione(id, ora),
            }
        }
        self.arma_sveglia_claude(ora);
        self.aggiorna_claude_pillola();
    }

    fn togli_voci_sessione(&self, id: &str) {
        for voce in claude::id_voci(id) {
            self.togli_voce(&voce);
        }
    }

    /// La voce di una sessione secondo il suo stato di adesso: quella giusta c'è (nuova o
    /// cambiata), le altre due no. Una voce uguale a quella che c'è già non si rimette.
    fn voci_sessione(&self, id: &str, ora: Instant) {
        let voce = {
            let s = self.claude_sessioni.borrow();
            s.elenco().into_iter().find(|x| x.id == id).and_then(|x| claude::voce_sessione(x, ora))
        };
        let tenuta = voce.as_ref().map(|v| v.id.clone());
        for altra in claude::id_voci(id) {
            if tenuta.as_deref() != Some(altra.as_str()) {
                self.togli_voce(&altra);
            }
        }
        let Some(v) = voce else { return };
        let uguale = self.attese.borrow().voce(&v.id).is_some_and(|x| {
            x.livello == v.livello && x.titolo == v.titolo && x.dettaglio == v.dettaglio && x.pulsanti == v.pulsanti
        });
        if !uguale {
            self.metti_voce(v, true);
        }
    }

    /// Una sveglia singola alla prossima scadenza di una sessione (nessuna se non ce ne sono).
    fn arma_sveglia_claude(&self, ora: Instant) {
        match self.claude_sessioni.borrow().prossima_scadenza(ora) {
            Some(tra) => self.sveglia_claude.start(slint::TimerMode::SingleShot, tra + MARGINE_SVEGLIA, || {
                if let Some(p) = ponte() {
                    p.claude_scadute(Instant::now());
                }
            }),
            None => self.sveglia_claude.stop(),
        }
    }

    /// Sessioni silenziose da troppo: spariscono, con le loro voci.
    fn claude_scadute(&self, ora: Instant) {
        let tolte = self.claude_sessioni.borrow_mut().scadute(ora);
        for id in &tolte {
            self.togli_voci_sessione(id);
        }
        self.arma_sveglia_claude(ora);
        self.aggiorna_claude_pillola();
    }

    /// Il pannello si è chiuso: l'elenco è stato guardato, i «fatto» sono visti e le loro voci
    /// spariscono (sfumano). Aperto, il pannello li ha lasciati dov'erano.
    pub(super) fn claude_pannello_chiuso(&self) {
        let ora = Instant::now();
        let ids: Vec<String> = {
            let mut s = self.claude_sessioni.borrow_mut();
            s.segna_viste();
            s.elenco().into_iter().map(|x| x.id.clone()).collect()
        };
        for id in &ids {
            self.voci_sessione(id, ora);
        }
        self.aggiorna_claude_pillola();
    }

    /// Il timer da un secondo del testo di attività serve solo se almeno una sessione lavora, la
    /// pillola si vede e non dorme. Una condizione sola, per estenderla (Task 9: «o la vista
    /// Claude è aperta»).
    pub(super) fn attivita_serve(&self) -> bool {
        self.claude_sessioni.borrow().lavorano() > 0 && self.pillola_mostrata.get() && !self.pillola_dorme()
    }

    fn pillola_dorme(&self) -> bool {
        self.mostrato.borrow().as_ref().is_some_and(|v| v.umore == Umore::Dorme)
    }

    /// Occhi e testo di attività della pillola, e il timer che lo tiene al secondo: ogni proprietà
    /// si riscrive solo se cambia. La pillola nascosta non si tocca (riprende da sé quando
    /// ricompare: `ridisegna_pillola` passa di qui).
    pub(super) fn aggiorna_claude_pillola(&self) {
        let ora = Instant::now();
        let serve = self.attivita_serve();
        if serve {
            if !self.timer_attivita.running() {
                self.timer_attivita.start(slint::TimerMode::Repeated, Duration::from_secs(1), || {
                    if let Some(p) = ponte() {
                        p.aggiorna_claude_pillola();
                    }
                });
            }
        } else if self.timer_attivita.running() {
            self.timer_attivita.stop();
        }
        let Some(ui) = self.pillola.upgrade().filter(|p| p.get_attiva()) else { return };
        let (tinta, parti, nome) = {
            let s = self.claude_sessioni.borrow();
            (claude::tinta_occhi(&s), claude::parti_attivita(&s, ora), claude::nome_attivita(&s, ora))
        };
        let tinta = tinta.map_or(LivelloUi::Nessuno, pillola::livello_ui);
        if ui.get_tinta_occhi() != tinta {
            ui.set_tinta_occhi(tinta);
        }
        // Dalla ritirata e da nascosta si riscrive quando la pillola torna; da addormentata il
        // testo non c'è.
        if !self.pillola_mostrata.get() {
            return;
        }
        let (sinistra, destra, nome) = match parti {
            Some((a, d)) if !self.pillola_dorme() => (a, d, nome.unwrap_or_default()),
            _ => (String::new(), String::new(), String::new()),
        };
        let testo = if sinistra.is_empty() { String::new() } else { format!("{sinistra} · {destra}") };
        let mut cambiato = false;
        if ui.get_attivita() != testo.as_str() {
            let destra = if destra.is_empty() { String::new() } else { format!(" · {destra}") };
            ui.set_attivita(testo.as_str().into());
            ui.set_attivita_sinistra(sinistra.as_str().into());
            ui.set_attivita_destra(destra.as_str().into());
            cambiato = true;
        }
        if ui.get_nome_attivita() != nome.as_str() {
            ui.set_nome_attivita(nome.as_str().into());
        }
        // La pillola si allarga o si stringe: il ritaglio della finestra la segue.
        if cambiato {
            pillola::lati_cambiati(&ui);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::attenzione::Livello;
    use crate::ponte::test::ponte_di_prova;
    use claude_hook::evento::TipoEvento;
    use std::cell::Cell;
    use std::rc::Rc;

    fn ev(id: &str, tipo: TipoEvento) -> EventoHook {
        EventoHook {
            sessione: id.into(),
            tipo,
            cartella: "progetto-a".into(),
            strumento: None,
            notifica: None,
            errore: None,
            riassunto: None,
        }
    }

    fn manda(p: &Ponte, e: EventoHook) {
        p.claude_evento(e, None, Instant::now());
    }

    fn voci_claude(p: &Ponte) -> Vec<(String, Livello, String)> {
        p.attese
            .borrow()
            .ordinate()
            .into_iter()
            .filter(|v| v.id.starts_with(claude::PREFISSO_CLAUDE))
            .map(|v| (v.id.clone(), v.livello, v.titolo.clone()))
            .collect()
    }

    #[test]
    fn eventi_diventano_voci() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert!(voci_claude(&p).is_empty(), "chi lavora non ha voce");
        manda(&p, ev("a", TipoEvento::Stop));
        let voci = voci_claude(&p);
        assert_eq!(voci.len(), 1);
        assert_eq!(voci[0].1, Livello::Informazione);
        assert_eq!(voci[0].2, "Claude ha finito · progetto-a");
        assert!(voci[0].0.starts_with("claude.") && voci[0].0.ends_with(".fatto"), "{}", voci[0].0);
        manda(&p, ev("a", TipoEvento::SessionEnd));
        assert!(voci_claude(&p).is_empty(), "la sessione chiusa toglie la voce");
    }

    #[test]
    fn una_sola_voce_per_sessione_che_cambia_stato() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::PermissionRequest));
        assert_eq!(voci_claude(&p).iter().map(|v| (v.1, v.2.as_str())).collect::<Vec<_>>(), [(Livello::Attenzione, "Claude ti aspetta · progetto-a")]);
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert!(voci_claude(&p).is_empty(), "riprende a lavorare: via la voce");
        let errore = EventoHook { errore: Some("rate_limit".into()), ..ev("a", TipoEvento::StopFailure) };
        manda(&p, errore);
        let voci = voci_claude(&p);
        assert_eq!(voci.len(), 1);
        assert_eq!(voci[0].2, "Claude si è fermato · progetto-a");
        assert_eq!(p.attese.borrow().voce(&voci[0].0).unwrap().dettaglio, "Limite raggiunto");
    }

    #[test]
    fn il_pid_del_terminale_si_tiene() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.claude_evento(ev("a", TipoEvento::UserPromptSubmit), Some(4321), Instant::now());
        assert_eq!(p.claude_sessioni.borrow().elenco()[0].pid, Some(4321));
    }

    #[test]
    fn azione_sulle_voci_claude() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let chiamate = Rc::new(Cell::new(0));
        let c = chiamate.clone();
        p.su_apri_impostazioni(move || c.set(c.get() + 1));
        p.azione(ID_COLLEGAMENTO, PULSANTE_IMPOSTAZIONI);
        assert_eq!(chiamate.get(), 1, "il pulsante apre le Impostazioni");
        p.azione(ID_COLLEGAMENTO, "altro");
        p.azione("claude.0123.fatto", PULSANTE_IMPOSTAZIONI);
        assert_eq!(chiamate.get(), 1, "altri pulsanti e altre voci: niente");
    }

    #[test]
    fn fatto_sparisce_alla_chiusura_del_pannello() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::Stop));
        assert_eq!(voci_claude(&p).len(), 1);
        let pannello = crate::Pannello::new().unwrap();
        p.pannello_aperto(&pannello);
        assert_eq!(voci_claude(&p).len(), 1, "col pannello aperto la voce resta: la si sta guardando");
        assert!(!p.claude_sessioni.borrow().elenco()[0].vista);
        p.pannello_chiuso();
        assert!(voci_claude(&p).is_empty(), "chiuso il pannello il «fatto» è visto e sparisce");
        assert!(p.claude_sessioni.borrow().elenco()[0].vista);
    }

    #[test]
    fn scollegato_niente_ascoltatore() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        assert!(!p.casa.con_config(|c| c.claude.collegato));
        avvia_se_collegato_con(&p, false, None);
        assert!(p.claude_ascolto.borrow().is_none());
        // Collegato ma senza codice nel portachiavi: nessun ascolto (e nessuna voce).
        p.casa.aggiorna_config(|c| {
            c.claude.collegato = true;
            c.claude.porta = Some(1);
        });
        avvia_se_collegato_con(&p, false, None);
        assert!(p.claude_ascolto.borrow().is_none());
        // In prova, senza la variabile (o con una che non è un codice): nessun ascolto.
        avvia_se_collegato_con(&p, true, None);
        avvia_se_collegato_con(&p, true, Some("non un codice".into()));
        assert!(p.claude_ascolto.borrow().is_none());
        assert!(voci_claude(&p).is_empty());
        // In prova con un codice valido: ascolta (la porta è libera).
        let porta = porta_libera();
        p.casa.aggiorna_config(|c| c.claude.porta = Some(porta));
        avvia_se_collegato_con(&p, true, Some(CodiceHook::nuovo().unwrap().come_testo()));
        assert_eq!(p.claude_ascolto.borrow().as_ref().map(|a| a.porta()), Some(porta));
    }

    fn porta_libera() -> u16 {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.local_addr().unwrap().port()
    }

    #[test]
    fn avvia_con_il_codice_e_si_ferma() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let porta = porta_libera();
        p.casa.aggiorna_config(|c| {
            c.claude.collegato = true;
            c.claude.porta = Some(porta);
        });
        avvia_con(&p, CodiceHook::nuovo().unwrap());
        assert_eq!(p.claude_ascolto.borrow().as_ref().map(|a| a.porta()), Some(porta));
        assert!(!p.attese.borrow().contiene(ID_COLLEGAMENTO));
        // Di nuovo, con un altro codice: il vecchio ascolto lascia la porta e riparte.
        avvia_con(&p, CodiceHook::nuovo().unwrap());
        assert_eq!(p.claude_ascolto.borrow().as_ref().map(|a| a.porta()), Some(porta));
        spegni();
        assert!(p.claude_ascolto.borrow().is_none());
    }

    #[test]
    fn porta_occupata_da_avviso() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let occupante = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let porta = occupante.local_addr().unwrap().port();
        p.casa.aggiorna_config(|c| {
            c.claude.collegato = true;
            c.claude.porta = Some(porta);
        });
        avvia_con(&p, CodiceHook::nuovo().unwrap());
        assert!(p.claude_ascolto.borrow().is_none());
        let a = p.attese.borrow();
        let v = a.voce(ID_COLLEGAMENTO).expect("voce del collegamento");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.titolo, "Collegamento con Claude Code non disponibile");
        assert_eq!(v.dettaglio, format!("La porta {porta} è usata da un altro programma."));
        assert_eq!(v.pulsanti, vec![Pulsante::new(PULSANTE_IMPOSTAZIONI, "Impostazioni")]);
        drop(occupante);
    }

    #[test]
    fn il_codice_dal_portachiavi() {
        use casa_core::segreti::MemoriaSegreti;
        let m = MemoriaSegreti::new();
        assert!(codice_da(&m, false, None).is_none());
        let codice = CodiceHook::nuovo().unwrap();
        m.salva(SEGRETO_HOOK, &codice.come_testo()).unwrap();
        let letto = codice_da(&m, false, None).expect("codice nel portachiavi");
        assert_eq!(letto.come_testo(), codice.come_testo());
        // Un valore che non è un codice non vale.
        m.salva(SEGRETO_HOOK, "troppo corto").unwrap();
        assert!(codice_da(&m, false, None).is_none());
    }

    #[test]
    fn i_rifiuti_dell_ascoltatore_arrivano_alla_diagnosi() {
        let diagnosi = Arc::new(Diagnosi::new(true));
        let g = GestoreIsola { diagnosi: diagnosi.clone() };
        g.rifiutata();
        g.rifiutata();
        assert_eq!(diagnosi.valore(Contatore::claude_rifiutate), 2);
        assert_eq!(diagnosi.valore(Contatore::claude_eventi), 0, "un rifiuto non è un evento");
    }

    #[test]
    fn senza_eventi_attivita_destra_e_vuota() {
        let (_s, p, pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert_eq!(pillola.get_attivita_destra(), " · 0:00");
        manda(&p, ev("a", TipoEvento::Stop));
        assert_eq!(pillola.get_attivita(), "");
        assert_eq!(pillola.get_attivita_destra(), "");
        assert_eq!(pillola.get_attivita_sinistra(), "");
    }

    #[test]
    fn sveglia_e_timer_solo_quando_servono() {
        let (_s, p, pillola, _d) = ponte_di_prova();
        assert!(!p.sveglia_claude.running() && !p.timer_attivita.running(), "a riposo nessun timer");
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert!(p.sveglia_claude.running(), "una sessione scade fra 30 minuti");
        assert!(p.timer_attivita.running(), "lavora, la pillola si vede");
        assert_eq!(pillola.get_attivita(), "Claude · Pensa · 0:00");
        assert_eq!(pillola.get_nome_attivita(), "Claude, Pensa, meno di un minuto");
        // Pillola ritirata o nascosta: il timer si ferma.
        p.pillola_visibile(false);
        assert!(!p.timer_attivita.running());
        p.pillola_visibile(true);
        assert!(p.timer_attivita.running());
        // Smette di lavorare: il timer si ferma, il testo sparisce.
        manda(&p, ev("a", TipoEvento::Stop));
        assert!(!p.timer_attivita.running());
        assert_eq!(pillola.get_attivita(), "");
        manda(&p, ev("a", TipoEvento::SessionEnd));
        assert!(!p.sveglia_claude.running(), "nessuna sessione: nessuna sveglia");
    }

    #[test]
    fn umore_dorme_ferma_il_timer_da_un_secondo() {
        let (_s, p, pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert!(p.timer_attivita.running());
        assert_eq!(pillola.get_attivita(), "Claude · Pensa · 0:00");
        // L'utente è inattivo da dieci minuti: la pillola dorme, niente timer né testo.
        p.inattivo(crate::umore::DORME_DOPO);
        assert!(p.pillola_dorme(), "umore dorme");
        assert!(!p.timer_attivita.running(), "dorme: il timer si ferma");
        assert_eq!(pillola.get_attivita(), "");
        // Si risveglia: il timer riparte e il testo torna.
        p.inattivo(Duration::ZERO);
        assert!(!p.pillola_dorme());
        assert!(p.timer_attivita.running());
        assert_eq!(pillola.get_attivita(), "Claude · Pensa · 0:00");
    }

    #[test]
    fn occhi_colorati_dallo_stato_piu_urgente() {
        let (_s, p, pillola, _d) = ponte_di_prova();
        assert_eq!(pillola.get_tinta_occhi(), LivelloUi::Nessuno);
        manda(&p, ev("a", TipoEvento::Stop));
        assert_eq!(pillola.get_tinta_occhi(), LivelloUi::Informazione);
        manda(&p, ev("b", TipoEvento::PermissionRequest));
        assert_eq!(pillola.get_tinta_occhi(), LivelloUi::Attenzione);
        manda(&p, ev("b", TipoEvento::SessionEnd));
        assert_eq!(pillola.get_tinta_occhi(), LivelloUi::Informazione);
        p.pannello_chiuso();
        assert_eq!(pillola.get_tinta_occhi(), LivelloUi::Nessuno, "il fatto visto non colora più");
    }

    #[test]
    fn le_scadenze_tolgono_sessioni_e_voci() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::Stop));
        assert_eq!(voci_claude(&p).len(), 1);
        p.claude_scadute(Instant::now() + Duration::from_secs(31 * 60));
        assert!(voci_claude(&p).is_empty());
        assert!(p.claude_sessioni.borrow().elenco().is_empty());
    }
}
