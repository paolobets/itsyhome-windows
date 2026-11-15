//! Registro delle app apribili dal pannello: sono le icone del dock, nell'ordine. La prima,
//! la Panoramica, è la schermata principale.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoceApp {
    pub id: &'static str,
    pub nome: &'static str,
    pub icona: &'static str,
}

pub fn app_disponibili() -> Vec<VoceApp> {
    vec![
        VoceApp { id: "panoramica", nome: "Panoramica", icona: "panoramica" },
        VoceApp { id: "casa", nome: "Casa", icona: "casa" },
        VoceApp { id: "pc", nome: "PC", icona: "pc" },
        // I collegamenti veloci (Task 14f): pagine web aperte nel browser predefinito.
        VoceApp { id: "collegamenti", nome: "Collegamenti", icona: "collegamenti" },
        VoceApp { id: "impostazioni", nome: "Impostazioni", icona: "impostazioni" },
    ]
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn app_disponibili_con_panoramica() {
        let ids: Vec<_> = app_disponibili().iter().map(|a| a.id).collect();
        assert_eq!(ids, ["panoramica", "casa", "pc", "collegamenti", "impostazioni"]);
    }

    #[test]
    fn app_disponibili_solo_quelle_vere() {
        let v = app_disponibili();
        let ids: Vec<_> = v.iter().map(|a| a.id).collect();
        assert_eq!(ids, ["panoramica", "casa", "pc", "collegamenti", "impostazioni"]);
        let nomi: Vec<_> = v.iter().map(|a| a.nome).collect();
        assert_eq!(nomi, ["Panoramica", "Casa", "PC", "Collegamenti", "Impostazioni"]);
    }
}
