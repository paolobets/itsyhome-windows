// In release niente console: l'assistente vive solo nella pillola.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod attenzione;
mod avvio;
mod collegamenti;
mod diagnosi;
mod dispositivi;
mod finestra_impostazioni;
mod finestre;
mod gestione;
mod impostazioni;
mod layout;
mod orologio;
mod ottimismo;
mod pannello;
mod pc_stats;
mod pillola;
mod ponte;
mod registro;
mod scelte;
mod sistema;
mod stanze;
mod umore;
#[cfg(test)]
mod test_ui;

use casa_core::config::ConfigStore;
use casa_core::segreti::{KeyringSegreti, Segreti};
use casa_core::servizio::CasaService;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use umore::{Cervello, Umore};

slint::include_modules!();

/// Titolo della finestra della pillola: dice al gancio di winit quale finestra non deve
/// prendersi il fuoco quando ricompare.
const TITOLO_PILLOLA: &str = "ItsyHome";

/// `--umore <calm|think|alert|happy|sleep>`: forza un umore (solo per prova).
fn umore_forzato(mut argomenti: impl Iterator<Item = String>) -> Option<Umore> {
    while let Some(a) = argomenti.next() {
        if a == "--umore" {
            return match argomenti.next()?.as_str() {
                "calm" => Some(Umore::Tranquillo),
                "think" => Some(Umore::Pensa),
                "alert" => Some(Umore::TiCerca),
                "happy" => Some(Umore::Contento),
                "sleep" => Some(Umore::Dorme),
                _ => None,
            };
        }
    }
    None
}

fn main() -> Result<(), slint::PlatformError> {
    // Istanza unica: se ce n'è già una, la si avvisa (apre il pannello) e si esce subito.
    let nome_istanza = sistema::istanza::nome(std::env::var_os(avvio::VAR_CARTELLA).as_deref());
    let Some(istanza) = sistema::istanza::prendi(&nome_istanza) else {
        return Ok(());
    };

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("skia-software".into())
        .with_winit_window_attributes_hook(|attributi| {
            // Pillola e pannello fuori dalla barra delle applicazioni; la finestra delle
            // Impostazioni no: è una finestra normale, che si può perdere dietro alle altre.
            #[cfg(windows)]
            let attributi = {
                use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
                let skip = attributi.title != finestra_impostazioni::TITOLO;
                attributi.with_skip_taskbar(skip)
            };
            // La pillola ricompare quando il pannello si chiude (spesso per un clic su un'altra
            // finestra): non deve rubare il fuoco a quella finestra.
            if attributi.title == TITOLO_PILLOLA {
                attributi.with_active(false)
            } else {
                attributi
            }
        })
        .select()?;

    // Configurazione: cartella di prova se c'è la variabile (e allora niente import dalla v2),
    // altrimenti quella vera; al primo avvio si importa la v2 e si salva subito.
    let (cartella, v2) = avvio::cartelle(std::env::var_os(avvio::VAR_CARTELLA));
    let segreti: Arc<dyn Segreti> = Arc::new(KeyringSegreti);
    let errore_import = avvio::importa_se_serve(&cartella, v2.as_deref(), segreti.as_ref());
    if let Some(e) = &errore_import {
        eprintln!("{e}");
    }
    // Registro diagnostico (solo contatori), acceso con `ITSYHOME_DIAGNOSI=1`: `diagnosi.log`
    // accanto alla configurazione.
    let registro = Arc::new(casa_core::diagnosi::Diagnosi::new(diagnosi::acceso(
        std::env::var_os(diagnosi::VARIABILE).as_deref(),
    )));
    if registro.acceso() {
        diagnosi::avvia(registro.clone(), &cartella);
    }
    let casa = Rc::new(CasaService::avvia_con_diagnosi(
        Arc::new(ConfigStore::apri(&cartella)),
        segreti.clone(),
        ponte::ascoltatore(),
        registro,
    ));
    // Prove locali: avvio con il sistema in una chiave di prova; identificativo dei toast solo se
    // la prova lo chiede (`ITSYHOME_PROVA_NOTIFICHE=1`).
    let in_prova = std::env::var_os(avvio::VAR_CARTELLA).is_some_and(|v| !v.is_empty());
    if in_prova {
        sistema::autoavvio::usa_chiavi_di_prova();
    }
    if !in_prova || std::env::var_os("ITSYHOME_PROVA_NOTIFICHE").is_some_and(|v| v == "1") {
        if let Err(e) = sistema::notifiche::registra(&cartella) {
            eprintln!("{e}");
        }
    }
    // Avvio con il sistema: si legge com'è davvero (anche spento da Gestione attività) e la
    // configurazione lo segue; non si riaccende mai da solo (al più si aggiorna il percorso).
    sincronizza_avvio(&casa);

    let pillola = Pillola::new()?;
    pillola.global::<Movimento>().set_ridotto(sistema::movimento::ridotto());
    // Le misure della pillola in Rust (finestre.rs) e in Slint coincidono: lo controlla il test
    // `test_ui::misure_slint_e_rust`, in `cargo test` e senza finestre.

    let argomenti: Vec<String> = std::env::args().skip(1).collect();
    let forzato = umore_forzato(argomenti.iter().cloned());
    let cervello = Rc::new(RefCell::new(Cervello::new(Instant::now())));
    let ponte = ponte::collega(casa.clone(), segreti, &pillola, cervello, forzato, errore_import);
    let ponte_servizi = ponte.clone();
    // `--prova-avviso <bloccante|attenzione|informazione> [--dopo <s>] [--lungo]`: una voce finta
    // con 2 pulsanti finti, subito o dopo `s` secondi (per provarla mentre la pillola è ritirata);
    // `--lungo`: titolo tagliato nella pillola e testo da scorrere nel pannello.
    if let Some(livello) = attenzione::avviso_di_prova(argomenti.iter().cloned()) {
        let lungo = argomenti.iter().any(|a| a == "--lungo");
        match attenzione::secondi_dopo(argomenti.iter().cloned()) {
            Some(s) => slint::Timer::single_shot(std::time::Duration::from_secs(s), move || {
                if let Some(p) = ponte::ponte() {
                    p.prova_avviso(livello, lungo);
                }
            }),
            None => ponte.prova_avviso(livello, lungo),
        }
    }
    // La batteria si controlla ogni 60 s anche a pannello chiuso (unica eccezione al campionamento
    // solo a pannello aperto): un thread che dorme, niente su senza batteria.
    pc_stats::sorveglia_batteria(|lettura| {
        slint::invoke_from_event_loop(move || ponte::batteria(lettura)).is_ok()
    });

    let apertura = pannello::Apertura::collega(&pillola, ponte);
    finestre::segui_schermo(&pillola, pillola::al_cambio_schermo);
    sistema::servizi::avvia(&pillola, apertura.clone(), ponte_servizi);
    // Un secondo avvio suona il campanello: qui si apre il pannello.
    istanza.su_richiamo(|| {
        let _ = slint::invoke_from_event_loop(sistema::servizi::apri);
    });

    // `--apri`: pannello aperto all'avvio e fisso, cioè non si chiude perdendo il fuoco
    // (misure e foto mentre il PC si usa); Esc e gli occhi lo chiudono.
    if argomenti.iter().any(|a| a == "--apri") {
        let debole = pillola.as_weak();
        let apertura = apertura.clone();
        let _ = slint::spawn_local(async move {
            let Some(p) = debole.upgrade() else { return };
            if p.window().winit_window().await.is_ok() {
                apertura.apri_fisso();
            }
        });
    }

    // `--prova-gestione`: la finestra di gestione con una mail di prova (aperta, chiusa, riaperta);
    // `--prova-gestione-cicli <n>`: n aperture e chiusure, alla fine chiusa.
    // Un numero mancante o non valido lo si dice (la prova non parte), invece di ignorarlo.
    let cicli = argomenti.iter().position(|a| a == "--prova-gestione-cicli").and_then(|i| {
        let valore = argomenti.get(i + 1).map(String::as_str).unwrap_or("");
        let numero = valore.parse::<u32>().ok();
        if numero.is_none() {
            eprintln!("--prova-gestione-cicli vuole un numero di cicli (es. 5), non «{valore}»: prova non avviata");
        }
        numero
    });
    if cicli.is_some() || argomenti.iter().any(|a| a == "--prova-gestione") {
        gestione::prova(&cartella, cicli);
    }

    casa.connetti_attivo();
    // Il ciclo eventi vive anche senza finestre visibili (pillola nascosta da uno schermo intero
    // a pannello chiuso): finisce solo con «Esci» (menu «⋯» del pannello o della tray).
    let esito = pillola.show().and_then(|_| slint::run_event_loop_until_quit());
    // Sempre, anche se mostrare la pillola o il ciclo eventi non sono riusciti. In ordine:
    // servizi (tray, scorciatoie), finestra di gestione (processo raccolto), ponte, pannello; il `CasaService` si chiude qui, con il suo
    // thread ancora vivo, quando `casa` esce di scena.
    sistema::servizi::spegni();
    gestione::spegni();
    ponte::spegni();
    drop(apertura);
    let _ = pillola.hide();
    esito
}

/// L'interruttore «Avvia con il sistema» segue il registro (vedi `sistema::autoavvio`).
fn sincronizza_avvio(casa: &CasaService) {
    match sistema::autoavvio::all_avvio() {
        Ok(attivo) if casa.config().launch_at_login != attivo => casa.aggiorna_config(|c| c.launch_at_login = attivo),
        Ok(_) => {}
        Err(e) => eprintln!("{e}"),
    }
}
