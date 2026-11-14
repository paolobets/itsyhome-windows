//! Controlli sull'interfaccia (`.slint`) che girano in `cargo test`, senza finestre: i sorgenti
//! (colori a mano) e i componenti creati col backend di prova di Slint (misure, accessibilità).

use crate::{finestre, layout, sistema, Pannello, Pillola};
use std::cell::Cell;
use std::path::{Path, PathBuf};

thread_local! {
    static PIATTAFORMA: Cell<bool> = const { Cell::new(false) };
}

/// Il backend di prova di Slint per questo thread (ogni test ha il suo): nessuna finestra, nessun
/// ciclo di eventi, il tempo delle animazioni è finto.
pub fn piattaforma_di_prova() {
    if !PIATTAFORMA.with(|p| p.replace(true)) {
        i_slint_backend_testing::init_no_event_loop();
    }
}

/// Tutti i file `.slint` dell'assistente e della cornice della gestione.
fn file_slint() -> Vec<PathBuf> {
    fn raccogli(cartella: &Path, file: &mut Vec<PathBuf>) {
        for voce in std::fs::read_dir(cartella).expect("cartella dell'interfaccia leggibile") {
            let percorso = voce.expect("voce leggibile").path();
            if percorso.is_dir() {
                raccogli(&percorso, file);
            } else if percorso.extension().is_some_and(|e| e == "slint") {
                file.push(percorso);
            }
        }
    }
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut file = Vec::new();
    raccogli(&crate_dir.join("ui"), &mut file);
    raccogli(&crate_dir.join("../gestione/ui"), &mut file);
    assert!(file.len() >= 10, "trovati solo {} file .slint", file.len());
    file
}

/// Il codice di una riga senza il commento `//` (le stringhe dell'interfaccia non contengono `//`).
fn senza_commento(riga: &str) -> &str {
    riga.split("//").next().unwrap_or("")
}

/// Un letterale di colore nel codice: `#` seguito da 3–8 cifre esadecimali, o `rgb(` / `rgba(`.
fn colore_a_mano(codice: &str) -> bool {
    if codice.contains("rgb(") || codice.contains("rgba(") {
        return true;
    }
    codice.match_indices('#').any(|(i, _)| {
        let cifre = codice[i + 1..].chars().take_while(char::is_ascii_hexdigit).count();
        (3..=8).contains(&cifre)
    })
}

#[test]
fn riconosce_i_colori_a_mano() {
    assert!(colore_a_mano("background: #fb923c;"));
    assert!(colore_a_mano("color: rgba(255, 255, 255, 0.04);"));
    assert!(!colore_a_mano("background: Tema.card-fondo;"));
    assert!(!colore_a_mano("text: \"+\" + altre;"));
}

#[test]
fn ui_senza_colori_a_mano() {
    // FE7: i colori stanno solo in tema.slint, come token di ruolo.
    let mut trovati = Vec::new();
    for percorso in file_slint() {
        if percorso.file_name().is_some_and(|n| n == "tema.slint") {
            continue;
        }
        let testo = std::fs::read_to_string(&percorso).expect("file .slint leggibile");
        for (n, riga) in testo.lines().enumerate() {
            if colore_a_mano(senza_commento(riga)) {
                trovati.push(format!("{}:{}: {}", percorso.display(), n + 1, riga.trim()));
            }
        }
    }
    assert!(trovati.is_empty(), "colori scritti a mano fuori da tema.slint:\n{}", trovati.join("\n"));
}

#[test]
fn misure_slint_e_rust() {
    // FE10: le misure che Rust usa per posizione, ritaglio e scarti degli occhi coincidono con
    // quelle di faccia.slint, pillola.slint e pannello.slint (prima: solo debug_assert all'avvio).
    piattaforma_di_prova();
    let pillola = Pillola::new().expect("pillola creata col backend di prova");
    assert_eq!(pillola.get_margine_aura(), finestre::MARGINE_AURA, "margine dell'aura");
    assert_eq!(
        (pillola.get_larghezza_riposo(), pillola.get_altezza_riposo()),
        (finestre::LARGHEZZA_PILLOLA, finestre::ALTEZZA_PILLOLA),
        "pillola a riposo"
    );
    assert_eq!(
        (pillola.get_larghezza_attenzione(), pillola.get_altezza_attenzione()),
        (finestre::LARGHEZZA_ATTENZIONE, finestre::ALTEZZA_ATTENZIONE),
        "pillola che chiama"
    );
    assert_eq!(
        (pillola.get_larghezza_bloccante(), pillola.get_altezza_bloccante()),
        (finestre::LARGHEZZA_BLOCCANTE, finestre::ALTEZZA_BLOCCANTE),
        "pillola bloccante"
    );
    assert_eq!(
        (pillola.get_larghezza_finestra(), pillola.get_altezza_finestra()),
        (finestre::LARGHEZZA_FINESTRA_PILLOLA, finestre::ALTEZZA_FINESTRA_PILLOLA),
        "finestra della pillola"
    );
    assert_eq!(
        (pillola.get_scarto_occhi_attenzione(), pillola.get_scarto_occhi_attenzione_y()),
        finestre::SCARTO_OCCHI_ATTENZIONE,
        "occhi della pillola che chiama"
    );
    assert_eq!(
        (pillola.get_scarto_occhi_bloccante(), pillola.get_scarto_occhi_bloccante_y()),
        finestre::SCARTO_OCCHI_BLOCCANTE,
        "occhi della pillola bloccante"
    );
    assert_eq!(
        (pillola.get_larghezza_linea(), pillola.get_altezza_linea()),
        (sistema::ritirata::LARGHEZZA_LINEA, sistema::ritirata::ALTEZZA_LINEA),
        "linea della ritirata"
    );

    let pannello = Pannello::new().expect("pannello creato col backend di prova");
    assert_eq!(pannello.get_larghezza_pannello(), finestre::LARGHEZZA_PANNELLO, "larghezza del pannello");
    assert_eq!(pannello.get_altezza_riga(), finestre::ALTEZZA_RIGA_PANNELLO, "riga alta del pannello");
    assert_eq!(pannello.get_numero_layout() as usize, layout::TUTTI.len(), "layout della Panoramica");
    drop((pillola, pannello));
}

mod accessibilita {
    //! FE13/FE16: ruoli, nomi e stati che Narrator legge (UI Automation li riceve da AccessKit).
    use super::piattaforma_di_prova;
    use crate::{
        AspettoPillolaUi, FinestraImpostazioni, LivelloUi, Pannello, Pillola, PulsanteUi, StatPcUi, VoceAppUi,
        VoceAttesaUi,
    };
    use i_slint_backend_testing::{AccessibleRole, ElementHandle};
    use slint::{ComponentHandle, ModelRc, VecModel};

    fn elemento(radice: &impl i_slint_backend_testing::ElementRoot, nome: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(radice, nome)
            .next()
            .unwrap_or_else(|| panic!("nessun elemento accessibile «{nome}»"))
    }

    /// Un pannello aperto nel backend di prova (finestra finta): le parti create con `if` e `for`
    /// esistono solo in una finestra mostrata.
    fn pannello() -> Pannello {
        let p = Pannello::new().expect("pannello");
        p.set_aperto(true);
        p.show().expect("pannello mostrato nel backend di prova");
        p
    }

    fn app() -> ModelRc<VoceAppUi> {
        let voce = |id: &str, nome: &str| VoceAppUi { id: id.into(), nome: nome.into(), icona: Default::default() };
        ModelRc::new(VecModel::from(vec![
            voce("panoramica", "Panoramica"),
            voce("casa", "Casa"),
            voce("pc", "PC"),
            voce("impostazioni", "Impostazioni"),
        ]))
    }

    #[test]
    fn dock_e_puntini_sono_schede_con_lo_stato() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(app());
        let panoramica = elemento(&p, "Panoramica");
        assert_eq!(panoramica.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!(panoramica.accessible_item_selected(), Some(true));
        assert_eq!((panoramica.accessible_item_index(), panoramica.accessible_item_count()), (Some(0), Some(4)));
        let casa = elemento(&p, "Casa");
        assert_eq!(casa.accessible_item_selected(), Some(false));
        assert_eq!(casa.accessible_item_index(), Some(1));

        let orologio = elemento(&p, "Layout Orologio");
        assert_eq!(orologio.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!(orologio.accessible_item_selected(), Some(true));
        p.set_layout(1);
        assert_eq!(elemento(&p, "Layout Orologio").accessible_item_selected(), Some(false));
        assert_eq!(elemento(&p, "Layout Quadranti").accessible_item_selected(), Some(true));
        assert_eq!(elemento(&p, "Pannello di ItsyHome").accessible_role(), Some(AccessibleRole::Groupbox));
    }

    #[test]
    fn voce_chiusa_ed_espansa() {
        piattaforma_di_prova();
        let p = pannello();
        let pulsanti = ModelRc::new(VecModel::from(vec![
            PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false },
            PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true },
        ]));
        p.set_attese(ModelRc::new(VecModel::from(vec![VoceAttesaUi {
            id: "v".into(),
            livello: LivelloUi::Bloccante,
            titolo: "Avviso di prova".into(),
            sotto: "Aspetta te · prova".into(),
            dettaglio: "Il testo completo".into(),
            icona: Default::default(),
            pulsanti,
        }])));
        let chiusa = elemento(&p, "Avviso di prova, Aspetta te · prova");
        assert_eq!(chiusa.accessible_role(), Some(AccessibleRole::Button));
        assert_eq!((chiusa.accessible_expandable(), chiusa.accessible_expanded()), (Some(true), Some(false)));

        p.invoke_espandi(0);
        let espansa = elemento(&p, "Avviso di prova, Aspetta te · prova, voce 1 di 1");
        assert_eq!(espansa.accessible_role(), Some(AccessibleRole::Groupbox));
        assert_eq!(espansa.accessible_expanded(), Some(true));
        assert_eq!(espansa.accessible_description().as_deref(), Some("Il testo completo"));
        assert_eq!(elemento(&p, "Nega").accessible_role(), Some(AccessibleRole::Button));
        assert_eq!(elemento(&p, "Consenti").accessible_role(), Some(AccessibleRole::Button));
    }

    #[test]
    fn quadranti_dicono_nome_e_valore() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_layout(1);
        p.set_pc(StatPcUi { pronta: true, cpu: 0.12, cpu_testo: "12%".into(), ..Default::default() });
        let cpu = elemento(&p, "CPU");
        assert_eq!(cpu.accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert_eq!(cpu.accessible_value().as_deref(), Some("12%"));
        // Senza sensori, il quadrante CASA dice lo stato della casa.
        p.set_stato_casa("Casa non connessa".into());
        assert_eq!(elemento(&p, "CASA, Casa non connessa").accessible_role(), Some(AccessibleRole::ProgressIndicator));
    }

    #[test]
    fn pillola_e_un_pulsante_col_nome_giusto() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        assert_eq!(elemento(&p, "ItsyHome").accessible_role(), Some(AccessibleRole::Button));
        p.set_aspetto(AspettoPillolaUi::Puntino);
        assert_eq!(elemento(&p, "ItsyHome, una voce in attesa").accessible_role(), Some(AccessibleRole::Button));
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("La casa chiede di te".into());
        assert_eq!(elemento(&p, "La casa chiede di te").accessible_role(), Some(AccessibleRole::Button));
    }

    #[test]
    fn sezioni_delle_impostazioni_selezionate() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        let ha = elemento(&f, "Home Assistant");
        assert_eq!(ha.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!((ha.accessible_item_selected(), ha.accessible_item_count()), (Some(true), Some(3)));
        f.set_sezione(2);
        assert_eq!(elemento(&f, "Informazioni").accessible_item_selected(), Some(true));
        assert_eq!(elemento(&f, "Home Assistant").accessible_item_selected(), Some(false));
    }

    fn tasto(p: &Pannello, testo: impl Into<slint::SharedString> + Clone) {
        use slint::platform::WindowEvent;
        p.window().dispatch_event(WindowEvent::KeyPressed { text: testo.clone().into() });
        p.window().dispatch_event(WindowEvent::KeyReleased { text: testo.into() });
    }

    #[test]
    fn la_tastiera_regge_col_fuoco_sugli_elementi() {
        // FE13: `tasti` avvolge il pannello e intercetta i tasti prima degli elementi che hanno il
        // fuoco vero; Tab, frecce, Invio ed Esc fanno quello che facevano.
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(app());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        // Apertura finita (nel backend di prova il timer non gira): il fuoco vero va sugli elementi.
        p.global::<crate::Fuochi>().set_pronti(true);
        let chiuso = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiuso.clone();
        p.on_chiudi(move || c.set(true));
        // Contenuto (puntini) → Tab al dock → → su «Casa» → Invio: si apre la vista Casa.
        tasto(&p, Key::Tab);
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "casa");
        // Backspace torna alla Panoramica; Esc chiude.
        tasto(&p, Key::Backspace);
        assert_eq!(p.get_vista(), "panoramica");
        tasto(&p, Key::Escape);
        assert!(chiuso.get(), "Esc chiude il pannello");
    }

    #[test]
    fn invio_non_consente_anche_col_fuoco_sul_pulsante() {
        // Il pulsante «Consenti» ha il fuoco vero (e sa premersi con Invio da solo), ma `tasti`
        // intercetta prima: Invio da solo non concede, Ctrl+Invio sì (UX3/FE15).
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        let pulsanti = ModelRc::new(VecModel::from(vec![
            PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false },
            PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true },
        ]));
        p.set_attese(ModelRc::new(VecModel::from(vec![VoceAttesaUi {
            id: "v".into(),
            livello: LivelloUi::Bloccante,
            titolo: "Permesso".into(),
            sotto: "Aspetta te · prova".into(),
            dettaglio: "Posso?".into(),
            icona: Default::default(),
            pulsanti,
        }])));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione(move |_, id| a.borrow_mut().push(id.to_string()));
        p.invoke_espandi(0);
        // Il fuoco parte da «Nega»: → su «Consenti», poi Invio da solo.
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio da solo ha consentito: {:?}", azioni.borrow());
        // Ctrl+Invio sì.
        use slint::platform::WindowEvent;
        p.window().dispatch_event(WindowEvent::KeyPressed { text: Key::Control.into() });
        tasto(&p, Key::Return);
        p.window().dispatch_event(WindowEvent::KeyReleased { text: Key::Control.into() });
        assert_eq!(*azioni.borrow(), ["consenti"]);
    }

    /// Un pannello con la voce «Permesso» (Nega / Consenti) espansa e il fuoco vero su «Consenti»;
    /// restituisce le azioni arrivate.
    fn pannello_sul_consenso() -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        let pulsanti = ModelRc::new(VecModel::from(vec![
            PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false },
            PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true },
        ]));
        p.set_attese(ModelRc::new(VecModel::from(vec![VoceAttesaUi {
            id: "v".into(),
            livello: LivelloUi::Bloccante,
            titolo: "Permesso".into(),
            sotto: "Aspetta te · prova".into(),
            dettaglio: "Posso?".into(),
            icona: Default::default(),
            pulsanti,
        }])));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione(move |_, id| a.borrow_mut().push(id.to_string()));
        p.invoke_espandi(0);
        tasto(&p, Key::RightArrow);
        (p, azioni)
    }

    /// `tasto` con i modificatori premuti prima e rilasciati dopo.
    fn con_modificatori(p: &Pannello, modificatori: &[slint::platform::Key], t: slint::platform::Key) {
        use slint::platform::WindowEvent;
        for m in modificatori {
            p.window().dispatch_event(WindowEvent::KeyPressed { text: (*m).into() });
        }
        tasto(p, t);
        for m in modificatori.iter().rev() {
            p.window().dispatch_event(WindowEvent::KeyReleased { text: (*m).into() });
        }
    }

    #[test]
    fn consenti_solo_con_ctrl_invio_esatto() {
        // Giro di correzioni 1, rilievo 1 (bloccante): col fuoco vero su «Consenti» nessuna
        // combinazione diversa da Ctrl+Invio deve concedere (UX3/FE15). AltGr su Windows arriva
        // come Ctrl+Alt.
        use slint::platform::Key;
        let casi: [(&str, &[Key], Key); 9] = [
            ("Invio", &[], Key::Return),
            ("Spazio", &[], Key::Space),
            ("Alt+Invio", &[Key::Alt], Key::Return),
            ("Ctrl+Spazio", &[Key::Control], Key::Space),
            ("Maiusc+Invio", &[Key::Shift], Key::Return),
            ("Alt+Spazio", &[Key::Alt], Key::Space),
            ("AltGr+Invio (Ctrl+Alt)", &[Key::Control, Key::Alt], Key::Return),
            ("AltGr+Invio", &[Key::AltGr], Key::Return),
            ("Maiusc+Ctrl+Invio", &[Key::Shift, Key::Control], Key::Return),
        ];
        for (nome, modificatori, t) in casi {
            let (p, azioni) = pannello_sul_consenso();
            con_modificatori(&p, modificatori, t);
            assert!(azioni.borrow().is_empty(), "{nome} ha concesso: {:?}", azioni.borrow());
        }
        let (p, azioni) = pannello_sul_consenso();
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"], "Ctrl+Invio concede");
    }

    #[test]
    fn la_pillola_non_concede_da_tastiera() {
        // Giro di correzioni 1, rilievo 2: anche con la finestra della pillola a fuoco, Tab non
        // porta ai suoi pulsanti e Invio o Spazio non premono nulla (il permesso nella pillola
        // si dà col clic o, col testo intero, dal pannello).
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("Posso?".into());
        p.set_pulsanti(ModelRc::new(VecModel::from(vec![
            PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false },
            PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true },
        ])));
        p.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione(move |id| a.borrow_mut().push(id.to_string()));
        for _ in 0..3 {
            for t in [Key::Tab, Key::Return, Key::Space] {
                p.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
                p.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
            }
        }
        assert!(azioni.borrow().is_empty(), "la tastiera ha premuto: {:?}", azioni.borrow());
    }
}
