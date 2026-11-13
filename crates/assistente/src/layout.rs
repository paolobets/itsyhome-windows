//! Disposizione della schermata principale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Orologio,
    Quadranti,
    Colori,
    TutteLeApp,
}

const TUTTI: [Layout; 4] = [Layout::Orologio, Layout::Quadranti, Layout::Colori, Layout::TutteLeApp];

pub fn da_str(s: &str) -> Layout {
    TUTTI.into_iter().find(|l| l.come_str() == s).unwrap_or(Layout::Orologio)
}

impl Layout {
    /// Dall'indice del dock (0..3); fuori intervallo: Orologio.
    pub fn da_indice(i: i32) -> Layout {
        usize::try_from(i).ok().and_then(|i| TUTTI.get(i).copied()).unwrap_or(Layout::Orologio)
    }

    pub fn come_str(self) -> &'static str {
        match self {
            Layout::Orologio => "orologio",
            Layout::Quadranti => "quadranti",
            Layout::Colori => "colori",
            Layout::TutteLeApp => "tutte",
        }
    }

    pub fn indice(self) -> i32 {
        match self {
            Layout::Orologio => 0,
            Layout::Quadranti => 1,
            Layout::Colori => 2,
            Layout::TutteLeApp => 3,
        }
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
        assert_eq!(Layout::da_indice(4), Layout::Orologio);
        assert_eq!(da_str("boh"), Layout::Orologio);
    }
}
