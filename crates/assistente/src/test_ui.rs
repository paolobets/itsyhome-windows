//! Controlli sull'interfaccia (`.slint`) che girano in `cargo test`, senza finestre: i sorgenti
//! (colori a mano) e i componenti creati col backend di prova di Slint (misure, accessibilità).

use crate::{finestre, layout, sistema, Pannello, Pillola};
use std::cell::Cell;
use std::path::{Path, PathBuf};

thread_local! {
    static PIATTAFORMA: Cell<bool> = const { Cell::new(false) };
}

/// Il backend di prova di Slint per questo thread (ogni test ha il suo): nessuna finestra, nessun
/// ciclo di eventi, il tempo delle animazioni è finto.
pub fn piattaforma_di_prova() {
    if !PIATTAFORMA.with(|p| p.replace(true)) {
        i_slint_backend_testing::init_no_event_loop();
    }
}

/// Tutti i file `.slint` dell'assistente e della cornice della gestione.
fn file_slint() -> Vec<PathBuf> {
    fn raccogli(cartella: &Path, file: &mut Vec<PathBuf>) {
        for voce in std::fs::read_dir(cartella).expect("cartella dell'interfaccia leggibile") {
            let percorso = voce.expect("voce leggibile").path();
            if percorso.is_dir() {
                raccogli(&percorso, file);
            } else if percorso.extension().is_some_and(|e| e == "slint") {
                file.push(percorso);
            }
        }
    }
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut file = Vec::new();
    raccogli(&crate_dir.join("ui"), &mut file);
    raccogli(&crate_dir.join("../gestione/ui"), &mut file);
    assert!(file.len() >= 10, "trovati solo {} file .slint", file.len());
    file
}

/// Il codice di una riga senza il commento `//` (le stringhe dell'interfaccia non contengono `//`).
fn senza_commento(riga: &str) -> &str {
    riga.split("//").next().unwrap_or("")
}

/// Un letterale di colore nel codice: `#` seguito da 3–8 cifre esadecimali, o `rgb(` / `rgba(`.
fn colore_a_mano(codice: &str) -> bool {
    if codice.contains("rgb(") || codice.contains("rgba(") {
        return true;
    }
    codice.match_indices('#').any(|(i, _)| {
        let cifre = codice[i + 1..].chars().take_while(char::is_ascii_hexdigit).count();
        (3..=8).contains(&cifre)
    })
}

#[test]
fn riconosce_i_colori_a_mano() {
    assert!(colore_a_mano("background: #fb923c;"));
    assert!(colore_a_mano("color: rgba(255, 255, 255, 0.04);"));
    assert!(!colore_a_mano("background: Tema.card-fondo;"));
    assert!(!colore_a_mano("text: \"+\" + altre;"));
}

#[test]
fn ui_senza_colori_a_mano() {
    // FE7: i colori stanno solo in tema.slint, come token di ruolo.
    let mut trovati = Vec::new();
    for percorso in file_slint() {
        if percorso.file_name().is_some_and(|n| n == "tema.slint") {
            continue;
        }
        let testo = std::fs::read_to_string(&percorso).expect("file .slint leggibile");
        for (n, riga) in testo.lines().enumerate() {
            if colore_a_mano(senza_commento(riga)) {
                trovati.push(format!("{}:{}: {}", percorso.display(), n + 1, riga.trim()));
            }
        }
    }
    assert!(trovati.is_empty(), "colori scritti a mano fuori da tema.slint:\n{}", trovati.join("\n"));
}

#[test]
fn misure_slint_e_rust() {
    // FE10: le misure che Rust usa per posizione, ritaglio e scarti degli occhi coincidono con
    // quelle di faccia.slint, pillola.slint e pannello.slint (prima: solo debug_assert all'avvio).
    piattaforma_di_prova();
    let pillola = Pillola::new().expect("pillola creata col backend di prova");
    assert_eq!(pillola.get_margine_aura(), finestre::MARGINE_AURA, "margine dell'aura");
    assert_eq!(
        (pillola.get_larghezza_riposo(), pillola.get_altezza_riposo()),
        (finestre::LARGHEZZA_PILLOLA, finestre::ALTEZZA_PILLOLA),
        "pillola a riposo"
    );
    assert_eq!(
        (pillola.get_larghezza_attenzione(), pillola.get_altezza_attenzione()),
        (finestre::LARGHEZZA_ATTENZIONE, finestre::ALTEZZA_ATTENZIONE),
        "pillola che chiama"
    );
    assert_eq!(
        (pillola.get_larghezza_bloccante(), pillola.get_altezza_bloccante()),
        (finestre::LARGHEZZA_BLOCCANTE, finestre::ALTEZZA_BLOCCANTE),
        "pillola bloccante"
    );
    assert_eq!(
        (pillola.get_larghezza_finestra(), pillola.get_altezza_finestra()),
        (finestre::LARGHEZZA_FINESTRA_PILLOLA, finestre::ALTEZZA_FINESTRA_PILLOLA),
        "finestra della pillola"
    );
    assert_eq!(
        (pillola.get_scarto_occhi_attenzione(), pillola.get_scarto_occhi_attenzione_y()),
        finestre::SCARTO_OCCHI_ATTENZIONE,
        "occhi della pillola che chiama"
    );
    assert_eq!(
        (pillola.get_scarto_occhi_bloccante(), pillola.get_scarto_occhi_bloccante_y()),
        finestre::SCARTO_OCCHI_BLOCCANTE,
        "occhi della pillola bloccante"
    );
    assert_eq!(
        (pillola.get_larghezza_linea(), pillola.get_altezza_linea()),
        (sistema::ritirata::LARGHEZZA_LINEA, sistema::ritirata::ALTEZZA_LINEA),
        "linea della ritirata"
    );

    let pannello = Pannello::new().expect("pannello creato col backend di prova");
    assert_eq!(pannello.get_larghezza_pannello(), finestre::LARGHEZZA_PANNELLO, "larghezza del pannello");
    assert_eq!(pannello.get_altezza_riga(), finestre::ALTEZZA_RIGA_PANNELLO, "riga alta del pannello");
    assert_eq!(pannello.get_numero_layout() as usize, layout::TUTTI.len(), "layout della Panoramica");
    drop((pillola, pannello));
}
