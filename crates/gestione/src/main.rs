//! `itsyhome-gestione`: la finestra di gestione dell'assistente, in un processo a parte.
//!
//! Nasce alla prima `apri` dell'assistente e muore alla `chiudi` o alla chiusura della finestra.
//! Parla solo su stdin/stdout (una riga JSON per messaggio, vedi `gestione-protocollo`); se stdin
//! si chiude (l'assistente è uscito o è morto) esce da sola. Non riceve token né configurazione:
//! solo la cartella dei dati del motore web (`--dati <cartella>`).
//!
//! Cornice disegnata in Slint (stesso stile dell'assistente); il contenuto (HTML ostile)
//! sta in un riquadro wry figlio della finestra Slint: JavaScript spento, devtools spenti,
//! navigazione bloccata, nuove finestre, scaricamenti e permessi negati, menu contestuale e
//! scorciatoie del browser spenti. Esc chiude anche col fuoco nel riquadro web. Se il motore web
//! manca (o con `--senza-web`, per prova), piano B: il contenuto come testo semplice.
//!
//! Il titolo della cornice è testo NON fidato (con la Mail, l'oggetto): va solo in un `Text`.
//!
//! Legami tra i crate: la cornice importa `../../assistente/ui/tema.slint` (una sola fonte per
//! colori e caratteri) e l'assistente include `assets/prova-mail.html` di questo crate; un
//! `cargo build -p assistente` non costruisce questo eseguibile (serve `-p gestione`), e chi
//! distribuisce l'assistente deve mettere `itsyhome-gestione(.exe)` accanto al suo eseguibile.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod misure;
mod sicurezza;
mod testo;

use gestione_protocollo::{messaggio_da_riga, riga_da_messaggio, schede_pulite, titolo_pulito, Messaggio, SchedaDati};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;
use wry::dpi::{PhysicalPosition, PhysicalSize};
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
    /// Misura e posizione già decise (alla prima `apri`, sull'area di lavoro del monitor).
    misurata: bool,
    /// I modelli delle schede, creati una volta e aggiornati sul posto (vedi `ModelloSchede`).
    schede: ModelloSchede,
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
    // Le schede: «Apri la Console» rimanda l'id della pagina, «Aggiorna» chiede una nuova lettura.
    // Decide tutto l'assistente (qui nessuna rete e nessun indirizzo).
    cornice.on_apri_console(|id| invia(&Messaggio::Azione { id: id.to_string() }));
    cornice.on_aggiorna(|| invia(&Messaggio::Azione { id: ID_AGGIORNA.into() }));
    segui_la_scala(&cornice);
    let debole = cornice.as_weak();
    cornice.on_trascina(move || {
        if let Some(c) = debole.upgrade() {
            let _ = c.window().with_winit_window(|w| w.drag_window());
        }
    });
    // Tab da «Chiudi» o un tasto di scorrimento sulla cornice: il fuoco nel riquadro web, che
    // allora scorre la mail con PagSu/PagGiù/frecce (Esc continua a chiudere, vedi `esc_chiude`).
    cornice.on_al_contenuto(|| {
        STATO.with_borrow(|s| {
            if let Some(web) = s.as_ref().and_then(|s| s.web.as_ref()) {
                if let Err(e) = web.focus() {
                    eprintln!("gestione: fuoco al contenuto non riuscito: {e}");
                }
            }
        });
    });
    // Il modello delle schede si dà alla cornice una volta sola.
    let schede = ModelloSchede::nuovo();
    cornice.set_schede(schede.modello());
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
            misurata: false,
            schede,
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

/// L'azione «Aggiorna» delle schede (le altre sono `console:…`, decise dall'assistente).
const ID_AGGIORNA: &str = "aggiorna";

/// Il messaggio vuole il riquadro web? Solo `apri` (HTML): le schede sono native, senza web.
pub fn serve_il_web(m: &Messaggio) -> bool {
    matches!(m, Messaggio::Apri { .. })
}

fn gestisci(m: Messaggio) {
    // Il riquadro web si vede solo per l'HTML: con le schede (e a ogni `apri` dopo) si nasconde o si
    // rimostra.
    if matches!(m, Messaggio::Apri { .. } | Messaggio::ApriSchede { .. }) {
        let serve = serve_il_web(&m);
        STATO.with_borrow(|s| {
            if let Some(web) = s.as_ref().and_then(|s| s.web.as_ref()) {
                let _ = web.set_visible(serve);
            }
        });
    }
    match m {
        Messaggio::Apri { titolo, html } => apri(titolo, html),
        Messaggio::ApriSchede { titolo, schede } => apri_schede(titolo, schede),
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
        // Seconda difesa: lo ripulisce già l'assistente, ma la finestra non si fida.
        stato.cornice.set_titolo(titolo_pulito(&titolo).into());
        stato.cornice.set_con_schede(false);
        if !stato.misurata {
            stato.misurata = true;
            misura_sull_area_di_lavoro(&stato.cornice);
        }
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

/// Le schede del dettaglio delle API: Slint nativo, niente riquadro web. Una nuova `apri_schede`
/// sostituisce i dati e lascia la scheda scelta.
fn apri_schede(titolo: String, schede: Vec<SchedaDati>) {
    STATO.with_borrow_mut(|s| {
        let Some(stato) = s.as_mut() else { return };
        stato.cornice.set_titolo(titolo_pulito(&titolo).into());
        // I modelli non si sostituiscono mai: si aggiornano sul posto, così le schede e le righe
        // che hanno il fuoco restano le stesse e il fuoco (con Esc e le frecce) sopravvive.
        stato.schede.aggiorna(schede_pulite(&schede));
        stato.cornice.set_con_schede(true);
        stato.cornice.set_piano_b(false);
        if !stato.misurata {
            stato.misurata = true;
            misura_sull_area_di_lavoro(&stato.cornice);
        }
        if let Err(e) = stato.cornice.show() {
            eprintln!("gestione: finestra non mostrata: {e}");
            let _ = slint::quit_event_loop();
            return;
        }
        invia(&Messaggio::Pronta);
    });
}

/// I modelli delle schede, creati **una sola volta** (la cornice ne tiene sempre lo stesso) e
/// aggiornati sul posto. Un repeater di Slint che riceve un `ModelRc` diverso (o un `set_vec`)
/// ricrea tutte le sue istanze: la scheda o la riga col fuoco sparirebbe e con lei il fuoco (Esc e le
/// frecce smetterebbero di funzionare). Qui si usano solo `set_row_data`, `push` e `remove`, e solo
/// per ciò che è cambiato davvero; le righe di ogni scheda hanno un loro `VecModel` fisso.
struct ModelloSchede {
    schede: Rc<VecModel<SchedaUi>>,
    righe: Vec<Rc<VecModel<RigaUi>>>,
    /// Ciò che c'è adesso nei modelli, per confrontarlo con ciò che arriva.
    dati: Vec<SchedaDati>,
}

impl ModelloSchede {
    fn nuovo() -> Self {
        Self { schede: Rc::new(VecModel::default()), righe: Vec::new(), dati: Vec::new() }
    }

    /// Il modello da dare alla cornice (sempre lo stesso).
    fn modello(&self) -> ModelRc<SchedaUi> {
        ModelRc::from(self.schede.clone())
    }

    /// Porta i modelli a `nuovi` cambiando solo ciò che è diverso.
    fn aggiorna(&mut self, nuovi: Vec<SchedaDati>) {
        while self.righe.len() < nuovi.len() {
            self.righe.push(Rc::new(VecModel::default()));
        }
        for (i, n) in nuovi.iter().enumerate() {
            let prima = self.dati.get(i);
            let colonne_uguali = prima.is_some_and(|p| p.colonne == n.colonne);
            let vecchie = prima.filter(|_| colonne_uguali).map(|p| p.righe.as_slice());
            sincronizza_righe(&self.righe[i], vecchie, n);
            let testa_uguale = prima.is_some_and(|p| p.nome == n.nome && p.nota == n.nota && colonne_uguali && p.azione_console == n.azione_console);
            if testa_uguale {
                continue;
            }
            let ui = SchedaUi {
                nome: n.nome.as_str().into(),
                nota: n.nota.as_str().into(),
                colonne: testi(&n.colonne),
                righe: ModelRc::from(self.righe[i].clone()),
                azione_console: n.azione_console.as_str().into(),
            };
            if i < self.schede.row_count() {
                self.schede.set_row_data(i, ui);
            } else {
                self.schede.push(ui);
            }
        }
        while self.schede.row_count() > nuovi.len() {
            self.schede.remove(self.schede.row_count() - 1);
        }
        self.righe.truncate(nuovi.len());
        self.dati = nuovi;
    }
}

fn testi(v: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.iter().map(|t| SharedString::from(t.as_str())).collect::<Vec<_>>()))
}

/// Porta le righe del modello a quelle di `n`: una riga uguale a prima (stesse colonne e stesse
/// celle) non si tocca; una cambiata si riscrive sul posto; le righe in più si aggiungono o si
/// tolgono in coda. `vecchie` è `None` se le colonne sono cambiate (allora tutte si riscrivono).
fn sincronizza_righe(modello: &VecModel<RigaUi>, vecchie: Option<&[Vec<String>]>, n: &SchedaDati) {
    for (j, celle) in n.righe.iter().enumerate() {
        if j < modello.row_count() {
            if vecchie.and_then(|v| v.get(j)) != Some(celle) {
                modello.set_row_data(j, riga_ui(&n.colonne, celle));
            }
        } else {
            modello.push(riga_ui(&n.colonne, celle));
        }
    }
    while modello.row_count() > n.righe.len() {
        modello.remove(modello.row_count() - 1);
    }
}

fn riga_ui(colonne: &[String], celle: &[String]) -> RigaUi {
    RigaUi { celle: testi(celle), testo: testo_riga(colonne, celle).into() }
}

/// Ciò che Narrator legge di una riga: «Nome: Chiave di prova A, Stato: Attiva…», senza le celle
/// vuote («—» compreso, che non dice nulla).
fn testo_riga(colonne: &[String], celle: &[String]) -> String {
    let parti: Vec<String> = celle
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_empty())
        .map(|(i, c)| match colonne.get(i) {
            Some(nome) if !nome.is_empty() => format!("{nome}: {c}"),
            _ => c.clone(),
        })
        .collect();
    parti.join(", ")
}

/// Prima di mostrarla la prima volta: al più 780 × 680 logici, ridotta se l'area di lavoro del
/// monitor (quello del puntatore) è più piccola, e centrata lì. Senza l'area (fuori da Windows,
/// o se la lettura non riesce) resta la misura preferita della cornice.
fn misura_sull_area_di_lavoro(cornice: &Cornice) {
    let Some((area, scala)) = area_di_lavoro() else { return };
    let ((larghezza, altezza), (x, y)) = misure::misura(area, scala);
    cornice.window().set_size(slint::LogicalSize::new(larghezza, altezza));
    cornice.window().set_position(slint::PhysicalPosition::new(x, y));
}

/// L'area di lavoro (senza barra delle applicazioni, pixel fisici) e la scala del monitor sotto
/// il puntatore.
#[cfg(windows)]
fn area_di_lavoro() -> Option<(misure::Rettangolo, f32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut punto = POINT { x: 0, y: 0 };
    // SAFETY: puntatori a variabili locali valide per tutta la chiamata; il monitor restituito da
    // MonitorFromPoint (mai nullo con DEFAULTTONEAREST) si usa solo qui.
    unsafe {
        if GetCursorPos(&mut punto) == 0 {
            return None;
        }
        let monitor = MonitorFromPoint(punto, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let scala = if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) == 0 {
            dpi_x as f32 / 96.0
        } else {
            1.0
        };
        let r = info.rcWork;
        Some(((r.left, r.top, r.right - r.left, r.bottom - r.top), scala))
    }
}

#[cfg(not(windows))]
fn area_di_lavoro() -> Option<(misure::Rettangolo, f32)> {
    None
}

fn mostra_piano_b(stato: &mut Stato, html: &str) {
    stato.cornice.set_testo_semplice(testo::html_a_testo(html).into());
    stato.cornice.set_piano_b(true);
    invia(&Messaggio::Pronta);
}

/// Il riquadro web si appoggia alla finestra di winit, che esiste solo dopo il primo `show`.
///
/// `build_as_child` gira un ciclo di messaggi annidato (WebView2 si crea in modo asincrono):
/// lo si chiama SENZA tenere `STATO` in prestito, così una `apri` smistata in quel ciclo non trova
/// il prestito occupato. Fino alla fine `in_costruzione` resta vero e una `apri` nel frattempo
/// lascia solo il suo contenuto in `in_attesa` (vince l'ultimo).
fn costruisci_web(debole: slint::Weak<Cornice>) {
    let _ = slint::spawn_local(async move {
        let Some(cornice) = debole.upgrade() else { return };
        let pronta = cornice.window().winit_window().await.is_ok();
        let preso = STATO.with_borrow_mut(|s| {
            let stato = s.as_mut()?;
            if !pronta {
                eprintln!("gestione: finestra di winit assente, piano B");
                stato.in_costruzione = false;
                stato.senza_web = true;
                let html = stato.in_attesa.take().unwrap_or_default();
                mostra_piano_b(stato, &html);
                return None;
            }
            let html = stato.in_attesa.take().unwrap_or_default();
            let contesto = stato.contesto.take().unwrap_or_else(|| WebContext::new(Some(stato.dati.clone())));
            stato.caricato.set(false);
            Some((html, contesto, stato.caricato.clone()))
        });
        let Some((html, mut contesto, caricato)) = preso else { return };

        let esito = crea_web(&cornice, &mut contesto, caricato, &html);

        STATO.with_borrow_mut(|s| {
            // Finestra già in chiusura: il riquadro appena nato si smonta qui.
            let Some(stato) = s.as_mut() else { return };
            stato.in_costruzione = false;
            stato.contesto = Some(contesto);
            match esito {
                Ok(web) => {
                    // Contenuto arrivato durante la costruzione: sostituisce il primo.
                    if let Some(nuovo) = stato.in_attesa.take() {
                        stato.caricato.set(false);
                        if let Err(e) = web.load_html(&sicurezza::con_csp(&nuovo)) {
                            eprintln!("gestione: contenuto non caricato: {e}");
                        }
                    }
                    stato.web = Some(web);
                    #[cfg(windows)]
                    prova_dpi(&stato.cornice);
                }
                Err(e) => {
                    // Motore web assente o guasto: da qui in poi solo testo.
                    eprintln!("gestione: motore web non disponibile ({e}), piano B");
                    stato.senza_web = true;
                    let html = stato.in_attesa.take().unwrap_or(html);
                    mostra_piano_b(stato, &html);
                }
            }
        });
    });
}

/// L'area del riquadro web adesso: rientrato dentro la cornice, sotto la linea di separazione
/// (non tocca mai la testata), in pixel fisici alla scala attuale della finestra.
fn area_da(cornice: &Cornice) -> Rect {
    let dimensione = cornice.window().size();
    area_per(cornice, (dimensione.width, dimensione.height), cornice.window().scale_factor())
}

/// L'area del riquadro web per una finestra di `finestra` pixel fisici a scala `scala`.
fn area_per(cornice: &Cornice, finestra: (u32, u32), scala: f32) -> Rect {
    let lato = cornice.get_bordo() + cornice.get_rientro();
    let alto = cornice.get_alto_testata() + cornice.get_sotto_linea();
    let (x, y, larghezza, altezza) = misure::area_web(finestra, scala, lato, alto);
    Rect { position: PhysicalPosition::new(x, y).into(), size: PhysicalSize::new(larghezza, altezza).into() }
}

/// FE20: quando cambia la scala (la finestra passa a un monitor con un'altra scala, o cambia il
/// DPI) o la misura, la cornice si ridisegna da sé; il riquadro web no, quindi lo si rimette al
/// suo posto. Subito, dentro `Resized` (con la misura nuova dell'evento e la scala della finestra
/// di winit, già aggiornata da `ScaleFactorChanged`), PRIMA che Slint disegni il fotogramma nuovo:
/// spostato dopo, il riquadro lascerebbe scoperta una fascia che Slint, disegnando solo le zone
/// cambiate, non ridipinge (visto dal vivo: pixel vecchi della scala di prima sotto la testata).
/// Poi, al giro dopo del ciclo, di nuovo con le misure di Slint, e `Tema.scala` (spessori sul
/// pixel) segue.
fn segui_la_scala(cornice: &Cornice) {
    use slint::winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor};
    let debole = cornice.as_weak();
    cornice.window().on_winit_window_event(move |finestra, evento| {
        if let WindowEvent::Resized(misura) = evento {
            if let Some(scala) = finestra.with_winit_window(|w| w.scale_factor() as f32) {
                if let Some(cornice) = debole.upgrade() {
                    sposta_web(area_per(&cornice, (misura.width, misura.height), scala));
                }
            }
        }
        if matches!(evento, WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Resized(_)) {
            let debole = debole.clone();
            slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                let Some(cornice) = debole.upgrade() else { return };
                riposiziona_web(&cornice);
            });
        }
        EventResult::Propagate
    });
}

fn sposta_web(area: Rect) {
    if std::env::var_os(VAR_PROVA_DPI).is_some() {
        eprintln!("gestione: riquadro web {area:?}");
    }
    // Durante un evento di winit `STATO` può essere già in prestito (ciclo annidato di WebView2):
    // allora ci pensa il giro dopo del ciclo.
    let _ = STATO.try_with(|s| {
        if let Ok(s) = s.try_borrow() {
            if let Some(web) = s.as_ref().and_then(|s| s.web.as_ref()) {
                if let Err(e) = web.set_bounds(area) {
                    eprintln!("gestione: riquadro web non riposizionato: {e}");
                }
            }
        }
    });
}

fn riposiziona_web(cornice: &Cornice) {
    let scala = cornice.window().scale_factor();
    if cornice.global::<Tema>().get_scala() != scala {
        cornice.global::<Tema>().set_scala(scala);
    }
    sposta_web(area_da(cornice));
}

fn crea_web(cornice: &Cornice, contesto: &mut WebContext, caricato: Rc<Cell<bool>>, html: &str) -> Result<WebView, String> {
    let area = area_da(cornice);
    cornice.global::<Tema>().set_scala(cornice.window().scale_factor());

    let caricato_nav = caricato.clone();
    let caricato_fine = caricato;
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
    let web = costruttore.build_as_child(&cornice.window().window_handle()).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if let Err(e) = esc_chiude(&web) {
        // Senza il gestore Esc funziona solo col fuoco sulla cornice: si continua lo stesso.
        eprintln!("gestione: Esc nel riquadro web non disponibile: {e}");
    }
    Ok(web)
}

/// Prova di FE20: `ITSYHOME_PROVA_DPI=<dpi>` nell'ambiente (lo eredita dall'assistente).
const VAR_PROVA_DPI: &str = "ITSYHOME_PROVA_DPI";

/// Prova di FE20 (solo con `ITSYHOME_PROVA_DPI=<dpi>`): 2 s dopo la nascita del riquadro web la
/// finestra si manda da sola un WM_DPICHANGED a quel DPI, come quando passa a un monitor con
/// un'altra scala, e 3 s dopo uno al DPI di prima; nel log la scala e l'area del riquadro dopo
/// ciascuno. Da un altro processo non si può: Windows rifiuta un WM_DPICHANGED che arriva da
/// fuori (il rettangolo suggerito è un puntatore).
#[cfg(windows)]
fn prova_dpi(cornice: &Cornice) {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Some(dpi) = std::env::var(VAR_PROVA_DPI).ok().and_then(|v| v.parse::<u32>().ok()) else { return };
    let finestra = cornice.window().window_handle();
    let Ok(maniglia) = finestra.window_handle() else { return };
    let RawWindowHandle::Win32(w) = maniglia.as_raw() else { return };
    let hwnd = w.hwnd.get();
    // SAFETY: la finestra della cornice esiste (il riquadro web le è appena nato dentro).
    let prima = unsafe { GetDpiForWindow(hwnd) };
    slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
        manda_dpi(hwnd, dpi);
        slint::Timer::single_shot(std::time::Duration::from_secs(3), move || manda_dpi(hwnd, prima));
    });
}

/// WM_DPICHANGED alla propria finestra col rettangolo che suggerirebbe Windows: stessa posizione,
/// misura scalata dal DPI di adesso a `dpi`.
#[cfg(windows)]
fn manda_dpi(hwnd: isize, dpi: u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, SendMessageW, WM_DPICHANGED};
    // SAFETY: `hwnd` è la finestra di questo processo (stesso thread); il rettangolo è locale e
    // vale per tutta la SendMessageW, che è sincrona.
    unsafe {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if GetWindowRect(hwnd, &mut r) == 0 {
            return;
        }
        let adesso = GetDpiForWindow(hwnd).max(1) as f32;
        let scala = dpi as f32 / adesso;
        let nuovo = RECT {
            left: r.left,
            top: r.top,
            right: r.left + ((r.right - r.left) as f32 * scala).round() as i32,
            bottom: r.top + ((r.bottom - r.top) as f32 * scala).round() as i32,
        };
        eprintln!("gestione: prova DPI {dpi} (era {adesso})");
        SendMessageW(hwnd, WM_DPICHANGED, ((dpi << 16) | dpi) as usize, &nuovo as *const RECT as isize);
    }
}

/// Codice del tasto Esc (VK_ESCAPE).
// Il gestore di Esc nel riquadro web c'è solo su Windows: fuori restano per i test.
#[cfg_attr(not(windows), allow(dead_code))]
const TASTO_ESC: u32 = 0x1B;

/// Esc premuto (non rilasciato) chiude la finestra, ovunque sia il fuoco.
#[cfg_attr(not(windows), allow(dead_code))]
fn tasto_chiude(tasto: u32, premuto: bool) -> bool {
    premuto && tasto == TASTO_ESC
}

/// Solo KEY_DOWN conta come «premuto»: con Alt premuto arriva SYSTEM_KEY_DOWN, e Alt+Esc resta a
/// Windows (il rilascio e KEY_UP non chiudono mai).
#[cfg(windows)]
fn premuto(tipo: webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_KEY_EVENT_KIND) -> bool {
    tipo == webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
}

/// Col fuoco nel riquadro web i tasti vanno a WebView2, non alla cornice: Esc (che non produce un
/// carattere) arriva come «tasto acceleratore» anche con le scorciatoie del browser spente.
#[cfg(windows)]
fn esc_chiude(web: &WebView) -> Result<(), String> {
    use webview2_com::AcceleratorKeyPressedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_KEY_EVENT_KIND;
    use wry::WebViewExtWindows;

    let gestore = AcceleratorKeyPressedEventHandler::create(Box::new(|_, argomenti| {
        let Some(argomenti) = argomenti else { return Ok(()) };
        let mut tipo = COREWEBVIEW2_KEY_EVENT_KIND::default();
        let mut tasto = 0u32;
        // SAFETY: puntatori a variabili locali valide per tutta la chiamata.
        unsafe {
            argomenti.KeyEventKind(&mut tipo)?;
            argomenti.VirtualKey(&mut tasto)?;
        }
        if tasto_chiude(tasto, premuto(tipo)) {
            // SAFETY: chiamata COM su argomenti validi durante l'evento.
            unsafe { argomenti.SetHandled(true)? };
            eprintln!("gestione: Esc nel riquadro web, chiusura");
            // Stesso thread dell'interfaccia; non tocca `STATO`.
            let _ = slint::quit_event_loop();
        }
        Ok(())
    }));
    let mut gettone = 0i64;
    // SAFETY: controller vivo quanto il riquadro; il gestore resta registrato fino alla sua fine.
    unsafe { web.controller().add_AcceleratorKeyPressed(&gestore, &mut gettone) }.map_err(|e| e.to_string())
}

#[cfg(test)]
mod test {
    use super::*;

    /// Una scheda di prova con `n` righe «Nome … · Stato …».
    fn scheda_di_prova(nome: &str, n: usize) -> SchedaDati {
        SchedaDati {
            nome: nome.into(),
            nota: "Letto alle 14:05".into(),
            colonne: vec!["Nome".into(), "Stato".into()],
            righe: (0..n).map(|i| vec![format!("{nome} {i}"), "Attiva".into()]).collect(),
            azione_console: "console:spesa".into(),
        }
    }

    fn tasto(c: &Cornice, tasto: slint::platform::Key) {
        let testo: slint::SharedString = tasto.into();
        c.window().dispatch_event(slint::platform::WindowEvent::KeyPressed { text: testo.clone() });
        c.window().dispatch_event(slint::platform::WindowEvent::KeyReleased { text: testo });
    }

    /// Le schede come le vede Narrator: un `tab-list` con le schede `tab` (la scelta è selezionata,
    /// con posizione e numero), una `list` con le righe `list-item` che si leggono per intero; ←/→
    /// spostano la scelta, ↑/↓ le righe.
    #[test]
    fn le_schede_sono_accessibili_e_la_selezione_segue_i_tasti() {
        use i_slint_backend_testing::{AccessibleRole, ElementHandle};
        use slint::platform::Key;
        i_slint_backend_testing::init_no_event_loop();
        let c = Cornice::new().expect("cornice");
        c.show().expect("mostrata nel backend di prova");
        c.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let dati = [scheda_di_prova("Spesa", 3), scheda_di_prova("Token", 2), scheda_di_prova("Chiavi", 40)];
        let mut modello = ModelloSchede::nuovo();
        c.set_schede(modello.modello());
        modello.aggiorna(dati.to_vec());
        c.set_con_schede(true);

        let con_ruolo = |ruolo: AccessibleRole| -> Vec<ElementHandle> {
            ElementHandle::find_by_element_type_name(&c, "Linguetta").chain(ElementHandle::find_by_element_type_name(&c, "RigaTabella")).filter(|e| e.accessible_role() == Some(ruolo)).collect()
        };
        let lista = ElementHandle::find_by_accessible_label(&c, "Schede del dettaglio delle API").next().expect("tab-list");
        assert_eq!(lista.accessible_role(), Some(AccessibleRole::TabList));
        let tab = con_ruolo(AccessibleRole::Tab);
        assert_eq!(tab.iter().map(|t| t.accessible_label().unwrap().to_string()).collect::<Vec<_>>(), ["Spesa", "Token", "Chiavi"]);
        assert_eq!(tab.iter().map(|t| t.accessible_item_index()).collect::<Vec<_>>(), [Some(0), Some(1), Some(2)]);
        assert!(tab.iter().all(|t| t.accessible_item_count() == Some(3) && t.accessible_item_selectable() == Some(true)));
        let scelta = |tab: &[ElementHandle]| tab.iter().map(|t| t.accessible_item_selected().unwrap()).collect::<Vec<_>>();
        assert_eq!(scelta(&tab), [true, false, false]);
        // Le righe: `list-item` che leggono le celle coi nomi delle colonne.
        let righe = con_ruolo(AccessibleRole::ListItem);
        assert_eq!(righe.len(), 3);
        assert_eq!(righe[1].accessible_label().unwrap().as_str(), "Nome: Spesa 1, Stato: Attiva");
        assert_eq!((righe[1].accessible_item_index(), righe[1].accessible_item_count()), (Some(1), Some(3)));
        let elenco = ElementHandle::find_by_accessible_label(&c, "Spesa").find(|e| e.accessible_role() == Some(AccessibleRole::List));
        assert!(elenco.is_some(), "la tabella è una `list` col nome della scheda");

        // ←/→/Fine/Inizio: la scelta segue i tasti (il fuoco è sulla scheda scelta).
        tasto(&c, Key::RightArrow);
        assert_eq!(scelta(&con_ruolo(AccessibleRole::Tab)), [false, true, false]);
        tasto(&c, Key::RightArrow);
        tasto(&c, Key::RightArrow);
        assert_eq!(scelta(&con_ruolo(AccessibleRole::Tab)), [false, false, true], "ai bordi si resta");
        tasto(&c, Key::Home);
        assert_eq!(scelta(&con_ruolo(AccessibleRole::Tab)), [true, false, false]);
        tasto(&c, Key::End);
        assert_eq!(scelta(&con_ruolo(AccessibleRole::Tab)), [false, false, true]);
        // Con la scheda «Chiavi» le righe sono 40; Slint crea solo quelle in vista (misurato: 18 di
        // 40 in questa finestra), ma ognuna sa di essere «di 40».
        let viste = con_ruolo(AccessibleRole::ListItem);
        assert!(!viste.is_empty() && viste.len() < 40, "{}", viste.len());
        assert!(viste.iter().all(|r| r.accessible_item_count() == Some(40)));
        assert_eq!(viste[0].accessible_label().unwrap().as_str(), "Nome: Chiavi 0, Stato: Attiva");
        // Una scheda tocca di nuovo: Narrator rilegge la scelta (cambia `selected`, non i nomi).
        tasto(&c, Key::LeftArrow);
        assert_eq!(scelta(&con_ruolo(AccessibleRole::Tab)), [false, true, false]);
        assert_eq!(con_ruolo(AccessibleRole::ListItem).len(), 2);
    }

    /// Un aggiornamento dei dati a finestra aperta non toglie il fuoco: la scheda o la riga che ce
    /// l'ha resta la stessa istanza (i modelli si aggiornano sul posto), quindi le frecce cambiano
    /// ancora scheda ed Esc arriva a `chiudi`.
    #[test]
    fn il_fuoco_sopravvive_agli_aggiornamenti() {
        use i_slint_backend_testing::{AccessibleRole, ElementHandle};
        use slint::platform::Key;
        i_slint_backend_testing::init_no_event_loop();
        let c = Cornice::new().expect("cornice");
        c.show().expect("mostrata nel backend di prova");
        c.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let chiuso = Rc::new(Cell::new(0));
        let k = chiuso.clone();
        c.on_chiudi(move || k.set(k.get() + 1));
        let mut modello = ModelloSchede::nuovo();
        c.set_schede(modello.modello());
        modello.aggiorna(vec![scheda_di_prova("Spesa", 3), scheda_di_prova("Token", 2), scheda_di_prova("Chiavi", 5)]);
        c.set_con_schede(true);
        let scelta = || -> Vec<bool> {
            ElementHandle::find_by_element_type_name(&c, "Linguetta")
                .filter(|e| e.accessible_role() == Some(AccessibleRole::Tab))
                .map(|e| e.accessible_item_selected().unwrap())
                .collect()
        };
        let righe = || -> Vec<String> {
            ElementHandle::find_by_element_type_name(&c, "RigaTabella").map(|e| e.accessible_label().unwrap().to_string()).collect()
        };
        // Il fuoco è sulla scheda scelta: un aggiornamento con dati cambiati (nota, righe) e poi ←/→.
        let mut cambiati = vec![scheda_di_prova("Spesa", 6), scheda_di_prova("Token", 1), scheda_di_prova("Chiavi", 5)];
        cambiati[0].nota = "Letto alle 17:00".into();
        cambiati[0].righe[0][1] = "Nuova".into();
        modello.aggiorna(cambiati);
        assert_eq!(righe().len(), 6, "le righe nuove ci sono");
        assert!(righe()[0].contains("Stato: Nuova"), "{:?}", righe()[0]);
        tasto(&c, Key::RightArrow);
        assert_eq!(scelta(), [false, true, false], "la scheda col fuoco è sopravvissuta");
        tasto(&c, Key::LeftArrow);
        assert_eq!(scelta(), [true, false, false]);
        // Il fuoco passa a una riga (↓), poi i dati cambiano ancora (la prima riga, il numero di
        // righe): la riga col fuoco c'è ancora e Esc arriva a `chiudi`.
        tasto(&c, Key::DownArrow);
        let mut ancora = vec![scheda_di_prova("Spesa", 4), scheda_di_prova("Token", 1), scheda_di_prova("Chiavi", 5)];
        ancora[0].righe[0][0] = "Spesa cambiata".into();
        modello.aggiorna(ancora);
        assert!(righe()[0].contains("Spesa cambiata"));
        assert_eq!(righe().len(), 4);
        tasto(&c, Key::Escape);
        assert_eq!(chiuso.get(), 1, "Esc arriva a `chiudi` anche dopo l'aggiornamento");
        // Senza l'aggiornamento sul posto (un `set_schede` con un modello nuovo) il fuoco sparirebbe.
        c.set_schede(ModelRc::new(VecModel::from(vec![SchedaUi::default()])));
        tasto(&c, Key::Escape);
        assert_eq!(chiuso.get(), 1, "con un modello nuovo il fuoco è perso: è ciò che si evita");
    }

    /// Le righe fuori vista non esistono finché non ci si scorre: Fine e PagGiù le mettono in vista
    /// e il fuoco le raggiunge lo stesso (Esc arriva a `chiudi` solo se il fuoco è su un elemento vivo).
    #[test]
    fn il_fuoco_raggiunge_una_riga_fuori_vista() {
        use i_slint_backend_testing::ElementHandle;
        use slint::platform::Key;
        i_slint_backend_testing::init_no_event_loop();
        let c = Cornice::new().expect("cornice");
        c.show().expect("mostrata nel backend di prova");
        c.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let chiuso = Rc::new(Cell::new(0));
        let k = chiuso.clone();
        c.on_chiudi(move || k.set(k.get() + 1));
        let mut modello = ModelloSchede::nuovo();
        c.set_schede(modello.modello());
        modello.aggiorna(vec![scheda_di_prova("Chiavi", 120)]);
        c.set_con_schede(true);
        let ha = |testo: &str| ElementHandle::find_by_element_type_name(&c, "RigaTabella").any(|e| e.accessible_label().unwrap().contains(testo));
        assert!(!ha("Chiavi 119"), "l'ultima riga non esiste ancora");
        tasto(&c, Key::DownArrow);
        tasto(&c, Key::End);
        assert!(ha("Chiavi 119"), "Fine l'ha messa in vista");
        tasto(&c, Key::Escape);
        assert_eq!(chiuso.get(), 1, "e il fuoco c'è: Esc arriva a `chiudi`");
        tasto(&c, Key::Home);
        tasto(&c, Key::PageDown);
        tasto(&c, Key::Escape);
        assert_eq!(chiuso.get(), 2, "anche dopo Inizio e PagGiù");
    }

    #[test]
    fn la_riga_si_legge_con_i_nomi_delle_colonne() {
        let colonne = ["Nome".to_owned(), "Stato".to_owned(), "Scade".to_owned()];
        let celle = ["Chiave di prova A".to_owned(), "Attiva".to_owned(), String::new()];
        assert_eq!(testo_riga(&colonne, &celle), "Nome: Chiave di prova A, Stato: Attiva");
        assert_eq!(testo_riga(&[], &["solo".to_owned()]), "solo");
    }

    #[test]
    fn apri_schede_non_chiede_il_web() {
        let schede = Messaggio::ApriSchede { titolo: "API Claude".into(), schede: vec![] };
        assert!(!serve_il_web(&schede), "le schede sono native: niente riquadro web");
        assert!(serve_il_web(&Messaggio::Apri { titolo: "T".into(), html: "<p>x</p>".into() }));
        for m in [Messaggio::Chiudi, Messaggio::Pronta, Messaggio::Chiusa, Messaggio::Azione { id: "a".into() }] {
            assert!(!serve_il_web(&m), "{m:?}");
        }
    }

    #[test]
    fn solo_esc_premuto_chiude() {
        assert!(tasto_chiude(TASTO_ESC, true));
        assert!(!tasto_chiude(TASTO_ESC, false), "il rilascio non chiude (niente doppio colpo)");
        assert!(!tasto_chiude(0x0D, true), "Invio no");
        assert!(!tasto_chiude(0x7B, true), "F12 no");
    }

    #[cfg(windows)]
    #[test]
    fn alt_esc_e_rilascio_non_contano_come_premuto() {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_KEY_UP,
            COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_UP,
        };
        assert!(premuto(COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN));
        assert!(!premuto(COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN), "Alt+Esc resta a Windows");
        assert!(!premuto(COREWEBVIEW2_KEY_EVENT_KIND_KEY_UP));
        assert!(!premuto(COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_UP));
    }
}
