// In release niente console: l'assistente vive solo nella pillola.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod finestre;
mod layout;
mod orologio;
mod pannello;
mod pc_stats;
mod pillola;
mod registro;
mod scelte;
mod umore;

use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
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

    let pillola = Pillola::new()?;
    debug_assert_eq!(pillola.get_larghezza_finestra(), finestre::LARGHEZZA_FINESTRA_PILLOLA);
    debug_assert_eq!(pillola.get_margine_aura(), finestre::MARGINE_AURA);

    let argomenti: Vec<String> = std::env::args().skip(1).collect();
    let (umore, messaggio) = match umore_forzato(argomenti.iter().cloned()) {
        Some(u) => (u, if u == Umore::TiCerca { MESSAGGIO_DI_PROVA } else { "" }.to_owned()),
        None => {
            // Nessuno alimenta ancora il cervello (arriverà con casa e notifiche): resta tranquillo.
            let ora = Instant::now();
            let cervello = Cervello::new(ora);
            (cervello.umore(ora), cervello.messaggio().unwrap_or_default().to_owned())
        }
    };
    pillola::imposta_umore(&pillola, umore, &messaggio);

    let apertura = pannello::Apertura::collega(&pillola);
    finestre::segui_schermo(&pillola, pillola::al_cambio_schermo);

    // `--apri`: pannello aperto all'avvio (misure e foto senza mouse).
    if argomenti.iter().any(|a| a == "--apri") {
        let debole = pillola.as_weak();
        let apertura = apertura.clone();
        let _ = slint::spawn_local(async move {
            let Some(p) = debole.upgrade() else { return };
            if p.window().winit_window().await.is_ok() {
                apertura.apri();
            }
        });
    }

    pillola.run()
}
