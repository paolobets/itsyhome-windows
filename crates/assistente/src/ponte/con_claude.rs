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
    self, ID_AGGIORNA, ID_COLLEGAMENTO, ID_DECISIONE_TARDIVA, ID_TERMINALE_MANCANTE, PERMESSI_RISOLVIBILI, PULSANTE_CONSENTI, PULSANTE_NEGA, PULSANTE_TERMINALE,
};
use crate::pillola;
use crate::umore::Umore;
use crate::{FinestraImpostazioni, LimiteClaudeUi, LivelloUi, RigaSessioneUi};
use casa_core::config::LimiteVisto;
use casa_core::diagnosi::{Contatore, Diagnosi};
use chrono::TimeZone;
use slint::Model;
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

/// Quanti permessi possono aspettare insieme (quante connessioni serve il tubo). Uno in
/// più non ha voce né canale: Claude Code ripiega sul terminale.
const MASSIMO_PERMESSI: usize = 16;
/// Quanto il tubo aspetta una decisione: due secondi meno dell'aiutante (`ATTESA_MASSIMA`), così
/// quando il tubo risponde o rinuncia l'aiutante c'è ancora e un clic a ridosso del limite non
/// finisce nel vuoto.
const ATTESA_TUBO: Duration = Duration::from_secs(ATTESA_MASSIMA.as_secs() - 2);
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
    /// I permessi (voci con un pulsante delicato) col loro posto nell'elenco ordinato: la Coda li
    /// mostra tutti, e uno che cambia posto si arma di nuovo.
    permessi: Vec<(usize, String)>,
    dal: Cell<Instant>,
}

impl Armamento {
    pub(super) fn nuovo() -> Armamento {
        Armamento { pillola: None, pannello: None, permessi: Vec::new(), dal: Cell::new(Instant::now()) }
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
        attesa_massima: ATTESA_TUBO,
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
            #[cfg(windows)]
            crate::sistema::primo_piano::ricorda_terminali(true);
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
            permesso: None,
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
    } else if voce == ID_DECISIONE_TARDIVA {
        p.togli_voce(voce);
    } else if voce == ID_AGGIORNA && pulsante == crate::attenzione::PULSANTE_HO_CAPITO {
        // Non si ripropone a ogni avvio (fino al prossimo collega/scollega).
        p.casa.aggiorna_config(|c| c.claude.aggiornamento_ignorato = true);
        p.togli_voce(voce);
    } else if voce == ID_AGGIORNA && pulsante == PULSANTE_IMPOSTAZIONI {
        // Si apre direttamente la sezione Claude, dove il collegamento si aggiorna (con anteprima e conferma).
        crate::finestra_impostazioni::apri_alla_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE, || {
            p.chiedi_impostazioni()
        });
    } else if claude::e_voce_di_avviso(voce) {
        // «Ho capito» (e ogni altro pulsante): l'avviso è già segnato come dato e non torna.
        p.togli_voce(voce);
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
        #[cfg(windows)]
        crate::sistema::primo_piano::ricorda_terminali(false);
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
fn consegna(e: EventoHook, catena: Vec<(u32, String)>, sequenza: u64) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            let sessione = e.sessione.clone();
            p.claude_evento(e, sequenza, Instant::now());
            p.claude_imposta_catena(&sessione, catena);
        }
    });
}

/// Porta un permesso al thread dell'interfaccia.
fn consegna_permesso(
    id: u64,
    e: EventoHook,
    catena: Vec<(u32, String)>,
    tx: Option<Sender<Decisione>>,
    sequenza: u64,
) -> Result<(), slint::EventLoopError> {
    slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            let sessione = e.sessione.clone();
            p.claude_permesso(id, e, tx, sequenza, Instant::now());
            p.claude_imposta_catena(&sessione, catena);
        }
    })
}

impl Gestore for GestoreIsola {
    fn evento(&self, a: Arrivo) {
        if let Contenuto::Evento(e) = a.contenuto {
            consegna(e, a.catena_mittente, a.sequenza);
        }
    }

    /// Un permesso: l'interfaccia ne fa una voce bloccante e questo thread aspetta la decisione
    /// sul canale. Ciò che non è una richiesta di permesso non apre nessuna attesa (Claude Code
    /// prosegue col suo flusso); col ramo B la voce c'è ma la risposta no.
    fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso> {
        let Contenuto::Evento(e) = a.contenuto else { return None };
        let catena = a.catena_mittente;
        let sequenza = a.sequenza;
        if e.tipo != TipoEvento::PermissionRequest {
            consegna(e, catena, sequenza);
            return None;
        }
        self.diagnosi.conta(Contatore::claude_permessi);
        let id = PROSSIMO_PERMESSO.fetch_add(1, Ordering::Relaxed);
        if !PERMESSI_RISOLVIBILI {
            let _ = consegna_permesso(id, e, catena, None, sequenza);
            return None;
        }
        let (tx, rx) = channel();
        // Senza ciclo eventi nessuno può rispondere: il canale cade e Claude ripiega.
        consegna_permesso(id, e, catena, Some(tx), sequenza).ok()?;
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

/// Adesso, in secondi dall'epoca (i limiti dell'abbonamento e il file di stato parlano così).
pub(super) fn adesso_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Voci di contesto tenute al più (una per sessione, come le sessioni).
const MASSIMO_CONTESTI: usize = 16;

impl Ponte {
    /// I dati della barra di stato, sul thread dell'interfaccia. I limiti sono dell'account: le
    /// finestre presenti sostituiscono quelle note, una per una; una barra senza `rate_limits` (o
    /// senza una finestra) non cancella ciò che si sa. Il contesto è per sessione.
    pub(super) fn claude_barra(&self, d: DatiBarra) {
        self.claude_barra_a(d, adesso_unix());
    }

    /// Come `claude_barra`, a un'ora data (secondi dall'epoca): i test ne passano una.
    pub(super) fn claude_barra_a(&self, d: DatiBarra, ora: i64) {
        self.casa.diagnosi().conta(Contatore::claude_barre);
        // Il titolo della sessione (`session_name`) arriva solo da qui, non dagli hook: se cambia
        // voci, vista e nome della pillola lo seguono; assente non cancella quello di prima.
        let cambi = self.claude_sessioni.borrow_mut().imposta_titolo(&d.sessione, d.titolo.as_deref());
        if !cambi.is_empty() {
            self.claude_cambiate(&cambi, Instant::now());
        }
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
        self.claude_ricorda_limiti(ora);
        // Limiti e contesto cambiati: avvisi e vista Claude (se aperta).
        self.claude_limiti_a(ora);
    }

    /// RC-56: i limiti appena arrivati nel file di stato (`ConfigClaude`), per mostrarli dopo un
    /// riavvio. Solo se il valore cambia e al più una volta al minuto: lo decide
    /// `claude::limite_da_salvare` nel momento in cui il dato arriva, senza timer.
    fn claude_ricorda_limiti(&self, ora: i64) {
        let (cinque, sette) = self.limiti_a(ora);
        let (salvato_c, salvato_s) = self.casa.con_config(|c| (c.claude.ultimo_cinque_ore, c.claude.ultimo_sette_giorni));
        let ultima = [salvato_c, salvato_s].into_iter().flatten().map(|v| v.visto_alle).max();
        let nuovo_c = cinque.and_then(|l| claude::limite_da_salvare(&l, salvato_c, ultima, ora));
        let nuovo_s = sette.and_then(|l| claude::limite_da_salvare(&l, salvato_s, ultima, ora));
        if nuovo_c.is_none() && nuovo_s.is_none() {
            return;
        }
        self.casa.aggiorna_config(|c| {
            c.claude.ultimo_cinque_ore = nuovo_c.or(c.claude.ultimo_cinque_ore);
            c.claude.ultimo_sette_giorni = nuovo_s.or(c.claude.ultimo_sette_giorni);
        });
    }

    /// I limiti dell'abbonamento sono cambiati (una barra, la soglia, la sveglia dell'azzeramento).
    pub(super) fn claude_limiti_cambiati(&self) {
        self.claude_limiti_a(adesso_unix());
    }

    /// Come `claude_limiti_cambiati`, a un'ora data (i test ne passano una). Una sveglia singola
    /// all'azzeramento del primo limite (anche a vista chiusa: toglie la voce d'avviso della
    /// finestra che si azzera); gli avvisi a soglia, una volta per finestra; poi la vista.
    pub(super) fn claude_limiti_a(&self, ora: i64) {
        let (cinque, sette) = self.limiti_a(ora);
        self.arma_sveglia_limiti(self.prossimo_azzeramento(ora), ora);
        let soglia = self.casa.con_config(|c| c.claude.soglia_limite).clamp(50, 100);
        for (finestra, limite) in [(claude::Finestra::CinqueOre, cinque), (claude::Finestra::SetteGiorni, sette)] {
            let Some(l) = limite else {
                // La finestra si è azzerata (o non c'è più il dato): l'avviso non ha più senso.
                self.togli_voce(finestra.id_voce());
                continue;
            };
            let avvisato = self.casa.con_config(|c| c.claude.limiti_avvisati.get(finestra.chiave()).copied());
            if let Some(v) = claude::avviso_limite(finestra, &l, soglia, avvisato, ora) {
                self.casa.aggiorna_config(|c| {
                    c.claude.limiti_avvisati.insert(finestra.chiave().to_owned(), l.azzera_alle);
                });
                self.metti_voce(v, true);
            }
        }
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.aggiorna_soglia_claude(&p);
        }
        self.aggiorna_vista_claude_a(ora);
        // La carta Agenti mostra il limite più alto.
        self.moduli_aggiorna("agenti");
    }

    /// Il primo azzeramento fra i limiti mostrati: quelli arrivati e, finché non ne arriva uno
    /// nuovo, quelli ricordati dal file di stato (RC-56). Per la sveglia singola dei limiti.
    fn prossimo_azzeramento(&self, ora: i64) -> Option<i64> {
        let (cinque, sette) = self.limiti_a(ora);
        let (ricordato_c, ricordato_s) = self.limiti_ricordati(ora);
        [cinque.map(|l| l.azzera_alle), sette.map(|l| l.azzera_alle), ricordato_c.map(|v| v.azzera_alle), ricordato_s.map(|v| v.azzera_alle)]
            .into_iter()
            .flatten()
            .min()
    }

    /// I limiti del file di stato che la vista mostra al posto di quelli che non sono (ancora)
    /// arrivati in questa esecuzione: una finestra per volta, solo se in corso (RC-56).
    fn limiti_ricordati(&self, ora: i64) -> (Option<LimiteVisto>, Option<LimiteVisto>) {
        let (arrivato_c, arrivato_s) = self.claude_limiti.borrow().map_or((false, false), |(c, s)| (c.is_some(), s.is_some()));
        let (salvato_c, salvato_s) = self.casa.con_config(|c| (c.claude.ultimo_cinque_ore, c.claude.ultimo_sette_giorni));
        let ancora_buono = |v: &LimiteVisto| v.azzera_alle > ora && v.percentuale.is_finite();
        (salvato_c.filter(|v| !arrivato_c && ancora_buono(v)), salvato_s.filter(|v| !arrivato_s && ancora_buono(v)))
    }

    /// La soglia d'avviso dell'utente (80 % di serie) al pannello: da lì l'arco dei limiti e della
    /// spesa diventa caldo, nella vista Claude (RC-44). Solo se cambia.
    pub(super) fn aggiorna_soglia_claude(&self, p: &crate::Pannello) {
        let soglia = claude::frazione_soglia(self.casa.con_config(|c| c.claude.soglia_limite));
        if p.get_claude_soglia() != soglia {
            p.set_claude_soglia(soglia);
        }
    }

    /// Impostazioni › Claude › Avvisi: i campi partono dalla configurazione (il limite all'80 % di
    /// serie, la soglia di spesa vuota = nessuna) e salvano al volo ciò che è valido.
    pub(crate) fn collega_soglie(&self, f: &FinestraImpostazioni) {
        let (limite, spesa) = self.casa.con_config(|c| (c.claude.soglia_limite.clamp(50, 100), c.claude.soglia_spesa));
        f.set_soglia_limite(limite.to_string().into());
        f.set_soglia_spesa(spesa.map(crate::lati::testo_massimo).unwrap_or_default().into());
        f.on_soglia_limite_scritta(|testo| ponte().map(|p| p.cambia_soglia_limite(&testo)).unwrap_or_default().into());
        f.on_soglia_spesa_scritta(|testo| ponte().map(|p| p.cambia_soglia_spesa(&testo)).unwrap_or_default().into());
    }

    /// «Avvisa al … % del limite»: valida e salva; restituisce l'errore da mostrare (vuoto =
    /// salvato). Un valore rifiutato non si salva.
    pub(crate) fn cambia_soglia_limite(&self, testo: &str) -> String {
        match claude::leggi_soglia_limite(testo) {
            Err(e) => e.to_owned(),
            Ok(v) => {
                if self.casa.con_config(|c| c.claude.soglia_limite) != v {
                    self.casa.aggiorna_config(|c| c.claude.soglia_limite = v);
                    // Abbassata sotto un limite già alto: l'avviso scatta subito.
                    self.claude_limiti_cambiati();
                }
                String::new()
            }
        }
    }

    /// «Soglia di spesa mensile ($)»: vuota = nessuna. Valida e salva; restituisce l'errore.
    pub(crate) fn cambia_soglia_spesa(&self, testo: &str) -> String {
        match claude::leggi_soglia_spesa(testo) {
            Err(e) => e.to_owned(),
            Ok(v) => {
                if self.casa.con_config(|c| c.claude.soglia_spesa) != v {
                    self.casa.aggiorna_config(|c| c.claude.soglia_spesa = v);
                    // Sui dati che ci sono (se freschi) e sul fondo scala dell'arco e del campo «Massimo».
                    self.claude_avviso_spesa(chrono::Utc::now());
                }
                String::new()
            }
        }
    }

    /// Un riepilogo nuovo: avviso di spesa a soglia e di spesa insolita. Solo con dati completi e
    /// aggiornati.
    pub(super) fn claude_riepilogo_cambiato(&self, adesso: chrono::DateTime<chrono::Utc>) {
        self.claude_avviso_spesa(adesso);
        self.claude_avviso_insolita(adesso);
    }

    /// Il riepilogo dell'ultima lettura se è riuscito, ha la spesa del mese e i suoi dati sono di
    /// questo mese (UTC): mai un avviso da dati assenti, falliti o fermi al mese scorso.
    /// Si presta in lettura (nessuna copia del `Riepilogo`): chi lo tiene non deve chiamare nulla
    /// che riscriva `api_ultimo`.
    fn riepilogo_fresco(&self, adesso: chrono::DateTime<chrono::Utc>) -> Option<std::cell::Ref<'_, api_claude::Riepilogo>> {
        let mese = adesso.format("%Y-%m").to_string();
        std::cell::Ref::filter_map(self.api_ultimo.borrow(), |u| match u {
            Some((_, Ok(r))) if r.errore.is_none() && r.aggiornato_a.is_some_and(|a| a.format("%Y-%m").to_string() == mese) => Some(r),
            _ => None,
        })
        .ok()
    }

    /// L'avviso di spesa a soglia (80 % e 100 %), una volta per gradino e per mese UTC.
    fn claude_avviso_spesa(&self, adesso: chrono::DateTime<chrono::Utc>) {
        let Some(soglia) = self.casa.con_config(|c| c.claude.soglia_spesa) else { return };
        let Some(spesa) = self.riepilogo_fresco(adesso).and_then(|r| r.spesa_mese) else { return };
        let mese = adesso.format("%Y-%m").to_string();
        let gia = self.casa.con_config(|c| c.claude.spesa_avvisata.get(&mese).copied());
        if let Some((gradino, voce)) = claude::avviso_spesa(&spesa, soglia, &mese, gia) {
            self.casa.aggiorna_config(|c| {
                // Restano solo i gradini del mese corrente: il file non cresce.
                c.claude.spesa_avvisata.retain(|m, _| *m == mese);
                c.claude.spesa_avvisata.insert(mese.clone(), gradino);
            });
            self.metti_voce(voce, true);
        }
    }

    /// L'avviso di spesa insolita: ieri è costato più di 3 volte la media dei 7 giorni prima
    /// (`api_claude::spesa_insolita`, solo con dati completi), una volta al giorno.
    fn claude_avviso_insolita(&self, adesso: chrono::DateTime<chrono::Utc>) {
        let oggi = adesso.date_naive();
        let ieri = oggi - chrono::Days::new(1);
        // Il prestito finisce qui: sotto non si tocca più `api_ultimo`.
        let importo = {
            let Some(r) = self.riepilogo_fresco(adesso) else { return };
            if !api_claude::spesa_insolita(&r, oggi) {
                return;
            }
            let Some(importo) = r.giorni.iter().find(|(d, _)| *d == ieri).map(|(_, i)| *i) else { return };
            importo
        };
        let giorno = ieri.format("%Y-%m-%d").to_string();
        if self.casa.con_config(|c| c.claude.insolita_avvisata.as_deref() == Some(giorno.as_str())) {
            return;
        }
        self.casa.aggiorna_config(|c| c.claude.insolita_avvisata = Some(giorno));
        self.metti_voce(claude::voce_insolita(&importo, ieri), true);
    }

    /// I limiti dell'abbonamento (5 ore, 7 giorni) a un'ora data (secondi dall'epoca): un limite
    /// già azzerato (`azzera_alle <= ora`) vale come assente. Senza abbonamento niente anelli.
    pub fn limiti_a(&self, ora: i64) -> (Option<Limite>, Option<Limite>) {
        let (c, s) = self.claude_limiti.borrow().unwrap_or((None, None));
        let buono = |l: &Limite| l.azzera_alle > ora && l.percentuale.is_finite();
        (c.filter(buono), s.filter(buono))
    }

    /// Come `limiti_a`, all'ora di adesso (per i test: il codice legge l'ora una volta sola e la
    /// passa a `limiti_a` e a `limiti_ricordati`).
    #[cfg(test)]
    pub fn limiti(&self) -> (Option<Limite>, Option<Limite>) {
        self.limiti_a(adesso_unix())
    }

    /// Un evento di Claude Code, sul thread dell'interfaccia.
    /// `sequenza`: l'ordine di arrivo della connessione (`Arrivo::sequenza`).
    pub(super) fn claude_evento(&self, e: EventoHook, sequenza: u64, ora: Instant) {
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
            s.ricevi(&e, ora)
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
                p.set_vista("agenti".into());
                self.vista_claude(true);
            }
            None => {
                self.vista_richiesta.set(Some("agenti"));
                crate::sistema::servizi::apri();
            }
        }
    }

    /// Un permesso chiesto da Claude Code, sul thread dell'interfaccia: una voce bloccante e il
    /// canale della risposta. Oltre i 16 in attesa il canale cade: il tubo risponde `Nessuna` e
    /// decide il terminale (stesso esito di nessuna attesa). La voce non c'è: resta quella normale
    /// «Claude ti aspetta».
    pub(super) fn claude_permesso(
        &self,
        id: u64,
        e: EventoHook,
        tx: Option<Sender<Decisione>>,
        sequenza: u64,
        ora: Instant,
    ) {
        if self.claude_permessi.borrow().len() >= MASSIMO_PERMESSI || self.claude_permessi.borrow().contains_key(&id) {
            drop(tx);
            self.claude_evento(e, sequenza, ora);
            return;
        }
        self.claude_permessi.borrow_mut().insert(id, Pendente { sessione: e.sessione.clone(), tx, sequenza });
        let titolo = self
            .claude_sessioni
            .borrow()
            .elenco()
            .into_iter()
            .find(|s| s.id == e.sessione)
            .map(|s| s.titolo.clone())
            .unwrap_or_default();
        let voce = claude::voce_permesso(id, &e, &titolo, ora, adesso_unix());
        // Prima il permesso in elenco, poi l'evento: la voce «ti aspetta» della sessione cede il
        // posto a quella del permesso.
        self.claude_evento(e, sequenza, ora);
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
        // Il tubo può aver già rinunciato (tempo scaduto, client chiuso): senza chi
        // riceve, la decisione non c'è stata e il terminale aspetta ancora.
        if p.tx.as_ref().is_some_and(|tx| tx.send(decisione).is_err()) {
            self.claude_permesso_chiuso(id, &p.sessione);
            // Il clic è arrivato dopo che Claude Code aveva smesso di aspettare: non ha avuto
            // effetto, e l'utente lo deve sapere (decide il terminale).
            self.metti_voce(claude::voce_decisione_tardiva(Instant::now()), false);
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

    /// Dopo ogni cambio delle voci: se la voce della pillola o la prima del pannello è cambiata, o
    /// se un permesso (voce con un pulsante delicato) è comparso o ha cambiato posto nella Coda
    /// (che mostra tutti i permessi: una riga che sale sotto il puntatore porta un altro
    /// «Consenti»), «Consenti» si arma fra `ARMAMENTO_CONSENTI`.
    pub(super) fn claude_riarma(&self, ora: Instant) {
        let (pillola, pannello, permessi) = {
            let a = self.attese.borrow();
            let v = a.ordinate();
            (
                v.iter().filter(|x| x.livello == Livello::Bloccante).min_by_key(|x| x.creata).map(|x| x.id.clone()),
                v.first().map(|x| x.id.clone()),
                v.iter()
                    .enumerate()
                    .filter(|(_, x)| x.pulsanti.iter().any(|p| p.delicato))
                    .map(|(i, x)| (i, x.id.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        let riarma = {
            let mut arm = self.claude_armamento.borrow_mut();
            // Un permesso nuovo o spostato (una sua coppia posto-id che prima non c'era); toglierne
            // uno senza spostare gli altri non riarma.
            let spostati = permessi.iter().any(|p| !arm.permessi.contains(p));
            let riarma = (pillola.is_some() && arm.pillola != pillola) || (pannello.is_some() && arm.pannello != pannello) || spostati;
            arm.pillola = pillola;
            arm.pannello = pannello;
            arm.permessi = permessi;
            riarma
        };
        if riarma {
            self.sposta_armamento(ora);
        }
    }

    /// L'unica strada che sposta `dal`: la guardia ignora «Consenti» (e, in `azione_voce`, anche
    /// «Nega») fino a `ora + ARMAMENTO_CONSENTI`. A pannello aperto conta un armamento
    /// (`Pannello.armamenti`, ridisegno F2 Task 4; pre-flight C5: +1 se e solo se `dal` si sposta a
    /// pannello aperto), e il pannello mostra i pulsanti inerti per gli stessi 600 ms. Il pannello
    /// non decide la guardia: la segue.
    fn sposta_armamento(&self, ora: Instant) {
        self.claude_armamento.borrow().dal.set(ora + ARMAMENTO_CONSENTI);
        if let Some(p) = self.pannello.borrow().upgrade() {
            p.set_armamenti(p.get_armamenti().wrapping_add(1));
        }
    }

    /// Un pulsante della voce che ti aspetta, nella Panoramica (ridisegno F1, Task 8): nei 600 ms
    /// dopo che la voce mostrata è cambiata (o è comparsa) si ignora, anche «Nega» — un doppio clic
    /// su «Nega» non risponde anche alla voce che prende il suo posto, sotto lo stesso punto. Poi è
    /// un'`azione` come le altre («Consenti» resta controllato dal suo armamento).
    pub fn azione_voce(&self, voce: &str, pulsante: &str, ora: Instant) {
        if ora < self.claude_armamento.borrow().dal.get() {
            return;
        }
        self.azione(voce, pulsante);
    }

    /// Una superficie che mostra i permessi compare (pannello aperto, pillola che torna): con un
    /// permesso in attesa «Consenti» si arma fra `ARMAMENTO_CONSENTI`.
    pub(super) fn claude_superficie_compare(&self, ora: Instant) {
        if !self.claude_permessi.borrow().is_empty() {
            self.sposta_armamento(ora);
        }
    }

    /// Il pannello ha cambiato vista (`vista-cambiata`): ciò che vive solo a vista aperta segue.
    pub fn vista_cambiata(&self, p: &crate::Pannello) {
        let vista = p.get_vista();
        // Wi-Fi e Bluetooth (M2, Task 8) si seguono solo a vista PC aperta.
        self.vista_pc(vista == "pc");
        // La vista Claude (M3, Task 9) scrive e conta il tempo solo a vista aperta.
        self.vista_claude(vista == "agenti");
        // La Coda (ridisegno F1, Task 7): entrandoci «Consenti» si riarma.
        self.vista_coda(vista == "coda");
        // La scheda della vista nuova si allarga col nome (la capsula scivola).
        self.scrivi_schede(p);
    }

    /// La Coda si apre (vero) o si chiude (altra vista, pannello chiuso). È una superficie che
    /// mostra i permessi: entrandoci, con un permesso in attesa, «Consenti» si arma fra
    /// `ARMAMENTO_CONSENTI` (il fuoco o il mouse vi arrivano su una riga appena comparsa).
    pub fn vista_coda(&self, si: bool) {
        if self.vista_coda.replace(si) != si && si {
            self.claude_superficie_compare(Instant::now());
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

    /// Il tubo ha lasciato il permesso `id` senza risposta: via la voce.
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
        self.moduli_aggiorna("agenti");
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
        self.moduli_aggiorna("agenti");
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
        // «Ha finito» visto: la carta torna alle sessioni ferme (a pannello chiuso: all'apertura).
        self.moduli_aggiorna("agenti");
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
        // Chiusa o appena aperta l'ordine delle righe non c'è: all'apertura si decide di nuovo
        // (per urgenza) e il fuoco riparte dalla prima riga (RC-53).
        self.claude_ordine.borrow_mut().clear();
        if si {
            self.claude_righe.set_vec(Vec::new());
            if let Some(p) = self.pannello.borrow().upgrade() {
                p.set_claude_fuoco(0);
            }
            // Il terminale di una sessione può essere sparito dall'ultima volta: lo si rilegge qui.
            #[cfg(any(windows, test))]
            self.claude_rileggi_ripiego();
            // Un limite ricordato dal file di stato scade con la sveglia singola dei limiti.
            self.arma_sveglia_limiti(self.prossimo_azzeramento(adesso_unix()), adesso_unix());
        }
        // La sveglia dei limiti non dipende dalla vista: serve anche a togliere gli avvisi.
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
                        p.claude_limiti_cambiati();
                    }
                });
            }
        }
    }

    /// I dati della vista Claude, scritti solo se la vista è aperta e solo ciò che è cambiato: le
    /// righe (una per sessione, in ordine fermo: RC-52 e RC-53), i limiti dell'abbonamento (un
    /// limite assente o azzerato non ha anello; uno non ancora arrivato dopo il riavvio è quello
    /// ricordato, attenuato: RC-56), la spesa del mese e se Claude Code è collegato.
    fn aggiorna_vista_claude(&self) {
        self.aggiorna_vista_claude_a(adesso_unix());
    }

    /// Come `aggiorna_vista_claude`, a un'ora data (secondi dall'epoca): i limiti scaduti a quell'ora
    /// non si mostrano.
    pub(super) fn aggiorna_vista_claude_a(&self, ora: i64) {
        if !self.vista_claude.get() {
            return;
        }
        let Some(p) = self.pannello.borrow().upgrade() else { return };
        let collegato = self.casa.con_config(|c| c.claude.collegato);
        if p.get_claude_collegato() != collegato {
            p.set_claude_collegato(collegato);
        }
        // Scollegato non ci sono sessioni da mostrare: lo stato vuoto parla da sé.
        let (righe, ordine) = if collegato {
            claude::righe_sessioni(
                &self.claude_sessioni.borrow(),
                &self.claude_contesto.borrow(),
                Instant::now(),
                &self.claude_ordine.borrow(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        *self.claude_ordine.borrow_mut() = ordine;
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
        // Il fuoco da tastiera non si perde (RC-53): resta sulla sua sessione, e se la sua riga è
        // sparita passa a quella che ne prende il posto (la più vicina), mai oltre l'ultima.
        let fuoco = p.get_claude_fuoco().max(0);
        let col_fuoco = self.claude_righe.row_data(fuoco as usize).map(|r| r.id);
        let voci: Vec<RigaSessioneUi> = righe
            .into_iter()
            .map(|r| {
                let (ripiego, a_finestra) = (!raggiungibili.contains(&r.id), a_finestre.contains(&r.id));
                riga_ui(r, ripiego, a_finestra)
            })
            .collect();
        let nuovo_fuoco = col_fuoco
            .and_then(|id| voci.iter().position(|v| v.id == id))
            .unwrap_or_else(|| (fuoco as usize).min(voci.len().saturating_sub(1)));
        let n = super::aggiorna_modello(&self.claude_righe, voci);
        self.casa.diagnosi().conta_n(Contatore::ui_righe, n);
        if p.get_claude_fuoco() != nuovo_fuoco as i32 {
            p.set_claude_fuoco(nuovo_fuoco as i32);
        }
        let (cinque, sette) = self.limiti_della_vista(ora);
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

    /// I due limiti come li vuole la vista: quelli arrivati, o per la finestra che non è ancora
    /// arrivata quello ricordato dal file di stato (attenuato, «dalle…»); assente (o azzerato)
    /// = nessun anello.
    fn limiti_della_vista(&self, ora: i64) -> (LimiteClaudeUi, LimiteClaudeUi) {
        let adesso = chrono::Local.timestamp_opt(ora, 0).single().unwrap_or_else(chrono::Local::now);
        let (cinque, sette) = self.limiti_a(ora);
        let (ricordato_c, ricordato_s) = self.limiti_ricordati(ora);
        let uno = |arrivato: Option<Limite>, ricordato: Option<LimiteVisto>| match (arrivato, ricordato) {
            (Some(l), _) => limite_ui(Some(l), adesso),
            (None, Some(v)) => limite_ricordato_ui(&v, adesso),
            (None, None) => LimiteClaudeUi::default(),
        };
        (uno(cinque, ricordato_c), uno(sette, ricordato_s))
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

/// Le catene portate davanti finora in questo thread (e le dimentica): per i test degli altri
/// pezzi del ponte che arrivano al terminale (l'azione della carta Agenti).
#[cfg(test)]
pub(super) fn portati_davanti_prova() -> Vec<Vec<(u32, String)>> {
    PORTATI_DAVANTI.with(|v| std::mem::take(&mut *v.borrow_mut()))
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
    use claude_hook::sessioni::Stato;
    // Le parole e i colori della vista di F1, come prima (la pagina Agenti di F2 usa
    // `moduli::pagina_agenti`). «Tocca a te», come la carta: «Ti aspetta» è la parola del tono
    // Aspetta, del permesso bloccante (vocabolario chiuso).
    let (stato, livello) = match r.stato {
        Stato::Lavora => ("Lavora", None),
        Stato::TiAspetta => ("Tocca a te", Some(Livello::Attenzione)),
        Stato::Fatto => ("Fatto", Some(Livello::Informazione)),
        Stato::Errore => ("Errore", Some(Livello::Attenzione)),
    };
    RigaSessioneUi {
        terminale: if ripiego && cfg!(windows) { "ultimo terminale usato" } else { "" }.into(),
        avviso: if finestra_non_scheda && cfg!(windows) { "Apre la finestra del terminale, non la scheda" } else { "" }.into(),
        id: r.id.into(),
        cartella: r.cartella.into(),
        titolo: r.titolo.into(),
        stato: stato.into(),
        livello: livello.map_or(LivelloUi::Nessuno, pillola::livello_ui),
        strumento: r.strumento.into(),
        durata: r.durata.into(),
        contesto: r.contesto.map(|c| format!("{c}%")).unwrap_or_default().into(),
        contesto_caldo: r.contesto_caldo,
    }
}

/// Un limite dell'abbonamento per l'anello; assente (o azzerato) = nessun anello.
fn limite_ui(l: Option<Limite>, ora: chrono::DateTime<chrono::Local>) -> LimiteClaudeUi {
    match l {
        None => LimiteClaudeUi::default(),
        Some(l) => {
            let t = claude::testo_limite(&l, ora);
            LimiteClaudeUi { presente: true, frazione: t.frazione, testo: t.testo.into(), azzera: t.azzera.into(), vecchio: false }
        }
    }
}

/// Un limite ricordato dal file di stato (RC-56): la percentuale di quando lo si è visto, «dalle
/// 20:29» al posto di «si azzera…», e `vecchio` perché la vista lo attenui.
fn limite_ricordato_ui(v: &LimiteVisto, adesso: chrono::DateTime<chrono::Local>) -> LimiteClaudeUi {
    let t = claude::testo_limite_ricordato(v, adesso);
    LimiteClaudeUi { presente: true, frazione: t.frazione, testo: t.testo.into(), azzera: t.azzera.into(), vecchio: true }
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
        p.claude_evento(e, SEQUENZA.fetch_add(1, Ordering::Relaxed), Instant::now());
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
                titolo: String::new(),
                stato: claude_hook::sessioni::Stato::Fatto,
                vista: false,
                errore: None,
                strumento: String::new(),
                durata: String::new(),
                contesto: None,
                contesto_caldo: false,
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
        assert_eq!(p.vista_richiesta.get(), Some("agenti"));
        let pannello = con_pannello(&p);
        assert_eq!(pannello.get_vista().as_str(), "agenti");
        assert_eq!(p.vista_richiesta.get(), None, "una volta sola");
    }

    /// Una voce di prova: `fonte`, `livello` e, se `permesso`, i pulsanti di un permesso (Nega e
    /// il delicato Consenti).
    fn voce_di_prova(id: &str, fonte: Fonte, livello: Livello, permesso: bool) -> Voce {
        Voce {
            id: id.into(),
            livello,
            fonte,
            titolo: "Titolo".into(),
            dettaglio: String::new(),
            pulsanti: if permesso { vec![Pulsante::new("nega", "Nega"), Pulsante::delicato("consenti", "Consenti")] } else { vec![] },
            creata: Instant::now(),
            permesso: None,
        }
    }

    /// Ridisegno F1, Task 7: il clic sulla pillola porta alla destinazione. I sette casi: permesso
    /// allargato → Coda col permesso aperto (fix 1: il testo intero si legge lì); voce allargata
    /// di Claude che non è un permesso → Agenti; di Casa → Casa; del PC → PC; pillola col puntino →
    /// Coda; attività di Claude → Agenti; riposo → Panoramica. Sostituisce i test di `Espandi`.
    #[test]
    fn dalla_pillola_alla_destinazione() {
        use crate::ponte::DallaPillola;
        let (_s, p, pillola, _d) = ponte_di_prova();
        let dove = |p: &Ponte| p.apertura_dalla_pillola(pillola.get_attivita_visibile());
        // Riposo: la Panoramica.
        assert_eq!(dove(&p), DallaPillola::Panoramica, "riposo");
        // Claude lavora e la pillola lo dice: Agenti.
        manda(&p, ev("a", TipoEvento::UserPromptSubmit));
        assert!(pillola.get_attivita_visibile());
        assert_eq!(dove(&p), DallaPillola::Vista("agenti"), "attività di Claude");
        // Voci allargate (attenzione appena arrivata): la pagina del loro modulo.
        for (id, fonte, vista) in [("casa.x", Fonte::Casa, "casa"), ("pc.x", Fonte::Pc, "pc"), ("claude.x.fatto", Fonte::Claude, "agenti")] {
            p.metti_voce(voce_di_prova(id, fonte, Livello::Attenzione, false), false);
            assert!(!pillola.get_attivita_visibile(), "{id}: allargata, niente attività");
            assert_eq!(dove(&p), DallaPillola::Vista(vista), "{id}");
            p.togli_voce(id);
        }
        // Un permesso allargato: aperto nella Coda col testo intero (fix 1, C1), alla sua posizione.
        p.metti_voce(voce_di_prova("casa.info", Fonte::Casa, Livello::Informazione, false), false);
        p.metti_voce(voce_di_prova("claude.permesso.7", Fonte::Claude, Livello::Bloccante, true), false);
        assert_eq!(dove(&p), DallaPillola::Permesso(0), "permesso");
        p.togli_voce("claude.permesso.7");
        p.togli_voce("casa.info");
        // Senza risposta dalla pillola il permesso ha solo «Apri il terminale»: è ancora un permesso.
        p.metti_voce(voce_di_prova("claude.permesso.8", Fonte::Claude, Livello::Bloccante, false), false);
        assert_eq!(dove(&p), DallaPillola::Permesso(0), "permesso senza risposta dalla pillola");
        p.togli_voce("claude.permesso.8");
        manda(&p, ev("a", TipoEvento::SessionEnd));
        // Il puntino (una sola informazione: non allarga): la Coda.
        p.metti_voce(voce_di_prova("casa.info", Fonte::Casa, Livello::Informazione, false), false);
        assert_eq!(p.mostrato.borrow().as_ref().map(|v| v.aspetto.clone()), Some(crate::attenzione::AspettoPillola::Puntino));
        assert_eq!(dove(&p), DallaPillola::Vista("coda"), "puntino");
        p.togli_voce("casa.info");
        assert_eq!(dove(&p), DallaPillola::Panoramica, "di nuovo a riposo");
    }

    /// La vista chiesta dalla pillola vale per il pannello che si apre, una volta sola, su ogni
    /// sistema; se quel pannello non si apre non resta per il prossimo.
    #[test]
    fn la_vista_chiesta_vale_per_un_pannello_solo() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.chiedi_vista("agenti");
        p.pannello_chiuso();
        assert_eq!(p.vista_richiesta.get(), None, "un pannello non aperto non la lascia in sospeso");
        assert_eq!(con_pannello(&p).get_vista().as_str(), "panoramica");
        p.chiedi_vista("agenti");
        let pannello = con_pannello(&p);
        assert_eq!(pannello.get_vista().as_str(), "agenti");
        assert_eq!(p.vista_richiesta.get(), None);
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
            titolo: String::new(),
            stato: claude_hook::sessioni::Stato::Fatto,
            vista: false,
            errore: None,
            strumento: String::new(),
            durata: String::new(),
            contesto: None,
            contesto_caldo: false,
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
            assert_eq!(c.attesa_massima, ATTESA_TUBO);
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
        // RC-36: la durata sta da sola a destra, senza il «·» davanti.
        assert_eq!(pillola.get_attivita_destra(), "0:00");
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
    /// che aspetta il tubo.
    fn permesso(p: &Ponte, id: u64, sessione: &str) -> Receiver<Decisione> {
        let (tx, rx) = channel();
        p.claude_permesso(id, chiede(sessione, "ls -la"), Some(tx), SEQUENZA.fetch_add(1, Ordering::Relaxed), Instant::now());
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
        // Ricevitore caduto con la voce ancora lì (il tubo ha chiuso): l'invio che fallisce si ignora.
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

    /// Ridisegno F1, Task 7: aprire la Coda con un permesso in attesa è una superficie che compare:
    /// «Consenti» si arma di nuovo fra `ARMAMENTO_CONSENTI`. Restare nella Coda non riarma; uscirne
    /// e rientrare sì.
    #[test]
    fn coda_riarma_consenti() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx = permesso(&p, 1, "a");
        armato(&p);
        let prima = Instant::now();
        p.vista_coda(true);
        let dal = p.claude_armamento.borrow().dal.get();
        assert!(dal >= prima + ARMAMENTO_CONSENTI && dal <= Instant::now() + ARMAMENTO_CONSENTI, "armamento spostato");
        p.claude_decidi(1, Decisione::Consenti, dal - Duration::from_millis(1));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty), "Consenti subito è ignorato");
        // Già nella Coda: un secondo avviso della stessa vista non sposta nulla.
        armato(&p);
        p.vista_coda(true);
        assert!(p.claude_armamento.borrow().dal.get() <= Instant::now(), "nessun riarmo restando nella Coda");
        p.vista_coda(false);
        p.vista_coda(true);
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now(), "rientrando si riarma");
    }

    /// Un pannello vero col ponte di prova, agganciato come in `pannello.rs` (senza il
    /// campionatore del PC), con un permesso in attesa e «Consenti» armato.
    fn pannello_col_permesso(p: &Rc<Ponte>) -> (crate::Pannello, Receiver<Decisione>) {
        use slint::ComponentHandle;
        let pannello = crate::Pannello::new().unwrap();
        crate::pannello::collega_fuoco(&pannello);
        pannello.set_aperto(true);
        pannello.show().unwrap();
        p.riempi(&pannello);
        p.pannello_aperto(&pannello);
        let (ponte, debole) = (p.clone(), pannello.as_weak());
        pannello.on_vista_cambiata(move || {
            if let Some(x) = debole.upgrade() {
                ponte.vista_cambiata(&x);
            }
        });
        let rx = permesso(p, 1, "a");
        slint::platform::update_timers_and_animations();
        (pannello, rx)
    }

    /// Ridisegno F1, Task 8: la voce che ti aspetta compare nella Panoramica (anche tornandoci
    /// da un'altra vista): «Consenti» si riarma.
    #[test]
    fn la_voce_che_compare_nella_panoramica_riarma() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let (pannello, _rx) = pannello_col_permesso(&p);
        assert!(pannello.get_voce_mostrata(), "la voce ti aspetta al posto dell'orologio");
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now(), "comparsa: riarmato");
        pannello.set_vista("pc".into());
        slint::platform::update_timers_and_animations();
        armato(&p);
        pannello.set_vista("panoramica".into());
        slint::platform::update_timers_and_animations();
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now(), "tornando nella Panoramica si riarma");
        p.pannello_chiuso();
    }

    /// Ridisegno F1, Task 8: un pulsante della voce che ti aspetta premuto nei 600 ms dopo un
    /// cambio della voce mostrata si ignora, anche «Nega» (il secondo clic di un doppio clic non
    /// nega la voce che ha preso il posto della prima); dopo, «Nega» nega.
    #[test]
    fn azione_voce_ignorata_appena_dopo_il_cambio() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx_a = permesso(&p, 1, "a");
        let id = voce_di(1);
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now(), "la voce nuova arma");
        p.azione_voce(&id, PULSANTE_NEGA, Instant::now());
        assert_eq!(rx_a.try_recv(), Err(TryRecvError::Empty), "«Nega» subito dopo il cambio è ignorato");
        assert!(p.attese.borrow().contiene(&id));
        armato(&p);
        p.azione_voce(&id, PULSANTE_NEGA, Instant::now());
        assert_eq!(rx_a.try_recv(), Ok(Decisione::Nega), "passato il tempo «Nega» nega");
    }

    /// Fix 1 del Task 7 (M2 della revisione): entrare nella Coda dal pannello vero passa da
    /// `vista-cambiata` a `vista_coda` e riarma «Consenti». (Dal Task 8 la riga «In attesa» non
    /// c'è più: si entra cambiando vista.)
    #[test]
    fn entrare_nella_coda_dal_pannello_riarma() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let (pannello, _rx) = pannello_col_permesso(&p);
        armato(&p);
        assert!(p.claude_armamento.borrow().dal.get() <= Instant::now(), "armato prima di entrare");
        pannello.set_vista("coda".into());
        slint::platform::update_timers_and_animations();
        assert_eq!(pannello.get_vista(), "coda");
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now(), "entrando nella Coda «Consenti» si riarma");
        p.pannello_chiuso();
    }

    /// Fix 2 del Task 7 (N1): dentro la Coda già armata, aprire un permesso (dove «Consenti»
    /// compare) riarma: un «Consenti» premuto subito dopo l'apertura è ignorato e il canale resta.
    #[test]
    fn aprire_il_permesso_nella_coda_riarma() {
        use i_slint_backend_testing::ElementHandle;
        use slint::ComponentHandle;
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = crate::Pannello::new().unwrap();
        pannello.set_aperto(true);
        pannello.show().unwrap();
        p.riempi(&pannello);
        p.pannello_aperto(&pannello);
        let rx = permesso(&p, 1, "a");
        pannello.set_vista("coda".into());
        slint::platform::update_timers_and_animations();
        // Nella Coda da più di 600 ms.
        armato(&p);
        let apri = ElementHandle::find_by_accessible_label(&pannello, "Apri").next().expect("«Apri» del permesso");
        apri.invoke_accessible_default_action();
        let _ = ElementHandle::find_by_element_id(&pannello, "VoceAperta::titolo").count();
        slint::platform::update_timers_and_animations();
        let _ = ElementHandle::find_by_element_id(&pannello, "VoceAperta::titolo").count();
        slint::platform::update_timers_and_animations();
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now(), "«Consenti» comparso: riarmato");
        // Il clic su «Consenti» subito dopo non concede.
        let consenti = ElementHandle::find_by_accessible_label(&pannello, "Consenti").next().expect("«Consenti» (testo corto)");
        consenti.invoke_accessible_default_action();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty), "consenso entro 600 ms ignorato");
        assert!(ha_voce(&p, &voce_di(1)), "il permesso resta");
        p.pannello_chiuso();
    }

    /// Fix 1 del Task 7 (I1): la Coda mostra tutti i «Consenti». Tolto il permesso di mezzo, quello
    /// più vecchio sale al suo posto (sotto il puntatore): si arma di nuovo, anche se la prima voce
    /// e il bloccante più vecchio non cambiano. Toglierne uno senza spostare gli altri no.
    #[test]
    fn tolgo_il_permesso_di_mezzo_riarma() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let _r1 = permesso(&p, 1, "a");
        let _r2 = permesso(&p, 2, "b");
        let _r3 = permesso(&p, 3, "c");
        armato(&p);
        p.togli_voce(&voce_di(2));
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now(), "il permesso salito si arma di nuovo");
        // Una voce che non è un permesso, sotto i permessi, va e viene: nessuno si sposta, nessun riarmo.
        p.metti_voce(voce_di_prova("casa.info", Fonte::Casa, Livello::Informazione, false), false);
        armato(&p);
        p.togli_voce("casa.info");
        assert!(p.claude_armamento.borrow().dal.get() <= Instant::now(), "nessun permesso si è spostato");
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

    /// Ridisegno F2, Task 4 (pre-flight C5): `Pannello.armamenti` cresce di uno **se e solo se**
    /// `dal` si sposta a pannello aperto, perché è lì che la guardia di `azione_voce` ignora anche
    /// «Nega» e il pannello deve dirlo. Con un permesso in attesa la superficie che compare e il
    /// permesso che cambia posto contano uno ciascuno; senza permessi la superficie che compare
    /// non conta, il primo bloccante che cambia sì; nulla che non sposti `dal`; nulla a pannello
    /// chiuso. Due spostamenti di fila (consenti-compare più `claude_riarma`, nota del Task 1)
    /// contano due: l'aspetto riparte dall'ultimo, come `dal`.
    #[test]
    fn armamenti_contano_ogni_riarmo() {
        // `Inerzia` (ui/componenti.slint) dura 600 ms: i due tempi vanno cambiati insieme.
        assert_eq!(ARMAMENTO_CONSENTI, Duration::from_millis(600));
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = crate::Pannello::new().unwrap();
        p.pannello_aperto(&pannello);
        let prima = pannello.get_armamenti();
        // Senza permessi: la superficie che compare non sposta `dal`, nessun conteggio.
        p.claude_superficie_compare(Instant::now());
        assert_eq!(pannello.get_armamenti(), prima, "senza permessi nessun cambio");
        // Il primo bloccante (non un permesso) cambia: `dal` si sposta, «Nega» è ignorato: +1.
        armato(&p);
        p.metti_voce(voce_di_prova("casa.blocco", Fonte::Casa, Livello::Bloccante, false), false);
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now());
        assert_eq!(pannello.get_armamenti(), prima + 1, "il bloccante nuovo arma");
        p.togli_voce("casa.blocco");
        // Tre permessi; poi la superficie che compare: +1.
        let _r1 = permesso(&p, 1, "a");
        let _r2 = permesso(&p, 2, "b");
        let _r3 = permesso(&p, 3, "c");
        armato(&p);
        let n = pannello.get_armamenti();
        p.claude_superficie_compare(Instant::now());
        assert_eq!(pannello.get_armamenti(), n + 1, "la superficie che compare arma");
        // Il permesso di mezzo tolto: quello sotto sale, `claude_riarma` sposta `dal`: +1.
        armato(&p);
        p.togli_voce(&voce_di(2));
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now());
        assert_eq!(pannello.get_armamenti(), n + 2, "il permesso spostato arma");
        // Una voce che non sposta nessun permesso né la prima: `dal` fermo, nessun conteggio.
        armato(&p);
        p.metti_voce(voce_di_prova("casa.info", Fonte::Casa, Livello::Informazione, false), false);
        p.togli_voce("casa.info");
        assert!(p.claude_armamento.borrow().dal.get() <= Instant::now());
        assert_eq!(pannello.get_armamenti(), n + 2, "dal fermo: nessun conteggio");
        // Due spostamenti di fila contano due.
        p.claude_superficie_compare(Instant::now());
        p.claude_superficie_compare(Instant::now());
        assert_eq!(pannello.get_armamenti(), n + 4);
        // A pannello chiuso `dal` si sposta ancora (la pillola), ma nessuno scrive sul pannello.
        p.pannello_chiuso();
        let chiuso = pannello.get_armamenti();
        p.claude_superficie_compare(Instant::now());
        p.togli_voce(&voce_di(1));
        assert!(p.claude_armamento.borrow().dal.get() > Instant::now());
        assert_eq!(pannello.get_armamenti(), chiuso, "nessuna scrittura a pannello chiuso");
    }

    #[test]
    fn invio_non_riuscito_non_e_una_decisione() {
        use claude_hook::sessioni::Stato;
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let rx = permesso(&p, 1, "a");
        drop(rx); // il tubo ha rinunciato
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
        p.claude_permesso(1, chiede("a", "ls"), Some(tx), prima + 1, Instant::now());
        p.claude_evento(ev("a", TipoEvento::PreToolUse), prima, Instant::now());
        assert!(ha_voce(&p, &voce_di(1)), "una PreToolUse più vecchia non chiude");
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        // Una più nuova sì.
        p.claude_evento(ev("a", TipoEvento::PreToolUse), prima + 2, Instant::now());
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
        DatiBarra { sessione: sessione.into(), modello: Some("Opus 5.5".into()), contesto: Some(40.0), cinque_ore: l(cinque), sette_giorni: l(sette), titolo: None }
    }

    /// La barra di una sessione col suo titolo (`session_name`).
    fn barra_titolata(sessione: &str, titolo: Option<&str>) -> DatiBarra {
        DatiBarra { titolo: titolo.map(str::to_owned), ..barra(sessione, None, None) }
    }

    /// Due sessioni nella stessa cartella: il titolo dalla barra arriva alla vista Claude, alle
    /// voci «In attesa» e al nome della pillola; un titolo assente non cancella il precedente, e la
    /// barra non crea sessioni.
    #[test]
    fn il_titolo_della_barra_arriva_a_vista_voci_e_pillola() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        let c_e_voce = |p: &Ponte, titolo: &str| voci_claude(p).iter().any(|(_, _, t)| t == titolo);
        // Una barra per una sessione sconosciuta non la crea.
        p.claude_barra(barra_titolata("x", Some("Fantasma")));
        assert!(p.claude_sessioni.borrow().elenco().is_empty());
        manda(&p, EventoHook { cartella: "bin".into(), ..ev("a", TipoEvento::UserPromptSubmit) });
        manda(&p, EventoHook { cartella: "bin".into(), ..ev("b", TipoEvento::PermissionRequest) });
        p.vista_claude(true);
        let titoli = |pannello: &crate::Pannello| -> Vec<(String, String)> {
            let righe = pannello.get_claude_righe();
            (0..righe.row_count())
                .map(|i| righe.row_data(i).unwrap())
                .map(|r| (r.cartella.to_string(), r.titolo.to_string()))
                .collect()
        };
        assert_eq!(titoli(&pannello), [("bin".into(), String::new()), ("bin".into(), String::new())]);
        assert!(c_e_voce(&p, "Claude ti aspetta · bin"));
        // I titoli arrivano con le barre; l'ordine fermo della vista non cambia.
        p.claude_barra(barra_titolata("b", Some("Aggiornare i test")));
        p.claude_barra(barra_titolata("a", Some("Sistemare il restyle")));
        assert_eq!(
            titoli(&pannello),
            [("bin".into(), "Aggiornare i test".into()), ("bin".into(), "Sistemare il restyle".into())]
        );
        assert!(c_e_voce(&p, "Claude ti aspetta · Aggiornare i test"));
        assert!(!c_e_voce(&p, "Claude ti aspetta · bin"), "la voce vecchia non resta");
        // Una barra senza titolo non lo cancella; una con un titolo nuovo lo cambia.
        p.claude_barra(barra_titolata("b", None));
        assert_eq!(titoli(&pannello)[0].1, "Aggiornare i test");
        p.claude_barra(barra_titolata("b", Some("Altro")));
        assert_eq!(titoli(&pannello)[0].1, "Altro");
        // Il nome per Narrator della pillola: «a» lavora da sola, col suo titolo.
        let nome = claude::nome_attivita(&p.claude_sessioni.borrow(), Instant::now()).unwrap();
        assert_eq!(nome, "Claude, Sistemare il restyle, Pensa, meno di un minuto");
    }

    /// Il titolo è anche nel permesso che una sessione già titolata chiede.
    #[test]
    fn il_permesso_dice_il_titolo_della_sessione() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        manda(&p, EventoHook { cartella: "bin".into(), ..ev("a", TipoEvento::UserPromptSubmit) });
        p.claude_barra(barra_titolata("a", Some("Sistemare il restyle")));
        let e = EventoHook {
            cartella: "bin".into(),
            strumento: Some("Bash".into()),
            riassunto: Some("ls".into()),
            ..ev("a", TipoEvento::PermissionRequest)
        };
        p.claude_permesso(1, e, None, 2, Instant::now());
        let dettaglio = p.attese.borrow().voce(&claude::id_voce_permesso(1)).map(|v| v.dettaglio.clone());
        assert_eq!(dettaglio.as_deref(), Some("ls\nSessione: Sistemare il restyle"));
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
        // Un'altra sessione che chiede qualcosa: a pannello aperto va in fondo (RC-53).
        manda(&p, ev("b", TipoEvento::PermissionRequest));
        let righe = pannello.get_claude_righe();
        assert_eq!(righe.row_count(), 2);
        assert_eq!(righe.row_data(1).unwrap().stato.as_str(), "Tocca a te");
        assert_eq!(righe.row_data(1).unwrap().livello, LivelloUi::Attenzione);
        // Cinque sessioni: cinque righe (la colonna scorre, RC-52).
        for id in ["c", "d", "e"] {
            manda(&p, ev(id, TipoEvento::UserPromptSubmit));
        }
        assert_eq!(pannello.get_claude_righe().row_count(), 5);
        // Solo chi lavora ha una durata (le altre righe non ne hanno).
        let prima: Vec<String> = (0..5).map(|i| pannello.get_claude_righe().row_data(i).unwrap().durata.to_string()).collect();
        assert_eq!(prima.iter().filter(|d| !d.is_empty()).count(), 4);
        // Chiusa di nuovo: gli eventi non scrivono più.
        p.vista_claude(false);
        manda(&p, ev("e", TipoEvento::SessionEnd));
        assert_eq!(pannello.get_claude_righe().row_count(), 5);
        // Riaperta: la lista è quella vera.
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_righe().row_count(), 4);
        // Il pannello chiuso del tutto: la vista si chiude da sé.
        p.pannello_chiuso();
        assert!(!p.vista_claude.get());
    }

    /// Un evento con una cartella per sessione («progetto-a» per «a»): le righe si distinguono.
    fn evc(id: &str, tipo: TipoEvento) -> EventoHook {
        EventoHook { cartella: format!("progetto-{id}"), ..ev(id, tipo) }
    }

    /// Le cartelle delle righe della vista, dall'alto.
    fn cartelle_vista(pannello: &crate::Pannello) -> Vec<String> {
        let righe = pannello.get_claude_righe();
        (0..righe.row_count()).map(|i| righe.row_data(i).unwrap().cartella.to_string()).collect()
    }

    /// RC-53: l'ordine si decide all'apertura (per urgenza) e poi sta fermo: chi cambia stato non
    /// salta, chi arriva va in fondo; alla riapertura si riordina.
    #[test]
    fn l_ordine_e_fermo_a_pannello_aperto_e_si_rifa_alla_riapertura() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        manda(&p, evc("a", TipoEvento::UserPromptSubmit));
        manda(&p, evc("b", TipoEvento::Stop));
        p.vista_claude(true);
        assert_eq!(cartelle_vista(&pannello), ["progetto-b", "progetto-a"], "all'apertura: per urgenza");
        // «a» chiede un permesso: per urgenza passerebbe in cima, ma la riga resta dov'è.
        manda(&p, evc("a", TipoEvento::PermissionRequest));
        assert_eq!(cartelle_vista(&pannello), ["progetto-b", "progetto-a"]);
        assert_eq!(pannello.get_claude_righe().row_data(1).unwrap().stato.as_str(), "Tocca a te", "lo stato però si aggiorna");
        // Una sessione nuova, anche urgente, in fondo.
        manda(&p, evc("c", TipoEvento::PermissionRequest));
        assert_eq!(cartelle_vista(&pannello), ["progetto-b", "progetto-a", "progetto-c"]);
        // Chiusa e riaperta: si riordina (chi aspetta, il più recente prima; poi chi ha finito).
        p.vista_claude(false);
        p.vista_claude(true);
        assert_eq!(cartelle_vista(&pannello), ["progetto-c", "progetto-a", "progetto-b"]);
    }

    /// RC-53 e Review Focus 4: il fuoco da tastiera non si perde mai. Segue la sua sessione se ne
    /// sparisce un'altra, passa alla riga più vicina se sparisce lei, riparte dalla prima all'apertura.
    #[test]
    fn il_fuoco_delle_righe_segue_la_sessione_e_passa_alla_piu_vicina() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        for id in ["a", "b", "c", "d"] {
            manda(&p, evc(id, TipoEvento::UserPromptSubmit));
        }
        p.vista_claude(true);
        // Chi lavora: la più recente prima, quindi d, c, b, a. Il fuoco sulla terza («b»).
        assert_eq!(cartelle_vista(&pannello), ["progetto-d", "progetto-c", "progetto-b", "progetto-a"]);
        pannello.set_claude_fuoco(2);
        let con_fuoco = || cartelle_vista(&pannello)[pannello.get_claude_fuoco() as usize].clone();
        assert_eq!(con_fuoco(), "progetto-b");
        // Ne sparisce una sopra: il fuoco resta su «b» (che sale di un posto).
        manda(&p, ev("d", TipoEvento::SessionEnd));
        assert_eq!((pannello.get_claude_fuoco(), con_fuoco().as_str()), (1, "progetto-b"));
        // Sparisce la riga col fuoco: passa a quella che ora occupa il posto, cioè la più vicina.
        manda(&p, ev("b", TipoEvento::SessionEnd));
        assert_eq!((pannello.get_claude_fuoco(), con_fuoco().as_str()), (1, "progetto-a"));
        // Sparisce l'ultima, che aveva il fuoco: va all'ultima rimasta.
        manda(&p, ev("a", TipoEvento::SessionEnd));
        assert_eq!((pannello.get_claude_fuoco(), con_fuoco().as_str()), (0, "progetto-c"));
        // Nessuna sessione: il fuoco resta valido (0), niente indici fuori dalle righe.
        manda(&p, ev("c", TipoEvento::SessionEnd));
        assert_eq!(pannello.get_claude_fuoco(), 0);
        // Alla riapertura riparte dalla prima riga.
        manda(&p, evc("e", TipoEvento::UserPromptSubmit));
        manda(&p, evc("f", TipoEvento::UserPromptSubmit));
        pannello.set_claude_fuoco(1);
        p.vista_claude(false);
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_fuoco(), 0);
    }

    /// RC-52: tutte le sessioni arrivano alla vista, anche dieci: è lei che scorre.
    #[test]
    fn dieci_sessioni_sono_dieci_righe() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        for i in 0..10 {
            manda(&p, ev(&format!("s{i}"), TipoEvento::UserPromptSubmit));
        }
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_righe().row_count(), 10);
    }

    /// RC-55: il contesto sopra l'80 % arriva alla riga come caldo.
    #[test]
    fn il_contesto_alto_e_caldo_nella_riga() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        p.casa().aggiorna_config(|c| c.claude.collegato = true);
        manda(&p, ev("a", TipoEvento::Stop));
        p.vista_claude(true);
        p.claude_contesto.borrow_mut().insert("a".into(), 81.0);
        p.aggiorna_vista_claude();
        let r = pannello.get_claude_righe().row_data(0).unwrap();
        assert_eq!((r.contesto.as_str(), r.contesto_caldo), ("81%", true));
        p.claude_contesto.borrow_mut().insert("a".into(), 80.0);
        p.aggiorna_vista_claude();
        let r = pannello.get_claude_righe().row_data(0).unwrap();
        assert_eq!((r.contesto.as_str(), r.contesto_caldo), ("80%", false));
    }

    fn visto(percentuale: f32, azzera_alle: i64, visto_alle: i64) -> casa_core::config::LimiteVisto {
        casa_core::config::LimiteVisto { percentuale, azzera_alle, visto_alle }
    }

    /// RC-56: dopo un riavvio i limiti mostrano l'ultimo valore salvato, attenuato e con «visto
    /// alle…», finché non arriva un dato nuovo; uno già azzerato non si mostra.
    #[test]
    fn dopo_il_riavvio_i_limiti_si_ricordano_attenuati() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        let ora = ora_adesso();
        let salvato = visto(87.0, ora + 3600, ora - 300);
        p.casa().aggiorna_config(|c| {
            c.claude.ultimo_cinque_ore = Some(salvato);
            // Questo si è già azzerato: non si mostra.
            c.claude.ultimo_sette_giorni = Some(visto(50.0, ora - 10, ora - 4000));
        });
        p.vista_claude(true);
        let c = pannello.get_claude_cinque_ore();
        assert!(c.presente && c.vecchio, "il valore salvato, attenuato");
        assert_eq!(c.testo.as_str(), "87%");
        assert!((c.frazione - 0.87).abs() < 1e-5);
        assert_eq!(c.azzera.as_str(), claude::testo_visto(&salvato, chrono::Utc::now(), &chrono::Local));
        assert!(c.azzera.starts_with("dal"), "{}", c.azzera);
        let s = pannello.get_claude_sette_giorni();
        assert!(!s.presente && !s.vecchio, "scaduto: non si mostra");
        // Gli avvisi non usano mai il valore ricordato: 87 % sopra la soglia dell'80 % non fa
        // nascere la voce delle 5 ore finché non arriva un dato fresco.
        assert_eq!(p.casa().con_config(|c| c.claude.soglia_limite), 80);
        p.claude_limiti_a(ora);
        assert!(p.attese.borrow().voce(claude::ID_LIMITE_5H).is_none(), "nessun avviso dal valore ricordato");
        // Una barra nuova per la sola finestra delle 5 ore la sostituisce: non è più attenuata.
        p.claude_barra(barra("a", Some((40.0, ora + 7200)), None));
        let c = pannello.get_claude_cinque_ore();
        assert!(c.presente && !c.vecchio);
        assert_eq!((c.testo.as_str(), c.azzera.starts_with("si azzera")), ("40%", true));
    }

    /// RC-56: il valore ricordato sparisce da solo quando si azzera, con la sveglia singola dei
    /// limiti (nessun timer nuovo).
    #[test]
    fn il_limite_ricordato_scade_con_la_sveglia_dei_limiti() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        let ora = ora_adesso();
        p.casa().aggiorna_config(|c| c.claude.ultimo_cinque_ore = Some(visto(87.0, ora + 600, ora - 300)));
        p.vista_claude(true);
        assert!(p.sveglia_limiti.running());
        assert_eq!(p.limiti_armati.get(), Some(ora + 600));
        // All'ora dell'azzeramento la vista si riscrive senza il valore.
        p.aggiorna_vista_claude_a(ora + 601);
        assert!(!pannello.get_claude_cinque_ore().presente);
    }

    /// RC-56: si scrive nel file di stato solo se il valore cambia e al massimo una volta al
    /// minuto, nel momento in cui il dato arriva (nessun timer).
    #[test]
    fn i_limiti_si_salvano_solo_se_cambiano_e_una_volta_al_minuto() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        let (fine5, fine7) = (ora + 3600, ora + 90_000);
        let salvato = || (p.casa().config().claude.ultimo_cinque_ore, p.casa().config().claude.ultimo_sette_giorni);
        assert_eq!(salvato(), (None, None));
        p.claude_barra_a(barra("a", Some((50.0, fine5)), Some((20.0, fine7))), ora);
        assert_eq!(salvato(), (Some(visto(50.0, fine5, ora)), Some(visto(20.0, fine7, ora))));
        // Il valore cambia dopo 10 secondi: troppo presto, resta quello di prima.
        p.claude_barra_a(barra("a", Some((51.0, fine5)), None), ora + 10);
        assert_eq!(salvato().0, Some(visto(50.0, fine5, ora)));
        // Dopo un minuto, lo stesso valore nuovo si salva (col momento in cui lo si è visto).
        p.claude_barra_a(barra("a", Some((51.0, fine5)), None), ora + 70);
        assert_eq!(salvato().0, Some(visto(51.0, fine5, ora + 70)));
        // Un valore uguale non riscrive nulla (il momento resta quello di quando è cambiato).
        p.claude_barra_a(barra("a", Some((51.2, fine5 + 3)), Some((20.0, fine7))), ora + 500);
        assert_eq!(salvato(), (Some(visto(51.0, fine5, ora + 70)), Some(visto(20.0, fine7, ora))));
        // Un limite già azzerato non si salva.
        p.claude_barra_a(barra("a", Some((99.0, ora + 100)), None), ora + 1000);
        assert_eq!(salvato().0, Some(visto(51.0, fine5, ora + 70)));
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

    /// RC-44: il colore caldo parte dalla soglia d'avviso dell'utente (80 di serie), non da un
    /// 85 % fisso: il pannello la riceve da Rust e la segue se cambia.
    #[test]
    fn la_soglia_d_avviso_arriva_al_pannello() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let pannello = con_pannello(&p);
        assert!((pannello.get_claude_soglia() - 0.80).abs() < 1e-6, "di serie 80");
        assert_eq!(p.cambia_soglia_limite("70"), "");
        assert!((pannello.get_claude_soglia() - 0.70).abs() < 1e-6, "dopo il cambio 70");
        // Fuori dai 50..100 si ferma ai bordi, come l'avviso.
        p.casa().aggiorna_config(|c| c.claude.soglia_limite = 30);
        p.claude_limiti_cambiati();
        assert!((pannello.get_claude_soglia() - 0.50).abs() < 1e-6);
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
    fn sveglia_singola_all_azzeramento_anche_a_vista_chiusa() {
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
        // Task 12: la sveglia serve anche a togliere la voce d'avviso di un limite, quindi non
        // dipende dalla vista: chiusa, resta armata (una sola, mai ripetuta).
        p.vista_claude(false);
        assert!(p.sveglia_limiti.running());
        assert_eq!(p.limiti_armati.get(), Some(ora + 300));
        p.vista_claude(true);
        assert!(p.sveglia_limiti.running());
        // Un limite che scade toglie l'anello e la sveglia.
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
        let dati = chrono::Utc::now() - chrono::Duration::minutes(5);
        let r = api_claude::Riepilogo { spesa_mese: api_claude::importo_da("41280"), aggiornato_a: Some(dati), ..Default::default() };
        p.api_arrivato(p.api_generazione.get(), "14:05".into(), Ok(r));
        assert_eq!(pannello.get_claude_spesa().as_str(), "412,80 $");
        let locale = dati.with_timezone(&chrono::Local);
        // Di oggi dice solo l'ora (a meno di una mezzanotte nel mezzo: allora dice «ieri»).
        let nota = pannello.get_claude_spesa_nota();
        assert!(nota.as_str() == format!("aggiornato alle {}", locale.format("%H:%M")) || nota.starts_with("aggiornato ieri alle "), "{nota}");
        // RC-32: dati di ieri dicono il giorno, mai un'ora che sembra di stamattina.
        let ieri = chrono::Utc::now() - chrono::Duration::hours(30);
        let r = api_claude::Riepilogo { spesa_mese: api_claude::importo_da("41280"), aggiornato_a: Some(ieri), ..Default::default() };
        p.api_arrivato(p.api_generazione.get(), "14:05".into(), Ok(r));
        assert_eq!(pannello.get_claude_spesa_nota().as_str(), format!("aggiornato {}", claude::quando_alle(ieri, chrono::Utc::now(), &chrono::Local)));
        assert!(!pannello.get_claude_spesa_nota().starts_with("aggiornato alle "));
        // La chiave tolta: torna la frase.
        p.api_togli_ui();
        p.vista_claude(false);
        p.vista_claude(true);
        assert_eq!(pannello.get_claude_spesa().as_str(), "");
        assert!(pannello.get_claude_spesa_nota().as_str().starts_with("Aggiungi una chiave"));
    }

    // ─── Avvisi (Task 12) ────────────────────────────────────────────────────

    fn ora_adesso() -> i64 {
        chrono::Local::now().timestamp()
    }

    fn ha_voce_claude(p: &Ponte, id: &str) -> bool {
        p.attese.borrow().voce(id).is_some()
    }

    #[test]
    fn limiti_avvisati_salvati_in_config() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        // Sotto la soglia (80): niente voce e niente salvato.
        p.claude_barra(barra("a", Some((50.0, ora + 600)), Some((30.0, ora + 90_000))));
        assert!(!ha_voce_claude(&p, "claude.limite.5h"));
        assert!(p.casa().config().claude.limiti_avvisati.is_empty());
        // Oltre: la voce di attenzione e la fine della finestra salvata, una finestra alla volta.
        p.claude_barra(barra("a", Some((82.0, ora + 600)), None));
        assert!(ha_voce_claude(&p, "claude.limite.5h"));
        assert!(!ha_voce_claude(&p, "claude.limite.7g"));
        let c = p.casa().config().claude;
        assert_eq!(c.limiti_avvisati.get("5h"), Some(&(ora + 600)));
        assert!(!c.limiti_avvisati.contains_key("7g"));
        let v = p.attese.borrow().voce("claude.limite.5h").cloned().unwrap();
        assert_eq!((v.livello, v.titolo.as_str()), (Livello::Attenzione, "Limite 5 ore all'82%"));
        p.claude_barra(barra("a", None, Some((90.0, ora + 90_000))));
        assert_eq!(p.casa().config().claude.limiti_avvisati.get("7g"), Some(&(ora + 90_000)));
    }

    #[test]
    fn un_avviso_di_limite_una_volta_sola_anche_dopo_un_riavvio() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        p.claude_barra(barra("a", Some((82.0, ora + 600)), None));
        assert!(ha_voce_claude(&p, "claude.limite.5h"));
        // «Ho capito»: via, e con altre barre della stessa finestra non torna.
        p.azione("claude.limite.5h", crate::attenzione::PULSANTE_HO_CAPITO);
        assert!(!ha_voce_claude(&p, "claude.limite.5h"));
        p.claude_barra(barra("a", Some((95.0, ora + 600)), None));
        assert!(!ha_voce_claude(&p, "claude.limite.5h"), "una volta per finestra");
        // Una finestra nuova avvisa di nuovo.
        p.claude_barra(barra("a", Some((85.0, ora + 18_600)), None));
        assert!(ha_voce_claude(&p, "claude.limite.5h"));
        // Riavvio: ponte nuovo, stessa configurazione (la finestra era già avvisata).
        let (_s2, q, _pillola2, _d2) = ponte_di_prova();
        q.casa().aggiorna_config(|c| {
            c.claude.limiti_avvisati.insert("7g".into(), ora + 90_000);
        });
        q.claude_barra(barra("a", None, Some((99.0, ora + 90_000))));
        assert!(!ha_voce_claude(&q, "claude.limite.7g"), "già avvisata prima del riavvio");
    }

    #[test]
    fn la_soglia_dei_limiti_e_quella_della_configurazione() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        p.casa().aggiorna_config(|c| c.claude.soglia_limite = 60);
        p.claude_barra(barra("a", Some((62.0, ora + 600)), None));
        assert!(ha_voce_claude(&p, "claude.limite.5h"));
    }

    #[test]
    fn niente_avvisi_di_limiti_scaduti_o_assenti() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.claude_barra(barra("a", Some((95.0, 1000)), Some((f32::NAN, FUTURO))));
        p.claude_barra(barra("a", None, None));
        assert!(!ha_voce_claude(&p, "claude.limite.5h") && !ha_voce_claude(&p, "claude.limite.7g"));
        assert!(p.casa().config().claude.limiti_avvisati.is_empty());
    }

    #[test]
    fn la_voce_del_limite_sparisce_quando_la_finestra_si_azzera() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        p.claude_barra(barra("a", Some((82.0, ora + 600)), Some((85.0, ora + 90_000))));
        assert!(ha_voce_claude(&p, "claude.limite.5h") && ha_voce_claude(&p, "claude.limite.7g"));
        // Passa l'azzeramento della finestra delle 5 ore: la sua voce va, quella dei 7 giorni resta.
        p.claude_limiti_a(ora + 601);
        assert!(!ha_voce_claude(&p, "claude.limite.5h"));
        assert!(ha_voce_claude(&p, "claude.limite.7g"));
        p.claude_limiti_a(ora + 90_001);
        assert!(!ha_voce_claude(&p, "claude.limite.7g"));
    }

    #[test]
    fn una_sola_sveglia_senza_timer_ripetuti() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        assert!(!p.sveglia_limiti.running(), "a riposo nessuna sveglia");
        // L'avviso deve poter togliere la voce anche a vista chiusa: la sveglia c'è comunque.
        p.claude_barra(barra("a", Some((82.0, ora + 600)), Some((20.0, ora + 90_000))));
        assert!(p.sveglia_limiti.running());
        assert_eq!(p.limiti_armati.get(), Some(ora + 600));
        // Scaduti tutti: niente più sveglia.
        p.claude_barra(barra("a", Some((10.0, 1000)), Some((20.0, 2000))));
        assert!(!p.sveglia_limiti.running());
    }

    fn riepilogo_con_spesa(spesa: &str, fresco: bool) -> api_claude::Riepilogo {
        api_claude::Riepilogo {
            spesa_mese: api_claude::importo_da(spesa),
            aggiornato_a: fresco.then(chrono::Utc::now),
            ..Default::default()
        }
    }

    fn arriva(p: &Ponte, r: api_claude::Riepilogo) {
        p.api_arrivato(p.api_generazione.get(), "14:05".into(), Ok(r));
    }

    fn voce_spesa(p: &Ponte) -> Option<String> {
        p.attese.borrow().voce("claude.spesa").map(|v| v.titolo.clone())
    }

    fn mese_corrente() -> String {
        chrono::Utc::now().format("%Y-%m").to_string()
    }

    #[test]
    fn avvisi_di_spesa_ottanta_e_cento_una_volta_per_mese() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        // Nessuna soglia: mai.
        arriva(&p, riepilogo_con_spesa("30000", true));
        assert_eq!(voce_spesa(&p), None);
        p.casa().aggiorna_config(|c| c.claude.soglia_spesa = Some(250.0));
        // Un mese vecchio nel file si pota quando si salva un avviso nuovo, quello corrente resta.
        p.casa().aggiorna_config(|c| {
            c.claude.spesa_avvisata.insert("2020-01".into(), 100);
        });
        // Sotto l'80 %: niente.
        arriva(&p, riepilogo_con_spesa("19000", true));
        assert_eq!(voce_spesa(&p), None);
        // 80 %: la voce e il gradino salvato per il mese.
        arriva(&p, riepilogo_con_spesa("20000", true));
        assert_eq!(voce_spesa(&p).as_deref(), Some("Spesa API all'80% della soglia (250,00 $)"));
        assert_eq!(p.casa().config().claude.spesa_avvisata.get(&mese_corrente()), Some(&80));
        assert!(!p.casa().config().claude.spesa_avvisata.contains_key("2020-01"), "il mese vecchio si pota");
        // Voce chiusa e riletta: non torna.
        p.azione("claude.spesa", crate::attenzione::PULSANTE_HO_CAPITO);
        arriva(&p, riepilogo_con_spesa("21000", true));
        assert_eq!(voce_spesa(&p), None, "una volta per gradino");
        // 100 %: il secondo gradino.
        arriva(&p, riepilogo_con_spesa("25000", true));
        assert_eq!(voce_spesa(&p).as_deref(), Some("Spesa API al 100% della soglia (250,00 $)"));
        assert_eq!(p.casa().config().claude.spesa_avvisata.get(&mese_corrente()), Some(&100));
    }

    #[test]
    fn niente_avvisi_di_spesa_con_dati_assenti_o_vecchi() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        p.casa().aggiorna_config(|c| c.claude.soglia_spesa = Some(100.0));
        // Dati senza data di aggiornamento: non si sa quanto sono vecchi.
        arriva(&p, riepilogo_con_spesa("20000", false));
        assert_eq!(voce_spesa(&p), None);
        // Una lettura fallita e una senza spesa.
        p.api_arrivato(p.api_generazione.get(), "14:05".into(), Err("rete".into()));
        arriva(&p, riepilogo_con_spesa("", true));
        assert_eq!(voce_spesa(&p), None);
        // Dati fermi al mese scorso (aggiornati prima dell'inizio di questo mese): niente.
        let mut vecchi = riepilogo_con_spesa("20000", true);
        vecchi.aggiornato_a = Some(chrono::Utc::now() - chrono::Duration::days(40));
        arriva(&p, vecchi);
        assert_eq!(voce_spesa(&p), None);
        assert!(p.casa().config().claude.spesa_avvisata.is_empty());
    }

    #[test]
    fn la_soglia_di_spesa_cambiata_vale_sui_dati_che_ci_sono() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        arriva(&p, riepilogo_con_spesa("20000", true));
        assert_eq!(voce_spesa(&p), None);
        assert_eq!(p.cambia_soglia_spesa("250"), "");
        assert_eq!(voce_spesa(&p).as_deref(), Some("Spesa API all'80% della soglia (250,00 $)"));
    }

    fn riepilogo_insolito(oggi: chrono::NaiveDate) -> api_claude::Riepilogo {
        let euro = |s: &str| api_claude::importo_da(s).unwrap();
        let mut giorni = vec![(oggi - chrono::Days::new(1), euro("4000"))];
        giorni.extend((2..=8).map(|n| (oggi - chrono::Days::new(n), euro("1000"))));
        api_claude::Riepilogo {
            aggiornato_a: Some(chrono::Utc::now()),
            spesa_mese: Some(euro("10000")),
            spesa_ieri: Some(euro("4000")),
            giorni,
            ..Default::default()
        }
    }

    #[test]
    fn spesa_insolita_una_volta_al_giorno() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        let oggi = chrono::Utc::now().date_naive();
        arriva(&p, riepilogo_insolito(oggi));
        let v = p.attese.borrow().voce("claude.spesa.insolita").cloned().expect("la voce");
        assert_eq!((v.livello, v.titolo.as_str()), (Livello::Informazione, "Spesa API insolita ieri: 40,00 $"));
        let ieri = (oggi - chrono::Days::new(1)).format("%Y-%m-%d").to_string();
        assert_eq!(p.casa().config().claude.insolita_avvisata.as_deref(), Some(ieri.as_str()));
        // Chiusa e riletta lo stesso giorno: non torna.
        p.azione("claude.spesa.insolita", crate::attenzione::PULSANTE_HO_CAPITO);
        arriva(&p, riepilogo_insolito(oggi));
        assert!(!ha_voce_claude(&p, "claude.spesa.insolita"), "una volta al giorno");
        // Dati non completi (senza data di aggiornamento): niente, e niente avviso salvato.
        let (_s2, q, _pillola2, _d2) = ponte_di_prova();
        q.api_salva_ui("sk-ant-admin01-di-prova-1a2b");
        let mut r = riepilogo_insolito(oggi);
        r.aggiornato_a = None;
        arriva(&q, r);
        assert!(!ha_voce_claude(&q, "claude.spesa.insolita"));
        assert_eq!(q.casa().config().claude.insolita_avvisata, None);
    }

    #[test]
    fn resets_at_che_oscilla_non_riavvisa_ne_riscrive() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        p.claude_barra(barra("a", Some((82.0, ora + 600)), None));
        p.azione("claude.limite.5h", crate::attenzione::PULSANTE_HO_CAPITO);
        // La stessa finestra con la fine spostata di qualche secondo: né voce né scrittura.
        p.claude_barra(barra("a", Some((85.0, ora + 603)), None));
        assert!(!ha_voce_claude(&p, "claude.limite.5h"));
        assert_eq!(p.casa().config().claude.limiti_avvisati.get("5h"), Some(&(ora + 600)), "nessuna scrittura");
    }

    #[test]
    fn soglia_dei_limiti_del_file_fuori_scala_si_riporta_a_50_100() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let ora = ora_adesso();
        p.casa().aggiorna_config(|c| c.claude.soglia_limite = 0);
        p.claude_barra(barra("a", Some((30.0, ora + 600)), None));
        assert!(!ha_voce_claude(&p, "claude.limite.5h"), "0 vale 50: il 30 % non avvisa");
        p.claude_barra(barra("a", Some((55.0, ora + 600)), None));
        assert!(ha_voce_claude(&p, "claude.limite.5h"));
    }

    #[test]
    fn collega_soglie_parte_dalla_configurazione_e_salva_dai_campi() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.casa().aggiorna_config(|c| {
            c.claude.soglia_limite = 70;
            c.claude.soglia_spesa = Some(99.5);
        });
        let f = crate::FinestraImpostazioni::new().unwrap();
        p.collega_soglie(&f);
        assert_eq!(f.get_soglia_limite().as_str(), "70");
        assert_eq!(f.get_soglia_spesa().as_str(), "99,5");
        // I callback veri: rifiutano senza salvare, accettano salvando.
        assert!(!f.invoke_soglia_limite_scritta("abc".into()).is_empty());
        assert_eq!(p.casa().config().claude.soglia_limite, 70);
        assert_eq!(f.invoke_soglia_limite_scritta("90".into()).as_str(), "");
        assert_eq!(p.casa().config().claude.soglia_limite, 90);
        assert!(!f.invoke_soglia_spesa_scritta("1.000".into()).is_empty());
        assert_eq!(p.casa().config().claude.soglia_spesa, Some(99.5));
        assert_eq!(f.invoke_soglia_spesa_scritta("".into()).as_str(), "");
        assert_eq!(p.casa().config().claude.soglia_spesa, None);
    }

    #[test]
    fn nei_test_nessuna_notifica_vera() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        let prima = crate::sistema::notifiche::CHIAMATE_PROVA.with(std::cell::Cell::get);
        p.claude_barra(barra("a", Some((95.0, ora_adesso() + 600)), None));
        // La voce di attenzione ha chiesto il toast: è stato solo contato, non mostrato.
        assert!(crate::sistema::notifiche::CHIAMATE_PROVA.with(std::cell::Cell::get) > prima);
    }

    #[test]
    fn soglie_scritte_validate_e_salvate() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        // Il limite: intero da 50 a 100.
        assert_eq!(p.cambia_soglia_limite("90"), "");
        assert_eq!(p.casa().config().claude.soglia_limite, 90);
        for male in ["abc", "49", "101", "", "80,5", "-3"] {
            assert!(!p.cambia_soglia_limite(male).is_empty(), "{male:?} si rifiuta");
        }
        assert_eq!(p.casa().config().claude.soglia_limite, 90, "un valore rifiutato non si salva");
        assert_eq!(p.cambia_soglia_limite(" 50 "), "");
        assert_eq!(p.cambia_soglia_limite("100"), "");
        assert_eq!(p.casa().config().claude.soglia_limite, 100);
        // La spesa: vuota = nessuna, altrimenti positiva con virgola o punto.
        assert_eq!(p.cambia_soglia_spesa("250"), "");
        assert_eq!(p.casa().config().claude.soglia_spesa, Some(250.0));
        assert_eq!(p.cambia_soglia_spesa("99,5"), "");
        assert_eq!(p.casa().config().claude.soglia_spesa, Some(99.5));
        assert_eq!(p.cambia_soglia_spesa("12.25"), "");
        assert_eq!(p.casa().config().claude.soglia_spesa, Some(12.25));
        for male in ["abc", "0", "-5", "inf", "NaN", "1e400"] {
            assert!(!p.cambia_soglia_spesa(male).is_empty(), "{male:?} si rifiuta");
        }
        assert_eq!(p.casa().config().claude.soglia_spesa, Some(12.25));
        assert_eq!(p.cambia_soglia_spesa("  "), "");
        assert_eq!(p.casa().config().claude.soglia_spesa, None, "vuota: nessuna soglia");
    }
}
