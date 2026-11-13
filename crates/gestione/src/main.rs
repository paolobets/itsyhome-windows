//! `itsyhome-gestione`: la finestra di gestione dell'assistente, in un processo a parte.
//!
//! Nasce alla prima `apri` dell'assistente e muore alla `chiudi` o alla chiusura della finestra.
//! Parla solo su stdin/stdout (una riga JSON per messaggio, vedi `gestione-protocollo`); se stdin
//! si chiude (l'assistente è uscito o è morto) esce da sola. Non riceve token né configurazione:
//! solo la cartella dei dati del motore web (`--dati <cartella>`).
//!
//! Cornice fidata disegnata in Slint (stesso stile dell'assistente); il contenuto (HTML ostile)
//! sta in un riquadro wry figlio della finestra Slint: JavaScript spento, devtools spenti,
//! navigazione bloccata, nuove finestre, scaricamenti e permessi negati, menu contestuale e
//! scorciatoie del browser spenti. Se il motore web manca (o con `--senza-web`, per prova), piano
//! B: il contenuto come testo semplice.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sicurezza;
mod testo;

use gestione_protocollo::{messaggio_da_riga, riga_da_messaggio, Messaggio};
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{NewWindowResponse, PageLoadEvent, PermissionResponse, Rect, WebContext, WebView, WebViewBuilder};

slint::include_modules!();

/// Stato della finestra, solo sul thread dell'interfaccia.
struct Stato {
    cornice: Cornice,
    dati: PathBuf,
    senza_web: bool,
    contesto: Option<WebContext>,
    web: Option<WebView>,
    /// Vero dopo il caricamento del contenuto corrente: da lì ogni navigazione è bloccata.
    caricato: Rc<Cell<bool>>,
    /// Il riquadro web è in costruzione (si aspetta la finestra di winit).
    in_costruzione: bool,
    /// Contenuto arrivato mentre il riquadro si costruisce.
    in_attesa: Option<String>,
}

thread_local! {
    static STATO: RefCell<Option<Stato>> = const { RefCell::new(None) };
}

/// Una riga verso l'assistente. Se lo stdout è chiuso l'assistente non c'è più: lo stdin si
/// chiuderà a sua volta e si esce da lì.
fn invia(m: &Messaggio) {
    let mut uscita = std::io::stdout().lock();
    let _ = writeln!(uscita, "{}", riga_da_messaggio(m));
    let _ = uscita.flush();
}

fn valore_di(argomenti: &[String], nome: &str) -> Option<String> {
    argomenti.iter().position(|a| a == nome).and_then(|i| argomenti.get(i + 1)).cloned()
}

fn main() {
    let argomenti: Vec<String> = std::env::args().skip(1).collect();
    let dati = valore_di(&argomenti, "--dati")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("itsyhome-gestione-webview"));
    let senza_web = argomenti.iter().any(|a| a == "--senza-web");

    if let Err(e) = avvia(dati, senza_web) {
        eprintln!("gestione: finestra non avviata: {e}");
    }
    // Sempre, anche dopo un errore: l'assistente sa che la finestra non c'è più.
    invia(&Messaggio::Chiusa);
}

fn avvia(dati: PathBuf, senza_web: bool) -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("skia-software".into())
        .select()?;

    let cornice = Cornice::new()?;
    cornice.on_chiudi(|| {
        let _ = slint::quit_event_loop();
    });
    let debole = cornice.as_weak();
    cornice.on_trascina(move || {
        if let Some(c) = debole.upgrade() {
            let _ = c.window().with_winit_window(|w| w.drag_window());
        }
    });
    STATO.with_borrow_mut(|s| {
        *s = Some(Stato {
            cornice,
            dati,
            senza_web,
            contesto: None,
            web: None,
            caricato: Rc::new(Cell::new(false)),
            in_costruzione: false,
            in_attesa: None,
        })
    });

    // Lettore dello stdin: ogni riga al thread dell'interfaccia; a stdin chiuso si esce.
    std::thread::spawn(|| {
        for riga in std::io::stdin().lock().lines() {
            let Ok(riga) = riga else { break };
            match messaggio_da_riga(&riga) {
                Ok(m) => {
                    if slint::invoke_from_event_loop(move || gestisci(m)).is_err() {
                        return;
                    }
                }
                Err(e) => eprintln!("gestione: {e}"),
            }
        }
        let _ = slint::invoke_from_event_loop(|| {
            let _ = slint::quit_event_loop();
        });
    });

    // Il ciclo vive anche a finestra non ancora mostrata (si aspetta la prima `apri`).
    let esito = slint::run_event_loop_until_quit();
    // Prima il riquadro web (chiude i processi di WebView2), poi la finestra.
    STATO.with_borrow_mut(|s| {
        if let Some(stato) = s.take() {
            drop(stato.web);
            drop(stato.contesto);
            let _ = stato.cornice.hide();
        }
    });
    esito
}

fn gestisci(m: Messaggio) {
    match m {
        Messaggio::Apri { titolo, html } => apri(titolo, html),
        Messaggio::Chiudi => {
            let _ = slint::quit_event_loop();
        }
        // Messaggi che vanno solo verso l'assistente.
        Messaggio::Pronta | Messaggio::Chiusa | Messaggio::Azione { .. } => {
            eprintln!("gestione: messaggio inatteso {m:?}")
        }
    }
}

fn apri(titolo: String, html: String) {
    STATO.with_borrow_mut(|s| {
        let Some(stato) = s.as_mut() else { return };
        stato.cornice.set_titolo(titolo.into());
        if let Err(e) = stato.cornice.show() {
            eprintln!("gestione: finestra non mostrata: {e}");
            let _ = slint::quit_event_loop();
            return;
        }
        if stato.senza_web {
            mostra_piano_b(stato, &html);
        } else if let Some(web) = &stato.web {
            stato.caricato.set(false);
            if let Err(e) = web.load_html(&sicurezza::con_csp(&html)) {
                eprintln!("gestione: contenuto non caricato: {e}");
                mostra_piano_b(stato, &html);
            }
        } else {
            stato.in_attesa = Some(html);
            if !stato.in_costruzione {
                stato.in_costruzione = true;
                costruisci_web(stato.cornice.as_weak());
            }
        }
    });
}

fn mostra_piano_b(stato: &mut Stato, html: &str) {
    stato.cornice.set_testo_semplice(testo::html_a_testo(html).into());
    stato.cornice.set_piano_b(true);
    invia(&Messaggio::Pronta);
}

/// Il riquadro web si appoggia alla finestra di winit, che esiste solo dopo il primo `show`.
fn costruisci_web(debole: slint::Weak<Cornice>) {
    let _ = slint::spawn_local(async move {
        let Some(cornice) = debole.upgrade() else { return };
        let pronta = cornice.window().winit_window().await.is_ok();
        STATO.with_borrow_mut(|s| {
            let Some(stato) = s.as_mut() else { return };
            stato.in_costruzione = false;
            let html = stato.in_attesa.take().unwrap_or_default();
            if !pronta {
                eprintln!("gestione: finestra di winit assente, piano B");
                stato.senza_web = true;
                mostra_piano_b(stato, &html);
                return;
            }
            match crea_web(stato, &html) {
                Ok(web) => stato.web = Some(web),
                Err(e) => {
                    // Motore web assente o guasto: da qui in poi solo testo.
                    eprintln!("gestione: motore web non disponibile ({e}), piano B");
                    stato.senza_web = true;
                    mostra_piano_b(stato, &html);
                }
            }
        });
    });
}

fn crea_web(stato: &mut Stato, html: &str) -> Result<WebView, String> {
    let cornice = &stato.cornice;
    let bordo = cornice.get_bordo();
    let alto = cornice.get_alto_testata();
    let dimensione = cornice.window().size().to_logical(cornice.window().scale_factor());
    let area = Rect {
        position: LogicalPosition::new(bordo, alto).into(),
        size: LogicalSize::new(dimensione.width - 2.0 * bordo, dimensione.height - alto - bordo).into(),
    };

    stato.caricato.set(false);
    let caricato_nav = stato.caricato.clone();
    let caricato_fine = stato.caricato.clone();
    let contesto = stato.contesto.get_or_insert_with(|| WebContext::new(Some(stato.dati.clone())));
    let costruttore = WebViewBuilder::new_with_web_context(contesto)
        .with_bounds(area)
        // Le mail si leggono su fondo bianco, come in ogni client di posta (il fondo predefinito
        // del motore segue il tema scuro di Windows e il testo nero sparirebbe).
        .with_background_color((255, 255, 255, 255))
        .with_html(sicurezza::con_csp(html))
        .with_javascript_disabled()
        .with_devtools(false)
        .with_focused(false)
        .with_clipboard(false)
        .with_general_autofill_enabled(false)
        .with_navigation_handler(move |url| {
            let si = sicurezza::consenti_navigazione(&url, caricato_nav.get());
            if !si {
                eprintln!("gestione: navigazione bloccata");
            }
            si
        })
        .with_new_window_req_handler(|_, _| {
            eprintln!("gestione: nuova finestra negata");
            NewWindowResponse::Deny
        })
        .with_download_started_handler(|_, _| {
            eprintln!("gestione: scaricamento negato");
            false
        })
        .with_permission_handler(|_| PermissionResponse::Deny)
        .with_on_page_load_handler(move |evento, _| {
            // Primo «finito» del contenuto corrente: da qui la navigazione è chiusa.
            if matches!(evento, PageLoadEvent::Finished) && !caricato_fine.get() {
                caricato_fine.set(true);
                invia(&Messaggio::Pronta);
            }
        });
    #[cfg(windows)]
    let costruttore = {
        use wry::WebViewBuilderExtWindows;
        costruttore
            .with_browser_accelerator_keys(false)
            .with_default_context_menus(false)
            .with_theme(wry::Theme::Light)
    };
    costruttore.build_as_child(&cornice.window().window_handle()).map_err(|e| e.to_string())
}
