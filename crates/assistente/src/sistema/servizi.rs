//! I servizi di sistema al lavoro sulla pillola, sul thread dell'interfaccia.
//!
//! Timer:
//! - uno solo fisso, ogni 5 s: schermo intero (con l'isteresi), pillola rimessa sopra e al suo
//!   posto (cambi di area di lavoro o di risoluzione senza eventi di finestra), inattività e, solo
//!   su Windows, l'elenco dei monitor nel menu (sottomenu «Schermo»: si ricostruisce solo se cambia);
//! - una sveglia singola quando un cambio di schermo intero aspetta conferma (3 s / 1,5 s);
//! - il sondaggio del mouse ogni 100 ms SOLO mentre la pillola è ritirata e visibile (la linea
//!   lascia passare i clic, quindi non riceve eventi del mouse).

#[cfg(windows)]
use super::menu::{schermi_del_menu, Comando, MenuIsola};
use super::ritirata::{self, Ritirata, SONDAGGIO_MOUSE};
use super::scorciatoie::{self, Scorciatoia, Scorciatoie};
#[cfg(windows)]
use super::tray::{self, Tray};
use super::{inattivita, passi, pillola_visibile, schermo_intero, IsteresiSchermoIntero, Visibilita};
use crate::attenzione::Livello;
#[cfg(windows)]
use super::primo_piano::{self, GanciPrimoPiano};
#[cfg(windows)]
use crate::finestre::InfoMonitor;
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
    /// Il gancio sul primo piano (nasce al primo giro, muore in `spegni`).
    #[cfg(windows)]
    gancio: RefCell<Option<GanciPrimoPiano>>,
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
    // Il monitor preferito (anche fuori da Windows, dove non si sceglie ma la preferenza vale),
    // prima che qualunque finestra si posizioni.
    finestre::imposta_schermo_preferito(ponte.casa().config().schermo);
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
        #[cfg(windows)]
        gancio: RefCell::new(None),
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
        // Lo schermo intero si coglie appena cambia la finestra in primo piano, non al giro dei 5 s.
        #[cfg(windows)]
        {
            *s.gancio.borrow_mut() = primo_piano::ascolta(primo_piano_cambiato);
        }
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
            // Il gancio si toglie sul thread che lo ha messo (questo): `Drop` chiama UnhookWinEvent.
            s.gancio.borrow_mut().take();
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
    // I monitor possono essere cambiati dall'ultimo giro da 5 s: il menu si rifà un attimo prima.
    s.schermi_nel_menu(finestre::elenco_monitor(finestra));
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
        Comando::Schermo(i) => scegli_schermo(i),
        Comando::Esci => {
            let _ = slint::quit_event_loop();
        }
    }
}

/// «Schermo» del menu: pillola e pannello (se aperto) vanno sul monitor `i` dell'elenco mostrato nel
/// menu. La scelta si salva (il nome del monitor non va nei log) e vale anche quando il monitor
/// manca: allora la pillola sta sul principale e torna appena il monitor ricompare (controllo da
/// 5 s). Un indice che non esiste più (monitor tolto fra la costruzione del menu e la scelta) non
/// fa nulla.
#[cfg(windows)]
fn scegli_schermo(i: usize) {
    let Some(s) = servizi() else { return };
    let Some(monitor) = s.monitor() else { return };
    let Some(scelto) = monitor.get(i) else { return };
    // Un nome illeggibile (vuoto) non si può ritrovare: vale come «il principale».
    let nome = (!scelto.nome.is_empty()).then(|| scelto.nome.clone());
    s.ponte.casa().aggiorna_config(|c| c.schermo = nome.clone());
    finestre::imposta_schermo_preferito(nome);
    if let Some(p) = s.pillola.upgrade() {
        pillola::al_cambio_schermo(&p);
        s.zona.set(zona_della_linea(&p));
    }
    s.apertura.riposiziona();
    // Lo schermo intero conta sul monitor di adesso della pillola.
    s.controlla_schermo_intero();
    s.schermi_nel_menu(Some(monitor));
}

/// È cambiata la finestra in primo piano (gancio `EVENT_SYSTEM_FOREGROUND`): lo schermo intero si
/// ricontrolla subito. L'isteresi (3 s / 1,5 s) resta quella di sempre; il controllo da 5 s resta
/// per F11 in un browser, che non cambia la finestra in primo piano.
#[cfg(windows)]
fn primo_piano_cambiato() {
    if let Some(s) = servizi() {
        s.controlla_schermo_intero();
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
            pillola::colora_linea(&p, s.ponte.riempimento_linea());
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
    /// I monitor collegati: li dà la finestra della pillola o, se è nascosta (pannello aperto) e
    /// non li sa, quella del pannello. `None` se non si sa.
    #[cfg(windows)]
    fn monitor(&self) -> Option<Vec<InfoMonitor>> {
        let dalla_pillola = self.pillola.upgrade().and_then(|p| finestre::elenco_monitor(p.window()));
        dalla_pillola.or_else(|| self.apertura.con_finestra(finestre::elenco_monitor).flatten())
    }

    /// Il sottomenu «Schermo» dei due menu, rifatto solo se cambiano i monitor o la spunta (con
    /// `muda` il menu dell'icona nella barra non ha un evento «prima dell'apertura»: si tiene
    /// aggiornato qui, al giro dei 5 s e dopo una scelta). `monitor`: l'elenco di adesso, `None` =
    /// non si sa (non si tocca nulla).
    #[cfg(windows)]
    fn schermi_nel_menu(&self, monitor: Option<Vec<InfoMonitor>>) {
        let Some(monitor) = monitor else { return };
        let schermi = schermi_del_menu(&monitor, finestre::monitor_in_uso(&monitor));
        let ritirata = self.ritirata.borrow().attiva();
        let esito = match self.menu.borrow_mut().as_mut() {
            Some(m) => m.schermi(schermi, ritirata),
            None => return,
        };
        match esito {
            // Il menu è nuovo: l'icona nella barra, se c'è, prende quello.
            Ok(true) => {
                if let (Some(t), Some(m)) = (self.tray.borrow().as_ref(), self.menu.borrow().as_ref()) {
                    t.menu(m);
                }
            }
            Ok(false) => {}
            Err(e) => eprintln!("menu dell'isola non aggiornato: {e}"),
        }
    }

    /// Il giro dei 5 s.
    fn controlla(&self) {
        #[cfg(windows)]
        self.schermi_nel_menu(self.monitor());
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
        // I lati della pillola campionano solo mentre la pillola si vede.
        self.ponte.pillola_visibile(nuova == Visibilita::Pillola);
        let passi = passi(prima, nuova, aperto);
        if passi.ferma_sondaggio {
            self.mouse.stop();
        }
        match passi.linea {
            Some(true) => pillola::mostra_linea(&p, self.ponte.riempimento_linea()),
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
            // Scelto un altro schermo a pillola nascosta (pannello aperto dal «⋯»): al ritorno
            // la pillola va dove si è scelto, senza aspettare il giro dei 5 s.
            pillola::al_cambio_schermo(&p);
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
