//! Notifiche di sistema (toast su Windows) con `notify-rust`. Mai un errore visibile: se il
//! sistema non le mostra (notifiche spente, «Non disturbare», servizio assente) resta solo una
//! riga nel log. Su un thread a parte: mostrare un toast passa da WinRT e non deve fermare
//! l'interfaccia.
//!
//! Su Windows il toast esce a nome di Windows PowerShell (l'identificativo predefinito di
//! notify-rust): un identificativo nostro richiede un collegamento registrato nel menu Start,
//! che arriverà con l'installatore.

/// Mostra un toast. Ritorna subito.
pub fn mostra(titolo: &str, messaggio: &str) {
    let (titolo, messaggio) = (titolo.to_owned(), messaggio.to_owned());
    let avviato = std::thread::Builder::new().name("notifica".into()).spawn(move || {
        let esito = notify_rust::Notification::new().appname("ItsyHome").summary(&titolo).body(&messaggio).show();
        if let Err(e) = esito {
            eprintln!("notifica di sistema non mostrata: {e}");
        }
    });
    if let Err(e) = avviato {
        eprintln!("notifica di sistema non mostrata: {e}");
    }
}
