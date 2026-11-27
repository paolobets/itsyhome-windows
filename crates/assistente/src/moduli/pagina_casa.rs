//! La pagina Casa sull'impianto Comandi (ridisegno F3): filtri, card, scene, sintesi e stato come
//! funzioni pure. Il ponte le chiama coi dati che ha già (menu, stato della connessione, tocchi in
//! corso) e scrive i modelli solo a pagina aperta, solo se cambiano.
//!
//! **Una sola vista ottimistica** (C1): le funzioni che dicono quali luci sono accese
//! (`del_filtro`, `luci_accese_del_filtro`, e per il conto della testata `accese`) ricevono lo stato
//! ottimistico dei tocchi in corso, così una luce appena spenta esce subito da «Luci accese», dal
//! conteggio e da «Spegni tutte», senza aspettare la conferma di Home Assistant.

use super::casa::{gradi, COLLEGA, RIPROVA, SPEGNI};
use super::registro::tono_ui;
use super::{Tono, R1};
use crate::ottimismo::Ottimismo;
use crate::pannello::icona;
use crate::ponte::{
    con_ottimismo, frase_errore, genere, luci_accese_in, luci_accese_ora, preferito, riprovabile, voce_toccabile, Preferito, Tipo,
};
use crate::{CardUi, FiltroUi, SintesiUi, StatoPaginaUi, TipoCardUi, TipoStatoPaginaUi};
use casa_core::eventi::StatoConnessione;
use casa_core::types::{AppEntity, MenuData, Room};
use std::collections::HashMap;

/// «★ Preferiti»: solo se fra i preferiti scelti c'è qualcosa che non è una scena (le scene preferite
/// stanno nella fila «SCENE»).
pub const FILTRO_PREFERITI: &str = "casa.preferiti";
/// «Luci accese»: le luci accese di tutta la casa.
pub const FILTRO_LUCI: &str = "casa.luci";
/// «Tutta la casa»: ogni dispositivo delle stanze.
pub const FILTRO_TUTTA: &str = "casa.tutta";
/// «Altro»: le entità senza stanza (l'`area_id` di quella stanza è vuoto, e un id vuoto si
/// confonderebbe con «nessun filtro»).
pub const FILTRO_ALTRO: &str = "casa.altro";
/// «Spegni la stanza»: l'azione della testata in una stanza.
pub const SPEGNI_STANZA: &str = "casa.spegni-stanza";

/// Le icone dei filtri che non sono una stanza (`FiltroUi.icona`): una stanza ha l'indice in
/// `Icone.stanze` (≥ 0, quello scelto dall'utente o quello predefinito dal nome); «Altro» -1 come
/// nelle Impostazioni (-2 sono le scene, che qui non sono un filtro).
pub const ICONA_ALTRO: i32 = -1;
pub const ICONA_PREFERITI: i32 = -3;
pub const ICONA_LUCI: i32 = -4;
pub const ICONA_TUTTA: i32 = -5;

/// «1 dispositivo», «6 dispositivi».
fn dispositivi(n: usize) -> String {
    if n == 1 { "1 dispositivo".into() } else { format!("{n} dispositivi") }
}

/// L'id con cui una stanza è un filtro: il suo `area_id`, o `FILTRO_ALTRO` per quella senza stanza.
fn id_stanza(r: &Room) -> &str {
    if r.area_id.is_empty() { FILTRO_ALTRO } else { r.area_id.as_str() }
}

/// C'è almeno un preferito che non è una scena (le scene preferite stanno nella fila SCENE): solo
/// allora «Preferiti» è un filtro e la pagina si apre lì.
fn ci_sono_preferiti(menu: &MenuData) -> bool {
    menu.favorites.iter().any(|e| !matches!(e, AppEntity::Scene(_)))
}

/// La stanza di un filtro (`None` per i filtri della casa e per una stanza sparita).
fn stanza_di<'a>(menu: &'a MenuData, filtro: &str) -> Option<&'a Room> {
    menu.rooms.iter().find(|r| id_stanza(r) == filtro)
}

/// I filtri della colonna di sinistra, in quest'ordine: «Preferiti» (solo con un preferito che non
/// è una scena, `ci_sono_preferiti`), «Luci accese» col numero (`accese`: le luci di adesso, con l'ottimismo), «Tutta la
/// casa», poi le stanze nell'ordine salvato col numero dei dispositivi, il filo sopra la prima.
/// `icone`: le icone scelte dall'utente per le stanze (`Config.area_icons`).
pub fn filtri(menu: &MenuData, accese: usize, icone: &HashMap<String, String>) -> Vec<FiltroUi> {
    let semplice = |id: &str, nome: &str, icona: i32| FiltroUi {
        id: id.into(),
        nome: nome.into(),
        icona,
        numero: Default::default(),
        filo: false,
        narratore: nome.into(),
    };
    let mut v = Vec::with_capacity(menu.rooms.len() + 3);
    if ci_sono_preferiti(menu) {
        v.push(semplice(FILTRO_PREFERITI, "Preferiti", ICONA_PREFERITI));
    }
    v.push(FiltroUi {
        numero: accese.to_string().into(),
        narratore: if accese == 0 { "Luci accese, nessuna".into() } else { format!("Luci accese, {accese}").into() },
        ..semplice(FILTRO_LUCI, "Luci accese", ICONA_LUCI)
    });
    v.push(semplice(FILTRO_TUTTA, "Tutta la casa", ICONA_TUTTA));
    for (i, r) in menu.rooms.iter().enumerate() {
        let n = r.entities.len();
        let icona = if r.area_id.is_empty() {
            ICONA_ALTRO
        } else {
            crate::stanze::icona(icone.get(&r.area_id).map(String::as_str), &r.name) as i32
        };
        v.push(FiltroUi {
            id: id_stanza(r).into(),
            nome: r.name.as_str().into(),
            icona,
            numero: n.to_string().into(),
            filo: i == 0,
            narratore: format!("{}, {}", r.name, dispositivi(n)).into(),
        });
    }
    v
}

/// Il filtro con cui si apre la pagina: «Preferiti» se c'è un preferito che non è una scena,
/// altrimenti «Tutta la casa» (decisione di Paolo).
pub fn filtro_iniziale(menu: &MenuData) -> &'static str {
    if ci_sono_preferiti(menu) { FILTRO_PREFERITI } else { FILTRO_TUTTA }
}

/// Il filtro scelto se esiste ancora (una stanza nascosta in Home Assistant sparisce, i preferiti
/// tolti dalle Impostazioni pure), altrimenti quello iniziale.
pub fn filtro_valido(menu: &MenuData, scelto: &str) -> String {
    let c_e = match scelto {
        FILTRO_LUCI | FILTRO_TUTTA => true,
        FILTRO_PREFERITI => ci_sono_preferiti(menu),
        altro => stanza_di(menu, altro).is_some(),
    };
    if c_e { scelto.to_owned() } else { filtro_iniziale(menu).to_owned() }
}

/// Le card di un filtro, scene escluse (stanno nella fila «SCENE»), nell'ordine di Home Assistant e
/// con lo stato ottimistico dei tocchi in corso. In «Luci accese» solo le luci accese e
/// disponibili di adesso: una appena spenta esce subito. Un filtro che non c'è più è vuoto.
pub fn del_filtro(menu: &MenuData, filtro: &str, ottimismo: &Ottimismo<bool>) -> Vec<Preferito> {
    let entita: Vec<&AppEntity> = match filtro {
        FILTRO_LUCI => return luci_accese_ora(menu, ottimismo),
        FILTRO_PREFERITI => menu.favorites.iter().collect(),
        FILTRO_TUTTA => menu.rooms.iter().flat_map(|r| r.entities.iter()).collect(),
        altro => stanza_di(menu, altro).map(|r| r.entities.iter().collect()).unwrap_or_default(),
    };
    entita
        .into_iter()
        .filter(|e| !matches!(e, AppEntity::Scene(_)))
        .map(|e| con_ottimismo(preferito(e), ottimismo))
        .collect()
}

/// Le scene della fila «SCENE»: tutte quelle della casa (il mapper non le lega a una stanza) in
/// ogni filtro tranne «Luci accese», dove la fila non c'è.
pub fn scene(menu: &MenuData, filtro: &str) -> Vec<Preferito> {
    if filtro == FILTRO_LUCI {
        return Vec::new();
    }
    menu.scenes.iter().map(|s| preferito(&AppEntity::Scene(s.clone()))).collect()
}

/// Le luci accese di adesso di un filtro: quelle della stanza se il filtro è una stanza, altrimenti
/// quelle di tutta la casa. È il numero di «{a} accese su {n}» e di «Spegni la stanza».
pub fn luci_accese_del_filtro(menu: &MenuData, filtro: &str, ottimismo: &Ottimismo<bool>) -> usize {
    match stanza_di(menu, filtro) {
        Some(r) => luci_accese_in(menu, Some(&r.area_id), ottimismo).len(),
        None => luci_accese_ora(menu, ottimismo).len(),
    }
}

/// Un sensore numerico, come si legge sulla card: «21,5°» (una temperatura, come la carta), «48 %»
/// (con lo spazio), «0,5 kWh», un numero senza unità com'è. `None` se non è un sensore o il suo
/// valore non è un numero (allora è una lettura con la sua parola).
fn lettura_numerica(p: &Preferito) -> Option<String> {
    if !p.entity_id.starts_with("sensor.") {
        return None;
    }
    let (numero, unita) = p.valore.split_once(' ').unwrap_or((p.valore.as_str(), ""));
    if !numero.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        return None;
    }
    let valore: f64 = numero.replace(',', ".").parse().ok().filter(|v: &f64| v.is_finite())?;
    Some(match unita {
        u if u.starts_with('°') => gradi(valore),
        "" => numero.to_owned(),
        "%" => format!("{} %", valore.round() as i64),
        u => {
            // Al più un decimale, con la virgola, come i gradi.
            let decimi = (valore * 10.0).round() / 10.0;
            let testo = if decimi.fract() == 0.0 { format!("{}", decimi as i64) } else { format!("{decimi:.1}") };
            format!("{} {u}", testo.replace('.', ","))
        }
    })
}

/// La card di un dispositivo. `connesso`: la casa risponde (senza, ogni card è tenue e non
/// toccabile). Tipo, parola e valore:
/// - luce: «Accesa» (con «80 %» se si sa e se si mostra accesa) o «Spenta»; interruttore: «Acceso»
///   o «Spento»; scena: «Scena»;
/// - sensore numerico: lettura col numero grande e la parola = il nome; ogni altra lettura
///   (tapparella, serratura, termostato…): lettura senza valore, la parola è il suo stato;
/// - non disponibile: «Non disponibile», tenue; un comando fallito: «Non riuscito», `errore`.
///
/// Per Narrator: «{nome}, {parola}[, {valore}]», e «, sola lettura» se la card non si tocca mai.
pub fn card(p: &Preferito, connesso: bool) -> CardUi {
    let errore = p.errore.is_some();
    let acceso = p.acceso && p.disponibile && p.tipo == Tipo::Interruttore;
    let numerico = if p.tipo == Tipo::Lettura && p.disponibile && !errore { lettura_numerica(p) } else { None };
    let (parola, valore) = if errore {
        ("Non riuscito".to_owned(), String::new())
    } else if !p.disponibile {
        ("Non disponibile".to_owned(), String::new())
    } else {
        match p.tipo {
            Tipo::Interruttore => {
                let luce = p.entity_id.starts_with("light.");
                let parola = match (luce, acceso) {
                    (true, true) => "Accesa",
                    (true, false) => "Spenta",
                    (false, true) => "Acceso",
                    (false, false) => "Spento",
                };
                // La luminosità solo di una luce che si mostra accesa (con l'ottimismo, anche quella
                // appena spenta ha già perso la sua).
                let valore = p.luminosita.filter(|_| luce && acceso).map(|l| format!("{l} %")).unwrap_or_default();
                (parola.to_owned(), valore)
            }
            Tipo::Scena => ("Scena".to_owned(), String::new()),
            Tipo::Lettura => match numerico.clone() {
                Some(v) => (p.nome.clone(), v),
                None => (p.valore.clone(), String::new()),
            },
        }
    };
    let sola_lettura = p.tipo == Tipo::Lettura;
    // Un sensore numerico ha per parola il nome: per Narrator il nome una volta sola.
    let parola_detta = (numerico.is_none()).then_some(parola.as_str());
    let narratore = [Some(p.nome.as_str()), parola_detta, Some(valore.as_str()), sola_lettura.then_some("sola lettura")]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    CardUi {
        id: p.entity_id.as_str().into(),
        tipo: match p.tipo {
            Tipo::Interruttore => TipoCardUi::Interruttore,
            Tipo::Scena => TipoCardUi::Scena,
            Tipo::Lettura => TipoCardUi::Lettura,
        },
        genere: genere(p),
        nome: p.nome.as_str().into(),
        stato: parola.into(),
        valore: valore.into(),
        acceso,
        toccabile: voce_toccabile(p, connesso),
        tenue: !p.disponibile || !connesso,
        errore,
        iniziale: Default::default(),
        tinta: 0,
        narratore: narratore.into(),
    }
}

/// Una frase dell'azione della testata: (id, testo, primaria, con l'icona).
type Azione = (&'static str, &'static str, bool, bool);

/// La sintesi in cima alla pagina. Il titolo è `r1.fatto` (lo stesso della carta) nei filtri della
/// casa e il nome della stanza in una stanza, col suo «{a} accese su {n}» (o «{n} dispositivi»).
/// `accese`: le luci accese **del filtro come si vedono adesso** (`luci_accese_del_filtro`: con
/// l'ottimismo), per il «Spegni tutte» e il «Spegni la stanza» che spariscono al tocco. Azioni:
/// «Spegni tutte» primaria (bianca, con l'icona) nei filtri della casa, «Spegni la stanza»
/// secondaria in una stanza, «Riprova» o «Collega» secondarie a casa scollegata coi dati, che dice
/// anche «dati delle 10:42» (senza dati l'azione è nello stato della pagina, una volta sola).
/// Niente «aggiornato ora»: servirebbe un orologio che lo rinfresca.
pub fn sintesi(stato: &StatoConnessione, r1: &R1, filtro: &str, menu: Option<&MenuData>, accese: usize, dati_delle: Option<&str>) -> SintesiUi {
    let (tono, titolo, sotto, azione): (Tono, String, String, Option<Azione>) = match (stato, menu) {
        (StatoConnessione::NonConfigurato, _) => (Tono::Vuoto, r1.fatto.clone(), String::new(), None),
        // In connessione con dati vecchi (a ogni tentativo): come scollegata con dati, più sotto.
        (StatoConnessione::InConnessione, None) | (StatoConnessione::Connesso, None) => (Tono::Calmo, r1.fatto.clone(), String::new(), None),
        (StatoConnessione::Connesso, Some(m)) => match stanza_di(m, filtro) {
            Some(r) => {
                let n = r.entities.len();
                let sotto = match accese {
                    0 => dispositivi(n),
                    1 => format!("1 accesa su {n}"),
                    a => format!("{a} accese su {n}"),
                };
                (Tono::Calmo, r.name.clone(), sotto, (accese > 0).then_some((SPEGNI_STANZA, "Spegni la stanza", false, true)))
            }
            None => (Tono::Calmo, r1.fatto.clone(), String::new(), (accese > 0).then_some((SPEGNI, "Spegni tutte", true, true))),
        },
        // Scollegata: Riprova se la casa non risponde, Collega se serve un passo nelle Impostazioni.
        // Senza dati l'azione sta solo nello stato della pagina (fuori servizio, con lo stesso
        // pulsante): un solo «Riprova», come Collegamenti nello stato vuoto (revisione finale di F3).
        _ => {
            let sotto = dati_delle.map_or_else(|| "nessun dato".to_owned(), |ora| format!("dati delle {ora}"));
            let azione = if riprovabile(stato) { (RIPROVA, "Riprova", false, false) } else { (COLLEGA, "Collega", false, false) };
            (Tono::Fermo, r1.fatto.clone(), sotto, menu.map(|_| azione))
        }
    };
    let (azione_id, azione_testo, azione_primaria, con_icona) = azione.unwrap_or(("", "", false, false));
    SintesiUi {
        tono: tono_ui(tono),
        titolo: titolo.into(),
        sotto: sotto.into(),
        azione_id: azione_id.into(),
        azione_testo: azione_testo.into(),
        azione_primaria,
        // L'icona «spegni» (Lucide «power», come nella bozza `tc-casa-luci.png` e
        // `tc-casa-stanza.png`) per «Spegni tutte» e «Spegni la stanza». Mai l'`Image` vuota: non
        // è uguale a se stessa e la sintesi sarebbe sempre «cambiata».
        azione_icona: icona(if con_icona { "spegni" } else { "nessuna" }),
        ..Default::default()
    }
}

/// Lo stato della pagina (spec §5.6): non configurata → un invito con «Collega»; in connessione (o
/// connessa) senza dati → primo avvio; scollegata senza dati → fuori servizio con la frase
/// dell'errore, «Nessun dato» e «Riprova» (o «Collega» se non si può riprovare); con dati, connessa,
/// scollegata o in connessione (a ogni tentativo di riconnessione) → la pagina piena; connessa
/// senza dispositivi → vuoto, senza azione.
pub fn stato(stato: &StatoConnessione, menu: Option<&MenuData>) -> StatoPaginaUi {
    let frase = |tipo, frase: &str, sotto: &str, (azione_id, azione_testo): (&str, &str)| StatoPaginaUi {
        tipo,
        frase: frase.into(),
        sotto: sotto.into(),
        azione_id: azione_id.into(),
        azione_testo: azione_testo.into(),
    };
    match (stato, menu) {
        (StatoConnessione::NonConfigurato, _) => {
            frase(TipoStatoPaginaUi::Vuoto, "Collega la casa", "Qui accendi luci e scene di Home Assistant.", (COLLEGA, "Collega"))
        }
        // Con dati vecchi la pagina resta piena anche mentre ci si riconnette (nessuna griglia che
        // sparisce e torna a ogni tentativo): vale il ramo «scollegata con dati» qui sotto.
        (StatoConnessione::InConnessione, None) | (StatoConnessione::Connesso, None) => {
            frase(TipoStatoPaginaUi::PrimoAvvio, "Mi collego alla casa", "", ("", ""))
        }
        (StatoConnessione::Connesso, Some(m)) => {
            let n: usize = m.rooms.iter().map(|r| r.entities.len()).sum::<usize>() + m.scenes.len();
            if n == 0 {
                frase(TipoStatoPaginaUi::Vuoto, "Nessun dispositivo da mostrare", "Aggiungi dei dispositivi alle stanze in Home Assistant.", ("", ""))
            } else {
                StatoPaginaUi { tipo: TipoStatoPaginaUi::Pieno, ..Default::default() }
            }
        }
        // Scollegata, con gli ultimi dati: la pagina c'è (card tenui) e la testata dice «Scollegata».
        (_, Some(_)) => StatoPaginaUi { tipo: TipoStatoPaginaUi::Pieno, ..Default::default() },
        (scollegata, None) => {
            let intera = match scollegata {
                StatoConnessione::Errore(e) => frase_errore(e),
                StatoConnessione::TokenNonValido => "Token non valido",
                StatoConnessione::TokenMancante => "Token mancante",
                _ => "Casa non connessa",
            };
            let azione = if riprovabile(scollegata) { (RIPROVA, "Riprova") } else { (COLLEGA, "Collega") };
            frase(TipoStatoPaginaUi::FuoriServizio, intera, "Nessun dato", azione)
        }
    }
}

/// La nota sotto le card (`comandi-nota`): solo in «Luci accese», dove le luci spente non compaiono
/// (`card`: quante card ci sono).
pub fn nota(filtro: &str, card: usize) -> &'static str {
    match (filtro, card) {
        (FILTRO_LUCI, 0) => "Nessuna luce accesa: le luci spente non compaiono in questo filtro.",
        (FILTRO_LUCI, _) => "Le luci spente non compaiono in questo filtro.",
        _ => "",
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::moduli::controlla;
    use crate::ponte::test::{interruttore, luce, luce_con_luminosita, luce_non_disponibile, menu, scena, sensore, serratura};
    use crate::{GenereUi, TonoModuloUi};
    use casa_core::types::SceneEntity;
    use std::time::Instant;

    fn nessun_tocco() -> Ottimismo<bool> {
        Ottimismo::default()
    }

    /// La casa di prova: due stanze con una luce 80 % accesa, una spenta, una presa, un sensore, e
    /// una scena. Con `preferiti` il preferito è la luce `light.a`.
    fn casa(preferiti: bool) -> MenuData {
        let mut m = menu(
            if preferiti { vec![luce_con_luminosita("light.a", 80.0)] } else { vec![] },
            vec![
                vec![luce_con_luminosita("light.a", 80.0), luce("light.b", false), interruttore("switch.c", true), sensore("sensor.t", "21.5", Some("°C"))],
                vec![luce("light.d", true), luce_non_disponibile("light.e")],
            ],
        );
        m.rooms[0].name = "Soggiorno".into();
        m.rooms[1].name = "Cucina".into();
        let AppEntity::Scene(s) = scena("scene.sera") else { unreachable!() };
        m.scenes = vec![s];
        m
    }

    fn ids(v: &[Preferito]) -> Vec<&str> {
        v.iter().map(|p| p.entity_id.as_str()).collect()
    }

    /// L'ordine dei filtri, il filo sulla prima stanza, i numeri e i nomi per Narrator; senza
    /// preferiti nessun «Preferiti» e la pagina si apre su «Tutta la casa».
    #[test]
    fn filtri_e_ordine() {
        let m = casa(true);
        let f = filtri(&m, 2, &HashMap::new());
        let visti: Vec<(&str, &str, &str, bool)> = f.iter().map(|f| (f.id.as_str(), f.nome.as_str(), f.numero.as_str(), f.filo)).collect();
        assert_eq!(
            visti,
            [
                (FILTRO_PREFERITI, "Preferiti", "", false),
                (FILTRO_LUCI, "Luci accese", "2", false),
                (FILTRO_TUTTA, "Tutta la casa", "", false),
                ("a0", "Soggiorno", "4", true),
                ("a1", "Cucina", "2", false),
            ]
        );
        let narratori: Vec<&str> = f.iter().map(|f| f.narratore.as_str()).collect();
        assert_eq!(narratori, ["Preferiti", "Luci accese, 2", "Tutta la casa", "Soggiorno, 4 dispositivi", "Cucina, 2 dispositivi"]);
        assert_eq!(filtro_iniziale(&m), FILTRO_PREFERITI);

        let senza = casa(false);
        let f = filtri(&senza, 0, &HashMap::new());
        let id: Vec<&str> = f.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(id, [FILTRO_LUCI, FILTRO_TUTTA, "a0", "a1"], "nessun «Preferiti» se l'utente non ne ha scelti");
        assert_eq!(filtro_iniziale(&senza), FILTRO_TUTTA);
        assert_eq!((f[0].numero.as_str(), f[0].narratore.as_str()), ("0", "Luci accese, nessuna"));

        // Con sole scene fra i preferiti: nessun «Preferiti», si parte da «Tutta la casa»; le scene
        // restano nella fila SCENE.
        let mut solo_scene = casa(false);
        solo_scene.favorites = vec![scena("scene.sera")];
        let id: Vec<String> = filtri(&solo_scene, 0, &HashMap::new()).iter().map(|f| f.id.to_string()).collect();
        assert_eq!(id, [FILTRO_LUCI, FILTRO_TUTTA, "a0", "a1"]);
        assert_eq!(filtro_iniziale(&solo_scene), FILTRO_TUTTA);
        assert_eq!(filtro_valido(&solo_scene, FILTRO_PREFERITI), FILTRO_TUTTA);
        assert_eq!(ids(&scene(&solo_scene, FILTRO_TUTTA)), ["scene.sera"]);
        // Una scena e una luce fra i preferiti: «Preferiti» c'è (per la luce).
        solo_scene.favorites.push(luce("light.a", true));
        assert_eq!(filtro_iniziale(&solo_scene), FILTRO_PREFERITI);
    }

    /// C7: l'icona di un filtro è un indice (le stanze, quella scelta dall'utente o quella
    /// predefinita dal nome) o un codice negativo; «Altro» ha il suo id e non si confonde con «nessun
    /// filtro»; una sola stanza con un dispositivo dice «1 dispositivo».
    #[test]
    fn icone_dei_filtri_e_altro() {
        let mut m = casa(true);
        m.rooms.push(Room { area_id: String::new(), name: "Altro".into(), entities: vec![luce("light.z", false)] });
        let mut icone = HashMap::new();
        icone.insert("a1".to_owned(), "🎮".to_owned());
        let f = filtri(&m, 0, &icone);
        let codici: Vec<i32> = f.iter().map(|f| f.icona).collect();
        assert_eq!(&codici[..3], [ICONA_PREFERITI, ICONA_LUCI, ICONA_TUTTA]);
        assert_eq!(codici[3] as usize, crate::stanze::icona(None, "Soggiorno"), "predefinita dal nome");
        assert_eq!(codici[4] as usize, crate::stanze::indice("🎮").unwrap(), "quella scelta dall'utente");
        assert_eq!(codici[5], ICONA_ALTRO);
        let altro = &f[5];
        assert_eq!((altro.id.as_str(), altro.numero.as_str(), altro.narratore.as_str()), (FILTRO_ALTRO, "1", "Altro, 1 dispositivo"));
        assert!(!altro.id.is_empty());
        // «Altro» è un filtro valido e si apre sulla sua entità.
        assert_eq!(filtro_valido(&m, FILTRO_ALTRO), FILTRO_ALTRO);
        assert_eq!(ids(&del_filtro(&m, FILTRO_ALTRO, &nessun_tocco())), ["light.z"]);
    }

    /// Un filtro che non c'è più (una stanza nascosta in HA, i preferiti tolti) torna all'iniziale.
    #[test]
    fn filtro_sparito_torna_all_iniziale() {
        let m = casa(true);
        assert_eq!(filtro_valido(&m, "a9"), filtro_iniziale(&m));
        assert_eq!(filtro_valido(&m, ""), FILTRO_PREFERITI, "un id vuoto non è un filtro");
        for buono in [FILTRO_PREFERITI, FILTRO_LUCI, FILTRO_TUTTA, "a0", "a1"] {
            assert_eq!(filtro_valido(&m, buono), buono);
        }
        // Senza preferiti «Preferiti» non è più valido e il ripiego è «Tutta la casa».
        let senza = casa(false);
        assert_eq!(filtro_valido(&senza, FILTRO_PREFERITI), FILTRO_TUTTA);
        assert_eq!(filtro_valido(&senza, "a9"), FILTRO_TUTTA);
        // Una stanza che sparisce mentre è il filtro scelto.
        let mut meno = casa(true);
        meno.rooms.remove(1);
        assert_eq!(filtro_valido(&meno, "a1"), FILTRO_PREFERITI);
        assert!(del_filtro(&meno, "a1", &nessun_tocco()).is_empty(), "un filtro che non c'è è vuoto");
    }

    fn pref(e: AppEntity) -> Preferito {
        preferito(&e)
    }

    /// Il tipo, la parola, il valore e il resto di ogni genere, per Narrator compreso.
    #[test]
    fn card_per_genere() {
        // (card, tipo, parola, valore, acceso, toccabile, tenue, narratore)
        let c = |p: &Preferito| card(p, true);
        let riga = |c: &CardUi| (c.tipo, c.stato.to_string(), c.valore.to_string(), c.acceso, c.toccabile, c.tenue, c.narratore.to_string());
        let t = TipoCardUi::Interruttore;
        let l = TipoCardUi::Lettura;

        let accesa = c(&pref(luce_con_luminosita("light.a", 80.0)));
        assert_eq!(riga(&accesa), (t, "Accesa".into(), "80 %".into(), true, true, false, "Nome light.a, Accesa, 80 %".into()));
        assert_eq!(accesa.genere, GenereUi::Luce);
        assert_eq!(c(&pref(luce("light.a", true))).valore, "", "luce accesa senza luminosità nota: solo «Accesa»");
        assert_eq!(riga(&c(&pref(luce("light.b", false)))), (t, "Spenta".into(), "".into(), false, true, false, "Nome light.b, Spenta".into()));
        let presa = c(&pref(interruttore("switch.c", true)));
        assert_eq!(riga(&presa), (t, "Acceso".into(), "".into(), true, true, false, "Nome switch.c, Acceso".into()));
        assert_eq!(presa.genere, GenereUi::Presa);
        assert_eq!(c(&pref(interruttore("switch.c", false))).stato, "Spento");
        let sc = c(&pref(scena("scene.sera")));
        assert_eq!(riga(&sc), (TipoCardUi::Scena, "Scena".into(), "".into(), false, true, false, "Nome scene.sera, Scena".into()));

        // Il sensore numerico: il valore grande, la parola è il nome, mai toccabile.
        let temp = c(&pref(sensore("sensor.t", "21.5", Some("°C"))));
        assert_eq!(riga(&temp), (l, "Nome sensor.t".into(), "21,5°".into(), false, false, false, "Nome sensor.t, 21,5°, sola lettura".into()));
        assert_eq!(temp.genere, GenereUi::Termo);
        assert_eq!(c(&pref(sensore("sensor.t", "21.53", Some("°C")))).valore, "21,5°", "un decimale al più, come la carta");
        assert_eq!(c(&pref(sensore("sensor.t", "9.0", Some("°C")))).valore, "9°");
        assert_eq!(c(&pref(sensore("sensor.t", "-3", Some("°C")))).valore, "-3°");
        let um = c(&pref(sensore("sensor.u", "48", Some("%"))));
        assert_eq!((um.valore.as_str(), um.stato.as_str(), um.genere), ("48 %", "Nome sensor.u", GenereUi::Goccia));
        assert_eq!(c(&pref(sensore("sensor.u", "48.5", Some("%")))).valore, "49 %", "la percentuale è un intero");
        assert_eq!(c(&pref(sensore("sensor.u", "48.37", Some("%")))).valore, "48 %");
        assert_eq!(c(&pref(sensore("sensor.e", "0.5", Some("kWh")))).valore, "0,5 kWh");
        assert_eq!(c(&pref(sensore("sensor.e", "1234.56", Some("kWh")))).valore, "1234,6 kWh", "al più un decimale");
        assert_eq!(c(&pref(sensore("sensor.e", "12.0", Some("W")))).valore, "12 W");
        assert_eq!(c(&pref(sensore("sensor.n", "7", None))).valore, "7");
        // Un sensore con una parola (non un numero) è una lettura senza valore.
        let parola = c(&pref(sensore("sensor.m", "Piove", None)));
        assert_eq!((parola.tipo, parola.stato.as_str(), parola.valore.as_str()), (l, "Piove", ""));
        assert_eq!(parola.narratore, "Nome sensor.m, Piove, sola lettura");

        // Una serratura è una lettura e non si tocca; lo stato è la sua parola.
        let porta = c(&pref(serratura("lock.porta")));
        assert_eq!(riga(&porta), (l, "Chiusa".into(), "".into(), false, false, false, "Nome lock.porta, Chiusa, sola lettura".into()));
        assert_eq!(porta.genere, GenereUi::Casa);

        // Non disponibile: tenue, nessun tocco, nessun valore.
        let spenta = c(&pref(luce_non_disponibile("light.e")));
        assert_eq!(riga(&spenta), (t, "Non disponibile".into(), "".into(), false, false, true, "Nome light.e, Non disponibile".into()));
        assert!(!c(&pref(sensore("sensor.t", "21", Some("°C")))).tenue);
        let mut fuori = pref(sensore("sensor.t", "21", Some("°C")));
        fuori.disponibile = false;
        let fuori = c(&fuori);
        assert_eq!((fuori.stato.as_str(), fuori.valore.as_str(), fuori.tenue), ("Non disponibile", "", true));
        assert_eq!(fuori.narratore, "Nome sensor.t, Non disponibile, sola lettura");

        // Un comando fallito: «Non riuscito», con il segno dell'errore.
        let mut guasta = pref(luce_con_luminosita("light.a", 80.0));
        guasta.errore = Some("La casa non ha confermato il comando".into());
        let guasta = c(&guasta);
        assert_eq!((guasta.stato.as_str(), guasta.valore.as_str(), guasta.errore), ("Non riuscito", "", true));
        assert_eq!(guasta.narratore, "Nome light.a, Non riuscito");
        assert!(!c(&pref(luce("light.a", true))).errore);

        // Casa scollegata: tutte tenui e non toccabili, ma con la loro parola.
        for p in [pref(luce("light.a", true)), pref(interruttore("switch.c", false)), pref(scena("scene.sera")), pref(serratura("lock.porta")), pref(sensore("sensor.t", "21", Some("°C")))] {
            let g = card(&p, false);
            assert!(g.tenue && !g.toccabile, "{}: {g:?}", p.entity_id);
        }
        assert_eq!(card(&pref(luce("light.a", true)), false).stato, "Accesa");
    }

    /// Con l'ottimismo: una luce mostrata spenta non tiene la luminosità, una appena accesa non
    /// ne inventa una (C1).
    #[test]
    fn card_con_l_ottimismo_e_la_luminosita() {
        let m = casa(false);
        let mut o = nessun_tocco();
        o.tocca("light.a", false, &true, Instant::now());
        o.tocca("light.b", true, &false, Instant::now());
        let tutte = del_filtro(&m, FILTRO_TUTTA, &o);
        let a = card(tutte.iter().find(|p| p.entity_id == "light.a").unwrap(), true);
        assert_eq!((a.stato.as_str(), a.valore.as_str(), a.acceso), ("Spenta", "", false), "appena spenta: niente «80 %»");
        let b = card(tutte.iter().find(|p| p.entity_id == "light.b").unwrap(), true);
        assert_eq!((b.stato.as_str(), b.valore.as_str(), b.acceso), ("Accesa", "", true), "appena accesa: nessuna luminosità inventata");
    }

    /// «Luci accese» sono le accese e disponibili nell'ordine delle stanze, senza scene; le scene
    /// sono una fila a parte, in ogni filtro tranne questo.
    #[test]
    fn luci_accese_solo_accese_e_scene_fuori() {
        let m = casa(true);
        let o = nessun_tocco();
        assert_eq!(ids(&del_filtro(&m, FILTRO_LUCI, &o)), ["light.a", "light.d"], "né spente, né non disponibili, né prese");
        assert!(scene(&m, FILTRO_LUCI).is_empty(), "nessuna fila SCENE in «Luci accese»");
        for filtro in [FILTRO_TUTTA, FILTRO_PREFERITI, "a0", "a1"] {
            assert_eq!(ids(&scene(&m, filtro)), ["scene.sera"], "{filtro}: le scene sono di tutta la casa");
        }
        assert_eq!(scene(&m, FILTRO_TUTTA)[0].tipo, Tipo::Scena);
        // Le card: mai le scene, nell'ordine di Home Assistant.
        assert_eq!(ids(&del_filtro(&m, FILTRO_TUTTA, &o)), ["light.a", "light.b", "switch.c", "sensor.t", "light.d", "light.e"]);
        assert_eq!(ids(&del_filtro(&m, "a1", &o)), ["light.d", "light.e"]);
        assert_eq!(ids(&del_filtro(&m, FILTRO_PREFERITI, &o)), ["light.a"]);
        // Una scena fra i preferiti sta nella fila SCENE, non fra le card.
        let mut con_scena = casa(true);
        con_scena.favorites.push(scena("scene.sera"));
        assert_eq!(ids(&del_filtro(&con_scena, FILTRO_PREFERITI, &o)), ["light.a"]);
        assert_eq!(del_filtro(&casa(true), FILTRO_PREFERITI, &o)[0].luminosita, Some(80));
    }

    /// C1: una luce appena spenta esce subito da «Luci accese» e dal conteggio, una appena accesa
    /// entra, prima di ogni conferma di Home Assistant; il conteggio di una stanza segue lo stesso.
    #[test]
    fn luci_accese_esce_subito_col_tocco() {
        let m = casa(true);
        let mut o = nessun_tocco();
        o.tocca("light.a", false, &true, Instant::now());
        assert_eq!(ids(&del_filtro(&m, FILTRO_LUCI, &o)), ["light.d"], "appena spenta: fuori subito");
        assert_eq!(luci_accese_del_filtro(&m, FILTRO_LUCI, &o), 1);
        assert_eq!(luci_accese_del_filtro(&m, "a0", &o), 0, "il conto della stanza segue");
        assert_eq!(luci_accese_del_filtro(&m, "a1", &o), 1);
        assert_eq!(luci_accese_del_filtro(&m, FILTRO_TUTTA, &o), 1, "nei filtri della casa il conto è della casa");
        o.tocca("light.b", true, &false, Instant::now());
        assert_eq!(ids(&del_filtro(&m, FILTRO_LUCI, &o)), ["light.b", "light.d"], "appena accesa: dentro subito");
        assert_eq!(luci_accese_del_filtro(&m, "a0", &o), 1);
        // La conferma (l'attesa tolta) riporta ai dati veri.
        assert_eq!(luci_accese_del_filtro(&m, FILTRO_LUCI, &nessun_tocco()), 2);
        // Il filtro «Luci accese» di un menu senza luci accese è vuoto, con la sua nota.
        let spenti = menu(vec![], vec![vec![luce("light.x", false)]]);
        assert!(del_filtro(&spenti, FILTRO_LUCI, &o).is_empty());
    }

    /// La sintesi per ogni stato: titolo, sotto, azione e se è primaria.
    #[test]
    fn sintesi_per_stato() {
        use StatoConnessione as S;
        let m = casa(true);
        let r1 = |s: &str| R1 { fatto: s.into(), valore: String::new() };
        // Le due icone sono vere e diverse, ognuna uguale a se stessa: un'`Image` vuota non lo è
        // (in Slint non è uguale nemmeno a un'altra vuota) e la sintesi risulterebbe sempre cambiata.
        assert!(icona("spegni") == icona("spegni") && icona("nessuna") == icona("nessuna") && icona("spegni") != icona("nessuna"));
        let v = |s: &SintesiUi| {
            (s.tono, s.titolo.to_string(), s.sotto.to_string(), s.azione_id.to_string(), s.azione_testo.to_string(), s.azione_primaria, s.azione_icona == icona("spegni"))
        };

        // Connessa con 4 accese: «Spegni tutte», primaria, con l'icona (nei tre filtri della casa).
        for filtro in [FILTRO_PREFERITI, FILTRO_LUCI, FILTRO_TUTTA] {
            let s = sintesi(&S::Connesso, &r1("4 luci accese"), filtro, Some(&m), 4, Some("10:42"));
            assert_eq!(v(&s), (TonoModuloUi::Calmo, "4 luci accese".into(), "".into(), SPEGNI.into(), "Spegni tutte".into(), true, true), "{filtro}");
        }
        // Zero accese: nessuna azione.
        let s = sintesi(&S::Connesso, &r1("Luci spente"), FILTRO_TUTTA, Some(&m), 0, None);
        assert_eq!(v(&s), (TonoModuloUi::Calmo, "Luci spente".into(), "".into(), "".into(), "".into(), false, false));

        // Una stanza: il suo nome, «2 accese su 4», «Spegni la stanza» secondaria.
        let s = sintesi(&S::Connesso, &r1("4 luci accese"), "a0", Some(&m), 2, None);
        assert_eq!(v(&s), (TonoModuloUi::Calmo, "Soggiorno".into(), "2 accese su 4".into(), SPEGNI_STANZA.into(), "Spegni la stanza".into(), false, true));
        assert_eq!(sintesi(&S::Connesso, &r1("x"), "a0", Some(&m), 1, None).sotto, "1 accesa su 4");
        // Una stanza senza luci accese: i dispositivi, nessuna azione.
        let s = sintesi(&S::Connesso, &r1("Luci spente"), "a0", Some(&m), 0, None);
        assert_eq!(v(&s), (TonoModuloUi::Calmo, "Soggiorno".into(), "4 dispositivi".into(), "".into(), "".into(), false, false));

        // Scollegata con dati: «Scollegata», «dati delle 10:42», «Riprova» (errore) o «Collega».
        let s = sintesi(&S::Errore("timeout".into()), &r1("Scollegata"), "a0", Some(&m), 2, Some("10:42"));
        assert_eq!(v(&s), (TonoModuloUi::Fermo, "Scollegata".into(), "dati delle 10:42".into(), RIPROVA.into(), "Riprova".into(), false, false));
        let s = sintesi(&S::Disconnesso, &r1("Scollegata"), FILTRO_TUTTA, Some(&m), 4, Some("10:42"));
        assert_eq!((s.azione_id.as_str(), s.azione_testo.as_str(), s.azione_primaria), (COLLEGA, "Collega", false), "nessun «Spegni» a casa scollegata");
        // Senza dati l'azione sta solo nello stato della pagina: un solo «Riprova» (o «Collega»).
        let s = sintesi(&S::TokenNonValido, &r1("Scollegata"), FILTRO_TUTTA, None, 0, None);
        assert_eq!((s.sotto.as_str(), s.azione_id.as_str(), s.azione_testo.as_str()), ("nessun dato", "", ""));
        assert_eq!(stato(&S::TokenNonValido, None).azione_id, COLLEGA, "l'azione è nello stato");
        let s = sintesi(&S::Errore("timeout".into()), &r1("Scollegata"), FILTRO_TUTTA, None, 0, None);
        assert_eq!((s.tono, s.azione_id.as_str()), (TonoModuloUi::Fermo, ""));
        assert_eq!(stato(&S::Errore("timeout".into()), None).azione_id, RIPROVA, "un solo «Riprova», nello stato");

        // Non configurata e in connessione: nessuna azione (la pagina ha il suo stato).
        let s = sintesi(&S::NonConfigurato, &r1("Da collegare"), FILTRO_TUTTA, None, 0, None);
        assert_eq!(v(&s), (TonoModuloUi::Vuoto, "Da collegare".into(), "".into(), "".into(), "".into(), false, false));
        let s = sintesi(&S::InConnessione, &r1("Mi collego"), FILTRO_TUTTA, None, 0, None);
        assert_eq!((s.tono, s.titolo.as_str(), s.azione_id.as_str()), (TonoModuloUi::Calmo, "Mi collego", ""));
        let s = sintesi(&S::Connesso, &r1("Mi collego"), FILTRO_TUTTA, None, 0, None);
        assert_eq!((s.titolo.as_str(), s.azione_id.as_str()), ("Mi collego", ""));

        // Una stanza sparita (filtro vecchio): la sintesi è quella della casa.
        let s = sintesi(&S::Connesso, &r1("4 luci accese"), "a9", Some(&m), 4, None);
        assert_eq!((s.titolo.as_str(), s.azione_id.as_str()), ("4 luci accese", SPEGNI));
        // I campi di Agenti restano vuoti.
        assert_eq!(s.l1, Default::default());
        assert_eq!((s.spesa.as_str(), s.spesa_nota.as_str()), ("", ""));

        // Gli stessi dati, la stessa sintesi: il ponte la riscrive solo se cambia, quindi una
        // sintesi deve essere uguale a se stessa (anche senza azione e col pulsante «Riprova»).
        let uguali = |stato: &StatoConnessione, filtro: &str, accese: usize| {
            let una = || sintesi(stato, &r1("x"), filtro, Some(&m), accese, Some("10:42"));
            assert_eq!(una(), una(), "{stato:?} {filtro} {accese}");
        };
        uguali(&S::Connesso, FILTRO_TUTTA, 4);
        uguali(&S::Connesso, FILTRO_TUTTA, 0);
        uguali(&S::Connesso, "a0", 2);
        uguali(&S::Errore("x".into()), FILTRO_TUTTA, 0);
    }

    /// La sintesi, come la carta, usa lo stesso testo: `r1.fatto` della carta di Casa (a ogni
    /// stato) è il titolo nei filtri della casa.
    #[test]
    fn titolo_della_sintesi_e_quello_della_carta() {
        use StatoConnessione as S;
        let m = casa(true);
        for (s, menu) in [
            (S::NonConfigurato, None),
            (S::InConnessione, None),
            (S::Connesso, None),
            (S::Connesso, Some(&m)),
            (S::Errore("x".into()), Some(&m)),
            (S::Disconnesso, None),
        ] {
            let r = super::super::casa::riassunto(&s, menu, 2, Some("10:42"));
            let sint = sintesi(&s, &r.r1, FILTRO_TUTTA, menu, 2, Some("10:42"));
            assert_eq!(sint.titolo, r.r1.fatto.as_str(), "{s:?}");
            assert!(!sint.titolo.is_empty());
        }
        // E la sintesi è del Task 2: nessun «Spegni tutte» con titolo «Luci spente».
        let r = super::super::casa::riassunto(&S::Connesso, Some(&m), 0, None);
        let sint = sintesi(&S::Connesso, &r.r1, FILTRO_LUCI, Some(&m), 0, None);
        assert_eq!((sint.titolo.as_str(), sint.azione_id.as_str()), ("Luci spente", ""));
        assert_eq!(controlla("casa", &r), Ok(()));
    }

    /// I quattro stati della pagina (spec §5.6) e la pagina piena.
    #[test]
    fn stato_della_pagina() {
        use StatoConnessione as S;
        let m = casa(true);
        let v = |s: &StatoPaginaUi| (s.tipo, s.frase.to_string(), s.sotto.to_string(), s.azione_id.to_string(), s.azione_testo.to_string());

        // Non configurata: un invito con «Collega».
        assert_eq!(
            v(&stato(&S::NonConfigurato, None)),
            (TipoStatoPaginaUi::Vuoto, "Collega la casa".into(), "Qui accendi luci e scene di Home Assistant.".into(), COLLEGA.into(), "Collega".into())
        );
        // In connessione (o connessa ma senza ancora i dati): primo avvio, nessuna azione.
        for s in [stato(&S::InConnessione, None), stato(&S::Connesso, None)] {
            assert_eq!(v(&s), (TipoStatoPaginaUi::PrimoAvvio, "Mi collego alla casa".into(), "".into(), "".into(), "".into()));
        }
        // Riconnessione con dati vecchi (casa-core dice «in connessione» a ogni tentativo): la pagina
        // resta piena, la griglia non sparisce e non torna.
        assert_eq!(stato(&S::InConnessione, Some(&m)).tipo, TipoStatoPaginaUi::Pieno);
        assert_eq!(stato(&S::InConnessione, Some(&m)), stato(&S::Errore("x".into()), Some(&m)));
        // E la sintesi è quella dello scollegato con dati: «dati delle», «Riprova».
        let r1 = R1 { fatto: "Scollegata".into(), valore: String::new() };
        let con = |s: &S| sintesi(s, &r1, FILTRO_TUTTA, Some(&m), 2, Some("10:42"));
        let s = con(&S::InConnessione);
        assert_eq!((s.sotto.as_str(), s.azione_id.as_str(), s.azione_primaria), ("dati delle 10:42", RIPROVA, false));
        assert_eq!(s, con(&S::Errore("x".into())));
        // Scollegata senza dati: fuori servizio con la frase dell'errore, «Riprova» o «Collega».
        assert_eq!(
            v(&stato(&S::Errore("timeout".into()), None)),
            (TipoStatoPaginaUi::FuoriServizio, "Casa non raggiungibile".into(), "Nessun dato".into(), RIPROVA.into(), "Riprova".into())
        );
        let p = stato(&S::Errore("portachiavi: x".into()), None);
        assert_eq!((p.frase.as_str(), p.azione_id.as_str()), ("Token salvato non leggibile", RIPROVA));
        let p = stato(&S::TokenMancante, None);
        assert_eq!((p.tipo, p.frase.as_str(), p.azione_id.as_str(), p.azione_testo.as_str()), (TipoStatoPaginaUi::FuoriServizio, "Token mancante", COLLEGA, "Collega"));
        let p = stato(&S::Disconnesso, None);
        assert_eq!((p.frase.as_str(), p.azione_id.as_str()), ("Casa non connessa", COLLEGA));
        // Scollegata ma con gli ultimi dati: la pagina c'è, con le card tenui.
        assert_eq!(stato(&S::Errore("timeout".into()), Some(&m)).tipo, TipoStatoPaginaUi::Pieno);
        // Connessa senza dispositivi: vuoto, senza azione. Con dispositivi: pieno.
        let vuota = menu(vec![], vec![]);
        assert_eq!(
            v(&stato(&S::Connesso, Some(&vuota))),
            (TipoStatoPaginaUi::Vuoto, "Nessun dispositivo da mostrare".into(), "Aggiungi dei dispositivi alle stanze in Home Assistant.".into(), "".into(), "".into())
        );
        assert_eq!(stato(&S::Connesso, Some(&m)).tipo, TipoStatoPaginaUi::Pieno);
        // Solo scene: ci sono dispositivi da mostrare.
        let mut solo_scene = menu(vec![], vec![]);
        solo_scene.scenes = vec![SceneEntity { base: match scena("scene.x") { AppEntity::Scene(s) => s.base, _ => unreachable!() } }];
        assert_eq!(stato(&S::Connesso, Some(&solo_scene)).tipo, TipoStatoPaginaUi::Pieno);
    }

    /// La nota sotto le card: solo in «Luci accese», con o senza luci.
    #[test]
    fn nota_solo_in_luci_accese() {
        assert_eq!(nota(FILTRO_LUCI, 3), "Le luci spente non compaiono in questo filtro.");
        assert_eq!(nota(FILTRO_LUCI, 0), "Nessuna luce accesa: le luci spente non compaiono in questo filtro.");
        for f in [FILTRO_TUTTA, FILTRO_PREFERITI, "a0"] {
            assert_eq!(nota(f, 0), "");
            assert_eq!(nota(f, 5), "");
        }
    }

    /// Senza dato nessuno «0 %»: una luce accesa senza luminosità nota, o con una che arrotonda a
    /// zero, ha il valore vuoto.
    #[test]
    fn nessuno_zero_per_assenza() {
        let c = card(&pref(luce("light.a", true)), true);
        assert!(c.valore.is_empty(), "{c:?}");
        assert_eq!(pref(luce_con_luminosita("light.a", 0.2)).luminosita, None);
        assert!(card(&pref(luce_con_luminosita("light.a", 0.2)), true).valore.is_empty(), "0 % non è una luminosità da dire");
    }
}
