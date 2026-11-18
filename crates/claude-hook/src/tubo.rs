//! Il canale protetto tra Claude Code e isola (spec M3 §10): un tubo con nome di Windows riservato
//! all'utente corrente. Isola è il server ([`avvia`]); l'aiutante `isola-hook.exe` è il cliente
//! ([`collega`]).
//!
//! Chi serve non decide nulla: eventi, permessi e barre passano al [`Gestore`]. Le connessioni
//! rifiutate non lo vedono mai; il servitore le conta (`rifiutate`) e lo avvisa (`rifiutata`).
//!
//! ## Fatti verificati sulla documentazione (18/11/2026, non da memoria)
//!
//! Claude Code, code.claude.com/docs/en/hooks:
//! - Forma exec: con `args` il `command` è un eseguibile lanciato direttamente con `args` come
//!   argomenti, senza shell (`shell` è ignorato). Su Windows deve essere un vero `.exe`.
//! - «Exit code 0 with no output means the hook has no decision to report, so the tool call
//!   continues through the normal permission flow». Stdout di un hook sincrono conta come JSON
//!   solo se è un unico oggetto; stderr di un hook che esce con 0 Claude non lo vede mai.
//! - `timeout` (secondi) predefinito 600 per `command`; non applicato agli hook `async: true`.
//! - `async: true` (solo `type: "command"`): Claude non aspetta; `decision`,
//!   `permissionDecision`, `continue` non hanno effetto, **ma** `additionalContext` e
//!   `systemMessage` dell'output arrivano a Claude al turno dopo. Per questo l'aiutante non stampa
//!   mai nulla fuori dalla risposta a un `PermissionRequest`.
//! - `PermissionRequest`: `hookSpecificOutput.decision.behavior` `"allow"`/`"deny"`, `message` solo
//!   con `"deny"`; un'uscita 2 non conta.
//!
//! Windows, learn.microsoft.com:
//! - `CreateNamedPipeW`: `FILE_FLAG_FIRST_PIPE_INSTANCE` → se il tubo esiste già la creazione
//!   fallisce con `ERROR_ACCESS_DENIED`; `PIPE_REJECT_REMOTE_CLIENTS` rifiuta i clienti di rete;
//!   `PIPE_TYPE_MESSAGE` + `PIPE_READMODE_MESSAGE`: un messaggio non letto per intero dà
//!   `ERROR_MORE_DATA`; `nMaxInstances` lo fissa la prima istanza, le altre devono ripeterlo; con
//!   `lpSecurityAttributes` nullo l'ACL predefinita dà lettura a Everyone e all'anonimo (per questo
//!   passiamo il nostro descrittore).
//! - `CreateFileW` sul tubo: senza istanze → `ERROR_FILE_NOT_FOUND`; istanze tutte collegate →
//!   `ERROR_PIPE_BUSY`; `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`: il server può solo
//!   identificare il cliente, non impersonarlo. Il lato cliente parte in modalità byte: va messo
//!   in modalità messaggio (`SetNamedPipeHandleState`).
//! - `GetNamedPipeServerProcessId` / `GetNamedPipeClientProcessId`: il PID dell'altro capo.
//! - `QueryFullProcessImageNameW`: serve `PROCESS_QUERY_LIMITED_INFORMATION`; `dwFlags = 0` dà il
//!   percorso Win32; `lpdwSize` in caratteri.
//! - `ConvertStringSecurityDescriptorToSecurityDescriptorW`: `SDDL_REVISION_1`; il descrittore è
//!   auto-relativo e si libera con `LocalFree`.
//!
//! ## Il descrittore e le istanze
//! - SDDL `O:<SID>D:P(A;;GA;;;<SID>)` (SID dell'utente): proprietario esplicito, che l'aiutante
//!   rilegge sulla sua maniglia prima di scrivere (vedi `verifica`); DACL protetta (nessuna
//!   ereditarietà) con un solo ACE che dà tutto all'utente corrente; nessun altro utente, né SYSTEM
//!   né Administrators né Everyone. Senza etichetta d'integrità esplicita il tubo vale «media, no
//!   write up»: un processo a bassa integrità (sandbox di un browser) non lo apre.
//! - Prima istanza con `FILE_FLAG_FIRST_PIPE_INSTANCE`: se il nome è già preso, `CanaleOccupato`.
//! - 16 connessioni servite insieme (come il Task 2); `nMaxInstances` = 18 = le 16, quella appena
//!   accettata e giudicata, e quella nuova in ascolto, creata **prima** di lasciare la precedente.
//!   Un'istanza in ascolto andata in errore si scollega e si riusa (non si chiude): finché isola
//!   è accesa e le istanze si creano, il nome non resta libero. Se la creazione fallisce
//!   `FALLIMENTI_MASSIMI` volte di fila, il servitore smette e lo dice al gestore (`canale_perso`)
//!   invece di riprovare in silenzio.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use std::time::Duration;

use crate::barra::DatiBarra;
use crate::evento::EventoHook;

/// Quanto si aspetta una decisione prima di rispondere «nessuna»: sotto i 120 s dell'hook.
pub const ATTESA_MASSIMA: Duration = Duration::from_secs(115);
/// Tempo per ricevere il messaggio dopo l'accettazione.
pub const TEMPO_LETTURA: Duration = Duration::from_secs(2);
/// Connessioni servite insieme; la successiva si accetta, si conta come rifiutata e si chiude.
pub const CONNESSIONI_MASSIME: usize = 16;
/// L'eseguibile di isola: l'aiutante parla solo con lui, nella sua stessa cartella.
pub const NOME_SERVER: &str = "assistente.exe";
/// L'aiutante: isola accetta solo lui, nella sua stessa cartella.
pub const NOME_AIUTANTE: &str = "isola-hook.exe";
/// In prova (`ITSYHOME_ASSISTENTE_DIR`) isola serve `nome_tubo()` con questo suffisso: un'isola di
/// prova e quella vera non si contendono il nome. L'aiutante vero usa sempre il nome senza
/// suffisso; per parlare con un'isola di prova serve l'esempio `aiutante_prova`.
pub const SUFFISSO_PROVA: &str = "-prova";
/// Il prefisso di ogni nome di tubo locale.
const PREFISSO_TUBO: &str = r"\\.\pipe\";

const SESSIONI_RICORDATE: usize = 16;
#[cfg_attr(not(windows), allow(dead_code))]
const PASSO_ATTESA: Duration = Duration::from_millis(250);

/// Che cosa è arrivato, già riconvalidato.
#[derive(Clone)]
pub enum Contenuto {
    Evento(EventoHook),
    Barra(DatiBarra),
}

/// Un messaggio accettato, passato al gestore.
#[derive(Clone)]
pub struct Arrivo {
    pub contenuto: Contenuto,
    /// Il PID dell'aiutante che ha mandato il messaggio, solo al primo evento di un `session_id`
    /// nuovo; altrimenti `None`. Per la barra sempre `None`.
    pub pid_mittente: Option<u32>,
    /// Il numero d'ordine della connessione, preso all'accettazione (prima di leggere): cresce a
    /// ogni connessione accettata. Dice a chi riceve su thread diversi quale è arrivato prima.
    pub sequenza: u64,
    /// Con `pid_mittente`: l'aiutante e i suoi genitori (PID e nome dell'eseguibile, al più 6:
    /// aiutante → `claude.exe` → terminale…), presi mentre l'aiutante è ancora collegato. Per
    /// portare davanti il terminale (Task 10). Vuota altrimenti.
    pub catena_mittente: Vec<(u32, String)>,
}

impl std::fmt::Debug for Arrivo {
    // Comandi e percorsi del riassunto mai nei log: solo il tipo.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tipo = match &self.contenuto {
            Contenuto::Evento(e) => format!("{:?}", e.tipo),
            Contenuto::Barra(_) => "Barra".to_owned(),
        };
        f.debug_struct("Arrivo")
            .field("tipo", &tipo)
            .field("pid_mittente", &self.pid_mittente)
            .field("sequenza", &self.sequenza)
            .field("catena_mittente", &self.catena_mittente.len())
            .finish()
    }
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
    pub decisione: Receiver<Decisione>,
}

/// Chi riceve ciò che il servitore ha accettato.
pub trait Gestore: Send + Sync + 'static {
    /// Un evento diverso da `PermissionRequest`: l'aiutante ha già avuto il «ricevuto».
    fn evento(&self, a: Arrivo);
    /// Un `PermissionRequest`. `None` = rispondi subito «nessuna decisione».
    fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso>;
    /// L'aiutante ha chiuso o l'attesa è finita senza decisione utile: il permesso `id` non serve più.
    fn abbandonato(&self, id: u64);
    /// Dati della barra di stato: l'aiutante ha già avuto il «ricevuto».
    fn barra(&self, a: Arrivo);
    /// Una connessione è stata rifiutata (stesso conteggio di `Servitore::rifiutate`). Nessun dato;
    /// può arrivare da qualunque thread di connessione.
    fn rifiutata(&self) {}
    /// Il servitore non riesce più a creare istanze del tubo e ha smesso: isola non riceve più
    /// nulla finché non riapre il canale. Arriva dal thread di ascolto, una volta.
    fn canale_perso(&self) {}
}

/// Come servire.
#[derive(Debug, Clone)]
pub struct ConfigTubo {
    /// `\\.\pipe\…`: quello vero è `nome_tubo()`, i test ne usano uno unico.
    pub nome: String,
    /// L'unico eseguibile ammesso come cliente.
    pub cliente_atteso: PathBuf,
    pub attesa_massima: Duration,
}

#[derive(Debug)]
pub enum ErroreAvvio {
    /// Il nome del tubo è già preso da un altro programma: isola non si collega.
    CanaleOccupato,
    Altro(String),
}

/// Il nome del tubo dell'utente corrente: `\\.\pipe\isola-claude-<SID>`.
pub fn nome_tubo() -> Result<String, String> {
    let sid = crate::verifica::sid_utente()
        .ok_or_else(|| "SID dell'utente non disponibile".to_owned())?;
    Ok(format!("{PREFISSO_TUBO}isola-claude-{sid}"))
}

/// Un nome di tubo locale e semplice: niente percorsi di rete né altri file.
fn nome_valido(nome: &str) -> bool {
    nome.strip_prefix(PREFISSO_TUBO).is_some_and(|resto| {
        (1..=200).contains(&resto.len())
            && resto
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

/// Dove l'aiutante manda: il tubo e l'unico eseguibile che può esserne il server.
#[derive(Debug, Clone)]
pub struct Destinazione {
    pub nome: String,
    pub server_atteso: PathBuf,
}

impl Destinazione {
    /// Quella vera: il tubo dell'utente e `assistente.exe` nella cartella dell'eseguibile che sta
    /// girando (`current_exe`, non `argv[0]` né l'ambiente). Non esiste altro modo di cambiarla.
    pub fn reale() -> Option<Destinazione> {
        let exe = std::env::current_exe().ok()?;
        Some(Destinazione {
            nome: nome_tubo().ok()?,
            server_atteso: exe.parent()?.join(NOME_SERVER),
        })
    }
}

/// Un canale aperto verso un server **già verificato** (vedi `collega`).
pub struct Canale {
    #[cfg(windows)]
    h: sys::Maniglia,
}

impl Canale {
    /// Manda un messaggio entro `tempo`; `false` se non è andato.
    pub fn manda(&self, messaggio: &[u8], tempo: Duration) -> bool {
        #[cfg(windows)]
        {
            sys::scrivi(&self.h, messaggio, tempo, None)
        }
        #[cfg(not(windows))]
        {
            let _ = (messaggio, tempo);
            false
        }
    }

    /// Un messaggio di risposta (al più 4 KB) entro `tempo`.
    pub fn ricevi(&self, tempo: Duration) -> Option<Vec<u8>> {
        #[cfg(windows)]
        {
            sys::leggi(&self.h, 4096, Some(tempo), None).ok()
        }
        #[cfg(not(windows))]
        {
            let _ = tempo;
            None
        }
    }
}

/// Apre il tubo e **verifica il server** prima di restituire il canale: il processo che l'ha
/// creato gira come l'utente corrente ed esegue esattamente `server_atteso`. Altrimenti `None` e
/// nessun byte è stato scritto. Tubo assente → `None` subito; istanze momentaneamente tutte
/// occupate (isola sta preparando la prossima) → si riprova per al più `attesa`.
pub fn collega(d: &Destinazione, attesa: Duration) -> Option<Canale> {
    if !nome_valido(&d.nome) {
        return None;
    }
    #[cfg(windows)]
    {
        let h = sys::apri_cliente(&d.nome, attesa)?;
        // 1. Il proprietario dell'oggetto tubo (fissato dal kernel) è l'utente corrente.
        if !crate::verifica::tubo_dell_utente(h.0) {
            return None;
        }
        // 2. Il processo server: stesso utente, integrità almeno media, esattamente `server_atteso`.
        let immagine = crate::verifica::immagine_server(h.0)?;
        crate::verifica::stesso_file(&immagine, &d.server_atteso).then_some(Canale { h })
    }
    #[cfg(not(windows))]
    {
        let _ = attesa;
        None
    }
}

/// Il servitore in funzione. `Drop` ferma l'ascolto e raccoglie i thread.
pub struct Servitore {
    #[cfg(windows)]
    interno: server::Interno,
}

impl Servitore {
    /// Quante connessioni sono state rifiutate finora.
    pub fn rifiutate(&self) -> u64 {
        #[cfg(windows)]
        {
            self.interno.rifiutate()
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
}

/// Crea il tubo e lo serve. Fuori da Windows non c'è: `Altro`.
pub fn avvia(c: ConfigTubo, g: std::sync::Arc<dyn Gestore>) -> Result<Servitore, ErroreAvvio> {
    if !nome_valido(&c.nome) {
        return Err(ErroreAvvio::Altro("nome del tubo non valido".into()));
    }
    #[cfg(windows)]
    {
        server::avvia(c, g).map(|interno| Servitore { interno })
    }
    #[cfg(not(windows))]
    {
        let _ = g;
        Err(ErroreAvvio::Altro("Disponibile solo su Windows".into()))
    }
}

/// I `session_id` già visti (al più 16: quando è pieno si scarta il più vecchio).
#[derive(Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct SessioniViste(Mutex<VecDeque<String>>);

#[cfg_attr(not(windows), allow(dead_code))]
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

/// Le primitive di Windows: maniglie, eventi, istanze, I/O sovrapposto con scadenza.
#[cfg(windows)]
pub(crate) mod sys {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::{null, null_mut};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED,
        ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
        OPEN_EXISTING, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PeekNamedPipe,
        SetNamedPipeHandleState, WaitNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_MESSAGE, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE,
    };
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

    /// Istanze del tubo al massimo (vedi l'intestazione del modulo).
    pub const ISTANZE_MASSIME: u32 = super::CONNESSIONI_MASSIME as u32 + 2;
    const BUFFER: u32 = 64 * 1024;

    /// Una maniglia chiusa all'uscita.
    pub struct Maniglia(pub HANDLE);

    impl Drop for Maniglia {
        fn drop(&mut self) {
            if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
                // SAFETY: maniglia aperta da noi e chiusa una volta sola.
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    /// Un evento a ripristino manuale.
    pub struct EventoWin(Maniglia);

    impl EventoWin {
        pub fn nuovo() -> Option<EventoWin> {
            // SAFETY: nessun nome, nessun attributo.
            let h = unsafe { CreateEventW(null(), 1, 0, null()) };
            (h != 0).then(|| EventoWin(Maniglia(h)))
        }
        pub fn segnala(&self) {
            // SAFETY: evento valido.
            unsafe { SetEvent(self.0 .0) };
        }
        /// `true` se è (o diventa entro `tempo`) segnalato.
        pub fn attendi(&self, tempo: Duration) -> bool {
            attendi_uno(&[self.0 .0], Some(tempo)) == Some(0)
        }
    }

    /// Quale maniglia si è segnalata per prima (`None` = scaduto o errore).
    fn attendi_uno(h: &[HANDLE], tempo: Option<Duration>) -> Option<usize> {
        let ms = tempo.map_or(INFINITE, |t| {
            t.as_millis().min(u128::from(INFINITE - 1)) as u32
        });
        // SAFETY: tutte maniglie valide, al più 2.
        let w = unsafe { WaitForMultipleObjects(h.len() as u32, h.as_ptr(), 0, ms) };
        let i = w.wrapping_sub(WAIT_OBJECT_0) as usize;
        (w != WAIT_TIMEOUT && i < h.len()).then_some(i)
    }

    /// Una stringa larga terminata da zero.
    pub fn larga(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Il descrittore di sicurezza «solo l'utente corrente».
    pub struct Descrittore(*mut c_void);

    // SAFETY: buffer auto-relativo nostro, mai modificato dopo la creazione; si libera una volta.
    unsafe impl Send for Descrittore {}
    // SAFETY: come sopra, sola lettura.
    unsafe impl Sync for Descrittore {}

    impl Drop for Descrittore {
        fn drop(&mut self) {
            // SAFETY: buffer dato da `ConvertStringSecurityDescriptorToSecurityDescriptorW`.
            unsafe { LocalFree(self.0) };
        }
    }

    /// L'SDDL del tubo: DACL protetta, un solo ACE con tutti i diritti al SID dell'utente.
    pub fn sddl_solo_utente(sid: &str) -> String {
        format!("O:{sid}D:P(A;;GA;;;{sid})")
    }

    pub fn descrittore_solo_utente() -> Result<Descrittore, String> {
        let sid = crate::verifica::sid_utente()
            .ok_or_else(|| "SID dell'utente non disponibile".to_owned())?;
        let sddl = larga(&sddl_solo_utente(&sid));
        let mut sd: *mut c_void = null_mut();
        // SAFETY: stringa terminata da zero; `sd` riceve un buffer che `Descrittore` libera.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null_mut(),
            )
        };
        if ok == 0 || sd.is_null() {
            // SAFETY: nessun argomento.
            return Err(format!("descrittore non creato (errore {})", unsafe {
                GetLastError()
            }));
        }
        Ok(Descrittore(sd))
    }

    /// Una nuova istanza del tubo (lato server, I/O sovrapposto). L'errore è il codice di Windows.
    pub fn crea_istanza(nome: &[u16], prima: bool, d: &Descrittore) -> Result<Maniglia, u32> {
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: d.0,
            bInheritHandle: 0,
        };
        let apertura = PIPE_ACCESS_DUPLEX
            | FILE_FLAG_OVERLAPPED
            | if prima {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        let modo =
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: nome terminato da zero; `sa` e il descrittore vivono per tutta la chiamata.
        let h = unsafe {
            CreateNamedPipeW(
                nome.as_ptr(),
                apertura,
                modo,
                ISTANZE_MASSIME,
                BUFFER,
                BUFFER,
                0,
                &sa,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            // SAFETY: nessun argomento.
            return Err(unsafe { GetLastError() });
        }
        Ok(Maniglia(h))
    }

    /// Esito dell'attesa di un cliente.
    pub enum Collegamento {
        Collegato,
        Fermo,
        Errore,
    }

    /// Aspetta un cliente sull'istanza, senza giri a vuoto, finché arriva o `fermo` è segnalato.
    pub fn connetti(h: &Maniglia, fermo: &EventoWin) -> Collegamento {
        let Some(ev) = EventoWin::nuovo() else {
            return Collegamento::Errore;
        };
        // SAFETY: OVERLAPPED azzerata è valida; vive fino al completamento (aspettato sotto).
        let mut ov: OVERLAPPED = unsafe { zeroed() };
        ov.hEvent = ev.0 .0;
        // SAFETY: maniglia valida, `ov` nostro.
        if unsafe { ConnectNamedPipe(h.0, &mut ov) } != 0 {
            return Collegamento::Collegato;
        }
        // SAFETY: nessun argomento.
        match unsafe { GetLastError() } {
            ERROR_PIPE_CONNECTED => Collegamento::Collegato,
            ERROR_IO_PENDING => match attendi_uno(&[ev.0 .0, fermo.0 .0], None) {
                Some(0) => {
                    let mut n = 0u32;
                    // SAFETY: operazione completata.
                    if unsafe { GetOverlappedResult(h.0, &ov, &mut n, 0) } != 0 {
                        Collegamento::Collegato
                    } else {
                        Collegamento::Errore
                    }
                }
                _ => {
                    annulla(h, &ov);
                    Collegamento::Fermo
                }
            },
            _ => Collegamento::Errore,
        }
    }

    /// Annulla un'operazione e ne aspetta la fine: dopo, `ov` e il buffer si possono lasciare.
    fn annulla(h: &Maniglia, ov: &OVERLAPPED) {
        let mut n = 0u32;
        // SAFETY: `ov` è quella dell'operazione in corso su `h`.
        unsafe {
            CancelIoEx(h.0, ov);
            GetOverlappedResult(h.0, ov, &mut n, 1);
        }
    }

    /// Completa un'operazione avviata: i byte trasferiti, o il codice d'errore
    /// (`WAIT_TIMEOUT` = scaduta, `ERROR_OPERATION_ABORTED` = fermata).
    fn completa(
        h: &Maniglia,
        ov: &OVERLAPPED,
        avviata: i32,
        tempo: Option<Duration>,
        fermo: Option<&EventoWin>,
    ) -> Result<u32, u32> {
        if avviata == 0 {
            // SAFETY: nessun argomento.
            let e = unsafe { GetLastError() };
            if e != ERROR_IO_PENDING {
                return Err(e);
            }
            let mut attese = vec![ov.hEvent];
            if let Some(f) = fermo {
                attese.push(f.0 .0);
            }
            match attendi_uno(&attese, tempo) {
                Some(0) => {}
                Some(_) => {
                    annulla(h, ov);
                    return Err(ERROR_OPERATION_ABORTED);
                }
                None => {
                    annulla(h, ov);
                    return Err(WAIT_TIMEOUT);
                }
            }
        }
        let mut n = 0u32;
        // SAFETY: operazione completata.
        if unsafe { GetOverlappedResult(h.0, ov, &mut n, 0) } == 0 {
            // SAFETY: nessun argomento.
            return Err(unsafe { GetLastError() });
        }
        Ok(n)
    }

    /// Un messaggio intero di al più `massimo` byte. `Err` con il codice di Windows:
    /// `ERROR_MORE_DATA` = più grande, `WAIT_TIMEOUT` = non arrivato in tempo.
    pub fn leggi(
        h: &Maniglia,
        massimo: usize,
        tempo: Option<Duration>,
        fermo: Option<&EventoWin>,
    ) -> Result<Vec<u8>, u32> {
        let ev = EventoWin::nuovo().ok_or(0u32)?;
        let mut buf = vec![0u8; massimo.max(1)];
        // SAFETY: OVERLAPPED azzerata; vive fino al completamento.
        let mut ov: OVERLAPPED = unsafe { zeroed() };
        ov.hEvent = ev.0 .0;
        // SAFETY: `buf` è lungo `massimo` e vive fino al completamento (o all'annullamento atteso).
        let avviata =
            unsafe { ReadFile(h.0, buf.as_mut_ptr(), buf.len() as u32, null_mut(), &mut ov) };
        let n = completa(h, &ov, avviata, tempo, fermo)?;
        buf.truncate(n as usize);
        Ok(buf)
    }

    /// Scrive un messaggio intero entro `tempo`.
    pub fn scrivi(h: &Maniglia, dati: &[u8], tempo: Duration, fermo: Option<&EventoWin>) -> bool {
        let Some(ev) = EventoWin::nuovo() else {
            return false;
        };
        // SAFETY: come sopra.
        let mut ov: OVERLAPPED = unsafe { zeroed() };
        ov.hEvent = ev.0 .0;
        // SAFETY: `dati` vive fino al completamento.
        let avviata =
            unsafe { WriteFile(h.0, dati.as_ptr(), dati.len() as u32, null_mut(), &mut ov) };
        completa(h, &ov, avviata, Some(tempo), fermo).is_ok_and(|n| n as usize == dati.len())
    }

    /// Scollega l'istanza dal cliente (andato o in errore) perché torni in ascolto.
    pub fn scollega(h: &Maniglia) {
        // SAFETY: maniglia valida del lato server.
        unsafe { DisconnectNamedPipe(h.0) };
    }

    /// `true` se l'altro capo ha chiuso (o il tubo non è più usabile).
    pub fn chiuso(h: &Maniglia) -> bool {
        let mut disponibili = 0u32;
        // SAFETY: nessun buffer; non blocca.
        unsafe { PeekNamedPipe(h.0, null_mut(), 0, null_mut(), &mut disponibili, null_mut()) == 0 }
    }

    /// Apre il lato cliente (I/O sovrapposto, solo identificazione, modalità messaggio). Tubo
    /// assente → `None` subito; tutte le istanze occupate → `WaitNamedPipeW` e un altro tentativo,
    /// per al più `attesa` in tutto.
    pub fn apri_cliente(nome: &str, attesa: Duration) -> Option<Maniglia> {
        let n = larga(nome);
        let fine = std::time::Instant::now() + attesa;
        let h = loop {
            // SAFETY: nome terminato da zero; nessun attributo di sicurezza, nessun modello.
            let h = unsafe {
                CreateFileW(
                    n.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    0,
                )
            };
            if h != INVALID_HANDLE_VALUE {
                break h;
            }
            // SAFETY: nessun argomento.
            if unsafe { GetLastError() } != ERROR_PIPE_BUSY {
                return None;
            }
            let resto = fine.saturating_duration_since(std::time::Instant::now());
            let ms = resto.as_millis().min(1000) as u32;
            // SAFETY: nome terminato da zero. Un'attesa scaduta o fallita chiude il tentativo.
            if ms == 0 || unsafe { WaitNamedPipeW(n.as_ptr(), ms) } == 0 {
                return None;
            }
        };
        let h = Maniglia(h);
        let modo = PIPE_READMODE_MESSAGE;
        // SAFETY: maniglia valida; solo la modalità di lettura.
        if unsafe { SetNamedPipeHandleState(h.0, &modo, null(), null()) } == 0 {
            return None;
        }
        Some(h)
    }
}

/// Il servitore (solo Windows).
#[cfg(windows)]
mod server {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::mpsc::RecvTimeoutError;
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY};

    use super::sys::{self, Collegamento, Descrittore, EventoWin, Maniglia};
    use super::{
        Arrivo, ConfigTubo, Contenuto, Decisione, ErroreAvvio, Gestore, SessioniViste,
        CONNESSIONI_MASSIME, PASSO_ATTESA, TEMPO_LETTURA,
    };
    use crate::evento::TipoEvento;
    use crate::messaggio::{Richiesta, Risposta, MESSAGGIO_MASSIMO};
    use crate::verifica;

    struct Contesto {
        cliente_atteso: PathBuf,
        attesa_massima: Duration,
        gestore: Arc<dyn Gestore>,
        rifiutate: AtomicU64,
        sessioni: SessioniViste,
        fermo: AtomicBool,
        fermo_ev: EventoWin,
        connessioni: AtomicUsize,
        /// Il prossimo numero d'ordine di connessione (vedi `Arrivo::sequenza`).
        sequenza: AtomicU64,
        /// I thread delle connessioni, raccolti alla fine.
        figli: Mutex<Vec<JoinHandle<()>>>,
    }

    impl Contesto {
        fn conta_rifiuto(&self) {
            self.rifiutate.fetch_add(1, Ordering::Relaxed);
            self.gestore.rifiutata();
        }
        fn fermato(&self) -> bool {
            self.fermo.load(Ordering::SeqCst)
        }
    }

    pub(super) struct Interno {
        contesto: Arc<Contesto>,
        thread: Option<JoinHandle<()>>,
    }

    impl Interno {
        pub(super) fn rifiutate(&self) -> u64 {
            self.contesto.rifiutate.load(Ordering::Relaxed)
        }
    }

    impl Drop for Interno {
        fn drop(&mut self) {
            self.contesto.fermo.store(true, Ordering::SeqCst);
            self.contesto.fermo_ev.segnala();
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
            // Le connessioni vedono il fermo (I/O annullato, attese dei permessi entro 250 ms):
            // finite loro, nessuna istanza del tubo resta aperta e il nome si può riprendere.
            let figli: Vec<JoinHandle<()>> = std::mem::take(
                &mut *self
                    .contesto
                    .figli
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            );
            for f in figli {
                let _ = f.join();
            }
        }
    }

    pub(super) fn avvia(c: ConfigTubo, g: Arc<dyn Gestore>) -> Result<Interno, ErroreAvvio> {
        let d = sys::descrittore_solo_utente().map_err(ErroreAvvio::Altro)?;
        let nome = sys::larga(&c.nome);
        let prima = sys::crea_istanza(&nome, true, &d).map_err(|e| match e {
            ERROR_ACCESS_DENIED | ERROR_PIPE_BUSY => ErroreAvvio::CanaleOccupato,
            e => ErroreAvvio::Altro(format!("tubo non creato (errore {e})")),
        })?;
        let fermo_ev =
            EventoWin::nuovo().ok_or_else(|| ErroreAvvio::Altro("evento non creato".into()))?;
        let contesto = Arc::new(Contesto {
            cliente_atteso: c.cliente_atteso,
            attesa_massima: c.attesa_massima,
            gestore: g,
            rifiutate: AtomicU64::new(0),
            sessioni: SessioniViste::default(),
            fermo: AtomicBool::new(false),
            fermo_ev,
            connessioni: AtomicUsize::new(0),
            sequenza: AtomicU64::new(1),
            figli: Mutex::new(Vec::new()),
        });
        let c2 = Arc::clone(&contesto);
        let thread = thread::Builder::new()
            .name("claude-tubo".into())
            .spawn(move || accetta(prima, nome, d, c2))
            .map_err(|e| ErroreAvvio::Altro(e.to_string()))?;
        Ok(Interno {
            contesto,
            thread: Some(thread),
        })
    }

    /// Conta una connessione servita finché vive.
    struct Guardiano(Arc<Contesto>);

    impl Drop for Guardiano {
        fn drop(&mut self) {
            self.0.connessioni.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn accetta(prima: Maniglia, nome: Vec<u16>, d: Descrittore, ctx: Arc<Contesto>) {
        let mut ascolto = Some(prima);
        // Errori di fila (creazione di un'istanza o attesa di un cliente); si azzera a ogni cliente.
        let mut fallimenti = 0u32;
        while !ctx.fermato() {
            if fallimenti >= FALLIMENTI_MASSIMI {
                // Niente giri a vuoto senza fine: si smette e lo si dice.
                ctx.gestore.canale_perso();
                break;
            }
            let h = match ascolto.take() {
                Some(h) => h,
                None => match sys::crea_istanza(&nome, false, &d) {
                    Ok(h) => h,
                    Err(_) => {
                        fallimenti += 1;
                        if ctx.fermo_ev.attendi(RESPIRO) {
                            break;
                        }
                        continue;
                    }
                },
            };
            match sys::connetti(&h, &ctx.fermo_ev) {
                Collegamento::Fermo => break,
                Collegamento::Errore => {
                    // Un cliente arrivato e già sparito: la stessa istanza si scollega e torna in
                    // ascolto, così il nome non resta mai senza istanze.
                    sys::scollega(&h);
                    ascolto = Some(h);
                    fallimenti += 1;
                    if ctx.fermo_ev.attendi(RESPIRO) {
                        break;
                    }
                    continue;
                }
                Collegamento::Collegato => fallimenti = 0,
            }
            if ctx.fermato() {
                break;
            }
            // La prossima istanza in ascolto prima di lasciare questa: il nome non resta mai libero.
            ascolto = sys::crea_istanza(&nome, false, &d).ok();
            let sequenza = ctx.sequenza.fetch_add(1, Ordering::SeqCst);
            let gia = ctx.connessioni.fetch_add(1, Ordering::SeqCst);
            let guardiano = Guardiano(Arc::clone(&ctx));
            if gia >= CONNESSIONI_MASSIME {
                ctx.conta_rifiuto();
                drop(h);
                continue;
            }
            let c2 = Arc::clone(&ctx);
            let esito = thread::Builder::new()
                .name("claude-conn".into())
                .spawn(move || {
                    let _guardiano = guardiano;
                    servi(h, &c2, sequenza);
                });
            match esito {
                Ok(t) => {
                    let mut figli = ctx.figli.lock().unwrap_or_else(|e| e.into_inner());
                    figli.retain(|f| !f.is_finished());
                    figli.push(t);
                }
                // Il closure è stato lasciato cadere: istanza chiusa e contatore restituito.
                Err(_) => ctx.conta_rifiuto(),
            }
        }
    }

    /// Errori di fila dopo cui il servitore smette (`Gestore::canale_perso`): circa mezzo secondo.
    const FALLIMENTI_MASSIMI: u32 = 50;
    /// La pausa fra due tentativi dopo un errore.
    const RESPIRO: Duration = Duration::from_millis(10);

    fn servi(h: Maniglia, ctx: &Contesto, sequenza: u64) {
        if istruisci(&h, ctx, sequenza).is_err() {
            ctx.conta_rifiuto();
        }
    }

    fn rispondi(h: &Maniglia, ctx: &Contesto, r: Risposta) {
        let _ = sys::scrivi(h, &r.in_byte(), TEMPO_LETTURA, Some(&ctx.fermo_ev));
    }

    /// Dopo la risposta si aspetta che l'aiutante chiuda (al più 2 s), così la risposta non si
    /// perde con la chiusura della nostra istanza.
    fn aspetta_chiusura(h: &Maniglia, ctx: &Contesto) {
        let _ = sys::leggi(h, 1, Some(TEMPO_LETTURA), Some(&ctx.fermo_ev));
    }

    /// Le regole del servitore. `Err` = rifiutata (la connessione si chiude senza risposta).
    fn istruisci(h: &Maniglia, ctx: &Contesto, sequenza: u64) -> Result<(), ()> {
        // 1. Chi è il cliente, prima di leggere un solo byte.
        let pid = verifica::pid_cliente(h.0).ok_or(())?;
        let immagine = verifica::immagine_cliente(h.0).ok_or(())?;
        if !verifica::stesso_file(&immagine, &ctx.cliente_atteso) {
            return Err(());
        }
        // 2. Un messaggio, entro 2 s e 64 KB, riconvalidato.
        let dati = sys::leggi(
            h,
            MESSAGGIO_MASSIMO,
            Some(TEMPO_LETTURA),
            Some(&ctx.fermo_ev),
        )
        .map_err(|_| ())?;
        let richiesta = Richiesta::da_byte(&dati).map_err(|_| ())?;
        // 3. Da qui la richiesta è accettata.
        match richiesta {
            Richiesta::Barra(d) => {
                rispondi(h, ctx, Risposta::Ricevuto);
                ctx.gestore.barra(Arrivo {
                    contenuto: Contenuto::Barra(d),
                    pid_mittente: None,
                    sequenza,
                    catena_mittente: Vec::new(),
                });
            }
            Richiesta::Evento(e) => {
                let pid_mittente = ctx.sessioni.nuova(&e.sessione).then_some(pid);
                // La catena si prende ora, prima della risposta: l'aiutante è ancora collegato.
                let catena_mittente = pid_mittente
                    .map(verifica::catena_processi)
                    .unwrap_or_default();
                let permesso = e.tipo == TipoEvento::PermissionRequest;
                let arrivo = Arrivo {
                    contenuto: Contenuto::Evento(e),
                    pid_mittente,
                    sequenza,
                    catena_mittente,
                };
                if permesso {
                    attendi_permesso(h, ctx, arrivo);
                } else {
                    rispondi(h, ctx, Risposta::Ricevuto);
                    ctx.gestore.evento(arrivo);
                }
            }
        }
        aspetta_chiusura(h, ctx);
        Ok(())
    }

    fn attendi_permesso(h: &Maniglia, ctx: &Contesto, arrivo: Arrivo) {
        let Some(attesa) = ctx.gestore.permesso(arrivo) else {
            rispondi(h, ctx, Risposta::Nessuna);
            return;
        };
        let id = attesa.id;
        let inizio = Instant::now();
        // Che cosa rispondere quando il ciclo finisce; `None` = niente (aiutante sparito o fermo).
        let risposta: Option<Risposta> = loop {
            if ctx.fermato() || sys::chiuso(h) {
                break None;
            }
            let resto = ctx.attesa_massima.saturating_sub(inizio.elapsed());
            if resto.is_zero() {
                break Some(Risposta::Nessuna);
            }
            match attesa.decisione.recv_timeout(PASSO_ATTESA.min(resto)) {
                Ok(Decisione::Consenti) => return rispondi(h, ctx, Risposta::Consenti),
                Ok(Decisione::Nega) => return rispondi(h, ctx, Risposta::Nega),
                Ok(Decisione::Nessuna) | Err(RecvTimeoutError::Disconnected) => {
                    break Some(Risposta::Nessuna)
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        };
        if let Some(r) = risposta {
            rispondi(h, ctx, r);
        }
        ctx.gestore.abbandonato(id);
    }
}

/// Un nome di tubo unico per i test: mai quello vero.
#[cfg(test)]
pub(crate) fn nome_di_prova() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{PREFISSO_TUBO}isola-prova-{}-{}-{t}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn nome_del_tubo_per_utente() {
        if cfg!(windows) {
            let n = nome_tubo().unwrap();
            assert!(n.starts_with(r"\\.\pipe\isola-claude-S-1-"), "{n}");
            assert!(nome_valido(&n));
        } else {
            assert!(nome_tubo().is_err());
        }
        assert!(nome_valido(&nome_di_prova()));
        for storto in [
            r"\\server\pipe\x",
            r"\\.\pipe\",
            r"\\.\pipe\a\b",
            r"C:\x",
            r"\\.\pipe\a b",
            "x",
        ] {
            assert!(!nome_valido(storto), "{storto}");
        }
    }

    #[test]
    fn sessioni_viste_al_massimo_16_scarta_la_piu_vecchia() {
        let s = SessioniViste::default();
        assert!(s.nuova("a"));
        assert!(!s.nuova("a"));
        for i in 0..15 {
            assert!(s.nuova(&format!("s{i}")));
        }
        assert!(!s.nuova("a"));
        assert!(s.nuova("ultima"));
        assert!(s.nuova("a"), "la più vecchia è stata scartata");
        assert!(!s.nuova("ultima"));
    }

    #[test]
    fn debug_di_arrivo_senza_contenuto() {
        let e = EventoHook {
            sessione: "s".into(),
            tipo: crate::evento::TipoEvento::PermissionRequest,
            cartella: "CARTELLA-SEGRETA".into(),
            strumento: Some("Bash".into()),
            notifica: None,
            errore: None,
            riassunto: Some("COMANDO-SEGRETO".into()),
        };
        let a = Arrivo {
            contenuto: Contenuto::Evento(e),
            pid_mittente: Some(7),
            sequenza: 9,
            catena_mittente: vec![(7, "isola-hook.exe".into()), (5, "claude.exe".into())],
        };
        let d = format!("{a:?}");
        assert!(!d.contains("SEGRET"), "{d}");
        assert!(
            d.contains("PermissionRequest") && d.contains("Some(7)") && d.contains('9'),
            "{d}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn fuori_da_windows_non_c_e() {
        struct Nessuno;
        impl Gestore for Nessuno {
            fn evento(&self, _: Arrivo) {}
            fn permesso(&self, _: Arrivo) -> Option<AttesaPermesso> {
                None
            }
            fn abbandonato(&self, _: u64) {}
            fn barra(&self, _: Arrivo) {}
        }
        let c = ConfigTubo {
            nome: nome_di_prova(),
            cliente_atteso: PathBuf::new(),
            attesa_massima: ATTESA_MASSIMA,
        };
        assert!(matches!(
            avvia(c, std::sync::Arc::new(Nessuno)),
            Err(ErroreAvvio::Altro(_))
        ));
        let d = Destinazione {
            nome: nome_di_prova(),
            server_atteso: PathBuf::new(),
        };
        assert!(collega(&d, Duration::ZERO).is_none());
    }
}

#[cfg(all(test, windows))]
mod test_tubo;
