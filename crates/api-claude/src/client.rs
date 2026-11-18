//! Il client HTTP: UN solo metodo di rete, `leggi`, e sempre `GET` su un percorso dell'elenco
//! chiuso (`letture::PERCORSI_AMMESSI`). Per costruzione non c'è modo di mandare altro, e il
//! test `sorgente_senza_scritture` (in `lib.rs`) controlla che in questo file non compaia mai
//! una chiamata che scrive.
//!
//! La chiave parte solo verso `https://api.anthropic.com` (HTTPS obbligatorio, certificato
//! verificato con le radici del sistema, nessun reindirizzamento seguito, nessun proxy
//! dall'ambiente). Il client si crea per una lettura e si lascia andare subito dopo.

use crate::chiave::ChiaveAdmin;
use crate::letture::{Lettura, PERCORSI_AMMESSI};
use std::sync::Arc;
use std::time::Duration;

/// L'unico indirizzo vero.
const BASE_ANTHROPIC: &str = "https://api.anthropic.com";
/// Versione dell'API (negli esempi della documentazione del 18/11/2026).
const VERSIONE_API: &str = "2026-10-21";
/// Intestazione beta delle letture analitiche (documentazione: obbligatoria su `/analytics/*`).
const BETA_ANALITICA: &str = "analytics-usage-cost-2026-08-24";
const USER_AGENT: &str = "ItsyHome-Assistente";
const TIMEOUT: Duration = Duration::from_secs(20);
/// Una risposta più grande di così non è una risposta delle API: si scarta.
const MASSIMO_CORPO: u64 = 8 * 1024 * 1024;

/// Perché una lettura non è riuscita. Le frasi non contengono mai la chiave né il corpo della risposta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreApi {
    ChiaveNonValida,
    PermessoNegato,
    TroppeRichieste,
    Rete,
    Servizio(u16),
    Formato,
}

impl std::fmt::Display for ErroreApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroreApi::ChiaveNonValida => write!(f, "La chiave non è valida o è scaduta."),
            ErroreApi::PermessoNegato => {
                write!(f, "La chiave non ha il permesso per questa lettura.")
            }
            ErroreApi::TroppeRichieste => write!(f, "Troppe richieste: riprovo più tardi."),
            ErroreApi::Rete => write!(f, "Non riesco a raggiungere le API di Anthropic."),
            ErroreApi::Servizio(codice) => {
                write!(f, "Le API hanno risposto con un errore ({codice}).")
            }
            ErroreApi::Formato => write!(f, "La risposta delle API non è nel formato atteso."),
        }
    }
}

impl std::error::Error for ErroreApi {}

/// Il valore dell'intestazione `x-api-key`: mai stampabile.
struct Credenziale(String);

pub struct ClientApi {
    agente: ureq::Agent,
    base: String,
    credenziale: Credenziale,
}

impl std::fmt::Debug for ClientApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientApi")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

/// Le radici dei certificati del sistema (come `casa-core`); quelle illeggibili si saltano.
fn radici_del_sistema() -> ureq::tls::RootCerts {
    let trovate = rustls_native_certs::load_native_certs();
    let certificati: Vec<ureq::tls::Certificate<'static>> = trovate
        .certs
        .iter()
        .map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned())
        .collect();
    ureq::tls::RootCerts::Specific(Arc::new(certificati))
}

fn agente(solo_https: bool) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .root_certs(radici_del_sistema())
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .build();
    ureq::config::Config::builder()
        .https_only(solo_https)
        // Un reindirizzamento non si segue mai (porterebbe la chiave altrove): si legge come risposta.
        .max_redirects(0)
        .max_redirects_will_error(false)
        // Le risposte di errore le leggiamo noi dal codice di stato.
        .http_status_as_error(false)
        // Niente proxy dall'ambiente: la chiave va dritta ad Anthropic.
        .proxy(None)
        .timeout_global(Some(TIMEOUT))
        .user_agent(USER_AGENT)
        .tls_config(tls)
        .build()
        .new_agent()
}

/// Il client vero: base fissa, solo HTTPS.
pub fn anthropic(chiave: ChiaveAdmin) -> ClientApi {
    ClientApi {
        agente: agente(true),
        base: BASE_ANTHROPIC.to_owned(),
        credenziale: Credenziale(chiave.segreto().to_owned()),
    }
}

/// Il client di prova, contro un server finto in locale: solo `http://127.0.0.1:<porta>` e una
/// chiave che comincia per `finta-` (una chiave vera non può finire in un server di prova).
pub fn finto(base: &str, chiave_finta: &str) -> Result<ClientApi, String> {
    let base = base.trim().trim_end_matches('/');
    let porta = base
        .strip_prefix("http://127.0.0.1:")
        .ok_or("l'indirizzo di prova deve essere http://127.0.0.1:<porta>")?;
    if porta.is_empty()
        || !porta.bytes().all(|b| b.is_ascii_digit())
        || porta.parse::<u16>().is_err()
    {
        return Err("la porta dell'indirizzo di prova non è valida".to_owned());
    }
    if !chiave_finta.starts_with("finta-") {
        return Err("la chiave di prova deve cominciare per «finta-»".to_owned());
    }
    Ok(ClientApi {
        agente: agente(false),
        base: base.to_owned(),
        credenziale: Credenziale(chiave_finta.to_owned()),
    })
}

impl ClientApi {
    /// L'unico metodo di rete, sempre `GET`. Restituisce il corpo della risposta 2xx.
    pub fn leggi(&self, l: &Lettura) -> Result<String, ErroreApi> {
        let percorso = l.percorso();
        // Ridondante col tipo, voluto: nulla fuori dall'elenco parte mai.
        if !PERCORSI_AMMESSI.contains(&percorso) {
            return Err(ErroreApi::Formato);
        }
        let mut richiesta = self
            .agente
            .get(format!("{}{percorso}", self.base))
            .header("x-api-key", &self.credenziale.0)
            .header("anthropic-version", VERSIONE_API);
        if l.analitica() {
            richiesta = richiesta.header("anthropic-beta", BETA_ANALITICA);
        }
        for (nome, valore) in l.query() {
            richiesta = richiesta.query(nome, valore);
        }
        let mut risposta = richiesta.call().map_err(|_| ErroreApi::Rete)?;
        let codice = risposta.status().as_u16();
        match codice {
            200..=299 => {}
            401 => return Err(ErroreApi::ChiaveNonValida),
            403 => return Err(ErroreApi::PermessoNegato),
            429 => return Err(ErroreApi::TroppeRichieste),
            altro => return Err(ErroreApi::Servizio(altro)),
        }
        risposta
            .body_mut()
            .with_config()
            .limit(MASSIMO_CORPO)
            .read_to_string()
            .map_err(|_| ErroreApi::Formato)
    }
}
