//! Piano B: se il motore web manca, l'HTML diventa testo semplice mostrato dalla cornice Slint.

/// Toglie i tag e il contenuto di `<head>`, `<script>`, `<style>`; i blocchi (paragrafi, righe,
/// celle) diventano a capo o spazi; le entità più comuni si decodificano; righe vuote compattate.
pub fn html_a_testo(html: &str) -> String {
    let mut uscita = String::new();
    let mut resto = html;
    while let Some(i) = resto.find('<') {
        uscita.push_str(&decodifica(&resto[..i]));
        let dopo = &resto[i + 1..];
        let Some(fine) = dopo.find('>') else {
            resto = "";
            break;
        };
        let tag = dopo[..fine].trim().to_ascii_lowercase();
        let nome: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        resto = &dopo[fine + 1..];
        if tag.starts_with("!--") {
            // Commento: si salta fino a `-->`.
            resto = dopo.find("-->").map_or("", |j| &dopo[j + 3..]);
            continue;
        }
        if !tag.starts_with('/') && matches!(nome.as_str(), "script" | "style" | "head" | "title") {
            let chiusura = format!("</{nome}");
            resto = resto.to_ascii_lowercase().find(&chiusura).map_or("", |j| {
                let r = &resto[j..];
                r.find('>').map_or("", |k| &r[k + 1..])
            });
            continue;
        }
        match nome.as_str() {
            "br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "table" | "hr" => {
                uscita.push('\n')
            }
            "td" | "th" => uscita.push('\t'),
            _ => {}
        }
    }
    uscita.push_str(&decodifica(resto));
    compatta(&uscita)
}

fn decodifica(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&euro;", "€")
        .replace("&amp;", "&")
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
            <body><h1>Ciao</h1><script>alert(1)</script><!-- nascosto --><p>Uno &amp; due&nbsp;€</p>\
            <table><tr><td>A</td><td>19,90 &euro;</td></tr></table></body></html>";
        assert_eq!(html_a_testo(html), "Ciao\n\nUno & due €\n\nA 19,90 €");
    }
}
