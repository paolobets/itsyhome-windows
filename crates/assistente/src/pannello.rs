//! Apertura e chiusura del pannello: la pillola si nasconde e al suo posto scende il pannello,
//! con gli occhi dove stavano i suoi. L'orologio (un risveglio al minuto) e il campionamento delle
//! statistiche del PC (un campione ogni 2 s) girano solo a pannello aperto; la GPU (Task 10) solo
//! se la vista di adesso la mostra (vista PC, o un quadrante «GPU» della Panoramica).
//!
//! Il pannello nasce a ogni apertura e viene distrutto alla chiusura: da chiuso non restano né
//! la sua finestra né i suoi buffer né animazioni in sospeso, e la pillola torna al suo peso.
//! I dati della casa e l'umore arrivano dal `Ponte`, che il pannello riprende a ogni apertura.

use crate::layout::Layout;
use crate::moduli::registro::Registro;
use crate::moduli::{panoramica, Tono};
use crate::pc_stats::{formatta_usato_totale, formatta_velocita, tinta_batteria, Campionatore, Fonti, StatPc, TintaBatteria};
#[cfg(test)]
use crate::pc_stats::{LetturaBatteria, StatoBatteria};
use crate::ponte::{DallaPillola, Ponte};
use crate::sistema::menu::{self, Voce};
use crate::sistema::servizi;
use crate::{
    finestra_impostazioni, finestre, orologio, scelte, Movimento, Pannello, Pillola, SchedaBarraUi, SchermoMenuUi,
    StatPcUi, TintaBatteriaUi, VoceMenuUi,
};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use chrono::Timelike;
use std::time::Duration;

/// Ogni quanto si campionano le statistiche del PC (solo a pannello aperto).
const PERIODO_CAMPIONI: Duration = Duration::from_secs(2);
/// Il secondo campione arriva presto: la CPU (e la velocità della rete) sono differenze tra due
/// letture, e il «—» della prima apertura deve durare meno dell'apertura stessa.
const SECONDO_CAMPIONE: Duration = Duration::from_millis(300);

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
    /// Le fonti lente (dischi, reti, GPU) che il campionatore legge: solo quelle che la vista di
    /// adesso mostra (vista PC, quadranti scelti). Le cambia il thread dell'interfaccia, le legge
    /// il thread dei campioni a ogni giro. (Il brief del Task 10 chiedeva un `AtomicBool vista_pc`
    /// per la GPU: è il campo `gpu` di queste fonti, che vale anche per il quadrante «GPU».)
    fonti: Arc<Mutex<Fonti>>,
}

thread_local! {
    /// Le icone già lette, per nome: l'SVG si analizza una volta sola (`Image` si clona senza
    /// costo e Slint tiene in cache il rasterizzato per immagine).
    static ICONE: RefCell<HashMap<&'static str, Image>> = RefCell::new(HashMap::new());
}

/// Le icone che Rust passa all'interfaccia: un solo elenco, nome e SVG insieme.
const ICONE_SVG: [(&str, &[u8]); 8] = [
    ("panoramica", include_bytes!("../assets/icone/panoramica.svg")),
    // Il modulo Agenti (ridisegno F1): il terminale di Lucide (ISC).
    ("agenti", include_bytes!("../assets/icone/lucide/terminal.svg")),
    ("casa", include_bytes!("../assets/icone/casa.svg")),
    ("pc", include_bytes!("../assets/icone/pc.svg")),
    // La catena di Lucide (ISC), come le card della vista Collegamenti.
    ("collegamenti", include_bytes!("../assets/icone/lucide/link.svg")),
    // Fonte «prova» delle voci in attesa (un campanello).
    ("avviso", include_bytes!("../assets/icone/avviso.svg")),
    // Fonte «Claude» delle voci in attesa (M3): una stella a otto raggi, disegnata qui.
    ("claude", include_bytes!("../assets/icone/claude.svg")),
    // L'azione «Spegni tutte» della carta Casa (ridisegno F1).
    ("luce", include_bytes!("../assets/icone/luce.svg")),
];

/// L'icona SVG di un'app del registro (bianca: il colore lo dà l'interfaccia), dalla cache;
/// un nome sconosciuto dà un'immagine vuota.
pub fn icona(nome: &str) -> Image {
    let Some(&(chiave, svg)) = ICONE_SVG.iter().find(|(n, _)| *n == nome) else { return Image::default() };
    ICONE.with(|c| {
        c.borrow_mut().entry(chiave).or_insert_with(|| Image::load_from_svg_data(svg).unwrap_or_default()).clone()
    })
}

/// Le schede della barra con la vista `vista` aperta: la Panoramica (la schermata principale) e poi
/// i moduli nell'ordine del registro. Sulla scheda il nome breve maiuscolo (spec C12: «LINK», non
/// «COLLEGAMENTI»), per Narrator il nome intero col tono effettivo; la geometria la dà
/// `panoramica::geometria_schede` (la scheda della vista aperta è larga, col nome). Le Impostazioni
/// non sono una scheda (Task 14i): si aprono dal menu «⋯» in alto a destra e dalla tray.
pub(crate) fn schede_ui(registro: &Registro, vista: &str) -> Vec<SchedaBarraUi> {
    let voci: Vec<(&str, &str, String, &str, Tono)> = std::iter::once(("panoramica", "Panoramica", "PANORAMICA".to_owned(), "panoramica", Tono::Calmo))
        .chain(registro.ordine().into_iter().map(|m| (m.id, m.nome, m.nome_breve.to_uppercase(), m.icona, registro.tono_effettivo(m.id))))
        .collect();
    let nomi: Vec<&str> = voci.iter().map(|v| v.2.as_str()).collect();
    let attiva = voci.iter().position(|v| v.0 == vista).unwrap_or(usize::MAX);
    let geometria = panoramica::geometria_schede(&nomi, attiva);
    voci.iter()
        .zip(geometria)
        .enumerate()
        .map(|(i, (&(id, nome, ref breve, icona_id, tono), (x, larghezza)))| SchedaBarraUi {
            id: id.into(),
            nome: breve.as_str().into(),
            icona: icona(icona_id),
            x,
            larghezza,
            segno: panoramica::segno(tono),
            narratore: panoramica::narratore(nome, tono, i, voci.len()).into(),
        })
        .collect()
}

fn aggiorna_ora(ponte: &Ponte, pannello: &Pannello) {
    let (ora, data) = orologio::adesso();
    // Si riscrive solo se cambia: una proprietà invariata non deve ridisegnare nulla.
    if pannello.get_ora() != ora.as_str() {
        pannello.set_ora(ora.into());
    }
    if pannello.get_data() != data.as_str() {
        pannello.set_data(data.into());
    }
    // La data breve cambia a mezzanotte, e nel minuto nuovo se ne approfitta per rileggere tutto:
    // `riga_sinistra` si riscrive solo se il testo è diverso.
    ponte.aggiorna_riga_sinistra(pannello);
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

/// Le statistiche come le vede l'interfaccia. Prima del secondo campione, o se la CPU non si è
/// potuta leggere, la CPU è "—".
fn stat_ui(s: &StatPc, cpu_pronta: bool) -> StatPcUi {
    let cpu = s.cpu_pct.filter(|_| cpu_pronta);
    let ram = frazione(s.ram_usata, s.ram_totale);
    let disco = frazione(s.disco_usato, s.disco_totale);
    let batteria = s.batteria.map(|b| b.percentuale);
    let gpu = s.gpu_pct.map(|g| g.clamp(0.0, 100.0));
    StatPcUi {
        pronta: cpu.is_some(),
        cpu: cpu.unwrap_or(0.0) / 100.0,
        ram,
        disco,
        cpu_testo: cpu.map(percentuale).unwrap_or_else(|| "—".into()).into(),
        ram_testo: percentuale(ram * 100.0).into(),
        ram_dettaglio: formatta_usato_totale(s.ram_usata, s.ram_totale).into(),
        disco_testo: if s.disco_totale == 0 { "—".to_owned() } else { percentuale(disco * 100.0) }.into(),
        disco_dettaglio: formatta_usato_totale(s.disco_usato, s.disco_totale).into(),
        rete_giu: formatta_velocita(s.rete_giu_bps).into(),
        rete_su: formatta_velocita(s.rete_su_bps).into(),
        // Senza batteria la riga resta nascosta (mai "0%").
        ha_batteria: batteria.is_some(),
        batteria_testo: batteria.map(percentuale).unwrap_or_default().into(),
        batteria_frazione: batteria.map(|b| b / 100.0).unwrap_or(0.0),
        in_carica: s.batteria.is_some_and(|b| b.collegata()),
        tinta_batteria: tinta_ui(s.batteria.map_or(TintaBatteria::Normale, |b| tinta_batteria(&b))),
        // Senza dato la riga GPU non c'è (mai "0%").
        ha_gpu: gpu.is_some(),
        gpu: gpu.map_or(0.0, |g| g / 100.0),
        gpu_testo: gpu.map(percentuale).unwrap_or_default().into(),
    }
}

/// La tinta del glifo della batteria come la conosce l'interfaccia (pannello e pillola).
pub fn tinta_ui(t: TintaBatteria) -> TintaBatteriaUi {
    match t {
        TintaBatteria::Carica => TintaBatteriaUi::Carica,
        TintaBatteria::Normale => TintaBatteriaUi::Normale,
        TintaBatteria::Informazione => TintaBatteriaUi::Informazione,
        TintaBatteria::Attenzione => TintaBatteriaUi::Attenzione,
    }
}

/// Le fonti lente che una vista mostra: la vista PC tutte (dischi, rete e GPU), la Panoramica a
/// Quadranti quelle dei quadranti scelti (`quadranti`, chiesta solo lì), il resto nessuna.
fn fonti_della_vista(vista: &str, layout: Layout, quadranti: impl FnOnce() -> Fonti) -> Fonti {
    match (vista, layout) {
        ("pc", _) => Fonti::TUTTE,
        ("panoramica", Layout::Quadranti) => quadranti(),
        _ => Fonti::default(),
    }
}

/// Cosa fa «⋯» in alto a destra del pannello.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AltreOpzioni {
    /// Il menu dell'isola (Slint, dentro il pannello; per ora solo Windows: le voci agiscono su
    /// ritirata, schermo ed uscita, che fuori da Windows non ci sono).
    Menu,
    /// Le Impostazioni, direttamente: fuori da Windows il menu non c'è ancora, e senza
    /// l'ingranaggio nel dock «⋯» è l'unico accesso alle Impostazioni dal pannello.
    Impostazioni,
}

fn azione_altre_opzioni(menu_nativo: bool) -> AltreOpzioni {
    if menu_nativo {
        AltreOpzioni::Menu
    } else {
        AltreOpzioni::Impostazioni
    }
}

/// Le voci del menu «⋯» come le vede l'interfaccia (`voci_pannello`, unica fonte con la tray): le
/// righe del primo livello e quelle del sottomenu «Schermo» (se c'è) e la posizione dello schermo
/// spuntato (-1 nessuno). Gli id sono quelli del menu nativo (`menu::id`): Slint li rimanda e
/// `menu::comando` li ritraduce.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn modelli_menu(voci: &[Voce]) -> (Vec<VoceMenuUi>, Vec<SchermoMenuUi>, i32) {
    let mut principali = Vec::with_capacity(voci.len());
    let mut schermi = Vec::new();
    for v in voci {
        match v {
            Voce::Semplice(c, testo) => principali.push(VoceMenuUi {
                id: menu::id(*c).into(),
                testo: testo.as_str().into(),
                separatore: menu::separatore_prima(v),
                sotto: false,
            }),
            Voce::Schermo(righe) => {
                principali.push(VoceMenuUi { id: "schermo".into(), testo: "Schermo".into(), separatore: false, sotto: true });
                schermi = righe
                    .iter()
                    .map(|r| SchermoMenuUi { id: menu::id(r.comando).into(), testo: r.testo.as_str().into(), scelto: r.scelto })
                    .collect();
            }
        }
    }
    let scelto = schermi.iter().position(|s| s.scelto).map_or(-1, |i| i as i32);
    (principali, schermi, scelto)
}

/// Il modello ha già queste righe?
#[cfg_attr(not(windows), allow(dead_code))]
fn stesse_righe<T: Clone + PartialEq + 'static>(modello: &ModelRc<T>, righe: &[T]) -> bool {
    modello.row_count() == righe.len() && righe.iter().enumerate().all(|(i, r)| modello.row_data(i).as_ref() == Some(r))
}

/// Scrive nel pannello le voci del menu «⋯» (a ogni apertura del menu), ma solo quelle che sono
/// cambiate (monitor o ritirata).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn riempi_menu(p: &Pannello, voci: &[Voce]) {
    let (principali, schermi, scelto) = modelli_menu(voci);
    if !stesse_righe(&p.get_voci_menu(), &principali) {
        p.set_voci_menu(ModelRc::new(VecModel::from(principali)));
    }
    if !stesse_righe(&p.get_schermi_menu(), &schermi) {
        p.set_schermi_menu(ModelRc::new(VecModel::from(schermi)));
    }
    if p.get_menu_schermo_scelto() != scelto {
        p.set_menu_schermo_scelto(scelto);
    }
}

/// La prossima riga dopo `da` (-1 = nessuna ancora; si riparte dall'inizio) il cui testo comincia
/// con `lettera` (senza badare alle maiuscole), come i menu nativi; -1 se nessuna o se `lettera`
/// non è un solo carattere alfanumerico.
pub(crate) fn prossima_iniziale(testi: &[String], da: i32, lettera: &str) -> i32 {
    let mut caratteri = lettera.chars();
    let (Some(c), None) = (caratteri.next(), caratteri.next()) else { return -1 };
    if !c.is_alphanumeric() {
        return -1;
    }
    let inizia = |t: &String| t.chars().next().is_some_and(|p| p.to_lowercase().eq(c.to_lowercase()));
    let n = testi.len() as i32;
    (1..=n)
        .map(|passo| (da + passo).rem_euclid(n.max(1)))
        .find(|&i| inizia(&testi[i as usize]))
        .unwrap_or(-1)
}

/// Collega la ricerca per iniziale del menu «⋯» (la lettera premuta a menu aperto).
pub(crate) fn collega_iniziali(p: &Pannello) {
    let debole = p.as_weak();
    p.on_cerca_iniziale_menu(move |lettera, sotto, da| {
        let Some(p) = debole.upgrade() else { return -1 };
        let testi: Vec<String> = if sotto {
            p.get_schermi_menu().iter().map(|s| s.testo.to_string()).collect()
        } else {
            p.get_voci_menu().iter().map(|v| v.testo.to_string()).collect()
        };
        prossima_iniziale(&testi, da, lettera.as_str())
    });
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
            fonti: Arc::new(Mutex::new(Fonti::default())),
        });
        let a = Rc::downgrade(&apertura);
        pillola.on_cliccata(move || dopo(&a, Apertura::apri));
        let a = Rc::downgrade(&apertura);
        apertura.ponte.su_apri_impostazioni(move || dopo(&a, Apertura::apri_impostazioni));
        let a = Rc::downgrade(&apertura);
        apertura.ponte.su_chiudi_pannello(move || dopo(&a, Apertura::chiudi));
        apertura
    }

    /// Le Impostazioni hanno una finestra loro: il pannello si chiude e lei si apre (o, se c'è
    /// già, torna davanti).
    fn apri_impostazioni(&self) {
        self.chiudi();
        finestra_impostazioni::apri_impostazioni();
    }

    /// Un campione subito e poi uno ogni 2 s, finché il pannello è aperto, su un thread a parte:
    /// preparare il campionatore e il primo campione costa centinaia di millisecondi (contatori
    /// di sistema, dischi, reti, batteria) e sul thread dell'interfaccia ritarderebbe l'apertura.
    /// Al thread dell'interfaccia arrivano solo i numeri (`StatPc`) e un riferimento debole.
    fn avvia_campionamento(&self, pannello: &Pannello) {
        let ferma = Arc::new(AtomicBool::new(false));
        let stop = ferma.clone();
        let debole = pannello.as_weak();
        self.aggiorna_fonti(pannello);
        let fonti = self.fonti.clone();
        let thread = std::thread::Builder::new().name("campioni-pc".into()).spawn(move || {
            let mut campionatore = Campionatore::new(fonti_lette(&fonti));
            let mut attesa = SECONDO_CAMPIONE;
            while !stop.load(Ordering::Relaxed) {
                let stat = campionatore.campiona(fonti_lette(&fonti));
                let pronta = campionatore.cpu_pronta();
                let debole = debole.clone();
                let consegnato = slint::invoke_from_event_loop(move || {
                    if let Some(p) = debole.upgrade() {
                        // Solo se cambia: `pc` invalida archi, righe e quadranti che lo leggono.
                        let nuovo = stat_ui(&stat, pronta);
                        if p.get_pc() != nuovo {
                            p.set_pc(nuovo);
                        }
                        // La carta PC dallo stesso campione (prima che la CPU sia pronta: «—»).
                        if let Some(ponte) = crate::ponte::ponte() {
                            ponte.moduli_aggiorna_con("pc", crate::moduli::pc::riassunto(pronta.then_some(&stat)));
                        }
                    }
                });
                if consegnato.is_err() {
                    break;
                }
                // Svegliato prima del tempo da `ferma_campionamento` o da fonti cambiate
                // (`aggiorna_fonti`). La GPU appena accesa ha il valore al campione dopo: poco
                // tempo, come il secondo campione della CPU.
                std::thread::park_timeout(attesa);
                attesa = if campionatore.gpu_in_attesa() { SECONDO_CAMPIONE } else { PERIODO_CAMPIONI };
            }
        });
        match thread {
            Ok(t) => *self.campionamento.borrow_mut() = Some((ferma, t.thread().clone())),
            Err(e) => eprintln!("statistiche del PC non avviate: {e}"),
        }
    }

    /// L'orologio si sveglia al prossimo minuto (nessun layout mostra i secondi) e si rifasa a
    /// ogni scatto: un risveglio al minuto invece di 60. Se un layout mostrerà i secondi, qui
    /// tornerà un timer da 1 s solo per lui.
    fn programma_orologio(&self) {
        let adesso = chrono::Local::now();
        let tra = orologio::al_prossimo_minuto(adesso.second(), adesso.nanosecond());
        let a = self.io.clone();
        self.orologio.start(slint::TimerMode::SingleShot, tra, move || {
            let Some(a) = a.upgrade() else { return };
            if let Some(p) = a.pannello.borrow().as_ref() {
                aggiorna_ora(&a.ponte, p);
            }
            a.programma_orologio();
        });
    }

    /// Le fonti lente che la vista di adesso mostra: la vista PC tutte, i Quadranti quelle dei
    /// quadranti scelti, il resto nessuna (CPU e RAM si leggono sempre). Se cambiano, il thread
    /// dei campioni si sveglia subito: la GPU si accende (o il suo lettore si butta) senza
    /// aspettare i 2 s.
    fn aggiorna_fonti(&self, p: &Pannello) {
        let fonti = fonti_della_vista(p.get_vista().as_str(), Layout::da_indice(p.get_layout()), || {
            self.ponte.fonti_dei_quadranti()
        });
        let cambiate = match self.fonti.lock() {
            Ok(mut f) => std::mem::replace(&mut *f, fonti) != fonti,
            Err(_) => false,
        };
        if cambiate {
            if let Some((_, thread)) = self.campionamento.borrow().as_ref() {
                thread.unpark();
            }
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
        pannello.global::<Movimento>().set_ridotto(servizi::movimento_ridotto());
        pannello.set_layout(self.layout.get());
        self.ponte.riempi(&pannello);

        let a = self.io.clone();
        pannello.on_chiudi(move || dopo(&a, Apertura::chiudi));
        // Vista o layout cambiati: il campionatore legge dischi e reti solo se si vedono, e le
        // radio vivono solo a vista PC aperta.
        let a = self.io.clone();
        let debole = pannello.as_weak();
        pannello.on_vista_cambiata(move || {
            if let (Some(a), Some(p)) = (a.upgrade(), debole.upgrade()) {
                a.aggiorna_fonti(&p);
                a.ponte.vista_cambiata(&p);
            }
        });
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
        // La vista la cambia già l'interfaccia (Casa e PC hanno già i loro dati). «Impostazioni»
        // (non più nel dock dal Task 14i: «Apri Impostazioni» della vista Casa vuota) apre la sua
        // finestra e chiude il pannello, fuori da questo callback.
        let a = self.io.clone();
        pannello.on_apri_app(move |id| {
            if id == "impostazioni" {
                dopo(&a, Apertura::apri_impostazioni);
            }
        });
        // «⋯» in alto a destra: il menu dell'isola senza «Apri», dentro il pannello (Slint, RC-58).
        // Rust gli dà le voci di adesso un attimo prima che si apra (fuori da Windows non ce ne
        // sono, il menu non si apre e «⋯» apre le Impostazioni).
        let a = self.io.clone();
        let debole = pannello.as_weak();
        pannello.on_menu(move || match azione_altre_opzioni(cfg!(windows)) {
            AltreOpzioni::Menu => {
                #[cfg(windows)]
                if let Some(p) = debole.upgrade() {
                    riempi_menu(&p, &servizi::voci_del_menu(p.window()));
                }
                #[cfg(not(windows))]
                let _ = &debole;
            }
            AltreOpzioni::Impostazioni => dopo(&a, Apertura::apri_impostazioni),
        });
        collega_iniziali(&pannello);
        // Una voce del menu scelta: l'azione di sempre, al giro dopo (fuori da questo callback).
        pannello.on_scelta_menu(|id| {
            #[cfg(windows)]
            servizi::comando_dal_pannello(&id);
            #[cfg(not(windows))]
            let _ = id;
        });
        // Un collegamento: il browser predefinito e, se è partito, il pannello si chiude (fuori da
        // questo callback). Se non è partito il pannello resta aperto e lo dice.
        let a = self.io.clone();
        pannello.on_apri_collegamento(move |id| {
            if a.upgrade().is_some_and(|ap| ap.ponte.apri_collegamento(&id)) {
                dopo(&a, Apertura::chiudi);
            }
        });
        // «Impostazioni» della vista Claude scollegata: le Impostazioni sulla scheda Claude.
        let a = self.io.clone();
        pannello.on_gestisci_claude(move || {
            let a = a.clone();
            finestra_impostazioni::apri_alla_sezione(finestra_impostazioni::SEZIONE_CLAUDE, move || {
                dopo(&a, Apertura::apri_impostazioni);
                true
            });
        });
        // «Modifica» e «Aggiungi un collegamento»: le Impostazioni sulla scheda Collegamenti.
        let a = self.io.clone();
        pannello.on_gestisci_collegamenti(move || {
            let a = a.clone();
            finestra_impostazioni::apri_alla_sezione(finestra_impostazioni::SEZIONE_COLLEGAMENTI, move || {
                dopo(&a, Apertura::apri_impostazioni);
                true
            });
        });

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
        // Nell'Orologio la riga alta ha solo il meteo (la data è già grande al centro).
        if let Some(p) = self.pannello.borrow().as_ref() {
            self.ponte.aggiorna_riga_sinistra(p);
            self.aggiorna_fonti(p);
        }
    }

    /// Il movimento ridotto cambiato a pannello aperto (controllo da 5 s dei servizi).
    pub fn movimento(&self, ridotto: bool) {
        if let Some(p) = self.pannello.borrow().as_ref() {
            if p.global::<Movimento>().get_ridotto() != ridotto {
                p.global::<Movimento>().set_ridotto(ridotto);
            }
        }
    }

    /// Il pannello è aperto (la pillola, intanto, è nascosta).
    pub fn aperto(&self) -> bool {
        self.pannello.borrow().is_some()
    }

    /// La finestra del pannello, se aperto, a `f`.
    #[cfg(windows)]
    pub fn con_finestra<R>(&self, f: impl FnOnce(&slint::Window) -> R) -> Option<R> {
        self.pannello.borrow().as_ref().map(|p| f(p.window()))
    }

    /// Il pannello aperto va al suo posto sul monitor scelto adesso (scelto un altro schermo dal
    /// menu): posizione e scala come all'apertura.
    #[cfg(windows)]
    pub fn riposiziona(&self) {
        let pannello = self.pannello.borrow();
        let Some(p) = pannello.as_ref() else { return };
        if let Some((area, scala)) = finestre::schermo_di(p.window()) {
            finestre::sposta(p.window(), finestre::posizione_pannello(area, scala));
            finestre::scrivi_scala(&p.global::<crate::Tema>(), scala);
        }
    }

    pub fn apri(&self) {
        let Some(pillola) = self.pillola.upgrade() else { return };
        if let Some(p) = self.pannello.borrow().as_ref() {
            // Già aperto (scorciatoia, tray, seconda istanza): solo davanti, col fuoco.
            p.window().with_winit_window(|w| w.focus_window());
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
            finestre::scrivi_scala(&pannello.global::<crate::Tema>(), scala);
        }

        // Aprendo dalla pillola allargata (che chiama) gli occhi del pannello partono da dove
        // stanno i suoi e scivolano al centro durante l'apertura.
        // In altezza gli occhi della pillola stanno 6 sopra quelli del pannello (riga alta 44 contro
        // la pillola di 32): partono da lì e scendono (`SCARTO_OCCHI_PANNELLO_Y`).
        if pillola.get_aspetto() == crate::AspettoPillolaUi::Allargata {
            pannello.set_partenza_occhi(pillola.get_scarto_occhi_allerta());
            pannello.set_partenza_occhi_y(pillola.get_scarto_occhi_allerta_y() + finestre::SCARTO_OCCHI_PANNELLO_Y);
        } else {
            // A riposo con un lato solo gli occhi non stanno al centro (M2, Task 4): partono da lì.
            pannello.set_partenza_occhi(pillola.get_scarto_occhi_riposo());
            pannello.set_partenza_occhi_y(finestre::SCARTO_OCCHI_PANNELLO_Y);
        }
        // La destinazione di ciò che la pillola mostra: la pagina di un modulo o la Coda
        // (`pannello_aperto` la mette), un permesso aperto nella Coda col testo intero,
        // altrimenti la Panoramica.
        match self.ponte.apertura_dalla_pillola(pillola.get_attivita_visibile()) {
            DallaPillola::Vista(vista) => self.ponte.chiedi_vista(vista),
            DallaPillola::Permesso(indice) => pannello.invoke_apri_coda_su(indice),
            DallaPillola::Panoramica => {}
        }
        // Parte con l'umore della pillola; il ponte glielo aggiorna appena la pillola è sparita.
        pannello.set_umore(pillola.get_umore());
        aggiorna_ora(&self.ponte, &pannello);
        self.programma_orologio();

        self.avvia_campionamento(&pannello);

        if let Err(e) = pannello.show() {
            eprintln!("pannello non mostrato: {e}");
            self.orologio.stop();
            self.ferma_campionamento();
            // `riempi` ha acceso i modelli della vista Casa: un pannello mai mostrato non li guarda.
            self.ponte.pannello_chiuso();
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
        servizi::pannello_aperto();
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
        // Prima la pillola (o la linea, o nulla se c'è uno schermo intero: lo decidono i servizi
        // di sistema), poi via il pannello, così tra i due non si vede il vuoto.
        self.ponte.pannello_chiuso();
        if let Some(pillola) = self.pillola.upgrade() {
            servizi::pannello_chiuso(&pillola, &self.ponte);
        }
        let _ = pannello.hide();
        drop(pannello);
    }
}

/// Le fonti che il thread `campioni-pc` deve leggere adesso. Con il `Mutex` avvelenato (solo dopo
/// un panico) le fonti lente restano spente (`Fonti::default()`): accenderle tutte aprirebbe anche
/// il lettore della GPU senza che nessuno l'abbia chiesto.
fn fonti_lette(f: &Mutex<Fonti>) -> Fonti {
    f.lock().map(|g| *g).unwrap_or_default()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mutex_avvelenato_fonti_lente_spente() {
        let fonti = Arc::new(Mutex::new(Fonti::TUTTE));
        assert_eq!(fonti_lette(&fonti), Fonti::TUTTE);
        let f = fonti.clone();
        let _ = std::thread::spawn(move || {
            let _g = f.lock().unwrap();
            panic!("avvelena il Mutex (panico voluto nel test)");
        })
        .join();
        assert!(fonti.is_poisoned());
        assert_eq!(fonti_lette(&fonti), Fonti::default(), "GPU, dischi e reti spenti");
    }

    fn stat(cpu: Option<f32>) -> StatPc {
        StatPc {
            cpu_pct: cpu,
            ram_usata: 1,
            ram_totale: 2,
            disco_usato: 1,
            disco_totale: 4,
            rete_giu_bps: 0,
            rete_su_bps: 0,
            batteria: None,
            gpu_pct: None,
        }
    }

    #[test]
    fn tinta_della_batteria_nell_interfaccia() {
        let mut s = stat(Some(10.0));
        // Senza batteria: normale, riga nascosta.
        let ui = stat_ui(&s, true);
        assert!(!ui.ha_batteria);
        assert_eq!(ui.tinta_batteria, TintaBatteriaUi::Normale);
        s.batteria = Some(LetturaBatteria { percentuale: 18.0, stato: StatoBatteria::Scarica });
        let ui = stat_ui(&s, true);
        assert!(ui.ha_batteria && !ui.in_carica);
        assert_eq!(ui.tinta_batteria, TintaBatteriaUi::Informazione);
        s.batteria = Some(LetturaBatteria { percentuale: 18.0, stato: StatoBatteria::InCarica });
        let ui = stat_ui(&s, true);
        assert!(ui.in_carica);
        assert_eq!(ui.tinta_batteria, TintaBatteriaUi::Carica);
    }

    #[test]
    fn cpu_illeggibile_e_un_trattino_non_zero() {
        // GetSystemTimes fallito: «—», non «0%» spacciato per un dato.
        let ui = stat_ui(&stat(None), true);
        assert_eq!(ui.cpu_testo, "—");
        assert!(!ui.pronta);
        let ui = stat_ui(&stat(Some(12.4)), true);
        assert_eq!(ui.cpu_testo, "12%");
        assert!(ui.pronta);
        // Prima del secondo campione resta «—» anche con un numero.
        assert_eq!(stat_ui(&stat(Some(12.4)), false).cpu_testo, "—");
    }

    #[test]
    fn fonti_lente_solo_dove_si_vedono() {
        let solo_disco = || Fonti { disco: true, ..Fonti::default() };
        assert_eq!(fonti_della_vista("pc", Layout::Orologio, solo_disco), Fonti::TUTTE);
        assert_eq!(fonti_della_vista("panoramica", Layout::Quadranti, solo_disco), solo_disco());
        assert_eq!(fonti_della_vista("panoramica", Layout::Orologio, solo_disco), Fonti::default());
        assert_eq!(fonti_della_vista("casa", Layout::Quadranti, solo_disco), Fonti::default());
    }

    /// M2, Task 10: la GPU si legge solo nella vista PC o con un quadrante «GPU» nella Panoramica
    /// a Quadranti; nell'Orologio, nella Casa e nei Collegamenti mai.
    #[test]
    fn gpu_solo_dove_si_vede() {
        let con_gpu = || crate::lati::fonti_dei_quadranti(&[crate::lati::Lato::Gpu]);
        let senza_gpu = || crate::lati::fonti_dei_quadranti(&[crate::lati::Lato::Cpu]);
        assert!(fonti_della_vista("pc", Layout::Orologio, senza_gpu).gpu);
        assert!(fonti_della_vista("panoramica", Layout::Quadranti, con_gpu).gpu);
        assert!(!fonti_della_vista("panoramica", Layout::Quadranti, senza_gpu).gpu);
        assert!(!fonti_della_vista("panoramica", Layout::Orologio, con_gpu).gpu);
        assert!(!fonti_della_vista("casa", Layout::Quadranti, con_gpu).gpu);
        assert!(!fonti_della_vista("collegamenti", Layout::Quadranti, con_gpu).gpu);
    }

    #[test]
    fn gpu_nell_interfaccia() {
        let mut s = stat(Some(10.0));
        let ui = stat_ui(&s, true);
        assert!(!ui.ha_gpu, "senza dato niente riga");
        assert_eq!(ui.gpu_testo, "");
        s.gpu_pct = Some(36.6);
        let ui = stat_ui(&s, true);
        assert!(ui.ha_gpu);
        assert_eq!(ui.gpu_testo, "37%");
        assert!((ui.gpu - 0.366).abs() < 1e-6);
    }

    #[test]
    fn altre_opzioni_apre_il_menu_o_le_impostazioni() {
        // Task 14i, giro 1: dove c'è il menu «⋯» lo apre; altrove (macOS, Linux) apre
        // direttamente le Impostazioni, l'unico accesso dal pannello.
        assert_eq!(azione_altre_opzioni(true), AltreOpzioni::Menu);
        assert_eq!(azione_altre_opzioni(false), AltreOpzioni::Impostazioni);
    }

    #[test]
    fn la_lettera_salta_alla_prossima_voce_che_comincia_cosi() {
        let t: Vec<String> = ["Impostazioni", "Informazioni", "Ritira l'isola", "Esci"].map(String::from).into();
        assert_eq!(prossima_iniziale(&t, -1, "r"), 2);
        assert_eq!(prossima_iniziale(&t, 2, "R"), 2, "una sola: resta lì");
        assert_eq!(prossima_iniziale(&t, 0, "i"), 1, "dopo la corrente");
        assert_eq!(prossima_iniziale(&t, 1, "i"), 0, "si riparte dall'inizio");
        assert_eq!(prossima_iniziale(&t, 0, "z"), -1);
        assert_eq!(prossima_iniziale(&t, 0, "ab"), -1, "un solo carattere");
        assert_eq!(prossima_iniziale(&t, 0, "\n"), -1);
        assert_eq!(prossima_iniziale(&[], 0, "a"), -1);
    }

    #[test]
    fn i_modelli_del_menu_vengono_da_voci_pannello() {
        // Task 8 (RC-58): stesse voci e ordine di `voci_pannello`, id che tornano al `Comando`, il
        // filo prima di «Esci», le righe e la spunta di «Schermo».
        use crate::sistema::menu::{comando, voci_pannello, Comando};
        let schermi = vec![("Schermo principale".to_owned(), false), ("Schermo 2 · 2560×1440".to_owned(), true)];
        let (voci, righe, scelto) = modelli_menu(&voci_pannello(false, &schermi));
        let testi: Vec<_> = voci.iter().map(|v| v.testo.to_string()).collect();
        assert_eq!(testi, ["Impostazioni", "Informazioni", "Schermo", "Ritira l'isola", "Esci"]);
        let con_filo: Vec<_> = voci.iter().filter(|v| v.separatore).map(|v| v.testo.to_string()).collect();
        assert_eq!(con_filo, ["Esci"]);
        assert_eq!(voci.iter().filter(|v| v.sotto).count(), 1);
        assert!(voci[2].sotto);
        assert_eq!(comando(&voci[0].id), Some(Comando::Impostazioni));
        assert_eq!(comando(&voci[3].id), Some(Comando::Ritirata));
        assert_eq!(comando(&voci[4].id), Some(Comando::Esci));
        assert_eq!(comando(&voci[2].id), None, "«Schermo» apre il sottomenu, non è un comando");
        assert_eq!(righe.iter().map(|r| comando(&r.id)).collect::<Vec<_>>(), [Some(Comando::Schermo(0)), Some(Comando::Schermo(1))]);
        assert_eq!(righe.iter().map(|r| r.scelto).collect::<Vec<_>>(), [false, true]);
        assert_eq!(scelto, 1);
        // Un monitor solo: niente «Schermo» e nessuna riga.
        let (voci, righe, scelto) = modelli_menu(&voci_pannello(true, &[("Schermo principale".to_owned(), true)]));
        assert_eq!(voci.iter().map(|v| v.testo.to_string()).collect::<Vec<_>>(), ["Impostazioni", "Informazioni", "Mostra l'isola", "Esci"]);
        assert!(righe.is_empty() && scelto == -1);
    }

    #[test]
    fn ogni_app_del_registro_ha_la_sua_icona() {
        let voci = schede_ui(&Registro::nuovo(&[]), "panoramica");
        let ids: Vec<String> = voci.iter().map(|v| v.id.to_string()).collect();
        assert_eq!(ids, ["panoramica", "agenti", "casa", "pc", "collegamenti"]);
        // Sulla scheda il nome breve maiuscolo, per Narrator il nome intero.
        let nomi: Vec<String> = voci.iter().map(|v| v.nome.to_string()).collect();
        assert_eq!(nomi, ["PANORAMICA", "AGENTI", "CASA", "PC", "LINK"]);
        assert_eq!(voci[4].narratore, "Collegamenti, scheda 5 di 5");
        for app in voci.iter() {
            let dimensioni = app.icona.size();
            assert!(dimensioni.width > 0 && dimensioni.height > 0, "icona di {}", app.id);
        }
        // Agenti ha il terminale di Lucide, non la stella di Claude (che resta per le voci).
        assert_ne!(icona("agenti"), icona("claude"));
        // Tutte le icone dell'elenco si leggono, comprese quelle che non sono app («avviso»).
        for (nome, _) in ICONE_SVG {
            let dimensioni = icona(nome).size();
            assert!(dimensioni.width > 0 && dimensioni.height > 0, "icona {nome}");
        }
        assert_eq!(icona("non-esiste").size().width, 0);
    }
}
