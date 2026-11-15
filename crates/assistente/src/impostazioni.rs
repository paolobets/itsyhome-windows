//! Impostazioni (finestra dedicata, `finestra_impostazioni`): logica pura degli ambienti di Home
//! Assistant (validazione del modulo, aggiunta/modifica/rimozione nella configurazione) e i testi
//! che la finestra mostra. Un token scritto nel modulo passa di qui solo per arrivare al
//! portachiavi (`Segreti`, nel `Ponte`): mai nella configurazione, mai nei log (il `Debug` di
//! `Modulo` e `Ambiente` lo nasconde).

use casa_core::config::Config;
use casa_core::eventi::StatoConnessione;
use casa_core::types::{HaEnvironment, TestConnectionResult};

/// Schemi accettati: quelli di Home Assistant (http/https) e i WebSocket diretti.
const SCHEMI: [&str; 4] = ["http://", "https://", "ws://", "wss://"];

/// L'URL di Home Assistant come lo scrive una persona: aggiunge `http://` se manca lo schema,
/// rifiuta il vuoto e gli spazi, toglie la `/` finale.
pub fn valida_url(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Scrivi l'indirizzo di Home Assistant".into());
    }
    if s.chars().any(char::is_whitespace) {
        return Err("L'indirizzo non può contenere spazi".into());
    }
    let completo = if s.contains("://") { s.to_owned() } else { format!("http://{s}") };
    let completo = completo.trim_end_matches('/').to_owned();
    let Some(schema) = SCHEMI.iter().find(|p| completo.to_ascii_lowercase().starts_with(*p)) else {
        return Err("L'indirizzo deve iniziare con http:// o https://".into());
    };
    if completo.len() == schema.len() {
        return Err("Manca il nome o l'indirizzo IP dopo lo schema".into());
    }
    Ok(completo)
}

/// Il modulo «aggiungi/modifica ambiente» come arriva dalla vista. `id` vuoto = nuovo ambiente;
/// `token` vuoto in modifica = lascia quello salvato.
#[derive(Clone, Default)]
pub struct Modulo {
    pub id: String,
    pub nome: String,
    pub url: String,
    pub token: String,
}

/// Il modulo controllato, pronto da applicare. `token` è `Some` solo se ne è stato scritto uno.
#[derive(Clone, PartialEq)]
pub struct Ambiente {
    pub id: String,
    pub nome: String,
    pub url: String,
    pub nuovo: bool,
    pub token: Option<String>,
}

/// Il token non compare mai in un `Debug` (log, messaggi d'errore dei test).
fn token_nascosto(t: &str) -> &'static str {
    if t.is_empty() { "" } else { "«nascosto»" }
}

impl std::fmt::Debug for Modulo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modulo")
            .field("id", &self.id)
            .field("nome", &self.nome)
            .field("url", &self.url)
            .field("token", &token_nascosto(&self.token))
            .finish()
    }
}

impl std::fmt::Debug for Ambiente {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ambiente")
            .field("id", &self.id)
            .field("nome", &self.nome)
            .field("url", &self.url)
            .field("nuovo", &self.nuovo)
            .field("token", &self.token.as_deref().map(token_nascosto))
            .finish()
    }
}

/// Nome predefinito di un ambiente lasciato senza nome.
pub const NOME_PREDEFINITO: &str = "Casa";

/// Controlla il modulo contro la configurazione attuale. Errori in frasi per la vista.
pub fn prepara(config: &Config, m: &Modulo) -> Result<Ambiente, String> {
    let url = valida_url(&m.url)?;
    let nome = match m.nome.trim() {
        "" => NOME_PREDEFINITO.to_owned(),
        n => n.to_owned(),
    };
    let token = Some(m.token.trim().to_owned()).filter(|t| !t.is_empty());
    if token.as_deref().is_some_and(|t| t.chars().any(char::is_whitespace)) {
        return Err("Il token non può contenere spazi".into());
    }
    if m.id.is_empty() {
        if token.is_none() {
            return Err("Scrivi il token di accesso di Home Assistant".into());
        }
        return Ok(Ambiente { id: nuovo_id(config, &nome), nome, url, nuovo: true, token });
    }
    if !config.environments.iter().any(|e| e.id == m.id) {
        return Err("Questo ambiente non esiste più".into());
    }
    Ok(Ambiente { id: m.id.clone(), nome, url, nuovo: false, token })
}

/// Un id stabile e leggibile dal nome («Casa al mare» → `casa-al-mare`), unico nella
/// configurazione (`-2`, `-3`… se serve). È anche il nome della voce nel portachiavi.
pub fn nuovo_id(config: &Config, nome: &str) -> String {
    let mut base: String = nome
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    base = base.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if base.is_empty() {
        base = "ambiente".into();
    }
    let esiste = |id: &str| config.environments.iter().any(|e| e.id == id);
    if !esiste(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !esiste(id)).expect("un id libero")
}

/// Scrive l'ambiente nella configurazione (nuovo in fondo, o sostituisce nome e URL).
/// Restituisce vero se l'URL è cambiato rispetto a prima (o l'ambiente è nuovo).
pub fn applica(config: &mut Config, a: &Ambiente) -> bool {
    match config.environments.iter_mut().find(|e| e.id == a.id) {
        Some(e) => {
            let cambiato = e.ha_url != a.url;
            e.name = a.nome.clone();
            e.ha_url = a.url.clone();
            cambiato
        }
        None => {
            config.environments.push(HaEnvironment { id: a.id.clone(), name: a.nome.clone(), ha_url: a.url.clone() });
            true
        }
    }
}

/// Toglie l'ambiente; se era quello attivo non c'è più un ambiente attivo. Restituisce vero se
/// era l'attivo.
pub fn togli(config: &mut Config, id: &str) -> bool {
    config.environments.retain(|e| e.id != id);
    let era_attivo = config.active_env_id.as_deref() == Some(id);
    if era_attivo {
        config.active_env_id = None;
    }
    era_attivo
}

/// Tono di uno stato nella vista: decide solo il colore del pallino (la parola c'è sempre).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tono {
    Ok,
    Attesa,
    Errore,
    Spento,
}

/// Lo stato di un ambiente nell'elenco: solo quello attivo ha una connessione.
pub fn stato_ambiente(attivo: bool, s: &StatoConnessione) -> (&'static str, Tono) {
    if !attivo {
        return ("Non attivo", Tono::Spento);
    }
    match s {
        StatoConnessione::Connesso => ("Connesso", Tono::Ok),
        StatoConnessione::InConnessione => ("Connessione…", Tono::Attesa),
        StatoConnessione::Disconnesso | StatoConnessione::NonConfigurato => ("Disconnesso", Tono::Spento),
        StatoConnessione::TokenNonValido => ("Token non valido", Tono::Errore),
        StatoConnessione::TokenMancante => ("Token mancante", Tono::Errore),
        StatoConnessione::Errore(e) if e.starts_with("portachiavi") => ("Token salvato non leggibile", Tono::Errore),
        StatoConnessione::Errore(_) => ("Non raggiungibile", Tono::Errore),
    }
}

/// L'ambiente attivo ha una connessione aperta o che ci sta provando (pulsante «Disconnetti»);
/// altrimenti il pulsante è «Connetti».
pub fn collegato(attivo: bool, s: &StatoConnessione) -> bool {
    attivo && matches!(s, StatoConnessione::Connesso | StatoConnessione::InConnessione | StatoConnessione::Errore(_))
}

/// L'esito di «Prova connessione» in una frase, col suo tono. Il dettaglio tecnico, se c'è, va
/// in Informazioni.
pub fn esito_prova(r: &TestConnectionResult) -> (String, Tono) {
    if r.ok {
        let testo = match r.entity_count {
            Some(1) => "Connessione riuscita · 1 entità".to_owned(),
            Some(n) => format!("Connessione riuscita · {n} entità"),
            None => "Connessione riuscita".to_owned(),
        };
        return (testo, Tono::Ok);
    }
    let testo = match r.error.as_deref() {
        Some("token non valido") => "Token non valido: Home Assistant l'ha rifiutato",
        _ => "Home Assistant non raggiungibile: controlla l'indirizzo",
    };
    (testo.into(), Tono::Errore)
}

/// Il dettaglio tecnico per la sezione Informazioni: ultimo errore di connessione, dell'import
/// dalla v2, del salvataggio della configurazione e dell'apertura di un collegamento. Vuoto se
/// non c'è nessun errore (la finestra lo dice allora in una frase, nel carattere normale).
pub fn dettagli_tecnici(
    connessione: Option<&str>,
    import: Option<&str>,
    config: Option<&str>,
    collegamento: Option<&str>,
) -> String {
    [
        ("Connessione", connessione),
        ("Import dalla versione precedente", import),
        ("Configurazione", config),
        ("Collegamento", collegamento),
    ]
        .into_iter()
        .filter_map(|(cosa, e)| e.map(|e| format!("{cosa}: {e}")))
        .collect::<Vec<String>>()
        .join("\n")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn url_senza_schema_diventa_http() {
        assert_eq!(valida_url("192.0.2.10:8123").as_deref(), Ok("http://192.0.2.10:8123"));
        assert_eq!(valida_url("  casa.local:8123 ").as_deref(), Ok("http://casa.local:8123"));
        // Uno schema già scritto resta com'è.
        assert_eq!(valida_url("https://ha.esempio.it").as_deref(), Ok("https://ha.esempio.it"));
        assert_eq!(valida_url("ws://127.0.0.1:18129").as_deref(), Ok("ws://127.0.0.1:18129"));
    }

    #[test]
    fn url_slash_finale_tolto() {
        assert_eq!(valida_url("http://192.0.2.10:8123/").as_deref(), Ok("http://192.0.2.10:8123"));
        assert_eq!(valida_url("192.0.2.10:8123//").as_deref(), Ok("http://192.0.2.10:8123"));
    }

    #[test]
    fn url_vuoto_o_con_spazi_rifiutato() {
        assert!(valida_url("").is_err());
        assert!(valida_url("   ").is_err());
        assert!(valida_url("http://").is_err());
        assert!(valida_url("192.0.2.10 :8123").is_err());
        assert!(valida_url("casa locale").is_err());
    }

    fn config_con(ids: &[&str], attivo: Option<&str>) -> Config {
        Config {
            environments: ids
                .iter()
                .map(|id| HaEnvironment { id: (*id).into(), name: format!("Nome {id}"), ha_url: "http://h:8123".into() })
                .collect(),
            active_env_id: attivo.map(str::to_owned),
            ..Config::default()
        }
    }

    fn modulo(id: &str, nome: &str, url: &str, token: &str) -> Modulo {
        Modulo { id: id.into(), nome: nome.into(), url: url.into(), token: token.into() }
    }

    #[test]
    fn nuovo_ambiente_vuole_il_token_e_ha_un_id_unico() {
        let c = config_con(&["casa"], Some("casa"));
        assert!(prepara(&c, &modulo("", "Casa", "h:8123", "")).is_err());
        let a = prepara(&c, &modulo("", "Casa", "h:8123/", " tok ")).unwrap();
        assert_eq!(a.id, "casa-2");
        assert!(a.nuovo);
        assert_eq!(a.url, "http://h:8123");
        assert_eq!(a.token.as_deref(), Some("tok"));
        assert_eq!(nuovo_id(&c, "Casa al Mare!"), "casa-al-mare");
        assert_eq!(nuovo_id(&c, "  "), "ambiente");
        // Senza nome: «Casa».
        assert_eq!(prepara(&c, &modulo("", " ", "h", "t")).unwrap().nome, NOME_PREDEFINITO);
    }

    #[test]
    fn modifica_senza_token_lascia_quello_salvato() {
        let c = config_con(&["casa"], Some("casa"));
        let a = prepara(&c, &modulo("casa", "Casa nuova", "h:8123", "")).unwrap();
        assert!(!a.nuovo);
        assert_eq!(a.token, None);
        let mut c2 = c.clone();
        assert!(!applica(&mut c2, &a), "stesso URL");
        assert_eq!(c2.environments[0].name, "Casa nuova");
        assert!(prepara(&c, &modulo("sparito", "x", "h", "")).is_err());
    }

    #[test]
    fn rimuovere_l_attivo_lascia_nessun_attivo() {
        let mut c = config_con(&["casa", "mare"], Some("casa"));
        assert!(!togli(&mut c, "mare"));
        assert_eq!(c.active_env_id.as_deref(), Some("casa"));
        assert!(togli(&mut c, "casa"));
        assert_eq!(c.active_env_id, None);
        assert!(c.environments.is_empty());
    }

    #[test]
    fn stato_dell_ambiente_e_pulsante() {
        assert_eq!(stato_ambiente(false, &StatoConnessione::Connesso), ("Non attivo", Tono::Spento));
        assert_eq!(stato_ambiente(true, &StatoConnessione::Connesso), ("Connesso", Tono::Ok));
        assert_eq!(stato_ambiente(true, &StatoConnessione::TokenNonValido).0, "Token non valido");
        assert_eq!(stato_ambiente(true, &StatoConnessione::Errore("x".into())).0, "Non raggiungibile");
        assert!(collegato(true, &StatoConnessione::Errore("x".into())), "riprova da solo: si può fermare");
        assert!(!collegato(true, &StatoConnessione::TokenNonValido));
        assert!(!collegato(false, &StatoConnessione::Connesso));
    }

    #[test]
    fn esito_della_prova_in_parole() {
        let ok = TestConnectionResult { ok: true, entity_count: Some(6), error: None };
        assert_eq!(esito_prova(&ok), ("Connessione riuscita · 6 entità".into(), Tono::Ok));
        let token = TestConnectionResult { ok: false, entity_count: None, error: Some("token non valido".into()) };
        assert!(esito_prova(&token).0.starts_with("Token non valido"));
        let giu = TestConnectionResult { ok: false, entity_count: None, error: Some("connessione non riuscita: IO".into()) };
        assert_eq!(esito_prova(&giu).1, Tono::Errore);
        assert!(!esito_prova(&giu).0.contains("IO"), "niente dettaglio tecnico nella frase");
    }

    #[test]
    fn il_debug_non_mostra_il_token() {
        let m = modulo("", "Casa", "h", "segreto-123");
        assert!(!format!("{m:?}").contains("segreto-123"));
        let a = prepara(&Config::default(), &m).unwrap();
        assert!(!format!("{a:?}").contains("segreto-123"));
        assert!(format!("{a:?}").contains("nascosto"));
    }

    #[test]
    fn dettagli_tecnici_righe() {
        assert_eq!(dettagli_tecnici(None, None, None, None), "", "nessun errore: la frase la scrive la finestra");
        assert_eq!(
            dettagli_tecnici(Some("14:02 · token rifiutato"), None, Some("disco pieno"), Some("«NAS» non aperto")),
            "Connessione: 14:02 · token rifiutato\nConfigurazione: disco pieno\nCollegamento: «NAS» non aperto"
        );
    }
}
