//! La chiave da amministratore e le letture delle API di Anthropic (M3, Task 8, spec §5 e §6.7).
//!
//! La chiave sta **solo** nel portachiavi del sistema (`isola:claude-admin`); qui si legge, si
//! controlla e si passa a `api-claude`, che la tiene il tempo di una lettura. Nelle Impostazioni
//! se ne vedono solo le ultime 4 cifre. Non va in nessun log né messaggio.
//!
//! Le letture girano in un thread `api-claude` creato per ogni lettura (client creato e lasciato
//! andare) e il riepilogo torna con `invoke_from_event_loop`. Il timer da 3 ore esiste **solo**
//! mentre la chiave c'è: si arma all'avvio (dopo 60 s) e al salvataggio, si ferma con «Togli».
//!
//! In prova (`ITSYHOME_ASSISTENTE_DIR`) la chiave sta in un'altra voce del portachiavi e le
//! variabili `ITSYHOME_API_CLAUDE_BASE` / `ITSYHOME_API_CLAUDE_CHIAVE` indicano un server finto
//! (`api_claude::finto`: solo 127.0.0.1 e chiave «finta-…»). Fuori dalla prova la chiave del
//! portachiavi va solo a `api_claude::anthropic`.

use crate::ponte::{ponte, Ponte};
use crate::FinestraImpostazioni;
use api_claude::{aggiorna, anthropic, finto, ChiaveAdmin, ClientApi, Lettura, Riepilogo, Scheda};
use casa_core::diagnosi::Contatore;
use chrono::{DateTime, Local, Utc};
use std::time::Duration;

/// La voce del portachiavi della chiave da amministratore.
pub(crate) const ID_PORTACHIAVI: &str = "isola:claude-admin";
/// In prova un'altra voce: la prova non tocca mai quella vera.
const ID_PORTACHIAVI_PROVA: &str = "isola:claude-admin-prova";
pub(crate) const VAR_BASE: &str = "ITSYHOME_API_CLAUDE_BASE";
pub(crate) const VAR_CHIAVE: &str = "ITSYHOME_API_CLAUDE_CHIAVE";

/// All'avvio: la prima lettura dopo un minuto (l'app non deve rallentare la partenza).
const PRIMA_LETTURA: Duration = Duration::from_secs(60);
/// Appena salvata la chiave: poco, così si vede subito che funziona.
const DOPO_SALVATAGGIO: Duration = Duration::from_secs(5);
/// Poi ogni 3 ore: i dati arrivano con 4-24 ore di ritardo, leggere più spesso non dà nulla.
const OGNI: Duration = Duration::from_secs(3 * 60 * 60);

fn in_prova() -> bool {
    std::env::var_os(crate::avvio::VAR_CARTELLA).is_some_and(|v| !v.is_empty())
}

fn id_portachiavi(in_prova: bool) -> &'static str {
    if in_prova {
        ID_PORTACHIAVI_PROVA
    } else {
        ID_PORTACHIAVI
    }
}

/// Il server finto, solo in prova e solo se le due variabili ci sono.
fn finta(in_prova: bool, base: Option<String>, chiave: Option<String>) -> Option<(String, String)> {
    match (in_prova, base, chiave) {
        (true, Some(b), Some(c)) if !b.is_empty() && !c.is_empty() => Some((b, c)),
        _ => None,
    }
}

fn finta_dall_ambiente() -> Option<(String, String)> {
    finta(
        in_prova(),
        std::env::var(VAR_BASE).ok(),
        std::env::var(VAR_CHIAVE).ok(),
    )
}

/// Il client per una lettura. Con un server finto indicato si usa **solo** quello (se non è valido
/// è un errore: mai ripiegare sulla chiave vera); senza, la chiave va solo ad Anthropic.
pub(crate) fn crea_client(
    salvata: &str,
    finta: Option<(String, String)>,
) -> Result<ClientApi, String> {
    match finta {
        Some((base, chiave)) => finto(&base, &chiave),
        None => ChiaveAdmin::nuova(salvata)
            .map(anthropic)
            .map_err(str::to_owned),
    }
}

/// Un giro di letture; il client si lascia andare appena finito.
fn leggi_riepilogo(
    salvata: &str,
    finta: Option<(String, String)>,
    adesso: DateTime<Utc>,
) -> Result<Riepilogo, String> {
    let client = crea_client(salvata, finta)?;
    let riepilogo = aggiorna(&client, adesso);
    drop(client);
    Ok(riepilogo)
}

/// «Chiave valida» o la frase dell'errore: una sola lettura, l'organizzazione.
fn prova_chiave(salvata: &str, finta: Option<(String, String)>) -> (String, bool) {
    match crea_client(salvata, finta) {
        Err(e) => (e, true),
        Ok(client) => match client.leggi(&Lettura::Organizzazione) {
            Ok(_) => ("Chiave valida".to_owned(), false),
            Err(e) => (e.to_string(), true),
        },
    }
}

fn nome_scheda(s: &Scheda) -> &'static str {
    match s {
        Scheda::Spesa => "Spesa",
        Scheda::Token => "Token",
        Scheda::Chiavi => "Chiavi",
        Scheda::WorkspaceMembri => "Workspace e membri",
        Scheda::Limiti => "Limiti",
    }
}

/// La riga sotto la chiave: com'è andata l'ultima lettura (nulla di segreto: la spesa del mese e
/// l'ora di aggiornamento dei dati).
pub(crate) fn riga_stato(
    ora: &str,
    esito: &Result<Riepilogo, String>,
    aggiornato: Option<&str>,
) -> String {
    let r = match esito {
        Err(e) => return format!("Lettura delle {ora} non riuscita: {e}"),
        Ok(r) => r,
    };
    if let Some(e) = r.errore {
        return format!("Lettura delle {ora} non riuscita: {e}");
    }
    let mut parti = vec![format!("Lettura delle {ora}")];
    parti.push(match r.spesa_mese {
        Some(s) => format!("spesa del mese {}", s.in_dollari()),
        None => "nessun dato di spesa".to_owned(),
    });
    if let Some(a) = aggiornato {
        parti.push(format!("dati aggiornati alle {a}"));
    }
    if !r.non_disponibili.is_empty() {
        let nomi: Vec<_> = r.non_disponibili.iter().map(nome_scheda).collect();
        parti.push(format!("non disponibili: {}", nomi.join(", ")));
    }
    parti.join(" · ")
}

impl Ponte {
    /// All'avvio (chiamata da `main`): se la chiave c'è, la prima lettura fra 60 s e poi ogni 3 ore.
    pub fn avvia_api(&self) {
        if matches!(self.segreti.leggi(id_portachiavi(in_prova())), Ok(Some(_))) {
            self.api_arma(PRIMA_LETTURA);
        }
    }

    /// Accende il timer: prima lettura fra `primo`, poi ogni 3 ore. Un solo timer, solo con la chiave.
    fn api_arma(&self, primo: Duration) {
        self.sveglia_api
            .start(slint::TimerMode::Repeated, primo, || {
                if let Some(p) = ponte() {
                    p.sveglia_api.set_interval(OGNI);
                    p.api_leggi();
                }
            });
    }

    /// Una lettura completa in un thread `api-claude`; se un'altra è in corso non ne parte una seconda.
    fn api_leggi(&self) {
        if self.api_in_corso.replace(true) {
            return;
        }
        let id = id_portachiavi(in_prova());
        let segreti = self.segreti.clone();
        let finta = finta_dall_ambiente();
        let diagnosi = self.casa.diagnosi_condivisa();
        diagnosi.conta(Contatore::api_letture);
        let partito = std::thread::Builder::new()
            .name("api-claude".into())
            .spawn(move || {
                let esito = match segreti.leggi(id) {
                    Ok(Some(chiave)) => leggi_riepilogo(&chiave, finta, Utc::now()),
                    Ok(None) => Err("Nessuna chiave salvata.".to_owned()),
                    Err(_) => Err("Il portachiavi non risponde.".to_owned()),
                };
                if !matches!(&esito, Ok(r) if r.errore.is_none() && r.non_disponibili.is_empty()) {
                    diagnosi.conta(Contatore::api_errori);
                }
                let ora = Local::now().format("%H:%M").to_string();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(p) = ponte() {
                        p.api_arrivato(ora, esito);
                    }
                });
            });
        if partito.is_err() {
            self.api_in_corso.set(false);
        }
    }

    /// Il riepilogo è arrivato (thread dell'interfaccia). Se nel frattempo la chiave è stata tolta
    /// si scarta: chi non ha la chiave non tiene dati.
    pub(crate) fn api_arrivato(&self, ora: String, esito: Result<Riepilogo, String>) {
        self.api_in_corso.set(false);
        if !self.sveglia_api.running() {
            return;
        }
        *self.api_ultimo.borrow_mut() = Some((ora, esito));
        self.con_finestra_claude(|f| self.mostra_api_stato(f));
    }

    fn mostra_api_stato(&self, f: &FinestraImpostazioni) {
        let testo = match &*self.api_ultimo.borrow() {
            None => String::new(),
            Some((ora, esito)) => {
                let aggiornato = esito
                    .as_ref()
                    .ok()
                    .and_then(|r| r.aggiornato_a)
                    .map(|a| a.with_timezone(&Local).format("%H:%M").to_string());
                riga_stato(ora, esito, aggiornato.as_deref())
            }
        };
        f.set_api_stato(testo.into());
    }

    fn api_esito(&self, testo: &str, errore: bool) {
        self.con_finestra_claude(|f| {
            f.set_api_esito(testo.into());
            f.set_api_esito_errore(errore);
        });
    }

    /// Lo stato iniziale del blocco e le sue azioni, alla creazione della finestra.
    pub(crate) fn collega_api(&self, f: &FinestraImpostazioni) {
        match self.segreti.leggi(id_portachiavi(in_prova())) {
            Ok(Some(k)) => {
                f.set_api_presente(true);
                f.set_api_ultime(
                    ChiaveAdmin::nuova(&k)
                        .map(|c| c.ultime_quattro())
                        .unwrap_or_default()
                        .into(),
                );
            }
            _ => f.set_api_presente(false),
        }
        f.set_api_occupato(self.api_prova_in_corso.get());
        self.mostra_api_stato(f);
        f.on_api_salva(|campo| {
            if let Some(p) = ponte() {
                p.api_salva_ui(campo.as_str());
            }
        });
        f.on_api_prova(|campo| {
            if let Some(p) = ponte() {
                p.api_prova_ui(campo.as_str());
            }
        });
        f.on_api_togli(|| {
            if let Some(p) = ponte() {
                p.api_togli_ui();
            }
        });
    }

    /// «Salva»: controlla la forma, la mette nel portachiavi (se non l'accetta non resta in nessun
    /// altro posto), svuota il campo e mostra le ultime 4 cifre.
    pub(crate) fn api_salva_ui(&self, campo: &str) {
        let chiave = match ChiaveAdmin::nuova(campo) {
            Ok(c) => c,
            Err(e) => return self.api_esito(e, true),
        };
        if self
            .segreti
            .salva(id_portachiavi(in_prova()), chiave.per_il_portachiavi())
            .is_err()
        {
            return self.api_esito("Il portachiavi non ha accettato la chiave.", true);
        }
        self.con_finestra_claude(|f| {
            f.set_api_campo("".into());
            f.set_api_presente(true);
            f.set_api_ultime(chiave.ultime_quattro().into());
        });
        self.api_esito(
            "Chiave salvata. La prima lettura parte fra pochi secondi.",
            false,
        );
        self.api_arma(DOPO_SALVATAGGIO);
    }

    /// «Togli»: via dal portachiavi, via il timer, via i dati letti.
    pub(crate) fn api_togli_ui(&self) {
        if self.segreti.cancella(id_portachiavi(in_prova())).is_err() {
            return self.api_esito("Il portachiavi non ha tolto la chiave.", true);
        }
        self.sveglia_api.stop();
        *self.api_ultimo.borrow_mut() = None;
        self.con_finestra_claude(|f| {
            f.set_api_presente(false);
            f.set_api_ultime("".into());
            f.set_api_campo("".into());
            f.set_api_stato("".into());
        });
        self.api_esito("Chiave tolta.", false);
    }

    /// «Prova»: una sola lettura (l'organizzazione) con la chiave scritta nel campo o, se è vuoto,
    /// con quella salvata.
    pub(crate) fn api_prova_ui(&self, campo: &str) {
        if self.api_prova_in_corso.replace(true) {
            return;
        }
        let id = id_portachiavi(in_prova());
        let salvata = if campo.trim().is_empty() {
            match self.segreti.leggi(id) {
                Ok(Some(k)) => Ok(k),
                Ok(None) => Err("Incolla prima la chiave da amministratore."),
                Err(_) => Err("Il portachiavi non risponde."),
            }
        } else {
            Ok(campo.to_owned())
        };
        let chiave = match salvata {
            Ok(k) => k,
            Err(e) => {
                self.api_prova_in_corso.set(false);
                return self.api_esito(e, true);
            }
        };
        self.con_finestra_claude(|f| {
            f.set_api_occupato(true);
            f.set_api_esito("".into());
        });
        let finta = finta_dall_ambiente();
        let partito = std::thread::Builder::new()
            .name("api-claude".into())
            .spawn(move || {
                let (testo, errore) = prova_chiave(&chiave, finta);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(p) = ponte() {
                        p.api_prova_finita(&testo, errore);
                    }
                });
            });
        if partito.is_err() {
            self.api_prova_finita("Non riesco ad avviare la lettura.", true);
        }
    }

    pub(crate) fn api_prova_finita(&self, testo: &str, errore: bool) {
        self.api_prova_in_corso.set(false);
        self.con_finestra_claude(|f| f.set_api_occupato(false));
        self.api_esito(testo, errore);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ponte::test::ponte_di_prova;
    use chrono::TimeZone;

    #[test]
    fn il_server_finto_solo_in_prova_e_con_entrambe_le_variabili() {
        let b = || Some("http://127.0.0.1:1".to_owned());
        let c = || Some("finta-x".to_owned());
        assert!(finta(true, b(), c()).is_some());
        assert!(finta(false, b(), c()).is_none(), "fuori dalla prova mai");
        assert!(finta(true, b(), None).is_none());
        assert!(finta(true, None, c()).is_none());
        assert!(finta(true, b(), Some(String::new())).is_none());
    }

    #[test]
    fn la_voce_del_portachiavi_di_prova_e_diversa() {
        assert_eq!(id_portachiavi(false), "isola:claude-admin");
        assert_ne!(id_portachiavi(true), id_portachiavi(false));
    }

    #[test]
    fn con_un_server_finto_non_si_ripiega_sulla_chiave_vera() {
        let vera = "sk-ant-admin01-chiave-vera-1a2b";
        // Indirizzo finto non valido: errore, non un client verso Anthropic con la chiave vera.
        let e = crea_client(
            vera,
            Some(("https://api.anthropic.com".into(), "finta-x".into())),
        );
        assert!(e.is_err());
        let e = crea_client(vera, Some(("http://127.0.0.1:1".into(), vera.into())));
        assert!(e.is_err(), "la chiave vera non va a un server di prova");
        assert!(crea_client(vera, Some(("http://127.0.0.1:1".into(), "finta-x".into()))).is_ok());
        assert!(crea_client(vera, None).is_ok());
        assert!(
            crea_client("sk-ant-api03-normale", None).is_err(),
            "una chiave non da amministratore non parte"
        );
    }

    #[test]
    fn la_riga_di_stato_dice_cosa_c_e_e_cosa_manca() {
        let ok = Riepilogo {
            spesa_mese: api_claude::importo_da("41280"),
            non_disponibili: vec![Scheda::Limiti, Scheda::WorkspaceMembri],
            ..Default::default()
        };
        let t = riga_stato("14:05", &Ok(ok), Some("12:02"));
        assert_eq!(t, "Lettura delle 14:05 · spesa del mese 412,80 $ · dati aggiornati alle 12:02 · non disponibili: Limiti, Workspace e membri");
        let vuoto = riga_stato("14:05", &Ok(Riepilogo::default()), None);
        assert_eq!(
            vuoto, "Lettura delle 14:05 · nessun dato di spesa",
            "vuoto non è 0 $"
        );
        let rifiutata = Riepilogo {
            errore: Some(api_claude::ErroreApi::ChiaveNonValida),
            ..Default::default()
        };
        assert_eq!(
            riga_stato("14:05", &Ok(rifiutata), None),
            "Lettura delle 14:05 non riuscita: La chiave non è valida o è scaduta."
        );
        assert_eq!(
            riga_stato("14:05", &Err("Nessuna chiave salvata.".into()), None),
            "Lettura delle 14:05 non riuscita: Nessuna chiave salvata."
        );
        assert!(!riga_stato("14:05", &Err("x".into()), None).contains("sk-ant"));
    }

    #[test]
    fn leggi_riepilogo_contro_il_server_finto() {
        // Il server finto delle fixture vive in api-claude (solo test): qui basta un server che
        // risponde 401 a tutto, per vedere che l'errore risale senza la chiave.
        use std::io::{Read, Write};
        let ascolto = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = ascolto.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            ascolto.set_nonblocking(false).unwrap();
            let (mut s, _) = ascolto.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let corpo = "{}";
            let _ = write!(
                s,
                "HTTP/1.1 401 X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{corpo}",
                corpo.len()
            );
        });
        let adesso = Utc.with_ymd_and_hms(2026, 11, 17, 10, 43, 0).unwrap();
        let r = leggi_riepilogo(
            "",
            Some((format!("http://127.0.0.1:{porta}"), "finta-prova".into())),
            adesso,
        )
        .unwrap();
        t.join().unwrap();
        assert_eq!(r.errore, Some(api_claude::ErroreApi::ChiaveNonValida));
        let testo = riga_stato("10:43", &Ok(r), None);
        assert!(!testo.contains("finta-prova"));
    }

    #[test]
    fn il_timer_esiste_solo_con_la_chiave() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        assert!(!p.sveglia_api.running(), "senza chiave nessun timer");
        p.avvia_api();
        assert!(
            !p.sveglia_api.running(),
            "senza chiave l'avvio non arma nulla"
        );

        p.api_salva_ui("sk-ant-api03-normale");
        assert!(
            !p.sveglia_api.running(),
            "una chiave non valida non arma il timer"
        );
        assert_eq!(p.segreti.leggi(ID_PORTACHIAVI).unwrap(), None);

        p.api_salva_ui(" sk-ant-admin01-di-prova-1a2b \n");
        assert_eq!(
            p.segreti.leggi(ID_PORTACHIAVI).unwrap().as_deref(),
            Some("sk-ant-admin01-di-prova-1a2b")
        );
        assert!(p.sveglia_api.running(), "con la chiave il timer c'è");

        p.api_togli_ui();
        assert!(
            !p.sveglia_api.running(),
            "tolta la chiave il timer sparisce"
        );
        assert_eq!(p.segreti.leggi(ID_PORTACHIAVI).unwrap(), None);
        assert!(p.api_ultimo.borrow().is_none());
    }

    #[test]
    fn avvio_con_la_chiave_arma_il_timer() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.segreti
            .salva(ID_PORTACHIAVI, "sk-ant-admin01-di-prova-1a2b")
            .unwrap();
        p.avvia_api();
        assert!(p.sveglia_api.running());
        p.sveglia_api.stop();
    }

    #[test]
    fn un_riepilogo_che_arriva_dopo_il_togli_si_scarta() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.api_in_corso.set(true);
        p.api_arrivato("10:00".into(), Ok(Riepilogo::default()));
        assert!(!p.api_in_corso.get());
        assert!(
            p.api_ultimo.borrow().is_none(),
            "senza chiave (timer spento) nessun dato si tiene"
        );
    }

    #[test]
    fn la_prova_senza_chiave_lo_dice() {
        let (_s, p, _pillola, _d) = ponte_di_prova();
        p.api_prova_ui("");
        assert!(
            !p.api_prova_in_corso.get(),
            "nessun thread parte senza chiave"
        );
    }
}
