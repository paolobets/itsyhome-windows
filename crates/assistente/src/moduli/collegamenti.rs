//! Il riassunto dei Collegamenti: quanti sono e quello che l'utente apre di più.

use super::{riga_con_nome, Azione, Riassunto, Tono, R1};
use casa_core::config::Collegamento;

/// «Apri {nome}»: il collegamento più usato, come un clic sulla sua card.
pub const APRI: &str = "collegamenti.apri";
/// «Aggiungi»: le Impostazioni sulla scheda Collegamenti.
pub const AGGIUNGI: &str = "collegamenti.aggiungi";

/// Il collegamento più usato: più lanci; a parità il più recente; con tutti a zero il primo
/// nell'ordine scelto dall'utente.
pub fn piu_usato(c: &[Collegamento]) -> Option<&Collegamento> {
    // `max_by_key` tiene l'ultimo dei pari: a parità completa si vuole il primo, quindi si scorre
    // tenendo il candidato solo se il nuovo è strettamente meglio.
    c.iter().reduce(|migliore, x| if (x.lanci, x.ultimo_lancio) > (migliore.lanci, migliore.ultimo_lancio) { x } else { migliore })
}

/// La carta dei Collegamenti. Nessun conteggio è mai «0»: senza collegamenti la carta è vuota e
/// propone di aggiungerne uno. Testi corti come da scala (la carta ha 113 px con il bottone):
/// «{n} link» e «Nessun link»; il nome intero resta per Narrator e per il bottone.
pub fn riassunto(c: &[Collegamento]) -> Riassunto {
    let Some(primo) = piu_usato(c) else {
        return Riassunto {
            tono: Tono::Vuoto,
            titolo: "Nessun link".into(),
            titolo_numerico: false,
            sec: "Aggiungine uno".into(),
            sec_intera: "Aggiungine uno".into(),
            azione: Some(Azione { id: AGGIUNGI, testo: "Aggiungi".into(), icona: "collegamenti" }),
            r1: R1 { fatto: "Nessun collegamento".into(), valore: String::new() },
        };
    };
    let titolo = format!("{} link", c.len());
    let intera = format!("Più usato: {}", primo.nome);
    // Se per il nome non resta spazio leggibile, la riga dice solo «Più usato».
    let sec = riga_con_nome("Più usato: ", &primo.nome, "", true).unwrap_or_else(|| "Più usato".into());
    Riassunto {
        tono: Tono::Calmo,
        titolo,
        titolo_numerico: false,
        sec,
        sec_intera: intera,
        azione: Some(Azione { id: APRI, testo: format!("Apri {}", primo.nome), icona: "collegamenti" }),
        r1: R1 { fatto: if c.len() == 1 { "1 collegamento".into() } else { format!("{} collegamenti", c.len()) }, valore: String::new() },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::moduli::controlla;
    use crate::moduli::misura::{larghezza, Stile, MARGINE, SPAZIO_R1};

    fn col(id: &str, nome: &str, lanci: u32, ultimo: i64) -> Collegamento {
        Collegamento { id: id.into(), nome: nome.into(), url: format!("https://{id}.esempio.lan"), lanci, ultimo_lancio: ultimo }
    }

    fn id_piu_usato(c: &[Collegamento]) -> Option<&str> {
        piu_usato(c).map(|c| c.id.as_str())
    }

    #[test]
    fn piu_usato_e_parita_al_piu_recente() {
        // Parità di lanci: il più recente.
        assert_eq!(id_piu_usato(&[col("a", "A", 3, 10), col("b", "B", 3, 20)]), Some("b"));
        assert_eq!(id_piu_usato(&[col("a", "A", 3, 20), col("b", "B", 3, 10)]), Some("a"));
        // Più lanci batte l'ora: (5, t=1) contro (3, t=99).
        assert_eq!(id_piu_usato(&[col("a", "A", 3, 99), col("b", "B", 5, 1)]), Some("b"));
        // Tutti a zero (file di prima): il primo dell'ordine dell'utente.
        assert_eq!(id_piu_usato(&[col("a", "A", 0, 0), col("b", "B", 0, 0), col("c", "C", 0, 0)]), Some("a"));
        // Parità completa: il primo.
        assert_eq!(id_piu_usato(&[col("a", "A", 2, 7), col("b", "B", 2, 7)]), Some("a"));
        assert_eq!(id_piu_usato(&[]), None);
    }

    #[test]
    fn scala_per_numero() {
        let r = riassunto(&[col("nas", "NAS", 0, 0)]);
        assert_eq!((r.tono, r.titolo.as_str(), r.sec.as_str()), (Tono::Calmo, "1 link", "Più usato: NAS"));
        assert!(!r.titolo_numerico);
        let a = r.azione.expect("azione");
        assert_eq!((a.id, a.testo.as_str()), (APRI, "Apri NAS"));
        let r = riassunto(&[col("a", "A", 1, 5), col("b", "Proxmox", 4, 9), col("c", "C", 2, 1)]);
        assert_eq!((r.titolo.as_str(), r.sec.as_str()), ("3 link", "Più usato: Proxmox"));
        assert_eq!(r.sec_intera, "Più usato: Proxmox");
        assert_eq!(r.azione.map(|a| a.testo), Some("Apri Proxmox".to_owned()));
    }

    /// Il titolo della testata (`r1.fatto`): nessuno, uno, tanti; il valore non c'è.
    #[test]
    fn scala_per_stato() {
        let fatto = |c: &[Collegamento]| riassunto(c).r1;
        assert_eq!(fatto(&[]), R1 { fatto: "Nessun collegamento".into(), valore: String::new() });
        assert_eq!(fatto(&[col("a", "NAS", 0, 0)]).fatto, "1 collegamento");
        assert_eq!(fatto(&[col("a", "NAS", 0, 0), col("b", "Proxmox", 4, 9)]).fatto, "2 collegamenti");
        let molti: Vec<Collegamento> = (0..100).map(|i| col(&format!("c{i}"), "NAS", 0, 0)).collect();
        assert_eq!(fatto(&molti).fatto, "100 collegamenti");
    }

    #[test]
    fn ogni_gradino_sta_nei_limiti() {
        let molti: Vec<Collegamento> = (0..100).map(|i| col(&format!("c{i}"), "NAS", 0, 0)).collect();
        for c in [
            vec![col("a", "NAS", 0, 0)],
            vec![col("a", "W", 0, 0); 10],
            vec![col("a", "WWWWWWWWWW", 3, 1); 99],
            molti,
        ] {
            let r = riassunto(&c);
            assert_eq!(controlla("collegamenti", &r), Ok(()), "{r:?}");
            // Il titolo della testata c'è sempre, e `controlla` lo misura (caratteri e pixel).
            assert!(!r.r1.fatto.is_empty(), "{r:?}");
            assert!(larghezza(&r.r1.fatto, Stile::Titolo) <= SPAZIO_R1 - MARGINE, "{r:?}");
        }
        assert_eq!(controlla("collegamenti", &riassunto(&[])), Ok(()));
        // Il nome più lungo non entra nel titolo della testata: nome da 40 «W», titolo uguale.
        let lungo = riassunto(&[col("a", &"W".repeat(40), 4, 1)]);
        assert_eq!(lungo.r1.fatto, "1 collegamento");
        assert_eq!(controlla("collegamenti", &lungo), Ok(()));
    }

    /// Senza collegamenti il titolo della testata dice la cosa a parole: mai «0».
    #[test]
    fn assenza_non_e_zero() {
        let r = riassunto(&[]);
        assert!(!r.r1.fatto.contains('0') && !r.r1.valore.contains('0'), "{r:?}");
        assert!(!r.r1.fatto.is_empty(), "mai un vuoto");
        assert_eq!(r.r1.fatto, "Nessun collegamento");
    }

    /// Un nome di 40 «W»: nella carta ellissi, intero per Narrator e nel bottone; il registro non
    /// scarta il riassunto.
    #[test]
    fn piu_usato_con_nome_lunghissimo() {
        let nome = "W".repeat(40);
        let r = riassunto(&[col("a", &nome, 4, 1), col("b", "Altro", 1, 1)]);
        assert_eq!(controlla("collegamenti", &r), Ok(()), "{r:?}");
        assert!(r.sec.ends_with('…'), "ellissi nella carta: {}", r.sec);
        assert!(r.sec.starts_with("Più usato: "), "{}", r.sec);
        assert_eq!(r.sec_intera, format!("Più usato: {nome}"), "intero per Narrator");
        assert_eq!(r.azione.map(|a| a.testo), Some(format!("Apri {nome}")), "intero anche nel bottone");
        // Il registro accetta il riassunto.
        let mut reg = crate::moduli::registro::Registro::nuovo(&[]);
        assert_eq!(reg.aggiorna("collegamenti", riassunto(&[col("a", &nome, 4, 1)])), Ok(true));
    }

    /// Senza collegamenti: carta vuota con il motivo e «Aggiungi», mai «0».
    #[test]
    fn nessun_collegamento() {
        let r = riassunto(&[]);
        assert_eq!(controlla("collegamenti", &r), Ok(()));
        assert_eq!((r.tono, r.titolo.as_str(), r.sec.as_str()), (Tono::Vuoto, "Nessun link", "Aggiungine uno"));
        assert!(!r.titolo.contains('0') && !r.sec.contains('0'), "{r:?}");
        let a = r.azione.expect("azione");
        assert_eq!((a.id, a.testo.as_str()), (AGGIUNGI, "Aggiungi"));
    }
}
