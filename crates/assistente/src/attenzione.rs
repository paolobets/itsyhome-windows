//! Attenzione: le voci «In attesa» di tutte le fonti, con i 3 livelli uguali per tutte
//! (spec §3bis e «Precisazioni»), e come la pillola le mostra. Logica pura: il tempo arriva
//! sempre dal chiamante.
//!
//! - *bloccante* (rosso, aspetta te): pillola larga e **ferma** finché la voce c'è;
//! - *attenzione* (ambra, guarda presto): pillola larga per 20 s (o finché non la vedi aprendo il
//!   pannello), poi un **puntino** sotto gli occhi finché la voce è in attesa;
//! - *informazione* (blu, quando vuoi): solo il puntino, l'umore non cambia.
//!
//! Fonti di M1: notifiche persistenti di Home Assistant (informazione, «Archivia»), batteria del PC
//! sotto il 10% e scollegata (attenzione; da M2 anche sotto il 20%: informazione con «Ho capito»), Home Assistant irraggiungibile da oltre 5 minuti
//! (attenzione, «Riprova»), token di Home Assistant rifiutato o mancante (attenzione, subito,
//! «Impostazioni»), e `--prova-avviso <livello>`.

use crate::pc_stats::{LetturaBatteria, SOGLIA_BATTERIA_INFO, SOGLIA_BATTERIA_INFO_FINE};
use casa_core::eventi::StatoConnessione;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Livello {
    Bloccante,
    Attenzione,
    Informazione,
}

impl Livello {
    /// Più alto = più urgente.
    fn peso(self) -> u8 {
        match self {
            Livello::Bloccante => 3,
            Livello::Attenzione => 2,
            Livello::Informazione => 1,
        }
    }

    /// La parola per l'utente (decisione B).
    pub fn parola(self) -> &'static str {
        match self {
            Livello::Bloccante => "Aspetta te",
            Livello::Attenzione => "Guarda presto",
            Livello::Informazione => "Quando vuoi",
        }
    }

    pub fn da_str(s: &str) -> Option<Livello> {
        match s {
            "bloccante" => Some(Livello::Bloccante),
            "attenzione" => Some(Livello::Attenzione),
            "informazione" => Some(Livello::Informazione),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fonte {
    Casa,
    Pc,
    /// Claude Code (M3).
    Claude,
    Prova,
}

impl Fonte {
    /// Nome per l'interfaccia (icona) e parola dopo il livello («attenzione · pc»).
    pub fn come_stringa(self) -> &'static str {
        match self {
            Fonte::Casa => "casa",
            Fonte::Pc => "pc",
            Fonte::Claude => "claude",
            Fonte::Prova => "prova",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulsante {
    pub id: String,
    pub testo: String,
    /// Concede un permesso («Consenti»): nel pannello mai con Invio da solo, solo Ctrl+Invio o
    /// clic; nella pillola solo se il testo della richiesta si legge tutto (decisione A).
    pub delicato: bool,
}

impl Pulsante {
    pub fn new(id: &str, testo: &str) -> Pulsante {
        Pulsante { id: id.into(), testo: testo.into(), delicato: false }
    }

    pub fn delicato(id: &str, testo: &str) -> Pulsante {
        Pulsante { delicato: true, ..Pulsante::new(id, testo) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Voce {
    pub id: String,
    pub livello: Livello,
    pub fonte: Fonte,
    pub titolo: String,
    pub dettaglio: String,
    pub pulsanti: Vec<Pulsante>,
    pub creata: Instant,
}

/// Pulsanti di una voce: stanno anche nella pillola, quindi al massimo due.
pub const MASSIMO_PULSANTI: usize = 2;
/// Quanto resta larga la pillola per una voce di attenzione non ancora vista.
pub const ALLARGATA_PER: Duration = Duration::from_secs(20);

/// Le voci in attesa. `metti` con un id già presente aggiorna la voce e ne tiene l'ora di
/// creazione (un aggiornamento non fa ripartire i 20 s della pillola larga); ma se il livello
/// **sale** (informazione → attenzione, attenzione → bloccante) la voce conta come nuova: riparte
/// l'ora di creazione e non è più «vista», così pillola larga, notifica e suono valgono come per
/// una voce appena arrivata (vale per ogni fonte che riusa lo stesso id fra livelli diversi).
#[derive(Debug, Default)]
pub struct Attese {
    voci: Vec<Voce>,
    /// Voci viste aprendo il pannello: un'attenzione vista non allarga più la pillola.
    viste: HashSet<String>,
}

impl Attese {
    pub fn metti(&mut self, mut v: Voce) {
        v.pulsanti.truncate(MASSIMO_PULSANTI);
        match self.voci.iter_mut().find(|x| x.id == v.id) {
            Some(x) => {
                if v.livello.peso() > x.livello.peso() {
                    self.viste.remove(&v.id);
                } else {
                    v.creata = x.creata;
                }
                *x = v;
            }
            None => self.voci.push(v),
        }
    }

    /// Vera se `v` arriverebbe come novità: id non presente, o livello più alto di quello presente.
    pub fn e_novita(&self, v: &Voce) -> bool {
        self.voce(&v.id).is_none_or(|x| v.livello.peso() > x.livello.peso())
    }

    pub fn togli(&mut self, id: &str) {
        self.voci.retain(|v| v.id != id);
        self.viste.remove(id);
    }

    /// Bloccanti, poi attenzione, poi informazione; a pari livello la più recente prima.
    pub fn ordinate(&self) -> Vec<&Voce> {
        let mut v: Vec<&Voce> = self.voci.iter().collect();
        v.sort_by(|a, b| b.livello.peso().cmp(&a.livello.peso()).then(b.creata.cmp(&a.creata)));
        v
    }

    pub fn piu_urgente(&self) -> Option<&Voce> {
        self.ordinate().into_iter().next()
    }

    pub fn voce(&self, id: &str) -> Option<&Voce> {
        self.voci.iter().find(|v| v.id == id)
    }

    pub fn contiene(&self, id: &str) -> bool {
        self.voce(id).is_some()
    }

    /// Il pannello si è aperto: tutte le voci di adesso sono viste.
    pub fn segna_viste(&mut self) {
        self.viste.extend(self.voci.iter().map(|v| v.id.clone()));
    }

    fn vista(&self, id: &str) -> bool {
        self.viste.contains(id)
    }
}

/// Come sta la pillola (spec §3bis, precisazioni).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AspettoPillola {
    Riposo,
    Puntino,
    Allargata { voce_id: String, ferma: bool },
}

/// Un'attenzione allarga la pillola nei suoi primi 20 s, se non è stata vista.
fn allarga_ancora(a: &Attese, v: &Voce, ora: Instant) -> bool {
    v.livello == Livello::Attenzione && !a.vista(&v.id) && ora.saturating_duration_since(v.creata) < ALLARGATA_PER
}

pub fn aspetto_pillola(attese: &Attese, ora: Instant) -> AspettoPillola {
    let ordinate = attese.ordinate();
    // Fra più bloccanti la pillola chiama per **il più vecchio** (da quanto aspetta); il pannello
    // resta nell'ordine di `ordinate`.
    if let Some(b) = ordinate.iter().filter(|v| v.livello == Livello::Bloccante).min_by_key(|v| v.creata) {
        return AspettoPillola::Allargata { voce_id: b.id.clone(), ferma: true };
    }
    if let Some(a) = ordinate.iter().find(|v| allarga_ancora(attese, v, ora)) {
        return AspettoPillola::Allargata { voce_id: a.id.clone(), ferma: false };
    }
    if ordinate.is_empty() {
        AspettoPillola::Riposo
    } else {
        AspettoPillola::Puntino
    }
}

/// Quanti bloccanti ci sono oltre a quello che la pillola mostra: la pillola aggiunge « +N».
pub fn altri_bloccanti(a: &Attese) -> usize {
    a.voci.iter().filter(|v| v.livello == Livello::Bloccante).count().saturating_sub(1)
}

/// Fra quanto l'aspetto della pillola cambierà da solo (fine dei 20 s di un'attenzione); `None`
/// se nulla scade: solo allora serve una sveglia.
pub fn prossimo_cambio_aspetto(attese: &Attese, ora: Instant) -> Option<Duration> {
    attese
        .voci
        .iter()
        .filter(|v| allarga_ancora(attese, v, ora))
        .map(|v| (v.creata + ALLARGATA_PER).saturating_duration_since(ora))
        .min()
}

/// Il livello più urgente in attesa (per l'umore).
pub fn livello_massimo(attese: &Attese) -> Option<Livello> {
    attese.piu_urgente().map(|v| v.livello)
}

/// Un lavoro in corso (Claude Code, Focus, la prova): la linea della ritirata si riempie con la
/// sua frazione. In M2 lo usa solo `--prova-avanzamento`.
#[derive(Debug, Clone, PartialEq)]
pub struct Avanzamento {
    pub id: String,
    /// 0..1 (fuori scala si ritaglia).
    pub frazione: f32,
    pub livello: Livello,
}

/// Fra gli avanzamenti, quello che la linea mostra: il più urgente, a pari livello il più vecchio.
pub fn avanzamento_mostrato(avanzamenti: &[Avanzamento]) -> Option<&Avanzamento> {
    avanzamenti.iter().fold(None, |scelto: Option<&Avanzamento>, a| match scelto {
        Some(s) if s.livello.peso() >= a.livello.peso() => Some(s),
        _ => Some(a),
    })
}

/// Colore e riempimento della linea della ritirata: il colore è il livello più urgente fra voci
/// e avanzamento (nessuno: neutro); il riempimento è la frazione dell'avanzamento (ritagliata a
/// 0..1), oppure 1.0 (linea piena, come in M1) se non ce n'è uno.
pub fn riempimento_linea(livello_massimo: Option<Livello>, avanzamento: Option<&Avanzamento>) -> (Option<Livello>, f32) {
    let Some(a) = avanzamento else { return (livello_massimo, 1.0) };
    let livello = match livello_massimo {
        Some(l) if l.peso() >= a.livello.peso() => l,
        _ => a.livello,
    };
    // Un NaN non deve arrivare all'interfaccia: vale «vuota».
    let frazione = if a.frazione.is_nan() { 0.0 } else { a.frazione.clamp(0.0, 1.0) };
    (Some(livello), frazione)
}

/// Pixel logici riempiti della linea: la frazione si riscrive in Slint solo se questo cambia.
pub fn pixel_riempiti(frazione: f32) -> u32 {
    (frazione.clamp(0.0, 1.0) * crate::sistema::ritirata::LARGHEZZA_LINEA).round() as u32
}

/// `--prova-avanzamento <secondi>`: la durata della prova.
pub fn avanzamento_di_prova(mut argomenti: impl Iterator<Item = String>) -> Option<u64> {
    while let Some(a) = argomenti.next() {
        if a == "--prova-avanzamento" {
            return argomenti.next()?.parse().ok().filter(|s| *s > 0);
        }
    }
    None
}

// ─── Fonti di M1 ─────────────────────────────────────────────────────────────

pub const ID_CASA_OFFLINE: &str = "casa.connessione";
pub const PULSANTE_RIPROVA: &str = "riprova";
/// Token rifiutato da Home Assistant o assente: si sistema solo nelle Impostazioni.
pub const ID_CASA_TOKEN: &str = "casa.token";
pub const PULSANTE_IMPOSTAZIONI: &str = "impostazioni";
pub const ID_BATTERIA: &str = "pc.batteria";
pub const PULSANTE_ARCHIVIA: &str = "archivia";
/// Prefisso delle voci delle notifiche persistenti di HA: `casa.notifica.<notification_id>`.
pub const PREFISSO_NOTIFICA: &str = "casa.notifica.";
pub const ID_PROVA: &str = "prova.avviso";
/// Pulsante della pillola al posto di un consenso col testo tagliato: apre il pannello sulla voce.
pub const PULSANTE_APRI: &str = "apri";
/// Voci dei servizi di sistema (scorciatoia occupata): «Ho capito» le toglie.
pub const PREFISSO_SISTEMA: &str = "sistema.";
pub const PULSANTE_HO_CAPITO: &str = "ho-capito";
/// Prefisso dell'id delle voci dei dispositivi Bluetooth scarichi («pc.bt.<impronta>»).
pub const PREFISSO_BT: &str = "pc.bt.";

/// Dopo quanto tempo senza Home Assistant la cosa diventa un'attenzione.
pub const CASA_OFFLINE_DOPO: Duration = Duration::from_secs(5 * 60);
/// Batteria sotto questa percentuale e scollegata = attenzione.
pub const SOGLIA_BATTERIA: f32 = 10.0;
/// Una voce della batteria già presente sparisce solo sopra questa percentuale.
pub const SOGLIA_BATTERIA_FINE: f32 = 12.0;

/// Tiene il conto di da quanto Home Assistant non è connesso (dall'avvio, se non lo è mai stato)
/// e decide la voce della casa:
/// - irraggiungibile da oltre 5 minuti (errori, tentativi): «Casa non raggiungibile» (nell'assistente
///   la casa si chiama sempre «Casa»; «Home Assistant» solo nelle Impostazioni e per il token);
/// - token rifiutato o mancante: subito una voce sua (aspettare non serve, si sistema solo nelle
///   Impostazioni);
/// - «Disconnetti» voluto o nessun ambiente configurato: nessuna voce.
#[derive(Debug)]
pub struct SorveglianzaCasa {
    non_connessa_da: Option<Instant>,
    /// Falso dopo un «Disconnetti» voluto o senza ambiente: non c'è nulla da sorvegliare.
    sorvegliata: bool,
    token: Option<ProblemaToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProblemaToken {
    NonValido,
    Mancante,
}

impl SorveglianzaCasa {
    pub fn new(ora: Instant) -> Self {
        SorveglianzaCasa { non_connessa_da: Some(ora), sorvegliata: true, token: None }
    }

    pub fn ricevi(&mut self, s: &StatoConnessione, ora: Instant) {
        // Il tempo passato col token rotto non conta per i 5 minuti: uscendo da un problema di
        // token (token sistemato, nuovo tentativo) il conto riparte da adesso.
        let usciva_dal_token = self.token.is_some();
        self.token = match s {
            StatoConnessione::TokenNonValido => Some(ProblemaToken::NonValido),
            StatoConnessione::TokenMancante => Some(ProblemaToken::Mancante),
            _ => None,
        };
        match s {
            StatoConnessione::Connesso => self.non_connessa_da = None,
            StatoConnessione::Disconnesso | StatoConnessione::NonConfigurato => {
                self.sorvegliata = false;
                self.non_connessa_da = None;
            }
            StatoConnessione::InConnessione | StatoConnessione::Errore(_) => {
                self.sorvegliata = true;
                if usciva_dal_token {
                    self.non_connessa_da = Some(ora);
                } else {
                    self.non_connessa_da.get_or_insert(ora);
                }
            }
            StatoConnessione::TokenNonValido | StatoConnessione::TokenMancante => {
                self.sorvegliata = true;
                self.non_connessa_da.get_or_insert(ora);
            }
        }
    }

    pub fn voce(&self, ora: Instant) -> Option<Voce> {
        if let Some(problema) = self.token {
            let (titolo, dettaglio) = match problema {
                ProblemaToken::NonValido => (
                    "Token di Home Assistant non valido",
                    "Home Assistant ha rifiutato il token di accesso. Creane uno nuovo nel tuo profilo di Home Assistant e scrivilo in Impostazioni › Home Assistant.",
                ),
                ProblemaToken::Mancante => (
                    "Token di Home Assistant mancante",
                    "Per questo ambiente non c'è un token di accesso salvato. Scrivilo in Impostazioni › Home Assistant.",
                ),
            };
            return Some(Voce {
                id: ID_CASA_TOKEN.into(),
                livello: Livello::Attenzione,
                fonte: Fonte::Casa,
                titolo: titolo.into(),
                dettaglio: dettaglio.into(),
                pulsanti: vec![Pulsante::new(PULSANTE_IMPOSTAZIONI, "Impostazioni")],
                creata: self.non_connessa_da.unwrap_or(ora),
            });
        }
        let da = self.non_connessa_da.filter(|_| self.sorvegliata)?;
        (ora.saturating_duration_since(da) >= CASA_OFFLINE_DOPO).then(|| Voce {
            id: ID_CASA_OFFLINE.into(),
            livello: Livello::Attenzione,
            fonte: Fonte::Casa,
            titolo: "Casa non raggiungibile".into(),
            dettaglio: "La casa non risponde da più di 5 minuti. Riprovo da solo; puoi anche riprovare adesso.".into(),
            pulsanti: vec![Pulsante::new(PULSANTE_RIPROVA, "Riprova")],
            creata: da + CASA_OFFLINE_DOPO,
        })
    }

    /// Fra quanto comparirà la voce «non raggiungibile»; `None` se è già comparsa o non comparirà.
    pub fn scadenza(&self, ora: Instant) -> Option<Duration> {
        if self.token.is_some() {
            return None;
        }
        let da = self.non_connessa_da.filter(|_| self.sorvegliata)?;
        let fine = da + CASA_OFFLINE_DOPO;
        (fine > ora).then(|| fine - ora)
    }
}

/// La voce della batteria del PC, con due livelli sullo stesso `ID_BATTERIA`:
/// - scollegata sotto il 10% (12% se la voce è già un'attenzione): *attenzione*, senza pulsanti;
/// - scollegata sotto il 20% (22% se la voce è già presente) e non zittita: *informazione*, con
///   «Ho capito» (zittisce fino alla ricarica);
/// - in carica, piena o da 22% in su: nessuna voce, e il «Ho capito» si dimentica.
///
/// Le due isteresi evitano che una batteria che oscilla attorno alla soglia faccia comparire e
/// sparire la voce ogni minuto.
#[derive(Debug, Default)]
pub struct SorveglianzaBatteria {
    zittita: bool,
}

impl SorveglianzaBatteria {
    /// `presente`: il livello con cui la voce sta ora tra le attese (`None` se non c'è).
    pub fn osserva(&mut self, l: &LetturaBatteria, presente: Option<Livello>, ora: Instant) -> Option<Voce> {
        let p = l.percentuale;
        // Stato sconosciuto (es. portatile con limite di carica che non sta caricando): nessuna
        // voce, ma il «Ho capito» resta: se lo stato oscilla fra Scarica e Sconosciuto l'avviso non
        // rinasce. Il «Ho capito» si dimentica solo con alimentatore sicuramente collegato o ≥ 22%.
        if l.collegata() || p >= SOGLIA_BATTERIA_INFO_FINE {
            self.zittita = false;
            return None;
        }
        if !l.scollegata() {
            return None;
        }
        let soglia_attenzione = if presente == Some(Livello::Attenzione) { SOGLIA_BATTERIA_FINE } else { SOGLIA_BATTERIA };
        let soglia_info = if presente.is_some() { SOGLIA_BATTERIA_INFO_FINE } else { SOGLIA_BATTERIA_INFO };
        let titolo = format!("Batteria al {}%", p.round() as i32);
        if p < soglia_attenzione {
            return Some(Voce {
                id: ID_BATTERIA.into(),
                livello: Livello::Attenzione,
                fonte: Fonte::Pc,
                titolo,
                dettaglio: "Collega l'alimentatore: il PC si spegnerà presto.".into(),
                pulsanti: vec![],
                creata: ora,
            });
        }
        (p < soglia_info && !self.zittita).then(|| Voce {
            id: ID_BATTERIA.into(),
            livello: Livello::Informazione,
            fonte: Fonte::Pc,
            titolo,
            dettaglio: "Collega l'alimentatore quando puoi.".into(),
            pulsanti: vec![Pulsante::new(PULSANTE_HO_CAPITO, "Ho capito")],
            creata: ora,
        })
    }

    /// «Ho capito»: niente più voce informativa finché la batteria non torna in carica (o ≥ 22%).
    pub fn zittisci(&mut self) {
        self.zittita = true;
    }
}

/// Una notifica persistente di Home Assistant: informazione, con «Archivia».
pub fn voce_notifica(id: &str, titolo: &str, messaggio: &str, ora: Instant) -> Voce {
    Voce {
        id: format!("{PREFISSO_NOTIFICA}{id}"),
        livello: Livello::Informazione,
        fonte: Fonte::Casa,
        titolo: titolo.into(),
        dettaglio: messaggio.into(),
        pulsanti: vec![Pulsante::new(PULSANTE_ARCHIVIA, "Archivia")],
        creata: ora,
    }
}

/// `--prova-avviso <livello> [--lungo]`: una voce finta che chiede un permesso con 2 pulsanti
/// finti (entrambi la tolgono). `lungo`: titolo che non sta nella pillola (lì «Nega» + «Apri») e
/// testo più lungo dell'area della voce espansa (si scorre).
pub fn voce_di_prova(livello: Livello, lungo: bool, ora: Instant) -> Voce {
    let (titolo, dettaglio) = if lungo {
        (
            format!("Avviso di prova ({}): consentire a un programma di prova di cancellare la cartella dist e tutto il suo contenuto?", livello.parola().to_lowercase()),
            "Questo è un avviso di prova lungo, creato con --prova-avviso e --lungo. I due pulsanti non fanno nulla: lo tolgono soltanto.\n\nServe a provare che un testo più lungo dello spazio della voce espansa si legge tutto: con la rotella del mouse o con le frecce ↑ e ↓ quando il fuoco è sulla voce, il testo scorre.\n\nNella pillola il titolo è tagliato: per questo al posto di «Consenti» c'è «Apri», che porta qui, dove il testo si legge intero e si decide.\n\nRiga in più per lo scorrimento.\n\nUltima riga dell'avviso di prova.".to_owned(),
        )
    } else {
        (
            format!("Avviso di prova: {}", livello.parola().to_lowercase()),
            "Questo è un avviso di prova creato con --prova-avviso. I due pulsanti non fanno nulla: lo tolgono soltanto.".to_owned(),
        )
    };
    Voce {
        id: ID_PROVA.into(),
        livello,
        fonte: Fonte::Prova,
        titolo,
        dettaglio,
        pulsanti: vec![Pulsante::new("nega", "Nega"), Pulsante::delicato("consenti", "Consenti")],
        creata: ora,
    }
}

/// `--prova-avviso <bloccante|attenzione|informazione>`.
pub fn avviso_di_prova(mut argomenti: impl Iterator<Item = String>) -> Option<Livello> {
    while let Some(a) = argomenti.next() {
        if a == "--prova-avviso" {
            return Livello::da_str(&argomenti.next()?);
        }
    }
    None
}

/// `--dopo <s>`: l'avviso di prova arriva dopo `s` secondi.
pub fn secondi_dopo(mut argomenti: impl Iterator<Item = String>) -> Option<u64> {
    while let Some(a) = argomenti.next() {
        if a == "--dopo" {
            return argomenti.next()?.parse().ok();
        }
    }
    None
}

/// La notifica di sistema (toast, `notify-rust`): per bloccante, attenzione e notifiche di Home
/// Assistant, una volta per voce nuova e solo a pannello chiuso (vedi `toast`).
pub fn notifica_sistema(v: &Voce) {
    let (titolo, dettaglio) = testo_notifica(v);
    crate::sistema::notifiche::mostra(&titolo, &dettaglio);
}

/// Il testo della notifica di sistema. Windows può conservarla nel centro notifiche: per un
/// permesso di Claude Code non porta mai il comando o il percorso, solo la sessione.
pub fn testo_notifica(v: &Voce) -> (String, String) {
    if v.id.starts_with(crate::claude::PREFISSO_PERMESSO) {
        let cartella = v.dettaglio.rsplit_once("Sessione: ").map_or("Claude", |(_, c)| c);
        return (format!("Claude chiede un permesso · {cartella}"), String::new());
    }
    (v.titolo.clone(), v.dettaglio.clone())
}

/// La notifica di sistema spetta a bloccante e attenzione, e alle notifiche di Home Assistant
/// (come ItsyHome v2); non alle altre informazioni.
pub fn merita_notifica_di_sistema(v: &Voce) -> bool {
    v.livello != Livello::Informazione || v.id.starts_with(PREFISSO_NOTIFICA)
}

/// Una voce nuova fa il toast se lo merita e se il pannello è chiuso: a pannello aperto la voce
/// compare subito nella riga «In attesa», che si sta già guardando.
pub fn toast(v: &Voce, pannello_aperto: bool) -> bool {
    !pannello_aperto && merita_notifica_di_sistema(v)
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    fn avanzamento(frazione: f32, livello: Livello) -> Avanzamento {
        Avanzamento { id: "prova".into(), frazione, livello }
    }

    #[test]
    fn linea_piena_senza_avanzamento() {
        assert_eq!(riempimento_linea(Some(Livello::Attenzione), None), (Some(Livello::Attenzione), 1.0));
        assert_eq!(riempimento_linea(None, None), (None, 1.0));
    }

    #[test]
    fn linea_con_avanzamento() {
        let a = avanzamento(0.4, Livello::Informazione);
        assert_eq!(riempimento_linea(None, Some(&a)), (Some(Livello::Informazione), 0.4));
        assert_eq!(riempimento_linea(Some(Livello::Bloccante), Some(&a)), (Some(Livello::Bloccante), 0.4));
        // Voce meno urgente dell'avanzamento: vince l'avanzamento.
        let b = avanzamento(0.4, Livello::Attenzione);
        assert_eq!(riempimento_linea(Some(Livello::Informazione), Some(&b)), (Some(Livello::Attenzione), 0.4));
    }

    #[test]
    fn frazione_fuori_scala_ritagliata() {
        assert_eq!(riempimento_linea(None, Some(&avanzamento(1.7, Livello::Informazione))).1, 1.0);
        assert_eq!(riempimento_linea(None, Some(&avanzamento(-0.2, Livello::Informazione))).1, 0.0);
        assert_eq!(riempimento_linea(None, Some(&avanzamento(f32::NAN, Livello::Informazione))).1, 0.0);
    }

    #[test]
    fn pixel_riempiti_al_pixel() {
        assert_eq!(pixel_riempiti(0.5), 150);
        assert_eq!(pixel_riempiti(0.001), 0);
        assert_eq!(pixel_riempiti(1.0), 300);
        assert_eq!(pixel_riempiti(2.0), 300);
    }

    #[test]
    fn avanzamento_mostrato_il_piu_urgente_poi_il_piu_vecchio() {
        assert_eq!(avanzamento_mostrato(&[]), None);
        let (a, b, c) = (
            Avanzamento { id: "a".into(), ..avanzamento(0.1, Livello::Informazione) },
            Avanzamento { id: "b".into(), ..avanzamento(0.2, Livello::Attenzione) },
            Avanzamento { id: "c".into(), ..avanzamento(0.3, Livello::Attenzione) },
        );
        assert_eq!(avanzamento_mostrato(&[a, b.clone(), c]).map(|x| x.id.as_str()), Some("b"));
    }

    #[test]
    fn flag_prova_avanzamento() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        assert_eq!(avanzamento_di_prova(args(&["--prova-avanzamento", "20"])), Some(20));
        assert_eq!(avanzamento_di_prova(args(&["--prova-avanzamento", "0"])), None);
        assert_eq!(avanzamento_di_prova(args(&["--prova-avanzamento", "x"])), None);
        assert_eq!(avanzamento_di_prova(args(&[])), None);
    }

    #[test]
    fn a_pannello_aperto_niente_toast() {
        let t = Instant::now();
        let b = voce("b", Livello::Bloccante, t);
        assert!(toast(&b, false));
        assert!(!toast(&b, true));
        let ha = voce_notifica("lav", "Lavatrice", "Finita", t);
        assert!(toast(&ha, false));
        assert!(!toast(&ha, true));
        assert!(!toast(&voce("i", Livello::Informazione, t), false));
    }

    pub fn voce(id: &str, livello: Livello, creata: Instant) -> Voce {
        Voce {
            id: id.into(),
            livello,
            fonte: Fonte::Prova,
            titolo: format!("Titolo {id}"),
            dettaglio: String::new(),
            pulsanti: vec![],
            creata,
        }
    }

    fn ids(a: &Attese) -> Vec<&str> {
        a.ordinate().iter().map(|v| v.id.as_str()).collect()
    }

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn ordine_per_livello_poi_recenti() {
        let t = Instant::now();
        let mut a = Attese::default();
        a.metti(voce("info", Livello::Informazione, t + S(5)));
        a.metti(voce("att-vecchia", Livello::Attenzione, t));
        a.metti(voce("blocca", Livello::Bloccante, t));
        a.metti(voce("att-nuova", Livello::Attenzione, t + S(1)));
        assert_eq!(ids(&a), ["blocca", "att-nuova", "att-vecchia", "info"]);
        assert_eq!(a.piu_urgente().map(|v| v.id.as_str()), Some("blocca"));
        a.togli("blocca");
        assert_eq!(a.piu_urgente().map(|v| v.id.as_str()), Some("att-nuova"));
    }

    #[test]
    fn stesso_id_aggiorna_non_duplica() {
        let t = Instant::now();
        let mut a = Attese::default();
        a.metti(voce("x", Livello::Attenzione, t));
        let mut nuova = voce("x", Livello::Attenzione, t + S(30));
        nuova.titolo = "Aggiornata".into();
        a.metti(nuova);
        assert_eq!(a.ordinate().len(), 1);
        assert_eq!(a.ordinate()[0].titolo, "Aggiornata");
    }

    #[test]
    fn bloccante_allarga_ferma_finche_ce() {
        let t = Instant::now();
        let mut a = Attese::default();
        a.metti(voce("info", Livello::Informazione, t));
        a.metti(voce("b", Livello::Bloccante, t));
        let attesa = AspettoPillola::Allargata { voce_id: "b".into(), ferma: true };
        assert_eq!(aspetto_pillola(&a, t), attesa);
        assert_eq!(aspetto_pillola(&a, t + S(10 * 60)), attesa);
        a.togli("b");
        assert_eq!(aspetto_pillola(&a, t + S(10 * 60)), AspettoPillola::Puntino);
    }

    #[test]
    fn la_notifica_di_un_permesso_non_ha_il_comando() {
        let mut e = claude_hook::evento::EventoHook {
            sessione: "a".into(),
            tipo: claude_hook::evento::TipoEvento::PermissionRequest,
            cartella: "progetto".into(),
            strumento: Some("PowerShell".into()),
            notifica: None,
            errore: None,
            riassunto: Some("rm -rf segreto".into()),
        };
        let v = crate::claude::voce_permesso(3, &e, Instant::now());
        assert!(v.titolo.contains("segreto") && v.dettaglio.contains("segreto"), "la voce in pillola resta completa");
        let (titolo, dettaglio) = testo_notifica(&v);
        assert_eq!(titolo, "Claude chiede un permesso · progetto");
        assert!(!titolo.contains("segreto") && !dettaglio.contains("segreto"));
        e.cartella.clear();
        let v = crate::claude::voce_permesso(3, &e, Instant::now());
        assert_eq!(testo_notifica(&v).0, "Claude chiede un permesso · Claude");
        // Le altre voci non cambiano.
        let altra = voce("x", Livello::Attenzione, Instant::now());
        assert_eq!(testo_notifica(&altra), (altra.titolo.clone(), altra.dettaglio.clone()));
    }

    #[test]
    fn pillola_col_bloccante_piu_vecchio_e_piu_n() {
        let t = Instant::now();
        let mut a = Attese::default();
        assert_eq!(altri_bloccanti(&a), 0);
        a.metti(voce("media", Livello::Bloccante, t + S(5)));
        a.metti(voce("vecchia", Livello::Bloccante, t));
        a.metti(voce("nuova", Livello::Bloccante, t + S(9)));
        a.metti(voce("att", Livello::Attenzione, t));
        assert_eq!(aspetto_pillola(&a, t + S(10)), AspettoPillola::Allargata { voce_id: "vecchia".into(), ferma: true });
        assert_eq!(altri_bloccanti(&a), 2, "gli altri due bloccanti, non l'attenzione");
        // Il pannello resta nell'ordine di `ordinate`: la più recente prima.
        assert_eq!(ids(&a), ["nuova", "media", "vecchia", "att"]);
        a.togli("vecchia");
        assert_eq!(aspetto_pillola(&a, t + S(10)), AspettoPillola::Allargata { voce_id: "media".into(), ferma: true });
        assert_eq!(altri_bloccanti(&a), 1);
        a.togli("media");
        assert_eq!(altri_bloccanti(&a), 0);
    }

    #[test]
    fn attenzione_allarga_venti_secondi_poi_puntino() {
        let t = Instant::now();
        let mut a = Attese::default();
        a.metti(voce("a", Livello::Attenzione, t));
        assert_eq!(
            aspetto_pillola(&a, t + Duration::from_millis(19_900)),
            AspettoPillola::Allargata { voce_id: "a".into(), ferma: false }
        );
        assert_eq!(aspetto_pillola(&a, t + Duration::from_millis(20_100)), AspettoPillola::Puntino);
    }

    #[test]
    fn sola_informazione_puntino() {
        let t = Instant::now();
        let mut a = Attese::default();
        a.metti(voce("i", Livello::Informazione, t));
        assert_eq!(aspetto_pillola(&a, t), AspettoPillola::Puntino);
    }

    #[test]
    fn vuoto_riposo() {
        assert_eq!(aspetto_pillola(&Attese::default(), Instant::now()), AspettoPillola::Riposo);
    }

    #[test]
    fn al_massimo_due_pulsanti() {
        let t = Instant::now();
        let mut v = voce("p", Livello::Bloccante, t);
        v.pulsanti = ["Uno", "Due", "Tre"]
            .iter()
            .map(|s| Pulsante::new(&s.to_lowercase(), s))
            .collect();
        let mut a = Attese::default();
        a.metti(v);
        let testi: Vec<&str> = a.ordinate()[0].pulsanti.iter().map(|p| p.testo.as_str()).collect();
        assert_eq!(testi, ["Uno", "Due"]);
    }

    #[test]
    fn livelli_con_le_parole_per_l_utente() {
        // Decisione B: nell'interfaccia le parole, nel codice i nomi interni.
        assert_eq!(Livello::Bloccante.parola(), "Aspetta te");
        assert_eq!(Livello::Attenzione.parola(), "Guarda presto");
        assert_eq!(Livello::Informazione.parola(), "Quando vuoi");
        assert_eq!(voce_di_prova(Livello::Bloccante, false, Instant::now()).titolo, "Avviso di prova: aspetta te");
    }

    #[test]
    fn ha_offline_cinque_minuti_diventa_attenzione() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::Connesso, t);
        s.ricevi(&StatoConnessione::Errore("connessione non riuscita".into()), t);
        assert_eq!(s.voce(t + S(4 * 60 + 59)), None);
        let v = s.voce(t + S(5 * 60 + 1)).expect("voce dopo 5 minuti");
        assert_eq!(v.titolo, "Casa non raggiungibile");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.fonte, Fonte::Casa);
        assert_eq!(v.id, ID_CASA_OFFLINE);
        assert_eq!(v.pulsanti.iter().map(|p| p.testo.as_str()).collect::<Vec<_>>(), ["Riprova"]);
        // Connessione tornata: la voce sparisce.
        s.ricevi(&StatoConnessione::Connesso, t + S(6 * 60));
        assert_eq!(s.voce(t + S(20 * 60)), None);
    }

    #[test]
    fn disconnetti_voluto_nessuna_voce() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::Connesso, t);
        s.ricevi(&StatoConnessione::Disconnesso, t + S(1));
        assert_eq!(s.voce(t + S(30 * 60)), None);
        assert_eq!(s.scadenza(t + S(2)), None);
        // Riconnettendosi la sorveglianza riparte da lì.
        s.ricevi(&StatoConnessione::InConnessione, t + S(60));
        s.ricevi(&StatoConnessione::Errore("Home Assistant non risponde".into()), t + S(61));
        assert_eq!(s.voce(t + S(60 + 4 * 60)), None);
        assert!(s.voce(t + S(60 + 5 * 60 + 1)).is_some());
    }

    #[test]
    fn senza_ambiente_nessuna_voce() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::NonConfigurato, t);
        assert_eq!(s.voce(t + S(30 * 60)), None);
        assert_eq!(s.scadenza(t), None);
    }

    #[test]
    fn token_non_valido_voce_subito_con_parole_sue() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::InConnessione, t);
        s.ricevi(&StatoConnessione::TokenNonValido, t + S(1));
        let v = s.voce(t + S(1)).expect("voce subito, senza aspettare 5 minuti");
        assert_eq!(v.id, ID_CASA_TOKEN);
        assert_eq!(v.titolo, "Token di Home Assistant non valido");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.pulsanti.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), [PULSANTE_IMPOSTAZIONI]);
        // Mai «non raggiungibile» per un token, nemmeno dopo 5 minuti.
        assert_eq!(s.voce(t + S(10 * 60)).map(|v| v.id), Some(ID_CASA_TOKEN.to_owned()));
        assert_eq!(s.scadenza(t + S(2)), None);
        s.ricevi(&StatoConnessione::Connesso, t + S(20));
        assert_eq!(s.voce(t + S(20)), None);
    }

    #[test]
    fn token_mancante_voce_subito() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::TokenMancante, t);
        let v = s.voce(t).expect("voce subito");
        assert_eq!(v.id, ID_CASA_TOKEN);
        assert_eq!(v.titolo, "Token di Home Assistant mancante");
        // Disconnesso apposta: niente voce.
        s.ricevi(&StatoConnessione::Disconnesso, t + S(1));
        assert_eq!(s.voce(t + S(1)), None);
    }

    #[test]
    fn il_tempo_col_token_rotto_non_conta_per_i_cinque_minuti() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::TokenMancante, t);
        // Token sistemato dopo 6 minuti: si riconnette, nessuna «non raggiungibile» subito.
        s.ricevi(&StatoConnessione::InConnessione, t + S(6 * 60));
        assert_eq!(s.voce(t + S(6 * 60)), None);
        assert_eq!(s.scadenza(t + S(6 * 60)), Some(CASA_OFFLINE_DOPO));
    }

    #[test]
    fn dopo_il_token_un_errore_aspetta_i_suoi_cinque_minuti() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::TokenNonValido, t);
        s.ricevi(&StatoConnessione::Errore("Home Assistant non risponde".into()), t + S(6 * 60));
        assert_eq!(s.voce(t + S(6 * 60)), None);
        assert_eq!(s.voce(t + S(11 * 60) - Duration::from_millis(1)), None);
        assert_eq!(s.voce(t + S(11 * 60)).map(|v| v.id), Some(ID_CASA_OFFLINE.to_owned()));
    }

    use crate::pc_stats::StatoBatteria;

    fn lettura(percentuale: f32, stato: StatoBatteria) -> LetturaBatteria {
        LetturaBatteria { percentuale, stato }
    }

    #[test]
    fn batteria_sotto_dieci_scollegata() {
        let t = Instant::now();
        let mut s = SorveglianzaBatteria::default();
        let v = s.osserva(&lettura(9.0, StatoBatteria::Scarica), None, t).expect("9% scollegata");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.fonte, Fonte::Pc);
        assert!(v.titolo.contains("9%"), "{}", v.titolo);
        assert_eq!(s.osserva(&lettura(9.0, StatoBatteria::InCarica), None, t), None);
        // L'11% non è più attenzione: da M2 è l'informazione sotto il 20%.
        let v = s.osserva(&lettura(11.0, StatoBatteria::Scarica), None, t).expect("11% scollegata");
        assert_eq!(v.livello, Livello::Informazione);
    }

    #[test]
    fn batteria_con_isteresi() {
        let t = Instant::now();
        let mut s = SorveglianzaBatteria::default();
        let scarica = |p| lettura(p, StatoBatteria::Scarica);
        // Già un'attenzione: resta all'11% scollegata, sopra il 12% scende a informazione.
        let v = s.osserva(&scarica(11.0), Some(Livello::Attenzione), t).expect("11% già attenzione");
        assert_eq!(v.livello, Livello::Attenzione);
        let v = s.osserva(&scarica(12.5), Some(Livello::Attenzione), t).expect("12,5% già attenzione");
        assert_eq!(v.livello, Livello::Informazione);
        assert_eq!(s.osserva(&lettura(5.0, StatoBatteria::InCarica), Some(Livello::Attenzione), t), None);
        // Non presente: l'attenzione compare solo sotto il 10%.
        let v = s.osserva(&scarica(11.0), None, t).expect("11% nuova");
        assert_eq!(v.livello, Livello::Informazione);
    }

    #[test]
    fn venti_per_cento_e_informazione_con_ho_capito() {
        let mut s = SorveglianzaBatteria::default();
        let v = s.osserva(&lettura(19.0, StatoBatteria::Scarica), None, Instant::now()).expect("19% scollegata");
        assert_eq!(v.livello, Livello::Informazione);
        assert_eq!(v.id, ID_BATTERIA);
        assert_eq!(v.pulsanti, vec![Pulsante::new(PULSANTE_HO_CAPITO, "Ho capito")]);
        assert_eq!(v.titolo, "Batteria al 19%");
        assert_eq!(v.dettaglio, "Collega l'alimentatore quando puoi.");
        // A 20% esatti la voce non parte (il glifo sì).
        assert_eq!(s.osserva(&lettura(20.0, StatoBatteria::Scarica), None, Instant::now()), None);
    }

    #[test]
    fn dieci_per_cento_resta_attenzione() {
        let mut s = SorveglianzaBatteria::default();
        let v = s.osserva(&lettura(9.0, StatoBatteria::Scarica), None, Instant::now()).expect("9% scollegata");
        assert_eq!(v.livello, Livello::Attenzione);
        assert!(v.pulsanti.is_empty());
        // A 10% esatti non è più attenzione (come in M1: «sotto» il 10%).
        let v = s.osserva(&lettura(10.0, StatoBatteria::Scarica), None, Instant::now()).expect("10% scollegata");
        assert_eq!(v.livello, Livello::Informazione);
    }

    #[test]
    fn ho_capito_zittisce_fino_alla_ricarica() {
        let t = Instant::now();
        let mut s = SorveglianzaBatteria::default();
        s.zittisci();
        assert_eq!(s.osserva(&lettura(18.0, StatoBatteria::Scarica), None, t), None);
        let v = s.osserva(&lettura(9.0, StatoBatteria::Scarica), None, t).expect("l'attenzione non si zittisce");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(s.osserva(&lettura(9.0, StatoBatteria::InCarica), None, t), None);
        let v = s.osserva(&lettura(19.0, StatoBatteria::Scarica), None, t).expect("dopo la ricarica torna");
        assert_eq!(v.livello, Livello::Informazione);
    }

    #[test]
    fn stato_sconosciuto_non_dimentica_il_ho_capito() {
        let t = Instant::now();
        let mut s = SorveglianzaBatteria::default();
        s.zittisci();
        assert_eq!(s.osserva(&lettura(18.0, StatoBatteria::Sconosciuto), None, t), None);
        // Tornata a scaricarsi: resta zittita.
        assert_eq!(s.osserva(&lettura(17.0, StatoBatteria::Scarica), None, t), None);
        // Solo l'alimentatore sicuramente collegato la riattiva.
        assert_eq!(s.osserva(&lettura(17.0, StatoBatteria::Piena), None, t), None);
        assert!(s.osserva(&lettura(17.0, StatoBatteria::Scarica), None, t).is_some());
    }

    /// Giro 1 della revisione: 19% (informazione, non zittita) → 9% sullo stesso id: per la
    /// pillola, la notifica e il suono è una voce nuova, come in M1.
    #[test]
    fn dal_venti_al_dieci_per_cento_arriva_come_voce_nuova() {
        let t = Instant::now();
        let mut s = SorveglianzaBatteria::default();
        let mut a = Attese::default();
        let info = s.osserva(&lettura(19.0, StatoBatteria::Scarica), None, t).expect("19%");
        assert!(a.e_novita(&info));
        a.metti(info);
        // Passa del tempo e il pannello viene aperto: la voce informativa è vista e «vecchia».
        a.segna_viste();
        let dopo = t + Duration::from_secs(30 * 60);
        assert_eq!(aspetto_pillola(&a, dopo), AspettoPillola::Puntino);

        let attenzione = s
            .osserva(&lettura(9.0, StatoBatteria::Scarica), a.voce(ID_BATTERIA).map(|v| v.livello), dopo)
            .expect("9%");
        assert_eq!(attenzione.livello, Livello::Attenzione);
        // È una novità: il ponte manda la notifica (`toast`) e `voce_arrivata` solo se lo è.
        assert!(a.e_novita(&attenzione));
        assert!(toast(&attenzione, false));
        a.metti(attenzione);
        assert_eq!(
            aspetto_pillola(&a, dopo),
            AspettoPillola::Allargata { voce_id: ID_BATTERIA.into(), ferma: false },
            "la pillola si allarga come per una voce nuova"
        );
        assert_eq!(aspetto_pillola(&a, dopo + ALLARGATA_PER + Duration::from_secs(1)), AspettoPillola::Puntino);
        // Un aggiornamento dello stesso livello non è una novità e non riparte.
        let ancora = s.osserva(&lettura(8.0, StatoBatteria::Scarica), Some(Livello::Attenzione), dopo).expect("8%");
        assert!(!a.e_novita(&ancora));
    }

    #[test]
    fn la_discesa_di_livello_non_e_una_novita() {
        let t = Instant::now();
        let mut a = Attese::default();
        a.metti(voce("x", Livello::Attenzione, t));
        let giu = voce("x", Livello::Informazione, t + Duration::from_secs(5));
        assert!(!a.e_novita(&giu));
        a.metti(giu);
        assert_eq!(a.voce("x").map(|v| v.creata), Some(t), "l'ora di creazione resta");
    }

    #[test]
    fn isteresi_ventidue() {
        let t = Instant::now();
        let mut s = SorveglianzaBatteria::default();
        let scarica = |p| lettura(p, StatoBatteria::Scarica);
        assert!(s.osserva(&scarica(21.5), Some(Livello::Informazione), t).is_some());
        assert_eq!(s.osserva(&scarica(22.0), Some(Livello::Informazione), t), None);
        assert_eq!(s.osserva(&scarica(21.0), None, t), None);
    }
}
