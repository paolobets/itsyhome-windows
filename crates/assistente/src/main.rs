// In release niente console: l'assistente vive solo nella pillola.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod finestre;
// Registro delle app e layout servono al pannello (task 6+): fino ad allora non hanno chiamanti.
#[allow(dead_code)]
mod layout;
#[allow(dead_code)]
mod registro;
mod umore;

use std::time::Instant;
use umore::{Cervello, Umore};

slint::include_modules!();

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
            attributi
        })
        .select()?;

    let ui = Pillola::new()?;

    let (umore, messaggio) = match umore_forzato(std::env::args().skip(1)) {
        Some(u) => (u, if u == Umore::TiCerca { MESSAGGIO_DI_PROVA } else { "" }.to_owned()),
        None => {
            // Nessuno alimenta ancora il cervello (arriverà con casa e notifiche): resta tranquillo.
            let ora = Instant::now();
            let cervello = Cervello::new(ora);
            (cervello.umore(ora), cervello.messaggio().unwrap_or_default().to_owned())
        }
    };
    ui.set_umore(umore.come_stringa().into());
    ui.set_messaggio(messaggio.into());

    // Il clic aprirà il pannello (task 6).
    ui.on_cliccata(|| {});

    finestre::segui_schermo(&ui, |ui| ui.get_larghezza_finestra());
    ui.run()
}
