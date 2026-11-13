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
        }
        assert_eq!(da_str("boh"), Layout::Orologio);
    }
}
