//! Finestra di gestione (spec §3bis): un processo separato, `itsyhome-gestione`, accanto
//! all'eseguibile dell'assistente. Nasce alla prima `apri`, muore alla `chiudi` o quando l'utente
//! chiude la finestra; parla con l'assistente su stdin/stdout (una riga JSON per messaggio, vedi
//! `gestione-protocollo`). Il processo non riceve token né configurazione: solo la cartella dei
//! dati del motore web, sotto la cartella dell'assistente.
//!
//! Per ogni processo due thread:
//! - lo **scrittore** manda le righe allo stdin del figlio (il thread dell'interfaccia non si
//!   blocca mai su una pipe piena) e, quando la `Gestione` lo lascia andare, chiude lo stdin;
//! - il **guardiano** legge lo stdout del figlio (righe al più di 64 KiB: è il lato esposto al
//!   contenuto ostile): un `chiusa` o lo stdout chiuso vogliono dire finestra chiusa, consegnata
//!   una volta sola; poi raccoglie il figlio (`wait`), così non restano processi zombie.
//!
//! All'uscita dell'assistente si aspetta al più `ATTESA_USCITA`; un figlio ancora vivo (appeso) si
//! uccide: l'uscita non si blocca mai. Se l'assistente muore, lo stdin del figlio si chiude e il
//! figlio esce da solo.
//!
//! Legami tra i crate: la mail di prova sta in `crates/gestione/assets/` (vedi `include_str!` più
//! sotto); l'eseguibile `itsyhome-gestione` si costruisce con `-p gestione`, non con
//! `-p assistente`, e va distribuito accanto a quello dell'assistente.

use gestione_protocollo::{messaggio_da_riga, riga_da_messaggio, Messaggio};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const NOME_ESEGUIBILE: &str = if cfg!(windows) { "itsyhome-gestione.exe" } else { "itsyhome-gestione" };

/// Cartella dei dati del motore web (WebView2), dentro quella dell'assistente: segue quindi
/// anche `ITSYHOME_ASSISTENTE_DIR` nelle prove.
pub fn cartella_dati(cartella_assistente: &Path) -> PathBuf {
    cartella_assistente.join("gestione-webview")
}

/// Quanto si aspetta un figlio che deve uscire (dopo la chiusura della finestra, o all'uscita
/// dell'assistente) prima di ucciderlo.
const ATTESA_USCITA: Duration = Duration::from_secs(5);

/// Lunghezza massima di una riga dal figlio: i suoi messaggi sono minuscoli. Verso il figlio
/// (lato fidato, HTML anche grandi) nessun limite.
const LIMITE_RIGA: usize = 64 * 1024;

type AlMessaggio = Arc<dyn Fn(Messaggio) + Send + Sync>;

pub struct Gestione {
    /// Percorso assoluto accanto all'eseguibile dell'assistente, o perché non lo si conosce.
    eseguibile: Result<PathBuf, String>,
    dati: PathBuf,
    al_messaggio: AlMessaggio,
    figlio: Option<Figlio>,
    /// Processi lasciati andare (chiusi o in chiusura): si aspettano all'uscita.
    congedati: Vec<Congedato>,
}

struct Figlio {
    /// Righe verso lo scrittore; chiuderlo chiude lo stdin del figlio.
    invio: Sender<String>,
    /// Falso appena la finestra si chiude (`chiusa` o stdout chiuso).
    viva: Arc<AtomicBool>,
    fili: Congedato,
}

struct Congedato {
    processo: Arc<Mutex<Child>>,
    guardiano: JoinHandle<()>,
    scrittore: JoinHandle<()>,
}

impl Congedato {
    fn finito(&self) -> bool {
        self.guardiano.is_finished() && self.scrittore.is_finished()
    }

    /// Aspetta i due thread fino a `limite`; poi uccide il figlio (stdout e stdin si chiudono e i
    /// thread tornano) e li raccoglie.
    fn chiudi_entro(self, limite: Instant) {
        while !self.finito() && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(20));
        }
        if !self.finito() {
            eprintln!("finestra di gestione: il processo non esce, lo si chiude");
            if let Ok(mut p) = self.processo.lock() {
                let _ = p.kill();
            }
        }
        let _ = self.guardiano.join();
        let _ = self.scrittore.join();
    }
}

impl Gestione {
    /// `al_messaggio` riceve `pronta`, `azione` e `chiusa` (una volta per processo, anche se il
    /// figlio è morto senza dirlo). **Gira sul thread del guardiano**, non su quello
    /// dell'interfaccia: chi tocca l'interfaccia passa da `slint::invoke_from_event_loop`.
    pub fn new(cartella_assistente: &Path, al_messaggio: impl Fn(Messaggio) + Send + Sync + 'static) -> Self {
        let eseguibile = std::env::current_exe()
            .map_err(|e| format!("finestra di gestione: posizione dell'assistente sconosciuta: {e}"))
            .and_then(|e| {
                e.parent()
                    .map(|d| d.join(NOME_ESEGUIBILE))
                    .ok_or_else(|| "finestra di gestione: cartella dell'assistente sconosciuta".to_owned())
            });
        Self::costruisci(eseguibile, cartella_dati(cartella_assistente), al_messaggio)
    }

    #[cfg(test)]
    pub fn con_eseguibile(
        eseguibile: PathBuf,
        dati: PathBuf,
        al_messaggio: impl Fn(Messaggio) + Send + Sync + 'static,
    ) -> Self {
        Self::costruisci(Ok(eseguibile), dati, al_messaggio)
    }

    fn costruisci(
        eseguibile: Result<PathBuf, String>,
        dati: PathBuf,
        al_messaggio: impl Fn(Messaggio) + Send + Sync + 'static,
    ) -> Self {
        Self { eseguibile, dati, al_messaggio: Arc::new(al_messaggio), figlio: None, congedati: Vec::new() }
    }

    /// Mostra il contenuto: avvia il processo se non c'è (o se la sua finestra è stata chiusa),
    /// poi gli manda `apri`.
    ///
    /// `Ok` vuol dire **inviato**, non mostrato: fa fede `Pronta` (in `al_messaggio`). Se l'utente
    /// ha appena chiuso la finestra e la `chiusa` non è ancora arrivata, l'invio va a vuoto e dopo
    /// arriva `Chiusa`: chi vuole il contenuto visibile ripete `apri`.
    pub fn apri(&mut self, titolo: &str, html: &str) -> Result<(), String> {
        self.raccogli_finiti();
        if self.figlio.as_ref().is_some_and(|f| !f.viva.load(Ordering::SeqCst)) {
            self.congeda();
        }
        let riga = riga_da_messaggio(&Messaggio::Apri { titolo: titolo.to_owned(), html: html.to_owned() });
        if self.figlio.is_none() {
            self.figlio = Some(self.avvia()?);
        }
        let figlio = self.figlio.as_ref().expect("figlio appena avviato");
        if let Err(mpsc::SendError(riga)) = figlio.invio.send(riga) {
            // Lo scrittore è uscito: pipe rotta, figlio morto. Nuovo avvio, una volta.
            eprintln!("finestra di gestione: invio non riuscito, nuovo avvio");
            self.congeda();
            let figlio = self.avvia()?;
            figlio.invio.send(riga).map_err(|_| "finestra di gestione: invio non riuscito".to_owned())?;
            self.figlio = Some(figlio);
        }
        Ok(())
    }

    /// Chiude la finestra: `chiudi` e poi stdin chiuso; i thread del processo lo raccolgono.
    pub fn chiudi(&mut self) {
        if let Some(figlio) = &self.figlio {
            let _ = figlio.invio.send(riga_da_messaggio(&Messaggio::Chiudi));
        }
        self.congeda();
    }

    /// Lascia andare il figlio corrente: canale chiuso (lo scrittore manda ciò che resta e chiude
    /// lo stdin, il figlio esce comunque), thread tenuti da parte fino all'uscita.
    fn congeda(&mut self) {
        if let Some(figlio) = self.figlio.take() {
            drop(figlio.invio);
            self.congedati.push(figlio.fili);
        }
    }

    /// Raccoglie i thread dei processi già usciti (join immediato).
    fn raccogli_finiti(&mut self) {
        let (finiti, vivi): (Vec<_>, Vec<_>) = self.congedati.drain(..).partition(Congedato::finito);
        self.congedati = vivi;
        for c in finiti {
            c.chiudi_entro(Instant::now());
        }
    }

    fn avvia(&self) -> Result<Figlio, String> {
        let eseguibile = self.eseguibile.as_ref().map_err(Clone::clone)?;
        if !eseguibile.is_file() {
            return Err(format!("finestra di gestione: eseguibile non trovato: {}", eseguibile.display()));
        }
        let mut comando = Command::new(eseguibile);
        comando
            .arg("--dati")
            .arg(&self.dati)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            // Niente configurazione dell'assistente nel figlio.
            .env_remove(crate::avvio::VAR_CARTELLA)
            .env_remove("ITSYHOME_PROVA_NOTIFICHE");
        // Né variabili che cambiano il motore web (es. argomenti del browser con
        // `--remote-debugging-port`, che riaccenderebbe il debug remoto).
        for (nome, _) in std::env::vars_os() {
            if nome.to_string_lossy().to_ascii_uppercase().starts_with("WEBVIEW2_") {
                comando.env_remove(nome);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Nessuna console per il figlio (in debug è un programma da console).
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            comando.creation_flags(CREATE_NO_WINDOW);
        }
        let mut processo = comando.spawn().map_err(|e| format!("finestra di gestione non avviata: {e}"))?;
        let stdin = processo.stdin.take().ok_or("finestra di gestione: stdin assente")?;
        let stdout = processo.stdout.take().ok_or("finestra di gestione: stdout assente")?;
        let processo = Arc::new(Mutex::new(processo));
        let viva = Arc::new(AtomicBool::new(true));
        let (invio, ricezione) = mpsc::channel::<String>();

        let filo = |nome: &str, corpo: Box<dyn FnOnce() + Send>| {
            std::thread::Builder::new()
                .name(nome.into())
                .spawn(corpo)
                .map_err(|e| format!("finestra di gestione: thread non avviato: {e}"))
        };
        let scrittore = filo("gestione-scrittore", Box::new(move || scrivi_tutto(stdin, ricezione)));
        let scrittore = match scrittore {
            Ok(s) => s,
            Err(e) => {
                let _ = processo.lock().map(|mut p| p.kill());
                return Err(e);
            }
        };
        let guardiano = {
            let (processo, viva, al_messaggio) = (processo.clone(), viva.clone(), self.al_messaggio.clone());
            filo("gestione-guardiano", Box::new(move || guarda(processo, stdout, viva, al_messaggio)))
        };
        let guardiano = match guardiano {
            Ok(g) => g,
            Err(e) => {
                let _ = processo.lock().map(|mut p| p.kill());
                return Err(e);
            }
        };
        Ok(Figlio { invio, viva, fili: Congedato { processo, guardiano, scrittore } })
    }
}

impl Drop for Gestione {
    /// L'uscita non si blocca mai: una sola scadenza per tutti i processi, poi `kill`.
    fn drop(&mut self) {
        self.chiudi();
        let limite = Instant::now() + ATTESA_USCITA;
        for c in self.congedati.drain(..) {
            c.chiudi_entro(limite);
        }
    }
}

/// Thread scrittore: una riga per messaggio; a canale chiuso (o pipe rotta) chiude lo stdin.
fn scrivi_tutto(mut stdin: ChildStdin, righe: mpsc::Receiver<String>) {
    for riga in righe {
        if writeln!(stdin, "{riga}").and_then(|_| stdin.flush()).is_err() {
            return;
        }
    }
}

/// Una riga letta dal figlio.
#[derive(Debug, PartialEq)]
enum Riga {
    Testo(String),
    /// Oltre `LIMITE_RIGA`: scartata fino al suo `\n`.
    TroppoLunga,
    Fine,
}

/// Legge una riga senza mai tenere in memoria più di `LIMITE_RIGA` (+1) byte.
fn leggi_riga(lettore: &mut impl BufRead, buf: &mut Vec<u8>) -> Riga {
    buf.clear();
    match lettore.by_ref().take(LIMITE_RIGA as u64 + 1).read_until(b'\n', buf) {
        Ok(0) | Err(_) => return Riga::Fine,
        Ok(_) if buf.ends_with(b"\n") || buf.len() <= LIMITE_RIGA => {
            return Riga::Testo(String::from_utf8_lossy(buf).into_owned())
        }
        Ok(_) => {}
    }
    // Troppo lunga: si scarta il resto, a pezzi, fino al suo `\n` (o alla fine).
    loop {
        buf.clear();
        match lettore.by_ref().take(LIMITE_RIGA as u64).read_until(b'\n', buf) {
            Ok(0) | Err(_) => return Riga::TroppoLunga,
            Ok(_) if buf.ends_with(b"\n") => return Riga::TroppoLunga,
            Ok(_) => {}
        }
    }
}

/// Thread guardiano di un processo: inoltra i messaggi, poi raccoglie il figlio.
fn guarda(
    processo: Arc<Mutex<Child>>,
    stdout: std::process::ChildStdout,
    viva: Arc<AtomicBool>,
    al_messaggio: AlMessaggio,
) {
    let mut lettore = BufReader::new(stdout);
    let mut buf = Vec::new();
    loop {
        match leggi_riga(&mut lettore, &mut buf) {
            Riga::Fine => break,
            Riga::TroppoLunga => eprintln!("finestra di gestione: riga troppo lunga, scartata"),
            Riga::Testo(riga) => match messaggio_da_riga(&riga) {
                Ok(Messaggio::Chiusa) => break,
                Ok(m) => al_messaggio(m),
                Err(e) => eprintln!("finestra di gestione: {e}"),
            },
        }
    }
    // `chiusa` o stdout chiuso: per l'assistente la finestra non c'è più (una volta sola).
    viva.store(false, Ordering::SeqCst);
    al_messaggio(Messaggio::Chiusa);
    let limite = Instant::now() + ATTESA_USCITA;
    loop {
        let esito = processo.lock().map(|mut p| p.try_wait());
        match esito {
            Ok(Ok(Some(_))) => return,
            Ok(Ok(None)) if Instant::now() < limite => std::thread::sleep(Duration::from_millis(50)),
            _ => break,
        }
    }
    eprintln!("finestra di gestione: il processo non esce da solo, lo si chiude");
    if let Ok(mut p) = processo.lock() {
        let _ = p.kill();
        let _ = p.wait();
    }
}

// ─── Prova: `--prova-gestione` ───

thread_local! {
    static PROVA: RefCell<Option<Gestione>> = const { RefCell::new(None) };
}

const MAIL_DI_PROVA: &str = include_str!("../../gestione/assets/prova-mail.html");

/// Calendario della prova: (quando, apri o chiudi). Senza `cicli` (`--prova-gestione`) apre a 3 s
/// (prima c'è la misura dell'assistente da solo), chiude a 10 s e riapre a 18 s, lasciandola
/// aperta. Con `cicli` (`--prova-gestione-cicli <n>`) ripete `n` volte apri (3 s + 12 s × i) e
/// chiudi 7 s dopo, e alla fine resta chiusa: misure dei cicli ripetuti.
fn calendario_prova(cicli: Option<u32>) -> Vec<(Duration, bool)> {
    let s = Duration::from_secs;
    match cicli {
        None => vec![(s(3), true), (s(10), false), (s(18), true)],
        Some(n) => (0..u64::from(n))
            .flat_map(|i| [(s(3 + 12 * i), true), (s(10 + 12 * i), false)])
            .collect(),
    }
}

/// `--prova-gestione [--prova-gestione-cicli <n>]`: la finestra con la mail di prova secondo
/// `calendario_prova`. Tempi di apertura nel log.
pub fn prova(cartella_assistente: &Path, cicli: Option<u32>) {
    let aperta_alle: Arc<Mutex<Option<Instant>>> = Arc::default();
    let per_messaggi = aperta_alle.clone();
    let gestione = Gestione::new(cartella_assistente, move |m| match m {
        Messaggio::Pronta => {
            let ms = per_messaggi.lock().ok().and_then(|t| *t).map(|t| t.elapsed().as_millis());
            eprintln!("finestra di gestione: pronta dopo {} ms", ms.unwrap_or_default());
        }
        m => eprintln!("finestra di gestione: {m:?}"),
    });
    PROVA.with_borrow_mut(|p| *p = Some(gestione));
    for (quando, apri) in calendario_prova(cicli) {
        let aperta_alle = aperta_alle.clone();
        slint::Timer::single_shot(quando, move || {
            PROVA.with_borrow_mut(|p| {
                let Some(g) = p.as_mut() else { return };
                if !apri {
                    g.chiudi();
                    return;
                }
                if let Ok(mut t) = aperta_alle.lock() {
                    *t = Some(Instant::now());
                }
                if let Err(e) = g.apri("Riepilogo del tuo ordine", MAIL_DI_PROVA) {
                    eprintln!("{e}");
                }
            });
        });
    }
}

/// All'uscita dell'assistente: chiude la finestra di prova e ne raccoglie il processo.
pub fn spegni() {
    drop(PROVA.with_borrow_mut(Option::take));
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn righe_lunghe_scartate_senza_tenerle_in_memoria() {
        let lunga = "x".repeat(LIMITE_RIGA * 3);
        let dati = format!("{{\"tipo\":\"pronta\"}}\n{lunga}\nbreve\n{}", "y".repeat(10));
        let mut lettore = Cursor::new(dati.into_bytes());
        let mut buf = Vec::new();
        assert_eq!(leggi_riga(&mut lettore, &mut buf), Riga::Testo("{\"tipo\":\"pronta\"}\n".into()));
        assert_eq!(leggi_riga(&mut lettore, &mut buf), Riga::TroppoLunga);
        assert!(buf.capacity() <= 2 * (LIMITE_RIGA + 1), "memoria limitata: {}", buf.capacity());
        assert_eq!(leggi_riga(&mut lettore, &mut buf), Riga::Testo("breve\n".into()));
        // L'ultima riga senza a capo vale (il figlio è uscito a metà riga).
        assert_eq!(leggi_riga(&mut lettore, &mut buf), Riga::Testo("y".repeat(10)));
        assert_eq!(leggi_riga(&mut lettore, &mut buf), Riga::Fine);
    }

    #[test]
    fn calendario_della_prova() {
        let s = Duration::from_secs;
        assert_eq!(calendario_prova(None), vec![(s(3), true), (s(10), false), (s(18), true)]);
        let cicli = calendario_prova(Some(5));
        assert_eq!(cicli.len(), 10);
        assert_eq!(cicli[..4], [(s(3), true), (s(10), false), (s(15), true), (s(22), false)]);
        assert_eq!(cicli.last(), Some(&(s(58), false)), "alla fine resta chiusa");
        assert!(cicli.windows(2).all(|c| c[0].0 < c[1].0 && c[0].1 != c[1].1), "alterna apri e chiudi");
        assert!(calendario_prova(Some(0)).is_empty());
    }

    #[test]
    fn senza_posizione_dell_assistente_errore_senza_percorsi_relativi() {
        let dir = tempfile::tempdir().unwrap();
        let mut g = Gestione::costruisci(Err("posizione sconosciuta".into()), dir.path().join("dati"), |_| {});
        assert_eq!(g.apri("T", "x"), Err("posizione sconosciuta".into()));
    }

    #[test]
    fn senza_eseguibile_apri_e_un_errore_non_un_panico() {
        let dir = tempfile::tempdir().unwrap();
        let mut g = Gestione::con_eseguibile(dir.path().join(NOME_ESEGUIBILE), dir.path().join("dati"), |_| {});
        let errore = g.apri("T", "<p>x</p>").unwrap_err();
        assert!(errore.contains("non trovato"), "{errore}");
        g.chiudi();
    }

    /// Il figlio finto (`examples/figlio_finto.rs`), costruito da `cargo test` accanto ai test.
    fn figlio_finto() -> PathBuf {
        let test = std::env::current_exe().unwrap();
        let cartella = test.parent().unwrap().parent().unwrap().join("examples");
        cartella.join(if cfg!(windows) { "figlio_finto.exe" } else { "figlio_finto" })
    }

    /// Una `Gestione` col figlio finto nel modo dato e il canale dei messaggi ricevuti.
    fn con_figlio(modo: &str) -> (Gestione, std::sync::mpsc::Receiver<Messaggio>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let g = Gestione::con_eseguibile(figlio_finto(), dir.path().join(modo), move |m| {
            let _ = tx.lock().unwrap().send(m);
        });
        (g, rx, dir)
    }

    fn chiuse(rx: &std::sync::mpsc::Receiver<Messaggio>) -> usize {
        rx.try_iter().filter(|m| *m == Messaggio::Chiusa).count()
    }

    #[test]
    fn chiusa_arriva_una_volta_sola() {
        // Il figlio scrive `chiusa` due volte: l'assistente la consegna una volta.
        let (mut g, rx, _dir) = con_figlio("educato");
        g.apri("T", "<p>x</p>").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(Messaggio::Pronta));
        g.chiudi();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(Messaggio::Chiusa));
        drop(g);
        assert_eq!(chiuse(&rx), 0, "una sola Chiusa");

        // Figlio che muore senza dire niente: lo stdout chiuso vale come una `chiusa`.
        let (mut g, rx, _dir) = con_figlio("muto");
        g.apri("T", "<p>x</p>").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(Messaggio::Chiusa));
        drop(g);
        assert_eq!(chiuse(&rx), 0, "una sola Chiusa");
    }

    #[test]
    fn un_figlio_appeso_non_blocca_l_uscita_e_viene_ucciso() {
        let (mut g, rx, dir) = con_figlio("appeso");
        g.apri("T", "<p>x</p>").unwrap();
        // Il figlio è partito (ha scritto il suo PID).
        let pid_file = dir.path().join("pid.txt");
        // Il file può esistere ancora vuoto o scritto a metà: si rilegge finché è un numero.
        let inizio = Instant::now();
        let pid: u32 = loop {
            let letto = std::fs::read_to_string(&pid_file).ok().and_then(|t| t.trim().parse().ok());
            if let Some(pid) = letto {
                break pid;
            }
            assert!(inizio.elapsed() < Duration::from_secs(10), "il figlio non ha scritto il suo PID");
            std::thread::sleep(Duration::from_millis(20));
        };

        // L'uscita dell'assistente (Drop) torna entro la scadenza, anche col figlio appeso.
        let (fatto, finito) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(g);
            let _ = fatto.send(());
        });
        assert!(finito.recv_timeout(ATTESA_USCITA + Duration::from_secs(5)).is_ok(), "uscita bloccata");
        assert_eq!(chiuse(&rx), 1);
        assert!(!vivo(pid), "il figlio appeso è stato ucciso e raccolto");
    }

    /// Vero se il processo `pid` è ancora in vita.
    #[cfg(windows)]
    fn vivo(pid: u32) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        // SAFETY: handle aperto e chiuso qui; codice d'uscita in una variabile locale.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h == 0 {
                return false;
            }
            let mut codice = 0u32;
            let ok = GetExitCodeProcess(h, &mut codice) != 0;
            CloseHandle(h);
            ok && codice == STILL_ACTIVE as u32
        }
    }

    #[cfg(not(windows))]
    fn vivo(pid: u32) -> bool {
        Path::new(&format!("/proc/{pid}")).exists()
    }
}
