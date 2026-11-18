//! Regole per il contenuto ostile dell'area web (mail, documenti: mai fidati).
//!
//! Le difese stanno a più livelli: JavaScript spento e devtools spenti nel motore web,
//! navigazione bloccata (`consenti_navigazione`), nuove finestre e scaricamenti negati, e in più
//! una Content-Security-Policy messa in testa al documento (`con_testa_fidata`) che chiude anche ciò che il
//! gestore della navigazione non vede: riquadri (`<iframe>`), moduli, file locali e ogni risorsa
//! remota (immagini, stili, caratteri).

/// La sola navigazione permessa è il caricamento iniziale del contenuto passato con `apri`
/// (WebView2 lo annuncia come `about:blank` o `data:text/html`); dopo, ogni navigazione è
/// bloccata (clic sui link, `<meta refresh>`, moduli).
pub fn consenti_navigazione(url: &str, gia_caricato: bool) -> bool {
    !gia_caricato && (url == "about:blank" || url.starts_with("data:text/html"))
}

/// Politica del contenuto: niente script, riquadri, moduli, oggetti e file locali, e **nessuna
/// risorsa remota** (immagini, fogli di stile, `@import`, caratteri): un pixel tracciante in una
/// mail direbbe al mittente che l'hai aperta e quando. Restano gli stili in linea e le immagini
/// incorporate (`data:`) o allegate (`cid:`). WebView2 non risolve `cid:` da sé: la milestone Mail
/// dovrà riscrivere quegli indirizzi in `data:` (o servirli con un protocollo proprio).
/// Fuori dalla portata della CSP resta `<link rel="dns-prefetch">`: al più una risoluzione DNS.
pub const CSP: &str = "default-src 'none'; img-src data: cid:; style-src 'unsafe-inline'; font-src data:; \
form-action 'none'; frame-src 'none'; base-uri 'none'";

/// R1-24: la barra di scorrimento della pagina, scura come la cornice (cursore viola grigio sul
/// fondo della cornice, `Tema.fondo-basso` #0a0015), al posto di quella chiara del motore web. Tocca solo la barra: la
/// mail resta fedele al mittente (fondo, colori, caratteri), e una mail che sceglie la sua barra
/// vince (la regola è la prima del documento, senza `!important`). Niente `color-scheme`: farebbe
/// passare al tema scuro anche il testo delle mail che non hanno un fondo.
pub const STILE_BARRA: &str = "<style>:root{scrollbar-color:#5c5370 #0a0015}</style>";

/// La testa fidata del documento: la politica (`CSP`) come primo elemento e poi lo stile della
/// barra (`STILE_BARRA`). Va subito dopo un eventuale `<!doctype>` iniziale (così la pagina resta
/// in modalità standard), altrimenti in cima. Un `<head>` cercato nel testo si potrebbe
/// nascondere in un commento: la cima del documento no.
pub fn con_testa_fidata(html: &str) -> String {
    let meta = format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">{STILE_BARRA}"#);
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
    fn la_politica_non_lascia_uscire_nulla() {
        assert!(CSP.starts_with("default-src 'none';"));
        assert!(!CSP.contains("http"), "nessuna risorsa remota: {CSP}");
        assert!(CSP.contains("img-src data: cid:;"));
        assert!(CSP.contains("style-src 'unsafe-inline';"));
        assert!(CSP.contains("font-src data:;"));
    }

    #[test]
    fn la_politica_e_il_primo_elemento_del_documento() {
        let testa = format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">{STILE_BARRA}"#);
        assert_eq!(
            con_testa_fidata("<!DOCTYPE html><html><head><title>x</title>"),
            format!("<!DOCTYPE html>{testa}<html><head><title>x</title>")
        );
        assert_eq!(con_testa_fidata("\n  <!doctype html>\n<p>a</p>"), format!("\n  <!doctype html>{testa}\n<p>a</p>"));
        // Senza doctype (o con un commento davanti) va in cima: mai dentro un commento.
        assert_eq!(con_testa_fidata("<!-- <head> --><p>a</p>"), format!("{testa}<!-- <head> --><p>a</p>"));
        assert_eq!(con_testa_fidata(""), testa);
    }

    /// R1-24: la mail resta com'è (fondo, colori e caratteri del mittente); cambia solo la barra di
    /// scorrimento della pagina, scura come la cornice. Nessuna regola sul contenuto, nessuna
    /// risorsa esterna, e la politica resta il primo elemento (lo stile viene dopo).
    #[test]
    fn la_barra_di_scorrimento_e_scura_e_la_mail_no() {
        assert!(STILE_BARRA.starts_with("<style>:root{scrollbar-color:") && STILE_BARRA.ends_with("}</style>"));
        for vietato in ["background", "color-scheme", "body", "url(", "@import", "!important", "http"] {
            assert!(!STILE_BARRA.contains(vietato), "{vietato} in {STILE_BARRA}");
        }
        assert!(con_testa_fidata("<p>a</p>").starts_with("<meta http-equiv=\"Content-Security-Policy\""));
    }
}
