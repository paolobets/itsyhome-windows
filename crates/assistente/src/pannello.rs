//! Apertura e chiusura del pannello: la pillola si nasconde e al suo posto scende il pannello,
//! con gli occhi dove stavano i suoi. L'orologio (timer da 1 s) e il campionamento delle
//! statistiche del PC (timer da 2 s) girano solo a pannello aperto.
//!
//! Il pannello nasce a ogni apertura e viene distrutto alla chiusura: da chiuso non restano né
//! la sua finestra né i suoi buffer né animazioni in sospeso, e la pillola torna al suo peso.

use crate::layout::Layout;
use crate::registro::app_disponibili;
use crate::pc_stats::{formatta_byte, formatta_velocita, Campionatore, StatPc};
use crate::{finestre, orologio, scelte, Pannello, Pillola, StatPcUi, VoceAppUi};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

/// Ogni quanto si campionano le statistiche del PC (solo a pannello aperto).
const PERIODO_CAMPIONI: Duration = Duration::from_secs(2);

pub struct Apertura {
    io: Weak<Apertura>,
    pillola: slint::Weak<Pillola>,
    pannello: RefCell<Option<Pannello>>,
    /// Layout scelto (indice del dock), ricordato tra un'apertura e l'altra.
    layout: Cell<i32>,
    /// Vero da quando il pannello, dopo l'apertura, ha ricevuto il fuoco: solo da lì una
    /// perdita del fuoco (clic fuori) lo chiude. Così un `Focused(false)` di passaggio durante
    /// l'apertura non lo richiude subito.
    armato: Cell<bool>,
    orologio: slint::Timer,
    /// Nasce all'apertura e muore alla chiusura, come il timer che lo alimenta.
    campionatore: RefCell<Option<Campionatore>>,
    campionamento: slint::Timer,
}

/// L'icona SVG di un'app del registro (bianca: il colore lo dà l'interfaccia).
pub fn icona(nome: &str) -> Image {
    let svg: &[u8] = match nome {
        "casa" => include_bytes!("../assets/icone/casa.svg"),
        "pc" => include_bytes!("../assets/icone/pc.svg"),
        "impostazioni" => include_bytes!("../assets/icone/impostazioni.svg"),
        _ => return Image::default(),
    };
    Image::load_from_svg_data(svg).unwrap_or_default()
}

fn voci_app() -> ModelRc<VoceAppUi> {
    let voci: Vec<VoceAppUi> = app_disponibili()
        .into_iter()
        .map(|a| VoceAppUi { id: a.id.into(), nome: a.nome.into(), icona: icona(a.icona) })
        .collect();
    ModelRc::new(VecModel::from(voci))
}

fn aggiorna_ora(pannello: &Pannello) {
    let (ora, data) = orologio::adesso();
    // Si riscrive solo se cambia: una proprietà invariata non deve ridisegnare nulla.
    if pannello.get_ora() != ora.as_str() {
        pannello.set_ora(ora.into());
    }
    if pannello.get_data() != data.as_str() {
        pannello.set_data(data.into());
    }
}

fn percentuale(frazione: f32) -> String {
    format!("{}%", frazione.round() as i32)
}

fn frazione(usato: u64, totale: u64) -> f32 {
    if totale == 0 {
        0.0
    } else {
        (usato as f64 / totale as f64).clamp(0.0, 1.0) as f32
    }
}

/// Le statistiche come le vede l'interfaccia. Prima del secondo campione la CPU è "—".
fn stat_ui(s: &StatPc, cpu_pronta: bool) -> StatPcUi {
    let ram = frazione(s.ram_usata, s.ram_totale);
    let disco = frazione(s.disco_usato, s.disco_totale);
    let batteria = s.batteria_pct;
    StatPcUi {
        pronta: cpu_pronta,
        cpu: s.cpu_pct / 100.0,
        ram,
        disco,
        cpu_testo: if cpu_pronta { percentuale(s.cpu_pct) } else { "—".into() }.into(),
        ram_testo: percentuale(ram * 100.0).into(),
        ram_dettaglio: format!("{} di {}", formatta_byte(s.ram_usata), formatta_byte(s.ram_totale)).into(),
        disco_testo: if s.disco_totale == 0 { "—".to_owned() } else { percentuale(disco * 100.0) }.into(),
        disco_dettaglio: format!("{} di {}", formatta_byte(s.disco_usato), formatta_byte(s.disco_totale)).into(),
        rete_giu: formatta_velocita(s.rete_giu_bps).into(),
        rete_su: formatta_velocita(s.rete_su_bps).into(),
        // Senza batteria la riga resta nascosta (mai "0%").
        ha_batteria: batteria.is_some(),
        batteria_testo: batteria.map(percentuale).unwrap_or_default().into(),
        batteria_frazione: batteria.map(|b| b / 100.0).unwrap_or(0.0),
        in_carica: s.in_carica.unwrap_or(false),
    }
}

/// Rimanda `f` al giro successivo del ciclo eventi (fuori dal gestore in corso: la chiusura
/// distrugge il pannello, mai dentro un suo callback).
fn dopo(apertura: &Weak<Apertura>, f: fn(&Apertura)) {
    let apertura = apertura.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(a) = apertura.upgrade() {
            f(&a);
        }
    });
}

impl Apertura {
    pub fn collega(pillola: &Pillola) -> Rc<Apertura> {
        let apertura = Rc::new_cyclic(|io| Apertura {
            io: io.clone(),
            pillola: pillola.as_weak(),
            pannello: RefCell::new(None),
            layout: Cell::new(scelte::layout_salvato().indice()),
            armato: Cell::new(false),
            orologio: slint::Timer::default(),
            campionatore: RefCell::new(None),
            campionamento: slint::Timer::default(),
        });
        let a = Rc::downgrade(&apertura);
        pillola.on_cliccata(move || dopo(&a, Apertura::apri));
        apertura
    }

    /// Un campione subito e poi uno ogni 2 s, finché il pannello è aperto.
    fn avvia_campionamento(&self, pannello: &Pannello) {
        *self.campionatore.borrow_mut() = Some(Campionatore::new());
        let io = self.io.clone();
        let debole = pannello.as_weak();
        let campiona = move || {
            let (Some(a), Some(p)) = (io.upgrade(), debole.upgrade()) else { return };
            if let Some(c) = a.campionatore.borrow_mut().as_mut() {
                let stat = c.campiona();
                p.set_pc(stat_ui(&stat, c.cpu_pronta()));
            };
        };
        campiona();
        self.campionamento.start(slint::TimerMode::Repeated, PERIODO_CAMPIONI, campiona);
    }

    fn ferma_campionamento(&self) {
        self.campionamento.stop();
        *self.campionatore.borrow_mut() = None;
    }

    /// Crea il pannello e ne collega i callback.
    fn crea(&self) -> Result<Pannello, slint::PlatformError> {
        let pannello = Pannello::new()?;
        pannello.set_app(voci_app());
        pannello.set_layout(self.layout.get());

        let a = self.io.clone();
        pannello.on_chiudi(move || dopo(&a, Apertura::chiudi));
        let a = self.io.clone();
        pannello.on_scegli_layout(move |i| {
            if let Some(a) = a.upgrade() {
                a.layout.set(i);
            }
            scelte::salva_layout(Layout::da_indice(i));
        });
        // La vista la cambia già l'interfaccia; le app la riempiranno nei task 8–9.
        pannello.on_apri_app(|_| {});

        let a = self.io.clone();
        pannello.window().on_winit_window_event(move |_, evento| {
            if let winit::event::WindowEvent::Focused(fuoco) = evento {
                if let Some(ap) = a.upgrade() {
                    if *fuoco {
                        ap.armato.set(true);
                    } else if ap.armato.get() {
                        dopo(&a, Apertura::chiudi);
                    }
                }
            }
            EventResult::Propagate
        });
        Ok(pannello)
    }

    pub fn apri(&self) {
        let Some(pillola) = self.pillola.upgrade() else { return };
        if self.pannello.borrow().is_some() {
            return;
        }
        let pannello = match self.crea() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("pannello non creato: {e}");
                return;
            }
        };
        self.armato.set(false);

        // Posizione dal monitor della pillola: prima di mostrarlo, così nasce già al suo posto.
        let schermo = finestre::schermo_di(pillola.window());
        if let Some((area, scala)) = schermo {
            finestre::sposta(pannello.window(), finestre::posizione_pannello(area, scala));
        }

        pannello.set_umore(pillola.get_umore());
        aggiorna_ora(&pannello);
        let debole = pannello.as_weak();
        self.orologio.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            if let Some(p) = debole.upgrade() {
                aggiorna_ora(&p);
            }
        });

        self.avvia_campionamento(&pannello);

        if let Err(e) = pannello.show() {
            eprintln!("pannello non mostrato: {e}");
            self.orologio.stop();
            self.ferma_campionamento();
            return;
        }
        // Da `aperto` falso (appena creato) a vero: parte l'animazione di apertura.
        pannello.set_aperto(true);

        let debole = pannello.as_weak();
        let _ = slint::spawn_local(async move {
            let Some(p) = debole.upgrade() else { return };
            if p.window().winit_window().await.is_err() {
                return;
            }
            // Senza la pillola non si sapeva il monitor: lo si chiede al pannello stesso.
            if schermo.is_none() {
                if let Some((area, scala)) = finestre::schermo_di(p.window()) {
                    finestre::sposta(p.window(), finestre::posizione_pannello(area, scala));
                }
            }
            p.window().with_winit_window(|w| w.focus_window());
        });
        *self.pannello.borrow_mut() = Some(pannello);

        // Nascosta solo ora che il pannello è visibile (vedi `chiudi`).
        pillola.set_attiva(false);
        let _ = pillola.hide();
    }

    pub fn chiudi(&self) {
        let Some(pannello) = self.pannello.borrow_mut().take() else { return };
        self.armato.set(false);
        self.orologio.stop();
        self.ferma_campionamento();
        // Prima la pillola, poi via il pannello: con zero finestre visibili il ciclo eventi
        // di Slint terminerebbe.
        if let Some(pillola) = self.pillola.upgrade() {
            pillola.set_attiva(true);
            let _ = pillola.show();
        }
        let _ = pannello.hide();
        drop(pannello);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ogni_app_del_registro_ha_la_sua_icona() {
        for app in app_disponibili() {
            let dimensioni = icona(app.icona).size();
            assert!(dimensioni.width > 0 && dimensioni.height > 0, "icona di {}", app.id);
        }
    }
}
