//! Il riepilogo: una sequenza CHIUSA di letture (chiavi, workspace, membri, costi, consumi,
//! limiti) messa insieme in una struttura sola. Una lettura che fallisce mette la sua scheda in
//! `non_disponibili` e le altre proseguono; un risultato vuoto o senza `data_refreshed_at` è
//! «nessun dato» (`None`), mai «0 $».

use crate::client::{ClientApi, ErroreApi};
use crate::letture::{Lettura, Raggruppa};
use crate::modello::{
    self, importo_da, Chiave, Importo, Membro, PaginaAnalitica, PaginaElenco, PaginaLimiti,
    RigaCosto, RigaUso,
};
use chrono::{DateTime, Datelike, DurationRound, NaiveDate, NaiveTime, TimeDelta, Utc};
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheda {
    Spesa,
    Token,
    Chiavi,
    WorkspaceMembri,
    Limiti,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Token {
    pub ingresso: u64,
    pub uscita: u64,
    pub cache_lettura: u64,
    pub cache_scrittura: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoChiave {
    pub id: String,
    pub nome: String,
    pub stato: String,
    pub scadenza: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoWorkspace {
    pub id: String,
    pub nome: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoMembro {
    pub id: String,
    pub email: String,
    pub nome: String,
    pub ruolo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GruppoLimiti {
    pub nome: String,
    pub limiti: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Riepilogo {
    pub aggiornato_a: Option<DateTime<Utc>>,
    pub spesa_mese: Option<Importo>,
    pub spesa_ieri: Option<Importo>,
    pub giorni: Vec<(NaiveDate, Importo)>,
    pub per_modello: Vec<(String, Importo)>,
    pub per_chiave: Vec<(String, Importo, Option<u64>)>,
    pub per_workspace: Vec<(String, Importo)>,
    pub token: Option<Token>,
    pub chiamate_mese: Option<u64>,
    pub chiavi: Vec<InfoChiave>,
    pub workspace: Vec<InfoWorkspace>,
    pub membri: Vec<InfoMembro>,
    pub limiti: Vec<GruppoLimiti>,
    pub non_disponibili: Vec<Scheda>,
    /// Il motivo, se NESSUNA scheda è stata letta (chiave non valida, rete assente…).
    pub errore: Option<ErroreApi>,
}

// ─── Il giro ───

/// Quante pagine al massimo per una lettura: oltre, il risultato non si spaccia per completo.
const PAGINE_MASSIME: usize = 10;

/// Le righe di una lettura analitica (tutte le pagine) e da quando sono aggiornate.
struct Righe<R> {
    righe: Vec<R>,
    aggiornato: Option<DateTime<Utc>>,
}

struct Giro<'a> {
    client: &'a ClientApi,
    /// Chiave rifiutata o troppe richieste: non si insiste con le altre letture.
    fermo: Option<ErroreApi>,
    primo_errore: Option<ErroreApi>,
    /// `data_refreshed_at` di ogni lettura analitica riuscita.
    aggiornamenti: Vec<Option<DateTime<Utc>>>,
}

impl Giro<'_> {
    fn leggi<T: DeserializeOwned>(&mut self, l: &Lettura) -> Result<T, ErroreApi> {
        if let Some(e) = self.fermo {
            return Err(e);
        }
        let esito = self
            .client
            .leggi(l)
            .and_then(|corpo| serde_json::from_str::<T>(&corpo).map_err(|_| ErroreApi::Formato));
        if let Err(e) = &esito {
            if matches!(e, ErroreApi::ChiaveNonValida | ErroreApi::TroppeRichieste) {
                self.fermo = Some(*e);
            }
            self.primo_errore.get_or_insert(*e);
        }
        esito
    }

    /// Una lettura analitica con tutte le sue pagine (cursore `next_page`).
    fn analitica<R: DeserializeOwned>(
        &mut self,
        fai: impl Fn(Option<String>) -> Lettura,
    ) -> Result<Righe<R>, ErroreApi> {
        let mut righe = Vec::new();
        let mut aggiornato: Option<DateTime<Utc>> = None;
        let mut prima = true;
        let mut pagina = None;
        for _ in 0..PAGINE_MASSIME {
            let p: PaginaAnalitica<R> = self.leggi(&fai(pagina.take()))?;
            righe.extend(p.data);
            // La più vecchia delle date: i dati valgono fin lì; una pagina senza data li rende senza data.
            aggiornato = match (prima, aggiornato, p.data_refreshed_at) {
                (true, _, d) => d,
                (false, Some(a), Some(d)) => Some(a.min(d)),
                _ => None,
            };
            prima = false;
            if !p.has_more {
                self.aggiornamenti.push(aggiornato);
                return Ok(Righe { righe, aggiornato });
            }
            match p.next_page {
                Some(n) => pagina = Some(n),
                None => return self.sbagliato(),
            }
        }
        self.sbagliato()
    }

    /// Un elenco con cursore `after_id`, tutte le pagine.
    fn elenco<E: DeserializeOwned>(
        &mut self,
        fai: impl Fn(Option<String>) -> Lettura,
    ) -> Result<Vec<E>, ErroreApi> {
        let mut tutti = Vec::new();
        let mut dopo = None;
        for _ in 0..PAGINE_MASSIME {
            let p: PaginaElenco<E> = self.leggi(&fai(dopo.take()))?;
            tutti.extend(p.data);
            if !p.has_more {
                return Ok(tutti);
            }
            match p.last_id {
                Some(i) => dopo = Some(i),
                None => return self.sbagliato(),
            }
        }
        self.sbagliato()
    }

    /// Pagine finite male o troppe: il risultato sarebbe incompleto, quindi la scheda no.
    fn sbagliato<T>(&mut self) -> Result<T, ErroreApi> {
        self.primo_errore.get_or_insert(ErroreApi::Formato);
        Err(ErroreApi::Formato)
    }
}

const SENZA_NOME: &str = "(non indicato)";

fn mezzanotte(d: NaiveDate) -> DateTime<Utc> {
    d.and_time(NaiveTime::MIN).and_utc()
}

/// Somma delle righe di costo; `None` se non c'è nessuna riga o i dati non hanno data.
fn somma(righe: &Righe<RigaCosto>) -> Option<Importo> {
    if righe.righe.is_empty() || righe.aggiornato.is_none() {
        return None;
    }
    Some(
        righe
            .righe
            .iter()
            .filter_map(|r| importo_da(&r.cost.amount))
            .fold(Importo::default(), |a, b| a + b),
    )
}

/// Costo per etichetta (modello, chiave, workspace), nell'ordine dell'API (dal più caro).
fn per_etichetta(
    righe: &Righe<RigaCosto>,
    chi: impl Fn(&RigaCosto) -> Option<&String>,
) -> Vec<(Option<String>, Importo)> {
    if righe.aggiornato.is_none() {
        return Vec::new();
    }
    righe
        .righe
        .iter()
        .filter_map(|r| importo_da(&r.cost.amount).map(|i| (chi(r).cloned(), i)))
        .collect()
}

/// Mette insieme tutto: sequenza chiusa di letture. `adesso` è l'istante del giro (il mese si
/// legge dal primo, in UTC, fino all'ora intera; i giorni sono gli 8 prima di oggi).
pub fn aggiorna(c: &ClientApi, adesso: DateTime<Utc>) -> Riepilogo {
    let mut g = Giro {
        client: c,
        fermo: None,
        primo_errore: None,
        aggiornamenti: Vec::new(),
    };
    let mut r = Riepilogo::default();

    let oggi = adesso.date_naive();
    let primo = NaiveDate::from_ymd_opt(oggi.year(), oggi.month(), 1).unwrap_or(oggi);
    let da_mese = mezzanotte(primo);
    let a_mese = adesso.duration_trunc(TimeDelta::hours(1)).unwrap_or(adesso);
    // Alle 00:xx del primo l'ora intera coincide con l'inizio: nessun mese da chiedere ancora.
    let mese_chiedibile = a_mese > da_mese;
    let da_giorni = mezzanotte(oggi - chrono::Days::new(8));
    let a_giorni = mezzanotte(oggi);

    // Chiavi (e i nomi per le altre schede).
    let chiavi = g.elenco::<Chiave>(|dopo| Lettura::Chiavi { dopo });
    let nomi_chiavi: HashMap<String, String> = match &chiavi {
        Ok(v) => v.iter().map(|k| (k.id.clone(), k.name.clone())).collect(),
        Err(_) => HashMap::new(),
    };
    match chiavi {
        Ok(v) => {
            r.chiavi = v
                .into_iter()
                .map(|k| InfoChiave {
                    id: k.id,
                    nome: k.name,
                    stato: k.status,
                    scadenza: k.expires_at,
                })
                .collect();
        }
        Err(_) => r.non_disponibili.push(Scheda::Chiavi),
    }

    // Workspace e membri.
    let workspace = g.elenco::<modello::Workspace>(|dopo| Lettura::Workspace { dopo });
    let nomi_workspace: HashMap<String, String> = match &workspace {
        Ok(v) => v.iter().map(|w| (w.id.clone(), w.name.clone())).collect(),
        Err(_) => HashMap::new(),
    };
    let membri = g.elenco::<Membro>(|dopo| Lettura::Membri { dopo });
    match (workspace, membri) {
        (Ok(w), Ok(m)) => {
            r.workspace = w
                .into_iter()
                .map(|w| InfoWorkspace {
                    id: w.id,
                    nome: w.name,
                })
                .collect();
            r.membri = m
                .into_iter()
                .map(|m| InfoMembro {
                    id: m.id,
                    email: m.email,
                    nome: m.name,
                    ruolo: m.role,
                })
                .collect();
        }
        _ => r.non_disponibili.push(Scheda::WorkspaceMembri),
    }

    // Spesa: mese per modello, per chiave, per workspace; poi i giorni.
    let costo = |per: Raggruppa, da: DateTime<Utc>, a: DateTime<Utc>, giorni: bool| {
        move |pagina: Option<String>| Lettura::Costo {
            da,
            a,
            per,
            giorni,
            pagina,
        }
    };
    let spesa = (|| -> Result<_, ErroreApi> {
        let (mut modello, mut chiave, mut area) = (None, None, None);
        if mese_chiedibile {
            modello = Some(g.analitica::<RigaCosto>(costo(
                Raggruppa::Modello,
                da_mese,
                a_mese,
                false,
            ))?);
            chiave =
                Some(g.analitica::<RigaCosto>(costo(Raggruppa::Chiave, da_mese, a_mese, false))?);
            area = Some(g.analitica::<RigaCosto>(costo(
                Raggruppa::Workspace,
                da_mese,
                a_mese,
                false,
            ))?);
        }
        let giorni =
            g.analitica::<RigaCosto>(costo(Raggruppa::Nessuno, da_giorni, a_giorni, true))?;
        Ok((modello, chiave, area, giorni))
    })();
    let mut costi_chiave: Vec<(Option<String>, Importo)> = Vec::new();
    match spesa {
        Ok((modello, chiave, area, giorni)) => {
            if let Some(m) = &modello {
                r.spesa_mese = somma(m);
                r.per_modello = per_etichetta(m, |x| x.group.model.as_ref())
                    .into_iter()
                    .map(|(n, i)| (n.unwrap_or_else(|| SENZA_NOME.to_owned()), i))
                    .collect();
            }
            if let Some(k) = &chiave {
                costi_chiave = per_etichetta(k, |x| x.group.api_key_id.as_ref());
            }
            if let Some(w) = &area {
                r.per_workspace = per_etichetta(w, |x| x.group.workspace_id.as_ref())
                    .into_iter()
                    .map(|(id, i)| {
                        (
                            id.map(|id| nomi_workspace.get(&id).cloned().unwrap_or(id))
                                .unwrap_or_else(|| SENZA_NOME.to_owned()),
                            i,
                        )
                    })
                    .collect();
            }
            if giorni.aggiornato.is_some() {
                let mut per_giorno: BTreeMap<NaiveDate, Importo> = BTreeMap::new();
                for riga in &giorni.righe {
                    if let Some(i) = importo_da(&riga.cost.amount) {
                        let e = per_giorno.entry(riga.starting_at.date_naive()).or_default();
                        *e = *e + i;
                    }
                }
                r.giorni = per_giorno.into_iter().collect();
                r.spesa_ieri = r
                    .giorni
                    .iter()
                    .find(|(d, _)| *d == oggi - chrono::Days::new(1))
                    .map(|(_, i)| *i);
            }
        }
        Err(_) => r.non_disponibili.push(Scheda::Spesa),
    }

    // Token e chiamate: mese intero, poi per chiave.
    let uso = |per: Raggruppa| {
        move |pagina: Option<String>| Lettura::Uso {
            da: da_mese,
            a: a_mese,
            per,
            giorni: false,
            pagina,
        }
    };
    let mut chiamate_chiave: HashMap<String, u64> = HashMap::new();
    if mese_chiedibile {
        let token = (|| -> Result<_, ErroreApi> {
            let tutto = g.analitica::<RigaUso>(uso(Raggruppa::Nessuno))?;
            let per_chiave = g.analitica::<RigaUso>(uso(Raggruppa::Chiave))?;
            Ok((tutto, per_chiave))
        })();
        match token {
            Ok((tutto, per_chiave)) => {
                if tutto.aggiornato.is_some() && !tutto.righe.is_empty() {
                    let mut t = Token::default();
                    let mut chiamate: Option<u64> = None;
                    for riga in &tutto.righe {
                        let u = &riga.usage;
                        t.ingresso += u.uncached_input_tokens;
                        t.uscita += u.output_tokens;
                        t.cache_lettura += u.cache_read_input_tokens;
                        t.cache_scrittura += u.cache_creation.ephemeral_1h_input_tokens
                            + u.cache_creation.ephemeral_5m_input_tokens;
                        if let Some(n) = u.requests {
                            chiamate = Some(chiamate.unwrap_or(0) + n);
                        }
                    }
                    r.token = Some(t);
                    r.chiamate_mese = chiamate;
                }
                if per_chiave.aggiornato.is_some() {
                    for riga in &per_chiave.righe {
                        if let (Some(id), Some(n)) = (&riga.group.api_key_id, riga.usage.requests) {
                            *chiamate_chiave.entry(id.clone()).or_default() += n;
                        }
                    }
                }
            }
            Err(_) => r.non_disponibili.push(Scheda::Token),
        }
    }

    // Per chiave: nome (o id se l'elenco delle chiavi non c'è), spesa, chiamate.
    r.per_chiave = costi_chiave
        .into_iter()
        .map(|(id, importo)| match id {
            Some(id) => {
                let chiamate = chiamate_chiave.get(&id).copied();
                (
                    nomi_chiavi.get(&id).cloned().unwrap_or(id),
                    importo,
                    chiamate,
                )
            }
            None => ("(senza chiave)".to_owned(), importo, None),
        })
        .collect();

    // Limiti di velocità.
    match g.leggi::<PaginaLimiti>(&Lettura::LimitiVelocita) {
        Ok(p) => {
            r.limiti = p
                .data
                .into_iter()
                .map(|l| GruppoLimiti {
                    nome: l
                        .group
                        .display_name
                        .filter(|n| !n.is_empty())
                        .unwrap_or(l.group.tipo),
                    limiti: l.limits.into_iter().map(|v| (v.tipo, v.value)).collect(),
                })
                .collect();
        }
        Err(_) => r.non_disponibili.push(Scheda::Limiti),
    }

    r.aggiornato_a = g.aggiornamenti.iter().flatten().min().copied();
    // Se non s'è letto niente, il perché; altrimenti ogni scheda dice da sé cosa manca.
    if r.non_disponibili.len() == 5 || g.fermo.is_some() {
        r.errore = g.fermo.or(g.primo_errore);
    }
    r
}

/// Ieri costa più di 3 volte la media dei 7 giorni prima: solo se i dati di ieri sono completi
/// (aggiornati almeno fino alla fine di ieri), se i 7 giorni ci sono tutti e la loro media non è zero.
pub fn spesa_insolita(r: &Riepilogo, oggi: NaiveDate) -> bool {
    let Some(aggiornato) = r.aggiornato_a else {
        return false;
    };
    if aggiornato < mezzanotte(oggi) {
        return false;
    }
    let giorno = |n: u64| {
        r.giorni
            .iter()
            .find(|(d, _)| *d == oggi - chrono::Days::new(n))
            .map(|(_, i)| i.micro_centesimi)
    };
    let Some(ieri) = giorno(1) else { return false };
    let mut somma: i128 = 0;
    for n in 2..=8 {
        match giorno(n) {
            Some(v) => somma += v,
            None => return false,
        }
    }
    // ieri > 3 × (somma / 7)  equivale a  7 × ieri > 3 × somma, senza divisioni.
    somma > 0 && ieri.saturating_mul(7) > somma.saturating_mul(3)
}
