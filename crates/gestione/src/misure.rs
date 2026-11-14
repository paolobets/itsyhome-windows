//! Misure della finestra di gestione: grande al più 780 × 680 logici, ridotta se l'area di lavoro
//! del monitor non basta (a 1920 × 1080 con scala 150% l'area utile è di circa 1280 × 688
//! logici), con 24 px di respiro per lato; mai sotto 480 × 360. Centrata sull'area di lavoro.

/// Rettangolo in pixel fisici: x, y, larghezza, altezza.
pub type Rettangolo = (i32, i32, i32, i32);

pub const LARGHEZZA_MASSIMA: f32 = 780.0;
pub const ALTEZZA_MASSIMA: f32 = 680.0;
pub const LARGHEZZA_MINIMA: f32 = 480.0;
pub const ALTEZZA_MINIMA: f32 = 360.0;
/// Respiro totale (24 px per lato) fra la finestra e i bordi dell'area di lavoro.
pub const RESPIRO: f32 = 48.0;

/// Dimensione logica e posizione fisica (angolo in alto a sinistra) sull'area di lavoro `area`
/// con scala `scala`.
pub fn misura(area: Rettangolo, scala: f32) -> ((f32, f32), (i32, i32)) {
    let scala = if scala > 0.0 { scala } else { 1.0 };
    let (ax, ay, al, aa) = area;
    let larghezza = (al as f32 / scala - RESPIRO).clamp(LARGHEZZA_MINIMA, LARGHEZZA_MASSIMA);
    let altezza = (aa as f32 / scala - RESPIRO).clamp(ALTEZZA_MINIMA, ALTEZZA_MASSIMA);
    let x = ax + (al - (larghezza * scala).round() as i32) / 2;
    let y = ay + (aa - (altezza * scala).round() as i32) / 2;
    ((larghezza, altezza), (x, y))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn a_1080p_con_scala_150_e_175_la_finestra_entra() {
        // 1920 × 1080, barra delle applicazioni da 48 px fisici in basso.
        let area = (0, 0, 1920, 1032);
        let ((l, a), (x, y)) = misura(area, 1.5);
        assert_eq!((l, a), (780.0, 640.0), "1280 × 688 logici: alta 688 − 48");
        assert_eq!((x, y), (375, 36), "centrata: (1920 − 1170) / 2, (1032 − 960) / 2");
        let ((l, a), _) = misura(area, 1.75);
        assert_eq!(l, 780.0);
        assert!((a - (1032.0 / 1.75 - 48.0)).abs() < 0.01, "{a}");
    }

    #[test]
    fn su_un_monitor_grande_resta_780_per_680() {
        // 4K al 175%, area su un secondo monitor a destra.
        let ((l, a), (x, y)) = misura((3840, 0, 3840, 2112), 1.75);
        assert_eq!((l, a), (780.0, 680.0));
        assert_eq!((x, y), (3840 + (3840 - 1365) / 2, (2112 - 1190) / 2));
    }

    #[test]
    fn mai_sotto_il_minimo() {
        let ((l, a), _) = misura((0, 0, 800, 600), 2.0);
        assert_eq!((l, a), (LARGHEZZA_MINIMA, ALTEZZA_MINIMA));
    }
}
