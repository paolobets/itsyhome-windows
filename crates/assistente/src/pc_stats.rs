//! Statistiche del PC (CPU, RAM, dischi, rete, batteria, GPU) per la carta PC e la vista PC.
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
/// mostra (la vista PC). CPU, RAM e batteria si leggono sempre (costano poco).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fonti {
    pub disco: bool,
    pub rete: bool,
    /// La GPU (M2, Task 10, solo Windows): il lettore della GPU esiste solo finché è vero.
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
    /// Il lettore della GPU: nasce quando la GPU si mostra (`usa_gpu`), muore appena non si
    /// mostra più o col campionatore, alla chiusura del pannello.
    #[cfg(windows)]
    gpu: StatoGpu,
}

/// Lo stato della lettura della GPU nel campionatore del pannello.
#[cfg(windows)]
enum StatoGpu {
    /// Nessun lettore: la GPU non si mostra.
    Spenta,
    /// Lettore acceso. L'utilizzo è una differenza fra due letture: la prima la fa `apri`; il
    /// campione in cui il lettore nasce non legge (sarebbe a pochi millisecondi dalla base) e porta
    /// `letture` a 1; dal campione dopo si legge (`letture` 2).
    Accesa { lettore: gpu::LettoreGpu, letture: u8 },
    /// Apertura fallita (nessun adattatore con nodi, API assente o in errore): non si riprova a
    /// ogni campione, solo dopo averla spenta (vista lasciata, pannello chiuso).
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

    /// Accende o spegne la lettura della GPU: con `si` il lettore nasce (se non c'è già), senza
    /// si butta. Fuori da Windows non fa nulla (la GPU non c'è).
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

    /// Vero se il lettore della GPU è acceso (solo a GPU mostrata).
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

    /// Vero se il campionatore legge i dischi (fuori dalla vista PC: mai).
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

/// L'utilizzo della GPU fra due letture dei tempi di esecuzione dei nodi (motori) della GPU:
/// per ogni nodo il tempo di esecuzione passato fra `prima` e `ora` diviso il tempo trascorso
/// (tutto in unità di 100 ns), poi il nodo più occupato, come Gestione attività; fuori scala 100.
/// `None` senza nodi, con elenchi di lunghezza diversa (adattatori cambiati) o senza tempo
/// trascorso. Un contatore che torna indietro conta zero.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn gpu_da_tempi(prima: &[i64], ora: &[i64], trascorso_100ns: u64) -> Option<f32> {
    if prima.is_empty() || prima.len() != ora.len() || trascorso_100ns == 0 {
        return None;
    }
    let massimo = prima
        .iter()
        .zip(ora)
        .map(|(p, o)| o.saturating_sub(*p).max(0) as f64 / trascorso_100ns as f64 * 100.0)
        .fold(0.0, f64::max);
    Some(massimo.clamp(0.0, 100.0) as f32)
}

/// La GPU su Windows dai contatori del kernel grafico (M2, Task 10, decisione D7).
///
/// Perché non PDH: il contatore «\GPU Engine(*)\Utilization Percentage» con PDH lasciava
/// +6,00 MB di memoria privata dopo il primo ciclo apri/chiudi (tabelle che PDH carica e non
/// libera, come per la CPU in M1); D7 ne ammette al massimo 2. PDH è tolto del tutto.
///
/// Documentazione letta (learn.microsoft.com, sezione driver, WDK `d3dkmthk.h`, 16/11/2026):
/// - `D3DKMTEnumAdapters2` (Gdi32, da Windows 8): due chiamate, la prima con `pAdapters` NULL dà
///   in `NumAdapters` il numero massimo, la seconda riempie l'elenco e dice quanti; errori
///   `STATUS_BUFFER_TOO_SMALL`, `STATUS_INVALID_PARAMETER`, `STATUS_NO_MEMORY`. Gli handle degli
///   adattatori restituiti vanno chiusi da chi chiama con `D3DKMTCloseAdapter`.
/// - `D3DKMT_ADAPTERINFO`: `hAdapter`, `AdapterLuid` (unico fino al riavvio, cambia anche se il
///   driver si aggiorna o l'adattatore si toglie), `NumOfSources`, `bPrecisePresentRegionsPreferred`.
/// - `D3DKMTCloseAdapter` (da Vista): `STATUS_SUCCESS` o `STATUS_INVALID_PARAMETER`.
/// - `D3DKMTQueryStatistics` e le sue strutture (`D3DKMT_QUERYSTATISTICS`,
///   `D3DKMT_QUERYSTATISTICS_PROCESS_NODE_INFORMATION`): la pagina dice **«Reserved for system
///   use. Do not use.»** La struttura è pubblicata nell'header (tipo, `AdapterLuid`, `hProcess`,
///   `QueryResult`, l'unione delle richieste con `QueryNode.NodeId`), ma il significato dei campi
///   no. Si usa come System Informer: `D3DKMT_QUERYSTATISTICS_ADAPTER` per
///   `AdapterInformation.NodeCount`, poi `D3DKMT_QUERYSTATISTICS_NODE` per nodo e
///   `NodeInformation.GlobalInformation.RunningTime` (tempo di esecuzione cumulativo, in unità di
///   100 ns come lo tratta System Informer: l'unità non è documentata).
/// - Tipi da `windows-sys` 0.52 (feature `Wdk_Graphics_Direct3D`, funzioni da gdi32.dll): le
///   posizioni dei campi usati si controllano con `const` (sotto) contro l'ordine dell'header.
///   Il kernel potrebbe conoscere un'unione dei risultati più grande di quella dei metadati:
///   la richiesta ha 4 KB di margine dopo la struttura, così non si scrive mai fuori.
/// - Nessun handle resta aperto: gli adattatori si chiudono subito dopo averne letto il LUID
///   (`D3DKMTQueryStatistics` vuole il LUID, non l'handle). Niente COM, nessun thread
///   particolare: tutto sul thread dei campioni del pannello (`campioni-pc`).
/// - Se l'API manca o fallisce la GPU è `None` (riga assente): mai un panico.
#[cfg(windows)]
mod gpu {
    use std::time::Instant;
    use windows_sys::Wdk::Graphics::Direct3D::{
        D3DKMTCloseAdapter, D3DKMTEnumAdapters2, D3DKMTQueryStatistics, D3DKMT_ADAPTERINFO, D3DKMT_CLOSEADAPTER,
        D3DKMT_ENUMADAPTERS2, D3DKMT_QUERYSTATISTICS, D3DKMT_QUERYSTATISTICS_ADAPTER,
        D3DKMT_QUERYSTATISTICS_ADAPTER_INFORMATION, D3DKMT_QUERYSTATISTICS_NODE,
        D3DKMT_QUERYSTATISTICS_NODE_INFORMATION, D3DKMT_QUERYSTATISTICS_PROCESS_NODE_INFORMATION,
        D3DKMT_QUERYSTATISTICS_TYPE,
    };
    use windows_sys::Win32::Foundation::LUID;

    // Le posizioni dei campi usati, contro l'ordine di `d3dkmthk.h`: `Type` (4 byte), `AdapterLuid`
    // (8, allineato a 4), `hProcess` (puntatore), poi `QueryResult`; `NodeCount` è il secondo
    // `ULONG` delle informazioni dell'adattatore; `RunningTime` è il primo campo del nodo.
    const _: () = {
        use std::mem::offset_of;
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS, Type) == 0);
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS, AdapterLuid) == 4);
        #[cfg(target_pointer_width = "64")]
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS, hProcess) == 16);
        #[cfg(target_pointer_width = "64")]
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS, QueryResult) == 24);
        // `Anonymous` porta `QueryNode.NodeId`, il campo che il kernel LEGGE: un suo spostamento
        // silenzioso in `windows-sys` chiederebbe un nodo sbagliato. Misurati dal vivo dalla
        // revisione del Task 10: `Anonymous` a 800, struttura da 808 byte.
        #[cfg(target_pointer_width = "64")]
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS, Anonymous) == 800);
        #[cfg(target_pointer_width = "64")]
        assert!(std::mem::size_of::<D3DKMT_QUERYSTATISTICS>() == 808);
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS_ADAPTER_INFORMATION, NodeCount) == 4);
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS_NODE_INFORMATION, GlobalInformation) == 0);
        assert!(offset_of!(D3DKMT_QUERYSTATISTICS_PROCESS_NODE_INFORMATION, RunningTime) == 0);
        assert!(std::mem::size_of::<D3DKMT_ADAPTERINFO>() == 20);
        assert!(std::mem::size_of::<D3DKMT_CLOSEADAPTER>() == 4);
    };

    /// Margine dopo la richiesta: il kernel scrive il risultato con la sua dimensione.
    const MARGINE: usize = 4096;

    /// La richiesta a `D3DKMTQueryStatistics`, con il margine.
    #[repr(C)]
    struct Richiesta {
        q: D3DKMT_QUERYSTATISTICS,
        _margine: [u8; MARGINE],
    }

    /// Una richiesta azzerata per l'adattatore `luid`.
    fn richiesta(tipo: D3DKMT_QUERYSTATISTICS_TYPE, luid: LUID) -> Box<Richiesta> {
        // SAFETY: strutture e unioni di soli numeri e puntatori: lo zero è un valore valido.
        let mut r: Box<Richiesta> = Box::new(unsafe { std::mem::zeroed() });
        r.q.Type = tipo;
        r.q.AdapterLuid = luid;
        r
    }

    /// Chiede al kernel; vero se `NT_SUCCESS`.
    fn chiedi(r: &mut Richiesta) -> bool {
        // Il puntatore viene dall'intera richiesta (margine compreso) e da un riferimento
        // mutabile: il kernel ci scrive il risultato, anche se la firma dice `*const`.
        let p = (r as *mut Richiesta).cast::<D3DKMT_QUERYSTATISTICS>().cast_const();
        // SAFETY: richiesta valida e scrivibile, con 4 KB di margine dopo la struttura.
        unsafe { D3DKMTQueryStatistics(p) >= 0 }
    }

    /// Il tempo di esecuzione cumulativo di un nodo; `None` se il kernel non risponde.
    fn tempo(luid: LUID, nodo: u32) -> Option<i64> {
        let mut r = richiesta(D3DKMT_QUERYSTATISTICS_NODE, luid);
        r.q.Anonymous.QueryNode.NodeId = nodo;
        if !chiedi(&mut r) {
            return None;
        }
        // SAFETY: con `D3DKMT_QUERYSTATISTICS_NODE` il kernel riempie `NodeInformation`.
        Some(unsafe { r.q.QueryResult.NodeInformation.GlobalInformation.RunningTime })
    }

    /// I nodi (motori) di tutti gli adattatori grafici e la base dei loro tempi di esecuzione.
    pub struct LettoreGpu {
        /// Adattatore e numero di ogni nodo che ha risposto all'apertura.
        nodi: Vec<(LUID, u32)>,
        tempi: Vec<i64>,
        quando: Instant,
    }

    impl LettoreGpu {
        /// Elenca gli adattatori (chiudendone subito gli handle), ne conta i nodi e legge la base
        /// dei tempi. Tiene solo i nodi che rispondono: sul PC di casa un adattatore su tre dice
        /// 13 nodi ma rifiuta la richiesta del nodo (`STATUS_INVALID_PARAMETER`). `None` se l'API
        /// manca o fallisce o se nessun nodo risponde.
        pub fn apri() -> Option<LettoreGpu> {
            let mut nodi = Vec::new();
            let mut tempi = Vec::new();
            for luid in elenca()? {
                let mut r = richiesta(D3DKMT_QUERYSTATISTICS_ADAPTER, luid);
                if !chiedi(&mut r) {
                    continue;
                }
                // SAFETY: con `D3DKMT_QUERYSTATISTICS_ADAPTER` il kernel riempie `AdapterInformation`.
                // Un limite largo: un valore assurdo non deve far fare migliaia di richieste.
                let quanti = unsafe { r.q.QueryResult.AdapterInformation.NodeCount }.min(64);
                for nodo in 0..quanti {
                    if let Some(t) = tempo(luid, nodo) {
                        nodi.push((luid, nodo));
                        tempi.push(t);
                    }
                }
            }
            if nodi.is_empty() {
                return None;
            }
            Some(LettoreGpu { nodi, tempi, quando: Instant::now() })
        }

        /// L'utilizzo dalla lettura di prima a questa; `None` se una lettura fallisce (la base
        /// resta quella di prima).
        pub fn gpu(&mut self) -> Option<f32> {
            let ora: Vec<i64> = self.nodi.iter().map(|&(luid, nodo)| tempo(luid, nodo)).collect::<Option<_>>()?;
            let adesso = Instant::now();
            let trascorso = (adesso.duration_since(self.quando).as_nanos() / 100) as u64;
            let valore = super::gpu_da_tempi(&self.tempi, &ora, trascorso);
            self.tempi = ora;
            self.quando = adesso;
            valore
        }
    }

    /// I LUID degli adattatori grafici; gli handle si chiudono subito. `None` se l'elenco fallisce.
    fn elenca() -> Option<Vec<LUID>> {
        let mut e = D3DKMT_ENUMADAPTERS2 { NumAdapters: 0, pAdapters: std::ptr::null_mut() };
        // SAFETY: prima chiamata con `pAdapters` NULL: scrive solo il numero massimo.
        if unsafe { D3DKMTEnumAdapters2(&mut e) } < 0 {
            return None;
        }
        let massimo = e.NumAdapters.min(64);
        if massimo == 0 {
            return Some(Vec::new());
        }
        // SAFETY: strutture di soli numeri: lo zero è un valore valido.
        let mut info: Vec<D3DKMT_ADAPTERINFO> = vec![unsafe { std::mem::zeroed() }; massimo as usize];
        e.NumAdapters = massimo;
        e.pAdapters = info.as_mut_ptr();
        // SAFETY: elenco di `NumAdapters` strutture, vivo per tutta la chiamata.
        if unsafe { D3DKMTEnumAdapters2(&mut e) } < 0 {
            return None;
        }
        let quanti = (e.NumAdapters as usize).min(info.len());
        let luid = info[..quanti]
            .iter()
            .map(|a| {
                // SAFETY: handle restituito da `D3DKMTEnumAdapters2`, chiuso una volta sola.
                unsafe { D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: a.hAdapter }) };
                a.AdapterLuid
            })
            .collect();
        Some(luid)
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

    /// M2 Task 4: nessuna vista mostra il disco → il campionatore non lo legge (l'elenco dei
    /// dischi nemmeno si crea); appena la vista PC lo chiede, lo legge.
    #[test]
    fn si_legge_solo_cio_che_si_mostra() {
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

    /// Un secondo in unità di 100 ns.
    const SECONDO: u64 = 10_000_000;

    #[test]
    fn gpu_massimo_fra_i_nodi() {
        // In un secondo: il nodo 0 ha lavorato 0,3 s, il nodo 1 0,55 s, il nodo 2 0,4 s →
        // il più occupato, 55%, non la media né la somma.
        let prima = [1_000, 50_000, 7];
        let ora = [1_000 + 3_000_000, 50_000 + 5_500_000, 7 + 4_000_000];
        let g = gpu_da_tempi(&prima, &ora, SECONDO).expect("valore");
        assert!((g - 55.0).abs() < 1e-4, "gpu {g}");
        // Fermi: 0, non `None` (la GPU c'è).
        assert_eq!(gpu_da_tempi(&prima, &prima, SECONDO), Some(0.0));
        // Il tempo trascorso conta: lo stesso lavoro in 2 s è la metà.
        let g = gpu_da_tempi(&[0], &[5_000_000], 2 * SECONDO).expect("valore");
        assert!((g - 25.0).abs() < 1e-4, "gpu {g}");
    }

    #[test]
    fn gpu_vuota_e_fuori_scala() {
        assert_eq!(gpu_da_tempi(&[], &[], SECONDO), None, "nessun nodo");
        assert_eq!(gpu_da_tempi(&[0, 0], &[1], SECONDO), None, "adattatori cambiati fra le letture");
        assert_eq!(gpu_da_tempi(&[0], &[1], 0), None, "nessun tempo trascorso");
        // Più tempo di esecuzione che tempo trascorso (arrotondamenti del kernel): 100.
        assert_eq!(gpu_da_tempi(&[0], &[18_000_000], SECONDO), Some(100.0));
        // Un contatore che torna indietro (adattatore riavviato): 0, mai negativo.
        assert_eq!(gpu_da_tempi(&[9_000_000], &[1_000], SECONDO), Some(0.0));
        assert_eq!(gpu_da_tempi(&[i64::MIN], &[i64::MAX], SECONDO), Some(100.0), "nessun trabocco");
    }

    /// Il lettore della GPU c'è solo finché la GPU si mostra: nasce con `Fonti::gpu`, si butta
    /// appena non si mostra più. Il primo campione non ha valore (è la base), dal secondo sì (se
    /// il PC ha un adattatore grafico WDDM con nodi).
    #[cfg(windows)]
    #[test]
    fn gpu_solo_finche_si_vede() {
        let senza = Fonti::default();
        let con = Fonti { gpu: true, ..Fonti::default() };
        let mut c = Campionatore::new(senza);
        let s = c.campiona(senza);
        assert!(!c.legge_gpu(), "GPU non mostrata: nessun lettore");
        assert_eq!(s.gpu_pct, None);
        assert!(!c.gpu_in_attesa());
        let s = c.campiona(con);
        assert_eq!(s.gpu_pct, None, "il primo campione è la base");
        if !c.legge_gpu() {
            // Nessun adattatore con nodi (macchina senza driver WDDM): la riga resta assente.
            return;
        }
        assert!(c.gpu_in_attesa(), "il valore arriva al campione dopo");
        std::thread::sleep(Duration::from_millis(300));
        let s = c.campiona(con);
        assert!(!c.gpu_in_attesa());
        let g = s.gpu_pct.expect("dal secondo campione la GPU ha un valore");
        assert!((0.0..=100.0).contains(&g), "gpu {g}");
        let s = c.campiona(senza);
        assert!(!c.legge_gpu(), "GPU non più mostrata: lettore buttato");
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
    /// non si misura): cinque cicli «vista PC aperta» (lettore acceso, dieci campioni) e «vista
    /// lasciata» (lettore buttato). La memoria privata dopo i cinque cicli deve stare entro 2 MB
    /// da quella di PRIMA del primo ciclo (con PDH erano +6 MB). Si lancia a mano, da solo:
    /// `cargo test -p assistente gpu_memoria -- --ignored --nocapture --test-threads=1`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn gpu_memoria_dopo_cinque_cicli() {
        const MB: f64 = 1024.0 * 1024.0;
        let ciclo = || {
            let mut lettore = gpu::LettoreGpu::apri().expect("adattatore grafico con nodi");
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
        let crescita = |dopo: u64| (dopo as f64 - prima as f64) / MB;
        println!(
            "GPU {valore:?}; memoria privata: prima {:.2} MB, dopo il primo ciclo {:.2} MB ({:+.2}), \
             dopo cinque cicli {:.2} MB ({:+.2} da prima)",
            mb(prima),
            mb(dopo_il_primo),
            crescita(dopo_il_primo),
            mb(dopo_cinque),
            crescita(dopo_cinque),
        );
        assert!(valore.is_some(), "la GPU si legge");
        assert!(crescita(dopo_cinque) <= 2.0, "oltre +2 MB da prima del primo ciclo (D7)");
        // Nessun controllo sugli handle: gli adattatori aperti con `D3DKMTOpenAdapterFromLuid`
        // sono handle del kernel grafico, non oggetti del processo, e `GetProcessHandleCount` non
        // li conta (la revisione del Task 10 l'ha provato: 82 → 82 anche senza
        // `D3DKMTCloseAdapter`). Un controllo così non potrebbe fallire per la perdita che
        // dichiara di cercare; la chiusura è provata dalla lettura del codice (`Drop` di
        // `LettoreGpu`).
    }

    #[test]
    fn loopback_escluso() {
        assert!(e_loopback("Loopback Pseudo-Interface 1"));
        assert!(e_loopback("lo"));
        assert!(!e_loopback("Wi-Fi"));
    }
}
