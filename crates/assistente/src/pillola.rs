//! La pillola: umore, aspetto (riposo, puntino, allargata che chiama) e ritaglio della finestra.
//!
//! La finestra della pillola è sempre grande quanto la pillola più grande (la bloccante) e la
//! pillola le sta al centro, attaccata in alto, quindi cambiando aspetto nulla si sposta. Per non
//! rubare clic, la finestra è ritagliata alla pillola con la sua aura (pulsanti compresi): il
//! ritaglio cresce subito quando la pillola cresce e cala solo a fine animazione.

use crate::attenzione::{altri_bloccanti, aspetto_pillola, pixel_riempiti, Attese, AspettoPillola, Fonte, Livello, Pulsante, PULSANTE_APRI};
use crate::umore::Umore;
use crate::{finestre, pannello, AspettoPillolaUi, LivelloUi, Pillola, PulsanteUi, UmoreUi};
use slint::{ComponentHandle, Image, ModelRc, VecModel};
use std::cell::Cell;
use std::time::{Duration, Instant};

/// L'animazione di larghezza e altezza in pillola.slint dura 220 ms: il ritaglio cala dopo, con
/// un margine.
const DOPO_ANIMAZIONE: Duration = Duration::from_millis(260);

thread_local! {
    /// Larghezza e altezza logiche del ritaglio in vigore (le riapplica chi cambia schermo o DPI).
    static RITAGLIO: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
    /// Conta i cambi d'aspetto: un restringimento rimandato vale solo se nel frattempo non ne è
    /// arrivato un altro.
    static GIRO: Cell<u64> = const { Cell::new(0) };
    /// Vero mentre la pillola è ritirata (linea): ogni ritaglio va alla linea.
    static LINEA: Cell<bool> = const { Cell::new(false) };
}

/// Tutto ciò che la pillola mostra: si riscrive solo quando cambia.
#[derive(Debug, Clone, PartialEq)]
pub struct VistaPillola {
    pub umore: Umore,
    pub aspetto: AspettoPillola,
    /// Livello del puntino o della voce allargata.
    pub livello: Livello,
    pub fonte: Fonte,
    pub testo: String,
    /// « +N» quando ci sono altri bloccanti oltre a quello mostrato (fuori dal testo tagliato).
    pub altri: String,
    /// La voce che la pillola allargata chiama (vuota altrimenti).
    pub voce_id: String,
    pub pulsanti: Vec<Pulsante>,
}

impl VistaPillola {
    pub fn riposo(umore: Umore) -> VistaPillola {
        VistaPillola {
            umore,
            aspetto: AspettoPillola::Riposo,
            livello: Livello::Informazione,
            fonte: Fonte::Prova,
            testo: String::new(),
            altri: String::new(),
            voce_id: String::new(),
            pulsanti: vec![],
        }
    }

    /// La voce che la pillola allargata sta chiamando.
    pub fn voce_allargata(&self) -> Option<&str> {
        match &self.aspetto {
            AspettoPillola::Allargata { voce_id, .. } => Some(voce_id),
            _ => None,
        }
    }
}

/// Che cosa mostra la pillola con queste voci in attesa e questo umore.
pub fn vista(umore: Umore, attese: &Attese, ora: Instant) -> VistaPillola {
    let aspetto = aspetto_pillola(attese, ora);
    let voce = match &aspetto {
        AspettoPillola::Riposo => None,
        AspettoPillola::Puntino => attese.piu_urgente(),
        AspettoPillola::Allargata { voce_id, .. } => attese.voce(voce_id),
    };
    let Some(v) = voce else { return VistaPillola::riposo(umore) };
    let allargata = matches!(aspetto, AspettoPillola::Allargata { .. });
    let altri = match altri_bloccanti(attese) {
        n if allargata && v.livello == Livello::Bloccante && n > 0 => format!(" +{n}"),
        _ => String::new(),
    };
    VistaPillola {
        umore,
        livello: v.livello,
        fonte: v.fonte,
        // Testo e pulsanti contano solo da allargata (il puntino non li mostra).
        testo: if allargata { v.titolo.clone() } else { String::new() },
        altri,
        voce_id: if allargata { v.id.clone() } else { String::new() },
        pulsanti: if allargata { v.pulsanti.clone() } else { vec![] },
        aspetto,
    }
}

/// L'icona di chi chiama.
pub fn icona_fonte(f: Fonte) -> Image {
    pannello::icona(match f {
        Fonte::Casa => "casa",
        Fonte::Pc => "pc",
        Fonte::Claude => "claude",
        Fonte::Prova => "avviso",
    })
}

/// I pulsanti della pillola allargata: quelli della voce, ma con il testo tagliato un consenso
/// (pulsante delicato) diventa «Apri» (pannello su quella voce, testo intero, lì si decide).
pub fn pulsanti_pillola(pulsanti: &[Pulsante], testo_tagliato: bool) -> Vec<Pulsante> {
    pulsanti
        .iter()
        .map(|p| if p.delicato && testo_tagliato { Pulsante::new(PULSANTE_APRI, "Apri") } else { p.clone() })
        .collect()
}

pub fn pulsanti_ui(p: &[Pulsante]) -> ModelRc<PulsanteUi> {
    let voci: Vec<PulsanteUi> = p
        .iter()
        .map(|p| PulsanteUi { id: p.id.as_str().into(), testo: p.testo.as_str().into(), delicato: p.delicato })
        .collect();
    ModelRc::new(VecModel::from(voci))
}

fn ritaglia(ui: &Pillola, visibile: (f32, f32)) {
    RITAGLIO.with(|r| r.set(visibile));
    applica_ritaglio(ui);
}

/// Il ritaglio in vigore: la linea se ritirata, altrimenti la fascia della pillola.
fn applica_ritaglio(ui: &Pillola) {
    if LINEA.with(Cell::get) {
        finestre::ritaglia_a_linea(ui.window());
    } else {
        finestre::ritaglia_alla_pillola(ui.window(), RITAGLIO.with(Cell::get));
    }
}

/// Ritirata: la pillola diventa la linea, con clic passanti. `livello`: il più urgente in
/// attesa o dell'avanzamento (colore della linea), `None` = neutra; poi quanto è riempita (vedi
/// `attenzione::riempimento_linea`). Prima di mostrarla, così nasce già linea.
pub fn mostra_linea(ui: &Pillola, riempimento: (Option<Livello>, f32)) {
    colora_linea(ui, riempimento);
    if LINEA.with(|l| l.replace(true)) {
        return;
    }
    ui.set_ritirata(true);
    applica_ritaglio(ui);
    finestre::clic_passanti(ui.window(), true);
}

/// Colore e riempimento della linea: ognuno si riscrive solo se cambia, e il riempimento solo se
/// cambiano i pixel riempiti (un avanzamento lento non ridisegna a ogni centesimo).
pub fn colora_linea(ui: &Pillola, (livello, frazione): (Option<Livello>, f32)) {
    let colore = livello.map_or(LivelloUi::Nessuno, livello_ui);
    if ui.get_livello_linea() != colore {
        ui.set_livello_linea(colore);
    }
    if pixel_riempiti(ui.get_riempimento_linea()) != pixel_riempiti(frazione) {
        ui.set_riempimento_linea(frazione);
    }
}

/// L'umore come lo conosce l'interfaccia (pillola e pannello).
pub fn umore_ui(u: Umore) -> UmoreUi {
    match u {
        Umore::Tranquillo => UmoreUi::Tranquillo,
        Umore::Pensa => UmoreUi::Pensa,
        Umore::TiCerca => UmoreUi::TiCerca,
        Umore::Contento => UmoreUi::Contento,
        Umore::Dorme => UmoreUi::Dorme,
    }
}

/// Il livello di una voce come lo conosce l'interfaccia (colori e forme).
pub fn livello_ui(l: Livello) -> LivelloUi {
    match l {
        Livello::Bloccante => LivelloUi::Bloccante,
        Livello::Attenzione => LivelloUi::Attenzione,
        Livello::Informazione => LivelloUi::Informazione,
    }
}

/// Fine della ritirata: di nuovo la pillola, che prende i clic.
pub fn togli_linea(ui: &Pillola) {
    if !LINEA.with(|l| l.replace(false)) {
        return;
    }
    // Togliendo `WS_EX_LAYERED` Windows butta la superficie della finestra, ma Slint ridisegna
    // solo le zone cambiate (linea e pillola): ai lati della pillola, nell'aura trasparente,
    // restavano due rettangoli chiari. Nascosta e rimostrata, la finestra si ridisegna tutta.
    let _ = ui.hide();
    finestre::clic_passanti(ui.window(), false);
    ui.set_ritirata(false);
    applica_ritaglio(ui);
    let _ = ui.show();
}

pub fn imposta(ui: &Pillola, v: &VistaPillola) {
    let prima = RITAGLIO.with(Cell::get);
    let (aspetto, ferma) = match &v.aspetto {
        AspettoPillola::Riposo => (AspettoPillolaUi::Riposo, false),
        AspettoPillola::Puntino => (AspettoPillolaUi::Puntino, false),
        AspettoPillola::Allargata { ferma, .. } => (AspettoPillolaUi::Allargata, *ferma),
    };
    let allargata = aspetto == AspettoPillolaUi::Allargata;
    ui.set_umore(umore_ui(v.umore));
    ui.set_aspetto(aspetto);
    ui.set_ferma(ferma);
    ui.set_livello(livello_ui(v.livello));
    ui.set_messaggio(v.testo.as_str().into());
    if ui.get_altri() != v.altri.as_str() {
        ui.set_altri(v.altri.as_str().into());
    }
    if allargata {
        ui.set_icona_fonte(icona_fonte(v.fonte));
    }
    if ui.get_voce_id() != v.voce_id.as_str() {
        ui.set_voce_id(v.voce_id.as_str().into());
    }
    ui.set_pulsanti(pulsanti_ui(&v.pulsanti));
    // Con i pulsanti della voce la riga sa se il testo ci sta (misura sulle larghezze finali, non
    // su quelle animate): se no, il consenso diventa «Apri».
    if allargata {
        let finali = pulsanti_pillola(&v.pulsanti, ui.get_testo_tagliato());
        if finali != v.pulsanti {
            ui.set_pulsanti(pulsanti_ui(&finali));
        }
    }

    ritaglia_dopo_il_cambio(ui, prima);
}

/// I lati della pillola sono cambiati (M2, Task 4): se la pillola si allarga o si stringe, il
/// ritaglio segue come per un cambio d'aspetto (cresce subito, cala a fine animazione).
pub fn lati_cambiati(ui: &Pillola) {
    let prima = RITAGLIO.with(Cell::get);
    if prima != visibile(ui) {
        ritaglia_dopo_il_cambio(ui, prima);
    }
}

/// Dal ritaglio `prima` a quello della pillola di adesso: subito il più grande, a fine animazione
/// quello giusto (solo se nel frattempo non è arrivato un altro cambio).
fn ritaglia_dopo_il_cambio(ui: &Pillola, prima: (f32, f32)) {
    let dopo = visibile(ui);
    let giro = GIRO.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    let (subito, poi) = ritagli(prima, dopo);
    ritaglia(ui, subito);
    if let Some(poi) = poi {
        let debole = ui.as_weak();
        slint::Timer::single_shot(DOPO_ANIMAZIONE, move || {
            if GIRO.with(Cell::get) != giro {
                return;
            }
            if let Some(ui) = debole.upgrade() {
                ritaglia(&ui, poi);
            }
        });
    }
}

/// Larghezza e altezza logiche della pillola con l'aura, a fine animazione.
fn visibile(ui: &Pillola) -> (f32, f32) {
    (ui.get_larghezza_visibile(), ui.get_altezza_visibile())
}

/// Passando dal ritaglio `prima` a `dopo`: quello da applicare subito (abbastanza grande per
/// tutta l'animazione, in ogni direzione) e, se la pillola cala, quello da applicare a fine
/// animazione.
fn ritagli(prima: (f32, f32), dopo: (f32, f32)) -> ((f32, f32), Option<(f32, f32)>) {
    let subito = (prima.0.max(dopo.0), prima.1.max(dopo.1));
    (subito, (subito != dopo).then_some(dopo))
}

/// Finestra appena nata, spostata, DPI o monitor cambiati: posizione e ritaglio di nuovo.
pub fn al_cambio_schermo(ui: &Pillola) {
    finestre::scrivi_scala(&ui.global::<crate::Tema>(), ui.window().scale_factor());
    finestre::posiziona_pillola(ui.window());
    applica_ritaglio(ui);
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::attenzione::test::voce;

    #[test]
    fn la_pillola_allargata_mostra_titolo_e_pulsanti_della_voce() {
        let t = Instant::now();
        let mut a = Attese::default();
        let mut v = voce("b", Livello::Bloccante, t);
        v.pulsanti = vec![Pulsante::new("nega", "Nega"), Pulsante::new("consenti", "Consenti")];
        a.metti(v);
        a.metti(voce("i", Livello::Informazione, t));
        let vp = vista(Umore::TiCerca, &a, t);
        assert_eq!(vp.voce_allargata(), Some("b"));
        assert_eq!(vp.testo, "Titolo b");
        assert_eq!(vp.pulsanti.len(), 2);
        assert_eq!(vp.livello, Livello::Bloccante);
        // Il puntino prende il colore della voce più urgente, senza testo né pulsanti.
        a.togli("b");
        let vp = vista(Umore::Tranquillo, &a, t);
        assert_eq!(vp.aspetto, AspettoPillola::Puntino);
        assert_eq!(vp.livello, Livello::Informazione);
        assert!(vp.testo.is_empty() && vp.pulsanti.is_empty());
        assert_eq!(vista(Umore::Tranquillo, &Attese::default(), t), VistaPillola::riposo(Umore::Tranquillo));
    }

    #[test]
    fn permesso_col_testo_tagliato_nega_e_apri() {
        // Decisione A di Paolo: se il testo non si legge tutto, dalla pillola non si consente.
        let permesso = vec![Pulsante::new("nega", "Nega"), Pulsante::delicato("consenti", "Consenti")];
        let tagliato = pulsanti_pillola(&permesso, true);
        assert_eq!(tagliato, vec![Pulsante::new("nega", "Nega"), Pulsante::new(PULSANTE_APRI, "Apri")]);
        assert!(tagliato.iter().all(|p| !p.delicato), "nessun consenso possibile dalla pillola");
        // Il testo ci sta: «Nega» / «Consenti» come sono.
        assert_eq!(pulsanti_pillola(&permesso, false), permesso);
        // Senza un pulsante delicato (Riprova, Archivia…) nulla cambia, tagliato o no.
        let riprova = vec![Pulsante::new("riprova", "Riprova")];
        assert_eq!(pulsanti_pillola(&riprova, true), riprova);
        assert!(pulsanti_pillola(&[], true).is_empty());
    }

    #[test]
    fn la_frazione_si_riscrive_solo_se_cambiano_i_pixel() {
        crate::test_ui::piattaforma_di_prova();
        let ui = Pillola::new().expect("pillola creata col backend di prova");
        // Linea piena di partenza (come in M1).
        assert_eq!(ui.get_riempimento_linea(), 1.0);
        colora_linea(&ui, (None, 0.5));
        assert_eq!(ui.get_riempimento_linea(), 0.5);
        // 0.5004 sono ancora 150 px: la proprieta' non si tocca.
        colora_linea(&ui, (None, 0.5004));
        assert_eq!(ui.get_riempimento_linea(), 0.5);
        // 0.51 sono 153 px: si riscrive.
        colora_linea(&ui, (None, 0.51));
        assert_eq!(ui.get_riempimento_linea(), 0.51);
    }

    #[test]
    fn il_ritaglio_cresce_subito_e_cala_a_fine_animazione() {
        // A riposo → che chiama: cresce subito in tutte e due le direzioni.
        assert_eq!(ritagli((140.0, 48.0), (416.0, 52.0)), ((416.0, 52.0), None));
        // Che chiama → riposo: resta grande per l'animazione, poi cala.
        assert_eq!(ritagli((416.0, 52.0), (140.0, 48.0)), ((416.0, 52.0), Some((140.0, 48.0))));
        // Uguale: nulla da rimandare.
        assert_eq!(ritagli((140.0, 48.0), (140.0, 48.0)), ((140.0, 48.0), None));
        // Più larga ma più bassa: subito la più grande in ogni direzione, poi quella giusta.
        assert_eq!(ritagli((140.0, 56.0), (416.0, 52.0)), ((416.0, 56.0), Some((416.0, 52.0))));
    }
}
