//! Notifiche di sistema (toast su Windows) con `notify-rust`. Mai un errore visibile: se il
//! sistema non le mostra (notifiche spente, «Non disturbare», servizio assente) resta solo una
//! riga nel log.
//!
//! Un solo thread per tutti i toast, con una coda corta (mostrare un toast passa da WinRT e non
//! deve fermare l'interfaccia; una raffica oltre la coda si scarta, scrivendolo nel log).
//!
//! Su Windows i toast escono a nome «ItsyHome»: l'app registra il proprio identificativo
//! (AUMID) sotto `HKCU\Software\Classes\AppUserModelId\ItsyHome.Assistente` con `DisplayName` e
//! `IconUri`, come fanno le app non pacchettizzate. Senza registrazione (prove locali) escono a
//! nome di Windows PowerShell, l'identificativo predefinito di notify-rust. Il clic sul toast non
//! apre nulla (servirebbe un attivatore COM).

use std::path::Path;
// La coda e il thread dei toast non esistono nei test (`mostra` conta e basta).
#[cfg(not(test))]
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
#[cfg(any(windows, not(test)))]
use std::sync::OnceLock;

/// Identificativo dei toast dell'assistente.
// Lo usa solo il codice di Windows.
#[cfg_attr(not(windows), allow(dead_code))]
pub const AUMID: &str = "ItsyHome.Assistente";
/// Quanti toast possono aspettare il loro turno.
#[cfg(not(test))]
const CODA: usize = 8;

/// L'AUMID registrato con successo (altrimenti i toast usano quello di PowerShell).
#[cfg(windows)]
static REGISTRATO: OnceLock<String> = OnceLock::new();
#[cfg(not(test))]
static INVIO: OnceLock<Option<SyncSender<(String, String)>>> = OnceLock::new();

/// Registra l'identificativo dei toast in HKCU con nome e icona (scritta in `cartella`).
#[cfg(windows)]
pub fn registra(cartella: &Path) -> Result<(), String> {
    use windows_registry::CURRENT_USER;
    let icona = cartella.join("icona-notifiche.png");
    let png: &[u8] = include_bytes!("../../assets/tray.png");
    if std::fs::read(&icona).ok().as_deref() != Some(png) {
        std::fs::create_dir_all(cartella)
            .and_then(|_| std::fs::write(&icona, png))
            .map_err(|e| format!("icona delle notifiche non scritta: {e}"))?;
    }
    let chiave = CURRENT_USER
        .create(format!(r"Software\Classes\AppUserModelId\{AUMID}"))
        .map_err(|e| format!("identificativo delle notifiche non registrato: {e}"))?;
    chiave
        .set_string("DisplayName", "ItsyHome")
        .and_then(|_| chiave.set_string("IconUri", icona.to_string_lossy()))
        .map_err(|e| format!("identificativo delle notifiche non registrato: {e}"))?;
    let _ = REGISTRATO.set(AUMID.to_owned());
    Ok(())
}

#[cfg(not(windows))]
pub fn registra(_cartella: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(test))]
fn coda() -> Option<&'static SyncSender<(String, String)>> {
    INVIO
        .get_or_init(|| {
            let (tx, rx) = sync_channel::<(String, String)>(CODA);
            let avviato = std::thread::Builder::new().name("notifiche".into()).spawn(move || {
                for (titolo, messaggio) in rx {
                    let mut n = notify_rust::Notification::new();
                    n.appname("ItsyHome").summary(&titolo).body(&messaggio);
                    #[cfg(windows)]
                    if let Some(id) = REGISTRATO.get() {
                        n.app_id(id);
                    }
                    if let Err(e) = n.show() {
                        eprintln!("notifica di sistema non mostrata: {e}");
                    }
                }
            });
            match avviato {
                Ok(_) => Some(tx),
                Err(e) => {
                    eprintln!("notifiche di sistema non disponibili: {e}");
                    None
                }
            }
        })
        .as_ref()
}

/// Mette un toast in coda. Ritorna subito.
#[cfg(test)]
pub fn mostra(titolo: &str, messaggio: &str) {
    // Nei test mai una notifica vera (finirebbe nel centro notifiche dell'utente): si conta soltanto.
    CHIAMATE_PROVA.with(|c| c.set(c.get() + 1));
    let _ = (titolo, messaggio);
}

#[cfg(test)]
thread_local! {
    /// Quante volte questo test (questo thread) ha chiesto una notifica: nessuna è partita. Per
    /// thread, così un altro test in parallelo non altera il conto.
    pub static CHIAMATE_PROVA: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(not(test))]
pub fn mostra(titolo: &str, messaggio: &str) {
    let Some(tx) = coda() else { return };
    match tx.try_send((titolo.to_owned(), messaggio.to_owned())) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => eprintln!("notifica di sistema scartata (troppe in coda): {titolo}"),
        Err(TrySendError::Disconnected(_)) => eprintln!("notifica di sistema non mostrata: {titolo}"),
    }
}
