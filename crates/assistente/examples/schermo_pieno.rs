//! Finestra di prova a schermo intero, solo per misurare quanto la pillola ci mette a sparire e a
//! tornare (Task 5, M2). Non è un test automatico: è un processo che si lancia a mano o dal test,
//! mai contro un'assistente di cui non si è padroni, e non manda alcun input ad app altrui.
//!
//! `cargo run --example schermo_pieno -- [--monitor <n>] [--per <s>]`: finestra senza bordi che
//! dopo 2 s va a schermo intero sul monitor `n` (0 = primo; se manca prende quello di partenza),
//! ci resta `s` secondi (8 se manca) e si chiude.

use slint::winit_030::{winit, WinitWindowAccessor};
use std::time::Duration;
use winit::window::Fullscreen;

slint::slint! {
    export component Prova inherits Window {
        title: "Prova schermo intero";
        no-frame: true;
        preferred-width: 400px;
        preferred-height: 300px;
        background: black;
    }
}

/// Il valore numerico che segue `nome` fra gli argomenti.
fn numero(argomenti: &[String], nome: &str) -> Option<u64> {
    let i = argomenti.iter().position(|a| a == nome)?;
    argomenti.get(i + 1)?.parse().ok()
}

fn main() -> Result<(), slint::PlatformError> {
    let argomenti: Vec<String> = std::env::args().skip(1).collect();
    let monitor = numero(&argomenti, "--monitor").map(|n| n as usize);
    let per = Duration::from_secs(numero(&argomenti, "--per").unwrap_or(8));

    let finestra = Prova::new()?;
    finestra.show()?;
    let debole = finestra.as_weak();
    slint::Timer::single_shot(Duration::from_secs(2), move || {
        let Some(f) = debole.upgrade() else { return };
        f.window().with_winit_window(|w| {
            let scelto = match monitor {
                Some(n) => w.available_monitors().nth(n),
                None => w.current_monitor(),
            };
            if scelto.is_none() && monitor.is_some() {
                eprintln!("monitor non trovato: la finestra resta com'e'");
                return;
            }
            w.set_fullscreen(Some(Fullscreen::Borderless(scelto)));
        });
        slint::Timer::single_shot(per, || {
            let _ = slint::quit_event_loop();
        });
    });
    slint::run_event_loop_until_quit()?;
    Ok(())
}
