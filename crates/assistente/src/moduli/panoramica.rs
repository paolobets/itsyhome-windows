//! La barra delle schede della cornice (ridisegno F1, Task 6): dove sta ogni scheda e come la
//! legge Narrator. Funzioni pure: le scrive nel pannello `Ponte::scrivi_schede`.
//!
//! La Panoramica a griglia (Task 8): i bersagli del fuoco da tastiera sono numeri fissi (le
//! stesse costanti stanno in `ui/guscio/panoramica.slint`, un test le confronta) e l'ordine di Tab
//! lo decide `ordine_tab` da ciò che c'è adesso (`Composizione`).

use super::misura::{larghezza, Stile};
use super::Tono;

/// La voce che ti aspetta: «Nega» (il primo pulsante non delicato), l'altro pulsante («Consenti»
/// o «Apri») e «+N in attesa ›».
pub const ID_NEGA: i32 = 100;
pub const ID_CONSENTI: i32 = 101;
pub const ID_ALTRE: i32 = 102;
/// Le schede della barra: `ID_SCHEDA + k`.
pub const ID_SCHEDA: i32 = 200;
pub const ID_RICERCA: i32 = 300;
pub const ID_VOLUME: i32 = 310;
pub const ID_MENU: i32 = 320;
/// «N in attesa ›» della riga alta.
pub const ID_IN_ATTESA: i32 = 330;
/// I posti delle carte: la carta k è `2k`, il suo bottone d'azione `2k + 1`.
pub const POSTI: usize = 4;

/// Ciò che la Panoramica mostra adesso, per l'ordine di Tab: la voce che ti aspetta (coi suoi
/// pulsanti), il suo «+N», «N in attesa ›» della riga alta, quali carte hanno l'azione, quante
/// schede ha la barra e se c'è il volume.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composizione {
    pub voce: bool,
    pub altre: bool,
    pub in_attesa: bool,
    pub azioni: Vec<bool>,
    pub schede: usize,
    pub volume: bool,
}

/// L'ordine di Tab della Panoramica: voce («Nega» → «Consenti» → «+N»), «N in attesa ›» (spec
/// §5.9, C11: la riga alta in cima, prima delle carte), carte (corpo → azione, solo se c'è),
/// schede, ricerca, volume (se c'è), «⋯». Con al più `POSTI` moduli nessuna carta è coperta.
pub fn ordine_tab(c: &Composizione) -> Vec<i32> {
    let mut ordine = Vec::new();
    if c.voce {
        ordine.extend([ID_NEGA, ID_CONSENTI]);
        if c.altre {
            ordine.push(ID_ALTRE);
        }
    }
    if c.in_attesa {
        ordine.push(ID_IN_ATTESA);
    }
    for (k, &azione) in c.azioni.iter().take(POSTI).enumerate() {
        let k = k as i32;
        ordine.push(2 * k);
        if azione {
            ordine.push(2 * k + 1);
        }
    }
    ordine.extend((0..c.schede as i32).map(|k| ID_SCHEDA + k));
    ordine.push(ID_RICERCA);
    if c.volume {
        ordine.push(ID_VOLUME);
    }
    ordine.push(ID_MENU);
    ordine
}

/// Il bersaglio dopo (o prima) di `attuale`, in giro; un `attuale` che non è nell'elenco (nessun
/// fuoco, bersaglio sparito): il primo andando avanti, l'ultimo tornando indietro.
pub fn prossimo(ordine: &[i32], attuale: i32, avanti: bool) -> i32 {
    let n = ordine.len();
    if n == 0 {
        return attuale;
    }
    match ordine.iter().position(|&id| id == attuale) {
        Some(i) if avanti => ordine[(i + 1) % n],
        Some(i) => ordine[(i + n - 1) % n],
        None if avanti => ordine[0],
        None => ordine[n - 1],
    }
}

/// Quante voci in attesa dice «N in attesa ›» della riga alta: tutte, se nessuna voce ti aspetta
/// al posto dell'orologio; con la voce mostrata nessuna (le altre le conta il suo «+N»).
pub fn in_attesa_non_mostrate(voci: usize, voce_mostrata: bool) -> usize {
    if voce_mostrata {
        0
    } else {
        voci
    }
}

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

    fn composizione() -> Composizione {
        Composizione { azioni: vec![true, true, false, true], schede: 5, volume: true, ..Composizione::default() }
    }

    #[test]
    fn ordine_tab_della_spec() {
        let schede: Vec<i32> = (200..205).collect();
        let mut atteso = vec![0, 1, 2, 3, 4, 6, 7];
        atteso.extend(&schede);
        atteso.extend([300, 310, 320]);
        assert_eq!(ordine_tab(&composizione()), atteso, "senza voce");

        let con_voce = ordine_tab(&Composizione { voce: true, altre: true, ..composizione() });
        assert_eq!(con_voce[..4], [100, 101, 102, 0], "con voce e «+N»");
        assert_eq!(con_voce.len(), atteso.len() + 3);
        let senza_altre = ordine_tab(&Composizione { voce: true, ..composizione() });
        assert_eq!(senza_altre[..3], [100, 101, 0], "senza «+N»");

        let senza_volume = ordine_tab(&Composizione { volume: false, ..composizione() });
        assert!(!senza_volume.contains(&310), "{senza_volume:?}");
        assert_eq!(senza_volume[senza_volume.len() - 2..], [300, 320]);

        // C11: «N in attesa ›» subito dopo la voce (qui assente), prima delle carte.
        let in_attesa = ordine_tab(&Composizione { in_attesa: true, ..composizione() });
        assert_eq!(in_attesa[..2], [330, 0], "«N in attesa ›» prima delle carte");
        let tutto = ordine_tab(&Composizione { voce: true, altre: true, in_attesa: true, ..composizione() });
        assert_eq!(tutto[..5], [100, 101, 102, 330, 0]);
        // Le costanti sono quelle della spec.
        assert_eq!((ID_NEGA, ID_CONSENTI, ID_ALTRE, ID_SCHEDA, ID_RICERCA, ID_VOLUME, ID_MENU, ID_IN_ATTESA), (100, 101, 102, 200, 300, 310, 320, 330));
    }

    #[test]
    fn prossimo_in_giro() {
        let ordine = ordine_tab(&composizione());
        assert_eq!(prossimo(&ordine, 0, true), 1);
        assert_eq!(prossimo(&ordine, 1, true), 2);
        assert_eq!(prossimo(&ordine, 4, true), 6, "la carta senza azione salta il secondo passo");
        assert_eq!(prossimo(&ordine, 6, false), 4);
        assert_eq!(prossimo(&ordine, 320, true), 0, "da «⋯» si torna in cima");
        assert_eq!(prossimo(&ordine, 0, false), 320, "dalla prima indietro: «⋯»");
        assert_eq!(prossimo(&ordine, -1, true), 0, "nessun fuoco: il primo");
        assert_eq!(prossimo(&ordine, -1, false), 320, "nessun fuoco indietro: l'ultimo");
        assert_eq!(prossimo(&ordine, 100, true), 0, "la voce sparita: il primo");
        assert_eq!(prossimo(&[], 5, true), 5);
    }

    #[test]
    fn in_attesa_non_mostrate_dalla_riga_alta() {
        assert_eq!(in_attesa_non_mostrate(3, false), 3);
        assert_eq!(in_attesa_non_mostrate(0, false), 0);
        assert_eq!(in_attesa_non_mostrate(3, true), 0, "con la voce le altre le conta «+N»");
        assert_eq!(in_attesa_non_mostrate(1, true), 0);
    }
}
