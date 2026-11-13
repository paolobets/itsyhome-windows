// In release niente console: l'assistente vive solo nella pillola.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod avvio;
mod finestre;
mod layout;
mod orologio;
mod pannello;
mod pc_stats;
mod pillola;
mod ponte;
mod registro;
mod scelte;
mod umore;

use casa_core::config::ConfigStore;
use casa_core::segreti::KeyringSegreti;
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

/// Messaggio finto mostrato con `--umore alert`, per vedere la pillola allargata.
const MESSAGGIO_DI_PROVA: &str = "Lavatrice finita";

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
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("skia-software".into())
        .with_winit_window_attributes_hook(|attributi| {
            #[cfg(windows)]
            let attributi = {
                use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
                attributi.with_skip_taskbar(true)
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
    let segreti = Arc::new(KeyringSegreti);
    let errore_import = avvio::importa_se_serve(&cartella, v2.as_deref(), segreti.as_ref());
    if let Some(e) = &errore_import {
        eprintln!("{e}");
    }
    let casa = Rc::new(CasaService::avvia(Arc::new(ConfigStore::apri(&cartella)), segreti, ponte::ascoltatore()));

    let pillola = Pillola::new()?;
    debug_assert_eq!(pillola.get_larghezza_finestra(), finestre::LARGHEZZA_FINESTRA_PILLOLA);
    debug_assert_eq!(pillola.get_margine_aura(), finestre::MARGINE_AURA);
    debug_assert_eq!(pillola.get_scarto_occhi_allerta(), finestre::SCARTO_OCCHI_ALLERTA);

    let argomenti: Vec<String> = std::env::args().skip(1).collect();
    let forzato = umore_forzato(argomenti.iter().cloned());
    if let Some(u) = forzato {
        let messaggio = if u == Umore::TiCerca { MESSAGGIO_DI_PROVA } else { "" };
        pillola::imposta_umore(&pillola, u, messaggio);
    }
    let cervello = Rc::new(RefCell::new(Cervello::new(Instant::now())));
    let ponte = ponte::collega(casa.clone(), &pillola, cervello, forzato.is_some(), errore_import);

    let apertura = pannello::Apertura::collega(&pillola, ponte);
    finestre::segui_schermo(&pillola, pillola::al_cambio_schermo);

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

    casa.connetti_attivo();
    pillola.run()
}
