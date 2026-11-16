//! Le sorgenti dei lati della pillola (M2, Task 4): accende e spegne i soli timer che servono e
//! riscrive un lato solo quando il suo contenuto cambia.
//!
//! - CPU e RAM: un timer Slint da 5 s (`PERIODO_LATI`) col `CampionatoreLeggero`, sul thread
//!   dell'interfaccia (le due letture costano microsecondi), solo se CPU o RAM sono scelte e la
//!   pillola si vede e non dorme.
//! - L'ora: una sveglia al minuto (rifasata a ogni scatto), alle stesse condizioni.
//! - La batteria resta sul thread da 60 s di M1 (`pc_stats::sorveglia_batteria`); meteo e sensori
//!   arrivano dal ponte a ogni `MenuData`: nessuna lettura in più.
//! - La rete (Task 8, solo Windows): la segue `OsservaRete`, che accende e spegne il ponte
//!   (`radio::osserva_rete_acceso`); qui arriva solo lo stato (`rete`), mai il nome della rete.
//!
//! «Si vede» = `Visibilita::Pillola` (non ritirata, non nascosta, pannello chiuso) e non
//! allargata (lì i lati sono nascosti). Mentre la pillola non si vede o dorme nessun timer gira e
//! nulla si ridisegna: i dati arrivati intanto si mostrano al ritorno. Con i lati «niente / niente»
//! non c'è nulla di acceso: il consumo a riposo resta quello di M1.
//!
//! «Dorme» con la pillola visibile (giro di correzioni 1): entrando, un solo ridisegno svuota i
//! lati a tempo (ora, CPU, RAM: fermi sarebbero sbagliati) e lascia batteria, meteo e sensori; poi
//! più nulla fino al risveglio, che li riporta col valore di adesso. Il posto dei lati resta (il
//! campione): la pillola non cambia larghezza.

use crate::lati::{self, Bisogni, ContenutoLato, DatiLati, Lato};
use crate::pc_stats::{CampionatoreLeggero, LetturaBatteria, PERIODO_LATI};
use crate::sistema::radio::StatoRete;
use crate::{orologio, pannello, pillola, ContenutoLatoUi, Pillola, TipoReteUi};
use casa_core::types::{MenuData, SensoreNumerico};
use chrono::Timelike;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

/// Il primo valore della CPU è una differenza fra due letture: arriva poco dopo l'accensione,
/// non dopo 5 s di «CPU —».
const PRIMO_CAMPIONE: Duration = Duration::from_millis(500);

struct Sorgenti {
    pillola: slint::Weak<Pillola>,
    sinistro: Lato,
    destro: Lato,
    dati: DatiLati,
    /// Pillola visibile e sveglia: solo allora i timer girano e i lati si riscrivono.
    attivi: bool,
    /// Pillola visibile e addormentata: i lati a tempo sono vuoti e nulla si riscrive.
    addormentati: bool,
    /// Ultimi lati scritti nella pillola (`None`: mai scritti).
    mostrati: Option<(ContenutoLatoUi, ContenutoLatoUi)>,
    campionatore: Option<CampionatoreLeggero>,
    cpu_ram: slint::Timer,
    orologio: slint::Timer,
}

thread_local! {
    static SORGENTI: RefCell<Option<Sorgenti>> = const { RefCell::new(None) };
}

/// Collega la pillola con i lati scelti (all'avvio, dal ponte). Nessun timer finché `aggiorna`
/// non dice che servono.
pub fn collega(pillola: &Pillola, sinistro: Lato, destro: Lato) {
    let s = Sorgenti {
        pillola: slint::ComponentHandle::as_weak(pillola),
        sinistro,
        destro,
        dati: DatiLati::default(),
        attivi: false,
        addormentati: false,
        mostrati: None,
        campionatore: None,
        cpu_ram: slint::Timer::default(),
        orologio: slint::Timer::default(),
    };
    SORGENTI.with(|x| *x.borrow_mut() = Some(s));
}

/// A fine programma (o di un test): via timer e campionatore.
pub fn spegni() {
    SORGENTI.with(|x| x.borrow_mut().take());
}

fn con<R>(f: impl FnOnce(&mut Sorgenti) -> R) -> Option<R> {
    SORGENTI.with(|x| x.borrow_mut().as_mut().map(f))
}

/// I lati scelti nelle Impostazioni: i dati che non servono più si dimenticano e i lati si
/// riscrivono. Chi la chiama poi chiama `aggiorna` (i timer seguono i bisogni nuovi).
pub fn scegli(sinistro: Lato, destro: Lato) {
    con(|s| {
        s.sinistro = sinistro;
        s.destro = destro;
        let b = lati::bisogni(&s.sinistro, &s.destro);
        if !b.cpu_ram {
            s.campionatore = None;
            s.dati.cpu = None;
            s.dati.ram = None;
        }
        if !b.ora {
            s.dati.ora.clear();
        }
        if !b.rete {
            s.dati.rete = None;
        }
        // Una scelta nuova si vede anche da addormentata (cambia l'ingombro).
        scrivi(s);
    });
}

/// I bisogni dei lati scelti adesso (nessuno se la pillola non è collegata).
pub fn bisogni_scelti() -> Bisogni {
    con(|s| lati::bisogni(&s.sinistro, &s.destro)).unwrap_or_default()
}

/// Il PC ha una batteria (la prima lettura del thread della batteria è arrivata): solo allora
/// «Batteria» è fra le scelte.
pub fn ha_batteria() -> bool {
    con(|s| s.dati.batteria.is_some()).unwrap_or(false)
}

/// Accende o spegne le sorgenti: il campionatore da 5 s solo se `cpu_ram && visibile && !dorme`,
/// l'orologio al minuto solo se `ora && visibile && !dorme`. Si può chiamare spesso: cambia
/// qualcosa solo se cambia lo stato.
pub fn aggiorna(bisogni: Bisogni, visibile: bool, dorme: bool) {
    let attivi = visibile && !dorme;
    let addormentati = visibile && dorme;
    let primo = con(|s| {
        // Da riscrivere: tornata visibile e sveglia, appena addormentata (un solo ridisegno che
        // svuota i lati a tempo), o un dato letto qui sotto.
        let mut cambiato = (attivi && !s.attivi) || (addormentati && !s.addormentati);
        s.attivi = attivi;
        s.addormentati = addormentati;
        let mut primo = false;
        if bisogni.cpu_ram && attivi {
            if !s.cpu_ram.running() {
                match s.campionatore.as_mut() {
                    // Di nuovo visibile: un campione subito (la differenza copre la pausa).
                    Some(c) => {
                        let (cpu, ram) = c.campiona();
                        s.dati.cpu = cpu;
                        s.dati.ram = Some(ram);
                        cambiato = true;
                    }
                    None => {
                        s.campionatore = Some(CampionatoreLeggero::new());
                        primo = true;
                    }
                }
                s.cpu_ram.start(slint::TimerMode::Repeated, PERIODO_LATI, campiona);
            }
        } else {
            s.cpu_ram.stop();
            if !bisogni.cpu_ram {
                s.campionatore = None;
            }
        }
        if bisogni.ora && attivi {
            if !s.orologio.running() {
                s.dati.ora = orologio::adesso().0;
                programma_orologio(s);
                cambiato = true;
            }
        } else {
            s.orologio.stop();
        }
        if cambiato || s.mostrati.is_none() {
            scrivi(s);
        }
        primo
    })
    .unwrap_or(false);
    if primo {
        slint::Timer::single_shot(PRIMO_CAMPIONE, campiona);
    }
}

/// Un campione di CPU e RAM (timer da 5 s): si riscrive solo il lato che cambia.
fn campiona() {
    con(|s| {
        if !s.attivi {
            return;
        }
        let Some(c) = s.campionatore.as_mut() else { return };
        let (cpu, ram) = c.campiona();
        s.dati.cpu = cpu;
        s.dati.ram = Some(ram);
        ridisegna(s);
    });
}

/// L'orologio si sveglia al prossimo minuto e si rifasa a ogni scatto.
fn programma_orologio(s: &Sorgenti) {
    let adesso = chrono::Local::now();
    let tra = orologio::al_prossimo_minuto(adesso.second(), adesso.nanosecond());
    s.orologio.start(slint::TimerMode::SingleShot, tra, || {
        con(|s| {
            if !s.attivi {
                return;
            }
            s.dati.ora = orologio::adesso().0;
            ridisegna(s);
            programma_orologio(s);
        });
    });
}

/// Una lettura della batteria (thread da 60 s di M1).
pub fn batteria(lettura: LetturaBatteria) {
    con(|s| {
        if s.dati.batteria != Some(lettura) {
            s.dati.batteria = Some(lettura);
            ridisegna(s);
        }
    });
}

/// Come è collegato il PC (da `OsservaRete`, Task 8): si riscrive solo il lato che cambia.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn rete(stato: StatoRete) {
    con(|s| {
        if s.dati.rete != Some(stato) {
            s.dati.rete = Some(stato);
            ridisegna(s);
        }
    });
}

/// I dati della casa (a ogni `MenuData`): la temperatura esterna e i valori dei soli sensori
/// scelti per i lati.
pub fn casa(menu: &MenuData) {
    con(|s| {
        let (meteo, sensori) = dati_casa(menu, &s.sinistro, &s.destro);
        if s.dati.meteo != meteo || s.dati.sensori != sensori {
            s.dati.meteo = meteo;
            s.dati.sensori = sensori;
            ridisegna(s);
        }
    });
}

/// Meteo (se scelto) e sensori scelti (valore e unità), dai dati della casa
/// (`MenuData::sensori_numerici`).
fn dati_casa(menu: &MenuData, sinistro: &Lato, destro: &Lato) -> (Option<String>, HashMap<String, SensoreNumerico>) {
    let scelti = [sinistro, destro];
    let meteo = if scelti.contains(&&Lato::Meteo) {
        menu.meteo.as_ref().and_then(orologio::temperatura_breve)
    } else {
        None
    };
    let mut sensori = HashMap::new();
    for l in scelti {
        let Lato::Sensore(id) = l else { continue };
        if let Some(n) = menu.sensori_numerici.iter().find(|n| n.entity_id == *id) {
            sensori.insert(id.clone(), n.clone());
        }
    }
    (meteo, sensori)
}

/// Il lato `l` come lo conosce l'interfaccia: il contenuto (vuoto se è a tempo e la pillola dorme)
/// e il campione su cui si misura, che dipende solo dalla scelta.
pub fn lato_ui(l: &Lato, d: &DatiLati, dorme: bool) -> ContenutoLatoUi {
    let c = if dorme && lati::a_tempo(l) { ContenutoLato::Vuoto } else { lati::contenuto(l, d) };
    let mut ui = match c {
        ContenutoLato::Vuoto => ContenutoLatoUi::default(),
        ContenutoLato::Testo(t) => ContenutoLatoUi { tipo: 1, testo: t.as_str().into(), ..Default::default() },
        ContenutoLato::Batteria { testo, frazione, tinta } => ContenutoLatoUi {
            tipo: 2,
            testo: testo.as_str().into(),
            frazione,
            tinta: pannello::tinta_ui(tinta),
            ..Default::default()
        },
        ContenutoLato::Rete(r) => {
            let (rete, tacche) = rete_ui(r);
            ContenutoLatoUi { tipo: 3, rete, tacche, ..Default::default() }
        }
    };
    ui.campione = lati::campione(l, d).unwrap_or_default().into();
    ui.con_glifo = *l == Lato::Batteria;
    // Il posto dell'icona della rete resta anche prima della prima lettura.
    ui.con_rete = *l == Lato::Rete;
    ui
}

/// La rete come la conosce l'interfaccia (pillola e vista PC): il tipo e le tacche (0..=3).
pub fn rete_ui(r: StatoRete) -> (TipoReteUi, i32) {
    match r {
        StatoRete::Nessuna => (TipoReteUi::Nessuna, 0),
        StatoRete::Cavo => (TipoReteUi::Cavo, 0),
        StatoRete::WiFi { tacche } => (TipoReteUi::Wifi, i32::from(tacche.min(3))),
    }
}

/// Riscrive i lati che cambiano solo da attivi (un dato nuovo): da nascosta o addormentata nulla
/// si ridisegna.
fn ridisegna(s: &mut Sorgenti) {
    if s.attivi {
        scrivi(s);
    }
}

/// Scrive nella pillola i lati che cambiano (da attivi, o da addormentati coi lati a tempo vuoti;
/// da nascosta nulla) e, se la larghezza cambia, il ritaglio.
fn scrivi(s: &mut Sorgenti) {
    if !s.attivi && !s.addormentati {
        return;
    }
    let Some(ui) = s.pillola.upgrade() else { return };
    let dorme = s.addormentati;
    let nuovi = (lato_ui(&s.sinistro, &s.dati, dorme), lato_ui(&s.destro, &s.dati, dorme));
    let prima = s.mostrati.replace(nuovi.clone());
    if prima.as_ref() == Some(&nuovi) {
        return;
    }
    let (sx, dx) = nuovi;
    if ui.get_lato_sinistro() != sx {
        ui.set_lato_sinistro(sx);
    }
    if ui.get_lato_destro() != dx {
        ui.set_lato_destro(dx);
    }
    // Ritaglia solo se la larghezza cambia: con i campioni, solo per una scelta nuova.
    pillola::lati_cambiati(&ui);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pc_stats::StatoBatteria;
    use crate::test_ui::piattaforma_di_prova;
    use slint::ComponentHandle;

    /// Con niente / niente nessun timer gira mai; con la CPU il timer gira solo da visibile e
    /// sveglia; la batteria arriva e si riscrive solo il lato che cambia.
    #[test]
    fn timer_solo_se_servono() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        collega(&p, Lato::Niente, Lato::Niente);
        aggiorna(bisogni_scelti(), true, false);
        let accesi = || con(|s| (s.cpu_ram.running(), s.orologio.running(), s.campionatore.is_some())).unwrap();
        assert_eq!(accesi(), (false, false, false), "niente / niente: nulla acceso");

        scegli(Lato::Cpu, Lato::Ora);
        aggiorna(bisogni_scelti(), true, false);
        assert_eq!(accesi(), (true, true, true));
        assert_eq!(p.get_lato_destro().tipo, 1, "l'ora si scrive subito");
        // Nascosta o addormentata: tutto fermo (il campionatore resta, per riprendere).
        aggiorna(bisogni_scelti(), false, false);
        assert_eq!(accesi(), (false, false, true));
        aggiorna(bisogni_scelti(), true, true);
        assert_eq!(accesi(), (false, false, true));
        // Da addormentata i lati a tempo sono vuoti, col loro posto; un dato nuovo non si
        // riscrive, una scelta nuova sì.
        assert_eq!((p.get_lato_sinistro().tipo, p.get_lato_sinistro().campione.as_str()), (0, "CPU 100%"));
        assert_eq!((p.get_lato_destro().tipo, p.get_lato_destro().campione.as_str()), (0, "00:00"));
        batteria(LetturaBatteria { percentuale: 50.0, stato: StatoBatteria::Scarica });
        assert_eq!(p.get_lato_sinistro().tipo, 0, "un dato nuovo non ridisegna");
        scegli(Lato::Batteria, Lato::Niente);
        assert_eq!(p.get_lato_sinistro().testo, "50%", "la scelta nuova si vede");
        aggiorna(bisogni_scelti(), true, false);
        assert_eq!(accesi(), (false, false, false), "batteria sola: nessun timer, e il campionatore va via");
        assert_eq!(p.get_lato_sinistro().tipo, 2);
        assert_eq!(p.get_lato_sinistro().testo, "50%");
        assert_eq!(p.get_lato_destro().tipo, 0);
        assert!(ha_batteria());
        spegni();
    }

    /// Giro di correzioni 1: la larghezza della pillola si misura sul campione del lato, quindi
    /// non cambia col numero di cifre («CPU 9%» → «CPU 10%» → «CPU 100%»), né con un lato solo
    /// né con due.
    #[test]
    fn larghezza_non_cambia_col_valore() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        let mut d = DatiLati { ora: "9:05".into(), ..Default::default() };
        for (sinistro, destro) in [(Lato::Niente, Lato::Cpu), (Lato::Ora, Lato::Ram), (Lato::Meteo, Lato::Cpu)] {
            let mut larghezze = Vec::new();
            for (v, meteo) in [(9.0, "9°"), (10.0, "-3°"), (100.0, "-10,5°")] {
                (d.cpu, d.ram, d.meteo) = (Some(v), Some(v), Some(meteo.into()));
                p.set_lato_sinistro(lato_ui(&sinistro, &d, false));
                p.set_lato_destro(lato_ui(&destro, &d, false));
                larghezze.push(p.get_larghezza_pillola());
            }
            assert!(larghezze[0] > crate::finestre::LARGHEZZA_PILLOLA, "{sinistro:?}/{destro:?}: {larghezze:?}");
            assert!(larghezze.iter().all(|l| *l == larghezze[0]), "{sinistro:?}/{destro:?}: {larghezze:?}");
        }
        // Con un valore più corto («CPU —») il posto resta quello del campione.
        let prima = p.get_larghezza_pillola();
        d.cpu = None;
        p.set_lato_destro(lato_ui(&Lato::Cpu, &d, false));
        assert_eq!(p.get_larghezza_pillola(), prima);
    }

    /// Giro di correzioni 1: entrando in «dorme» con la pillola visibile i lati a tempo (ora, CPU,
    /// RAM) si svuotano con un solo ridisegno e la pillola non cambia larghezza; batteria, meteo e
    /// sensori restano; al risveglio tornano col valore di adesso.
    #[test]
    fn dorme_svuota_i_lati_a_tempo_senza_cambiare_larghezza() {
        piattaforma_di_prova();
        let p = Pillola::new().expect("pillola");
        p.show().expect("pillola mostrata nel backend di prova");
        collega(&p, Lato::Cpu, Lato::Batteria);
        batteria(LetturaBatteria { percentuale: 80.0, stato: StatoBatteria::Scarica });
        aggiorna(bisogni_scelti(), true, false);
        con(|s| {
            s.dati.cpu = Some(23.0);
            ridisegna(s);
        });
        assert_eq!(p.get_lato_sinistro().testo, "CPU 23%");
        let larghezza = p.get_larghezza_pillola();

        aggiorna(bisogni_scelti(), true, true);
        let (sx, dx) = (p.get_lato_sinistro(), p.get_lato_destro());
        assert_eq!((sx.tipo, sx.testo.as_str(), sx.campione.as_str()), (0, "", "CPU 100%"), "la CPU si svuota");
        assert_eq!((dx.tipo, dx.testo.as_str()), (2, "80%"), "la batteria resta");
        assert_eq!(p.get_larghezza_pillola(), larghezza, "stessa larghezza");
        assert!(elemento_con_nome(&p, "ItsyHome, batteria 80%"), "Narrator non legge la CPU ferma");
        // Poi più nulla: né un dato nuovo né un altro «dorme».
        let scritti = con(|s| s.mostrati.clone()).unwrap();
        batteria(LetturaBatteria { percentuale: 79.0, stato: StatoBatteria::Scarica });
        aggiorna(bisogni_scelti(), true, true);
        assert_eq!(p.get_lato_destro().testo, "80%");
        assert_eq!(con(|s| s.mostrati.clone()).unwrap(), scritti);

        // Al risveglio la CPU torna subito (un campione nuovo) e la batteria è quella di adesso.
        aggiorna(bisogni_scelti(), true, false);
        assert_eq!(p.get_lato_sinistro().tipo, 1);
        assert!(p.get_lato_sinistro().testo.starts_with("CPU "), "{}", p.get_lato_sinistro().testo);
        assert_eq!(p.get_lato_destro().testo, "79%");
        assert_eq!(p.get_larghezza_pillola(), larghezza);
        spegni();
    }

    fn elemento_con_nome(p: &Pillola, nome: &str) -> bool {
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(p, nome).next().is_some()
    }

    #[test]
    fn sensori_e_meteo_dai_dati_della_casa() {
        use crate::ponte::test::{menu, sensore};
        let mut m = menu(vec![sensore("sensor.sala_t", "21.5", Some("°C"))], vec![]);
        m.meteo = Some(casa_core::types::Meteo {
            entity_id: "weather.casa".into(),
            condizione: "cloudy".into(),
            temperatura: Some(9.2),
            unita: "°C".into(),
        });
        let (meteo, sensori) = dati_casa(&m, &Lato::Meteo, &Lato::Sensore("sensor.sala_t".into()));
        assert_eq!(meteo.as_deref(), Some("9°"));
        assert_eq!(sensori.get("sensor.sala_t").and_then(|n| n.valore), Some(21.5));
        // Non scelti: nulla.
        let (meteo, sensori) = dati_casa(&m, &Lato::Cpu, &Lato::Niente);
        assert!(meteo.is_none() && sensori.is_empty());
    }
}
