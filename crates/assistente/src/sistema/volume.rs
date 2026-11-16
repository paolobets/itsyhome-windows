//! Il volume del PC (M2, Task 7): la rotella sulla pillola e il cursore della vista PC, letti e
//! comandati con Core Audio. Solo su Windows: altrove `Audio` non esiste e `disponibile()` è
//! falso (la riga «VOLUME» della vista PC non c'è, la rotella sulla pillola non fa nulla).
//!
//! Documentazione Microsoft letta prima di scrivere (learn.microsoft.com, 16/11/2026), con i
//! vincoli che questo modulo rispetta:
//!
//! - `IMMDeviceEnumerator::GetDefaultAudioEndpoint(dataFlow, role)`: il dispositivo predefinito
//!   per direzione e ruolo; il chiamante rilascia l'`IMMDevice`. Senza alcun dispositivo
//!   restituisce `E_NOTFOUND` (la pagina dice anche `ERROR_NOT_FOUND`) e nessun puntatore: è il
//!   caso «PC senza uscita audio», che qui diventa `consegna(None)`, non un errore. Oggi Windows
//!   dà i tre ruoli allo stesso dispositivo; si chiede `eRender` / `eMultimedia`.
//! - `IAudioEndpointVolume::SetMasterVolumeLevelScalar(fLevel, pguidEventContext)`: `fLevel`
//!   normalizzato 0.0..=1.0 (fuori: `E_INVALIDARG`), sulla curva «audio-tapered» di Windows, la
//!   stessa delle posizioni dei cursori: quindi il valore va al cursore così com'è. Ogni client
//!   registrato riceve `OnNotify`; con il contesto nullo arriva `GUID_NULL`.
//! - `IAudioEndpointVolume::SetMute(bMute, pguidEventContext)`: `S_OK` se lo stato cambia,
//!   `S_FALSE` se era già quello (non è un errore).
//! - `RegisterControlChangeNotify` / `UnregisterControlChangeNotify`: la registrazione fa
//!   `AddRef` sul callback, la cancellazione `Release`. Va cancellata prima di rilasciare l'ultimo
//!   riferimento al callback e all'`IAudioEndpointVolume`, mai dal distruttore del callback (si
//!   bloccherebbero a vicenda: perdite). Le notifiche arrivano finché l'`IAudioEndpointVolume`
//!   esiste: cambiando dispositivo si cancella sul vecchio e si registra sul nuovo.
//! - `IAudioEndpointVolumeCallback::OnNotify(AUDIO_VOLUME_NOTIFICATION_DATA)`: `bMuted` e
//!   `fMasterVolume` (oltre al contesto e ai canali); gira su un thread di Windows, non sul
//!   nostro: niente lavoro pesante, nessuna attesa. Qui legge due numeri e li consegna
//!   (`invoke_from_event_loop`, che accoda e torna subito).
//! - `IMMNotificationClient::OnDefaultDeviceChanged(flow, role, pwstrDefaultDeviceId)`: chiamato
//!   una volta per ruolo cambiato (tre volte per un cambio dall'interfaccia di Windows); l'id è
//!   nullo se non resta nessun dispositivo per quel ruolo. Regole dell'interfaccia: metodi non
//!   bloccanti, mai attendere un oggetto di sincronizzazione, mai chiamare
//!   `Register/UnregisterEndpointNotificationCallback` dentro un callback, mai rilasciare lì
//!   l'ultimo riferimento a un oggetto MMDevice. Quindi il callback si limita a un `send` sul
//!   canale del thread `audio` (solo per `eRender` / `eMultimedia`: un cambio, un ricollegamento),
//!   e ricollegarsi lo fa il thread `audio`.
//! - Crate `windows` 0.62: `#[implement(Interfaccia)]` (da `windows-core`) su una struct genera
//!   `Struct_Impl`; si implementa `Interfaccia_Impl for Struct_Impl` e l'oggetto COM nasce con
//!   `let i: Interfaccia = Struct { … }.into()`.
//!
//! Leggerezza: il thread `audio` nasce la prima volta che serve (primo scatto della rotella sulla
//! pillola o prima apertura del pannello) e poi resta, fermo su `recv()`: nessun sondaggio, le
//! callback di Windows a riposo non costano nulla.

// Fuori da Windows, nei programmi (non nei test) la parte pura non ha chi la usa: non si compila.
#[cfg(windows)]
use std::time::Duration;

/// Quanti pixel logici di rotella fanno un punto percentuale. Uno scatto della rotella vale
/// 60 px in Slint (winit `LineDelta` × 60): 2 punti a scatto, come TwoCode. **Da tarare dal vivo.**
#[cfg(any(windows, test))]
pub const PX_PER_PUNTO: f32 = 30.0;

/// Quanto resta la barra del volume al posto degli occhi dopo l'ultimo cambio (la usa solo il
/// ponte, su Windows).
#[cfg(windows)]
pub const BARRA_PER: Duration = Duration::from_millis(1200);

/// Il volume del dispositivo di uscita predefinito: livello 0.0..=1.0 e muto.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatoVolume {
    pub livello: f32,
    pub muto: bool,
}

/// Il volume si può leggere e comandare su questo sistema (su Windows; se poi c'è un dispositivo
/// lo dice il primo `StatoVolume`).
pub fn disponibile() -> bool {
    cfg!(windows)
}

/// Il livello in punti percentuali interi (quello che si mostra e si dice a Narrator).
#[cfg(any(windows, test))]
pub fn percento(livello: f32) -> i32 {
    (livello.clamp(0.0, 1.0) * 100.0).round() as i32
}

/// La rotella accumulata: i pixel che non fanno ancora un punto restano per l'evento dopo (un
/// touchpad manda tanti piccoli passi). Cambiando verso il resto si butta: in giù dopo un mezzo
/// passo in su si scende subito.
#[cfg(any(windows, test))]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Rotella {
    resto: f32,
}

#[cfg(any(windows, test))]
impl Rotella {
    /// Uno spostamento della rotella (in su = positivo) in punti percentuali interi.
    pub fn scorri(&mut self, delta_px: f32) -> i32 {
        if self.resto * delta_px < 0.0 {
            self.resto = 0.0;
        }
        let totale = self.resto + delta_px;
        let punti = (totale / PX_PER_PUNTO).trunc();
        self.resto = totale - punti * PX_PER_PUNTO;
        punti as i32
    }
}

/// Il livello `attuale` spostato di `punti` percentuali, nei limiti 0.0..=1.0. Si parte dal
/// punto intero più vicino: a forza di passi il livello non va alla deriva.
#[cfg(any(windows, test))]
pub fn nuovo_volume(attuale: f32, punti: i32) -> f32 {
    ((percento(attuale) + punti) as f32 / 100.0).clamp(0.0, 1.0)
}

/// Chi comanda il volume: `Audio` su Windows, un finto nei test.
#[cfg(any(windows, test))]
pub trait Volume {
    fn imposta(&self, livello: f32);
    fn muto(&self, si: bool);
}

/// Uno spostamento della rotella sulla pillola, con l'ultimo stato letto (`None`: nessun
/// dispositivo, o non ancora letto: la rotella non fa nulla e non accumula). Restituisce lo stato
/// da mostrare nella barra se la rotella ha fatto almeno un punto (anche al limite, dove il
/// livello non cambia ma la barra si vede). In su da muto l'audio torna, come coi tasti del volume
/// di Windows; in giù resta muto.
#[cfg(any(windows, test))]
pub fn muovi(rotella: &mut Rotella, stato: Option<StatoVolume>, delta_px: f32, volume: &dyn Volume) -> Option<StatoVolume> {
    let stato = stato?;
    let punti = rotella.scorri(delta_px);
    if punti == 0 {
        return None;
    }
    let livello = nuovo_volume(stato.livello, punti);
    if percento(livello) != percento(stato.livello) {
        volume.imposta(livello);
    }
    let muto = stato.muto && punti < 0;
    if stato.muto && !muto {
        volume.muto(false);
    }
    Some(StatoVolume { livello, muto })
}

#[cfg(windows)]
pub use com::Audio;

#[cfg(windows)]
mod com {
    use super::{StatoVolume, Volume};
    use std::sync::mpsc::{self, Receiver, Sender};
    use windows::core::{Result, PCWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Media::Audio::Endpoints::{
        IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
    };
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
        MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows_core::implement;

    /// Chi riceve ogni stato letto (dal thread `audio` o dal thread di Windows di `OnNotify`):
    /// deve solo accodarlo al thread dell'interfaccia.
    type Consegna = fn(Option<StatoVolume>);

    enum Comando {
        Imposta(f32),
        Muto(bool),
        /// Il dispositivo predefinito è cambiato (o sparito): ci si ricollega.
        Ricollega,
        Ferma,
    }

    /// Il thread `audio`: riceve i comandi da un canale e consegna gli stati.
    pub struct Audio {
        invia: Sender<Comando>,
    }

    impl Audio {
        /// Avvia il thread `audio`: il primo stato (o `None` senza dispositivo) arriva appena
        /// collegato, poi uno a ogni cambio di volume, muto o dispositivo.
        pub fn avvia(consegna: Consegna) -> Audio {
            let (invia, ricevi) = mpsc::channel();
            let per_le_notifiche = invia.clone();
            let avviato = std::thread::Builder::new()
                .name("audio".into())
                .spawn(move || gira(consegna, per_le_notifiche, ricevi));
            if let Err(e) = avviato {
                eprintln!("volume del PC non avviato: {e}");
            }
            Audio { invia }
        }
    }

    impl Volume for Audio {
        fn imposta(&self, livello: f32) {
            let _ = self.invia.send(Comando::Imposta(livello));
        }

        fn muto(&self, si: bool) {
            let _ = self.invia.send(Comando::Muto(si));
        }
    }

    impl Drop for Audio {
        /// Il thread cancella le sue registrazioni, chiude COM e finisce (nessuna attesa qui).
        fn drop(&mut self) {
            let _ = self.invia.send(Comando::Ferma);
        }
    }

    /// `IAudioEndpointVolumeCallback`: volume o muto cambiati, da chiunque (anche dai tasti).
    #[implement(IAudioEndpointVolumeCallback)]
    struct Richiamo {
        consegna: Consegna,
    }

    impl IAudioEndpointVolumeCallback_Impl for Richiamo_Impl {
        fn OnNotify(&self, dati: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
            // SAFETY: Windows passa la struttura della notifica, valida per tutta la chiamata;
            // un puntatore nullo si ignora.
            if let Some(d) = unsafe { dati.as_ref() } {
                (self.consegna)(Some(StatoVolume { livello: d.fMasterVolume, muto: d.bMuted.as_bool() }));
            }
            Ok(())
        }
    }

    /// `IMMNotificationClient`: conta solo il cambio del dispositivo predefinito, che si segnala
    /// al thread `audio` (vedi i vincoli in testa al modulo).
    #[implement(IMMNotificationClient)]
    struct Dispositivi {
        invia: Sender<Comando>,
    }

    impl IMMNotificationClient_Impl for Dispositivi_Impl {
        fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
            Ok(())
        }

        fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
            Ok(())
        }

        fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
            Ok(())
        }

        fn OnDefaultDeviceChanged(&self, flusso: EDataFlow, ruolo: ERole, _: &PCWSTR) -> Result<()> {
            if flusso == eRender && ruolo == eMultimedia {
                let _ = self.invia.send(Comando::Ricollega);
            }
            Ok(())
        }

        fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
            Ok(())
        }
    }

    fn gira(consegna: Consegna, invia: Sender<Comando>, ricevi: Receiver<Comando>) {
        // SAFETY: COM su questo thread, nell'appartamento multithread; chiuso alla fine con
        // `CoUninitialize`, dopo che `lavora` ha rilasciato ogni oggetto COM.
        if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
            eprintln!("volume del PC: COM non inizializzato: {e}");
            consegna(None);
            return;
        }
        lavora(consegna, invia, &ricevi);
        // SAFETY: bilancia il `CoInitializeEx` riuscito qui sopra, sullo stesso thread.
        unsafe { CoUninitialize() };
    }

    fn lavora(consegna: Consegna, invia: Sender<Comando>, ricevi: &Receiver<Comando>) {
        // SAFETY: COM è inizializzato su questo thread (`gira`).
        let enumeratore: IMMDeviceEnumerator = match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(e) => e,
            Err(e) => {
                eprintln!("volume del PC: elenco dei dispositivi audio non disponibile: {e}");
                consegna(None);
                return;
            }
        };
        let dispositivi: IMMNotificationClient = Dispositivi { invia }.into();
        // SAFETY: enumeratore e callback vivi; cancellata prima di rilasciarli, qui sotto.
        let registrato = unsafe { enumeratore.RegisterEndpointNotificationCallback(&dispositivi) };
        if let Err(e) = &registrato {
            eprintln!("volume del PC: il cambio di dispositivo non si seguirà: {e}");
        }
        let richiamo: IAudioEndpointVolumeCallback = Richiamo { consegna }.into();
        let mut attuale = collega(&enumeratore, &richiamo, consegna);

        // Fermo qui finché non arriva un comando: nessun sondaggio.
        while let Ok(comando) = ricevi.recv() {
            match comando {
                Comando::Imposta(livello) => {
                    if let Some(v) = &attuale {
                        // SAFETY: `v` è vivo; contesto nullo (`GUID_NULL` nella notifica).
                        if let Err(e) = unsafe { v.SetMasterVolumeLevelScalar(livello.clamp(0.0, 1.0), std::ptr::null()) } {
                            eprintln!("volume del PC non cambiato: {e}");
                        }
                    }
                }
                Comando::Muto(si) => {
                    if let Some(v) = &attuale {
                        // SAFETY: come sopra. `S_FALSE` (già in quello stato) non è un errore.
                        if let Err(e) = unsafe { v.SetMute(si, std::ptr::null()) } {
                            eprintln!("muto del PC non cambiato: {e}");
                        }
                    }
                }
                Comando::Ricollega => {
                    scollega(attuale.take(), &richiamo);
                    attuale = collega(&enumeratore, &richiamo, consegna);
                }
                Comando::Ferma => break,
            }
        }

        scollega(attuale.take(), &richiamo);
        if registrato.is_ok() {
            // SAFETY: lo stesso callback registrato sopra, fuori da ogni callback.
            let _ = unsafe { enumeratore.UnregisterEndpointNotificationCallback(&dispositivi) };
        }
    }

    /// Il volume del dispositivo predefinito di adesso, con la notifica registrata, e il suo
    /// primo stato consegnato; `None` (e `consegna(None)`) se non c'è un'uscita audio.
    fn collega(
        enumeratore: &IMMDeviceEnumerator,
        richiamo: &IAudioEndpointVolumeCallback,
        consegna: Consegna,
    ) -> Option<IAudioEndpointVolume> {
        // SAFETY: enumeratore vivo su questo thread COM; il dispositivo si rilascia da solo.
        let volume = unsafe { enumeratore.GetDefaultAudioEndpoint(eRender, eMultimedia) }
            .and_then(|d| unsafe { d.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) });
        let volume = match volume {
            Ok(v) => v,
            Err(e) => {
                // Di solito E_NOTFOUND: nessuna uscita audio.
                eprintln!("volume del PC: nessun dispositivo di uscita ({})", e.code());
                consegna(None);
                return None;
            }
        };
        // Prima la notifica, poi la lettura: un cambio fra le due non va perso.
        // SAFETY: `volume` e `richiamo` vivi; la registrazione si cancella in `scollega`.
        if let Err(e) = unsafe { volume.RegisterControlChangeNotify(richiamo) } {
            eprintln!("volume del PC: i cambi da fuori non si vedranno: {e}");
        }
        // SAFETY: `volume` vivo.
        let letto = unsafe { (volume.GetMasterVolumeLevelScalar(), volume.GetMute()) };
        match letto {
            (Ok(livello), Ok(muto)) => consegna(Some(StatoVolume { livello, muto: muto.as_bool() })),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("volume del PC non letto: {e}");
                consegna(None);
            }
        }
        Some(volume)
    }

    /// Cancella la notifica prima di rilasciare il volume (vedi in testa al modulo).
    fn scollega(volume: Option<IAudioEndpointVolume>, richiamo: &IAudioEndpointVolumeCallback) {
        if let Some(v) = volume {
            // SAFETY: lo stesso callback registrato in `collega`, fuori da ogni callback.
            let _ = unsafe { v.UnregisterControlChangeNotify(richiamo) };
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn uno_scatto_due_punti() {
        assert_eq!(Rotella::default().scorri(60.0), 2);
        assert_eq!(Rotella::default().scorri(-60.0), -2);
    }

    #[test]
    fn il_resto_si_conserva() {
        let mut r = Rotella::default();
        assert_eq!(r.scorri(20.0), 0);
        assert_eq!(r.scorri(20.0), 1);
        // Cambiando verso il resto (10 px in su) si butta: 30 px in giù sono un punto.
        assert_eq!(r.scorri(-30.0), -1);
        assert_eq!(Rotella::default().scorri(-30.0), -1);
    }

    #[test]
    fn volume_nei_limiti() {
        assert_eq!(nuovo_volume(0.99, 5), 1.0);
        assert_eq!(nuovo_volume(0.01, -5), 0.0);
        assert!((nuovo_volume(0.40, 2) - 0.42).abs() < 1e-6);
        // Si parte dal punto intero più vicino.
        assert!((nuovo_volume(0.404, 2) - 0.42).abs() < 1e-6);
        assert_eq!(percento(0.425), 43);
    }

    /// Il finto `Audio`: annota i comandi, non tocca nulla.
    #[derive(Default)]
    struct Finto {
        comandi: RefCell<Vec<String>>,
    }

    impl Volume for Finto {
        fn imposta(&self, livello: f32) {
            self.comandi.borrow_mut().push(format!("imposta {}", percento(livello)));
        }

        fn muto(&self, si: bool) {
            self.comandi.borrow_mut().push(format!("muto {si}"));
        }
    }

    #[test]
    fn senza_dispositivo_audio_niente_volume() {
        let finto = Finto::default();
        let mut r = Rotella::default();
        assert_eq!(muovi(&mut r, None, 60.0, &finto), None);
        assert_eq!(muovi(&mut r, None, -600.0, &finto), None);
        assert!(finto.comandi.borrow().is_empty(), "nessun comando senza dispositivo");
        assert_eq!(r, Rotella::default(), "e nessun accumulo");
        // Col dispositivo: uno scatto in su, due punti.
        let stato = Some(StatoVolume { livello: 0.40, muto: false });
        let nuovo = muovi(&mut r, stato, 60.0, &finto).expect("uno scatto");
        assert!((nuovo.livello - 0.42).abs() < 1e-6 && !nuovo.muto);
        assert_eq!(*finto.comandi.borrow(), ["imposta 42"]);
    }

    #[test]
    fn al_limite_la_barra_si_vede_ma_nulla_cambia() {
        let finto = Finto::default();
        let mut r = Rotella::default();
        let pieno = Some(StatoVolume { livello: 1.0, muto: false });
        assert_eq!(muovi(&mut r, pieno, 60.0, &finto), pieno);
        assert!(finto.comandi.borrow().is_empty());
        // Mezzo scatto: niente barra.
        assert_eq!(muovi(&mut Rotella::default(), pieno, 10.0, &finto), None);
    }

    #[test]
    fn in_su_da_muto_l_audio_torna() {
        let finto = Finto::default();
        let muto = Some(StatoVolume { livello: 0.30, muto: true });
        let giu = muovi(&mut Rotella::default(), muto, -60.0, &finto).expect("in giù");
        assert!(giu.muto, "in giù resta muto");
        let su = muovi(&mut Rotella::default(), muto, 60.0, &finto).expect("in su");
        assert!(!su.muto);
        assert_eq!(*finto.comandi.borrow(), ["imposta 28", "imposta 32", "muto false"]);
    }

    /// Sulla macchina vera: avvia `Audio`, aspetta il primo stato e **non cambia nulla** (nessun
    /// `imposta` né `muto`). `cargo test -p assistente -- --ignored legge_il_volume_attuale`.
    #[cfg(windows)]
    #[test]
    #[ignore = "legge il volume vero del PC (senza cambiarlo)"]
    fn legge_il_volume_attuale() {
        use std::sync::Mutex;
        use std::time::Instant;
        static LETTI: Mutex<Vec<Option<StatoVolume>>> = Mutex::new(Vec::new());
        fn consegna(s: Option<StatoVolume>) {
            if let Ok(mut l) = LETTI.lock() {
                l.push(s);
            }
        }
        let audio = Audio::avvia(consegna);
        let inizio = Instant::now();
        let primo = loop {
            if let Some(s) = LETTI.lock().ok().and_then(|l| l.first().copied()) {
                break s;
            }
            assert!(inizio.elapsed() < Duration::from_secs(5), "nessuno stato in 5 s");
            std::thread::sleep(Duration::from_millis(20));
        };
        match primo {
            Some(s) => {
                assert!((0.0..=1.0).contains(&s.livello), "livello fuori scala: {}", s.livello);
                eprintln!("volume attuale: {}%{}", percento(s.livello), if s.muto { ", muto" } else { "" });
            }
            None => eprintln!("nessun dispositivo di uscita audio"),
        }
        drop(audio);
    }
}
