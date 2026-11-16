//! I servizi di sistema al lavoro sulla pillola, sul thread dell'interfaccia.
//!
//! Timer:
//! - uno solo fisso, ogni 5 s: schermo intero (con l'isteresi), pillola rimessa sopra e al suo
//!   posto (cambi di area di lavoro o di risoluzione senza eventi di finestra), inattività;
//! - una sveglia singola quando un cambio di schermo intero aspetta conferma (3 s / 1,5 s);
//! - il sondaggio del mouse ogni 100 ms SOLO mentre la pillola è ritirata e visibile (la linea
//!   lascia passare i clic, quindi non riceve eventi del mouse).

#[cfg(windows)]
use super::menu::{Comando, MenuIsola};
use super::ritirata::{self, Ritirata, SONDAGGIO_MOUSE};
use super::scorciatoie::{self, Scorciatoia, Scorciatoie};
#[cfg(windows)]
use super::tray::{self, Tray};
use super::{inattivita, passi, pillola_visibile, schermo_intero, IsteresiSchermoIntero, Visibilita};
use crate::attenzione::Livello;
use crate::finestre::{self, RettangoloFisico};
use crate::pannello::Apertura;
use crate::ponte::Ponte;
use crate::{pillola, Movimento, Pillola};
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Ogni quanto il controllo di schermo intero, primo piano, posizione e inattività.
pub const PERIODO_CONTROLLO: Duration = Duration::from_secs(5);

pub struct Servizi {
    pillola: slint::Weak<Pillola>,
    apertura: Rc<Apertura>,
    ponte: Rc<Ponte>,
    isteresi: RefCell<IsteresiSchermoIntero>,
    ritirata: RefCell<Ritirata>,
    /// Come si vede adesso la pillola.
    mostrata: Cell<Visibilita>,
    controllo: slint::Timer,
    conferma: slint::Timer,
    mouse: slint::Timer,
    /// Zona sensibile della linea (pixel fisici), presa entrando nella ritirata e a ogni controllo.
    zona: Cell<Option<RettangoloFisico>>,
    /// Il menu dell'isola (icona nella barra e, senza «Apri», il «⋯» del pannello).
    #[cfg(windows)]
    menu: RefCell<Option<MenuIsola>>,
    /// L'icona nella barra, solo se l'utente l'ha accesa.
    #[cfg(windows)]
    tray: RefCell<Option<Tray>>,
    scorciatoie: RefCell<Option<Scorciatoie>>,
}

thread_local! {
    static SERVIZI: RefCell<Option<Rc<Servizi>>> = const { RefCell::new(None) };
}

/// Il movimento ridotto chiesto dal sistema adesso (vedi `sistema::movimento`).
pub fn movimento_ridotto() -> bool {
    super::movimento::ridotto()
}

fn servizi() -> Option<Rc<Servizi>> {
    SERVIZI.with(|s| s.borrow().clone())
}

/// Avvia i servizi (dopo pillola, ponte e pannello). Menu, icona nella barra (se accesa) e
/// scorciatoie nascono al primo giro del ciclo eventi.
pub fn avvia(pillola: &Pillola, apertura: Rc<Apertura>, ponte: Rc<Ponte>) {
    let s = Rc::new(Servizi {
        pillola: pillola.as_weak(),
        apertura,
        ponte,
        isteresi: RefCell::new(IsteresiSchermoIntero::default()),
        ritirata: RefCell::new(Ritirata::default()),
        mostrata: Cell::new(Visibilita::Pillola),
        controllo: slint::Timer::default(),
        conferma: slint::Timer::default(),
        mouse: slint::Timer::default(),
        zona: Cell::new(None),
        #[cfg(windows)]
        menu: RefCell::new(None),
        #[cfg(windows)]
        tray: RefCell::new(None),
        scorciatoie: RefCell::new(None),
    });
    SERVIZI.with(|x| *x.borrow_mut() = Some(s.clone()));

    s.controllo.start(slint::TimerMode::Repeated, PERIODO_CONTROLLO, || {
        if let Some(s) = servizi() {
            s.controlla();
        }
    });
    slint::Timer::single_shot(Duration::ZERO, || {
        let Some(s) = servizi() else { return };
        #[cfg(windows)]
        match MenuIsola::crea(al_comando, s.ritirata.borrow().attiva()) {
            Ok(m) => *s.menu.borrow_mut() = Some(m),
            Err(e) => eprintln!("menu dell'isola non creato: {e}"),
        }
        mostra_icona(s.ponte.casa().config().mostra_icona_barra);
        let (gestore, non_attive) = scorciatoie::registra(al_tasto);
        *s.scorciatoie.borrow_mut() = gestore;
        for (sc, occupata) in non_attive {
            s.ponte.metti_voce(scorciatoie::voce_non_attiva(sc, occupata, Instant::now()), false);
        }
        s.controlla();
    });
}

/// A fine programma: via icona (sparisce subito, non al passaggio del mouse), menu e scorciatoie.
pub fn spegni() {
    if let Some(s) = SERVIZI.with(|x| x.borrow_mut().take()) {
        s.controllo.stop();
        s.conferma.stop();
        s.mouse.stop();
        #[cfg(windows)]
        {
            s.tray.borrow_mut().take();
            s.menu.borrow_mut().take();
        }
        s.scorciatoie.borrow_mut().take();
    }
}

/// L'icona nella barra accesa o spenta, subito (Impostazioni › Sistema e avvio). Fuori da
/// Windows non c'è.
pub fn mostra_icona(si: bool) {
    #[cfg(windows)]
    {
        let Some(s) = servizi() else { return };
        if si == s.tray.borrow().is_some() {
            return;
        }
        if !si {
            s.tray.borrow_mut().take();
            return;
        }
        let creata = match s.menu.borrow().as_ref() {
            Some(m) => tray::crea(m, apri),
            None => Err("menu dell'isola mancante".into()),
        };
        match creata {
            Ok(t) => *s.tray.borrow_mut() = Some(t),
            Err(e) => eprintln!("icona nella barra non creata: {e}"),
        }
    }
    #[cfg(not(windows))]
    let _ = si;
}

/// «⋯» in alto a destra del pannello (Task 14i): il menu dell'isola senza «Apri», sotto il
/// pulsante e allineato al suo bordo destro (`destra`, `basso`: pixel logici della finestra del
/// pannello). Il pannello resta in primo piano mentre il menu è aperto: chiuso senza scelta (Esc,
/// clic altrove) il fuoco resta su «⋯». La scelta si esegue al giro dopo, fuori dal callback del
/// pannello: Impostazioni, Informazioni, la ritirata ed Esci lo chiudono. `tastiera`: aperto da
/// tasto o da Narrator, la prima voce parte evidenziata.
#[cfg(windows)]
pub fn menu_dal_pannello(finestra: &slint::Window, destra: f32, basso: f32, tastiera: bool) {
    let Some(s) = servizi() else { return };
    let Some(hwnd) = finestre::maniglia(finestra) else { return };
    let punto = super::menu::punto_pannello(destra, basso, finestra.scale_factor());
    let scelta = {
        let menu = s.menu.borrow();
        menu.as_ref().and_then(|m| m.mostra_dal_pannello(hwnd, punto, tastiera))
    };
    if let Some(c) = scelta {
        slint::Timer::single_shot(Duration::ZERO, move || al_comando_dal_pannello(c));
    }
}

/// Una voce del menu del pannello. La ritirata vale per la pillola: il pannello si chiude e la
/// pillola si ritira (o torna) subito, non alla chiusura come da icona o tasti. Le altre voci
/// fanno quello che fanno dal menu dell'icona nella barra.
#[cfg(windows)]
fn al_comando_dal_pannello(c: Comando) {
    match c {
        Comando::Ritirata => {
            let Some(s) = servizi() else { return };
            s.ritirata.borrow_mut().commuta();
            // La chiusura passa da `pannello_chiuso`, che applica la ritirata appena commutata.
            s.apertura.chiudi();
        }
        c => al_comando(c),
    }
}

/// Fuori da Windows non c'è né menu né icona nella barra: nessuno chiama questa funzione.
#[cfg(windows)]
fn al_comando(c: Comando) {
    match c {
        Comando::Apri => apri(),
        Comando::Impostazioni => {
            if let Some(s) = servizi() {
                let _ = s.ponte.chiedi_impostazioni();
            }
        }
        Comando::Informazioni => {
            use crate::finestra_impostazioni::{apri_alla_sezione, SEZIONE_INFORMAZIONI};
            apri_alla_sezione(SEZIONE_INFORMAZIONI, || servizi().is_some_and(|s| s.ponte.chiedi_impostazioni()));
        }
        Comando::Ritirata => commuta_ritirata(),
        Comando::Esci => {
            let _ = slint::quit_event_loop();
        }
    }
}

fn al_tasto(sc: Scorciatoia) {
    match sc {
        Scorciatoia::Apri => apri(),
        Scorciatoia::Ritirata => commuta_ritirata(),
    }
}

/// Scorciatoia «apri», «Apri» del menu, clic sull'icona, seconda istanza: la pillola esce dalla ritirata, torna
/// visibile e sopra (salvataggio), e il pannello si apre (o, se aperto, torna davanti).
pub fn apri() {
    let Some(s) = servizi() else { return };
    s.ritirata.borrow_mut().esci();
    s.applica();
    if let Some(p) = s.pillola.upgrade() {
        finestre::metti_sopra(p.window());
    }
    s.apertura.apri();
}

/// «Ritira l'isola» / «Mostra l'isola» del menu, Ctrl+Alt+Maiusc+I.
pub fn commuta_ritirata() {
    let Some(s) = servizi() else { return };
    // Da icona o tasti il pannello resta com'è e la ritirata vale da quando si chiude (dal «⋯»
    // del pannello invece si chiude subito: `al_comando_dal_pannello`).
    s.ritirata.borrow_mut().commuta();
    s.applica();
}

/// Una voce nuova in attesa (dal ponte): un bloccante fa uscire dalla ritirata.
pub fn voce_arrivata(livello: Livello) {
    let Some(s) = servizi() else { return };
    let prima = s.ritirata.borrow().attiva();
    s.ritirata.borrow_mut().arriva(livello);
    if prima && !s.ritirata.borrow().attiva() {
        s.applica();
    }
}

/// Le voci in attesa sono cambiate: la linea prende il colore del livello più urgente.
pub fn colora_linea() {
    let Some(s) = servizi() else { return };
    if s.mostrata.get() == Visibilita::Linea {
        if let Some(p) = s.pillola.upgrade() {
            pillola::colora_linea(&p, s.ponte.livello_massimo());
        }
    }
}

/// Il pannello si è aperto (e la pillola nascosta).
pub fn pannello_aperto() {
    if let Some(s) = servizi() {
        s.applica();
    }
}

/// Il pannello si sta chiudendo: pillola, linea o niente (schermo intero). Senza servizi (mai,
/// salvo errori di avvio) la pillola torna e basta.
pub fn pannello_chiuso(p: &Pillola, ponte: &Ponte) {
    match servizi() {
        Some(s) => s.applica(),
        None => {
            p.set_attiva(true);
            ponte.ridisegna_pillola();
            let _ = p.show();
        }
    }
}

impl Servizi {
    /// Il giro dei 5 s.
    fn controlla(&self) {
        self.controlla_schermo_intero();
        if let Some(p) = self.pillola.upgrade() {
            // «Effetti di animazione» cambiato mentre l'app gira: lo prendono qui la pillola e,
            // se aperti, il pannello e le Impostazioni (che lo leggono anche quando nascono).
            let ridotto = movimento_ridotto();
            if p.global::<Movimento>().get_ridotto() != ridotto {
                p.global::<Movimento>().set_ridotto(ridotto);
            }
            self.apertura.movimento(ridotto);
            crate::finestra_impostazioni::movimento(ridotto);
            if self.mostrata.get() != Visibilita::Nascosta {
                finestre::metti_sopra(p.window());
                pillola::al_cambio_schermo(&p);
                self.zona.set(zona_della_linea(&p));
            }
        }
        if let Some(d) = inattivita::durata() {
            self.ponte.inattivo(d);
        }
    }

    fn controlla_schermo_intero(&self) {
        let Some(p) = self.pillola.upgrade() else { return };
        let attivo = schermo_intero::attivo(finestre::monitor_di(p.window()));
        let tra = self.isteresi.borrow_mut().osserva(attivo, Instant::now());
        match tra {
            Some(t) => self.conferma.start(slint::TimerMode::SingleShot, t, || {
                if let Some(s) = servizi() {
                    s.controlla_schermo_intero();
                }
            }),
            None => self.conferma.stop(),
        }
        self.applica();
    }

    /// Porta la pillola allo stato deciso da `pillola_visibile`. Il pannello aperto non si
    /// tocca: la pillola, allora, l'ha già nascosta `Apertura`.
    fn applica(&self) {
        let Some(p) = self.pillola.upgrade() else { return };
        let aperto = self.apertura.aperto();
        let nuova = pillola_visibile(self.isteresi.borrow().nascosta(), aperto, self.ritirata.borrow().attiva());
        let prima = self.mostrata.replace(nuova);
        #[cfg(windows)]
        if let Some(m) = self.menu.borrow().as_ref() {
            m.ritirata(self.ritirata.borrow().attiva());
        }
        let passi = passi(prima, nuova, aperto);
        if passi.ferma_sondaggio {
            self.mouse.stop();
        }
        match passi.linea {
            Some(true) => pillola::mostra_linea(&p, self.ponte.livello_massimo()),
            Some(false) => pillola::togli_linea(&p),
            None => {}
        }
        if let Some(a) = passi.attiva {
            p.set_attiva(a);
        }
        if passi.ridisegna {
            self.ponte.ridisegna_pillola();
        }
        if passi.nascondi {
            let _ = p.hide();
        }
        if passi.mostra {
            let _ = p.show();
        }
        if passi.sopra {
            finestre::metti_sopra(p.window());
        }
        if passi.avvia_sondaggio {
            self.zona.set(zona_della_linea(&p));
            self.ritirata.borrow_mut().mouse(false, Instant::now());
            self.mouse.start(slint::TimerMode::Repeated, SONDAGGIO_MOUSE, || {
                if let Some(s) = servizi() {
                    s.sonda_mouse();
                }
            });
        }
    }

    /// Ritirata: il mouse sta sulla linea da 450 ms? Allora la pillola torna.
    fn sonda_mouse(&self) {
        let sopra = match (self.zona.get(), finestre::cursore()) {
            (Some(z), Some(c)) => ritirata::dentro(z, c),
            _ => false,
        };
        if self.ritirata.borrow_mut().mouse(sopra, Instant::now()) {
            self.applica();
        }
    }
}

fn zona_della_linea(p: &Pillola) -> Option<RettangoloFisico> {
    finestre::schermo_di(p.window()).map(|(area, scala)| ritirata::zona_linea(area, scala))
}
