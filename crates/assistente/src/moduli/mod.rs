//! Moduli del pannello (ridisegno F1): le carte della Panoramica e le loro regole.
//!
//! Il contratto è uguale per ogni modulo: un'identità fissa (`MODULI`) e un `Riassunto` con un
//! `Tono` del vocabolario chiuso (spec §5.3), un titolo, una riga e al più un'azione. I testi
//! stanno nei limiti in pixel della carta (spec §13.6–§13.7): `controlla` li misura coi caratteri
//! incorporati, e il `Registro` scarta un riassunto che non ci sta.
//!
//! I riassunti dei moduli sono funzioni pure (`agenti`, `casa`, `pc`): il ponte le chiama quando
//! cambia un loro dato (`ponte::con_moduli`).

pub mod agenti;
pub mod casa;
pub mod misura;
pub mod pc;
pub mod registro;

use crate::attenzione::Livello;
use misura::{accorcia, larghezza, Stile, MARGINE, SPAZIO_CON_AZIONE, SPAZIO_PIENO};
use std::fmt;

/// Lo stato di un modulo, uguale per tutti (spec §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tono {
    /// Non configurato o primo avvio («—» col motivo).
    Vuoto,
    Calmo,
    /// Al lavoro, suona, conta.
    Attivo,
    /// Livello attenzione.
    Guarda,
    /// Livello bloccante.
    Aspetta,
    Fatto,
    Errore,
    /// Non raggiungibile, spento, dato vecchio.
    Fermo,
}

impl Tono {
    /// Più alto = vince (pillola e arbitraggio): aspetta > guarda = errore > fatto > attivo > calmo.
    pub fn priorita(self) -> u8 {
        match self {
            Tono::Aspetta => 5,
            Tono::Guarda | Tono::Errore => 4,
            Tono::Fatto => 3,
            Tono::Attivo => 2,
            Tono::Calmo | Tono::Fermo | Tono::Vuoto => 1,
        }
    }

    /// La parola breve della capsula della carta (spec §13.5); calmo e vuoto non hanno capsula.
    pub fn parola(self) -> &'static str {
        match self {
            Tono::Aspetta => "Aspetta",
            Tono::Guarda => "Guarda",
            Tono::Attivo => "Al lavoro",
            Tono::Fatto => "Fatto",
            Tono::Errore => "Errore",
            Tono::Fermo => "Fermo",
            Tono::Calmo | Tono::Vuoto => "",
        }
    }

    /// La forma lunga per Narrator (spec §5.3: nella pillola, nella Coda e nei nomi accessibili).
    pub fn per_narrator(self) -> &'static str {
        match self {
            Tono::Aspetta => "ti aspetta",
            Tono::Guarda => "guarda presto",
            Tono::Attivo => "al lavoro",
            Tono::Fatto => "fatto",
            Tono::Errore => "errore",
            Tono::Fermo => "non risponde",
            Tono::Calmo | Tono::Vuoto => "",
        }
    }

    /// La luce che sale dal basso (spec §13.3): solo per al lavoro, guarda e aspetta. La applica
    /// `Tema.luce-tono` in Slint; qui la regola per il test.
    #[cfg(test)]
    pub fn luce(self) -> bool {
        matches!(self, Tono::Attivo | Tono::Guarda | Tono::Aspetta)
    }

    /// Il tono che una voce «In attesa» dà al suo modulo.
    pub fn da_livello(l: Livello) -> Tono {
        match l {
            Livello::Bloccante => Tono::Aspetta,
            Livello::Attenzione => Tono::Guarda,
            Livello::Informazione => Tono::Calmo,
        }
    }
}

/// Chi è un modulo: identificativo fisso, nome (scheda, Narrator), nome breve (etichetta della
/// carta accanto alla capsula) e icona (`pannello::icona`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identita {
    pub id: &'static str,
    pub nome: &'static str,
    pub nome_breve: &'static str,
    pub icona: &'static str,
}

/// I moduli di F1, nell'ordine predefinito.
pub const MODULI: [Identita; 4] = [
    Identita { id: "agenti", nome: "Agenti", nome_breve: "Agenti", icona: "agenti" },
    Identita { id: "casa", nome: "Casa", nome_breve: "Casa", icona: "casa" },
    Identita { id: "pc", nome: "PC", nome_breve: "PC", icona: "pc" },
    Identita { id: "collegamenti", nome: "Collegamenti", nome_breve: "Link", icona: "collegamenti" },
];

/// L'azione di una carta (il bottone tondo): `id` lo ritraduce il modulo, `testo` è per Narrator
/// e per il suggerimento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Azione {
    pub id: &'static str,
    pub testo: String,
    pub icona: &'static str,
}

/// Quello che una carta mostra. `sec` è la riga (già accorciata), `sec_intera` la stessa per
/// Narrator; `titolo_numerico`: il titolo è un numero (mono 26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Riassunto {
    pub tono: Tono,
    pub titolo: String,
    pub titolo_numerico: bool,
    pub sec: String,
    pub sec_intera: String,
    pub azione: Option<Azione>,
}

impl Riassunto {
    /// Primo avvio o dato assente: «—» col motivo, nessuna azione (assenza ≠ zero).
    pub fn primo_avvio(motivo: &str) -> Riassunto {
        Riassunto {
            tono: Tono::Vuoto,
            titolo: "—".into(),
            titolo_numerico: false,
            sec: motivo.into(),
            sec_intera: motivo.into(),
            azione: None,
        }
    }
}

/// Un testo di un riassunto che non sta nella carta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuoriMisura {
    pub modulo: &'static str,
    pub campo: &'static str,
    pub larghezza: f32,
    pub massimo: f32,
}

impl fmt::Display for FuoriMisura {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "modulo {}: {} largo {:.1} px, massimo {:.1} px",
            self.modulo, self.campo, self.larghezza, self.massimo
        )
    }
}

/// Lo spazio di un testo della carta: quello della carta (meno il bottone, se c'è l'azione) meno
/// il margine fra questa misura e la resa di Slint.
fn massimo(con_azione: bool) -> f32 {
    (if con_azione { SPAZIO_CON_AZIONE } else { SPAZIO_PIENO }) - MARGINE
}

/// Il nome dell'utente sotto questa larghezza non si legge più: meglio la riga senza.
const MINIMO_NOME: f32 = 32.0;

/// Una riga della carta accorciata con «…» se non ci sta (rete di sicurezza: i testi fissi delle
/// scale ci stanno già, lo provano i test di ogni modulo).
pub fn riga(testo: &str, con_azione: bool) -> String {
    accorcia(testo, Stile::Riga, massimo(con_azione))
}

/// Una riga col nome dell'utente in mezzo (`{prima}{nome}{dopo}`): il nome si accorcia allo
/// spazio che le altre parti lasciano. `None` se per il nome restano meno di `MINIMO_NOME` px (chi
/// chiama sceglie un'altra riga). La riga intera, per Narrator, la compone chi chiama.
pub fn riga_con_nome(prima: &str, nome: &str, dopo: &str, con_azione: bool) -> Option<String> {
    let massimo = massimo(con_azione);
    let intera = format!("{prima}{nome}{dopo}");
    if larghezza(&intera, Stile::Riga) <= massimo {
        return Some(intera);
    }
    let spazio = massimo - larghezza(prima, Stile::Riga) - larghezza(dopo, Stile::Riga);
    (spazio >= MINIMO_NOME).then(|| format!("{prima}{}{dopo}", accorcia(nome, Stile::Riga, spazio)))
}

/// Titolo e riga stanno nello spazio della carta (meno quello del bottone, se c'è l'azione),
/// col margine fra questa misura e la resa di Slint.
pub fn controlla(id: &'static str, r: &Riassunto) -> Result<(), FuoriMisura> {
    let massimo = massimo(r.azione.is_some());
    let stile_titolo = if r.titolo_numerico { Stile::TitoloNumerico } else { Stile::Titolo };
    for (campo, testo, stile) in [("titolo", &r.titolo, stile_titolo), ("sec", &r.sec, Stile::Riga)] {
        let l = larghezza(testo, stile);
        if l > massimo {
            return Err(FuoriMisura { modulo: id, campo, larghezza: l, massimo });
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    const TONI: [Tono; 8] =
        [Tono::Vuoto, Tono::Calmo, Tono::Attivo, Tono::Guarda, Tono::Aspetta, Tono::Fatto, Tono::Errore, Tono::Fermo];

    #[test]
    fn priorita_della_spec() {
        // Aspetta > Guarda = Errore > Fatto > Attivo > Calmo (spec §5.3).
        assert!(Tono::Aspetta.priorita() > Tono::Guarda.priorita());
        assert_eq!(Tono::Guarda.priorita(), Tono::Errore.priorita());
        assert!(Tono::Errore.priorita() > Tono::Fatto.priorita());
        assert!(Tono::Fatto.priorita() > Tono::Attivo.priorita());
        assert!(Tono::Attivo.priorita() > Tono::Calmo.priorita());
        assert_eq!(Tono::Calmo.priorita(), Tono::Fermo.priorita());
        assert_eq!(Tono::Calmo.priorita(), Tono::Vuoto.priorita());

        let parole: Vec<_> = TONI.iter().map(|t| t.parola()).collect();
        assert_eq!(parole, ["", "", "Al lavoro", "Guarda", "Aspetta", "Fatto", "Errore", "Fermo"]);
        let lunghe: Vec<_> = TONI.iter().map(|t| t.per_narrator()).collect();
        assert_eq!(lunghe, ["", "", "al lavoro", "guarda presto", "ti aspetta", "fatto", "errore", "non risponde"]);
        let con_luce: Vec<_> = TONI.iter().filter(|t| t.luce()).copied().collect();
        assert_eq!(con_luce, [Tono::Attivo, Tono::Guarda, Tono::Aspetta]);

        assert_eq!(Tono::da_livello(Livello::Bloccante), Tono::Aspetta);
        assert_eq!(Tono::da_livello(Livello::Attenzione), Tono::Guarda);
        assert_eq!(Tono::da_livello(Livello::Informazione), Tono::Calmo);
    }

    /// Spec §13.5: nome breve maiuscolo + 22 (icona e spazio) + capsula (parola + 30) + 6 sta nei
    /// 154 px della carta, per ogni modulo e ogni parola; «Al lavoro» è la più larga.
    #[test]
    fn capsula_e_nome_breve_stanno_nella_carta() {
        let con_parola: Vec<_> = TONI.iter().filter(|t| !t.parola().is_empty()).collect();
        let piu_larga = con_parola
            .iter()
            .max_by(|a, b| larghezza(a.parola(), Stile::Capsula).total_cmp(&larghezza(b.parola(), Stile::Capsula)))
            .expect("toni con parola");
        assert_eq!(piu_larga.parola(), "Al lavoro", "la parola più larga è il tetto della capsula");
        for m in MODULI {
            for t in &con_parola {
                let totale = larghezza(&m.nome_breve.to_uppercase(), Stile::Etichetta)
                    + 22.0
                    + (larghezza(t.parola(), Stile::Capsula) + 30.0)
                    + 6.0;
                assert!(totale <= SPAZIO_PIENO, "{} con «{}»: {totale}", m.id, t.parola());
            }
        }
    }

    #[test]
    fn primo_avvio_e_trattino_col_motivo() {
        let r = Riassunto::primo_avvio("Casa da collegare");
        assert_eq!((r.tono, r.titolo.as_str(), r.sec.as_str()), (Tono::Vuoto, "—", "Casa da collegare"));
        assert_eq!(r.sec_intera, "Casa da collegare");
        assert!(r.azione.is_none() && !r.titolo_numerico);
    }

    #[test]
    fn fuori_misura_si_legge_coi_campi() {
        let f = FuoriMisura { modulo: "casa", campo: "sec", larghezza: 120.5, massimo: 113.0 };
        assert_eq!(f.to_string(), "modulo casa: sec largo 120.5 px, massimo 113.0 px");
    }
}
