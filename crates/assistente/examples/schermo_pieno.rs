//! Finestra di prova a schermo intero, solo per misurare quanto la pillola ci mette a sparire e a
//! tornare (Task 5, M2). Non è un test automatico: è un processo che si lancia a mano o dal test,
//! mai contro un'assistente di cui non si è padroni, e non manda alcun input ad app altrui.
//!
//! `cargo run --example schermo_pieno -- [--monitor <n>] [--per <s>] [--dopo <s>]`: finestra senza
//! bordi che entra a schermo intero sul monitor `n` (0 = primo; se manca, quello corrente) appena
//! compare, ci resta `s` secondi (8 se manca) e si chiude. Stampa l'elenco dei monitor con
//! l'indice (l'ordine non è garantito uguale fra un avvio e l'altro).
//!
//! Perché subito: la finestra che compare già a schermo intero diventa la finestra in primo piano
//! in quel momento, e il gancio del primo piano della pillola scatta (la pillola sparisce dopo
//! 3 s). Una finestra già in primo piano che poi si allarga NON cambia il primo piano (come l'F11
//! di un browser): lo coglie solo il controllo da 5 s, quindi la pillola può metterci fino a 8 s.
//! `--dopo <s>` (di serie 0) ritarda lo schermo intero di `s` secondi per riprodurre quel caso.

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
    let dopo = Duration::from_secs(numero(&argomenti, "--dopo").unwrap_or(0));

    let finestra = Prova::new()?;
    finestra.show()?;
    let debole = finestra.as_weak();
    // La finestra di sistema nasce col ciclo eventi: si aspetta che ci sia, poi si va a schermo intero.
    slint::spawn_local(async move {
        let Some(f) = debole.upgrade() else { return };
        if f.window().winit_window().await.is_err() {
            eprintln!("finestra di sistema non creata");
            let _ = slint::quit_event_loop();
            return;
        }
        let a_schermo_intero = move || {
            let Some(f) = debole.upgrade() else { return };
            f.window().with_winit_window(|w| {
                for (i, m) in w.available_monitors().enumerate() {
                    let (p, d) = (m.position(), m.size());
                    eprintln!("monitor {i}: {}x{} in ({}, {})", d.width, d.height, p.x, p.y);
                }
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
        };
        if dopo.is_zero() {
            a_schermo_intero();
        } else {
            slint::Timer::single_shot(dopo, a_schermo_intero);
        }
    })
    .expect("ciclo eventi di Slint");
    slint::run_event_loop_until_quit()?;
    Ok(())
}
