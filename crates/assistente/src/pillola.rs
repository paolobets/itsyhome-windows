//! La pillola: umore, aspetto (riposo, puntino, allargata che chiama) e ritaglio della finestra.
//!
//! La finestra della pillola è sempre larga quanto la pillola più larga e la pillola le sta al
//! centro, quindi cambiando aspetto nulla si sposta. Per non rubare clic, la finestra è ritagliata
//! alla fascia visibile (pulsanti compresi): si allarga subito quando la pillola cresce e si
//! stringe solo a fine animazione quando la pillola cala.

use crate::attenzione::{aspetto_pillola, Attese, AspettoPillola, Fonte, Livello, Pulsante};
use crate::umore::Umore;
use crate::{finestre, pannello, Pillola, PulsanteUi};
use slint::{ComponentHandle, Image, ModelRc, VecModel};
use std::cell::Cell;
use std::time::{Duration, Instant};

/// L'animazione della larghezza in pillola.slint dura 220 ms: si stringe dopo, con un margine.
const DOPO_ANIMAZIONE: Duration = Duration::from_millis(260);

thread_local! {
    /// Larghezza logica del ritaglio in vigore (la riapplica chi cambia schermo o DPI).
    static RITAGLIO: Cell<f32> = const { Cell::new(0.0) };
    /// Conta i cambi d'aspetto: un restringimento rimandato vale solo se nel frattempo non ne è
    /// arrivato un altro.
    static GIRO: Cell<u64> = const { Cell::new(0) };
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
    VistaPillola {
        umore,
        livello: v.livello,
        fonte: v.fonte,
        // Testo e pulsanti contano solo da allargata (il puntino non li mostra).
        testo: if allargata { v.titolo.clone() } else { String::new() },
        pulsanti: if allargata { v.pulsanti.clone() } else { vec![] },
        aspetto,
    }
}

/// L'icona di chi chiama.
pub fn icona_fonte(f: Fonte) -> Image {
    pannello::icona(match f {
        Fonte::Casa => "casa",
        Fonte::Pc => "pc",
        Fonte::Prova => "avviso",
    })
}

pub fn pulsanti_ui(p: &[Pulsante]) -> ModelRc<PulsanteUi> {
    let voci: Vec<PulsanteUi> =
        p.iter().map(|p| PulsanteUi { id: p.id.as_str().into(), testo: p.testo.as_str().into() }).collect();
    ModelRc::new(VecModel::from(voci))
}

fn ritaglia(ui: &Pillola, larghezza: f32) {
    RITAGLIO.with(|r| r.set(larghezza));
    finestre::ritaglia_al_centro(ui.window(), larghezza);
}

pub fn imposta(ui: &Pillola, v: &VistaPillola) {
    let prima = RITAGLIO.with(Cell::get);
    let (aspetto, ferma) = match &v.aspetto {
        AspettoPillola::Riposo => ("riposo", false),
        AspettoPillola::Puntino => ("puntino", false),
        AspettoPillola::Allargata { ferma, .. } => ("allargata", *ferma),
    };
    ui.set_umore(v.umore.come_stringa().into());
    ui.set_aspetto(aspetto.into());
    ui.set_ferma(ferma);
    ui.set_livello(v.livello.come_stringa().into());
    ui.set_messaggio(v.testo.as_str().into());
    if aspetto == "allargata" {
        ui.set_icona_fonte(icona_fonte(v.fonte));
    }
    ui.set_pulsanti(pulsanti_ui(&v.pulsanti));

    let dopo = ui.get_larghezza_visibile();
    let giro = GIRO.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    if dopo >= prima {
        ritaglia(ui, dopo);
    } else {
        let debole = ui.as_weak();
        slint::Timer::single_shot(DOPO_ANIMAZIONE, move || {
            if GIRO.with(Cell::get) != giro {
                return;
            }
            if let Some(ui) = debole.upgrade() {
                ritaglia(&ui, ui.get_larghezza_visibile());
            }
        });
    }
}

/// Finestra appena nata, spostata, DPI o monitor cambiati: posizione e ritaglio di nuovo.
pub fn al_cambio_schermo(ui: &Pillola) {
    finestre::posiziona_pillola(ui.window());
    finestre::ritaglia_al_centro(ui.window(), RITAGLIO.with(Cell::get));
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
}
