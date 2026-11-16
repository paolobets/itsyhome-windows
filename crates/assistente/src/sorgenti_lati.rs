//! Le sorgenti dei lati della pillola (M2, Task 4): accende e spegne i soli timer che servono e
//! riscrive un lato solo quando il suo contenuto cambia.
//!
//! - CPU e RAM: un timer Slint da 5 s (`PERIODO_LATI`) col `CampionatoreLeggero`, sul thread
//!   dell'interfaccia (le due letture costano microsecondi), solo se CPU o RAM sono scelte e la
//!   pillola si vede e non dorme.
//! - L'ora: una sveglia al minuto (rifasata a ogni scatto), alle stesse condizioni.
//! - La batteria resta sul thread da 60 s di M1 (`pc_stats::sorveglia_batteria`); meteo e sensori
//!   arrivano dal ponte a ogni `MenuData`: nessuna lettura in più.
//!
//! «Si vede» = `Visibilita::Pillola` (non ritirata, non nascosta, pannello chiuso) e non
//! allargata (lì i lati sono nascosti). Mentre la pillola non si vede o dorme nessun timer gira e
//! nulla si ridisegna: i dati arrivati intanto si mostrano al ritorno. Con i lati «niente / niente»
//! non c'è nulla di acceso: il consumo a riposo resta quello di M1.

use crate::lati::{self, Bisogni, ContenutoLato, DatiLati, Lato};
use crate::pc_stats::{CampionatoreLeggero, LetturaBatteria, PERIODO_LATI};
use crate::{orologio, pannello, pillola, ContenutoLatoUi, Pillola};
use casa_core::types::{AppEntity, MenuData};
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
    /// Ultimi contenuti scritti nella pillola (`None`: mai scritti).
    mostrati: Option<(ContenutoLato, ContenutoLato)>,
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
        ridisegna(s);
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
    let primo = con(|s| {
        // Da riscrivere: tornata visibile e sveglia, o un dato letto qui sotto.
        let mut cambiato = attivi && !s.attivi;
        s.attivi = attivi;
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
            ridisegna(s);
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

/// Meteo (se scelto) e valori grezzi dei sensori scelti, dai dati della casa.
fn dati_casa(menu: &MenuData, sinistro: &Lato, destro: &Lato) -> (Option<String>, HashMap<String, String>) {
    let scelti = [sinistro, destro];
    let meteo = if scelti.contains(&&Lato::Meteo) {
        menu.meteo.as_ref().and_then(orologio::temperatura_breve)
    } else {
        None
    };
    let mut sensori = HashMap::new();
    for l in scelti {
        let Lato::Sensore(id) = l else { continue };
        let valore = menu
            .favorites
            .iter()
            .chain(menu.rooms.iter().flat_map(|r| r.entities.iter()))
            .find_map(|e| match e {
                AppEntity::Sensor(x) if x.base.entity_id == *id && x.base.is_available => Some(x.value.clone()),
                _ => None,
            });
        if let Some(v) = valore {
            sensori.insert(id.clone(), v);
        }
    }
    (meteo, sensori)
}

/// Il contenuto di un lato come lo conosce l'interfaccia.
pub fn contenuto_ui(c: &ContenutoLato) -> ContenutoLatoUi {
    match c {
        ContenutoLato::Vuoto => ContenutoLatoUi::default(),
        ContenutoLato::Testo(t) => ContenutoLatoUi { tipo: 1, testo: t.as_str().into(), ..Default::default() },
        ContenutoLato::Batteria { testo, frazione, tinta } => ContenutoLatoUi {
            tipo: 2,
            testo: testo.as_str().into(),
            frazione: *frazione,
            tinta: pannello::tinta_ui(*tinta),
        },
    }
}

/// Riscrive nella pillola i lati che cambiano (solo da attivi: da nascosta o addormentata nulla si
/// ridisegna) e, se la larghezza cambia, il ritaglio.
fn ridisegna(s: &mut Sorgenti) {
    if !s.attivi {
        return;
    }
    let Some(ui) = s.pillola.upgrade() else { return };
    let nuovi = (lati::contenuto(&s.sinistro, &s.dati), lati::contenuto(&s.destro, &s.dati));
    let prima = s.mostrati.replace(nuovi.clone());
    if prima.as_ref() == Some(&nuovi) {
        return;
    }
    let (sx, dx) = (contenuto_ui(&nuovi.0), contenuto_ui(&nuovi.1));
    if ui.get_lato_sinistro() != sx {
        ui.set_lato_sinistro(sx);
    }
    if ui.get_lato_destro() != dx {
        ui.set_lato_destro(dx);
    }
    pillola::lati_cambiati(&ui);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pc_stats::StatoBatteria;
    use crate::test_ui::piattaforma_di_prova;

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
        // Da dormiente nulla si riscrive; al risveglio sì.
        batteria(LetturaBatteria { percentuale: 50.0, stato: StatoBatteria::Scarica });
        scegli(Lato::Batteria, Lato::Niente);
        assert_eq!(p.get_lato_sinistro().testo, "CPU —", "da addormentata la pillola resta com'era");
        aggiorna(bisogni_scelti(), true, false);
        assert_eq!(accesi(), (false, false, false), "batteria sola: nessun timer, e il campionatore va via");
        assert_eq!(p.get_lato_sinistro().tipo, 2);
        assert_eq!(p.get_lato_sinistro().testo, "50%");
        assert_eq!(p.get_lato_destro().tipo, 0);
        assert!(ha_batteria());
        spegni();
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
        assert_eq!(sensori.get("sensor.sala_t").map(String::as_str), Some("21.5"));
        // Non scelti: nulla.
        let (meteo, sensori) = dati_casa(&m, &Lato::Cpu, &Lato::Niente);
        assert!(meteo.is_none() && sensori.is_empty());
    }
}
