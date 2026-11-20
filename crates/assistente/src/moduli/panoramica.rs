//! La barra delle schede della cornice (ridisegno F1, Task 6): dove sta ogni scheda e come la
//! legge Narrator. Funzioni pure: le scrive nel pannello `Ponte::scrivi_schede`.

use super::misura::{larghezza, Stile};
use super::Tono;

/// Una scheda inattiva: solo l'icona, in un cerchio di 36.
const INATTIVA: f32 = 36.0;
/// La scheda attiva: margine sinistro, icona, spazio fra icona e nome, (nome), margine destro.
const MARGINE_SINISTRO: f32 = 12.0;
const ICONA: f32 = 18.0;
const SPAZIO_ICONA: f32 = 8.0;
const MARGINE_DESTRO: f32 = 14.0;
/// Dopo la Panoramica: il divisore (1) e i suoi margini (12 in tutto).
const DIVISORE: f32 = 1.0 + 12.0;
/// Spazio fra due schede.
const PASSO: f32 = 2.0;

/// `(x, larghezza)` di ogni scheda: inattiva 36; attiva 12 + 18 + 8 + nome + 14, col nome misurato
/// coi caratteri incorporati (`Stile::Etichetta`, lo stesso stile con cui Slint lo disegna) e
/// arrotondato in su al pixel. Dopo la prima (la Panoramica) c'è il divisore. `attiva` fuori
/// dall'elenco (una vista che non è una scheda): tutte inattive. I nomi arrivano già maiuscoli.
pub fn geometria_schede(nomi: &[&str], attiva: usize) -> Vec<(f32, f32)> {
    let mut x = 0.0;
    nomi.iter()
        .enumerate()
        .map(|(i, nome)| {
            if i == 1 {
                x += DIVISORE;
            }
            let w = if i == attiva {
                MARGINE_SINISTRO + ICONA + SPAZIO_ICONA + larghezza(nome, Stile::Etichetta).ceil() + MARGINE_DESTRO
            } else {
                INATTIVA
            };
            let posto = (x, w);
            x += w + PASSO;
            posto
        })
        .collect()
}

/// Il segno della scheda (variante «misto», spec §5.3): occhietti solo per «ti aspetta» (1) e
/// «guarda presto» (2); gli altri toni non hanno segno sulla scheda (lo dicono la carta e Narrator).
pub fn segno(tono: Tono) -> i32 {
    match tono {
        Tono::Aspetta => 1,
        Tono::Guarda => 2,
        _ => 0,
    }
}

/// Il nome per Narrator: «{nome}[, {stato}], scheda {i} di {n}» (i da 1). Lo stato c'è per ogni
/// tono che ha una parola (al lavoro, fatto…), anche quando la scheda non ha segno.
pub fn narratore(nome: &str, tono: Tono, posizione: usize, quante: usize) -> String {
    let stato = tono.per_narrator();
    if stato.is_empty() {
        format!("{nome}, scheda {} di {quante}", posizione + 1)
    } else {
        format!("{nome}, {stato}, scheda {} di {quante}", posizione + 1)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const NOMI: [&str; 5] = ["PANORAMICA", "AGENTI", "CASA", "PC", "LINK"];

    /// Le x crescono e le schede non si toccano (passo 2, divisore dopo la prima).
    fn senza_sovrapposizioni(g: &[(f32, f32)]) {
        for coppia in g.windows(2) {
            let ((x0, w0), (x1, _)) = (coppia[0], coppia[1]);
            assert!(x1 >= x0 + w0 + PASSO, "sovrapposte: {g:?}");
        }
    }

    #[test]
    fn geometria_delle_schede() {
        let g = geometria_schede(&NOMI, 0);
        assert_eq!(g.len(), 5);
        assert!(g[0].1 > INATTIVA, "la Panoramica attiva è larga col nome: {g:?}");
        assert!(g[1..].iter().all(|&(_, w)| w == INATTIVA), "{g:?}");
        assert_eq!(g[0].0, 0.0);
        assert_eq!(g[1].0, g[0].1 + PASSO + DIVISORE, "divisore dopo la Panoramica");
        senza_sovrapposizioni(&g);

        let g = geometria_schede(&NOMI, 2);
        assert!(g[2].1 > INATTIVA, "la terza (Casa) larga: {g:?}");
        assert_eq!(g[0].1, INATTIVA, "la Panoramica torna a 36");
        assert_eq!(g[1].1, INATTIVA);
        senza_sovrapposizioni(&g);
        // Larghezza della scheda attiva: 12 + 18 + 8 + nome + 14, col nome misurato.
        let nome = larghezza("CASA", Stile::Etichetta).ceil();
        assert_eq!(g[2].1, 12.0 + 18.0 + 8.0 + nome + 14.0);

        // Una vista che non è una scheda: tutte inattive.
        assert!(geometria_schede(&NOMI, 9).iter().all(|&(_, w)| w == INATTIVA));
    }

    #[test]
    fn segni_e_nomi_per_narrator() {
        assert_eq!(segno(Tono::Aspetta), 1);
        assert_eq!(segno(Tono::Guarda), 2);
        for t in [Tono::Vuoto, Tono::Calmo, Tono::Attivo, Tono::Fatto, Tono::Errore, Tono::Fermo] {
            assert_eq!(segno(t), 0, "{t:?}");
        }
        assert_eq!(narratore("Agenti", Tono::Aspetta, 1, 5), "Agenti, ti aspetta, scheda 2 di 5");
        assert_eq!(narratore("Casa", Tono::Attivo, 2, 5), "Casa, al lavoro, scheda 3 di 5");
        assert_eq!(narratore("Panoramica", Tono::Calmo, 0, 5), "Panoramica, scheda 1 di 5");
    }
}
