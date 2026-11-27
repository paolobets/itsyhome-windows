//! Le pagine sugli impianti di F3 nel ponte (ridisegno F3, Task 4b, 5 e 7): le pagine Casa e
//! Collegamenti sull'impianto Comandi, sugli stessi modelli (una sola pagina alla volta), e la
//! pagina PC sull'impianto Misure.
//!
//! Leggerezza (spec §3): i modelli della pagina (`comandi_*`, `misure_*`) si riempiono solo a
//! pagina aperta (`pagina_aperta`), si riscrivono solo le righe cambiate (`aggiorna_modello`) e le
//! proprietà solo se cambiano (`scrivi_se_cambia`); lasciata la pagina si svuotano. Ogni dato della
//! casa che cambia passa da `casa_cambiata` (pre-flight C10): la pagina, se aperta, e la carta. Ogni
//! campione del PC passa da `campione_pc`: la carta del PC e, se aperta, la pagina.

use super::{aggiorna_modello, luci_accese_ora, ponte, scrivi_se_cambia, Ponte};
use crate::moduli::pagina_casa::{self, FILTRO_ALTRO};
use crate::moduli::pagina_pc::{self, Quarto, RighePc};
use crate::moduli::{casa, collegamenti, pagina_collegamenti, pc};
use crate::pc_stats::StatPc;
use crate::{CardUi, FiltroUi, Pannello, SintesiUi, StatoPaginaUi, TipoStatoPaginaUi};
use casa_core::diagnosi::Contatore;
use slint::{Model, ModelRc, SharedString};
use std::time::{Duration, Instant};

/// La guardia dei pulsanti della Casa che nascono sotto il clic (revisione finale di F3, I1): 600
/// ms, come l'armamento di «Consenti» dei permessi. Vedi `Ponte::azione_casa_dal`.
pub(crate) const GUARDIA_CASA: Duration = Duration::from_millis(600);

/// Un campione del PC come lo consegna il thread del pannello: i numeri, la CPU pronta (è una
/// differenza fra due letture) e le fonti lente lette (dischi, reti e GPU, solo a pagina PC aperta;
/// la GPU appena accesa dà il valore al campione dopo: fino ad allora non sono «lette»).
pub(crate) struct CampionePc {
    stat: StatPc,
    cpu_pronta: bool,
    lente_lette: bool,
}

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
    /// chiusura con ""): la pagina che si apre si scrive da capo (Casa col filtro chiesto prima
    /// dell'apertura o con quello iniziale; il PC con l'ultimo campione); quella lasciata svuota i
    /// suoi modelli.
    pub fn vista_pagina(&self, vista: &str) {
        let nuova = match vista {
            "casa" => Some("casa"),
            "collegamenti" => Some("collegamenti"),
            "pc" => Some("pc"),
            _ => None,
        };
        if self.pagina_aperta.get() == nuova {
            return;
        }
        self.pagina_aperta.set(nuova);
        if self.misure_valori.row_count() + self.misure_righe.row_count() > 0 {
            self.misure_valori.set_vec(Vec::new());
            self.misure_righe.set_vec(Vec::new());
        }
        self.scrivi_pagina_pc();
        if self.comandi_filtri.row_count() + self.comandi_card.row_count() + self.comandi_scene.row_count() > 0 {
            self.comandi_filtri.set_vec(Vec::new());
            self.comandi_card.set_vec(Vec::new());
            self.comandi_scene.set_vec(Vec::new());
        }
        *self.filtro_casa.borrow_mut() = None;
        self.filtro_mostrato.borrow_mut().clear();
        // La pagina Casa che si apre non ha ancora mostrato nulla: la sua prima testata e il suo
        // primo stato non «nascono sotto il clic» (la guardia guarda solo i cambi a pagina aperta).
        *self.azione_casa_mostrata.borrow_mut() = None;
        self.stato_casa_mostrato.set(None);
        if !matches!(nuova, Some("casa" | "collegamenti")) {
            // I dati di nessuna pagina dei Comandi: l'impianto non mostra nulla finché una sua
            // pagina non riscrive.
            if let Some(p) = self.pannello.borrow().upgrade() {
                scrivi_se_cambia(p.get_comandi_modulo(), SharedString::new(), |v| p.set_comandi_modulo(v));
            }
        }
        self.scrivi_pagina_casa();
        self.scrivi_pagina_collegamenti();
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
        if self.pagina_aperta.get() == Some("casa") {
            *self.filtro_casa.borrow_mut() = Some(filtro.to_owned());
            self.scrivi_pagina_casa();
        } else {
            *self.filtro_richiesto.borrow_mut() = Some(filtro.to_owned());
        }
    }

    /// Una card o una scena toccata (clic, Invio, Narrator): il suo comando; in Collegamenti il
    /// lancio del collegamento. Va chiamata solo subito dopo il gesto dell'utente.
    pub fn tocca_card(&self, modulo: &str, id: &str) {
        self.tocca_card_alle(modulo, id, Instant::now());
    }

    /// `tocca_card` all'istante `ora`. Una card della Casa toccata nei `GUARDIA_CASA` dopo che la
    /// pagina è passata da uno stato senza card (il «Riprova» dello stato al centro, il primo
    /// avvio) a quello pieno si ignora: il doppio clic su «Riprova» non accende una luce nata al suo
    /// posto (revisione finale di F3, I1).
    pub(crate) fn tocca_card_alle(&self, modulo: &str, id: &str, ora: Instant) {
        match modulo {
            "casa" if ora < self.card_casa_dal.get() => {}
            "casa" => self.tocca(id),
            "collegamenti" => self.lancia_collegamento(id),
            _ => eprintln!("tocco di una pagina sconosciuta: {modulo} {id}"),
        }
    }

    /// Vero se «Spegni tutte» e «Spegni la stanza» da un clic valgono all'istante `ora`: sono
    /// passati `GUARDIA_CASA` da quando è nata l'ultima azione della testata o della carta di Casa.
    pub(super) fn azione_casa_armata(&self, ora: Instant) -> bool {
        ora >= self.azione_casa_dal.get()
    }

    /// È nata un'azione nuova nella testata o nella carta di Casa: la guardia riparte.
    pub(super) fn azione_casa_nata(&self) {
        self.azione_casa_dal.set(Instant::now() + GUARDIA_CASA);
    }

    /// Un collegamento da aprire (la card della pagina, «Apri {nome}» della carta): il browser e,
    /// se è partito, il pannello si chiude dal gancio `chiudi_pannello` (fuori da questo callback).
    /// Se non è partito il pannello resta aperto con l'errore nel sotto della sintesi.
    pub(super) fn lancia_collegamento(&self, id: &str) {
        if self.apri_collegamento(id) {
            if let Some(chiudi) = self.chiudi_pannello.borrow().as_ref() {
                chiudi();
            }
        }
    }

    /// I collegamenti o i loro lanci sono cambiati (salvati, tolti, spostati, aperti): la pagina
    /// Collegamenti, se aperta, e la carta.
    pub(super) fn collegamenti_cambiati(&self) {
        self.scrivi_pagina_collegamenti();
        self.moduli_aggiorna("collegamenti");
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

    /// Scrive la pagina Casa, solo se è aperta (`pagina_aperta`) e solo ciò che è cambiato: i
    /// modelli riga per riga, le proprietà se diverse. La card col fuoco resta la stessa per id (o
    /// la più vicina se la sua sparisce); un filtro nuovo riparte dalla prima. Un filtro scelto
    /// che non c'è più (una stanza nascosta in Home Assistant) torna a quello iniziale col fuoco sui
    /// filtri (Review Focus 1). Il filtro chiesto prima dell'apertura vale una volta sola.
    pub fn scrivi_pagina_casa(&self) {
        if self.pagina_aperta.get() != Some("casa") {
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
        // La guardia di I1: un'azione della testata diversa da quella mostrata (e non vuota) nasce
        // dove c'era l'altra («Riprova» → «Spegni tutte» alla connessione); le card nascono dove
        // c'era lo stato (il suo «Riprova» al centro). La prima scrittura dopo l'apertura no.
        let azione = pagina.sintesi.azione_id.clone();
        let azione_prima = self.azione_casa_mostrata.replace(Some(azione.clone()));
        if azione_prima.is_some_and(|a| a != azione) && !azione.is_empty() {
            self.azione_casa_nata();
        }
        let stato_prima = self.stato_casa_mostrato.replace(Some(pagina.stato.tipo));
        if stato_prima.is_some_and(|t| t != TipoStatoPaginaUi::Pieno) && pagina.stato.tipo == TipoStatoPaginaUi::Pieno {
            self.card_casa_dal.set(Instant::now() + GUARDIA_CASA);
        }
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

    /// Scrive la pagina Collegamenti, solo se è aperta (`pagina_aperta`) e solo ciò che è cambiato:
    /// le card riga per riga, le proprietà se diverse. Niente filtri né scene (l'impianto dà
    /// quattro colonne). La card col fuoco resta la stessa per id, o passa alla più vicina se la
    /// sua sparisce (un collegamento tolto dalle Impostazioni, Review Focus 1). Il titolo della
    /// sintesi è `r1.fatto` della carta; il sotto è il più usato o l'errore dell'ultimo lancio.
    pub fn scrivi_pagina_collegamenti(&self) {
        if self.pagina_aperta.get() != Some("collegamenti") {
            return;
        }
        let elenco = self.casa.con_config(|c| c.collegamenti.clone());
        let r1 = collegamenti::riassunto(&elenco).r1;
        let card: Vec<CardUi> = elenco.iter().map(pagina_collegamenti::card).collect();
        let sintesi = pagina_collegamenti::sintesi(&elenco, &r1, self.errore_lancio.borrow().as_deref());
        let stato = pagina_collegamenti::stato(elenco.len());

        let p = self.pannello.borrow().upgrade();
        let fuoco = p.as_ref().map_or(0, |p| p.get_comandi_fuoco().max(0) as usize);
        let nuovo_fuoco = self
            .comandi_card
            .row_data(fuoco)
            .and_then(|c| card.iter().position(|nuova| nuova.id == c.id))
            .unwrap_or_else(|| fuoco.min(card.len().saturating_sub(1)));

        let d = self.casa.diagnosi();
        d.conta(Contatore::ui_ricostruzioni);
        d.conta_n(Contatore::ui_righe, aggiorna_modello(&self.comandi_card, card));
        let Some(p) = p else { return };
        scrivi_se_cambia(p.get_comandi_modulo(), SharedString::from("collegamenti"), |v| p.set_comandi_modulo(v));
        // Nessun filtro: l'indice scelto e quello iniziale sono sempre 0.
        scrivi_se_cambia(p.get_comandi_filtro(), 0, |v| p.set_comandi_filtro(v));
        scrivi_se_cambia(p.get_comandi_filtro_iniziale(), 0, |v| p.set_comandi_filtro_iniziale(v));
        scrivi_se_cambia(p.get_comandi_fuoco(), nuovo_fuoco as i32, |v| p.set_comandi_fuoco(v));
        scrivi_se_cambia(p.get_comandi_sintesi(), sintesi, |v| p.set_comandi_sintesi(v));
        scrivi_se_cambia(p.get_comandi_stato(), stato, |v| p.set_comandi_stato(v));
        scrivi_se_cambia(p.get_comandi_nota(), SharedString::new(), |v| p.set_comandi_nota(v));
    }
}

impl Ponte {
    /// Il pannello appena creato: i modelli dell'impianto Misure (la pagina PC).
    pub(super) fn misure_nel_pannello(&self, p: &Pannello) {
        p.set_misure_valori(ModelRc::from(self.misure_valori.clone()));
        p.set_misure_righe(ModelRc::from(self.misure_righe.clone()));
    }

    /// Un campione del PC dal thread del pannello (uno ogni 2 s, solo a pannello aperto): il ponte
    /// tiene l'ultimo, decide il quarto posto al primo campione con le fonti lente lette (poi resta
    /// fermo fino alla chiusura del pannello: `gpu_pct: None` vuol dire anche «non ancora letta»),
    /// aggiorna la carta del PC come prima e, a pagina aperta, scrive solo ciò che cambia.
    pub fn campione_pc(&self, stat: StatPc, cpu_pronta: bool, lente_lette: bool) {
        if lente_lette && self.pc_quarto.get() == Quarto::DaDecidere {
            self.pc_quarto.set(pagina_pc::decidi_quarto(&stat, true));
        }
        // La carta PC dallo stesso campione (prima che la CPU sia pronta: «—»).
        self.moduli_aggiorna_con("pc", pc::riassunto(cpu_pronta.then_some(&stat)));
        *self.pc_campione.borrow_mut() = Some(CampionePc { stat, cpu_pronta, lente_lette });
        self.scrivi_pagina_pc();
    }

    /// Il pannello si chiude: l'ultimo campione e la scelta del quarto posto si dimenticano
    /// (pre-flight C13). La pagina riaperta aspetta un campione fresco.
    pub(super) fn dimentica_campione_pc(&self) {
        self.pc_campione.borrow_mut().take();
        self.pc_quarto.set(Quarto::DaDecidere);
    }

    /// Il volume del PC come lo leggono le righe e la sintesi: livello 0..1 e muto; `None` senza
    /// uscita audio (e fuori da Windows, dove il volume non c'è).
    fn volume_pc(&self) -> Option<(f32, bool)> {
        #[cfg(windows)]
        {
            self.volume.get().map(|s| (s.livello, s.muto))
        }
        #[cfg(not(windows))]
        None
    }

    /// Le righe della pagina PC con lo stato che il ponte tiene (pre-flight C6): radio (col valore
    /// voluto da un tocco), nome della rete, cuffie, volume, rifiuto. Fuori da Windows solo la rete.
    fn righe_pc(&self, velocita: Option<(u64, u64)>) -> Vec<crate::RigaMisuraUi> {
        #[cfg(windows)]
        {
            use crate::sistema::radio::TipoRadio;
            let nome = self.nome_rete.borrow();
            let cuffie = self.cuffie.borrow();
            pagina_pc::righe(&RighePc {
                windows: true,
                velocita,
                nome_rete: nome.as_deref(),
                wifi: self.radio_mostrata(TipoRadio::WiFi),
                bluetooth: self.radio_mostrata(TipoRadio::Bluetooth),
                cuffie: cuffie.as_ref().map(|c| (c.nome.as_str(), c.percentuale)),
                volume: self.volume_pc(),
                errore_radio: self.errore_radio.get(),
            })
        }
        #[cfg(not(windows))]
        pagina_pc::righe(&RighePc {
            windows: false,
            velocita,
            nome_rete: None,
            wifi: None,
            bluetooth: None,
            cuffie: None,
            volume: None,
            errore_radio: None,
        })
    }

    /// Scrive la pagina PC, solo se è aperta e solo ciò che è cambiato: le misure e le righe riga
    /// per riga, sintesi e stato se diversi. Finché il quarto posto non è deciso (nessun campione
    /// con le fonti lente) la pagina resta «Lettura in corso», senza misure né righe: un posto che
    /// compare e poi sparisce sotto gli occhi non si mostra mai. Il traffico della rete solo da un
    /// campione con le fonti lente.
    pub fn scrivi_pagina_pc(&self) {
        if self.pagina_aperta.get() != Some("pc") {
            return;
        }
        let scelta = self.pc_quarto.get();
        let campione = self.pc_campione.borrow();
        let pronto = campione.as_ref().filter(|c| c.cpu_pronta).map(|c| &c.stat);
        // Titolo e tono della carta del PC: fuori soglia la testata ha gli occhietti (I2).
        let carta = pc::riassunto(pronto);
        let sintesi = pagina_pc::sintesi(carta.tono, &carta.r1, self.volume_pc());
        let (misure, righe, stato) = match campione.as_ref().filter(|_| scelta != Quarto::DaDecidere) {
            Some(c) => (
                pagina_pc::misure(Some(&c.stat), c.cpu_pronta, c.lente_lette, scelta).to_vec(),
                self.righe_pc(c.lente_lette.then_some((c.stat.rete_giu_bps, c.stat.rete_su_bps))),
                pagina_pc::stato(Some(&c.stat), c.cpu_pronta),
            ),
            None => (Vec::new(), Vec::new(), pagina_pc::stato(None, false)),
        };
        drop(campione);

        let righe = aggiorna_modello(&self.misure_valori, misure) + aggiorna_modello(&self.misure_righe, righe);
        self.casa.diagnosi().conta_n(Contatore::ui_righe, righe);
        let Some(p) = self.pannello.borrow().upgrade() else { return };
        scrivi_se_cambia(p.get_misure_sintesi(), sintesi, |v| p.set_misure_sintesi(v));
        scrivi_se_cambia(p.get_misure_stato(), stato, |v| p.set_misure_stato(v));
    }
}

#[cfg(test)]
mod test {
    use crate::moduli::casa::{RIPROVA, SPEGNI};
    use crate::moduli::pagina_casa::{FILTRO_LUCI, FILTRO_PREFERITI, FILTRO_TUTTA, SPEGNI_STANZA};
    use crate::moduli::pagina_collegamenti::MODIFICA;
    use crate::ponte::test::{interruttore, luce, menu, ponte_di_prova, scena, Spegni};
    use crate::ponte::{Ponte, PROPRIETA_SCRITTE, RIGHE_SCRITTE};
    use crate::{Pannello, TipoStatoPaginaUi};
    use casa_core::config::Collegamento;
    use casa_core::eventi::{CasaEvento, StatoConnessione};
    use casa_core::types::{AppEntity, MenuData};
    use slint::Model;
    use std::cell::{Cell, RefCell};
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
        ponte.vista_pagina("casa");
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
        ponte.vista_pagina("panoramica");
        assert_eq!((p.get_comandi_card().row_count(), p.get_comandi_filtri().row_count()), (0, 0), "nessuna riga in memoria");
        azzera();
        ponte.ricevi(CasaEvento::Dati(Arc::new(casa_di_prova(false, true))));
        assert_eq!(scritte(), (0, 0), "pagina chiusa: nulla di scritto");
        assert_eq!(p.get_comandi_card().row_count(), 0);

        // Riaperta: i dati di adesso; un filtro sparito (o mai esistito) torna all'iniziale.
        ponte.vista_pagina("casa");
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
        ponte.vista_pagina("panoramica");
        p.invoke_filtro_pagina("casa".into(), FILTRO_LUCI.into());
        assert_eq!(p.get_comandi_card().row_count(), 0, "a pagina chiusa nulla si scrive");
        ponte.vista_pagina("casa");
        assert_eq!(id_filtro(&p), FILTRO_LUCI);
        assert_eq!(id_card(&p), ["light.a", "light.b", "light.c"]);
        ponte.vista_pagina("panoramica");
        ponte.vista_pagina("casa");
        assert_eq!(id_filtro(&p), FILTRO_PREFERITI, "di nuovo dal dock: l'iniziale");
        // La richiesta arrivata dopo l'apertura vale subito.
        p.invoke_filtro_pagina("casa".into(), FILTRO_LUCI.into());
        assert_eq!(id_filtro(&p), FILTRO_LUCI);
        // Un pannello chiuso dimentica la richiesta.
        ponte.vista_pagina("panoramica");
        ponte.filtro_pagina("casa", FILTRO_LUCI);
        ponte.pannello_chiuso();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        ponte.vista_pagina("casa");
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
        // I clic arrivano passata la guardia dei pulsanti appena nati (revisione finale di F3, I1:
        // `la_guardia_della_casa_ferma_il_secondo_clic`).
        let dopo = |ponte: &Ponte, azione: &str| ponte.azione_pagina_alle("casa", azione, "", ponte.azione_casa_dal.get());
        dopo(&ponte, SPEGNI_STANZA);
        let numero = |id: &str| ponte.ottimismo.borrow().numero(id);
        let (na, nb) = (numero("light.a").expect("light.a"), numero("light.b").expect("light.b"));
        assert!(numero("light.c").is_none(), "l'altra stanza non si tocca");
        assert!(numero("switch.p").is_none(), "una presa non è una luce");
        let veri = ponte.menu.borrow().as_ref().map(|m| crate::ponte::luci_accese_in(m, Some("a0"), &Default::default()).len());
        assert_eq!(veri, Some(2), "il menu dice ancora due accese");
        assert_eq!(sintesi(&p), ("3 dispositivi".into(), "".into()), "il pulsante sparisce subito");
        // Il secondo clic nello stesso punto: nessun comando nuovo.
        dopo(&ponte, SPEGNI_STANZA);
        assert_eq!((numero("light.a"), numero("light.b")), (Some(na), Some(nb)));
        // Un comando fallito riporta la luce e il pulsante.
        ponte.ricevi(CasaEvento::Esito { entity_id: "light.b".into(), numero: nb, ok: false, errore: Some("guasto".into()) });
        assert_eq!(sintesi(&p), ("1 accesa su 3".into(), SPEGNI_STANZA.into()));
        // Fuori da una stanza «Spegni la stanza» non fa nulla.
        ponte.filtro_pagina("casa", FILTRO_TUTTA);
        dopo(&ponte, SPEGNI_STANZA);
        assert!(numero("light.c").is_none());
        // «Spegni tutte» della testata spegne il resto della casa.
        dopo(&ponte, SPEGNI);
        assert!(numero("light.c").is_some());
    }

    /// Revisione finale di F3, I1: «Riprova» resta nella testata durante la riconnessione e alla
    /// connessione lo stesso pulsante diventa «Spegni tutte» (in una stanza «Spegni la stanza»).
    /// Un clic entro 600 ms dal cambio si ignora (nessun tocco, ottimismo vuoto), dalla pagina e
    /// dalla carta; dopo i 600 ms passa. Il tempo è quello simulato (`*_alle`).
    #[test]
    fn la_guardia_della_casa_ferma_il_secondo_clic() {
        use std::time::Duration;
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(false, false));
        let numero = |id: &str| ponte.ottimismo.borrow().numero(id);
        let nessun_tocco = || ["light.a", "light.b", "light.c"].iter().all(|id| numero(id).is_none());
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Errore("timeout".into())));
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::InConnessione));
        assert_eq!(p.get_comandi_sintesi().azione_id, RIPROVA, "«Riprova» durante la riconnessione");
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        assert_eq!(p.get_comandi_sintesi().azione_id, SPEGNI, "lo stesso pulsante è diventato «Spegni tutte»");
        let dal = ponte.azione_casa_dal.get();
        let appena_prima = dal - Duration::from_millis(1);
        // Il secondo clic, entro i 600 ms: dalla testata e dalla carta, nessun tocco.
        ponte.azione_pagina_alle("casa", SPEGNI, "", appena_prima);
        ponte.azione_modulo_alle("casa", SPEGNI, appena_prima);
        assert!(nessun_tocco(), "dentro i 600 ms: ottimismo vuoto");
        assert_eq!(p.get_comandi_sintesi().azione_id, SPEGNI, "nulla è cambiato");
        // Passati i 600 ms: spegne.
        ponte.azione_pagina_alle("casa", SPEGNI, "", dal);
        assert!(numero("light.a").is_some() && numero("light.b").is_some() && numero("light.c").is_some());
    }

    /// Come `la_guardia_della_casa_ferma_il_secondo_clic`, in una stanza: «Riprova» che alla
    /// connessione diventa «Spegni la stanza».
    #[test]
    fn la_guardia_della_casa_in_una_stanza() {
        use std::time::Duration;
        let (_s, ponte, p, _t) = casa_aperta(casa_di_prova(false, false));
        let numero = |id: &str| ponte.ottimismo.borrow().numero(id);
        ponte.filtro_pagina("casa", "a0");
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Errore("timeout".into())));
        assert_eq!(p.get_comandi_sintesi().azione_id, RIPROVA);
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        assert_eq!(p.get_comandi_sintesi().azione_id, SPEGNI_STANZA);
        let dal = ponte.azione_casa_dal.get();
        ponte.azione_pagina_alle("casa", SPEGNI_STANZA, "", dal - Duration::from_millis(1));
        assert!(numero("light.a").is_none() && numero("light.b").is_none(), "dentro i 600 ms: nessun tocco");
        ponte.azione_pagina_alle("casa", SPEGNI_STANZA, "", dal + Duration::from_millis(1));
        assert!(numero("light.a").is_some() && numero("light.b").is_some(), "dopo: la stanza si spegne");
        assert!(numero("light.c").is_none(), "l'altra stanza no");
    }

    /// Revisione finale di F3, I1 (caso affine): a casa scollegata senza dati il «Riprova» dello
    /// stato sta al centro della pagina; alla connessione lì nascono le card. Un tocco su una card
    /// entro 600 ms dal passaggio allo stato pieno si ignora, dopo passa. Una pagina già piena che
    /// cambia dati non arma nulla.
    #[test]
    fn la_guardia_delle_card_nate_al_posto_dello_stato() {
        use std::time::Duration;
        let (_s, ponte, pillola, cartella) = ponte_di_prova();
        let _tieni = (pillola, cartella);
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        // Il ponte di prova parte coi dati: qui la casa non ne ha ancora.
        *ponte.menu.borrow_mut() = None;
        p.set_vista("casa".into());
        ponte.vista_pagina("casa");
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Errore("timeout".into())));
        let s = p.get_comandi_stato();
        assert_eq!((s.tipo, s.azione_id.as_str()), (TipoStatoPaginaUi::FuoriServizio, RIPROVA), "il «Riprova» dello stato");
        assert_eq!(p.get_comandi_sintesi().azione_id, "", "uno solo: la testata non lo ripete");
        let prima = ponte.card_casa_dal.get();
        ponte.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        ponte.ricevi(CasaEvento::Dati(Arc::new(casa_di_prova(false, false))));
        assert_eq!(p.get_comandi_stato().tipo, TipoStatoPaginaUi::Pieno);
        let dal = ponte.card_casa_dal.get();
        assert!(dal > prima, "il passaggio allo stato pieno arma la guardia");
        let numero = |id: &str| ponte.ottimismo.borrow().numero(id);
        ponte.tocca_card_alle("casa", "light.a", dal - Duration::from_millis(1));
        assert!(numero("light.a").is_none(), "dentro i 600 ms: nessun tocco");
        ponte.tocca_card_alle("casa", "light.a", dal);
        assert!(numero("light.a").is_some(), "dopo: il tocco passa");
        // Dati nuovi a pagina piena: nessuna guardia nuova.
        ponte.ricevi(CasaEvento::Dati(Arc::new(casa_di_prova(false, true))));
        assert_eq!(ponte.card_casa_dal.get(), dal);
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

    /// Il ponte di prova con tre collegamenti (NAS, Proxmox, Router; nessun lancio) e il pannello
    /// aperto sulla pagina Collegamenti. Il browser è finto (`su_apri_url`): nessun browser vero.
    fn collegamenti_aperti() -> (Spegni, Rc<Ponte>, Pannello, impl Sized) {
        let (spegni, ponte, pillola, cartella) = ponte_di_prova();
        ponte.casa().aggiorna_config(|c| {
            c.collegamenti = [("nas", "NAS"), ("pve", "Proxmox"), ("router", "Router")]
                .into_iter()
                .map(|(id, nome)| Collegamento { id: id.into(), nome: nome.into(), url: format!("https://{id}.esempio.lan/"), ..Default::default() })
                .collect();
        });
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        p.set_vista("collegamenti".into());
        ponte.vista_pagina("collegamenti");
        (spegni, ponte, p, (pillola, cartella))
    }

    fn sotto(p: &Pannello) -> String {
        p.get_comandi_sintesi().sotto.to_string()
    }

    /// La card di un collegamento, dalla pagina: l'URL va al browser (finto) e il pannello si
    /// chiude solo se è partito, con un lancio in più; se il browser non parte il pannello resta
    /// aperto, il sotto della sintesi dice quale non si è aperto e il conteggio non cambia; il
    /// lancio riuscito toglie la frase. L'errore del ponte per le Impostazioni è un'altra cosa.
    #[test]
    fn lancio_dalla_pagina() {
        let (_s, ponte, p, _t) = collegamenti_aperti();
        let aperti = Rc::new(RefCell::new(Vec::<String>::new()));
        let a = aperti.clone();
        ponte.su_apri_url(|_| Err("ShellExecuteW: 2".into()));
        let chiusure = Rc::new(Cell::new(0));
        let c = chiusure.clone();
        ponte.su_chiudi_pannello(move || c.set(c.get() + 1));
        let lanci = |ponte: &Ponte| ponte.casa().config().collegamenti.iter().map(|c| c.lanci).collect::<Vec<_>>();
        assert_eq!(id_card(&p), ["nas", "pve", "router"]);
        assert_eq!((p.get_comandi_sintesi().titolo.as_str(), sotto(&p).as_str()), ("3 collegamenti", "Più usato: NAS"));

        // Il browser non parte: pannello aperto, l'errore nel sotto, nessun lancio in più.
        ponte.tocca_card("collegamenti", "nas");
        ponte.tocca_card("collegamenti", "nas");
        assert_eq!((chiusure.get(), lanci(&ponte)), (0, vec![0, 0, 0]), "nessuna chiusura, nessun lancio");
        let frase = "Non si è aperto: dettagli in Impostazioni, scheda Informazioni · «NAS»";
        assert_eq!(sotto(&p), frase);
        // L'indicazione di recupero sta in testa (l'ellissi del sotto taglia il nome, non lei) e
        // nessun glifo vietato.
        assert!(frase.starts_with("Non si è aperto: dettagli in Impostazioni") && !frase.contains('›'));
        assert_eq!(p.get_comandi_sintesi().tono, crate::TonoModuloUi::Errore);
        assert!(ponte.errore_collegamento.borrow().as_deref().is_some_and(|e| e.contains("ShellExecuteW: 2")), "il dettaglio per le Impostazioni c'è ancora");
        assert_eq!(p.get_comandi_modulo(), "collegamenti");

        // Il browser parte: l'URL, un lancio in più, il pannello si chiude, la frase sparisce.
        ponte.su_apri_url(move |url| {
            a.borrow_mut().push(url.to_owned());
            Ok(())
        });
        ponte.tocca_card("collegamenti", "pve");
        assert_eq!(*aperti.borrow(), ["https://pve.esempio.lan/"]);
        assert_eq!((chiusure.get(), lanci(&ponte)), (1, vec![0, 1, 0]));
        assert_eq!(sotto(&p), "Più usato: Proxmox", "il più usato è cambiato e la frase non c'è più");
        assert!(ponte.errore_lancio.borrow().is_none());
        // Un doppio tocco prima della chiusura non lancia due volte.
        ponte.tocca_card("collegamenti", "pve");
        assert_eq!((aperti.borrow().len(), chiusure.get(), lanci(&ponte)), (1, 1, vec![0, 1, 0]));
        // Un collegamento che non c'è: niente.
        ponte.pannello_chiuso();
        ponte.tocca_card("collegamenti", "sparito");
        assert_eq!((aperti.borrow().len(), chiusure.get()), (1, 1));
    }

    /// C24: la frase dell'ultimo lancio non riuscito muore col pannello, come il testo di prima: il
    /// pannello nuovo non la rivede.
    #[test]
    fn errore_del_lancio_muore_col_pannello() {
        let (_s, ponte, p, _t) = collegamenti_aperti();
        ponte.su_apri_url(|_| Err("ShellExecuteW: 2".into()));
        ponte.tocca_card("collegamenti", "router");
        assert!(sotto(&p).starts_with("Non si è aperto") && sotto(&p).ends_with("«Router»"), "{}", sotto(&p));
        // Lasciare la pagina e tornarci a pannello aperto non la toglie (è ancora vero).
        ponte.vista_pagina("panoramica");
        ponte.vista_pagina("collegamenti");
        assert!(sotto(&p).starts_with("Non si è aperto") && sotto(&p).ends_with("«Router»"), "{}", sotto(&p));
        ponte.pannello_chiuso();
        assert!(ponte.errore_lancio.borrow().is_none());
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        p.set_vista("collegamenti".into());
        ponte.vista_pagina("collegamenti");
        assert_eq!(sotto(&p), "Più usato: NAS");
    }

    /// Review Focus 5: due scritture di fila senza cambi non riscrivono nessuna riga e nessuna
    /// proprietà; a pagina chiusa un cambio dei collegamenti non scrive nulla (e i modelli sono
    /// vuoti); un cambio vero sì.
    #[test]
    fn pagina_collegamenti_scritta_solo_se_cambia() {
        let (_s, ponte, p, _t) = collegamenti_aperti();
        assert_eq!(p.get_comandi_modulo(), "collegamenti");
        assert_eq!((p.get_comandi_filtri().row_count(), p.get_comandi_scene().row_count()), (0, 0), "nessun filtro né scena");
        assert_eq!(p.get_comandi_stato().tipo, TipoStatoPaginaUi::Pieno);
        assert_eq!(p.get_comandi_sintesi().azione_id, MODIFICA);
        azzera();
        ponte.scrivi_pagina_collegamenti();
        ponte.scrivi_pagina_collegamenti();
        assert_eq!(scritte(), (0, 0), "nulla di cambiato, nulla di scritto");
        // Un cambio vero: un collegamento spostato.
        ponte.sposta_collegamento("router", -1);
        assert_eq!(id_card(&p), ["nas", "router", "pve"]);
        let (righe, proprieta) = scritte();
        assert!(righe > 0, "le righe che cambiano si scrivono: {righe} righe, {proprieta} proprietà");
        // A pagina chiusa: modelli vuoti e nulla scritto.
        ponte.vista_pagina("panoramica");
        assert_eq!(p.get_comandi_card().row_count(), 0, "nessuna riga in memoria");
        azzera();
        ponte.elimina_collegamento("nas");
        assert_eq!(scritte().1, 0, "pagina chiusa: nessuna proprietà scritta");
        assert_eq!(p.get_comandi_card().row_count(), 0);
        // Riaperta: i dati di adesso.
        ponte.vista_pagina("collegamenti");
        assert_eq!(id_card(&p), ["router", "pve"]);
    }

    /// Review Focus 1: un collegamento tolto dalle Impostazioni a pagina aperta. Il fuoco resta
    /// sulla stessa card (per id) se c'è ancora, passa alla più vicina se è quella tolta, e senza
    /// collegamenti la pagina diventa lo stato vuoto con «Aggiungi un collegamento».
    #[test]
    fn collegamento_tolto_con_la_pagina_aperta() {
        let (_s, ponte, p, _t) = collegamenti_aperti();
        p.set_comandi_fuoco(2);
        // Tolto uno prima di quello col fuoco: il fuoco segue il suo (Router passa dal posto 2 al 1).
        ponte.elimina_collegamento("nas");
        assert_eq!((id_card(&p), p.get_comandi_fuoco()), (vec!["pve".to_owned(), "router".to_owned()], 1));
        // Tolto proprio quello col fuoco, l'ultimo: il fuoco sul nuovo ultimo.
        ponte.elimina_collegamento("router");
        assert_eq!((id_card(&p), p.get_comandi_fuoco()), (vec!["pve".to_owned()], 0));
        assert_eq!((p.get_comandi_sintesi().titolo.as_str(), sotto(&p).as_str()), ("1 collegamento", "Più usato: Proxmox"));
        // Nessuno: stato vuoto con il suo invito, nessuna «Modifica».
        ponte.elimina_collegamento("pve");
        let s = p.get_comandi_stato();
        assert_eq!((s.tipo, s.azione_id.as_str()), (TipoStatoPaginaUi::Vuoto, crate::moduli::collegamenti::AGGIUNGI));
        assert_eq!((p.get_comandi_sintesi().titolo.as_str(), p.get_comandi_sintesi().azione_id.as_str()), ("Nessun collegamento", ""));
        assert_eq!(p.get_comandi_card().row_count(), 0);
        // Un collegamento salvato torna a riempire la pagina.
        assert_eq!(ponte.salva_collegamento("", "NAS", "nas.esempio.lan"), "");
        assert_eq!(p.get_comandi_stato().tipo, TipoStatoPaginaUi::Pieno);
        assert_eq!(p.get_comandi_card().row_count(), 1);
    }

    /// C8 dal ponte: Casa e Collegamenti scrivono sugli stessi modelli. Passando dall'una all'altra
    /// i modelli sono quelli della sola pagina aperta (filtri e scene di Casa non restano in
    /// Collegamenti, i collegamenti non restano in Casa) e `comandi-modulo` dice di chi sono.
    #[test]
    fn comandi_pagine_dal_ponte_non_si_mescolano() {
        let (_s, ponte, p, _t) = collegamenti_aperti();
        ponte.ricevi(CasaEvento::Dati(Arc::new(casa_di_prova(false, false))));
        assert_eq!((p.get_comandi_modulo().as_str(), id_card(&p)), ("collegamenti", vec!["nas".to_owned(), "pve".to_owned(), "router".to_owned()]));
        assert_eq!((p.get_comandi_filtri().row_count(), p.get_comandi_scene().row_count()), (0, 0));
        // Casa: le sue card, i suoi filtri, le sue scene; nessun collegamento.
        p.set_vista("casa".into());
        ponte.vista_pagina("casa");
        assert_eq!(p.get_comandi_modulo(), "casa");
        assert_eq!(id_card(&p), ["light.a", "light.b", "switch.p", "light.c"]);
        assert!(p.get_comandi_filtri().row_count() > 0 && p.get_comandi_scene().row_count() == 1);
        // Un collegamento salvato con la pagina Casa aperta non scrive nulla sui suoi modelli.
        assert_eq!(ponte.salva_collegamento("", "Switch", "switch.esempio.lan"), "");
        assert_eq!(id_card(&p), ["light.a", "light.b", "switch.p", "light.c"]);
        // Di nuovo Collegamenti: i suoi dati, senza filtri né scene.
        p.set_vista("collegamenti".into());
        ponte.vista_pagina("collegamenti");
        assert_eq!(p.get_comandi_modulo(), "collegamenti");
        assert_eq!(id_card(&p), ["nas", "pve", "router", "switch"]);
        assert_eq!((p.get_comandi_filtri().row_count(), p.get_comandi_scene().row_count()), (0, 0));
        assert_eq!(p.get_comandi_nota(), "");
        // Un evento della casa a pagina Collegamenti aperta non tocca i suoi modelli.
        ponte.ricevi(CasaEvento::Dati(Arc::new(casa_di_prova(false, true))));
        assert_eq!(id_card(&p), ["nas", "pve", "router", "switch"]);
        assert_eq!((p.get_comandi_filtri().row_count(), p.get_comandi_scene().row_count()), (0, 0));
    }

    // ─── La pagina PC (ridisegno F3, Task 7) ───

    const GB: u64 = 1 << 30;

    /// Un campione del PC di prova: CPU, RAM a metà, disco, senza batteria; la GPU se data.
    fn campione(cpu: f32, gpu: Option<f32>) -> crate::pc_stats::StatPc {
        crate::pc_stats::StatPc {
            cpu_pct: Some(cpu),
            ram_usata: 8 * GB,
            ram_totale: 16 * GB,
            disco_usato: 294 * GB,
            disco_totale: 476 * GB,
            rete_giu_bps: 1_258_291,
            rete_su_bps: 314_573,
            batteria: None,
            gpu_pct: gpu,
        }
    }

    /// Il ponte di prova col pannello aperto sulla pagina PC (la vista e la pagina), senza campioni.
    fn pc_aperta() -> (Spegni, Rc<Ponte>, Pannello, impl Sized) {
        let (spegni, ponte, pillola, cartella) = ponte_di_prova();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        p.set_vista("pc".into());
        ponte.vista_pagina("pc");
        (spegni, ponte, p, (pillola, cartella))
    }

    fn id_misure(p: &Pannello) -> Vec<String> {
        p.get_misure_valori().iter().map(|m| if m.presente { m.id.to_string() } else { String::new() }).collect()
    }

    fn valore(p: &Pannello, id: &str) -> String {
        p.get_misure_valori().iter().find(|m| m.id == id).map(|m| m.valore.to_string()).unwrap_or_default()
    }

    /// Review Focus 5 (pre-flight C11, C13; requisito a del controllore): due campioni uguali non
    /// riscrivono nessuna riga né proprietà, un cambio vero sì. Finché il quarto posto non è deciso
    /// (nessun campione con le fonti lente: anche il primo dopo l'accensione della GPU) la pagina
    /// resta «Lettura in corso» senza misure: il quarto posto non compare per poi sparire. A pagina
    /// chiusa un campione aggiorna solo la carta; riaprendo l'ultimo campione è già scritto; un
    /// pannello nuovo non riusa il campione né la scelta del quarto posto.
    #[test]
    fn pagina_pc_scritta_solo_se_cambia() {
        use crate::TipoStatoPaginaUi::{Pieno, PrimoAvvio};
        let (_s, ponte, p, _t) = pc_aperta();
        assert_eq!((p.get_misure_stato().tipo, p.get_misure_valori().row_count()), (PrimoAvvio, 0), "nessun campione");
        assert_eq!(p.get_misure_stato().frase, "Lettura in corso");
        // Fonti lente non ancora lette (la GPU appena accesa): la pagina aspetta, la carta no.
        ponte.campione_pc(campione(12.0, None), true, false);
        assert_eq!((p.get_misure_stato().tipo, p.get_misure_valori().row_count(), p.get_misure_righe().row_count()), (PrimoAvvio, 0, 0));
        assert_eq!(ponte.registro().r1("pc").map(|r| r.fatto.clone()).as_deref(), Some("Tutto regolare"), "la carta segue il campione");
        // Il primo campione con le fonti lente: la GPU c'è, il quarto posto è suo.
        ponte.campione_pc(campione(12.0, Some(37.0)), true, true);
        assert_eq!(p.get_misure_stato().tipo, Pieno);
        assert_eq!(id_misure(&p), ["cpu", "memoria", "disco", "gpu"]);
        assert!(p.get_misure_righe().row_count() > 0);
        assert_eq!(p.get_misure_sintesi().titolo, "Tutto regolare");

        azzera();
        ponte.campione_pc(campione(12.0, Some(37.0)), true, true);
        ponte.campione_pc(campione(12.0, Some(37.0)), true, true);
        assert_eq!(scritte(), (0, 0), "nulla di cambiato, nulla di scritto");
        // Un cambio vero: la CPU sale oltre la soglia (misura e titolo della sintesi).
        ponte.campione_pc(campione(91.0, Some(37.0)), true, true);
        let (righe, proprieta) = scritte();
        assert!(righe > 0 && proprieta > 0, "un cambio si scrive: {righe} righe, {proprieta} proprietà");
        assert_eq!((valore(&p, "cpu").as_str(), p.get_misure_sintesi().titolo.as_str()), ("91", "CPU 91 %"));
        // Il quarto posto resta della GPU anche quando il campione non la porta.
        ponte.campione_pc(campione(91.0, None), true, true);
        assert_eq!((id_misure(&p)[3].as_str(), valore(&p, "gpu").as_str()), ("gpu", "—"));

        // A pagina chiusa: modelli vuoti, un campione aggiorna solo la carta.
        ponte.vista_pagina("panoramica");
        assert_eq!((p.get_misure_valori().row_count(), p.get_misure_righe().row_count()), (0, 0), "nessuna riga in memoria");
        // Un campione che lascia la carta com'era: nulla di scritto (nemmeno le carte).
        azzera();
        ponte.campione_pc(campione(91.0, Some(20.0)), true, false);
        assert_eq!(scritte(), (0, 0), "pagina chiusa: nulla di scritto");
        // Uno che cambia la carta: la carta sì, la pagina no.
        ponte.campione_pc(campione(40.0, Some(20.0)), true, false);
        assert_eq!(ponte.registro().r1("pc").map(|r| r.fatto.clone()).as_deref(), Some("Tutto regolare"), "la carta sì");
        assert_eq!((p.get_misure_valori().row_count(), p.get_misure_righe().row_count(), scritte().1), (0, 0, 0), "la pagina no");
        // Riaperta: l'ultimo campione è già scritto (senza fonti lente: il disco «Lettura in corso»).
        ponte.vista_pagina("pc");
        assert_eq!((p.get_misure_stato().tipo, valore(&p, "cpu").as_str(), valore(&p, "gpu").as_str()), (Pieno, "40", "20"));
        assert_eq!(valore(&p, "disco"), "—");

        // Un pannello nuovo: niente campione vecchio mostrato come attuale, quarto posto da decidere.
        ponte.pannello_chiuso();
        let p = Pannello::new().expect("pannello");
        ponte.riempi(&p);
        ponte.pannello_aperto(&p);
        p.set_vista("pc".into());
        ponte.vista_pagina("pc");
        assert_eq!((p.get_misure_stato().tipo, p.get_misure_valori().row_count()), (PrimoAvvio, 0), "C13: un campione fresco");
        ponte.campione_pc(campione(12.0, None), true, true);
        assert_eq!(id_misure(&p), ["cpu", "memoria", "disco", ""], "né batteria né GPU: il posto vuoto");
    }

    /// Da `esito_a_vista_pc_lasciata_ignorato` (pre-flight C5): un rifiuto che arriva quando le
    /// radio non sono più seguite (pagina PC lasciata, pannello chiuso) si ignora: nessuna pagina di
    /// Windows, nessun rifiuto da mostrare al rientro. La scelta della pagina è pura e si prova su
    /// ogni piattaforma; su Windows anche il ponte.
    #[test]
    fn esito_a_pagina_pc_lasciata_ignorato() {
        use crate::connessioni::esito;
        use crate::sistema::browser::PaginaWindows;
        use crate::sistema::radio::{EsitoRadio, TipoRadio};
        for e in [EsitoRadio::Negato, EsitoRadio::Errore, EsitoRadio::Fatto] {
            assert_eq!(esito(false, TipoRadio::WiFi, e), None, "{e:?}: radio non seguite");
        }
        assert_eq!(esito(true, TipoRadio::WiFi, EsitoRadio::Negato), Some(PaginaWindows::ImpostazioniWifi));
        assert_eq!(esito(true, TipoRadio::Bluetooth, EsitoRadio::Errore), Some(PaginaWindows::Bluetooth));
        assert_eq!(esito(true, TipoRadio::Bluetooth, EsitoRadio::Fatto), None);
        #[cfg(windows)]
        {
            use crate::sistema::radio::StatoRadio;
            let (_s, ponte, p, _t) = pc_aperta();
            ponte.campione_pc(campione(12.0, None), true, true);
            *ponte.stati_radio.borrow_mut() = Some(vec![StatoRadio { tipo: TipoRadio::WiFi, acceso: true }]);
            ponte.dimentica_radio();
            for e in [EsitoRadio::Negato, EsitoRadio::Errore] {
                assert_eq!(ponte.esito_radio_senza_aprire(TipoRadio::WiFi, e), None, "{e:?}: nessuna pagina");
            }
            assert_eq!(ponte.errore_radio.get(), None, "nessun rifiuto da mostrare al rientro");
            let righe: Vec<_> = p.get_misure_righe().iter().collect();
            assert!(righe.iter().all(|r| r.errore.is_empty()));
            assert_eq!(righe[0].valore, "Lettura in corso", "il Wi-Fi non passa a «Non presente»");
        }
    }

    /// Da `rifiuto_mostra_errore_e_stato_reale` (Review Focus 4 di M2, pre-flight C2 e C6; solo
    /// Windows): il tocco sposta subito la riga sul valore voluto; Windows rifiuta → la riga torna
    /// allo stato vero (l'ultimo letto), il rifiuto sta solo sulla riga della radio che ha fallito e
    /// la pagina di Windows giusta è da aprire (qui non si apre: `esito_radio_senza_aprire`); un
    /// tocco nuovo toglie il rifiuto. Nessuna radio vera si tocca.
    #[cfg(windows)]
    #[test]
    fn rifiuto_della_radio_torna_allo_stato_vero() {
        use crate::moduli::pagina_pc::{BLUETOOTH, WIFI};
        use crate::sistema::browser::PaginaWindows;
        use crate::sistema::radio::{EsitoRadio, StatoRadio, TipoRadio};
        let (_s, ponte, p, _t) = pc_aperta();
        ponte.campione_pc(campione(12.0, None), true, true);
        *ponte.stati_radio.borrow_mut() = Some(vec![
            StatoRadio { tipo: TipoRadio::WiFi, acceso: true },
            StatoRadio { tipo: TipoRadio::Bluetooth, acceso: false },
        ]);
        ponte.scrivi_pagina_pc();
        let riga = |id: &str| p.get_misure_righe().iter().find(|r| r.id == id).expect("riga");
        assert!(riga(WIFI).acceso && !riga(BLUETOOTH).acceso);
        let rifiuto = "Non riuscito: apro le Impostazioni di Windows";

        // Il tocco: subito spento.
        ponte.radio_voluta(TipoRadio::WiFi, false);
        assert!(!riga(WIFI).acceso, "la riga si sposta subito");
        assert!(riga(WIFI).errore.is_empty());
        // Windows rifiuta: di nuovo acceso, il rifiuto solo sul Wi-Fi, la pagina del Wi-Fi.
        assert_eq!(ponte.esito_radio_senza_aprire(TipoRadio::WiFi, EsitoRadio::Negato), Some(PaginaWindows::ImpostazioniWifi));
        assert!(riga(WIFI).acceso, "di nuovo lo stato vero");
        assert_eq!((riga(WIFI).errore.as_str(), riga(BLUETOOTH).errore.as_str()), (rifiuto, ""));
        assert_eq!(p.get_misure_righe().iter().filter(|r| r.narratore.contains(rifiuto)).count(), 1, "Narrator lo legge una volta");
        // Un errore del Bluetooth: la sua pagina, il rifiuto passa a lui.
        assert_eq!(ponte.esito_radio_senza_aprire(TipoRadio::Bluetooth, EsitoRadio::Errore), Some(PaginaWindows::Bluetooth));
        assert_eq!((riga(WIFI).errore.as_str(), riga(BLUETOOTH).errore.as_str()), ("", rifiuto));
        // Un tocco nuovo toglie il rifiuto; un esito riuscito non fa nulla (lo stato vero arriva da sé).
        ponte.radio_voluta(TipoRadio::Bluetooth, true);
        assert!(riga(BLUETOOTH).acceso && riga(BLUETOOTH).errore.is_empty());
        assert_eq!(ponte.esito_radio_senza_aprire(TipoRadio::Bluetooth, EsitoRadio::Fatto), None);
        assert!(p.get_misure_righe().iter().all(|r| r.errore.is_empty()));
        // Lo stato vero che arriva prende il posto del valore voluto (qui: il Bluetooth resta spento).
        *ponte.stati_radio.borrow_mut() = Some(vec![
            StatoRadio { tipo: TipoRadio::WiFi, acceso: true },
            StatoRadio { tipo: TipoRadio::Bluetooth, acceso: false },
        ]);
        ponte.radio_volute.set([None; 2]);
        ponte.scrivi_pagina_pc();
        assert!(!riga(BLUETOOTH).acceso);
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
