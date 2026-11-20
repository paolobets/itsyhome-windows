//! Il riassunto del PC: la scala per stato della carta, da un campione delle statistiche.

use super::{riga, Riassunto, Tono};
use crate::pc_stats::{StatPc, SOGLIA_BATTERIA_INFO};

/// Da qui in su CPU o RAM meritano uno sguardo.
const SOGLIA_CARICO: f32 = 85.0;

fn percento(v: f32) -> u32 {
    v.round().clamp(0.0, 100.0) as u32
}

fn carta(tono: Tono, titolo: String, testo: &str) -> Riassunto {
    Riassunto { tono, titolo, titolo_numerico: false, sec: riga(testo, false), sec_intera: testo.into(), azione: None }
}

/// La carta del PC. `None`: il primo campione utile non è ancora arrivato (la CPU è una differenza
/// fra due campioni). Una misura che non si legge (CPU fallita, RAM a zero) non si mostra: mai «0».
pub fn riassunto(stat: Option<&StatPc>) -> Riassunto {
    let Some(s) = stat else { return Riassunto::primo_avvio("Lettura in corso") };
    let cpu = s.cpu_pct.filter(|c| c.is_finite()).map(percento);
    let ram = (s.ram_totale > 0).then(|| percento(s.ram_usata as f32 / s.ram_totale as f32 * 100.0));
    let cpu_testo = cpu.map(|c| format!("CPU {c} %"));
    let ram_testo = ram.map(|r| format!("RAM {r} %"));
    let misure = match (&cpu_testo, &ram_testo) {
        (Some(c), Some(r)) => format!("{c}, {r}"),
        (Some(m), None) | (None, Some(m)) => m.clone(),
        // Nessuna misura: assenza, non calma (spec Review Focus 3).
        (None, None) => return Riassunto::primo_avvio("Misure non lette"),
    };
    // La misura più carica sopra la soglia nel titolo, l'altra sotto.
    let carica = [(cpu, &cpu_testo, &ram_testo), (ram, &ram_testo, &cpu_testo)]
        .into_iter()
        .filter(|(v, _, _)| v.is_some_and(|v| v as f32 >= SOGLIA_CARICO))
        .reduce(|a, b| if b.0 > a.0 { b } else { a });
    if let Some((_, Some(titolo), altra)) = carica {
        return carta(Tono::Guarda, titolo.clone(), altra.as_deref().unwrap_or("—"));
    }
    if let Some(b) = s.batteria.filter(|b| b.scollegata() && b.percentuale <= SOGLIA_BATTERIA_INFO) {
        return carta(Tono::Guarda, format!("Batteria {} %", percento(b.percentuale)), &misure);
    }
    carta(Tono::Calmo, "Tutto regolare".into(), &misure)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::moduli::controlla;
    use crate::pc_stats::{LetturaBatteria, StatoBatteria};

    const GB: u64 = 1 << 30;

    fn stat(cpu: Option<f32>, ram: u64, batteria: Option<(f32, StatoBatteria)>) -> StatPc {
        StatPc {
            cpu_pct: cpu,
            ram_usata: ram * GB / 100,
            ram_totale: GB,
            disco_usato: 0,
            disco_totale: 0,
            rete_giu_bps: 0,
            rete_su_bps: 0,
            batteria: batteria.map(|(percentuale, stato)| LetturaBatteria { percentuale, stato }),
            gpu_pct: None,
        }
    }

    fn vista(r: &Riassunto) -> (Tono, &str, &str) {
        (r.tono, r.titolo.as_str(), r.sec.as_str())
    }

    #[test]
    fn scala_per_stato() {
        let r = riassunto(None);
        assert_eq!(vista(&r), (Tono::Vuoto, "—", "Lettura in corso"));
        assert_eq!(vista(&riassunto(Some(&stat(Some(90.4), 40, None)))), (Tono::Guarda, "CPU 90 %", "RAM 40 %"));
        assert_eq!(vista(&riassunto(Some(&stat(Some(12.0), 88, None)))), (Tono::Guarda, "RAM 88 %", "CPU 12 %"));
        // Tutte e due cariche: la più carica nel titolo.
        assert_eq!(vista(&riassunto(Some(&stat(Some(86.0), 97, None)))), (Tono::Guarda, "RAM 97 %", "CPU 86 %"));
        let scarica = stat(Some(12.0), 40, Some((18.0, StatoBatteria::Scarica)));
        assert_eq!(vista(&riassunto(Some(&scarica))), (Tono::Guarda, "Batteria 18 %", "CPU 12 %, RAM 40 %"));
        let in_carica = stat(Some(12.0), 40, Some((18.0, StatoBatteria::InCarica)));
        assert_eq!(vista(&riassunto(Some(&in_carica))), (Tono::Calmo, "Tutto regolare", "CPU 12 %, RAM 40 %"));
        let piena = stat(Some(12.0), 40, Some((60.0, StatoBatteria::Scarica)));
        assert_eq!(vista(&riassunto(Some(&piena))), (Tono::Calmo, "Tutto regolare", "CPU 12 %, RAM 40 %"));
        assert!(riassunto(Some(&piena)).azione.is_none(), "il PC non ha azioni");
    }

    #[test]
    fn ogni_gradino_sta_nei_limiti() {
        for s in [
            stat(Some(100.0), 100, None),
            stat(Some(100.0), 10, None),
            stat(Some(10.0), 100, Some((20.0, StatoBatteria::Scarica))),
            stat(Some(100.0), 84, Some((20.0, StatoBatteria::Scarica))),
            stat(Some(84.0), 84, Some((100.0, StatoBatteria::Scarica))),
            stat(None, 100, None),
        ] {
            let r = riassunto(Some(&s));
            assert_eq!(controlla("pc", &r), Ok(()), "{r:?}");
            assert!(!r.sec.ends_with('…'), "{r:?}");
        }
        let r = riassunto(Some(&stat(Some(84.0), 84, Some((100.0, StatoBatteria::Scarica)))));
        assert_eq!(r.sec, "CPU 84 %, RAM 84 %");
        assert_eq!(controlla("pc", &riassunto(None)), Ok(()));
    }

    /// Senza campione «—» col motivo; una misura che non si legge non diventa «0 %».
    #[test]
    fn assenza_non_e_zero() {
        let r = riassunto(None);
        assert_eq!(r.titolo, "—");
        assert!(!r.titolo.contains('0') && !r.sec.contains('0'));
        let mut senza = stat(None, 40, None);
        senza.ram_totale = 0;
        let r = riassunto(Some(&senza));
        // Nessuna misura letta: «—» col motivo (vuoto), non «Tutto regolare».
        assert_eq!(vista(&r), (Tono::Vuoto, "—", "Misure non lette"));
        let r = riassunto(Some(&stat(None, 40, None)));
        assert_eq!(r.sec, "RAM 40 %", "CPU non letta: solo la RAM");
    }
}
