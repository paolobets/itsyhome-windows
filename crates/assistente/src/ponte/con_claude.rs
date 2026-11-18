//! Claude Code nel ponte (M3, Task 4 e 7A): il tubo protetto con l'aiutante `isola-hook`, le voci
//! delle sessioni, il colore degli occhi e la pillola media «attività dal vivo».
//!
//! Nessun timer a riposo: una sveglia singola alla prossima scadenza di una sessione, e il timer
//! da un secondo del testo di attività solo mentre serve (vedi `attivita_serve`).

mod api;
mod collegamento;
mod dettagli;

pub use collegamento::Contesto;
use super::{ponte, Ponte, MARGINE_SVEGLIA};
use crate::attenzione::{Fonte, Livello, Pulsante, Voce, PULSANTE_IMPOSTAZIONI};
use crate::claude::{
    self, ID_AGGIORNA, ID_COLLEGAMENTO, ID_TERMINALE_MANCANTE, PERMESSI_RISOLVIBILI, PULSANTE_CONSENTI, PULSANTE_NEGA, PULSANTE_TERMINALE,
};
use crate::pillola;
use crate::umore::Umore;
use crate::{LimiteClaudeUi, LivelloUi, RigaSessioneUi};
use casa_core::diagnosi::{Contatore, Diagnosi};
use claude_hook::evento::{EventoHook, TipoEvento};
use claude_hook::sessioni::Cambio;
use claude_hook::tubo::{NOME_AIUTANTE, SUFFISSO_PROVA};
use claude_hook::{DatiBarra, Limite};
use claude_hook::{
    avvia, nome_tubo, Arrivo, AttesaPermesso, ConfigTubo, Contenuto, Decisione, ErroreAvvio, Gestore, ATTESA_MASSIMA,
};
use std::cell::Cell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// In prova (e solo con `ITSYHOME_ASSISTENTE_DIR`) il cliente ammesso dal tubo è questo percorso
/// invece di `isola-hook.exe` accanto all'assistente: serve alle prove con `sessioni_finte`.
pub const VARIABILE_AIUTANTE: &str = "ITSYHOME_AIUTANTE_ATTESO";

/// Quanti permessi possono aspettare insieme (quante connessioni accetta l'ascoltatore). Uno in
/// più non ha voce né canale: Claude Code ripiega sul terminale.
const MASSIMO_PERMESSI: usize = 16;
/// Dopo che la voce mostrata cambia, «Consenti» non risponde per questo tempo: chi stava per
/// cliccare (o ha cliccato due volte) su una richiesta non deve consentirne un'altra mai letta.
pub(super) const ARMAMENTO_CONSENTI: Duration = Duration::from_millis(600);

/// Il numero del prossimo permesso: unico e crescente per tutto il processo, anche se l'ascolto
/// riparte (un numero vecchio non può mai indicare un permesso nuovo).
static PROSSIMO_PERMESSO: AtomicU64 = AtomicU64::new(1);

/// Che cosa vedeva l'utente quando la voce mostrata è cambiata l'ultima volta: la voce della
/// pillola (il bloccante più vecchio) e la prima del pannello. Con un cambio, «Consenti» si
/// arma solo dopo `ARMAMENTO_CONSENTI` (confronto di istanti, nessun timer).
pub(super) struct Armamento {
    pillola: Option<String>,
    pannello: Option<String>,
    dal: Cell<Instant>,
}

impl Armamento {
    pub(super) fn nuovo() -> Armamento {
        Armamento { pillola: None, pannello: None, dal: Cell::new(Instant::now()) }
    }
}

/// Un permesso che aspetta: la sessione che l'ha chiesto e dove mandare la decisione (nessun canale
/// nel ramo B, dove la decisione si dà solo nel terminale). Solo chi ha chiesto riceve la risposta.
pub(super) struct Pendente {
    sessione: String,
    tx: Option<Sender<Decisione>>,
    /// Il numero d'ordine di arrivo della richiesta (`Arrivo::sequenza`): un evento di chiusura
    /// la chiude solo se è arrivato dopo.
    sequenza: u64,
}

/// L'unico cliente che il tubo accetta: `isola-hook.exe` nella cartella di `exe`; in prova, se
/// data, la variabile `ITSYHOME_AIUTANTE_ATTESO` (passata: i test non leggono l'ambiente).
fn cliente_atteso(exe: &Path, in_prova: bool, variabile: Option<OsString>) -> PathBuf {
    match variabile.filter(|v| in_prova && !v.is_empty()) {
        Some(v) => PathBuf::from(v),
        None => exe.with_file_name(NOME_AIUTANTE),
    }
}

/// La configurazione vera del tubo: il nome dell'utente (in prova con `-prova`) e l'aiutante
/// accanto a questo programma. `None` se il nome non si può calcolare (SID dell'utente illeggibile).
pub fn config_tubo() -> Option<ConfigTubo> {
    let in_prova = std::env::var_os(crate::avvio::VAR_CARTELLA).is_some_and(|v| !v.is_empty());
    let exe = std::env::current_exe().ok()?;
    Some(config_tubo_da(&nome_tubo().ok()?, &exe, in_prova, std::env::var_os(VARIABILE_AIUTANTE)))
}

/// `config_tubo` con nome, eseguibile, «in prova» e variabile passati.
fn config_tubo_da(nome: &str, exe: &Path, in_prova: bool, variabile: Option<OsString>) -> ConfigTubo {
    ConfigTubo {
        nome: if in_prova { format!("{nome}{SUFFISSO_PROVA}") } else { nome.to_owned() },
        cliente_atteso: cliente_atteso(exe, in_prova, variabile),
        attesa_massima: ATTESA_MASSIMA,
    }
}

/// Chiamata da `main` dopo `ponte::collega`: se Claude Code è collegato, si apre il tubo.
pub fn avvia_se_collegato(p: &Ponte) {
    avvia_se_collegato_con(p, config_tubo());
}

/// `avvia_se_collegato` con la configurazione passata (i test usano un nome di tubo unico).
fn avvia_se_collegato_con(p: &Ponte, config: Option<ConfigTubo>) {
    if !p.casa.con_config(|c| c.claude.collegato) {
        return;
    }
    match config {
        Some(c) => avvia_con(p, c),
        None if cfg!(windows) => voce_non_disponibile(p, ErroreAvvio::Altro("nome del tubo".into())),
        None => {}
    }
}

/// Apre il tubo con questa configurazione (le Impostazioni, al collegamento, lo chiamano). Un tubo
/// già aperto si chiude prima. Fuori da Windows non c'è tubo: non fa nulla.
pub fn avvia_con(p: &Ponte, config: ConfigTubo) {
    if !cfg!(windows) {
        return;
    }
    // Chiude il vecchio (tutte le istanze, prima di riprendere il nome) e i suoi permessi.
    let generazione = p.claude_ferma_tubo();
    match avvia(config, Arc::new(GestoreIsola::new(p.casa.diagnosi_condivisa(), generazione))) {
        Ok(servitore) => {
            *p.claude_ascolto.borrow_mut() = Some(servitore);
            p.togli_voce(ID_COLLEGAMENTO);
        }
        Err(e) => voce_non_disponibile(p, e),
    }
}

/// La voce *attenzione* «Collegamento con Claude Code non disponibile».
fn voce_non_disponibile(p: &Ponte, e: ErroreAvvio) {
    let dettaglio = match e {
        ErroreAvvio::CanaleOccupato => "Il canale è occupato da un altro programma.",
        ErroreAvvio::Altro(_) => "Non è stato possibile aprire il canale.",
    };
    p.metti_voce(
        Voce {
            id: ID_COLLEGAMENTO.into(),
            livello: Livello::Attenzione,
            fonte: Fonte::Claude,
            titolo: "Collegamento con Claude Code non disponibile".into(),
            dettaglio: dettaglio.into(),
            pulsanti: vec![Pulsante::new(PULSANTE_IMPOSTAZIONI, "Impostazioni")],
            creata: Instant::now(),
        },
        true,
    );
}

/// A fine programma, prima di `ponte::spegni`: il tubo si chiude (i thread si raccolgono) e le
/// sveglie si fermano.
pub fn spegni() {
    if let Some(p) = ponte() {
        p.claude_ferma_tubo();
        p.sveglia_claude.stop();
        p.timer_attivita.stop();
        // La finestra delle schede (se c'è) si chiude e il suo processo si raccoglie.
        p.dettagli_spegni();
    }
}

/// Un pulsante di una voce di Claude Code (il Task 12 ci aggiunge i suoi).
pub fn azione(p: &Ponte, voce: &str, pulsante: &str) {
    if pulsante == PULSANTE_TERMINALE {
        // «Apri il terminale» sulle voci «ti aspetta» e «ha finito»: la stessa strada del clic su
        // una riga della vista Claude (`apri-sessione`).
        if let Some(impronta) = claude::impronta_da_voce(voce) {
            p.claude_apri_terminale(impronta);
        }
    } else if voce == ID_TERMINALE_MANCANTE {
        // «Ho capito» (e ogni altro pulsante): la voce ha detto quel che doveva.
        p.sveglia_terminale.stop();
        p.togli_voce(voce);
    } else if voce == ID_COLLEGAMENTO && pulsante == PULSANTE_IMPOSTAZIONI {
        p.chiedi_impostazioni();
    } else if voce == ID_AGGIORNA && pulsante == PULSANTE_IMPOSTAZIONI {
        // Si apre direttamente la sezione Claude, dove il collegamento si aggiorna (con anteprima e conferma).
        crate::finestra_impostazioni::apri_alla_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE, || {
            p.chiedi_impostazioni()
        });
    } else if let Some(id) = claude::id_permesso_da_voce(voce) {
        let decisione = match pulsante {
            PULSANTE_CONSENTI => Decisione::Consenti,
            PULSANTE_NEGA => Decisione::Nega,
            _ => return,
        };
        p.claude_decidi(id, decisione, Instant::now());
    }
}

/// Chi riceve ciò che il tubo ha accettato (già riconvalidato). Gira sui thread delle connessioni e
/// passa al thread dell'interfaccia solo l'evento ridotto.
struct GestoreIsola {
    /// Il registro condiviso (contatori atomici): i rifiuti si contano sul thread della connessione.
    diagnosi: Arc<Diagnosi>,
    /// Il numero del tubo a cui appartiene questo gestore (`Ponte::claude_generazione` al momento
    /// dell'apertura): un avviso arrivato quando il tubo è già un altro si ignora.
    generazione: u64,
}

impl GestoreIsola {
    fn new(diagnosi: Arc<Diagnosi>, generazione: u64) -> GestoreIsola {
        GestoreIsola { diagnosi, generazione }
    }
}

impl Ponte {
    /// Ferma il tubo in funzione (se c'è), chiude i suoi permessi (nessuna decisione: spariscono le
    /// voci, nessun clic può più indicarli) e fa salire il numero del tubo. Restituisce il nuovo
    /// numero, per il tubo che si apre subito dopo.
    pub(super) fn claude_ferma_tubo(&self) -> u64 {
        let g = self.claude_generazione.get() + 1;
        self.claude_generazione.set(g);
        let servitore = self.claude_ascolto.borrow_mut().take();
        drop(servitore);
        self.claude_chiudi_tutti();
        g
    }

    /// Il tubo numero `generazione` ha detto di aver perso il canale. Vale solo se è ancora quello
    /// in funzione: un avviso in ritardo di un servitore già sostituito o chiuso non fa nulla.
    pub(super) fn claude_canale_perso(&self, generazione: u64) {
        if generazione != self.claude_generazione.get() {
            return;
        }
        self.claude_ferma_tubo();
        voce_non_disponibile(self, ErroreAvvio::CanaleOccupato);
    }
}

/// Porta un evento al thread dell'interfaccia. Se il ciclo eventi è finito non c'è più nessuno:
/// si ignora.
fn consegna(e: EventoHook, pid: Option<u32>, catena: Vec<(u32, String)>, sequenza: u64) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            let sessione = e.sessione.clone();
            p.claude_evento(e, pid, sequenza, Instant::now());
            p.claude_imposta_catena(&sessione, catena);
        }
    });
}

/// Porta un permesso al thread dell'interfaccia.
fn consegna_permesso(
    id: u64,
    e: EventoHook,
    pid: Option<u32>,
    catena: Vec<(u32, String)>,
    tx: Option<Sender<Decisione>>,
    sequenza: u64,
) -> Result<(), slint::EventLoopError> {
    slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            let sessione = e.sessione.clone();
            p.claude_permesso(id, e, pid, tx, sequenza, Instant::now());
            p.claude_imposta_catena(&sessione, catena);
        }
    })
}

impl Gestore for GestoreIsola {
    fn evento(&self, a: Arrivo) {
        if let Contenuto::Evento(e) = a.contenuto {
            consegna(e, a.pid_mittente, a.catena_mittente, a.sequenza);
        }
    }

    /// Un permesso: l'interfaccia ne fa una voce bloccante e questo thread aspetta la decisione
    /// sul canale. Ciò che non è una richiesta di permesso non apre nessuna attesa (Claude Code
    /// prosegue col suo flusso); col ramo B la voce c'è ma la risposta no.
    fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso> {
        let Contenuto::Evento(e) = a.contenuto else { return None };
        let pid = a.pid_mittente;
        let catena = a.catena_mittente;
        let sequenza = a.sequenza;
        if e.tipo != TipoEvento::PermissionRequest {
            consegna(e, pid, catena, sequenza);
            return None;
        }
        let id = PROSSIMO_PERMESSO.fetch_add(1, Ordering::Relaxed);
        if !PERMESSI_RISOLVIBILI {
            let _ = consegna_permesso(id, e, pid, catena, None, sequenza);
            return None;
        }
        let (tx, rx) = channel();
        // Senza ciclo eventi nessuno può rispondere: il canale cade e Claude ripiega.
        consegna_permesso(id, e, pid, catena, Some(tx), sequenza).ok()?;
        Some(AttesaPermesso { id, decisione: rx })
    }

    /// Nessuna risposta è arrivata (tempo scaduto, aiutante sparito, tubo chiuso): la voce non
    /// serve più.
    fn abbandonato(&self, id: u64) {
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(p) = ponte() {
                p.claude_permesso_finito(id);
            }
        });
    }

    /// Anche con nessun evento valido (un cliente che non è l'aiutante, un messaggio storto) i
    /// rifiuti arrivano al registro diagnostico, senza dati della richiesta.
    fn rifiutata(&self) {
        self.diagnosi.conta(Contatore::claude_rifiutate);
    }

    /// Il tubo non riesce più a creare istanze (nome preso da altri, risorse finite): il servitore
    /// ha smesso. Lo si chiude davvero, i permessi in attesa si chiudono e compare la voce
    /// «Collegamento con Claude Code non disponibile».
    fn canale_perso(&self) {
        let generazione = self.generazione;
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(p) = ponte() {
                p.claude_canale_perso(generazione);
            }
        });
    }

    /// La barra di stato (già riconvalidata dal tubo): al thread dell'interfaccia va solo
    /// `DatiBarra`.
    fn barra(&self, a: Arrivo) {
        let Contenuto::Barra(d) = a.contenuto else { return };
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(p) = ponte() {
                p.claude_barra(d);
            }
        });
    }
}

/// Voci di contesto tenute al più (una per sessione, come le sessioni).
const MASSIMO_CONTESTI: usize = 16;

impl Ponte {
    /// I dati della barra di stato, sul thread dell'interfaccia. I limiti sono dell'account: le
    /// finestre presenti sostituiscono quelle note, una per una; una barra senza `rate_limits` (o
    /// senza una finestra) non cancella ciò che si sa. Il contesto è per sessione.
    pub(super) fn claude_barra(&self, d: DatiBarra) {
        self.casa.diagnosi().conta(Contatore::claude_barre);
        {
            let mut l = self.claude_limiti.borrow_mut();
            let (cinque, sette) = l.get_or_insert((None, None));
            if d.cinque_ore.is_some() {
                *cinque = d.cinque_ore;
            }
            if d.sette_giorni.is_some() {
                *sette = d.sette_giorni;
            }
        }
        {
            let mut contesto = self.claude_contesto.borrow_mut();
            match d.contesto {
                None => {
                    contesto.remove(&d.sessione);
                }
                Some(c) => {
                    if !contesto.contains_key(&d.sessione) && contesto.len() >= MASSIMO_CONTESTI {
                        // Pieno: si lasciano solo le sessioni che gli eventi conoscono ancora.
                        let note: Vec<String> =
                            self.claude_sessioni.borrow().elenco().iter().map(|s| s.id.clone()).collect();
                        contesto.retain(|id, _| note.contains(id));
                    }
                    if contesto.contains_key(&d.sessione) || contesto.len() < MASSIMO_CONTESTI {
                        contesto.insert(d.sessione, c);
                    }
                }
            }
        }
        // Limiti e contesto cambiati: la vista Claude (se aperta) li riscrive.
        self.aggiorna_vista_claude();
    }

    /// I limiti dell'abbonamento (5 ore, 7 giorni) a un'ora data (secondi dall'epoca): un limite
    /// già azzerato (`azzera_alle <= ora`) vale come assente. Senza abbonamento niente anelli.
    pub fn limiti_a(&self, ora: i64) -> (Option<Limite>, Option<Limite>) {
        let (c, s) = self.claude_limiti.borrow().unwrap_or((None, None));
        let buono = |l: &Limite| l.azzera_alle > ora && l.percentuale.is_finite();
        (c.filter(buono), s.filter(buono))
    }

    /// Come `limiti_a`, all'ora di adesso.
    pub fn limiti(&self) -> (Option<Limite>, Option<Limite>) {
        let ora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.limiti_a(ora)
    }

    /// Un evento di Claude Code, sul thread dell'interfaccia. `pid`: il processo del terminale,
    /// trovato alla prima richiesta di una sessione.
    /// `sequenza`: l'ordine di arrivo della connessione (`Arrivo::sequenza`).
    pub(super) fn claude_evento(&self, e: EventoHook, pid: Option<u32>, sequenza: u64, ora: Instant) {
        self.casa.diagnosi().conta(Contatore::claude_eventi);
        // Claude Code è andato avanti (o la sessione è finita): chi aspettava una risposta non
        // serve più. Non la `Notification`: Claude ne manda una `permission_prompt` mentre il
        // permesso è ancora aperto.
        if matches!(
            e.tipo,
            TipoEvento::PreToolUse
                | TipoEvento::UserPromptSubmit
                | TipoEvento::Stop
                | TipoEvento::StopFailure
                | TipoEvento::SessionEnd
        ) {
            self.claude_chiudi_permessi(&e.sessione, sequenza);
        }
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

    /// La catena dei processi (aiutante -> `claude.exe` -> terminale...) che il tubo ha preso al
    /// primo evento di una sessione, tenuta per portare davanti il suo terminale. Vale solo per
    /// una sessione già nota e solo se non è vuota (di norma arriva col primo evento e basta);
    /// una per sessione, quindi 16 al più; tolta con la sessione.
    pub(super) fn claude_imposta_catena(&self, sessione: &str, catena: Vec<(u32, String)>) {
        if catena.is_empty() || !self.claude_sessioni.borrow().elenco().iter().any(|s| s.id == sessione) {
            return;
        }
        let mut catene = self.claude_catene.borrow_mut();
        if !catene.contains_key(sessione) && catene.len() >= MASSIMO_CONTESTI {
            // Pieno: si lasciano solo le sessioni che gli eventi conoscono ancora.
            let note: Vec<String> = self.claude_sessioni.borrow().elenco().iter().map(|s| s.id.clone()).collect();
            catene.retain(|id, _| note.contains(id));
        }
        if catene.contains_key(sessione) || catene.len() < MASSIMO_CONTESTI {
            let raggiungibile = raggiungibili(std::slice::from_ref(&catena))[0];
            catene.insert(sessione.to_owned(), catena);
            let mut ripiego = self.claude_ripiego.borrow_mut();
            if raggiungibile {
                ripiego.remove(sessione);
            } else {
                ripiego.insert(sessione.to_owned());
            }
        }
        drop(catene);
        // La riga della sessione (già scritta dall'evento) ora sa di avere la catena.
        self.aggiorna_vista_claude();
    }

    /// Rilegge per ogni sessione con la catena salvata se il suo terminale si raggiunge (una sola
    /// istantanea per tutte). Si fa all'apertura della vista Claude, non su un timer.
    #[cfg(any(windows, test))]
    fn claude_rileggi_ripiego(&self) {
        if self.claude_catene.borrow().is_empty() {
            return;
        }
        let (ids, catene): (Vec<String>, Vec<Vec<(u32, String)>>) =
            self.claude_catene.borrow().iter().map(|(i, c)| (i.clone(), c.clone())).unzip();
        let esiti = raggiungibili(&catene);
        let mut ripiego = self.claude_ripiego.borrow_mut();
        for (id, ok) in ids.into_iter().zip(esiti) {
            if ok {
                ripiego.remove(&id);
            } else {
                ripiego.insert(id);
            }
        }
    }

    /// Porta davanti il terminale della sessione con quell'impronta (il pulsante «Apri il
    /// terminale» di una voce, o il clic o l'Invio su una riga della vista Claude). Va chiamata
    /// solo subito dopo un gesto dell'utente: Windows concede il primo piano solo allora.
    /// Un'impronta che non è più di nessuna sessione non fa nulla.
    pub(crate) fn claude_apri_terminale(&self, impronta: &str) {
        let Some(id) = self
            .claude_sessioni
            .borrow()
            .elenco()
            .iter()
            .find(|s| claude::id_sessione(&s.id) == impronta)
            .map(|s| s.id.clone())
        else {
            return;
        };
        #[cfg(all(not(windows), not(test)))]
        {
            // Altrove non c'è un terminale da portare davanti: si apre la vista Claude.
            let _ = id;
            self.apri_la_vista_claude();
        }
        #[cfg(any(windows, test))]
        self.porta_davanti_sessione(&id);
    }

    /// Il clic vero: la catena salvata, poi il ripiego sull'ultimo terminale; la riga dice
    /// «ultimo terminale usato» da quando si è ripiegato, e se non c'è nulla lo dice una voce.
    #[cfg(any(windows, test))]
    fn porta_davanti_sessione(&self, id: &str) {
        let salvata = self.claude_catene.borrow().get(id).cloned().unwrap_or_default();
        let esito = porta_davanti_il_terminale(&salvata);
        // Un rifiuto di Windows non dice nulla sul terminale della sessione: la riga resta com'è.
        let cambiato = match esito {
            Esito::Sessione => self.claude_ripiego.borrow_mut().remove(id),
            Esito::Ripiego | Esito::Niente => self.claude_ripiego.borrow_mut().insert(id.to_owned()),
            Esito::Rifiutato => false,
        };
        if cambiato {
            self.aggiorna_vista_claude();
        }
        let voce = match esito {
            Esito::Niente => Some(claude::voce_terminale_mancante(Instant::now())),
            Esito::Rifiutato => Some(claude::voce_terminale_rifiutato(Instant::now())),
            Esito::Sessione | Esito::Ripiego => None,
        };
        if let Some(voce) = voce {
            self.metti_voce(voce, false);
            self.sveglia_terminale.start(slint::TimerMode::SingleShot, Duration::from_secs(10), || {
                if let Some(p) = ponte() {
                    p.togli_voce(ID_TERMINALE_MANCANTE);
                }
            });
        }
    }

    /// Fuori da Windows: il pannello sulla vista Claude. Già aperto: ci si porta; chiuso: si apre e
    /// `pannello_aperto` lo mette sulla vista Claude (un pannello nuovo parte dalla panoramica).
    #[cfg(any(not(windows), test))]
    fn apri_la_vista_claude(&self) {
        let aperto = self.pannello.borrow().upgrade();
        match aperto {
            Some(p) => {
                p.set_vista("claude".into());
                self.vista_claude(true);
            }
            None => {
                self.vista_richiesta.set(Some("claude"));
                crate::sistema::servizi::apri();
            }
        }
    }

    /// Un permesso chiesto da Claude Code, sul thread dell'interfaccia: una voce bloccante e il
    /// canale della risposta. Oltre i 16 in attesa il canale cade: l'ascoltatore risponde `{}` e
    /// decide il terminale (stesso esito di nessuna attesa). La voce non c'è: resta quella normale
    /// «Claude ti aspetta».
    pub(super) fn claude_permesso(
        &self,
        id: u64,
        e: EventoHook,
        pid: Option<u32>,
        tx: Option<Sender<Decisione>>,
        sequenza: u64,
        ora: Instant,
    ) {
        if self.claude_permessi.borrow().len() >= MASSIMO_PERMESSI || self.claude_permessi.borrow().contains_key(&id) {
            drop(tx);
            self.claude_evento(e, pid, sequenza, ora);
            return;
        }
        self.claude_permessi.borrow_mut().insert(id, Pendente { sessione: e.sessione.clone(), tx, sequenza });
        let voce = claude::voce_permesso(id, &e, ora);
        // Prima il permesso in elenco, poi l'evento: la voce «ti aspetta» della sessione cede il
        // posto a quella del permesso.
        self.claude_evento(e, pid, sequenza, ora);
        self.metti_voce(voce, true);
    }

    /// Il gesto dell'utente su una voce di permesso: la decisione va **solo** a chi ha chiesto e la
    /// voce sparisce. Un clic tardivo (voce già tolta) non fa nulla.
    /// «Consenti» entro `ARMAMENTO_CONSENTI` da un cambio della voce mostrata si ignora (la voce e
    /// il canale restano); «Nega» no.
    pub(super) fn claude_decidi(&self, id: u64, decisione: Decisione, ora: Instant) {
        if decisione == Decisione::Consenti && ora < self.claude_armamento.borrow().dal.get() {
            return;
        }
        let Some(p) = self.claude_permessi.borrow_mut().remove(&id) else { return };
        // L'ascoltatore può aver già rinunciato (tempo scaduto, client chiuso): senza chi
        // riceve, la decisione non c'è stata e il terminale aspetta ancora.
        if p.tx.as_ref().is_some_and(|tx| tx.send(decisione).is_err()) {
            self.claude_permesso_chiuso(id, &p.sessione);
            return;
        }
        // Risposto dall'assistente: Claude Code esegue (o rinuncia) subito, e senza un nuovo
        // evento la sessione resterebbe «ti aspetta». Va al lavoro, con lo strumento che aveva.
        self.togli_voce(&claude::id_voce_permesso(id));
        let ora = Instant::now();
        let cambi = self.claude_sessioni.borrow_mut().segna_al_lavoro(&p.sessione, ora);
        self.claude_cambiate(&cambi, ora);
        self.voci_sessione(&p.sessione, ora);
    }

    /// Un'azione della pillola: vale solo se la voce è ancora quella che la pillola mostra (e che
    /// aveva i pulsanti sotto gli occhi dell'utente); altrimenti è un clic su una voce cambiata.
    pub(super) fn azione_pillola(&self, voce_resa: &str, pulsante: &str) {
        let mostrata = self.mostrato.borrow().as_ref().and_then(|v| v.voce_allargata().map(str::to_owned));
        if mostrata.as_deref() == Some(voce_resa) {
            self.azione(voce_resa, pulsante);
        }
    }

    /// Dopo ogni cambio delle voci: se la voce della pillola o la prima del pannello è cambiata,
    /// «Consenti» si arma fra `ARMAMENTO_CONSENTI`.
    pub(super) fn claude_riarma(&self, ora: Instant) {
        let (pillola, pannello) = {
            let a = self.attese.borrow();
            let v = a.ordinate();
            (
                v.iter().filter(|x| x.livello == Livello::Bloccante).min_by_key(|x| x.creata).map(|x| x.id.clone()),
                v.first().map(|x| x.id.clone()),
            )
        };
        let mut arm = self.claude_armamento.borrow_mut();
        if arm.pillola != pillola || arm.pannello != pannello {
            if (pillola.is_some() && arm.pillola != pillola) || (pannello.is_some() && arm.pannello != pannello) {
                arm.dal.set(ora + ARMAMENTO_CONSENTI);
            }
            arm.pillola = pillola;
            arm.pannello = pannello;
        }
    }

    /// Una superficie che mostra i permessi compare (pannello aperto, pillola che torna): con un
    /// permesso in attesa «Consenti» si arma fra `ARMAMENTO_CONSENTI`.
    pub(super) fn claude_superficie_compare(&self, ora: Instant) {
        if !self.claude_permessi.borrow().is_empty() {
            self.claude_armamento.borrow().dal.set(ora + ARMAMENTO_CONSENTI);
        }
    }

    /// Un nuovo ascolto parte: chi aspettava sul vecchio non c'è più. `Nessuna` a tutti, voci tolte.
    pub(super) fn claude_chiudi_tutti(&self) {
        let tutti: Vec<(u64, Pendente)> = self.claude_permessi.borrow_mut().drain().collect();
        let sessioni: Vec<String> = tutti.iter().map(|(_, p)| p.sessione.clone()).collect();
        for (id, p) in tutti {
            if let Some(tx) = &p.tx {
                let _ = tx.send(Decisione::Nessuna);
            }
            self.togli_voce(&claude::id_voce_permesso(id));
        }
        let ora = Instant::now();
        for s in sessioni {
            self.voci_sessione(&s, ora);
        }
    }

    /// L'ascoltatore ha lasciato il permesso `id` senza risposta: via la voce.
    pub(super) fn claude_permesso_finito(&self, id: u64) {
        let Some(p) = self.claude_permessi.borrow_mut().remove(&id) else { return };
        self.claude_permesso_chiuso(id, &p.sessione);
    }

    /// Una sessione è andata avanti: i suoi permessi in attesa si chiudono senza decisione (Claude
    /// Code risponde da sé) e le voci spariscono. Le altre sessioni non si toccano.
    /// Solo quelli arrivati **prima** dell'evento che chiude (`sequenza` più bassa): una
    /// `PreToolUse` e la sua stessa `PermissionRequest` arrivano a pochi istanti e su connessioni
    /// diverse, e la richiesta può passare per prima dall'interfaccia.
    fn claude_chiudi_permessi(&self, sessione: &str, sequenza: u64) {
        let chiusi: Vec<(u64, Pendente)> = {
            let mut m = self.claude_permessi.borrow_mut();
            let ids: Vec<u64> = m.iter().filter(|(_, p)| p.sessione == sessione && p.sequenza < sequenza).map(|(id, _)| *id).collect();
            ids.into_iter().filter_map(|id| m.remove(&id).map(|p| (id, p))).collect()
        };
        for (id, p) in chiusi {
            if let Some(tx) = &p.tx {
                let _ = tx.send(Decisione::Nessuna);
            }
            self.togli_voce(&claude::id_voce_permesso(id));
        }
    }

    /// Tolto il permesso dall'elenco: via la sua voce e la sessione riprende la voce normale
    /// (quella «ti aspetta» era nascosta dal permesso).
    fn claude_permesso_chiuso(&self, id: u64, sessione: &str) {
        self.togli_voce(&claude::id_voce_permesso(id));
        self.voci_sessione(sessione, Instant::now());
        self.aggiorna_claude_pillola();
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
        self.claude_contesto.borrow_mut().remove(id);
        self.claude_catene.borrow_mut().remove(id);
        self.claude_ripiego.borrow_mut().remove(id);
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
        // Con un permesso in attesa parla solo quello (bloccante): «Claude ti aspetta» no.
        let [aspetta, _, _] = claude::id_voci(id);
        let col_permesso = self.claude_permessi.borrow().values().any(|p| p.sessione == id);
        let voce = voce.filter(|v| !(col_permesso && v.id == aspetta));
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

    /// Il timer da un secondo serve solo se almeno una sessione lavora **e** qualcosa mostra la
    /// durata: la pillola (si vede e non dorme) oppure la vista Claude aperta (a pannello aperto la
    /// pillola è nascosta, e la vista deve scorrere lo stesso). Una condizione sola, qui.
    pub(super) fn attivita_serve(&self) -> bool {
        self.claude_sessioni.borrow().lavorano() > 0
            && (self.vista_claude.get() || (self.pillola_mostrata.get() && !self.pillola_dorme()))
    }

    /// La vista Claude si apre o si chiude (il pannello cambia vista o si chiude). Aperta scrive i
    /// suoi dati e, se serve, accende il timer da un secondo; chiusa non lavora più.
    pub fn vista_claude(&self, si: bool) {
        if self.vista_claude.replace(si) == si {
            return;
        }
        if si {
            // Il terminale di una sessione può essere sparito dall'ultima volta: lo si rilegge qui.
            #[cfg(any(windows, test))]
            self.claude_rileggi_ripiego();
        }
        if !si {
            self.sveglia_limiti.stop();
            self.limiti_armati.set(None);
        }
        self.aggiorna_claude_pillola();
    }

    /// Una sveglia singola (mai ripetuta) all'azzeramento del primo limite mostrato: ripassa dalla
    /// vista, che toglie l'anello scaduto. Si riarma solo se l'istante cambia; senza limiti si ferma.
    fn arma_sveglia_limiti(&self, prossimo: Option<i64>, adesso: i64) {
        if self.limiti_armati.get() == prossimo {
            return;
        }
        self.limiti_armati.set(prossimo);
        match prossimo {
            None => self.sveglia_limiti.stop(),
            Some(quando) => {
                // Un secondo dopo, perché `azzera_alle <= ora` valga di sicuro.
                let tra = Duration::from_secs((quando - adesso).max(0) as u64 + 1);
                self.sveglia_limiti.start(slint::TimerMode::SingleShot, tra, || {
                    if let Some(p) = ponte() {
                        p.limiti_armati.set(None);
                        p.aggiorna_vista_claude();
                    }
                });
            }
        }
    }

    /// I dati della vista Claude, scritti solo se la vista è aperta e solo ciò che è cambiato: le
    /// righe (tre al più), i limiti dell'abbonamento (un limite assente o azzerato non ha anello),
    /// la spesa del mese e se Claude Code è collegato.
    fn aggiorna_vista_claude(&self) {
        if !self.vista_claude.get() {
            return;
        }
        let Some(p) = self.pannello.borrow().upgrade() else { return };
        let collegato = self.casa.con_config(|c| c.claude.collegato);
        if p.get_claude_collegato() != collegato {
            p.set_claude_collegato(collegato);
        }
        // Scollegato non ci sono sessioni da mostrare: lo stato vuoto parla da sé.
        let (righe, altre) = if collegato {
            claude::righe_sessioni(&self.claude_sessioni.borrow(), &self.claude_contesto.borrow(), Instant::now())
        } else {
            (Vec::new(), 0)
        };
        // Per impronta: la catena c'è e raggiunge il terminale / è un Windows Terminal o un VS Code.
        let (raggiungibili, a_finestre): (Vec<String>, Vec<String>) = {
            let ripiego = self.claude_ripiego.borrow();
            let catene = self.claude_catene.borrow();
            let a_finestre = |c: &Vec<(u32, String)>| {
                c.iter().any(|(_, n)| n.eq_ignore_ascii_case("WindowsTerminal.exe") || n.eq_ignore_ascii_case("Code.exe"))
            };
            (
                catene.keys().filter(|id| !ripiego.contains(*id)).map(|id| claude::id_sessione(id)).collect(),
                catene.iter().filter(|(_, c)| a_finestre(c)).map(|(id, _)| claude::id_sessione(id)).collect(),
            )
        };
        let voci = righe
            .into_iter()
            .map(|r| {
                let (ripiego, a_finestra) = (!raggiungibili.contains(&r.id), a_finestre.contains(&r.id));
                riga_ui(r, ripiego, a_finestra)
            })
            .collect();
        let n = super::aggiorna_modello(&self.claude_righe, voci);
        self.casa.diagnosi().conta_n(Contatore::ui_righe, n);
        let altre = altre as i32;
        if p.get_claude_altre() != altre {
            p.set_claude_altre(altre);
        }
        let adesso = chrono::Local::now();
        let (cinque, sette) = self.limiti();
        self.arma_sveglia_limiti([cinque, sette].into_iter().flatten().map(|l| l.azzera_alle).min(), adesso.timestamp());
        let (cinque, sette) = (limite_ui(cinque, adesso), limite_ui(sette, adesso));
        if p.get_claude_cinque_ore() != cinque {
            p.set_claude_cinque_ore(cinque);
        }
        if p.get_claude_sette_giorni() != sette {
            p.set_claude_sette_giorni(sette);
        }
        // Il timer della lettura esiste solo con la chiave da amministratore.
        let con_chiave = self.sveglia_api.running();
        let (spesa, nota) = {
            let ultimo = self.api_ultimo.borrow();
            claude::testo_spesa(ultimo.as_ref().and_then(|(_, esito)| esito.as_ref().ok()), con_chiave)
        };
        if p.get_claude_dettagli() != con_chiave {
            p.set_claude_dettagli(con_chiave);
        }
        if p.get_claude_spesa() != spesa.as_str() {
            p.set_claude_spesa(spesa.as_str().into());
        }
        if p.get_claude_spesa_nota() != nota.as_str() {
            p.set_claude_spesa_nota(nota.as_str().into());
        }
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
        // La vista Claude (se aperta) segue le sessioni: a pannello aperto la pillola è nascosta e
        // più sotto si esce, ma la vista deve scorrere comunque.
        self.aggiorna_vista_claude();
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

/// Come è andato un «Apri il terminale».
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Esito {
    /// Davanti la finestra del terminale della sessione.
    Sessione,
    /// La catena non portava a nessuna finestra: davanti l'ultimo terminale usato.
    Ripiego,
    /// Nessuna finestra da portare davanti.
    Niente,
    /// La finestra c'è ma Windows non ha permesso di portarla davanti.
    Rifiutato,
}

/// Il passo finale di «Apri il terminale» (solo Windows): la catena salvata, l'elenco dei processi
/// di adesso e le finestre visibili danno la finestra del terminale; senza, quella dell'ultimo
/// terminale in primo piano (`primo_piano`).
#[cfg(all(windows, not(test)))]
fn porta_davanti_il_terminale(salvata: &[(u32, String)]) -> Esito {
    use crate::sistema::{primo_piano, terminale};
    let (hwnd, ripiego) =
        match terminale::finestra_della_sessione(salvata, &terminale::processi(), &terminale::finestre_visibili()) {
            Some(h) => (Some(h), false),
            None => (primo_piano::ultimo_terminale(), true),
        };
    match hwnd {
        Some(h) if terminale::porta_davanti(h) => {
            if ripiego {
                Esito::Ripiego
            } else {
                Esito::Sessione
            }
        }
        Some(_) => Esito::Rifiutato,
        None => Esito::Niente,
    }
}

/// Per ogni catena: la sua finestra si trova (una sola istantanea per tutte).
#[cfg(all(windows, not(test)))]
fn raggiungibili(catene: &[Vec<(u32, String)>]) -> Vec<bool> {
    use crate::sistema::terminale;
    let (processi, finestre) = (terminale::processi(), terminale::finestre_visibili());
    catene.iter().map(|c| terminale::finestra_della_sessione(c, &processi, &finestre).is_some()).collect()
}

// Fuori da Windows (e fuori dai test) non c'è nulla da raggiungere: nessuno lo chiede.
#[cfg(all(not(windows), not(test)))]
fn raggiungibili(catene: &[Vec<(u32, String)>]) -> Vec<bool> {
    vec![true; catene.len()]
}

// Nei test non si tocca nessuna finestra: si annota la catena con cui sarebbe partita e l'esito
// (e la raggiungibilità) lo decide il test.
#[cfg(test)]
thread_local! {
    static PORTATI_DAVANTI: std::cell::RefCell<Vec<Vec<(u32, String)>>> = const { std::cell::RefCell::new(Vec::new()) };
    static ESITO_PROVA: Cell<Esito> = const { Cell::new(Esito::Sessione) };
    static RAGGIUNGIBILE_PROVA: Cell<bool> = const { Cell::new(true) };
    /// Quante istantanee (una per chiamata a `raggiungibili`) si sono prese.
    static ISTANTANEE_PROVA: Cell<u32> = const { Cell::new(0) };
}

#[cfg(test)]
fn porta_davanti_il_terminale(salvata: &[(u32, String)]) -> Esito {
    PORTATI_DAVANTI.with(|v| v.borrow_mut().push(salvata.to_vec()));
    ESITO_PROVA.with(Cell::get)
}

#[cfg(test)]
fn raggiungibili(catene: &[Vec<(u32, String)>]) -> Vec<bool> {
    ISTANTANEE_PROVA.with(|c| c.set(c.get() + 1));
    vec![RAGGIUNGIBILE_PROVA.with(Cell::get); catene.len()]
}

/// Una riga della vista Claude come la vuole l'interfaccia. `ripiego`: il terminale della sessione
/// non si raggiunge (nessuna catena salvata o nessuna finestra), il clic ripiega sull'ultimo
/// terminale usato (solo Windows). `finestra_non_scheda`: il terminale probabile è Windows Terminal
/// o VS Code, di cui si porta davanti la finestra e non la scheda.
fn riga_ui(r: claude::RigaSessione, ripiego: bool, finestra_non_scheda: bool) -> RigaSessioneUi {
    RigaSessioneUi {
        terminale: if ripiego && cfg!(windows) { "ultimo terminale usato" } else { "" }.into(),
        avviso: if finestra_non_scheda && cfg!(windows) { "Apre la finestra del terminale, non la scheda" } else { "" }.into(),
        id: r.id.into(),
        cartella: r.cartella.into(),
        stato: r.stato_testo.into(),
        livello: r.livello.map_or(LivelloUi::Nessuno, pillola::livello_ui),
        strumento: r.strumento.into(),
        durata: r.durata.into(),
        contesto: r.contesto.unwrap_or_default().into(),
    }
}

/// Un limite dell'abbonamento per l'anello; assente (o azzerato) = nessun anello.
fn limite_ui(l: Option<Limite>, ora: chrono::DateTime<chrono::Local>) -> LimiteClaudeUi {
    match l {
        None => LimiteClaudeUi::default(),
        Some(l) => {
            let t = claude::testo_limite(&l, ora);
            LimiteClaudeUi { presente: true, frazione: t.frazione, testo: t.testo.into(), azzera: t.azzera.into() }
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
        p.claude_evento(e, None, SEQUENZA.fetch_add(1, Ordering::Relaxed), Instant::now());
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
        p.claude_evento(ev("a", TipoEvento::UserPromptSubmit), Some(4321), SEQUENZA.fetch_add(1, Ordering::Relaxed), Instant::now());
        assert_eq!(p.claude_sessioni.borrow().elenco()[0].pid, Some(4321));
    }

    fn catena_prova() -> Vec<(u32, String)> {
        vec![(7, "isola-hook.exe".into()), (5, "claude.exe".into()), (3, "WindowsTerminal.exe".into())]
    }

    fn portati_davanti() -> Vec<Vec<(u32, String)>> {
        PORTATI_DAVANTI.with(|v| std::mem::take(&mut *v.borrow_mut()))
    }

    /// Ripristina i valori di prova (i thread di test si riusano).
    fn esito_prova(e: Esito, raggiungibile: bool) {
        ESITO_PROVA.with(|c| c.set(e));
        RAGGIUNGIBILE_PROVA.with(|c| c.set(raggiungibile));
    }

    /// Una sessione con la catena arrivata col primo evento.
    fn con_catena(p: &Ponte, id: &str, tipo: TipoEvento) {
        manda(p, ev(id, tipo));
        p.claude_imposta_catena(id, catena_prova());
    }

    #[test]
    fn la_catena_si_tiene_per_sessione_con_tetto_e_si_toglie_con_la_sessione() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        // Sessione ignota: niente. Catena vuota: niente.
        p.claude_imposta_catena("a", catena_prova());
        assert!(p.claude_catene.borrow().is_empty());
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        p.claude_imposta_catena("a", Vec::new());
        assert!(p.claude_catene.borrow().is_empty());
        p.claude_imposta_catena("a", catena_prova());
        assert_eq!(p.claude_catene.borrow().get("a"), Some(&catena_prova()));
        manda(&p, ev("a", TipoEvento::SessionEnd));
        assert!(p.claude_catene.borrow().is_empty(), "sparisce con la sessione");
        // Al più 16 (come le sessioni).
        for i in 0..40 {
            con_catena(&p, &format!("s{i}"), TipoEvento::UserPromptSubmit);
        }
        assert!(p.claude_catene.borrow().len() <= 16);
        manda(&p, ev("s0", TipoEvento::SessionEnd));
        assert!(!p.claude_catene.borrow().contains_key("s0"));
    }

    #[test]
    fn il_pulsante_terminale_porta_davanti_la_catena_della_sessione() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        portati_davanti();
        con_catena(&p, "a", TipoEvento::Stop);
        manda(&p, ev("b", TipoEvento::PermissionRequest));
        let voci = voci_claude(&p);
        let fatto = voci.iter().find(|v| v.0.ends_with(".fatto")).expect("voce fatto").0.clone();
        let aspetta = voci.iter().find(|v| v.0.ends_with(".aspetta")).expect("voce aspetta").0.clone();
        for v in [&fatto, &aspetta] {
            let pulsanti = p.attese.borrow().voce(v).unwrap().pulsanti.clone();
            assert_eq!(pulsanti.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), [PULSANTE_TERMINALE]);
        }
        p.azione(&fatto, PULSANTE_TERMINALE);
        assert_eq!(portati_davanti(), vec![catena_prova()]);
        // La sessione senza catena (aiutante non visto): parte lo stesso, con la catena vuota (ripiego).
        p.azione(&aspetta, PULSANTE_TERMINALE);
        assert_eq!(portati_davanti(), vec![Vec::<(u32, String)>::new()]);
        // Altri pulsanti, voci che non sono sessioni e impronte ignote: nessuna finestra.
        p.azione(&fatto, "altro");
        p.azione(ID_COLLEGAMENTO, PULSANTE_TERMINALE);
        p.azione("claude.permesso.1", PULSANTE_TERMINALE);
        p.azione("claude.0123456789abcdef.fatto", PULSANTE_TERMINALE);
        assert!(portati_davanti().is_empty());
    }

    #[test]
    fn il_clic_sulla_riga_della_vista_porta_davanti_lo_stesso_terminale() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        portati_davanti();
        con_catena(&p, "a", TipoEvento::UserPromptSubmit);
        let impronta = claude::id_sessione("a");
        p.claude_apri_terminale(&impronta);
        assert_eq!(portati_davanti(), vec![catena_prova()]);
        // Una sessione sparita (impronta ormai di nessuno): niente.
        manda(&p, ev("a", TipoEvento::SessionEnd));
        p.claude_apri_terminale(&impronta);
        assert!(portati_davanti().is_empty());
    }

    #[test]
    fn la_riga_dice_ultimo_terminale_usato_solo_senza_catena() {
        let riga = |senza| riga_ui(
            claude::RigaSessione {
                id: "x".into(),
                cartella: "c".into(),
                stato_testo: "Fatto",
                livello: None,
                strumento: String::new(),
                durata: String::new(),
                contesto: None,
            },
            senza,
            false,
        );
        assert_eq!(riga(false).terminale.as_str(), "");
        assert_eq!(riga(true).terminale.as_str(), if cfg!(windows) { "ultimo terminale usato" } else { "" });
    }

    /// Il filo intero: catena salvata -> raggiungibilità -> riga della vista. Se la finestra della
    /// catena non c'è la riga lo dichiara; un clic andato bene lo toglie, uno ripiegato lo rimette.
    #[test]
    fn la_riga_dichiara_il_ripiego_quando_puo_scattare() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        esito_prova(Esito::Sessione, true);
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        let attesa = if cfg!(windows) { "ultimo terminale usato" } else { "" };
        p.vista_claude(true);
        // Catena con finestra: nessuna dichiarazione.
        con_catena(&p, "a", TipoEvento::UserPromptSubmit);
        assert_eq!(pannello.get_claude_righe().row_data(0).unwrap().terminale.as_str(), "");
        // Catena senza finestra (terminale chiuso, passaggio di consegne di Windows 11...).
        esito_prova(Esito::Ripiego, false);
        con_catena(&p, "b", TipoEvento::UserPromptSubmit);
        let terminale = |id: &str| testo_terminale(&pannello, id);
        assert_eq!(terminale("a"), "");
        assert_eq!(terminale("b"), attesa);
        // Riaprendo la vista si rilegge: ora anche «a» non ha più la finestra.
        p.vista_claude(false);
        p.vista_claude(true);
        assert_eq!((terminale("a"), terminale("b")), (attesa.to_owned(), attesa.to_owned()));
        // Un clic che ripiega lo dichiara da lì in poi; uno riuscito lo toglie.
        esito_prova(Esito::Sessione, true);
        p.claude_apri_terminale(&claude::id_sessione("a"));
        assert_eq!(terminale("a"), "");
        esito_prova(Esito::Ripiego, true);
        p.claude_apri_terminale(&claude::id_sessione("a"));
        assert_eq!(terminale("a"), attesa);
        esito_prova(Esito::Sessione, true);
    }

    /// Il testo «ultimo terminale usato» (o vuoto) della riga della sessione `id`.
    fn testo_terminale(pannello: &crate::Pannello, id: &str) -> String {
        let righe = pannello.get_claude_righe();
        (0..righe.row_count())
            .map(|i| righe.row_data(i).unwrap())
            .find(|r| r.id.as_str() == claude::id_sessione(id))
            .unwrap()
            .terminale
            .to_string()
    }

    #[test]
    fn un_rifiuto_di_windows_ha_la_sua_voce_e_non_segna_il_ripiego() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        esito_prova(Esito::Sessione, true);
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        p.vista_claude(true);
        con_catena(&p, "a", TipoEvento::UserPromptSubmit);
        esito_prova(Esito::Rifiutato, true);
        p.claude_apri_terminale(&claude::id_sessione("a"));
        esito_prova(Esito::Sessione, true);
        assert!(!p.claude_ripiego.borrow().contains("a"), "il terminale della sessione c'è: non è un ripiego");
        assert_eq!(testo_terminale(&pannello, "a"), "");
        let voce = p.attese.borrow().voce(ID_TERMINALE_MANCANTE).cloned().expect("la voce c'è");
        assert_eq!(voce.dettaglio, "Windows non ha permesso di portare davanti il terminale: cliccalo nella barra.");
        assert_ne!(voce.titolo, "Non trovo il terminale di questa sessione");
        assert!(p.sveglia_terminale.running());
    }

    #[test]
    fn senza_catene_la_rilettura_non_fa_nulla() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        let prima = ISTANTANEE_PROVA.with(Cell::get);
        p.claude_rileggi_ripiego();
        assert_eq!(ISTANTANEE_PROVA.with(Cell::get), prima, "nessuna catena, nessuna istantanea");
        // Con una catena salvata l'istantanea si prende.
        con_catena(&p, "a", TipoEvento::UserPromptSubmit);
        let prima = ISTANTANEE_PROVA.with(Cell::get);
        p.claude_rileggi_ripiego();
        assert_eq!(ISTANTANEE_PROVA.with(Cell::get), prima + 1);
    }

    #[test]
    fn fuori_da_windows_un_pannello_chiuso_si_apre_sulla_vista_claude() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.apri_la_vista_claude();
        assert_eq!(p.vista_richiesta.get(), Some("claude"));
        let pannello = con_pannello(&p);
        assert_eq!(pannello.get_vista().as_str(), "claude");
        assert_eq!(p.vista_richiesta.get(), None, "una volta sola");
    }

    #[test]
    fn senza_finestra_una_voce_lo_dice_e_sparisce() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        con_catena(&p, "a", TipoEvento::UserPromptSubmit);
        esito_prova(Esito::Niente, true);
        p.claude_apri_terminale(&claude::id_sessione("a"));
        esito_prova(Esito::Sessione, true);
        let voce = p.attese.borrow().voce(ID_TERMINALE_MANCANTE).cloned().expect("la voce c'è");
        assert_eq!(voce.livello, Livello::Informazione);
        assert_eq!(voce.titolo, "Non trovo il terminale di questa sessione");
        assert!(p.sveglia_terminale.running(), "sparisce da sola");
        p.azione(ID_TERMINALE_MANCANTE, crate::attenzione::PULSANTE_HO_CAPITO);
        assert!(p.attese.borrow().voce(ID_TERMINALE_MANCANTE).is_none());
        assert!(!p.sveglia_terminale.running());
        // Un clic andato bene non la mette.
        p.claude_apri_terminale(&claude::id_sessione("a"));
        assert!(p.attese.borrow().voce(ID_TERMINALE_MANCANTE).is_none());
    }

    #[test]
    fn l_avviso_della_finestra_solo_per_windows_terminal_e_vs_code() {
        let r = || claude::RigaSessione {
            id: "x".into(),
            cartella: "c".into(),
            stato_testo: "Fatto",
            livello: None,
            strumento: String::new(),
            durata: String::new(),
            contesto: None,
        };
        assert_eq!(riga_ui(r(), false, false).avviso.as_str(), "");
        let con = riga_ui(r(), false, true).avviso.to_string();
        assert_eq!(con, if cfg!(windows) { "Apre la finestra del terminale, non la scheda" } else { "" });
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

    /// Una configurazione con un nome di tubo unico (mai quello vero) che accetta questo processo.
    #[cfg(windows)]
    fn config_di_prova() -> ConfigTubo {
        use std::sync::atomic::AtomicU64;
        static N: AtomicU64 = AtomicU64::new(0);
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        ConfigTubo {
            nome: format!(r"\\.\pipe\isola-prova-assistente-{}-{}-{t}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)),
            cliente_atteso: std::env::current_exe().unwrap(),
            attesa_massima: ATTESA_MASSIMA,
        }
    }

    #[test]
    fn il_cliente_atteso_e_l_aiutante_accanto() {
        let exe = Path::new("/app/assistente.exe");
        let accanto = Path::new("/app/isola-hook.exe");
        assert_eq!(cliente_atteso(exe, false, None), accanto);
        // La variabile vale solo in prova.
        let v = Some(OsString::from("/prova/sessioni_finte.exe"));
        assert_eq!(cliente_atteso(exe, false, v.clone()), accanto);
        assert_eq!(cliente_atteso(exe, true, v), Path::new("/prova/sessioni_finte.exe"));
        assert_eq!(cliente_atteso(exe, true, Some(OsString::new())), accanto);
        // In prova il tubo ha un nome suo; fuori, quello dell'utente.
        let vero = config_tubo_da(r"\\.\pipe\isola-claude-S-1-5-21-1", exe, false, None);
        assert_eq!(vero.nome, r"\\.\pipe\isola-claude-S-1-5-21-1");
        let prova = config_tubo_da(r"\\.\pipe\isola-claude-S-1-5-21-1", exe, true, None);
        assert_eq!(prova.nome, r"\\.\pipe\isola-claude-S-1-5-21-1-prova");
        assert_eq!(prova.cliente_atteso, accanto);
        if cfg!(windows) {
            let c = config_tubo().expect("configurazione vera");
            assert!(c.nome.starts_with(r"\\.\pipe\isola-claude-S-1-"), "{}", c.nome);
            assert_eq!(c.attesa_massima, ATTESA_MASSIMA);
        }
    }

    #[cfg(windows)]
    #[test]
    fn scollegato_niente_tubo() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        assert!(!p.casa.con_config(|c| c.claude.collegato));
        avvia_se_collegato_con(&p, Some(config_di_prova()));
        assert!(p.claude_ascolto.borrow().is_none());
        assert!(voci_claude(&p).is_empty());
        // Collegato: il tubo si apre, senza codice né porta.
        p.casa.aggiorna_config(|c| {
            c.claude.collegato = true;
        });
        avvia_se_collegato_con(&p, Some(config_di_prova()));
        assert!(p.claude_ascolto.borrow().is_some());
        assert!(!p.attese.borrow().contiene(ID_COLLEGAMENTO));
        spegni();
        // Senza configurazione (SID illeggibile): voce, nessun tubo.
        avvia_se_collegato_con(&p, None);
        assert!(p.claude_ascolto.borrow().is_none());
        let a = p.attese.borrow();
        assert_eq!(a.voce(ID_COLLEGAMENTO).unwrap().dettaglio, "Non è stato possibile aprire il canale.");
    }

    #[cfg(windows)]
    #[test]
    fn avvia_e_si_ferma() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let c = config_di_prova();
        avvia_con(&p, c.clone());
        assert!(p.claude_ascolto.borrow().is_some());
        assert!(!p.attese.borrow().contiene(ID_COLLEGAMENTO));
        // Di nuovo con lo stesso nome: il vecchio tubo lascia il nome e il nuovo lo riprende.
        avvia_con(&p, c);
        assert!(p.claude_ascolto.borrow().is_some(), "riaperto");
        assert!(!p.attese.borrow().contiene(ID_COLLEGAMENTO));
        spegni();
        assert!(p.claude_ascolto.borrow().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn canale_occupato_da_avviso() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let c = config_di_prova();
        // Un altro «programma» (un altro servitore) ha già il nome.
        struct Nessuno;
        impl Gestore for Nessuno {
            fn evento(&self, _: Arrivo) {}
            fn permesso(&self, _: Arrivo) -> Option<AttesaPermesso> {
                None
            }
            fn abbandonato(&self, _: u64) {}
            fn barra(&self, _: Arrivo) {}
        }
        let _occupante = avvia(c.clone(), Arc::new(Nessuno)).unwrap();
        avvia_con(&p, c);
        assert!(p.claude_ascolto.borrow().is_none());
        let a = p.attese.borrow();
        let v = a.voce(ID_COLLEGAMENTO).expect("voce del collegamento");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.titolo, "Collegamento con Claude Code non disponibile");
        assert_eq!(v.dettaglio, "Il canale è occupato da un altro programma.");
        assert_eq!(v.pulsanti, vec![Pulsante::new(PULSANTE_IMPOSTAZIONI, "Impostazioni")]);
    }

    /// Un «canale perso» in ritardo di un tubo già sostituito (o chiuso) non ferma quello nuovo.
    #[cfg(windows)]
    #[test]
    fn l_avviso_di_un_tubo_vecchio_non_ferma_quello_nuovo() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        avvia_con(&p, config_di_prova());
        let vecchia = p.claude_generazione.get();
        avvia_con(&p, config_di_prova());
        let nuova = p.claude_generazione.get();
        assert!(nuova > vecchia);
        // L'avviso del vecchio arriva solo ora: ignorato, il nuovo resta aperto e nessuna voce.
        p.claude_canale_perso(vecchia);
        assert!(p.claude_ascolto.borrow().is_some(), "il tubo nuovo resta");
        assert!(!p.attese.borrow().contiene(ID_COLLEGAMENTO));
        // L'avviso del tubo in funzione vale: si ferma e compare la voce.
        p.claude_canale_perso(nuova);
        assert!(p.claude_ascolto.borrow().is_none());
        assert!(p.attese.borrow().contiene(ID_COLLEGAMENTO));
        // Un tubo riaperto dopo: l'avviso della «nuova» di prima non lo tocca.
        avvia_con(&p, config_di_prova());
        p.claude_canale_perso(nuova);
        assert!(p.claude_ascolto.borrow().is_some());
        // Chiuso a mano (spegni, scollega): un avviso in ritardo non riapre nulla né mette voci.
        let attuale = p.claude_generazione.get();
        spegni();
        p.togli_voce(ID_COLLEGAMENTO);
        p.claude_canale_perso(attuale);
        assert!(!p.attese.borrow().contiene(ID_COLLEGAMENTO), "tubo già chiuso: nessun avviso");
    }

    #[test]
    fn i_rifiuti_dell_ascoltatore_arrivano_alla_diagnosi() {
        let diagnosi = Arc::new(Diagnosi::new(true));
        let g = GestoreIsola::new(diagnosi.clone(), 1);
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

    // ─── Permessi (Task 5) ───────────────────────────────────────────────────

    use std::sync::mpsc::{Receiver, TryRecvError};

    fn chiede(sessione: &str, comando: &str) -> EventoHook {
        EventoHook {
            strumento: Some("PowerShell".into()),
            riassunto: Some(comando.into()),
            ..ev(sessione, TipoEvento::PermissionRequest)
        }
    }

    /// Un permesso della sessione `sessione`, come lo consegna il gestore: il ricevitore è quello
    /// che aspetta l'ascoltatore.
    fn permesso(p: &Ponte, id: u64, sessione: &str) -> Receiver<Decisione> {
        let (tx, rx) = channel();
        p.claude_permesso(id, chiede(sessione, "ls -la"), None, Some(tx), SEQUENZA.fetch_add(1, Ordering::Relaxed), Instant::now());
        rx
    }

    /// «Consenti» armato: è passato il tempo dall'ultimo cambio della voce mostrata.
    fn armato(p: &Ponte) {
        p.claude_armamento.borrow().dal.set(Instant::now());
    }

    fn consenti(p: &Ponte, id: u64) {
        armato(p);
        p.azione(&voce_di(id), PULSANTE_CONSENTI);
    }

    /// Numeri d'ordine di arrivo per le prove: ogni permesso o evento di prova ne prende uno più alto.
    static SEQUENZA: AtomicU64 = AtomicU64::new(1);

    fn voce_di(id: u64) -> String {
        claude::id_voce_permesso(id)
    }

    fn ha_voce(p: &Ponte, id: &str) -> bool {
        p.attese.borrow().contiene(id)
    }

    fn ids_voci(p: &Ponte) -> Vec<String> {
        voci_claude(p).into_iter().map(|v| v.0).collect()
    }

    #[test]
    fn consenti_va_solo_a_chi_ha_chiesto() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx1 = permesso(&p, 1, "a");
        let rx2 = permesso(&p, 2, "b");
        assert!(ha_voce(&p, &voce_di(1)) && ha_voce(&p, &voce_di(2)));
        consenti(&p, 1);
        assert_eq!(rx1.try_recv(), Ok(Decisione::Consenti));
        assert_eq!(rx2.try_recv(), Err(TryRecvError::Empty), "l'altra sessione non riceve nulla");
        assert!(!ha_voce(&p, &voce_di(1)), "la voce decisa sparisce");
        assert!(ha_voce(&p, &voce_di(2)));
        // Nega sul secondo: solo il secondo.
        p.azione(&voce_di(2), PULSANTE_NEGA);
        assert_eq!(rx2.try_recv(), Ok(Decisione::Nega));
        assert_eq!(rx1.try_recv(), Err(TryRecvError::Disconnected), "il primo non riceve altro");
        assert!(p.claude_permessi.borrow().is_empty());
    }

    #[test]
    fn clic_tardivo_o_estraneo_non_fa_nulla() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx = permesso(&p, 1, "a");
        // Un pulsante che non decide, un id che non c'è, una voce che non è un permesso.
        p.azione(&voce_di(1), "altro");
        p.azione(&voce_di(99), PULSANTE_CONSENTI);
        p.azione("claude.permesso.x", PULSANTE_CONSENTI);
        p.azione("claude.0123.aspetta", PULSANTE_CONSENTI);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        assert!(ha_voce(&p, &voce_di(1)));
        // Decisa una volta, un secondo clic (voce già tolta, mittente caduto) è un no-op.
        p.azione(&voce_di(1), PULSANTE_NEGA);
        assert_eq!(rx.try_recv(), Ok(Decisione::Nega));
        drop(rx); // ora il ricevitore non c'è più: l'invio fallirebbe
        consenti(&p, 1);
        // Ricevitore caduto con la voce ancora lì (l'ascoltatore ha chiuso): l'invio che fallisce si ignora.
        let rx = permesso(&p, 2, "a");
        drop(rx);
        consenti(&p, 2);
        assert!(!ha_voce(&p, &voce_di(2)));
    }

    #[test]
    fn nessun_consenso_senza_gesto() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx = permesso(&p, 1, "a");
        p.claude_permesso_finito(1); // abbandonato(id)
        assert!(!ha_voce(&p, &voce_di(1)), "voce tolta");
        match rx.try_recv() {
            Ok(d) => panic!("nessun gesto, ma è arrivato {d:?}"),
            Err(e) => assert_eq!(e, TryRecvError::Disconnected),
        }
        // Un permesso aperto che vede tanti altri eventi non decide mai da solo.
        let rx = permesso(&p, 2, "a");
        manda(&p, ev("a", TipoEvento::Notification));
        manda(&p, EventoHook { notifica: Some("permission_prompt".into()), ..ev("a", TipoEvento::Notification) });
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn permesso_sparisce_se_la_sessione_va_avanti() {
        for chiude in [
            TipoEvento::Stop,
            TipoEvento::UserPromptSubmit,
            TipoEvento::PreToolUse,
            TipoEvento::StopFailure,
            TipoEvento::SessionEnd,
        ] {
            let (_s, p, _pillola, _d) = ponte_di_prova();
            let rx = permesso(&p, 1, "a");
            let altro = permesso(&p, 2, "b");
            // Un evento di un'altra sessione non tocca il permesso.
            manda(&p, ev("b", chiude));
            assert!(ha_voce(&p, &voce_di(1)), "{chiude:?} di un'altra sessione");
            assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
            assert_eq!(altro.try_recv(), Ok(Decisione::Nessuna), "{chiude:?} chiude il suo");
            assert!(!ha_voce(&p, &voce_di(2)));
            // Un evento della stessa sessione: Nessuna e voce tolta.
            manda(&p, ev("a", chiude));
            assert_eq!(rx.try_recv(), Ok(Decisione::Nessuna), "{chiude:?}");
            assert!(!ha_voce(&p, &voce_di(1)), "{chiude:?}");
            assert!(p.claude_permessi.borrow().is_empty());
        }
    }

    #[test]
    fn la_notifica_non_chiude_il_permesso() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx = permesso(&p, 1, "a");
        // Claude manda una `permission_prompt` ~6 s dopo, col permesso ancora aperto.
        manda(&p, EventoHook { notifica: Some("permission_prompt".into()), ..ev("a", TipoEvento::Notification) });
        assert!(ha_voce(&p, &voce_di(1)), "la voce resta");
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        assert!(p.claude_permessi.borrow().contains_key(&1));
    }

    #[test]
    fn con_un_permesso_la_sessione_non_dice_ti_aspetta() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let _rx = permesso(&p, 1, "a");
        assert_eq!(ids_voci(&p), [voce_di(1)], "solo la voce del permesso");
        // Chiuso dalla decisione, la sessione lavora: nessuna voce (vedi `dopo_la_decisione_la_sessione_lavora`).
        p.azione(&voce_di(1), PULSANTE_NEGA);
        assert!(voci_claude(&p).is_empty());
        // Chiuso da un evento che porta a «fatto»: la voce del permesso va, resta il «fatto».
        let _rx = permesso(&p, 2, "a");
        assert_eq!(ids_voci(&p), [voce_di(2)]);
        manda(&p, ev("a", TipoEvento::Stop));
        assert_eq!(voci_claude(&p).iter().map(|v| v.2.as_str()).collect::<Vec<_>>(), ["Claude ha finito · progetto-a"]);
        // Abbandonato: torna «ti aspetta».
        let _rx = permesso(&p, 3, "b");
        p.claude_permesso_finito(3);
        assert!(voci_claude(&p).iter().any(|v| v.2 == "Claude ti aspetta · progetto-a" && v.0.ends_with(".aspetta")));
        assert!(!ha_voce(&p, &voce_di(3)));
    }

    #[test]
    fn permesso_di_sessione_chiusa() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        manda(&p, ev("a", TipoEvento::SessionEnd));
        assert!(p.claude_sessioni.borrow().elenco().is_empty());
        let rx = permesso(&p, 1, "a");
        let s = p.claude_sessioni.borrow();
        assert_eq!(s.elenco().len(), 1, "la sessione rinasce");
        assert_eq!(s.elenco()[0].stato, claude_hook::sessioni::Stato::TiAspetta);
        drop(s);
        assert!(ha_voce(&p, &voce_di(1)), "voce presente");
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        consenti(&p, 1);
        assert_eq!(rx.try_recv(), Ok(Decisione::Consenti));
    }

    #[test]
    fn al_massimo_sedici_permessi_in_attesa() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ricevitori: Vec<_> = (1..=16).map(|i| permesso(&p, i, &format!("s{i}"))).collect();
        assert_eq!(p.claude_permessi.borrow().len(), 16);
        // Il diciassettesimo: nessun canale (cade subito), nessuna voce di permesso, resta «ti aspetta».
        let rx = permesso(&p, 17, "s17");
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        assert!(!ha_voce(&p, &voce_di(17)));
        assert_eq!(p.claude_permessi.borrow().len(), 16);
        assert!(voci_claude(&p).iter().any(|v| v.0.ends_with(".aspetta") && v.2 == "Claude ti aspetta · progetto-a"));
        // Liberato un posto, il successivo entra.
        p.azione(&voce_di(1), PULSANTE_NEGA);
        let _rx = permesso(&p, 18, "s18");
        assert!(ha_voce(&p, &voce_di(18)));
        assert!(ricevitori[1].try_recv().is_err());
    }

    #[test]
    fn dopo_la_decisione_la_sessione_lavora() {
        use claude_hook::sessioni::Stato;
        let (_s, p, _pillola, _d) = ponte_di_prova();
        for (pulsante, atteso) in [(PULSANTE_CONSENTI, Decisione::Consenti), (PULSANTE_NEGA, Decisione::Nega)] {
            let rx = permesso(&p, 1, "a");
            armato(&p);
            p.azione(&voce_di(1), pulsante);
            assert_eq!(rx.try_recv(), Ok(atteso));
            let s = p.claude_sessioni.borrow();
            let x = s.elenco()[0];
            assert_eq!((x.stato, x.strumento.as_deref()), (Stato::Lavora, Some("PowerShell")));
            assert!(x.lavora_da.is_some());
            drop(s);
            assert!(voci_claude(&p).is_empty(), "niente «ti aspetta» dopo la risposta");
        }
        // Abbandonato o chiuso senza decisione: il terminale aspetta ancora, resta «ti aspetta».
        let _rx = permesso(&p, 2, "b");
        p.claude_permesso_finito(2);
        assert_eq!(p.claude_sessioni.borrow().elenco().iter().find(|x| x.id == "b").unwrap().stato, Stato::TiAspetta);
        assert!(voci_claude(&p).iter().any(|v| v.0.ends_with(".aspetta")));
    }

    #[test]
    fn consenti_subito_dopo_un_cambio_si_ignora() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let t = Instant::now();
        let rx = permesso(&p, 1, "a");
        let dal = p.claude_armamento.borrow().dal.get();
        assert!(dal > t, "la voce nuova arma il ritardo");
        // Dentro i 600 ms: ignorato, la voce e il canale restano; Nega invece passa sempre.
        p.claude_decidi(1, Decisione::Consenti, dal - Duration::from_millis(1));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        assert!(ha_voce(&p, &voce_di(1)));
        // Dopo: accettato.
        p.claude_decidi(1, Decisione::Consenti, dal);
        assert_eq!(rx.try_recv(), Ok(Decisione::Consenti));
        // Nega non aspetta nessun ritardo.
        let rx2 = permesso(&p, 2, "b");
        p.claude_decidi(2, Decisione::Nega, Instant::now());
        assert_eq!(rx2.try_recv(), Ok(Decisione::Nega));
        // La voce mostrata cambia (la prima risolta, appare la seconda): il ritardo riparte.
        let _a = permesso(&p, 3, "c");
        let _b = permesso(&p, 4, "d");
        armato(&p);
        let prima = p.claude_armamento.borrow().dal.get();
        p.claude_decidi(3, Decisione::Nega, Instant::now());
        assert!(p.claude_armamento.borrow().dal.get() > prima, "ora la pillola mostra l'altra: ritardo di nuovo");
        assert_eq!(p.claude_permessi.borrow().len(), 1);
    }

    #[test]
    fn clic_della_pillola_su_voce_cambiata_non_vale() {
        let (_s, p, pillola, _d) = ponte_di_prova();
        let rx1 = permesso(&p, 1, "a");
        let rx2 = permesso(&p, 2, "b");
        armato(&p);
        // La pillola mostra la 1: un clic reso per la 2 (o per altro) non decide nulla.
        assert_eq!(pillola.get_voce_id(), voce_di(1));
        p.azione_pillola(&voce_di(2), PULSANTE_NEGA);
        p.azione_pillola("altro", PULSANTE_NEGA);
        assert_eq!((rx1.try_recv(), rx2.try_recv()), (Err(TryRecvError::Empty), Err(TryRecvError::Empty)));
        // Un clic reso per la voce mostrata vale; dopo, la pillola mostra la 2 e un clic vecchio
        // (reso per la 1) non la tocca.
        p.azione_pillola(&voce_di(1), PULSANTE_NEGA);
        assert_eq!(rx1.try_recv(), Ok(Decisione::Nega));
        assert_eq!(pillola.get_voce_id(), voce_di(2));
        armato(&p);
        p.azione_pillola(&voce_di(1), PULSANTE_CONSENTI);
        assert_eq!(rx2.try_recv(), Err(TryRecvError::Empty));
    }

    #[cfg(windows)]
    #[test]
    fn riavviando_il_tubo_i_permessi_vecchi_si_chiudono() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.casa.aggiorna_config(|c| c.claude.collegato = true);
        let rx = permesso(&p, 1, "a");
        assert!(ha_voce(&p, &voce_di(1)));
        avvia_con(&p, config_di_prova());
        assert_eq!(rx.try_recv(), Ok(Decisione::Nessuna), "il vecchio permesso è chiuso");
        assert!(!ha_voce(&p, &voce_di(1)) && p.claude_permessi.borrow().is_empty());
        // Un id già in elenco non si sovrascrive: il nuovo arrivo ripiega sul terminale.
        let _vivo = permesso(&p, 5, "b");
        let doppio = permesso(&p, 5, "c");
        assert_eq!(doppio.try_recv(), Err(TryRecvError::Disconnected));
        assert_eq!(p.claude_permessi.borrow().get(&5).map(|x| x.sessione.as_str()), Some("b"));
        // I numeri nuovi non tornano indietro.
        let a = PROSSIMO_PERMESSO.fetch_add(1, Ordering::Relaxed);
        let b = PROSSIMO_PERMESSO.fetch_add(1, Ordering::Relaxed);
        assert!(b > a);
        spegni();
    }

    #[test]
    fn il_pannello_o_la_pillola_che_compaiono_armano_di_nuovo() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx = permesso(&p, 1, "a");
        armato(&p);
        // Il pannello si apre con un permesso in attesa: Consenti subito è ignorato, dopo vale.
        let pannello = crate::Pannello::new().unwrap();
        p.pannello_aperto(&pannello);
        let dal = p.claude_armamento.borrow().dal.get();
        assert!(dal > Instant::now() - Duration::from_millis(1) && dal <= Instant::now() + ARMAMENTO_CONSENTI);
        p.claude_decidi(1, Decisione::Consenti, dal - Duration::from_millis(1));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        // Il pannello si chiude e la pillola ricompare: di nuovo il ritardo.
        armato(&p);
        p.pannello_chiuso();
        let dal = p.claude_armamento.borrow().dal.get();
        assert!(dal > Instant::now(), "il ritardo è ripartito");
        p.claude_decidi(1, Decisione::Consenti, dal - Duration::from_millis(1));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        p.claude_decidi(1, Decisione::Consenti, dal);
        assert_eq!(rx.try_recv(), Ok(Decisione::Consenti));
        // Senza permessi in attesa non si arma nulla.
        armato(&p);
        let prima = p.claude_armamento.borrow().dal.get();
        p.ridisegna_pillola();
        assert_eq!(p.claude_armamento.borrow().dal.get(), prima);
    }

    #[test]
    fn invio_non_riuscito_non_e_una_decisione() {
        use claude_hook::sessioni::Stato;
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx = permesso(&p, 1, "a");
        drop(rx); // l'ascoltatore ha rinunciato
        consenti(&p, 1);
        assert!(!ha_voce(&p, &voce_di(1)), "la voce se ne va");
        assert_eq!(p.claude_sessioni.borrow().elenco()[0].stato, Stato::TiAspetta, "il terminale aspetta ancora");
        assert!(voci_claude(&p).iter().any(|v| v.0.ends_with(".aspetta")));
    }

    #[test]
    fn la_chiusura_vale_solo_per_i_permessi_arrivati_prima() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        // La PreToolUse è partita prima della richiesta (sequenza più bassa) ma l'interfaccia
        // la vede dopo: il permesso resta.
        let prima = SEQUENZA.fetch_add(2, Ordering::Relaxed);
        let (tx, rx) = channel();
        p.claude_permesso(1, chiede("a", "ls"), None, Some(tx), prima + 1, Instant::now());
        p.claude_evento(ev("a", TipoEvento::PreToolUse), None, prima, Instant::now());
        assert!(ha_voce(&p, &voce_di(1)), "una PreToolUse più vecchia non chiude");
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        // Una più nuova sì.
        p.claude_evento(ev("a", TipoEvento::PreToolUse), None, prima + 2, Instant::now());
        assert_eq!(rx.try_recv(), Ok(Decisione::Nessuna));
        assert!(!ha_voce(&p, &voce_di(1)));
    }

    #[test]
    fn la_pillola_chiama_per_il_piu_vecchio_e_dice_piu_n() {
        let (_s, p, pillola, _d) = ponte_di_prova();
        let _a = permesso(&p, 1, "a");
        assert_eq!(pillola.get_altri(), "");
        let _b = permesso(&p, 2, "b");
        let _c = permesso(&p, 3, "c");
        assert_eq!(pillola.get_altri(), " +2");
        assert_eq!(pillola.get_messaggio(), "Claude chiede: Comando ls -la");
        p.azione(&voce_di(1), PULSANTE_NEGA);
        assert_eq!(pillola.get_altri(), " +1");
        p.azione(&voce_di(2), PULSANTE_NEGA);
        assert_eq!(pillola.get_altri(), "");
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

    fn barra(sessione: &str, cinque: Option<(f32, i64)>, sette: Option<(f32, i64)>) -> DatiBarra {
        let l = |x: Option<(f32, i64)>| x.map(|(percentuale, azzera_alle)| Limite { percentuale, azzera_alle });
        DatiBarra { sessione: sessione.into(), modello: Some("Opus 5.5".into()), contesto: Some(40.0), cinque_ore: l(cinque), sette_giorni: l(sette) }
    }

    const FUTURO: i64 = 4_000_000_000;

    #[test]
    fn limiti_dall_ultimo_arrivo_con_limiti() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        assert_eq!(p.limiti(), (None, None), "senza barre niente anelli");
        p.claude_barra(barra("a", Some((10.0, FUTURO)), Some((20.0, FUTURO))));
        p.claude_barra(barra("b", Some((30.0, FUTURO + 1)), Some((40.0, FUTURO + 2))));
        let (c, s) = p.limiti();
        assert_eq!((c.unwrap().percentuale, c.unwrap().azzera_alle), (30.0, FUTURO + 1));
        assert_eq!((s.unwrap().percentuale, s.unwrap().azzera_alle), (40.0, FUTURO + 2));
        // Una barra senza rate_limits non cancella ciò che si sa.
        p.claude_barra(barra("a", None, None));
        assert_eq!(p.limiti(), (c, s));
        // Una finestra alla volta: solo quella presente cambia.
        p.claude_barra(barra("a", None, Some((50.0, FUTURO + 3))));
        let (c2, s2) = p.limiti();
        assert_eq!(c2, c);
        assert_eq!(s2.unwrap().percentuale, 50.0);
    }

    #[test]
    fn mai_barre_con_limiti_niente_anelli_e_scaduti_assenti() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.claude_barra(barra("a", None, None));
        assert_eq!(p.limiti(), (None, None));
        p.claude_barra(barra("a", Some((10.0, 1000)), Some((20.0, 2000))));
        assert_eq!(p.limiti_a(999).0.map(|l| l.percentuale), Some(10.0));
        assert_eq!(p.limiti_a(1000).0, None, "azzera_alle <= ora vale come assente");
        assert_eq!(p.limiti_a(1000).1.map(|l| l.percentuale), Some(20.0));
        assert_eq!(p.limiti_a(2000), (None, None));
        assert_eq!(p.limiti(), (None, None), "con l'ora vera sono già scaduti");
    }

    #[test]
    fn contesto_per_sessione_con_tetto_e_pulizia() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        p.claude_barra(barra("a", None, None));
        assert_eq!(p.claude_contesto.borrow().get("a"), Some(&40.0));
        // Sparisce con la sessione (SessionEnd) e con la scadenza.
        manda(&p, ev("a", TipoEvento::SessionEnd));
        assert!(p.claude_contesto.borrow().is_empty());
        manda(&p, ev("b", TipoEvento::UserPromptSubmit));
        p.claude_barra(barra("b", None, None));
        p.claude_scadute(Instant::now() + Duration::from_secs(31 * 60));
        assert!(p.claude_contesto.borrow().is_empty());
        // Al più 16 voci, anche con sessioni mai viste dagli eventi.
        for i in 0..40 {
            p.claude_barra(barra(&format!("x{i}"), None, None));
        }
        assert!(p.claude_contesto.borrow().len() <= 16);
    }

    // ─── Vista Claude (Task 9) ───────────────────────────────────────────────

    /// Un pannello aperto sul ponte, come lo apre `pannello.rs`: modelli e callback collegati.
    fn con_pannello(p: &Ponte) -> crate::Pannello {
        let pannello = crate::Pannello::new().unwrap();
        p.pannello_aperto(&pannello);
        p.riempi(&pannello);
        pannello
    }

    use slint::Model;

    #[test]
    fn il_timer_della_vista_claude_serve_solo_con_chi_lavora() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        // La vista aperta senza nessuno al lavoro non accende nulla.
        p.vista_claude(true);
        assert!(!p.timer_attivita.running(), "vista aperta ma nessuno lavora");
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert!(p.timer_attivita.running());
        // A pannello aperto la pillola è nascosta: la vista deve comunque scorrere.
        p.pillola_visibile(false);
        assert!(p.timer_attivita.running(), "vista aperta: il timer gira anche con la pillola nascosta");
        assert!(p.attivita_serve());
        // Vista chiusa e pillola nascosta: niente.
        p.vista_claude(false);
        assert!(!p.timer_attivita.running() && !p.attivita_serve());
        p.vista_claude(true);
        assert!(p.timer_attivita.running());
        // Chi smette di lavorare ferma il timer anche a vista aperta.
        manda(&p, ev("a", TipoEvento::Stop));
        assert!(!p.timer_attivita.running());
        // E la pillola visibile resta una ragione sufficiente, come prima.
        p.vista_claude(false);
        p.pillola_visibile(true);
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert!(p.timer_attivita.running());
    }

    #[test]
    fn la_vista_claude_si_scrive_solo_aperta() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        // Vista chiusa: gli eventi non scrivono nulla nel pannello.
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert_eq!(pannello.get_claude_righe().row_count(), 0, "vista chiusa: nessun lavoro");
        // Si apre: le righe ci sono, di colpo.
        p.vista_claude(true);
        let righe = pannello.get_claude_righe();
        assert_eq!(righe.row_count(), 1);
        let r = righe.row_data(0).unwrap();
        assert_eq!((r.cartella.as_str(), r.stato.as_str(), r.durata.as_str()), ("progetto-a", "Lavora", "0:00"));
        assert_eq!(r.id.as_str(), claude::id_sessione("a"));
        assert_eq!(pannello.get_claude_altre(), 0);
        // Un'altra sessione, poi chi chiede qualcosa: l'ordine è quello dell'urgenza.
        manda(&p, ev("b", TipoEvento::PermissionRequest));
        let righe = pannello.get_claude_righe();
        assert_eq!(righe.row_count(), 2);
        assert_eq!(righe.row_data(0).unwrap().stato.as_str(), "Ti aspetta");
        assert_eq!(righe.row_data(0).unwrap().livello, LivelloUi::Attenzione);
        // Cinque sessioni: tre righe e «+2».
        for id in ["c", "d", "e"] {
            manda(&p, ev(id, TipoEvento::UserPromptSubmit));
        }
        assert_eq!((pannello.get_claude_righe().row_count(), pannello.get_claude_altre()), (3, 2));
        // Solo chi lavora ha una durata (le altre righe non ne hanno).
        let prima: Vec<String> = (0..3).map(|i| pannello.get_claude_righe().row_data(i).unwrap().durata.to_string()).collect();
        assert_eq!(prima.iter().filter(|d| !d.is_empty()).count(), 2);
        // Chiusa di nuovo: gli eventi non scrivono più.
        p.vista_claude(false);
        manda(&p, ev("e", TipoEvento::SessionEnd));
        assert_eq!(pannello.get_claude_righe().row_count(), 3);
        // Riaperta: la lista è quella vera.
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_altre(), 1);
        // Il pannello chiuso del tutto: la vista si chiude da sé.
        p.pannello_chiuso();
        assert!(!p.vista_claude.get());
    }

    #[test]
    fn il_collegamento_si_dice_alla_vista() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.vista_claude(true);
        assert!(!pannello.get_claude_collegato(), "di serie non è collegato");
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        p.vista_claude(false);
        p.vista_claude(true);
        assert!(pannello.get_claude_collegato());
    }

    #[test]
    fn anelli_dai_limiti_senza_zeri_inventati() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.vista_claude(true);
        // PF9: nessuna barra, nessun anello.
        assert!(!pannello.get_claude_cinque_ore().presente && !pannello.get_claude_sette_giorni().presente);
        // Una barra con la sola finestra delle 5 ore: solo quell'anello.
        p.claude_barra(barra("a", Some((36.6, FUTURO)), None));
        let c = pannello.get_claude_cinque_ore();
        assert!(c.presente && !pannello.get_claude_sette_giorni().presente);
        assert_eq!(c.testo.as_str(), "37%");
        assert!((c.frazione - 0.366).abs() < 1e-5);
        // Le due finestre.
        p.claude_barra(barra("a", None, Some((61.0, FUTURO + 5))));
        assert!(pannello.get_claude_cinque_ore().presente);
        assert_eq!(pannello.get_claude_sette_giorni().testo.as_str(), "61%");
        // Un limite già azzerato è assente, mai «0%».
        p.claude_barra(barra("a", Some((10.0, 1000)), Some((20.0, 2000))));
        assert!(!pannello.get_claude_cinque_ore().presente && !pannello.get_claude_sette_giorni().presente);
    }

    #[test]
    fn sveglia_singola_all_azzeramento_solo_a_vista_aperta() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let _pannello = con_pannello(&p);
        let ora = chrono::Local::now().timestamp();
        p.vista_claude(true);
        assert!(!p.sveglia_limiti.running(), "senza limiti nessuna sveglia");
        // Due limiti: la sveglia è per il primo ad azzerarsi.
        p.claude_barra(barra("a", Some((10.0, ora + 600)), Some((20.0, ora + 90_000))));
        assert!(p.sveglia_limiti.running());
        assert_eq!(p.limiti_armati.get(), Some(ora + 600));
        // Cambia il primo: si riarma per il nuovo istante.
        p.claude_barra(barra("a", Some((10.0, ora + 300)), None));
        assert_eq!(p.limiti_armati.get(), Some(ora + 300));
        // Vista chiusa: niente sveglia.
        p.vista_claude(false);
        assert!(!p.sveglia_limiti.running());
        assert_eq!(p.limiti_armati.get(), None);
        p.claude_barra(barra("a", Some((10.0, ora + 300)), None));
        assert!(!p.sveglia_limiti.running(), "a vista chiusa non si arma");
        // Riaperta: si arma di nuovo; un limite che scade toglie l'anello e la sveglia.
        p.vista_claude(true);
        assert!(p.sveglia_limiti.running());
        p.claude_barra(barra("a", Some((10.0, 1000)), Some((20.0, 2000))));
        assert!(!p.sveglia_limiti.running(), "senza limiti validi la sveglia si ferma");
    }

    #[test]
    fn percentuale_non_finita_e_assenza_non_zero() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.vista_claude(true);
        p.claude_barra(barra("a", Some((f32::NAN, FUTURO)), Some((f32::INFINITY, FUTURO))));
        assert!(!pannello.get_claude_cinque_ore().presente && !pannello.get_claude_sette_giorni().presente);
        assert_eq!(p.limiti(), (None, None));
    }

    #[test]
    fn spesa_nella_vista_con_e_senza_chiave() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_spesa_nota().as_str(), "Aggiungi una chiave da amministratore nelle Impostazioni");
        assert_eq!(pannello.get_claude_spesa().as_str(), "");
        // Con la chiave ma senza lettura: nessun dato.
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        p.vista_claude(false);
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_spesa_nota().as_str(), "nessun dato");
        // Arriva una lettura a vista aperta: la vista si aggiorna da sé.
        let r = api_claude::Riepilogo {
            spesa_mese: api_claude::importo_da("41280"),
            aggiornato_a: Some(chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 18, 14, 2, 0).unwrap()),
            ..Default::default()
        };
        p.api_arrivato(p.api_generazione.get(), "14:05".into(), Ok(r));
        assert_eq!(pannello.get_claude_spesa().as_str(), "412,80 $");
        let locale = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 18, 14, 2, 0).unwrap().with_timezone(&chrono::Local);
        assert_eq!(pannello.get_claude_spesa_nota().as_str(), format!("aggiornato alle {}", locale.format("%H:%M")));
        // La chiave tolta: torna la frase.
        p.api_togli_ui();
        p.vista_claude(false);
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_spesa().as_str(), "");
        assert!(pannello.get_claude_spesa_nota().as_str().starts_with("Aggiungi una chiave"));
    }
}
