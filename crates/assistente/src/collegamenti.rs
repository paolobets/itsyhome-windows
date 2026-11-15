//! Collegamenti veloci (Task 14f): pagine web delle gestioni, salvate nella configurazione
//! (`Config::collegamenti`) e aperte nel browser predefinito del sistema, mai nella finestra di
//! gestione (lì JavaScript è spento e le pagine di amministrazione non funzionerebbero).
//!
//! Qui la logica pura: controllo del nome e dell'indirizzo, aggiunta, modifica, eliminazione e
//! ordine. Chi apre (`sistema::browser`) ricontrolla l'indirizzo: il file si può scrivere a mano.

use casa_core::config::{Collegamento, Config};
use gestione_protocollo::{carattere_invisibile, titolo_pulito};

/// Lunghezza massima del nome, in caratteri.
pub const LUNGHEZZA_NOME: usize = 60;

/// Il nome come lo mostra il pannello: senza caratteri di controllo né invisibili (direzione,
/// larghezza zero), spazi compressi; obbligatorio, al più `LUNGHEZZA_NOME` caratteri.
pub fn valida_nome(s: &str) -> Result<String, String> {
    let nome = titolo_pulito(s);
    let quanti = nome.chars().count();
    if quanti == 0 {
        return Err("Scrivi un nome".into());
    }
    if quanti > LUNGHEZZA_NOME {
        return Err(format!("Al massimo {LUNGHEZZA_NOME} caratteri (ora {quanti})"));
    }
    Ok(nome)
}

/// Caratteri mai ammessi in un indirizzo: un browser li codificherebbe, ma qui potrebbero uscire
/// dall'argomento con cui il sistema passa l'indirizzo al browser (le virgolette soprattutto).
const VIETATI: [char; 9] = ['"', '<', '>', '\\', '^', '`', '{', '}', '|'];

/// Lo schema di un URL secondo la RFC 3986: una lettera, poi lettere, cifre, `+`, `-`, `.`.
fn sembra_schema(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|p| p.is_ascii_alphabetic())
        && c.all(|x| x.is_ascii_alphanumeric() || matches!(x, '+' | '-' | '.'))
}

/// Separa lo schema dal resto. Senza schema (`esempio.lan:8006/x`) si intende `https`; un
/// `nome:` seguito da altro che non sia una porta (`file:`, `javascript:`, `data:`, `mailto:`) è
/// uno schema.
fn schema_e_resto(s: &str) -> (String, &str) {
    if let Some(i) = s.find("://") {
        if sembra_schema(&s[..i]) {
            return (s[..i].to_ascii_lowercase(), &s[i + 3..]);
        }
    }
    if let Some(i) = s.find(':') {
        let (prima, dopo) = (&s[..i], &s[i + 1..]);
        let fine = dopo.find(['/', '?', '#']).unwrap_or(dopo.len());
        let porta = !dopo[..fine].is_empty() && dopo[..fine].chars().all(|c| c.is_ascii_digit());
        if sembra_schema(prima) && !porta {
            return (prima.to_ascii_lowercase(), dopo);
        }
    }
    ("https".into(), s)
}

/// L'indirizzo come lo scrive una persona, controllato e normalizzato: solo `http://` e
/// `https://` (senza schema si aggiunge `https://`), niente spazi, caratteri di controllo,
/// invisibili o vietati, niente credenziali (`utente:password@`), un host e, se c'è, una porta
/// valida. Schema e host in minuscolo; porta, percorso, query e frammento restano come sono. Un
/// nome internazionale si accetta così com'è (nessuna conversione in punycode).
pub fn valida_indirizzo(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Scrivi l'indirizzo della pagina".into());
    }
    if s.chars().any(char::is_whitespace) {
        return Err("L'indirizzo non può contenere spazi".into());
    }
    if s.chars().any(|c| c.is_control() || carattere_invisibile(c) || VIETATI.contains(&c)) {
        return Err("L'indirizzo contiene caratteri non ammessi".into());
    }
    let (schema, resto) = schema_e_resto(s);
    if schema != "http" && schema != "https" {
        return Err("Solo indirizzi http:// o https://".into());
    }
    let fine = resto.find(['/', '?', '#']).unwrap_or(resto.len());
    let (autorita, coda) = resto.split_at(fine);
    if autorita.contains('@') {
        return Err("Niente nome utente e password nell'indirizzo: inseriscili nella pagina".into());
    }
    let (host, porta) = if let Some(dentro) = autorita.strip_prefix('[') {
        // IPv6 fra parentesi quadre: «[fd00::1]:8006».
        let Some(chiusa) = dentro.find(']') else { return Err("Nome del sito non valido".into()) };
        let ip = &dentro[..chiusa];
        if ip.is_empty() || !ip.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
            return Err("Nome del sito non valido".into());
        }
        let dopo = &dentro[chiusa + 1..];
        let porta = match dopo.strip_prefix(':') {
            Some(p) => Some(p),
            None if dopo.is_empty() => None,
            None => return Err("Nome del sito non valido".into()),
        };
        (format!("[{}]", ip.to_ascii_lowercase()), porta)
    } else {
        let (host, porta) = match autorita.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (autorita, None),
        };
        if host.is_empty() {
            return Err("Manca il nome o l'indirizzo IP del sito".into());
        }
        if !host.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '_')) {
            return Err("Nome del sito non valido".into());
        }
        (host.to_ascii_lowercase(), porta)
    };
    let porta = match porta {
        None => String::new(),
        Some(p) => match p.parse::<u16>() {
            Ok(n) if n > 0 && p.chars().all(|c| c.is_ascii_digit()) => format!(":{p}"),
            _ => return Err("Porta non valida (da 1 a 65535)".into()),
        },
    };
    Ok(format!("{schema}://{host}{porta}{coda}"))
}

/// L'indirizzo accorciato per il pannello: host, porta e percorso, senza schema, query e
/// frammento (e senza la `/` finale). I puntini, se non ci sta, li mette l'interfaccia.
pub fn indirizzo_corto(url: &str) -> String {
    let resto = url.split_once("://").map_or(url, |(_, r)| r);
    let fine = resto.find(['?', '#']).unwrap_or(resto.len());
    resto[..fine].trim_end_matches('/').to_owned()
}

/// Un id stabile e leggibile dal nome («Proxmox VE» → `proxmox-ve`), unico fra i collegamenti
/// (`-2`, `-3`… se serve). Non cambia se poi il nome cambia.
pub fn nuovo_id(config: &Config, nome: &str) -> String {
    let base: String = nome
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut base = base.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if base.is_empty() {
        base = "collegamento".into();
    }
    let esiste = |id: &str| config.collegamenti.iter().any(|c| c.id == id);
    if !esiste(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !esiste(id)).expect("un id libero")
}

/// Aggiunge (`id` vuoto: in fondo) o modifica un collegamento, dopo aver controllato nome e
/// indirizzo. Restituisce l'id, o l'errore da mostrare.
pub fn salva(config: &mut Config, id: &str, nome: &str, url: &str) -> Result<String, String> {
    let nome = valida_nome(nome)?;
    let url = valida_indirizzo(url)?;
    if id.is_empty() {
        let id = nuovo_id(config, &nome);
        config.collegamenti.push(Collegamento { id: id.clone(), nome, url });
        return Ok(id);
    }
    let Some(c) = config.collegamenti.iter_mut().find(|c| c.id == id) else {
        return Err("Questo collegamento non esiste più".into());
    };
    c.nome = nome;
    c.url = url;
    Ok(id.to_owned())
}

/// Toglie il collegamento; falso se non c'era.
pub fn elimina(config: &mut Config, id: &str) -> bool {
    let prima = config.collegamenti.len();
    config.collegamenti.retain(|c| c.id != id);
    config.collegamenti.len() != prima
}

/// Sposta il collegamento di un posto (`verso` -1 su, +1 giù); falso se è già al bordo.
pub fn sposta(config: &mut Config, id: &str, verso: i32) -> bool {
    if verso == 0 {
        return false;
    }
    let Some(i) = config.collegamenti.iter().position(|c| c.id == id) else { return false };
    let Some(j) = i.checked_add_signed(verso.signum() as isize).filter(|j| *j < config.collegamenti.len()) else {
        return false;
    };
    config.collegamenti.swap(i, j);
    true
}

/// L'indirizzo da aprire per `id`, ricontrollato (il file si può scrivere a mano): `None` se il
/// collegamento non c'è, l'errore se l'indirizzo salvato non è ammesso.
pub fn da_aprire(config: &Config, id: &str) -> Option<Result<String, String>> {
    let c = config.collegamenti.iter().find(|c| c.id == id)?;
    Some(valida_indirizzo(&c.url))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn indirizzi_buoni_e_normalizzati() {
        let casi = [
            ("https://esempio.lan:8006", "https://esempio.lan:8006"),
            ("  http://esempio.lan/  ", "http://esempio.lan/"),
            // Senza schema: https.
            ("esempio.lan:8006", "https://esempio.lan:8006"),
            ("pve.esempio.lan", "https://pve.esempio.lan"),
            ("192.0.2.10:8080/admin", "https://192.0.2.10:8080/admin"),
            // Schema e host in minuscolo; porta, percorso, query e frammento come sono.
            ("HTTPS://Esempio.LAN:8006/Pagina?Chiave=Valore#Parte", "https://esempio.lan:8006/Pagina?Chiave=Valore#Parte"),
            ("HtTp://ESEMPIO.lan", "http://esempio.lan"),
            ("[FD00::1]:8443/ui", "https://[fd00::1]:8443/ui"),
            ("http://[::1]", "http://[::1]"),
            ("esempio.lan?u=http://altro", "https://esempio.lan?u=http://altro"),
            ("localhost:3000", "https://localhost:3000"),
            // Nome internazionale: accettato così com'è.
            ("https://città.esempio", "https://città.esempio"),
            ("nas_casa.lan", "https://nas_casa.lan"),
        ];
        for (scritto, atteso) in casi {
            assert_eq!(valida_indirizzo(scritto).as_deref(), Ok(atteso), "{scritto:?}");
        }
    }

    #[test]
    fn indirizzi_cattivi_rifiutati() {
        let casi = [
            "",
            "   ",
            "file:///C:/Windows/win.ini",
            "file:C:/x",
            "javascript:alert(1)",
            "JavaScript://%0aalert(1)",
            "data:text/html,<b>x</b>",
            "data:text/html;base64,PGI+",
            "mailto:qualcuno@esempio.lan",
            "ftp://esempio.lan",
            "ms-settings:display",
            "vbscript:msgbox",
            "https://esempio .lan",
            "https://esempio.lan/a b",
            "https://esempio.lan/\u{0007}",
            "https://esem\u{202E}pio.lan",
            "https://esempio\u{200B}.lan",
            "https://esempio.lan/\"--x",
            "https://esempio.lan/<script>",
            "\\\\server\\share",
            "https://",
            "https:///percorso",
            "//esempio.lan",
            "https://:8006",
            "https://esempio.lan:0",
            "https://esempio.lan:65536",
            "https://esempio.lan:abc",
            "https://esempio.lan:",
            "https://[fd00::1",
            "https://[zz]:80",
            "https://es%65mpio.lan",
        ];
        for scritto in casi {
            assert!(valida_indirizzo(scritto).is_err(), "{scritto:?} accettato: {:?}", valida_indirizzo(scritto));
        }
    }

    #[test]
    fn messaggi_chiari() {
        assert_eq!(valida_indirizzo("").unwrap_err(), "Scrivi l'indirizzo della pagina");
        assert_eq!(valida_indirizzo("a b").unwrap_err(), "L'indirizzo non può contenere spazi");
        assert_eq!(valida_indirizzo("javascript:alert(1)").unwrap_err(), "Solo indirizzi http:// o https://");
        assert_eq!(valida_indirizzo("file:///C:/x").unwrap_err(), "Solo indirizzi http:// o https://");
        // Credenziali: rifiutate con un messaggio che dice perché.
        for scritto in ["https://utente:segreto@esempio.lan:8006", "utente@esempio.lan", "http://:p@esempio.lan"] {
            assert!(valida_indirizzo(scritto).unwrap_err().contains("nome utente e password"), "{scritto:?}");
        }
        assert!(valida_indirizzo("https://esempio.lan:70000").unwrap_err().contains("Porta"));
        assert!(valida_indirizzo("https://esem\u{202E}pio.lan").unwrap_err().contains("non ammessi"));
    }

    #[test]
    fn nome_ripulito_e_limitato() {
        assert_eq!(valida_nome("  Proxmox   VE ").as_deref(), Ok("Proxmox VE"));
        // Direzione e larghezza zero spariscono: il nome non si mostra diverso da com'è.
        assert_eq!(valida_nome("NAS\u{202E}xob\u{200B}").as_deref(), Ok("NASxob"));
        assert_eq!(valida_nome("Riga\nnuova").as_deref(), Ok("Riga nuova"));
        assert_eq!(valida_nome("").unwrap_err(), "Scrivi un nome");
        assert_eq!(valida_nome(" \u{202E}\t").unwrap_err(), "Scrivi un nome");
        let sessanta = "è".repeat(LUNGHEZZA_NOME);
        assert_eq!(valida_nome(&sessanta).as_deref(), Ok(sessanta.as_str()), "60 caratteri (non byte): va bene");
        assert_eq!(valida_nome(&"x".repeat(61)).unwrap_err(), "Al massimo 60 caratteri (ora 61)");
    }

    #[test]
    fn indirizzo_accorciato_per_il_pannello() {
        assert_eq!(indirizzo_corto("https://esempio.lan:8006"), "esempio.lan:8006");
        assert_eq!(indirizzo_corto("https://esempio.lan:8006/"), "esempio.lan:8006");
        assert_eq!(indirizzo_corto("http://nas.esempio.lan/admin/?x=1#y"), "nas.esempio.lan/admin");
        assert_eq!(indirizzo_corto("https://esempio.lan#parte"), "esempio.lan");
    }

    fn con(collegamenti: &[(&str, &str)]) -> Config {
        Config {
            collegamenti: collegamenti
                .iter()
                .map(|(id, nome)| Collegamento { id: (*id).into(), nome: (*nome).into(), url: format!("https://{id}.esempio.lan") })
                .collect(),
            ..Config::default()
        }
    }

    fn ids(c: &Config) -> Vec<&str> {
        c.collegamenti.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn aggiunta_modifica_eliminazione() {
        let mut c = Config::default();
        assert_eq!(salva(&mut c, "", "Proxmox", "esempio.lan:8006"), Ok("proxmox".into()));
        assert_eq!(salva(&mut c, "", "Proxmox", "https://altro.esempio.lan"), Ok("proxmox-2".into()), "id unico");
        assert_eq!(salva(&mut c, "", "  ", "x.lan"), Err("Scrivi un nome".into()));
        assert_eq!(salva(&mut c, "", "Male", "file:///x"), Err("Solo indirizzi http:// o https://".into()));
        assert_eq!(ids(&c), ["proxmox", "proxmox-2"], "gli errori non salvano nulla");
        assert_eq!(c.collegamenti[0].url, "https://esempio.lan:8006", "salvato normalizzato");
        // Modifica: stesso id e stesso posto, nome e indirizzo nuovi.
        assert_eq!(salva(&mut c, "proxmox-2", "Router", "HTTP://Router.esempio.lan/"), Ok("proxmox-2".into()));
        assert_eq!(c.collegamenti[1], Collegamento { id: "proxmox-2".into(), nome: "Router".into(), url: "http://router.esempio.lan/".into() });
        assert_eq!(salva(&mut c, "sparito", "X", "x.lan"), Err("Questo collegamento non esiste più".into()));
        assert!(elimina(&mut c, "proxmox"));
        assert!(!elimina(&mut c, "proxmox"));
        assert_eq!(ids(&c), ["proxmox-2"]);
        assert_eq!(nuovo_id(&c, "Città d'arte!"), "citt-d-arte");
        assert_eq!(nuovo_id(&c, "∞"), "collegamento");
    }

    #[test]
    fn ordine_su_e_giu() {
        let mut c = con(&[("a", "A"), ("b", "B"), ("c", "C")]);
        assert!(sposta(&mut c, "c", -1));
        assert_eq!(ids(&c), ["a", "c", "b"]);
        assert!(sposta(&mut c, "a", 1));
        assert_eq!(ids(&c), ["c", "a", "b"]);
        assert!(!sposta(&mut c, "c", -1), "già in cima");
        assert!(!sposta(&mut c, "b", 1), "già in fondo");
        assert!(!sposta(&mut c, "x", 1));
        assert_eq!(ids(&c), ["c", "a", "b"]);
    }

    #[test]
    fn da_aprire_ricontrolla_l_indirizzo_salvato() {
        let mut c = con(&[("a", "A")]);
        assert_eq!(da_aprire(&c, "a"), Some(Ok("https://a.esempio.lan".into())));
        assert_eq!(da_aprire(&c, "x"), None);
        // Scritto a mano nel file: non si apre.
        c.collegamenti[0].url = "file:///C:/Windows/System32/calc.exe".into();
        assert!(matches!(da_aprire(&c, "a"), Some(Err(_))));
    }
}
