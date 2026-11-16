//! Wi-Fi, Bluetooth e icona della rete (M2, Task 8), con WinRT. Solo su Windows: altrove esiste
//! soltanto `StatoRete` (il lato «Rete» della pillola non si offre e la vista PC mostra solo la
//! velocità della rete, senza riquadri delle radio).
//!
//! Documentazione Microsoft letta prima di scrivere (learn.microsoft.com, 16/11/2026), con i
//! vincoli che questo modulo rispetta:
//!
//! - `Windows.Devices.Radios.Radio` (uwp/api/windows.devices.radios.radio): classe «Agile»
//!   (si può usare da qualunque thread); Windows 10 10240 in su. Le radio possono comparire e
//!   sparire mentre l'app gira; un interruttore fisico può tenerle in `RadioState.Disabled`.
//! - `Radio.GetRadiosAsync()`: un'istantanea delle radio presenti in quel momento. Elencarle e
//!   leggerne `State` è **sempre permesso**, senza alcun accesso (pagina di RequestAccessAsync).
//! - `Radio.RequestAccessAsync()`: serve **solo** per cambiare lo stato; può chiedere il consenso
//!   la prima volta; va chiamato quando l'utente prova a cambiare (non all'avvio) e il risultato
//!   si tiene per la sessione. Qui: alla prima commutazione di ogni `Radio` (che vive solo a
//!   vista PC aperta), tenuto solo se `Allowed`.
//! - `RadioAccessStatus` (uwp/api/windows.devices.radios.radioaccessstatus): `Unspecified` 0
//!   («da trattare come negato»), `Allowed` 1, `DeniedByUser` 2 (pagina della privacy delle
//!   radio), `DeniedBySystem` 3 (permessi o criteri). Un valore diverso da `Allowed` non vuol dire
//!   che la radio manchi: la si legge lo stesso, ma non la si comanda. Vedi `esito_da`.
//! - `Radio.SetStateAsync(RadioState)`: solo `On` od `Off` (`Disabled` è riservato al sistema);
//!   restituisce un `RadioAccessStatus`; `Allowed` vuol dire «richiesta accettata», il passaggio
//!   avviene dopo, e hardware o criteri possono lasciarla com'era: lo stato vero si conferma con
//!   `StateChanged` (o rileggendo `State`). Niente raffiche di On/Off: i comandi passano uno alla
//!   volta dal thread `radio`. Dopo un `Allowed` l'esito resta in sospeso: è `Fatto` appena uno
//!   `StateChanged` porta la radio nello stato voluto; altrimenti, dopo `VERIFICA_DOPO`, lo stato
//!   si rilegge e `esito_cambio` decide: se la radio è rimasta com'era (criteri, modalità aereo,
//!   interruttore fisico) è un rifiuto silenzioso → `Negato`, come un rifiuto esplicito.
//! - `Radio.StateChanged`: un gestore per radio (`TypedEventHandler<Radio, IInspectable>`), tolto
//!   con `RemoveStateChanged(cookie)` prima di lasciare la radio. Il gestore qui fa solo un `send`
//!   al thread `radio`.
//! - Capacità del manifesto (`radios`): sono per le app UWP/impacchettate. Pagina «WinRT APIs not
//!   supported in desktop apps» (windows/apps/desktop/modernize/winrt-api-desktop-app-support):
//!   né `Windows.Devices.Radios` né `Windows.Networking.Connectivity` sono fra le API che
//!   richiedono l'identità del pacchetto (MSIX), quindi si usano da un'app desktop non
//!   impacchettata. Avvertenza della stessa pagina: «la maggior parte dei metodi `Request…` non
//!   è supportata nelle app desktop» (quelli che mostrano un `Windows.UI.Popups`). Se
//!   `RequestAccessAsync` fallisce o nega, la commutazione finisce in `Errore`/`Negato` e si apre
//!   la pagina delle Impostazioni di Windows: è il comportamento previsto per ogni rifiuto, nessun
//!   blocco. **Da confermare dal vivo** che dia `Allowed` su questo PC.
//! - `NetworkInformation.NetworkStatusChanged` (uwp/api/windows.networking.connectivity.
//!   networkinformation.networkstatuschanged): evento statico, «lo stato della rete è cambiato
//!   per una connessione»; si toglie con `RemoveNetworkStatusChanged(cookie)`. Arriva su un thread
//!   di Windows: il gestore qui sveglia soltanto il thread `rete`.
//! - `NetworkInformation.GetInternetConnectionProfile()`: il profilo della connessione a Internet
//!   (nessuno: errore o nullo → `StatoRete::Nessuna`); `ConnectionProfile.IsWlanConnectionProfile`
//!   distingue il Wi-Fi; `ConnectionProfile.GetSignalBars()` (uwp/api/windows.networking.
//!   connectivity.connectionprofile.getsignalbars): `IReference<byte>` 0..=5, «le tacche che
//!   mostra Windows», può mancare (nullo) → `tacche_da`.
//! - `WlanConnectionProfileDetails.GetConnectedSsid()`: il nome della rete. Letto **solo** a vista
//!   PC aperta, mai scritto nei log né nella pillola. Da Windows 11 24H2 può arrivare vuoto senza
//!   il permesso della posizione: allora il riquadro dice «Acceso».
//! - Crate `windows` 0.62: le operazioni asincrone (`windows_future::IAsyncOperation`) hanno
//!   `join()`, che aspetta il risultato sul thread che chiama: qui solo sui thread `radio` e
//!   `rete`, mai su quello dell'interfaccia. COM/WinRT si inizializza su ciascuno dei due thread
//!   (multithread, come il thread `audio`).
//!
//! Leggerezza: `OsservaRete` è acceso solo se il lato «Rete» è scelto e la pillola si vede, o se
//! la vista PC è aperta (`osserva_rete_acceso`); vive solo di eventi, e rilegge le tacche ogni
//! `TACCHE_OGNI` solo mentre la connessione è Wi-Fi. `Radio` vive solo a vista PC aperta: nasce
//! entrandoci, e lasciandola (o chiudendo il pannello) toglie i gestori e libera le radio.

/// Come è collegato il PC, per l'icona della rete (pillola e vista PC). Mai il nome della rete.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatoRete {
    Nessuna,
    Cavo,
    WiFi { tacche: u8 },
}

/// Le barre di Windows (0..=5) in tacche dell'icona (0..=3): 0→0, 1–2→1, 3–4→2, 5→3;
/// senza valore 0.
#[cfg(any(windows, test))]
pub fn tacche_da(barre: Option<u8>) -> u8 {
    match barre {
        None | Some(0) => 0,
        Some(1 | 2) => 1,
        Some(3 | 4) => 2,
        Some(_) => 3,
    }
}

/// Ogni quanto si rileggono le tacche, solo mentre la connessione è Wi-Fi.
#[cfg(windows)]
pub const TACCHE_OGNI: std::time::Duration = std::time::Duration::from_secs(30);

/// Dopo un cambio accettato (`Allowed`) senza uno `StateChanged` nello stato voluto, lo stato si
/// rilegge una volta dopo questo tempo e si decide l'esito (`esito_cambio`).
#[cfg(windows)]
const VERIFICA_DOPO: std::time::Duration = std::time::Duration::from_secs(3);

/// Quale radio.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoRadio {
    WiFi,
    Bluetooth,
}

/// Una radio presente e se è accesa (`RadioState::On`). Una radio che manca non è nell'elenco.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatoRadio {
    pub tipo: TipoRadio,
    pub acceso: bool,
}

/// Com'è andato un cambio.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EsitoRadio {
    Fatto,
    Negato,
    Errore,
}

/// Da `RadioAccessStatus` (il suo valore numerico): `Allowed` (1) → `Fatto`, `DeniedByUser` (2)
/// e `DeniedBySystem` (3) → `Negato`, il resto (`Unspecified` 0) → `Errore`.
#[cfg(any(windows, test))]
pub fn esito_da(stato_accesso: i32) -> EsitoRadio {
    match stato_accesso {
        1 => EsitoRadio::Fatto,
        2 | 3 => EsitoRadio::Negato,
        _ => EsitoRadio::Errore,
    }
}

/// L'esito finale di un cambio. `risposta`: quello di `SetStateAsync` (`esito_da`); `voluto`: lo
/// stato chiesto (acceso o no); `letto`: lo stato riletto dopo. Un rifiuto esplicito resta tale;
/// un `Fatto` con la radio rimasta com'era (criteri, modalità aereo, hardware) diventa `Negato`,
/// così compaiono la riga in ambra e la pagina delle Impostazioni come per un rifiuto esplicito.
#[cfg(any(windows, test))]
pub fn esito_cambio(voluto: bool, letto: bool, risposta: EsitoRadio) -> EsitoRadio {
    match risposta {
        EsitoRadio::Fatto if letto != voluto => EsitoRadio::Negato,
        altro => altro,
    }
}

/// `OsservaRete` acceso: lato «Rete» scelto e pillola visibile (e sveglia), oppure vista PC
/// aperta. Altrimenti spento.
#[cfg(any(windows, test))]
pub fn osserva_rete_acceso(lato_rete: bool, pillola_visibile: bool, vista_pc: bool) -> bool {
    (lato_rete && pillola_visibile) || vista_pc
}

#[cfg(windows)]
pub use winrt::{OsservaRete, Radio};

#[cfg(windows)]
mod winrt {
    use super::{esito_cambio, esito_da, tacche_da, EsitoRadio, StatoRadio, StatoRete, TipoRadio, TACCHE_OGNI, VERIFICA_DOPO};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::sync::Arc;
    use std::time::Instant;
    use windows::Devices::Radios::{Radio as RadioWinRT, RadioAccessStatus, RadioKind, RadioState};
    use windows::Foundation::TypedEventHandler;
    use windows::Networking::Connectivity::{ConnectionProfile, NetworkInformation, NetworkStatusChangedEventHandler};
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    use windows_core::IInspectable;

    /// Chi riceve gli stati delle radio (dal thread `radio`): deve solo accodarli al thread
    /// dell'interfaccia.
    type ConsegnaRadio = fn(Vec<StatoRadio>);
    /// Chi riceve l'esito di un cambio (dal thread `radio`).
    type ConsegnaEsito = fn(TipoRadio, EsitoRadio);

    enum Comando {
        /// Un `StateChanged`: si rileggono gli stati.
        Rileggi,
        Commuta(TipoRadio, bool, ConsegnaEsito),
        Ferma,
    }

    /// Il thread `radio`: le radio Wi-Fi e Bluetooth, i loro `StateChanged` e i cambi.
    pub struct Radio {
        invia: Sender<Comando>,
    }

    impl Radio {
        /// Avvia il thread `radio`: il primo elenco arriva appena lette le radio, poi uno a ogni
        /// `StateChanged`.
        pub fn avvia(consegna: ConsegnaRadio) -> Radio {
            let (invia, ricevi) = mpsc::channel();
            let per_gli_eventi = invia.clone();
            let avviato = std::thread::Builder::new()
                .name("radio".into())
                .spawn(move || gira_radio(consegna, per_gli_eventi, ricevi));
            if let Err(e) = avviato {
                eprintln!("radio non avviate: {e}");
            }
            Radio { invia }
        }

        /// Accende o spegne una radio; l'esito arriva a `esito` (dal thread `radio`).
        pub fn commuta(&self, tipo: TipoRadio, acceso: bool, esito: ConsegnaEsito) {
            if self.invia.send(Comando::Commuta(tipo, acceso, esito)).is_err() {
                esito(tipo, EsitoRadio::Errore);
            }
        }
    }

    impl Drop for Radio {
        /// Il thread toglie i gestori, libera le radio, chiude COM e finisce (nessuna attesa qui).
        fn drop(&mut self) {
            let _ = self.invia.send(Comando::Ferma);
        }
    }

    fn gira_radio(consegna: ConsegnaRadio, invia: Sender<Comando>, ricevi: Receiver<Comando>) {
        // SAFETY: COM/WinRT su questo thread, multithread; chiuso alla fine dopo che
        // `lavora_radio` ha rilasciato ogni oggetto.
        if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
            eprintln!("radio: COM non inizializzato: {e}");
            consegna(Vec::new());
            return;
        }
        lavora_radio(consegna, invia, &ricevi);
        // SAFETY: bilancia il `CoInitializeEx` riuscito qui sopra, sullo stesso thread.
        unsafe { CoUninitialize() };
    }

    /// La prima radio Wi-Fi e la prima Bluetooth (le altre, come la rete mobile, non contano).
    fn trova_radio() -> Vec<(TipoRadio, RadioWinRT)> {
        let elenco = match RadioWinRT::GetRadiosAsync().and_then(|op| op.join()) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("radio non elencate: {e}");
                return Vec::new();
            }
        };
        let mut trovate: Vec<(TipoRadio, RadioWinRT)> = Vec::new();
        for i in 0..elenco.Size().unwrap_or(0) {
            let Ok(r) = elenco.GetAt(i) else { continue };
            let tipo = match r.Kind() {
                Ok(RadioKind::WiFi) => TipoRadio::WiFi,
                Ok(RadioKind::Bluetooth) => TipoRadio::Bluetooth,
                _ => continue,
            };
            if !trovate.iter().any(|(t, _)| *t == tipo) {
                trovate.push((tipo, r));
            }
        }
        trovate
    }

    fn stati(radio: &[(TipoRadio, RadioWinRT, i64)]) -> Vec<StatoRadio> {
        radio
            .iter()
            .map(|(tipo, r, _)| StatoRadio { tipo: *tipo, acceso: r.State().is_ok_and(|s| s == RadioState::On) })
            .collect()
    }

    /// Un cambio accettato (`Allowed`) di cui non si sa ancora se è avvenuto.
    struct Verifica {
        tipo: TipoRadio,
        voluto: bool,
        entro: Instant,
        esito: ConsegnaEsito,
    }

    fn lavora_radio(consegna: ConsegnaRadio, invia: Sender<Comando>, ricevi: &Receiver<Comando>) {
        // Un gestore `StateChanged` per radio: fa solo un `send` (vedi in testa al modulo).
        let radio: Vec<(TipoRadio, RadioWinRT, i64)> = trova_radio()
            .into_iter()
            .map(|(tipo, r)| {
                let invia = invia.clone();
                let gestore = TypedEventHandler::<RadioWinRT, IInspectable>::new(move |_, _| {
                    let _ = invia.send(Comando::Rileggi);
                    Ok(())
                });
                let cookie = r.StateChanged(&gestore).unwrap_or_else(|e| {
                    eprintln!("radio: i cambi da fuori non si vedranno: {e}");
                    0
                });
                (tipo, r, cookie)
            })
            .collect();
        drop(invia);
        consegna(stati(&radio));

        // L'accesso, chiesto alla prima commutazione e tenuto finché vive questa `Radio`.
        let mut permesso = false;
        // I cambi accettati in attesa di conferma (al più uno per radio).
        let mut verifiche: Vec<Verifica> = Vec::new();
        loop {
            let comando = match verifiche.iter().map(|v| v.entro).min() {
                Some(quando) => match ricevi.recv_timeout(quando.saturating_duration_since(Instant::now())) {
                    Ok(c) => c,
                    Err(RecvTimeoutError::Timeout) => {
                        // Scaduta: si rilegge e si decide. Prima gli stati (il riquadro torna a
                        // quello vero), poi l'esito.
                        let letti = stati(&radio);
                        consegna(letti.clone());
                        let adesso = Instant::now();
                        verifiche.retain(|v| {
                            if v.entro > adesso {
                                return true;
                            }
                            let letto = letti.iter().any(|s| s.tipo == v.tipo && s.acceso);
                            (v.esito)(v.tipo, esito_cambio(v.voluto, letto, EsitoRadio::Fatto));
                            false
                        });
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                // Fermo qui finché non arriva un comando o un evento: nessun sondaggio.
                None => match ricevi.recv() {
                    Ok(c) => c,
                    Err(_) => break,
                },
            };
            match comando {
                Comando::Rileggi => {
                    let letti = stati(&radio);
                    consegna(letti.clone());
                    // Uno `StateChanged` nello stato voluto conferma il cambio; uno diverso (un
                    // passaggio intermedio) lascia aspettare fino a `entro`.
                    verifiche.retain(|v| {
                        let arrivato = letti.iter().any(|s| s.tipo == v.tipo && s.acceso == v.voluto);
                        if arrivato {
                            (v.esito)(v.tipo, EsitoRadio::Fatto);
                        }
                        !arrivato
                    });
                }
                Comando::Commuta(tipo, acceso, esito) => {
                    let Some((_, r, _)) = radio.iter().find(|(t, _, _)| *t == tipo) else {
                        esito(tipo, EsitoRadio::Errore);
                        continue;
                    };
                    if !permesso {
                        let accesso = RadioWinRT::RequestAccessAsync().and_then(|op| op.join());
                        match accesso {
                            Ok(RadioAccessStatus::Allowed) => permesso = true,
                            Ok(altro) => {
                                esito(tipo, esito_da(altro.0));
                                continue;
                            }
                            Err(e) => {
                                eprintln!("radio: accesso non chiesto: {e}");
                                esito(tipo, EsitoRadio::Errore);
                                continue;
                            }
                        }
                    }
                    let voluto = if acceso { RadioState::On } else { RadioState::Off };
                    let fatto = match r.SetStateAsync(voluto).and_then(|op| op.join()) {
                        Ok(s) => esito_da(s.0),
                        Err(e) => {
                            eprintln!("radio: cambio non riuscito: {e}");
                            EsitoRadio::Errore
                        }
                    };
                    // Un tocco nuovo sulla stessa radio sostituisce l'attesa di prima.
                    verifiche.retain(|v| v.tipo != tipo);
                    if fatto == EsitoRadio::Fatto {
                        // Accettato: l'esito si sa alla conferma (`StateChanged`) o alla rilettura.
                        verifiche.push(Verifica { tipo, voluto: acceso, entro: Instant::now() + VERIFICA_DOPO, esito });
                    } else {
                        esito(tipo, fatto);
                    }
                }
                Comando::Ferma => break,
            }
        }

        // Via i gestori prima di lasciare le radio.
        for (_, r, cookie) in &radio {
            if *cookie != 0 {
                let _ = r.RemoveStateChanged(*cookie);
            }
        }
    }

    /// Il thread `rete`: segue la connessione a Internet con `NetworkStatusChanged` (e le tacche
    /// ogni `TACCHE_OGNI` solo se Wi-Fi). Il nome della rete solo su richiesta (vista PC aperta).
    pub struct OsservaRete {
        ferma: Arc<AtomicBool>,
        /// Il nome della rete serve (vista PC aperta).
        con_nome: Arc<AtomicBool>,
        /// Il nome va riconsegnato anche se uguale (appena richiesto).
        rimanda_nome: Arc<AtomicBool>,
        thread: Option<std::thread::Thread>,
    }

    impl OsservaRete {
        /// Avvia il thread `rete`: il primo stato arriva subito, poi uno a ogni cambio. Il nome
        /// della rete (solo dopo `chiedi_nome(true)`) arriva a `consegna_nome`.
        pub fn avvia(consegna: fn(StatoRete), consegna_nome: fn(Option<String>)) -> OsservaRete {
            let ferma = Arc::new(AtomicBool::new(false));
            let con_nome = Arc::new(AtomicBool::new(false));
            let rimanda_nome = Arc::new(AtomicBool::new(false));
            let (f, c, r) = (ferma.clone(), con_nome.clone(), rimanda_nome.clone());
            let thread = std::thread::Builder::new()
                .name("rete".into())
                .spawn(move || gira_rete(consegna, consegna_nome, &f, &c, &r));
            let thread = match thread {
                Ok(t) => Some(t.thread().clone()),
                Err(e) => {
                    eprintln!("rete non osservata: {e}");
                    None
                }
            };
            OsservaRete { ferma, con_nome, rimanda_nome, thread }
        }

        /// Il nome della rete serve (vero) o non più (falso).
        pub fn chiedi_nome(&self, si: bool) {
            if self.con_nome.swap(si, Ordering::Relaxed) != si && si {
                self.rimanda_nome.store(true, Ordering::Relaxed);
                if let Some(t) = &self.thread {
                    t.unpark();
                }
            }
        }
    }

    impl Drop for OsservaRete {
        /// Il thread toglie il gestore, chiude COM e finisce (nessuna attesa qui).
        fn drop(&mut self) {
            self.ferma.store(true, Ordering::Relaxed);
            if let Some(t) = &self.thread {
                t.unpark();
            }
        }
    }

    /// Lo stato della connessione a Internet e il profilo (per il nome).
    fn leggi_rete() -> (StatoRete, Option<ConnectionProfile>) {
        let Ok(profilo) = NetworkInformation::GetInternetConnectionProfile() else {
            return (StatoRete::Nessuna, None);
        };
        let stato = if profilo.IsWlanConnectionProfile().unwrap_or(false) {
            let barre = profilo.GetSignalBars().and_then(|r| r.Value()).ok();
            StatoRete::WiFi { tacche: tacche_da(barre) }
        } else {
            StatoRete::Cavo
        };
        (stato, Some(profilo))
    }

    /// Il nome della rete Wi-Fi collegata (vuoto: `None`).
    fn nome_rete(profilo: &ConnectionProfile) -> Option<String> {
        let nome = profilo.WlanConnectionProfileDetails().and_then(|d| d.GetConnectedSsid()).ok()?.to_string();
        (!nome.is_empty()).then_some(nome)
    }

    fn gira_rete(
        consegna: fn(StatoRete),
        consegna_nome: fn(Option<String>),
        ferma: &AtomicBool,
        con_nome: &AtomicBool,
        rimanda_nome: &AtomicBool,
    ) {
        // SAFETY: COM/WinRT su questo thread, multithread; chiuso alla fine.
        if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
            eprintln!("rete: COM non inizializzato: {e}");
            return;
        }
        let io = std::thread::current();
        let gestore = NetworkStatusChangedEventHandler::new(move |_| {
            io.unpark();
            Ok(())
        });
        let cookie = match NetworkInformation::NetworkStatusChanged(&gestore) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("rete: i cambi non si vedranno: {e}");
                None
            }
        };
        let mut ultimo: Option<StatoRete> = None;
        let mut ultimo_nome: Option<Option<String>> = None;
        while !ferma.load(Ordering::Relaxed) {
            let (stato, profilo) = leggi_rete();
            if ultimo != Some(stato) {
                ultimo = Some(stato);
                consegna(stato);
            }
            if con_nome.load(Ordering::Relaxed) {
                let nome = match (&stato, &profilo) {
                    (StatoRete::WiFi { .. }, Some(p)) => nome_rete(p),
                    _ => None,
                };
                if rimanda_nome.swap(false, Ordering::Relaxed) || ultimo_nome.as_ref() != Some(&nome) {
                    ultimo_nome = Some(nome.clone());
                    consegna_nome(nome);
                }
            } else {
                ultimo_nome = None;
            }
            drop(profilo);
            // Svegliato da `NetworkStatusChanged`, da `chiedi_nome` o dalla fine; le tacche si
            // rileggono da sole solo se Wi-Fi.
            if matches!(stato, StatoRete::WiFi { .. }) {
                std::thread::park_timeout(TACCHE_OGNI);
            } else {
                std::thread::park();
            }
        }
        if let Some(c) = cookie {
            let _ = NetworkInformation::RemoveNetworkStatusChanged(c);
        }
        drop(gestore);
        // SAFETY: bilancia il `CoInitializeEx` riuscito qui sopra, sullo stesso thread.
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tacche_da_barre() {
        assert_eq!(tacche_da(None), 0);
        for (barre, tacche) in [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3)] {
            assert_eq!(tacche_da(Some(barre)), tacche, "{barre} barre");
        }
    }

    #[test]
    fn esito_dell_accesso() {
        // I valori di `RadioAccessStatus`: Unspecified 0, Allowed 1, DeniedByUser 2, DeniedBySystem 3.
        assert_eq!(esito_da(1), EsitoRadio::Fatto);
        assert_eq!(esito_da(2), EsitoRadio::Negato);
        assert_eq!(esito_da(3), EsitoRadio::Negato);
        assert_eq!(esito_da(0), EsitoRadio::Errore);
        assert_eq!(esito_da(42), EsitoRadio::Errore);
    }

    /// Il rifiuto silenzioso: Windows risponde `Allowed` ma la radio riletta non è nello stato
    /// voluto (criteri, modalità aereo, hardware) → `Negato`, come un rifiuto esplicito.
    #[test]
    fn esito_dopo_la_rilettura() {
        use EsitoRadio::*;
        // (voluto, letto, risposta) → esito
        for (voluto, letto, risposta, atteso) in [
            (true, true, Fatto, Fatto),
            (false, false, Fatto, Fatto),
            (true, false, Fatto, Negato),
            (false, true, Fatto, Negato),
            (true, false, Negato, Negato),
            (true, true, Negato, Negato),
            (false, true, Errore, Errore),
            (false, false, Errore, Errore),
        ] {
            assert_eq!(esito_cambio(voluto, letto, risposta), atteso, "voluto {voluto}, letto {letto}, risposta {risposta:?}");
        }
    }

    /// I valori numerici di `esito_da` sono quelli di WinRT.
    #[cfg(windows)]
    #[test]
    fn valori_di_radio_access_status() {
        use windows::Devices::Radios::RadioAccessStatus;
        assert_eq!(esito_da(RadioAccessStatus::Allowed.0), EsitoRadio::Fatto);
        assert_eq!(esito_da(RadioAccessStatus::DeniedByUser.0), EsitoRadio::Negato);
        assert_eq!(esito_da(RadioAccessStatus::DeniedBySystem.0), EsitoRadio::Negato);
        assert_eq!(esito_da(RadioAccessStatus::Unspecified.0), EsitoRadio::Errore);
    }

    #[test]
    fn osserva_rete_solo_quando_serve() {
        assert!(!osserva_rete_acceso(false, true, false), "lato non scelto, vista PC chiusa");
        assert!(!osserva_rete_acceso(true, false, false), "lato scelto ma pillola non visibile");
        assert!(osserva_rete_acceso(true, true, false));
        assert!(osserva_rete_acceso(false, false, true), "vista PC aperta");
    }

    /// Sulla macchina vera: legge rete e radio **senza cambiare nulla** (nessun `commuta`) e non
    /// scrive il nome della rete. `cargo test -p assistente -- --ignored legge_rete_e_radio`.
    #[cfg(windows)]
    #[test]
    #[ignore = "legge rete e radio vere del PC (senza cambiarle)"]
    fn legge_rete_e_radio() {
        use std::sync::Mutex;
        use std::time::{Duration, Instant};
        static RETE: Mutex<Vec<StatoRete>> = Mutex::new(Vec::new());
        static RADIO: Mutex<Vec<Vec<StatoRadio>>> = Mutex::new(Vec::new());
        fn rete(s: StatoRete) {
            if let Ok(mut v) = RETE.lock() {
                v.push(s);
            }
        }
        fn nome(_: Option<String>) {}
        fn radio(s: Vec<StatoRadio>) {
            if let Ok(mut v) = RADIO.lock() {
                v.push(s);
            }
        }
        let osserva = OsservaRete::avvia(rete, nome);
        let r = Radio::avvia(radio);
        let inizio = Instant::now();
        while RETE.lock().map(|v| v.is_empty()).unwrap_or(true) || RADIO.lock().map(|v| v.is_empty()).unwrap_or(true) {
            assert!(inizio.elapsed() < Duration::from_secs(10), "nulla letto in 10 s");
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!("rete: {:?}", RETE.lock().unwrap()[0]);
        eprintln!("radio: {:?}", RADIO.lock().unwrap()[0]);
        drop(r);
        drop(osserva);
    }
}
