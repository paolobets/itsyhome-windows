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

/// Il codice di una riga senza il commento `//`, tagliato solo fuori dalle stringhe (un URL in un
/// `text:` non è un commento).
fn senza_commento(riga: &str) -> &str {
    let mut in_stringa = false;
    let mut prima = ' ';
    for (i, c) in riga.char_indices() {
        if c == '"' && prima != '\\' {
            in_stringa = !in_stringa;
        } else if !in_stringa && c == '/' && prima == '/' {
            return &riga[..i - 1];
        }
        prima = c;
    }
    riga
}

/// Un colore scritto a mano nel codice: `#` seguito da 3–8 cifre esadecimali, `rgb(` / `rgba(`,
/// `hsv(` o i colori per nome di Slint (`Colors.red`).
fn colore_a_mano(codice: &str) -> bool {
    if ["rgb(", "rgba(", "hsv(", "Colors."].iter().any(|m| codice.contains(m)) {
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
    assert!(colore_a_mano("color: Colors.red;"));
    assert!(colore_a_mano("color: Colors.hsv(120, 0.5, 0.5);"));
    assert!(!colore_a_mano("background: Tema.card-fondo;"));
    assert!(!colore_a_mano("text: \"+\" + altre;"));
    // Il commento si toglie solo fuori dalle stringhe.
    assert!(colore_a_mano(senza_commento("text: \"https://slint.dev\"; color: #fff;")));
    assert!(!colore_a_mano(senza_commento("color: Tema.testo; // era #fff")));
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

#[test]
fn linea_riempita_a_meta() {
    // M2 Task 5: a pillola ritirata con riempimento 0.5 il riempimento è largo metà linea (150 px);
    // senza avanzamento (1.0) è tutta la linea, come in M1.
    piattaforma_di_prova();
    let pillola = Pillola::new().expect("pillola creata col backend di prova");
    pillola.set_ritirata(true);
    assert_eq!(pillola.get_larghezza_riempimento(), sistema::ritirata::LARGHEZZA_LINEA, "linea piena di partenza");
    pillola.set_riempimento_linea(0.5);
    assert_eq!(pillola.get_larghezza_riempimento(), 150.0, "riempimento a metà");
    pillola.set_riempimento_linea(0.0);
    assert_eq!(pillola.get_larghezza_riempimento(), 0.0, "riempimento vuoto");
}

mod accessibilita {
    //! FE13/FE16: ruoli, nomi e stati che Narrator legge (UI Automation li riceve da AccessKit).
    use super::piattaforma_di_prova;
    use crate::{
        AspettoPillolaUi, FinestraImpostazioni, LivelloUi, Pannello, Pillola, PulsanteUi, StatPcUi, VoceAppUi,
        VoceAttesaUi,
    };
    use i_slint_backend_testing::{AccessibleRole, ElementHandle};
    use slint::{ComponentHandle, Model, ModelRc, VecModel};

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
            voce("claude", "Claude"),
            voce("pc", "PC"),
            voce("collegamenti", "Collegamenti"),
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
        assert_eq!((panoramica.accessible_item_index(), panoramica.accessible_item_count()), (Some(0), Some(5)));
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

    /// 18/11: nella voce espansa il titolo si legge intero (va a capo, niente «…») e la riga sotto
    /// e il testo scendono di conseguenza; un titolo corto occupa una riga sola.
    #[test]
    fn titolo_della_voce_espansa_va_a_capo() {
        use i_slint_backend_testing::ElementHandle;
        piattaforma_di_prova();
        let voce = |titolo: &str| VoceAttesaUi {
            id: "v".into(),
            livello: LivelloUi::Bloccante,
            titolo: titolo.into(),
            sotto: "Aspetta te · Prova".into(),
            dettaglio: "Il testo completo".into(),
            icona: Default::default(),
            pulsanti: ModelRc::new(VecModel::from(Vec::<PulsanteUi>::new())),
        };
        let altezza = |p: &Pannello, id: &str| {
            let e = ElementHandle::find_by_element_id(p, id).next().unwrap_or_else(|| panic!("nessun elemento {id}"));
            (e.absolute_position().y, e.size().height)
        };
        let p = pannello();
        p.set_attese(ModelRc::new(VecModel::from(vec![voce("Permesso breve")])));
        p.invoke_espandi(0);
        let (_, una_riga) = altezza(&p, "VoceEspansa::titolo");
        let (y_sotto_corto, _) = altezza(&p, "VoceEspansa::sotto-testo");

        let lungo = concat!(
            "Consentire a un programma di prova di cancellare la cartella dei documenti temporanei ",
            "e di tutte le sue sottocartelle, compresi i file nascosti e quelli di sistema?"
        );
        p.set_attese(ModelRc::new(VecModel::from(vec![voce(lungo)])));
        let (_, piu_righe) = altezza(&p, "VoceEspansa::titolo");
        let (y_sotto_lungo, _) = altezza(&p, "VoceEspansa::sotto-testo");
        assert!(piu_righe > una_riga * 1.5, "titolo lungo a capo: {piu_righe} contro {una_riga}");
        assert!(y_sotto_lungo > y_sotto_corto, "la riga sotto scende: {y_sotto_lungo} contro {y_sotto_corto}");
    }

    /// 18/11 (Paolo): «Nega» e «Consenti» sono entrambi neutri, senza tinta d'azione; la tinta resta
    /// all'ultimo pulsante di una voce che non è un permesso («Apri», «Riprova»).
    #[test]
    fn nega_e_consenti_sono_neutri() {
        use crate::{RegolaPulsanti, TipoPulsante};
        piattaforma_di_prova();
        let p = pannello();
        let regola = p.global::<RegolaPulsanti>();
        // «Nega» (primo, non delicato) e «Consenti» (ultimo, delicato): nessuno ha la tinta.
        assert_eq!(regola.invoke_tipo(false, false), TipoPulsante::Neutro, "Nega");
        assert_eq!(regola.invoke_tipo(true, true), TipoPulsante::Neutro, "Consenti");
        // Un'azione sicura in coda la mantiene.
        assert_eq!(regola.invoke_tipo(true, false), TipoPulsante::Azione, "Apri");
        // La regola vale solo se pannello e pillola la usano davvero: se tornassero a decidere da
        // sé (ultimo = azione) questo test deve fallire.
        for (nome, sorgente) in [
            ("attesa.slint", include_str!("../ui/attesa.slint")),
            ("pillola.slint", include_str!("../ui/pillola.slint")),
        ] {
            assert!(sorgente.contains("RegolaPulsanti.tipo("), "{nome} non usa RegolaPulsanti.tipo");
            assert!(!sorgente.contains("? TipoPulsante.azione"), "{nome} decide la tinta da sé");
        }
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
        // Nella vista PC la rete è una velocità (testo), CPU un indicatore.
        p.set_vista("pc".into());
        assert_eq!(elemento(&p, "RETE").accessible_role(), Some(AccessibleRole::Text));
        p.set_vista("panoramica".into());
        // Senza sensori, il quadrante CASA dice lo stato della casa.
        p.set_stato_casa("Casa non connessa".into());
        assert_eq!(elemento(&p, "CASA, Casa non connessa").accessible_role(), Some(AccessibleRole::ProgressIndicator));
    }

    /// M2 Task 1: la riga BATTERIA col glifo dice ancora nome, valore e stato per Narrator; il
    /// glifo non è un elemento accessibile (il testo accanto dice tutto).
    #[test]
    fn riga_batteria_col_glifo() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_vista("pc".into());
        p.set_pc(StatPcUi {
            pronta: true,
            ha_batteria: true,
            batteria_testo: "18%".into(),
            batteria_frazione: 0.18,
            tinta_batteria: crate::TintaBatteriaUi::Informazione,
            ..Default::default()
        });
        let riga = elemento(&p, "BATTERIA");
        assert_eq!(riga.accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert_eq!(riga.accessible_value().as_deref(), Some("18%"));
        assert_eq!(riga.accessible_description().as_deref(), Some("A batteria"));
        // Il glifo c'è (19 × 8 px), sta nella riga a sinistra del valore e non è accessibile: il
        // test fallisce se la riga torna senza glifo.
        let glifi: Vec<_> = ElementHandle::find_by_element_type_name(&p, "GlifoBatteria").collect();
        assert_eq!(glifi.len(), 1, "un solo glifo nella riga BATTERIA");
        assert_eq!((glifi[0].size().width, glifi[0].size().height), (19.0, 8.0));
        assert_ne!(glifi[0].accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert!(glifi[0].accessible_label().is_none(), "il glifo non ha un nome per Narrator");
        let (riga_pos, glifo_pos) = (riga.absolute_position(), glifi[0].absolute_position());
        assert!(
            glifo_pos.x > riga_pos.x + riga.size().width / 2.0,
            "il glifo sta nella colonna del valore, a destra"
        );
        // Senza batteria niente riga e niente glifo.
        p.set_pc(StatPcUi { pronta: true, ..Default::default() });
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "GlifoBatteria").count(), 0);
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

    /// 16/11 (Paolo): il clic destro sulla pillola non fa nulla (il menu sta nel «⋯» del
    /// pannello); il clic sinistro apre il pannello come prima.
    #[test]
    fn clic_destro_sulla_pillola_non_fa_nulla() {
        use slint::platform::{PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&str>::new()));
        let e = eventi.clone();
        p.on_cliccata(move || e.borrow_mut().push("cliccata"));
        let pillola = elemento(&p, "ItsyHome");
        let (pos, lato) = (pillola.absolute_position(), pillola.size());
        let centro = slint::LogicalPosition::new(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        let clic = |button| {
            p.window().dispatch_event(WindowEvent::PointerMoved { position: centro });
            p.window().dispatch_event(WindowEvent::PointerPressed { position: centro, button });
            p.window().dispatch_event(WindowEvent::PointerReleased { position: centro, button });
        };
        clic(PointerEventButton::Right);
        assert!(eventi.borrow().is_empty(), "il clic destro non apre il pannello");
        clic(PointerEventButton::Left);
        assert_eq!(*eventi.borrow(), ["cliccata"]);
    }

    /// Task 14d: «Mostra icona nella barra» in Impostazioni › Sistema, spenta di serie; il
    /// commutatore la accende (e lo dice a Rust); fuori da Windows non si mostra.
    #[test]
    fn interruttore_dell_icona_nella_barra() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(5);
        f.show().expect("impostazioni mostrate nel backend di prova");
        let chiesti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let c = chiesti.clone();
        f.on_cambia_icona_barra(move |si| c.borrow_mut().push(si));
        // Lo stesso nome ha anche il titolo (un testo): si prende il commutatore.
        let interruttore = ElementHandle::find_by_accessible_label(&f, "Mostra icona nella barra")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
            .expect("commutatore «Mostra icona nella barra»");
        assert_eq!(interruttore.accessible_checked(), Some(false), "spenta di serie");
        interruttore.invoke_accessible_default_action();
        assert_eq!(*chiesti.borrow(), [true]);
        assert_eq!(interruttore.accessible_checked(), Some(true));
        f.set_icona_barra_disponibile(false);
        assert!(ElementHandle::find_by_accessible_label(&f, "Mostra icona nella barra").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&f, "Avvia con il sistema").next().is_some());
    }

    /// Fuori da Windows (nessun menu né icona nella barra) Impostazioni › Sistema ha «Esci
    /// dall'assistente» con il pulsante «Esci»: si vede solo con il flag acceso, ha ruolo e nome per
    /// Narrator, si raggiunge con Tab e si preme con Invio, e chiama il callback che chiude l'app.
    #[test]
    fn esci_nelle_impostazioni_fuori_da_windows() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(5);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let chiamate = std::rc::Rc::new(std::cell::Cell::new(0));
        let c = chiamate.clone();
        f.on_esci(move || c.set(c.get() + 1));
        let ce = |nome: &str| ElementHandle::find_by_accessible_label(&f, nome).next().is_some();
        // Spento (di serie, e su Windows): né riga né pulsante.
        assert!(!ce("Esci dall'assistente") && !ce("Esci"));
        f.set_fuori_da_windows(true);
        assert!(ce("Esci dall'assistente"), "la riga si vede");
        let esci = ElementHandle::find_by_accessible_label(&f, "Esci")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .expect("pulsante «Esci»");
        // Come lo aziona Narrator.
        esci.invoke_accessible_default_action();
        assert_eq!(chiamate.get(), 1, "il pulsante chiama il callback");
        // Da tastiera: Tab fino al pulsante (al più 20 passi), poi Invio.
        let tasto = |t: Key| {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
        };
        for _ in 0..20 {
            if chiamate.get() == 2 {
                break;
            }
            tasto(Key::Tab);
            tasto(Key::Return);
            // Invio su una voce del menu laterale cambia sezione: si torna a Sistema, il fuoco resta.
            f.set_sezione(5);
        }
        assert_eq!(chiamate.get(), 2, "Tab raggiunge il pulsante e Invio lo preme");
        f.set_fuori_da_windows(false);
        assert!(!ce("Esci dall'assistente") && !ce("Esci"), "spento di nuovo: sparito");
    }

    #[test]
    fn sezioni_delle_impostazioni_selezionate() {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        let ha = elemento(&f, "Home Assistant");
        assert_eq!(ha.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!((ha.accessible_item_selected(), ha.accessible_item_count()), (Some(true), Some(7)));
        assert_eq!(elemento(&f, "Collegamenti").accessible_item_index(), Some(2));
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_INFORMAZIONI);
        assert_eq!(elemento(&f, "Informazioni").accessible_item_selected(), Some(true));
        assert_eq!(elemento(&f, "Home Assistant").accessible_item_selected(), Some(false));
    }

    /// Task 14e: Impostazioni › Dispositivi. Ogni pulsante ha il suo nome per Narrator e porta a
    /// Rust l'azione con la riga, la chiave e la stanza; le frecce ai bordi sono spente; la scelta
    /// dell'icona si apre, sceglie e si chiude.
    #[test]
    fn scheda_dispositivi_pulsanti_e_scelta_icona() {
        use crate::{GenereUi, RigaDispositivoUi, TipoRigaUi};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        let riga = |id: &str, tipo, chiave: &str, area: &str, nome: &str, su, giu| RigaDispositivoUi {
            id: id.into(),
            tipo,
            chiave: chiave.into(),
            area: area.into(),
            nome: nome.into(),
            sotto: "".into(),
            icona: 6,
            genere: GenereUi::Luce,
            su,
            giu,
            puo_icona: tipo == TipoRigaUi::TestataStanza,
            ..Default::default()
        };
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![
            riga("h:", TipoRigaUi::TestataPreferiti, "__preferiti__", "", "Preferiti", false, false),
            riga("s:cucina", TipoRigaUi::TestataStanza, "cucina", "cucina", "Cucina", false, true),
            riga("d:light.a", TipoRigaUi::Dispositivo, "light.a", "cucina", "Luce A", false, true),
            riga("d:light.b", TipoRigaUi::Dispositivo, "light.b", "cucina", "Luce B", true, false),
            riga("s:sala", TipoRigaUi::TestataStanza, "sala", "sala", "Sala", true, false),
        ])));
        f.set_nomi_icone(ModelRc::new(VecModel::from(
            crate::stanze::ICONE.iter().map(|(_, n)| slint::SharedString::from(*n)).collect::<Vec<_>>(),
        )));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (a1, a2) = (azioni.clone(), azioni.clone());
        f.on_azione_dispositivo(move |azione, id, chiave, area| a1.borrow_mut().push(format!("{azione} {id} {chiave} {area}")));
        f.on_scegli_icona(move |area, i| a2.borrow_mut().push(format!("icona {area} {i}")));

        assert_eq!(elemento(&f, "Sposta su «Luce A»").accessible_enabled(), Some(false), "già in cima");
        // I comandi stanno a destra della riga, larga quanto la colonna del contenuto (prima le
        // righe nascevano strette e i pulsanti finivano sopra i nomi).
        let occhio = elemento(&f, "Nascondi «Luce B»");
        let larghezza = f.window().size().to_logical(f.window().scale_factor()).width;
        assert!(occhio.absolute_position().x > larghezza - 80.0, "occhio a x {} su {larghezza}", occhio.absolute_position().x);
        elemento(&f, "Sposta su «Luce B»").invoke_accessible_default_action();
        elemento(&f, "Aggiungi «Luce A» ai preferiti").invoke_accessible_default_action();
        elemento(&f, "Nascondi «Luce B»").invoke_accessible_default_action();
        elemento(&f, "Nascondi la stanza «Sala»").invoke_accessible_default_action();
        elemento(&f, "Sposta su «Sala»").invoke_accessible_default_action();
        elemento(&f, "Comprimi «Cucina»").invoke_accessible_default_action();
        assert_eq!(
            *azioni.borrow(),
            [
                "su d:light.b light.b cucina",
                "preferito d:light.a light.a cucina",
                "nascondi d:light.b light.b cucina",
                "nascondi s:sala sala sala",
                "su s:sala sala sala",
                "comprimi s:cucina cucina cucina",
            ]
        );
        // La scelta dell'icona: si apre col nome della stanza, la scelta va a Rust e si chiude.
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        let scelta = elemento(&f, "ICONA DI «Cucina»");
        assert_eq!(scelta.accessible_role(), Some(AccessibleRole::Groupbox));
        let pentola = elemento(&f, "Pentola");
        assert_eq!(pentola.accessible_item_selected(), Some(true), "l'icona di adesso è evidenziata");
        elemento(&f, "Videogiochi").invoke_accessible_default_action();
        assert_eq!(azioni.borrow().last().map(String::as_str), Some("icona cucina 36"));
        assert!(ElementHandle::find_by_accessible_label(&f, "ICONA DI «Cucina»").next().is_none(), "chiusa");
    }

    /// 18/11: le istruzioni di «Dispositivi» (ordine, stella, occhio) non compaiono con l'elenco vuoto.
    #[test]
    fn dispositivi_senza_righe_niente_istruzioni() {
        use crate::{RigaDispositivoUi, TipoRigaUi};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        let istruzioni = "Ordine con le frecce, preferiti con la stella, nascosti con l'occhio.";
        assert!(ElementHandle::find_by_accessible_label(&f, "Nessun dispositivo da gestire").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&f, istruzioni).next().is_none(), "istruzioni senza elenco");
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![RigaDispositivoUi {
            id: "s:cucina".into(),
            tipo: TipoRigaUi::TestataStanza,
            chiave: "cucina".into(),
            nome: "Cucina".into(),
            ..Default::default()
        }])));
        assert!(ElementHandle::find_by_accessible_label(&f, istruzioni).next().is_some(), "istruzioni con l'elenco");
    }

    /// Task 14e: la scelta dell'icona da tastiera (frecce, Invio) ed Esc che la chiude senza
    /// chiudere la finestra.
    #[test]
    fn scelta_icona_da_tastiera() {
        use crate::{RigaDispositivoUi, TipoRigaUi};
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![RigaDispositivoUi {
            id: "s:cucina".into(),
            tipo: TipoRigaUi::TestataStanza,
            chiave: "cucina".into(),
            area: "cucina".into(),
            nome: "Cucina".into(),
            icona: 6,
            puo_icona: true,
            ..Default::default()
        }])));
        f.set_nomi_icone(ModelRc::new(VecModel::from(
            crate::stanze::ICONE.iter().map(|(_, n)| slint::SharedString::from(*n)).collect::<Vec<_>>(),
        )));
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2) = (eventi.clone(), eventi.clone());
        f.on_scegli_icona(move |area, i| e1.borrow_mut().push(format!("icona {area} {i}")));
        f.on_chiudi(move || e2.borrow_mut().push("chiudi finestra".into()));
        let tasto = |t: Key| {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
        };
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        // Esc chiude la scelta, non la finestra.
        tasto(Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "ICONA DI «Cucina»").next().is_none());
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
        // Di nuovo: dalla 6 (Pentola) → giù di una riga (16) e a destra (17), Invio.
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        tasto(Key::DownArrow);
        tasto(Key::RightArrow);
        tasto(Key::Return);
        assert_eq!(*eventi.borrow(), ["icona cucina 17"]);
    }

    /// Task 14f, N1 del 14e: un clic dentro il riquadro della scelta dell'icona ma fuori da
    /// un'icona (titolo, margini) non la chiude; un clic fuori sì.
    #[test]
    fn scelta_icona_trattiene_i_clic_dentro_il_riquadro() {
        use crate::{RigaDispositivoUi, TipoRigaUi};
        use slint::platform::{PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(1);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.set_righe_dispositivi(ModelRc::new(VecModel::from(vec![RigaDispositivoUi {
            id: "s:cucina".into(),
            tipo: TipoRigaUi::TestataStanza,
            chiave: "cucina".into(),
            area: "cucina".into(),
            nome: "Cucina".into(),
            icona: 6,
            puo_icona: true,
            ..Default::default()
        }])));
        f.set_nomi_icone(ModelRc::new(VecModel::from(
            crate::stanze::ICONE.iter().map(|(_, n)| slint::SharedString::from(*n)).collect::<Vec<_>>(),
        )));
        // Due volte: il primo clic su elementi appena nati da un `if` il backend di prova non lo
        // consegna sempre; col secondo il controllo vale (senza il rimedio il titolo la chiudeva).
        let clic = |x: f32, y: f32| {
            let posizione = slint::LogicalPosition::new(x, y);
            let button = PointerEventButton::Left;
            for _ in 0..2 {
                f.window().dispatch_event(WindowEvent::PointerMoved { position: posizione });
                f.window().dispatch_event(WindowEvent::PointerPressed { position: posizione, button });
                f.window().dispatch_event(WindowEvent::PointerReleased { position: posizione, button });
            }
        };
        let aperta = || ElementHandle::find_by_accessible_label(&f, "ICONA DI «Cucina»").next().is_some();
        elemento(&f, "Cambia icona di «Cucina»").invoke_accessible_default_action();
        let riquadro = elemento(&f, "ICONA DI «Cucina»");
        let (pos, lato) = (riquadro.absolute_position(), riquadro.size());
        // Sul titolo e nell'angolo in basso a destra (sotto l'ultima riga, a destra dell'ultima colonna).
        clic(pos.x + 24.0, pos.y + 24.0);
        assert!(aperta(), "un clic sul titolo l'ha chiusa");
        clic(pos.x + lato.width - 6.0, pos.y + lato.height - 6.0);
        assert!(aperta(), "un clic nel margine l'ha chiusa");
        // Sotto il riquadro, ancora nell'elenco: la chiude come Esc.
        clic(pos.x + 24.0, pos.y + lato.height + 20.0);
        assert!(!aperta(), "un clic fuori dal riquadro la chiude");
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

    /// Task 14i: in alto a destra il pulsante «⋯» («Altre opzioni»), che chiede il menu dell'isola
    /// col mouse, con Narrator e da tastiera; nel dock (le app vere del registro) niente Impostazioni.
    #[test]
    fn altre_opzioni_nella_riga_alta() {
        use slint::platform::{PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(crate::pannello::voci_app());
        let menu = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(f32, f32, bool)>::new()));
        let m = menu.clone();
        p.on_menu(move |x, y, tastiera| m.borrow_mut().push((x, y, tastiera)));
        let piu = elemento(&p, "Altre opzioni");
        assert_eq!(piu.accessible_role(), Some(AccessibleRole::Button));
        let (pos, lato) = (piu.absolute_position(), piu.size());
        assert!(lato.width >= 28.0 && lato.height >= 28.0, "bersaglio di almeno 28 × 28: {lato:?}");
        assert!(pos.x > 700.0 && pos.y < 10.0, "in alto a destra: {pos:?}");
        assert!(ElementHandle::find_by_accessible_label(&p, "Collegamenti").next().is_some(), "il dock vero è caricato");
        assert!(ElementHandle::find_by_accessible_label(&p, "Impostazioni").next().is_none(), "niente Impostazioni nel dock");

        piu.invoke_accessible_default_action();
        assert_eq!(menu.borrow().len(), 1, "Narrator lo preme");
        // Il menu si apre sotto il pulsante, al suo bordo destro (coordinate logiche della finestra);
        // con Narrator come da tastiera: prima voce evidenziata.
        let (x, y, tastiera) = menu.borrow()[0];
        assert_eq!((x, y), (pos.x + lato.width, pos.y + lato.height));
        assert!(tastiera, "da Narrator la prima voce è evidenziata");

        let centro = slint::LogicalPosition::new(pos.x + lato.width / 2.0, pos.y + lato.height / 2.0);
        let clic = || {
            p.window().dispatch_event(WindowEvent::PointerMoved { position: centro });
            p.window().dispatch_event(WindowEvent::PointerPressed { position: centro, button: PointerEventButton::Left });
            p.window().dispatch_event(WindowEvent::PointerReleased { position: centro, button: PointerEventButton::Left });
        };
        clic();
        assert_eq!(menu.borrow().len(), 2, "il clic lo apre");
        assert!(!menu.borrow()[1].2, "col mouse nessuna voce evidenziata");
    }

    /// Task 14i: Tab dal dock arriva a «⋯» (e Maiusc+Tab dalla Panoramica senza voci in attesa);
    /// Invio e Spazio lì aprono il menu; Esc chiude il pannello come prima.
    #[test]
    fn tab_arriva_ad_altre_opzioni() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(app());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&str>::new()));
        let (e1, e2) = (eventi.clone(), eventi.clone());
        p.on_menu(move |_, _, tastiera| e1.borrow_mut().push(if tastiera { "menu" } else { "menu col mouse" }));
        p.on_chiudi(move || e2.borrow_mut().push("chiudi"));
        let e3 = eventi.clone();
        p.on_scorri_layout(move |_| e3.borrow_mut().push("layout"));
        // Contenuto → Tab: dock → Tab: «⋯» → Invio.
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["menu"]);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-menu"), "fuoco su «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::Space);
        assert_eq!(*eventi.borrow(), ["menu", "menu"]);
        // Tab da «⋯» torna al contenuto (Panoramica senza voci in attesa): Invio cambia layout.
        tasto(&p, Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["menu", "menu", "layout"]);
        // Maiusc+Tab dal contenuto: «⋯» (niente «In attesa»), Invio apre il menu.
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["menu", "menu", "layout", "menu"]);
        tasto(&p, Key::Escape);
        assert_eq!(*eventi.borrow(), ["menu", "menu", "layout", "menu", "chiudi"]);
    }

    /// L'id dell'elemento col fuoco vero nel pannello («Pannello::fuoco-menu»).
    fn col_fuoco_pannello(p: &Pannello) -> String {
        let fuoco = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
        fuoco
            .as_ref()
            .and_then(|i| i.element_type_names_and_ids(0))
            .and_then(|v| v.first().map(|(_, id)| id.to_string()))
            .unwrap_or_default()
    }

    fn voce_casa(id: &str, nome: &str) -> crate::PreferitoUi {
        crate::PreferitoUi {
            entity_id: id.into(),
            nome: nome.into(),
            genere: crate::GenereUi::Luce,
            stato: "Spento".into(),
            disponibile: true,
            toccabile: true,
            ..Default::default()
        }
    }

    /// Un pannello sulla vista Casa con due stanze, un preferito e i dispositivi di una stanza;
    /// restituisce gli eventi (stanze aperte e chiuse, tocchi, chiusura del pannello).
    fn pannello_casa() -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(app());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_casa_pronta(true);
        let stanza = |id: &str, nome: &str, quanti| crate::StanzaUi { area_id: id.into(), nome: nome.into(), icona: 0, quanti };
        p.set_stanze(ModelRc::new(VecModel::from(vec![stanza("cucina", "Cucina", 2), stanza("soggiorno", "Soggiorno", 1)])));
        p.set_preferiti(ModelRc::new(VecModel::from(vec![voce_casa("light.preferita", "Preferita")])));
        p.set_dispositivi(ModelRc::new(VecModel::from(vec![
            voce_casa("light.a", "Luce A"),
            voce_casa("light.b", "Luce B"),
        ])));
        p.set_vista("casa".into());
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3, e4) = (eventi.clone(), eventi.clone(), eventi.clone(), eventi.clone());
        p.on_apri_stanza(move |a| e1.borrow_mut().push(format!("apri {a}")));
        p.on_chiudi_stanza(move || e2.borrow_mut().push("chiudi stanza".into()));
        p.on_tocca(move |id| e3.borrow_mut().push(format!("tocca {id}")));
        p.on_chiudi(move || e4.borrow_mut().push("chiudi pannello".into()));
        (p, eventi)
    }

    /// Task 14e/14g: le stanze sono schede di un elenco (nome e quanti dispositivi per Narrator):
    /// una sola selezionata, «Preferiti» o la stanza aperta.
    #[test]
    fn stanze_della_casa_accessibili() {
        let (p, _) = pannello_casa();
        let cucina = elemento(&p, "Cucina, 2 dispositivi");
        assert_eq!(cucina.accessible_role(), Some(AccessibleRole::Tab));
        assert_eq!(cucina.accessible_item_selected(), Some(false));
        assert!(elemento(&p, "Soggiorno, 1 dispositivo").accessible_role().is_some());
        cucina.invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), 0);
        assert_eq!(elemento(&p, "Cucina, 2 dispositivi").accessible_item_selected(), Some(true));
        // La griglia mostra i dispositivi della stanza al posto dei preferiti.
        assert!(ElementHandle::find_by_accessible_label(&p, "Luce A, Spento").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferita, Spento").next().is_none());
        // Giro di correzioni 1: la scheda già selezionata resta tale (come «Preferiti»); si torna ai
        // preferiti con «Preferiti».
        elemento(&p, "Cucina, 2 dispositivi").invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), 0, "un secondo tocco non chiude la stanza");
        scheda(&p, "Preferiti").invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), -1);
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferita, Spento").next().is_some());
    }

    /// La scheda dell'elenco con quel nome (il titolo della griglia può avere lo stesso testo).
    fn scheda(p: &Pannello, nome: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(p, nome)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Tab))
            .unwrap_or_else(|| panic!("nessuna scheda «{nome}»"))
    }

    fn titolo(p: &Pannello, testo: &str) -> bool {
        ElementHandle::find_by_accessible_label(p, testo).any(|e| e.accessible_role() == Some(AccessibleRole::Text))
    }

    /// Task 14g: la prima voce dell'elenco è «Preferiti» (stella, senza numero: vedi il giro di
    /// correzioni 1), selezionata all'apertura; toccandola la griglia torna ai preferiti col titolo
    /// «Preferiti». Nessuna intestazione «STANZE» sopra «Preferiti»; nella riga alta non c'è più
    /// «‹ Indietro».
    #[test]
    fn preferiti_prima_voce_delle_stanze() {
        let (p, eventi) = pannello_casa();
        let elenco = elemento(&p, "Preferiti e stanze");
        assert_eq!(elenco.accessible_role(), Some(AccessibleRole::TabList));
        assert!(ElementHandle::find_by_accessible_label(&p, "STANZE").next().is_none(), "niente «STANZE» sopra «Preferiti»");
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferiti, 1 dispositivo").next().is_none(), "senza numero");
        let preferiti = scheda(&p, "Preferiti");
        assert_eq!(preferiti.accessible_item_selected(), Some(true), "selezionata all'apertura");
        assert_eq!((preferiti.accessible_item_index(), preferiti.accessible_item_count()), (Some(0), Some(3)));
        let cucina = elemento(&p, "Cucina, 2 dispositivi");
        assert_eq!((cucina.accessible_item_index(), cucina.accessible_item_count()), (Some(1), Some(3)));
        assert!(titolo(&p, "Preferiti"), "titolo della griglia");
        assert!(ElementHandle::find_by_accessible_label(&p, "Indietro").next().is_none(), "niente «Indietro»");

        cucina.invoke_accessible_default_action();
        assert_eq!(scheda(&p, "Preferiti").accessible_item_selected(), Some(false));
        assert!(!titolo(&p, "Preferiti"), "titolo: la stanza");
        assert!(titolo(&p, "Cucina"));

        scheda(&p, "Preferiti").invoke_accessible_default_action();
        assert_eq!(p.get_stanza_aperta(), -1);
        assert_eq!(scheda(&p, "Preferiti").accessible_item_selected(), Some(true));
        assert!(ElementHandle::find_by_accessible_label(&p, "Preferita, Spento").next().is_some());
        // «Preferiti» già selezionata: toccarla di nuovo non cambia nulla.
        scheda(&p, "Preferiti").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["apri cucina", "chiudi stanza"]);
    }

    /// Task 14g: «‹ Indietro» non c'è in nessuna vista (si torna alla Panoramica dal dock, con
    /// Backspace o Alt+←).
    #[test]
    fn nessun_indietro_nelle_viste() {
        let (p, _) = pannello_casa();
        for vista in ["casa", "claude", "pc", "collegamenti", "panoramica"] {
            p.set_vista(vista.into());
            assert!(ElementHandle::find_by_accessible_label(&p, "Indietro").next().is_none(), "«Indietro» in {vista}");
        }
    }

    /// Fa passare `ms` millisecondi di tempo finto (timer e animazioni del backend di prova).
    fn avanza(ms: u64) {
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(ms));
    }

    /// Il timer da 16 ms fa partire l'arrivo, poi i 280 ms dell'animazione: in due passi, perché
    /// l'animazione parte dall'istante in cui scatta il timer.
    fn fine_scorrimento() {
        avanza(20);
        avanza(300);
    }

    /// M2 Task 3: la riga sinistra (data e meteo) c'è nella Panoramica; nelle app la parte
    /// sinistra della riga alta resta vuota (niente «Indietro»).
    #[test]
    fn riga_sinistra_solo_nella_panoramica() {
        let (p, _) = pannello_casa();
        p.set_riga_sinistra("VEN 13 NOV · 9°".into());
        p.set_riga_sinistra_nome("venerdì 13 novembre, 9 gradi".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "venerdì 13 novembre, 9 gradi").next().is_none(), "vista casa: vuota");
        assert!(ElementHandle::find_by_accessible_label(&p, "Indietro").next().is_none());
        p.set_vista("panoramica".into());
        assert!(titolo(&p, "venerdì 13 novembre, 9 gradi"), "Panoramica: visibile");
        p.set_vista("casa".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "venerdì 13 novembre, 9 gradi").next().is_none(), "vista casa: vuota");
    }

    /// M2 Task 3: il contenuto scorre dal lato della scheda scelta (280 ms) e la riga alta e il
    /// dock restano fermi. Il ritorno alla Panoramica (dal dock, Backspace, Alt+←) va verso l'indice 0.
    #[test]
    fn scorrimento_dal_lato_della_scheda() {
        let (p, _) = pannello_casa();
        p.set_vista("panoramica".into());
        avanza(400);
        assert_eq!(p.get_scarto_contenuto(), 0.0);
        p.set_vista("pc".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), 26.0, "da destra, subito");
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0, "arrivato");
        p.set_vista("panoramica".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), -26.0, "verso la Panoramica: da sinistra");
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0);
        // Fra due app: casa (1) → pc (2) da destra; pc → casa da sinistra.
        p.set_vista("casa".into());
        fine_scorrimento();
        p.set_vista("pc".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), 26.0);
        fine_scorrimento();
        p.set_vista("casa".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), -26.0);
        fine_scorrimento();
        // Backspace dalla vista: verso la Panoramica.
        tasto(&p, slint::platform::Key::Backspace);
        avanza(0);
        assert_eq!(p.get_vista(), "panoramica");
        assert_eq!(p.get_scarto_contenuto(), -26.0);
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0);
    }

    /// M2 Task 3: con «Effetti di animazione» spento nessuno scarto, solo la dissolvenza.
    #[test]
    fn scorrimento_spento_col_movimento_ridotto() {
        let (p, _) = pannello_casa();
        p.global::<crate::Movimento>().set_ridotto(true);
        p.set_vista("pc".into());
        avanza(0);
        assert_eq!(p.get_scarto_contenuto(), 0.0);
        fine_scorrimento();
        assert_eq!(p.get_scarto_contenuto(), 0.0);
    }

    /// Task 14e/14g: da tastiera si va dalle card alle stanze (←, su «Preferiti»), si sceglie (↓) e
    /// si apre (Invio); il fuoco passa ai dispositivi, Invio tocca. Esc a livelli: prima torna a
    /// «Preferiti», poi chiude il pannello; Backspace torna prima a «Preferiti», poi alla Panoramica.
    #[test]
    fn stanza_da_tastiera_ed_esc_a_livelli() {
        use slint::platform::Key;
        let (p, eventi) = pannello_casa();
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 1);
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri soggiorno", "tocca light.b"]);
        tasto(&p, Key::Escape);
        assert_eq!(p.get_stanza_aperta(), -1, "il primo Esc torna a «Preferiti»");
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi stanza"));
        assert_eq!(scheda(&p, "Preferiti").accessible_item_selected(), Some(true));
        // Il fuoco è su «Preferiti»: ↓ e Invio riaprono la prima stanza.
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 0);
        tasto(&p, Key::Backspace);
        assert_eq!((p.get_stanza_aperta(), p.get_vista().as_str()), (-1, "casa"), "Backspace torna prima a «Preferiti»");
        tasto(&p, Key::Backspace);
        assert_eq!(p.get_vista(), "panoramica");
        // Il pannello ha ancora la tastiera (prima il fuoco finiva su una card che spariva).
        let e5 = eventi.clone();
        p.on_scorri_layout(move |_| e5.borrow_mut().push("scorri".into()));
        tasto(&p, Key::RightArrow);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("scorri"));
        tasto(&p, Key::Escape);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi pannello"), "{:?}", eventi.borrow());
    }

    /// Task 14g: Invio su «Preferiti» (già selezionata) porta il fuoco alle card; con una stanza
    /// aperta la chiude. ↑ dalla prima stanza torna a «Preferiti».
    #[test]
    fn preferiti_da_tastiera() {
        use slint::platform::Key;
        let (p, eventi) = pannello_casa();
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Return);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["tocca light.preferita"], "Invio su «Preferiti» porta alle card");
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 0);
        // Giro di correzioni 1: ← porta sulla stanza aperta, Invio lì torna alle card (come su
        // «Preferiti») senza chiuderla.
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), 0, "Invio sulla stanza aperta non la chiude");
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("tocca light.a"), "il fuoco è tornato alle card");
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(p.get_stanza_aperta(), -1, "Invio su «Preferiti» chiude la stanza");
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi stanza"));
    }

    /// Task 14e: Tab passa dalle stanze alle card e al dock; Maiusc+Tab torna indietro.
    #[test]
    fn tab_fra_stanze_card_e_dock() {
        use slint::platform::Key;
        let (p, eventi) = pannello_casa();
        // Contenuto (card) → Maiusc+Tab: stanze, su «Preferiti»; ↓ e Invio aprono la prima.
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri cucina"]);
        // Esc torna a «Preferiti» e il fuoco resta sulle stanze; Tab porta alle card.
        tasto(&p, Key::Escape);
        tasto(&p, Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("tocca light.preferita"));
    }

    fn con_modificatori_pannello(p: &Pannello, modificatori: &[slint::platform::Key], t: slint::platform::Key) {
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
        // combinazione diversa da Ctrl+Invio deve concedere (UX3/FE15).
        // Ctrl+Alt+Invio: su Windows Slint legge Ctrl+Alt come AltGr e, se non nasce un carattere
        // alfanumerico, toglie tutti e due; nel backend di prova arriva quindi come Invio semplice.
        // Il caso resta (un AltGr non deve concedere), ma NON esercita il `!evento.modifiers.alt`
        // della clausola Ctrl+Invio in pannello.slint: quel ramo è coperto solo dalla prova dal
        // vivo con winit (giro di correzioni 1: Ctrl+Alt+Invio non ha concesso). Alt+Invio prova
        // l'altro ramo, quello che prende e ignora Invio con un modificatore.
        use slint::platform::Key;
        let casi: [(&str, &[Key], Key); 9] = [
            ("Invio", &[], Key::Return),
            ("Spazio", &[], Key::Space),
            ("Alt+Invio", &[Key::Alt], Key::Return),
            ("Ctrl+Spazio", &[Key::Control], Key::Space),
            ("Maiusc+Invio", &[Key::Shift], Key::Return),
            ("Alt+Spazio", &[Key::Alt], Key::Space),
            ("Ctrl+Alt+Invio (qui arriva come Invio semplice)", &[Key::Control, Key::Alt], Key::Return),
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
    fn invio_da_solo_non_consente() {
        // La voce vera di un permesso nel pannello: il fuoco parte da «Nega» (Invio = nega), il
        // consenso solo con Ctrl+Invio.
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        let e = claude_hook::evento::EventoHook {
            sessione: "a".into(),
            tipo: claude_hook::evento::TipoEvento::PermissionRequest,
            cartella: "progetto".into(),
            strumento: Some("PowerShell".into()),
            notifica: None,
            errore: None,
            riassunto: Some("ls".into()),
        };
        let voce = crate::claude::voce_permesso(1, &e, std::time::Instant::now());
        p.set_attese(ModelRc::new(VecModel::from(vec![VoceAttesaUi {
            id: voce.id.as_str().into(),
            livello: LivelloUi::Bloccante,
            titolo: voce.titolo.as_str().into(),
            sotto: "Aspetta te · claude".into(),
            dettaglio: voce.dettaglio.as_str().into(),
            icona: Default::default(),
            pulsanti: crate::pillola::pulsanti_ui(&voce.pulsanti),
        }])));
        let azioni = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let a = azioni.clone();
        p.on_azione(move |_, id| a.borrow_mut().push(id.to_string()));
        p.invoke_espandi(0);
        tasto(&p, Key::Return);
        assert_eq!(*azioni.borrow(), ["nega"], "Invio da solo nega, non consente");
        azioni.borrow_mut().clear();
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        assert!(azioni.borrow().is_empty(), "Invio su «Consenti» non concede: {:?}", azioni.borrow());
        con_modificatori(&p, &[Key::Control], Key::Return);
        assert_eq!(*azioni.borrow(), ["consenti"], "Ctrl+Invio concede");
    }

    #[test]
    fn pillola_mostra_piu_due() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("Claude chiede: Comando ls".into());
        assert_eq!(p.get_larghezza_altri(), 0.0);
        assert!(elemento(&p, "Claude chiede: Comando ls").accessible_role().is_some());
        p.set_altri(" +2".into());
        assert!(p.get_larghezza_altri() > 0.0, "« +2» ha il suo spazio");
        assert!(elemento(&p, "Claude chiede: Comando ls +2").accessible_role().is_some(), "Narrator lo legge");
        // Il testo tagliato non lo copre: un titolo lunghissimo si taglia, « +2» resta.
        p.set_messaggio("x".repeat(400).into());
        assert!(p.get_testo_tagliato());
        assert!(p.get_larghezza_altri() > 0.0);
    }

    #[test]
    fn la_pillola_non_concede_da_tastiera() {
        // Giro di correzioni 1, rilievo 2: anche con la finestra della pillola a fuoco, Invio o
        // Spazio non premono nulla (il permesso nella pillola si dà col clic o, col testo intero,
        // dal pannello). Che Tab non porti ai pulsanti lo prova `la_pillola_non_da_il_fuoco_ai_pulsanti`.
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
        p.on_azione(move |_, id| a.borrow_mut().push(id.to_string()));
        for _ in 0..3 {
            for t in [Key::Tab, Key::Return, Key::Space] {
                p.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
                p.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
            }
        }
        assert!(azioni.borrow().is_empty(), "la tastiera ha premuto: {:?}", azioni.borrow());
    }

    #[test]
    fn la_pillola_non_da_il_fuoco_ai_pulsanti() {
        // Giro di correzioni 2, N1: Tab e Maiusc+Tab (ripetuti, nei due ordini dei pulsanti) non
        // portano il fuoco su un pulsante della pillola, quindi lì niente anello del fuoco. La
        // pillola non ha altro che possa prendere il fuoco: qualunque elemento col fuoco sarebbe
        // un suo pulsante.
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        for consenti_primo in [false, true] {
            let p = Pillola::new().expect("pillola");
            p.show().expect("pillola mostrata nel backend di prova");
            p.set_aspetto(AspettoPillolaUi::Allargata);
            let nega = PulsanteUi { id: "nega".into(), testo: "Nega".into(), delicato: false };
            let consenti = PulsanteUi { id: "consenti".into(), testo: "Consenti".into(), delicato: true };
            p.set_pulsanti(ModelRc::new(VecModel::from(if consenti_primo { vec![consenti, nega] } else { vec![nega, consenti] })));
            p.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
            for _ in 0..4 {
                for t in [Key::Tab, Key::Backtab] {
                    p.window().dispatch_event(WindowEvent::KeyPressed { text: t.into() });
                    p.window().dispatch_event(WindowEvent::KeyReleased { text: t.into() });
                    // Il fuoco (se c'è) sta sulla tastiera della pillola, mai su un pulsante né su
                    // qualcosa dentro un pulsante (da cui i tasti risalirebbero al pulsante).
                    let col_fuoco = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
                    let dove = col_fuoco
                        .as_ref()
                        .and_then(|f| f.element_type_names_and_ids(0))
                        .and_then(|v| v.first().map(|(_, id)| id.to_string()))
                        .unwrap_or_default();
                    assert!(
                        col_fuoco.is_none() || dove == "Pillola::tastiera",
                        "consenti_primo={consenti_primo}: dopo {t:?} il fuoco è su «{dove}»"
                    );
                }
            }
        }
    }

    // ─── Task 14f: collegamenti ─────────────────────────────────────────────

    fn collegamento(id: &str, nome: &str, url: &str) -> crate::CollegamentoUi {
        crate::CollegamentoUi {
            id: id.into(),
            nome: nome.into(),
            url: url.into(),
            corto: crate::collegamenti::indirizzo_corto(url).into(),
        }
    }

    fn quattro_collegamenti() -> ModelRc<crate::CollegamentoUi> {
        ModelRc::new(VecModel::from(vec![
            collegamento("proxmox", "Proxmox", "https://esempio.lan:8006"),
            collegamento("nas", "NAS", "http://nas.esempio.lan/admin?x=1"),
            collegamento("router", "Router", "https://router.esempio.lan"),
            collegamento("stampante", "Stampante", "http://192.0.2.20"),
        ]))
    }

    /// Un pannello sulla vista Collegamenti; restituisce gli eventi (aperture, «gestisci»,
    /// chiusura del pannello).
    fn pannello_collegamenti(voci: ModelRc<crate::CollegamentoUi>) -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(app());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_collegamenti(voci);
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3) = (eventi.clone(), eventi.clone(), eventi.clone());
        p.on_apri_collegamento(move |id| e1.borrow_mut().push(format!("apri {id}")));
        p.on_gestisci_collegamenti(move || e2.borrow_mut().push("gestisci".into()));
        p.on_chiudi(move || e3.borrow_mut().push("chiudi pannello".into()));
        (p, eventi)
    }

    /// Il dock ha «Collegamenti»; la vista mostra le card (nome e indirizzo accorciato per
    /// Narrator, pulsanti) nell'ordine dato; un tocco chiede di aprire quell'id.
    #[test]
    fn vista_collegamenti_card_accessibili() {
        let (p, eventi) = pannello_collegamenti(quattro_collegamenti());
        let scheda = elemento(&p, "Collegamenti");
        assert_eq!((scheda.accessible_role(), scheda.accessible_item_index()), (Some(AccessibleRole::Tab), Some(4)));
        scheda.invoke_accessible_default_action();
        assert_eq!(p.get_vista(), "collegamenti");
        let proxmox = elemento(&p, "Proxmox, esempio.lan:8006");
        assert_eq!(proxmox.accessible_role(), Some(AccessibleRole::Button));
        assert_eq!(proxmox.accessible_description().as_deref(), Some("Si apre nel browser"));
        assert!(ElementHandle::find_by_accessible_label(&p, "NAS, nas.esempio.lan/admin").next().is_some(), "senza query");
        // Ordine: Proxmox, NAS e Router nella prima riga (3 colonne), Stampante sotto.
        let x = |nome: &str| elemento(&p, nome).absolute_position();
        let (a, b, c, d) = (
            x("Proxmox, esempio.lan:8006"),
            x("NAS, nas.esempio.lan/admin"),
            x("Router, router.esempio.lan"),
            x("Stampante, 192.0.2.20"),
        );
        assert!(a.x < b.x && b.x < c.x && a.y == c.y && d.y > a.y && d.x == a.x, "{a:?} {b:?} {c:?} {d:?}");
        elemento(&p, "NAS, nas.esempio.lan/admin").invoke_accessible_default_action();
        elemento(&p, "Modifica i collegamenti").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["apri nas", "gestisci"]);
    }

    /// Da tastiera: frecce fra le card (3 colonne), Invio apre; ↑ dalla prima riga porta a
    /// «Modifica», Invio lì apre le Impostazioni; Esc chiude il pannello (un solo livello).
    #[test]
    fn vista_collegamenti_da_tastiera() {
        use slint::platform::Key;
        let (p, eventi) = pannello_collegamenti(quattro_collegamenti());
        // Dal dock: Tab, → fino a «Collegamenti», Invio.
        tasto(&p, Key::Tab);
        for _ in 0..4 {
            tasto(&p, Key::RightArrow);
        }
        tasto(&p, Key::Return);
        assert_eq!(p.get_vista(), "collegamenti");
        // Il fuoco va sul contenuto (dal dock Tab passa da «⋯» e poi al contenuto: «Indietro» non
        // c'è più): prima card; ↓ porta alla quarta (sotto la prima), Invio.
        tasto(&p, Key::Tab);
        tasto(&p, Key::Tab);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        // → dall'ultima resta lì; ↑ torna alla prima riga, → alla seconda card.
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::RightArrow);
        tasto(&p, Key::Return);
        // ↑ dalla prima riga: «Modifica».
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri stampante", "apri nas", "gestisci"]);
        let col_fuoco = i_slint_core::window::WindowInner::from_pub(p.window()).focus_item.borrow().upgrade();
        assert!(col_fuoco.is_some(), "il pannello ha ancora il fuoco");
        tasto(&p, Key::Escape);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi pannello"));
    }

    /// Senza collegamenti: lo stato vuoto con «Aggiungi un collegamento», che apre le
    /// Impostazioni (col mouse, con Narrator e con Invio).
    #[test]
    fn vista_collegamenti_vuota() {
        use slint::platform::Key;
        let (p, eventi) = pannello_collegamenti(ModelRc::new(VecModel::from(Vec::<crate::CollegamentoUi>::new())));
        p.set_vista("collegamenti".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "Nessun collegamento").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Modifica i collegamenti").next().is_none(), "niente da modificare");
        let aggiungi = elemento(&p, "Aggiungi un collegamento");
        assert_eq!(aggiungi.accessible_role(), Some(AccessibleRole::Button));
        aggiungi.invoke_accessible_default_action();
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["gestisci", "gestisci"]);
    }

    /// Un errore di apertura compare nella testata della vista.
    #[test]
    fn vista_collegamenti_dice_se_non_si_e_aperto() {
        let (p, _) = pannello_collegamenti(quattro_collegamenti());
        p.set_vista("collegamenti".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "Si aprono nel browser").next().is_some());
        p.set_errore_collegamenti("«NAS» non si è aperto".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "«NAS» non si è aperto").next().is_some());
    }

    // ─── Task 9: vista Claude ───────────────────────────────────────────────

    fn riga_claude(id: &str, cartella: &str, stato: &str, livello: crate::LivelloUi) -> crate::RigaSessioneUi {
        crate::RigaSessioneUi {
            id: id.into(),
            cartella: cartella.into(),
            stato: stato.into(),
            livello,
            strumento: Default::default(),
            durata: Default::default(),
            contesto: Default::default(),
            terminale: Default::default(),
            avviso: Default::default(),
        }
    }

    fn tre_righe_claude() -> ModelRc<crate::RigaSessioneUi> {
        ModelRc::new(VecModel::from(vec![
            riga_claude("aaaa", "uno", "Ti aspetta", crate::LivelloUi::Attenzione),
            riga_claude("bbbb", "due", "Fatto", crate::LivelloUi::Informazione),
            riga_claude("cccc", "tre", "Lavora", crate::LivelloUi::Nessuno),
        ]))
    }

    fn limite_claude(testo: &str, frazione: f32, azzera: &str) -> crate::LimiteClaudeUi {
        crate::LimiteClaudeUi { presente: true, frazione, testo: testo.into(), azzera: azzera.into() }
    }

    /// Un pannello sulla vista Claude; restituisce gli eventi («apri <id>», «gestisci»).
    fn pannello_claude(
        collegato: bool,
        righe: ModelRc<crate::RigaSessioneUi>,
    ) -> (Pannello, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(app());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_claude_collegato(collegato);
        p.set_claude_righe(righe);
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2) = (eventi.clone(), eventi.clone());
        p.on_apri_sessione(move |id| e1.borrow_mut().push(format!("apri {id}")));
        p.on_gestisci_claude(move || e2.borrow_mut().push("gestisci".into()));
        p.set_vista("claude".into());
        (p, eventi)
    }

    /// Scollegato: la frase e «Impostazioni» (col mouse, con Narrator e con Invio); collegato ma
    /// senza sessioni: «Nessuna sessione aperta», senza pulsanti.
    #[test]
    fn vista_claude_stati_vuoti() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(false, ModelRc::new(VecModel::from(Vec::<crate::RigaSessioneUi>::new())));
        assert!(ElementHandle::find_by_accessible_label(&p, "Collega Claude Code dalle Impostazioni").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Nessuna sessione aperta").next().is_none());
        let impostazioni = elemento(&p, "Impostazioni");
        assert_eq!(impostazioni.accessible_role(), Some(AccessibleRole::Button));
        impostazioni.invoke_accessible_default_action();
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["gestisci", "gestisci"]);

        p.set_claude_collegato(true);
        assert!(ElementHandle::find_by_accessible_label(&p, "Nessuna sessione aperta").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "Collega Claude Code dalle Impostazioni").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&p, "Impostazioni").next().is_none(), "niente pulsante");
        // Invio senza righe non fa nulla.
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().len(), 2);
    }

    /// PF9: un limite assente non ha l'anello (mai «0%»); senza nessuno dei due la card dice
    /// «Limiti non disponibili»; con i dati gli anelli hanno nome, valore e quando si azzerano.
    #[test]
    fn anelli_nascosti_senza_abbonamento() {
        let (p, _) = pannello_claude(true, tre_righe_claude());
        let ha = |nome: &str| ElementHandle::find_by_accessible_label(&p, nome).next().is_some();
        assert!(!ha("Limite 5 ore") && !ha("Limite settimanale"), "senza limiti nessun anello");
        assert!(ha("Limiti non disponibili"));

        p.set_claude_cinque_ore(limite_claude("37%", 0.37, "si azzera alle 14:30"));
        assert!(ha("Limite 5 ore") && !ha("Limite settimanale"), "solo la finestra che c'è");
        assert!(!ha("Limiti non disponibili"));
        let anello = elemento(&p, "Limite 5 ore");
        assert_eq!(anello.accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert_eq!(anello.accessible_value().as_deref(), Some("37%"));
        assert_eq!(anello.accessible_description().as_deref(), Some("si azzera alle 14:30"));

        p.set_claude_sette_giorni(limite_claude("61%", 0.61, "si azzera sabato alle 09:00"));
        assert_eq!(elemento(&p, "Limite settimanale").accessible_value().as_deref(), Some("61%"));
        // Un limite scaduto torna assente: l'anello sparisce.
        p.set_claude_cinque_ore(Default::default());
        assert!(!ha("Limite 5 ore") && ha("Limite settimanale"));
    }

    /// La spesa del mese: il valore e la nota; senza valore solo la nota (nessuno «0»).
    #[test]
    fn vista_claude_spesa_del_mese() {
        let (p, _) = pannello_claude(true, tre_righe_claude());
        let nota = "Aggiungi una chiave da amministratore nelle Impostazioni";
        p.set_claude_spesa("".into());
        p.set_claude_spesa_nota(nota.into());
        assert_eq!(elemento(&p, "Spesa API del mese").accessible_value().as_deref(), Some(nota));
        p.set_claude_spesa("412,80 $".into());
        p.set_claude_spesa_nota("aggiornato alle 16:02".into());
        let spesa = elemento(&p, "Spesa API del mese");
        assert_eq!(spesa.accessible_value().as_deref(), Some("412,80 $"));
        assert_eq!(spesa.accessible_description().as_deref(), Some("aggiornato alle 16:02"));
    }

    /// Concordanza: «+1 altra» e «e un'altra sessione»; da due in su «+N altre».
    #[test]
    fn vista_claude_altre_al_singolare() {
        let (p, _) = pannello_claude(true, tre_righe_claude());
        p.set_claude_altre(1);
        assert!(ElementHandle::find_by_accessible_label(&p, "e un'altra sessione").next().is_some());
        p.set_claude_altre(2);
        assert!(ElementHandle::find_by_accessible_label(&p, "e altre 2 sessioni").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&p, "e un'altra sessione").next().is_none());
    }

    /// Il dettaglio di «Limiti non disponibili» dice il vero: scollegato si collega, collegato si aspetta.
    #[test]
    fn limiti_non_disponibili_spiega_il_motivo() {
        let (p, _) = pannello_claude(false, ModelRc::new(VecModel::from(Vec::<crate::RigaSessioneUi>::new())));
        let ha = |t: &str| ElementHandle::find_by_accessible_label(&p, t).next().is_some();
        assert!(ha("Si vedono dopo aver collegato Claude Code."));
        p.set_claude_collegato(true);
        assert!(ha("Compaiono dopo la prima risposta di una sessione, con un abbonamento Claude."));
        assert!(!ha("Si vedono dopo aver collegato Claude Code."));
    }

    /// Il fuoco segue la sessione, non la posizione: se le righe si riordinano Invio apre la stessa.
    #[test]
    fn il_fuoco_delle_righe_segue_la_sessione() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(true, tre_righe_claude());
        tasto(&p, Key::DownArrow); // «bbbb»
        // Riordino: «bbbb» passa in cima.
        p.set_claude_righe(ModelRc::new(VecModel::from(vec![
            riga_claude("bbbb", "due", "Ti aspetta", crate::LivelloUi::Attenzione),
            riga_claude("aaaa", "uno", "Fatto", crate::LivelloUi::Informazione),
            riga_claude("cccc", "tre", "Lavora", crate::LivelloUi::Nessuno),
        ])));
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri bbbb"]);
        // Se la sessione sparisce, il fuoco torna alla prima riga.
        p.set_claude_righe(ModelRc::new(VecModel::from(vec![riga_claude("cccc", "tre", "Lavora", crate::LivelloUi::Nessuno)])));
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"));
    }

    /// ↓ due volte e Invio: `apri-sessione` con l'id della terza riga; ↑ risale, e i bordi tengono.
    #[test]
    fn righe_da_tastiera() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(true, tre_righe_claude());
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["apri cccc"]);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"), "in fondo resta lì");
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri bbbb"));
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri aaaa"));
        // Narrator e il mouse: la riga è un pulsante col nome e lo stato.
        let riga = elemento(&p, "tre, Lavora");
        assert_eq!(riga.accessible_role(), Some(AccessibleRole::Button));
        riga.invoke_accessible_default_action();
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"));
        // Esc chiude il pannello come altrove.
        let chiuso = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiuso.clone();
        p.on_chiudi(move || c.set(true));
        tasto(&p, Key::Escape);
        assert!(chiuso.get());
    }

    /// Con la chiave da amministratore «Dettagli» sta sotto l'ultima riga: ↓ dall'ultima ci arriva,
    /// Invio apre le schede, ↑ torna alla riga; senza la chiave il pulsante non c'è.
    #[test]
    fn dettagli_da_tastiera_e_da_narrator() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(true, tre_righe_claude());
        assert!(ElementHandle::find_by_accessible_label(&p, "Dettagli delle API").next().is_none(), "senza la chiave non c'è");
        let e = eventi.clone();
        p.on_apri_dettagli_claude(move || e.borrow_mut().push("dettagli".into()));
        p.set_claude_dettagli(true);
        let pulsante = elemento(&p, "Dettagli delle API");
        assert_eq!(pulsante.accessible_role(), Some(AccessibleRole::Button));
        // ↓ fino all'ultima riga e poi su «Dettagli»; Invio apre le schede.
        for _ in 0..3 {
            tasto(&p, Key::DownArrow);
        }
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["dettagli"]);
        // ↑ torna alla riga, Invio apre la sessione; ancora ↑ sale di una riga.
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("apri cccc"));
        // Il clic o Narrator sul pulsante fanno la stessa cosa.
        pulsante.invoke_accessible_default_action();
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("dettagli"));
    }

    /// Scollegato ma con la chiave: ↓ da «Impostazioni» porta su «Dettagli», ↑ torna.
    #[test]
    fn dettagli_anche_da_scollegato() {
        use slint::platform::Key;
        let (p, eventi) = pannello_claude(false, ModelRc::new(VecModel::from(Vec::<crate::RigaSessioneUi>::new())));
        let e = eventi.clone();
        p.on_apri_dettagli_claude(move || e.borrow_mut().push("dettagli".into()));
        p.set_claude_dettagli(true);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["gestisci"]);
        tasto(&p, Key::DownArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("dettagli"));
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("gestisci"));
    }

    /// Il caso più pieno (tre righe con tutto, «+N altre», due anelli, spesa con la nota più lunga):
    /// nulla esce dalla vista e la vista sta nel pannello.
    #[test]
    fn vista_claude_entra_nel_pannello() {
        let mut righe = Vec::new();
        for (i, stato) in ["Ti aspetta", "Fatto", "Lavora"].iter().enumerate() {
            righe.push(crate::RigaSessioneUi {
                strumento: "Modifica".into(),
                durata: "1:02:05".into(),
                contesto: "100%".into(),
                ..riga_claude(&format!("id{i}"), "una-cartella-dal-nome-davvero-molto-lungo", stato, crate::LivelloUi::Attenzione)
            });
        }
        let (p, _) = pannello_claude(true, ModelRc::new(VecModel::from(righe)));
        p.set_claude_altre(12);
        p.set_claude_cinque_ore(limite_claude("100%", 1.0, "si azzera domani alle 14:30"));
        p.set_claude_sette_giorni(limite_claude("100%", 1.0, "si azzera mercoledì alle 14:30"));
        p.set_claude_spesa_nota("Aggiungi una chiave da amministratore nelle Impostazioni".into());
        let vista = ElementHandle::find_by_element_type_name(&p, "VistaClaude").next().expect("vista Claude");
        let (cp, cs) = (vista.absolute_position(), vista.size());
        for e in vista.query_descendants().find_all() {
            let (pos, lato) = (e.absolute_position(), e.size());
            if lato.width <= 0.0 || lato.height <= 0.0 {
                continue;
            }
            assert!(
                pos.x >= cp.x - 0.5
                    && pos.y >= cp.y - 0.5
                    && pos.x + lato.width <= cp.x + cs.width + 0.5
                    && pos.y + lato.height <= cp.y + cs.height + 0.5,
                "{:?} «{}» esce dalla vista: {pos:?} {lato:?} fuori da {cp:?} {cs:?}",
                e.type_name(),
                e.accessible_label().unwrap_or_default()
            );
        }
        assert!(cp.x >= 0.0 && cp.y >= 0.0 && cp.x + cs.width <= 800.0 && cp.y + cs.height <= 300.0);
        assert!(ElementHandle::find_by_accessible_label(&p, "e altre 12 sessioni").next().is_some(), "«+12 altre»");
    }

    /// Impostazioni › Claude (M3, Task 7 e 7B): l'anteprima si apre col fuoco su «Annulla» (Invio per
    /// sbaglio non scrive), «Scrivi in settings.json» è raggiungibile con Tab, Esc annulla; lo
    /// scollegamento chiede conferma; i pulsanti hanno il loro nome per Narrator.
    #[test]
    fn sezione_claude_anteprima_conferma_e_annulla() {
        use slint::platform::{Key, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&'static str>::new()));
        let debole = f.as_weak();
        let e = eventi.clone();
        f.on_claude_prepara(move || {
            e.borrow_mut().push("prepara");
            let f = debole.upgrade().unwrap();
            f.set_claude_anteprima("{\n  \"hooks\": {}\n}".into());
            f.set_claude_avvisi("La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili.".into());
            f.set_claude_file("settings-finto.json".into());
            f.set_claude_passo(1);
        });
        let e = eventi.clone();
        f.on_claude_conferma(move || e.borrow_mut().push("conferma"));
        let e = eventi.clone();
        f.on_claude_annulla(move || e.borrow_mut().push("annulla"));
        let e = eventi.clone();
        f.on_claude_scollega(move || e.borrow_mut().push("scollega"));
        let tasto = |k: Key| {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: k.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: k.into() });
        };
        let ce = |nome: &str| ElementHandle::find_by_accessible_label(&f, nome).next().is_some();

        // A riposo, non collegato: solo «Collega Claude Code», e la nota sui permessi nel terminale
        // (nessun «Rigenera il codice», nessuna nota sul terminale da riaprire: il codice non c'è più).
        assert!(ce("Collega Claude Code"));
        assert!(!ce("Scrivi in settings.json") && !ce("Rigenera il codice") && !ce("Scollega"));
        assert!(ce("Con isola chiusa Claude Code prosegue da solo: i permessi li decidi nel terminale."));
        assert!(!ce("Codice segreto") && !ce("Copia il codice"));
        assert!(ce("Non collegato"));

        // Anteprima: avviso, riquadro leggibile, i due pulsanti.
        elemento(&f, "Collega Claude Code").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["prepara"]);
        assert!(ce("Scrivi in settings.json") && ce("Annulla"));
        assert!(ce("Voci di isola come resteranno in settings.json"));
        assert!(ce("La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili."));
        assert!(ce("Il resto di settings.json resta com'è (la prima volta ne tengo una copia accanto). Le sessioni di Claude Code avviate dopo il collegamento lo useranno."));
        assert!(!ce("I terminali aperti prima vanno chiusi e riaperti."));
        assert!(!ce("Collega Claude Code"), "durante l'anteprima il pulsante non c'è");

        // Il fuoco parte da «Annulla»: Invio annulla (non scrive).
        tasto(Key::Return);
        assert_eq!(*eventi.borrow(), ["prepara", "annulla"]);
        assert!(!ce("Scrivi in settings.json"));
        assert_eq!(f.get_claude_passo(), 0);

        // Di nuovo: Tab porta a «Scrivi in settings.json» e Invio conferma.
        elemento(&f, "Collega Claude Code").invoke_accessible_default_action();
        tasto(Key::Tab);
        tasto(Key::Return);
        assert_eq!(eventi.borrow().last(), Some(&"conferma"), "{:?}", eventi.borrow());
        // Esc dall'anteprima annulla.
        tasto(Key::Escape);
        assert_eq!(eventi.borrow().last(), Some(&"annulla"));
        assert_eq!(f.get_claude_passo(), 0);

        // Collegato: solo Scollega (niente «Rigenera il codice»); chiede conferma, col fuoco su «Annulla».
        eventi.borrow_mut().clear();
        f.set_claude_collegato(true);
        f.set_claude_stato("Collegato".into());
        assert!(ce("Collegato") && !ce("Rigenera il codice"));
        elemento(&f, "Scollega").invoke_accessible_default_action();
        assert_eq!(f.get_claude_passo(), 2);
        assert!(ce("Togliere il collegamento con Claude Code?"));
        assert!(
            ce("Da settings.json escono solo le voci di isola (hook e barra di stato); il resto resta com'è."),
            "la conferma dice ciò che succede davvero"
        );
        assert!(!ce("Da settings.json escono solo le voci di isola, il resto resta com'è; il codice segreto viene cancellato."));
        tasto(Key::Return);
        assert_eq!(eventi.borrow().last(), Some(&"annulla"));
        assert_eq!(f.get_claude_passo(), 0);
        elemento(&f, "Scollega").invoke_accessible_default_action();
        tasto(Key::Tab);
        tasto(Key::Return);
        assert_eq!(eventi.borrow().last(), Some(&"scollega"), "{:?}", eventi.borrow());

        // Fuori da Windows: «Disponibile solo su Windows», nessun pulsante per collegare.
        f.set_claude_passo(0);
        f.set_claude_collegato(false);
        assert!(ce("Collega Claude Code"));
        f.set_claude_disponibile(false);
        f.set_claude_stato("Disponibile solo su Windows".into());
        assert!(ce("Disponibile solo su Windows") && !ce("Collega Claude Code"));
    }

    /// Le Impostazioni sulla scheda Collegamenti, coi controlli veri dei campi; restituisce gli
    /// eventi (salvataggi, eliminazioni, spostamenti, chiusura della finestra).
    fn scheda_collegamenti() -> (FinestraImpostazioni, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_COLLEGAMENTI);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        f.set_collegamenti(quattro_collegamenti());
        f.on_controlla_nome_collegamento(|n| crate::collegamenti::valida_nome(&n).err().unwrap_or_default().into());
        f.on_controlla_indirizzo_collegamento(|u| crate::collegamenti::valida_indirizzo(&u).err().unwrap_or_default().into());
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3, e4) = (eventi.clone(), eventi.clone(), eventi.clone(), eventi.clone());
        f.on_salva_collegamento(move |id, nome, url| {
            e1.borrow_mut().push(format!("salva [{id}] [{nome}] [{url}]"));
            "".into()
        });
        f.on_elimina_collegamento(move |id| e2.borrow_mut().push(format!("elimina {id}")));
        f.on_sposta_collegamento(move |id, verso| e3.borrow_mut().push(format!("sposta {id} {verso}")));
        f.on_chiudi(move || e4.borrow_mut().push("chiudi finestra".into()));
        (f, eventi)
    }

    fn tasto_finestra(f: &FinestraImpostazioni, testo: impl Into<slint::SharedString> + Clone) {
        use slint::platform::WindowEvent;
        f.window().dispatch_event(WindowEvent::KeyPressed { text: testo.clone().into() });
        f.window().dispatch_event(WindowEvent::KeyReleased { text: testo.into() });
    }

    fn scrivi(f: &FinestraImpostazioni, testo: &str) {
        for c in testo.chars() {
            tasto_finestra(f, c.to_string());
        }
    }

    fn campo(f: &FinestraImpostazioni, etichetta: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(f, etichetta)
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .unwrap_or_else(|| panic!("nessun campo «{etichetta}»"))
    }

    /// La scheda: sezione «Collegamenti» fra Dispositivi e Sistema; righe coi pulsanti nominati;
    /// le frecce ai bordi spente; le azioni arrivano a Rust con l'id giusto.
    #[test]
    fn scheda_collegamenti_righe_e_ordine() {
        let (f, eventi) = scheda_collegamenti();
        assert_eq!(elemento(&f, "Collegamenti").accessible_item_selected(), Some(true));
        assert_eq!(elemento(&f, "Sposta su «Proxmox»").accessible_enabled(), Some(false), "già in cima");
        assert_eq!(elemento(&f, "Sposta giù «Stampante»").accessible_enabled(), Some(false), "già in fondo");
        assert!(ElementHandle::find_by_accessible_label(&f, "http://nas.esempio.lan/admin?x=1").next().is_some(), "indirizzo intero");
        elemento(&f, "Sposta giù «Proxmox»").invoke_accessible_default_action();
        elemento(&f, "Sposta su «Router»").invoke_accessible_default_action();
        assert_eq!(*eventi.borrow(), ["sposta proxmox 1", "sposta router -1"]);
        // Senza collegamenti: lo stato vuoto, e «Aggiungi un collegamento» resta.
        f.set_collegamenti(ModelRc::new(VecModel::from(Vec::<crate::CollegamentoUi>::new())));
        assert!(ElementHandle::find_by_accessible_label(&f, "Nessun collegamento").next().is_some());
        assert!(ElementHandle::find_by_accessible_label(&f, "Aggiungi un collegamento").next().is_some());
        // 18/11: a elenco vuoto il pulsante sta sotto il messaggio e al centro (come nel pannello).
        let messaggio = elemento(&f, "Nessun collegamento");
        let pulsante = elemento(&f, "Aggiungi un collegamento");
        let centro = |e: &ElementHandle| e.absolute_position().x + e.size().width / 2.0;
        assert!((centro(&messaggio) - centro(&pulsante)).abs() < 2.0, "centrato: {} contro {}", centro(&messaggio), centro(&pulsante));
        assert!(pulsante.absolute_position().y > messaggio.absolute_position().y + messaggio.size().height, "sotto il messaggio");
    }

    /// Il modulo: etichette visibili; controllo al blur solo su un campo toccato; l'errore sparisce
    /// appena corretto; Invio salva; Esc a livelli (campo, modulo, finestra).
    #[test]
    fn scheda_collegamenti_modulo_con_controlli() {
        use slint::platform::Key;
        let (f, eventi) = scheda_collegamenti();
        elemento(&f, "Aggiungi un collegamento").invoke_accessible_default_action();
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVO COLLEGAMENTO").next().is_some());
        // Il fuoco è nel Nome: Tab senza scrivere non segnala nulla (campo solo attraversato).
        tasto_finestra(&f, Key::Tab);
        assert_eq!(campo(&f, "Nome").accessible_description().as_deref(), Some(""));
        // Indirizzo sbagliato: al blur l'errore compare.
        scrivi(&f, "file:///x");
        tasto_finestra(&f, Key::Tab);
        let errore = campo(&f, "Indirizzo").accessible_description().unwrap_or_default();
        assert_eq!(errore, "Solo indirizzi http:// o https://");
        assert!(ElementHandle::find_by_accessible_label(&f, "Solo indirizzi http:// o https://").next().is_some(), "testo visibile");
        // Si torna nel campo, si seleziona tutto e si riscrive: l'errore sparisce al primo
        // carattere che rende l'indirizzo valido, prima di lasciare il campo.
        con_modificatori_finestra(&f, &[Key::Shift], Key::Tab);
        con_modificatori_finestra(&f, &[Key::Control], "a");
        scrivi(&f, "e");
        assert_eq!(campo(&f, "Indirizzo").accessible_description().as_deref(), Some(""));
        scrivi(&f, "sempio.lan:8006");
        // Invio col nome vuoto: il nome si segnala e prende il fuoco; nulla si salva.
        tasto_finestra(&f, Key::Return);
        assert_eq!(campo(&f, "Nome").accessible_description().as_deref(), Some("Scrivi un nome"));
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
        scrivi(&f, "Proxmox");
        assert_eq!(campo(&f, "Nome").accessible_description().as_deref(), Some(""), "corretto: errore via");
        tasto_finestra(&f, Key::Return);
        assert_eq!(*eventi.borrow(), ["salva [] [Proxmox] [esempio.lan:8006]"]);
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVO COLLEGAMENTO").next().is_none(), "salvato: modulo chiuso");
        // Modifica: i campi partono dai valori salvati; Esc esce dal campo, poi chiude il modulo
        // (senza salvare), poi chiude la finestra.
        elemento(&f, "Modifica «NAS»").invoke_accessible_default_action();
        assert_eq!(campo(&f, "Nome").accessible_value().as_deref(), Some("NAS"));
        assert_eq!(campo(&f, "Indirizzo").accessible_value().as_deref(), Some("http://nas.esempio.lan/admin?x=1"));
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "MODIFICA «NAS»").next().is_some(), "il primo Esc esce solo dal campo");
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "MODIFICA «NAS»").next().is_none(), "il secondo chiude il modulo");
        assert_eq!(eventi.borrow().len(), 1, "niente salvato né chiuso: {:?}", eventi.borrow());
        tasto_finestra(&f, Key::Escape);
        assert_eq!(eventi.borrow().last().map(String::as_str), Some("chiudi finestra"));
    }

    fn con_modificatori_finestra(f: &FinestraImpostazioni, modificatori: &[slint::platform::Key], t: impl Into<slint::SharedString> + Clone) {
        use slint::platform::WindowEvent;
        for m in modificatori {
            f.window().dispatch_event(WindowEvent::KeyPressed { text: (*m).into() });
        }
        tasto_finestra(f, t);
        for m in modificatori.iter().rev() {
            f.window().dispatch_event(WindowEvent::KeyReleased { text: (*m).into() });
        }
    }

    /// Il testo del controllo col fuoco (id dell'elemento, «Componente::id»), "" se nessuno.
    fn col_fuoco(f: &FinestraImpostazioni) -> String {
        let fuoco = i_slint_core::window::WindowInner::from_pub(f.window()).focus_item.borrow().upgrade();
        fuoco
            .as_ref()
            .and_then(|i| i.element_type_names_and_ids(0))
            .and_then(|v| v.first().map(|(_, id)| id.to_string()))
            .unwrap_or_default()
    }

    /// L'eliminazione chiede conferma dentro la finestra nominando il collegamento; il fuoco parte
    /// da «Annulla» (Invio lì non elimina); Esc annulla; «Elimina» elimina quell'id.
    #[test]
    fn scheda_collegamenti_conferma_di_eliminazione() {
        use slint::platform::Key;
        let (f, eventi) = scheda_collegamenti();
        let conferma = |f: &FinestraImpostazioni| ElementHandle::find_by_accessible_label(f, "Eliminare “NAS”?").next();
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        assert_eq!(conferma(&f).and_then(|c| c.accessible_role()), Some(AccessibleRole::Groupbox));
        assert!(col_fuoco(&f).ends_with("::annulla"), "fuoco su «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert!(conferma(&f).is_none(), "Invio su «Annulla» chiude la conferma");
        // Tab resta nella conferma; Esc annulla senza chiudere la finestra.
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        tasto_finestra(&f, Key::Tab);
        assert!(col_fuoco(&f).ends_with("::elimina"), "fuoco su «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Tab);
        assert!(col_fuoco(&f).ends_with("::annulla"), "fuoco su «{}»", col_fuoco(&f));
        tasto_finestra(&f, Key::Escape);
        assert!(conferma(&f).is_none());
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
        // Un clic dentro il riquadro (sul testo) non la chiude; «Elimina» elimina.
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        let riquadro = conferma(&f).unwrap();
        let (pos, _) = (riquadro.absolute_position(), riquadro.size());
        let punto = slint::LogicalPosition::new(pos.x + 30.0, pos.y + 60.0);
        use slint::platform::{PointerEventButton, WindowEvent};
        let button = PointerEventButton::Left;
        // Due volte: il primo clic su elementi appena nati da un `if` il backend di prova non lo
        // consegna sempre (visto anche nella scelta dell'icona); col secondo il controllo vale.
        for _ in 0..2 {
            f.window().dispatch_event(WindowEvent::PointerMoved { position: punto });
            f.window().dispatch_event(WindowEvent::PointerPressed { position: punto, button });
            f.window().dispatch_event(WindowEvent::PointerReleased { position: punto, button });
        }
        assert!(conferma(&f).is_some(), "un clic nel riquadro l'ha chiusa");
        elemento(&f, "Elimina “NAS”").invoke_accessible_default_action();
        assert!(conferma(&f).is_none());
        assert_eq!(*eventi.borrow(), ["elimina nas"]);
    }

    /// Giro di rifinitura del 14f: Esc a livelli anche nel modulo degli ambienti di Home
    /// Assistant (prima chiudeva la finestra, perdendo quanto scritto, token compreso).
    #[test]
    fn modulo_ambienti_esc_a_livelli() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let chiusa = std::rc::Rc::new(std::cell::Cell::new(false));
        let c = chiusa.clone();
        f.on_chiudi(move || c.set(true));
        elemento(&f, "Aggiungi un ambiente").invoke_accessible_default_action();
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVO AMBIENTE").next().is_some());
        scrivi(&f, "Casa");
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVO AMBIENTE").next().is_some(), "il primo Esc esce dal campo");
        tasto_finestra(&f, Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&f, "NUOVO AMBIENTE").next().is_none(), "il secondo chiude il modulo");
        assert!(!chiusa.get(), "la finestra resta aperta");
        tasto_finestra(&f, Key::Escape);
        assert!(chiusa.get(), "il terzo chiude la finestra");
    }

    /// Giro di rifinitura: un clic fuori dalla conferma vale «Annulla»; con la conferma aperta i
    /// pulsanti delle righe sotto sono spenti (Narrator e UIA non li invocano).
    #[test]
    fn conferma_clic_fuori_e_righe_spente() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (f, eventi) = scheda_collegamenti();
        elemento(&f, "Elimina «NAS»").invoke_accessible_default_action();
        assert_eq!(elemento(&f, "Elimina «Router»").accessible_enabled(), Some(false));
        elemento(&f, "Elimina «Router»").invoke_accessible_default_action();
        assert!(ElementHandle::find_by_accessible_label(&f, "Eliminare “NAS”?").next().is_some(), "la conferma non cambia");
        let riquadro = elemento(&f, "Eliminare “NAS”?");
        let (pos, lato) = (riquadro.absolute_position(), riquadro.size());
        let punto = slint::LogicalPosition::new(pos.x + 20.0, pos.y + lato.height + 40.0);
        let button = PointerEventButton::Left;
        for _ in 0..2 {
            f.window().dispatch_event(WindowEvent::PointerMoved { position: punto });
            f.window().dispatch_event(WindowEvent::PointerPressed { position: punto, button });
            f.window().dispatch_event(WindowEvent::PointerReleased { position: punto, button });
        }
        assert!(ElementHandle::find_by_accessible_label(&f, "Eliminare “NAS”?").next().is_none(), "il clic fuori annulla");
        assert_eq!(elemento(&f, "Elimina «Router»").accessible_enabled(), Some(true));
        assert!(eventi.borrow().is_empty(), "{:?}", eventi.borrow());
    }

    // ─── M2, Task 4: lati della pillola, sezione Isola, selettore, quadranti ───

    fn testo_lato(t: &str) -> crate::ContenutoLatoUi {
        crate::ContenutoLatoUi { tipo: 1, testo: t.into(), ..Default::default() }
    }

    /// Le misure della pillola coi lati sono quelle di `lati::larghezza_riposo` e
    /// `lati::scarto_occhi_riposo`: un lato, due lati, la batteria col glifo; da allargata i lati
    /// non contano. Il nome per Narrator dice i valori, nell'ordine.
    #[test]
    fn misure_dei_lati_slint_e_rust() {
        use crate::lati::{larghezza_riposo, scarto_occhi_riposo};
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(0.0, 0.0), "senza lati come M1");
        assert_eq!(p.get_scarto_occhi_riposo(), 0.0);

        p.set_lato_destro(testo_lato("CPU 23%"));
        let destro = p.get_ingombro_lato_destro();
        assert!(destro > 30.0, "il testo ha una larghezza: {destro}");
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(0.0, destro));
        assert_eq!(p.get_scarto_occhi_riposo(), scarto_occhi_riposo(0.0, destro));
        assert_eq!(elemento(&p, "ItsyHome, CPU 23%").accessible_role(), Some(AccessibleRole::Button));

        p.set_lato_sinistro(crate::ContenutoLatoUi {
            tipo: 2,
            testo: "80%".into(),
            frazione: 0.8,
            tinta: crate::TintaBatteriaUi::Normale,
            ..Default::default()
        });
        let sinistro = p.get_ingombro_lato_sinistro();
        assert!(sinistro > 23.0, "glifo (19 + 4) più il testo: {sinistro}");
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(sinistro, destro));
        assert_eq!(p.get_scarto_occhi_riposo(), 0.0, "due lati: occhi al centro");
        assert!(elemento(&p, "ItsyHome, batteria 80%, CPU 23%").accessible_role().is_some());
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "GlifoBatteria").count(), 1);

        // Col puntino i lati restano; allargata la larghezza è quella dell'attenzione.
        p.set_aspetto(AspettoPillolaUi::Puntino);
        assert!(elemento(&p, "ItsyHome, batteria 80%, CPU 23%, una voce in attesa").accessible_role().is_some());
        assert_eq!(p.get_larghezza_pillola(), larghezza_riposo(sinistro, destro));
        p.set_aspetto(AspettoPillolaUi::Allargata);
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTENZIONE);
        assert!(p.get_larghezza_riposo() <= crate::finestre::LARGHEZZA_BLOCCANTE);
    }

    /// M3, Task 4: la pillola media dell'attività di Claude Code. Larga `LARGHEZZA_ATTIVITA`, i lati
    /// si nascondono, il nome per Narrator è quello a parole; da allargata l'attività non si vede.
    #[test]
    fn pillola_media_misure() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        assert_eq!(p.get_larghezza_attivita(), crate::finestre::LARGHEZZA_ATTIVITA);
        p.set_lato_destro(testo_lato("CPU 23%"));
        assert!(p.get_lati_visibili() && !p.get_attivita_visibile());
        assert_ne!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);

        p.set_attivita("Claude · Modifica · 3:12".into());
        p.set_nome_attivita("Claude, Modifica, 3 minuti".into());
        assert!(p.get_attivita_visibile());
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
        assert!(!p.get_lati_visibili(), "i lati si nascondono");
        assert!(p.get_larghezza_visibile() <= p.get_larghezza_finestra());
        assert_eq!(elemento(&p, "ItsyHome, Claude, Modifica, 3 minuti").accessible_role(), Some(AccessibleRole::Button));
        // Col puntino resta; il testo sta dentro la pillola (occhi + testo + margine).
        p.set_aspetto(AspettoPillolaUi::Puntino);
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
        assert!(elemento(&p, "ItsyHome, Claude, Modifica, 3 minuti, una voce in attesa").accessible_role().is_some());
        // Il testo più lungo che Rust può scrivere ci sta senza tagliarsi troppo: lo misura la prova
        // viva; qui basta che la pillola non cresca.
        p.set_attivita("Claude · UnNomeLunghi · 1:02:05".into());
        p.set_attivita_sinistra("Claude · UnNomeLunghi".into());
        p.set_attivita_destra(" · 1:02:05".into());
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
        // La durata sta sempre intera dentro la pillola, anche col nome più lungo: si taglia solo
        // il nome (la sua larghezza arriva fin dove parte la durata).
        let (x, l) = (p.get_attivita_destra_x(), p.get_attivita_destra_larghezza());
        assert!(l > 30.0, "la durata ha una larghezza: {l}");
        assert!(x + l <= crate::finestre::LARGHEZZA_ATTIVITA, "la durata esce dalla pillola: {x} + {l}");
        assert!(x >= 50.0, "la durata non copre gli occhi: {x}");

        // Allargata (una voce che chiama): l'attività non si vede e la larghezza è quella dell'attenzione.
        p.set_aspetto(AspettoPillolaUi::Allargata);
        assert!(!p.get_attivita_visibile());
        assert_eq!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTENZIONE);
        // Ritirata: nemmeno.
        p.set_aspetto(AspettoPillolaUi::Riposo);
        p.set_ritirata(true);
        assert!(!p.get_attivita_visibile());
        // Senza testo la pillola torna quella di prima, coi lati.
        p.set_ritirata(false);
        p.set_attivita("".into());
        assert!(!p.get_attivita_visibile() && p.get_lati_visibili());
        assert_ne!(p.get_larghezza_pillola(), crate::finestre::LARGHEZZA_ATTIVITA);
    }

    /// La sezione «Isola» sta fra Collegamenti e Sistema; «Informazioni» passa alla 5.
    #[test]
    fn sezione_isola_e_informazioni_spostata() {
        let (f, _) = scheda_isola();
        f.set_sezione(0);
        assert_eq!(elemento(&f, "Isola").accessible_item_index(), Some(3));
        assert_eq!(elemento(&f, "Claude").accessible_item_index(), Some(4));
        assert_eq!(elemento(&f, "Sistema").accessible_item_index(), Some(5));
        assert_eq!(elemento(&f, "Informazioni").accessible_item_index(), Some(6));
        assert_eq!(crate::finestra_impostazioni::SEZIONE_CLAUDE, 4);
        assert_eq!(crate::finestra_impostazioni::SEZIONE_INFORMAZIONI, 6);
        f.set_sezione(3);
        assert_eq!(elemento(&f, "Isola").accessible_item_selected(), Some(true));
        for nome in ["A sinistra degli occhi", "A destra degli occhi", "Meteo da", "Quadrante 1", "Quadrante 3"] {
            let e = ElementHandle::find_by_accessible_label(&f, nome)
                .find(|e| e.accessible_role() == Some(AccessibleRole::Combobox))
                .unwrap_or_else(|| panic!("selettore «{nome}»"));
            assert_eq!(e.accessible_expandable(), Some(true));
        }
        // Le note «solo su Windows» compaiono solo fuori da Windows (e con la minuscola dopo i due punti).
        let rete = "«Rete»: solo su Windows, solo l'icona della connessione. Il nome della rete non compare mai nella pillola.";
        let gpu = "«GPU»: solo su Windows.";
        assert!(ElementHandle::find_by_accessible_label(&f, rete).next().is_none(), "nota Rete su Windows");
        assert!(ElementHandle::find_by_accessible_label(&f, gpu).next().is_none(), "nota GPU su Windows");
        f.set_fuori_da_windows(true);
        assert!(ElementHandle::find_by_accessible_label(&f, rete).next().is_some(), "nota Rete fuori da Windows");
        assert!(ElementHandle::find_by_accessible_label(&f, gpu).next().is_some(), "nota GPU fuori da Windows");
        // L'avviso delle cuffie ha la sua sezione «AVVISI», fuori da «PILLOLA».
        assert!(ElementHandle::find_by_accessible_label(&f, "AVVISI").next().is_none(), "AVVISI senza cuffie");
        f.set_avvisi_bt_disponibile(true);
        assert!(ElementHandle::find_by_accessible_label(&f, "AVVISI").next().is_some(), "AVVISI con le cuffie");
    }

    /// Le scelte arrivate a Rust: (0/1 lato o 10 + posizione del quadrante, indice).
    type Scelte = std::rc::Rc<std::cell::RefCell<Vec<(i32, i32)>>>;

    /// Impostazioni › Isola coi selettori riempiti come fa il ponte.
    fn scheda_isola() -> (FinestraImpostazioni, Scelte) {
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        let voci = |v: &[&str]| -> ModelRc<slint::SharedString> {
            ModelRc::new(VecModel::from(v.iter().map(|s| slint::SharedString::from(*s)).collect::<Vec<_>>()))
        };
        f.set_scelte_lati(voci(&["Niente", "Ora", "CPU", "RAM"]));
        f.set_scelte_quadranti(voci(&["Niente", "Ora", "CPU", "RAM", "Disco", "Potenza · Fotovoltaico"]));
        f.set_scelte_meteo(voci(&["Automatico"]));
        let q = |indice, con_massimo| crate::SceltaQuadranteUi {
            indice,
            con_massimo,
            massimo: "".into(),
            predefinito: if con_massimo { "3000".into() } else { "".into() },
        };
        f.set_quadranti_scelti(ModelRc::new(VecModel::from(vec![q(2, false), q(3, false), q(5, true)])));
        f.set_sezione(3);
        f.show().expect("impostazioni mostrate nel backend di prova");
        // Alta abbastanza da vedere tutte le righe (la ricerca salta ciò che lo scorrimento nasconde).
        f.window().set_size(slint::LogicalSize::new(720.0, 900.0));
        f.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        let scelti = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let s = scelti.clone();
        f.on_scegli_lato(move |quale, i| s.borrow_mut().push((quale, i)));
        let s = scelti.clone();
        f.on_scegli_quadrante(move |posizione, i| s.borrow_mut().push((10 + posizione, i)));
        (f, scelti)
    }

    fn selettore(f: &FinestraImpostazioni, nome: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(f, nome)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Combobox))
            .unwrap_or_else(|| {
                let tutti: Vec<String> = i_slint_backend_testing::ElementQuery::from_root(f)
                    .match_descendants()
                    .match_accessible_role(AccessibleRole::Combobox)
                    .find_all()
                    .iter()
                    .map(|e| format!("{:?}", e.accessible_label()))
                    .collect();
                panic!("selettore «{nome}» (ci sono: {tutti:?})")
            })
    }

    /// `Scelta` da tastiera: Tab ci arriva, Invio apre, ↓ e Invio scelgono la seconda voce, Esc
    /// chiude senza scegliere e il fuoco resta sul pulsante.
    #[test]
    fn scelta_da_tastiera() {
        use slint::platform::Key;
        let (f, scelti) = scheda_isola();
        f.set_indice_lato_sinistro(0);
        let sinistra = selettore(&f, "A sinistra degli occhi");
        assert_eq!(sinistra.accessible_value().as_deref(), Some("Niente"));
        assert_eq!(sinistra.accessible_expanded(), Some(false));
        // Tab fino al primo selettore (dopo le sei sezioni).
        for _ in 0..12 {
            if col_fuoco(&f).ends_with("selettore") {
                break;
            }
            tasto_finestra(&f, Key::Tab);
        }
        assert!(col_fuoco(&f).ends_with("selettore"), "Tab arriva al selettore: {}", col_fuoco(&f));
        tasto_finestra(&f, Key::Return);
        assert_eq!(sinistra.accessible_expanded(), Some(true), "Invio apre l'elenco");
        assert!(ElementHandle::find_by_accessible_label(&f, "Ora")
            .any(|e| e.accessible_role() == Some(AccessibleRole::ListItem)), "le voci dell'elenco sono accessibili");
        tasto_finestra(&f, Key::DownArrow);
        tasto_finestra(&f, Key::Return);
        assert_eq!(*scelti.borrow(), [(0, 1)], "↓ e Invio scelgono «Ora»");
        assert_eq!(sinistra.accessible_expanded(), Some(false));
        assert!(col_fuoco(&f).ends_with("selettore"), "il fuoco torna al pulsante: {}", col_fuoco(&f));
        // Il ponte riscrive la scelta: il valore per Narrator la segue.
        f.set_indice_lato_sinistro(1);
        assert_eq!(sinistra.accessible_value().as_deref(), Some("Ora"));
        // Riaperto: Esc chiude, nulla di scelto, il fuoco resta.
        tasto_finestra(&f, Key::Return);
        assert_eq!(sinistra.accessible_expanded(), Some(true));
        tasto_finestra(&f, Key::DownArrow);
        tasto_finestra(&f, Key::Escape);
        assert_eq!(sinistra.accessible_expanded(), Some(false));
        assert_eq!(scelti.borrow().len(), 1, "Esc non sceglie");
        assert!(col_fuoco(&f).ends_with("selettore"), "Esc lascia il fuoco sul pulsante: {}", col_fuoco(&f));
        // Un secondo Esc (fuori dall'elenco) arriva alla finestra, come altrove.
        assert!(ElementHandle::find_by_accessible_label(&f, "Isola").next().is_some());
    }

    /// Il selettore di un quadrante: dice la scelta, Narrator lo apre e sceglie, e accanto a un
    /// sensore in watt c'è il campo «Massimo» col predefinito come suggerimento.
    #[test]
    fn selettore_del_quadrante() {
        let (f, scelti) = scheda_isola();
        let primo = selettore(&f, "Quadrante 1");
        assert_eq!(primo.accessible_value().as_deref(), Some("CPU"));
        assert_eq!(selettore(&f, "Quadrante 3").accessible_value().as_deref(), Some("Potenza · Fotovoltaico"));
        primo.invoke_accessible_default_action();
        assert_eq!(primo.accessible_expanded(), Some(true));
        ElementHandle::find_by_accessible_label(&f, "Disco")
            .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
            .expect("voce «Disco»")
            .invoke_accessible_default_action();
        assert_eq!(*scelti.borrow(), [(10, 4)], "quadrante 1 → «Disco»");
        let campi = |f: &FinestraImpostazioni| -> Vec<ElementHandle> {
            ElementHandle::find_by_accessible_label(f, "Quadrante 3, massimo")
                .filter(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
                .collect()
        };
        let massimi = campi(&f);
        assert_eq!(massimi.len(), 1, "il campo «Massimo» solo accanto al sensore in watt, col nome del quadrante");
        assert_eq!(massimi[0].accessible_placeholder_text().as_deref(), Some("3000"));
        // Giro di correzioni 1: scritto a mano e poi un altro sensore in watt, il campo mostra il
        // massimo del sensore nuovo (il testo scritto non resta).
        massimi[0].set_accessible_value("4500");
        assert_eq!(massimi[0].accessible_value().as_deref(), Some("4500"));
        let modello = f.get_quadranti_scelti();
        modello.set_row_data(
            2,
            crate::SceltaQuadranteUi { indice: 4, con_massimo: true, massimo: "6000".into(), predefinito: "3000".into() },
        );
        assert_eq!(campi(&f)[0].accessible_value().as_deref(), Some("6000"));
    }

    /// M3, Task 12: Impostazioni › Claude › Avvisi. «Avvisa al … % del limite» parte da 80;
    /// un testo che non è un numero da 50 a 100 si rifiuta con un errore che dice cosa scrivere e
    /// non si salva; lo stesso per la soglia di spesa (vuota = nessuna).
    #[test]
    fn soglie_nelle_impostazioni() {
        use slint::platform::{Key, PointerEventButton, WindowEvent};
        piattaforma_di_prova();
        let f = FinestraImpostazioni::new().expect("impostazioni");
        f.set_soglia_limite("80".into());
        f.set_sezione(crate::finestra_impostazioni::SEZIONE_CLAUDE);
        f.show().expect("impostazioni mostrate nel backend di prova");
        f.window().set_size(slint::LogicalSize::new(760.0, 1400.0));
        f.window().dispatch_event(WindowEvent::WindowActiveChanged(true));
        let salvati = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let s = salvati.clone();
        f.on_soglia_limite_scritta(move |t| match crate::claude::leggi_soglia_limite(&t) {
            Ok(v) => {
                s.borrow_mut().push(format!("limite {v}"));
                "".into()
            }
            Err(e) => e.into(),
        });
        let s = salvati.clone();
        f.on_soglia_spesa_scritta(move |t| match crate::claude::leggi_soglia_spesa(&t) {
            Ok(v) => {
                s.borrow_mut().push(format!("spesa {v:?}"));
                "".into()
            }
            Err(e) => e.into(),
        });
        let campo = |nome: &str| {
            ElementHandle::find_by_accessible_label(&f, nome)
                .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
                .unwrap_or_else(|| panic!("campo «{nome}»"))
        };
        // Scrive nel campo e preme Invio (dopo averci dato il fuoco con un clic).
        let scrivi = |nome: &str, testo: &str| {
            let c = campo(nome);
            let (pos, misura) = (c.absolute_position(), c.size());
            let centro = slint::LogicalPosition::new(pos.x + misura.width / 2.0, pos.y + misura.height / 2.0);
            f.window().dispatch_event(WindowEvent::PointerMoved { position: centro });
            f.window().dispatch_event(WindowEvent::PointerPressed { position: centro, button: PointerEventButton::Left });
            f.window().dispatch_event(WindowEvent::PointerReleased { position: centro, button: PointerEventButton::Left });
            c.set_accessible_value(testo);
            f.window().dispatch_event(WindowEvent::KeyPressed { text: Key::Return.into() });
            f.window().dispatch_event(WindowEvent::KeyReleased { text: Key::Return.into() });
        };
        const LIMITE: &str = "Avvisa al … % del limite";
        const SPESA: &str = "Soglia di spesa mensile ($)";
        assert_eq!(campo(LIMITE).accessible_value().as_deref(), Some("80"), "il predefinito");
        assert_eq!(campo(SPESA).accessible_value().as_deref(), Some(""), "nessuna soglia di spesa");
        assert_eq!(campo(LIMITE).accessible_description().as_deref(), Some(""), "nessun errore a riposo");

        scrivi(LIMITE, "abc");
        assert!(salvati.borrow().is_empty(), "«abc» non si salva");
        let errore = campo(LIMITE).accessible_description().unwrap_or_default();
        assert!(errore.contains("50 a 100"), "l'errore dice cosa scrivere: {errore:?}");
        scrivi(LIMITE, "101");
        assert!(salvati.borrow().is_empty(), "oltre 100 non si salva");
        scrivi(LIMITE, "90");
        assert!(!salvati.borrow().is_empty() && salvati.borrow().iter().all(|s| s == "limite 90"), "{:?}", salvati.borrow());
        assert_eq!(campo(LIMITE).accessible_description().as_deref(), Some(""), "salvato: l'errore sparisce");

        let spese = || salvati.borrow().iter().filter(|s| s.starts_with("spesa")).cloned().collect::<Vec<_>>();
        scrivi(SPESA, "zero");
        assert!(spese().is_empty(), "«zero» non si salva");
        assert!(campo(SPESA).accessible_description().unwrap_or_default().contains("maggiore di zero"));
        scrivi(SPESA, "99,5");
        assert_eq!(spese(), ["spesa Some(99.5)"]);
        scrivi(SPESA, "");
        assert_eq!(spese(), ["spesa Some(99.5)", "spesa None"], "vuota = nessuna soglia");
    }

    /// Di serie il pannello mostra i tre quadranti di M1; il modello del ponte li sostituisce
    /// (un sensore col suo valore, l'ora del pannello) e «niente» non c'è.
    #[test]
    fn quadranti_configurabili_nel_pannello() {
        use crate::{FonteQuadranteUi, QuadranteUi};
        piattaforma_di_prova();
        let p = pannello();
        p.set_layout(1);
        for nome in ["CPU", "RAM", "DISCO"] {
            assert_eq!(elemento(&p, nome).accessible_role(), Some(AccessibleRole::ProgressIndicator), "{nome} di serie");
        }
        p.set_ora("14:05".into());
        p.set_quadranti(ModelRc::new(VecModel::from(vec![
            QuadranteUi { fonte: FonteQuadranteUi::Ora, etichetta: "ORA".into(), frazione: -1.0, ..Default::default() },
            QuadranteUi {
                fonte: FonteQuadranteUi::Sensore,
                etichetta: "CASA".into(),
                valore: "21,5 °C".into(),
                frazione: 0.63,
                dettaglio: "Sala".into(),
                genere: crate::GenereUi::Termo,
            },
        ])));
        assert_eq!(elemento(&p, "ORA").accessible_value().as_deref(), Some("14:05"));
        assert_eq!(elemento(&p, "CASA, Sala").accessible_value().as_deref(), Some("21,5 °C"));
        assert!(ElementHandle::find_by_accessible_label(&p, "DISCO").next().is_none(), "il disco non è più scelto");
    }

    // ─── M2, Task 7: volume del PC ───

    /// La barra del volume sta dove stanno gli occhi: con `volume-mostrato` 0.5 c'è la barra
    /// (40 × 6, centrata sugli occhi) e gli occhi no, e la pillola per Narrator è «Volume 50%»;
    /// con −1 tornano gli occhi.
    #[test]
    fn barra_del_volume_al_posto_degli_occhi() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let conta = |id: &str| ElementHandle::find_by_element_id(&p, id).count();
        assert_eq!(conta("Pillola::occhi"), 1);
        assert_eq!(conta("Pillola::barra-volume"), 0);

        p.set_volume_mostrato(0.5);
        assert_eq!(conta("Pillola::barra-volume"), 1, "barra visibile");
        assert_eq!(conta("Pillola::occhi"), 0, "occhi nascosti");
        let barra = ElementHandle::find_by_element_id(&p, "Pillola::barra-volume").next().expect("barra");
        assert_eq!((barra.size().width, barra.size().height), (40.0, 6.0));
        let centro = barra.absolute_position().x + barra.size().width / 2.0;
        assert_eq!(centro, p.get_larghezza_finestra() / 2.0, "senza lati gli occhi (e la barra) stanno al centro");
        assert_eq!(elemento(&p, "Volume 50%").accessible_role(), Some(AccessibleRole::Button));
        p.set_volume_muto(true);
        assert!(ElementHandle::find_by_accessible_label(&p, "Volume 50%, muto").next().is_some());

        p.set_volume_mostrato(-1.0);
        assert_eq!(conta("Pillola::occhi"), 1, "occhi di nuovo visibili");
        assert_eq!(conta("Pillola::barra-volume"), 0);
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome").next().is_some());
    }

    /// La rotella sopra la pillola arriva a Rust solo se accesa (Windows e interruttore) e solo a
    /// riposo o col puntino, mai da allargata.
    #[test]
    fn rotella_sulla_pillola_solo_a_riposo() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let ricevuti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f32>::new()));
        let r = ricevuti.clone();
        p.on_rotella(move |d| r.borrow_mut().push(d));
        elemento(&p, "ItsyHome").scroll(0.0, 60.0);
        assert!(ricevuti.borrow().is_empty(), "spenta di serie");
        p.set_rotella_attiva(true);
        elemento(&p, "ItsyHome").scroll(0.0, 60.0);
        elemento(&p, "ItsyHome").scroll(0.0, -60.0);
        p.set_aspetto(AspettoPillolaUi::Puntino);
        elemento(&p, "ItsyHome, una voce in attesa").scroll(0.0, 60.0);
        assert_eq!(*ricevuti.borrow(), [60.0, -60.0, 60.0]);
        p.set_aspetto(AspettoPillolaUi::Allargata);
        p.set_messaggio("La casa chiede di te".into());
        elemento(&p, "La casa chiede di te").scroll(0.0, 60.0);
        assert_eq!(ricevuti.borrow().len(), 3, "allargata la rotella non cambia il volume");
    }

    /// Quanti elementi accessibili parlano di volume nel pannello.
    fn elementi_volume(p: &Pannello) -> usize {
        i_slint_backend_testing::ElementQuery::from_root(p)
            .match_descendants()
            .find_all()
            .iter()
            .filter(|e| e.accessible_label().is_some_and(|l| l.to_lowercase().contains("volume")))
            .count()
    }

    /// Vista PC (D3 di Paolo): senza volume disponibile la riga non c'è (nessun elemento
    /// «Volume», non una riga spenta); con il volume ci sono l'altoparlante (pulsante, muto sì/no
    /// anche con Narrator) e il cursore (slider col valore).
    #[test]
    fn altoparlante_assente_senza_volume() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_vista("pc".into());
        assert!(ElementHandle::find_by_accessible_label(&p, "RETE").next().is_some(), "vista PC caricata");
        assert_eq!(elementi_volume(&p), 0, "senza volume nessun elemento «Volume»");

        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        let altoparlante = elemento(&p, "Volume 40%");
        assert_eq!(altoparlante.accessible_role(), Some(AccessibleRole::Button));
        let cursore = elemento(&p, "Volume");
        assert_eq!(cursore.accessible_role(), Some(AccessibleRole::Slider));
        assert_eq!(cursore.accessible_value().as_deref(), Some("40%"));
        assert_eq!((cursore.accessible_value_minimum(), cursore.accessible_value_maximum()), (Some(0.0), Some(100.0)));
        assert_eq!(cursore.size().width, 120.0, "largo quanto le barre delle altre righe");
        let muti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let m = muti.clone();
        p.on_muto_volume(move |si| m.borrow_mut().push(si));
        altoparlante.invoke_accessible_default_action();
        assert_eq!(*muti.borrow(), [true]);
        p.set_volume_muto(true);
        assert!(ElementHandle::find_by_accessible_label(&p, "Volume 40%, muto").next().is_some());

        p.set_volume_disponibile(false);
        assert_eq!(elementi_volume(&p), 0, "dispositivo sparito: la riga sparisce");
    }

    /// Il cursore della vista PC da tastiera: il fuoco del contenuto è sul cursore, → due volte
    /// danno +4 punti, Home e Fine 0 e 100, Invio muto sì/no; Narrator lo sposta di 2 punti.
    #[test]
    fn cursore_da_tastiera() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello();
        p.set_app(app());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_volume_disponibile(true);
        p.set_volume(0.40);
        p.set_vista("pc".into());
        let livelli = std::rc::Rc::new(std::cell::RefCell::new(Vec::<i32>::new()));
        let l = livelli.clone();
        p.on_imposta_volume(move |v| l.borrow_mut().push((v * 100.0).round() as i32));
        let muti = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let m = muti.clone();
        p.on_muto_volume(move |si| m.borrow_mut().push(si));

        tasto(&p, Key::RightArrow);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-volume"), "fuoco sul cursore: «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::RightArrow);
        assert_eq!(*livelli.borrow(), [42, 44], "+4 punti");
        assert_eq!(elemento(&p, "Volume").accessible_value().as_deref(), Some("44%"), "il cursore segue subito");
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Home);
        tasto(&p, Key::End);
        assert_eq!(*livelli.borrow(), [42, 44, 42, 0, 100]);
        tasto(&p, Key::Return);
        assert_eq!(*muti.borrow(), [true], "Invio: muto");
        elemento(&p, "Volume").invoke_accessible_decrement_action();
        assert_eq!(livelli.borrow().last(), Some(&98), "Narrator: 2 punti in meno");
        assert_eq!(p.get_vista(), "pc", "le frecce non cambiano vista");
        assert_eq!(
            elemento(&p, "Volume").accessible_description().as_deref(),
            Some("Invio: silenzia o riattiva"),
            "Narrator sa che Invio sul cursore comanda il muto"
        );
    }

    /// Impostazioni › Isola: «Volume con la rotella sulla pillola» solo dove c'è il volume
    /// (Windows), acceso di serie; il commutatore lo dice a Rust.
    #[test]
    fn interruttore_del_volume_nella_sezione_isola() {
        let (f, _) = scheda_isola();
        let interruttore = |f: &FinestraImpostazioni| {
            ElementHandle::find_by_accessible_label(f, "Volume con la rotella sulla pillola")
                .find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
        };
        assert!(interruttore(&f).is_none(), "senza volume la riga non c'è");
        f.set_volume_rotella_disponibile(true);
        let i = interruttore(&f).expect("interruttore");
        assert_eq!(i.accessible_checked(), Some(true), "acceso di serie");
        let cambi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let c = cambi.clone();
        f.on_cambia_volume_rotella(move |si| c.borrow_mut().push(si));
        i.invoke_accessible_default_action();
        assert_eq!(*cambi.borrow(), [false]);
        assert_eq!(i.accessible_checked(), Some(false));
    }

    /// Impostazioni › Isola: «Avvisa quando le cuffie Bluetooth sono scariche» solo su Windows,
    /// acceso di serie; il commutatore lo dice a Rust.
    #[test]
    fn interruttore_degli_avvisi_bt_nella_sezione_isola() {
        let (f, _) = scheda_isola();
        let nome = "Avvisa quando le cuffie Bluetooth sono scariche";
        let interruttore = |f: &FinestraImpostazioni| {
            ElementHandle::find_by_accessible_label(f, nome).find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
        };
        assert!(interruttore(&f).is_none(), "fuori da Windows la riga non c'è");
        f.set_avvisi_bt_disponibile(true);
        let i = interruttore(&f).expect("interruttore");
        assert_eq!(i.accessible_checked(), Some(true), "acceso di serie");
        let cambi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let c = cambi.clone();
        f.on_cambia_avvisi_bt(move |si| c.borrow_mut().push(si));
        i.invoke_accessible_default_action();
        assert_eq!(*cambi.borrow(), [false]);
        assert_eq!(i.accessible_checked(), Some(false));
    }

    // ─── M2, Task 9: le cuffie Bluetooth nella vista PC ───

    fn lettura_bt(imp: &str, nome: &str, percentuale: u8) -> crate::sistema::batterie_bt::LetturaBt {
        crate::sistema::batterie_bt::LetturaBt { impronta: imp.into(), nome: nome.into(), percentuale, connesso: true }
    }

    /// La riga «CUFFIE» sta in «Energia e audio»: valore e glifo con la tinta informazione a 20%
    /// o meno; con più dispositivi «+N» e per Narrator l'elenco; senza dispositivi non c'è.
    #[test]
    fn riga_cuffie_nella_vista_pc() {
        use crate::sistema::batterie_bt::{riga_cuffie, scrivi_cuffie};
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let cuffie = |p: &Pannello| ElementHandle::find_by_accessible_label(p, "CUFFIE").next();
        assert!(cuffie(&p).is_none(), "senza lettura la riga non c'è");
        scrivi_cuffie(&p, riga_cuffie(&[lettura_bt("a", "Cuffie finte", 18)]).as_ref());
        let c = cuffie(&p).expect("riga");
        assert_eq!(c.accessible_value().as_deref(), Some("18%"));
        assert_eq!(c.accessible_description().as_deref(), Some("Cuffie finte al 18%"));
        assert_eq!(p.get_cuffie_tinta(), crate::TintaBatteriaUi::Informazione);
        assert_eq!(p.get_cuffie_dettaglio(), "Cuffie finte");
        // Più dispositivi: il più scarico, «+1» e l'elenco per Narrator.
        scrivi_cuffie(
            &p,
            riga_cuffie(&[lettura_bt("a", "Cuffie finte", 60), lettura_bt("b", "Mouse finto", 30)]).as_ref(),
        );
        let c = cuffie(&p).expect("riga");
        assert_eq!(p.get_cuffie_dettaglio(), "Mouse finto +1");
        assert_eq!(p.get_cuffie_tinta(), crate::TintaBatteriaUi::Normale);
        assert_eq!(c.accessible_description().as_deref(), Some("Mouse finto al 30%, Cuffie finte al 60%"));
        // Nessun dispositivo connesso con la batteria leggibile: la riga sparisce.
        scrivi_cuffie(&p, None);
        assert!(cuffie(&p).is_none());
    }

    // ─── M2, Task 8: Wi-Fi, Bluetooth e icona della rete ───

    use crate::sistema::radio::{EsitoRadio, StatoRadio, TipoRadio};

    /// Un pannello sulla vista PC con tutto: batteria, volume muto, connessioni con nome lungo,
    /// rete veloce, Wi-Fi acceso e Bluetooth spento.
    fn pannello_connessioni() -> Pannello {
        let p = pannello();
        p.set_app(app());
        p.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        p.global::<crate::Fuochi>().set_pronti(true);
        p.set_pc(StatPcUi {
            pronta: true,
            cpu: 0.5,
            cpu_testo: "100%".into(),
            ram: 0.6,
            ram_testo: "100%".into(),
            ram_dettaglio: "15,9 / 15,9 GB".into(),
            disco: 0.7,
            disco_testo: "100%".into(),
            disco_dettaglio: "1023,9 / 1023,9 GB".into(),
            rete_giu: "999,9 MB/s".into(),
            rete_su: "999,9 MB/s".into(),
            ha_batteria: true,
            batteria_testo: "100%".into(),
            batteria_frazione: 1.0,
            in_carica: true,
            ha_gpu: true,
            gpu: 1.0,
            gpu_testo: "100%".into(),
            ..Default::default()
        });
        p.set_volume_disponibile(true);
        p.set_volume(0.4);
        p.set_volume_muto(true);
        p.set_connessioni_disponibili(true);
        p.set_rete_nota(true);
        p.set_rete(crate::TipoReteUi::Wifi);
        p.set_tacche(2);
        p.set_nome_rete("Una rete di prova dal nome davvero lunghissimo".into());
        crate::connessioni::scrivi_radio(
            &p,
            &[
                StatoRadio { tipo: TipoRadio::WiFi, acceso: true },
                StatoRadio { tipo: TipoRadio::Bluetooth, acceso: false },
            ],
        );
        p.set_vista("pc".into());
        p
    }

    /// Il pannello è 800 × 300 fisso: nella vista PC, col caso più pieno (batteria, volume muto,
    /// rete veloce, nome della rete lungo, riga del rifiuto, GPU al 100%), nulla esce dalla card né
    /// dalla sua colonna.
    #[test]
    fn vista_pc_entra_nel_pannello() {
        piattaforma_di_prova();
        let p = pannello_connessioni();
        p.set_errore_radio(true);
        // Il caso più pieno anche per le cuffie: nome lungo e due dispositivi in più.
        crate::sistema::batterie_bt::scrivi_cuffie(
            &p,
            crate::sistema::batterie_bt::riga_cuffie(&[
                lettura_bt("a", "Cuffie finte con un nome lunghissimo", 18),
                lettura_bt("b", "Mouse finto", 50),
                lettura_bt("c", "Tastiera finta", 90),
            ])
            .as_ref(),
        );
        let vista = ElementHandle::find_by_element_type_name(&p, "VistaPc").next().expect("vista PC");
        let dentro = |contenitore: &ElementHandle, nome: &str| {
            let (cp, cs) = (contenitore.absolute_position(), contenitore.size());
            for e in contenitore.query_descendants().find_all() {
                let (pos, lato) = (e.absolute_position(), e.size());
                if lato.width <= 0.0 || lato.height <= 0.0 {
                    continue;
                }
                let fuori = pos.x < cp.x - 0.5
                    || pos.y < cp.y - 0.5
                    || pos.x + lato.width > cp.x + cs.width + 0.5
                    || pos.y + lato.height > cp.y + cs.height + 0.5;
                assert!(
                    !fuori,
                    "{:?} «{}» esce da {nome}: {pos:?} {lato:?} fuori da {cp:?} {cs:?}",
                    e.type_name(),
                    e.accessible_label().unwrap_or_default()
                );
            }
        };
        dentro(&vista, "la vista PC");
        let (pos, lato) = (vista.absolute_position(), vista.size());
        assert!(pos.x >= 0.0 && pos.y >= 0.0 && pos.x + lato.width <= 800.0 && pos.y + lato.height <= 300.0);
        for colonna in ["Sistema", "Energia e audio", "Connessioni"] {
            dentro(&elemento(&p, colonna), colonna);
        }
        // La riga GPU (Task 10) c'è, è l'ultima della colonna «Sistema» e sta nella card.
        let gpu = elemento(&p, "GPU");
        let (gp, gs) = (gpu.absolute_position(), gpu.size());
        let disco = elemento(&p, "DISCO");
        assert!(gp.y >= disco.absolute_position().y + disco.size().height - 0.5, "GPU sotto il disco");
        assert!(gp.y + gs.height <= pos.y + lato.height + 0.5, "GPU dentro la card");
        // La riga del rifiuto c'è e il nome della rete si vede (accorciato).
        assert!(ElementHandle::find_by_accessible_label(&p, "Non riuscito: apro le Impostazioni di Windows").next().is_some());
        assert_eq!(
            elemento(&p, "Wi-Fi").accessible_description().as_deref(),
            Some("Una rete di prova dal nome davvero lunghissimo")
        );
        assert_eq!(
            elemento(&p, "RETE").accessible_value().as_deref(),
            Some("↓ 999,9 MB/s · ↑ 999,9 MB/s, Wi-Fi, 2 tacche su 3")
        );
    }

    /// M2, Task 10: nella vista PC la riga «GPU» (indicatore col valore, barra come la CPU) c'è
    /// solo se c'è il dato; senza (fuori da Windows, lettura fallita) nessuna riga, mai «0%».
    #[test]
    fn riga_gpu_solo_se_c_e() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_vista("pc".into());
        p.set_pc(StatPcUi { pronta: true, ha_gpu: false, gpu: 0.37, gpu_testo: "37%".into(), ..Default::default() });
        assert!(ElementHandle::find_by_accessible_label(&p, "GPU").next().is_none(), "senza dato niente riga");
        p.set_pc(StatPcUi { pronta: true, ha_gpu: true, gpu: 0.37, gpu_testo: "37%".into(), ..Default::default() });
        let gpu = elemento(&p, "GPU");
        assert_eq!(gpu.accessible_role(), Some(AccessibleRole::ProgressIndicator));
        assert_eq!(gpu.accessible_value().as_deref(), Some("37%"));
        // Il testo «37%» si vede nella riga.
        let testo = gpu
            .query_descendants()
            .find_all()
            .into_iter()
            .any(|e| e.accessible_label().as_deref() == Some("37%"));
        assert!(testo, "il valore della GPU si vede");
    }

    /// M2, Task 10: il quadrante «GPU» della Panoramica a Quadranti legge `StatPcUi`: col dato
    /// valore e arco, senza «—».
    #[test]
    fn quadrante_gpu() {
        piattaforma_di_prova();
        let p = pannello();
        p.set_layout(1);
        p.set_quadranti(slint::ModelRc::new(slint::VecModel::from(crate::ponte::quadranti_ui(
            &[crate::lati::Lato::Gpu],
            None,
            &std::collections::HashMap::new(),
            &crate::lati::DatiClaude::default(),
        ))));
        let gpu = elemento(&p, "GPU");
        assert_eq!(gpu.accessible_value().as_deref(), Some("—"));
        p.set_pc(StatPcUi { pronta: true, ha_gpu: true, gpu: 0.37, gpu_testo: "37%".into(), ..Default::default() });
        assert_eq!(elemento(&p, "GPU").accessible_value().as_deref(), Some("37%"));
    }

    /// Riquadri: interruttori per Narrator con lo stato; Bluetooth spento; senza radio
    /// «Non presente» e non cliccabile. Fuori da Windows solo la rete.
    #[test]
    fn riquadri_wifi_e_bluetooth() {
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let wifi = elemento(&p, "Wi-Fi");
        assert_eq!(wifi.accessible_role(), Some(AccessibleRole::Switch));
        assert_eq!(wifi.accessible_checked(), Some(true));
        let bt = elemento(&p, "Bluetooth");
        assert_eq!(bt.accessible_role(), Some(AccessibleRole::Switch));
        assert_eq!(bt.accessible_checked(), Some(false));
        assert_eq!(bt.accessible_description().as_deref(), Some("Spento"));
        let comandi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(i32, bool)>::new()));
        let c = comandi.clone();
        p.on_commuta_radio(move |t, a| c.borrow_mut().push((t, a)));
        bt.invoke_accessible_default_action();
        assert_eq!(*comandi.borrow(), [(1, true)]);
        assert_eq!(bt.accessible_checked(), Some(true), "l'interruttore si sposta subito");

        // Senza Bluetooth: «Non presente», non cliccabile.
        crate::connessioni::scrivi_radio(&p, &[StatoRadio { tipo: TipoRadio::WiFi, acceso: false }]);
        let bt = elemento(&p, "Bluetooth");
        assert_eq!(bt.accessible_description().as_deref(), Some("Non presente"));
        assert_eq!(bt.accessible_enabled(), Some(false));
        assert_eq!(bt.accessible_checked(), Some(false));
        bt.invoke_accessible_default_action();
        assert_eq!(comandi.borrow().len(), 1, "una radio che manca non si comanda");
        assert_eq!(elemento(&p, "Wi-Fi").accessible_description().as_deref(), Some("Spento"));

        // Fuori da Windows (connessioni non disponibili): solo la rete, niente riquadri né righe.
        p.set_connessioni_disponibili(false);
        assert!(ElementHandle::find_by_accessible_label(&p, "Wi-Fi").next().is_none());
        assert!(ElementHandle::find_by_accessible_label(&p, "Reti Wi-Fi ›").next().is_none());
        assert_eq!(elemento(&p, "RETE").accessible_value().as_deref(), Some("↓ 999,9 MB/s · ↑ 999,9 MB/s"));
    }

    /// Review focus 4: Windows rifiuta lo spegnimento del Wi-Fi. Il riquadro si sposta subito su
    /// spento; all'esito `Negato` compare la riga in ambra, il riquadro torna allo stato vero
    /// (acceso, l'ultimo `StateChanged`) e si apre la pagina del Wi-Fi delle Impostazioni.
    #[test]
    fn rifiuto_mostra_errore_e_stato_reale() {
        use crate::sistema::browser::PaginaWindows;
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let reali = [
            StatoRadio { tipo: TipoRadio::WiFi, acceso: true },
            StatoRadio { tipo: TipoRadio::Bluetooth, acceso: false },
        ];
        let comandi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(i32, bool)>::new()));
        let c = comandi.clone();
        p.on_commuta_radio(move |t, a| c.borrow_mut().push((t, a)));
        let riga = |p: &Pannello| {
            ElementHandle::find_by_accessible_label(p, "Non riuscito: apro le Impostazioni di Windows").next().is_some()
        };

        elemento(&p, "Wi-Fi").invoke_accessible_default_action();
        assert_eq!(*comandi.borrow(), [(0, false)], "il comando: Wi-Fi spento");
        assert_eq!(elemento(&p, "Wi-Fi").accessible_checked(), Some(false));
        assert!(!riga(&p));

        let pagina = crate::connessioni::esito(&p, Some(&reali), TipoRadio::WiFi, EsitoRadio::Negato);
        assert_eq!(pagina, Some(PaginaWindows::ImpostazioniWifi), "si apre la pagina del Wi-Fi");
        assert!(riga(&p), "la riga in ambra c'è");
        assert_eq!(elemento(&p, "Wi-Fi").accessible_checked(), Some(true), "di nuovo acceso, lo stato vero");

        // Un errore del Bluetooth apre la pagina del Bluetooth; un nuovo tocco toglie la riga.
        assert_eq!(
            crate::connessioni::esito(&p, Some(&reali), TipoRadio::Bluetooth, EsitoRadio::Errore),
            Some(PaginaWindows::Bluetooth)
        );
        elemento(&p, "Bluetooth").invoke_accessible_default_action();
        assert!(!riga(&p), "il tocco nuovo toglie la riga del rifiuto");
        assert_eq!(crate::connessioni::esito(&p, Some(&reali), TipoRadio::Bluetooth, EsitoRadio::Fatto), None);
        assert!(!riga(&p), "riuscito: nessuna riga");
    }

    /// Un rifiuto che arriva dopo aver lasciato la vista PC (radio non più seguite) si ignora:
    /// nessuna riga in ambra al rientro, nessuna pagina aperta, i riquadri non diventano
    /// «Non presente».
    #[test]
    fn esito_a_vista_pc_lasciata_ignorato() {
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let riga = |p: &Pannello| {
            ElementHandle::find_by_accessible_label(p, "Non riuscito: apro le Impostazioni di Windows").next().is_some()
        };
        // Lasciata la vista: le radio sono dimenticate.
        crate::connessioni::dimentica_radio(&p);
        assert!(!p.get_radio_lette());
        for esito in [EsitoRadio::Negato, EsitoRadio::Errore] {
            assert_eq!(crate::connessioni::esito(&p, None, TipoRadio::WiFi, esito), None, "{esito:?}: nessuna pagina");
        }
        assert!(!p.get_errore_radio(), "nessuna riga in ambra da mostrare al rientro");
        assert!(!p.get_radio_lette(), "i riquadri non passano a «Non presente»");
        assert!(p.get_wifi_presente() && p.get_bt_presente(), "le radio non risultano sparite");
        assert!(!riga(&p));
    }

    /// Vista PC da tastiera: dal volume Tab passa ai riquadri e alle righe (Wi-Fi, Bluetooth,
    /// «Reti Wi-Fi ›», «Dispositivi Bluetooth ›») e poi al dock; Maiusc+Tab torna indietro. Le
    /// frecce girano nella colonna; Invio commuta un riquadro o apre la pagina; Esc chiude il
    /// pannello come prima.
    #[test]
    fn connessioni_da_tastiera() {
        use slint::platform::Key;
        piattaforma_di_prova();
        let p = pannello_connessioni();
        let eventi = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let (e1, e2, e3) = (eventi.clone(), eventi.clone(), eventi.clone());
        p.on_commuta_radio(move |t, a| e1.borrow_mut().push(format!("radio {t} {a}")));
        p.on_apri_pagina(move |i| e2.borrow_mut().push(format!("pagina {i}")));
        p.on_chiudi(move || e3.borrow_mut().push("chiudi".into()));
        let ultimo = |e: &std::rc::Rc<std::cell::RefCell<Vec<String>>>| e.borrow().last().cloned().unwrap_or_default();

        // Il contenuto parte dal volume; Tab: Wi-Fi (il fuoco vero sul riquadro), poi Bluetooth.
        tasto(&p, Key::Tab);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-riquadro"), "fuoco sul riquadro: «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::Tab);
        tasto(&p, Key::Return);
        assert_eq!(*eventi.borrow(), ["radio 1 true"], "Invio sul Bluetooth (spento) lo accende");
        // ↓ dal Bluetooth: «Dispositivi Bluetooth ›»; Invio apre la pagina del Bluetooth.
        tasto(&p, Key::DownArrow);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-collegamento"), "fuoco sulla riga: «{}»", col_fuoco_pannello(&p));
        tasto(&p, Key::Return);
        assert_eq!(ultimo(&eventi), "pagina 1");
        // ←: «Reti Wi-Fi ›»; Spazio apre la pagina Wi-Fi delle Impostazioni.
        tasto(&p, Key::LeftArrow);
        tasto(&p, Key::Space);
        assert_eq!(ultimo(&eventi), "pagina 0");
        // ↑: di nuovo il Wi-Fi (Invio lo spegne).
        tasto(&p, Key::UpArrow);
        tasto(&p, Key::Return);
        assert_eq!(ultimo(&eventi), "radio 0 false");
        // Maiusc+Tab dal Wi-Fi torna al volume.
        con_modificatori_pannello(&p, &[Key::Shift], Key::Tab);
        assert!(col_fuoco_pannello(&p).ends_with("::fuoco-volume"), "fuoco sul cursore: «{}»", col_fuoco_pannello(&p));
        // Quattro Tab: Wi-Fi, Bluetooth, reti, dispositivi.
        for _ in 0..4 {
            tasto(&p, Key::Tab);
        }
        tasto(&p, Key::Return);
        assert_eq!(ultimo(&eventi), "pagina 1", "l'ultima riga");
        // Ancora Tab: il dock (le frecce lì muovono il fuoco, non la vista).
        tasto(&p, Key::Tab);
        tasto(&p, Key::RightArrow);
        assert_eq!(p.get_vista(), "pc");
        assert_eq!(ultimo(&eventi), "pagina 1", "dal dock nessun comando delle connessioni");
        // Esc fa quello che fa già nel pannello: lo chiude.
        tasto(&p, Key::Escape);
        assert_eq!(ultimo(&eventi), "chiudi");
    }

    /// La pillola col lato «Rete»: solo l'icona (16 px, posto riservato anche prima della
    /// lettura), e Narrator dice il tipo e le tacche, mai il nome della rete.
    #[test]
    fn lato_rete_nella_pillola() {
        use crate::lati::{DatiLati, Lato};
        use crate::sistema::radio::StatoRete;
        use crate::sorgenti_lati::lato_ui;
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let mut d = DatiLati::default();
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, false));
        assert_eq!(p.get_ingombro_lato_destro(), 16.0, "il posto dell'icona c'è già");
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome").next().is_some());
        d.rete = Some(StatoRete::WiFi { tacche: 2 });
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, false));
        assert_eq!(p.get_ingombro_lato_destro(), 16.0);
        assert_eq!(p.get_larghezza_pillola(), crate::lati::larghezza_riposo(0.0, 16.0));
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome, Wi-Fi, 2 tacche su 3").next().is_some());
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "IconaRete").count(), 1);
        d.rete = Some(StatoRete::Cavo);
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, false));
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome, rete via cavo").next().is_some());
        // In «dorme» `OsservaRete` è spento: l'icona si svuota (non mostra una rete forse caduta),
        // ma il posto resta e la pillola non cambia larghezza (m1 della revisione finale di M2).
        let larghezza = p.get_larghezza_pillola();
        p.set_lato_destro(lato_ui(&Lato::Rete, &d, true));
        assert_eq!(ElementHandle::find_by_element_type_name(&p, "IconaRete").count(), 0, "in «dorme» l'icona si svuota");
        assert!(ElementHandle::find_by_accessible_label(&p, "ItsyHome").next().is_some(), "Narrator non legge la rete ferma");
        assert_eq!(p.get_ingombro_lato_destro(), 16.0, "il posto resta");
        assert_eq!(p.get_larghezza_pillola(), larghezza, "stessa larghezza");
    }
}
