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
//! sotto il 10% e scollegata (attenzione), Home Assistant irraggiungibile da oltre 5 minuti
//! (attenzione, «Riprova»), e `--prova-avviso <livello>`.

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

    /// Nome per l'interfaccia (colore della barretta) e parola sotto il titolo.
    pub fn come_stringa(self) -> &'static str {
        match self {
            Livello::Bloccante => "bloccante",
            Livello::Attenzione => "attenzione",
            Livello::Informazione => "informazione",
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
    Prova,
}

impl Fonte {
    /// Nome per l'interfaccia (icona) e parola dopo il livello («attenzione · pc»).
    pub fn come_stringa(self) -> &'static str {
        match self {
            Fonte::Casa => "casa",
            Fonte::Pc => "pc",
            Fonte::Prova => "prova",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulsante {
    pub id: String,
    pub testo: String,
}

impl Pulsante {
    pub fn new(id: &str, testo: &str) -> Pulsante {
        Pulsante { id: id.into(), testo: testo.into() }
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
/// creazione (un aggiornamento non fa ripartire i 20 s della pillola larga).
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
                v.creata = x.creata;
                *x = v;
            }
            None => self.voci.push(v),
        }
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
    if let Some(b) = ordinate.iter().find(|v| v.livello == Livello::Bloccante) {
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

// ─── Fonti di M1 ─────────────────────────────────────────────────────────────

pub const ID_CASA_OFFLINE: &str = "casa.connessione";
pub const PULSANTE_RIPROVA: &str = "riprova";
pub const ID_BATTERIA: &str = "pc.batteria";
pub const PULSANTE_ARCHIVIA: &str = "archivia";
/// Prefisso delle voci delle notifiche persistenti di HA: `casa.notifica.<notification_id>`.
pub const PREFISSO_NOTIFICA: &str = "casa.notifica.";
pub const ID_PROVA: &str = "prova.avviso";

/// Dopo quanto tempo senza Home Assistant la cosa diventa un'attenzione.
pub const CASA_OFFLINE_DOPO: Duration = Duration::from_secs(5 * 60);
/// Batteria sotto questa percentuale e scollegata = attenzione.
pub const SOGLIA_BATTERIA: f32 = 10.0;
/// Una voce della batteria già presente sparisce solo sopra questa percentuale.
pub const SOGLIA_BATTERIA_FINE: f32 = 12.0;

/// Tiene il conto di da quanto Home Assistant non è connesso (dall'avvio, se non lo è mai stato).
/// Senza un ambiente configurato non c'è nulla di irraggiungibile: nessuna voce.
#[derive(Debug)]
pub struct SorveglianzaCasa {
    non_connessa_da: Option<Instant>,
    configurata: bool,
}

/// Errore di casa-core quando non c'è un ambiente da cui connettersi.
const NESSUN_AMBIENTE: &str = "nessun ambiente configurato";

impl SorveglianzaCasa {
    pub fn new(ora: Instant) -> Self {
        SorveglianzaCasa { non_connessa_da: Some(ora), configurata: true }
    }

    pub fn ricevi(&mut self, s: &StatoConnessione, ora: Instant) {
        self.configurata = !matches!(s, StatoConnessione::Errore(e) if e == NESSUN_AMBIENTE);
        if *s == StatoConnessione::Connesso {
            self.non_connessa_da = None;
        } else if self.non_connessa_da.is_none() {
            self.non_connessa_da = Some(ora);
        }
    }

    pub fn voce(&self, ora: Instant) -> Option<Voce> {
        let da = self.non_connessa_da.filter(|_| self.configurata)?;
        (ora.saturating_duration_since(da) >= CASA_OFFLINE_DOPO).then(|| Voce {
            id: ID_CASA_OFFLINE.into(),
            livello: Livello::Attenzione,
            fonte: Fonte::Casa,
            titolo: "Home Assistant non raggiungibile".into(),
            dettaglio: "La casa non risponde da più di 5 minuti. Riprovo da solo; puoi anche riprovare adesso.".into(),
            pulsanti: vec![Pulsante::new(PULSANTE_RIPROVA, "Riprova")],
            creata: da + CASA_OFFLINE_DOPO,
        })
    }

    /// Fra quanto la voce comparirà; `None` se è già comparsa o non comparirà.
    pub fn scadenza(&self, ora: Instant) -> Option<Duration> {
        let da = self.non_connessa_da.filter(|_| self.configurata)?;
        let fine = da + CASA_OFFLINE_DOPO;
        (fine > ora).then(|| fine - ora)
    }
}

/// La batteria del PC sotto il 10% e scollegata: un'attenzione. In carica o sopra soglia: nulla.
pub fn voce_batteria(percentuale: f32, scollegata: bool, ora: Instant) -> Option<Voce> {
    voce_batteria_con_isteresi(percentuale, scollegata, false, ora)
}

/// Come `voce_batteria`, ma una voce già presente resta fino al 12% (o finché non è in carica):
/// una batteria che oscilla attorno al 10% non fa comparire e sparire la voce ogni minuto.
pub fn voce_batteria_con_isteresi(percentuale: f32, scollegata: bool, gia_presente: bool, ora: Instant) -> Option<Voce> {
    let soglia = if gia_presente { SOGLIA_BATTERIA_FINE } else { SOGLIA_BATTERIA };
    (scollegata && percentuale < soglia).then(|| Voce {
        id: ID_BATTERIA.into(),
        livello: Livello::Attenzione,
        fonte: Fonte::Pc,
        titolo: format!("Batteria al {}%", percentuale.round() as i32),
        dettaglio: "Collega l'alimentatore: il PC si spegnerà presto.".into(),
        pulsanti: vec![],
        creata: ora,
    })
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

/// `--prova-avviso <livello>`: una voce finta con 2 pulsanti finti (entrambi la tolgono).
pub fn voce_di_prova(livello: Livello, ora: Instant) -> Voce {
    Voce {
        id: ID_PROVA.into(),
        livello,
        fonte: Fonte::Prova,
        titolo: format!("Avviso di prova: {}", livello.come_stringa()),
        dettaglio: "Questo è un avviso di prova creato con --prova-avviso. I due pulsanti non fanno nulla: lo tolgono soltanto.".into(),
        pulsanti: vec![Pulsante::new("nega", "Nega"), Pulsante::new("consenti", "Consenti")],
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

/// Aggancio della notifica di sistema (toast): il Task 12 lo collega a `notify-rust`. Si chiama
/// per bloccante, attenzione e notifiche di Home Assistant, una volta per voce nuova.
pub fn notifica_sistema(v: &Voce) {
    let _ = v;
}

/// La notifica di sistema spetta a bloccante e attenzione, e alle notifiche di Home Assistant
/// (come ItsyHome v2); non alle altre informazioni.
pub fn merita_notifica_di_sistema(v: &Voce) -> bool {
    v.livello != Livello::Informazione || v.id.starts_with(PREFISSO_NOTIFICA)
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

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
            .map(|s| Pulsante { id: s.to_lowercase(), testo: (*s).into() })
            .collect();
        let mut a = Attese::default();
        a.metti(v);
        let testi: Vec<&str> = a.ordinate()[0].pulsanti.iter().map(|p| p.testo.as_str()).collect();
        assert_eq!(testi, ["Uno", "Due"]);
    }

    #[test]
    fn ha_offline_cinque_minuti_diventa_attenzione() {
        let t = Instant::now();
        let mut s = SorveglianzaCasa::new(t);
        s.ricevi(&StatoConnessione::Connesso, t);
        s.ricevi(&StatoConnessione::Errore("connessione non riuscita".into()), t);
        assert_eq!(s.voce(t + S(4 * 60 + 59)), None);
        let v = s.voce(t + S(5 * 60 + 1)).expect("voce dopo 5 minuti");
        assert_eq!(v.titolo, "Home Assistant non raggiungibile");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.fonte, Fonte::Casa);
        assert_eq!(v.id, ID_CASA_OFFLINE);
        assert_eq!(v.pulsanti.iter().map(|p| p.testo.as_str()).collect::<Vec<_>>(), ["Riprova"]);
        // Connessione tornata: la voce sparisce.
        s.ricevi(&StatoConnessione::Connesso, t + S(6 * 60));
        assert_eq!(s.voce(t + S(20 * 60)), None);
    }

    #[test]
    fn batteria_sotto_dieci_scollegata() {
        let t = Instant::now();
        let v = voce_batteria(9.0, true, t).expect("9% scollegata");
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.fonte, Fonte::Pc);
        assert!(v.titolo.contains("9%"), "{}", v.titolo);
        assert_eq!(voce_batteria(9.0, false, t), None);
        assert_eq!(voce_batteria(11.0, true, t), None);
    }

    #[test]
    fn batteria_con_isteresi() {
        let t = Instant::now();
        // Già presente: resta all'11% scollegata, sparisce sopra il 12% o in carica.
        assert!(voce_batteria_con_isteresi(11.0, true, true, t).is_some());
        assert_eq!(voce_batteria_con_isteresi(12.5, true, true, t), None);
        assert_eq!(voce_batteria_con_isteresi(5.0, false, true, t), None);
        // Non presente: compare solo sotto il 10%.
        assert_eq!(voce_batteria_con_isteresi(11.0, true, false, t), None);
    }
}
