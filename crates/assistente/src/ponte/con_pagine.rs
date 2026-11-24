//! Le pagine sugli impianti di F3 nel ponte (ridisegno F3, Task 4b): per ora la pagina Casa
//! sull'impianto Comandi (dal Task 5 anche Collegamenti, sugli stessi modelli).
//!
//! Leggerezza (spec §3): i modelli della pagina (`comandi_*`) si riempiono solo a pagina aperta
//! (`pagina_comandi`), si riscrivono solo le righe cambiate (`aggiorna_modello`) e le proprietà
//! solo se cambiano (`scrivi_se_cambia`); lasciata la pagina si svuotano. Ogni dato della casa che
//! cambia passa da `casa_cambiata` (pre-flight C10): la pagina, se aperta, e la carta.

use super::{aggiorna_modello, luci_accese_ora, ponte, scrivi_se_cambia, Ponte};
use crate::moduli::casa;
use crate::moduli::pagina_casa::{self, FILTRO_ALTRO};
use crate::{CardUi, FiltroUi, Pannello, SintesiUi, StatoPaginaUi};
use casa_core::diagnosi::Contatore;
use slint::{Model, ModelRc, SharedString};

/// Ciò che la pagina Casa scrive, calcolato coi dati di adesso (vedi `Ponte::pagina_casa`).
struct PaginaCasa {
    filtro: String,
    filtri: Vec<FiltroUi>,
    card: Vec<CardUi>,
    scene: Vec<CardUi>,
    sintesi: SintesiUi,
    stato: StatoPaginaUi,
    nota: &'static str,
    /// Gli indici del filtro scelto e di quello iniziale fra i filtri.
    indice: i32,
    iniziale: i32,
}

impl Ponte {
    /// Il pannello appena creato: i modelli dell'impianto Comandi e i suoi due gesti.
    pub(super) fn comandi_nel_pannello(&self, p: &Pannello) {
        p.set_comandi_filtri(ModelRc::from(self.comandi_filtri.clone()));
        p.set_comandi_card(ModelRc::from(self.comandi_card.clone()));
        p.set_comandi_scene(ModelRc::from(self.comandi_scene.clone()));
        p.on_filtro_pagina(|modulo, filtro| {
            if let Some(ponte) = ponte() {
                ponte.filtro_pagina(&modulo, &filtro);
            }
        });
        p.on_tocca_card(|modulo, id| {
            if let Some(ponte) = ponte() {
                ponte.tocca_card(&modulo, &id);
            }
        });
    }

    /// Il pannello mostra la vista `vista` (cambio di vista, apertura su una vista chiesta,
    /// chiusura con ""): la pagina dei Comandi che si apre si scrive da capo, col filtro chiesto
    /// prima dell'apertura o con quello iniziale; quella lasciata svuota i suoi modelli.
    pub fn vista_comandi(&self, vista: &str) {
        let nuova = match vista {
            "casa" => Some("casa"),
            _ => None,
        };
        if self.pagina_comandi.get() == nuova {
            return;
        }
        self.pagina_comandi.set(nuova);
        if self.comandi_filtri.row_count() + self.comandi_card.row_count() + self.comandi_scene.row_count() > 0 {
            self.comandi_filtri.set_vec(Vec::new());
            self.comandi_card.set_vec(Vec::new());
            self.comandi_scene.set_vec(Vec::new());
        }
        *self.filtro_casa.borrow_mut() = None;
        self.filtro_mostrato.borrow_mut().clear();
        if nuova.is_none() {
            // I dati di nessuna pagina: l'impianto non mostra nulla finché una pagina non riscrive.
            if let Some(p) = self.pannello.borrow().upgrade() {
                scrivi_se_cambia(p.get_comandi_modulo(), SharedString::new(), |v| p.set_comandi_modulo(v));
            }
        }
        self.scrivi_pagina_casa();
    }

    /// Un dato della casa è cambiato (stato, dati, un tocco, un esito, un'attesa scaduta, la
    /// configurazione): la pagina Casa, se aperta, e la carta di Casa.
    pub(super) fn casa_cambiata(&self) {
        self.scrivi_pagina_casa();
        self.moduli_aggiorna("casa");
    }

    /// Un filtro scelto nella pagina (clic, tastiera) o chiesto dalla carta Casa con le luci accese.
    /// Un solo ingresso (pre-flight C9): a pagina aperta lo sceglie subito; a pagina chiusa lo tiene
    /// per la prima scrittura dopo l'apertura (la carta cambia vista e chiede il filtro nello
    /// stesso gesto: la richiesta può arrivare prima o dopo che la pagina si apra).
    pub fn filtro_pagina(&self, modulo: &str, filtro: &str) {
        if modulo != "casa" {
            eprintln!("filtro di una pagina sconosciuta: {modulo} {filtro}");
            return;
        }
        if self.pagina_comandi.get() == Some("casa") {
            *self.filtro_casa.borrow_mut() = Some(filtro.to_owned());
            self.scrivi_pagina_casa();
        } else {
            *self.filtro_richiesto.borrow_mut() = Some(filtro.to_owned());
        }
    }

    /// Una card o una scena toccata (clic, Invio, Narrator): il suo comando. Va chiamata solo
    /// subito dopo il gesto dell'utente.
    pub fn tocca_card(&self, modulo: &str, id: &str) {
        match modulo {
            "casa" => self.tocca(id),
            _ => eprintln!("tocco di una pagina sconosciuta: {modulo} {id}"),
        }
    }

    /// «Spegni la stanza» della testata: le luci accese della stanza del filtro scelto (con
    /// l'ottimismo, come `spegni_tutte`). Fuori da una stanza non fa nulla.
    pub(super) fn spegni_la_stanza_scelta(&self) {
        let Some(filtro) = self.filtro_casa.borrow().clone() else { return };
        let area = if filtro == FILTRO_ALTRO { String::new() } else { filtro };
        let c_e = self.menu.borrow().as_ref().is_some_and(|m| m.rooms.iter().any(|r| r.area_id == area));
        if c_e {
            self.spegni_stanza(&area);
        }
    }

    /// La pagina Casa coi dati di adesso: il filtro (quello chiesto, o lo scelto se c'è ancora,
    /// altrimenti l'iniziale), i filtri col numero delle luci accese, le card del filtro e le scene
    /// con l'ottimismo dei tocchi e gli errori, la sintesi (`r1.fatto` della carta), lo stato e la
    /// nota. `scelto`: il filtro voluto (`None` = l'iniziale).
    fn pagina_casa(&self, scelto: Option<&str>) -> PaginaCasa {
        let connesso = self.connesso();
        let icone = self.casa.con_config(|c| c.area_icons.clone());
        let stato = self.stato.borrow();
        let menu = self.menu.borrow();
        let menu = menu.as_deref();
        let dati = self.dati_delle.borrow();
        let ottimismo = self.ottimismo.borrow();
        let errori = self.errori.borrow();
        let filtro = match (menu, scelto) {
            (Some(m), Some(s)) => pagina_casa::filtro_valido(m, s),
            (Some(m), None) => pagina_casa::filtro_iniziale(m).to_owned(),
            (None, s) => s.unwrap_or_default().to_owned(),
        };
        let pronta = |mut p: super::Preferito| {
            p.errore = errori.get(&p.entity_id).cloned();
            pagina_casa::card(&p, connesso)
        };
        let accese_casa = menu.map_or(0, |m| luci_accese_ora(m, &ottimismo).len());
        let (filtri, card, scene, accese, iniziale) = match menu {
            Some(m) => (
                pagina_casa::filtri(m, accese_casa, &icone),
                pagina_casa::del_filtro(m, &filtro, &ottimismo).into_iter().map(pronta).collect(),
                pagina_casa::scene(m, &filtro).into_iter().map(pronta).collect(),
                pagina_casa::luci_accese_del_filtro(m, &filtro, &ottimismo),
                pagina_casa::filtro_iniziale(m),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), 0, ""),
        };
        // Il titolo della sintesi è `r1.fatto` della carta di Casa (un solo testo per i due posti).
        let r1 = casa::riassunto(&stato, menu, accese_casa, dati.as_deref()).r1;
        let posto = |id: &str| filtri.iter().position(|f| f.id.as_str() == id).unwrap_or(0) as i32;
        PaginaCasa {
            indice: posto(&filtro),
            iniziale: posto(iniziale),
            sintesi: pagina_casa::sintesi(&stato, &r1, &filtro, menu, accese, dati.as_deref()),
            stato: pagina_casa::stato(&stato, menu),
            nota: pagina_casa::nota(&filtro, card.len()),
            filtro,
            filtri,
            card,
            scene,
        }
    }

    /// Scrive la pagina Casa, solo se è aperta (`pagina_comandi`) e solo ciò che è cambiato: i
    /// modelli riga per riga, le proprietà se diverse. La card col fuoco resta la stessa per id (o
    /// la più vicina se la sua sparisce); un filtro nuovo riparte dalla prima. Un filtro scelto
    /// che non c'è più (una stanza nascosta in Home Assistant) torna a quello iniziale col fuoco sui
    /// filtri (Review Focus 1). Il filtro chiesto prima dell'apertura vale una volta sola.
    pub fn scrivi_pagina_casa(&self) {
        if self.pagina_comandi.get() != Some("casa") {
            return;
        }
        let richiesto = self.filtro_richiesto.borrow_mut().take();
        let scelto = richiesto.or_else(|| self.filtro_casa.borrow().clone());
        let pagina = self.pagina_casa(scelto.as_deref());
        let con_dati = self.menu.borrow().is_some();
        let sparito = con_dati && scelto.as_deref().is_some_and(|s| s != pagina.filtro);
        // Senza dati il filtro voluto resta per quando arrivano.
        *self.filtro_casa.borrow_mut() = if con_dati { Some(pagina.filtro.clone()) } else { scelto };
        let stesso_filtro = *self.filtro_mostrato.borrow() == pagina.filtro;
        *self.filtro_mostrato.borrow_mut() = pagina.filtro.clone();

        let p = self.pannello.borrow().upgrade();
        let fuoco = p.as_ref().map_or(0, |p| p.get_comandi_fuoco().max(0) as usize);
        let col_fuoco = if stesso_filtro { self.comandi_card.row_data(fuoco).map(|c| c.id) } else { None };
        let nuovo_fuoco = if stesso_filtro {
            col_fuoco
                .and_then(|id| pagina.card.iter().position(|c| c.id == id))
                .unwrap_or_else(|| fuoco.min(pagina.card.len().saturating_sub(1)))
        } else {
            0
        };

        let d = self.casa.diagnosi();
        d.conta(Contatore::ui_ricostruzioni);
        let righe = aggiorna_modello(&self.comandi_filtri, pagina.filtri)
            + aggiorna_modello(&self.comandi_card, pagina.card)
            + aggiorna_modello(&self.comandi_scene, pagina.scene);
        d.conta_n(Contatore::ui_righe, righe);
        let Some(p) = p else { return };
        scrivi_se_cambia(p.get_comandi_modulo(), SharedString::from("casa"), |v| p.set_comandi_modulo(v));
        scrivi_se_cambia(p.get_comandi_filtro(), pagina.indice, |v| p.set_comandi_filtro(v));
        scrivi_se_cambia(p.get_comandi_filtro_iniziale(), pagina.iniziale, |v| p.set_comandi_filtro_iniziale(v));
        scrivi_se_cambia(p.get_comandi_fuoco(), nuovo_fuoco as i32, |v| p.set_comandi_fuoco(v));
        scrivi_se_cambia(p.get_comandi_sintesi(), pagina.sintesi, |v| p.set_comandi_sintesi(v));
        scrivi_se_cambia(p.get_comandi_stato(), pagina.stato, |v| p.set_comandi_stato(v));
        scrivi_se_cambia(p.get_comandi_nota(), SharedString::from(pagina.nota), |v| p.set_comandi_nota(v));
        if sparito {
            scrivi_se_cambia(p.get_comandi_lato(), 0, |v| p.set_comandi_lato(v));
        }
    }
}

#[cfg(test)]
mod test {
    use crate::moduli::casa::{RIPROVA, SPEGNI};
    use crate::moduli::pagina_casa::{FILTRO_LUCI, FILTRO_PREFERITI, FILTRO_TUTTA, SPEGNI_STANZA};
    use crate::ponte::test::{interruttore, luce, menu, ponte_di_prova, scena, Spegni};
    use crate::ponte::{Ponte, PROPRIETA_SCRITTE, RIGHE_SCRITTE};
    use crate::{Pannello, TipoStatoPaginaUi};
    use casa_core::eventi::{CasaEvento, StatoConnessione};
    use casa_core::types::{AppEntity, MenuData};
    use slint::Model;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

    /// La casa di prova: una luce preferita, la stanza «a0» con due luci accese e una presa, la
    /// stanza «a1» con una luce accesa, e una scena.
    fn casa_di_prova(preferiti: bool, presa: bool) -> MenuData {
        let mut m = menu(
            if preferiti { vec![luce("light.a", true)] } else { vec![] },
            vec![vec![luce("light.a", true), luce("light.b", true), interruttore("switch.p", presa)], vec![luce("light.c", true)]],
        );
        let AppEntity::Scene(s) = scena("scene.sera") else { unreachable!() };
        m.scenes = vec![s];
        m
    }

    /// Il ponte di prova col pannello aperto sulla pagina Casa (la vista e la pagina), coi dati.
    fn casa_aperta(m: MenuData) -> (Spegni, Rc<Ponte>, Pannello, impl Sized) {
        let (spegni, ponte, pillola, cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        ponte.ricevi(CasaEvento::Dati(Arc::new(m)));
        p.set_vista("casa".into());
        ponte.vista_comandi("casa");
        (spegni, ponte, p, (pillola, cartella))
    }

    fn id_card(p: &Pannello) -> Vec<String> {
        p.get_comandi_card().iter().map(|c| c.id.to_string()).collect()
    }

    fn id_filtro(p: &Pannello) -> String {
        p.get_comandi_filtri().row_data(p.get_comandi_filtro() as usize).map(|f| f.id.to_string()).unwrap_or_default()
    }

    fn azzera() {
        RIGHE_SCRITTE.with(|c| c.set(0));
        PROPRIETA_SCRITTE.with(|c| c.set(0));
    }

    fn scritte() -> (u64, u64) {
        (RIGHE_SCRITTE.with(Cell::get), PROPRIETA_SCRITTE.with(Cell::get))
    }

    /// Review Focus 5 (pre-flight C11): due scritture di fila senza cambi non riscrivono nessuna
    /// riga e nessuna proprietà; un cambio vero sì. A pagina chiusa un evento di Home Assistant non
    /// scrive nulla (e i modelli sono vuoti). Un filtro che non c'è più torna a quello iniziale.
    #[test]
    fn pagina_casa_scritta_solo_se_cambia() {
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(false, false));
        assert_eq!(p.get_comandi_modulo(), "casa");
        assert_eq!(id_filtro(&p), FILTRO_TUTTA, "senza preferiti si parte da «Tutta la casa»");
        assert_eq!(id_card(&p), ["light.a", "light.b", "switch.p", "light.c"]);
        assert_eq!(p.get_comandi_scene().row_count(), 1);
        azzera();
        ponte.scrivi_pagina_casa();
        ponte.scrivi_pagina_casa();
        assert_eq!(scritte(), (0, 0), "nulla di cambiato, nulla di scritto");
        // Un cambio vero: una luce spenta (la card e la sintesi cambiano).
        ponte.tocca("light.b");
        let (righe, proprieta) = scritte();
        assert!(righe > 0 && proprieta > 0, "un cambio si scrive: {righe} righe, {proprieta} proprietà");

        // A pagina chiusa: modelli vuoti, e un evento di HA che cambierebbe la pagina (la presa,
        // non la carta) non scrive nulla.
        ponte.vista_comandi("panoramica");
        assert_eq!((p.get_comandi_card().row_count(), p.get_comandi_filtri().row_count()), (0, 0), "nessuna riga in memoria");
        azzera();
        ponte.ricevi(CasaEvento::Dati(Arc::new(casa_di_prova(false, true))));
        assert_eq!(scritte(), (0, 0), "pagina chiusa: nulla di scritto");
        assert_eq!(p.get_comandi_card().row_count(), 0);

        // Riaperta: i dati di adesso; un filtro sparito (o mai esistito) torna all'iniziale.
        ponte.vista_comandi("casa");
        assert_eq!(p.get_comandi_card().row_data(2).map(|c| c.acceso), Some(true), "la presa accesa intanto");
        ponte.filtro_pagina("casa", "a1");
        assert_eq!(id_filtro(&p), "a1");
        ponte.filtro_pagina("casa", "a9");
        assert_eq!(id_filtro(&p), FILTRO_TUTTA, "un filtro che non c'è: quello iniziale");
        assert_eq!(p.get_comandi_filtro(), p.get_comandi_filtro_iniziale());
    }

    /// Review Focus 1: la stanza scelta sparisce dal menu nuovo (nascosta in Home Assistant) → il
    /// filtro torna a quello iniziale e il fuoco va sui filtri; le card sono quelle del filtro.
    #[test]
    fn comandi_filtro_sparito_torna_all_iniziale() {
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(true, false));
        assert_eq!(id_filtro(&p), FILTRO_PREFERITI, "con un preferito si parte da «Preferiti»");
        ponte.filtro_pagina("casa", "a1");
        assert_eq!((id_filtro(&p), id_card(&p)), ("a1".to_owned(), vec!["light.c".to_owned()]));
        p.set_comandi_lato(1);
        p.set_comandi_fuoco(0);
        let mut senza = casa_di_prova(true, false);
        senza.rooms.remove(1);
        ponte.ricevi(CasaEvento::Dati(Arc::new(senza)));
        assert_eq!(id_filtro(&p), FILTRO_PREFERITI);
        assert_eq!(p.get_comandi_filtro(), p.get_comandi_filtro_iniziale());
        assert_eq!(p.get_comandi_lato(), 0, "il fuoco sui filtri");
        assert_eq!(id_card(&p), ["light.a"], "le card dei preferiti");
        // Una stanza scelta che resta (anche se cambia posto) non sposta nulla.
        ponte.filtro_pagina("casa", "a0");
        p.set_comandi_lato(1);
        let mut altro_ordine = casa_di_prova(true, false);
        altro_ordine.rooms.swap(0, 1);
        ponte.ricevi(CasaEvento::Dati(Arc::new(altro_ordine)));
        assert_eq!((id_filtro(&p).as_str(), p.get_comandi_lato()), ("a0", 1));
    }

    /// La card col fuoco resta la stessa per id quando i dati cambiano, o passa alla più vicina se
    /// la sua sparisce (una luce spenta esce da «Luci accese»); un filtro nuovo parte dalla prima.
    #[test]
    fn il_fuoco_resta_sulla_stessa_card() {
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(false, false));
        ponte.filtro_pagina("casa", FILTRO_LUCI);
        assert_eq!(id_card(&p), ["light.a", "light.b", "light.c"]);
        p.set_comandi_fuoco(1);
        // light.a spenta: esce, il fuoco segue light.b (ora la prima).
        ponte.tocca("light.a");
        assert_eq!(id_card(&p), ["light.b", "light.c"]);
        assert_eq!(p.get_comandi_fuoco(), 0, "sulla stessa card, al suo posto nuovo");
        // Spenta proprio quella col fuoco (l'ultima): il fuoco sulla più vicina.
        p.set_comandi_fuoco(1);
        ponte.tocca("light.c");
        assert_eq!((id_card(&p), p.get_comandi_fuoco()), (vec!["light.b".to_owned()], 0));
        // Un altro filtro: dalla prima.
        p.set_comandi_fuoco(0);
        ponte.filtro_pagina("casa", FILTRO_TUTTA);
        p.set_comandi_fuoco(3);
        ponte.filtro_pagina("casa", "a0");
        assert_eq!(p.get_comandi_fuoco(), 0);
    }

    /// Pre-flight C9: il filtro chiesto prima che la pagina si apra (la carta Casa con le luci
    /// accese) vale per la prima scrittura e basta; aprendo dal dock si riparte dall'iniziale; un
    /// pannello chiuso lo dimentica. Passa dal pannello, come il gesto vero.
    #[test]
    fn il_filtro_chiesto_dalla_carta_vale_una_volta() {
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(true, false));
        ponte.vista_comandi("panoramica");
        p.invoke_filtro_pagina("casa".into(), FILTRO_LUCI.into());
        assert_eq!(p.get_comandi_card().row_count(), 0, "a pagina chiusa nulla si scrive");
        ponte.vista_comandi("casa");
        assert_eq!(id_filtro(&p), FILTRO_LUCI);
        assert_eq!(id_card(&p), ["light.a", "light.b", "light.c"]);
        ponte.vista_comandi("panoramica");
        ponte.vista_comandi("casa");
        assert_eq!(id_filtro(&p), FILTRO_PREFERITI, "di nuovo dal dock: l'iniziale");
        // La richiesta arrivata dopo l'apertura vale subito.
        p.invoke_filtro_pagina("casa".into(), FILTRO_LUCI.into());
        assert_eq!(id_filtro(&p), FILTRO_LUCI);
        // Un pannello chiuso dimentica la richiesta.
        ponte.vista_comandi("panoramica");
        ponte.filtro_pagina("casa", FILTRO_LUCI);
        ponte.pannello_chiuso();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        ponte.vista_comandi("casa");
        assert_eq!(id_filtro(&p), FILTRO_PREFERITI);
    }

    /// Ruling m5 del Task 3: «Spegni la stanza» nasce da `luci_accese_in`, non da un modello
    /// dell'interfaccia. Una luce appena spenta non si conta (il menu dice ancora accesa): il
    /// pulsante sparisce subito, un secondo clic non manda comandi, le luci di un'altra stanza non
    /// si toccano; un comando fallito riporta la luce e il pulsante.
    #[test]
    fn spegni_stanza_conta_con_l_ottimismo() {
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(false, false));
        ponte.filtro_pagina("casa", "a0");
        let sintesi = |p: &Pannello| {
            let s = p.get_comandi_sintesi();
            (s.sotto.to_string(), s.azione_id.to_string())
        };
        assert_eq!(sintesi(&p), ("2 accese su 3".into(), SPEGNI_STANZA.into()));
        ponte.azione_pagina("casa", SPEGNI_STANZA, "");
        let numero = |id: &str| ponte.ottimismo.borrow().numero(id);
        let (na, nb) = (numero("light.a").expect("light.a"), numero("light.b").expect("light.b"));
        assert!(numero("light.c").is_none(), "l'altra stanza non si tocca");
        assert!(numero("switch.p").is_none(), "una presa non è una luce");
        let veri = ponte.menu.borrow().as_ref().map(|m| crate::ponte::luci_accese_in(m, Some("a0"), &Default::default()).len());
        assert_eq!(veri, Some(2), "il menu dice ancora due accese");
        assert_eq!(sintesi(&p), ("3 dispositivi".into(), "".into()), "il pulsante sparisce subito");
        // Il secondo clic nello stesso punto: nessun comando nuovo.
        ponte.azione_pagina("casa", SPEGNI_STANZA, "");
        assert_eq!((numero("light.a"), numero("light.b")), (Some(na), Some(nb)));
        // Un comando fallito riporta la luce e il pulsante.
        ponte.ricevi(CasaEvento::Esito { entity_id: "light.b".into(), numero: nb, ok: false, errore: Some("guasto".into()) });
        assert_eq!(sintesi(&p), ("1 accesa su 3".into(), SPEGNI_STANZA.into()));
        // Fuori da una stanza «Spegni la stanza» non fa nulla.
        ponte.filtro_pagina("casa", FILTRO_TUTTA);
        ponte.azione_pagina("casa", SPEGNI_STANZA, "");
        assert!(numero("light.c").is_none());
        // «Spegni tutte» della testata spegne il resto della casa.
        ponte.azione_pagina("casa", SPEGNI, "");
        assert!(numero("light.c").is_some());
    }

    /// Requisito b del Task 4b: con i dati presenti, a ogni tentativo di riconnessione (casa-core
    /// dice «in connessione») il titolo resta quello dello scollegato: il `r1` vero del registro,
    /// che è anche il titolo della testata, non alterna «Scollegata» e «Mi collego».
    #[test]
    fn la_riconnessione_coi_dati_resta_scollegata() {
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(false, false));
        let r1 = || ponte.registro().r1("casa").map(|r| r.fatto.clone()).unwrap_or_default();
        assert_eq!(r1(), "3 luci accese");
        for (stato, atteso) in [
            (StatoConnessione::Errore("timeout".into()), "Scollegata"),
            (StatoConnessione::InConnessione, "Scollegata"),
            (StatoConnessione::Errore("timeout".into()), "Scollegata"),
            (StatoConnessione::InConnessione, "Scollegata"),
        ] {
            ponte.ricevi(CasaEvento::Stato(stato.clone()));
            assert_eq!(r1(), atteso, "{stato:?}: il r1 del registro");
            assert_eq!(p.get_comandi_sintesi().titolo, atteso, "{stato:?}: il titolo della testata");
            assert_eq!(p.get_comandi_stato().tipo, TipoStatoPaginaUi::Pieno, "{stato:?}: le card restano");
            assert_eq!(p.get_comandi_sintesi().azione_id, RIPROVA);
        }
    }

    /// La pagina non configurata è un invito («Collega la casa» con «Collega»); scollegata con i
    /// dati resta piena; connessa senza dispositivi è vuota (R1-13, spec §5.6).
    #[test]
    fn stati_della_pagina_dal_ponte() {
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(false, false));
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::NonConfigurato));
        let s = p.get_comandi_stato();
        assert_eq!((s.tipo, s.frase.as_str(), s.azione_id.as_str()), (TipoStatoPaginaUi::Vuoto, "Collega la casa", crate::moduli::casa::COLLEGA));
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Disconnesso));
        assert_eq!(p.get_comandi_stato().tipo, TipoStatoPaginaUi::Pieno, "scollegata coi dati: la pagina c'è");
        assert!(p.get_comandi_card().iter().all(|c| c.tenue && !c.toccabile), "card tenui e non toccabili");
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        ponte.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![]))));
        assert_eq!(p.get_comandi_stato().tipo, TipoStatoPaginaUi::Vuoto);
    }
}
