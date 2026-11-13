//! Piano B: se il motore web manca, l'HTML diventa testo semplice mostrato dalla cornice Slint.
//!
//! L'HTML è ostile: il lavoro è lineare nella sua lunghezza (nessuna copia del resto del
//! documento a ogni tag), così migliaia di `<style>` non bloccano la finestra.

/// Toglie i tag e il contenuto di `<head>`, `<script>`, `<style>`, `<title>` e dei commenti; i
/// blocchi (paragrafi, righe, celle) diventano a capo o spazi; le entità (con nome comuni e
/// numeriche) si decodificano; righe vuote compattate.
pub fn html_a_testo(html: &str) -> String {
    let mut uscita = String::with_capacity(html.len() / 2);
    let mut i = 0;
    while let Some(rel) = html[i..].find('<') {
        let apertura = i + rel;
        decodifica_in(&html[i..apertura], &mut uscita);
        let dopo = apertura + 1;
        if html[dopo..].starts_with("!--") {
            // Commento: si salta fino a `-->`.
            i = html[dopo + 3..].find("-->").map_or(html.len(), |j| dopo + 3 + j + 3);
            continue;
        }
        let Some(fine_rel) = html[dopo..].find('>') else {
            i = html.len();
            break;
        };
        let tag = html[dopo..dopo + fine_rel].trim();
        i = dopo + fine_rel + 1;
        let chiusura = tag.starts_with('/');
        let nome = nome_del_tag(tag.trim_start_matches('/'));
        if !chiusura && ["script", "style", "head", "title"].iter().any(|n| n.eq_ignore_ascii_case(nome)) {
            // Contenuto saltato fino a `</nome …>` (maiuscole o minuscole).
            i = match trova_senza_maiuscole(html, i, &format!("</{nome}")) {
                Some(j) => html[j..].find('>').map_or(html.len(), |k| j + k + 1),
                None => html.len(),
            };
            continue;
        }
        const BLOCCHI: [&str; 13] = ["br", "p", "div", "tr", "li", "h1", "h2", "h3", "h4", "h5", "h6", "table", "hr"];
        if BLOCCHI.iter().any(|n| n.eq_ignore_ascii_case(nome)) {
            uscita.push('\n');
        } else if nome.eq_ignore_ascii_case("td") || nome.eq_ignore_ascii_case("th") {
            uscita.push('\t');
        }
    }
    decodifica_in(&html[i..], &mut uscita);
    compatta(&uscita)
}

/// Il nome di un tag: le lettere e cifre iniziali.
fn nome_del_tag(tag: &str) -> &str {
    let fine = tag.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(tag.len());
    &tag[..fine]
}

/// Prima posizione di `ago` (ASCII) in `pagliaio[da..]`, senza badare alle maiuscole.
fn trova_senza_maiuscole(pagliaio: &str, da: usize, ago: &str) -> Option<usize> {
    let p = pagliaio.as_bytes();
    let a = ago.as_bytes();
    (da..=p.len().checked_sub(a.len())?).find(|&j| p[j..j + a.len()].eq_ignore_ascii_case(a))
}

/// Decodifica le entità in un solo passaggio (così `&amp;lt;` resta `&lt;`).
fn decodifica_in(s: &str, uscita: &mut String) {
    let mut resto = s;
    while let Some(e) = resto.find('&') {
        uscita.push_str(&resto[..e]);
        let dopo = &resto[e + 1..];
        // Un'entità è corta: oltre 10 caratteri senza `;` è testo normale.
        let fine = dopo.char_indices().take(10).find(|&(_, c)| c == ';').map(|(k, _)| k);
        match fine.and_then(|k| entita(&dopo[..k]).map(|c| (k, c))) {
            Some((k, c)) => {
                uscita.push(c);
                resto = &dopo[k + 1..];
            }
            None => {
                uscita.push('&');
                resto = dopo;
            }
        }
    }
    uscita.push_str(resto);
}

fn entita(nome: &str) -> Option<char> {
    if let Some(num) = nome.strip_prefix('#') {
        let codice = match num.strip_prefix(['x', 'X']) {
            Some(esa) => u32::from_str_radix(esa, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(codice).filter(|c| *c != '\0');
    }
    Some(match nome {
        "nbsp" => ' ',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "amp" => '&',
        "euro" => '€',
        "laquo" => '«',
        "raquo" => '»',
        "egrave" => 'è',
        "eacute" => 'é',
        "agrave" => 'à',
        "igrave" => 'ì',
        "ograve" => 'ò',
        "ugrave" => 'ù',
        _ => return None,
    })
}

/// Spazi in fila → uno; al più una riga vuota tra i blocchi; niente spazi ai bordi.
fn compatta(s: &str) -> String {
    let mut righe: Vec<String> = Vec::new();
    for riga in s.lines() {
        let riga = riga.split_whitespace().collect::<Vec<_>>().join(" ");
        if riga.is_empty() && righe.last().is_none_or(|r| r.is_empty()) {
            continue;
        }
        righe.push(riga);
    }
    while righe.last().is_some_and(|r| r.is_empty()) {
        righe.pop();
    }
    righe.join("\n")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn l_html_diventa_testo_leggibile() {
        let html = "<!doctype html><html><head><title>T</title><style>p{color:red}</style></head>\
            <body><h1>Ciao</h1><SCRIPT>alert(1)</Script><!-- nascosto --><p>Uno &amp; due&nbsp;€</p>\
            <table><tr><td>A</td><td>19,90 &euro;</td></tr></table></body></html>";
        assert_eq!(html_a_testo(html), "Ciao\n\nUno & due €\n\nA 19,90 €");
    }

    #[test]
    fn entita_numeriche_e_doppie() {
        assert_eq!(html_a_testo("l&#8217;ora &#x2019; &amp;lt; &#0; &bogus; a & b"), "l’ora ’ &lt; &#0; &bogus; a & b");
    }

    #[test]
    fn tanti_stili_non_rallentano() {
        // 20 000 `<style>` senza chiusura e 20 000 aperti e chiusi: con la vecchia copia del resto
        // a ogni tag erano miliardi di byte copiati.
        let aperti = "<style>".repeat(20_000);
        assert_eq!(html_a_testo(&aperti), "");
        let chiusi = "<style>x</style>a".repeat(20_000);
        let inizio = std::time::Instant::now();
        assert_eq!(html_a_testo(&chiusi).len(), 20_000);
        assert!(inizio.elapsed() < std::time::Duration::from_secs(2));
    }
}
