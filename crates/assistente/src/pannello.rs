//! Apertura e chiusura del pannello: la pillola si nasconde e al suo posto scende il pannello,
//! con gli occhi dove stavano i suoi. L'orologio (un risveglio al minuto) e il campionamento delle
//! statistiche del PC (un campione ogni 2 s) girano solo a pannello aperto; dischi, reti e GPU
//! (Task 10) solo nella vista PC, l'unica che li mostra.
//!
//! Il pannello nasce a ogni apertura e viene distrutto alla chiusura: da chiuso non restano né
//! la sua finestra né i suoi buffer né animazioni in sospeso, e la pillola torna al suo peso.
//! I dati della casa e l'umore arrivano dal `Ponte`, che il pannello riprende a ogni apertura.

use crate::moduli::panoramica::{fascia, in_attesa_non_mostrate, ordine_tab, prossimo, Composizione};
use crate::moduli::registro::Registro;
use crate::moduli::{panoramica, Tono};
use crate::pc_stats::{Campionatore, Fonti, TintaBatteria};
use crate::ponte::{DallaPillola, Ponte};
use crate::sistema::menu::{self, Voce};
use crate::sistema::servizi;
use crate::{
    finestra_impostazioni, finestre, orologio, Movimento, Pannello, Pillola, SchedaBarraUi, SchermoMenuUi, TintaBatteriaUi,
    VoceMenuUi,
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
    /// adesso mostra (solo la vista PC). Le cambia il thread dell'interfaccia, le legge il thread
    /// dei campioni a ogni giro. (Il brief del Task 10 chiedeva un `AtomicBool vista_pc` per la
    /// GPU: è il campo `gpu` di queste fonti.)
    fonti: Arc<Mutex<Fonti>>,
}

thread_local! {
    /// Le icone già lette, per nome: l'SVG si analizza una volta sola (`Image` si clona senza
    /// costo e Slint tiene in cache il rasterizzato per immagine).
    static ICONE: RefCell<HashMap<&'static str, Image>> = RefCell::new(HashMap::new());
}

/// Le icone che Rust passa all'interfaccia: un solo elenco, nome e SVG insieme.
const ICONE_SVG: [(&str, &[u8]); 20] = [
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
    // Le icone della pagina Casa che Rust passa alle strutture (ridisegno F3): la stella dei
    // preferiti e il tasto di accensione di «Spegni» (Lucide «star» e «power», ISC).
    ("stella", include_bytes!("../assets/icone/lucide/star.svg")),
    ("spegni", include_bytes!("../assets/icone/lucide/power.svg")),
    // Le misure e le righe della pagina PC (ridisegno F3, Task 7; Lucide, ISC): «cpu»,
    // «memory-stick», «hard-drive», «battery», «monitor» (la GPU, come in M2), «wifi»,
    // «bluetooth», «volume-2» e «settings» (i rimandi alle Impostazioni di Windows).
    ("cpu", include_bytes!("../assets/icone/lucide/cpu.svg")),
    ("memoria", include_bytes!("../assets/icone/lucide/memory-stick.svg")),
    ("disco", include_bytes!("../assets/icone/lucide/hard-drive.svg")),
    ("batteria", include_bytes!("../assets/icone/lucide/battery.svg")),
    ("gpu", include_bytes!("../assets/icone/lucide/monitor.svg")),
    ("wifi", include_bytes!("../assets/icone/wifi-3.svg")),
    ("bluetooth", include_bytes!("../assets/icone/bluetooth.svg")),
    ("volume", include_bytes!("../assets/icone/altoparlante.svg")),
    ("impostazioni", include_bytes!("../assets/icone/lucide/settings.svg")),
    // Un'icona che non disegna nulla, per i posti dove un'azione non ne ha una. Non si lascia
    // l'`Image` vuota: in Slint un'immagine vuota non è uguale a se stessa, e una riga che la
    // contiene risulterebbe sempre cambiata (e sempre riscritta).
    ("nessuna", include_bytes!("../assets/icone/nessuna.svg")),
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

/// L'ora «09:45» nel pannello: la stringa (riga alta, Narrator) e le sue cifre divise per l'ora
/// grande a celle ferme. Si riscrive solo se cambia: una proprietà invariata non deve ridisegnare.
pub(crate) fn scrivi_ora(pannello: &Pannello, ora: &str) {
    if pannello.get_ora() != ora {
        pannello.set_ora(ora.into());
        let cifre: Vec<slint::SharedString> = ora.chars().map(|c| c.to_string().into()).collect();
        pannello.set_ora_cifre(ModelRc::new(VecModel::from(cifre)));
    }
}

/// Ciò che la Panoramica mostra adesso, per l'ordine di Tab (`ordine_tab`).
fn composizione(p: &Pannello) -> Composizione {
    Composizione {
        voce: p.get_voce_mostrata(),
        altre: p.get_voce_altre(),
        in_attesa: p.get_in_attesa() > 0,
        azioni: p.get_moduli().iter().map(|m| m.con_azione).collect(),
        schede: p.get_schede().row_count(),
        volume: p.get_volume_disponibile(),
    }
}

/// Collega Tab nella Panoramica: il pannello chiede il bersaglio dopo (o prima) di quello di
/// adesso, e Rust lo calcola da ciò che c'è (`ordine_tab`, `prossimo`).
pub(crate) fn collega_fuoco(p: &Pannello) {
    let debole = p.as_weak();
    p.on_prossimo_fuoco(move |attuale, avanti| {
        let Some(p) = debole.upgrade() else { return attuale };
        prossimo(&ordine_tab(&composizione(&p)), attuale, avanti)
    });
}

/// Dove sta la voce `id` fra le voci in attesa (-1: non c'è). Per `indice-voce` del pannello: il
/// permesso aperto nella Coda segue il suo id quando cambia posto (ridisegno F2, Task 3).
pub(crate) fn indice_voce(voci: &impl Model<Data = crate::VoceAttesaUi>, id: &str) -> i32 {
    voci.iter().position(|v| v.id == id).map_or(-1, |i| i as i32)
}

/// «N in attesa ›» della riga alta: quante voci in attesa la Panoramica non mostra già (con la
/// voce che ti aspetta, nessuna: le altre le conta il suo «+N»). Solo se cambia.
pub(crate) fn scrivi_in_attesa(p: &Pannello, voci: usize, voce_mostrata: bool) {
    let n = in_attesa_non_mostrate(voci, voce_mostrata) as i32;
    if p.get_in_attesa() != n {
        p.set_in_attesa(n);
    }
}

/// La fascia dell'Orologio: gli avvisi sono le voci che la Panoramica non mostra già (quelle di
/// «N in attesa ›», `in-attesa`), `in_attesa_di_dati` i moduli senza il primo riassunto. Ogni
/// proprietà solo se cambia.
pub(crate) fn scrivi_fascia(p: &Pannello, in_attesa_di_dati: usize) {
    let f = fascia(p.get_in_attesa().max(0) as usize, in_attesa_di_dati);
    if p.get_fascia_testo() != f.testo.as_str() {
        p.set_fascia_testo(f.testo.into());
    }
    if p.get_fascia_nome() != f.nome.as_str() {
        p.set_fascia_nome(f.nome.into());
    }
    if p.get_fascia_avvisi() != f.avvisi.as_str() {
        p.set_fascia_avvisi(f.avvisi.into());
    }
}

fn aggiorna_ora(ponte: &Ponte, pannello: &Pannello) {
    let (ora, data) = orologio::adesso();
    scrivi_ora(pannello, &ora);
    if pannello.get_data() != data.as_str() {
        pannello.set_data(data.into());
    }
    // La data breve cambia a mezzanotte, e nel minuto nuovo se ne approfitta per rileggere tutto:
    // `riga_sinistra` si riscrive solo se il testo è diverso.
    ponte.aggiorna_riga_sinistra(pannello);
}

/// Il campione ha letto le fonti lente (dischi, reti e GPU): erano chieste tutte (solo a pagina PC
/// aperta) e nessuna aspetta il suo primo valore (`in_attesa`): la GPU appena accesa dà `None` per
/// un campione (non è «nessuna GPU»), la rete al primo campione dà 0 B/s (non è «nessun traffico»,
/// revisione finale di F3). È da qui che la pagina PC decide il quarto posto (ridisegno F3, Task 7).
fn lente_lette(chieste: Fonti, in_attesa: bool) -> bool {
    chieste == Fonti::TUTTE && !in_attesa
}

/// La tinta del glifo della batteria come la conosce l'interfaccia (la pillola).
pub fn tinta_ui(t: TintaBatteria) -> TintaBatteriaUi {
    match t {
        TintaBatteria::Carica => TintaBatteriaUi::Carica,
        TintaBatteria::Normale => TintaBatteriaUi::Normale,
        TintaBatteria::Informazione => TintaBatteriaUi::Informazione,
        TintaBatteria::Attenzione => TintaBatteriaUi::Attenzione,
    }
}

/// Le fonti lente che una vista mostra: la vista PC tutte (dischi, rete e GPU), il resto nessuna
/// (la Panoramica a carte non mostra né dischi né reti né GPU).
fn fonti_della_vista(vista: &str) -> Fonti {
    if vista == "pc" {
        Fonti::TUTTE
    } else {
        Fonti::default()
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
                spunta: false,
                scelto: false,
            }),
            Voce::Spunta { comando, testo, scelto } => principali.push(VoceMenuUi {
                id: menu::id(*comando).into(),
                testo: testo.as_str().into(),
                separatore: false,
                sotto: false,
                spunta: true,
                scelto: *scelto,
            }),
            Voce::Schermo(righe) => {
                principali.push(VoceMenuUi {
                    id: "schermo".into(),
                    testo: "Schermo".into(),
                    separatore: false,
                    sotto: true,
                    spunta: false,
                    scelto: false,
                });
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
        let apertura = Rc::new_cyclic(|io| Apertura {
            io: io.clone(),
            pillola: pillola.as_weak(),
            ponte,
            pannello: RefCell::new(None),
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
                let chieste = fonti_lette(&fonti);
                let stat = campionatore.campiona(chieste);
                let pronta = campionatore.cpu_pronta();
                let lente = lente_lette(chieste, campionatore.gpu_in_attesa() || campionatore.rete_in_attesa());
                let debole = debole.clone();
                let consegnato = slint::invoke_from_event_loop(move || {
                    // Solo a pannello vivo: la carta del PC e, a pagina aperta, la pagina PC.
                    if debole.upgrade().is_some() {
                        if let Some(ponte) = crate::ponte::ponte() {
                            ponte.campione_pc(stat, pronta, lente);
                        }
                    }
                });
                if consegnato.is_err() {
                    break;
                }
                // Svegliato prima del tempo da `ferma_campionamento` o da fonti cambiate
                // (`aggiorna_fonti`). La GPU appena accesa e la rete appena chiesta hanno il valore
                // al campione dopo: poco tempo, come il secondo campione della CPU.
                std::thread::park_timeout(attesa);
                let rete_attesa = fonti_lette(&fonti).rete && campionatore.rete_in_attesa();
                attesa = if campionatore.gpu_in_attesa() || rete_attesa { SECONDO_CAMPIONE } else { PERIODO_CAMPIONI };
            }
        });
        match thread {
            Ok(t) => *self.campionamento.borrow_mut() = Some((ferma, t.thread().clone())),
            Err(e) => eprintln!("statistiche del PC non avviate: {e}"),
        }
    }

    /// L'orologio si sveglia al prossimo minuto (nessuna vista mostra i secondi) e si rifasa a
    /// ogni scatto: un risveglio al minuto invece di 60.
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

    /// Le fonti lente che la vista di adesso mostra: la vista PC tutte, il resto nessuna (CPU e
    /// RAM si leggono sempre). Se cambiano, il thread dei campioni si sveglia subito: la GPU si
    /// accende (o il suo lettore si butta) senza aspettare i 2 s.
    fn aggiorna_fonti(&self, p: &Pannello) {
        let fonti = fonti_della_vista(p.get_vista().as_str());
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
        self.ponte.riempi(&pannello);
        collega_fuoco(&pannello);

        let a = self.io.clone();
        pannello.on_chiudi(move || dopo(&a, Apertura::chiudi));
        // Vista cambiata: il campionatore legge dischi e reti solo se si vedono, e le radio vivono
        // solo a vista PC aperta.
        let a = self.io.clone();
        let debole = pannello.as_weak();
        pannello.on_vista_cambiata(move || {
            if let (Some(a), Some(p)) = (a.upgrade(), debole.upgrade()) {
                a.aggiorna_fonti(&p);
                a.ponte.vista_cambiata(&p);
            }
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
        // Un collegamento scelto (`tocca-card`: il browser predefinito e, se è partito, il pannello
        // si chiude dal gancio `chiudi_pannello` del ponte) e «Modifica» o «Aggiungi un
        // collegamento» (`azione-pagina`: le Impostazioni sulla scheda Collegamenti) passano dal
        // ponte, come le carte (Ridisegno F3, Task 5).

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
        // (`pannello_aperto` la mette), un permesso nella pagina Agenti sulla riga della sua
        // sessione (la pagina, alla prima scrittura, sceglie la riga ed entra nel permesso), un
        // permesso senza riga aperto nella Coda col testo intero, altrimenti la Panoramica.
        match self.ponte.apertura_dalla_pillola(pillola.get_attivita_visibile()) {
            DallaPillola::Vista(vista) => self.ponte.chiedi_vista(vista),
            DallaPillola::Permesso(voce) => self.ponte.chiedi_permesso(&voce),
            DallaPillola::PermessoInCoda(indice) => pannello.invoke_apri_coda_su(indice),
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

    /// Ridisegno F1, Task 8: dischi, reti e GPU si leggono solo nella vista PC; la Panoramica a
    /// carte, la Casa, gli Agenti, i Collegamenti e la Coda non li mostrano.
    #[test]
    fn fonti_lente_solo_nella_vista_pc() {
        assert_eq!(fonti_della_vista("pc"), Fonti::TUTTE);
        assert!(fonti_della_vista("pc").gpu);
        for vista in ["panoramica", "casa", "agenti", "collegamenti", "coda"] {
            assert_eq!(fonti_della_vista(vista), Fonti::default(), "{vista}");
        }
    }

    /// Ridisegno F3, Task 7: un campione ha le fonti lente lette solo se erano chieste tutte e la
    /// GPU non aspetta il primo valore (il primo campione dopo l'accensione della GPU non conta).
    #[test]
    fn fonti_lente_lette_solo_col_valore_della_gpu() {
        assert!(lente_lette(Fonti::TUTTE, false));
        assert!(!lente_lette(Fonti::TUTTE, true), "la GPU appena accesa: non ancora letta");
        assert!(!lente_lette(Fonti::default(), false), "fuori dalla pagina PC");
        assert!(!lente_lette(Fonti { gpu: false, ..Fonti::TUTTE }, false));
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
        let (voci, righe, scelto) = modelli_menu(&voci_pannello(false, &schermi, true));
        let testi: Vec<_> = voci.iter().map(|v| v.testo.to_string()).collect();
        assert_eq!(testi, ["Impostazioni", "Informazioni", "Schermo", "Non disturbare", "Ritira l'isola", "Esci"]);
        let con_filo: Vec<_> = voci.iter().filter(|v| v.separatore).map(|v| v.testo.to_string()).collect();
        assert_eq!(con_filo, ["Esci"]);
        assert_eq!(voci.iter().filter(|v| v.sotto).count(), 1);
        assert!(voci[2].sotto);
        assert_eq!(comando(&voci[0].id), Some(Comando::Impostazioni));
        assert_eq!(comando(&voci[3].id), Some(Comando::NonDisturbare));
        assert_eq!(comando(&voci[4].id), Some(Comando::Ritirata));
        assert_eq!(comando(&voci[5].id), Some(Comando::Esci));
        assert_eq!(comando(&voci[2].id), None, "«Schermo» apre il sottomenu, non è un comando");
        // «Non disturbare» è l'unica voce a spunta, con la spunta dell'interruttore.
        assert_eq!(voci.iter().filter(|v| v.spunta).map(|v| (v.testo.to_string(), v.scelto)).collect::<Vec<_>>(), [("Non disturbare".to_owned(), true)]);
        assert_eq!(righe.iter().map(|r| comando(&r.id)).collect::<Vec<_>>(), [Some(Comando::Schermo(0)), Some(Comando::Schermo(1))]);
        assert_eq!(righe.iter().map(|r| r.scelto).collect::<Vec<_>>(), [false, true]);
        assert_eq!(scelto, 1);
        // Un monitor solo: niente «Schermo» e nessuna riga.
        let (voci, righe, scelto) = modelli_menu(&voci_pannello(true, &[("Schermo principale".to_owned(), true)], false));
        assert_eq!(
            voci.iter().map(|v| v.testo.to_string()).collect::<Vec<_>>(),
            ["Impostazioni", "Informazioni", "Non disturbare", "Mostra l'isola", "Esci"]
        );
        assert!(!voci[2].scelto, "interruttore spento: niente spunta");
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
        // Le icone della pagina Casa (F3) sono ognuna la sua, e nessuna l'icona che non disegna.
        assert!(icona("spegni") != icona("luce") && icona("stella") != icona("luce") && icona("spegni") != icona("nessuna"));
        assert_eq!(icona("non-esiste").size().width, 0);
    }
}
