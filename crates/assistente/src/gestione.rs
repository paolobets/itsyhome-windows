//! Finestra di gestione (spec §3bis): un processo separato, `itsyhome-gestione`, accanto
//! all'eseguibile dell'assistente. Nasce alla prima `apri`, muore alla `chiudi` o quando l'utente
//! chiude la finestra; parla con l'assistente su stdin/stdout (una riga JSON per messaggio, vedi
//! `gestione-protocollo`). Il processo non riceve token né configurazione: solo la cartella dei
//! dati del motore web, sotto la cartella dell'assistente.
//!
//! Per ogni processo un thread «guardiano» legge lo stdout del figlio: un `chiusa` o lo stdout
//! chiuso vogliono dire finestra chiusa; poi il guardiano raccoglie il figlio (`wait`), così non
//! restano processi zombie. Se l'assistente muore, lo stdin del figlio si chiude e il figlio esce
//! da solo.

use gestione_protocollo::{messaggio_da_riga, riga_da_messaggio, Messaggio};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const NOME_ESEGUIBILE: &str = if cfg!(windows) { "itsyhome-gestione.exe" } else { "itsyhome-gestione" };

/// Cartella dei dati del motore web (WebView2), dentro quella dell'assistente: segue quindi
/// anche `ITSYHOME_ASSISTENTE_DIR` nelle prove.
pub fn cartella_dati(cartella_assistente: &Path) -> PathBuf {
    cartella_assistente.join("gestione-webview")
}

/// Quanto si aspetta l'uscita del figlio dopo la chiusura della finestra prima di forzarla.
const ATTESA_USCITA: Duration = Duration::from_secs(5);

type AlMessaggio = Arc<dyn Fn(Messaggio) + Send + Sync>;

pub struct Gestione {
    eseguibile: PathBuf,
    dati: PathBuf,
    al_messaggio: AlMessaggio,
    figlio: Option<Figlio>,
    /// Guardiani dei processi già chiusi o in chiusura: si aspettano all'uscita.
    guardiani: Vec<JoinHandle<()>>,
}

struct Figlio {
    stdin: ChildStdin,
    /// Falso appena la finestra si chiude (`chiusa` o stdout chiuso).
    viva: Arc<AtomicBool>,
    guardiano: JoinHandle<()>,
}

impl Gestione {
    /// `al_messaggio` riceve, dal thread del guardiano, `pronta`, `azione` e `chiusa` (una volta
    /// per processo, anche se il figlio è morto senza dirlo).
    pub fn new(cartella_assistente: &Path, al_messaggio: impl Fn(Messaggio) + Send + Sync + 'static) -> Self {
        let eseguibile = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join(NOME_ESEGUIBILE)))
            .unwrap_or_else(|| PathBuf::from(NOME_ESEGUIBILE));
        Self::con_eseguibile(eseguibile, cartella_dati(cartella_assistente), al_messaggio)
    }

    pub fn con_eseguibile(
        eseguibile: PathBuf,
        dati: PathBuf,
        al_messaggio: impl Fn(Messaggio) + Send + Sync + 'static,
    ) -> Self {
        Self { eseguibile, dati, al_messaggio: Arc::new(al_messaggio), figlio: None, guardiani: Vec::new() }
    }

    /// Mostra il contenuto: avvia il processo se non c'è (o se la sua finestra è stata chiusa),
    /// poi gli manda `apri`.
    pub fn apri(&mut self, titolo: &str, html: &str) -> Result<(), String> {
        self.guardiani.retain(|g| !g.is_finished());
        if self.figlio.as_ref().is_some_and(|f| !f.viva.load(Ordering::SeqCst)) {
            self.congeda();
        }
        let messaggio = Messaggio::Apri { titolo: titolo.to_owned(), html: html.to_owned() };
        if self.figlio.is_none() {
            self.figlio = Some(self.avvia()?);
        }
        let figlio = self.figlio.as_mut().expect("figlio appena avviato");
        if let Err(e) = scrivi(&mut figlio.stdin, &messaggio) {
            // Morto tra il controllo e la scrittura: lo si congeda e si riprova una volta.
            eprintln!("finestra di gestione: invio non riuscito ({e}), nuovo avvio");
            self.congeda();
            let mut figlio = self.avvia()?;
            scrivi(&mut figlio.stdin, &messaggio).map_err(|e| format!("finestra di gestione: {e}"))?;
            self.figlio = Some(figlio);
        }
        Ok(())
    }

    /// Chiude la finestra: `chiudi` e stdin chiuso; il guardiano raccoglie il processo.
    pub fn chiudi(&mut self) {
        if let Some(figlio) = self.figlio.as_mut() {
            let _ = scrivi(&mut figlio.stdin, &Messaggio::Chiudi);
        }
        self.congeda();
    }

    /// Lascia andare il figlio corrente: stdin chiuso (il figlio esce comunque), guardiano tenuto
    /// da parte fino all'uscita.
    fn congeda(&mut self) {
        if let Some(figlio) = self.figlio.take() {
            drop(figlio.stdin);
            self.guardiani.push(figlio.guardiano);
        }
    }

    fn avvia(&self) -> Result<Figlio, String> {
        if !self.eseguibile.is_file() {
            return Err(format!("finestra di gestione: eseguibile non trovato: {}", self.eseguibile.display()));
        }
        let mut comando = Command::new(&self.eseguibile);
        comando
            .arg("--dati")
            .arg(&self.dati)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            // Niente configurazione dell'assistente nel figlio.
            .env_remove(crate::avvio::VAR_CARTELLA)
            .env_remove("ITSYHOME_PROVA_NOTIFICHE");
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
        let viva = Arc::new(AtomicBool::new(true));
        let guardiano = {
            let viva = viva.clone();
            let al_messaggio = self.al_messaggio.clone();
            std::thread::Builder::new()
                .name("gestione".into())
                .spawn(move || guarda(processo, stdout, viva, al_messaggio))
                .map_err(|e| format!("finestra di gestione: guardiano non avviato: {e}"))?
        };
        Ok(Figlio { stdin, viva, guardiano })
    }
}

impl Drop for Gestione {
    fn drop(&mut self) {
        self.chiudi();
        for guardiano in self.guardiani.drain(..) {
            let _ = guardiano.join();
        }
    }
}

fn scrivi(stdin: &mut ChildStdin, m: &Messaggio) -> std::io::Result<()> {
    writeln!(stdin, "{}", riga_da_messaggio(m))?;
    stdin.flush()
}

/// Thread guardiano di un processo: inoltra i messaggi, poi raccoglie il figlio.
fn guarda(mut processo: Child, stdout: std::process::ChildStdout, viva: Arc<AtomicBool>, al_messaggio: AlMessaggio) {
    for riga in BufReader::new(stdout).lines() {
        let Ok(riga) = riga else { break };
        match messaggio_da_riga(&riga) {
            Ok(Messaggio::Chiusa) => break,
            Ok(m) => al_messaggio(m),
            Err(e) => eprintln!("finestra di gestione: {e}"),
        }
    }
    // `chiusa` o stdout chiuso: per l'assistente la finestra non c'è più.
    viva.store(false, Ordering::SeqCst);
    al_messaggio(Messaggio::Chiusa);
    let limite = Instant::now() + ATTESA_USCITA;
    loop {
        match processo.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < limite => std::thread::sleep(Duration::from_millis(50)),
            _ => break,
        }
    }
    eprintln!("finestra di gestione: il processo non esce da solo, lo si chiude");
    let _ = processo.kill();
    let _ = processo.wait();
}

// ─── Prova: `--prova-gestione` ───

thread_local! {
    static PROVA: RefCell<Option<Gestione>> = const { RefCell::new(None) };
}

const MAIL_DI_PROVA: &str = include_str!("../../gestione/assets/prova-mail.html");

/// `--prova-gestione`: apre la finestra con la mail di prova a 3 s (prima c'è la misura
/// dell'assistente da solo), la chiude a 10 s e la riapre a 18 s. Tempi di apertura nel log.
pub fn prova(cartella_assistente: &Path) {
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
    let apri = move || {
        if let Ok(mut t) = aperta_alle.lock() {
            *t = Some(Instant::now());
        }
        PROVA.with_borrow_mut(|p| {
            if let Some(Err(e)) = p.as_mut().map(|g| g.apri("Riepilogo del tuo ordine", MAIL_DI_PROVA)) {
                eprintln!("{e}");
            }
        });
    };
    let riapri = apri.clone();
    slint::Timer::single_shot(Duration::from_secs(3), apri);
    slint::Timer::single_shot(Duration::from_secs(10), || {
        PROVA.with_borrow_mut(|p| p.as_mut().map(Gestione::chiudi));
    });
    slint::Timer::single_shot(Duration::from_secs(18), riapri);
}

/// All'uscita dell'assistente: chiude la finestra di prova e ne raccoglie il processo.
pub fn spegni() {
    drop(PROVA.with_borrow_mut(Option::take));
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn senza_eseguibile_apri_e_un_errore_non_un_panico() {
        let dir = tempfile::tempdir().unwrap();
        let mut g = Gestione::con_eseguibile(dir.path().join(NOME_ESEGUIBILE), dir.path().join("dati"), |_| {});
        let errore = g.apri("T", "<p>x</p>").unwrap_err();
        assert!(errore.contains("non trovato"), "{errore}");
        g.chiudi();
    }

    #[test]
    fn i_dati_del_motore_web_stanno_nella_cartella_dell_assistente() {
        assert_eq!(cartella_dati(Path::new("C:/prova")), Path::new("C:/prova").join("gestione-webview"));
    }
}
