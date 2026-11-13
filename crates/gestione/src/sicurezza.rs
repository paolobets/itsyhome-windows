//! Regole per il contenuto ostile dell'area web (mail, documenti: mai fidati).
//!
//! Le difese stanno a più livelli: JavaScript spento e devtools spenti nel motore web,
//! navigazione bloccata (`consenti_navigazione`), nuove finestre e scaricamenti negati, e in più
//! una Content-Security-Policy messa in testa al documento (`con_csp`) che chiude anche ciò che il
//! gestore della navigazione non vede: riquadri (`<iframe>`), moduli, file locali.

/// La sola navigazione permessa è il caricamento iniziale del contenuto passato con `apri`
/// (WebView2 lo annuncia come `about:blank` o `data:text/html`); dopo, ogni navigazione è
/// bloccata (clic sui link, `<meta refresh>`, moduli).
pub fn consenti_navigazione(url: &str, gia_caricato: bool) -> bool {
    !gia_caricato && (url == "about:blank" || url.starts_with("data:text/html"))
}

/// Politica del contenuto: niente script, riquadri, moduli, oggetti e file locali; restano gli
/// stili in linea e le immagini (incorporate o via https).
pub const CSP: &str = "default-src 'none'; img-src data: https:; style-src 'unsafe-inline' https:; \
font-src data: https:; form-action 'none'; frame-src 'none'; base-uri 'none'";

/// Mette la politica come primo elemento del documento: subito dopo un eventuale `<!doctype>`
/// iniziale (così la pagina resta in modalità standard), altrimenti in cima. Un `<head>` cercato
/// nel testo si potrebbe nascondere in un commento: la cima del documento no.
pub fn con_csp(html: &str) -> String {
    let meta = format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#);
    let corpo = html.trim_start_matches('\u{feff}');
    let inizio = corpo.len() - corpo.trim_start().len();
    let resto = &corpo[inizio..];
    let ha_doctype = resto.get(..9).is_some_and(|s| s.eq_ignore_ascii_case("<!doctype"));
    match resto.find('>').filter(|_| ha_doctype) {
        Some(fine) => {
            let taglio = inizio + fine + 1;
            format!("{}{meta}{}", &corpo[..taglio], &corpo[taglio..])
        }
        None => format!("{meta}{corpo}"),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn navigazione_consentita_solo_al_primo_caricamento() {
        assert!(consenti_navigazione("about:blank", false));
        assert!(!consenti_navigazione("https://x", true));
        // Prima del caricamento passa solo il contenuto locale, non un indirizzo esterno.
        assert!(!consenti_navigazione("https://x", false));
        assert!(!consenti_navigazione("file:///C:/Windows/win.ini", false));
        // Dopo il caricamento nemmeno about:blank.
        assert!(!consenti_navigazione("about:blank", true));
    }

    #[test]
    fn la_politica_e_il_primo_elemento_del_documento() {
        let meta = format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#);
        assert_eq!(
            con_csp("<!DOCTYPE html><html><head><title>x</title>"),
            format!("<!DOCTYPE html>{meta}<html><head><title>x</title>")
        );
        assert_eq!(con_csp("\n  <!doctype html>\n<p>a</p>"), format!("\n  <!doctype html>{meta}\n<p>a</p>"));
        // Senza doctype (o con un commento davanti) va in cima: mai dentro un commento.
        assert_eq!(con_csp("<!-- <head> --><p>a</p>"), format!("{meta}<!-- <head> --><p>a</p>"));
        assert_eq!(con_csp(""), meta);
    }
}
