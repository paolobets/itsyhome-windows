//! Misura dei testi in pixel, coi caratteri incorporati nell'app.

/// Gli stili di testo delle carte: carattere, peso, corpo e tracking come in `tema.slint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stile {
    /// Inter 700, 18 px, tracking -0,27.
    Titolo,
    /// JetBrains Mono NL 500, 26 px, tracking -0,52.
    TitoloNumerico,
    /// Inter 500, 12 px.
    Riga,
    /// JetBrains Mono NL 500, 11 px, tracking +0,77 (.07em). Per ora solo i test (la capsula
    /// accanto al nome breve); le schede della barra la misureranno (Task 6).
    #[cfg(test)]
    Etichetta,
    /// Inter 700, 11 px. Solo i test lo misurano (la capsula la dimensiona Slint).
    #[cfg(test)]
    Capsula,
}

/// Spazio per il testo di una carta senza il bottone d'azione: 182 − 2 × 14.
pub const SPAZIO_PIENO: f32 = 154.0;
/// Spazio per il testo di una carta col bottone d'azione: 154 − 32 − 6.
pub const SPAZIO_CON_AZIONE: f32 = 116.0;
/// Differenza ammessa fra questa misura e la resa di Slint, che applica anche la crenatura.
pub const MARGINE: f32 = 3.0;

const INTER_MEDIO: &[u8] = include_bytes!("../../assets/font/Inter-Medium.ttf");
const INTER_NERO: &[u8] = include_bytes!("../../assets/font/Inter-Bold.ttf");
const MONO_MEDIO: &[u8] = include_bytes!("../../assets/font/JetBrainsMonoNL-Medium.ttf");

/// Un carattere incorporato: letto una volta sola dai byte del `.ttf`.
struct Carattere {
    faccia: ttf_parser::Face<'static>,
}

impl Carattere {
    fn leggi(byte: &'static [u8]) -> Carattere {
        Carattere { faccia: ttf_parser::Face::parse(byte, 0).expect("carattere incorporato valido") }
    }

    /// Avanzamento di un carattere in unità del corpo (em); un glifo assente conta come `.notdef`.
    fn avanzamento(&self, c: char) -> f32 {
        let glifo = self.faccia.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0));
        let unita = self.faccia.glyph_hor_advance(glifo).unwrap_or(0);
        f32::from(unita) / f32::from(self.faccia.units_per_em())
    }
}

thread_local! {
    static CARATTERI: [Carattere; 3] = [
        Carattere::leggi(INTER_MEDIO),
        Carattere::leggi(INTER_NERO),
        Carattere::leggi(MONO_MEDIO),
    ];
}

impl Stile {
    /// Posto del carattere in `CARATTERI`, corpo in px e tracking in px per carattere.
    fn parametri(self) -> (usize, f32, f32) {
        match self {
            Stile::Titolo => (1, 18.0, -0.27),
            Stile::TitoloNumerico => (2, 26.0, -0.52),
            Stile::Riga => (0, 12.0, 0.0),
            #[cfg(test)]
            Stile::Etichetta => (2, 11.0, 0.77),
            #[cfg(test)]
            Stile::Capsula => (1, 11.0, 0.0),
        }
    }
}

/// Larghezza in px di un carattere (avanzamento scalato più tracking).
fn larghezza_carattere(c: char, stile: Stile) -> f32 {
    let (posto, corpo, tracking) = stile.parametri();
    CARATTERI.with(|caratteri| caratteri[posto].avanzamento(c) * corpo + tracking)
}

/// Larghezza in px di un testo: somma degli avanzamenti scalati più il tracking per carattere
/// (senza crenatura: la differenza con Slint sta dentro `MARGINE`).
pub fn larghezza(testo: &str, stile: Stile) -> f32 {
    testo.chars().map(|c| larghezza_carattere(c, stile)).sum()
}

/// Il testo intero se sta in `massimo` px, altrimenti tagliato a carattere con «…» in fondo, in
/// modo che il tutto stia in `massimo`.
pub fn accorcia(testo: &str, stile: Stile, massimo: f32) -> String {
    if larghezza(testo, stile) <= massimo {
        return testo.to_string();
    }
    let ellissi = larghezza_carattere('…', stile);
    let mut usato = 0.0;
    let mut fine = 0;
    for (i, c) in testo.char_indices() {
        usato += larghezza_carattere(c, stile);
        if usato + ellissi > massimo {
            break;
        }
        fine = i + c.len_utf8();
    }
    format!("{}…", testo[..fine].trim_end())
}

#[cfg(test)]
mod test {
    use super::*;

    const CIFRE: &str = "0123456789";

    #[test]
    fn cifre_mono_uguali() {
        let zero = larghezza("0", Stile::TitoloNumerico);
        assert!(zero > 10.0, "una cifra del mono a 26 px è larga più di 10 px: {zero}");
        for c in CIFRE.chars() {
            assert_eq!(larghezza(&c.to_string(), Stile::TitoloNumerico), zero, "cifra {c}");
        }
        let ora = larghezza("88:88:88", Stile::TitoloNumerico);
        assert!((ora - 121.0).abs() <= 2.0, "88:88:88 = {ora}");
    }

    #[test]
    fn valori_dello_spike() {
        // Valori di 11-spike.md §4 (somma degli avanzamenti, senza crenatura).
        for (testo, stile, atteso) in [
            ("Tutto regolare.", Stile::Titolo, 125.0),
            ("4 luci accese", Stile::Titolo, 112.0),
            ("AGENTI", Stile::Etichetta, 44.0),
            ("Al lavoro", Stile::Capsula, 48.0),
        ] {
            let l = larghezza(testo, stile);
            assert!((l - atteso).abs() <= 3.0, "«{testo}» {stile:?} = {l}, atteso {atteso}");
        }
    }

    #[test]
    fn accorcia_con_ellissi() {
        let w = "W".repeat(40);
        let corto = accorcia(&w, Stile::Riga, SPAZIO_CON_AZIONE);
        assert!(corto.ends_with('…'), "{corto}");
        assert!(corto.chars().count() > 1, "qualche «W» resta: {corto}");
        assert!(
            larghezza(&corto, Stile::Riga) <= SPAZIO_CON_AZIONE,
            "{corto} = {}",
            larghezza(&corto, Stile::Riga)
        );
        assert_eq!(accorcia("Posta", Stile::Riga, SPAZIO_CON_AZIONE), "Posta");
    }

    #[test]
    fn spazi_della_carta() {
        // Carta 182 − 2 × 14 di margine; il bottone d'azione toglie 32 + 6 (spec §13.6).
        assert_eq!(SPAZIO_PIENO, 182.0 - 2.0 * 14.0);
        assert_eq!(SPAZIO_CON_AZIONE, SPAZIO_PIENO - 32.0 - 6.0);
    }
}
