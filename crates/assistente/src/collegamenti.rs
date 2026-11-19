//! Collegamenti veloci (Task 14f): pagine web delle gestioni, salvate nella configurazione
//! (`Config::collegamenti`) e aperte nel browser predefinito del sistema, mai nella finestra di
//! gestione (lì JavaScript è spento e le pagine di amministrazione non funzionerebbero).
//!
//! Qui la logica pura: controllo del nome e dell'indirizzo, aggiunta, modifica, eliminazione e
//! ordine. Chi apre (`sistema::browser`) ricontrolla l'indirizzo: il file si può scrivere a mano.

use casa_core::config::{Collegamento, Config};
use gestione_protocollo::{carattere_invisibile, testo_pulito};

/// Lunghezza massima del nome, in caratteri.
pub const LUNGHEZZA_NOME: usize = 60;

/// Il nome come lo mostra il pannello: senza caratteri di controllo né invisibili (direzione,
/// larghezza zero), spazi compressi; obbligatorio, al più `LUNGHEZZA_NOME` caratteri.
pub fn valida_nome(s: &str) -> Result<String, String> {
    let nome = testo_pulito(s);
    let quanti = nome.chars().count();
    if quanti == 0 {
        return Err("Scrivi un nome".into());
    }
    if quanti > LUNGHEZZA_NOME {
        return Err(format!("Al massimo {LUNGHEZZA_NOME} caratteri (ora {quanti})"));
    }
    Ok(nome)
}

/// Quanti colori ha la tavolozza dei cerchi (`Tema.colore-collegamento` in tema.slint; un test
/// controlla che i due numeri coincidano).
pub const TONI: usize = 8;

/// L'iniziale del nome, maiuscola, per il cerchio del collegamento: il primo carattere che sia
/// una lettera o una cifra («#» se il nome non ne ha).
pub fn iniziale(nome: &str) -> String {
    nome.chars()
        .find(|c| c.is_alphanumeric())
        .and_then(|c| c.to_uppercase().next())
        .map_or_else(|| "#".into(), String::from)
}

/// Il colore del cerchio, dal solo nome (senza maiuscole né spazi intorno): sempre lo stesso per lo
/// stesso nome, a ogni avvio e in ogni posizione dell'elenco. FNV-1a, scritto qui perché
/// `DefaultHasher` non promette lo stesso risultato fra una versione di Rust e l'altra.
pub fn tono(nome: &str) -> i32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in nome.trim().to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    // `TONI` è piccolo: il resto sta in un `i32`.
    (h % TONI as u64) as i32
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

/// Lunghezza massima di un indirizzo, in caratteri (`INTERNET_MAX_URL_LENGTH` di Windows: oltre,
/// alcuni percorsi di `ShellExecuteW` falliscono o troncano).
pub const LUNGHEZZA_INDIRIZZO: usize = 2048;

/// Un nome di host ASCII fatto di etichette vere: non vuote, al più 63 caratteri, senza `-` in
/// testa o in coda (`https://...` e `https://-` non sono siti).
fn host_ben_fatto(host: &str) -> bool {
    host.split('.').all(|e| !e.is_empty() && e.len() <= 63 && !e.starts_with('-') && !e.ends_with('-'))
}

/// L'indirizzo come lo scrive una persona, controllato e normalizzato: solo `http://` e
/// `https://` (senza schema si aggiunge `https://`), al più `LUNGHEZZA_INDIRIZZO` caratteri,
/// niente spazi, caratteri di controllo, invisibili o vietati, `%` solo seguito da due cifre
/// esadecimali, niente credenziali (`utente:password@`), un host e, se c'è, una porta valida.
///
/// Il risultato è la serializzazione di `url::Url` (le stesse regole WHATWG del browser: host in
/// minuscolo, `/` dopo l'host, porta predefinita tolta, caratteri speciali codificati): ciò che
/// si salva, si mostra e si apre è una stringa sola, senza una seconda interpretazione nel
/// browser. Per lo stesso motivo il nome del sito deve essere ASCII: con lettere a larghezza
/// piena, `²` od omografi cirillici il browser andrebbe (via IDNA) su un sito diverso da quello
/// che il pannello mostra. Un nome internazionale si scrive nella forma `xn--`.
pub fn valida_indirizzo(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Scrivi l'indirizzo della pagina".into());
    }
    if s.chars().count() > LUNGHEZZA_INDIRIZZO {
        return Err(format!("Indirizzo troppo lungo: al massimo {LUNGHEZZA_INDIRIZZO} caratteri"));
    }
    if s.chars().any(char::is_whitespace) {
        return Err("L'indirizzo non può contenere spazi".into());
    }
    if s.chars().any(|c| c.is_control() || carattere_invisibile(c) || VIETATI.contains(&c)) {
        return Err("L'indirizzo contiene caratteri non ammessi".into());
    }
    let b = s.as_bytes();
    if (0..b.len()).any(|i| b[i] == b'%' && !(i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit())) {
        return Err("Un «%» nell'indirizzo va seguito da due cifre esadecimali".into());
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
    if !autorita.is_ascii() {
        return Err("Scrivi il nome del sito con lettere semplici (un nome internazionale nella forma xn--)".into());
    }
    let ipv6 = autorita.starts_with('[');
    let (host, porta) = match autorita.rsplit_once(':') {
        // In un IPv6 i «:» stanno dentro le parentesi: la porta, se c'è, viene dopo «]».
        Some((h, p)) if !ipv6 || h.ends_with(']') => (h, Some(p)),
        _ => (autorita, None),
    };
    if host.is_empty() {
        return Err("Manca il nome o l'indirizzo IP del sito".into());
    }
    if let Some(p) = porta {
        if !(p.chars().all(|c| c.is_ascii_digit()) && p.parse::<u16>().is_ok_and(|n| n > 0)) {
            return Err("Porta non valida (da 1 a 65535)".into());
        }
    }
    let ip_valido = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .is_some_and(|h| h.parse::<std::net::Ipv6Addr>().is_ok());
    if ipv6 && !ip_valido {
        return Err("Nome del sito non valido".into());
    }
    let nome_valido = host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_')) && host_ben_fatto(host);
    if !ipv6 && !nome_valido {
        return Err("Nome del sito non valido".into());
    }
    let url = url::Url::parse(&format!("{schema}://{autorita}{coda}")).map_err(|_| "Indirizzo non valido".to_owned())?;
    // Doppio controllo su ciò che il browser vedrà davvero.
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) || !url.username().is_empty() || url.password().is_some() {
        return Err("Indirizzo non valido".into());
    }
    let finale = url.as_str();
    if !finale.is_ascii() || finale.chars().any(|c| VIETATI.contains(&c)) {
        return Err("L'indirizzo contiene caratteri non ammessi".into());
    }
    if finale.len() > LUNGHEZZA_INDIRIZZO {
        return Err(format!("Indirizzo troppo lungo: al massimo {LUNGHEZZA_INDIRIZZO} caratteri"));
    }
    Ok(finale.to_owned())
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

/// Toglie il collegamento (uno solo, il primo con quell'id); falso se non c'era.
pub fn elimina(config: &mut Config, id: &str) -> bool {
    let Some(i) = config.collegamenti.iter().position(|c| c.id == id) else { return false };
    config.collegamenti.remove(i);
    true
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

/// Ripara una configurazione scritta a mano: nomi ripuliti come nel modulo (controlli e
/// caratteri invisibili via, al più `LUNGHEZZA_NOME` caratteri; vuoto → «Collegamento»), id
/// vuoti o doppi rigenerati dal nome. Vero se ha cambiato qualcosa (allora va salvata). Gli
/// indirizzi non si toccano: si ricontrollano a ogni apertura (`da_aprire`).
pub fn ripara(config: &mut Config) -> bool {
    let mut cambiato = false;
    let mut visti: Vec<String> = Vec::new();
    let mut voci = std::mem::take(&mut config.collegamenti);
    // Un id rigenerato non deve prendere quello di un elemento che viene dopo.
    let originali: Vec<String> = voci.iter().map(|c| c.id.clone()).filter(|id| !id.is_empty()).collect();
    for c in &mut voci {
        let mut nome: String = testo_pulito(&c.nome).chars().take(LUNGHEZZA_NOME).collect();
        if nome.is_empty() {
            nome = "Collegamento".into();
        }
        if nome != c.nome {
            c.nome = nome;
            cambiato = true;
        }
        if c.id.is_empty() || visti.contains(&c.id) {
            let occupati: Vec<String> = visti.iter().chain(&originali).cloned().collect();
            let fatti = Config { collegamenti: voci_con_id(&occupati), ..Config::default() };
            c.id = nuovo_id(&fatti, &c.nome);
            cambiato = true;
        }
        visti.push(c.id.clone());
    }
    config.collegamenti = voci;
    cambiato
}

/// Collegamenti finti con questi id (per `nuovo_id` durante la riparazione).
fn voci_con_id(ids: &[String]) -> Vec<Collegamento> {
    ids.iter().map(|id| Collegamento { id: id.clone(), ..Collegamento::default() }).collect()
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
    fn iniziale_e_colore_dal_nome() {
        assert_eq!(iniziale("Proxmox"), "P");
        assert_eq!(iniziale("  nas di casa"), "N");
        assert_eq!(iniziale("«router»"), "R");
        assert_eq!(iniziale("3CX"), "3");
        assert_eq!(iniziale("època"), "È");
        assert_eq!(iniziale("···"), "#");
        // Stesso nome, stesso colore: anche con altre maiuscole o spazi intorno (il colore non
        // cambia riavviando, riordinando o rinominando di una maiuscola).
        assert_eq!(tono("Proxmox"), tono("Proxmox"));
        assert_eq!(tono("Proxmox"), tono("  proxmox "));
        // Sempre dentro la tavolozza.
        for nome in ["", "A", "NAS", "Stampante", "Router", "Proxmox", "Nome molto lungo con «virgolette»"] {
            assert!((0..TONI as i32).contains(&tono(nome)), "{nome}");
        }
        // Nomi diversi non finiscono tutti dello stesso colore.
        let diversi: std::collections::HashSet<i32> =
            ["Proxmox", "NAS", "Router", "Stampante", "Telecamere", "Domotica", "Backup"].iter().map(|n| tono(n)).collect();
        assert!(diversi.len() >= 4, "{diversi:?}");
    }

    #[test]
    fn indirizzi_buoni_e_normalizzati() {
        let casi = [
            // La serializzazione di `url::Url`: `/` dopo l'host, come la vede il browser.
            ("https://esempio.lan:8006", "https://esempio.lan:8006/"),
            ("  http://esempio.lan/  ", "http://esempio.lan/"),
            // Senza schema: https (anche per un IP locale, come chiede il brief).
            ("esempio.lan:8006", "https://esempio.lan:8006/"),
            ("pve.esempio.lan", "https://pve.esempio.lan/"),
            ("192.0.2.10:8080/admin", "https://192.0.2.10:8080/admin"),
            ("127.0.0.1:18171/prova", "https://127.0.0.1:18171/prova"),
            // Schema e host in minuscolo; porta, percorso, query e frammento come sono.
            ("HTTPS://Esempio.LAN:8006/Pagina?Chiave=Valore#Parte", "https://esempio.lan:8006/Pagina?Chiave=Valore#Parte"),
            ("HtTp://ESEMPIO.lan", "http://esempio.lan/"),
            ("[FD00::1]:8443/ui", "https://[fd00::1]:8443/ui"),
            ("http://[::1]", "http://[::1]/"),
            ("esempio.lan?u=http://altro", "https://esempio.lan/?u=http://altro"),
            ("localhost:3000", "https://localhost:3000/"),
            // Nome internazionale scritto nella forma xn--: resta quella.
            ("https://xn--citt-6qa.esempio", "https://xn--citt-6qa.esempio/"),
            ("nas_casa.lan", "https://nas_casa.lan/"),
            // Porta predefinita tolta, percent-encoding valido tenuto, lettere accentate del
            // percorso codificate come fa il browser.
            ("https://esempio.lan:443/x", "https://esempio.lan/x"),
            ("https://esempio.lan/a%20b", "https://esempio.lan/a%20b"),
            ("https://esempio.lan/città", "https://esempio.lan/citt%C3%A0"),
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
            // Host non ASCII: il browser li riscriverebbe (IDNA) in un sito diverso da quello mostrato.
            "https://ｅｓｅｍｐｉｏ.lan",
            "https://²².lan",
            "https://еxample.lan",
            "https://città.esempio",
            "https://CITTÀ.esempio",
            // IPv6 e nomi fatti solo di caratteri ammessi ma non validi.
            "https://[1]",
            "https://[:]",
            "https://[fd00::1]x",
            "https://...",
            "https://-",
            "https://-esempio.lan",
            "https://esempio-.lan",
            "https://esempio..lan",
            // Percent-encoding rotto.
            "https://esempio.lan/%zz",
            "https://esempio.lan/%4",
            "https://esempio.lan/%",
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
        assert!(valida_indirizzo("https://еxample.lan").unwrap_err().contains("xn--"), "omografo cirillico");
        assert!(valida_indirizzo("https://esempio.lan/%zz").unwrap_err().contains("%"));
    }

    #[test]
    fn indirizzo_al_limite_di_lunghezza() {
        let base = "https://esempio.lan/";
        let giusto = format!("{base}{}", "a".repeat(LUNGHEZZA_INDIRIZZO - base.len()));
        assert_eq!(valida_indirizzo(&giusto).as_deref(), Ok(giusto.as_str()), "2048: va bene");
        let lungo = format!("{giusto}a");
        assert_eq!(valida_indirizzo(&lungo).unwrap_err(), "Indirizzo troppo lungo: al massimo 2048 caratteri");
        // Anche se si allunga solo dopo la codifica (ogni «è» diventa sei caratteri).
        let codificato = format!("{base}{}", "è".repeat(400));
        assert!(valida_indirizzo(&codificato).unwrap_err().contains("troppo lungo"));
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
        // Contati prima di qualunque taglio (prima diceva «ora 200»).
        assert_eq!(valida_nome(&"x".repeat(250)).unwrap_err(), "Al massimo 60 caratteri (ora 250)");
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
        assert_eq!(c.collegamenti[0].url, "https://esempio.lan:8006/", "salvato normalizzato");
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

    /// Giro di rifinitura: un file scritto a mano con id vuoti o doppi e nomi sporchi si ripara
    /// alla lettura; `elimina` toglie un elemento solo.
    #[test]
    fn configurazione_scritta_a_mano_riparata() {
        let mut c = con(&[("a", "A"), ("a", "Doppio"), ("", "Senza id")]);
        c.collegamenti[1].nome = "Dop\u{202E}pio\n riga".into();
        assert!(ripara(&mut c));
        assert_eq!(ids(&c), ["a", "doppio-riga", "senza-id"]);
        assert_eq!(c.collegamenti[1].nome, "Doppio riga");
        assert!(!ripara(&mut c), "già a posto: nulla da salvare");
        let mut d = con(&[("x", "X"), ("y", "Y")]);
        d.collegamenti[1].id = "x".into();
        assert!(elimina(&mut d, "x"));
        assert_eq!(d.collegamenti.len(), 1, "solo il primo con quell'id");
    }

    #[test]
    fn da_aprire_ricontrolla_l_indirizzo_salvato() {
        let mut c = con(&[("a", "A")]);
        assert_eq!(da_aprire(&c, "a"), Some(Ok("https://a.esempio.lan/".into())));
        assert_eq!(da_aprire(&c, "x"), None);
        // Scritto a mano nel file: non si apre.
        c.collegamenti[0].url = "file:///C:/Windows/System32/calc.exe".into();
        assert!(matches!(da_aprire(&c, "a"), Some(Err(_))));
    }
}
