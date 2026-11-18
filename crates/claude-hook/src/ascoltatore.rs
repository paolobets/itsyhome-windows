//! L'ascoltatore locale degli hook di Claude Code (spec M3 §6.2): solo 127.0.0.1, un thread per
//! connessione, regole di rifiuto nell'ordine della spec e tutte **prima** di leggere il corpo
//! (salvo lo scarto di un corpo troppo grande), nessuna intestazione CORS.
//!
//! Chi ascolta non decide nulla: gli eventi e i permessi passano al [`Gestore`]. Le richieste
//! rifiutate non lo vedono mai; l'ascoltatore le conta (`rifiutate`).

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::codice::CodiceHook;
use crate::mittente;
use crate::{PERCORSO_BARRA, PERCORSO_EVENTO};

/// Quanto si aspetta una decisione prima di rispondere `{}`: sotto i 120 s dell'hook di Claude.
pub const ATTESA_MASSIMA: Duration = Duration::from_secs(115);
/// Corpo ammesso.
pub const CORPO_MASSIMO: usize = 256 * 1024;
/// Quanto di un corpo troppo grande si legge e si butta prima del `413`.
pub const SCARTO_MASSIMO: usize = 1024 * 1024;
/// Intestazioni: byte (e al più 32 righe).
pub const INTESTAZIONI_MASSIME: usize = 8 * 1024;
/// Tempo per leggere intestazioni e corpo.
pub const TEMPO_LETTURA: Duration = Duration::from_secs(2);
/// Connessioni aperte insieme; la successiva riceve `503`.
pub const CONNESSIONI_MASSIME: usize = 16;

const RIGHE_MASSIME: usize = 32;
const SESSIONI_RICORDATE: usize = 16;
const PASSO_ATTESA: Duration = Duration::from_millis(250);
const MESSAGGIO_NEGATO: &str = "Negato dall'utente dall'assistente ItsyHome.";

/// A quale percorso è arrivata la richiesta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Percorso {
    Evento,
    Barra,
}

/// Una richiesta accettata, passata al gestore.
#[derive(Debug, Clone)]
pub struct Arrivo {
    pub percorso: Percorso,
    pub corpo: Vec<u8>,
    pub porta_mittente: u16,
    /// Il PID del mittente, cercato solo alla prima richiesta di un `session_id` nuovo (e solo se
    /// la ricerca riesce); altrimenti `None`. Sul percorso della barra è sempre `None`.
    pub pid_mittente: Option<u32>,
}

/// Risposta a un permesso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisione {
    Consenti,
    Nega,
    /// Nessuna decisione: Claude prosegue con il suo flusso normale.
    Nessuna,
}

/// Un permesso in attesa: il gestore manda la decisione sul canale.
pub struct AttesaPermesso {
    pub id: u64,
    pub decisione: std::sync::mpsc::Receiver<Decisione>,
}

/// Chi riceve ciò che l'ascoltatore ha accettato.
pub trait Gestore: Send + Sync + 'static {
    /// Un evento diverso da `PermissionRequest`: l'ascoltatore ha già risposto `200 {}`.
    fn evento(&self, a: Arrivo);
    /// Un `PermissionRequest`. `None` = rispondi subito `{}`.
    fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso>;
    /// Il mittente ha chiuso o l'attesa è finita senza decisione utile: il permesso `id` non serve più.
    fn abbandonato(&self, id: u64);
    /// Dati della barra di stato: già risposto `200 {}`.
    fn barra(&self, a: Arrivo);
}

/// Come ascoltare.
pub struct ConfigAscolto {
    /// 0 = una porta libera a caso; altrimenti esattamente quella.
    pub porta: u16,
    pub codice: CodiceHook,
    pub attesa_massima: Duration,
}

#[derive(Debug)]
pub enum ErroreAvvio {
    /// La porta chiesta è già presa: non se ne prova un'altra.
    PortaOccupata,
    Altro(String),
}

struct Contesto {
    codice: CodiceHook,
    porta: u16,
    attesa_massima: Duration,
    gestore: Arc<dyn Gestore>,
    rifiutate: AtomicU64,
    sessioni: SessioniViste,
    fermo: AtomicBool,
    connessioni: AtomicUsize,
}

/// L'ascoltatore in funzione. `Drop` lo ferma e ne raccoglie il thread.
pub struct Ascoltatore {
    contesto: Arc<Contesto>,
    thread: Option<JoinHandle<()>>,
}

impl Ascoltatore {
    /// La porta su cui si ascolta (utile con `porta: 0`).
    pub fn porta(&self) -> u16 {
        self.contesto.porta
    }

    /// Quante richieste sono state rifiutate finora (ogni risposta non 2xx e ogni chiusura senza
    /// risposta che non ha chiamato il gestore).
    pub fn rifiutate(&self) -> u64 {
        self.contesto.rifiutate.load(Ordering::Relaxed)
    }
}

impl Drop for Ascoltatore {
    fn drop(&mut self) {
        self.contesto.fermo.store(true, Ordering::SeqCst);
        // `accept` si sveglia con una connessione a noi stessi.
        let me = SocketAddr::from((Ipv4Addr::LOCALHOST, self.contesto.porta));
        let svegliato = TcpStream::connect_timeout(&me, Duration::from_secs(1)).is_ok();
        if let Some(t) = self.thread.take() {
            // Senza risveglio il thread resterebbe in `accept`: meglio lasciarlo che bloccarsi.
            if svegliato {
                let _ = t.join();
            }
        }
    }
}

/// Mette in ascolto su 127.0.0.1.
pub fn avvia(c: ConfigAscolto, g: Arc<dyn Gestore>) -> Result<Ascoltatore, ErroreAvvio> {
    let ascolto = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, c.porta))).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            ErroreAvvio::PortaOccupata
        } else {
            ErroreAvvio::Altro(e.to_string())
        }
    })?;
    let porta = ascolto.local_addr().map_err(|e| ErroreAvvio::Altro(e.to_string()))?.port();
    let contesto = Arc::new(Contesto {
        codice: c.codice,
        porta,
        attesa_massima: c.attesa_massima,
        gestore: g,
        rifiutate: AtomicU64::new(0),
        sessioni: SessioniViste::default(),
        fermo: AtomicBool::new(false),
        connessioni: AtomicUsize::new(0),
    });
    let c2 = Arc::clone(&contesto);
    let thread = thread::Builder::new()
        .name("claude-ascolto".into())
        .spawn(move || accetta(ascolto, c2))
        .map_err(|e| ErroreAvvio::Altro(e.to_string()))?;
    Ok(Ascoltatore { contesto, thread: Some(thread) })
}

/// Conta una connessione aperta finché vive.
struct Guardiano(Arc<Contesto>);

impl Drop for Guardiano {
    fn drop(&mut self) {
        self.0.connessioni.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accetta(ascolto: TcpListener, ctx: Arc<Contesto>) {
    for flusso in ascolto.incoming() {
        if ctx.fermo.load(Ordering::SeqCst) {
            break;
        }
        let Ok(flusso) = flusso else {
            // Un errore di `accept` (es. troppe risorse): un attimo di respiro, non un giro a vuoto.
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        let prima = ctx.connessioni.fetch_add(1, Ordering::SeqCst);
        let guardiano = Guardiano(Arc::clone(&ctx));
        if prima >= CONNESSIONI_MASSIME {
            ctx.rifiutate.fetch_add(1, Ordering::Relaxed);
            rifiuto_senza_lettura(flusso);
            continue;
        }
        let c2 = Arc::clone(&ctx);
        let _ = thread::Builder::new().name("claude-conn".into()).spawn(move || {
            let _guardiano = guardiano;
            servi(flusso, &c2);
        });
    }
}

/// `503` senza leggere la richiesta. Si svuota ciò che è già arrivato, senza aspettare, perché
/// chiudere con dati non letti manda un RST che può cancellare la risposta nel client.
fn rifiuto_senza_lettura(mut flusso: TcpStream) {
    let _ = flusso.set_write_timeout(Some(Duration::from_millis(200)));
    scrivi(&mut flusso, 503, "");
    if flusso.set_nonblocking(true).is_ok() {
        let mut b = [0u8; 4096];
        let mut totale = 0;
        while totale < 64 * 1024 {
            match flusso.read(&mut b) {
                Ok(n) if n > 0 => totale += n,
                _ => break,
            }
        }
    }
}

/// Come finisce una richiesta rifiutata.
enum Fine {
    /// Risposta con quel codice.
    Rifiuto(u16),
    /// Chiusura senza risposta (lenta, interrotta).
    Chiudi,
}

#[derive(Deserialize)]
struct Testata {
    hook_event_name: String,
    session_id: Option<serde_json::Value>,
}

/// Le sole intestazioni che servono, già copiate.
struct Testa {
    metodo: String,
    percorso: String,
    host: Vec<Option<String>>,
    origin: bool,
    tipo: Vec<Option<String>>,
    autorizzazione: Vec<Option<String>>,
    lunghezza: Vec<Option<String>>,
    codifica: bool,
}

fn servi(mut flusso: TcpStream, ctx: &Contesto) {
    let _ = flusso.set_write_timeout(Some(TEMPO_LETTURA));
    let porta_mittente = flusso.peer_addr().map(|a| a.port()).unwrap_or(0);
    match istruisci(&mut flusso, ctx, porta_mittente) {
        Ok(()) => {}
        Err(Fine::Rifiuto(codice)) => {
            ctx.rifiutate.fetch_add(1, Ordering::Relaxed);
            scrivi(&mut flusso, codice, "");
            svuota_residuo(&mut flusso);
        }
        Err(Fine::Chiudi) => {
            ctx.rifiutate.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Dopo la risposta, un po' di ciò che il client ha ancora in volo viene letto e buttato (breve e
/// limitato), per non chiudere con dati non letti (RST).
fn svuota_residuo(flusso: &mut TcpStream) {
    let _ = flusso.set_read_timeout(Some(Duration::from_millis(100)));
    let mut b = [0u8; 4096];
    let mut totale = 0;
    while totale < 64 * 1024 {
        match flusso.read(&mut b) {
            Ok(n) if n > 0 => totale += n,
            _ => break,
        }
    }
}

/// Una lettura entro la scadenza comune a intestazioni e corpo.
fn leggi(flusso: &mut TcpStream, buf: &mut [u8], fine: Instant) -> Result<usize, Fine> {
    loop {
        let resto = fine.saturating_duration_since(Instant::now());
        if resto.is_zero() {
            return Err(Fine::Chiudi);
        }
        flusso.set_read_timeout(Some(resto)).map_err(|_| Fine::Chiudi)?;
        match flusso.read(buf) {
            Ok(0) => return Err(Fine::Chiudi),
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Fine::Chiudi),
        }
    }
}

/// Un solo valore per l'intestazione: assente o doppia non vale.
fn unico(valori: &[Option<String>]) -> Option<&str> {
    match valori {
        [Some(v)] => Some(v.as_str()),
        _ => None,
    }
}

fn tipo_json(v: &str) -> bool {
    let mut parti = v.split(';').map(str::trim);
    if !parti.next().is_some_and(|t| t.eq_ignore_ascii_case("application/json")) {
        return false;
    }
    match (parti.next(), parti.next()) {
        (None, _) => true,
        (Some(p), None) => p.eq_ignore_ascii_case("charset=utf-8"),
        _ => false,
    }
}

fn istruisci(flusso: &mut TcpStream, ctx: &Contesto, porta_mittente: u16) -> Result<(), Fine> {
    let fine = Instant::now() + TEMPO_LETTURA;

    // 1. Intestazioni, in un buffer di 8 KB.
    let mut buf = [0u8; INTESTAZIONI_MASSIME];
    let mut n = 0usize;
    let (testa, lunghezza_testa) = loop {
        let esito = {
            let mut righe = [httparse::EMPTY_HEADER; RIGHE_MASSIME];
            let mut r = httparse::Request::new(&mut righe);
            match r.parse(&buf[..n]) {
                Ok(httparse::Status::Complete(l)) => {
                    let testo = |v: &[u8]| std::str::from_utf8(v).ok().map(|s| s.trim().to_string());
                    let mut t = Testa {
                        metodo: r.method.unwrap_or("").to_string(),
                        percorso: r.path.unwrap_or("").to_string(),
                        host: Vec::new(),
                        origin: false,
                        tipo: Vec::new(),
                        autorizzazione: Vec::new(),
                        lunghezza: Vec::new(),
                        codifica: false,
                    };
                    for h in r.headers.iter() {
                        let nome = h.name;
                        if nome.eq_ignore_ascii_case("host") {
                            t.host.push(testo(h.value));
                        } else if nome.eq_ignore_ascii_case("origin") {
                            t.origin = true;
                        } else if nome.eq_ignore_ascii_case("content-type") {
                            t.tipo.push(testo(h.value));
                        } else if nome.eq_ignore_ascii_case("authorization") {
                            t.autorizzazione.push(testo(h.value));
                        } else if nome.eq_ignore_ascii_case("content-length") {
                            t.lunghezza.push(testo(h.value));
                        } else if nome.eq_ignore_ascii_case("transfer-encoding") {
                            t.codifica = true;
                        }
                    }
                    Some((t, l))
                }
                Ok(httparse::Status::Partial) => {
                    if n >= buf.len() {
                        return Err(Fine::Rifiuto(431));
                    }
                    None
                }
                Err(httparse::Error::TooManyHeaders) => return Err(Fine::Rifiuto(431)),
                Err(_) => return Err(Fine::Rifiuto(400)),
            }
        };
        if let Some(e) = esito {
            break e;
        }
        n += leggi(flusso, &mut buf[n..], fine)?;
    };

    // 2. Le regole, nell'ordine della spec, senza toccare il corpo.
    if testa.metodo != "POST" {
        return Err(Fine::Rifiuto(405));
    }
    if testa.origin {
        return Err(Fine::Rifiuto(403));
    }
    if unico(&testa.host) != Some(format!("127.0.0.1:{}", ctx.porta).as_str()) {
        return Err(Fine::Rifiuto(403));
    }
    if !unico(&testa.tipo).is_some_and(tipo_json) {
        return Err(Fine::Rifiuto(415));
    }
    if !unico(&testa.autorizzazione).is_some_and(|a| ctx.codice.verifica(a)) {
        return Err(Fine::Rifiuto(401));
    }
    if testa.codifica || testa.lunghezza.is_empty() {
        return Err(Fine::Rifiuto(411));
    }
    let lunghezza: usize = match unico(&testa.lunghezza).map(|v| v.parse()) {
        Some(Ok(l)) => l,
        _ => return Err(Fine::Rifiuto(400)),
    };

    let mut corpo: Vec<u8> = buf[lunghezza_testa..n].to_vec();
    if lunghezza > CORPO_MASSIMO {
        // Letto e buttato (fino a 1 MiB, entro il tempo), poi 413.
        let da_buttare = lunghezza.min(SCARTO_MASSIMO);
        let mut buttati = corpo.len();
        let mut pezzo = [0u8; 8192];
        while buttati < da_buttare {
            match leggi(flusso, &mut pezzo, fine) {
                Ok(k) => buttati += k,
                Err(_) => break,
            }
        }
        return Err(Fine::Rifiuto(413));
    }

    let percorso = testa.percorso.split('?').next().unwrap_or("");
    let percorso = if percorso == PERCORSO_EVENTO {
        Percorso::Evento
    } else if percorso == PERCORSO_BARRA {
        Percorso::Barra
    } else {
        return Err(Fine::Rifiuto(404));
    };

    // 3. Il corpo.
    corpo.truncate(lunghezza);
    let mut pezzo = [0u8; 8192];
    while corpo.len() < lunghezza {
        let k = leggi(flusso, &mut pezzo, fine)?;
        let manca = lunghezza - corpo.len();
        corpo.extend_from_slice(&pezzo[..k.min(manca)]);
    }

    // 4. Da qui in poi la richiesta è accettata.
    match percorso {
        Percorso::Barra => {
            scrivi(flusso, 200, "{}");
            ctx.gestore.barra(Arrivo {
                percorso,
                corpo,
                porta_mittente,
                pid_mittente: None,
            });
            Ok(())
        }
        Percorso::Evento => {
            let t: Testata = serde_json::from_slice(&corpo).map_err(|_| Fine::Rifiuto(400))?;
            let sessione = t.session_id.as_ref().and_then(|v| v.as_str());
            // Una sola ricerca per sessione nuova, prima di rispondere (il mittente è ancora lì).
            let pid_mittente = match sessione_valida(sessione) {
                Some(id) if ctx.sessioni.nuova(id) => mittente::pid_mittente(porta_mittente, ctx.porta),
                _ => None,
            };
            let arrivo = Arrivo { percorso, corpo, porta_mittente, pid_mittente };
            if t.hook_event_name == "PermissionRequest" {
                permesso(flusso, ctx, arrivo);
            } else {
                scrivi(flusso, 200, "{}");
                ctx.gestore.evento(arrivo);
            }
            Ok(())
        }
    }
}

/// `true` se il mittente ha chiuso la connessione (`peek` non bloccante, socket non bloccante).
fn chiusa(flusso: &TcpStream) -> bool {
    let mut b = [0u8; 1];
    match flusso.peek(&mut b) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => false,
        Err(_) => true,
    }
}

fn permesso(flusso: &mut TcpStream, ctx: &Contesto, arrivo: Arrivo) {
    let Some(attesa) = ctx.gestore.permesso(arrivo) else {
        scrivi(flusso, 200, "{}");
        return;
    };
    let id = attesa.id;
    if flusso.set_nonblocking(true).is_err() {
        ctx.gestore.abbandonato(id);
        return;
    }
    let inizio = Instant::now();
    // Cosa rispondere quando il ciclo finisce; `None` = niente (mittente sparito o ascolto fermo).
    let risposta: Option<String> = loop {
        if ctx.fermo.load(Ordering::SeqCst) || chiusa(flusso) {
            break None;
        }
        let resto = ctx.attesa_massima.saturating_sub(inizio.elapsed());
        if resto.is_zero() {
            break Some("{}".to_string());
        }
        match attesa.decisione.recv_timeout(PASSO_ATTESA.min(resto)) {
            Ok(Decisione::Consenti) => return rispondi_decisione(flusso, "allow"),
            Ok(Decisione::Nega) => return rispondi_decisione(flusso, "deny"),
            Ok(Decisione::Nessuna) | Err(RecvTimeoutError::Disconnected) => {
                break Some("{}".to_string())
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    };
    if let Some(corpo) = risposta {
        let _ = flusso.set_nonblocking(false);
        scrivi(flusso, 200, &corpo);
    }
    ctx.gestore.abbandonato(id);
}

fn rispondi_decisione(flusso: &mut TcpStream, comportamento: &str) {
    // Testo scritto a mano: l'ordine dei campi è quello della documentazione di Claude Code.
    let decisione = if comportamento == "deny" {
        let messaggio = serde_json::Value::from(MESSAGGIO_NEGATO);
        format!(r#"{{"behavior":"deny","message":{messaggio}}}"#)
    } else {
        format!(r#"{{"behavior":"{comportamento}"}}"#)
    };
    let corpo = format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{decisione}}}}}"#
    );
    let _ = flusso.set_nonblocking(false);
    scrivi(flusso, 200, &corpo);
}

/// Scrive la risposta e chiude il lato di scrittura. Mai intestazioni CORS.
fn scrivi(flusso: &mut TcpStream, codice: u16, corpo: &str) {
    let motivo = match codice {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let allow = if codice == 405 { "Allow: POST\r\n" } else { "" };
    let tipo = if corpo.is_empty() { "" } else { "Content-Type: application/json\r\n" };
    let testo = format!(
        "HTTP/1.1 {codice} {motivo}\r\n{allow}{tipo}Content-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
    let _ = flusso.write_all(testo.as_bytes());
    let _ = flusso.flush();
    let _ = flusso.shutdown(Shutdown::Write);
}

/// I `session_id` già visti (al più 16: quando è pieno si scarta il più vecchio).
#[derive(Default)]
struct SessioniViste(Mutex<VecDeque<String>>);

impl SessioniViste {
    /// `true` se `id` non c'era (e ora è ricordato).
    fn nuova(&self, id: &str) -> bool {
        let mut v = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if v.iter().any(|s| s == id) {
            return false;
        }
        if v.len() >= SESSIONI_RICORDATE {
            v.pop_front();
        }
        v.push_back(id.to_string());
        true
    }
}

/// Un `session_id` si accetta se è di 1..=128 caratteri `[A-Za-z0-9_-]`.
fn sessione_valida(s: Option<&str>) -> Option<&str> {
    s.filter(|s| {
        (1..=128).contains(&s.len())
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Shutdown, SocketAddr, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;
    use std::time::Instant;

    use crate::codice::CodiceHook;

    #[derive(Clone, Copy)]
    enum Modo {
        /// `permesso` risponde `None`.
        Subito,
        /// Decide subito.
        Decidi(Decisione),
        /// Non decide mai (tiene vivo il canale).
        Attendi,
    }

    struct Registro {
        modo: Mutex<Modo>,
        eventi: Mutex<Vec<Arrivo>>,
        permessi: Mutex<Vec<Arrivo>>,
        abbandonati: Mutex<Vec<u64>>,
        barre: Mutex<Vec<Arrivo>>,
        mittenti: Mutex<Vec<Sender<Decisione>>>,
        prossimo_id: AtomicUsize,
        evento_dorme: Duration,
    }

    impl Registro {
        fn nuovo(modo: Modo) -> Arc<Registro> {
            Arc::new(Registro {
                modo: Mutex::new(modo),
                eventi: Mutex::new(Vec::new()),
                permessi: Mutex::new(Vec::new()),
                abbandonati: Mutex::new(Vec::new()),
                barre: Mutex::new(Vec::new()),
                mittenti: Mutex::new(Vec::new()),
                prossimo_id: AtomicUsize::new(1),
                evento_dorme: Duration::ZERO,
            })
        }
        fn chiamate(&self) -> usize {
            self.eventi.lock().unwrap().len()
                + self.permessi.lock().unwrap().len()
                + self.abbandonati.lock().unwrap().len()
                + self.barre.lock().unwrap().len()
        }
    }

    impl Gestore for Registro {
        fn evento(&self, a: Arrivo) {
            std::thread::sleep(self.evento_dorme);
            self.eventi.lock().unwrap().push(a);
        }
        fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso> {
            self.permessi.lock().unwrap().push(a);
            let modo = *self.modo.lock().unwrap();
            if matches!(modo, Modo::Subito) {
                return None;
            }
            let id = self.prossimo_id.fetch_add(1, Ordering::SeqCst) as u64;
            let (tx, rx) = std::sync::mpsc::channel();
            if let Modo::Decidi(d) = modo {
                tx.send(d).unwrap();
            }
            self.mittenti.lock().unwrap().push(tx);
            Some(AttesaPermesso { id, decisione: rx })
        }
        fn abbandonato(&self, id: u64) {
            self.abbandonati.lock().unwrap().push(id);
        }
        fn barra(&self, a: Arrivo) {
            self.barre.lock().unwrap().push(a);
        }
    }

    fn avvia_prova(g: &Arc<Registro>, attesa: Duration) -> (Ascoltatore, CodiceHook, u16) {
        let codice = CodiceHook::nuovo().unwrap();
        let a = avvia(
            ConfigAscolto { porta: 0, codice: codice.clone(), attesa_massima: attesa },
            g.clone() as Arc<dyn Gestore>,
        )
        .expect("avvio");
        let porta = a.porta();
        (a, codice, porta)
    }

    /// Una richiesta HTTP costruita a pezzi, per cambiare una cosa alla volta.
    struct Rich {
        metodo: String,
        percorso: String,
        intestazioni: Vec<(String, String)>,
        corpo: Vec<u8>,
    }

    impl Rich {
        fn valida(porta: u16, codice: &CodiceHook, percorso: &str, corpo: &[u8]) -> Rich {
            Rich {
                metodo: "POST".into(),
                percorso: percorso.into(),
                intestazioni: vec![
                    ("Host".into(), format!("127.0.0.1:{porta}")),
                    ("Content-Type".into(), "application/json".into()),
                    ("Authorization".into(), format!("Bearer {}", codice.come_testo())),
                    ("Content-Length".into(), corpo.len().to_string()),
                ],
                corpo: corpo.to_vec(),
            }
        }
        fn senza(mut self, nome: &str) -> Rich {
            self.intestazioni.retain(|(n, _)| !n.eq_ignore_ascii_case(nome));
            self
        }
        fn con(self, nome: &str, valore: &str) -> Rich {
            let mut r = self.senza(nome);
            r.intestazioni.push((nome.into(), valore.into()));
            r
        }
        fn metodo(mut self, m: &str) -> Rich {
            self.metodo = m.into();
            self
        }
        fn byte(&self) -> Vec<u8> {
            let mut v = format!("{} {} HTTP/1.1\r\n", self.metodo, self.percorso).into_bytes();
            for (n, val) in &self.intestazioni {
                v.extend_from_slice(format!("{n}: {val}\r\n").as_bytes());
            }
            v.extend_from_slice(b"\r\n");
            v.extend_from_slice(&self.corpo);
            v
        }
    }

    fn collega(porta: u16) -> TcpStream {
        let s = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], porta))).expect("connessione");
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        s
    }

    fn leggi_tutto(s: &mut TcpStream) -> String {
        let mut v = Vec::new();
        let _ = s.read_to_end(&mut v);
        String::from_utf8_lossy(&v).into_owned()
    }

    fn invia(porta: u16, r: &Rich) -> String {
        let mut s = collega(porta);
        s.write_all(&r.byte()).unwrap();
        leggi_tutto(&mut s)
    }

    fn stato(risposta: &str) -> u16 {
        risposta.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
    }

    fn corpo_di(risposta: &str) -> &str {
        risposta.split_once("\r\n\r\n").map(|(_, c)| c).unwrap_or("")
    }

    const EVENTO: &[u8] = br#"{"hook_event_name":"PreToolUse","session_id":"s1"}"#;
    const PERMESSO: &[u8] =
        br#"{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"Bash"}"#;

    fn aspetta(mut f: impl FnMut() -> bool, secondi: u64) -> bool {
        let fine = Instant::now() + Duration::from_secs(secondi);
        while Instant::now() < fine {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        f()
    }

    /// Nessuna chiamata al gestore, nemmeno dopo la risposta (gli eventi lo chiamano dopo).
    fn nessuna_chiamata(g: &Registro) {
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(g.chiamate(), 0, "il gestore non doveva essere chiamato");
    }

    #[test]
    fn ascolta_solo_su_127_0_0_1() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, _c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let s = collega(porta);
        assert_eq!(s.peer_addr().unwrap().ip().to_string(), "127.0.0.1");
        assert_eq!(s.local_addr().unwrap().ip().to_string(), "127.0.0.1");
        let v6 = TcpStream::connect_timeout(
            &SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, porta)),
            Duration::from_secs(2),
        );
        assert!(v6.is_err(), "su [::1] non si deve poter entrare");
    }

    #[test]
    fn porta_occupata_errore_esplicito() {
        let occupata = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = occupata.local_addr().unwrap().port();
        let g = Registro::nuovo(Modo::Subito);
        let esito = avvia(
            ConfigAscolto {
                porta,
                codice: CodiceHook::nuovo().unwrap(),
                attesa_massima: ATTESA_MASSIMA,
            },
            g as Arc<dyn Gestore>,
        );
        assert!(matches!(esito, Err(ErroreAvvio::PortaOccupata)));
    }

    #[test]
    fn richiesta_valida_200_e_gestore_chiamato() {
        let g = Registro::nuovo(Modo::Subito);
        let (a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO));
        assert_eq!(stato(&r), 200);
        assert_eq!(corpo_di(&r), "{}");
        assert!(aspetta(|| g.eventi.lock().unwrap().len() == 1, 3));
        let e = &g.eventi.lock().unwrap()[0];
        assert_eq!(e.percorso, Percorso::Evento);
        assert_eq!(e.corpo, EVENTO);
        assert_eq!(a.rifiutate(), 0);
    }

    #[test]
    fn barra_risponde_e_poi_chiama_il_gestore() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_BARRA, br#"{"x":1}"#));
        assert_eq!(stato(&r), 200);
        assert!(aspetta(|| g.barre.lock().unwrap().len() == 1, 3));
        let b = &g.barre.lock().unwrap()[0];
        assert_eq!(b.percorso, Percorso::Barra);
        assert_eq!(b.pid_mittente, None);
        assert!(g.eventi.lock().unwrap().is_empty());
    }

    #[test]
    fn rifiuta_origin() {
        let g = Registro::nuovo(Modo::Subito);
        let (a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(
            porta,
            &Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO).con("Origin", "http://example.com"),
        );
        assert_eq!(stato(&r), 403);
        nessuna_chiamata(&g);
        assert_eq!(a.rifiutate(), 1);
    }

    #[test]
    fn rifiuta_host_diverso() {
        let g = Registro::nuovo(Modo::Subito);
        let (a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let base = || Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO);
        assert_eq!(stato(&invia(porta, &base().con("Host", &format!("localhost:{porta}")))), 403);
        assert_eq!(
            stato(&invia(porta, &base().con("Host", &format!("127.0.0.1:{}", porta ^ 1)))),
            403
        );
        assert_eq!(stato(&invia(porta, &base().con("Host", "127.0.0.1"))), 403);
        assert_eq!(stato(&invia(porta, &base().senza("Host"))), 403);
        nessuna_chiamata(&g);
        assert_eq!(a.rifiutate(), 4);
    }

    #[test]
    fn rifiuta_options_e_get() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        for m in ["OPTIONS", "GET"] {
            let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, b"").metodo(m));
            assert_eq!(stato(&r), 405, "{m}");
            assert!(!r.to_ascii_lowercase().contains("access-control"), "{m}: {r}");
        }
        nessuna_chiamata(&g);
    }

    #[test]
    fn rifiuta_codice_mancante_o_errato() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let base = || Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO);
        assert_eq!(stato(&invia(porta, &base().senza("Authorization"))), 401);
        let altro = CodiceHook::nuovo().unwrap().come_testo();
        assert_eq!(stato(&invia(porta, &base().con("Authorization", &format!("Bearer {altro}")))), 401);
        assert_eq!(stato(&invia(porta, &base().con("Authorization", "Bearer"))), 401);
        nessuna_chiamata(&g);
    }

    #[test]
    fn content_type_diverso_415() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let base = || Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO);
        assert_eq!(stato(&invia(porta, &base().con("Content-Type", "text/plain"))), 415);
        assert_eq!(stato(&invia(porta, &base().senza("Content-Type"))), 415);
        nessuna_chiamata(&g);
        // Con il charset è ammesso.
        let r = invia(porta, &base().con("Content-Type", "application/json; charset=utf-8"));
        assert_eq!(stato(&r), 200);
    }

    #[test]
    fn corpo_oltre_256k_413() {
        let g = Registro::nuovo(Modo::Subito);
        let (a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let grande = vec![b'a'; 300 * 1024];
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, &grande));
        assert_eq!(stato(&r), 413);
        nessuna_chiamata(&g);
        assert_eq!(a.rifiutate(), 1);
    }

    #[test]
    fn senza_lunghezza_o_con_transfer_encoding_411() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let base = || Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO);
        assert_eq!(stato(&invia(porta, &base().senza("Content-Length"))), 411);
        assert_eq!(stato(&invia(porta, &base().con("Transfer-Encoding", "chunked"))), 411);
        nessuna_chiamata(&g);
    }

    #[test]
    fn percorso_sconosciuto_404() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(porta, &Rich::valida(porta, &c, "/altro", EVENTO));
        assert_eq!(stato(&r), 404);
        nessuna_chiamata(&g);
    }

    #[test]
    fn intestazioni_troppo_grandi_431() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(
            porta,
            &Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO).con("X-Grande", &"a".repeat(9000)),
        );
        assert_eq!(stato(&r), 431);
        // Troppe righe.
        let mut molte = Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO);
        for i in 0..40 {
            molte = molte.con(&format!("X-{i}"), "1");
        }
        assert_eq!(stato(&invia(porta, &molte)), 431);
        nessuna_chiamata(&g);
    }

    #[test]
    fn json_malformato_400_senza_panico() {
        let g = Registro::nuovo(Modo::Subito);
        let (a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let annidato = "[".repeat(10_000);
        for corpo in ["", "[", "{\"hook_event_name\":", annidato.as_str()] {
            let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, corpo.as_bytes()));
            assert_eq!(stato(&r), 400, "corpo di {} byte", corpo.len());
        }
        nessuna_chiamata(&g);
        assert_eq!(a.rifiutate(), 4);
        // Il thread di ascolto è vivo.
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO));
        assert_eq!(stato(&r), 200);
    }

    #[test]
    fn lettura_lenta_chiusa_dopo_2s() {
        let g = Registro::nuovo(Modo::Subito);
        let (a, _c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let mut s = collega(porta);
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(b"P").unwrap();
        let inizio = Instant::now();
        let mut b = [0u8; 64];
        let esito = s.read(&mut b);
        let passato = inizio.elapsed();
        match esito {
            Ok(_) => {}
            Err(e) => assert!(
                !matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut),
                "il server non ha chiuso: {e}"
            ),
        }
        assert!(passato < Duration::from_millis(2500), "chiusa dopo {passato:?}");
        assert!(passato >= Duration::from_millis(1500), "chiusa troppo presto: {passato:?}");
        nessuna_chiamata(&g);
        assert_eq!(a.rifiutate(), 1);
    }

    #[test]
    fn diciassettesima_connessione_503() {
        let g = Registro::nuovo(Modo::Attendi);
        let (a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let mut aperte = Vec::new();
        for _ in 0..CONNESSIONI_MASSIME {
            let mut s = collega(porta);
            s.write_all(&Rich::valida(porta, &c, PERCORSO_EVENTO, PERMESSO).byte()).unwrap();
            aperte.push(s);
        }
        assert!(aspetta(|| g.permessi.lock().unwrap().len() == CONNESSIONI_MASSIME, 5));
        let inizio = Instant::now();
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO));
        assert_eq!(stato(&r), 503);
        assert!(inizio.elapsed() < Duration::from_millis(200), "{:?}", inizio.elapsed());
        assert_eq!(a.rifiutate(), 1);
        // Chiuse le attese, si torna a servire.
        drop(aperte);
        assert!(aspetta(|| g.abbandonati.lock().unwrap().len() == CONNESSIONI_MASSIME, 5));
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO));
        assert_eq!(stato(&r), 200);
    }

    #[test]
    fn risposta_prima_del_gestore() {
        let mut registro = Registro::nuovo(Modo::Subito);
        Arc::get_mut(&mut registro).unwrap().evento_dorme = Duration::from_secs(3);
        let (_a, c, porta) = avvia_prova(&registro, ATTESA_MASSIMA);
        let inizio = Instant::now();
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, EVENTO));
        assert_eq!(stato(&r), 200);
        assert!(inizio.elapsed() < Duration::from_millis(200), "{:?}", inizio.elapsed());
    }

    const ALLOW: &str =
        r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;

    #[test]
    fn permesso_consenti_risponde_allow() {
        let g = Registro::nuovo(Modo::Decidi(Decisione::Consenti));
        let (a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, PERMESSO));
        assert_eq!(stato(&r), 200);
        assert_eq!(corpo_di(&r), ALLOW);
        assert!(g.abbandonati.lock().unwrap().is_empty());
        assert_eq!(a.rifiutate(), 0);
    }

    #[test]
    fn permesso_nega_risponde_deny() {
        let g = Registro::nuovo(Modo::Decidi(Decisione::Nega));
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, PERMESSO));
        assert_eq!(stato(&r), 200);
        let v: serde_json::Value = serde_json::from_str(corpo_di(&r)).unwrap();
        let d = &v["hookSpecificOutput"]["decision"];
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(d["behavior"], "deny");
        assert_eq!(d["message"], "Negato dall'utente dall'assistente ItsyHome.");
    }

    #[test]
    fn permesso_senza_attesa_risponde_vuoto_subito() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, PERMESSO));
        assert_eq!(stato(&r), 200);
        assert_eq!(corpo_di(&r), "{}");
        assert_eq!(g.permessi.lock().unwrap().len(), 1);
    }

    #[test]
    fn permesso_nessuna_decisione_risponde_vuoto_e_abbandona() {
        let g = Registro::nuovo(Modo::Decidi(Decisione::Nessuna));
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, PERMESSO));
        assert_eq!(corpo_di(&r), "{}");
        assert!(aspetta(|| g.abbandonati.lock().unwrap().len() == 1, 3));
    }

    #[test]
    fn permesso_scaduto_risponde_vuoto() {
        let g = Registro::nuovo(Modo::Attendi);
        let (_a, c, porta) = avvia_prova(&g, Duration::from_millis(300));
        let inizio = Instant::now();
        let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, PERMESSO));
        assert_eq!(stato(&r), 200);
        assert_eq!(corpo_di(&r), "{}");
        assert!(inizio.elapsed() >= Duration::from_millis(300), "{:?}", inizio.elapsed());
        assert!(aspetta(|| g.abbandonati.lock().unwrap().as_slice() == [1], 3));
    }

    #[test]
    fn permesso_chiuso_dal_client_abbandonato() {
        let g = Registro::nuovo(Modo::Attendi);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let mut s = collega(porta);
        s.write_all(&Rich::valida(porta, &c, PERCORSO_EVENTO, PERMESSO).byte()).unwrap();
        assert!(aspetta(|| g.permessi.lock().unwrap().len() == 1, 3));
        assert!(g.abbandonati.lock().unwrap().is_empty());
        let _ = s.shutdown(Shutdown::Both);
        drop(s);
        assert!(aspetta(|| g.abbandonati.lock().unwrap().as_slice() == [1], 1));
    }

    #[test]
    fn mittente_cercato_una_volta_per_sessione() {
        let g = Registro::nuovo(Modo::Subito);
        let (_a, c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        let con_sessione = |id: &str| {
            format!(r#"{{"hook_event_name":"Stop","session_id":"{id}"}}"#).into_bytes()
        };
        for id in ["uno", "uno", "due"] {
            let r = invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, &con_sessione(id)));
            assert_eq!(stato(&r), 200);
        }
        assert!(aspetta(|| g.eventi.lock().unwrap().len() == 3, 3));
        let mut pid: Vec<Option<u32>> =
            g.eventi.lock().unwrap().iter().map(|e| e.pid_mittente).collect();
        pid.sort();
        // Il mittente è questo processo di prova: solo su Windows si trova.
        let atteso = if cfg!(windows) { Some(std::process::id()) } else { None };
        // Due sessioni nuove, una ripetuta.
        assert_eq!(pid, vec![None, atteso, atteso]);
        // Sessione senza id o con id non valido: mai cercata.
        let senza = br#"{"hook_event_name":"Stop"}"#;
        let storto = br#"{"hook_event_name":"Stop","session_id":"a b/c"}"#;
        invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, senza));
        invia(porta, &Rich::valida(porta, &c, PERCORSO_EVENTO, storto));
        assert!(aspetta(|| g.eventi.lock().unwrap().len() == 5, 3));
        let e = g.eventi.lock().unwrap();
        assert_eq!(e[3].pid_mittente, None);
        assert_eq!(e[4].pid_mittente, None);
    }

    #[test]
    fn sessioni_viste_al_massimo_16_scarta_la_piu_vecchia() {
        let s = SessioniViste::default();
        assert!(s.nuova("a"));
        assert!(!s.nuova("a"));
        for i in 0..15 {
            assert!(s.nuova(&format!("s{i}")));
        }
        // "a" è ancora dentro (16 elementi), una nuova la butta fuori.
        assert!(!s.nuova("a"));
        assert!(s.nuova("ultima"));
        assert!(s.nuova("a"), "la più vecchia è stata scartata");
        assert!(!s.nuova("ultima"));
    }

    #[test]
    fn identificatore_di_sessione_valido() {
        assert_eq!(sessione_valida(Some("abc-DEF_09")), Some("abc-DEF_09"));
        assert_eq!(sessione_valida(Some("")), None);
        assert_eq!(sessione_valida(Some("a b")), None);
        assert_eq!(sessione_valida(Some("è")), None);
        assert_eq!(sessione_valida(None), None);
        let lungo = "a".repeat(129);
        assert_eq!(sessione_valida(Some(&lungo)), None);
        let limite = "a".repeat(128);
        assert_eq!(sessione_valida(Some(&limite)), Some(limite.as_str()));
    }

    #[test]
    fn drop_ferma_l_ascolto() {
        let g = Registro::nuovo(Modo::Subito);
        let (a, _c, porta) = avvia_prova(&g, ATTESA_MASSIMA);
        drop(a);
        let esito = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], porta)),
            Duration::from_secs(2),
        );
        assert!(esito.is_err(), "dopo il Drop la porta non ascolta più");
    }
}
