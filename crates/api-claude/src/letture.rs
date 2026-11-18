//! L'elenco CHIUSO delle letture: ogni variante di `Lettura` ha un percorso fisso, tutti `GET`.
//! Non esiste altro modo di chiedere qualcosa alle API: la chiave da amministratore non ha
//! permessi ridotti, quindi la difesa è questa (e il test `percorsi_solo_nell_elenco_chiuso`).
//!
//! Verificato sulla documentazione il 18/11/2026 (platform.claude.com):
//! - `/docs/en/api/beta/organization/analytics/cost/list` e `.../analytics/usage/list`:
//!   `GET /v1/organizations/analytics/{cost,usage}`; intestazione obbligatoria
//!   `anthropic-beta: analytics-usage-cost-2026-08-24`; parametri `starting_at`, `ending_at`
//!   (richiesto con `time_bucket_width`, al più 31 giorni), `time_bucket_width` (`1d`|`1h`|`1m`),
//!   `group_by[]` (`model`, `api_key_id`, `workspace_id`…), `order_by` (`time`|`amount`…),
//!   `limit` 1-1000, `page`; risposta `data[]` di RIGHE (non di «bucket con results»):
//!   `starting_at`, `ending_at`, `group{…}`, `cost{amount,list_amount,requests}` oppure
//!   `usage{uncached_input_tokens,output_tokens,cache_read_input_tokens,cache_creation{…},requests,total_tokens}`;
//!   `data_refreshed_at` (nullo se nessun export copre l'intervallo), `has_more`, `next_page`
//!   (cursore scaduto = HTTP 410). `amount` = centesimi frazionari come testo («41280.000000» = 412,80 $).
//! - Admin API: `/docs/en/api/beta/organization/{retrieve,api_keys/list,workspaces/list,users/list,rate_limits/list}`:
//!   `GET /v1/organizations/{me,api_keys,workspaces,users,rate_limits}`; `anthropic-version: 2026-10-21`
//!   (negli esempi della documentazione); chiave nell'intestazione `x-api-key`
//!   (`/docs/en/manage-claude/admin-api-keys`, «Use the key»); elenchi con `limit` 1-1000,
//!   `after_id`, `has_more`, `last_id`; `workspaces` con `include_default`; `rate_limits` con
//!   `limit`/`page`/`next_page` (senza `limit` torna tutto in una pagina).
//! - La chiave `sk-ant-admin01-…` della Console non ha ambiti da scegliere (accesso pieno).
//! - Differenza dal piano: negli esempi la chiave appare come `Authorization: Bearer`; la pagina
//!   «Create an Admin API key» dice `x-api-key`: si usa `x-api-key`, come da brief.

use chrono::{DateTime, SecondsFormat, Utc};

/// Per cosa spezzare una lettura analitica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raggruppa {
    Nessuno,
    Modello,
    Chiave,
    Workspace,
}

impl Raggruppa {
    fn dimensione(self) -> Option<&'static str> {
        match self {
            Raggruppa::Nessuno => None,
            Raggruppa::Modello => Some("model"),
            Raggruppa::Chiave => Some("api_key_id"),
            Raggruppa::Workspace => Some("workspace_id"),
        }
    }
}

/// Le uniche letture possibili.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lettura {
    Organizzazione,
    Costo {
        da: DateTime<Utc>,
        a: DateTime<Utc>,
        per: Raggruppa,
        giorni: bool,
        pagina: Option<String>,
    },
    Uso {
        da: DateTime<Utc>,
        a: DateTime<Utc>,
        per: Raggruppa,
        giorni: bool,
        pagina: Option<String>,
    },
    Chiavi {
        dopo: Option<String>,
    },
    Workspace {
        dopo: Option<String>,
    },
    Membri {
        dopo: Option<String>,
    },
    LimitiVelocita,
}

/// Tutti i percorsi che l'assistente può chiedere. Un solo posto, un solo elenco.
pub const PERCORSI_AMMESSI: [&str; 7] = [
    "/v1/organizations/me",
    "/v1/organizations/analytics/cost",
    "/v1/organizations/analytics/usage",
    "/v1/organizations/api_keys",
    "/v1/organizations/workspaces",
    "/v1/organizations/users",
    "/v1/organizations/rate_limits",
];

/// Righe per pagina degli elenchi (massimo della documentazione: 1000).
const RIGHE_ELENCO: &str = "100";
/// Righe per pagina delle letture analitiche (massimo della documentazione).
const RIGHE_ANALITICA: &str = "1000";

fn istante(t: &DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

impl Lettura {
    pub fn percorso(&self) -> &'static str {
        match self {
            Lettura::Organizzazione => PERCORSI_AMMESSI[0],
            Lettura::Costo { .. } => PERCORSI_AMMESSI[1],
            Lettura::Uso { .. } => PERCORSI_AMMESSI[2],
            Lettura::Chiavi { .. } => PERCORSI_AMMESSI[3],
            Lettura::Workspace { .. } => PERCORSI_AMMESSI[4],
            Lettura::Membri { .. } => PERCORSI_AMMESSI[5],
            Lettura::LimitiVelocita => PERCORSI_AMMESSI[6],
        }
    }

    /// Le letture analitiche (beta) vogliono l'intestazione `anthropic-beta`.
    pub fn analitica(&self) -> bool {
        matches!(self, Lettura::Costo { .. } | Lettura::Uso { .. })
    }

    /// I parametri della richiesta, nell'ordine in cui si scrivono (il client li codifica).
    pub fn query(&self) -> Vec<(&'static str, String)> {
        match self {
            Lettura::Organizzazione | Lettura::LimitiVelocita => Vec::new(),
            Lettura::Costo {
                da,
                a,
                per,
                giorni,
                pagina,
            }
            | Lettura::Uso {
                da,
                a,
                per,
                giorni,
                pagina,
            } => {
                let mut q = vec![("starting_at", istante(da)), ("ending_at", istante(a))];
                if let Some(d) = per.dimensione() {
                    q.push(("group_by[]", d.to_owned()));
                }
                if *giorni {
                    q.push(("time_bucket_width", "1d".to_owned()));
                    q.push(("order_by", "time".to_owned()));
                }
                q.push(("limit", RIGHE_ANALITICA.to_owned()));
                if let Some(p) = pagina {
                    q.push(("page", p.clone()));
                }
                q
            }
            Lettura::Chiavi { dopo } => elenco(dopo, false),
            Lettura::Workspace { dopo } => elenco(dopo, true),
            Lettura::Membri { dopo } => elenco(dopo, false),
        }
    }
}

fn elenco(dopo: &Option<String>, con_predefinito: bool) -> Vec<(&'static str, String)> {
    let mut q = vec![("limit", RIGHE_ELENCO.to_owned())];
    if con_predefinito {
        q.push(("include_default", "true".to_owned()));
    }
    if let Some(d) = dopo {
        q.push(("after_id", d.clone()));
    }
    q
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::TimeZone;
    // `tutte` serve solo qui; nessun altro modulo la usa.

    fn t(g: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, g, h, 0, 0).unwrap()
    }

    pub(crate) fn tutte() -> Vec<Lettura> {
        vec![
            Lettura::Organizzazione,
            Lettura::Costo {
                da: t(1, 0),
                a: t(17, 10),
                per: Raggruppa::Modello,
                giorni: false,
                pagina: None,
            },
            Lettura::Costo {
                da: t(9, 0),
                a: t(17, 0),
                per: Raggruppa::Nessuno,
                giorni: true,
                pagina: Some("p".into()),
            },
            Lettura::Uso {
                da: t(1, 0),
                a: t(17, 10),
                per: Raggruppa::Chiave,
                giorni: false,
                pagina: None,
            },
            Lettura::Uso {
                da: t(1, 0),
                a: t(17, 10),
                per: Raggruppa::Workspace,
                giorni: true,
                pagina: None,
            },
            Lettura::Chiavi { dopo: None },
            Lettura::Workspace {
                dopo: Some("x".into()),
            },
            Lettura::Membri { dopo: None },
            Lettura::LimitiVelocita,
        ]
    }

    #[test]
    fn percorsi_solo_nell_elenco_chiuso() {
        for l in tutte() {
            assert!(PERCORSI_AMMESSI.contains(&l.percorso()), "{l:?}");
        }
        // Ogni voce dell'elenco è raggiunta da qualche variante e non ce ne sono doppie.
        for p in PERCORSI_AMMESSI {
            assert!(
                tutte().iter().any(|l| l.percorso() == p),
                "{p} non è raggiunto"
            );
        }
    }

    #[test]
    fn analitica_solo_costo_e_uso() {
        for l in tutte() {
            assert_eq!(l.analitica(), l.percorso().contains("/analytics/"), "{l:?}");
        }
    }

    #[test]
    fn parametri_come_da_documentazione() {
        let l = Lettura::Costo {
            da: t(9, 0),
            a: t(17, 0),
            per: Raggruppa::Nessuno,
            giorni: true,
            pagina: None,
        };
        assert_eq!(
            l.query(),
            vec![
                ("starting_at", "2026-11-09T00:00:00Z".to_owned()),
                ("ending_at", "2026-11-17T00:00:00Z".to_owned()),
                ("time_bucket_width", "1d".to_owned()),
                ("order_by", "time".to_owned()),
                ("limit", "1000".to_owned()),
            ]
        );
        let l = Lettura::Uso {
            da: t(1, 0),
            a: t(17, 10),
            per: Raggruppa::Chiave,
            giorni: false,
            pagina: Some("pg".into()),
        };
        let q = l.query();
        assert!(q.contains(&("group_by[]", "api_key_id".to_owned())));
        assert!(q.contains(&("page", "pg".to_owned())));
        assert!(!q.iter().any(|(k, _)| *k == "time_bucket_width"));
        assert!(Lettura::Workspace { dopo: None }
            .query()
            .contains(&("include_default", "true".to_owned())));
        assert!(Lettura::Chiavi {
            dopo: Some("k".into())
        }
        .query()
        .contains(&("after_id", "k".to_owned())));
        assert!(Lettura::Membri { dopo: None }
            .query()
            .contains(&("limit", "100".to_owned())));
        assert!(Lettura::Organizzazione.query().is_empty());
    }
}
