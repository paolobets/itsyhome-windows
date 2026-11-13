//! Apertura e chiusura del pannello: la pillola si nasconde e al suo posto scende il pannello,
//! con gli occhi dove stavano i suoi. L'orologio (timer da 1 s) e il campionamento delle
//! statistiche del PC (un campione ogni 2 s) girano solo a pannello aperto.
//!
//! Il pannello nasce a ogni apertura e viene distrutto alla chiusura: da chiuso non restano né
//! la sua finestra né i suoi buffer né animazioni in sospeso, e la pillola torna al suo peso.
//! I dati della casa e l'umore arrivano dal `Ponte`, che il pannello riprende a ogni apertura.

use crate::layout::{self, Layout};
use crate::registro::app_disponibili;
use crate::pc_stats::{formatta_byte, formatta_velocita, Campionatore, StatPc};
use crate::ponte::Ponte;
use crate::{finestre, orologio, scelte, Pannello, Pillola, StatPcUi, VoceAppUi};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Ogni quanto si campionano le statistiche del PC (solo a pannello aperto).
const PERIODO_CAMPIONI: Duration = Duration::from_secs(2);

/// Fine dell'apertura (finestra creata + animazione di 200 ms in pannello.slint, con margine):
/// da qui una perdita del fuoco chiude il pannello.
const ARMA_DOPO: Duration = Duration::from_millis(300);

pub struct Apertura {
    io: Weak<Apertura>,
    pillola: slint::Weak<Pillola>,
    ponte: Rc<Ponte>,
    pannello: RefCell<Option<Pannello>>,
    /// Layout della Panoramica (indice del puntino), ricordato tra un'apertura e l'altra.
    layout: Cell<i32>,
    /// Vero dalla fine dell'apertura: solo da lì una perdita del fuoco (clic fuori) chiude.
    /// Durante l'apertura il fuoco passa di mano da solo (la pillola appena cliccata si
    /// nasconde e Windows attiva un'altra finestra, il pannello nasce e chiede il fuoco): quei
    /// `Focused(false)` di passaggio non devono richiuderlo. A fine apertura, se il fuoco non
    /// c'è, lo si richiede.
    armato: Cell<bool>,
    /// Aperto con `--apri` (misure e foto): la perdita del fuoco non lo chiude, Esc e gli occhi sì.
    fisso: Cell<bool>,
    /// Conta le aperture: la sveglia che arma la chiusura vale solo per l'apertura che l'ha messa.
    aperture: Cell<u64>,
    orologio: slint::Timer,
    /// Il thread che campiona le statistiche del PC finché il pannello è aperto: il segnale di
    /// stop e il thread da svegliare.
    campionamento: RefCell<Option<(Arc<AtomicBool>, std::thread::Thread)>>,
}

/// L'icona SVG di un'app del registro (bianca: il colore lo dà l'interfaccia).
pub fn icona(nome: &str) -> Image {
    let svg: &[u8] = match nome {
        "panoramica" => include_bytes!("../assets/icone/panoramica.svg"),
        "casa" => include_bytes!("../assets/icone/casa.svg"),
        "pc" => include_bytes!("../assets/icone/pc.svg"),
        "impostazioni" => include_bytes!("../assets/icone/impostazioni.svg"),
        // Fonte «prova» delle voci in attesa (un campanello).
        "avviso" => include_bytes!("../assets/icone/avviso.svg"),
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
    pub fn collega(pillola: &Pillola, ponte: Rc<Ponte>) -> Rc<Apertura> {
        let layout = scelte::layout_salvato(ponte.casa()).indice();
        let apertura = Rc::new_cyclic(|io| Apertura {
            io: io.clone(),
            pillola: pillola.as_weak(),
            ponte,
            pannello: RefCell::new(None),
            layout: Cell::new(layout),
            armato: Cell::new(false),
            fisso: Cell::new(false),
            aperture: Cell::new(0),
            orologio: slint::Timer::default(),
            campionamento: RefCell::new(None),
        });
        let a = Rc::downgrade(&apertura);
        pillola.on_cliccata(move || dopo(&a, Apertura::apri));
        apertura
    }

    /// Un campione subito e poi uno ogni 2 s, finché il pannello è aperto, su un thread a parte:
    /// preparare il campionatore e il primo campione costa centinaia di millisecondi (contatori
    /// di sistema, dischi, reti, batteria) e sul thread dell'interfaccia ritarderebbe l'apertura.
    /// Al thread dell'interfaccia arrivano solo i numeri (`StatPc`) e un riferimento debole.
    fn avvia_campionamento(&self, pannello: &Pannello) {
        let ferma = Arc::new(AtomicBool::new(false));
        let stop = ferma.clone();
        let debole = pannello.as_weak();
        let thread = std::thread::Builder::new().name("campioni-pc".into()).spawn(move || {
            let mut campionatore = Campionatore::new();
            while !stop.load(Ordering::Relaxed) {
                let stat = campionatore.campiona();
                let pronta = campionatore.cpu_pronta();
                let debole = debole.clone();
                let consegnato = slint::invoke_from_event_loop(move || {
                    if let Some(p) = debole.upgrade() {
                        p.set_pc(stat_ui(&stat, pronta));
                    }
                });
                if consegnato.is_err() {
                    break;
                }
                // Svegliato prima del tempo da `ferma_campionamento`.
                std::thread::park_timeout(PERIODO_CAMPIONI);
            }
        });
        match thread {
            Ok(t) => *self.campionamento.borrow_mut() = Some((ferma, t.thread().clone())),
            Err(e) => eprintln!("statistiche del PC non avviate: {e}"),
        }
    }

    /// Ferma il thread senza aspettarlo: finisce da solo appena finito il campione in corso.
    fn ferma_campionamento(&self) {
        if let Some((ferma, thread)) = self.campionamento.borrow_mut().take() {
            ferma.store(true, Ordering::Relaxed);
            thread.unpark();
        }
    }

    /// Crea il pannello e ne collega i callback.
    fn crea(&self) -> Result<Pannello, slint::PlatformError> {
        let pannello = Pannello::new()?;
        debug_assert_eq!(pannello.get_larghezza_pannello(), finestre::LARGHEZZA_PANNELLO);
        debug_assert_eq!(pannello.get_numero_layout() as usize, layout::TUTTI.len());
        pannello.set_app(voci_app());
        pannello.set_layout(self.layout.get());
        self.ponte.riempi(&pannello);

        let a = self.io.clone();
        pannello.on_chiudi(move || dopo(&a, Apertura::chiudi));
        let a = self.io.clone();
        pannello.on_scegli_layout(move |i| {
            if let Some(a) = a.upgrade() {
                a.scegli_layout(Layout::da_indice(i));
            }
        });
        let a = self.io.clone();
        let debole = pannello.as_weak();
        pannello.on_scorri_layout(move |passo| {
            let (Some(a), Some(p)) = (a.upgrade(), debole.upgrade()) else { return };
            let ora = Layout::da_indice(p.get_layout());
            let nuovo = if passo < 0 { ora.precedente() } else { ora.successivo() };
            p.set_layout(nuovo.indice());
            a.scegli_layout(nuovo);
        });
        // La vista la cambia già l'interfaccia; Casa e PC hanno già i loro dati, le Impostazioni
        // arrivano col Task 11.
        pannello.on_apri_app(|_| {});

        let a = self.io.clone();
        pannello.window().on_winit_window_event(move |_, evento| {
            if let winit::event::WindowEvent::Focused(false) = evento {
                if a.upgrade().is_some_and(|ap| ap.armato.get() && !ap.fisso.get()) {
                    dopo(&a, Apertura::chiudi);
                }
            }
            EventResult::Propagate
        });
        Ok(pannello)
    }

    /// Il layout scelto: lo si ricorda per la prossima apertura e lo si salva.
    fn scegli_layout(&self, l: Layout) {
        self.layout.set(l.indice());
        scelte::salva_layout(self.ponte.casa(), l);
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

        // Aprendo dalla pillola allargata (che chiama) gli occhi del pannello partono da dove
        // stanno i suoi e scivolano al centro durante l'apertura; e il pannello si apre su quella
        // voce, espansa sul posto nella riga «In attesa».
        if pillola.get_aspetto() == "allargata" {
            pannello.set_partenza_occhi(pillola.get_scarto_occhi_allerta());
        }
        if let Some((id, indice)) = self.ponte.voce_della_pillola() {
            pannello.set_voce_aperta(id.into());
            pannello.set_indice_aperta(indice);
        }
        // Parte con l'umore della pillola; il ponte glielo aggiorna appena la pillola è sparita.
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

        // A fine apertura si arma la chiusura sulla perdita del fuoco; se il fuoco non c'è (è
        // passato a un'altra finestra mentre la pillola si nascondeva), lo si richiede: il
        // pannello aperto da un clic deve averlo, o Esc e il clic fuori non lo chiuderebbero.
        let a = self.io.clone();
        let questa = self.aperture.get() + 1;
        self.aperture.set(questa);
        slint::Timer::single_shot(ARMA_DOPO, move || {
            let Some(ap) = a.upgrade() else { return };
            let pannello = ap.pannello.borrow();
            let Some(p) = pannello.as_ref() else { return };
            // Il pannello fisso di `--apri` non ruba il fuoco a chi sta usando il PC.
            if ap.aperture.get() != questa || ap.armato.replace(true) || ap.fisso.get() {
                return;
            }
            p.window().with_winit_window(|w| {
                if !w.has_focus() {
                    w.focus_window();
                }
            });
        });

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
        // Nascosta solo ora che il pannello è visibile (vedi `chiudi`).
        pillola.set_attiva(false);
        let _ = pillola.hide();
        // Solo ora le voci in attesa sono viste: la pillola non si vede cambiare aspetto e il
        // pannello segue l'umore finché resta aperto.
        self.ponte.pannello_aperto(&pannello);
        *self.pannello.borrow_mut() = Some(pannello);
    }

    /// Apertura per prova (`--apri`): resta aperto anche se il fuoco va altrove (il PC intanto
    /// si usa), finché non lo si chiude con Esc o con gli occhi.
    pub fn apri_fisso(&self) {
        self.fisso.set(true);
        self.apri();
    }

    pub fn chiudi(&self) {
        let Some(pannello) = self.pannello.borrow_mut().take() else { return };
        self.armato.set(false);
        self.fisso.set(false);
        self.orologio.stop();
        self.ferma_campionamento();
        // Prima la pillola, poi via il pannello: con zero finestre visibili il ciclo eventi
        // di Slint terminerebbe.
        if let Some(pillola) = self.pillola.upgrade() {
            pillola.set_attiva(true);
            self.ponte.pannello_chiuso();
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
