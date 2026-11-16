//! Statistiche del PC (CPU, RAM, dischi, rete, batteria, GPU) per i quadranti e la vista PC.
//!
//! Il campionatore nasce all'apertura del pannello e muore alla chiusura: fuori da lì non si
//! misura nulla, salvo la batteria (`sorveglia_batteria`, ogni 60 s) e, solo se scelte per i lati
//! della pillola e la pillola si vede, CPU e RAM (`CampionatoreLeggero`, ogni 5 s). Refresh sempre
//! mirati (mai `refresh_all`, mai l'elenco dei processi); dischi e reti solo se una vista li mostra
//! (`Fonti`).
//!
//! CPU su Windows da `GetSystemTimes`, non da sysinfo: sysinfo la legge con PDH (contatori delle
//! prestazioni), che al primo uso carica le sue tabelle e non le libera più, nemmeno chiudendo la
//! query. Restavano ~7 MB e ~100 handle dopo la prima apertura del pannello (misura del Task 14a).
//!
//! GPU (M2, Task 10): vedi il modulo `gpu` in fondo (fonti, vincoli e misura della memoria).

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

/// Tinta del glifo della batteria (in Slint: `TintaBatteriaUi`, vedi `pannello::stat_ui`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TintaBatteria {
    Carica,
    Normale,
    Informazione,
    Attenzione,
}

/// Da questa percentuale in giù (scollegata) il glifo vira all'informazione (blu; ≤: a 20%
/// esatti il glifo è già blu, mentre la voce parte solo sotto il 20%). L'ambra è l'attenzione (≤ 10).
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
    /// Utilizzo della GPU 0..100 (il motore più occupato, come Gestione attività). `None` fuori
    /// da Windows, se la GPU non si mostra (nessuna lettura) o se la lettura non è riuscita.
    pub gpu_pct: Option<f32>,
}

/// Quali fonti lente legge il campionatore del pannello: dischi, reti e GPU solo se qualcosa li
/// mostra (la vista PC, o un quadrante scelto). CPU, RAM e batteria si leggono sempre (costano poco).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fonti {
    pub disco: bool,
    pub rete: bool,
    /// La GPU (M2, Task 10, solo Windows): la query PDH esiste solo finché è vero.
    pub gpu: bool,
}

impl Fonti {
    pub const TUTTE: Fonti = Fonti { disco: true, rete: true, gpu: true };
}

/// Il campionatore dei lati della pillola (M2, Task 4): solo CPU (GetSystemTimes su Windows) e
/// RAM, nessun disco né rete. Vive sul thread dell'interfaccia: le due letture costano
/// microsecondi.
pub struct CampionatoreLeggero {
    sistema: System,
    /// Ultima lettura dei tempi di sistema (CPU su Windows).
    #[cfg(windows)]
    tempi: Option<TempiCpu>,
    /// L'ultima percentuale calcolata: resta se fra due letture non passa tempo di CPU.
    #[cfg(windows)]
    ultima_cpu: Option<f32>,
}

/// Ogni quanto si campionano CPU e RAM per i lati della pillola (solo se scelte e visibili).
pub const PERIODO_LATI: Duration = Duration::from_secs(5);

impl Default for CampionatoreLeggero {
    fn default() -> Self {
        Self::new()
    }
}

impl CampionatoreLeggero {
    pub fn new() -> Self {
        #[cfg_attr(windows, allow(unused_mut))]
        let mut sistema = System::new();
        // Base di partenza per la CPU: la percentuale è la differenza tra due letture.
        #[cfg(not(windows))]
        sistema.refresh_cpu_usage();
        CampionatoreLeggero {
            sistema,
            #[cfg(windows)]
            tempi: tempi_cpu(),
            #[cfg(windows)]
            ultima_cpu: None,
        }
    }

    /// CPU (`None` se illeggibile) e RAM occupata, in percentuale.
    pub fn campiona(&mut self) -> (Option<f32>, f32) {
        let cpu = self.cpu();
        let (usata, totale) = self.ram();
        let ram = if totale == 0 { 0.0 } else { (usata as f64 / totale as f64 * 100.0) as f32 };
        (cpu, ram.clamp(0.0, 100.0))
    }

    /// RAM usata e totale, in byte.
    fn ram(&mut self) -> (u64, u64) {
        self.sistema.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let totale = self.sistema.total_memory();
        (self.sistema.used_memory().min(totale), totale)
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
}

pub struct Campionatore {
    /// CPU e RAM: lo stesso codice dei lati della pillola.
    leggero: CampionatoreLeggero,
    /// Dischi e reti nascono solo quando una vista li mostra (`Fonti`), poi restano fino alla
    /// chiusura del pannello.
    dischi: Option<Disks>,
    reti: Option<Networks>,
    /// Campioni della rete da quando le reti ci sono: la velocità è una differenza fra due.
    campioni_rete: u32,
    batterie: Option<Manager>,
    ultimo: Instant,
    campioni: u32,
    /// La query della GPU: nasce quando la GPU si mostra (`usa_gpu`), muore (con `PdhCloseQuery`)
    /// appena non si mostra più o col campionatore, alla chiusura del pannello.
    #[cfg(windows)]
    gpu: StatoGpu,
}

/// Lo stato della lettura della GPU nel campionatore del pannello.
#[cfg(windows)]
enum StatoGpu {
    /// Nessuna query: la GPU non si mostra.
    Spenta,
    /// Query aperta. L'utilizzo è una differenza fra due raccolte: la prima la fa `apri`; il
    /// campione in cui la query nasce non legge (sarebbe a pochi millisecondi dalla base) e porta
    /// `letture` a 1; dal campione dopo si legge (`letture` 2).
    Accesa { lettore: gpu::LettoreGpu, letture: u8 },
    /// Apertura fallita (niente contatori della GPU, PDH rotto): non si riprova a ogni campione,
    /// solo dopo averla spenta (vista lasciata, pannello chiuso).
    Fallita,
}

impl Default for Campionatore {
    fn default() -> Self {
        Self::new(Fonti::TUTTE)
    }
}

impl Campionatore {
    pub fn new(fonti: Fonti) -> Self {
        let mut c = Campionatore {
            leggero: CampionatoreLeggero::new(),
            dischi: None,
            reti: None,
            campioni_rete: 0,
            batterie: Manager::new().ok(),
            ultimo: Instant::now(),
            campioni: 0,
            #[cfg(windows)]
            gpu: StatoGpu::Spenta,
        };
        c.prepara(fonti);
        c
    }

    /// Accende o spegne la lettura della GPU: con `si` la query PDH nasce (se non c'è già), senza
    /// si chiude (`PdhCloseQuery`). Fuori da Windows non fa nulla (la GPU non c'è).
    pub fn usa_gpu(&mut self, si: bool) {
        #[cfg(windows)]
        match (&self.gpu, si) {
            (StatoGpu::Spenta, true) => {
                self.gpu = match gpu::LettoreGpu::apri() {
                    Some(lettore) => StatoGpu::Accesa { lettore, letture: 0 },
                    None => StatoGpu::Fallita,
                };
            }
            (StatoGpu::Accesa { .. } | StatoGpu::Fallita, false) => self.gpu = StatoGpu::Spenta,
            _ => {}
        }
        #[cfg(not(windows))]
        let _ = si;
    }

    /// Vero se la query della GPU è aperta (solo a GPU mostrata).
    #[cfg(all(test, windows))]
    pub fn legge_gpu(&self) -> bool {
        matches!(self.gpu, StatoGpu::Accesa { .. })
    }

    /// Vero se la GPU è appena stata accesa (`apri` e il primo campione) e il valore arriva al
    /// campione dopo: il thread dei campioni, allora, non aspetta 2 s ma poco (come per la CPU
    /// all'apertura). Dopo la lettura vera torna falso.
    #[cfg(windows)]
    pub fn gpu_in_attesa(&self) -> bool {
        matches!(self.gpu, StatoGpu::Accesa { letture: 1, .. })
    }

    #[cfg(not(windows))]
    pub fn gpu_in_attesa(&self) -> bool {
        false
    }

    /// Un campione della GPU, se accesa: `None` al primo (subito dopo la base) o se fallisce.
    fn gpu(&mut self) -> Option<f32> {
        #[cfg(windows)]
        if let StatoGpu::Accesa { lettore, letture } = &mut self.gpu {
            if *letture == 0 {
                *letture = 1;
                return None;
            }
            *letture = 2;
            return lettore.gpu();
        }
        None
    }

    /// Crea dischi e reti appena una vista li chiede (mai prima).
    fn prepara(&mut self, fonti: Fonti) {
        if fonti.disco && self.dischi.is_none() {
            self.dischi = Some(Disks::new_with_refreshed_list_specifics(disco_mirato()));
        }
        if fonti.rete && self.reti.is_none() {
            self.reti = Some(Networks::new_with_refreshed_list());
            self.campioni_rete = 0;
        }
    }

    /// Vero se il campionatore legge i dischi (nessun quadrante e nessuna vista li mostra: mai).
    #[cfg(test)]
    pub fn legge_dischi(&self) -> bool {
        self.dischi.is_some()
    }

    /// Vero dal secondo campione: la CPU è una differenza tra due letture, il primo valore non conta.
    pub fn cpu_pronta(&self) -> bool {
        self.campioni >= 2
    }

    /// Un campione delle fonti chieste adesso: dischi e reti solo se `fonti` li vuole (a zero
    /// altrimenti: l'interfaccia, allora, non li mostra).
    pub fn campiona(&mut self, fonti: Fonti) -> StatPc {
        self.prepara(fonti);
        self.usa_gpu(fonti.gpu);
        let gpu_pct = self.gpu();
        let cpu_pct = self.leggero.cpu();
        let (ram_usata, ram_totale) = self.leggero.ram();

        // Byte arrivati/partiti dall'ultimo campione, divisi per il tempo trascorso.
        let ora = Instant::now();
        let secondi = ora.duration_since(self.ultimo).as_secs_f64().max(0.001);
        self.ultimo = ora;
        let (mut giu, mut su) = (0u64, 0u64);
        let mut rete_pronta = false;
        if let Some(reti) = self.reti.as_mut().filter(|_| fonti.rete) {
            reti.refresh(true);
            for (nome, dati) in reti.iter() {
                if e_loopback(nome) {
                    continue;
                }
                giu = giu.saturating_add(dati.received());
                su = su.saturating_add(dati.transmitted());
            }
            rete_pronta = self.campioni_rete > 0;
            self.campioni_rete = self.campioni_rete.saturating_add(1);
        }
        let (rete_giu_bps, rete_su_bps) = if rete_pronta {
            ((giu as f64 / secondi) as u64, (su as f64 / secondi) as u64)
        } else {
            (0, 0)
        };

        let (mut disco_totale, mut disco_libero) = (0u64, 0u64);
        if let Some(dischi) = self.dischi.as_mut().filter(|_| fonti.disco) {
            dischi.refresh_specifics(true, disco_mirato());
            for d in dischi.list().iter().filter(|d| !d.is_removable()) {
                disco_totale = disco_totale.saturating_add(d.total_space());
                disco_libero = disco_libero.saturating_add(d.available_space());
            }
        }

        let batteria = self.batteria();
        self.campioni = self.campioni.saturating_add(1);

        StatPc {
            cpu_pct,
            ram_usata,
            ram_totale,
            disco_usato: disco_totale.saturating_sub(disco_libero),
            disco_totale,
            rete_giu_bps,
            rete_su_bps,
            batteria,
            gpu_pct,
        }
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

/// L'utilizzo della GPU dalle istanze del contatore PDH «GPU Engine» (nome dell'istanza e
/// valore, come `pid_1234_luid_0x…_phys_0_eng_0_engtype_3D`): si sommano i valori per tipo di
/// motore (`engtype_*`: 3D, VideoDecode, Copy…, tutti i processi insieme) e si prende il tipo più
/// occupato, come Gestione attività (che mostra il motore più occupato); fuori scala 100. `None`
/// se l'elenco è vuoto (nessun motore: niente GPU o lettura vuota).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn gpu_da_istanze(istanze: &[(String, f64)]) -> Option<f32> {
    if istanze.is_empty() {
        return None;
    }
    let mut per_tipo: Vec<(&str, f64)> = Vec::new();
    for (nome, valore) in istanze {
        let tipo = nome.rsplit_once("engtype_").map_or("", |(_, t)| t);
        let valore = if valore.is_finite() { valore.max(0.0) } else { 0.0 };
        match per_tipo.iter_mut().find(|(t, _)| *t == tipo) {
            Some((_, somma)) => *somma += valore,
            None => per_tipo.push((tipo, valore)),
        }
    }
    let massimo = per_tipo.iter().map(|(_, v)| *v).fold(0.0, f64::max);
    Some(massimo.clamp(0.0, 100.0) as f32)
}

/// La GPU su Windows con PDH (M2, Task 10).
///
/// Documentazione letta (learn.microsoft.com, 16/11/2026):
/// - `PdhOpenQueryW` (pdh.h, Pdh.lib): `szDataSource` NULL = dati in tempo reale; restituisce
///   `ERROR_SUCCESS` (0) o un codice PDH; la query si chiude con `PdhCloseQuery`.
/// - `PdhAddEnglishCounterW` (da Windows Vista): percorso indipendente dalla lingua (il Windows
///   di casa è in italiano: `PdhAddCounterW` vorrebbe i nomi tradotti). Un'istanza che non
///   esiste ancora non è un errore. Errori: `PDH_CSTATUS_NO_OBJECT`, `PDH_CSTATUS_NO_COUNTER`
///   (senza driver grafico WDDM l'oggetto «GPU Engine» non c'è: la riga resta assente).
/// - `PdhCollectQueryData`: PDH tiene il valore grezzo di adesso e quello di prima; i contatori
///   a velocità (come «Utilization Percentage») hanno bisogno di **due** raccolte per un valore.
///   `PDH_NO_DATA` se la query non ha contatori. Con il carattere jolly nell'istanza i valori si
///   leggono tutti insieme con `PdhGetFormattedCounterArrayW`.
/// - `PdhGetFormattedCounterArrayW`: due chiamate, la prima con buffer NULL e dimensione 0 →
///   `PDH_MORE_DATA` e la dimensione in byte; la seconda col buffer. Con una dimensione > 0 ma
///   troppo piccola non ci si deve fidare della dimensione restituita (qui si riparte da zero).
///   I nomi delle istanze stanno nello stesso buffer, dopo le strutture. `PDH_FMT_DOUBLE`; senza
///   `PDH_FMT_NOCAP100` ogni valore è già limitato a 100. I dati sono bloccati durante la
///   chiamata. Si usano solo le istanze con `CStatus` valido (`PDH_CSTATUS_VALID_DATA` o
///   `PDH_CSTATUS_NEW_DATA`): un processo appena nato ha un solo valore grezzo.
/// - `PdhCloseQuery`: chiude tutti i contatori della query, i suoi handle e libera la sua
///   memoria; dopo, i suoi handle non si usano più (qui: `Drop`).
/// - Il contatore «\GPU Engine(*)\Utilization Percentage» non ha una pagina sua su
///   learn.microsoft.com (solo risposte di Microsoft Q&A, secondo cui Gestione attività usa
///   proprio questo percorso col jolly); il blog DirectX «GPUs in the Task Manager» spiega che
///   l'utilizzo complessivo è quello del motore più occupato (vedi `gpu_da_istanze`).
/// - Thread: le funzioni PDH non chiedono COM né un thread particolare; la query vive e muore
///   sul thread dei campioni del pannello (`campioni-pc`), mai su quello dell'interfaccia.
/// - Memoria: PDH al primo uso carica tabelle che non libera più (nota in testa al file). La
///   decisione D7 vuole al massimo +2 MB dopo aver chiuso la vista PC: la misura è nel test
///   `gpu_memoria_dopo_cinque_cicli` (numeri nel CHANGELOG).
/// - Alternativa D7: `D3DKMTEnumAdapters2` (WDK, d3dkmthk.h, Gdi32, da Windows 8: due chiamate,
///   prima con `pAdapters` NULL per il numero massimo; gli handle degli adattatori vanno chiusi
///   con `D3DKMTCloseAdapter`) e `D3DKMTQueryStatistics`, che la pagina del WDK dice «Reserved
///   for system use. Do not use in your driver.»: struttura e significato dei campi non sono
///   documentati.
#[cfg(windows)]
mod gpu {
    use windows_sys::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA,
    };

    /// Il percorso del contatore, in inglese (`PdhAddEnglishCounterW`).
    const PERCORSO: &str = r"\GPU Engine(*)\Utilization Percentage";
    const SUCCESSO: u32 = 0;

    /// Una query PDH aperta con il contatore della GPU; `Drop` la chiude.
    pub struct LettoreGpu {
        query: isize,
        contatore: isize,
        /// Buffer delle istanze, allineato a 8 byte (le strutture hanno un `f64`).
        buffer: Vec<u64>,
    }

    impl LettoreGpu {
        /// Apre la query, aggiunge il contatore e fa la prima raccolta (la base della
        /// differenza). `None` se una delle tre non riesce (la query aperta si chiude).
        pub fn apri() -> Option<LettoreGpu> {
            let mut query: isize = 0;
            // SAFETY: origine NULL (tempo reale), puntatore a una variabile locale.
            if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != SUCCESSO {
                return None;
            }
            // Da qui in poi la query si chiude da sola (`Drop`), anche negli errori.
            let mut lettore = LettoreGpu { query, contatore: 0, buffer: Vec::new() };
            let percorso: Vec<u16> = PERCORSO.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: query aperta sopra, stringa terminata da zero che vive per tutta la chiamata.
            if unsafe { PdhAddEnglishCounterW(query, percorso.as_ptr(), 0, &mut lettore.contatore) } != SUCCESSO {
                return None;
            }
            // SAFETY: query aperta, con un contatore.
            if unsafe { PdhCollectQueryData(query) } != SUCCESSO {
                return None;
            }
            Some(lettore)
        }

        /// Una raccolta e l'utilizzo fra la raccolta di prima e questa; `None` se non riesce o
        /// se non c'è nessun motore.
        pub fn gpu(&mut self) -> Option<f32> {
            // SAFETY: query aperta in `apri`, chiusa solo in `Drop`.
            if unsafe { PdhCollectQueryData(self.query) } != SUCCESSO {
                return None;
            }
            super::gpu_da_istanze(&self.istanze()?)
        }

        /// Le istanze valide (nome e valore). Due chiamate come vuole la documentazione; se alla
        /// seconda il buffer non bastasse (`PDH_MORE_DATA`) si riparte, al più tre volte.
        fn istanze(&mut self) -> Option<Vec<(String, f64)>> {
            for _ in 0..3 {
                let (mut dimensione, mut quante) = (0u32, 0u32);
                // SAFETY: contatore della query aperta; buffer NULL con dimensione 0 (prima chiamata).
                let esito = unsafe {
                    PdhGetFormattedCounterArrayW(
                        self.contatore,
                        PDH_FMT_DOUBLE,
                        &mut dimensione,
                        &mut quante,
                        std::ptr::null_mut(),
                    )
                };
                if esito == SUCCESSO {
                    // Nessuna istanza: nessun motore.
                    return Some(Vec::new());
                }
                if esito != PDH_MORE_DATA {
                    return None;
                }
                let parole = (dimensione as usize).div_ceil(8);
                self.buffer.clear();
                self.buffer.resize(parole, 0);
                let mut dimensione = (parole * 8) as u32;
                let elementi = self.buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                // SAFETY: buffer di `dimensione` byte allineato a 8, vivo per tutta la chiamata.
                let esito = unsafe {
                    PdhGetFormattedCounterArrayW(self.contatore, PDH_FMT_DOUBLE, &mut dimensione, &mut quante, elementi)
                };
                if esito == PDH_MORE_DATA {
                    continue;
                }
                if esito != SUCCESSO {
                    return None;
                }
                // SAFETY: PDH ha scritto `quante` strutture all'inizio del buffer e i nomi (stringhe
                // terminate da zero) più avanti nello stesso buffer, che non si tocca fino a qui.
                let elementi = unsafe { std::slice::from_raw_parts(elementi.cast_const(), quante as usize) };
                let istanze = elementi
                    .iter()
                    .filter(|e| matches!(e.FmtValue.CStatus, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA))
                    // SAFETY: con `PDH_FMT_DOUBLE` il valore è in `doubleValue`; il nome è una
                    // stringa terminata da zero nel buffer.
                    .map(|e| unsafe { (stringa(e.szName), e.FmtValue.Anonymous.doubleValue) })
                    .collect();
                return Some(istanze);
            }
            None
        }
    }

    impl Drop for LettoreGpu {
        fn drop(&mut self) {
            // SAFETY: query aperta in `apri`; chiude anche il contatore, che dopo non si usa più.
            unsafe { PdhCloseQuery(self.query) };
        }
    }

    /// Una stringa UTF-16 terminata da zero (vuota se il puntatore è NULL).
    ///
    /// # Safety
    /// `p` è NULL o punta a una stringa terminata da zero valida.
    unsafe fn stringa(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut n = 0;
        // SAFETY: la stringa è terminata da zero (contratto della funzione).
        while unsafe { *p.add(n) } != 0 {
            n += 1;
        }
        // SAFETY: i primi `n` caratteri sono validi (letti sopra).
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
    }
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
        let mut c = Campionatore::new(Fonti::TUTTE);
        assert!(!c.cpu_pronta());
        c.campiona(Fonti::TUTTE);
        std::thread::sleep(Duration::from_millis(300));
        let s = c.campiona(Fonti::TUTTE);
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

    /// M2 Task 4: nessun quadrante né vista mostra il disco → il campionatore non lo legge
    /// (l'elenco dei dischi nemmeno si crea); appena una vista lo chiede, lo legge.
    #[test]
    fn quadranti_leggono_solo_cio_che_mostrano() {
        let solo_cpu_ram = Fonti::default();
        let mut c = Campionatore::new(solo_cpu_ram);
        let s = c.campiona(solo_cpu_ram);
        assert!(!c.legge_dischi(), "nessun disco scelto: i dischi non si leggono");
        assert_eq!((s.disco_totale, s.disco_usato), (0, 0));
        assert_eq!((s.rete_giu_bps, s.rete_su_bps), (0, 0));
        assert!(s.ram_totale > 0, "la RAM si legge sempre");
        let s = c.campiona(Fonti { disco: true, ..Fonti::default() });
        assert!(c.legge_dischi());
        assert!(s.disco_usato <= s.disco_totale);
    }

    #[test]
    fn campionatore_leggero_cpu_e_ram() {
        let mut c = CampionatoreLeggero::new();
        let (_, ram) = c.campiona();
        assert!((0.0..=100.0).contains(&ram) && ram > 0.0, "ram {ram}");
        std::thread::sleep(Duration::from_millis(200));
        let (cpu, ram) = c.campiona();
        let cpu = cpu.expect("CPU leggibile dal secondo campione");
        assert!((0.0..=100.0).contains(&cpu), "cpu {cpu}");
        assert!((0.0..=100.0).contains(&ram));
        assert_eq!(PERIODO_LATI, Duration::from_secs(5));
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

    // ─── M2, Task 10: GPU ───

    fn istanza(tipo: &str, valore: f64) -> (String, f64) {
        (format!("pid_1234_luid_0x00000000_0x0000D1A5_phys_0_eng_0_engtype_{tipo}"), valore)
    }

    #[test]
    fn gpu_massimo_fra_i_tipi() {
        // Due istanze 3D (processi diversi) si sommano: 55; il decodificatore video è a 40.
        let v = [istanza("3D", 30.0), istanza("3D", 25.0), istanza("VideoDecode", 40.0)];
        assert_eq!(gpu_da_istanze(&v), Some(55.0));
        let v = [istanza("3D", 5.0), istanza("VideoDecode", 40.0), istanza("Copy", 1.0)];
        assert_eq!(gpu_da_istanze(&v), Some(40.0));
    }

    #[test]
    fn gpu_vuota_e_fuori_scala() {
        assert_eq!(gpu_da_istanze(&[]), None);
        assert_eq!(gpu_da_istanze(&[istanza("3D", 180.0)]), Some(100.0));
        // Somme oltre 100 (più processi sullo stesso motore): mai fuori da 0..100.
        assert_eq!(gpu_da_istanze(&[istanza("3D", 70.0), istanza("3D", 60.0)]), Some(100.0));
        // Valori impossibili non diventano NaN né negativi.
        assert_eq!(gpu_da_istanze(&[istanza("3D", f64::NAN), istanza("Copy", -5.0)]), Some(0.0));
        // Motori tutti fermi: 0, non `None` (la GPU c'è).
        assert_eq!(gpu_da_istanze(&[istanza("3D", 0.0)]), Some(0.0));
    }

    /// La query della GPU c'è solo finché la GPU si mostra: nasce con `Fonti::gpu`, si chiude
    /// appena non si mostra più. Il primo campione non ha valore (è la base), dal secondo sì
    /// (se il PC ha un driver grafico con i contatori «GPU Engine»).
    #[cfg(windows)]
    #[test]
    fn gpu_solo_finche_si_vede() {
        let senza = Fonti::default();
        let con = Fonti { gpu: true, ..Fonti::default() };
        let mut c = Campionatore::new(senza);
        let s = c.campiona(senza);
        assert!(!c.legge_gpu(), "GPU non mostrata: nessuna query");
        assert_eq!(s.gpu_pct, None);
        assert!(!c.gpu_in_attesa());
        let s = c.campiona(con);
        assert_eq!(s.gpu_pct, None, "il primo campione è la base");
        if !c.legge_gpu() {
            // Nessun contatore della GPU (macchina senza driver WDDM): la riga resta assente.
            return;
        }
        assert!(c.gpu_in_attesa(), "il valore arriva al campione dopo");
        std::thread::sleep(Duration::from_millis(300));
        let s = c.campiona(con);
        assert!(!c.gpu_in_attesa());
        if let Some(g) = s.gpu_pct {
            assert!((0.0..=100.0).contains(&g), "gpu {g}");
        }
        let s = c.campiona(senza);
        assert!(!c.legge_gpu(), "GPU non più mostrata: query chiusa");
        assert_eq!(s.gpu_pct, None);
    }

    /// Memoria privata del processo di test, in byte.
    #[cfg(windows)]
    fn memoria_privata() -> u64 {
        use windows_sys::Win32::System::ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        // SAFETY: struttura locale della dimensione dichiarata in `cb`, pseudo-handle del processo.
        unsafe {
            let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            let p = &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS;
            assert_ne!(K32GetProcessMemoryInfo(GetCurrentProcess(), p, c.cb), 0);
            c.PrivateUsage as u64
        }
    }

    /// Cancello di memoria della decisione D7, nel processo di test (l'assistente vero è aperto e
    /// non si misura): cinque cicli «vista PC aperta» (query aperta, dieci campioni) e «vista
    /// lasciata» (query chiusa), memoria privata prima, dopo il primo ciclo e dopo il quinto.
    /// Il primo ciclo paga le tabelle che PDH carica e non libera (nota in testa al file); fra il
    /// primo e il quinto la crescita deve stare entro 2 MB. Si lancia a mano, da solo:
    /// `cargo test -p assistente gpu_memoria -- --ignored --nocapture --test-threads=1`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn gpu_memoria_dopo_cinque_cicli() {
        const MB: f64 = 1024.0 * 1024.0;
        let ciclo = || {
            let mut lettore = gpu::LettoreGpu::apri().expect("contatori della GPU");
            let mut ultimo = None;
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(100));
                ultimo = lettore.gpu();
            }
            drop(lettore);
            ultimo
        };
        let prima = memoria_privata();
        let valore = ciclo();
        let dopo_il_primo = memoria_privata();
        for _ in 0..4 {
            ciclo();
        }
        let dopo_cinque = memoria_privata();
        let mb = |b: u64| b as f64 / MB;
        println!(
            "GPU {valore:?}; memoria privata: prima {:.2} MB, dopo il primo ciclo {:.2} MB (+{:.2}), \
             dopo cinque cicli {:.2} MB (+{:.2} dal primo, +{:.2} da prima)",
            mb(prima),
            mb(dopo_il_primo),
            mb(dopo_il_primo.saturating_sub(prima)),
            mb(dopo_cinque),
            mb(dopo_cinque.saturating_sub(dopo_il_primo)),
            mb(dopo_cinque.saturating_sub(prima)),
        );
        assert!(
            dopo_cinque.saturating_sub(dopo_il_primo) as f64 <= 2.0 * MB,
            "la memoria cresce a ogni ciclo: serve la via D3DKMT (D7)"
        );
    }

    #[test]
    fn loopback_escluso() {
        assert!(e_loopback("Loopback Pseudo-Interface 1"));
        assert!(e_loopback("lo"));
        assert!(!e_loopback("Wi-Fi"));
    }
}
