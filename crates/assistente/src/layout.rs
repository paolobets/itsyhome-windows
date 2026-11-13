//! I due aspetti della Panoramica (Orologio e Quadranti), cambiabili al volo con i puntini, le
//! frecce e la rotella. L'ordine qui è quello dei puntini in `pannello.slint`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Orologio,
    Quadranti,
}

pub const TUTTI: [Layout; 2] = [Layout::Orologio, Layout::Quadranti];

/// Dal valore salvato. I layout tolti ("colori", "tutte") e i valori sconosciuti: Orologio.
pub fn da_str(s: &str) -> Layout {
    TUTTI.into_iter().find(|l| l.come_str() == s).unwrap_or(Layout::Orologio)
}

impl Layout {
    /// Dall'indice del puntino; fuori intervallo: Orologio.
    pub fn da_indice(i: i32) -> Layout {
        usize::try_from(i).ok().and_then(|i| TUTTI.get(i).copied()).unwrap_or(Layout::Orologio)
    }

    pub fn come_str(self) -> &'static str {
        match self {
            Layout::Orologio => "orologio",
            Layout::Quadranti => "quadranti",
        }
    }

    pub fn indice(self) -> i32 {
        match self {
            Layout::Orologio => 0,
            Layout::Quadranti => 1,
        }
    }

    /// Il layout dopo questo (dall'ultimo si torna al primo).
    pub fn successivo(self) -> Layout {
        TUTTI[(self.indice() as usize + 1) % TUTTI.len()]
    }

    /// Il layout prima di questo (dal primo si va all'ultimo).
    pub fn precedente(self) -> Layout {
        TUTTI[(self.indice() as usize + TUTTI.len() - 1) % TUTTI.len()]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn layout_andata_e_ritorno() {
        for (i, l) in TUTTI.into_iter().enumerate() {
            assert_eq!(da_str(l.come_str()), l);
            assert_eq!(l.indice(), i as i32);
            assert_eq!(Layout::da_indice(i as i32), l);
        }
        assert_eq!(Layout::da_indice(-1), Layout::Orologio);
        assert_eq!(Layout::da_indice(2), Layout::Orologio);
        assert_eq!(da_str("boh"), Layout::Orologio);
    }

    #[test]
    fn layout_solo_due_e_ciclici() {
        assert_eq!(Layout::Orologio.successivo(), Layout::Quadranti);
        assert_eq!(Layout::Quadranti.successivo(), Layout::Orologio);
        assert_eq!(Layout::Orologio.precedente(), Layout::Quadranti);
        assert_eq!(Layout::Quadranti.precedente(), Layout::Orologio);
    }

    #[test]
    fn layout_vecchi_diventano_orologio() {
        assert_eq!(da_str("colori"), Layout::Orologio);
        assert_eq!(da_str("tutte"), Layout::Orologio);
    }
}
