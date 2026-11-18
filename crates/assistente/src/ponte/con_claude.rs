//! Claude Code nel ponte (M3, Task 4 e 7A): il tubo protetto con l'aiutante `isola-hook`, le voci
//! delle sessioni, il colore degli occhi e la pillola media «attività dal vivo».
//!
//! Nessun timer a riposo: una sveglia singola alla prossima scadenza di una sessione, e il timer
//! da un secondo del testo di attività solo mentre serve (vedi `attivita_serve`).

mod api;
mod collegamento;

pub use collegamento::Contesto;
use super::{ponte, Ponte, MARGINE_SVEGLIA};
use crate::attenzione::{Fonte, Livello, Pulsante, Voce, PULSANTE_IMPOSTAZIONI};
use crate::claude::{self, ID_AGGIORNA, ID_COLLEGAMENTO, PERMESSI_RISOLVIBILI, PULSANTE_CONSENTI, PULSANTE_NEGA};
use crate::pillola;
use crate::umore::Umore;
use crate::LivelloUi;
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
    }
}

/// Un pulsante di una voce di Claude Code (i task 10 e 12 ci aggiungono i loro).
pub fn azione(p: &Ponte, voce: &str, pulsante: &str) {
    if voce == ID_COLLEGAMENTO && pulsante == PULSANTE_IMPOSTAZIONI {
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
            // «Apri il terminale» (ramo B) arriva col Task 10.
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
fn consegna(e: EventoHook, pid: Option<u32>, sequenza: u64) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.claude_evento(e, pid, sequenza, Instant::now());
        }
    });
}

/// Porta un permesso al thread dell'interfaccia.
fn consegna_permesso(
    id: u64,
    e: EventoHook,
    pid: Option<u32>,
    tx: Option<Sender<Decisione>>,
    sequenza: u64,
) -> Result<(), slint::EventLoopError> {
    slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.claude_permesso(id, e, pid, tx, sequenza, Instant::now());
        }
    })
}

impl Gestore for GestoreIsola {
    fn evento(&self, a: Arrivo) {
        if let Contenuto::Evento(e) = a.contenuto {
            consegna(e, a.pid_mittente, a.sequenza);
        }
    }

    /// Un permesso: l'interfaccia ne fa una voce bloccante e questo thread aspetta la decisione
    /// sul canale. Ciò che non è una richiesta di permesso non apre nessuna attesa (Claude Code
    /// prosegue col suo flusso); col ramo B la voce c'è ma la risposta no.
    fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso> {
        let Contenuto::Evento(e) = a.contenuto else { return None };
        let pid = a.pid_mittente;
        let sequenza = a.sequenza;
        if e.tipo != TipoEvento::PermissionRequest {
            consegna(e, pid, sequenza);
            return None;
        }
        let id = PROSSIMO_PERMESSO.fetch_add(1, Ordering::Relaxed);
        if !PERMESSI_RISOLVIBILI {
            let _ = consegna_permesso(id, e, pid, None, sequenza);
            return None;
        }
        let (tx, rx) = channel();
        // Senza ciclo eventi nessuno può rispondere: il canale cade e Claude ripiega.
        consegna_permesso(id, e, pid, Some(tx), sequenza).ok()?;
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

    /// I limiti dell'abbonamento (5 ore, 7 giorni) a un'ora data (secondi dall'epoca): un limite
    /// già azzerato (`azzera_alle <= ora`) vale come assente. Senza abbonamento niente anelli.
    #[allow(dead_code)] // lo leggono le viste dei task successivi
    pub fn limiti_a(&self, ora: i64) -> (Option<Limite>, Option<Limite>) {
        let (c, s) = self.claude_limiti.borrow().unwrap_or((None, None));
        (c.filter(|l| l.azzera_alle > ora), s.filter(|l| l.azzera_alle > ora))
    }

    /// Come `limiti_a`, all'ora di adesso.
    #[allow(dead_code)] // lo leggono le viste dei task successivi
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
}
