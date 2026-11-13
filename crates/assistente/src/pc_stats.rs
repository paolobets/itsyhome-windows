//! Statistiche del PC (CPU, RAM, dischi, rete, batteria) per i quadranti e la vista PC.
//!
//! Il campionatore nasce all'apertura del pannello e muore alla chiusura: fuori da lì non si
//! misura nulla, salvo la batteria (`sorveglia_batteria`, ogni 60 s). Refresh sempre mirati (mai
//! `refresh_all`, mai l'elenco dei processi).

use starship_battery::units::ratio::ratio;
use starship_battery::{Manager, State};
use std::time::{Duration, Instant};

/// Ogni quanto si legge la batteria a pannello chiuso.
pub const PERIODO_BATTERIA: Duration = Duration::from_secs(60);

/// Legge la batteria subito e poi ogni 60 s su un thread che per il resto dorme, e consegna
/// (percentuale, scollegata) a `consegna`; si ferma quando `consegna` dà falso (interfaccia
/// chiusa). Senza batteria alla prima lettura (PC fisso) o senza accesso alle batterie il thread
/// finisce subito; più avanti una lettura mancata si riprova al minuto dopo.
pub fn sorveglia_batteria(consegna: impl Fn(f32, bool) -> bool + Send + 'static) {
    let avviato = std::thread::Builder::new().name("batteria".into()).spawn(move || {
        let Ok(gestore) = Manager::new() else { return };
        // Nessuna batteria alla prima lettura = PC fisso: il thread finisce. Dopo, una lettura
        // mancata non lo ferma: si riprova al giro successivo.
        if leggi_batteria(&gestore).is_none() {
            return;
        }
        loop {
            if let Some((percentuale, scollegata)) = leggi_batteria(&gestore) {
                if !consegna(percentuale, scollegata) {
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

/// La prima batteria: percentuale e se si sta scaricando (scollegata). `None` senza batteria.
fn leggi_batteria(gestore: &Manager) -> Option<(f32, bool)> {
    let b = gestore.batteries().ok()?.next()?.ok()?;
    let percentuale = (b.state_of_charge().get::<ratio>() * 100.0).clamp(0.0, 100.0);
    Some((percentuale, b.state() == State::Discharging))
}
use sysinfo::{DiskRefreshKind, Disks, MemoryRefreshKind, Networks, System};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatPc {
    pub cpu_pct: f32,
    pub ram_usata: u64,
    pub ram_totale: u64,
    pub disco_usato: u64,
    pub disco_totale: u64,
    pub rete_giu_bps: u64,
    pub rete_su_bps: u64,
    pub batteria_pct: Option<f32>,
    pub in_carica: Option<bool>,
}

pub struct Campionatore {
    sistema: System,
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
        let mut sistema = System::new();
        // Base di partenza per la CPU: la percentuale è la differenza tra due letture.
        sistema.refresh_cpu_usage();
        Campionatore {
            sistema,
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
        self.sistema.refresh_cpu_usage();
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

        let (batteria_pct, in_carica) = self.batteria();
        self.campioni = self.campioni.saturating_add(1);

        let ram_totale = self.sistema.total_memory();
        StatPc {
            cpu_pct: self.sistema.global_cpu_usage().clamp(0.0, 100.0),
            ram_usata: self.sistema.used_memory().min(ram_totale),
            ram_totale,
            disco_usato: disco_totale.saturating_sub(disco_libero),
            disco_totale,
            rete_giu_bps,
            rete_su_bps,
            batteria_pct,
            in_carica,
        }
    }

    /// La prima batteria del PC; `(None, None)` se il PC non ne ha (desktop) o non si legge.
    fn batteria(&self) -> (Option<f32>, Option<bool>) {
        let Some(gestore) = &self.batterie else { return (None, None) };
        let Ok(mut batterie) = gestore.batteries() else { return (None, None) };
        match batterie.next() {
            Some(Ok(b)) => {
                let pct = (b.state_of_charge().get::<ratio>() * 100.0).clamp(0.0, 100.0);
                (Some(pct), Some(matches!(b.state(), State::Charging | State::Full)))
            }
            _ => (None, None),
        }
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
        let mut c = Campionatore::new();
        assert!(!c.cpu_pronta());
        c.campiona();
        std::thread::sleep(Duration::from_millis(300));
        let s = c.campiona();
        assert!(c.cpu_pronta());
        assert!((0.0..=100.0).contains(&s.cpu_pct), "cpu {}", s.cpu_pct);
        assert!(s.ram_totale > 0);
        assert!(s.ram_usata <= s.ram_totale);
        assert!(s.disco_usato <= s.disco_totale);
        if let Some(p) = s.batteria_pct {
            assert!((0.0..=100.0).contains(&p));
            assert!(s.in_carica.is_some());
        }
    }

    #[test]
    fn loopback_escluso() {
        assert!(e_loopback("Loopback Pseudo-Interface 1"));
        assert!(e_loopback("lo"));
        assert!(!e_loopback("Wi-Fi"));
    }
}
