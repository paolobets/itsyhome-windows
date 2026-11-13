//! Registro delle app apribili dal pannello.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoceApp {
    pub id: &'static str,
    pub nome: &'static str,
    pub icona: &'static str,
}

pub fn app_disponibili() -> Vec<VoceApp> {
    vec![
        VoceApp { id: "casa", nome: "Casa", icona: "casa" },
        VoceApp { id: "pc", nome: "PC", icona: "pc" },
        VoceApp { id: "impostazioni", nome: "Impostazioni", icona: "impostazioni" },
    ]
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn app_disponibili_solo_quelle_vere() {
        let v = app_disponibili();
        let ids: Vec<_> = v.iter().map(|a| a.id).collect();
        assert_eq!(ids, ["casa", "pc", "impostazioni"]);
        let nomi: Vec<_> = v.iter().map(|a| a.nome).collect();
        assert_eq!(nomi, ["Casa", "PC", "Impostazioni"]);
    }
}
