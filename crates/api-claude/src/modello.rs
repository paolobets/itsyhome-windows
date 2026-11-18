//! Le risposte, lette nei soli campi che servono (documentazione del 18/11/2026, vedi `letture`).
//! Ogni campo che può mancare ha un valore di ripiego: un campo nuovo o assente non fa fallire la
//! lettura, un testo che non è JSON sì (`ErroreApi::Formato`).

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Una somma di denaro in milionesimi di centesimo: l'API dà i centesimi frazionari con sei
/// decimali come testo («41280.000000» = 412,80 $), così non si passa mai da un `f64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Importo {
    pub micro_centesimi: i128,
}

/// Quanti micro-centesimi fa un centesimo.
const PER_CENTESIMO: i128 = 1_000_000;

/// «41280.000000» → 41 280 000 000. Accetta segno, cifre e al più un punto; oltre le sei cifre
/// decimali tronca. Testo vuoto o con altro dentro → `None`.
pub fn importo_da(s: &str) -> Option<Importo> {
    let s = s.trim();
    let (negativo, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (intera, decimali) = match s.split_once('.') {
        Some((i, d)) => (i, d),
        None => (s, ""),
    };
    if intera.is_empty()
        || !intera.bytes().all(|b| b.is_ascii_digit())
        || !decimali.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    // Un numero enorme non è una somma: i128 ha posto per ben oltre qualunque cifra vera.
    if intera.len() > 24 {
        return None;
    }
    let mut micro: i128 = intera.parse::<i128>().ok()?.checked_mul(PER_CENTESIMO)?;
    let mut cifre: String = decimali.chars().take(6).collect();
    while cifre.len() < 6 {
        cifre.push('0');
    }
    micro = micro.checked_add(cifre.parse::<i128>().ok()?)?;
    Some(Importo {
        micro_centesimi: if negativo { -micro } else { micro },
    })
}

impl Importo {
    /// «412,80 $»: dollari, virgola, centesimi; arrotondato al centesimo (mezzi centesimi via da zero).
    pub fn in_dollari(&self) -> String {
        let m = self.micro_centesimi;
        let assoluto = m.abs();
        let mut centesimi = assoluto / PER_CENTESIMO;
        if assoluto % PER_CENTESIMO >= PER_CENTESIMO / 2 {
            centesimi += 1;
        }
        let segno = if m < 0 && centesimi > 0 { "-" } else { "" };
        format!("{segno}{},{:02} $", centesimi / 100, centesimi % 100)
    }
}

impl std::ops::Add for Importo {
    type Output = Importo;
    fn add(self, o: Importo) -> Importo {
        Importo {
            micro_centesimi: self.micro_centesimi.saturating_add(o.micro_centesimi),
        }
    }
}

// ─── Letture analitiche ───

/// Le dimensioni per cui si è chiesto di spezzare (le altre non si chiedono, restano assenti).
#[derive(Debug, Default, Deserialize)]
pub struct Gruppo {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub api_key_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Costo {
    #[serde(default)]
    pub amount: String,
    #[serde(default)]
    pub requests: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RigaCosto {
    pub starting_at: DateTime<Utc>,
    #[serde(default)]
    pub group: Gruppo,
    #[serde(default)]
    pub cost: Costo,
}

#[derive(Debug, Default, Deserialize)]
pub struct CreazioneCache {
    #[serde(default)]
    pub ephemeral_1h_input_tokens: u64,
    #[serde(default)]
    pub ephemeral_5m_input_tokens: u64,
}

#[derive(Debug, Default, Deserialize)]
pub struct Consumo {
    #[serde(default)]
    pub uncached_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation: CreazioneCache,
    #[serde(default)]
    pub requests: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RigaUso {
    #[serde(default)]
    pub group: Gruppo,
    #[serde(default)]
    pub usage: Consumo,
}

/// Una pagina di una lettura analitica, di costo o di uso.
#[derive(Debug, Deserialize)]
pub struct PaginaAnalitica<R> {
    #[serde(default = "Vec::new")]
    pub data: Vec<R>,
    #[serde(default)]
    pub data_refreshed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub next_page: Option<String>,
}

// ─── Amministrazione ───

#[derive(Debug, Deserialize)]
pub struct Organizzazione {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Chiave {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// Il suggerimento che l'API dà al posto del valore («sk-ant-api03-ABC...wxyz»): si tiene solo
    /// la coda (`coda_suggerimento`), mai il prefisso.
    #[serde(default)]
    pub partial_key_hint: String,
}

/// Le ultime cifre di un suggerimento di chiave: ciò che sta dopo l'ultimo «...» (o «…»); senza
/// puntini, gli ultimi 4 caratteri. Il prefisso (`sk-ant-…`) non esce mai di qui.
pub fn coda_suggerimento(hint: &str) -> String {
    let hint = hint.trim();
    let dopo = hint
        .rsplit_once("...")
        .or_else(|| hint.rsplit_once('…'))
        .map(|(_, coda)| coda.to_owned())
        .unwrap_or_else(|| {
            let v: Vec<char> = hint.chars().collect();
            v[v.len().saturating_sub(4)..].iter().collect()
        });
    let dopo: String = dopo
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if dopo.contains("sk-ant") {
        String::new()
    } else {
        dopo
    }
}

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Membro {
    pub id: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
}

/// Una pagina di un elenco con cursore `after_id` (chiavi, workspace, membri).
#[derive(Debug, Deserialize)]
pub struct PaginaElenco<E> {
    #[serde(default = "Vec::new")]
    pub data: Vec<E>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub last_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct GruppoVelocita {
    #[serde(default, rename = "type")]
    pub tipo: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ValoreLimite {
    #[serde(default, rename = "type")]
    pub tipo: String,
    #[serde(default)]
    pub value: u64,
}

#[derive(Debug, Deserialize)]
pub struct LimiteVelocita {
    #[serde(default)]
    pub group: GruppoVelocita,
    #[serde(default)]
    pub limits: Vec<ValoreLimite>,
}

#[derive(Debug, Deserialize)]
pub struct PaginaLimiti {
    #[serde(default = "Vec::new")]
    pub data: Vec<LimiteVelocita>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn coda_del_suggerimento_senza_prefisso() {
        assert_eq!(coda_suggerimento("sk-ant-api03-FAK...0001"), "0001");
        assert_eq!(
            coda_suggerimento("sk-ant-api03-ABCDEFGHIJ"),
            "GHIJ",
            "senza puntini: gli ultimi 4"
        );
        assert_eq!(coda_suggerimento(""), "");
        assert!(!coda_suggerimento("sk-ant-").contains("sk-ant"));
    }

    #[test]
    fn importo_centesimi_sei_decimali() {
        assert_eq!(importo_da("41280.000000").unwrap().in_dollari(), "412,80 $");
        assert_eq!(importo_da("0.000000").unwrap().in_dollari(), "0,00 $");
        assert_eq!(importo_da("abc"), None);
    }

    #[test]
    fn importo_arrotonda_al_centesimo_e_rifiuta_il_resto() {
        assert_eq!(
            importo_da("0.5").unwrap().in_dollari(),
            "0,01 $",
            "mezzo centesimo sale"
        );
        assert_eq!(importo_da("0.499999").unwrap().in_dollari(), "0,00 $");
        assert_eq!(importo_da("123.78912").unwrap().in_dollari(), "1,24 $");
        assert_eq!(importo_da("-250").unwrap().in_dollari(), "-2,50 $");
        assert_eq!(
            importo_da("12"),
            Some(Importo {
                micro_centesimi: 12_000_000
            })
        );
        for no in ["", ".", "1.2.3", "1e5", "--1", "12 34", ".5"] {
            assert_eq!(importo_da(no), None, "{no:?}");
        }
    }

    #[test]
    fn somma_di_importi() {
        let a = importo_da("30000.000000").unwrap();
        let b = importo_da("11280.000000").unwrap();
        assert_eq!((a + b).in_dollari(), "412,80 $");
    }

    #[test]
    fn campi_assenti_non_fanno_fallire() {
        let p: PaginaAnalitica<RigaCosto> =
            serde_json::from_str(r#"{"data":[],"data_refreshed_at":null}"#).unwrap();
        assert!(
            p.data.is_empty()
                && p.data_refreshed_at.is_none()
                && !p.has_more
                && p.next_page.is_none()
        );
        let r: RigaCosto =
            serde_json::from_str(r#"{"starting_at":"2026-11-01T00:00:00Z","cost":{"amount":"1"}}"#)
                .unwrap();
        assert_eq!(r.cost.requests, None);
        assert!(r.group.model.is_none());
    }
}
