//! Il file del registro diagnostico (Task 14g): con `ITSYHOME_DIAGNOSI=1` un thread che dorme
//! scrive ogni 60 s una riga di soli contatori (vedi `casa_core::diagnosi`) in `diagnosi.log`,
//! nella cartella della configurazione (accanto ad `assistente.json`; con
//! `ITSYHOME_ASSISTENTE_DIR` nella cartella di prova). Oltre 1 MB il file diventa
//! `diagnosi.log.1` (quello di prima si cancella) e si riparte da vuoto: al massimo 2 MB.
//!
//! Ogni riga: data e ora locali, i contatori dell'ultimo minuto, poi la CPU usata dal processo
//! nel minuto (`cpu_ms`) e la sua memoria privata (`ram_privata_kb`). Mai dati della casa.
use casa_core::diagnosi::Diagnosi;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// La variabile d'ambiente che accende il registro («1»).
pub const VARIABILE: &str = "ITSYHOME_DIAGNOSI";
/// Il nome del file, nella cartella della configurazione.
pub const FILE: &str = "diagnosi.log";
/// Oltre questa dimensione il file ruota.
pub const TETTO: u64 = 1024 * 1024;
const OGNI: Duration = Duration::from_secs(60);
/// La riga scritta quando la rotazione non riesce e il file si svuota sul posto.
pub const ROTAZIONE_NON_RIUSCITA: &str = "rotazione_non_riuscita=1 file_svuotato=1";

/// Acceso solo con la variabile a «1».
pub fn acceso(valore: Option<&OsStr>) -> bool {
    valore.is_some_and(|v| v == "1")
}

/// Aggiunge una riga al file; se con la riga supererebbe `tetto`, il file diventa `<file>.1`
/// (quello di prima si perde) e la riga apre un file nuovo. Se il file non si può rinominare (un
/// lettore lo tiene aperto senza permetterlo, come `Get-Content -Wait`), si svuota sul posto e lo
/// dice con una riga: il registro non si ferma. La cartella si crea se manca.
pub fn scrivi(file: &Path, riga: &str, tetto: u64) -> std::io::Result<()> {
    if let Some(cartella) = file.parent() {
        std::fs::create_dir_all(cartella)?;
    }
    let lungo = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if lungo > 0 && lungo + riga.len() as u64 + 1 > tetto {
        let vecchio = vecchio(file);
        let _ = std::fs::remove_file(&vecchio);
        if std::fs::rename(file, &vecchio).is_err() {
            let mut f = std::fs::OpenOptions::new().write(true).truncate(true).open(file)?;
            writeln!(f, "{ROTAZIONE_NON_RIUSCITA}")?;
        }
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(file)?;
    writeln!(f, "{riga}")
}

fn vecchio(file: &Path) -> PathBuf {
    let mut nome = file.as_os_str().to_owned();
    nome.push(".1");
    PathBuf::from(nome)
}

fn ora() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Avvia il thread che scrive una riga al minuto. Dorme fra una riga e l'altra: nessun lavoro
/// sul thread dell'interfaccia.
pub fn avvia(diagnosi: Arc<Diagnosi>, cartella: &Path) {
    let file = cartella.join(FILE);
    let avviato = std::thread::Builder::new().name("diagnosi".into()).spawn(move || {
        let _ = scrivi(&file, &format!("{} avvio intervallo_s={}", ora(), OGNI.as_secs()), TETTO);
        let mut cpu_prima = processo::cpu_ms();
        loop {
            std::thread::sleep(OGNI);
            let cpu = processo::cpu_ms();
            let misure = [("cpu_ms", cpu.saturating_sub(cpu_prima)), ("ram_privata_kb", processo::privata_kb())];
            cpu_prima = cpu;
            if let Err(e) = scrivi(&file, &format!("{} {}", ora(), diagnosi.riga(&misure)), TETTO) {
                eprintln!("registro diagnostico non scritto: {e}");
            }
        }
    });
    if let Err(e) = avviato {
        eprintln!("registro diagnostico non avviato: {e}");
    }
}

/// CPU usata dal processo dall'avvio, in millisecondi (0 dove non si misura).
pub fn cpu_processo_ms() -> u64 {
    processo::cpu_ms()
}

#[cfg(windows)]
mod processo {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    /// CPU usata dal processo dall'avvio (utente + nucleo), in millisecondi.
    pub fn cpu_ms() -> u64 {
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut creato, mut uscito, mut nucleo, mut utente) = (zero, zero, zero, zero);
        // SAFETY: pseudo-handle del processo corrente, puntatori a variabili locali.
        let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut creato, &mut uscito, &mut nucleo, &mut utente) };
        if ok == 0 {
            return 0;
        }
        let cento_ns = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        (cento_ns(nucleo) + cento_ns(utente)) / 10_000
    }

    /// Memoria privata del processo (quella che misura il controllore), in KB.
    pub fn privata_kb() -> u64 {
        // SAFETY: struttura locale della dimensione dichiarata in `cb`.
        unsafe {
            let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            let p = &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS;
            if K32GetProcessMemoryInfo(GetCurrentProcess(), p, c.cb) == 0 {
                return 0;
            }
            c.PrivateUsage as u64 / 1024
        }
    }
}

#[cfg(not(windows))]
mod processo {
    pub fn cpu_ms() -> u64 {
        0
    }
    pub fn privata_kb() -> u64 {
        0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn acceso_solo_con_uno() {
        assert!(acceso(Some(OsStr::new("1"))));
        for v in ["", "0", "si", "true"] {
            assert!(!acceso(Some(OsStr::new(v))), "{v}");
        }
        assert!(!acceso(None));
    }

    #[test]
    fn oltre_il_tetto_il_file_ruota_e_resta_al_massimo_un_vecchio() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        let riga = "x".repeat(39); // 40 byte con l'a capo
        for _ in 0..5 {
            scrivi(&file, &riga, 100).unwrap();
        }
        // 2 righe (80 byte) per file: la quinta apre il terzo file, il primo è perso.
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 40);
        assert_eq!(std::fs::metadata(vecchio(&file)).unwrap().len(), 80);
        let nomi: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|v| v.unwrap().file_name()).collect();
        assert_eq!(nomi.len(), 2, "{nomi:?}");
    }

    /// Il file si scrive anche in una cartella che non esiste ancora (configurazione mai salvata).
    #[test]
    fn la_cartella_si_crea() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nuova").join("sotto").join(FILE);
        scrivi(&file, "riga", TETTO).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "riga\n");
    }

    /// Un lettore che tiene il file aperto senza permettere di rinominarlo (`Get-Content -Wait`, un
    /// visualizzatore): la rotazione non riesce, ma il registro non si ferma. Il file si svuota e
    /// lo dice, poi la riga si scrive.
    #[cfg(windows)]
    #[test]
    fn rotazione_con_il_file_aperto_da_un_lettore() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        let riga = "x".repeat(39);
        scrivi(&file, &riga, 100).unwrap();
        scrivi(&file, &riga, 100).unwrap();
        // Condivisione lettura e scrittura, niente cancellazione (né rinomina).
        let _lettore = std::fs::OpenOptions::new().read(true).share_mode(0x1 | 0x2).open(&file).unwrap();
        // Oltre il tetto (80 + 40 > 100): andrebbe ruotato.
        let nuova = "y".repeat(39);
        scrivi(&file, &nuova, 100).unwrap();
        let testo = std::fs::read_to_string(&file).unwrap();
        assert_eq!(testo, format!("{ROTAZIONE_NON_RIUSCITA}\n{nuova}\n"));
        assert!(!vecchio(&file).exists());
    }

    /// `cpu_ms` cresce mentre il processo lavora (la risoluzione dei tempi di Windows è di circa
    /// 15,6 ms: si lavora finché cresce, al più 2 s).
    #[cfg(windows)]
    #[test]
    fn misure_del_processo_plausibili() {
        assert!(processo::privata_kb() > 100);
        let prima = processo::cpu_ms();
        let inizio = std::time::Instant::now();
        let mut x = 0u64;
        while processo::cpu_ms() <= prima && inizio.elapsed() < Duration::from_secs(2) {
            for i in 0..1_000_000u64 {
                x = x.wrapping_add(i * i);
            }
            std::hint::black_box(x);
        }
        assert!(processo::cpu_ms() > prima, "cpu_ms fermo a {prima} dopo {:?} di lavoro", inizio.elapsed());
    }
}
