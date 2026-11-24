//! La pagina PC sull'impianto Misure (ridisegno F3, Task 6): le misure, le righe, la sintesi e lo
//! stato come funzioni pure. Il ponte le chiama col campione del PC e lo stato delle radio e
//! scrive i modelli solo a pagina aperta, solo se cambiano (Task 7).
#![allow(dead_code)]

use super::registro::tono_ui;
use super::{Tono, R1};
use crate::pannello::icona;
use super::pc::SOGLIA_CARICO;
use crate::pc_stats::{formatta_byte, StatPc, StatoBatteria};
use crate::{MisuraUi, RigaMisuraUi, SintesiUi, StatoPaginaUi, TipoRigaMisuraUi, TipoStatoPaginaUi};

/// «Muto» / «Riattiva l'audio»: l'azione della testata (l'audio del PC).
pub const MUTO: &str = "pc.muto";
/// Gli id delle righe della pagina (le azioni e il fuoco si dicono con questi).
pub const WIFI: &str = "pc.wifi";
pub const BLUETOOTH: &str = "pc.bluetooth";
pub const VOLUME: &str = "pc.volume";
pub const RIMANDI: &str = "pc.rimandi";
pub const RETE: &str = "pc.rete";

/// Il motivo di un «—»: la lettura non è ancora arrivata, oppure è arrivata e non dice nulla.
const IN_CORSO: &str = "Lettura in corso";
const NON_LETTA: &str = "Non letta";
/// Il rifiuto di Windows a un cambio di Wi-Fi o Bluetooth (la pagina giusta di Windows è già aperta).
const RIFIUTO: &str = "Non riuscito: apro le Impostazioni di Windows";
const GIB: f64 = (1u64 << 30) as f64;

/// Un percento intero 0..100 (il valore è finito: chi chiama lo ha già controllato).
fn percento(v: f32) -> u32 {
    v.round().clamp(0.0, 100.0) as u32
}

/// Una frazione 0..1 da un percento.
fn frazione(v: f32) -> f32 {
    (v / 100.0).clamp(0.0, 1.0)
}

/// Un valore che si può mostrare: finito.
fn buono(v: Option<f32>) -> Option<f32> {
    v.filter(|v| v.is_finite())
}

/// Il posto di una misura senza dato: «—», niente unità né barra, il motivo nel dettaglio. Per
/// Narrator «{etichetta}, {motivo in minuscolo}».
fn assente(id: &str, etichetta: &str, motivo: &str) -> MisuraUi {
    MisuraUi {
        presente: true,
        id: id.into(),
        icona: icona("nessuna"),
        etichetta: etichetta.into(),
        valore: "—".into(),
        unita: "".into(),
        frazione: 0.0,
        dettaglio: motivo.into(),
        tono: tono_ui(Tono::Vuoto),
        narratore: format!("{etichetta}, {}", motivo.to_lowercase()).into(),
    }
}

/// Il posto lasciato vuoto (né batteria né GPU): non si disegna, non si legge.
fn vuota() -> MisuraUi {
    MisuraUi { icona: icona("nessuna"), ..Default::default() }
}

/// Una misura in percento: «{etichetta}, {p} %» e, se c'è, il dettaglio.
fn in_percento(id: &str, etichetta: &str, pct: f32, dettaglio: String, tono: Tono) -> MisuraUi {
    let p = percento(pct);
    let narratore = if dettaglio.is_empty() { format!("{etichetta}, {p} %") } else { format!("{etichetta}, {p} %, {dettaglio}") };
    MisuraUi {
        presente: true,
        id: id.into(),
        icona: icona("nessuna"),
        etichetta: etichetta.into(),
        valore: p.to_string().into(),
        unita: "%".into(),
        frazione: frazione(pct),
        dettaglio: dettaglio.into(),
        tono: tono_ui(tono),
        narratore: narratore.into(),
    }
}

/// «attivo» da `SOGLIA_CARICO` in su (sul valore intero che si vede), come la carta del PC.
fn tono_carico(pct: f32) -> Tono {
    if percento(pct) as f32 >= SOGLIA_CARICO {
        Tono::Attivo
    } else {
        Tono::Calmo
    }
}

/// «7,7 di 16 GB»: l'unità si scrive una volta sola se è la stessa («512 MB di 1,5 GB» altrimenti).
/// Non è `formatta_usato_totale` («7,7 / 16 GB»), che resta per chi la usa.
pub fn usato_di_totale(usato: u64, totale: u64) -> String {
    let (u, t) = (formatta_byte(usato), formatta_byte(totale));
    match (u.rsplit_once(' '), t.rsplit_once(' ')) {
        (Some((num, unita_u)), Some((_, unita_t))) if unita_u == unita_t => format!("{num} di {t}"),
        _ => format!("{u} di {t}"),
    }
}

/// «1,2 MB/s giù, 0,3 MB/s su»: il traffico del Wi-Fi in byte al secondo come lo legge Narrator
/// (le frecce le disegna Slint). È il dato che c'è: non la velocità del collegamento.
pub fn velocita_testo(giu: u64, su: u64) -> String {
    format!("{} giù, {} su", crate::pc_stats::formatta_velocita(giu), crate::pc_stats::formatta_velocita(su))
}

fn cpu(stat: Option<&StatPc>, cpu_pronta: bool, gpu: Option<f32>) -> MisuraUi {
    let Some(s) = stat else { return assente("cpu", "CPU", IN_CORSO) };
    if !cpu_pronta {
        return assente("cpu", "CPU", IN_CORSO);
    }
    let Some(c) = buono(s.cpu_pct) else { return assente("cpu", "CPU", NON_LETTA) };
    // La GPU nel dettaglio solo se non ha un posto suo (c'è la batteria).
    let dettaglio = gpu.map(|g| format!("GPU {} %", percento(g))).unwrap_or_default();
    in_percento("cpu", "CPU", c, dettaglio, tono_carico(c))
}

fn memoria(stat: Option<&StatPc>) -> MisuraUi {
    let Some(s) = stat else { return assente("memoria", "Memoria", IN_CORSO) };
    if s.ram_totale == 0 {
        return assente("memoria", "Memoria", NON_LETTA);
    }
    let pct = s.ram_usata as f32 / s.ram_totale as f32 * 100.0;
    in_percento("memoria", "Memoria", pct, usato_di_totale(s.ram_usata, s.ram_totale), tono_carico(pct))
}

/// Il disco: i GB liberi (non la percentuale), la barra è l'occupato. Dischi e reti si leggono solo
/// a pagina PC aperta: finché `lente_lette` è falso il motivo è «Lettura in corso», mai «Non letta».
fn disco(stat: Option<&StatPc>, lente_lette: bool) -> MisuraUi {
    const ETICHETTA: &str = "Disco C:";
    let Some(s) = stat.filter(|_| lente_lette) else { return assente("disco", ETICHETTA, IN_CORSO) };
    if s.disco_totale == 0 {
        return assente("disco", ETICHETTA, NON_LETTA);
    }
    let liberi = (s.disco_totale.saturating_sub(s.disco_usato) as f64 / GIB).round() as u64;
    let totale = formatta_byte(s.disco_totale);
    MisuraUi {
        presente: true,
        id: "disco".into(),
        icona: icona("nessuna"),
        etichetta: ETICHETTA.into(),
        valore: liberi.to_string().into(),
        unita: "GB".into(),
        frazione: (s.disco_usato as f64 / s.disco_totale as f64).clamp(0.0, 1.0) as f32,
        dettaglio: format!("liberi su {totale}").into(),
        tono: tono_ui(Tono::Calmo),
        narratore: format!("{ETICHETTA}, {liberi} GB liberi su {totale}").into(),
    }
}

/// Il quarto posto: la batteria se c'è, altrimenti la GPU se si legge, altrimenti vuoto. Senza
/// campione è la batteria in attesa (quattro posti, tutti «—»).
fn quarto(stat: Option<&StatPc>, gpu: Option<f32>) -> MisuraUi {
    let Some(s) = stat else { return assente("batteria", "Batteria", IN_CORSO) };
    if let Some(b) = s.batteria.filter(|b| b.percentuale.is_finite()) {
        let (dettaglio, tono) = match b.stato {
            StatoBatteria::InCarica => ("In carica", Tono::Fatto),
            StatoBatteria::Piena => ("Carica", Tono::Fatto),
            StatoBatteria::Scarica => ("A batteria", Tono::Calmo),
            StatoBatteria::Sconosciuto => ("", Tono::Calmo),
        };
        return in_percento("batteria", "Batteria", b.percentuale, dettaglio.into(), tono);
    }
    match gpu {
        Some(g) => in_percento("gpu", "GPU", g, String::new(), Tono::Calmo),
        None => vuota(),
    }
}

/// Le quattro misure, ognuna al suo posto: CPU, Memoria, Disco (GB liberi) e Batteria; senza
/// batteria il quarto posto va alla GPU, senza né l'una né l'altra resta vuoto. `None`: il primo
/// campione non c'è ancora (quattro «—»). `cpu_pronta`: la CPU è una differenza fra due campioni.
/// `lente_lette`: il campionatore ha già letto le fonti lente (disco, GPU): finché è falso il
/// disco dice «Lettura in corso», e il quarto posto senza batteria si decide solo dopo, così non
/// compare e sparisce (`gpu_pct: None` vuol dire sia «nessuna GPU» sia «non ancora letta»).
pub fn misure(stat: Option<&StatPc>, cpu_pronta: bool, lente_lette: bool) -> [MisuraUi; 4] {
    let gpu = buono(stat.and_then(|s| s.gpu_pct));
    let con_batteria = stat.is_some_and(|s| s.batteria.is_some_and(|b| b.percentuale.is_finite()));
    [cpu(stat, cpu_pronta, gpu.filter(|_| con_batteria)), memoria(stat), disco(stat, lente_lette), quarto(stat, gpu)]
}

/// Quello che il ponte sa del PC per le righe: tutto già letto, niente di Windows qui dentro (la
/// pagina compila e si prova ovunque). `wifi` e `bluetooth`: `None` = non ancora letti, altrimenti
/// (presente, acceso). `velocita`: il traffico già in parole (`velocita_testo`). `volume`: livello
/// 0..1 e muto, `None` senza uscita audio. `errore_radio`: Windows ha rifiutato un cambio.
pub struct RighePc<'a> {
    pub windows: bool,
    pub velocita: Option<String>,
    pub nome_rete: Option<&'a str>,
    pub wifi: Option<(bool, bool)>,
    pub bluetooth: Option<(bool, bool)>,
    pub cuffie: Option<(&'a str, u8)>,
    pub volume: Option<(f32, bool)>,
    pub errore_radio: bool,
}

fn riga(id: &str, tipo: TipoRigaMisuraUi, nome: &str, valore: &str, narratore: String) -> RigaMisuraUi {
    RigaMisuraUi {
        id: id.into(),
        tipo,
        icona: icona("nessuna"),
        nome: nome.into(),
        valore: valore.into(),
        acceso: false,
        attiva: false,
        errore: "".into(),
        narratore: narratore.into(),
    }
}

/// Un interruttore di radio. `acceso_valore` e `acceso_narratore`: cosa dire a radio accesa (il
/// nome della rete e la velocità, o le cuffie); altrimenti «Acceso».
fn radio(id: &str, nome: &str, stato: Option<(bool, bool)>, acceso_valore: Option<(String, String)>, errore: bool) -> RigaMisuraUi {
    let (presente, acceso) = match stato {
        Some((presente, acceso)) => (presente, presente && acceso),
        None => (false, false),
    };
    let (valore, parola, dettaglio) = match stato {
        None => (IN_CORSO.to_owned(), IN_CORSO.to_lowercase(), None),
        Some((false, _)) => ("Non presente".to_owned(), "non presente".to_owned(), None),
        Some((true, false)) => ("Spento".to_owned(), "spento".to_owned(), None),
        Some((true, true)) => match acceso_valore {
            Some((v, d)) => (v, "acceso".to_owned(), Some(d)),
            None => ("Acceso".to_owned(), "acceso".to_owned(), None),
        },
    };
    let mut narratore = format!("{nome}, {parola}");
    if let Some(d) = dettaglio {
        narratore = format!("{narratore}, {d}");
    }
    let rifiuto = errore && presente;
    if rifiuto {
        narratore = format!("{narratore}, {RIFIUTO}");
    }
    RigaMisuraUi {
        acceso,
        attiva: presente,
        errore: if rifiuto { RIFIUTO.into() } else { "".into() },
        ..riga(id, TipoRigaMisuraUi::Interruttore, nome, &valore, narratore)
    }
}

/// Le righe della pagina, in quest'ordine: Wi-Fi, Bluetooth, Volume (solo con un'uscita audio) e i
/// rimandi alle pagine di Windows. Fuori da Windows una sola riga di stato, la rete.
pub fn righe(r: &RighePc) -> Vec<RigaMisuraUi> {
    if !r.windows {
        let (valore, narratore) = match &r.velocita {
            Some(v) => (v.as_str(), format!("Rete, {v}")),
            None => ("—", "Rete, non letta".to_owned()),
        };
        return vec![riga(RETE, TipoRigaMisuraUi::Stato, "Rete", valore, narratore)];
    }
    // Il nome della rete e il traffico solo a Wi-Fi acceso: «Casa 1,2 MB/s giù, 0,3 MB/s su».
    let rete: Vec<&str> = [r.nome_rete.filter(|n| !n.is_empty()), r.velocita.as_deref().filter(|v| !v.is_empty())].into_iter().flatten().collect();
    let wifi = (!rete.is_empty()).then(|| (rete.join(" "), rete.join(", ")));
    // Le cuffie: «Cuffie 70 %» a vista, il loro nome per Narrator.
    let cuffie = r.cuffie.map(|(nome, pct)| {
        let pct = pct.min(100);
        (format!("Cuffie {pct} %"), format!("{nome} {pct} %"))
    });
    let mut v = vec![radio(WIFI, "Wi-Fi", r.wifi, wifi, r.errore_radio), radio(BLUETOOTH, "Bluetooth", r.bluetooth, cuffie, r.errore_radio)];
    if let Some((livello, muto)) = r.volume.filter(|(l, _)| l.is_finite()) {
        let p = percento(livello * 100.0);
        let narratore = if muto { format!("Volume {p} %, muto") } else { format!("Volume {p} %") };
        v.push(RigaMisuraUi { acceso: !muto, attiva: true, ..riga(VOLUME, TipoRigaMisuraUi::Cursore, "Volume", &format!("{p} %"), narratore) });
    }
    v.push(RigaMisuraUi {
        attiva: true,
        ..riga(RIMANDI, TipoRigaMisuraUi::Rimandi, "Windows", "", "Windows, apre le impostazioni di rete, Bluetooth e suono".into())
    });
    v
}

/// La sintesi in cima alla pagina: il titolo è `r1.fatto` della carta; l'azione è «Muto» (o
/// «Riattiva l'audio» da muto) e c'è solo con un'uscita audio. Secondaria e senza icona: in F3 la
/// sola azione primaria è «Spegni tutte» di Casa.
pub fn sintesi(r1: &R1, volume: Option<(f32, bool)>) -> SintesiUi {
    let (azione_id, azione_testo) = match volume {
        Some((_, true)) => (MUTO, "Riattiva l'audio"),
        Some((_, false)) => (MUTO, "Muto"),
        None => ("", ""),
    };
    SintesiUi {
        tono: tono_ui(Tono::Calmo),
        titolo: r1.fatto.as_str().into(),
        azione_id: azione_id.into(),
        azione_testo: azione_testo.into(),
        azione_primaria: false,
        // Mai l'`Image` vuota (non è uguale a se stessa: la sintesi sarebbe sempre «cambiata»).
        azione_icona: icona("nessuna"),
        ..Default::default()
    }
}

/// Lo stato della pagina: senza il primo campione è il primo avvio («Lettura in corso»); col
/// campione la pagina c'è sempre, anche con la CPU non ancora pronta (il «—» sta nella misura).
pub fn stato(stat: Option<&StatPc>, _cpu_pronta: bool) -> StatoPaginaUi {
    match stat {
        None => StatoPaginaUi { tipo: TipoStatoPaginaUi::PrimoAvvio, frase: IN_CORSO.into(), ..Default::default() },
        Some(_) => StatoPaginaUi { tipo: TipoStatoPaginaUi::Pieno, ..Default::default() },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pc_stats::{LetturaBatteria, StatoBatteria};
    use crate::{TipoRigaMisuraUi, TipoStatoPaginaUi, TonoModuloUi};

    const GB: u64 = 1 << 30;

    fn stat() -> StatPc {
        StatPc {
            cpu_pct: Some(12.0),
            ram_usata: GB * 77 / 10,
            ram_totale: 16 * GB,
            disco_usato: 294 * GB,
            disco_totale: 476 * GB,
            rete_giu_bps: 0,
            rete_su_bps: 0,
            batteria: None,
            gpu_pct: None,
        }
    }

    fn con_batteria(percentuale: f32, stato: StatoBatteria) -> StatPc {
        StatPc { batteria: Some(LetturaBatteria { percentuale, stato }), ..stat() }
    }

    fn tutto() -> RighePc<'static> {
        RighePc {
            windows: true,
            velocita: Some("1,2 MB/s giù, 0,3 MB/s su".into()),
            nome_rete: Some("Casa"),
            wifi: Some((true, true)),
            bluetooth: Some((true, true)),
            cuffie: Some(("Cuffie di Paolo", 70)),
            volume: Some((0.4, false)),
            errore_radio: false,
        }
    }

    /// Le cinque misure della tabella, ognuna al suo posto: valore, unità, frazione, dettaglio e
    /// tono. Con la batteria la GPU va nel dettaglio della CPU; senza batteria la GPU è il quarto
    /// posto; senza tutte e due il quarto posto resta vuoto.
    #[test]
    fn misure_per_posto() {
        // Con batteria e GPU: la GPU nel dettaglio della CPU, la batteria al quarto posto.
        let s = StatPc { gpu_pct: Some(37.0), ..con_batteria(84.0, StatoBatteria::InCarica) };
        let m = misure(Some(&s), true, true);
        let visti: Vec<(&str, &str, &str, &str, &str)> = m.iter().map(|k| (k.id.as_str(), k.etichetta.as_str(), k.valore.as_str(), k.unita.as_str(), k.dettaglio.as_str())).collect();
        assert_eq!(
            visti,
            [
                ("cpu", "CPU", "12", "%", "GPU 37 %"),
                ("memoria", "Memoria", "48", "%", "7,7 di 16 GB"),
                ("disco", "Disco C:", "182", "GB", "liberi su 476 GB"),
                ("batteria", "Batteria", "84", "%", "In carica"),
            ]
        );
        assert!(m.iter().all(|k| k.presente));
        assert_eq!((m[0].frazione, m[3].frazione), (0.12, 0.84));
        assert!((m[1].frazione - 0.48).abs() < 0.01, "RAM {}", m[1].frazione);
        assert!((m[2].frazione - 294.0 / 476.0).abs() < 0.001, "il disco è l'occupato");
        assert_eq!((m[0].tono, m[1].tono, m[2].tono, m[3].tono), (TonoModuloUi::Calmo, TonoModuloUi::Calmo, TonoModuloUi::Calmo, TonoModuloUi::Fatto));

        // A batteria, o piena.
        let m = misure(Some(&con_batteria(60.0, StatoBatteria::Scarica)), true, true);
        assert_eq!((m[3].dettaglio.as_str(), m[3].tono), ("A batteria", TonoModuloUi::Calmo));
        let m = misure(Some(&con_batteria(100.0, StatoBatteria::Piena)), true, true);
        assert_eq!((m[3].dettaglio.as_str(), m[3].tono), ("Carica", TonoModuloUi::Fatto));

        // Con la batteria e senza GPU il dettaglio della CPU è vuoto.
        assert_eq!(misure(Some(&con_batteria(84.0, StatoBatteria::Scarica)), true, true)[0].dettaglio.as_str(), "");

        // Senza batteria la GPU è il quarto posto (e non è anche nel dettaglio della CPU).
        let s = StatPc { gpu_pct: Some(37.0), ..stat() };
        let m = misure(Some(&s), true, true);
        assert_eq!((m[3].presente, m[3].id.as_str(), m[3].etichetta.as_str(), m[3].valore.as_str(), m[3].unita.as_str(), m[3].dettaglio.as_str()), (true, "gpu", "GPU", "37", "%", ""));
        assert_eq!(m[3].frazione, 0.37);
        assert_eq!(m[0].dettaglio.as_str(), "");

        // Senza batteria né GPU, con le fonti lente già lette: il quarto posto resta vuoto.
        let m = misure(Some(&stat()), true, true);
        assert!(!m[3].presente);
        assert!(m[..3].iter().all(|k| k.presente));
        // Il posto vuoto resta vuoto anche prima delle fonti lente: la GPU non compare e sparisce.
        assert!(!misure(Some(&stat()), true, false)[3].presente);
    }

    /// Review Focus 4: un dato che manca è «—» col motivo, mai «0». Senza campione, quattro
    /// posti; la CPU non pronta è «—» solo lì; un disco di capienza zero e una RAM a zero non si
    /// leggono; senza le fonti lente disco e GPU dicono «Lettura in corso», mai «Non letta».
    #[test]
    fn misure_assenti_non_sono_zero() {
        let senza = |k: &MisuraUi, motivo: &str| {
            assert!(k.presente, "{}", k.id);
            assert_eq!((k.valore.as_str(), k.unita.as_str(), k.frazione, k.dettaglio.as_str()), ("—", "", 0.0, motivo), "{}", k.id);
            assert!(!k.valore.contains('0') && !k.narratore.contains('0'), "{}: {}", k.id, k.narratore);
        };

        let m = misure(None, false, false);
        for k in &m {
            senza(k, "Lettura in corso");
        }
        assert_eq!(m.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["cpu", "memoria", "disco", "batteria"]);
        assert!(m.iter().all(|k| k.tono == TonoModuloUi::Vuoto));

        // La CPU è una differenza fra due campioni: non pronta è «—» solo lì.
        let m = misure(Some(&stat()), false, true);
        senza(&m[0], "Lettura in corso");
        assert_eq!((m[1].valore.as_str(), m[2].valore.as_str()), ("48", "182"));
        // Pronta ma non letta: un altro motivo.
        let m = misure(Some(&StatPc { cpu_pct: None, ..stat() }), true, true);
        senza(&m[0], "Non letta");
        let m = misure(Some(&StatPc { cpu_pct: Some(f32::NAN), ..stat() }), true, true);
        senza(&m[0], "Non letta");

        // RAM a zero.
        let m = misure(Some(&StatPc { ram_totale: 0, ram_usata: 0, ..stat() }), true, true);
        senza(&m[1], "Non letta");
        assert_eq!(m[0].valore.as_str(), "12");

        // Disco: totale zero con le fonti lente lette = non letto; senza fonti lente = in corso, anche
        // se per caso un totale c'è.
        let m = misure(Some(&StatPc { disco_totale: 0, disco_usato: 0, ..stat() }), true, true);
        senza(&m[2], "Non letta");
        let m = misure(Some(&StatPc { disco_totale: 0, disco_usato: 0, ..stat() }), true, false);
        senza(&m[2], "Lettura in corso");
        senza(&misure(Some(&stat()), true, false)[2], "Lettura in corso");
    }

    /// CPU o RAM da 85 % in su sono «attivo»; sotto, calmo. La batteria in carica è «fatto».
    #[test]
    fn soglia_e_carica() {
        let tono = |cpu: f32, ram: u64, batteria: Option<(f32, StatoBatteria)>| {
            let s = StatPc { cpu_pct: Some(cpu), ram_usata: ram * GB / 100, ram_totale: GB, batteria: batteria.map(|(percentuale, stato)| LetturaBatteria { percentuale, stato }), ..stat() };
            let m = misure(Some(&s), true, true);
            (m[0].tono, m[1].tono, m[3].tono)
        };
        assert_eq!(tono(90.0, 10, None), (TonoModuloUi::Attivo, TonoModuloUi::Calmo, TonoModuloUi::Vuoto));
        assert_eq!(tono(85.0, 10, None).0, TonoModuloUi::Attivo, "la soglia è inclusa");
        assert_eq!(tono(84.0, 10, None).0, TonoModuloUi::Calmo);
        assert_eq!(tono(10.0, 91, None).1, TonoModuloUi::Attivo);
        assert_eq!(tono(10.0, 84, None).1, TonoModuloUi::Calmo);
        assert_eq!(tono(10.0, 10, Some((50.0, StatoBatteria::InCarica))).2, TonoModuloUi::Fatto);
        assert_eq!(tono(10.0, 10, Some((50.0, StatoBatteria::Scarica))).2, TonoModuloUi::Calmo);
        // La batteria scarica non è «attivo»: non è carico di lavoro.
        assert_eq!(tono(10.0, 10, Some((5.0, StatoBatteria::Scarica))).2, TonoModuloUi::Calmo);
    }

    /// Windows con tutto; senza Wi-Fi «Non presente» e non attivabile; senza volume nessun cursore;
    /// fuori da Windows una sola riga di stato; un cambio rifiutato porta il testo del rifiuto.
    #[test]
    fn righe_per_hardware() {
        let r = righe(&tutto());
        let ordine: Vec<(&str, TipoRigaMisuraUi)> = r.iter().map(|k| (k.id.as_str(), k.tipo)).collect();
        assert_eq!(
            ordine,
            [(WIFI, TipoRigaMisuraUi::Interruttore), (BLUETOOTH, TipoRigaMisuraUi::Interruttore), (VOLUME, TipoRigaMisuraUi::Cursore), (RIMANDI, TipoRigaMisuraUi::Rimandi)]
        );
        assert_eq!((r[0].nome.as_str(), r[0].valore.as_str(), r[0].acceso, r[0].attiva), ("Wi-Fi", "Casa 1,2 MB/s giù, 0,3 MB/s su", true, true));
        assert_eq!(r[0].narratore.as_str(), "Wi-Fi, acceso, Casa, 1,2 MB/s giù, 0,3 MB/s su");
        assert_eq!((r[1].nome.as_str(), r[1].valore.as_str(), r[1].acceso, r[1].attiva), ("Bluetooth", "Cuffie 70 %", true, true));
        assert_eq!(r[1].narratore.as_str(), "Bluetooth, acceso, Cuffie di Paolo 70 %");
        assert_eq!((r[2].nome.as_str(), r[2].valore.as_str(), r[2].acceso, r[2].attiva), ("Volume", "40 %", true, true));
        assert_eq!(r[2].narratore.as_str(), "Volume 40 %");
        assert_eq!((r[3].nome.as_str(), r[3].attiva), ("Windows", true));
        assert!(r.iter().all(|k| k.errore.is_empty()));

        // Il Wi-Fi acceso senza nome né velocità, spento; il Bluetooth acceso senza cuffie, spento.
        let b = righe(&RighePc { nome_rete: None, velocita: None, cuffie: None, ..tutto() });
        assert_eq!((b[0].valore.as_str(), b[1].valore.as_str()), ("Acceso", "Acceso"));
        let b = righe(&RighePc { wifi: Some((true, false)), bluetooth: Some((true, false)), ..tutto() });
        assert_eq!((b[0].valore.as_str(), b[0].acceso, b[0].attiva), ("Spento", false, true));
        assert_eq!((b[1].valore.as_str(), b[1].acceso, b[1].attiva), ("Spento", false, true));
        assert_eq!(b[1].narratore.as_str(), "Bluetooth, spento", "le cuffie non si dicono a radio spenta");

        // Senza Wi-Fi: «Non presente», non attivabile; il resto c'è.
        let n = righe(&RighePc { wifi: Some((false, false)), ..tutto() });
        assert_eq!((n[0].valore.as_str(), n[0].acceso, n[0].attiva, n[0].narratore.as_str()), ("Non presente", false, false, "Wi-Fi, non presente"));
        assert!(n[1].attiva);
        // Non ancora letti: né presenti né attivabili.
        let n = righe(&RighePc { wifi: None, bluetooth: None, ..tutto() });
        assert_eq!((n[0].valore.as_str(), n[0].attiva, n[1].valore.as_str(), n[1].attiva), ("Lettura in corso", false, "Lettura in corso", false));

        // Senza uscita audio nessuna riga cursore, e nessun elemento Volume.
        let s = righe(&RighePc { volume: None, ..tutto() });
        assert!(s.iter().all(|k| k.tipo != TipoRigaMisuraUi::Cursore && k.id != VOLUME && k.nome != "Volume"));
        assert_eq!(s.len(), 3);
        // Muto: il cursore non è acceso, il livello si legge ancora; il nome lo dice.
        let m = righe(&RighePc { volume: Some((0.4, true)), ..tutto() });
        assert_eq!((m[2].valore.as_str(), m[2].acceso, m[2].narratore.as_str()), ("40 %", false, "Volume 40 %, muto"));
        // Un livello fuori scala o non numerico non esce da 0..100.
        assert_eq!(righe(&RighePc { volume: Some((1.7, false)), ..tutto() })[2].valore.as_str(), "100 %");
        assert!(righe(&RighePc { volume: Some((f32::NAN, false)), ..tutto() }).iter().all(|k| k.id != VOLUME));

        // Fuori da Windows: una sola riga di stato con la velocità, nessun interruttore.
        let f = righe(&RighePc { windows: false, nome_rete: None, wifi: None, bluetooth: None, cuffie: None, volume: None, ..tutto() });
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].id.as_str(), f[0].tipo, f[0].nome.as_str(), f[0].valore.as_str(), f[0].attiva), (RETE, TipoRigaMisuraUi::Stato, "Rete", "1,2 MB/s giù, 0,3 MB/s su", false));
        let f = righe(&RighePc { windows: false, velocita: None, ..tutto() });
        assert_eq!((f.len(), f[0].valore.as_str()), (1, "—"));

        // Il rifiuto di Windows: il testo sta sulle radio che si possono azionare, non sul resto.
        let e = righe(&RighePc { errore_radio: true, ..tutto() });
        assert_eq!((e[0].errore.as_str(), e[1].errore.as_str()), ("Non riuscito: apro le Impostazioni di Windows", "Non riuscito: apro le Impostazioni di Windows"));
        assert!(e[2].errore.is_empty() && e[3].errore.is_empty());
        assert!(e[0].narratore.ends_with("Non riuscito: apro le Impostazioni di Windows"), "{}", e[0].narratore);
    }

    /// «Muto» con l'audio, «Riattiva l'audio» da muto, nessuna azione senza uscita audio; il
    /// titolo è `r1.fatto`. Prima del campione la pagina è al primo avvio, «Lettura in corso».
    #[test]
    fn sintesi_e_stato() {
        let r1 = R1 { fatto: "Tutto regolare".into(), valore: String::new() };
        let s = sintesi(&r1, Some((0.4, false)));
        assert_eq!((s.titolo.as_str(), s.azione_id.as_str(), s.azione_testo.as_str(), s.azione_primaria), ("Tutto regolare", MUTO, "Muto", false));
        let s = sintesi(&r1, Some((0.4, true)));
        assert_eq!((s.azione_id.as_str(), s.azione_testo.as_str()), (MUTO, "Riattiva l'audio"));
        let s = sintesi(&r1, None);
        assert_eq!((s.titolo.as_str(), s.azione_id.as_str(), s.azione_testo.as_str()), ("Tutto regolare", "", ""));
        let r1 = R1 { fatto: "CPU 91 %".into(), valore: String::new() };
        assert_eq!(sintesi(&r1, None).titolo.as_str(), "CPU 91 %");

        let primo = stato(None, false);
        assert_eq!((primo.tipo, primo.frase.as_str(), primo.azione_id.as_str()), (TipoStatoPaginaUi::PrimoAvvio, "Lettura in corso", ""));
        assert_eq!(stato(None, true).tipo, TipoStatoPaginaUi::PrimoAvvio);
        // Col campione la pagina c'è, anche se la CPU non è ancora pronta (lì il «—» è nella misura).
        assert_eq!(stato(Some(&stat()), true).tipo, TipoStatoPaginaUi::Pieno);
        assert_eq!(stato(Some(&stat()), false).tipo, TipoStatoPaginaUi::Pieno);
    }

    /// C12: nessun nome per Narrator è tutto maiuscolo. «CPU, 12 %» non ha lettere minuscole per
    /// costruzione (è una sigla), quindi: dove l'etichetta ha minuscole il nome non la contiene
    /// maiuscola; per le sigle il nome è esattamente quello atteso. Il controllo può fallire in
    /// entrambi i versi.
    #[test]
    fn narratore_senza_maiuscole() {
        let s = StatPc { gpu_pct: Some(37.0), ..con_batteria(84.0, StatoBatteria::InCarica) };
        let senza_batteria = StatPc { gpu_pct: Some(37.0), ..stat() };
        let mut visti = vec![];
        for (stat, lente) in [(Some(&s), true), (Some(&senza_batteria), true), (None, false)] {
            visti.extend(misure(stat, stat.is_some(), lente));
        }
        let mut sigle = 0;
        for k in visti.iter().filter(|k| k.presente) {
            assert!(!k.narratore.is_empty(), "{}", k.id);
            if k.etichetta.chars().any(char::is_lowercase) {
                assert!(!k.narratore.contains(&k.etichetta.to_uppercase()), "{}: {}", k.id, k.narratore);
                assert!(k.narratore.starts_with(k.etichetta.as_str()), "{}: {}", k.id, k.narratore);
            } else {
                sigle += 1;
                assert!(k.narratore.starts_with(&format!("{}, ", k.etichetta)), "{}: {}", k.id, k.narratore);
            }
        }
        assert!(sigle >= 3, "CPU (tre volte) e GPU sono sigle");
        let atteso = |id: &str, testo: &str| assert_eq!(visti.iter().find(|k| k.id == id).unwrap().narratore.as_str(), testo);
        let m = misure(Some(&s), true, true);
        assert_eq!(m[0].narratore.as_str(), "CPU, 12 %, GPU 37 %");
        assert_eq!(m[1].narratore.as_str(), "Memoria, 48 %, 7,7 di 16 GB");
        assert_eq!(m[2].narratore.as_str(), "Disco C:, 182 GB liberi su 476 GB");
        assert_eq!(m[3].narratore.as_str(), "Batteria, 84 %, In carica");
        atteso("gpu", "GPU, 37 %");
        assert_eq!(misure(None, false, false)[0].narratore.as_str(), "CPU, lettura in corso");

        let r = righe(&tutto());
        for k in &r {
            assert!(k.narratore.chars().any(char::is_lowercase), "{}", k.narratore);
            assert_ne!(k.narratore, k.narratore.to_uppercase());
        }
    }

    /// Simboli sempre SVG: nessun glifo decorativo (frecce, chevron, stelle) nei testi della pagina
    /// (le frecce della velocità le disegna Slint).
    #[test]
    fn testi_senza_glifi_decorativi() {
        let s = StatPc { gpu_pct: Some(37.0), ..con_batteria(84.0, StatoBatteria::InCarica) };
        let mut testi: Vec<String> = vec![];
        for k in misure(Some(&s), true, true).iter().chain(misure(None, false, false).iter()) {
            testi.extend([k.etichetta.to_string(), k.valore.to_string(), k.unita.to_string(), k.dettaglio.to_string(), k.narratore.to_string()]);
        }
        for r in [tutto(), RighePc { windows: false, ..tutto() }, RighePc { errore_radio: true, wifi: Some((false, false)), ..tutto() }] {
            for k in righe(&r) {
                testi.extend([k.nome.to_string(), k.valore.to_string(), k.errore.to_string(), k.narratore.to_string()]);
            }
        }
        let r1 = R1 { fatto: "Tutto regolare".into(), valore: String::new() };
        for v in [Some((0.4, false)), Some((0.4, true)), None] {
            let s = sintesi(&r1, v);
            testi.extend([s.titolo.to_string(), s.sotto.to_string(), s.azione_testo.to_string()]);
        }
        let primo = stato(None, false);
        testi.extend([primo.frase.to_string(), primo.sotto.to_string()]);
        assert!(testi.len() > 40);
        for t in &testi {
            assert!(!t.chars().any(|c| "↑↓←→›‹★☆•▸▾▴".contains(c)), "glifo decorativo in «{t}»");
        }
    }
}
