//! Controlli sull'interfaccia (`.slint`) che girano in `cargo test`, senza finestre: i sorgenti
//! (colori a mano) e i componenti creati col backend di prova di Slint (misure, accessibilità).

use crate::{finestre, sistema, Pannello, Pillola};
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

/// Come `piattaforma_di_prova`, ma le finestre disegnano davvero (skia senza schermo): per i test
/// che controllano i colori disegnati (`Window::take_snapshot`). Va chiamata per prima nel test:
/// il backend vale per tutto il thread.
pub fn piattaforma_che_disegna() {
    assert!(!PIATTAFORMA.with(|p| p.replace(true)), "il backend di questo thread c'è già");
    i_slint_core::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            renderer_name: Some("skia".into()),
            ..Default::default()
        },
    )))
    .expect("backend che disegna");
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

/// Il codice di una riga senza il commento `//`, tagliato solo fuori dalle stringhe (un URL in un
/// `text:` non è un commento).
fn senza_commento(riga: &str) -> &str {
    let mut in_stringa = false;
    let mut prima = ' ';
    for (i, c) in riga.char_indices() {
        if c == '"' && prima != '\\' {
            in_stringa = !in_stringa;
        } else if !in_stringa && c == '/' && prima == '/' {
            return &riga[..i - 1];
        }
        prima = c;
    }
    riga
}

/// Un colore scritto a mano nel codice: `#` seguito da 3–8 cifre esadecimali, `rgb(` / `rgba(`,
/// `hsv(` o i colori per nome di Slint (`Colors.red`).
fn colore_a_mano(codice: &str) -> bool {
    if ["rgb(", "rgba(", "hsv(", "Colors."].iter().any(|m| codice.contains(m)) {
        return true;
    }
    codice.match_indices('#').any(|(i, _)| {
        let cifre = codice[i + 1..].chars().take_while(char::is_ascii_hexdigit).count();
        (3..=8).contains(&cifre)
    })
}

/// I pezzi fra virgolette di una riga di codice (Slint o Rust), senza il commento `//`. Capisce
/// `"\\"` (la barra rovesciata è un carattere, non protegge la virgoletta dopo) e i caratteri
/// `'"'` / `'\"'`. Limiti noti: non vede le stringhe su più righe (`\` a capo) né le raw string.
fn stringhe_della_riga(riga: &str) -> Vec<String> {
    let c: Vec<char> = riga.chars().collect();
    let mut trovate = Vec::new();
    let mut corrente: Option<String> = None;
    let mut i = 0;
    while i < c.len() {
        if let Some(s) = corrente.as_mut() {
            if c[i] == '\\' {
                s.push('\\');
                if let Some(&dopo) = c.get(i + 1) {
                    s.push(dopo);
                }
                i += 2;
                continue;
            }
            if c[i] == '"' {
                trovate.extend(corrente.take());
            } else {
                s.push(c[i]);
            }
            i += 1;
            continue;
        }
        match c[i] {
            '"' => corrente = Some(String::new()),
            '/' if c.get(i + 1) == Some(&'/') => break,
            '\'' if c.get(i + 1) == Some(&'"') && c.get(i + 2) == Some(&'\'') => i += 2,
            '\'' if c.get(i + 1) == Some(&'\\') && c.get(i + 3) == Some(&'\'') => i += 3,
            _ => {}
        }
        i += 1;
    }
    trovate
}

/// Il sorgente Rust senza il blocco di test in fondo (`#[cfg(test)]` subito seguito da
/// `mod nome {`, anche con la visibilità: `pub(crate) mod test {`). Un `#[cfg(test)]` a metà file
/// davanti a una funzione, a un `use` o a `mod x;` (un file a parte) non lo chiude: il resto del
/// file, dove stanno i testi per l'utente, si controlla.
fn senza_modulo_di_test(testo: &str) -> String {
    let apre_un_modulo = |riga: &str| {
        let riga = riga.trim();
        let senza_visibilita = match riga.strip_prefix("pub") {
            Some(resto) if resto.starts_with('(') => resto.split_once(')').map_or(resto, |(_, dopo)| dopo),
            Some(resto) => resto,
            None => riga,
        };
        senza_visibilita.trim_start().strip_prefix("mod ").is_some_and(|nome| nome.trim_end().ends_with('{'))
    };
    let righe: Vec<&str> = testo.lines().collect();
    let fine = righe
        .windows(2)
        .position(|w| w[0].trim() == "#[cfg(test)]" && apre_un_modulo(w[1]))
        .unwrap_or(righe.len());
    righe[..fine].join("\n")
}

/// Le stringhe per l'utente che nominano un «ambiente»: nei `.slint` ogni stringa (testi ed etichette
/// per Narrator); nei sorgenti Rust dell'assistente quelle fuori dai test (il
/// blocco `#[cfg(test)] mod …` in fondo e `test_ui.rs` non contano). Fanno eccezione solo i due nomi interni elencati.
fn stringhe_con_ambiente() -> Vec<String> {
    // Non arrivano all'utente: la base dell'id di una casa senza nome e il nome del tipo nel Debug.
    const INTERNE: [&str; 2] = ["ambiente", "Ambiente"];
    let mut trovate = Vec::new();
    let cerca = |origine: &str, testo: &str, trovate: &mut Vec<String>| {
        for (n, riga) in testo.lines().enumerate() {
            for s in stringhe_della_riga(riga) {
                if s.to_lowercase().contains("ambient") && !INTERNE.contains(&s.as_str()) {
                    trovate.push(format!("{origine}:{}: «{s}»", n + 1));
                }
            }
        }
    };
    for percorso in file_slint() {
        let testo = std::fs::read_to_string(&percorso).expect("file .slint leggibile");
        cerca(&percorso.display().to_string(), &testo, &mut trovate);
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust = Vec::new();
    let mut cartelle = vec![src];
    while let Some(c) = cartelle.pop() {
        for voce in std::fs::read_dir(&c).expect("cartella dei sorgenti leggibile") {
            let p = voce.expect("voce leggibile").path();
            if p.is_dir() {
                cartelle.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") && p.file_name().is_some_and(|n| n != "test_ui.rs") {
                rust.push(p);
            }
        }
    }
    for percorso in rust {
        let testo = std::fs::read_to_string(&percorso).expect("sorgente leggibile");
        cerca(&percorso.display().to_string(), &senza_modulo_di_test(&testo), &mut trovate);
    }
    trovate
}

#[test]
fn riconosce_le_stringhe_con_ambiente() {
    assert_eq!(stringhe_della_riga(r#"text: "Nessun ambiente"; // un ambiente"#), ["Nessun ambiente"]);
    assert_eq!(stringhe_della_riga(r#"a("uno", "due \" tre")"#).len(), 2);
    assert!(stringhe_della_riga("let ambiente = 1; // «ambiente»").is_empty());
    // Il carattere virgoletta e la barra rovesciata non scombinano il resto della riga.
    assert_eq!(stringhe_della_riga(r#"['"', 'x'] + "Ambiente rimosso""#), ["Ambiente rimosso"]);
    assert_eq!(stringhe_della_riga(r#"['\"'] + "ambiente""#), ["ambiente"]);
    assert_eq!(stringhe_della_riga(r#"a("\\") + "ambiente""#), ["\\\\", "ambiente"]);
    assert_eq!(stringhe_della_riga(r#"url("https://x.it") // "ambiente""#), ["https://x.it"]);
}

#[test]
fn il_blocco_di_test_si_taglia_solo_in_fondo() {
    let testo = "fn a() { \"uno\" }\n#[cfg(test)]\nfn solo_prova() {}\nfn b() { \"Ambiente rimosso\" }\n\n#[cfg(test)]\nmod test {\n    \"ambiente\"\n}\n";
    let tenuto = senza_modulo_di_test(testo);
    assert!(tenuto.contains("Ambiente rimosso"), "dopo un #[cfg(test)] a metà file si guarda ancora");
    assert!(!tenuto.contains("\"ambiente\""), "il modulo di test in fondo si salta");
    assert_eq!(senza_modulo_di_test("fn a() {}\r\nfn b() {}\r\n"), "fn a() {}\nfn b() {}", "senza modulo di test");
    // N1 (revisione finale): `#[cfg(test)] mod x;` dichiara un file a parte, non chiude il sorgente.
    let testo = "#[cfg(test)]\nmod test_ui;\nfn c() { \"Ambiente dopo\" }\n#[cfg(test)]\nmod test {\n    \"ambiente\"\n}\n";
    let tenuto = senza_modulo_di_test(testo);
    assert!(tenuto.contains("Ambiente dopo"), "dopo `mod x;` si guarda ancora");
    assert!(!tenuto.contains("\"ambiente\""), "il blocco di test in fondo si salta");
    // N2: il modulo di test con la visibilità (`pub(crate) mod test {`) si riconosce.
    for testa in ["pub(crate) mod test {", "pub mod test {", "pub(super) mod test{"] {
        let tenuto = senza_modulo_di_test(&format!("fn d() {{}}\n#[cfg(test)]\n{testa}\n    \"ambiente\"\n}}\n"));
        assert_eq!(tenuto, "fn d() {}", "«{testa}»");
    }
}

#[test]
fn nessun_testo_per_l_utente_dice_ambiente() {
    // R1-21: per l'utente la casa si chiama «Casa»; «ambiente» resta solo negli identificativi.
    let trovate = stringhe_con_ambiente();
    assert!(trovate.is_empty(), "la parola «ambiente» arriva all'utente:\n{}", trovate.join("\n"));
}

/// Luminanza relativa WCAG di un colore `#rrggbb`.
fn luminanza(esadecimale: &str) -> f64 {
    let c = |i: usize| {
        let v = f64::from(u8::from_str_radix(&esadecimale[1 + 2 * i..3 + 2 * i], 16).expect("cifre esadecimali")) / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * c(0) + 0.7152 * c(1) + 0.0722 * c(2)
}

fn contrasto(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminanza(a), luminanza(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Il sorgente di `tema.slint`.
fn tema_slint() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/tema.slint")).expect("tema.slint leggibile")
}

/// Il colore `#rrggbb` di un token di `Tema`: se il token è definito come un altro token
/// (`attivo: caldo-2`) si segue l'alias fino al letterale.
fn colore_del_token(tema: &str, nome: &str) -> String {
    let cerca = format!("out property <color> {nome}:");
    let i = tema.find(&cerca).unwrap_or_else(|| panic!("token {nome} non trovato")) + cerca.len();
    let valore = tema[i..].split(';').next().expect("fine del token").trim();
    if valore.starts_with('#') && valore.len() == 7 {
        valore.to_string()
    } else if valore.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        colore_del_token(tema, valore)
    } else {
        panic!("il token {nome} vale «{valore}»: né un colore #rrggbb né l'alias di un altro token");
    }
}

#[test]
fn iniziali_dei_collegamenti_leggibili_sul_loro_cerchio() {
    // R1-23: l'iniziale (Tema.nero, scura) sul cerchio (tavolozza di `colore-collegamento` in
    // tema.slint) deve avere almeno 4,5:1 (WCAG AA, testo normale), per ogni colore.
    let tema = tema_slint();
    let colore = |nome: &str| colore_del_token(&tema, nome);
    let inizio = tema.find("function colore-collegamento").expect("funzione colore-collegamento");
    let corpo = &tema[inizio..];
    let corpo = &corpo[..corpo.find("\n    }").expect("fine della funzione")];
    let toni: Vec<&str> = corpo.split("return ").skip(1).map(|r| r.split(';').next().unwrap().trim()).collect();
    assert_eq!(toni.len(), crate::collegamenti::TONI, "un colore per ogni tono");
    let lettera = colore("nero");
    for t in toni {
        let sfondo = colore(t);
        let c = contrasto(&lettera, &sfondo);
        assert!(c >= 4.5, "{t} ({sfondo}) con la lettera {lettera}: {c:.2}:1");
    }
}

#[test]
fn contrasti_dei_token() {
    // Ridisegno F1, Task 1: parola ≥ 4,5:1, segno e bordo di controllo ≥ 3:1, sui fondi veri
    // (nero della finestra, g1 carta, g2 riga scelta).
    let tema = tema_slint();
    let colore = |nome: &str| colore_del_token(&tema, nome);
    let mut sotto = Vec::new();
    let mut verifica = |sopra: &str, fondo: &str, minimo: f64| {
        let c = contrasto(&colore(sopra), &colore(fondo));
        if c < minimo {
            sotto.push(format!("{sopra} su {fondo}: {c:.2}:1 (minimo {minimo}:1)"));
        }
    };
    for testo in ["testo-2", "testo-tenue"] {
        for fondo in ["nero", "g1", "g2"] {
            verifica(testo, fondo, 4.5);
        }
    }
    for tono in ["attivo", "aspetta", "guarda", "fatto", "errore"] {
        for fondo in ["nero", "g1"] {
            verifica(tono, fondo, 4.5);
        }
    }
    verifica("bordo-ctl", "g1", 3.0);
    verifica("testo-su-primario", "primario", 4.5);
    assert!(sotto.is_empty(), "contrasti sotto soglia:\n{}", sotto.join("\n"));
}

#[test]
fn menu_fondo_passaggio_scelta_in_ordine_crescente() {
    // Fix round 1: sul fondo del menu «⋯» la riga sotto il mouse e quella evidenziata devono
    // staccarsi, e in quest'ordine: fondo < passaggio < scelta (luminanza), con le righe del
    // menu che usano proprio quei token.
    let tema = tema_slint();
    let l = |nome: &str| luminanza(&colore_del_token(&tema, nome));
    let (fondo, passaggio, scelta) = (l("menu-fondo"), l("menu-sopra"), l("menu-scelta"));
    assert!(fondo < passaggio && passaggio < scelta, "ordine: fondo {fondo:.4}, passaggio {passaggio:.4}, scelta {scelta:.4}");
    let menu = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/menu.slint")).expect("menu.slint leggibile");
    assert!(!menu.contains("Tema.scheda-attiva") && !menu.contains("Tema.scheda-sopra"), "le righe del menu usano menu-sopra e menu-scelta");
}

/// Il corpo di un componente Slint (`component {nome} …` fino al componente seguente) nel file
/// `ui/{file}` dell'assistente.
fn componente_slint(file: &str, nome: &str) -> String {
    let testo = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(file)).expect("file leggibile");
    let inizio = testo.find(&format!("component {nome} ")).unwrap_or_else(|| panic!("componente {nome} non trovato in {file}"));
    let resto = &testo[inizio + 1..];
    let fine = ["\ncomponent ", "\nexport component "].iter().filter_map(|m| resto.find(m)).min().unwrap_or(resto.len());
    resto[..fine].to_string()
}

#[test]
fn contrasti_delle_superfici_vecchie() {
    // Ridisegno F1, Task 10: le coppie testo/fondo delle pagine di oggi (Casa, Agenti, PC,
    // Collegamenti), della pillola, delle Impostazioni e della gestione sulla base nera. Parola
    // ≥ 4,5:1, segno e bordo ≥ 3:1; una superficie sopra un'altra si stacca di luminanza.
    let tema = tema_slint();
    let colore = |nome: &str| colore_del_token(&tema, nome);
    let mut sotto = Vec::new();
    let mut verifica = |sopra: &str, fondo: &str, minimo: f64| {
        let c = contrasto(&colore(sopra), &colore(fondo));
        if c < minimo {
            sotto.push(format!("{sopra} su {fondo}: {c:.2}:1 (minimo {minimo}:1)"));
        }
    };
    // Testi: carte e righe (g1), righe sotto il mouse (g2), sovrapposizioni e suggerimenti.
    for testo in ["testo", "testo-2", "testo-tenue"] {
        for fondo in ["nero", "card-fondo", "scheda-sopra", "menu-fondo"] {
            verifica(testo, fondo, 4.5);
        }
    }
    verifica("segnaposto", "riquadro-fondo", 4.5);
    verifica("testo", "suggerimento", 4.5);
    verifica("testo", "menu-sopra", 4.5);
    verifica("testo", "menu-scelta", 4.5);
    // Pulsanti: grigio (neutro) col testo bianco, bianco (azione) col testo nero.
    for fondo in ["neutro-fondo", "neutro-sopra", "scheda-attiva"] {
        verifica("testo", fondo, 4.5);
    }
    for fondo in ["azione-fondo", "azione-sopra"] {
        verifica("testo-su-primario", fondo, 4.5);
    }
    // Lo stato della riga di una sessione e i livelli (pillola, Coda, Impostazioni), anche sulla
    // riga sotto il mouse.
    for tono in ["attivo", "bloccante", "attenzione", "informazione", "errore", "caldo-1", "caldo-2"] {
        for fondo in ["nero", "card-fondo", "scheda-sopra"] {
            verifica(tono, fondo, 4.5);
        }
    }
    // Segni e bordi: bordo dei campi, icona spenta, interruttore acceso.
    for fondo in ["card-fondo", "scheda-sopra"] {
        verifica("bordo-ctl", fondo, 3.0);
        verifica("icona-spenta", fondo, 3.0);
        verifica("interruttore-acceso", fondo, 3.0);
    }
    verifica("icona-spenta", "nero", 3.0);
    // Barre e archi: il pieno si legge sulla traccia e sulla carta (la traccia, come nelle bozze, è
    // un grigio appena sopra la carta: il valore lo dice il pieno, e la parola accanto).
    for pieno in ["petalo-2", "caldo-2", "attenzione", "fatto", "testo", "batteria-carica"] {
        for fondo in ["traccia", "card-fondo", "nero"] {
            verifica(pieno, fondo, 3.0);
        }
    }
    // L'anello di Windows: bianco sul nero e sulle carte, il filo nero esterno sul primario bianco.
    verifica("anello", "nero", 3.0);
    verifica("anello", "card-fondo", 3.0);
    verifica("anello-esterno", "primario", 3.0);
    assert!(sotto.is_empty(), "contrasti sotto soglia:\n{}", sotto.join("\n"));

    // Una superficie sopra un'altra se ne stacca (non solo col filo): le sovrapposizioni (menu,
    // elenco a comparsa, scelta dell'icona, conferma) sopra le righe g1; il suggerimento sopra la
    // riga sotto il mouse.
    let l = |nome: &str| luminanza(&colore(nome));
    assert!(l("menu-fondo") > l("card-fondo"), "sovrapposizioni più chiare delle righe");
    assert!(l("suggerimento") > l("scheda-sopra"), "il suggerimento sopra la riga sotto il mouse si stacca");
    // Il fondo del menu è un grigio scritto a sé, non l'alias del suggerimento (Task 1).
    let riga = tema.lines().find(|r| r.contains("out property <color> menu-fondo:")).expect("token menu-fondo");
    assert!(!riga.contains("suggerimento"), "menu-fondo alias del suggerimento: {riga}");
}

#[test]
fn sovrapposizioni_staccate_dalle_righe() {
    // Task 1, minore rimandato: la scelta dell'icona, la conferma dell'eliminazione e l'elenco
    // del selettore stanno sopra righe g1; il loro fondo è quello dei menu (g2) con l'ombra, e la
    // cella sotto il mouse e quella evidenziata usano i gradini del menu (si vedono sul g2).
    for (file, nome) in [("impostazioni.slint", "SceltaIcona"), ("impostazioni.slint", "ConfermaElimina"), ("componenti.slint", "Scelta")] {
        let corpo = componente_slint(file, nome);
        assert!(corpo.contains("background: Tema.menu-fondo"), "{nome}: il fondo non è menu-fondo");
        assert!(corpo.contains("drop-shadow-color: Tema.menu-ombra"), "{nome}: senza ombra");
        assert!(!corpo.contains("background: Tema.g1"), "{nome}: ancora g1 sopra le righe g1");
        assert!(!corpo.contains("Tema.scheda-sopra"), "{nome}: il passaggio g2 sparisce sul fondo g2");
    }
}

#[test]
fn bordo_dei_campi_come_i_controlli() {
    // Task 10: la casella di un campo delle Impostazioni sta su una card g1; il suo bordo è quello
    // dei controlli (≥ 3:1 sulla carta), non il filo fra le superfici (≈ 1,2:1).
    let campo = componente_slint("impostazioni.slint", "Campo");
    assert!(campo.contains("Tema.caldo-2 : Tema.bordo-ctl"), "il bordo della casella non è bordo-ctl");
}

/// Il valore di `text:` di ogni `Etichetta { … }` (mono maiuscolo) nel sorgente, fino al `;`.
fn testi_delle_etichette(sorgente: &str) -> Vec<String> {
    let mut testi = Vec::new();
    let mut resto = sorgente;
    while let Some(i) = resto.find("Etichetta {") {
        let dopo = &resto[i..];
        if let Some(t) = dopo.find("text:") {
            // Solo il `text:` di questa etichetta: prima della `}` che la chiude.
            if dopo.find('}').is_none_or(|c| t < c) {
                let valore = &dopo[t + 5..];
                testi.push(valore[..valore.find(';').unwrap_or(valore.len())].trim().to_string());
            }
        }
        resto = &dopo[1..];
    }
    testi
}

#[test]
fn riconosce_i_testi_delle_etichette() {
    let s = "a := Etichetta {\n  text: \"CPU\";\n}\nEtichetta { x: 1; }\nText { text: \"no\"; }\nEtichetta {\n text: a\n   ? b : c;\n}";
    assert_eq!(testi_delle_etichette(s), ["\"CPU\"", "a\n   ? b : c"]);
}

#[test]
fn etichette_mono_in_maiuscolo() {
    // Ridisegno F1 (spec §13.10): il mono è sempre maiuscolo con tracking. Un'etichetta col testo
    // scritto è già maiuscola; con un testo che arriva da fuori (nome di una stanza, colonna della
    // gestione) passa da `to-uppercase()`.
    let mut sbagliate = Vec::new();
    for file in file_slint() {
        let sorgente = std::fs::read_to_string(&file).expect("leggibile");
        for testo in testi_delle_etichette(&sorgente) {
            let maiuscolo = |t: &str| {
                let t = t.trim();
                t.starts_with('"') && t.ends_with('"') && t.matches('"').count() == 2 && t == t.to_uppercase()
            };
            // Un testo scritto, o una scelta fra testi scritti (`a ? "UNO" : "DUE"`), già maiuscoli.
            let scritti = testo.rsplit_once('?').map_or(testo.as_str(), |(_, rami)| rami).split(':').all(maiuscolo);
            let buono = scritti || testo.contains("to-uppercase()");
            if !buono {
                sbagliate.push(format!("{}: {testo}", file.file_name().unwrap().to_string_lossy()));
            }
        }
    }
    assert!(sbagliate.is_empty(), "etichette mono non maiuscole:\n{}", sbagliate.join("\n"));
}

#[test]
fn caratteri_incorporati() {
    // Ridisegno F1, Task 1: Inter 500/700 e JetBrains Mono NL 500, e nient'altro.
    let tema = tema_slint();
    let importati: Vec<&str> = tema
        .lines()
        .map(senza_commento)
        .filter_map(|r| r.trim().strip_prefix("import \"").and_then(|r| r.strip_suffix("\";")))
        .filter(|p| p.ends_with(".ttf"))
        .collect();
    assert_eq!(
        importati,
        ["../assets/font/Inter-Medium.ttf", "../assets/font/Inter-Bold.ttf", "../assets/font/JetBrainsMonoNL-Medium.ttf"],
        "i caratteri che tema.slint incorpora"
    );

    let mut vecchi = Vec::new();
    for percorso in file_slint() {
        let testo = std::fs::read_to_string(&percorso).expect("file .slint leggibile");
        for (n, riga) in testo.lines().enumerate() {
            // Pesi di prima: 400 e 600 non esistono più (500 e 700), anche dentro un ternario.
            let peso_vecchio = senza_commento(riga).split_once("font-weight:").is_some_and(|(_, dopo)| {
                dopo.split(|c: char| !c.is_ascii_digit()).any(|p| p == "400" || p == "600")
            });
            if riga.contains("Inter Tight") || peso_vecchio {
                vecchi.push(format!("{}:{}: {}", percorso.display(), n + 1, riga.trim()));
            }
        }
    }
    assert!(vecchi.is_empty(), "caratteri o pesi di prima:\n{}", vecchi.join("\n"));

    let cartella = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/font");
    let mut nomi: Vec<String> = std::fs::read_dir(&cartella)
        .expect("cartella dei caratteri leggibile")
        .map(|v| v.expect("voce leggibile").file_name().to_string_lossy().into_owned())
        .collect();
    nomi.sort();
    assert_eq!(
        nomi,
        ["Inter-Bold.ttf", "Inter-Medium.ttf", "JetBrainsMonoNL-Medium.ttf", "OFL-Inter.txt", "OFL-JetBrainsMono.txt"],
        "assets/font: tre caratteri e le due licenze, nient'altro"
    );
}

#[test]
fn finestre_col_peso_500() {
    // Ridisegno F1, Task 1: ogni finestra parte dal peso 500 (il «normale» del ridisegno); il 700
    // si chiede dove serve. Letto dal sorgente: una `Window` senza `default-font-weight: 500`
    // userebbe il 400, che non è incorporato.
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let finestre = [
        ("Pannello", ui.join("pannello.slint")),
        ("Pillola", ui.join("pillola.slint")),
        ("FinestraImpostazioni", ui.join("impostazioni.slint")),
        ("Cornice", ui.join("../../gestione/ui/cornice.slint")),
    ];
    for (nome, file) in finestre {
        let testo = std::fs::read_to_string(&file).expect("file .slint leggibile");
        let inizio = testo
            .find(&format!("export component {nome} inherits Window {{"))
            .unwrap_or_else(|| panic!("finestra {nome} non trovata"));
        let componente = &testo[inizio..];
        let componente = &componente[..componente.find("\n}\n").unwrap_or(componente.len())];
        assert!(
            componente.lines().map(senza_commento).any(|r| r.trim() == "default-font-weight: 500;"),
            "{nome} senza default-font-weight: 500"
        );
    }
    let tema = tema_slint();
    assert!(tema.contains("out property <string> carattere: \"Inter\";"), "carattere dell'interfaccia: Inter");
    assert!(
        tema.contains("out property <string> carattere-mono: \"JetBrains Mono NL\";"),
        "carattere dei numeri e delle etichette: JetBrains Mono NL"
    );
}

#[test]
fn riconosce_i_colori_a_mano() {
    assert!(colore_a_mano("background: #fb923c;"));
    assert!(colore_a_mano("color: rgba(255, 255, 255, 0.04);"));
    assert!(colore_a_mano("color: Colors.red;"));
    assert!(colore_a_mano("color: Colors.hsv(120, 0.5, 0.5);"));
    assert!(!colore_a_mano("background: Tema.card-fondo;"));
    assert!(!colore_a_mano("text: \"+\" + altre;"));
    // Il commento si toglie solo fuori dalle stringhe.
    assert!(colore_a_mano(senza_commento("text: \"https://slint.dev\"; color: #fff;")));
    assert!(!colore_a_mano(senza_commento("color: Tema.testo; // era #fff")));
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

    // Ridisegno F1: finestra 820 × 340 (isola 800 più le orecchie), riga alta 44.
    let pannello = Pannello::new().expect("pannello creato col backend di prova");
    assert_eq!(pannello.get_larghezza_finestra(), finestre::LARGHEZZA_FINESTRA_PANNELLO, "finestra del pannello");
    assert_eq!(pannello.get_larghezza_finestra(), 820.0);
    assert_eq!(pannello.get_larghezza_isola(), finestre::LARGHEZZA_PANNELLO, "isola del pannello");
    assert_eq!(pannello.get_altezza_pannello(), finestre::ALTEZZA_PANNELLO, "altezza del pannello");
    assert_eq!(pannello.get_altezza_pannello(), 340.0);
    assert_eq!(pannello.get_altezza_riga(), finestre::ALTEZZA_RIGA_PANNELLO, "riga alta del pannello");
    assert_eq!(pannello.get_altezza_riga(), 44.0);
    // Il corpo: 752 × 224 a (34, 44), tutto per la vista (Task 10).
    assert_eq!((pannello.get_centro_x(), pannello.get_centro_y(), pannello.get_centro_l()), (34.0, 44.0, 752.0));
    assert_eq!(pannello.get_centro_a(), 224.0);
    drop((pillola, pannello));
}

#[test]
fn misura_rust_come_slint() {
    // La misura dei testi in Rust (somma degli avanzamenti) sta entro `MARGINE` dalla resa di Slint.
    use crate::moduli::misura::{larghezza, Stile, MARGINE};
    piattaforma_di_prova();
    let misura = crate::MisuraTesto::new().expect("misura creata col backend di prova");
    let stili = [Stile::Titolo, Stile::TitoloNumerico, Stile::Riga, Stile::Etichetta, Stile::Capsula];
    let testi = [
        "Tutto regolare.",
        "4 luci accese",
        "AGENTI",
        "Al lavoro",
        "88:88:88",
        "Collega Claude",
        "7 giorni 61 %",
        "Perché è già così? Àèìòù",
    ];
    for (indice, stile) in stili.into_iter().enumerate() {
        for testo in testi {
            misura.set_testo(testo.into());
            misura.set_stile(indice as i32);
            let slint = misura.get_larghezza();
            let rust = larghezza(testo, stile);
            assert!(
                (rust - slint).abs() <= MARGINE,
                "«{testo}» {stile:?}: Rust {rust}, Slint {slint}"
            );
        }
    }
}

#[test]
fn linea_riempita_a_meta() {
    // M2 Task 5: a pillola ritirata con riempimento 0.5 il riempimento è largo metà linea (150 px);
    // senza avanzamento (1.0) è tutta la linea, come in M1.
    piattaforma_di_prova();
    let pillola = Pillola::new().expect("pillola creata col backend di prova");
    pillola.set_ritirata(true);
    assert_eq!(pillola.get_larghezza_riempimento(), sistema::ritirata::LARGHEZZA_LINEA, "linea piena di partenza");
    pillola.set_riempimento_linea(0.5);
    assert_eq!(pillola.get_larghezza_riempimento(), 150.0, "riempimento a metà");
    pillola.set_riempimento_linea(0.0);
    assert_eq!(pillola.get_larghezza_riempimento(), 0.0, "riempimento vuoto");
}

mod accessibilita {
    //! FE13/FE16: ruoli, nomi e stati che Narrator legge (UI Automation li riceve da AccessKit).
    use super::piattaforma_di_prova;
    use crate::{
        AspettoPillolaUi, FinestraImpostazioni, LivelloUi, Pannello, Pillola, PulsanteUi, SchedaBarraUi, StatPcUi,
        VoceAttesaUi,
    };
    use i_slint_backend_testing::{AccessibleRole, ElementHandle};
    use slint::{ComponentHandle, Model, ModelRc, VecModel};

    fn elemento(radice: &impl i_slint_backend_testing::ElementRoot, nome: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(radice, nome)
            .next()
            .unwrap_or_else(|| panic!("nessun elemento accessibile «{nome}»"))
    }

    /// Un pannello aperto nel backend di prova (finestra finta): le parti create con `if` e `for`
    /// esistono solo in una finestra mostrata.
    fn pannello() -> Pannello {
        let p = Pannello::new().expect("pannello");
        // Tab nella Panoramica: l'ordine lo calcola Rust, come nel pannello vero.
        crate::pannello::collega_fuoco(&p);
        // Dove sta una voce fra le attese, come lo dice il ponte (`indice_voce` sul modello).
        let debole = p.as_weak();
        p.on_indice_voce(move |id| debole.upgrade().map_or(-1, |p| crate::pannello::indice_voce(&p.get_attese(), &id)));
        p.set_aperto(true);
        p.show().expect("pannello mostrato nel backend di prova");
        p
    }

    /// Le schede della barra di un registro nuovo (Panoramica, Agenti, Casa, PC, Collegamenti), con
    /// la vista `vista` aperta.
    fn schede_di(vista: &str) -> ModelRc<SchedaBarraUi> {
        let registro = crate::moduli::registro::Registro::nuovo(&[]);
        ModelRc::new(VecModel::from(crate::pannello::schede_ui(&registro, vista)))
    }

    fn schede() -> ModelRc<SchedaBarraUi> {
        schede_di("panoramica")
    }

    /// Ridisegno F1, Task 3: l'ordine del registro mette Agenti per primo dopo la Panoramica; i
    /// nomi per Narrator dicono la posizione («scheda 2 di 5»).
    #[test]
    fn dock_e_puntini_sono_schede_con_lo_stato() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        let panoramica = elemento(&p, "Panoramica, scheda 1 di 5");
        assert_eq!(panoramica.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!(panoramica.accessible_item_selected(), Some(true));
        let agenti = elemento(&p, "Agenti, scheda 2 di 5");
        assert_eq!(agenti.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!(agenti.accessible_item_selected(), Some(false));
        let casa = elemento(&p, "Casa, scheda 3 di 5");
        assert_eq!(casa.accessible_item_selected(), Some(false));
        // Ridisegno F1, Task 8: niente più puntini dei layout.
        assert!(ElementHandle::find_by_accessible_label(&p, "Layout della Panoramica").next().is_none());
        assert_eq!(elemento(&p, "Pannello di ItsyHome").accessible_role(), Some(AccessibleRole::Groupbox));
        // La vista `agenti` è la scheda Agenti.
        p.set_vista("agenti".into());
        assert_eq!(elemento(&p, "Agenti, scheda 2 di 5").accessible_item_selected(), Some(true));
    }

    /// Ridisegno F1, Task 6: nella barra le schede (ruolo Tab, i nomi del modello), la ricerca
    /// inerte («Cerca, in arrivo») e il volume («Volume 40 %») sono pulsanti; Tab dalle schede
    /// arriva a ricerca, volume e «⋯», nell'ordine.
    #[test]
    fn barra_con_schede_ricerca_e_volume() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        for nome in ["Panoramica, scheda 1 di 5", "Agenti, scheda 2 di 5", "Casa, scheda 3 di 5", "PC, scheda 4 di 5", "Collegamenti, scheda 5 di 5"] {
            assert_eq!(elemento(&p, nome).accessible_role(), Some(AccessibleRole::Tab), "{nome}");
        }
        let cerca = elemento(&p, "Cerca, in arrivo");
        assert_eq!(cerca.accessible_role(), Some(AccessibleRole::Button));
        let volume = elemento(&p, "Volume 40 %");
        assert_eq!(volume.accessible_role(), Some(AccessibleRole::Button));
        // Barra in fondo: ricerca a sinistra, volume a destra, schede al centro.
        let y = cerca.absolute_position().y;
        assert!(y > 340.0 - 64.0 && y + cerca.size().height <= 340.0, "ricerca nella barra: {y}");
        assert!(cerca.absolute_position().x < 100.0 && volume.absolute_position().x > 700.0);
        let agenti = elemento(&p, "Agenti, scheda 2 di 5");
        let centro_schede = (elemento(&p, "Panoramica, scheda 1 di 5").absolute_position().x
            + elemento(&p, "Collegamenti, scheda 5 di 5").absolute_position().x
            + 36.0)
            / 2.0;
        assert!((centro_schede - 410.0).abs() <= 1.0, "schede centrate: {centro_schede}");
        assert!(agenti.absolute_position().y > 340.0 - 64.0);

        // Panoramica senza carte (ridisegno F1, Task 8: una scheda per Tab) → le cinque schede →
        // ricerca → volume → «⋯».
        for nome in ["Panoramica, scheda 1 di 5", "Agenti, scheda 2 di 5", "Casa, scheda 3 di 5", "PC, scheda 4 di 5", "Collegamenti, scheda 5 di 5"] {
            tasto(&p, Key::Tab);
            assert_eq!(etichetta_col_fuoco(&p), nome);
        }
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Cerca, in arrivo");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Volume 40 %");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Altre opzioni");
        // E indietro.
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Volume 40 %");
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Cerca, in arrivo");
        // La ricerca è inerte: Invio non cambia nulla.
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "panoramica");
        assert_eq!(etichetta_col_fuoco(&p), "Cerca, in arrivo");
    }

    /// Il pulsante del volume: clic = muto sì/no; la rotella in su ±2 punti per scatto (con
    /// l'impostazione accesa), fermo al limite; spenta l'impostazione la rotella passa.
    #[test]
    fn volume_clic_muto_e_rotella() {
        use slint::platform::{PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.set_volume_disponibile(true);
        p.set_volume(0.40);
        p.set_rotella_volume(true);
        let muti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let m = muti.clone();
        p.on_muto_volume(move |si| m.borrow_mut().push(si));
        let livelli = std::rc::Rc::new(std::cell::RefCell::new(Vec::<i32>::new()));
        let l = livelli.clone();
        p.on_imposta_volume(move |v| l.borrow_mut().push((v * 100.0).round() as i32));

        let volume = elemento(&p, "Volume 40 %");
        let (pos, lato) = (volume.absolute_position(), volume.size());
        let centro = slint::LogicalPosition::new(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        p.window().dispatch_event(WindowEvent::PointerMoved { position: centro });
        p.window().dispatch_event(WindowEvent::PointerPressed { position: centro, button: PointerEventButton::Left });
        p.window().dispatch_event(WindowEvent::PointerReleased { position: centro, button: PointerEventButton::Left });
        assert_eq!(*muti.borrow(), [true], "il clic chiede il muto");

        // Uno scatto della rotella (60 px) in su: +2 punti.
        volume.scroll(0.0, 60.0);
        assert_eq!(*livelli.borrow(), [42]);
        assert!((p.get_volume() - 0.42).abs() < 1e-6, "il pannello lo mostra subito");
        volume.scroll(0.0, -60.0);
        assert_eq!(*livelli.borrow(), [42, 40]);
        // Al limite resta lì, senza comandi.
        p.set_volume(1.0);
        elemento(&p, "Volume 100 %").scroll(0.0, 60.0);
        assert_eq!(p.get_volume(), 1.0);
        assert_eq!(*livelli.borrow(), [42, 40], "a 100 nessun comando");
        // Impostazione spenta: la rotella non cambia il volume.
        p.set_rotella_volume(false);
        p.set_volume(0.40);
        elemento(&p, "Volume 40 %").scroll(0.0, 60.0);
        assert_eq!(*livelli.borrow(), [42, 40]);
    }

    /// Senza uscita audio (o fuori da Windows) il pulsante non c'è (nessun elemento «Volume…») e
    /// le schede restano dove sono.
    #[test]
    fn volume_assente_geometria_ferma() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        let posti = |p: &Pannello| -> Vec<f32> {
            ["Panoramica, scheda 1 di 5", "Agenti, scheda 2 di 5", "Collegamenti, scheda 5 di 5"]
                .iter()
                .map(|n| elemento(p, n).absolute_position().x)
                .collect()
        };
        let con_volume = posti(&p);
        assert!(elementi_volume(&p) > 0);
        p.set_volume_disponibile(false);
        assert_eq!(elementi_volume(&p), 0, "nessun elemento «Volume…»");
        assert_eq!(posti(&p), con_volume, "schede ferme");
        assert!(ElementHandle::find_by_accessible_label(&p, "Cerca, in arrivo").next().is_some());
    }

    /// Il muto premuto da fuori (tastiera del PC): il nome per Narrator diventa «Volume, muto».
    #[test]
    fn volume_muto_da_fuori_cambia_il_nome() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        assert!(ElementHandle::find_by_accessible_label(&p, "Volume 40 %").next().is_some());
        p.set_volume_muto(true);
        let muto = elemento(&p, "Volume, muto");
        assert_eq!(muto.accessible_role(), Some(AccessibleRole::Button));
        assert!(ElementHandle::find_by_accessible_label(&p, "Volume 40 %").next().is_none());
        p.set_volume_muto(false);
        assert!(ElementHandle::find_by_accessible_label(&p, "Volume 40 %").next().is_some());
    }

    /// Review Focus 2: l'uscita audio sparisce col fuoco sul volume (cuffie staccate). Il fuoco va
    /// sulla ricerca, il vicino, e il Tab seguente non si perde: porta a «Altre opzioni».
    #[test]
    fn volume_che_sparisce_col_fuoco_addosso() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        // Contenuto → le cinque schede → ricerca → volume.
        for _ in 0..7 {
            tasto(&p, Key::Tab);
        }
        assert_eq!(etichetta_col_fuoco(&p), "Volume 40 %");
        p.set_volume_disponibile(false);
        // I `changed` girano al giro seguente del ciclo d'eventi.
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Cerca, in arrivo", "il fuoco va al vicino");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Altre opzioni", "il Tab seguente non si perde");
    }

    /// La scheda col segno: «Agenti, ti aspetta, scheda 2 di 5», selezionata quando la vista è
    /// `agenti`, allargata col nome breve; la capsula sta sotto di lei.
    #[test]
    fn scheda_attiva_ha_il_nome_e_lo_stato() {
        piattaforma_di_prova();
        let p = pannello();
        let mut registro = crate::moduli::registro::Registro::nuovo(&[]);
        registro.livelli_voci(&[("agenti", crate::attenzione::Livello::Bloccante)]);
        p.set_schede(ModelRc::new(VecModel::from(crate::pannello::schede_ui(&registro, "panoramica"))));
        let agenti = elemento(&p, "Agenti, ti aspetta, scheda 2 di 5");
        assert_eq!(agenti.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!(agenti.accessible_item_selected(), Some(false));
        assert_eq!(agenti.size().width, 36.0);
        p.set_vista("agenti".into());
        p.set_schede(ModelRc::new(VecModel::from(crate::pannello::schede_ui(&registro, "agenti"))));
        let agenti = elemento(&p, "Agenti, ti aspetta, scheda 2 di 5");
        assert_eq!(agenti.accessible_item_selected(), Some(true));
        assert!(agenti.size().width > 36.0, "attiva: col nome");
        assert!(ElementHandle::find_by_accessible_label(&p, "AGENTI").next().is_none(), "il nome visibile non è un secondo nome per Narrator");
    }

    /// Ondata finale F1 (I1): col movimento ridotto la barra non scivola. Cambiando pagina le
    /// schede, le icone, il gruppo centrato, la capsula e il divisore sono subito al loro posto
    /// finale (0 ms); con le animazioni accese, subito dopo il cambio sono ancora a metà strada.
    #[test]
    fn barra_salta_senza_scivolare_col_movimento_ridotto() {
        piattaforma_di_prova();
        // Posizione e larghezza di ogni pezzo della barra che si muove cambiando pagina.
        let geometria = |p: &Pannello| -> Vec<(String, f32, f32)> {
            let mut v = Vec::new();
            for nome in ["Panoramica, scheda 1 di 5", "Agenti, scheda 2 di 5", "Casa, scheda 3 di 5", "PC, scheda 4 di 5", "Collegamenti, scheda 5 di 5", "Moduli"] {
                let e = elemento(p, nome);
                v.push((nome.to_string(), e.absolute_position().x, e.size().width));
            }
            for id in ["SchedaBarra::icona", "Barra::capsula", "Barra::divisore"] {
                let mut trovati = 0;
                for e in ElementHandle::find_by_element_id(p, id) {
                    v.push((id.to_string(), e.absolute_position().x, e.size().width));
                    trovati += 1;
                }
                assert!(trovati > 0, "nessun «{id}»");
            }
            v
        };
        for ridotto in [false, true] {
            let p = pannello();
            p.global::<crate::Movimento>().set_ridotto(ridotto);
            p.set_schede(schede_di("panoramica"));
            avanza(400);
            let prima = geometria(&p);
            p.set_vista("agenti".into());
            p.set_schede(schede_di("agenti"));
            slint::platform::update_timers_and_animations();
            let subito = geometria(&p);
            avanza(400);
            let fine = geometria(&p);
            assert_ne!(prima, fine, "ridotto={ridotto}: la pagina nuova cambia la barra");
            if ridotto {
                let diversi: Vec<_> = subito.iter().zip(&fine).filter(|(a, b)| a != b).collect();
                assert!(diversi.is_empty(), "col movimento ridotto la barra scivola ancora: {diversi:?}");
            } else {
                // Ogni pezzo che si muove è ancora in viaggio subito dopo il cambio (la prova
                // può fallire: senza animazione `subito` sarebbe già `fine`).
                for id in ["Agenti, scheda 2 di 5", "Moduli", "SchedaBarra::icona", "Barra::capsula", "Barra::divisore"] {
                    let in_viaggio = subito.iter().zip(&fine).zip(&prima).any(|((a, b), c)| a.0 == id && a != b && b != c);
                    assert!(in_viaggio, "con le animazioni «{id}» dovrebbe scivolare");
                }
            }
        }
    }

    /// Una voce di prova per la Coda: `permesso` le dà «Nega» e il delicato «Consenti».
    fn voce_coda(id: &str, titolo: &str, livello: LivelloUi, sotto: &str, permesso: bool) -> VoceAttesaUi {
        let pulsanti = if permesso {
            vec![
                PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false },
                PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true },
            ]
        } else {
            vec![]
        };
        VoceAttesaUi {
            id: id.into(),
            livello,
            titolo: titolo.into(),
            sotto: sotto.into(),
            modulo: "Agenti".into(),
            dettaglio: "Il testo completo".into(),
            icona: Default::default(),
            pulsanti: ModelRc::new(VecModel::from(pulsanti)),
            permesso: Default::default(),
        }
    }

    /// Un pannello aperto sulla Coda con queste voci, la tastiera pronta; restituisce le azioni
    /// arrivate. Ridisegno F2, Task 3 (pre-flight C3): ascolta tutte e due le callback in un solo
    /// registro. Le decisioni su un permesso passano dalla guardia di Rust (`azione-voce`) e si
    /// registrano col solo id del pulsante («nega», «consenti»); un'`azione` diretta col prefisso
    /// («azione riprova»). Così un test che aspetta «consenti» fallisce se il consenso salta la
    /// guardia, e un test che aspetta il registro vuoto vede anche le azioni dirette.
    fn pannello_in_coda(voci: ModelRc<VoceAttesaUi>) -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let p = pannello();
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_attese(voci);
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione(move |_, id| a.borrow_mut().push(format!("azione {id}")));
        let a = azioni.clone();
        p.on_azione_voce(move |_, id| a.borrow_mut().push(id.to_string()));
        p.set_vista("coda".into());
        (p, azioni)
    }

    /// Il permesso del guscio è aperto nella Coda (il suo riquadro si vede).
    fn permesso_aperto(p: &Pannello) -> bool {
        ElementHandle::find_by_element_id(p, "Permesso::riquadro").next().is_some()
    }

    /// Un permesso di Claude Code coi dati divisi (Task 2): sessione e cartella «my-shop»,
    /// strumento «Comando», chiesto «alle 14:32».
    fn permesso_con_dati(id: &str, comando: &str) -> VoceAttesaUi {
        VoceAttesaUi {
            dettaglio: format!("{comando}\nSessione: my-shop").into(),
            permesso: crate::PermessoUi {
                sessione: "my-shop".into(),
                strumento: "Comando".into(),
                campo: "TERMINALE".into(),
                comando: comando.into(),
                cartella: "my-shop".into(),
                chiesto: "alle 14:32".into(),
            },
            ..voce_coda(id, &format!("Claude chiede: Comando {comando}"), LivelloUi::Bloccante, "Ti aspetta · Agenti", true)
        }
    }

    /// Un elemento visibile `nome` dentro il rettangolo di `dentro` (per intero).
    fn sta_dentro(e: &ElementHandle, dentro: &ElementHandle) -> bool {
        let (a, la, b, lb) = (e.absolute_position(), e.size(), dentro.absolute_position(), dentro.size());
        a.x >= b.x - 0.5 && a.y >= b.y - 0.5 && a.x + la.width <= b.x + lb.width + 0.5 && a.y + la.height <= b.y + lb.height + 0.5
    }

    /// Un elemento del permesso per id (visibile), o il panico.
    fn parte(p: &Pannello, id: &str) -> ElementHandle {
        ElementHandle::find_by_element_id(p, &format!("Permesso::{id}")).next().unwrap_or_else(|| panic!("nessun «{id}» visibile"))
    }

    /// Ridisegno F2, Task 3: un permesso coi dati divisi si apre nel componente del guscio.
    /// Capsula «Chiede il permesso», «chiesto alle 14:32» accanto (mai sotto i pulsanti, che con
    /// `pulsanti-in-alto` stanno nello stesso angolo: pre-flight C13), «TERMINALE», «$» e il
    /// comando, sotto la sessione; Narrator legge «Ti aspetta · Agenti: my-shop chiede: Comando»
    /// col comando e la sessione come descrizione, una volta sola (fix 1: i testi del comando e
    /// della sessione non hanno un nome loro). La cartella si vede solo se c'è e non è la sessione;
    /// «$» solo per lo strumento «Comando» (requisito del Task 2).
    #[test]
    fn permesso_mostra_comando_e_cartella() {
        use slint::platform::Key;
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_con_dati("claude.permesso.1", "npm test -- checkout")]));
        let (p, azioni) = pannello_in_coda(ModelRc::from(voci.clone()));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        let gruppo = elemento(&p, "Ti aspetta · Agenti: my-shop chiede: Comando");
        assert_eq!(gruppo.accessible_role(), Some(AccessibleRole::Groupbox));
        assert_eq!(gruppo.accessible_description().as_deref(), Some("npm test -- checkout\nmy-shop"));
        let riquadro = parte(&p, "riquadro");
        assert!(sta_dentro(&elemento(&p, "TERMINALE"), &riquadro));
        for id in ["dollaro", "comando", "sessione"] {
            let e = parte(&p, id);
            assert!(sta_dentro(&e, &riquadro), "«{id}» fuori dal riquadro");
            assert_eq!(e.accessible_role(), Some(AccessibleRole::None), "«{id}» letto due volte da Narrator");
        }
        // Il comando non ha un nome suo: Narrator lo legge una volta, nella descrizione.
        assert!(ElementHandle::find_by_accessible_label(&p, "npm test -- checkout").next().is_none());
        // La capsula del tono sta nel permesso, sopra il riquadro (decorativa: il tono lo dice il nome).
        let capsula = ElementHandle::find_by_element_type_name(&p, "Capsula")
            .find(|c| sta_dentro(c, &gruppo))
            .expect("la capsula «Chiede il permesso»");
        assert!(capsula.absolute_position().y + capsula.size().height <= riquadro.absolute_position().y);
        assert!(include_str!("../ui/guscio/permesso.slint").contains("parola: \"Chiede il permesso\""));
        // «chiesto alle 14:32» accanto alla capsula, a sinistra di «Nega».
        let chiesto = elemento(&p, "chiesto alle 14:32");
        let nega = elemento(&p, "Nega");
        assert!(chiesto.absolute_position().x >= capsula.absolute_position().x + capsula.size().width);
        assert!(chiesto.absolute_position().x + chiesto.size().width <= nega.absolute_position().x, "«chiesto» sotto «Nega»");
        // Una cartella diversa dalla sessione si vede dopo il punto.
        let mut v = voci.row_data(0).unwrap();
        v.permesso.cartella = "shop-api".into();
        voci.set_row_data(0, v.clone());
        slint::platform::update_timers_and_animations();
        assert_eq!(elemento(&p, "Ti aspetta · Agenti: my-shop chiede: Comando").accessible_description().as_deref(), Some("npm test -- checkout\nmy-shop · shop-api"));
        // Un altro strumento (un percorso, un indirizzo): niente «$», e la sua etichetta.
        v.permesso.strumento = "Modifica".into();
        v.permesso.campo = "FILE".into();
        voci.set_row_data(0, v);
        slint::platform::update_timers_and_animations();
        assert!(ElementHandle::find_by_element_id(&p, "Permesso::dollaro").next().is_none(), "«$» per uno strumento che non è un comando");
        assert!(parte(&p, "comando").absolute_position().x <= riquadro.absolute_position().x + 16.0);
        assert!(ElementHandle::find_by_accessible_label(&p, "TERMINALE").next().is_none(), "«TERMINALE» per un file");
        assert!(sta_dentro(&elemento(&p, "FILE"), &riquadro));
        assert!(azioni.borrow().is_empty());
    }

    /// Fix 1 del Task 3 (ruling del coordinatore): l'etichetta sopra il campo dice cos'è il testo —
    /// «TERMINALE» per un comando, «FILE» per Legge/Modifica/Scrive, «INDIRIZZO» per WebFetch,
    /// «RICERCA» per WebSearch; nessuna per gli altri strumenti. La sceglie Rust dal nome vero
    /// dello strumento (`claude::campo_del_permesso`), qui si mostra quella che arriva.
    #[test]
    fn etichetta_del_campo_dallo_strumento() {
        use slint::platform::Key;
        for campo in ["TERMINALE", "FILE", "INDIRIZZO", "RICERCA", ""] {
            let mut v = permesso_con_dati("claude.permesso.1", "qualcosa");
            v.permesso.campo = campo.into();
            let (p, _) = pannello_in_coda(ModelRc::new(VecModel::from(vec![v])));
            tasto(&p, Key::Return);
            lascia_girare(&p);
            let riquadro = parte(&p, "riquadro");
            for altro in ["TERMINALE", "FILE", "INDIRIZZO", "RICERCA"] {
                let visto = ElementHandle::find_by_accessible_label(&p, altro).next();
                if altro == campo {
                    assert!(visto.is_some_and(|e| sta_dentro(&e, &riquadro)), "«{campo}» manca");
                } else {
                    assert!(visto.is_none(), "«{altro}» al posto di «{campo}»");
                }
            }
            // «$» solo per il terminale.
            assert_eq!(ElementHandle::find_by_element_id(&p, "Permesso::dollaro").next().is_some(), campo == "TERMINALE", "«$» con «{campo}»");
        }
    }

    /// Ridisegno F2, Task 3 (Review Focus 5): un permesso senza dati di sessione (la voce di prova,
    /// domani un agente che non dà la cartella) mostra titolo e dettaglio intero in mono, nessun
    /// campo vuoto a vista («TERMINALE», «$», capsula), con le stesse regole: Ctrl+Invio concede
    /// solo dopo il fondo del testo.
    #[test]
    fn permesso_senza_dati_mostra_il_dettaglio() {
        use slint::platform::Key;
        let voce = permesso_lungo(12);
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![voce.clone()])));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        let gruppo = elemento(&p, &format!("Ti aspetta · Agenti: {}", voce.titolo));
        assert_eq!(gruppo.accessible_role(), Some(AccessibleRole::Groupbox));
        assert_eq!(gruppo.accessible_description().as_deref(), Some(voce.dettaglio.as_str()));
        for vuoto in ["TERMINALE", "$", "chiesto alle 14:32"] {
            assert!(ElementHandle::find_by_accessible_label(&p, vuoto).next().is_none(), "«{vuoto}» senza dati");
        }
        assert!(ElementHandle::find_by_element_type_name(&p, "Capsula").all(|c| !sta_dentro(&c, &gruppo)), "capsula senza dati");
        let titolo = ElementHandle::find_by_element_id(&p, "Permesso::titolo").next().expect("il titolo");
        let dettaglio = ElementHandle::find_by_element_id(&p, "Permesso::dettaglio").next().expect("il dettaglio");
        assert!(dettaglio.absolute_position().y >= titolo.absolute_position().y + titolo.size().height);
        assert!(ElementHandle::find_by_element_id(&p, "Permesso::comando").next().is_none(), "il comando senza dati");
        // Ctrl+Invio prima del fondo: nulla; il consenso non si raggiunge.
        con_modificatori(&p, &[Key::Control], Key::Return);
        tasto(&p, Key::RightArrow);
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert!(azioni.borrow().is_empty(), "concesso senza aver letto: {:?}", azioni.borrow());
        tasto(&p, Key::UpArrow);
        for _ in 0..40 {
            if ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_some() {
                break;
            }
            tasto(&p, Key::DownArrow);
            lascia_girare(&p);
        }
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"]);
    }

    /// Ridisegno F2, Task 3 (Review Focus 2): un comando di 2000 caratteri senza spazi va a capo
    /// nel riquadro (nessun testo oltre il bordo destro: tante righe, non una), la sessione resta
    /// leggibile e «Consenti» c'è solo dopo ↓ fino in fondo.
    #[test]
    fn permesso_lungo_su_una_riga_si_legge_tutto() {
        use slint::platform::Key;
        let comando = "x".repeat(2000);
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_con_dati("claude.permesso.1", &comando)])));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        let riquadro = ElementHandle::find_by_element_id(&p, "Permesso::riquadro").next().expect("il riquadro");
        let testo = ElementHandle::find_by_element_id(&p, "Permesso::comando").next().expect("il comando");
        let (r, lr, t, lt) = (riquadro.absolute_position(), riquadro.size(), testo.absolute_position(), testo.size());
        assert!(t.x + lt.width <= r.x + lr.width, "il comando passa il bordo destro");
        assert!(lt.height > 10.0 * 16.0, "il comando non va a capo: alto {}", lt.height);
        assert!(sta_dentro(&parte(&p, "sessione"), &riquadro), "la sessione si legge");
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none());
        tasto(&p, Key::UpArrow);
        let mut passi = 0;
        while ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none() {
            con_modificatori(&p, &[Key::Control], Key::Return);
            tasto(&p, Key::DownArrow);
            lascia_girare(&p);
            passi += 1;
            assert!(passi < 200, "il fondo del testo non arriva");
        }
        assert!(passi > 1, "il testo doveva scorrere: {passi} passi");
        assert!(azioni.borrow().is_empty(), "concesso prima del fondo: {:?}", azioni.borrow());
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::RightArrow);
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"]);
    }

    /// Ridisegno F2, Task 3 (rimandato di F1, pre-flight C14): il permesso aperto alla riga 1 e la
    /// voce 0 tolta: resta aperto (la validità la decide l'id, non la riga), il fuoco resta sul
    /// suo bersaglio e «Nega» non si sposta. Un permesso diverso al suo posto lo chiude e il fuoco
    /// torna sulla riga.
    #[test]
    fn permesso_che_cambia_posto_resta_aperto() {
        use slint::platform::Key;
        let voci = std::rc::Rc::new(VecModel::from(vec![
            voce_coda("a", "Primo", LivelloUi::Bloccante, "Ti aspetta · Agenti", true),
            permesso_con_dati("claude.permesso.2", "npm test"),
            voce_coda("c", "Terzo", LivelloUi::Bloccante, "Ti aspetta · Agenti", true),
        ]));
        let (p, azioni) = pannello_in_coda(ModelRc::from(voci.clone()));
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Claude chiede: Comando npm test, Ti aspetta · Agenti", "la riga 1");
        tasto(&p, Key::Return);
        lascia_girare(&p);
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        let nega = elemento(&p, "Nega");
        let prima = (nega.absolute_position(), nega.size());
        voci.remove(0);
        slint::platform::update_timers_and_animations();
        lascia_girare(&p);
        assert!(permesso_aperto(&p), "il permesso spostato si è chiuso");
        let gruppi: Vec<String> = ElementHandle::find_by_element_type_name(&p, "Permesso")
            .filter_map(|e| e.accessible_label().map(|s| s.to_string()))
            .collect();
        assert_eq!(gruppi, ["Ti aspetta · Agenti: my-shop chiede: Comando"], "lo stesso permesso");
        assert_eq!(etichetta_col_fuoco(&p), "Consenti", "il fuoco resta sul suo bersaglio");
        let nega = elemento(&p, "Nega");
        assert_eq!((nega.absolute_position(), nega.size()), prima, "«Nega» si è spostato");
        // Esc: di nuovo la sua riga, alla posizione nuova.
        tasto(&p, Key::Escape);
        lascia_girare(&p);
        assert_eq!(etichetta_col_fuoco(&p), "Claude chiede: Comando npm test, Ti aspetta · Agenti");
        // Riaperto, un permesso diverso al suo posto lo chiude: il fuoco torna sulla riga.
        tasto(&p, Key::Return);
        lascia_girare(&p);
        assert!(permesso_aperto(&p));
        voci.set_row_data(0, permesso_con_dati("claude.permesso.3", "rm -rf build"));
        slint::platform::update_timers_and_animations();
        lascia_girare(&p);
        assert!(!permesso_aperto(&p), "un altro permesso al suo posto: si chiude");
        assert_eq!(etichetta_col_fuoco(&p), "Claude chiede: Comando rm -rf build, Ti aspetta · Agenti");
        assert_eq!(ruolo_col_fuoco(&p), i_slint_core::items::AccessibleRole::ListItem);
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert!(azioni.borrow().is_empty(), "{:?}", azioni.borrow());
    }

    /// Ridisegno F2, Task 3: nella Coda ogni decisione su un permesso (clic e Invio su «Nega»,
    /// Ctrl+Invio su «Consenti», nel permesso aperto e nella riga) passa da `azione-voce`, la
    /// guardia dei 600 ms di Rust; nessuna da `azione`.
    #[test]
    fn decisioni_del_permesso_passano_dalla_guardia() {
        use slint::platform::Key;
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_con_dati("claude.permesso.1", "npm test")])));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        let (x, y) = centro(&elemento(&p, "Nega"));
        clic_in(&p, x, y);
        // Fix 1 (minor 5): dopo un clic l'anello da tastiera si spegne, il fuoco torna al pannello.
        assert_eq!(etichetta_col_fuoco(&p), "Pannello di ItsyHome");
        tasto(&p, Key::Return);
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "il tasto seguente riparte da «Nega»");
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        con_modificatori(&p, &[Key::Control], Key::Return);
        // La riga chiusa: «Nega» da tastiera e col clic.
        tasto(&p, Key::Escape);
        lascia_girare(&p);
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        tasto(&p, Key::Return);
        let (x, y) = centro(&elemento(&p, "Nega"));
        clic_in(&p, x, y);
        assert_eq!(*azioni.borrow(), ["nega", "nega", "consenti", "nega", "nega"]);
    }

    /// Il rettangolo di un elemento in coordinate della finestra: (sinistra, alto, destra, basso).
    fn rettangolo(e: &ElementHandle) -> (f32, f32, f32, f32) {
        let (a, l) = (e.absolute_position(), e.size());
        (a.x, a.y, a.x + l.width, a.y + l.height)
    }

    /// Due rettangoli staccati di almeno `spazio` px su uno dei quattro lati.
    fn staccati(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32), spazio: f32) -> bool {
        a.2 + spazio <= b.0 || b.2 + spazio <= a.0 || a.3 + spazio <= b.1 || b.3 + spazio <= a.1
    }

    /// I bersagli del clic nel corpo del pannello (fra la riga alta e la barra): righe d'elenco e
    /// pulsanti visibili, col nome e il rettangolo.
    fn bersagli_nel_corpo(p: &Pannello) -> Vec<(String, (f32, f32, f32, f32))> {
        i_slint_backend_testing::ElementQuery::from_root(p)
            .match_descendants()
            .match_predicate(|e| matches!(e.accessible_role(), Some(AccessibleRole::Button | AccessibleRole::ListItem)))
            .find_all()
            .into_iter()
            .map(|e| (e.accessible_label().unwrap_or_default().to_string(), rettangolo(&e)))
            .filter(|(_, r)| r.1 >= 44.0 - 0.5 && r.3 <= 44.0 + 224.0 + 0.5 && r.2 > r.0 && r.3 > r.1)
            .collect()
    }

    /// Lascia finire l'arrivo della vista (il timer da 16 ms, poi i 280 ms dello scorrimento).
    fn fine_arrivo() {
        slint::platform::update_timers_and_animations();
        avanza(20);
        slint::platform::update_timers_and_animations();
        avanza(400);
        slint::platform::update_timers_and_animations();
    }

    /// Fix 1 del Task 3 (revisione I1, regola M3): nessun pulsante del permesso aperto («Nega»,
    /// «Consenti», «Torna all'elenco») nasce sotto un bersaglio del fotogramma prima, per tutto il
    /// suo rettangolo (quattro lati, a fine arrivo) e con almeno 2 px di stacco: né sotto una riga
    /// della Coda o i suoi pulsanti, né sotto «Apri» e «Nega» della voce che ti aspetta da cui si
    /// arriva. Prima del fix i pulsanti alti 40 coprivano i 4 px alti della riga 0.
    #[test]
    fn pulsanti_del_permesso_fuori_dai_bersagli_di_prima() {
        // Dalla Coda: un permesso breve (il suo «Consenti» c'è subito) fra altre righe.
        let mut voci = vec![permesso_con_dati("claude.permesso.1", "npm test")];
        voci.extend((0..5).map(|i| {
            let mut v = voce_coda(&format!("r{i}"), &format!("Avviso {i}"), LivelloUi::Attenzione, "Guarda presto · Casa", false);
            v.pulsanti = ModelRc::new(VecModel::from(vec![PulsanteUi { id: "riprova".into(), testo: "Riprova".into(), delicato: false }]));
            v
        }));
        let (p, _) = pannello_in_coda(ModelRc::new(VecModel::from(voci)));
        fine_arrivo();
        let prima = bersagli_nel_corpo(&p);
        assert!(prima.iter().any(|(n, _)| n == "Apri") && prima.len() >= 6, "i bersagli dell'elenco: {prima:?}");
        let (x, y) = centro(&elemento(&p, "Apri"));
        clic_in(&p, x, y);
        lascia_girare(&p);
        fine_arrivo();
        let dopo = bersagli_nel_corpo(&p);
        for nome in ["Nega", "Consenti", "Torna all'elenco"] {
            assert!(dopo.iter().any(|(n, _)| n == nome), "«{nome}» manca nel permesso aperto: {dopo:?}");
        }
        for (n, r) in &dopo {
            for (m, q) in &prima {
                assert!(staccati(*r, *q, 2.0), "«{n}» {r:?} nasce sotto «{m}» {q:?}");
            }
        }
        // Dalla voce che ti aspetta: «Apri» porta al permesso aperto nella Coda.
        let (p, _) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![permesso_con_dati("claude.permesso.1", "npm test")])));
        fine_arrivo();
        let prima = bersagli_nel_corpo(&p);
        assert!(prima.iter().any(|(n, _)| n == "Apri"), "{prima:?}");
        let (x, y) = centro(&elemento(&p, "Apri"));
        clic_in(&p, x, y);
        lascia_girare(&p);
        fine_arrivo();
        assert!(permesso_aperto(&p));
        for (n, r) in bersagli_nel_corpo(&p) {
            for (m, q) in &prima {
                assert!(staccati(r, *q, 2.0), "«{n}» {r:?} nasce sotto «{m}» {q:?} della Panoramica");
            }
        }
    }

    /// Fix 1 del Task 3 (revisione I1): un doppio clic sul bordo alto della riga 0 (dove i
    /// pulsanti alti 40 la coprivano), in tutta la sua larghezza: il primo clic apre il permesso,
    /// il secondo non decide nulla.
    #[test]
    fn doppio_clic_sul_bordo_della_riga_non_decide() {
        use slint::platform::Key;
        let voci = vec![
            permesso_con_dati("claude.permesso.1", "npm test"),
            voce_coda("b", "Secondo", LivelloUi::Bloccante, "Ti aspetta · Agenti", true),
        ];
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(voci)));
        fine_arrivo();
        let riga = rettangolo(&elemento(&p, "Claude chiede: Comando npm test, Ti aspetta · Agenti"));
        // I pulsanti della riga che decidono da soli al primo clic («Nega»): lì il primo clic è
        // già una decisione voluta, non il caso da provare.
        let decidono: Vec<_> = bersagli_nel_corpo(&p).into_iter().filter(|(n, _)| n == "Nega").map(|(_, r)| r).collect();
        let mut prove = 0;
        for dy in [0.5f32, 2.0, 4.0] {
            let mut x = riga.0 + 2.0;
            while x < riga.2 - 1.0 {
                let y = riga.1 + dy;
                if decidono.iter().any(|r| x >= r.0 - 1.0 && x <= r.2 + 1.0 && y >= r.1 - 1.0 && y <= r.3 + 1.0) {
                    x += 12.0;
                    continue;
                }
                clic_in(&p, x, y);
                lascia_girare(&p);
                assert!(permesso_aperto(&p), "il primo clic a ({x}, {y}) non ha aperto");
                clic_in(&p, x, y);
                lascia_girare(&p);
                assert!(azioni.borrow().is_empty(), "il doppio clic a ({x}, {y}) ha deciso: {:?}", azioni.borrow());
                if permesso_aperto(&p) {
                    tasto(&p, Key::Escape);
                    lascia_girare(&p);
                }
                x += 12.0;
                prove += 1;
            }
        }
        assert!(prove > 150, "{prove} prove");
    }

    /// Fix 1 del Task 3 (revisione I2a): «Consenti» si disegna neutro come «Nega» (regola M3,
    /// decisione di Paolo): il colore del fondo dei due pulsanti, letto dal disegno vero (skia
    /// senza schermo), è il grigio neutro dei token; con la tinta d'azione sarebbe bianco.
    #[test]
    fn consenti_disegnato_neutro_come_nega() {
        use slint::platform::Key;
        super::piattaforma_che_disegna();
        let (p, _) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_con_dati("claude.permesso.1", "npm test")])));
        fine_arrivo();
        tasto(&p, Key::Return);
        lascia_girare(&p);
        fine_arrivo();
        let foto = p.window().take_snapshot().expect("il disegno del pannello");
        let scala = foto.width() as f32 / p.window().size().to_logical(p.window().scale_factor()).width;
        let colore = |nome: &str| {
            let (x0, y0, _, y1) = rettangolo(&elemento(&p, nome));
            let (x, y) = (((x0 + 8.0) * scala) as usize, (((y0 + y1) / 2.0) * scala) as usize);
            let px = foto.as_slice()[y * foto.width() as usize + x];
            (px.r, px.g, px.b)
        };
        let neutro = super::colore_del_token(&super::tema_slint(), "g3");
        let atteso = (
            u8::from_str_radix(&neutro[1..3], 16).unwrap(),
            u8::from_str_radix(&neutro[3..5], 16).unwrap(),
            u8::from_str_radix(&neutro[5..7], 16).unwrap(),
        );
        let vicino = |c: (u8, u8, u8)| c.0.abs_diff(atteso.0) <= 3 && c.1.abs_diff(atteso.1) <= 3 && c.2.abs_diff(atteso.2) <= 3;
        let (nega, consenti) = (colore("Nega"), colore("Consenti"));
        assert!(vicino(nega), "«Nega» disegnato {nega:?}, atteso il neutro {atteso:?}");
        assert!(vicino(consenti), "«Consenti» disegnato {consenti:?}, atteso il neutro {atteso:?}");
    }

    /// Fix 1 del Task 3 (revisione I2b, minor 1): letto A fino in fondo, un altro permesso B
    /// (lungo) arriva nello stesso componente senza un giro del ciclo d'eventi in mezzo: «Consenti»
    /// non c'è, né subito né dopo, finché non si vede il fondo di B. `visto` vale per l'id.
    #[test]
    fn visto_riparte_con_un_altro_permesso() {
        use slint::platform::Key;
        let lungo = |id: &str, parola: &str| permesso_con_dati(id, &format!("{parola} ").repeat(400));
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![
            lungo("claude.permesso.a", "alfa"),
            lungo("claude.permesso.b", "beta"),
        ])));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        tasto(&p, Key::UpArrow);
        for _ in 0..200 {
            if ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_some() {
                break;
            }
            tasto(&p, Key::DownArrow);
            lascia_girare(&p);
        }
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_some(), "letto A, «Consenti» c'è");
        p.invoke_apri_coda_su(1);
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none(), "B appena arrivato: «Consenti» di A");
        lascia_girare(&p);
        let gruppo = elemento(&p, "Ti aspetta · Agenti: my-shop chiede: Comando");
        assert!(gruppo.accessible_description().unwrap_or_default().starts_with("beta"), "il permesso B");
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none(), "B non letto: «Consenti»");
        assert!(elemento(&p, "Leggi fino in fondo per consentire").accessible_role().is_some());
        con_modificatori(&p, &[Key::Control], Key::Return);
        tasto(&p, Key::RightArrow);
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert!(azioni.borrow().is_empty(), "concesso B senza leggerlo: {:?}", azioni.borrow());
    }

    /// Fix 1 del Task 3 (ruling del coordinatore): chi usa il mouse torna all'elenco della Coda
    /// con «Torna all'elenco» (fuori dalla fascia delle righe: lo prova
    /// `pulsanti_del_permesso_fuori_dai_bersagli_di_prima`). Non decide nulla.
    #[test]
    fn torna_all_elenco_col_mouse() {
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_con_dati("claude.permesso.1", "npm test")])));
        let (x, y) = centro(&elemento(&p, "Apri"));
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert!(permesso_aperto(&p));
        let torna = elemento(&p, "Torna all'elenco");
        assert_eq!(torna.accessible_role(), Some(AccessibleRole::Button));
        let (x, y) = centro(&torna);
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert!(!permesso_aperto(&p), "il permesso resta aperto");
        assert!(elemento(&p, "In attesa").accessible_role().is_some(), "l'elenco torna");
        assert!(azioni.borrow().is_empty());
    }

    /// Ridisegno F2, Task 3 (pre-flight C1, C2): il permesso della Coda è un'istanza fissa del
    /// pannello, mai dentro un `if` né un `for`. Un id dentro un `if` non si raggiunge da fuori
    /// (spike del Task 3: «Cannot access id»), e un componente creato al momento dell'apertura
    /// non vedrebbe l'incremento di `armamenti` che la accompagna (l'aspetto inerte del Task 4).
    #[test]
    fn permesso_fisso_fuori_dagli_if() {
        let sorgente = include_str!("../ui/pannello.slint");
        let righe: Vec<&str> = sorgente.lines().map(super::senza_commento).collect();
        let i = righe.iter().position(|r| r.contains("coda-permesso := Permesso")).expect("il permesso della Coda");
        let mut profondita = 0i32;
        let mut contenitori = 0;
        for r in righe[..i].iter().rev() {
            profondita += r.matches('}').count() as i32 - r.matches('{').count() as i32;
            if profondita < 0 {
                let t = r.trim_start();
                assert!(!(t.starts_with("if ") && !t.starts_with("if (")) && !t.starts_with("for "), "il permesso sta dentro «{t}»");
                contenitori += 1;
                profondita = 0;
            }
        }
        assert!(contenitori >= 2, "dentro il pannello: {contenitori} blocchi");
    }

    /// Ridisegno F1, Task 7: → su una riga di permesso porta il fuoco vero su «Nega» (mai su
    /// «Consenti»), qualunque sia l'ordine dei pulsanti; ← torna alla riga. All'ingresso della Coda
    /// il fuoco è sulla riga, non su un pulsante.
    #[test]
    fn coda_entra_su_nega() {
        use slint::platform::Key;
        for consenti_primo in [false, true] {
            let mut voce = voce_coda("p", "Permesso", LivelloUi::Bloccante, "Ti aspetta · Agenti", true);
            if consenti_primo {
                let v: Vec<PulsanteUi> = voce.pulsanti.iter().collect();
                voce.pulsanti = ModelRc::new(VecModel::from(vec![v[1].clone(), v[0].clone()]));
            }
            let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![voce])));
            tasto(&p, Key::DownArrow);
            assert_eq!(etichetta_col_fuoco(&p), "Permesso, Ti aspetta · Agenti", "consenti_primo={consenti_primo}: la riga");
            tasto(&p, Key::RightArrow);
            assert_eq!(etichetta_col_fuoco(&p), "Nega", "consenti_primo={consenti_primo}: si entra su «Nega»");
            tasto(&p, Key::Return);
            assert_eq!(*azioni.borrow(), ["nega"], "consenti_primo={consenti_primo}: Invio su «Nega» nega");
        }
        // Senza «Nega»: il primo pulsante.
        let mut voce = voce_coda("r", "Casa non raggiungibile", LivelloUi::Attenzione, "Guarda presto · Casa", false);
        voce.pulsanti = ModelRc::new(VecModel::from(vec![PulsanteUi { id: "riprova".into(), testo: "Riprova".into(), delicato: false }]));
        let (p, _) = pannello_in_coda(ModelRc::new(VecModel::from(vec![voce])));
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Riprova");
        tasto(&p, Key::LeftArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Casa non raggiungibile, Guarda presto · Casa", "← alla riga");
    }

    /// Un permesso col riassunto lungo (il comando) e la sessione, come lo scrive `voce_permesso`.
    fn permesso_lungo(righe: usize) -> VoceAttesaUi {
        let comando: Vec<String> =
            (0..righe).map(|i| format!("Remove-Item -Recurse -Force C:\\Work\\cartella-{i}\\sottocartella")).collect();
        VoceAttesaUi {
            dettaglio: format!("{}\nSessione: progetto-a", comando.join(" ; ")).into(),
            ..voce_coda(
                "claude.permesso.1",
                "Claude chiede: PowerShell Remove-Item -Recurse -Force C:\\Work\\cartella-0\\sottocartella ; Remove-Item -Recurse…",
                LivelloUi::Bloccante,
                "Aspetta te · Agenti",
                true,
            )
        }
    }

    /// Fix 1 del Task 7 (C1, sostituisce `titolo_della_voce_espansa_va_a_capo`): un permesso si
    /// apre col titolo a capo (più righe, non tagliato in una) e il testo intero (comando e
    /// sessione) per Narrator e sullo schermo; la riga chiusa non offre «Consenti» ma «Apri».
    /// Ridisegno F2, Task 3: lo apre il componente `Permesso` del guscio (senza dati divisi:
    /// titolo e dettaglio), non più `VoceAperta`.
    #[test]
    fn permesso_aperto_si_legge_intero() {
        let (p, _) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_lungo(2)])));
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none(), "nella riga niente «Consenti»");
        assert_eq!(elemento(&p, "Apri").accessible_role(), Some(AccessibleRole::Button));
        elemento(&p, "Apri").invoke_accessible_default_action();
        lascia_girare(&p);
        let titolo = ElementHandle::find_by_element_id(&p, "Permesso::titolo").next().expect("il titolo del permesso");
        assert!(titolo.size().height > 30.0, "titolo lungo a capo: {}", titolo.size().height);
        let voce = ElementHandle::find_by_accessible_label(&p, &format!("Ti aspetta · Agenti: {}", permesso_lungo(2).titolo))
            .find(|e| e.accessible_role() == Some(AccessibleRole::Groupbox))
            .expect("il permesso aperto");
        let descrizione = voce.accessible_description().unwrap_or_default();
        assert!(descrizione.contains("cartella-1") && descrizione.ends_with("Sessione: progetto-a"), "{descrizione}");
        // Il testo sta in un riquadro visibile sotto il titolo, in mono.
        let dettaglio = ElementHandle::find_by_element_id(&p, "Permesso::dettaglio").next().expect("il testo intero");
        assert!(dettaglio.size().height > 0.0 && dettaglio.absolute_position().y >= titolo.absolute_position().y + titolo.size().height);
        let sorgente = include_str!("../ui/guscio/permesso.slint");
        let inizio = sorgente.find("dettaglio := Text").expect("il testo intero");
        assert!(sorgente[inizio..inizio + 500].contains("font-family: Tema.carattere-mono"), "il comando in mono");
    }

    /// Un clic sinistro (premuto e rilasciato) in questo punto della finestra.
    fn clic_in(p: &Pannello, x: f32, y: f32) {
        use slint::platform::{PointerEventButton, WindowEvent};
        let posizione = slint::LogicalPosition::new(x, y);
        p.window().dispatch_event(WindowEvent::PointerMoved { position: posizione });
        p.window().dispatch_event(WindowEvent::PointerPressed { position: posizione, button: PointerEventButton::Left });
        p.window().dispatch_event(WindowEvent::PointerReleased { position: posizione, button: PointerEventButton::Left });
    }

    /// Il centro di un elemento, in coordinate della finestra.
    fn centro(e: &ElementHandle) -> (f32, f32) {
        let (pos, lato) = (e.absolute_position(), e.size());
        (pos.x + lato.width / 2.0, pos.y + lato.height / 2.0)
    }

    /// Fix 2 del Task 7 (N1): nel permesso aperto «Nega» non si sposta mai. Prima del fondo del
    /// testo il posto di «Consenti» resta vuoto e inerte (nessun clic, nessun nome per Narrator);
    /// quando il fondo si vede «Consenti» nasce lì, e «Nega» è dove era. Un clic dove c'era «Nega»
    /// nega sempre.
    #[test]
    fn nega_non_si_sposta_quando_compare_consenti() {
        use slint::platform::Key;
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_lungo(12)])));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        let nega_prima = elemento(&p, "Nega");
        let (prima, lato_prima) = (nega_prima.absolute_position(), nega_prima.size());
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none());
        tasto(&p, Key::UpArrow);
        for _ in 0..40 {
            if ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_some() {
                break;
            }
            tasto(&p, Key::DownArrow);
            lascia_girare(&p);
        }
        let consenti = elemento(&p, "Consenti");
        let nega = elemento(&p, "Nega");
        assert_eq!((nega.absolute_position(), nega.size()), (prima, lato_prima), "«Nega» non si è spostato");
        // Il clic dove si era visto «Nega» nega.
        clic_in(&p, prima.x + lato_prima.width / 2.0, prima.y + lato_prima.height / 2.0);
        assert_eq!(*azioni.borrow(), ["nega"], "il clic dove era «Nega» nega");
        // «Consenti» non sta sopra «Nega».
        let (cx, _) = centro(&consenti);
        assert!(cx > prima.x + lato_prima.width || cx < prima.x, "«Consenti» accanto, non sopra");
    }

    /// Fix 2 del Task 7 (N1, caso secondario): un doppio clic su «Apri» di una riga bassa non
    /// concede mai: il secondo clic non trova «Consenti» dove c'era «Apri». Ridisegno F2, Task 3
    /// (F1 Task 7 n3): era `doppio_clic_su_apri_non_consente`; prova la posizione, il nome lo dice.
    #[test]
    fn consenti_non_nasce_sotto_apri() {
        let mut voci: Vec<VoceAttesaUi> =
            (0..3).map(|i| voce_coda(&format!("i{i}"), &format!("Avviso {i}"), LivelloUi::Informazione, "Quando vuoi · PC", false)).collect();
        voci.push(voce_coda("claude.permesso.9", "Permesso basso", LivelloUi::Bloccante, "Aspetta te · Agenti", true));
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(voci)));
        let apri = elemento(&p, "Apri");
        let (x, y) = centro(&apri);
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert!(permesso_aperto(&p), "il primo clic apre");
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert!(azioni.borrow().iter().all(|a| a != "consenti"), "il doppio clic ha concesso: {:?}", azioni.borrow());
        // Anche il posto di «Consenti» non è mai dove era «Apri».
        let consenti = elemento(&p, "Consenti");
        let (pos, lato) = (consenti.absolute_position(), consenti.size());
        let dentro = x >= pos.x && x <= pos.x + lato.width && y >= pos.y && y <= pos.y + lato.height;
        assert!(!dentro, "«Consenti» sta dove era «Apri»");
    }

    /// Fix 1 del Task 7 (C1, dec. 11): dalla pillola («Apri» o clic su un permesso) il pannello
    /// nasce sulla Coda col permesso già aperto e il testo intero, il fuoco su «Nega».
    #[test]
    fn dalla_pillola_il_permesso_si_apre_intero() {
        use slint::platform::Key;
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![
            voce_coda("casa", "Casa non raggiungibile", LivelloUi::Attenzione, "Guarda presto · Casa", false),
            permesso_lungo(2),
        ])));
        p.set_vista("panoramica".into());
        p.invoke_apri_coda_su(1);
        lascia_girare(&p);
        assert_eq!(p.get_vista(), "coda");
        assert!(permesso_aperto(&p), "il testo intero");
        tasto(&p, Key::Escape);
        tasto(&p, Key::Return);
        lascia_girare(&p);
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "riaperto da tastiera, il fuoco su «Nega»");
        assert!(azioni.borrow().is_empty());
    }

    /// Fix 1 del Task 7 (C1): col testo più lungo del riquadro «Consenti» non c'è (né col clic né
    /// con la tastiera né con Ctrl+Invio) finché il fondo del testo non si è visto; scorso il
    /// testo fino in fondo compare, e solo Ctrl+Invio concede.
    #[test]
    fn consenso_impossibile_prima_del_fondo_del_testo() {
        use slint::platform::Key;
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_lungo(12)])));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none(), "«Consenti» prima del fondo");
        assert!(ElementHandle::find_by_accessible_label(&p, "Leggi fino in fondo per consentire").next().is_some());
        // → e Ctrl+Invio non portano a nulla.
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "il consenso nascosto non si raggiunge");
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert!(azioni.borrow().is_empty(), "concesso senza aver letto: {:?}", azioni.borrow());
        // ↑ al testo, poi ↓ lo scorre fino in fondo; solo allora ↓ passa ai pulsanti.
        tasto(&p, Key::UpArrow);
        let mut passi = 0;
        while ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none() {
            assert_eq!(etichetta_col_fuoco(&p), format!("Ti aspetta · Agenti: {}", permesso_lungo(12).titolo), "sul testo");
            tasto(&p, Key::DownArrow);
            lascia_girare(&p);
            passi += 1;
            assert!(passi < 40, "il fondo del testo non arriva");
        }
        assert!(passi > 1, "il testo doveva scorrere: {passi} passi");
        assert!(azioni.borrow().is_empty());
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio da solo non concede");
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"]);
        // Esc richiude il permesso: di nuovo la riga, senza «Consenti».
        tasto(&p, Key::Escape);
        lascia_girare(&p);
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none());
        assert_eq!(p.get_vista(), "coda");
    }

    /// Ondata finale F1 (Task 7 n2): il fondo di un testo LUNGO riarma «Consenti» da solo. Aprire
    /// il permesso chiede un riarmo; scorrere fino in fondo, dove «Consenti» nasce, ne chiede un
    /// altro (i 600 ms ripartono da lì, non dall'apertura). Senza `consenti-compare` in
    /// `visto-fondo` la seconda richiesta manca.
    #[test]
    fn fondo_del_testo_lungo_riarma_il_consenso() {
        use slint::platform::Key;
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![permesso_lungo(12)])));
        let riarmi = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let r = riarmi.clone();
        p.on_consenti_compare(move || r.set(r.get() + 1));
        tasto(&p, Key::Return);
        lascia_girare(&p);
        let all_apertura = riarmi.get();
        assert_eq!(all_apertura, 1, "aprire il permesso riarma una volta");
        tasto(&p, Key::UpArrow);
        let mut passi = 0;
        while ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none() {
            assert_eq!(riarmi.get(), all_apertura, "riarmato prima del fondo (passo {passi})");
            tasto(&p, Key::DownArrow);
            lascia_girare(&p);
            passi += 1;
            assert!(passi < 40, "il fondo del testo non arriva");
        }
        assert!(passi > 1, "il testo doveva scorrere: {passi} passi");
        assert_eq!(riarmi.get(), all_apertura + 1, "il fondo del testo riarma «Consenti»");
        assert!(azioni.borrow().is_empty());
    }

    /// Ridisegno F1, Task 7 (C1, regola M3): «Nega» e «Consenti» sono entrambi neutri, senza
    /// tinta d'azione; la tinta resta all'ultimo pulsante di una voce che non è un permesso
    /// («Apri», «Riprova»). Sostituisce `nega_e_consenti_sono_neutri` (la voce espansa non c'è più).
    #[test]
    fn coda_nega_e_consenti_sono_neutri() {
        use crate::{RegolaPulsanti, TipoPulsante};
        piattaforma_di_prova();
        let p = pannello();
        let regola = p.global::<RegolaPulsanti>();
        // «Nega» (primo, non delicato) e «Consenti» (ultimo, delicato): nessuno ha la tinta.
        assert_eq!(regola.invoke_tipo(false, false), TipoPulsante::Neutro, "Nega");
        assert_eq!(regola.invoke_tipo(true, true), TipoPulsante::Neutro, "Consenti");
        // Un'azione sicura in coda la mantiene.
        assert_eq!(regola.invoke_tipo(true, false), TipoPulsante::Azione, "Apri");
        // La regola vale solo se la Coda e la pillola la usano davvero: se tornassero a decidere da
        // sé (ultimo = azione) questo test deve fallire.
        for (nome, sorgente) in [
            ("impianti/elenco.slint", include_str!("../ui/impianti/elenco.slint")),
            ("guscio/permesso.slint", include_str!("../ui/guscio/permesso.slint")),
            ("pillola.slint", include_str!("../ui/pillola.slint")),
            ("guscio/voce.slint", include_str!("../ui/guscio/voce.slint")),
        ] {
            assert!(sorgente.contains("RegolaPulsanti.tipo("), "{nome} non usa RegolaPulsanti.tipo");
            assert!(!sorgente.contains("TipoPulsante.azione"), "{nome} decide la tinta da sé");
        }
    }

    /// Ridisegno F1, Task 7 (Review Focus 2): il fuoco su «Consenti» dell'ultima riga, e la riga
    /// sparisce (permesso risolto nel terminale). Il fuoco va sulla riga precedente, non si perde,
    /// e il tasto ↑ seguente funziona.
    #[test]
    fn ultima_voce_tolta_col_fuoco_addosso() {
        use slint::platform::Key;
        let voci = std::rc::Rc::new(VecModel::from(vec![
            voce_coda("a", "Primo", LivelloUi::Bloccante, "Ti aspetta · Agenti", true),
            voce_coda("b", "Secondo", LivelloUi::Bloccante, "Ti aspetta · Agenti", true),
            voce_coda("c", "Terzo", LivelloUi::Bloccante, "Ti aspetta · Agenti", true),
        ]));
        let (p, azioni) = pannello_in_coda(ModelRc::from(voci.clone()));
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        // Il terzo si apre (Invio), → su «Consenti».
        tasto(&p, Key::Return);
        lascia_girare(&p);
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        voci.remove(2);
        // I `changed` girano al giro seguente del ciclo d'eventi.
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Secondo, Ti aspetta · Agenti", "il fuoco va sulla riga precedente");
        assert_eq!(ruolo_col_fuoco(&p), i_slint_core::items::AccessibleRole::ListItem, "sulla riga, non sul permesso chiuso");
        // Una riga che arriva dopo non si prende il fuoco da sola.
        voci.push(voce_coda("d", "Quarto", LivelloUi::Bloccante, "Ti aspetta · Agenti", true));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Secondo, Ti aspetta · Agenti", "il fuoco resta dov'era");
        tasto(&p, Key::UpArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Primo, Ti aspetta · Agenti", "il tasto seguente non si perde");
        // Una voce diversa al posto di quella aperta (stesso numero di righe): il permesso si
        // chiude, il fuoco lascia «Consenti» e torna sulla riga, mai sul consenso di un'altra
        // richiesta.
        tasto(&p, Key::Return);
        lascia_girare(&p);
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        voci.set_row_data(0, voce_coda("z", "Un altro", LivelloUi::Bloccante, "Ti aspetta · Agenti", true));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Un altro, Ti aspetta · Agenti");
        assert_eq!(ruolo_col_fuoco(&p), i_slint_core::items::AccessibleRole::ListItem, "la riga, il permesso si è chiuso");
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert!(azioni.borrow().is_empty(), "Ctrl+Invio sulla riga non concede: {:?}", azioni.borrow());
    }

    /// Ridisegno F1, Task 7: la Coda scorre; la riga col fuoco sotto il bordo viene portata in
    /// vista, e risalendo anche la prima torna dentro.
    #[test]
    fn coda_porta_in_vista_la_riga_col_fuoco() {
        use slint::platform::Key;
        let (p, _) = pannello_in_coda(voci_attesa(8));
        let lista = elemento(&p, "In attesa");
        assert_eq!(lista.accessible_role(), Some(AccessibleRole::List));
        let (cima, fondo) = (lista.absolute_position().y, lista.absolute_position().y + lista.size().height);
        // Una riga ritagliata fuori dalla lista non si trova affatto: è fuori vista.
        let dentro = |i: usize| {
            ElementHandle::find_by_accessible_label(&p, &format!("Titolo {i}, Quando vuoi · prova")).next().is_some_and(|r| {
                let y = r.absolute_position().y;
                y >= cima - 0.5 && y + r.size().height <= fondo + 0.5
            })
        };
        assert!(!dentro(7), "l'ultima riga parte fuori vista");
        for _ in 0..8 {
            tasto(&p, Key::DownArrow);
            slint::platform::update_timers_and_animations();
        }
        assert_eq!(etichetta_col_fuoco(&p), "Titolo 7, Quando vuoi · prova");
        assert!(dentro(7), "la riga col fuoco è in vista");
        for _ in 0..8 {
            tasto(&p, Key::UpArrow);
            slint::platform::update_timers_and_animations();
        }
        assert!(dentro(0), "risalendo la prima torna in vista");
    }

    /// Ridisegno F1, Task 7: la Coda senza voci dice «Niente in attesa» e offre «Torna alla
    /// Panoramica» (clic, Narrator e tastiera).
    #[test]
    fn coda_vuota_ha_lo_stato_vuoto() {
        use slint::platform::Key;
        let (p, _) = pannello_in_coda(ModelRc::new(VecModel::from(Vec::<VoceAttesaUi>::new())));
        assert_eq!(elemento(&p, "Niente in attesa").accessible_role(), Some(AccessibleRole::Text));
        let torna = elemento(&p, "Torna alla Panoramica");
        assert_eq!(torna.accessible_role(), Some(AccessibleRole::Button));
        torna.invoke_accessible_default_action();
        assert_eq!(p.get_vista(), "panoramica");
        // Da tastiera: il fuoco è sul pulsante, Invio torna.
        p.set_vista("coda".into());
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Torna alla Panoramica");
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "panoramica");
    }

    /// Ridisegno F1, Task 7: ogni riga della Coda è una voce d'elenco col nome «{titolo},
    /// {livello} · {modulo}» e il testo completo come descrizione; la testata si legge come testo.
    #[test]
    fn coda_righe_accessibili() {
        let (p, _) = pannello_in_coda(ModelRc::new(VecModel::from(vec![
            voce_coda("a", "Claude chiede: Comando ls", LivelloUi::Bloccante, "Ti aspetta · Agenti", true),
            voce_coda("b", "Casa non raggiungibile", LivelloUi::Attenzione, "Guarda presto · Casa", false),
            voce_coda("c", "Disco quasi pieno", LivelloUi::Informazione, "Quando vuoi · PC", false),
        ])));
        p.set_moduli_in_attesa(3);
        for (i, nome) in ["Claude chiede: Comando ls, Ti aspetta · Agenti", "Casa non raggiungibile, Guarda presto · Casa", "Disco quasi pieno, Quando vuoi · PC"]
            .iter()
            .enumerate()
        {
            let riga = elemento(&p, nome);
            assert_eq!(riga.accessible_role(), Some(AccessibleRole::ListItem), "{nome}");
            assert_eq!(riga.accessible_item_index(), Some(i), "{nome}");
            assert_eq!(riga.accessible_description().as_deref(), Some("Il testo completo"), "{nome}");
        }
        assert_eq!(elemento(&p, "3 in attesa").accessible_role(), Some(AccessibleRole::Text));
        assert_eq!(elemento(&p, "da 3 moduli").accessible_role(), Some(AccessibleRole::Text));
        p.set_moduli_in_attesa(1);
        assert_eq!(elemento(&p, "da 1 modulo").accessible_role(), Some(AccessibleRole::Text));
    }

    // ─── Ridisegno F1, Task 8: Panoramica a griglia, voce che ti aspetta, pool di fuoco ───

    /// I quattro moduli di un registro nuovo (primo avvio), come li scrive il ponte.
    fn moduli_di_prova() -> ModelRc<crate::ModuloUi> {
        ModelRc::new(VecModel::from(crate::moduli::registro::Registro::nuovo(&[]).moduli_ui()))
    }

    /// Un modulo di prova: nome per Narrator «{nome}, {titolo}», l'azione «{azione}, {nome}».
    fn modulo(id: &str, nome: &str, titolo: &str, azione: Option<&str>) -> crate::ModuloUi {
        crate::ModuloUi {
            id: id.into(),
            nome: nome.into(),
            nome_breve: nome.into(),
            titolo: titolo.into(),
            sec: "Riga di prova".into(),
            con_azione: azione.is_some(),
            azione_id: azione.unwrap_or_default().into(),
            azione_testo: azione.unwrap_or_default().into(),
            narratore: format!("{nome}, {titolo}").into(),
            narratore_azione: azione.map(|a| format!("{a}, {nome}")).unwrap_or_default().into(),
            ..Default::default()
        }
    }

    /// Agenti (azione), Casa (azione), PC (senza), Collegamenti (azione).
    fn quattro_moduli() -> std::rc::Rc<VecModel<crate::ModuloUi>> {
        std::rc::Rc::new(VecModel::from(vec![
            modulo("agenti", "Agenti", "2 al lavoro", Some("Apri il terminale")),
            modulo("casa", "Casa", "4 luci", Some("Spegni tutte")),
            modulo("pc", "PC", "Tutto regolare", None),
            modulo("collegamenti", "Collegamenti", "6 link", Some("Apri NAS")),
        ]))
    }

    /// Il pannello sulla Panoramica con questi moduli, l'ora «09:45», la tastiera pronta.
    fn pannello_panoramica(moduli: ModelRc<crate::ModuloUi>) -> Pannello {
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.set_moduli(moduli);
        crate::pannello::scrivi_ora(&p, "09:45");
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p
    }

    /// I quattro posti delle carte (visibili), nell'ordine.
    fn posti(p: &Pannello) -> Vec<ElementHandle> {
        let mut v: Vec<ElementHandle> =
            ElementHandle::find_by_element_type_name(p, "Posto").filter(|e| e.size().width > 0.0).collect();
        v.sort_by(|a, b| a.absolute_position().x.total_cmp(&b.absolute_position().x));
        v
    }

    /// L'elemento `id` dentro il posto della carta `k`.
    fn nella_carta(p: &Pannello, k: usize, id: &str) -> ElementHandle {
        let posto = &posti(p)[k];
        let (x0, x1) = (posto.absolute_position().x, posto.absolute_position().x + posto.size().width);
        ElementHandle::find_by_element_id(p, id)
            .find(|e| e.absolute_position().x >= x0 - 0.5 && e.absolute_position().x < x1)
            .unwrap_or_else(|| panic!("nessun {id} nella carta {k}"))
    }

    #[test]
    fn bersagli_uguali_in_rust_e_slint() {
        use crate::moduli::panoramica as r;
        piattaforma_di_prova();
        let p = pannello();
        let b = p.global::<crate::Bersagli>();
        assert_eq!(
            (b.get_nega(), b.get_consenti(), b.get_altre(), b.get_scheda(), b.get_ricerca(), b.get_volume(), b.get_menu(), b.get_in_attesa()),
            (r::ID_NEGA, r::ID_CONSENTI, r::ID_ALTRE, r::ID_SCHEDA, r::ID_RICERCA, r::ID_VOLUME, r::ID_MENU, r::ID_IN_ATTESA)
        );
    }

    /// Con quattro moduli: l'orologio grande in alto, le carte in una riga, e nessun elemento
    /// della griglia scende nella fascia della barra (y ≥ 276).
    #[test]
    fn panoramica_griglia_con_quattro_moduli() {
        let p = pannello_panoramica(moduli_di_prova());
        let orologio = elemento(&p, "09:45");
        assert_eq!(orologio.accessible_role(), Some(AccessibleRole::Text));
        assert!(orologio.size().height > 0.0 && orologio.absolute_position().y >= 44.0);
        assert_eq!(posti(&p).len(), 4);
        for tipo in ["Posto", "CartaVista", "OrologioGrande", "VoceInAttesa", "Text", "Image"] {
            for e in ElementHandle::find_by_element_type_name(&p, tipo).filter(|e| e.size().height > 0.0) {
                let fondo = e.absolute_position().y + e.size().height;
                // Gli elementi della barra stanno sotto 276 per costruzione: qui solo quelli del corpo.
                if e.absolute_position().y < 276.0 {
                    assert!(fondo <= 276.0 + 0.01, "{tipo} scende nella barra: fondo {fondo}");
                }
            }
        }
        let fondo_carte = posti(&p).iter().map(|e| e.absolute_position().y + e.size().height).fold(0.0, f32::max);
        assert!(fondo_carte <= 276.0, "carte sopra la barra: {fondo_carte}");
    }

    /// Le quattro carte sono alla stessa y, larghe 182, alte 108, a passo 190: al primo avvio,
    /// con dati calmi e con titoli al limite.
    #[test]
    fn carte_tutte_della_stessa_misura() {
        let lunghi = std::rc::Rc::new(VecModel::from(vec![
            modulo("agenti", "Agenti", &"W".repeat(40), Some("Apri il terminale")),
            modulo("casa", "Casa", &"Mmmmmmmm ".repeat(5), Some("Spegni tutte")),
            modulo("pc", "PC", "CPU 100 %, RAM 100 %, disco pieno", None),
            modulo("collegamenti", "Collegamenti", &"Collegamento lunghissimo".repeat(3), Some("Apri")),
        ]));
        for (nome, moduli) in
            [("primo avvio", moduli_di_prova()), ("calmi", ModelRc::from(quattro_moduli())), ("al limite", ModelRc::from(lunghi))]
        {
            let p = pannello_panoramica(moduli);
            let posti = posti(&p);
            assert_eq!(posti.len(), 4, "{nome}");
            let y = posti[0].absolute_position().y;
            for (k, posto) in posti.iter().enumerate() {
                assert_eq!(posto.absolute_position().y, y, "{nome}: carta {k} alla stessa y");
                assert_eq!((posto.size().width, posto.size().height), (182.0, 108.0), "{nome}: carta {k}");
                if k > 0 {
                    assert_eq!(posto.absolute_position().x - posti[k - 1].absolute_position().x, 190.0, "{nome}: passo");
                }
            }
            // Il gruppo è centrato nel corpo (34..786).
            let (sinistra, destra) = (posti[0].absolute_position().x, posti[3].absolute_position().x + 182.0);
            assert!((sinistra - 34.0 - (786.0 - destra)).abs() < 0.5, "{nome}: centrato {sinistra}..{destra}");
        }
    }

    /// A riposo nessun bottone d'azione si vede (valore e riga larghi 154); Tab dalla carta 0
    /// porta alla sua azione (che si vede: valore e riga a 116), poi alla carta 1; la carta PC,
    /// senza azione, non ha nessun nodo d'azione per Narrator; Maiusc+Tab torna indietro.
    #[test]
    fn azione_al_passaggio_o_al_fuoco() {
        use slint::platform::Key;
        let p = pannello_panoramica(ModelRc::from(quattro_moduli()));
        for k in 0..4 {
            assert_eq!(nella_carta(&p, k, "CartaVista::riga").size().width, 154.0, "carta {k} a riposo");
        }
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Agenti, 2 al lavoro");
        assert_eq!(nella_carta(&p, 0, "CartaVista::riga").size().width, 116.0, "col fuoco sulla carta l'azione si vede");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Apri il terminale, Agenti");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Casa, 4 luci");
        assert_eq!(nella_carta(&p, 0, "CartaVista::riga").size().width, 154.0, "lasciata la carta, l'azione sparisce");
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "PC, Tutto regolare");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Collegamenti, 6 link", "la carta PC salta il secondo passo");
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "PC, Tutto regolare");
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Spegni tutte, Casa");
        // Narrator: nessun pulsante senza nome («Azione: » vuoto) e nessun nodo «…, PC».
        assert_eq!(pulsanti_senza_nome(&p), 0);
        let nodi_pc = i_slint_backend_testing::ElementQuery::from_root(&p)
            .match_descendants()
            .find_all()
            .into_iter()
            .filter(|e| e.accessible_label().is_some_and(|l| l.ends_with(", PC")))
            .count();
        assert_eq!(nodi_pc, 0, "la carta PC non ha azione per Narrator");
        // Il passaggio del mouse mostra solo l'azione di quella carta.
        let (x, y) = centro(&posti(&p)[1]);
        p.window().dispatch_event(slint::platform::WindowEvent::PointerMoved { position: slint::LogicalPosition::new(x, y) });
        assert_eq!(nella_carta(&p, 1, "CartaVista::riga").size().width, 116.0, "al passaggio sulla carta Casa");
        assert_eq!(nella_carta(&p, 3, "CartaVista::riga").size().width, 154.0);
    }

    /// Review Focus 1: con l'azione visibile valore e riga finiscono prima del bottone
    /// (x < 182 − 14 − 32 − 6 dalla carta), con l'ellissi; senza azione hanno tutto lo spazio.
    #[test]
    fn carta_con_testi_lunghi_ha_ellissi_e_azione_libera() {
        use slint::platform::Key;
        let p = pannello_panoramica(ModelRc::new(VecModel::from(vec![
            crate::ModuloUi { sec: "W".repeat(40).into(), ..modulo("agenti", "Agenti", &"W".repeat(40), Some("Apri il terminale")) },
            modulo("casa", "Casa", "4 luci", None),
        ])));
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), format!("Agenti, {}", "W".repeat(40)), "il nome intero per Narrator");
        let x0 = posti(&p)[0].absolute_position().x;
        let limite = x0 + 182.0 - 14.0 - 32.0 - 6.0;
        for id in ["CartaVista::valore", "CartaVista::riga"] {
            let e = nella_carta(&p, 0, id);
            let fine = e.absolute_position().x + e.size().width;
            assert!(fine <= limite + 0.01, "{id} finisce a {fine}, oltre {limite}");
        }
        let valore = nella_carta(&p, 0, "CartaVista::valore");
        assert_ne!(valore.accessible_role(), Some(AccessibleRole::Text), "il testo della carta non è un nodo per Narrator");
        // Senza azione (carta Casa) tutto lo spazio: 154.
        assert_eq!(nella_carta(&p, 1, "CartaVista::valore").size().width, 154.0);
    }

    /// Un permesso che ti aspetta, col testo intero in una riga (si concede nella voce).
    fn permesso_corto() -> VoceAttesaUi {
        VoceAttesaUi { dettaglio: "npm test".into(), ..voce_coda("p1", "Claude chiede: PowerShell npm test", LivelloUi::Bloccante, "Aspetta te · Agenti", true) }
    }

    /// La Panoramica con le carte e queste voci in attesa; restituisce i pulsanti premuti della
    /// voce che ti aspetta (`azione-voce`).
    fn panoramica_con_voci(voci: ModelRc<VoceAttesaUi>) -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        let p = pannello_panoramica(ModelRc::from(quattro_moduli()));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione_voce(move |_, id| a.borrow_mut().push(id.to_string()));
        let a = azioni.clone();
        p.on_azione(move |_, id| a.borrow_mut().push(format!("coda {id}")));
        p.set_attese(voci);
        slint::platform::update_timers_and_animations();
        (p, azioni)
    }

    /// Con una voce bloccante: l'orologio grande lascia il posto alla voce, l'ora piccola torna
    /// nella riga alta, il fuoco iniziale è su «Nega»; le carte non si spostano.
    #[test]
    fn voce_al_posto_dell_orologio() {
        let p = pannello_panoramica(ModelRc::from(quattro_moduli()));
        let prima: Vec<_> = posti(&p).iter().map(|e| (e.absolute_position(), e.size())).collect();
        assert!(!p.get_ora_riga_visibile(), "con l'orologio grande niente ora piccola");
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_corto()]));
        p.set_attese(ModelRc::from(voci.clone()));
        slint::platform::update_timers_and_animations();
        assert!(ElementHandle::find_by_accessible_label(&p, "09:45").all(|e| e.absolute_position().y < 44.0), "niente orologio grande, solo l'ora piccola");
        let voce = elemento(&p, "Ti aspetta · Agenti: Claude chiede: PowerShell npm test");
        assert_eq!(voce.accessible_role(), Some(AccessibleRole::Groupbox));
        assert_eq!(voce.accessible_description().as_deref(), Some("npm test"));
        assert_eq!((voce.size().width, voce.size().height), (752.0, 108.0));
        assert!(p.get_ora_riga_visibile(), "l'ora piccola torna nella riga alta");
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "fuoco iniziale su «Nega»");
        let dopo: Vec<_> = posti(&p).iter().map(|e| (e.absolute_position(), e.size())).collect();
        assert_eq!(prima, dopo, "le carte non si spostano");
        // Tolta la voce, torna l'orologio.
        voci.remove(0);
        slint::platform::update_timers_and_animations();
        assert!(ElementHandle::find_by_accessible_label(&p, "09:45").any(|e| e.absolute_position().y >= 44.0), "l'orologio grande torna");
        assert!(!p.get_ora_riga_visibile());
    }

    /// La voce che ti aspetta col fuoco su «Consenti»: (Tab da «Nega»).
    fn voce_sul_consenso() -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        use slint::platform::Key;
        let (p, azioni) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![permesso_corto()])));
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti", "testo intero nella voce: «Consenti»");
        (p, azioni)
    }

    /// Le stesse 9 combinazioni della Coda: col fuoco su «Consenti» della voce solo Ctrl+Invio
    /// esatto concede.
    #[test]
    fn voce_consenti_solo_con_ctrl_invio_esatto() {
        use slint::platform::Key;
        let casi: [(&str, &[Key], Key); 9] = [
            ("Invio", &[], Key::Return),
            ("Spazio", &[], Key::Space),
            ("Alt+Invio", &[Key::Alt], Key::Return),
            ("Ctrl+Spazio", &[Key::Control], Key::Space),
            ("Maiusc+Invio", &[Key::Shift], Key::Return),
            ("Alt+Spazio", &[Key::Alt], Key::Space),
            ("Ctrl+Alt+Invio (qui arriva come Invio semplice)", &[Key::Control, Key::Alt], Key::Return),
            ("AltGr+Invio", &[Key::AltGr], Key::Return),
            ("Maiusc+Ctrl+Invio", &[Key::Shift, Key::Control], Key::Return),
        ];
        for (nome, modificatori, t) in casi {
            let (p, azioni) = voce_sul_consenso();
            con_modificatori(&p, modificatori, t);
            assert!(azioni.borrow().is_empty(), "{nome} ha concesso: {:?}", azioni.borrow());
        }
        let (p, azioni) = voce_sul_consenso();
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"], "Ctrl+Invio concede");
        // Ctrl+Invio su «Nega» non fa nulla (non concede, non nega).
        let (p, azioni) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![permesso_corto()])));
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert!(azioni.borrow().is_empty(), "{:?}", azioni.borrow());
    }

    /// Invio su «Consenti» della voce non concede; Invio su «Nega» nega. Un permesso col testo più
    /// lungo della voce non offre «Consenti» ma «Apri»: Invio lo apre nella Coda (testo intero,
    /// fuoco su «Nega»), senza rispondere.
    #[test]
    fn voce_invio_su_consenti_non_concede() {
        use slint::platform::Key;
        let (p, azioni) = voce_sul_consenso();
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio su «Consenti» ha concesso: {:?}", azioni.borrow());
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        tasto(&p, Key::Return);
        assert_eq!(*azioni.borrow(), ["nega"], "Invio su «Nega» nega");
        // Testo lungo (comando e sessione): «Apri», mai «Consenti».
        let (p, azioni) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![permesso_lungo(2)])));
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none(), "nella voce niente «Consenti»");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Apri");
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert!(azioni.borrow().is_empty(), "Ctrl+Invio su «Apri» non concede");
        tasto(&p, Key::Return);
        lascia_girare(&p);
        assert_eq!(p.get_vista(), "coda");
        assert!(permesso_aperto(&p), "il testo intero");
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        assert!(azioni.borrow().is_empty(), "{:?}", azioni.borrow());
    }

    /// Requisito del Task 7 (rereview-2 n1): un doppio clic su «Apri» della voce (o su «+N») non
    /// preme nulla: il secondo clic non trova «Nega» né «Consenti» del permesso aperto, né un
    /// pulsante di una riga della Coda.
    #[test]
    fn doppio_clic_sulla_voce_non_risponde() {
        let voci = || ModelRc::new(VecModel::from(vec![permesso_lungo(2), voce_coda("b", "Secondo", LivelloUi::Bloccante, "Aspetta te · Agenti", true)]));
        let (p, azioni) = panoramica_con_voci(voci());
        let (x, y) = centro(&elemento(&p, "Apri"));
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert!(permesso_aperto(&p), "il primo clic apre");
        for nome in ["Nega", "Consenti"] {
            for e in ElementHandle::find_by_accessible_label(&p, nome) {
                let (pos, lato) = (e.absolute_position(), e.size());
                assert!(!(x >= pos.x && x <= pos.x + lato.width && y >= pos.y && y <= pos.y + lato.height), "«{nome}» sotto il punto di «Apri»");
            }
        }
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert!(azioni.borrow().is_empty(), "il doppio clic su «Apri» ha risposto: {:?}", azioni.borrow());
        // «+N»: la Coda come elenco, nessun permesso aperto, il secondo clic non preme nulla.
        let (p, azioni) = panoramica_con_voci(voci());
        let (x, y) = centro(&elemento(&p, "Un'altra voce in attesa"));
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert_eq!(p.get_vista(), "coda");
        assert!(!permesso_aperto(&p), "«+N» non apre un permesso");
        clic_in(&p, x, y);
        lascia_girare(&p);
        assert!(azioni.borrow().is_empty(), "il doppio clic su «+N» ha risposto: {:?}", azioni.borrow());
    }

    /// Ondata finale F1 (m1): «+N in attesa ›», a fine arrivo e per intero, sta ad almeno 8 px
    /// da ogni «Nega» delle righe della Coda che apre: un doppio clic sul suo bordo, con qualche
    /// pixel di deriva, non preme il «Nega» (non armato) di una riga. Nella Griglia e nell'Orologio.
    #[test]
    fn altre_in_attesa_lontano_dai_nega_della_coda() {
        let voci = || {
            ModelRc::new(VecModel::from(vec![
                permesso_corto(),
                voce_coda("b", "Secondo", LivelloUi::Bloccante, "Aspetta te · Agenti", true),
                voce_coda("c", "Terzo", LivelloUi::Bloccante, "Aspetta te · Agenti", true),
            ]))
        };
        for orologio in [false, true] {
            let p = pannello_panoramica(ModelRc::from(quattro_moduli()));
            if orologio {
                p.set_panoramica(1);
                crate::pannello::scrivi_fascia(&p, 0);
            }
            p.set_attese(voci());
            slint::platform::update_timers_and_animations();
            avanza(400);
            let altre = elemento(&p, "Altre 2 voci in attesa");
            let (a, la) = (altre.absolute_position(), altre.size());
            assert!(la.width > 0.0 && la.height > 0.0);
            p.set_vista("coda".into());
            slint::platform::update_timers_and_animations();
            avanza(400);
            let negati: Vec<ElementHandle> = ElementHandle::find_by_accessible_label(&p, "Nega").collect();
            assert!(negati.len() >= 2, "orologio={orologio}: i «Nega» delle righe della Coda");
            for nega in negati {
                let (n, ln) = (nega.absolute_position(), nega.size());
                let dx = (n.x - (a.x + la.width)).max(a.x - (n.x + ln.width)).max(0.0);
                let dy = (n.y - (a.y + la.height)).max(a.y - (n.y + ln.height)).max(0.0);
                assert!(
                    dx.max(dy) >= 8.0,
                    "orologio={orologio}: «+N» ({}, {}, {}×{}) a {dx}/{dy} px da «Nega» ({}, {}, {}×{})",
                    a.x, a.y, la.width, la.height, n.x, n.y, ln.width, ln.height
                );
            }
        }
    }

    /// Ondata finale F1 (Task 8 m3): una voce con un solo pulsante, delicato. Il posto di «Nega»
    /// (100, col fuoco iniziale) resta vuoto e il pulsante va sull'altro posto; col testo che non
    /// sta in una riga è «Apri», mai «Consenti», e nessun tasto concede; col testo intero il
    /// fuoco iniziale non è sul consenso e Invio non concede.
    #[test]
    fn voce_con_solo_un_pulsante_delicato() {
        use slint::platform::Key;
        let solo_consenti = |mut v: VoceAttesaUi| {
            v.pulsanti = ModelRc::new(VecModel::from(vec![PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true }]));
            v
        };
        // Testo lungo: «Apri».
        let (p, azioni) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![solo_consenti(permesso_lungo(2))])));
        assert!(ElementHandle::find_by_accessible_label(&p, "Consenti").next().is_none(), "nella voce niente «Consenti»");
        assert_eq!(ElementHandle::find_by_accessible_label(&p, "Apri").count(), 1, "un «Apri» solo");
        assert_ne!(etichetta_col_fuoco(&p), "Consenti");
        for _ in 0..3 {
            con_modificatori(&p, &[Key::Control], Key::Return);
            tasto(&p, Key::Tab);
        }
        assert!(azioni.borrow().iter().all(|a| a != "consenti"), "concesso senza leggere: {:?}", azioni.borrow());
        // Testo intero: «Consenti» sull'altro posto, non sul fuoco iniziale.
        let (p, azioni) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![solo_consenti(permesso_corto())])));
        assert_eq!(ElementHandle::find_by_accessible_label(&p, "Consenti").count(), 1);
        assert_ne!(etichetta_col_fuoco(&p), "Consenti", "il fuoco iniziale sul consenso");
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio ha risposto: {:?}", azioni.borrow());
    }

    /// La voce `v` con un solo pulsante, delicato («Consenti»).
    fn solo_delicato(id: &str, v: VoceAttesaUi) -> VoceAttesaUi {
        VoceAttesaUi {
            id: id.into(),
            pulsanti: ModelRc::new(VecModel::from(vec![PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true }])),
            ..v
        }
    }

    /// Una voce bloccante qualunque dopo la prima (per «+1 in attesa»).
    fn seconda_voce() -> VoceAttesaUi {
        voce_coda("b2", "Secondo", LivelloUi::Bloccante, "Aspetta te · Agenti", true)
    }

    /// Rimandato n1 di F1: col fuoco su «Nega» della voce A, il modello mette al suo posto una
    /// voce B con il solo pulsante delicato. Il posto di «Nega» resta vuoto: il fuoco non si perde
    /// ma va su «Apri» (testo lungo), su «+1 in attesa» o sulla carta 0 (testo intero: «Consenti»
    /// visibile, mai col fuoco addosso). La tastiera regge: Esc chiude, Tab va avanti.
    #[test]
    fn voce_sostituita_da_una_con_solo_pulsante_delicato_tiene_il_fuoco() {
        use slint::platform::Key;
        // B col testo lungo: il fuoco su «Apri», Esc chiude subito il pannello.
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_corto()]));
        let (p, azioni) = panoramica_con_voci(ModelRc::from(voci.clone()));
        let chiuso = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiuso.clone();
        p.on_chiudi(move || c.set(true));
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        voci.set_row_data(0, solo_delicato("b", permesso_lungo(2)));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Apri", "il fuoco va su «Apri», non si perde");
        tasto(&p, Key::Escape);
        assert!(chiuso.get(), "la tastiera regge: Esc chiude il pannello");
        // Lo stesso, poi Tab: la carta 0.
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_corto()]));
        let (p, _) = panoramica_con_voci(ModelRc::from(voci.clone()));
        voci.set_row_data(0, solo_delicato("b", permesso_lungo(2)));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Apri");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Agenti, 2 al lavoro", "Tab da «Apri» alla carta 0");
        // B col testo intero («Consenti» visibile) e una seconda voce: «+1 in attesa».
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_corto(), seconda_voce()]));
        let (p, _) = panoramica_con_voci(ModelRc::from(voci.clone()));
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        voci.set_row_data(0, solo_delicato("b", permesso_corto()));
        slint::platform::update_timers_and_animations();
        assert_eq!(ElementHandle::find_by_accessible_label(&p, "Consenti").count(), 1, "«Consenti» visibile");
        assert_eq!(etichetta_col_fuoco(&p), "Un'altra voce in attesa", "il fuoco su «+1 in attesa», mai sul consenso");
        // Senza la seconda voce: la carta 0.
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_corto()]));
        let (p, _) = panoramica_con_voci(ModelRc::from(voci.clone()));
        voci.set_row_data(0, solo_delicato("b", permesso_corto()));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Agenti, 2 al lavoro", "il fuoco sulla carta 0, mai sul consenso");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Apri il terminale, Agenti", "il Tab seguente non si perde");
        // Senza carte né altre voci nessun bersaglio: la tastiera resta al pannello, Esc chiude.
        let p = pannello_panoramica(ModelRc::new(VecModel::from(Vec::<crate::ModuloUi>::new())));
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_corto()]));
        p.set_attese(ModelRc::from(voci.clone()));
        slint::platform::update_timers_and_animations();
        let chiuso = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiuso.clone();
        p.on_chiudi(move || c.set(true));
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        voci.set_row_data(0, solo_delicato("b", permesso_corto()));
        slint::platform::update_timers_and_animations();
        assert_ne!(etichetta_col_fuoco(&p), "Consenti");
        tasto(&p, Key::Escape);
        assert!(chiuso.get(), "senza bersaglio la tastiera regge: Esc chiude il pannello");
        // Pre-flight C20: col fuoco su «Apri» la stessa voce si aggiorna e il testo ora sta tutto
        // («Apri» diventa «Consenti»): il fuoco lascia il posto e «Consenti» si riarma.
        let voci = std::rc::Rc::new(VecModel::from(vec![solo_delicato("b", permesso_lungo(2)), seconda_voce()]));
        let (p, _) = panoramica_con_voci(ModelRc::from(voci.clone()));
        let armamenti = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let a = armamenti.clone();
        p.on_consenti_compare(move || a.set(a.get() + 1));
        assert_eq!(etichetta_col_fuoco(&p), "Apri", "la voce che compare: fuoco su «Apri»");
        voci.set_row_data(0, solo_delicato("b", permesso_corto()));
        slint::platform::update_timers_and_animations();
        assert_eq!(ElementHandle::find_by_accessible_label(&p, "Consenti").count(), 1);
        assert_eq!(etichetta_col_fuoco(&p), "Un'altra voce in attesa", "«Apri» diventato «Consenti»: il fuoco se ne va");
        assert_eq!(armamenti.get(), 1, "«Consenti» comparso: si riarma");
        assert!(azioni.borrow().is_empty(), "{:?}", azioni.borrow());
    }

    /// La voce con il solo pulsante delicato che compare su una Panoramica senza voce: stesso
    /// bersaglio di `voce_sostituita_da_una_con_solo_pulsante_delicato_tiene_il_fuoco`.
    #[test]
    fn voce_che_compare_con_solo_pulsante_delicato() {
        use slint::platform::Key;
        let casi: [(&str, Vec<VoceAttesaUi>, &str); 3] = [
            ("testo lungo", vec![solo_delicato("b", permesso_lungo(2))], "Apri"),
            ("testo intero e una seconda voce", vec![solo_delicato("b", permesso_corto()), seconda_voce()], "Un'altra voce in attesa"),
            ("testo intero", vec![solo_delicato("b", permesso_corto())], "Agenti, 2 al lavoro"),
        ];
        for (nome, nuove, atteso) in casi {
            let voci = std::rc::Rc::new(VecModel::from(Vec::<VoceAttesaUi>::new()));
            let (p, azioni) = panoramica_con_voci(ModelRc::from(voci.clone()));
            // Il fuoco sulla carta 1, poi la voce compare.
            tasto(&p, Key::Tab);
            tasto(&p, Key::Tab);
            tasto(&p, Key::Tab);
            assert_eq!(etichetta_col_fuoco(&p), "Casa, 4 luci", "{nome}");
            for v in nuove {
                voci.push(v);
            }
            slint::platform::update_timers_and_animations();
            assert_eq!(etichetta_col_fuoco(&p), atteso, "{nome}");
            tasto(&p, Key::Return);
            assert!(azioni.borrow().iter().all(|a| a != "consenti"), "{nome}: {:?}", azioni.borrow());
        }
    }

    /// Rimandato n2 di F1: con il solo «Apri» (il posto di «Nega» vuoto) titolo e riquadro del
    /// dettaglio finiscono 12 px prima di «Apri», non ci passano sotto.
    #[test]
    fn testo_della_voce_non_passa_sotto_apri() {
        let (p, _) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![solo_delicato("b", permesso_lungo(2))])));
        let apri = elemento(&p, "Apri");
        let limite = apri.absolute_position().x - 12.0;
        for id in ["VoceInAttesa::titolo", "VoceInAttesa::scatola"] {
            let e = ElementHandle::find_by_element_id(&p, id).next().unwrap_or_else(|| panic!("nessun {id}"));
            let fine = e.absolute_position().x + e.size().width;
            assert!(fine <= limite + 0.01, "{id} finisce a {fine}, oltre {limite}");
            assert!(e.size().width > 0.0, "{id} vuoto");
        }
    }

    /// F1 Task 8 m5: con due pulsanti il titolo della voce è largo 476 px (752 − 16 − 112 − 8 −
    /// 112 − 12 − 16) e ci sta «Claude chiede: » più lo strumento più lungo che la pillola mostra
    /// (`MASSIMO_STRUMENTO` caratteri, «W» il più largo).
    #[test]
    fn titolo_della_voce_con_strumento_al_massimo() {
        use crate::moduli::misura::{larghezza, Stile};
        let (p, _) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![permesso_corto()])));
        let titolo = ElementHandle::find_by_element_id(&p, "VoceInAttesa::titolo").next().expect("titolo della voce");
        assert_eq!(titolo.size().width, 476.0, "il titolo della voce con due pulsanti");
        let piu_lungo = format!("Claude chiede: {}", "W".repeat(crate::claude::MASSIMO_STRUMENTO));
        let l = larghezza(&piu_lungo, Stile::Titolo);
        assert!(l <= titolo.size().width, "«{piu_lungo}» largo {l} px non sta in {}", titolo.size().width);
    }

    /// Review Focus 2: fuoco su «Consenti», la voce tolta da Rust (permesso risolto nel
    /// terminale): il fuoco va sulla carta 0 e il Tab seguente alla sua azione.
    #[test]
    fn voce_che_sparisce_col_fuoco_addosso() {
        use slint::platform::Key;
        let voci = std::rc::Rc::new(VecModel::from(vec![permesso_corto()]));
        let (p, _) = panoramica_con_voci(ModelRc::from(voci.clone()));
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        voci.remove(0);
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Agenti, 2 al lavoro", "il fuoco va sulla prima carta");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Apri il terminale, Agenti", "il Tab seguente non si perde");
        // Una voce diversa al posto della prima, col fuoco su «Consenti»: il fuoco torna su «Nega».
        voci.push(permesso_corto());
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "la voce che compare prende il fuoco su «Nega»");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        voci.set_row_data(0, VoceAttesaUi { id: "altro".into(), ..permesso_corto() });
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "mai sul consenso di un'altra richiesta");
    }

    /// Review Focus 2: fuoco sull'azione della carta Casa e l'azione sparisce: il fuoco va sul
    /// corpo della stessa carta.
    #[test]
    fn azione_che_sparisce_col_fuoco_addosso() {
        use slint::platform::Key;
        let moduli = quattro_moduli();
        let p = pannello_panoramica(ModelRc::from(moduli.clone()));
        for _ in 0..4 {
            tasto(&p, Key::Tab);
        }
        assert_eq!(etichetta_col_fuoco(&p), "Spegni tutte, Casa");
        moduli.set_row_data(1, modulo("casa", "Casa", "Luci spente", None));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Casa, Luci spente", "sul corpo della stessa carta");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "PC, Tutto regolare");
    }

    /// Il nome dell'elemento col fuoco è `moduli[k].narratore`; cambiato il modello sotto il
    /// fuoco, il nome lo segue.
    #[test]
    fn nomi_per_narrator_dal_modello() {
        use slint::platform::Key;
        let moduli = quattro_moduli();
        let p = pannello_panoramica(ModelRc::from(moduli.clone()));
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), moduli.row_data(0).unwrap().narratore.as_str());
        moduli.set_row_data(0, modulo("agenti", "Agenti", "Ti aspetta", Some("Apri il terminale")));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Agenti, Ti aspetta");
    }

    /// Tre voci di attenzione (nessuna ti aspetta): «3 in attesa ›» nella riga alta (Narrator
    /// senza la freccia), raggiungibile con Tab subito dopo la riga alta (C11), clic → Coda.
    /// Con la voce che ti aspetta non c'è (le altre le conta il suo «+N»).
    #[test]
    fn in_attesa_nella_riga_alta() {
        use slint::platform::Key;
        let voci: Vec<VoceAttesaUi> =
            (0..3).map(|i| voce_coda(&format!("a{i}"), &format!("Avviso {i}"), LivelloUi::Attenzione, "Guarda presto · Casa", false)).collect();
        let (p, _) = panoramica_con_voci(ModelRc::new(VecModel::from(voci)));
        crate::pannello::scrivi_in_attesa(&p, 3, false);
        let riga = elemento(&p, "3 in attesa");
        assert_eq!(riga.accessible_role(), Some(AccessibleRole::Button));
        assert!(riga.absolute_position().y < 44.0, "nella riga alta");
        assert!(elemento(&p, "09:45").size().height > 0.0, "senza voce bloccante l'orologio resta");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "3 in attesa", "prima delle carte");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Agenti, 2 al lavoro");
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "coda", "Invio apre la Coda");
        p.set_vista("panoramica".into());
        let (x, y) = centro(&elemento(&p, "3 in attesa"));
        clic_in(&p, x, y);
        assert_eq!(p.get_vista(), "coda", "il clic apre la Coda");
        // Con una voce che ti aspetta: niente «N in attesa ›».
        p.set_vista("panoramica".into());
        crate::pannello::scrivi_in_attesa(&p, 3, true);
        assert!(ElementHandle::find_by_accessible_label(&p, "3 in attesa").next().is_none());
    }

    /// L'ora grande sta in celle ferme: con «11:11» e con «00:00» ogni cella è alla stessa x.
    #[test]
    fn orologio_a_celle_fisse() {
        let p = pannello_panoramica(ModelRc::from(quattro_moduli()));
        let celle = |p: &Pannello| -> Vec<(f32, f32)> {
            let mut v: Vec<(f32, f32)> = ElementHandle::find_by_element_id(p, "OrologioGrande::cifra")
                .map(|e| (e.absolute_position().x, e.size().width))
                .collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v
        };
        crate::pannello::scrivi_ora(&p, "11:11");
        let uno = celle(&p);
        crate::pannello::scrivi_ora(&p, "00:00");
        let zero = celle(&p);
        assert_eq!(uno.len(), 5);
        assert_eq!(uno, zero, "celle ferme");
        // Centrate nel corpo (34..786).
        let (sinistra, destra) = (uno[0].0, uno[4].0 + uno[4].1);
        assert!((sinistra - 34.0 - (786.0 - destra)).abs() < 0.5, "centrate: {sinistra}..{destra}");
    }

    // ─── Ridisegno F1, Task 9: Panoramica Orologio ───

    /// La Panoramica in modalità Orologio coi quattro moduli di prova e la fascia scritta come
    /// la scrive il ponte (nessun modulo in attesa di dati).
    fn pannello_orologio() -> Pannello {
        let p = pannello_panoramica(ModelRc::from(quattro_moduli()));
        p.set_panoramica(1);
        crate::pannello::scrivi_fascia(&p, 0);
        slint::platform::update_timers_and_animations();
        p
    }

    /// L'elemento `Fascia` (uno solo, sempre al suo posto).
    fn fascia_el(p: &Pannello) -> ElementHandle {
        let mut v: Vec<ElementHandle> = ElementHandle::find_by_element_type_name(p, "Fascia").collect();
        assert_eq!(v.len(), 1, "una fascia sola");
        v.remove(0)
    }

    /// Orologio: quattro tessere 182 × 84 alla stessa y, centrate, sopra la barra; l'azione di
    /// chi ce l'ha si vede senza passaggio né fuoco (valore e riga a 116, la carta PC a 154); la
    /// fascia alta 32 fra orologio e tessere, alla stessa y con e senza la voce che ti aspetta.
    #[test]
    fn panoramica_orologio_con_quattro_tessere() {
        let p = pannello_orologio();
        let posti = posti(&p);
        assert_eq!(posti.len(), 4);
        let y = posti[0].absolute_position().y;
        for (k, posto) in posti.iter().enumerate() {
            assert_eq!(posto.absolute_position().y, y, "tessera {k} alla stessa y");
            assert_eq!((posto.size().width, posto.size().height), (182.0, 84.0), "tessera {k}");
        }
        assert_eq!(y + 84.0, 44.0 + 224.0, "le tessere in fondo al corpo, sopra la barra");
        let (sinistra, destra) = (posti[0].absolute_position().x, posti[3].absolute_position().x + 182.0);
        assert!((sinistra - 34.0 - (786.0 - destra)).abs() < 0.5, "centrate {sinistra}..{destra}");
        for (k, largo) in [(0, 116.0), (1, 116.0), (2, 154.0), (3, 116.0)] {
            assert_eq!(nella_carta(&p, k, "CartaVista::riga").size().width, largo, "tessera {k}: azione sempre visibile");
        }
        assert_eq!(pulsanti_senza_nome(&p), 0);
        let fascia = fascia_el(&p);
        assert_eq!(fascia.size().height, 32.0);
        assert_eq!(fascia.absolute_position().y + 32.0, y, "la fascia subito sopra le tessere");
        let orologio = elemento(&p, "09:45");
        assert!(orologio.absolute_position().y + orologio.size().height <= fascia.absolute_position().y + 0.01, "l'orologio sopra la fascia");
        assert_eq!(elemento(&p, "Niente ti aspetta").accessible_role(), Some(AccessibleRole::Text));
        let prima = fascia.absolute_position().y;
        let posti_prima: Vec<_> = posti.iter().map(|e| (e.absolute_position(), e.size())).collect();
        p.set_attese(ModelRc::new(VecModel::from(vec![permesso_corto()])));
        avanza(400);
        assert_eq!(fascia_el(&p).absolute_position().y, prima, "la fascia alla stessa quota con la voce");
        let posti_dopo: Vec<_> = self::posti(&p).iter().map(|e| (e.absolute_position(), e.size())).collect();
        assert_eq!(posti_prima, posti_dopo, "le tessere non si muovono");
        assert!(ElementHandle::find_by_accessible_label(&p, "Niente ti aspetta").next().is_none(), "con la voce la fascia tace");
        // La Griglia: carte 108, nessuna fascia visibile.
        p.set_attese(ModelRc::new(VecModel::from(Vec::<VoceAttesaUi>::new())));
        p.set_panoramica(0);
        avanza(400);
        assert_eq!(self::posti(&p)[0].size().height, 108.0);
        assert!(ElementHandle::find_by_accessible_label(&p, "Niente ti aspetta").next().is_none(), "nella Griglia niente fascia");
        // (Il fuoco della voce sparita è andato sulla carta 0: si guarda la carta 1, che ha l'azione.)
        assert_eq!(nella_carta(&p, 1, "CartaVista::riga").size().width, 154.0, "nella Griglia l'azione solo al passaggio");
    }

    /// Orologio con la voce che ti aspetta: l'orologio grande sparisce (dissolvenza) e la voce
    /// prende il suo posto, alla stessa posizione della Griglia (gli stessi punti dei pulsanti: le
    /// regole sul doppio clic valgono uguali); le tessere restano ferme; il fuoco su «Nega»;
    /// Invio su «Consenti» non concede, Ctrl+Invio esatto sì; col movimento ridotto tutto subito.
    #[test]
    fn orologio_voce_al_posto_dell_ora() {
        use slint::platform::Key;
        let posizioni = |p: &Pannello| -> Vec<(String, f32, f32)> {
            ["Nega", "Consenti"]
                .iter()
                .map(|n| {
                    let e = elemento(p, n);
                    (n.to_string(), e.absolute_position().x, e.absolute_position().y)
                })
                .collect()
        };
        let (g, _) = panoramica_con_voci(ModelRc::new(VecModel::from(vec![permesso_corto()])));
        let nella_griglia = posizioni(&g);

        let p = pannello_orologio();
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione_voce(move |_, id| a.borrow_mut().push(id.to_string()));
        let prima: Vec<_> = posti(&p).iter().map(|e| (e.absolute_position(), e.size())).collect();
        assert!(!p.get_ora_riga_visibile());
        assert!(prima.iter().all(|(_, lato)| lato.height == 84.0), "tessere dell'Orologio");
        p.set_attese(ModelRc::new(VecModel::from(vec![permesso_corto()])));
        slint::platform::update_timers_and_animations();
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "fuoco iniziale su «Nega»");
        avanza(60);
        assert!(
            ElementHandle::find_by_accessible_label(&p, "09:45").any(|e| e.absolute_position().y >= 44.0),
            "a metà dei 160 ms l'orologio si sta ancora dissolvendo"
        );
        avanza(400);
        assert!(ElementHandle::find_by_accessible_label(&p, "09:45").all(|e| e.absolute_position().y < 44.0), "niente orologio grande, solo l'ora piccola");
        assert!(p.get_ora_riga_visibile(), "l'ora va nella riga alta");
        let voce = elemento(&p, "Ti aspetta · Agenti: Claude chiede: PowerShell npm test");
        assert_eq!((voce.size().width, voce.size().height), (752.0, 108.0));
        assert_eq!(posizioni(&p), nella_griglia, "pulsanti agli stessi punti della Griglia");
        let dopo: Vec<_> = posti(&p).iter().map(|e| (e.absolute_position(), e.size())).collect();
        assert_eq!(prima, dopo, "le tessere non si muovono");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti");
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio su «Consenti» ha concesso: {:?}", azioni.borrow());
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"], "Ctrl+Invio esatto concede");

        // Movimento ridotto: la voce entra senza scala né dissolvenza, l'orologio sparisce subito.
        let p = pannello_orologio();
        p.global::<crate::Movimento>().set_ridotto(true);
        p.set_attese(ModelRc::new(VecModel::from(vec![permesso_corto()])));
        slint::platform::update_timers_and_animations();
        assert!(ElementHandle::find_by_accessible_label(&p, "09:45").all(|e| e.absolute_position().y < 44.0), "col movimento ridotto subito");
    }

    /// Orologio: «2 avvisi ›» nella fascia (non nella riga alta) è un bersaglio: Tab ci arriva
    /// prima delle tessere (C11), Narrator lo legge senza freccia, Invio e clic aprono la Coda.
    #[test]
    fn orologio_avvisi_nella_fascia() {
        use slint::platform::Key;
        let p = pannello_orologio();
        let voci: Vec<VoceAttesaUi> =
            (0..2).map(|i| voce_coda(&format!("a{i}"), &format!("Avviso {i}"), LivelloUi::Attenzione, "Guarda presto · Casa", false)).collect();
        p.set_attese(ModelRc::new(VecModel::from(voci)));
        crate::pannello::scrivi_in_attesa(&p, 2, false);
        crate::pannello::scrivi_fascia(&p, 0);
        slint::platform::update_timers_and_animations();
        let avvisi = elemento(&p, "2 avvisi");
        assert_eq!(avvisi.accessible_role(), Some(AccessibleRole::Button));
        let fascia = fascia_el(&p);
        assert!(avvisi.absolute_position().y >= fascia.absolute_position().y, "nella fascia");
        assert!(ElementHandle::find_by_accessible_label(&p, "2 in attesa").next().is_none(), "nell'Orologio non nella riga alta");
        assert_eq!(elemento(&p, "Niente ti aspetta").accessible_role(), Some(AccessibleRole::Text));
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "2 avvisi", "prima delle tessere");
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Agenti, 2 al lavoro");
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "coda", "Invio apre la Coda");
        p.set_vista("panoramica".into());
        lascia_girare(&p);
        let (x, y) = centro(&elemento(&p, "2 avvisi"));
        clic_in(&p, x, y);
        assert_eq!(p.get_vista(), "coda", "il clic apre la Coda");
        // Gli avvisi risolti col fuoco addosso: il fuoco va alle tessere.
        p.set_vista("panoramica".into());
        lascia_girare(&p);
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "2 avvisi");
        crate::pannello::scrivi_in_attesa(&p, 0, false);
        crate::pannello::scrivi_fascia(&p, 0);
        slint::platform::update_timers_and_animations();
        assert!(ElementHandle::find_by_accessible_label(&p, "2 avvisi").next().is_none());
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Apri il terminale, Agenti", "il fuoco non si perde");
    }

    /// Impostazioni › Isola: la riga «Panoramica» («Griglia» / «Orologio») c'è, nessuna riga dei
    /// quadranti; scegliere «Orologio» chiama `scegli-panoramica(1)`.
    #[test]
    fn impostazioni_scelta_della_panoramica() {
        let (f, _) = scheda_isola();
        let scelte = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let s = scelte.clone();
        f.on_scegli_panoramica(move |i| s.borrow_mut().push(i));
        assert!(ElementHandle::find_by_accessible_label(&f, "Quadrante 1").next().is_none());
        let riga = selettore(&f, "Panoramica");
        assert_eq!(riga.accessible_value().as_deref(), Some("Griglia"), "di serie la Griglia");
        riga.invoke_accessible_default_action();
        ElementHandle::find_by_accessible_label(&f, "Orologio")
            .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
            .expect("voce «Orologio»")
            .invoke_accessible_default_action();
        assert_eq!(*scelte.borrow(), [1]);
        f.set_panoramica_scelta(1);
        assert_eq!(selettore(&f, "Panoramica").accessible_value().as_deref(), Some("Orologio"));
    }

    /// `n` voci «In attesa» di prova: «Titolo 0», «Titolo 1»… con livello «Quando vuoi».
    fn voci_attesa(n: usize) -> ModelRc<VoceAttesaUi> {
        ModelRc::new(VecModel::from(
            (0..n)
                .map(|i| VoceAttesaUi {
                    id: format!("v{i}").into(),
                    livello: LivelloUi::Informazione,
                    titolo: format!("Titolo {i}").into(),
                    sotto: "Quando vuoi · prova".into(),
                    modulo: "Prova".into(),
                    dettaglio: "Il testo completo".into(),
                    icona: Default::default(),
                    pulsanti: ModelRc::new(VecModel::from(Vec::<PulsanteUi>::new())),
                    permesso: Default::default(),
                })
                .collect::<Vec<_>>(),
        ))
    }

    /// RC-44: il colore caldo dell'arco parte SOPRA la soglia d'avviso dell'utente (70 → caldo a
    /// 71, non a 85), nella vista Claude; il PC resta all'85 %.
    #[test]
    fn il_caldo_parte_oltre_la_soglia_dell_utente() {
        piattaforma_di_prova();
        let p = pannello();
        let tema = p.global::<crate::Tema>();
        assert!(tema.invoke_oltre_soglia(0.71, 0.70), "71 con soglia 70: caldo");
        assert!(tema.invoke_oltre_soglia(0.70, 0.70), "alla soglia esatta come la voce d'avviso (>=)");
        assert!(!tema.invoke_oltre_soglia(0.69, 0.70));
        assert!(!tema.invoke_oltre_soglia(0.50, 0.70));
        assert!(tema.invoke_oltre_soglia(0.80, 0.70), "a 80 con soglia 70 l'arco è caldo (prima no)");
        let claude = include_str!("../ui/viste/claude.slint");
        assert!(!claude.contains("0.85"), "la vista Claude non ha più un 85 % fisso");
        assert!((p.get_claude_soglia() - 0.80).abs() < 1e-6, "di serie 80");
    }

    #[test]
    fn la_vista_pc_dice_nome_e_valore() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_pc(StatPcUi { pronta: true, cpu: 0.12, cpu_testo: "12%".into(), ..Default::default() });
        // Nella vista PC la rete è una velocità (testo).
        p.set_vista("pc".into());
        assert_eq!(elemento(&p, "RETE").accessible_role(), Some(AccessibleRole::Text));
    }

    /// M2 Task 1: la riga BATTERIA col glifo dice ancora nome, valore e stato per Narrator; il
    /// glifo non è un elemento accessibile (il testo accanto dice tutto).
    #[test]
    fn riga_batteria_col_glifo() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_vista("pc".into());
        p.set_pc(StatPcUi {
            pronta: true,
            ha_batteria: true,
            batteria_testo: "18%".into(),
            batteria_frazione: 0.18,
            tinta_batteria: crate::TintaBatteriaUi::Informazione,
            ..Default::default()
        });
        let riga = elemento(&p, "BATTERIA");
        assert_eq!(riga.accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert_eq!(riga.accessible_value().as_deref(), Some("18%"));
        assert_eq!(riga.accessible_description().as_deref(), Some("A batteria"));
        // Il glifo c'è (19 × 8 px), sta nella riga a sinistra del valore e non è accessibile: il
        // test fallisce se la riga torna senza glifo.
        let glifi: Vec<_> = ElementHandle::find_by_element_type_name(&p, "GlifoBatteria").collect();
        assert_eq!(glifi.len(), 1, "un solo glifo nella riga BATTERIA");
        assert_eq!((glifi[0].size().width, glifi[0].size().height), (19.0, 8.0));
        assert_ne!(glifi[0].accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert!(glifi[0].accessible_label().is_none(), "il glifo non ha un nome per Narrator");
        let (riga_pos, glifo_pos) = (riga.absolute_position(), glifi[0].absolute_position());
        assert!(
            glifo_pos.x > riga_pos.x + riga.size().width / 2.0,
            "il glifo sta nella colonna del valore, a destra"
        );
        // Senza batteria niente riga e niente glifo.
        p.set_pc(StatPcUi { pronta: true, ..Default::default() });
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "GlifoBatteria").count(), 0);
    }

    #[test]
    fn pillola_e_un_pulsante_col_nome_giusto() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        assert_eq!(elemento(&p, "ItsyHome").accessible_role(), Some(AccessibleRole::Button));
        p.set_aspetto(AspettoPillolaUi::Puntino);
        assert_eq!(elemento(&p, "ItsyHome, una voce in attesa").accessible_role(), Some(AccessibleRole::Button));
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("La casa chiede di te".into());
        assert_eq!(elemento(&p, "La casa chiede di te").accessible_role(), Some(AccessibleRole::Button));
    }

    /// 16/11 (Paolo): il clic destro sulla pillola non fa nulla (il menu sta nel «⋯» del
    /// pannello); il clic sinistro apre il pannello come prima.
    #[test]
    fn clic_destro_sulla_pillola_non_fa_nulla() {
        use slint::platform::{PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&str>::new()));
        let e = eventi.clone();
        p.on_cliccata(move || e.borrow_mut().push("cliccata"));
        let pillola = elemento(&p, "ItsyHome");
        let (pos, lato) = (pillola.absolute_position(), pillola.size());
        let centro = slint::LogicalPosition::new(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        let clic = |button| {
            p.window().dispatch_event(WindowEvent::PointerMoved { position: centro });
            p.window().dispatch_event(WindowEvent::PointerPressed { position: centro, button });
            p.window().dispatch_event(WindowEvent::PointerReleased { position: centro, button });
        };
        clic(PointerEventButton::Right);
        assert!(eventi.borrow().is_empty(), "il clic destro non apre il pannello");
        clic(PointerEventButton::Left);
        assert_eq!(*eventi.borrow(), ["cliccata"]);
    }

    /// Task 14d: «Mostra icona nella barra» in Impostazioni › Sistema, spenta di serie; il
    /// commutatore la accende (e lo dice a Rust); fuori da Windows non si mostra.
    #[test]
    fn interruttore_dell_icona_nella_barra() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(5);
        f.show().expect("impostazioni mostrate nel backend di prova");
        let chiesti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let c = chiesti.clone();
        f.on_cambia_icona_barra(move |si| c.borrow_mut().push(si));
        // Lo stesso nome ha anche il titolo (un testo): si prende il commutatore.
        let interruttore = ElementHandle::find_by_accessible_label(&f, "Mostra icona nella barra")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
            .expect("commutatore «Mostra icona nella barra»");
        assert_eq!(interruttore.accessible_checked(), Some(false), "spenta di serie");
        interruttore.invoke_accessible_default_action();
        assert_eq!(*chiesti.borrow(), [true]);
        assert_eq!(interruttore.accessible_checked(), Some(true));
        f.set_icona_barra_disponibile(false);
        assert!(ElementHandle::find_by_accessible_label(&f, "Mostra icona nella barra").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&f, "Avvia con il sistema").next().is_some());
    }

    /// Fuori da Windows (nessun menu né icona nella barra) Impostazioni › Sistema ha «Esci
    /// dall'assistente» con il pulsante «Esci»: si vede solo con il flag acceso, ha ruolo e nome per
    /// Narrator, si raggiunge con Tab e si preme con Invio, e chiama il callback che chiude l'app.
    #[test]
    fn esci_nelle_impostazioni_fuori_da_windows() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(5);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let chiamate = std::rc::Rc::new(std::cell::Cell::new(0));
        let c = chiamate.clone();
        f.on_esci(move || c.set(c.get() + 1));
        let ce = |nome: &str| ElementHandle::find_by_accessible_label(&f, nome).next().is_some();
        // Spento (di serie, e su Windows): né riga né pulsante.
        assert!(!ce("Esci dall'assistente") && !ce("Esci"));
        f.set_fuori_da_windows(true);
        assert!(ce("Esci dall'assistente"), "la riga si vede");
        let esci = ElementHandle::find_by_accessible_label(&f, "Esci")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .expect("pulsante «Esci»");
        // Come lo aziona Narrator.
        esci.invoke_accessible_default_action();
        assert_eq!(chiamate.get(), 1, "il pulsante chiama il callback");
        // Da tastiera: Tab fino al pulsante (al più 20 passi), poi Invio.
        let tasto = |t: Key| {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
        };
        for _ in 0..20 {
            if chiamate.get() == 2 {
                break;
            }
            tasto(Key::Tab);
            tasto(Key::Return);
            // Invio su una voce del menu laterale cambia sezione: si torna a Sistema, il fuoco resta.
            f.set_sezione(5);
        }
        assert_eq!(chiamate.get(), 2, "Tab raggiunge il pulsante e Invio lo preme");
        f.set_fuori_da_windows(false);
        assert!(!ce("Esci dall'assistente") && !ce("Esci"), "spento di nuovo: sparito");
    }

    #[test]
    fn sezioni_delle_impostazioni_selezionate() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        let ha = elemento(&f, "Casa");
        assert_eq!(ha.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!((ha.accessible_item_selected(), ha.accessible_item_count()), (Some(true), Some(7)));
        assert_eq!(elemento(&f, "Collegamenti").accessible_item_index(), Some(2));
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_INFORMAZIONI);
        assert_eq!(elemento(&f, "Informazioni").accessible_item_selected(), Some(true));
        assert_eq!(elemento(&f, "Casa").accessible_item_selected(), Some(false));
    }

    /// Task 14e: Impostazioni › Dispositivi. Ogni pulsante ha il suo nome per Narrator e porta a
    /// Rust l'azione con la riga, la chiave e la stanza; le frecce ai bordi sono spente; la scelta
    /// dell'icona si apre, sceglie e si chiude.
    #[test]
    fn scheda_dispositivi_pulsanti_e_scelta_icona() {
        use crate::{GenereUi, RigaDispositivoUi, TipoRigaUi};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        let riga = |id: &str, tipo, chiave: &str, area: &str, nome: &str, su, giu| RigaDispositivoUi {
            id: id.into(),
            tipo,
            chiave: chiave.into(),
            area: area.into(),
            nome: nome.into(),
            sotto: "".into(),
            icona: 6,
            genere: GenereUi::Luce,
            su,
            giu,
            puo_icona: tipo == TipoRigaUi::TestataStanza,
            ..Default::default()
        };
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![
            riga("h:", TipoRigaUi::TestataPreferiti, "__preferiti__", "", "Preferiti", false, false),
            riga("s:cucina", TipoRigaUi::TestataStanza, "cucina", "cucina", "Cucina", false, true),
            riga("d:light.a", TipoRigaUi::Dispositivo, "light.a", "cucina", "Luce A", false, true),
            riga("d:light.b", TipoRigaUi::Dispositivo, "light.b", "cucina", "Luce B", true, false),
            riga("s:sala", TipoRigaUi::TestataStanza, "sala", "sala", "Sala", true, false),
        ])));
        f.set_nomi_icone(ModelRc::new(VecModel::from(
            crate::stanze::ICONE.iter().map(|(_, n)| slint::SharedString::from(*n)).collect::<Vec<_>>(),
        )));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (a1, a2) = (azioni.clone(), azioni.clone());
        f.on_azione_dispositivo(move |azione, id, chiave, area| a1.borrow_mut().push(format!("{azione} {id} {chiave} {area}")));
        f.on_scegli_icona(move |area, i| a2.borrow_mut().push(format!("icona {area} {i}")));

        assert_eq!(elemento(&f, "Sposta su «Luce A»").accessible_enabled(), Some(false), "già in cima");
        // I comandi stanno a destra della riga, larga quanto la colonna del contenuto (prima le
        // righe nascevano strette e i pulsanti finivano sopra i nomi).
        let occhio = elemento(&f, "Nascondi «Luce B»");
        let larghezza = f.window().size().to_logical(f.window().scale_factor()).width;
        assert!(occhio.absolute_position().x > larghezza - 80.0, "occhio a x {} su {larghezza}", occhio.absolute_position().x);
        elemento(&f, "Sposta su «Luce B»").invoke_accessible_default_action();
        elemento(&f, "Aggiungi «Luce A» ai preferiti").invoke_accessible_default_action();
        elemento(&f, "Nascondi «Luce B»").invoke_accessible_default_action();
        elemento(&f, "Nascondi la stanza «Sala»").invoke_accessible_default_action();
        elemento(&f, "Sposta su «Sala»").invoke_accessible_default_action();
        elemento(&f, "Comprimi «Cucina»").invoke_accessible_default_action();
        assert_eq!(
            *azioni.borrow(),
            [
                "su d:light.b light.b cucina",
                "preferito d:light.a light.a cucina",
                "nascondi d:light.b light.b cucina",
                "nascondi s:sala sala sala",
                "su s:sala sala sala",
                "comprimi s:cucina cucina cucina",
            ]
        );
        // La scelta dell'icona: si apre col nome della stanza, la scelta va a Rust e si chiude.
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        let scelta = elemento(&f, "ICONA DI «Cucina»");
        assert_eq!(scelta.accessible_role(), Some(AccessibleRole::Groupbox));
        let pentola = elemento(&f, "Pentola");
        assert_eq!(pentola.accessible_item_selected(), Some(true), "l'icona di adesso è evidenziata");
        elemento(&f, "Videogiochi").invoke_accessible_default_action();
        assert_eq!(azioni.borrow().last().map(String::as_str), Some("icona cucina 36"));
        assert!(ElementHandle::find_by_accessible_label(&f, "ICONA DI «Cucina»").next().is_none(), "chiusa");
    }

    /// 18/11: le istruzioni di «Dispositivi» (ordine, stella, occhio) non compaiono con l'elenco vuoto.
    #[test]
    fn dispositivi_senza_righe_niente_istruzioni() {
        use crate::{RigaDispositivoUi, TipoRigaUi};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        let istruzioni = "Ordine con le frecce, preferiti con la stella, nascosti con l'occhio.";
        assert!(ElementHandle::find_by_accessible_label(&f, "Nessun dispositivo da gestire").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&f, istruzioni).next().is_none(), "istruzioni senza elenco");
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![RigaDispositivoUi {
            id: "s:cucina".into(),
            tipo: TipoRigaUi::TestataStanza,
            chiave: "cucina".into(),
            nome: "Cucina".into(),
            ..Default::default()
        }])));
        assert!(ElementHandle::find_by_accessible_label(&f, istruzioni).next().is_some(), "istruzioni con l'elenco");
    }

    /// Task 14e: la scelta dell'icona da tastiera (frecce, Invio) ed Esc che la chiude senza
    /// chiudere la finestra.
    #[test]
    fn scelta_icona_da_tastiera() {
        use crate::{RigaDispositivoUi, TipoRigaUi};
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![RigaDispositivoUi {
            id: "s:cucina".into(),
            tipo: TipoRigaUi::TestataStanza,
            chiave: "cucina".into(),
            area: "cucina".into(),
            nome: "Cucina".into(),
            icona: 6,
            puo_icona: true,
            ..Default::default()
        }])));
        f.set_nomi_icone(ModelRc::new(VecModel::from(
            crate::stanze::ICONE.iter().map(|(_, n)| slint::SharedString::from(*n)).collect::<Vec<_>>(),
        )));
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2) = (eventi.clone(), eventi.clone());
        f.on_scegli_icona(move |area, i| e1.borrow_mut().push(format!("icona {area} {i}")));
        f.on_chiudi(move || e2.borrow_mut().push("chiudi finestra".into()));
        let tasto = |t: Key| {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
        };
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        // Esc chiude la scelta, non la finestra.
        tasto(Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "ICONA DI «Cucina»").next().is_none());
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
        // Di nuovo: dalla 6 (Pentola) → giù di una riga (16) e a destra (17), Invio.
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        tasto(Key::DownArrow);
        tasto(Key::RightArrow);
        tasto(Key::Return);
        assert_eq!(*eventi.borrow(), ["icona cucina 17"]);
    }

    /// Task 14f, N1 del 14e: un clic dentro il riquadro della scelta dell'icona ma fuori da
    /// un'icona (titolo, margini) non la chiude; un clic fuori sì.
    #[test]
    fn scelta_icona_trattiene_i_clic_dentro_il_riquadro() {
        use crate::{RigaDispositivoUi, TipoRigaUi};
        use slint::platform::{PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![RigaDispositivoUi {
            id: "s:cucina".into(),
            tipo: TipoRigaUi::TestataStanza,
            chiave: "cucina".into(),
            area: "cucina".into(),
            nome: "Cucina".into(),
            icona: 6,
            puo_icona: true,
            ..Default::default()
        }])));
        f.set_nomi_icone(ModelRc::new(VecModel::from(
            crate::stanze::ICONE.iter().map(|(_, n)| slint::SharedString::from(*n)).collect::<Vec<_>>(),
        )));
        // Due volte: il primo clic su elementi appena nati da un `if` il backend di prova non lo
        // consegna sempre; col secondo il controllo vale (senza il rimedio il titolo la chiudeva).
        let clic = |x: f32, y: f32| {
            let posizione = slint::LogicalPosition::new(x, y);
            let button = PointerEventButton::Left;
            for _ in 0..2 {
                f.window().dispatch_event(WindowEvent::PointerMoved { position: posizione });
                f.window().dispatch_event(WindowEvent::PointerPressed { position: posizione, button });
                f.window().dispatch_event(WindowEvent::PointerReleased { position: posizione, button });
            }
        };
        let aperta = || ElementHandle::find_by_accessible_label(&f, "ICONA DI «Cucina»").next().is_some();
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        let riquadro = elemento(&f, "ICONA DI «Cucina»");
        let (pos, lato) = (riquadro.absolute_position(), riquadro.size());
        // Sul titolo e nell'angolo in basso a destra (sotto l'ultima riga, a destra dell'ultima colonna).
        clic(pos.x + 24.0, pos.y + 24.0);
        assert!(aperta(), "un clic sul titolo l'ha chiusa");
        clic(pos.x + lato.width - 6.0, pos.y + lato.height - 6.0);
        assert!(aperta(), "un clic nel margine l'ha chiusa");
        // Sotto il riquadro, ancora nell'elenco: la chiude come Esc.
        clic(pos.x + 24.0, pos.y + lato.height + 20.0);
        assert!(!aperta(), "un clic fuori dal riquadro la chiude");
    }

    fn tasto(p: &Pannello, testo: impl Into<slint::SharedString> + Clone) {
        use slint::platform::WindowEvent;
        p.window().dispatch_event(WindowEvent::KeyPressed { text: testo.clone().into() });
        p.window().dispatch_event(WindowEvent::KeyReleased { text: testo.into() });
    }

    #[test]
    fn la_tastiera_regge_col_fuoco_sugli_elementi() {
        // FE13: `tasti` avvolge il pannello e intercetta i tasti prima degli elementi che hanno il
        // fuoco vero; Tab, frecce, Invio ed Esc fanno quello che facevano.
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        // Apertura finita (nel backend di prova il timer non gira): il fuoco vero va sugli elementi.
        p.global::<crate::Fuochi>().set_pronti(true);
        let chiuso = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiuso.clone();
        p.on_chiudi(move || c.set(true));
        // Contenuto (puntini) → Tab al dock → → su «Agenti» (il primo modulo, ridisegno F1) → Invio:
        // si apre la vista Agenti.
        tasto(&p, Key::Tab);
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "agenti");
        // Backspace torna alla Panoramica; Esc chiude.
        tasto(&p, Key::Backspace);
        assert_eq!(p.get_vista(), "panoramica");
        tasto(&p, Key::Escape);
        assert!(chiuso.get(), "Esc chiude il pannello");
    }

    /// Task 14i: in alto a destra il pulsante «⋯» («Altre opzioni»), che chiede il menu dell'isola
    /// col mouse, con Narrator e da tastiera; nel dock (le app vere del registro) niente Impostazioni.
    #[test]
    fn altre_opzioni_nella_riga_alta() {
        use slint::platform::{PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        let menu = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let m = menu.clone();
        p.on_menu(move || m.set(m.get() + 1));
        let piu = elemento(&p, "Altre opzioni");
        assert_eq!(piu.accessible_role(), Some(AccessibleRole::Button));
        let (pos, lato) = (piu.absolute_position(), piu.size());
        assert!(lato.width >= 28.0 && lato.height >= 28.0, "bersaglio di almeno 28 × 28: {lato:?}");
        assert!(pos.x > 700.0 && pos.y < 10.0, "in alto a destra: {pos:?}");
        assert!(ElementHandle::find_by_accessible_label(&p, "Collegamenti, scheda 5 di 5").next().is_some(), "le schede vere sono caricate");
        assert!(
            !i_slint_backend_testing::ElementQuery::from_root(&p)
                .match_descendants()
                .find_all()
                .iter()
                .any(|e| e.accessible_role() == Some(AccessibleRole::Tab) && e.accessible_label().is_some_and(|l| l.starts_with("Impostazioni"))),
            "niente Impostazioni fra le schede"
        );

        piu.invoke_accessible_default_action();
        assert_eq!(menu.get(), 1, "Narrator lo preme: Rust è chiesto delle voci");
        // Dove, come e con quale tastiera si apre il menu lo provano i test del menu scuro
        // (`il_menu_sta_dentro_il_pannello_sotto_il_pulsante` e gli altri): qui Rust non dà voci.

        let centro = slint::LogicalPosition::new(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        let clic = || {
            p.window().dispatch_event(WindowEvent::PointerMoved { position: centro });
            p.window().dispatch_event(WindowEvent::PointerPressed { position: centro, button: PointerEventButton::Left });
            p.window().dispatch_event(WindowEvent::PointerReleased { position: centro, button: PointerEventButton::Left });
        };
        clic();
        assert_eq!(menu.get(), 2, "il clic lo chiede di nuovo");
    }

    /// Task 14i: Tab dal dock arriva a «⋯» (e Maiusc+Tab dalla Panoramica senza voci in attesa);
    /// Invio e Spazio lì aprono il menu; Esc chiude il pannello come prima.
    #[test]
    fn tab_arriva_ad_altre_opzioni() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&str>::new()));
        let (e1, e2) = (eventi.clone(), eventi.clone());
        p.on_menu(move || e1.borrow_mut().push("menu"));
        p.on_chiudi(move || e2.borrow_mut().push("chiudi"));
        // Panoramica senza carte: Tab → scheda Panoramica → Tab: ricerca → Tab: «⋯» (senza volume).
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Panoramica, scheda 1 di 5");
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["menu"]);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-menu"), "fuoco su «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::Space);
        assert_eq!(*eventi.borrow(), ["menu", "menu"]);
        // Tab da «⋯» torna in cima (la prima scheda); Maiusc+Tab torna a «⋯», Invio apre il menu.
        tasto(&p, Key::Tab);
        assert_eq!(etichetta_col_fuoco(&p), "Panoramica, scheda 1 di 5");
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["menu", "menu", "menu"]);
        tasto(&p, Key::Escape);
        assert_eq!(*eventi.borrow(), ["menu", "menu", "menu", "chiudi"]);
    }

    /// L'id dell'elemento col fuoco vero nel pannello («Pannello::fuoco-menu»).
    fn col_fuoco_pannello(p: &Pannello) -> String {
        let fuoco = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
        fuoco
            .as_ref()
            .and_then(|i| i.element_type_names_and_ids(0))
            .and_then(|v| v.first().map(|(_, id)| id.to_string()))
            .unwrap_or_default()
    }

    fn voce_casa(id: &str, nome: &str) -> crate::PreferitoUi {
        crate::PreferitoUi {
            entity_id: id.into(),
            nome: nome.into(),
            genere: crate::GenereUi::Luce,
            stato: "Spento".into(),
            disponibile: true,
            toccabile: true,
            ..Default::default()
        }
    }

    /// Un pannello sulla vista Casa con due stanze, un preferito e i dispositivi di una stanza;
    /// restituisce gli eventi (stanze aperte e chiuse, tocchi, chiusura del pannello).
    fn pannello_casa() -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_casa_pronta(true);
        let stanza = |id: &str, nome: &str, quanti| crate::StanzaUi { area_id: id.into(), nome: nome.into(), icona: 0, quanti };
        p.set_stanze(ModelRc::new(VecModel::from(vec![stanza("cucina", "Cucina", 2), stanza("soggiorno", "Soggiorno", 1)])));
        p.set_preferiti(ModelRc::new(VecModel::from(vec![voce_casa("light.preferita", "Preferita")])));
        p.set_dispositivi(ModelRc::new(VecModel::from(vec![
            voce_casa("light.a", "Luce A"),
            voce_casa("light.b", "Luce B"),
        ])));
        p.set_vista("casa".into());
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3, e4) = (eventi.clone(), eventi.clone(), eventi.clone(), eventi.clone());
        p.on_apri_stanza(move |a| e1.borrow_mut().push(format!("apri {a}")));
        p.on_chiudi_stanza(move || e2.borrow_mut().push("chiudi stanza".into()));
        p.on_tocca(move |id| e3.borrow_mut().push(format!("tocca {id}")));
        p.on_chiudi(move || e4.borrow_mut().push("chiudi pannello".into()));
        (p, eventi)
    }

    /// Task 14e/14g: le stanze sono schede di un elenco (nome e quanti dispositivi per Narrator):
    /// una sola selezionata, «Preferiti» o la stanza aperta.
    #[test]
    fn stanze_della_casa_accessibili() {
        let (p, _) = pannello_casa();
        let cucina = elemento(&p, "Cucina, 2 dispositivi");
        assert_eq!(cucina.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!(cucina.accessible_item_selected(), Some(false));
        assert!(elemento(&p, "Soggiorno, 1 dispositivo").accessible_role().is_some());
        cucina.invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), 0);
        assert_eq!(elemento(&p, "Cucina, 2 dispositivi").accessible_item_selected(), Some(true));
        // La griglia mostra i dispositivi della stanza al posto dei preferiti.
        assert!(ElementHandle::find_by_accessible_label(&p, "Luce A, Spento").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferita, Spento").next().is_none());
        // Giro di correzioni 1: la scheda già selezionata resta tale (come «Preferiti»); si torna ai
        // preferiti con «Preferiti».
        elemento(&p, "Cucina, 2 dispositivi").invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), 0, "un secondo tocco non chiude la stanza");
        scheda(&p, "Preferiti").invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), -1);
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferita, Spento").next().is_some());
    }

    /// La scheda dell'elenco con quel nome (il titolo della griglia può avere lo stesso testo).
    fn scheda(p: &Pannello, nome: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(p, nome)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Tab))
            .unwrap_or_else(|| panic!("nessuna scheda «{nome}»"))
    }

    fn titolo(p: &Pannello, testo: &str) -> bool {
        ElementHandle::find_by_accessible_label(p, testo).any(|e| e.accessible_role() == Some(AccessibleRole::Text))
    }

    /// Task 14g: la prima voce dell'elenco è «Preferiti» (stella, senza numero: vedi il giro di
    /// correzioni 1), selezionata all'apertura; toccandola la griglia torna ai preferiti col titolo
    /// «Preferiti». Nessuna intestazione «STANZE» sopra «Preferiti»; nella riga alta non c'è più
    /// «‹ Indietro».
    #[test]
    fn preferiti_prima_voce_delle_stanze() {
        let (p, eventi) = pannello_casa();
        let elenco = elemento(&p, "Preferiti e stanze");
        assert_eq!(elenco.accessible_role(), Some(AccessibleRole::TabList));
        assert!(ElementHandle::find_by_accessible_label(&p, "STANZE").next().is_none(), "niente «STANZE» sopra «Preferiti»");
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferiti, 1 dispositivo").next().is_none(), "senza numero");
        let preferiti = scheda(&p, "Preferiti");
        assert_eq!(preferiti.accessible_item_selected(), Some(true), "selezionata all'apertura");
        assert_eq!((preferiti.accessible_item_index(), preferiti.accessible_item_count()), (Some(0), Some(3)));
        let cucina = elemento(&p, "Cucina, 2 dispositivi");
        assert_eq!((cucina.accessible_item_index(), cucina.accessible_item_count()), (Some(1), Some(3)));
        assert!(titolo(&p, "Preferiti"), "titolo della griglia");
        assert!(ElementHandle::find_by_accessible_label(&p, "Indietro").next().is_none(), "niente «Indietro»");

        cucina.invoke_accessible_default_action();
        assert_eq!(scheda(&p, "Preferiti").accessible_item_selected(), Some(false));
        assert!(!titolo(&p, "Preferiti"), "titolo: la stanza");
        assert!(titolo(&p, "Cucina"));

        scheda(&p, "Preferiti").invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), -1);
        assert_eq!(scheda(&p, "Preferiti").accessible_item_selected(), Some(true));
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferita, Spento").next().is_some());
        // «Preferiti» già selezionata: toccarla di nuovo non cambia nulla.
        scheda(&p, "Preferiti").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["apri cucina", "chiudi stanza"]);
    }

    /// Task 14g: «‹ Indietro» non c'è in nessuna vista (si torna alla Panoramica dal dock, con
    /// Backspace o Alt+←).
    #[test]
    fn nessun_indietro_nelle_viste() {
        let (p, _) = pannello_casa();
        for vista in ["casa", "agenti", "pc", "collegamenti", "panoramica"] {
            p.set_vista(vista.into());
            assert!(ElementHandle::find_by_accessible_label(&p, "Indietro").next().is_none(), "«Indietro» in {vista}");
        }
    }

    /// Fa passare `ms` millisecondi di tempo finto (timer e animazioni del backend di prova).
    fn avanza(ms: u64) {
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(ms));
    }

    /// Il timer da 16 ms fa partire l'arrivo, poi i 280 ms dell'animazione: in due passi, perché
    /// l'animazione parte dall'istante in cui scatta il timer.
    fn fine_scorrimento() {
        avanza(20);
        avanza(300);
    }

    /// M2 Task 3: la riga sinistra (data e meteo) c'è nella Panoramica; nelle app la parte
    /// sinistra della riga alta resta vuota (niente «Indietro»).
    #[test]
    fn riga_sinistra_solo_nella_panoramica() {
        let (p, _) = pannello_casa();
        p.set_riga_sinistra("VEN 13 NOV · 9°".into());
        p.set_riga_sinistra_nome("venerdì 13 novembre, 9 gradi".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "venerdì 13 novembre, 9 gradi").next().is_none(), "vista casa: vuota");
        assert!(ElementHandle::find_by_accessible_label(&p, "Indietro").next().is_none());
        p.set_vista("panoramica".into());
        assert!(titolo(&p, "venerdì 13 novembre, 9 gradi"), "Panoramica: visibile");
        p.set_vista("casa".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "venerdì 13 novembre, 9 gradi").next().is_none(), "vista casa: vuota");
    }

    /// M2 Task 3: il contenuto scorre dal lato della scheda scelta (280 ms) e la riga alta e il
    /// dock restano fermi. Il ritorno alla Panoramica (dal dock, Backspace, Alt+←) va verso l'indice 0.
    #[test]
    fn scorrimento_dal_lato_della_scheda() {
        let (p, _) = pannello_casa();
        p.set_vista("panoramica".into());
        avanza(400);
        assert_eq!(p.get_scarto_contenuto(), 0.0);
        p.set_vista("pc".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), 26.0, "da destra, subito");
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0, "arrivato");
        p.set_vista("panoramica".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), -26.0, "verso la Panoramica: da sinistra");
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0);
        // Fra due app: casa (1) → pc (2) da destra; pc → casa da sinistra.
        p.set_vista("casa".into());
        fine_scorrimento();
        p.set_vista("pc".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), 26.0);
        fine_scorrimento();
        p.set_vista("casa".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), -26.0);
        fine_scorrimento();
        // Backspace dalla vista: verso la Panoramica.
        tasto(&p, slint::platform::Key::Backspace);
        avanza(0);
        assert_eq!(p.get_vista(), "panoramica");
        assert_eq!(p.get_scarto_contenuto(), -26.0);
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0);
    }

    /// M2 Task 3: con «Effetti di animazione» spento nessuno scarto, solo la dissolvenza.
    #[test]
    fn scorrimento_spento_col_movimento_ridotto() {
        let (p, _) = pannello_casa();
        p.global::<crate::Movimento>().set_ridotto(true);
        p.set_vista("pc".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), 0.0);
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0);
    }

    /// Task 14e/14g: da tastiera si va dalle card alle stanze (←, su «Preferiti»), si sceglie (↓) e
    /// si apre (Invio); il fuoco passa ai dispositivi, Invio tocca. Esc a livelli: prima torna a
    /// «Preferiti», poi chiude il pannello; Backspace torna prima a «Preferiti», poi alla Panoramica.
    #[test]
    fn stanza_da_tastiera_ed_esc_a_livelli() {
        use slint::platform::Key;
        let (p, eventi) = pannello_casa();
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 1);
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri soggiorno", "tocca light.b"]);
        tasto(&p, Key::Escape);
        assert_eq!(p.get_stanza_aperta(), -1, "il primo Esc torna a «Preferiti»");
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi stanza"));
        assert_eq!(scheda(&p, "Preferiti").accessible_item_selected(), Some(true));
        // Il fuoco è su «Preferiti»: ↓ e Invio riaprono la prima stanza.
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 0);
        tasto(&p, Key::Backspace);
        assert_eq!((p.get_stanza_aperta(), p.get_vista().as_str()), (-1, "casa"), "Backspace torna prima a «Preferiti»");
        tasto(&p, Key::Backspace);
        assert_eq!(p.get_vista(), "panoramica");
        // Il pannello ha ancora la tastiera (prima il fuoco finiva su una card che spariva): Tab
        // porta il fuoco su un elemento vero.
        tasto(&p, Key::Tab);
        let col_fuoco = etichetta_col_fuoco(&p);
        assert!(!col_fuoco.is_empty() && col_fuoco != "Pannello di ItsyHome", "fuoco su «{col_fuoco}»");
        tasto(&p, Key::Escape);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi pannello"), "{:?}", eventi.borrow());
    }

    /// Task 14g: Invio su «Preferiti» (già selezionata) porta il fuoco alle card; con una stanza
    /// aperta la chiude. ↑ dalla prima stanza torna a «Preferiti».
    #[test]
    fn preferiti_da_tastiera() {
        use slint::platform::Key;
        let (p, eventi) = pannello_casa();
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Return);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["tocca light.preferita"], "Invio su «Preferiti» porta alle card");
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 0);
        // Giro di correzioni 1: ← porta sulla stanza aperta, Invio lì torna alle card (come su
        // «Preferiti») senza chiuderla.
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 0, "Invio sulla stanza aperta non la chiude");
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("tocca light.a"), "il fuoco è tornato alle card");
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), -1, "Invio su «Preferiti» chiude la stanza");
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi stanza"));
    }

    /// Task 14e: Tab passa dalle stanze alle card e al dock; Maiusc+Tab torna indietro.
    #[test]
    fn tab_fra_stanze_card_e_dock() {
        use slint::platform::Key;
        let (p, eventi) = pannello_casa();
        // Contenuto (card) → Maiusc+Tab: stanze, su «Preferiti»; ↓ e Invio aprono la prima.
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri cucina"]);
        // Esc torna a «Preferiti» e il fuoco resta sulle stanze; Tab porta alle card.
        tasto(&p, Key::Escape);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("tocca light.preferita"));
    }

    /// Il nome accessibile dell'elemento col fuoco, o del primo suo antenato che ne ha uno.
    fn etichetta_col_fuoco(p: &Pannello) -> String {
        use i_slint_core::accessibility::AccessibleStringProperty;
        use i_slint_core::item_tree::ParentItemTraversalMode;
        let mut corrente = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
        while let Some(item) = corrente {
            if let Some(e) = item.accessible_string_property(AccessibleStringProperty::Label).filter(|e| !e.is_empty()) {
                return e.to_string();
            }
            corrente = item.parent_item(ParentItemTraversalMode::StopAtPopups);
        }
        String::new()
    }

    /// Un elenco di luci accese per la Casa filtrata, e il pannello che lo mostra.
    fn luci_accese_di_prova(ids: &[&str]) -> std::rc::Rc<VecModel<crate::PreferitoUi>> {
        std::rc::Rc::new(VecModel::from(
            ids.iter()
                .map(|id| crate::PreferitoUi { acceso: true, stato: "Acceso".into(), ..voce_casa(id, &format!("Nome {id}")) })
                .collect::<Vec<_>>(),
        ))
    }

    /// 18/11, R1-16: dentro la card dispositivo il ritmo è a 4/8: margine di 12, nome a 40 e stato
    /// a 20 dal fondo; nome e stato non si toccano né toccano l'icona.
    #[test]
    fn card_casa_ritmo_4_8() {
        let (p, _) = pannello_casa();
        let card = elemento(&p, "Preferita, Spento");
        let (cp, cs) = (card.absolute_position(), card.size());
        let figli: Vec<(f32, f32, f32, f32)> = card
            .query_descendants()
            .find_all()
            .into_iter()
            .filter(|e| e.size().width > 0.0 && e.size().height > 0.0)
            .map(|e| (e.absolute_position().x - cp.x, e.absolute_position().y - cp.y, e.size().width, e.size().height))
            .collect();
        assert!(figli.iter().any(|f| f.0 == 12.0 && f.1 == cs.height - 40.0 && f.2 == cs.width - 24.0), "nome: {figli:?}");
        assert!(figli.iter().any(|f| f.0 == 12.0 && f.1 == cs.height - 20.0 && f.2 == cs.width - 24.0), "stato: {figli:?}");
        let icona = figli.iter().find(|f| f.0 == 12.0 && f.2 == 22.0).expect("icona a 12");
        assert!(icona.1 + icona.3 <= cs.height - 40.0, "l'icona non tocca il nome: {icona:?} in {cs:?}");
    }

    /// 18/11 (Paolo) e ridisegno F1, Task 8: «N luci accese» porta ancora alla Casa filtrata, ma
    /// è il clic (o Narrator, o Invio) sulla carta Casa con luci accese; senza luci accese la carta
    /// apre la Casa completa.
    #[test]
    fn la_carta_casa_con_luci_accese_apre_la_casa_filtrata() {
        use slint::platform::Key;
        let (p, _) = pannello_casa();
        p.set_schede(schede());
        p.set_moduli(moduli_di_prova());
        let casa = p.get_moduli().iter().find(|m| m.id == "casa").expect("la carta Casa").narratore.to_string();
        p.set_vista("panoramica".into());
        elemento(&p, &casa).invoke_accessible_default_action();
        assert_eq!((p.get_vista().as_str(), p.get_filtro_luci()), ("casa", false), "senza luci: la Casa completa");
        p.set_vista("panoramica".into());
        p.set_luci(luci_accese_di_prova(&["light.a", "light.b"]).into());
        let carta = elemento(&p, &casa);
        assert_eq!(carta.accessible_role(), Some(AccessibleRole::Button));
        carta.invoke_accessible_default_action();
        assert_eq!((p.get_vista().as_str(), p.get_filtro_luci()), ("casa", true));
        assert!(titolo(&p, "Luci accese (2)"), "l'intestazione dice quante");
        assert!(ElementHandle::find_by_accessible_label(&p, "Nome light.a, Acceso").next().is_some());
        elemento(&p, "Spegni tutte");
        elemento(&p, "Tutta la casa");
        // Da tastiera: Tab porta alla carta Casa (Agenti, la sua azione, Casa), Invio la apre.
        tasto(&p, Key::Backspace);
        tasto(&p, Key::Backspace);
        assert_eq!(p.get_vista(), "panoramica");
        for _ in 0..8 {
            if etichetta_col_fuoco(&p) == casa {
                break;
            }
            tasto(&p, Key::Tab);
        }
        assert_eq!(etichetta_col_fuoco(&p), casa);
        tasto(&p, Key::Return);
        assert_eq!((p.get_vista().as_str(), p.get_filtro_luci()), ("casa", true), "Invio sulla carta");
    }

    /// Backspace nella Casa filtrata torna all'elenco completo; il secondo, alla Panoramica.
    #[test]
    fn backspace_esce_dal_filtro_poi_dalla_casa() {
        use slint::platform::Key;
        let (p, _) = pannello_casa();
        p.set_luci(luci_accese_di_prova(&["light.a"]).into());
        p.set_filtro_luci(true);
        tasto(&p, Key::Backspace);
        assert_eq!((p.get_vista().as_str(), p.get_filtro_luci()), ("casa", false), "Backspace esce dal filtro");
        tasto(&p, Key::Backspace);
        assert_eq!(p.get_vista(), "panoramica");
    }

    /// La Casa filtrata da tastiera: Invio spegne la luce col fuoco e l'elenco si accorcia (come fa
    /// il ponte, con `set_vec`) senza perdere il fuoco: passa alla riga che prende il posto, o alla
    /// precedente se era l'ultima. ↑ porta a «Spegni tutte», → a «Tutta la casa»; Esc torna
    /// all'elenco completo prima di chiudere.
    #[test]
    fn casa_filtrata_da_tastiera_senza_perdere_il_fuoco() {
        use slint::platform::Key;
        let (p, eventi) = pannello_casa();
        let luci = luci_accese_di_prova(&["light.a", "light.b", "light.c"]);
        p.set_luci(luci.clone().into());
        p.set_filtro_luci(true);
        let (e1, l1) = (eventi.clone(), luci.clone());
        p.on_tocca(move |id| {
            e1.borrow_mut().push(format!("tocca {id}"));
            // Come il ponte: la luce spenta esce subito dall'elenco, e il modello si riscrive.
            let resto: Vec<_> = l1.iter().filter(|l| l.entity_id != id).collect();
            l1.set_vec(resto);
        });
        let e2 = eventi.clone();
        p.on_spegni_tutte(move || e2.borrow_mut().push("spegni tutte".into()));
        tasto(&p, Key::Return);
        assert_eq!(etichetta_col_fuoco(&p), "Nome light.b, Acceso", "spenta la prima, il fuoco è sulla riga al suo posto");
        // Fuoco sull'ultima riga: spenta, il fuoco va alla precedente.
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Nome light.c, Acceso");
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["tocca light.a", "tocca light.c"]);
        assert_eq!(etichetta_col_fuoco(&p), "Nome light.b, Acceso", "spenta l'ultima, il fuoco va alla più vicina");
        // ↑ dalla prima riga: «Spegni tutte».
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("spegni tutte"));
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Tutta la casa");
        tasto(&p, Key::Return);
        assert!(!p.get_filtro_luci(), "«Tutta la casa» torna all'elenco completo");
        // Di nuovo filtrata: Esc esce dal filtro, e solo il secondo chiude il pannello.
        p.set_filtro_luci(true);
        tasto(&p, Key::Escape);
        assert_eq!((p.get_filtro_luci(), eventi.borrow().last().map(String::as_str)), (false, Some("spegni tutte")));
        tasto(&p, Key::Escape);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi pannello"));
    }

    /// Gli elementi accessibili con ruolo di pulsante e senza nome.
    fn pulsanti_senza_nome(p: &Pannello) -> usize {
        i_slint_backend_testing::ElementQuery::from_root(p)
            .match_descendants()
            .find_all()
            .iter()
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Button) && e.accessible_label().is_none_or(|l| l.is_empty()))
            .count()
    }

    /// Senza connessione la Casa filtrata non ha pulsanti che non funzionano: dice lo stato.
    #[test]
    fn casa_filtrata_senza_connessione_dice_lo_stato() {
        let (p, _) = pannello_casa();
        p.set_luci(luci_accese_di_prova(&["light.a"]).into());
        p.set_filtro_luci(true);
        elemento(&p, "Spegni tutte");
        p.set_casa_pronta(false);
        p.set_stato_casa("Casa non connessa".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "Spegni tutte").next().is_none());
        assert!(titolo(&p, "Casa non connessa"));
    }

    /// Spente tutte: lo stato vuoto lo dice, con «Mostra tutta la casa» (anche da tastiera).
    #[test]
    fn casa_filtrata_senza_luci_accese_ha_lo_stato_vuoto() {
        use slint::platform::Key;
        let (p, _) = pannello_casa();
        p.set_luci(luci_accese_di_prova(&["light.a"]).into());
        p.set_filtro_luci(true);
        assert!(!titolo(&p, "Nessuna luce accesa"));
        p.set_luci(luci_accese_di_prova(&[]).into());
        assert!(titolo(&p, "Nessuna luce accesa"));
        assert!(ElementHandle::find_by_accessible_label(&p, "Spegni tutte").next().is_none(), "senza luci niente «Spegni tutte»");
        let pulsante = elemento(&p, "Mostra tutta la casa");
        assert_eq!(pulsante.accessible_role(), Some(AccessibleRole::Button));
        tasto(&p, Key::Return);
        assert!(!p.get_filtro_luci(), "Invio mostra tutta la casa");
        // Anche il clic.
        p.set_filtro_luci(true);
        elemento(&p, "Mostra tutta la casa").invoke_accessible_default_action();
        assert!(!p.get_filtro_luci());
    }

    fn con_modificatori_pannello(p: &Pannello, modificatori: &[slint::platform::Key], t: slint::platform::Key) {
        use slint::platform::WindowEvent;
        for m in modificatori {
            p.window().dispatch_event(WindowEvent::KeyPressed { text: (*m).into() });
        }
        tasto(p, t);
        for m in modificatori.iter().rev() {
            p.window().dispatch_event(WindowEvent::KeyReleased { text: (*m).into() });
        }
    }

    #[test]
    fn invio_non_consente_anche_col_fuoco_sul_pulsante() {
        // Il pulsante «Consenti» ha il fuoco vero (e sa premersi con Invio da solo), ma `tasti`
        // intercetta prima: Invio da solo non concede, Ctrl+Invio sì (UX3/FE15).
        // Ridisegno F1, Task 7: sul permesso aperto nella Coda.
        use slint::platform::Key;
        let (p, azioni) = coda_sul_consenso();
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio da solo ha consentito: {:?}", azioni.borrow());
        // Ctrl+Invio sì.
        use slint::platform::WindowEvent;
        p.window().dispatch_event(WindowEvent::KeyPressed { text: Key::Control.into() });
        tasto(&p, Key::Return);
        p.window().dispatch_event(WindowEvent::KeyReleased { text: Key::Control.into() });
        assert_eq!(*azioni.borrow(), ["consenti"]);
    }

    /// Un pannello sulla Coda col permesso «Permesso» (Nega / Consenti, testo corto) aperto e il
    /// fuoco vero su «Consenti» (→ «Nega», → «Apri», Invio apre, → «Consenti»); restituisce le
    /// azioni arrivate.
    fn coda_sul_consenso() -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        use slint::platform::Key;
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![voce_coda(
            "v",
            "Permesso",
            LivelloUi::Bloccante,
            "Ti aspetta · prova",
            true,
        )])));
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Apri", "nella riga il consenso è «Apri»");
        tasto(&p, Key::Return);
        lascia_girare(&p);
        assert_eq!(etichetta_col_fuoco(&p), "Nega", "aperto, il fuoco è su «Nega»");
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti", "il fuoco vero è su «Consenti»");
        assert!(azioni.borrow().is_empty());
        (p, azioni)
    }

    /// Il ruolo del primo elemento con un nome, dall'elemento col fuoco in su (vedi
    /// `etichetta_col_fuoco`): distingue la riga dal permesso aperto, che hanno lo stesso nome.
    fn ruolo_col_fuoco(p: &Pannello) -> i_slint_core::items::AccessibleRole {
        use i_slint_core::accessibility::AccessibleStringProperty;
        use i_slint_core::item_tree::ParentItemTraversalMode;
        let mut corrente = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
        while let Some(item) = corrente {
            if item.accessible_string_property(AccessibleStringProperty::Label).is_some_and(|e| !e.is_empty()) {
                return item.accessible_role();
            }
            corrente = item.parent_item(ParentItemTraversalMode::StopAtPopups);
        }
        i_slint_core::items::AccessibleRole::None
    }

    /// Dopo l'apertura di un permesso: lascia girare i `changed` del permesso (dice al pannello se
    /// il fondo del testo si è visto).
    fn lascia_girare(p: &Pannello) {
        let _ = ElementHandle::find_by_element_id(p, "Permesso::riquadro").count();
        slint::platform::update_timers_and_animations();
        let _ = ElementHandle::find_by_element_id(p, "Permesso::riquadro").count();
        slint::platform::update_timers_and_animations();
    }

    /// `tasto` con i modificatori premuti prima e rilasciati dopo.
    fn con_modificatori(p: &Pannello, modificatori: &[slint::platform::Key], t: slint::platform::Key) {
        use slint::platform::WindowEvent;
        for m in modificatori {
            p.window().dispatch_event(WindowEvent::KeyPressed { text: (*m).into() });
        }
        tasto(p, t);
        for m in modificatori.iter().rev() {
            p.window().dispatch_event(WindowEvent::KeyReleased { text: (*m).into() });
        }
    }

    #[test]
    fn coda_consenti_solo_con_ctrl_invio_esatto() {
        // Giro di correzioni 1, rilievo 1 (bloccante): col fuoco vero su «Consenti» nessuna
        // combinazione diversa da Ctrl+Invio deve concedere (UX3/FE15). Ridisegno F1, Task 7: la
        // regola rinasce sulla riga della Coda (la voce espansa non c'è più).
        // Ctrl+Alt+Invio: su Windows Slint legge Ctrl+Alt come AltGr e, se non nasce un carattere
        // alfanumerico, toglie tutti e due; nel backend di prova arriva quindi come Invio semplice.
        // Il caso resta (un AltGr non deve concedere), ma NON esercita il `!evento.modifiers.alt`
        // della clausola Ctrl+Invio in pannello.slint: quel ramo è coperto solo dalla prova dal
        // vivo con winit (giro di correzioni 1: Ctrl+Alt+Invio non ha concesso). Alt+Invio prova
        // l'altro ramo, quello che prende e ignora Invio con un modificatore.
        use slint::platform::Key;
        let casi: [(&str, &[Key], Key); 9] = [
            ("Invio", &[], Key::Return),
            ("Spazio", &[], Key::Space),
            ("Alt+Invio", &[Key::Alt], Key::Return),
            ("Ctrl+Spazio", &[Key::Control], Key::Space),
            ("Maiusc+Invio", &[Key::Shift], Key::Return),
            ("Alt+Spazio", &[Key::Alt], Key::Space),
            ("Ctrl+Alt+Invio (qui arriva come Invio semplice)", &[Key::Control, Key::Alt], Key::Return),
            ("AltGr+Invio", &[Key::AltGr], Key::Return),
            ("Maiusc+Ctrl+Invio", &[Key::Shift, Key::Control], Key::Return),
        ];
        for (nome, modificatori, t) in casi {
            let (p, azioni) = coda_sul_consenso();
            con_modificatori(&p, modificatori, t);
            assert!(azioni.borrow().is_empty(), "{nome} ha concesso: {:?}", azioni.borrow());
        }
        let (p, azioni) = coda_sul_consenso();
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"], "Ctrl+Invio concede");
    }

    #[test]
    fn invio_da_solo_non_consente() {
        // La voce vera di un permesso nella Coda: → entra su «Nega» (Invio = nega), il consenso solo
        // con Ctrl+Invio.
        use slint::platform::Key;
        let e = claude_hook::evento::EventoHook {
            sessione: "a".into(),
            tipo: claude_hook::evento::TipoEvento::PermissionRequest,
            cartella: "progetto".into(),
            strumento: Some("PowerShell".into()),
            notifica: None,
            errore: None,
            riassunto: Some("ls".into()),
        };
        let voce = crate::claude::voce_permesso(1, &e, "", std::time::Instant::now(), 0);
        let (p, azioni) = pannello_in_coda(ModelRc::new(VecModel::from(vec![VoceAttesaUi {
            id: voce.id.as_str().into(),
            livello: LivelloUi::Bloccante,
            titolo: voce.titolo.as_str().into(),
            sotto: "Ti aspetta · Agenti".into(),
            modulo: "Agenti".into(),
            dettaglio: voce.dettaglio.as_str().into(),
            icona: Default::default(),
            pulsanti: crate::pillola::pulsanti_ui(&voce.pulsanti),
            permesso: Default::default(),
        }])));
        // Invio sulla riga apre il permesso col testo intero, il fuoco su «Nega»: non risponde.
        tasto(&p, Key::Return);
        lascia_girare(&p);
        assert!(azioni.borrow().is_empty(), "Invio sulla riga ha risposto: {:?}", azioni.borrow());
        assert_eq!(etichetta_col_fuoco(&p), "Nega");
        tasto(&p, Key::Return);
        assert_eq!(*azioni.borrow(), ["nega"], "Invio da solo nega, non consente");
        azioni.borrow_mut().clear();
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Consenti", "testo corto: «Consenti» c'è");
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio su «Consenti» non concede: {:?}", azioni.borrow());
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"], "Ctrl+Invio concede");
    }

    #[test]
    fn pillola_mostra_piu_due() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("Claude chiede: Comando ls".into());
        assert_eq!(p.get_larghezza_altri(), 0.0);
        assert!(elemento(&p, "Claude chiede: Comando ls").accessible_role().is_some());
        p.set_altri(" +2".into());
        assert!(p.get_larghezza_altri() > 0.0, "« +2» ha il suo spazio");
        assert!(elemento(&p, "Claude chiede: Comando ls +2").accessible_role().is_some(), "Narrator lo legge");
        // Il testo tagliato non lo copre: un titolo lunghissimo si taglia, « +2» resta.
        p.set_messaggio("x".repeat(400).into());
        assert!(p.get_testo_tagliato());
        assert!(p.get_larghezza_altri() > 0.0);
    }

    #[test]
    fn la_pillola_non_concede_da_tastiera() {
        // Giro di correzioni 1, rilievo 2: anche con la finestra della pillola a fuoco, Invio o
        // Spazio non premono nulla (il permesso nella pillola si dà col clic o, col testo intero,
        // dal pannello). Che Tab non porti ai pulsanti lo prova `la_pillola_non_da_il_fuoco_ai_pulsanti`.
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("Posso?".into());
        p.set_pulsanti(ModelRc::new(VecModel::from(vec![
            PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false },
            PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true },
        ])));
        p.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione(move |_, id| a.borrow_mut().push(id.to_string()));
        for _ in 0..3 {
            for t in [Key::Tab, Key::Return, Key::Space] {
                p.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
                p.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
            }
        }
        assert!(azioni.borrow().is_empty(), "la tastiera ha premuto: {:?}", azioni.borrow());
    }

    #[test]
    fn la_pillola_non_da_il_fuoco_ai_pulsanti() {
        // Giro di correzioni 2, N1: Tab e Maiusc+Tab (ripetuti, nei due ordini dei pulsanti) non
        // portano il fuoco su un pulsante della pillola, quindi lì niente anello del fuoco. La
        // pillola non ha altro che possa prendere il fuoco: qualunque elemento col fuoco sarebbe
        // un suo pulsante.
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        for consenti_primo in [false, true] {
            let p = Pillola::new().expect("pillola");
            p.show().expect("pillola mostrata nel backend di prova");
            p.set_aspetto(AspettoPillolaUi::Allargata);
            let nega = PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false };
            let consenti = PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true };
            p.set_pulsanti(ModelRc::new(VecModel::from(if consenti_primo { vec![consenti, nega] } else { vec![nega, consenti] })));
            p.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
            for _ in 0..4 {
                for t in [Key::Tab, Key::Backtab] {
                    p.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
                    p.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
                    // Il fuoco (se c'è) sta sulla tastiera della pillola, mai su un pulsante né su
                    // qualcosa dentro un pulsante (da cui i tasti risalirebbero al pulsante).
                    let col_fuoco = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
                    let dove = col_fuoco
                        .as_ref()
                        .and_then(|f| f.element_type_names_and_ids(0))
                        .and_then(|v| v.first().map(|(_, id)| id.to_string()))
                        .unwrap_or_default();
                    assert!(
                        col_fuoco.is_none() || dove == "Pillola::tastiera",
                        "consenti_primo={consenti_primo}: dopo {t:?} il fuoco è su «{dove}»"
                    );
                }
            }
        }
    }

    // ─── Task 14f: collegamenti ─────────────────────────────────────────────

    fn collegamento(id: &str, nome: &str, url: &str) -> crate::CollegamentoUi {
        crate::CollegamentoUi {
            id: id.into(),
            nome: nome.into(),
            url: url.into(),
            corto: crate::collegamenti::indirizzo_corto(url).into(),
            iniziale: crate::collegamenti::iniziale(nome).into(),
            tono: crate::collegamenti::tono(nome),
        }
    }

    fn quattro_collegamenti() -> ModelRc<crate::CollegamentoUi> {
        ModelRc::new(VecModel::from(vec![
            collegamento("proxmox", "Proxmox", "https://esempio.lan:8006"),
            collegamento("nas", "NAS", "http://nas.esempio.lan/admin?x=1"),
            collegamento("router", "Router", "https://router.esempio.lan"),
            collegamento("stampante", "Stampante", "http://192.0.2.20"),
        ]))
    }

    /// Un pannello sulla vista Collegamenti; restituisce gli eventi (aperture, «gestisci»,
    /// chiusura del pannello).
    fn pannello_collegamenti(voci: ModelRc<crate::CollegamentoUi>) -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_collegamenti(voci);
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3) = (eventi.clone(), eventi.clone(), eventi.clone());
        p.on_apri_collegamento(move |id| e1.borrow_mut().push(format!("apri {id}")));
        p.on_gestisci_collegamenti(move || e2.borrow_mut().push("gestisci".into()));
        p.on_chiudi(move || e3.borrow_mut().push("chiudi pannello".into()));
        (p, eventi)
    }

    /// La barra ha «Collegamenti» (ultima scheda); la vista mostra le card (nome e indirizzo
    /// accorciato per Narrator, pulsanti) nell'ordine dato; un tocco chiede di aprire quell'id.
    #[test]
    fn vista_collegamenti_card_accessibili() {
        let (p, eventi) = pannello_collegamenti(quattro_collegamenti());
        let scheda = elemento(&p, "Collegamenti, scheda 5 di 5");
        assert_eq!(scheda.accessible_role(), Some(AccessibleRole::Tab));
        scheda.invoke_accessible_default_action();
        assert_eq!(p.get_vista(), "collegamenti");
        let proxmox = elemento(&p, "Proxmox, esempio.lan:8006");
        assert_eq!(proxmox.accessible_role(), Some(AccessibleRole::Button));
        assert_eq!(proxmox.accessible_description().as_deref(), Some("Si apre nel browser"));
        assert!(ElementHandle::find_by_accessible_label(&p, "NAS, nas.esempio.lan/admin").next().is_some(), "senza query");
        // Ordine: Proxmox, NAS e Router nella prima riga (3 colonne), Stampante sotto.
        let x = |nome: &str| elemento(&p, nome).absolute_position();
        let (a, b, c, d) = (
            x("Proxmox, esempio.lan:8006"),
            x("NAS, nas.esempio.lan/admin"),
            x("Router, router.esempio.lan"),
            x("Stampante, 192.0.2.20"),
        );
        assert!(a.x < b.x && b.x < c.x && a.y == c.y && d.y > a.y && d.x == a.x, "{a:?} {b:?} {c:?} {d:?}");
        elemento(&p, "NAS, nas.esempio.lan/admin").invoke_accessible_default_action();
        elemento(&p, "Modifica i collegamenti").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["apri nas", "gestisci"]);
    }

    /// Da tastiera: frecce fra le card (3 colonne), Invio apre; ↑ dalla prima riga porta a
    /// «Modifica», Invio lì apre le Impostazioni; Esc chiude il pannello (un solo livello).
    #[test]
    fn vista_collegamenti_da_tastiera() {
        use slint::platform::Key;
        let (p, eventi) = pannello_collegamenti(quattro_collegamenti());
        // Dal dock: Tab, → fino a «Collegamenti», Invio.
        tasto(&p, Key::Tab);
        for _ in 0..4 {
            tasto(&p, Key::RightArrow);
        }
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "collegamenti");
        // Il fuoco va sul contenuto (dalle schede Tab passa dalla ricerca e da «⋯», senza volume,
        // e poi al contenuto: «Indietro» non c'è più): prima card; ↓ porta alla quarta (sotto la
        // prima), Invio.
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        // → dall'ultima resta lì; ↑ torna alla prima riga, → alla seconda card.
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        // ↑ dalla prima riga: «Modifica».
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri stampante", "apri nas", "gestisci"]);
        let col_fuoco = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
        assert!(col_fuoco.is_some(), "il pannello ha ancora il fuoco");
        tasto(&p, Key::Escape);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi pannello"));
    }

    /// Senza collegamenti: lo stato vuoto con «Aggiungi un collegamento», che apre le
    /// Impostazioni (col mouse, con Narrator e con Invio).
    #[test]
    fn vista_collegamenti_vuota() {
        use slint::platform::Key;
        let (p, eventi) = pannello_collegamenti(ModelRc::new(VecModel::from(Vec::<crate::CollegamentoUi>::new())));
        p.set_vista("collegamenti".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "Nessun collegamento").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Modifica i collegamenti").next().is_none(), "niente da modificare");
        let aggiungi = elemento(&p, "Aggiungi un collegamento");
        assert_eq!(aggiungi.accessible_role(), Some(AccessibleRole::Button));
        aggiungi.invoke_accessible_default_action();
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["gestisci", "gestisci"]);
    }

    /// Un errore di apertura compare nella testata della vista.
    #[test]
    fn vista_collegamenti_dice_se_non_si_e_aperto() {
        let (p, _) = pannello_collegamenti(quattro_collegamenti());
        p.set_vista("collegamenti".into());
        // R1-23: la testata non ripete più «COLLEGAMENTI» né «Si aprono nel browser»; resta il
        // suggerimento sulla card (per Narrator) e, se serve, l'errore.
        assert!(ElementHandle::find_by_accessible_label(&p, "Si aprono nel browser").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&p, "COLLEGAMENTI").next().is_none());
        // L'iniziale nel cerchio è decorativa: Narrator legge solo il nome della card.
        assert!(ElementHandle::find_by_accessible_label(&p, "P").next().is_none(), "iniziale di Proxmox esposta");
        assert!(ElementHandle::find_by_accessible_label(&p, "S").next().is_none(), "iniziale di Stampante esposta");
        assert!(ElementHandle::find_by_accessible_label(&p, "Proxmox, esempio.lan:8006").next().is_some());
        p.set_errore_collegamenti("«NAS» non si è aperto".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "«NAS» non si è aperto").next().is_some());
    }

    // ─── Task 9: vista Claude ───────────────────────────────────────────────

    fn riga_claude(id: &str, cartella: &str, stato: &str, livello: crate::LivelloUi) -> crate::RigaSessioneUi {
        crate::RigaSessioneUi {
            id: id.into(),
            cartella: cartella.into(),
            titolo: Default::default(),
            stato: stato.into(),
            livello,
            strumento: Default::default(),
            durata: Default::default(),
            contesto: Default::default(),
            contesto_caldo: false,
            terminale: Default::default(),
            avviso: Default::default(),
        }
    }

    fn tre_righe_claude() -> ModelRc<crate::RigaSessioneUi> {
        ModelRc::new(VecModel::from(vec![
            riga_claude("aaaa", "uno", "Tocca a te", crate::LivelloUi::Attenzione),
            riga_claude("bbbb", "due", "Fatto", crate::LivelloUi::Informazione),
            riga_claude("cccc", "tre", "Lavora", crate::LivelloUi::Nessuno),
        ]))
    }

    fn limite_claude(testo: &str, frazione: f32, azzera: &str) -> crate::LimiteClaudeUi {
        crate::LimiteClaudeUi { presente: true, frazione, testo: testo.into(), azzera: azzera.into(), vecchio: false }
    }

    /// Un pannello sulla vista Claude; restituisce gli eventi («apri <id>», «gestisci»).
    fn pannello_claude(
        collegato: bool,
        righe: ModelRc<crate::RigaSessioneUi>,
    ) -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_claude_collegato(collegato);
        p.set_claude_righe(righe);
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2) = (eventi.clone(), eventi.clone());
        p.on_apri_sessione(move |id| e1.borrow_mut().push(format!("apri {id}")));
        p.on_gestisci_claude(move || e2.borrow_mut().push("gestisci".into()));
        p.set_vista("agenti".into());
        (p, eventi)
    }

    /// Scollegato: la frase e «Impostazioni» (col mouse, con Narrator e con Invio); collegato ma
    /// senza sessioni: «Nessuna sessione aperta», senza pulsanti.
    #[test]
    fn vista_claude_stati_vuoti() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(false, ModelRc::new(VecModel::from(Vec::<crate::RigaSessioneUi>::new())));
        assert!(ElementHandle::find_by_accessible_label(&p, "Collega Claude Code dalle Impostazioni").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Nessuna sessione aperta").next().is_none());
        let impostazioni = elemento(&p, "Impostazioni");
        assert_eq!(impostazioni.accessible_role(), Some(AccessibleRole::Button));
        impostazioni.invoke_accessible_default_action();
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["gestisci", "gestisci"]);

        p.set_claude_collegato(true);
        assert!(ElementHandle::find_by_accessible_label(&p, "Nessuna sessione aperta").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Collega Claude Code dalle Impostazioni").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&p, "Impostazioni").next().is_none(), "niente pulsante");
        // Invio senza righe non fa nulla.
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().len(), 2);
    }

    /// PF9: un limite assente non ha l'anello (mai «0%»); senza nessuno dei due la card dice
    /// «Limiti non disponibili»; con i dati gli anelli hanno nome, valore e quando si azzerano.
    #[test]
    fn anelli_nascosti_senza_abbonamento() {
        let (p, _) = pannello_claude(true, tre_righe_claude());
        let ha = |nome: &str| ElementHandle::find_by_accessible_label(&p, nome).next().is_some();
        assert!(!ha("Limite 5 ore") && !ha("Limite 7 giorni"), "senza limiti nessun anello");
        assert!(ha("Limiti non disponibili"));

        p.set_claude_cinque_ore(limite_claude("37%", 0.37, "si azzera alle 14:30"));
        assert!(ha("Limite 5 ore") && !ha("Limite 7 giorni"), "solo la finestra che c'è");
        assert!(!ha("Limiti non disponibili"));
        let anello = elemento(&p, "Limite 5 ore");
        assert_eq!(anello.accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert_eq!(anello.accessible_value().as_deref(), Some("37%"));
        assert_eq!(anello.accessible_description().as_deref(), Some("si azzera alle 14:30"));

        p.set_claude_sette_giorni(limite_claude("61%", 0.61, "si azzera sabato alle 09:00"));
        assert_eq!(elemento(&p, "Limite 7 giorni").accessible_value().as_deref(), Some("61%"));
        // Un limite scaduto torna assente: l'anello sparisce.
        p.set_claude_cinque_ore(Default::default());
        assert!(!ha("Limite 5 ore") && ha("Limite 7 giorni"));
    }

    /// La spesa del mese: il valore e la nota; senza valore solo la nota (nessuno «0»).
    #[test]
    fn vista_claude_spesa_del_mese() {
        let (p, _) = pannello_claude(true, tre_righe_claude());
        let nota = "Aggiungi una chiave da amministratore nelle Impostazioni";
        p.set_claude_spesa("".into());
        p.set_claude_spesa_nota(nota.into());
        assert_eq!(elemento(&p, "Spesa API del mese").accessible_value().as_deref(), Some(nota));
        p.set_claude_spesa("412,80 $".into());
        p.set_claude_spesa_nota("aggiornato alle 16:02".into());
        let spesa = elemento(&p, "Spesa API del mese");
        assert_eq!(spesa.accessible_value().as_deref(), Some("412,80 $"));
        assert_eq!(spesa.accessible_description().as_deref(), Some("aggiornato alle 16:02"));
    }

    /// Il dettaglio di «Limiti non disponibili» dice il vero: scollegato si collega, collegato si aspetta.
    #[test]
    fn limiti_non_disponibili_spiega_il_motivo() {
        let (p, _) = pannello_claude(false, ModelRc::new(VecModel::from(Vec::<crate::RigaSessioneUi>::new())));
        let ha = |t: &str| ElementHandle::find_by_accessible_label(&p, t).next().is_some();
        assert!(ha("Si vedono dopo aver collegato Claude Code."));
        p.set_claude_collegato(true);
        assert!(ha("Compaiono dopo la prima risposta di una sessione, con un abbonamento Claude."));
        assert!(!ha("Si vedono dopo aver collegato Claude Code."));
    }

    /// Il fuoco è un indice che Rust tiene sulla sessione giusta (l'ordine è fermo): le frecce lo
    /// muovono e Invio apre la riga che lo ha, sempre dentro le righe che ci sono.
    #[test]
    fn il_fuoco_delle_righe_e_un_indice_sempre_valido() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(true, tre_righe_claude());
        tasto(&p, Key::DownArrow);
        assert_eq!(p.get_claude_fuoco(), 1);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri bbbb"]);
        // Rust sposta il fuoco (la sua sessione è scesa): Invio apre quella riga.
        p.set_claude_fuoco(2);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"));
        // Restano meno righe e l'indice non è ancora stato rimesso: Invio non va fuori dalle righe.
        p.set_claude_righe(ModelRc::new(VecModel::from(vec![riga_claude("aaaa", "uno", "Lavora", crate::LivelloUi::Nessuno)])));
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri aaaa"));
        tasto(&p, Key::DownArrow);
        assert_eq!(p.get_claude_fuoco(), 0, "dall'ultima riga ↓ resta lì");
    }

    /// ↓ due volte e Invio: `apri-sessione` con l'id della terza riga; ↑ risale, e i bordi tengono.
    #[test]
    fn righe_da_tastiera() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(true, tre_righe_claude());
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri cccc"]);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"), "in fondo resta lì");
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri bbbb"));
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri aaaa"));
        // Narrator e il mouse: la riga è un pulsante col nome e lo stato.
        let riga = elemento(&p, "tre, Lavora");
        assert_eq!(riga.accessible_role(), Some(AccessibleRole::Button));
        riga.invoke_accessible_default_action();
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"));
        // Esc chiude il pannello come altrove.
        let chiuso = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiuso.clone();
        p.on_chiudi(move || c.set(true));
        tasto(&p, Key::Escape);
        assert!(chiuso.get());
    }

    /// Con la chiave da amministratore «Dettagli» sta sotto l'ultima riga: ↓ dall'ultima ci arriva,
    /// Invio apre le schede, ↑ torna alla riga; senza la chiave il pulsante non c'è.
    #[test]
    fn dettagli_da_tastiera_e_da_narrator() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(true, tre_righe_claude());
        assert!(ElementHandle::find_by_accessible_label(&p, "Dettagli delle API").next().is_none(), "senza la chiave non c'è");
        let e = eventi.clone();
        p.on_apri_dettagli_claude(move || e.borrow_mut().push("dettagli".into()));
        p.set_claude_dettagli(true);
        let pulsante = elemento(&p, "Dettagli delle API");
        assert_eq!(pulsante.accessible_role(), Some(AccessibleRole::Button));
        // ↓ fino all'ultima riga e poi su «Dettagli»; Invio apre le schede.
        for _ in 0..3 {
            tasto(&p, Key::DownArrow);
        }
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["dettagli"]);
        // ↑ torna alla riga, Invio apre la sessione; ancora ↑ sale di una riga.
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"));
        // Il clic o Narrator sul pulsante fanno la stessa cosa.
        pulsante.invoke_accessible_default_action();
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("dettagli"));
    }

    /// Scollegato ma con la chiave: ↓ da «Impostazioni» porta su «Dettagli», ↑ torna.
    #[test]
    fn dettagli_anche_da_scollegato() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(false, ModelRc::new(VecModel::from(Vec::<crate::RigaSessioneUi>::new())));
        let e = eventi.clone();
        p.on_apri_dettagli_claude(move || e.borrow_mut().push("dettagli".into()));
        p.set_claude_dettagli(true);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["gestisci"]);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("dettagli"));
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("gestisci"));
    }

    /// Il caso più pieno (tre righe con tutto, due anelli, spesa con la nota più lunga): nulla esce
    /// dalla vista e la vista sta nel pannello.
    #[test]
    fn vista_claude_entra_nel_pannello() {
        let mut righe = Vec::new();
        for (i, stato) in ["Tocca a te", "Fatto", "Lavora"].iter().enumerate() {
            righe.push(crate::RigaSessioneUi {
                // Anche col titolo più lungo (60 caratteri) e la cartella: nulla esce dalla vista.
                titolo: "Un titolo di sessione davvero lungo, da sessanta caratteri…".into(),
                strumento: "Modifica".into(),
                durata: "1:02:05".into(),
                contesto: "100%".into(),
                contesto_caldo: true,
                terminale: "ultimo terminale usato".into(),
                ..riga_claude(&format!("id{i}"), "una-cartella-dal-nome-davvero-molto-lungo", stato, crate::LivelloUi::Attenzione)
            });
        }
        let (p, _) = pannello_claude(true, ModelRc::new(VecModel::from(righe)));
        p.set_claude_cinque_ore(limite_claude("100%", 1.0, "si azzera domani alle 14:30"));
        p.set_claude_sette_giorni(limite_claude("100%", 1.0, "si azzera mercoledì alle 14:30"));
        p.set_claude_spesa_nota("Aggiungi una chiave da amministratore nelle Impostazioni".into());
        let vista = ElementHandle::find_by_element_type_name(&p, "VistaClaude").next().expect("vista Claude");
        let (cp, cs) = (vista.absolute_position(), vista.size());
        for e in vista.query_descendants().find_all() {
            let (pos, lato) = (e.absolute_position(), e.size());
            if lato.width <= 0.0 || lato.height <= 0.0 {
                continue;
            }
            assert!(
                pos.x >= cp.x - 0.5
                    && pos.y >= cp.y - 0.5
                    && pos.x + lato.width <= cp.x + cs.width + 0.5
                    && pos.y + lato.height <= cp.y + cs.height + 0.5,
                "{:?} «{}» esce dalla vista: {pos:?} {lato:?} fuori da {cp:?} {cs:?}",
                e.type_name(),
                e.accessible_label().unwrap_or_default()
            );
        }
        // Dentro il corpo della cornice: 752 × 224 a (34, 44).
        assert!(cp.x >= 34.0 && cp.y >= 44.0 && cp.x + cs.width <= 34.0 + 752.0 && cp.y + cs.height <= 44.0 + 224.0, "{cp:?} {cs:?}");
    }

    /// Le righe di una vista con `n` sessioni che lavorano («s0», «s1»…).
    fn righe_che_lavorano(n: usize) -> ModelRc<crate::RigaSessioneUi> {
        ModelRc::new(VecModel::from(
            (0..n).map(|i| riga_claude(&format!("id{i}"), &format!("s{i}"), "Lavora", crate::LivelloUi::Nessuno)).collect::<Vec<_>>(),
        ))
    }

    /// Il rettangolo (alto, basso) di un elemento e quello della vista, in coordinate della finestra.
    fn y_di(e: &ElementHandle) -> (f32, f32) {
        (e.absolute_position().y, e.absolute_position().y + e.size().height)
    }

    /// RC-52: con dieci sessioni la colonna scorre: tutte raggiungibili con le frecce, la riga col
    /// fuoco resta sempre dentro la vista (in basso e, tornando su, in alto).
    #[test]
    fn la_colonna_delle_sessioni_scorre_e_tiene_in_vista_il_fuoco() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(true, righe_che_lavorano(10));
        let vista = ElementHandle::find_by_element_type_name(&p, "VistaClaude").next().expect("vista Claude");
        let (alto, basso) = y_di(&vista);
        // Una riga fuori dalla vista non è nemmeno esposta: «dentro» è falso.
        let dentro = |nome: &str| {
            ElementHandle::find_by_accessible_label(&p, nome).next().is_some_and(|e| {
                let (a, b) = y_di(&e);
                a >= alto - 0.5 && b <= basso + 0.5
            })
        };
        assert!(dentro("s0, Lavora") && !dentro("s9, Lavora"), "all'inizio l'ultima è fuori vista");
        for _ in 0..9 {
            tasto(&p, Key::DownArrow);
        }
        assert_eq!(p.get_claude_fuoco(), 9);
        assert!(dentro("s9, Lavora"), "il fuoco ha fatto scorrere la colonna");
        assert!(!dentro("s0, Lavora"));
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri id9"));
        for _ in 0..9 {
            tasto(&p, Key::UpArrow);
        }
        assert!(dentro("s0, Lavora"), "tornando su la colonna risale");
        assert!(!dentro("s9, Lavora"));
        // «+N altre» non c'è più: nessun testo del genere.
        assert!(ElementHandle::find_by_accessible_label(&p, "e altre 7 sessioni").next().is_none());
    }

    /// RC-52: due sessioni stanno ferme, senza bisogno di scorrere.
    #[test]
    fn con_poche_sessioni_niente_scorrimento() {
        let (p, _) = pannello_claude(true, righe_che_lavorano(3));
        let vista = ElementHandle::find_by_element_type_name(&p, "VistaClaude").next().expect("vista Claude");
        let (alto, basso) = y_di(&vista);
        for i in 0..3 {
            let (a, b) = y_di(&elemento(&p, &format!("s{i}, Lavora")));
            assert!(a >= alto - 0.5 && b <= basso + 0.5, "s{i}");
        }
        assert!((y_di(&elemento(&p, "s0, Lavora")).0 - alto).abs() < 0.5, "la prima è in cima");
    }

    /// RC-57: «ultimo terminale usato» è un'icona col nome (per Narrator), non il testo su ogni
    /// riga; senza ripiego non c'è.
    #[test]
    fn l_ultimo_terminale_usato_e_un_icona_con_il_nome() {
        let con = |id: &str, cartella: &str, terminale: &str| crate::RigaSessioneUi {
            terminale: terminale.into(),
            ..riga_claude(id, cartella, "Fatto", crate::LivelloUi::Informazione)
        };
        let righe = ModelRc::new(VecModel::from(vec![
            con("a", "uno", "ultimo terminale usato"),
            con("b", "due", ""),
            con("c", "tre", "ultimo terminale usato"),
        ]));
        let (p, eventi) = pannello_claude(true, righe);
        let icone: Vec<_> = ElementHandle::find_by_accessible_label(&p, "ultimo terminale usato").collect();
        assert_eq!(icone.len(), 2, "una per riga col ripiego, nessuna per le altre");
        assert!(icone.iter().all(|i| i.accessible_role() == Some(AccessibleRole::Image)));
        // La riga non ripete la frase nella sua descrizione (la dice l'icona).
        assert!(!elemento(&p, "uno, Fatto").accessible_description().unwrap_or_default().contains("terminale"));
        // La riga resta il pulsante che apre la sessione.
        elemento(&p, "uno, Fatto").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["apri a"]);
    }

    /// Due sessioni nella stessa cartella («bin») si distinguono dal titolo: la riga si chiama
    /// «titolo, cartella, stato» per Narrator; senza titolo resta «cartella, stato». Il titolo dà il
    /// nome visibile della riga e la cartella sta sotto, attenuata (vedi `RigaClaude`).
    #[test]
    fn la_riga_col_titolo_porta_titolo_e_cartella() {
        let con_titolo = |id: &str, titolo: &str| crate::RigaSessioneUi {
            titolo: titolo.into(),
            ..riga_claude(id, "bin", "Fatto", crate::LivelloUi::Informazione)
        };
        let righe = ModelRc::new(VecModel::from(vec![
            con_titolo("a", "Sistemare il restyle"),
            con_titolo("b", "Aggiornare i test"),
            con_titolo("c", ""),
        ]));
        let (p, eventi) = pannello_claude(true, righe);
        elemento(&p, "Aggiornare i test, bin, Fatto").invoke_accessible_default_action();
        elemento(&p, "Sistemare il restyle, bin, Fatto").invoke_accessible_default_action();
        elemento(&p, "bin, Fatto").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["apri b", "apri a", "apri c"]);
        // La riga col titolo non si chiama più solo «bin»: nessun elemento ambiguo a due righe.
        assert_eq!(ElementHandle::find_by_accessible_label(&p, "bin, Fatto").count(), 1);
    }

    /// Impostazioni › Claude (M3, Task 7 e 7B): l'anteprima si apre col fuoco su «Annulla» (Invio per
    /// sbaglio non scrive), «Scrivi in settings.json» è raggiungibile con Tab, Esc annulla; lo
    /// scollegamento chiede conferma; i pulsanti hanno il loro nome per Narrator.
    #[test]
    fn sezione_claude_anteprima_conferma_e_annulla() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&'static str>::new()));
        let debole = f.as_weak();
        let e = eventi.clone();
        f.on_claude_prepara(move || {
            e.borrow_mut().push("prepara");
            let f = debole.upgrade().unwrap();
            f.set_claude_anteprima("{\n  \"hooks\": {}\n}".into());
            f.set_claude_avvisi("La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili.".into());
            f.set_claude_file("settings-finto.json".into());
            f.set_claude_passo(1);
        });
        let e = eventi.clone();
        f.on_claude_conferma(move || e.borrow_mut().push("conferma"));
        let e = eventi.clone();
        f.on_claude_annulla(move || e.borrow_mut().push("annulla"));
        let e = eventi.clone();
        f.on_claude_scollega(move || e.borrow_mut().push("scollega"));
        let tasto = |k: Key| {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: k.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: k.into() });
        };
        let ce = |nome: &str| ElementHandle::find_by_accessible_label(&f, nome).next().is_some();

        // A riposo, non collegato: solo «Collega Claude Code», e la nota sui permessi nel terminale
        // (nessun «Rigenera il codice», nessuna nota sul terminale da riaprire: il codice non c'è più).
        assert!(ce("Collega Claude Code"));
        assert!(!ce("Scrivi in settings.json") && !ce("Rigenera il codice") && !ce("Scollega"));
        assert!(ce("Con isola chiusa Claude Code prosegue da solo: i permessi li decidi nel terminale."));
        assert!(!ce("Codice segreto") && !ce("Copia il codice"));
        assert!(ce("Non collegato"));

        // Anteprima: avviso, riquadro leggibile, i due pulsanti.
        elemento(&f, "Collega Claude Code").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["prepara"]);
        assert!(ce("Scrivi in settings.json") && ce("Annulla"));
        assert!(ce("Voci di isola come resteranno in settings.json"));
        assert!(ce("La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili."));
        assert!(ce("Il resto di settings.json resta com'è (la prima volta ne tengo una copia accanto). Le sessioni di Claude Code avviate dopo il collegamento lo useranno."));
        assert!(!ce("I terminali aperti prima vanno chiusi e riaperti."));
        assert!(!ce("Collega Claude Code"), "durante l'anteprima il pulsante non c'è");

        // Il fuoco parte da «Annulla»: Invio annulla (non scrive).
        tasto(Key::Return);
        assert_eq!(*eventi.borrow(), ["prepara", "annulla"]);
        assert!(!ce("Scrivi in settings.json"));
        assert_eq!(f.get_claude_passo(), 0);

        // Di nuovo: Tab porta a «Scrivi in settings.json» e Invio conferma.
        elemento(&f, "Collega Claude Code").invoke_accessible_default_action();
        tasto(Key::Tab);
        tasto(Key::Return);
        assert_eq!(eventi.borrow().last(), Some(&"conferma"), "{:?}", eventi.borrow());
        // Esc dall'anteprima annulla.
        tasto(Key::Escape);
        assert_eq!(eventi.borrow().last(), Some(&"annulla"));
        assert_eq!(f.get_claude_passo(), 0);

        // Collegato: solo Scollega (niente «Rigenera il codice»); chiede conferma, col fuoco su «Annulla».
        eventi.borrow_mut().clear();
        f.set_claude_collegato(true);
        f.set_claude_stato("Collegato".into());
        assert!(ce("Collegato") && !ce("Rigenera il codice"));
        elemento(&f, "Scollega").invoke_accessible_default_action();
        assert_eq!(f.get_claude_passo(), 2);
        assert!(ce("Togliere il collegamento con Claude Code?"));
        assert!(
            ce("Da settings.json escono solo le voci di isola (hook e barra di stato); il resto resta com'è."),
            "la conferma dice ciò che succede davvero"
        );
        assert!(!ce("Da settings.json escono solo le voci di isola, il resto resta com'è; il codice segreto viene cancellato."));
        tasto(Key::Return);
        assert_eq!(eventi.borrow().last(), Some(&"annulla"));
        assert_eq!(f.get_claude_passo(), 0);
        elemento(&f, "Scollega").invoke_accessible_default_action();
        tasto(Key::Tab);
        tasto(Key::Return);
        assert_eq!(eventi.borrow().last(), Some(&"scollega"), "{:?}", eventi.borrow());

        // Fuori da Windows: «Disponibile solo su Windows», nessun pulsante per collegare.
        f.set_claude_passo(0);
        f.set_claude_collegato(false);
        assert!(ce("Collega Claude Code"));
        f.set_claude_disponibile(false);
        f.set_claude_stato("Disponibile solo su Windows".into());
        assert!(ce("Disponibile solo su Windows") && !ce("Collega Claude Code"));
    }

    /// RC-31: aperta l'anteprima nella finestra della misura di serie, i due pulsanti stanno interi
    /// nella parte visibile (prima «Scrivi in settings.json» finiva mezzo sotto il bordo).
    #[test]
    fn anteprima_claude_coi_pulsanti_in_vista() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(720.0, 520.0));
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let debole = f.as_weak();
        f.on_claude_prepara(move || {
            let f = debole.upgrade().unwrap();
            f.set_claude_anteprima((0..30).map(|i| format!("  \"voce{i}\": {i},\n")).collect::<String>().into());
            f.set_claude_avvisi("La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili.".into());
            f.set_claude_file(r"C:\Users\prova\.claude\settings.json".into());
            f.set_claude_passo(1);
        });
        elemento(&f, "Collega Claude Code").invoke_accessible_default_action();
        let alto = f.window().size().to_logical(f.window().scale_factor()).height;
        for nome in ["Annulla", "Scrivi in settings.json"] {
            let p = ElementHandle::find_by_accessible_label(&f, nome)
                .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
                .unwrap_or_else(|| panic!("pulsante «{nome}»"));
            let (y, h) = (p.absolute_position().y, p.size().height);
            // Il contenuto finisce 24 sopra il bordo della finestra e comincia sotto il filo.
            assert!(y >= 18.0 && y + h <= alto - 24.0 + 0.5, "«{nome}» fuori vista: y {y} + {h} su {alto}");
        }
        assert!(col_fuoco(&f).ends_with("annulla-claude"), "fuoco su «{}»", col_fuoco(&f));
    }

    /// Posizione (nella finestra) e misura dell'elemento col fuoco vero.
    fn dove_il_fuoco(f: &FinestraImpostazioni) -> Option<(f32, f32, f32, f32)> {
        let fuoco = i_slint_core::window::WindowInner::from_pub(f.window()).focus_item.borrow().upgrade()?;
        // `map_to_window` parte dall'origine del genitore: la posizione dell'elemento va aggiunta.
        let g = fuoco.geometry();
        let p = fuoco.map_to_window(g.origin);
        Some((p.x, p.y, g.size.width, g.size.height))
    }

    /// Il fuoco vero è su questo elemento (stessa posizione nella finestra).
    fn ha_il_fuoco(f: &FinestraImpostazioni, e: &ElementHandle) -> bool {
        let p = e.absolute_position();
        dove_il_fuoco(f).is_some_and(|(x, y, _, _)| (x - p.x).abs() < 0.5 && (y - p.y).abs() < 0.5)
    }

    /// Fix 1, I-1 e M-1: da tastiera, dopo «Salva» il fuoco va su «Togli» della card della chiave
    /// (non al primo campo degli Avvisi); annullata la conferma torna su «Togli»; tolta la chiave va
    /// al campo «Chiave» che ricompare.
    #[test]
    fn il_fuoco_segue_la_chiave_salvata_e_tolta() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(760.0, 1400.0));
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let debole = f.as_weak();
        f.on_api_salva(move |_| {
            let f = debole.upgrade().unwrap();
            f.set_api_campo("".into());
            f.set_api_ultime("0000".into());
            f.set_api_presente(true);
        });
        let debole = f.as_weak();
        f.on_api_togli(move || debole.upgrade().unwrap().set_api_presente(false));
        let chiave = || campo(&f, "Chiave da amministratore");
        // Tab fino al campo della chiave, si scrive, Invio.
        for _ in 0..30 {
            if ha_il_fuoco(&f, &chiave()) {
                break;
            }
            tasto_finestra(&f, Key::Tab);
        }
        assert!(
            ha_il_fuoco(&f, &chiave()),
            "Tab arriva al campo della chiave: fuoco «{}» {:?}, campo {:?}",
            col_fuoco(&f),
            dove_il_fuoco(&f),
            chiave().absolute_position()
        );
        scrivi(&f, "sk-ant-admin01-finta");
        tasto_finestra(&f, Key::Return);
        assert!(f.get_api_presente());
        assert!(col_fuoco(&f).ends_with("togli-chiave"), "dopo «Salva» il fuoco è su «{}»", col_fuoco(&f));
        // Invio su «Togli» apre la conferma; Invio su «Annulla» la chiude e il fuoco torna su «Togli».
        tasto_finestra(&f, Key::Return);
        assert!(col_fuoco(&f).ends_with("annulla-togli"), "«{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert!(col_fuoco(&f).ends_with("togli-chiave"), "dopo «Annulla» il fuoco è su «{}»", col_fuoco(&f));
        // Di nuovo, e questa volta «Togli» rosso: il fuoco va al campo che ricompare.
        tasto_finestra(&f, Key::Return);
        tasto_finestra(&f, Key::Tab);
        tasto_finestra(&f, Key::Return);
        assert!(!f.get_api_presente());
        assert!(ha_il_fuoco(&f, &chiave()), "tolta la chiave il fuoco è sul campo, non su «{}»", col_fuoco(&f));
    }

    /// T6 m-1 (revisione finale): una chiave rifiutata da Rust non lascia acceso il segnale del
    /// fuoco su «Togli»: se poi la chiave compare per altra via, «Togli» non ruba il fuoco.
    #[test]
    fn chiave_rifiutata_non_lascia_il_fuoco_su_togli() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(760.0, 1400.0));
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        // Rust rifiuta: la chiave non si salva.
        f.on_api_salva(|_| {});
        let chiave = || campo(&f, "Chiave da amministratore");
        for _ in 0..30 {
            if ha_il_fuoco(&f, &chiave()) {
                break;
            }
            tasto_finestra(&f, Key::Tab);
        }
        assert!(ha_il_fuoco(&f, &chiave()), "Tab arriva al campo della chiave");
        scrivi(&f, "sbagliata");
        tasto_finestra(&f, Key::Return);
        assert!(!f.get_api_presente());
        f.set_api_presente(true);
        // «Togli» nasce (la ricerca fa nascere gli elementi creati con `if`).
        assert!(ElementHandle::find_by_accessible_label(&f, "Togli").next().is_some());
        assert!(!col_fuoco(&f).ends_with("togli-chiave"), "«Togli» ha preso il fuoco da un salvataggio rifiutato");
    }

    /// T6 m-2 (revisione finale): a 720×520, con un errore sotto la soglia di spesa, Tab fino alla
    /// casella porta in vista anche la riga dell'errore.
    #[test]
    fn tab_porta_in_vista_l_errore_della_soglia_di_spesa() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.on_soglia_spesa_scritta(|_| "Scrivi un numero di dollari, per esempio 250.".into());
        f.show().expect("impostazioni mostrate nel backend di prova");
        // Prima a finestra alta (niente scorrimento): si scrive un valore sbagliato e Invio, e
        // compare l'errore. Poi la misura di serie: la sezione è in cima e l'errore sotto il bordo.
        f.window().set_size(slint::LogicalSize::new(720.0, 1400.0));
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let spesa = || campo(&f, "Soglia di spesa mensile ($)");
        for _ in 0..30 {
            if ha_il_fuoco(&f, &spesa()) {
                break;
            }
            tasto_finestra(&f, Key::Tab);
        }
        scrivi(&f, "tanti");
        tasto_finestra(&f, Key::Return);
        let errore = || {
            ElementHandle::find_by_element_id(&f, "FinestraImpostazioni::errore-spesa-testo")
                .next()
                .expect("la riga d'errore c'è ed è visibile")
        };
        errore();
        f.window().set_size(slint::LogicalSize::new(720.0, 520.0));
        assert!(
            ElementHandle::find_by_element_id(&f, "FinestraImpostazioni::errore-spesa-testo")
                .next()
                .is_none_or(|e| e.absolute_position().y + e.size().height > 520.0),
            "all'apertura l'errore è sotto il bordo (altrimenti il test non prova nulla)"
        );
        // Si esce dalla casella e ci si torna con Tab (il giro ricomincia dall'alto).
        tasto_finestra(&f, Key::Tab);
        for _ in 0..40 {
            if ha_il_fuoco(&f, &spesa()) {
                break;
            }
            tasto_finestra(&f, Key::Tab);
        }
        assert!(ha_il_fuoco(&f, &spesa()), "Tab arriva alla soglia di spesa");
        let (y, h) = (errore().absolute_position().y, errore().size().height);
        assert!(y >= 18.0 && y + h <= 520.0 + 0.5, "la riga d'errore è fuori vista: y {y} + {h}");
    }

    /// Fix 1, I-2: alla misura di serie (720×520) Tab fino a «Soglia di spesa mensile» la porta
    /// intera nell'area visibile (ogni controllo della sezione Claude scorre in vista).
    #[test]
    fn tab_porta_in_vista_la_soglia_di_spesa() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(720.0, 520.0));
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        // Una casella oltre il bordo della finestra la ricerca non la vede (fuori dal ritaglio):
        // con Inter, più largo di Inter Tight, le righe vanno a capo prima e la casella parte
        // sotto i 520 px; «non trovata» vale quindi «sotto il bordo».
        let spesa = || {
            ElementHandle::find_by_accessible_label(&f, "Soglia di spesa mensile ($)")
                .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
        };
        assert!(
            spesa().is_none_or(|c| c.absolute_position().y + c.size().height > 520.0 - 24.0),
            "all'apertura la casella è sotto il bordo (altrimenti il test non prova nulla)"
        );
        for _ in 0..30 {
            if spesa().is_some_and(|c| ha_il_fuoco(&f, &c)) {
                break;
            }
            tasto_finestra(&f, Key::Tab);
        }
        let spesa = || campo(&f, "Soglia di spesa mensile ($)");
        assert!(ha_il_fuoco(&f, &spesa()), "Tab arriva alla soglia di spesa");
        let (_, y, _, h) = dove_il_fuoco(&f).unwrap();
        assert!(y >= 18.0 && y + h <= 520.0 - 24.0 + 0.5, "la casella col fuoco è fuori vista: y {y} + {h}");
    }

    /// Fix 1, M-2 e M-3: a finestra minima (600 px) le caselle della sezione Claude restano larghe
    /// abbastanza e, collegato e da aggiornare, i due pulsanti vanno sotto il testo della card.
    #[test]
    fn sezione_claude_a_finestra_minima() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.set_claude_collegato(true);
        f.set_claude_da_aggiornare(true);
        f.set_claude_stato("Da aggiornare".into());
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(600.0, 1400.0));
        for nome in ["Chiave da amministratore", "Avvisa quando un limite supera (%)", "Soglia di spesa mensile ($)"] {
            let l = campo(&f, nome).size().width;
            assert!(l >= 120.0, "«{nome}» largo solo {l}");
        }
        let titolo = elemento(&f, "Da aggiornare");
        let sotto = titolo.absolute_position().y + titolo.size().height;
        for nome in ["Scollega", "Aggiorna il collegamento"] {
            let p = elemento(&f, nome);
            assert!(p.absolute_position().y > sotto, "«{nome}» accanto al testo invece che sotto");
        }
        // Task 10: coi caratteri nuovi nessun pulsante esce dalla finestra né si accorcia.
        let tagliati = pulsanti_tagliati(&f, 600.0);
        assert!(tagliati.is_empty(), "{}", tagliati.join("\n"));
        // Alla misura di serie un pulsante solo (non collegato) resta accanto al testo.
        f.set_claude_collegato(false);
        f.set_claude_da_aggiornare(false);
        f.set_claude_stato("Non collegato".into());
        f.window().set_size(slint::LogicalSize::new(720.0, 1400.0));
        let titolo = elemento(&f, "Non collegato");
        let collega = elemento(&f, "Collega Claude Code");
        assert!(collega.absolute_position().y < titolo.absolute_position().y + 40.0, "«Collega» accanto al testo");
    }

    /// RC-33: «Togli» la chiave chiede conferma come «Scollega»: domanda, fuoco su «Annulla»
    /// (Invio lì non toglie nulla), Esc annulla, «Togli» rosso toglie davvero.
    #[test]
    fn togli_la_chiave_chiede_conferma() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.set_api_presente(true);
        f.set_api_ultime("1a2b".into());
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(760.0, 1400.0));
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let tolte = std::rc::Rc::new(std::cell::Cell::new(0));
        let t = tolte.clone();
        f.on_api_togli(move || t.set(t.get() + 1));
        let chiusa = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiusa.clone();
        f.on_chiudi(move || c.set(true));
        let domanda = "Togliere la chiave da amministratore?";
        let ce = |nome: &str| ElementHandle::find_by_accessible_label(&f, nome).next().is_some();
        let pulsante = |nome: &str| {
            ElementHandle::find_by_accessible_label(&f, nome)
                .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
                .unwrap_or_else(|| panic!("pulsante «{nome}»"))
        };

        // RC-46: con la chiave, le schede delle API si aprono anche da qui.
        let dettagli = std::rc::Rc::new(std::cell::Cell::new(0));
        let d = dettagli.clone();
        f.on_api_dettagli(move || d.set(d.get() + 1));
        pulsante("Dettagli delle API").invoke_accessible_default_action();
        assert_eq!(dettagli.get(), 1);

        pulsante("Togli").invoke_accessible_default_action();
        assert_eq!(tolte.get(), 0, "il primo «Togli» chiede soltanto");
        assert!(ce(domanda), "la domanda c'è");
        assert!(col_fuoco(&f).ends_with("annulla-togli"), "fuoco su «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert!(!ce(domanda) && tolte.get() == 0, "Invio su «Annulla» chiude senza togliere");

        pulsante("Togli").invoke_accessible_default_action();
        tasto_finestra(&f, Key::Escape);
        assert!(!ce(domanda) && tolte.get() == 0 && !chiusa.get(), "Esc chiude la conferma, non la finestra");

        pulsante("Togli").invoke_accessible_default_action();
        tasto_finestra(&f, Key::Tab);
        assert!(col_fuoco(&f).ends_with("conferma-togli"), "Tab porta a «Togli»: «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert_eq!(tolte.get(), 1, "il «Togli» rosso toglie");
        assert!(!ce(domanda));
        // Tolta la chiave (Rust riscrive lo stato) la conferma non resta appesa.
        f.set_api_presente(false);
        assert!(!ce("Togli"));
    }

    /// Le Impostazioni sulla scheda Collegamenti, coi controlli veri dei campi; restituisce gli
    /// eventi (salvataggi, eliminazioni, spostamenti, chiusura della finestra).
    fn scheda_collegamenti() -> (FinestraImpostazioni, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_COLLEGAMENTI);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        f.set_collegamenti(quattro_collegamenti());
        f.on_controlla_nome_collegamento(|n| crate::collegamenti::valida_nome(&n).err().unwrap_or_default().into());
        f.on_controlla_indirizzo_collegamento(|u| crate::collegamenti::valida_indirizzo(&u).err().unwrap_or_default().into());
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3, e4) = (eventi.clone(), eventi.clone(), eventi.clone(), eventi.clone());
        f.on_salva_collegamento(move |id, nome, url| {
            e1.borrow_mut().push(format!("salva [{id}] [{nome}] [{url}]"));
            "".into()
        });
        f.on_elimina_collegamento(move |id| e2.borrow_mut().push(format!("elimina {id}")));
        f.on_sposta_collegamento(move |id, verso| e3.borrow_mut().push(format!("sposta {id} {verso}")));
        f.on_chiudi(move || e4.borrow_mut().push("chiudi finestra".into()));
        (f, eventi)
    }

    fn tasto_finestra(f: &FinestraImpostazioni, testo: impl Into<slint::SharedString> + Clone) {
        use slint::platform::WindowEvent;
        f.window().dispatch_event(WindowEvent::KeyPressed { text: testo.clone().into() });
        f.window().dispatch_event(WindowEvent::KeyReleased { text: testo.into() });
    }

    fn scrivi(f: &FinestraImpostazioni, testo: &str) {
        for c in testo.chars() {
            tasto_finestra(f, c.to_string());
        }
    }

    fn campo(f: &FinestraImpostazioni, etichetta: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(f, etichetta)
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .unwrap_or_else(|| panic!("nessun campo «{etichetta}»"))
    }

    /// La scheda: sezione «Collegamenti» fra Dispositivi e Sistema; righe coi pulsanti nominati;
    /// le frecce ai bordi spente; le azioni arrivano a Rust con l'id giusto.
    #[test]
    fn scheda_collegamenti_righe_e_ordine() {
        let (f, eventi) = scheda_collegamenti();
        assert_eq!(elemento(&f, "Collegamenti").accessible_item_selected(), Some(true));
        assert_eq!(elemento(&f, "Sposta su «Proxmox»").accessible_enabled(), Some(false), "già in cima");
        assert_eq!(elemento(&f, "Sposta giù «Stampante»").accessible_enabled(), Some(false), "già in fondo");
        assert!(ElementHandle::find_by_accessible_label(&f, "http://nas.esempio.lan/admin?x=1").next().is_some(), "indirizzo intero");
        elemento(&f, "Sposta giù «Proxmox»").invoke_accessible_default_action();
        elemento(&f, "Sposta su «Router»").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["sposta proxmox 1", "sposta router -1"]);
        // Senza collegamenti: lo stato vuoto, e «Aggiungi un collegamento» resta.
        f.set_collegamenti(ModelRc::new(VecModel::from(Vec::<crate::CollegamentoUi>::new())));
        assert!(ElementHandle::find_by_accessible_label(&f, "Nessun collegamento").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&f, "Aggiungi un collegamento").next().is_some());
        // 18/11: a elenco vuoto il pulsante sta sotto il messaggio e al centro (come nel pannello).
        let messaggio = elemento(&f, "Nessun collegamento");
        let pulsante = elemento(&f, "Aggiungi un collegamento");
        let centro = |e: &ElementHandle| e.absolute_position().x + e.size().width / 2.0;
        assert!((centro(&messaggio) - centro(&pulsante)).abs() < 2.0, "centrato: {} contro {}", centro(&messaggio), centro(&pulsante));
        assert!(pulsante.absolute_position().y > messaggio.absolute_position().y + messaggio.size().height, "sotto il messaggio");
    }

    /// Il modulo: etichette visibili; controllo al blur solo su un campo toccato; l'errore sparisce
    /// appena corretto; Invio salva; Esc a livelli (campo, modulo, finestra).
    #[test]
    fn scheda_collegamenti_modulo_con_controlli() {
        use slint::platform::Key;
        let (f, eventi) = scheda_collegamenti();
        elemento(&f, "Aggiungi un collegamento").invoke_accessible_default_action();
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVO COLLEGAMENTO").next().is_some());
        // Il fuoco è nel Nome: Tab senza scrivere non segnala nulla (campo solo attraversato).
        tasto_finestra(&f, Key::Tab);
        assert_eq!(campo(&f, "Nome").accessible_description().as_deref(), Some(""));
        // Indirizzo sbagliato: al blur l'errore compare.
        scrivi(&f, "file:///x");
        tasto_finestra(&f, Key::Tab);
        let errore = campo(&f, "Indirizzo").accessible_description().unwrap_or_default();
        assert_eq!(errore, "Solo indirizzi http:// o https://");
        assert!(ElementHandle::find_by_accessible_label(&f, "Solo indirizzi http:// o https://").next().is_some(), "testo visibile");
        // Si torna nel campo, si seleziona tutto e si riscrive: l'errore sparisce al primo
        // carattere che rende l'indirizzo valido, prima di lasciare il campo.
        con_modificatori_finestra(&f, &[Key::Shift], Key::Tab);
        con_modificatori_finestra(&f, &[Key::Control], "a");
        scrivi(&f, "e");
        assert_eq!(campo(&f, "Indirizzo").accessible_description().as_deref(), Some(""));
        scrivi(&f, "sempio.lan:8006");
        // Invio col nome vuoto: il nome si segnala e prende il fuoco; nulla si salva.
        tasto_finestra(&f, Key::Return);
        assert_eq!(campo(&f, "Nome").accessible_description().as_deref(), Some("Scrivi un nome"));
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
        scrivi(&f, "Proxmox");
        assert_eq!(campo(&f, "Nome").accessible_description().as_deref(), Some(""), "corretto: errore via");
        tasto_finestra(&f, Key::Return);
        assert_eq!(*eventi.borrow(), ["salva [] [Proxmox] [esempio.lan:8006]"]);
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVO COLLEGAMENTO").next().is_none(), "salvato: modulo chiuso");
        // Modifica: i campi partono dai valori salvati; Esc esce dal campo, poi chiude il modulo
        // (senza salvare), poi chiude la finestra.
        elemento(&f, "Modifica «NAS»").invoke_accessible_default_action();
        assert_eq!(campo(&f, "Nome").accessible_value().as_deref(), Some("NAS"));
        assert_eq!(campo(&f, "Indirizzo").accessible_value().as_deref(), Some("http://nas.esempio.lan/admin?x=1"));
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "MODIFICA «NAS»").next().is_some(), "il primo Esc esce solo dal campo");
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "MODIFICA «NAS»").next().is_none(), "il secondo chiude il modulo");
        assert_eq!(eventi.borrow().len(), 1, "niente salvato né chiuso: {:?}", eventi.borrow());
        tasto_finestra(&f, Key::Escape);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi finestra"));
    }

    fn con_modificatori_finestra(f: &FinestraImpostazioni, modificatori: &[slint::platform::Key], t: impl Into<slint::SharedString> + Clone) {
        use slint::platform::WindowEvent;
        for m in modificatori {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: (*m).into() });
        }
        tasto_finestra(f, t);
        for m in modificatori.iter().rev() {
            f.window().dispatch_event(WindowEvent::KeyReleased { text: (*m).into() });
        }
    }

    /// Il testo del controllo col fuoco (id dell'elemento, «Componente::id»), "" se nessuno.
    fn col_fuoco(f: &FinestraImpostazioni) -> String {
        let fuoco = i_slint_core::window::WindowInner::from_pub(f.window()).focus_item.borrow().upgrade();
        fuoco
            .as_ref()
            .and_then(|i| i.element_type_names_and_ids(0))
            .and_then(|v| v.first().map(|(_, id)| id.to_string()))
            .unwrap_or_default()
    }

    /// L'eliminazione chiede conferma dentro la finestra nominando il collegamento; il fuoco parte
    /// da «Annulla» (Invio lì non elimina); Esc annulla; «Elimina» elimina quell'id.
    #[test]
    fn scheda_collegamenti_conferma_di_eliminazione() {
        use slint::platform::Key;
        let (f, eventi) = scheda_collegamenti();
        let conferma = |f: &FinestraImpostazioni| ElementHandle::find_by_accessible_label(f, "Eliminare “NAS”?").next();
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        assert_eq!(conferma(&f).and_then(|c| c.accessible_role()), Some(AccessibleRole::Groupbox));
        assert!(col_fuoco(&f).ends_with("::annulla"), "fuoco su «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert!(conferma(&f).is_none(), "Invio su «Annulla» chiude la conferma");
        // Tab resta nella conferma; Esc annulla senza chiudere la finestra.
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        tasto_finestra(&f, Key::Tab);
        assert!(col_fuoco(&f).ends_with("::elimina"), "fuoco su «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Tab);
        assert!(col_fuoco(&f).ends_with("::annulla"), "fuoco su «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Escape);
        assert!(conferma(&f).is_none());
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
        // Un clic dentro il riquadro (sul testo) non la chiude; «Elimina» elimina.
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        let riquadro = conferma(&f).unwrap();
        let (pos, _) = (riquadro.absolute_position(), riquadro.size());
        let punto = slint::LogicalPosition::new(pos.x + 30.0, pos.y + 60.0);
        use slint::platform::{PointerEventButton, WindowEvent};
        let button = PointerEventButton::Left;
        // Due volte: il primo clic su elementi appena nati da un `if` il backend di prova non lo
        // consegna sempre (visto anche nella scelta dell'icona); col secondo il controllo vale.
        for _ in 0..2 {
            f.window().dispatch_event(WindowEvent::PointerMoved { position: punto });
            f.window().dispatch_event(WindowEvent::PointerPressed { position: punto, button });
            f.window().dispatch_event(WindowEvent::PointerReleased { position: punto, button });
        }
        assert!(conferma(&f).is_some(), "un clic nel riquadro l'ha chiusa");
        elemento(&f, "Elimina “NAS”").invoke_accessible_default_action();
        assert!(conferma(&f).is_none());
        assert_eq!(*eventi.borrow(), ["elimina nas"]);
    }

    /// Giro di rifinitura del 14f: Esc a livelli anche nel modulo degli ambienti di Home
    /// Assistant (prima chiudeva la finestra, perdendo quanto scritto, token compreso).
    #[test]
    fn modulo_ambienti_esc_a_livelli() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let chiusa = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiusa.clone();
        f.on_chiudi(move || c.set(true));
        elemento(&f, "Aggiungi una casa").invoke_accessible_default_action();
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVA CASA").next().is_some());
        scrivi(&f, "Casa");
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVA CASA").next().is_some(), "il primo Esc esce dal campo");
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVA CASA").next().is_none(), "il secondo chiude il modulo");
        assert!(!chiusa.get(), "la finestra resta aperta");
        tasto_finestra(&f, Key::Escape);
        assert!(chiusa.get(), "il terzo chiude la finestra");
    }

    /// R1-17: un campo col fuoco ha l'anello bianco di tutti gli altri controlli, fuori dalla
    /// casella; uscito il fuoco l'anello sparisce. R1-19: il filo dei petali in cima è quello della
    /// gestione, 1,5 px arrotondato al pixel fisico.
    #[test]
    fn campo_col_fuoco_ha_l_anello_comune_e_filo_sul_pixel() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let anelli = || ElementHandle::find_by_element_type_name(&f, "Anello").collect::<Vec<_>>();
        elemento(&f, "Aggiungi una casa").invoke_accessible_default_action();
        let nome = campo(&f, "Nome");
        let a = anelli();
        assert_eq!(a.len(), 1, "un anello sul campo col fuoco");
        let (pa, la) = (a[0].absolute_position(), a[0].size());
        let (pn, ln) = (nome.absolute_position(), nome.size());
        // L'anello circonda la casella: comincia a sinistra del testo e la copre in altezza.
        assert!(pa.x < pn.x && pa.x + la.width > pn.x + ln.width, "anello {pa:?} {la:?} campo {pn:?} {ln:?}");
        tasto_finestra(&f, Key::Escape);
        assert!(anelli().is_empty(), "fuori dal campo niente anello");

        let filo = |scala: f32| {
            f.global::<crate::Tema>().set_scala(scala);
            ElementHandle::find_by_element_id(&f, "FinestraImpostazioni::filo-petali").next().expect("filo").size().height
        };
        assert_eq!(filo(1.0), 2.0, "a 100% 2 px");
        assert!((filo(1.75) - 3.0 / 1.75).abs() < 0.001, "a 175% 3 px fisici: {}", filo(1.75));
    }

    /// Giro di rifinitura: un clic fuori dalla conferma vale «Annulla»; con la conferma aperta i
    /// pulsanti delle righe sotto sono spenti (Narrator e UIA non li invocano).
    #[test]
    fn conferma_clic_fuori_e_righe_spente() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (f, eventi) = scheda_collegamenti();
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        assert_eq!(elemento(&f, "Elimina «Router»").accessible_enabled(), Some(false));
        elemento(&f, "Elimina «Router»").invoke_accessible_default_action();
        assert!(ElementHandle::find_by_accessible_label(&f, "Eliminare “NAS”?").next().is_some(), "la conferma non cambia");
        let riquadro = elemento(&f, "Eliminare “NAS”?");
        let (pos, lato) = (riquadro.absolute_position(), riquadro.size());
        let punto = slint::LogicalPosition::new(pos.x + 20.0, pos.y + lato.height + 40.0);
        let button = PointerEventButton::Left;
        for _ in 0..2 {
            f.window().dispatch_event(WindowEvent::PointerMoved { position: punto });
            f.window().dispatch_event(WindowEvent::PointerPressed { position: punto, button });
            f.window().dispatch_event(WindowEvent::PointerReleased { position: punto, button });
        }
        assert!(ElementHandle::find_by_accessible_label(&f, "Eliminare “NAS”?").next().is_none(), "il clic fuori annulla");
        assert_eq!(elemento(&f, "Elimina «Router»").accessible_enabled(), Some(true));
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
    }

    // ─── M2, Task 4: lati della pillola, sezione Isola, selettore, quadranti ───

    fn testo_lato(t: &str) -> crate::ContenutoLatoUi {
        crate::ContenutoLatoUi { tipo: 1, testo: t.into(), ..Default::default() }
    }

    /// Le misure della pillola coi lati sono quelle di `lati::larghezza_riposo` e
    /// `lati::scarto_occhi_riposo`: un lato, due lati, la batteria col glifo; da allargata i lati
    /// non contano. Il nome per Narrator dice i valori, nell'ordine.
    #[test]
    fn misure_dei_lati_slint_e_rust() {
        use crate::lati::{larghezza_riposo, scarto_occhi_riposo};
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(0.0, 0.0), "senza lati come M1");
        assert_eq!(p.get_scarto_occhi_riposo(), 0.0);

        p.set_lato_destro(testo_lato("CPU 23%"));
        let destro = p.get_ingombro_lato_destro();
        assert!(destro > 30.0, "il testo ha una larghezza: {destro}");
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(0.0, destro));
        assert_eq!(p.get_scarto_occhi_riposo(), scarto_occhi_riposo(0.0, destro));
        assert_eq!(elemento(&p, "ItsyHome, CPU 23%").accessible_role(), Some(AccessibleRole::Button));

        p.set_lato_sinistro(crate::ContenutoLatoUi {
            tipo: 2,
            testo: "80%".into(),
            frazione: 0.8,
            tinta: crate::TintaBatteriaUi::Normale,
            ..Default::default()
        });
        let sinistro = p.get_ingombro_lato_sinistro();
        assert!(sinistro > 23.0, "glifo (19 + 4) più il testo: {sinistro}");
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(sinistro, destro));
        assert_eq!(p.get_scarto_occhi_riposo(), 0.0, "due lati: occhi al centro");
        assert!(elemento(&p, "ItsyHome, batteria 80%, CPU 23%").accessible_role().is_some());
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "GlifoBatteria").count(), 1);

        // Col puntino i lati restano; allargata la larghezza è quella dell'attenzione.
        p.set_aspetto(AspettoPillolaUi::Puntino);
        assert!(elemento(&p, "ItsyHome, batteria 80%, CPU 23%, una voce in attesa").accessible_role().is_some());
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(sinistro, destro));
        p.set_aspetto(AspettoPillolaUi::Allargata);
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTENZIONE);
        assert!(p.get_larghezza_riposo() <= crate::finestre::LARGHEZZA_BLOCCANTE);
    }

    /// M3, Task 4: la pillola media dell'attività di Claude Code. Larga `LARGHEZZA_ATTIVITA`, i lati
    /// si nascondono, il nome per Narrator è quello a parole; da allargata l'attività non si vede.
    #[test]
    fn pillola_media_misure() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        assert_eq!(p.get_larghezza_attivita(), crate::finestre::LARGHEZZA_ATTIVITA);
        p.set_lato_destro(testo_lato("CPU 23%"));
        assert!(p.get_lati_visibili() && !p.get_attivita_visibile());
        assert_ne!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);

        p.set_attivita("Claude · Modifica · 3:12".into());
        p.set_nome_attivita("Claude, Modifica, 3 minuti".into());
        assert!(p.get_attivita_visibile());
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
        assert!(!p.get_lati_visibili(), "i lati si nascondono");
        assert!(p.get_larghezza_visibile() <= p.get_larghezza_finestra());
        assert_eq!(elemento(&p, "ItsyHome, Claude, Modifica, 3 minuti").accessible_role(), Some(AccessibleRole::Button));
        // Col puntino resta; il testo sta dentro la pillola (occhi + testo + margine).
        p.set_aspetto(AspettoPillolaUi::Puntino);
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
        assert!(elemento(&p, "ItsyHome, Claude, Modifica, 3 minuti, una voce in attesa").accessible_role().is_some());
        // Il testo più lungo che Rust può scrivere ci sta senza tagliarsi troppo: lo misura la prova
        // viva; qui basta che la pillola non cresca.
        p.set_attivita("Claude · UnNomeLunghi · 1:02:05".into());
        p.set_attivita_sinistra("Claude · UnNomeLunghi".into());
        p.set_attivita_destra("1:02:05".into());
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
        // La durata sta sempre intera dentro la pillola, anche col nome più lungo: si taglia solo
        // il nome (la sua larghezza arriva fin dove parte la durata).
        let (x, l) = (p.get_attivita_destra_x(), p.get_attivita_destra_larghezza());
        assert!(l > 30.0, "la durata ha una larghezza: {l}");
        assert!(x + l <= crate::finestre::LARGHEZZA_ATTIVITA, "la durata esce dalla pillola: {x} + {l}");
        assert!(x >= 50.0, "la durata non copre gli occhi: {x}");

        // Allargata (una voce che chiama): l'attività non si vede e la larghezza è quella dell'attenzione.
        p.set_aspetto(AspettoPillolaUi::Allargata);
        assert!(!p.get_attivita_visibile());
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTENZIONE);
        // Ritirata: nemmeno.
        p.set_aspetto(AspettoPillolaUi::Riposo);
        p.set_ritirata(true);
        assert!(!p.get_attivita_visibile());
        // Senza testo la pillola torna quella di prima, coi lati.
        p.set_ritirata(false);
        p.set_attivita("".into());
        assert!(!p.get_attivita_visibile() && p.get_lati_visibili());
        assert_ne!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
    }

    /// La sezione «Isola» sta fra Collegamenti e Sistema; «Informazioni» passa alla 5.
    #[test]
    fn sezione_isola_e_informazioni_spostata() {
        let (f, _) = scheda_isola();
        f.set_sezione(0);
        assert_eq!(elemento(&f, "Isola").accessible_item_index(), Some(3));
        assert_eq!(elemento(&f, "Claude").accessible_item_index(), Some(4));
        assert_eq!(elemento(&f, "Sistema").accessible_item_index(), Some(5));
        assert_eq!(elemento(&f, "Informazioni").accessible_item_index(), Some(6));
        assert_eq!(crate::finestra_impostazioni::SEZIONE_CLAUDE, 4);
        assert_eq!(crate::finestra_impostazioni::SEZIONE_INFORMAZIONI, 6);
        f.set_sezione(3);
        assert_eq!(elemento(&f, "Isola").accessible_item_selected(), Some(true));
        for nome in ["A sinistra degli occhi", "A destra degli occhi", "Meteo da"] {
            let e = ElementHandle::find_by_accessible_label(&f, nome)
                .find(|e| e.accessible_role() == Some(AccessibleRole::Combobox))
                .unwrap_or_else(|| panic!("selettore «{nome}»"));
            assert_eq!(e.accessible_expandable(), Some(true));
        }
        // Le note «solo su Windows» compaiono solo fuori da Windows (e con la minuscola dopo i due punti).
        let rete = "«Rete»: solo su Windows, solo l'icona della connessione. Il nome della rete non compare mai nella pillola.";
        assert!(ElementHandle::find_by_accessible_label(&f, rete).next().is_none(), "nota Rete su Windows");
        f.set_fuori_da_windows(true);
        assert!(ElementHandle::find_by_accessible_label(&f, rete).next().is_some(), "nota Rete fuori da Windows");
        // L'avviso delle cuffie ha la sua sezione «AVVISI», fuori da «PILLOLA».
        assert!(ElementHandle::find_by_accessible_label(&f, "AVVISI").next().is_none(), "AVVISI senza cuffie");
        f.set_avvisi_bt_disponibile(true);
        assert!(ElementHandle::find_by_accessible_label(&f, "AVVISI").next().is_some(), "AVVISI con le cuffie");
    }

    /// Le scelte arrivate a Rust: (0/1 lato, indice).
    type Scelte = std::rc::Rc<std::cell::RefCell<Vec<(i32, i32)>>>;

    /// Impostazioni › Isola coi selettori riempiti come fa il ponte.
    fn scheda_isola() -> (FinestraImpostazioni, Scelte) {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        let voci = |v: &[&str]| -> ModelRc<slint::SharedString> {
            ModelRc::new(VecModel::from(v.iter().map(|s| slint::SharedString::from(*s)).collect::<Vec<_>>()))
        };
        f.set_scelte_lati(voci(&["Niente", "Ora", "CPU", "RAM"]));
        f.set_scelte_meteo(voci(&["Automatico"]));
        f.set_sezione(3);
        f.show().expect("impostazioni mostrate nel backend di prova");
        // Alta abbastanza da vedere tutte le righe (la ricerca salta ciò che lo scorrimento nasconde).
        f.window().set_size(slint::LogicalSize::new(720.0, 900.0));
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let scelti = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let s = scelti.clone();
        f.on_scegli_lato(move |quale, i| s.borrow_mut().push((quale, i)));
        (f, scelti)
    }

    fn selettore(f: &FinestraImpostazioni, nome: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(f, nome)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Combobox))
            .unwrap_or_else(|| {
                let tutti: Vec<String> = i_slint_backend_testing::ElementQuery::from_root(f)
                    .match_descendants()
                    .match_accessible_role(AccessibleRole::Combobox)
                    .find_all()
                    .iter()
                    .map(|e| format!("{:?}", e.accessible_label()))
                    .collect();
                panic!("selettore «{nome}» (ci sono: {tutti:?})")
            })
    }

    /// `Scelta` da tastiera: Tab ci arriva, Invio apre, ↓ e Invio scelgono la seconda voce, Esc
    /// chiude senza scegliere e il fuoco resta sul pulsante.
    #[test]
    fn scelta_da_tastiera() {
        use slint::platform::Key;
        let (f, scelti) = scheda_isola();
        f.set_indice_lato_sinistro(0);
        let sinistra = selettore(&f, "A sinistra degli occhi");
        assert_eq!(sinistra.accessible_value().as_deref(), Some("Niente"));
        assert_eq!(sinistra.accessible_expanded(), Some(false));
        // Tab fino al primo selettore (dopo le sei sezioni: «Panoramica», Task 9), poi al secondo.
        for _ in 0..12 {
            if col_fuoco(&f).ends_with("selettore") {
                break;
            }
            tasto_finestra(&f, Key::Tab);
        }
        assert!(col_fuoco(&f).ends_with("selettore"), "Tab arriva al selettore: {}", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert_eq!(sinistra.accessible_expanded(), Some(false), "il primo selettore è «Panoramica»");
        tasto_finestra(&f, Key::Escape);
        tasto_finestra(&f, Key::Tab);
        assert!(col_fuoco(&f).ends_with("selettore"), "Tab arriva al selettore dei lati: {}", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert_eq!(sinistra.accessible_expanded(), Some(true), "Invio apre l'elenco");
        assert!(ElementHandle::find_by_accessible_label(&f, "Ora")
            .any(|e| e.accessible_role() == Some(AccessibleRole::ListItem)), "le voci dell'elenco sono accessibili");
        tasto_finestra(&f, Key::DownArrow);
        tasto_finestra(&f, Key::Return);
        assert_eq!(*scelti.borrow(), [(0, 1)], "↓ e Invio scelgono «Ora»");
        assert_eq!(sinistra.accessible_expanded(), Some(false));
        assert!(col_fuoco(&f).ends_with("selettore"), "il fuoco torna al pulsante: {}", col_fuoco(&f));
        // Il ponte riscrive la scelta: il valore per Narrator la segue.
        f.set_indice_lato_sinistro(1);
        assert_eq!(sinistra.accessible_value().as_deref(), Some("Ora"));
        // Riaperto: Esc chiude, nulla di scelto, il fuoco resta.
        tasto_finestra(&f, Key::Return);
        assert_eq!(sinistra.accessible_expanded(), Some(true));
        tasto_finestra(&f, Key::DownArrow);
        tasto_finestra(&f, Key::Escape);
        assert_eq!(sinistra.accessible_expanded(), Some(false));
        assert_eq!(scelti.borrow().len(), 1, "Esc non sceglie");
        assert!(col_fuoco(&f).ends_with("selettore"), "Esc lascia il fuoco sul pulsante: {}", col_fuoco(&f));
        // Un secondo Esc (fuori dall'elenco) arriva alla finestra, come altrove.
        assert!(ElementHandle::find_by_accessible_label(&f, "Isola").next().is_some());
    }

    /// Il selettore di un lato: dice la scelta, Narrator lo apre e sceglie. Ridisegno F1, Task 8:
    /// niente più «Quadrante n» né campo «Massimo».
    #[test]
    fn selettore_del_lato_e_niente_quadranti() {
        let (f, scelti) = scheda_isola();
        f.set_indice_lato_destro(2);
        let destra = selettore(&f, "A destra degli occhi");
        assert_eq!(destra.accessible_value().as_deref(), Some("CPU"));
        destra.invoke_accessible_default_action();
        assert_eq!(destra.accessible_expanded(), Some(true));
        ElementHandle::find_by_accessible_label(&f, "RAM")
            .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
            .expect("voce «RAM»")
            .invoke_accessible_default_action();
        assert_eq!(*scelti.borrow(), [(1, 3)], "a destra → «RAM»");
        assert!(ElementHandle::find_by_accessible_label(&f, "Quadrante 1").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&f, "QUADRANTI").next().is_none());
        let massimi = i_slint_backend_testing::ElementQuery::from_root(&f)
            .match_descendants()
            .find_all()
            .into_iter()
            .filter(|e| e.accessible_label().is_some_and(|l| l.ends_with(", massimo")))
            .count();
        assert_eq!(massimi, 0, "nessun campo «Massimo»");
    }

    /// M3, Task 12: Impostazioni › Claude › Avvisi. «Avvisa quando un limite supera (%)» parte da 80;
    /// un testo che non è un numero da 50 a 100 si rifiuta con un errore che dice cosa scrivere e
    /// non si salva; lo stesso per la soglia di spesa (vuota = nessuna).
    #[test]
    fn soglie_nelle_impostazioni() {
        use slint::platform::{Key, PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_soglia_limite("80".into());
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(760.0, 1400.0));
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let salvati = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let s = salvati.clone();
        f.on_soglia_limite_scritta(move |t| match crate::claude::leggi_soglia_limite(&t) {
            Ok(v) => {
                s.borrow_mut().push(format!("limite {v}"));
                "".into()
            }
            Err(e) => e.into(),
        });
        let s = salvati.clone();
        f.on_soglia_spesa_scritta(move |t| match crate::claude::leggi_soglia_spesa(&t) {
            Ok(v) => {
                s.borrow_mut().push(format!("spesa {v:?}"));
                "".into()
            }
            Err(e) => e.into(),
        });
        let campo = |nome: &str| {
            ElementHandle::find_by_accessible_label(&f, nome)
                .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
                .unwrap_or_else(|| panic!("campo «{nome}»"))
        };
        // Scrive nel campo e preme Invio (dopo averci dato il fuoco con un clic).
        let scrivi = |nome: &str, testo: &str| {
            let c = campo(nome);
            let (pos, misura) = (c.absolute_position(), c.size());
            let centro = slint::LogicalPosition::new(pos.x + misura.width / 2.0, pos.y + misura.height / 2.0);
            f.window().dispatch_event(WindowEvent::PointerMoved { position: centro });
            f.window().dispatch_event(WindowEvent::PointerPressed { position: centro, button: PointerEventButton::Left });
            f.window().dispatch_event(WindowEvent::PointerReleased { position: centro, button: PointerEventButton::Left });
            c.set_accessible_value(testo);
            f.window().dispatch_event(WindowEvent::KeyPressed { text: Key::Return.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: Key::Return.into() });
        };
        // RC-47: un'etichetta senza i puntini che sembravano un testo tagliato.
        const LIMITE: &str = "Avvisa quando un limite supera (%)";
        const SPESA: &str = "Soglia di spesa mensile ($)";
        // RC-47: le caselle della chiave e degli avvisi partono dalla stessa colonna.
        let x = |nome: &str| campo(nome).absolute_position().x;
        assert_eq!(x("Chiave da amministratore"), x(LIMITE));
        assert_eq!(x(SPESA), x(LIMITE));
        assert_eq!(campo(LIMITE).accessible_value().as_deref(), Some("80"), "il predefinito");
        assert_eq!(campo(SPESA).accessible_value().as_deref(), Some(""), "nessuna soglia di spesa");
        assert_eq!(campo(LIMITE).accessible_description().as_deref(), Some(""), "nessun errore a riposo");

        scrivi(LIMITE, "abc");
        assert!(salvati.borrow().is_empty(), "«abc» non si salva");
        let errore = campo(LIMITE).accessible_description().unwrap_or_default();
        assert!(errore.contains("50 a 100"), "l'errore dice cosa scrivere: {errore:?}");
        scrivi(LIMITE, "101");
        assert!(salvati.borrow().is_empty(), "oltre 100 non si salva");
        scrivi(LIMITE, "90");
        assert!(!salvati.borrow().is_empty() && salvati.borrow().iter().all(|s| s == "limite 90"), "{:?}", salvati.borrow());
        assert_eq!(campo(LIMITE).accessible_description().as_deref(), Some(""), "salvato: l'errore sparisce");

        let spese = || salvati.borrow().iter().filter(|s| s.starts_with("spesa")).cloned().collect::<Vec<_>>();
        scrivi(SPESA, "zero");
        assert!(spese().is_empty(), "«zero» non si salva");
        assert!(campo(SPESA).accessible_description().unwrap_or_default().contains("maggiore di zero"));
        scrivi(SPESA, "99,5");
        assert_eq!(spese(), ["spesa Some(99.5)"]);
        scrivi(SPESA, "");
        assert_eq!(spese(), ["spesa Some(99.5)", "spesa None"], "vuota = nessuna soglia");
    }

    // ─── M2, Task 7: volume del PC ───

    /// La barra del volume sta dove stanno gli occhi: con `volume-mostrato` 0.5 c'è la barra
    /// (40 × 6, centrata sugli occhi) e gli occhi no, e la pillola per Narrator è «Volume 50%»;
    /// con −1 tornano gli occhi.
    #[test]
    fn barra_del_volume_al_posto_degli_occhi() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let conta = |id: &str| ElementHandle::find_by_element_id(&p, id).count();
        assert_eq!(conta("Pillola::occhi"), 1);
        assert_eq!(conta("Pillola::barra-volume"), 0);

        p.set_volume_mostrato(0.5);
        assert_eq!(conta("Pillola::barra-volume"), 1, "barra visibile");
        assert_eq!(conta("Pillola::occhi"), 0, "occhi nascosti");
        let barra = ElementHandle::find_by_element_id(&p, "Pillola::barra-volume").next().expect("barra");
        assert_eq!((barra.size().width, barra.size().height), (40.0, 6.0));
        let centro = barra.absolute_position().x + barra.size().width / 2.0;
        assert_eq!(centro, p.get_larghezza_finestra() / 2.0, "senza lati gli occhi (e la barra) stanno al centro");
        // Task 10: lo stesso nome della barra e della vista PC, con lo spazio prima di «%».
        assert_eq!(elemento(&p, "Volume 50 %").accessible_role(), Some(AccessibleRole::Button));
        p.set_volume_muto(true);
        assert!(ElementHandle::find_by_accessible_label(&p, "Volume 50 %, muto").next().is_some());

        p.set_volume_mostrato(-1.0);
        assert_eq!(conta("Pillola::occhi"), 1, "occhi di nuovo visibili");
        assert_eq!(conta("Pillola::barra-volume"), 0);
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome").next().is_some());
    }

    /// La rotella sopra la pillola arriva a Rust solo se accesa (Windows e interruttore) e solo a
    /// riposo o col puntino, mai da allargata.
    #[test]
    fn rotella_sulla_pillola_solo_a_riposo() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let ricevuti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f32>::new()));
        let r = ricevuti.clone();
        p.on_rotella(move |d| r.borrow_mut().push(d));
        elemento(&p, "ItsyHome").scroll(0.0, 60.0);
        assert!(ricevuti.borrow().is_empty(), "spenta di serie");
        p.set_rotella_attiva(true);
        elemento(&p, "ItsyHome").scroll(0.0, 60.0);
        elemento(&p, "ItsyHome").scroll(0.0, -60.0);
        p.set_aspetto(AspettoPillolaUi::Puntino);
        elemento(&p, "ItsyHome, una voce in attesa").scroll(0.0, 60.0);
        assert_eq!(*ricevuti.borrow(), [60.0, -60.0, 60.0]);
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("La casa chiede di te".into());
        elemento(&p, "La casa chiede di te").scroll(0.0, 60.0);
        assert_eq!(ricevuti.borrow().len(), 3, "allargata la rotella non cambia il volume");
    }

    /// Quanti elementi accessibili parlano di volume nel pannello.
    fn elementi_volume(p: &Pannello) -> usize {
        i_slint_backend_testing::ElementQuery::from_root(p)
            .match_descendants()
            .find_all()
            .iter()
            .filter(|e| e.accessible_label().is_some_and(|l| l.to_lowercase().contains("volume")))
            .count()
    }

    /// Vista PC (D3 di Paolo): senza volume disponibile la riga non c'è (nessun elemento
    /// «Volume», non una riga spenta); con il volume ci sono l'altoparlante (pulsante, muto sì/no
    /// anche con Narrator) e il cursore (slider col valore).
    #[test]
    fn altoparlante_assente_senza_volume() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_vista("pc".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "RETE").next().is_some(), "vista PC caricata");
        assert_eq!(elementi_volume(&p), 0, "senza volume nessun elemento «Volume»");

        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        // Lo stesso nome del pulsante della barra («Volume 40 %»): quello della vista ha anche la
        // descrizione.
        let altoparlante = ElementHandle::find_by_accessible_label(&p, "Volume 40 %")
            .find(|e| e.accessible_description().as_deref() == Some("Disattiva l'audio"))
            .expect("altoparlante della vista PC");
        assert_eq!(altoparlante.accessible_role(), Some(AccessibleRole::Button));
        assert_eq!(ElementHandle::find_by_accessible_label(&p, "Volume 40 %").count(), 2, "barra e vista, un solo formato");
        let cursore = elemento(&p, "Volume");
        assert_eq!(cursore.accessible_role(), Some(AccessibleRole::Slider));
        assert_eq!(cursore.accessible_value().as_deref(), Some("40%"));
        assert_eq!((cursore.accessible_value_minimum(), cursore.accessible_value_maximum()), (Some(0.0), Some(100.0)));
        assert_eq!(cursore.size().width, 104.0, "largo quanto le barre delle altre righe");
        let muti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let m = muti.clone();
        p.on_muto_volume(move |si| m.borrow_mut().push(si));
        altoparlante.invoke_accessible_default_action();
        assert_eq!(*muti.borrow(), [true]);
        p.set_volume_muto(true);
        assert!(ElementHandle::find_by_accessible_label(&p, "Volume 40 %, muto").next().is_some());

        p.set_volume_disponibile(false);
        assert_eq!(elementi_volume(&p), 0, "dispositivo sparito: la riga sparisce");
    }

    /// Il cursore della vista PC da tastiera: il fuoco del contenuto è sul cursore, → due volte
    /// danno +4 punti, Home e Fine 0 e 100, Invio muto sì/no; Narrator lo sposta di 2 punti.
    #[test]
    fn cursore_da_tastiera() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_schede(schede());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_volume_disponibile(true);
        p.set_volume(0.40);
        p.set_vista("pc".into());
        let livelli = std::rc::Rc::new(std::cell::RefCell::new(Vec::<i32>::new()));
        let l = livelli.clone();
        p.on_imposta_volume(move |v| l.borrow_mut().push((v * 100.0).round() as i32));
        let muti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let m = muti.clone();
        p.on_muto_volume(move |si| m.borrow_mut().push(si));

        tasto(&p, Key::RightArrow);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-volume"), "fuoco sul cursore: «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::RightArrow);
        assert_eq!(*livelli.borrow(), [42, 44], "+4 punti");
        assert_eq!(elemento(&p, "Volume").accessible_value().as_deref(), Some("44%"), "il cursore segue subito");
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Home);
        tasto(&p, Key::End);
        assert_eq!(*livelli.borrow(), [42, 44, 42, 0, 100]);
        tasto(&p, Key::Return);
        assert_eq!(*muti.borrow(), [true], "Invio: muto");
        elemento(&p, "Volume").invoke_accessible_decrement_action();
        assert_eq!(livelli.borrow().last(), Some(&98), "Narrator: 2 punti in meno");
        assert_eq!(p.get_vista(), "pc", "le frecce non cambiano vista");
        assert_eq!(
            elemento(&p, "Volume").accessible_description().as_deref(),
            Some("Invio: silenzia o riattiva"),
            "Narrator sa che Invio sul cursore comanda il muto"
        );
    }

    /// Impostazioni › Isola: «Volume con la rotella sulla pillola» solo dove c'è il volume
    /// (Windows), acceso di serie; il commutatore lo dice a Rust.
    #[test]
    fn interruttore_del_volume_nella_sezione_isola() {
        let (f, _) = scheda_isola();
        let interruttore = |f: &FinestraImpostazioni| {
            ElementHandle::find_by_accessible_label(f, "Volume con la rotella sulla pillola")
                .find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
        };
        assert!(interruttore(&f).is_none(), "senza volume la riga non c'è");
        f.set_volume_rotella_disponibile(true);
        let i = interruttore(&f).expect("interruttore");
        assert_eq!(i.accessible_checked(), Some(true), "acceso di serie");
        let cambi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let c = cambi.clone();
        f.on_cambia_volume_rotella(move |si| c.borrow_mut().push(si));
        i.invoke_accessible_default_action();
        assert_eq!(*cambi.borrow(), [false]);
        assert_eq!(i.accessible_checked(), Some(false));
    }

    /// Impostazioni › Isola: «Avvisa quando le cuffie Bluetooth sono scariche» solo su Windows,
    /// acceso di serie; il commutatore lo dice a Rust.
    #[test]
    fn interruttore_degli_avvisi_bt_nella_sezione_isola() {
        let (f, _) = scheda_isola();
        let nome = "Avvisa quando le cuffie Bluetooth sono scariche";
        let interruttore = |f: &FinestraImpostazioni| {
            ElementHandle::find_by_accessible_label(f, nome).find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
        };
        assert!(interruttore(&f).is_none(), "fuori da Windows la riga non c'è");
        f.set_avvisi_bt_disponibile(true);
        let i = interruttore(&f).expect("interruttore");
        assert_eq!(i.accessible_checked(), Some(true), "acceso di serie");
        let cambi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let c = cambi.clone();
        f.on_cambia_avvisi_bt(move |si| c.borrow_mut().push(si));
        i.invoke_accessible_default_action();
        assert_eq!(*cambi.borrow(), [false]);
        assert_eq!(i.accessible_checked(), Some(false));
    }

    // ─── M2, Task 9: le cuffie Bluetooth nella vista PC ───

    fn lettura_bt(imp: &str, nome: &str, percentuale: u8) -> crate::sistema::batterie_bt::LetturaBt {
        crate::sistema::batterie_bt::LetturaBt { impronta: imp.into(), nome: nome.into(), percentuale, connesso: true }
    }

    /// La riga «CUFFIE» sta in «Energia e audio»: valore e glifo con la tinta informazione a 20%
    /// o meno; con più dispositivi «+N» e per Narrator l'elenco; senza dispositivi non c'è.
    #[test]
    fn riga_cuffie_nella_vista_pc() {
        use crate::sistema::batterie_bt::{riga_cuffie, scrivi_cuffie};
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let cuffie = |p: &Pannello| ElementHandle::find_by_accessible_label(p, "CUFFIE").next();
        assert!(cuffie(&p).is_none(), "senza lettura la riga non c'è");
        scrivi_cuffie(&p, riga_cuffie(&[lettura_bt("a", "Cuffie finte", 18)]).as_ref());
        let c = cuffie(&p).expect("riga");
        assert_eq!(c.accessible_value().as_deref(), Some("18%"));
        assert_eq!(c.accessible_description().as_deref(), Some("Cuffie finte al 18%"));
        assert_eq!(p.get_cuffie_tinta(), crate::TintaBatteriaUi::Informazione);
        assert_eq!(p.get_cuffie_dettaglio(), "Cuffie finte");
        // Più dispositivi: il più scarico, «+1» e l'elenco per Narrator.
        scrivi_cuffie(
            &p,
            riga_cuffie(&[lettura_bt("a", "Cuffie finte", 60), lettura_bt("b", "Mouse finto", 30)]).as_ref(),
        );
        let c = cuffie(&p).expect("riga");
        assert_eq!(p.get_cuffie_dettaglio(), "Mouse finto +1");
        assert_eq!(p.get_cuffie_tinta(), crate::TintaBatteriaUi::Normale);
        assert_eq!(c.accessible_description().as_deref(), Some("Mouse finto al 30%, Cuffie finte al 60%"));
        // Nessun dispositivo connesso con la batteria leggibile: la riga sparisce.
        scrivi_cuffie(&p, None);
        assert!(cuffie(&p).is_none());
    }

    // ─── M2, Task 8: Wi-Fi, Bluetooth e icona della rete ───

    use crate::sistema::radio::{EsitoRadio, StatoRadio, TipoRadio};

    /// Un pannello sulla vista PC con tutto: batteria, volume muto, connessioni con nome lungo,
    /// rete veloce, Wi-Fi acceso e Bluetooth spento.
    fn pannello_connessioni() -> Pannello {
        let p = pannello();
        p.set_schede(schede());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_pc(StatPcUi {
            pronta: true,
            cpu: 0.5,
            cpu_testo: "100%".into(),
            ram: 0.6,
            ram_testo: "100%".into(),
            ram_dettaglio: "15,9 / 15,9 GB".into(),
            disco: 0.7,
            disco_testo: "100%".into(),
            disco_dettaglio: "1023,9 / 1023,9 GB".into(),
            rete_giu: "999,9 MB/s".into(),
            rete_su: "999,9 MB/s".into(),
            ha_batteria: true,
            batteria_testo: "100%".into(),
            batteria_frazione: 1.0,
            in_carica: true,
            ha_gpu: true,
            gpu: 1.0,
            gpu_testo: "100%".into(),
            ..Default::default()
        });
        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        p.set_volume_muto(true);
        p.set_connessioni_disponibili(true);
        p.set_rete_nota(true);
        p.set_rete(crate::TipoReteUi::Wifi);
        p.set_tacche(2);
        p.set_nome_rete("Una rete di prova dal nome davvero lunghissimo".into());
        crate::connessioni::scrivi_radio(
            &p,
            &[
                StatoRadio { tipo: TipoRadio::WiFi, acceso: true },
                StatoRadio { tipo: TipoRadio::Bluetooth, acceso: false },
            ],
        );
        p.set_vista("pc".into());
        p
    }

    /// Il pannello è 820 × 340 fisso (corpo 752 × 224): nella vista PC, col caso più pieno (batteria, volume muto,
    /// rete veloce, nome della rete lungo, riga del rifiuto, GPU al 100%), nulla esce dalla card né
    /// dalla sua colonna.
    #[test]
    fn vista_pc_entra_nel_pannello() {
        piattaforma_di_prova();
        let p = pannello_connessioni();
        p.set_errore_radio(true);
        // Il caso più pieno anche per le cuffie: nome lungo e due dispositivi in più.
        crate::sistema::batterie_bt::scrivi_cuffie(
            &p,
            crate::sistema::batterie_bt::riga_cuffie(&[
                lettura_bt("a", "Cuffie finte con un nome lunghissimo", 18),
                lettura_bt("b", "Mouse finto", 50),
                lettura_bt("c", "Tastiera finta", 90),
            ])
            .as_ref(),
        );
        let vista = ElementHandle::find_by_element_type_name(&p, "VistaPc").next().expect("vista PC");
        let dentro = |contenitore: &ElementHandle, nome: &str| {
            let (cp, cs) = (contenitore.absolute_position(), contenitore.size());
            for e in contenitore.query_descendants().find_all() {
                let (pos, lato) = (e.absolute_position(), e.size());
                if lato.width <= 0.0 || lato.height <= 0.0 {
                    continue;
                }
                let fuori = pos.x < cp.x - 0.5
                    || pos.y < cp.y - 0.5
                    || pos.x + lato.width > cp.x + cs.width + 0.5
                    || pos.y + lato.height > cp.y + cs.height + 0.5;
                assert!(
                    !fuori,
                    "{:?} «{}» esce da {nome}: {pos:?} {lato:?} fuori da {cp:?} {cs:?}",
                    e.type_name(),
                    e.accessible_label().unwrap_or_default()
                );
            }
        };
        dentro(&vista, "la vista PC");
        let (pos, lato) = (vista.absolute_position(), vista.size());
        // Dentro il corpo della cornice: 752 × 224 a (34, 44).
        assert!(pos.x >= 34.0 && pos.y >= 44.0 && pos.x + lato.width <= 34.0 + 752.0 && pos.y + lato.height <= 44.0 + 224.0, "{pos:?} {lato:?}");
        for colonna in ["Sistema", "Energia e audio", "Connessioni"] {
            dentro(&elemento(&p, colonna), colonna);
        }
        // La riga GPU (Task 10) c'è, è l'ultima della colonna «Sistema» e sta nella card.
        let gpu = elemento(&p, "GPU");
        let (gp, gs) = (gpu.absolute_position(), gpu.size());
        let disco = elemento(&p, "DISCO");
        assert!(gp.y >= disco.absolute_position().y + disco.size().height - 0.5, "GPU sotto il disco");
        assert!(gp.y + gs.height <= pos.y + lato.height + 0.5, "GPU dentro la card");
        // La riga del rifiuto c'è e il nome della rete si vede (accorciato).
        assert!(ElementHandle::find_by_accessible_label(&p, "Non riuscito: apro le Impostazioni di Windows").next().is_some());
        assert_eq!(
            elemento(&p, "Wi-Fi").accessible_description().as_deref(),
            Some("Una rete di prova dal nome davvero lunghissimo")
        );
        assert_eq!(
            elemento(&p, "RETE").accessible_value().as_deref(),
            Some("↓ 999,9 MB/s · ↑ 999,9 MB/s, Wi-Fi, 2 tacche su 3")
        );
    }

    /// 18/11, R1-15 e R1-16: la vista PC ha un solo stile di etichetta (quelle delle righe, senza
    /// l'intestazione in tondo sopra) e il ritmo a 4/8: righe alte 40, la rete 28.
    #[test]
    fn vista_pc_un_solo_stile_e_ritmo_4_8() {
        piattaforma_di_prova();
        let p = pannello_connessioni();
        // Nessun testo di intestazione: i nomi dei gruppi li dice solo Narrator (groupbox).
        let vista = ElementHandle::find_by_element_type_name(&p, "VistaPc").next().expect("vista PC");
        for e in vista.query_descendants().find_all() {
            let t = e.accessible_label().unwrap_or_default();
            if ["Sistema", "Energia e audio", "Connessioni"].contains(&t.as_str()) {
                assert_eq!(e.accessible_role(), Some(AccessibleRole::Groupbox), "«{t}» solo come gruppo, mai come testo");
            }
        }
        let (cpu, ram, disco, gpu) = (elemento(&p, "CPU"), elemento(&p, "RAM"), elemento(&p, "DISCO"), elemento(&p, "GPU"));
        for (nome, riga) in [("CPU", &cpu), ("RAM", &ram), ("DISCO", &disco), ("GPU", &gpu)] {
            assert_eq!(riga.size().height, 40.0, "la riga {nome} è alta 40");
        }
        assert_eq!(ram.absolute_position().y - cpu.absolute_position().y, 40.0, "passo di 40");
        assert_eq!(elemento(&p, "RETE").size().height, 28.0);
        // Il margine laterale è 16: l'etichetta parte 16 px dentro la colonna.
        let colonna = elemento(&p, "Sistema");
        let etichetta = cpu
            .query_descendants()
            .find_all()
            .into_iter()
            .map(|e| e.absolute_position().x - colonna.absolute_position().x)
            .fold(f32::MAX, f32::min);
        assert_eq!(etichetta, 16.0, "margine di 16");
    }

    /// R1-4 (fix round 1): il dettaglio di RAM e disco entra intero, anche coi valori più larghi
    /// («1023,9 / 1023,9 GB», «63,9 / 127,9 GB»), nello spazio fra margine e colonna del valore.
    #[test]
    fn dettaglio_ram_e_disco_non_tagliato() {
        piattaforma_di_prova();
        for (ram, disco) in [("63,9 / 127,9 GB", "1023,9 / 1023,9 GB"), ("15,9 / 15,9 GB", "1863,0 / 1863,0 GB")] {
            let p = pannello_connessioni();
            p.set_pc(StatPcUi {
                pronta: true,
                ram: 0.5,
                ram_testo: "56%".into(),
                ram_dettaglio: ram.into(),
                disco: 0.5,
                disco_testo: "65%".into(),
                disco_dettaglio: disco.into(),
                ..Default::default()
            });
            for (nome, dettaglio) in [("RAM", ram), ("DISCO", disco)] {
                // Il Text vero del dettaglio e la sua misura sono nella stessa riga: stessa posizione.
                // La riga per y e anche per x: un'altra colonna alla stessa altezza (CUFFIE accanto a
                // DISCO) ha gli stessi id.
                let riga = elemento(&p, nome);
                let (y, x, larga) = (riga.absolute_position().y + 22.0, riga.absolute_position().x, riga.size().width);
                let della_riga = |id: &str| {
                    ElementHandle::find_by_element_id(&p, id)
                        .find(|e| {
                            let pos = e.absolute_position();
                            (pos.y - y).abs() < 0.5 && pos.x >= x - 0.5 && pos.x < x + larga
                        })
                        .unwrap_or_else(|| panic!("nessun {id} nella riga {nome}"))
                };
                let (misura, vero) = (della_riga("Riga::misura"), della_riga("Riga::dettaglio-testo"));
                assert!(
                    misura.size().width <= vero.size().width + 0.01,
                    "«{dettaglio}» largo {} in {} px: verrebbe accorciato",
                    misura.size().width,
                    vero.size().width
                );
            }
        }
    }

    /// M2, Task 10: nella vista PC la riga «GPU» (indicatore col valore, barra come la CPU) c'è
    /// solo se c'è il dato; senza (fuori da Windows, lettura fallita) nessuna riga, mai «0%».
    #[test]
    fn riga_gpu_solo_se_c_e() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_vista("pc".into());
        p.set_pc(StatPcUi { pronta: true, ha_gpu: false, gpu: 0.37, gpu_testo: "37%".into(), ..Default::default() });
        assert!(ElementHandle::find_by_accessible_label(&p, "GPU").next().is_none(), "senza dato niente riga");
        p.set_pc(StatPcUi { pronta: true, ha_gpu: true, gpu: 0.37, gpu_testo: "37%".into(), ..Default::default() });
        let gpu = elemento(&p, "GPU");
        assert_eq!(gpu.accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert_eq!(gpu.accessible_value().as_deref(), Some("37%"));
        // Il testo «37%» si vede nella riga.
        let testo = gpu
            .query_descendants()
            .find_all()
            .into_iter()
            .any(|e| e.accessible_label().as_deref() == Some("37%"));
        assert!(testo, "il valore della GPU si vede");
    }

    /// Riquadri: interruttori per Narrator con lo stato; Bluetooth spento; senza radio
    /// «Non presente» e non cliccabile. Fuori da Windows solo la rete.
    #[test]
    fn riquadri_wifi_e_bluetooth() {
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let wifi = elemento(&p, "Wi-Fi");
        assert_eq!(wifi.accessible_role(), Some(AccessibleRole::Switch));
        assert_eq!(wifi.accessible_checked(), Some(true));
        let bt = elemento(&p, "Bluetooth");
        assert_eq!(bt.accessible_role(), Some(AccessibleRole::Switch));
        assert_eq!(bt.accessible_checked(), Some(false));
        assert_eq!(bt.accessible_description().as_deref(), Some("Spento"));
        let comandi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(i32, bool)>::new()));
        let c = comandi.clone();
        p.on_commuta_radio(move |t, a| c.borrow_mut().push((t, a)));
        bt.invoke_accessible_default_action();
        assert_eq!(*comandi.borrow(), [(1, true)]);
        assert_eq!(bt.accessible_checked(), Some(true), "l'interruttore si sposta subito");

        // Senza Bluetooth: «Non presente», non cliccabile.
        crate::connessioni::scrivi_radio(&p, &[StatoRadio { tipo: TipoRadio::WiFi, acceso: false }]);
        let bt = elemento(&p, "Bluetooth");
        assert_eq!(bt.accessible_description().as_deref(), Some("Non presente"));
        assert_eq!(bt.accessible_enabled(), Some(false));
        assert_eq!(bt.accessible_checked(), Some(false));
        bt.invoke_accessible_default_action();
        assert_eq!(comandi.borrow().len(), 1, "una radio che manca non si comanda");
        assert_eq!(elemento(&p, "Wi-Fi").accessible_description().as_deref(), Some("Spento"));

        // Fuori da Windows (connessioni non disponibili): solo la rete, niente riquadri né righe.
        p.set_connessioni_disponibili(false);
        assert!(ElementHandle::find_by_accessible_label(&p, "Wi-Fi").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&p, "Reti Wi-Fi ›").next().is_none());
        assert_eq!(elemento(&p, "RETE").accessible_value().as_deref(), Some("↓ 999,9 MB/s · ↑ 999,9 MB/s"));
    }

    /// Review focus 4: Windows rifiuta lo spegnimento del Wi-Fi. Il riquadro si sposta subito su
    /// spento; all'esito `Negato` compare la riga in ambra, il riquadro torna allo stato vero
    /// (acceso, l'ultimo `StateChanged`) e si apre la pagina del Wi-Fi delle Impostazioni.
    #[test]
    fn rifiuto_mostra_errore_e_stato_reale() {
        use crate::sistema::browser::PaginaWindows;
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let reali = [
            StatoRadio { tipo: TipoRadio::WiFi, acceso: true },
            StatoRadio { tipo: TipoRadio::Bluetooth, acceso: false },
        ];
        let comandi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(i32, bool)>::new()));
        let c = comandi.clone();
        p.on_commuta_radio(move |t, a| c.borrow_mut().push((t, a)));
        let riga = |p: &Pannello| {
            ElementHandle::find_by_accessible_label(p, "Non riuscito: apro le Impostazioni di Windows").next().is_some()
        };

        elemento(&p, "Wi-Fi").invoke_accessible_default_action();
        assert_eq!(*comandi.borrow(), [(0, false)], "il comando: Wi-Fi spento");
        assert_eq!(elemento(&p, "Wi-Fi").accessible_checked(), Some(false));
        assert!(!riga(&p));

        let pagina = crate::connessioni::esito(&p, Some(&reali), TipoRadio::WiFi, EsitoRadio::Negato);
        assert_eq!(pagina, Some(PaginaWindows::ImpostazioniWifi), "si apre la pagina del Wi-Fi");
        assert!(riga(&p), "la riga in ambra c'è");
        assert_eq!(elemento(&p, "Wi-Fi").accessible_checked(), Some(true), "di nuovo acceso, lo stato vero");

        // Un errore del Bluetooth apre la pagina del Bluetooth; un nuovo tocco toglie la riga.
        assert_eq!(
            crate::connessioni::esito(&p, Some(&reali), TipoRadio::Bluetooth, EsitoRadio::Errore),
            Some(PaginaWindows::Bluetooth)
        );
        elemento(&p, "Bluetooth").invoke_accessible_default_action();
        assert!(!riga(&p), "il tocco nuovo toglie la riga del rifiuto");
        assert_eq!(crate::connessioni::esito(&p, Some(&reali), TipoRadio::Bluetooth, EsitoRadio::Fatto), None);
        assert!(!riga(&p), "riuscito: nessuna riga");
    }

    /// Un rifiuto che arriva dopo aver lasciato la vista PC (radio non più seguite) si ignora:
    /// nessuna riga in ambra al rientro, nessuna pagina aperta, i riquadri non diventano
    /// «Non presente».
    #[test]
    fn esito_a_vista_pc_lasciata_ignorato() {
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let riga = |p: &Pannello| {
            ElementHandle::find_by_accessible_label(p, "Non riuscito: apro le Impostazioni di Windows").next().is_some()
        };
        // Lasciata la vista: le radio sono dimenticate.
        crate::connessioni::dimentica_radio(&p);
        assert!(!p.get_radio_lette());
        for esito in [EsitoRadio::Negato, EsitoRadio::Errore] {
            assert_eq!(crate::connessioni::esito(&p, None, TipoRadio::WiFi, esito), None, "{esito:?}: nessuna pagina");
        }
        assert!(!p.get_errore_radio(), "nessuna riga in ambra da mostrare al rientro");
        assert!(!p.get_radio_lette(), "i riquadri non passano a «Non presente»");
        assert!(p.get_wifi_presente() && p.get_bt_presente(), "le radio non risultano sparite");
        assert!(!riga(&p));
    }

    /// Vista PC da tastiera: dal volume Tab passa ai riquadri e alle righe (Wi-Fi, Bluetooth,
    /// «Reti Wi-Fi ›», «Dispositivi Bluetooth ›») e poi al dock; Maiusc+Tab torna indietro. Le
    /// frecce girano nella colonna; Invio commuta un riquadro o apre la pagina; Esc chiude il
    /// pannello come prima.
    #[test]
    fn connessioni_da_tastiera() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3) = (eventi.clone(), eventi.clone(), eventi.clone());
        p.on_commuta_radio(move |t, a| e1.borrow_mut().push(format!("radio {t} {a}")));
        p.on_apri_pagina(move |i| e2.borrow_mut().push(format!("pagina {i}")));
        p.on_chiudi(move || e3.borrow_mut().push("chiudi".into()));
        let ultimo = |e: &std::rc::Rc<std::cell::RefCell<Vec<String>>>| e.borrow().last().cloned().unwrap_or_default();

        // Il contenuto parte dal volume; Tab: Wi-Fi (il fuoco vero sul riquadro), poi Bluetooth.
        tasto(&p, Key::Tab);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-riquadro"), "fuoco sul riquadro: «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["radio 1 true"], "Invio sul Bluetooth (spento) lo accende");
        // ↓ dal Bluetooth: «Dispositivi Bluetooth ›»; Invio apre la pagina del Bluetooth.
        tasto(&p, Key::DownArrow);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-collegamento"), "fuoco sulla riga: «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::Return);
        assert_eq!(ultimo(&eventi), "pagina 1");
        // ←: «Reti Wi-Fi ›»; Spazio apre la pagina Wi-Fi delle Impostazioni.
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Space);
        assert_eq!(ultimo(&eventi), "pagina 0");
        // ↑: di nuovo il Wi-Fi (Invio lo spegne).
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(ultimo(&eventi), "radio 0 false");
        // Maiusc+Tab dal Wi-Fi torna al volume.
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-volume"), "fuoco sul cursore: «{}»", col_fuoco_pannello(&p));
        // Quattro Tab: Wi-Fi, Bluetooth, reti, dispositivi.
        for _ in 0..4 {
            tasto(&p, Key::Tab);
        }
        tasto(&p, Key::Return);
        assert_eq!(ultimo(&eventi), "pagina 1", "l'ultima riga");
        // Ancora Tab: il dock (le frecce lì muovono il fuoco, non la vista).
        tasto(&p, Key::Tab);
        tasto(&p, Key::RightArrow);
        assert_eq!(p.get_vista(), "pc");
        assert_eq!(ultimo(&eventi), "pagina 1", "dal dock nessun comando delle connessioni");
        // Esc fa quello che fa già nel pannello: lo chiude.
        tasto(&p, Key::Escape);
        assert_eq!(ultimo(&eventi), "chiudi");
    }

    /// La pillola col lato «Rete»: solo l'icona (16 px, posto riservato anche prima della
    /// lettura), e Narrator dice il tipo e le tacche, mai il nome della rete.
    #[test]
    fn lato_rete_nella_pillola() {
        use crate::lati::{DatiLati, Lato};
        use crate::sistema::radio::StatoRete;
        use crate::sorgenti_lati::lato_ui;
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let mut d = DatiLati::default();
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, false));
        assert_eq!(p.get_ingombro_lato_destro(), 16.0, "il posto dell'icona c'è già");
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome").next().is_some());
        d.rete = Some(StatoRete::WiFi { tacche: 2 });
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, false));
        assert_eq!(p.get_ingombro_lato_destro(), 16.0);
        assert_eq!(p.get_larghezza_pillola(), crate::lati::larghezza_riposo(0.0, 16.0));
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome, Wi-Fi, 2 tacche su 3").next().is_some());
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "IconaRete").count(), 1);
        d.rete = Some(StatoRete::Cavo);
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, false));
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome, rete via cavo").next().is_some());
        // In «dorme» `OsservaRete` è spento: l'icona si svuota (non mostra una rete forse caduta),
        // ma il posto resta e la pillola non cambia larghezza (m1 della revisione finale di M2).
        let larghezza = p.get_larghezza_pillola();
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, true));
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "IconaRete").count(), 0, "in «dorme» l'icona si svuota");
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome").next().is_some(), "Narrator non legge la rete ferma");
        assert_eq!(p.get_ingombro_lato_destro(), 16.0, "il posto resta");
        assert_eq!(p.get_larghezza_pillola(), larghezza, "stessa larghezza");
    }

    // ─── Menu «⋯» scuro in Slint (RC-58, Task 8) ───

    /// Due monitor: il secondo con la spunta.
    fn due_schermi_di_prova() -> Vec<(String, bool)> {
        vec![("Schermo principale".into(), false), ("Schermo 2 · 2560×1440".into(), true)]
    }

    type IdScelti = std::rc::Rc<std::cell::RefCell<Vec<String>>>;
    type ChiusureChieste = std::rc::Rc<std::cell::Cell<u32>>;

    /// Un pannello con il menu pronto (come lo riempie Rust a ogni apertura), il fuoco attivo e
    /// due contatori: gli id delle voci scelte e quante volte il pannello ha chiesto di chiudersi.
    fn pannello_con_menu(ritirata: bool, schermi: &[(String, bool)]) -> (Pannello, IdScelti, ChiusureChieste) {
        piattaforma_di_prova();
        let p = pannello();
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        crate::pannello::riempi_menu(&p, &crate::sistema::menu::voci_pannello(ritirata, schermi));
        crate::pannello::collega_iniziali(&p);
        let scelte = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let s = scelte.clone();
        p.on_scelta_menu(move |id| s.borrow_mut().push(id.to_string()));
        let chiusure = std::rc::Rc::new(std::cell::Cell::new(0));
        let c = chiusure.clone();
        p.on_chiudi(move || c.set(c.get() + 1));
        (p, scelte, chiusure)
    }

    /// «⋯» attivato come da Narrator o da tastiera (la prima voce parte evidenziata).
    fn apri_menu(p: &Pannello) {
        elemento(p, "Altre opzioni").invoke_accessible_default_action();
    }

    fn riga_menu(p: &Pannello, testo: &str) -> Option<ElementHandle> {
        ElementHandle::find_by_accessible_label(p, testo).next()
    }

    /// RC-58: stesse voci e stesso ordine di `voci_pannello` (unica fonte, anche per la tray),
    /// il filo prima di «Esci», il sottomenu «Schermo» solo con più monitor.
    #[test]
    fn il_menu_del_pannello_ha_le_voci_di_voci_pannello_nello_stesso_ordine() {
        use crate::sistema::menu::{voci_pannello, Voce};
        for schermi in [Vec::new(), vec![("Schermo principale".to_owned(), true)], due_schermi_di_prova()] {
            for ritirata in [false, true] {
                let (p, _, _) = pannello_con_menu(ritirata, &schermi);
                assert!(riga_menu(&p, "Impostazioni").is_none(), "chiuso: nessuna voce");
                apri_menu(&p);
                let attese: Vec<String> = voci_pannello(ritirata, &schermi)
                    .iter()
                    .map(|v| match v {
                        Voce::Semplice(_, t) => t.clone(),
                        Voce::Schermo(_) => "Schermo".to_owned(),
                    })
                    .collect();
                let y: Vec<f32> = attese
                    .iter()
                    .map(|t| riga_menu(&p, t).unwrap_or_else(|| panic!("manca la voce «{t}»")).absolute_position().y)
                    .collect();
                assert!(y.windows(2).all(|w| w[0] < w[1]), "{attese:?} nello stesso ordine: {y:?}");
                // Il filo prima di «Esci»: lo scarto con la voce prima è più grande degli altri.
                let n = y.len();
                let passo = y[1] - y[0];
                assert!((y[n - 1] - y[n - 2]) > passo + 4.0, "il filo sta prima di «Esci»: {y:?}");
                assert!(y.windows(2).take(n - 2).all(|w| (w[1] - w[0] - passo).abs() < 0.5), "le altre voci a passo fermo: {y:?}");
                assert_eq!(attese.contains(&"Schermo".to_owned()), schermi.len() > 1);
                assert_eq!(attese.contains(&"Mostra l'isola".to_owned()), ritirata);
                assert!(riga_menu(&p, "Apri").is_none(), "«Apri» non c'è nel pannello (è già aperto)");
            }
        }
    }

    /// Ogni voce chiama la stessa azione di prima: l'id che Slint passa a Rust torna al `Comando`
    /// della voce, per ogni voce e per ogni riga del sottomenu.
    #[test]
    fn ogni_voce_del_menu_porta_al_suo_comando() {
        use crate::sistema::menu::{comando, Comando};
        let schermi = due_schermi_di_prova();
        let attese = [
            ("Impostazioni", Comando::Impostazioni),
            ("Informazioni", Comando::Informazioni),
            ("Ritira l'isola", Comando::Ritirata),
            ("Esci", Comando::Esci),
            ("Schermo principale", Comando::Schermo(0)),
            ("Schermo 2 · 2560×1440", Comando::Schermo(1)),
        ];
        for (testo, atteso) in attese {
            let (p, scelte, chiusure) = pannello_con_menu(false, &schermi);
            apri_menu(&p);
            if matches!(atteso, Comando::Schermo(_)) {
                riga_menu(&p, "Schermo").expect("il sottomenu").invoke_accessible_default_action();
            }
            riga_menu(&p, testo).unwrap_or_else(|| panic!("manca «{testo}»")).invoke_accessible_default_action();
            assert_eq!(scelte.borrow().len(), 1, "{testo}: una scelta sola");
            assert_eq!(comando(&scelte.borrow()[0]), Some(atteso), "{testo}");
            assert!(riga_menu(&p, "Impostazioni").is_none(), "{testo}: scelta fatta il menu si chiude");
            assert_eq!(chiusure.get(), 0, "{testo}: il pannello lo chiude Rust, non il menu");
            assert_eq!(etichetta_col_fuoco(&p), "Altre opzioni", "{testo}: il fuoco torna su «⋯»");
        }
        // «Mostra l'isola» è la stessa azione della ritirata.
        let (p, scelte, _) = pannello_con_menu(true, &[]);
        apri_menu(&p);
        riga_menu(&p, "Mostra l'isola").expect("la voce").invoke_accessible_default_action();
        assert_eq!(comando(&scelte.borrow()[0]), Some(Comando::Ritirata));
    }

    /// Da tastiera: Invio e Spazio attivano la voce col fuoco, che parte dalla prima.
    #[test]
    fn invio_e_spazio_attivano_la_voce_col_fuoco() {
        use crate::sistema::menu::{comando, Comando};
        use slint::platform::Key;
        for (tasto_scelta, giu, atteso) in [
            (Key::Return, 0, Comando::Impostazioni),
            (Key::Space, 1, Comando::Informazioni),
            (Key::Return, 2, Comando::Ritirata),
            (Key::Space, 3, Comando::Esci),
        ] {
            let (p, scelte, _) = pannello_con_menu(false, &[]);
            apri_menu(&p);
            // Le righe nascono al primo ridisegno (qui: alla prima ricerca): allora prendono il fuoco.
            assert!(riga_menu(&p, "Impostazioni").is_some());
            assert_eq!(etichetta_col_fuoco(&p), "Impostazioni", "aperto da tastiera: la prima voce");
            for _ in 0..giu {
                tasto(&p, Key::DownArrow);
            }
            tasto(&p, tasto_scelta);
            assert_eq!(comando(&scelte.borrow()[0]), Some(atteso));
        }
        // Con due monitor: → apre «Schermo», ↓ scende, Invio sceglie lo schermo.
        let (p, scelte, _) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Schermo");
        tasto(&p, Key::RightArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Schermo 2 · 2560×1440", "il sottomenu parte dallo schermo spuntato");
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(comando(&scelte.borrow()[0]), Some(Comando::Schermo(0)));
    }

    /// ↑ e ↓ girano intorno: ↑ dalla prima va all'ultima, ↓ dall'ultima torna alla prima (come i
    /// menu di Windows, che sostituiscono).
    #[test]
    fn frecce_nel_menu_girano_intorno() {
        use slint::platform::Key;
        let (p, _, _) = pannello_con_menu(false, &[]);
        apri_menu(&p);
        tasto(&p, Key::UpArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Esci");
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Impostazioni");
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Informazioni");
        // Nel sottomenu girano allo stesso modo.
        let (p, _, _) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Schermo principale", "dall'ultimo schermo al primo");
        tasto(&p, Key::UpArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Schermo 2 · 2560×1440");
    }

    /// Esc chiude SOLO il menu (a un livello per volta: prima il sottomenu), il pannello resta e il
    /// fuoco torna su «⋯»; un secondo Esc chiude il pannello come sempre.
    #[test]
    fn esc_chiude_solo_il_menu_e_il_fuoco_torna_su_altre_opzioni() {
        use slint::platform::Key;
        let (p, scelte, chiusure) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        assert!(riga_menu(&p, "Esci").is_some());
        tasto(&p, Key::Escape);
        assert!(riga_menu(&p, "Esci").is_none(), "il menu si è chiuso");
        assert_eq!(chiusure.get(), 0, "il pannello resta");
        assert_eq!(etichetta_col_fuoco(&p), "Altre opzioni", "fuoco su «⋯»");
        assert!(scelte.borrow().is_empty(), "nessuna voce eseguita");
        // Con il sottomenu aperto: Esc lo chiude, il menu resta, il fuoco torna su «Schermo».
        apri_menu(&p);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::RightArrow);
        assert!(riga_menu(&p, "Schermo principale").is_some());
        tasto(&p, Key::Escape);
        assert!(riga_menu(&p, "Schermo principale").is_none(), "il sottomenu si è chiuso");
        assert!(riga_menu(&p, "Esci").is_some(), "il menu resta");
        assert_eq!(etichetta_col_fuoco(&p), "Schermo");
        tasto(&p, Key::Escape);
        assert!(riga_menu(&p, "Esci").is_none());
        assert_eq!((chiusure.get(), etichetta_col_fuoco(&p).as_str()), (0, "Altre opzioni"));
        // Chiuso il menu, Esc è di nuovo del pannello.
        tasto(&p, Key::Escape);
        assert_eq!(chiusure.get(), 1, "il secondo Esc chiude il pannello");
    }

    /// ← chiude il sottomenu (e basta), → su una voce senza sottomenu non fa nulla.
    #[test]
    fn freccia_sinistra_chiude_il_sottomenu() {
        use slint::platform::Key;
        let (p, scelte, chiusure) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        tasto(&p, Key::RightArrow);
        assert!(riga_menu(&p, "Schermo principale").is_none(), "→ su «Impostazioni»: niente");
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::RightArrow);
        assert!(riga_menu(&p, "Schermo principale").is_some());
        tasto(&p, Key::LeftArrow);
        assert!(riga_menu(&p, "Schermo principale").is_none());
        assert_eq!(etichetta_col_fuoco(&p), "Schermo");
        assert!(riga_menu(&p, "Esci").is_some());
        assert!((chiusure.get(), scelte.borrow().len()) == (0, 0));
    }

    /// Lo schermo di adesso ha la spunta (per Narrator e nel disegno), gli altri no.
    #[test]
    fn lo_schermo_scelto_ha_la_spunta() {
        let (p, _, _) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        riga_menu(&p, "Schermo").expect("il sottomenu").invoke_accessible_default_action();
        let primo = riga_menu(&p, "Schermo principale").expect("primo schermo");
        let secondo = riga_menu(&p, "Schermo 2 · 2560×1440").expect("secondo schermo");
        assert_eq!(primo.accessible_checkable(), Some(true));
        assert_eq!(primo.accessible_checked(), Some(false));
        assert_eq!(secondo.accessible_checked(), Some(true));
        // Le voci semplici non sono a spunta.
        assert_ne!(riga_menu(&p, "Esci").expect("Esci").accessible_checkable(), Some(true));
    }

    /// Il clic fuori chiude il menu e non fa altro (il pannello resta); il clic su «⋯» col mouse lo
    /// apre senza anello sulle voci; un clic su una voce la sceglie.
    #[test]
    fn clic_fuori_chiude_il_menu() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (p, scelte, chiusure) = pannello_con_menu(false, &[]);
        let clic = |x: f32, y: f32| {
            let position = slint::LogicalPosition::new(x, y);
            p.window().dispatch_event(WindowEvent::PointerMoved { position });
            p.window().dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
            p.window().dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        };
        let piu = elemento(&p, "Altre opzioni");
        let (pos, lato) = (piu.absolute_position(), piu.size());
        clic(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        assert!(riga_menu(&p, "Esci").is_some(), "il clic su «⋯» apre il menu");
        // Un punto del pannello lontano dal menu (in basso a sinistra).
        clic(40.0, 220.0);
        assert!(riga_menu(&p, "Esci").is_none(), "il clic fuori lo chiude");
        assert_eq!(chiusure.get(), 0, "e non chiude il pannello");
        assert!(scelte.borrow().is_empty());
        assert_eq!(etichetta_col_fuoco(&p), "Pannello di ItsyHome", "col mouse il fuoco va al pannello");
        // Chiuso, il clic fuori torna a passare agli elementi sotto (qui: gli occhi chiudono il pannello).
        let occhi = elemento(&p, "Chiudi il pannello");
        let (pos, lato) = (occhi.absolute_position(), occhi.size());
        clic(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        assert_eq!(chiusure.get(), 1, "a menu chiuso gli elementi sotto rispondono");
        // Un clic su una voce la sceglie.
        let (p, scelte, _) = pannello_con_menu(false, &[]);
        let clic = |x: f32, y: f32| {
            let position = slint::LogicalPosition::new(x, y);
            p.window().dispatch_event(WindowEvent::PointerMoved { position });
            p.window().dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
            p.window().dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        };
        let piu = elemento(&p, "Altre opzioni");
        let (pos, lato) = (piu.absolute_position(), piu.size());
        clic(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        let esci = riga_menu(&p, "Esci").expect("Esci");
        let (pos, lato) = (esci.absolute_position(), esci.size());
        clic(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        assert_eq!(*scelte.borrow(), ["esci"]);
    }

    /// m-1 (revisione finale): a menu aperto la rotella non attraversa il menu (il volume della
    /// barra non cambia sotto); chiuso, la rotella sul volume torna a cambiarlo.
    #[test]
    fn la_rotella_non_attraversa_il_menu_aperto() {
        use slint::platform::WindowEvent;
        let (p, _, _) = pannello_con_menu(false, &[]);
        p.set_vista("panoramica".into());
        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        p.set_rotella_volume(true);
        let scorsi = std::rc::Rc::new(std::cell::Cell::new(0));
        let s = scorsi.clone();
        p.on_imposta_volume(move |_| s.set(s.get() + 1));
        let volume = elemento(&p, "Volume 40 %");
        let (pos, lato) = (volume.absolute_position(), volume.size());
        let rotella = || {
            let position = slint::LogicalPosition::new(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
            p.window().dispatch_event(WindowEvent::PointerMoved { position });
            p.window().dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: 60.0 });
        };
        apri_menu(&p);
        assert!(riga_menu(&p, "Esci").is_some());
        rotella();
        assert_eq!(scorsi.get(), 0, "a menu aperto il volume non cambia");
        assert!(riga_menu(&p, "Esci").is_some(), "e il menu resta aperto");
        tasto(&p, slint::platform::Key::Escape);
        assert!(riga_menu(&p, "Esci").is_none());
        rotella();
        assert_eq!(scorsi.get(), 1, "a menu chiuso la rotella cambia il volume");
    }

    /// Il mouse sul centro di un elemento (lo «passa sopra») e, con `clicca`, il clic sinistro.
    fn mouse_su(p: &Pannello, e: &ElementHandle, clicca: bool) {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (pos, lato) = (e.absolute_position(), e.size());
        let position = slint::LogicalPosition::new(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        p.window().dispatch_event(WindowEvent::PointerMoved { position });
        if clicca {
            p.window().dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
            p.window().dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        }
    }

    /// I-1: col mouse «Schermo» si apre al passaggio e il clic (il gesto naturale) NON lo richiude.
    #[test]
    fn col_mouse_il_clic_su_schermo_non_chiude_il_sottomenu() {
        let (p, _, _) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        let schermo = riga_menu(&p, "Schermo").expect("il sottomenu");
        mouse_su(&p, &schermo, false);
        assert!(riga_menu(&p, "Schermo principale").is_some(), "il passaggio lo apre");
        mouse_su(&p, &schermo, true);
        assert!(riga_menu(&p, "Schermo principale").is_some(), "il clic non lo richiude");
        // E si sceglie lo schermo col mouse.
        let (p, scelte, _) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        let schermo = riga_menu(&p, "Schermo").expect("il sottomenu");
        mouse_su(&p, &schermo, true);
        let primo = riga_menu(&p, "Schermo principale").expect("primo schermo");
        mouse_su(&p, &primo, true);
        assert_eq!(*scelte.borrow(), ["schermo:0"]);
    }

    /// m-6: le voci sotto «Schermo» accanto al sottomenu non lo chiudono (il mouse che scende in
    /// diagonale le attraversa); una voce sopra, o oltre il sottomenu, sì.
    #[test]
    fn il_sottomenu_resta_aperto_mentre_il_mouse_scende_verso_di_lui() {
        let tre: Vec<(String, bool)> = (0..3).map(|i| (format!("Monitor {i}"), i == 0)).collect();
        let (p, _, _) = pannello_con_menu(false, &tre);
        apri_menu(&p);
        mouse_su(&p, &riga_menu(&p, "Schermo").expect("Schermo"), false);
        assert!(riga_menu(&p, "Monitor 0").is_some());
        mouse_su(&p, &riga_menu(&p, "Ritira l'isola").expect("Ritira"), false);
        assert!(riga_menu(&p, "Monitor 0").is_some(), "accanto al sottomenu: resta");
        mouse_su(&p, &riga_menu(&p, "Informazioni").expect("Informazioni"), false);
        assert!(riga_menu(&p, "Monitor 0").is_none(), "una voce sopra lo chiude");
    }

    /// m-2: aperto col mouse, il primo tasto porta il fuoco vero sulla voce (non su «⋯»).
    #[test]
    fn dopo_il_clic_su_altre_opzioni_la_freccia_porta_il_fuoco_sulla_voce() {
        use slint::platform::Key;
        let (p, _, _) = pannello_con_menu(false, &[]);
        let piu = elemento(&p, "Altre opzioni");
        mouse_su(&p, &piu, true);
        assert!(riga_menu(&p, "Impostazioni").is_some());
        tasto(&p, Key::DownArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Impostazioni");
    }

    /// m-3: il menu è un gruppo con nome e «⋯» dice se è aperto.
    #[test]
    fn il_menu_ha_un_nome_e_altre_opzioni_dice_se_e_aperto() {
        let (p, _, _) = pannello_con_menu(false, &[]);
        assert_eq!(elemento(&p, "Altre opzioni").accessible_expanded(), Some(false));
        apri_menu(&p);
        assert_eq!(elemento(&p, "Altre opzioni").accessible_expanded(), Some(true));
        assert_eq!(elemento(&p, "Menu Altre opzioni").accessible_role(), Some(AccessibleRole::Groupbox));
    }

    /// m-4: la lettera salta alla voce che comincia così (anche nel sottomenu).
    #[test]
    fn la_lettera_salta_alla_voce_nel_menu() {
        use slint::platform::Key;
        let (p, _, _) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        tasto(&p, "r");
        assert_eq!(etichetta_col_fuoco(&p), "Ritira l'isola");
        tasto(&p, "E");
        assert_eq!(etichetta_col_fuoco(&p), "Esci");
        tasto(&p, "i");
        assert_eq!(etichetta_col_fuoco(&p), "Impostazioni");
        tasto(&p, "i");
        assert_eq!(etichetta_col_fuoco(&p), "Informazioni");
        tasto(&p, "s");
        tasto(&p, Key::RightArrow);
        tasto(&p, "schermo 2");
        assert_eq!(etichetta_col_fuoco(&p), "Schermo 2 · 2560×1440", "il nome intero non è una lettera");
        tasto(&p, "s");
        assert_eq!(etichetta_col_fuoco(&p), "Schermo principale", "nel sottomenu dopo l'ultima si riparte");
    }

    /// m-7: con molti monitor il sottomenu sta dentro il pannello e scorre col fuoco.
    #[test]
    fn con_molti_monitor_il_sottomenu_scorre_dentro_il_pannello() {
        use slint::platform::Key;
        let molti: Vec<(String, bool)> = (0..12).map(|i| (format!("Monitor {i}"), i == 0)).collect();
        let (p, _, _) = pannello_con_menu(false, &molti);
        apri_menu(&p);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::RightArrow);
        let dentro = |t: &str| {
            let r = riga_menu(&p, t).unwrap_or_else(|| panic!("manca «{t}»"));
            let (a, l) = (r.absolute_position(), r.size());
            a.y >= 0.0 && a.y + l.height <= p.get_altezza_pannello()
        };
        assert!(dentro("Monitor 0"), "la prima si vede all'apertura");
        tasto(&p, Key::UpArrow);
        assert_eq!(etichetta_col_fuoco(&p), "Monitor 11");
        assert!(dentro("Monitor 11"), "l'ultima si vede quando ha il fuoco");
    }

    /// Il menu sta dentro l'isola (800 × 340, nessuna finestra nuova), sotto «⋯» e a filo del
    /// suo bordo destro, anche col sottomenu aperto.
    #[test]
    fn il_menu_sta_dentro_il_pannello_sotto_il_pulsante() {
        let (p, _, _) = pannello_con_menu(false, &due_schermi_di_prova());
        apri_menu(&p);
        riga_menu(&p, "Schermo").expect("il sottomenu").invoke_accessible_default_action();
        let piu = elemento(&p, "Altre opzioni");
        let (pp, pl) = (piu.absolute_position(), piu.size());
        let esci = riga_menu(&p, "Esci").expect("Esci");
        let (ep, el) = (esci.absolute_position(), esci.size());
        assert!(ep.y > pp.y + pl.height, "sotto il pulsante");
        assert!(ep.x + el.width <= pp.x + pl.width + 0.5, "a filo del bordo destro: {ep:?} {el:?}");
        for testo in ["Impostazioni", "Esci", "Schermo principale", "Schermo 2 · 2560×1440"] {
            let r = riga_menu(&p, testo).expect("riga");
            let (a, l) = (r.absolute_position(), r.size());
            assert!(a.x >= 10.0 && a.y >= 0.0 && a.x + l.width <= 810.0 && a.y + l.height <= 340.0, "{testo} dentro l'isola: {a:?} {l:?}");
        }
        let sotto = riga_menu(&p, "Schermo 2 · 2560×1440").expect("riga");
        assert!(sotto.absolute_position().x + sotto.size().width <= esci.absolute_position().x, "il sottomenu sta a sinistra del menu");
    }

    // ─── Ridisegno F1, Task 10: le superfici di oggi sulla base nera ───

    /// Le viste di oggi stanno nel corpo della cornice e lo usano tutto: 752 × 224 a (34, 44)
    /// (prima stavano sopra la vecchia riga dei puntini, alte 202). Nessun elemento accessibile
    /// esce dal corpo.
    #[test]
    fn pagine_dentro_il_corpo() {
        let (casa, _) = pannello_casa();
        let (agenti, _) = pannello_claude(true, tre_righe_claude());
        agenti.set_claude_cinque_ore(limite_claude("34%", 0.34, "si azzera alle 14:30"));
        agenti.set_claude_sette_giorni(limite_claude("61%", 0.61, "si azzera mercoledì alle 14:30"));
        agenti.set_claude_spesa("12,40 $".into());
        let pc = pannello_connessioni();
        let (collegamenti, _) = pannello_collegamenti(quattro_collegamenti());
        collegamenti.set_vista("collegamenti".into());
        for (p, tipo) in [(&casa, "VistaCasa"), (&agenti, "VistaClaude"), (&pc, "VistaPc"), (&collegamenti, "VistaCollegamenti")] {
            // Lo scorrimento del cambio di vista finito.
            fine_scorrimento();
            fine_scorrimento();
            let vista = ElementHandle::find_by_element_type_name(p, tipo).next().unwrap_or_else(|| panic!("{tipo} assente"));
            let (pos, lato) = (vista.absolute_position(), vista.size());
            assert_eq!((pos.x, pos.y, lato.width, lato.height), (34.0, 44.0, 752.0, 224.0), "{tipo}: il corpo intero");
            for e in vista.query_descendants().find_all() {
                let (a, l) = (e.absolute_position(), e.size());
                if e.accessible_role().is_none_or(|r| r == AccessibleRole::None) || l.width <= 0.0 || l.height <= 0.0 {
                    continue;
                }
                assert!(
                    a.x >= 34.0 - 0.5 && a.y >= 44.0 - 0.5 && a.x + l.width <= 34.0 + 752.0 + 0.5 && a.y + l.height <= 44.0 + 224.0 + 0.5,
                    "{tipo}: «{}» esce dal corpo: {a:?} {l:?}",
                    e.accessible_label().unwrap_or_default()
                );
            }
        }
    }

    /// L'anello del fuoco è quello di Windows: 2 px bianchi e, fuori, 1 px nero (si vede sul nero
    /// e sul bianco del primario).
    #[test]
    fn anello_bianco_e_nero_come_windows() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        elemento(&f, "Aggiungi una casa").invoke_accessible_default_action();
        let anello = ElementHandle::find_by_element_type_name(&f, "Anello").next().expect("anello sul campo col fuoco");
        let base = anello.size().width;
        let mut larghezze: Vec<f32> = anello.query_descendants().find_all().iter().map(|e| e.size().width - base).collect();
        larghezze.sort_by(f32::total_cmp);
        // Distanza 2 di serie: il bianco largo 2 + 2 + 2 per lato, il nero 1 px ancora più fuori.
        assert_eq!(larghezze, [8.0, 10.0], "un filo bianco e uno nero fuori");
    }

    /// I pulsanti visibili di una finestra: nessuno esce dalla finestra né ha il testo accorciato.
    /// Il testo di un `Pulsante` è largo al più quanto lui meno 12: se lo tocca, è tagliato.
    fn pulsanti_tagliati(f: &FinestraImpostazioni, larghezza: f32) -> Vec<String> {
        use i_slint_backend_testing::ElementRoot;
        let mut tagliati = Vec::new();
        for e in f.root_element().query_descendants().find_all() {
            if e.accessible_role() != Some(AccessibleRole::Button) {
                continue;
            }
            let (a, l) = (e.absolute_position(), e.size());
            if l.width <= 0.0 || l.height <= 0.0 {
                continue;
            }
            let nome = e.accessible_label().unwrap_or_default();
            if a.x < -0.5 || a.x + l.width > larghezza + 0.5 {
                tagliati.push(format!("«{nome}» fuori dalla finestra: {a:?} {l:?}"));
            }
            if matches!(e.type_name().as_deref(), Some("Pulsante" | "Bottone")) {
                for t in e.query_descendants().find_all() {
                    if t.type_name().as_deref() == Some("Text") && t.size().width > l.width - 12.0 - 0.5 {
                        tagliati.push(format!("«{nome}» col testo accorciato: {} su {}", t.size().width, l.width));
                    }
                }
            }
        }
        tagliati
    }

    /// Task 10: a finestra minima (600 px) coi caratteri nuovi nessun pulsante tagliato, in ogni
    /// sezione, coi testi più lunghi che le sezioni mostrano.
    #[test]
    fn impostazioni_a_finestra_minima() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(600.0, 420.0));
        f.set_ambienti(ModelRc::new(VecModel::from(vec![crate::AmbienteUi {
            id: "a".into(),
            nome: "Una casa dal nome davvero molto lungo".into(),
            url: "https://casa.esempio.lan:8123".into(),
            stato: "Non raggiungibile".into(),
            attivo: true,
            collegato: true,
            ..Default::default()
        }])));
        f.set_collegamenti(quattro_collegamenti());
        f.set_claude_collegato(true);
        f.set_claude_da_aggiornare(true);
        f.set_claude_stato("Da aggiornare".into());
        f.set_api_presente(true);
        let mut tagliati = Vec::new();
        for sezione in 0..=crate::finestra_impostazioni::SEZIONE_INFORMAZIONI {
            f.set_sezione(sezione);
            tagliati.extend(pulsanti_tagliati(&f, 600.0).into_iter().map(|t| format!("sezione {sezione}: {t}")));
        }
        assert!(tagliati.is_empty(), "{}", tagliati.join("\n"));
    }
}
