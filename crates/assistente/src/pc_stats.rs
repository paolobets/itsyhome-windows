//! Statistiche del PC (CPU, RAM, dischi, rete, batteria) per i quadranti e la vista PC.
//!
//! Il campionatore nasce all'apertura del pannello e muore alla chiusura: fuori da lì non si
//! misura nulla, salvo la batteria (`sorveglia_batteria`, ogni 60 s). Refresh sempre mirati (mai
//! `refresh_all`, mai l'elenco dei processi).
//!
//! CPU su Windows da `GetSystemTimes`, non da sysinfo: sysinfo la legge con PDH (contatori delle
//! prestazioni), che al primo uso carica le sue tabelle e non le libera più, nemmeno chiudendo la
//! query. Restavano ~7 MB e ~100 handle dopo la prima apertura del pannello (misura del Task 14a).

use starship_battery::units::ratio::ratio;
use starship_battery::{Manager, State};
use std::time::{Duration, Instant};

/// Ogni quanto si legge la batteria a pannello chiuso.
pub const PERIODO_BATTERIA: Duration = Duration::from_secs(60);

/// In che stato è la batteria (dal `State` di starship_battery).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatoBatteria {
    InCarica,
    Piena,
    Scarica,
    Sconosciuto,
}

impl From<State> for StatoBatteria {
    fn from(s: State) -> Self {
        match s {
            State::Charging => StatoBatteria::InCarica,
            State::Full => StatoBatteria::Piena,
            State::Discharging => StatoBatteria::Scarica,
            _ => StatoBatteria::Sconosciuto,
        }
    }
}

/// Una lettura della batteria: percentuale (0..100) e stato.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LetturaBatteria {
    pub percentuale: f32,
    pub stato: StatoBatteria,
}

impl LetturaBatteria {
    /// Si sta scaricando: l'alimentatore non c'è.
    pub fn scollegata(&self) -> bool {
        self.stato == StatoBatteria::Scarica
    }

    /// Alimentatore collegato (in carica o piena).
    pub fn collegata(&self) -> bool {
        matches!(self.stato, StatoBatteria::InCarica | StatoBatteria::Piena)
    }
}

/// Tinta del glifo della batteria. Numerazione fissa: è `tinta-batteria` di `StatPcUi` (0..3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TintaBatteria {
    Carica = 0,
    Normale = 1,
    Informazione = 2,
    Attenzione = 3,
}

/// Da questa percentuale in giù (scollegata) il glifo vira all'informazione (≤: a 20% esatti è
/// già ambra, mentre la voce parte solo sotto il 20%).
pub const SOGLIA_BATTERIA_INFO: f32 = 20.0;
/// Una voce «informazione» già presente sparisce solo da questa percentuale in su.
pub const SOGLIA_BATTERIA_INFO_FINE: f32 = 22.0;

/// In carica o piena: `Carica`; scarica ≤ 10: `Attenzione`; scarica ≤ 20: `Informazione`; il resto
/// `Normale` (anche con stato sconosciuto: nessun allarme senza sapere se è scollegata).
pub fn tinta_batteria(l: &LetturaBatteria) -> TintaBatteria {
    if l.collegata() {
        TintaBatteria::Carica
    } else if !l.scollegata() {
        TintaBatteria::Normale
    } else if l.percentuale <= crate::attenzione::SOGLIA_BATTERIA {
        TintaBatteria::Attenzione
    } else if l.percentuale <= SOGLIA_BATTERIA_INFO {
        TintaBatteria::Informazione
    } else {
        TintaBatteria::Normale
    }
}

/// Legge la batteria subito e poi ogni 60 s su un thread che per il resto dorme, e consegna
/// la lettura a `consegna`; si ferma quando `consegna` dà falso (interfaccia
/// chiusa). Senza batteria alla prima lettura (PC fisso) o senza accesso alle batterie il thread
/// finisce subito; più avanti una lettura mancata si riprova al minuto dopo.
pub fn sorveglia_batteria(consegna: impl Fn(LetturaBatteria) -> bool + Send + 'static) {
    let avviato = std::thread::Builder::new().name("batteria".into()).spawn(move || {
        let Ok(gestore) = Manager::new() else { return };
        // Nessuna batteria alla prima lettura = PC fisso: il thread finisce. Dopo, una lettura
        // mancata non lo ferma: si riprova al giro successivo.
        if leggi_batteria(&gestore).is_none() {
            return;
        }
        loop {
            if let Some(lettura) = leggi_batteria(&gestore) {
                if !consegna(lettura) {
                    return;
                }
            }
            std::thread::sleep(PERIODO_BATTERIA);
        }
    });
    if let Err(e) = avviato {
        eprintln!("controllo della batteria non avviato: {e}");
    }
}

/// La prima batteria: percentuale e stato. `None` senza batteria.
fn leggi_batteria(gestore: &Manager) -> Option<LetturaBatteria> {
    let b = gestore.batteries().ok()?.next()?.ok()?;
    let percentuale = (b.state_of_charge().get::<ratio>() * 100.0).clamp(0.0, 100.0);
    Some(LetturaBatteria { percentuale, stato: b.state().into() })
}
use sysinfo::{DiskRefreshKind, Disks, MemoryRefreshKind, Networks, System};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatPc {
    /// `None` se la CPU non si è potuta leggere (GetSystemTimes fallito): «—», mai «0%».
    pub cpu_pct: Option<f32>,
    pub ram_usata: u64,
    pub ram_totale: u64,
    pub disco_usato: u64,
    pub disco_totale: u64,
    pub rete_giu_bps: u64,
    pub rete_su_bps: u64,
    /// `None` senza batteria (PC fisso) o se non si legge.
    pub batteria: Option<LetturaBatteria>,
}

pub struct Campionatore {
    sistema: System,
    /// Ultima lettura dei tempi di sistema (CPU su Windows).
    #[cfg(windows)]
    tempi: Option<TempiCpu>,
    /// L'ultima percentuale calcolata: resta se fra due letture non passa tempo di CPU.
    #[cfg(windows)]
    ultima_cpu: Option<f32>,
    dischi: Disks,
    reti: Networks,
    batterie: Option<Manager>,
    ultimo: Instant,
    campioni: u32,
}

impl Default for Campionatore {
    fn default() -> Self {
        Self::new()
    }
}

impl Campionatore {
    pub fn new() -> Self {
        #[cfg_attr(windows, allow(unused_mut))]
        let mut sistema = System::new();
        // Base di partenza per la CPU: la percentuale è la differenza tra due letture.
        #[cfg(not(windows))]
        sistema.refresh_cpu_usage();
        Campionatore {
            sistema,
            #[cfg(windows)]
            tempi: tempi_cpu(),
            #[cfg(windows)]
            ultima_cpu: None,
            dischi: Disks::new_with_refreshed_list_specifics(disco_mirato()),
            reti: Networks::new_with_refreshed_list(),
            batterie: Manager::new().ok(),
            ultimo: Instant::now(),
            campioni: 0,
        }
    }

    /// Vero dal secondo campione: la CPU è una differenza tra due letture, il primo valore non conta.
    pub fn cpu_pronta(&self) -> bool {
        self.campioni >= 2
    }

    pub fn campiona(&mut self) -> StatPc {
        let cpu_pct = self.cpu();
        self.sistema.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        self.dischi.refresh_specifics(true, disco_mirato());
        self.reti.refresh(true);

        // Byte arrivati/partiti dall'ultimo campione, divisi per il tempo trascorso.
        let ora = Instant::now();
        let secondi = ora.duration_since(self.ultimo).as_secs_f64().max(0.001);
        self.ultimo = ora;
        let (mut giu, mut su) = (0u64, 0u64);
        for (nome, dati) in self.reti.iter() {
            if e_loopback(nome) {
                continue;
            }
            giu = giu.saturating_add(dati.received());
            su = su.saturating_add(dati.transmitted());
        }
        let (rete_giu_bps, rete_su_bps) = if self.campioni == 0 {
            (0, 0)
        } else {
            ((giu as f64 / secondi) as u64, (su as f64 / secondi) as u64)
        };

        let (mut disco_totale, mut disco_libero) = (0u64, 0u64);
        for d in self.dischi.list().iter().filter(|d| !d.is_removable()) {
            disco_totale = disco_totale.saturating_add(d.total_space());
            disco_libero = disco_libero.saturating_add(d.available_space());
        }

        let batteria = self.batteria();
        self.campioni = self.campioni.saturating_add(1);

        let ram_totale = self.sistema.total_memory();
        StatPc {
            cpu_pct,
            ram_usata: self.sistema.used_memory().min(ram_totale),
            ram_totale,
            disco_usato: disco_totale.saturating_sub(disco_libero),
            disco_totale,
            rete_giu_bps,
            rete_su_bps,
            batteria,
        }
    }

    /// Percentuale di CPU dall'ultima lettura; `None` se GetSystemTimes non risponde (adesso o
    /// alla lettura prima): l'interfaccia mostra «—», non uno «0%» inventato (vedi `cpu_da`).
    #[cfg(windows)]
    fn cpu(&mut self) -> Option<f32> {
        let ora = tempi_cpu();
        let pct = cpu_da(self.tempi, ora, self.ultima_cpu);
        if ora.is_some() {
            self.tempi = ora;
        }
        self.ultima_cpu = pct;
        pct
    }

    #[cfg(not(windows))]
    fn cpu(&mut self) -> Option<f32> {
        self.sistema.refresh_cpu_usage();
        Some(self.sistema.global_cpu_usage().clamp(0.0, 100.0))
    }

    /// La prima batteria del PC; `None` se il PC non ne ha (desktop) o non si legge.
    fn batteria(&self) -> Option<LetturaBatteria> {
        leggi_batteria(self.batterie.as_ref()?)
    }
}

/// Tempi cumulativi del sistema (unità di 100 ns): inattivo, kernel (inattivo compreso), utente.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TempiCpu {
    inattivo: u64,
    kernel: u64,
    utente: u64,
}

/// La CPU fra la lettura di prima e quella di adesso: `None` se una delle due manca (GetSystemTimes
/// fallito). Se fra le due non è passato tempo di CPU (intervallo nullo) non c'è una differenza da
/// cui calcolare, ma nemmeno un errore: resta `ultima`, il valore di prima.
#[cfg_attr(not(windows), allow(dead_code))]
fn cpu_da(prima: Option<TempiCpu>, ora: Option<TempiCpu>, ultima: Option<f32>) -> Option<f32> {
    let (prima, ora) = (prima?, ora?);
    let totale = ora.kernel.saturating_sub(prima.kernel) + ora.utente.saturating_sub(prima.utente);
    if totale == 0 {
        return ultima;
    }
    Some(percentuale_cpu(prima, ora))
}

/// Percentuale di CPU occupata tra due letture: (kernel + utente − inattivo) / (kernel + utente).
#[cfg_attr(not(windows), allow(dead_code))]
fn percentuale_cpu(prima: TempiCpu, ora: TempiCpu) -> f32 {
    let totale = ora.kernel.saturating_sub(prima.kernel) + ora.utente.saturating_sub(prima.utente);
    if totale == 0 {
        return 0.0;
    }
    let inattivo = ora.inattivo.saturating_sub(prima.inattivo).min(totale);
    ((totale - inattivo) as f64 / totale as f64 * 100.0) as f32
}

#[cfg(windows)]
fn tempi_cpu() -> Option<TempiCpu> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut inattivo, mut kernel, mut utente) = (zero, zero, zero);
    // SAFETY: tre FILETIME locali, validi per tutta la chiamata.
    if unsafe { GetSystemTimes(&mut inattivo, &mut kernel, &mut utente) } == 0 {
        return None;
    }
    let valore = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    Some(TempiCpu { inattivo: valore(inattivo), kernel: valore(kernel), utente: valore(utente) })
}

fn disco_mirato() -> DiskRefreshKind {
    DiskRefreshKind::nothing().with_storage()
}

fn e_loopback(nome: &str) -> bool {
    let n = nome.to_lowercase();
    n == "lo" || n.starts_with("lo0") || n.contains("loopback")
}

/// Un numero con al più una cifra decimale e la virgola italiana ("1,5", "512").
fn numero(v: f64) -> String {
    let arrotondato = (v * 10.0).round() / 10.0;
    if arrotondato.fract() == 0.0 {
        format!("{arrotondato:.0}")
    } else {
        format!("{arrotondato:.1}").replace('.', ",")
    }
}

/// Base 1024, una cifra decimale, virgola italiana: "0 B", "512 MB", "1,5 GB".
pub fn formatta_byte(b: u64) -> String {
    const UNITA: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut valore = b as f64;
    let mut i = 0;
    // Si sale di unità anche quando l'arrotondamento darebbe "1024".
    while (valore * 10.0).round() / 10.0 >= 1024.0 && i < UNITA.len() - 1 {
        valore /= 1024.0;
        i += 1;
    }
    format!("{} {}", numero(valore), UNITA[i])
}

/// Come `formatta_byte`, al secondo: "1,2 MB/s".
pub fn formatta_velocita(bps: u64) -> String {
    format!("{}/s", formatta_byte(bps))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn formatta_byte_casi() {
        assert_eq!(formatta_byte(0), "0 B");
        assert_eq!(formatta_byte(512 * 1024 * 1024), "512 MB");
        assert_eq!(formatta_byte(1_610_612_736), "1,5 GB");
        assert_eq!(formatta_byte(1023), "1023 B");
        assert_eq!(formatta_byte(1024), "1 KB");
        // L'arrotondamento non deve mai dare "1024 KB".
        assert_eq!(formatta_byte(1024 * 1024 - 1), "1 MB");
    }

    #[test]
    fn formatta_velocita_casi() {
        assert_eq!(formatta_velocita(1_258_291), "1,2 MB/s");
        assert_eq!(formatta_velocita(0), "0 B/s");
    }

    #[test]
    fn campione_plausibile() {
        let mut c = Campionatore::new();
        assert!(!c.cpu_pronta());
        c.campiona();
        std::thread::sleep(Duration::from_millis(300));
        let s = c.campiona();
        assert!(c.cpu_pronta());
        let cpu = s.cpu_pct.expect("GetSystemTimes leggibile");
        assert!((0.0..=100.0).contains(&cpu), "cpu {cpu}");
        assert!(s.ram_totale > 0);
        assert!(s.ram_usata <= s.ram_totale);
        assert!(s.disco_usato <= s.disco_totale);
        if let Some(b) = s.batteria {
            assert!((0.0..=100.0).contains(&b.percentuale));
        }
    }

    #[test]
    fn tinta_della_batteria() {
        let tinta = |percentuale, stato| tinta_batteria(&LetturaBatteria { percentuale, stato });
        assert_eq!(tinta(50.0, StatoBatteria::InCarica), TintaBatteria::Carica);
        assert_eq!(tinta(15.0, StatoBatteria::Piena), TintaBatteria::Carica);
        assert_eq!(tinta(10.0, StatoBatteria::Scarica), TintaBatteria::Attenzione);
        assert_eq!(tinta(20.0, StatoBatteria::Scarica), TintaBatteria::Informazione);
        assert_eq!(tinta(21.0, StatoBatteria::Scarica), TintaBatteria::Normale);
        assert_eq!(tinta(5.0, StatoBatteria::Sconosciuto), TintaBatteria::Normale);
    }

    #[test]
    fn cpu_dai_tempi_di_sistema() {
        // Il tempo del kernel comprende l'inattività: 1000 di kernel di cui 600 inattivi + 1000
        // di utente = 1400 occupati su 2000.
        let prima = TempiCpu { inattivo: 1_000, kernel: 5_000, utente: 3_000 };
        let ora = TempiCpu { inattivo: 1_600, kernel: 6_000, utente: 4_000 };
        assert_eq!(percentuale_cpu(prima, ora), 70.0);
        assert_eq!(percentuale_cpu(prima, prima), 0.0, "nessun tempo passato: 0, non NaN");
        let tutto_inattivo = TempiCpu { inattivo: 2_000, kernel: 6_000, utente: 3_000 };
        assert_eq!(percentuale_cpu(prima, tutto_inattivo), 0.0);
        // Contatori che tornano indietro (non dovrebbe succedere): mai fuori da 0..100.
        assert!((0.0..=100.0).contains(&percentuale_cpu(ora, prima)));
    }

    #[test]
    fn cpu_illeggibile_e_nessun_valore() {
        // Il codice che legge i tempi, non la presentazione: con GetSystemTimes fallito niente
        // numero (l'interfaccia mostra «—»), mai «0%».
        let prima = TempiCpu { inattivo: 1_000, kernel: 5_000, utente: 3_000 };
        let ora = TempiCpu { inattivo: 1_600, kernel: 6_000, utente: 4_000 };
        assert_eq!(cpu_da(Some(prima), Some(ora), None), Some(70.0));
        assert_eq!(cpu_da(Some(prima), None, Some(42.0)), None, "lettura di adesso fallita");
        assert_eq!(cpu_da(None, Some(ora), Some(42.0)), None, "lettura di prima fallita");
    }

    #[test]
    fn intervallo_nullo_tiene_l_ultimo_valore() {
        // Due letture riuscite senza tempo di CPU fra loro non sono un errore: resta il valore di
        // prima (un «—» lo farebbe sembrare illeggibile); senza un valore di prima, nessuno.
        let prima = TempiCpu { inattivo: 1_000, kernel: 5_000, utente: 3_000 };
        assert_eq!(cpu_da(Some(prima), Some(prima), Some(42.0)), Some(42.0));
        assert_eq!(cpu_da(Some(prima), Some(prima), None), None);
    }

    #[test]
    fn loopback_escluso() {
        assert!(e_loopback("Loopback Pseudo-Interface 1"));
        assert!(e_loopback("lo"));
        assert!(!e_loopback("Wi-Fi"));
    }
}
