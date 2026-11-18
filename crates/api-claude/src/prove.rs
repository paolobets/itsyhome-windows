//! Prove contro il server finto: nessuna rete vera, nessuna chiave vera.

use crate::letture::PERCORSI_AMMESSI;
use crate::modello::importo_da;
use crate::riepilogo::{aggiorna, spesa_insolita, Riepilogo, Scheda};
use crate::server_finto::{fixture, gestore_fixture, ServerFinto};
use crate::{finto, ChiaveAdmin, ClientApi, ErroreApi, Lettura};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};

const CHIAVE_FINTA: &str = "finta-chiave-di-prova-7q9z";

fn adesso() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 17, 10, 43, 0).unwrap()
}

fn oggi() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
}

fn client(s: &ServerFinto) -> ClientApi {
    finto(&s.base(), CHIAVE_FINTA).unwrap()
}

/// Un riepilogo coi giorni dati: `ieri` e i 7 giorni prima (importi in dollari interi).
fn riepilogo_con(ieri: i64, prima: &[i64], aggiornato: Option<DateTime<Utc>>) -> Riepilogo {
    let mut giorni = Vec::new();
    for (i, d) in prima.iter().enumerate() {
        let data = oggi() - chrono::Duration::days(8 - i as i64);
        giorni.push((data, importo_da(&format!("{}", d * 100)).unwrap()));
    }
    giorni.push((
        oggi() - chrono::Duration::days(1),
        importo_da(&format!("{}", ieri * 100)).unwrap(),
    ));
    Riepilogo {
        aggiornato_a: aggiornato,
        giorni,
        ..Default::default()
    }
}

#[test]
fn client_fa_solo_get() {
    let s = ServerFinto::avvia(gestore_fixture);
    let r = aggiorna(&client(&s), adesso());
    assert!(r.non_disponibili.is_empty(), "{:?}", r.non_disponibili);
    let richieste = s.richieste();
    assert!(
        richieste.len() >= 10,
        "la sequenza intera: {}",
        richieste.len()
    );
    for q in &richieste {
        assert_eq!(q.metodo, "GET");
        assert!(
            PERCORSI_AMMESSI.contains(&q.percorso.as_str()),
            "{}",
            q.percorso
        );
    }
}

#[test]
fn intestazioni_giuste() {
    let s = ServerFinto::avvia(gestore_fixture);
    aggiorna(&client(&s), adesso());
    assert!(!s.richieste().is_empty(), "almeno una richiesta registrata");
    for q in s.richieste() {
        assert_eq!(q.intestazione("x-api-key"), Some(CHIAVE_FINTA));
        assert_eq!(q.intestazione("anthropic-version"), Some("2026-10-21"));
        assert_eq!(q.intestazione("user-agent"), Some("ItsyHome-Assistente"));
        let analitica = q.percorso.contains("/analytics/");
        assert_eq!(
            q.intestazione("anthropic-beta"),
            analitica.then_some("analytics-usage-cost-2026-08-24"),
            "{}",
            q.percorso
        );
    }
}

#[test]
fn chiave_mai_in_debug_ne_errori() {
    let c = ChiaveAdmin::nuova("sk-ant-admin01-SEGRETISSIMA-1a2b").unwrap();
    assert!(!format!("{c:?}").contains("SEGRETISSIMA"));
    let cl = finto("http://127.0.0.1:9", CHIAVE_FINTA).unwrap();
    assert!(
        !format!("{cl:?}").contains(CHIAVE_FINTA),
        "il Debug del client non mostra la chiave"
    );
    let errori = [
        ErroreApi::ChiaveNonValida,
        ErroreApi::PermessoNegato,
        ErroreApi::TroppeRichieste,
        ErroreApi::Rete,
        ErroreApi::Servizio(503),
        ErroreApi::Formato,
    ];
    for e in errori {
        let t = e.to_string();
        assert!(
            !t.contains("SEGRETISSIMA") && !t.contains(CHIAVE_FINTA) && !t.contains("sk-ant"),
            "{t}"
        );
        assert!(!t.is_empty());
    }
}

#[test]
fn finto_solo_locale_e_chiave_finta() {
    assert!(finto("https://api.anthropic.com", CHIAVE_FINTA).is_err());
    assert!(
        finto("http://127.0.0.1:1", "sk-ant-admin01-x").is_err(),
        "una chiave vera non va a un server di prova"
    );
    assert!(finto("http://localhost:1", CHIAVE_FINTA).is_err());
    assert!(finto("http://127.0.0.1:1@altro.example", CHIAVE_FINTA).is_err());
    assert!(finto("http://127.0.0.1:99999", CHIAVE_FINTA).is_err());
    assert!(finto("http://127.0.0.1:1", CHIAVE_FINTA).is_ok());
}

#[test]
fn risultato_vuoto_non_e_zero() {
    let s = ServerFinto::avvia(|r| {
        if r.percorso.contains("/analytics/") {
            (
                200,
                r#"{"data":[],"data_refreshed_at":null,"has_more":false,"next_page":null}"#
                    .to_owned(),
            )
        } else {
            gestore_fixture(r)
        }
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.spesa_mese, None);
    assert_eq!(r.spesa_ieri, None);
    assert_eq!(r.aggiornato_a, None);
    assert_eq!(r.token, None);
    assert_eq!(r.chiamate_mese, None);
    assert!(r.giorni.is_empty() && r.per_modello.is_empty());
    assert!(r.non_disponibili.is_empty(), "vuoto non è un errore");
    assert!(!spesa_insolita(&r, oggi()));
}

/// PF18: un mese ancora senza spesa è «0,00 $» solo se i dati arrivano fino alla fine del periodo
/// letto; se sono più vecchi, o senza data, non si sa e resta «nessun dato».
#[test]
fn mese_vuoto_e_zero_solo_con_dati_aggiornati_alla_fine_del_periodo() {
    let gestore = |aggiornato: &'static str| {
        move |r: &crate::server_finto::Richiesta| {
            if r.percorso.contains("/analytics/cost") && r.parametro("group_by[]") == Some("model")
            {
                (
                    200,
                    format!(
                        r#"{{"data":[],"data_refreshed_at":{aggiornato},"has_more":false,"next_page":null}}"#
                    ),
                )
            } else {
                gestore_fixture(r)
            }
        }
    };
    // Fine del periodo letto: le 10:00 del 17/11 (l'ora intera prima di `adesso`).
    let s = ServerFinto::avvia(gestore(r#""2026-11-17T10:00:00Z""#));
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(
        r.spesa_mese.map(|i| i.in_dollari()).as_deref(),
        Some("0,00 $")
    );
    // Dati fermi a prima della fine del periodo: non si sa.
    let s = ServerFinto::avvia(gestore(r#""2026-11-17T08:02:11Z""#));
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.spesa_mese, None);
    // Senza data: nessun dato.
    let s = ServerFinto::avvia(gestore("null"));
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.spesa_mese, None);
}

#[test]
fn dati_con_data_refreshed_at_nullo_sono_nessun_dato() {
    let s = ServerFinto::avvia(|r| match fixture(r) {
        Some(c) if r.percorso.contains("/analytics/") => (
            200,
            c.replace(
                "\"data_refreshed_at\": \"2026-11-17T08:02:11Z\"",
                "\"data_refreshed_at\": null",
            ),
        ),
        _ => gestore_fixture(r),
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.spesa_mese, None);
    assert_eq!(r.aggiornato_a, None);
}

#[test]
fn mese_dal_primo_in_utc() {
    let s = ServerFinto::avvia(gestore_fixture);
    aggiorna(&client(&s), adesso());
    let mese: Vec<_> = s
        .richieste()
        .into_iter()
        .filter(|q| {
            q.percorso.contains("/analytics/") && q.parametro("time_bucket_width").is_none()
        })
        .collect();
    assert!(!mese.is_empty());
    for q in mese {
        assert_eq!(q.parametro("starting_at"), Some("2026-11-01T00:00:00Z"));
        assert_eq!(q.parametro("ending_at"), Some("2026-11-17T10:00:00Z"));
    }
    let giorni: Vec<_> = s
        .richieste()
        .into_iter()
        .filter(|q| q.parametro("time_bucket_width").is_some())
        .collect();
    assert_eq!(giorni.len(), 1);
    assert_eq!(
        giorni[0].parametro("starting_at"),
        Some("2026-11-09T00:00:00Z"),
        "8 giorni prima di oggi"
    );
    assert_eq!(
        giorni[0].parametro("ending_at"),
        Some("2026-11-17T00:00:00Z")
    );
    assert_eq!(giorni[0].parametro("time_bucket_width"), Some("1d"));
    assert_eq!(giorni[0].parametro("order_by"), Some("time"));
}

#[test]
fn primo_del_mese_appena_scoccato_non_chiede_un_intervallo_vuoto() {
    // 00:30 del primo: la fine (ora intera) coincide con l'inizio del mese.
    let s = ServerFinto::avvia(gestore_fixture);
    let r = aggiorna(
        &client(&s),
        Utc.with_ymd_and_hms(2026, 12, 1, 0, 30, 0).unwrap(),
    );
    let analitiche: Vec<_> = s
        .richieste()
        .into_iter()
        .filter(|q| q.percorso.contains("/analytics/"))
        .collect();
    assert!(!analitiche.is_empty());
    assert!(analitiche
        .iter()
        .all(|q| q.parametro("starting_at") != q.parametro("ending_at")));
    assert_eq!(r.spesa_mese, None);
}

#[test]
fn pagine_seguite_al_massimo_dieci() {
    let s = ServerFinto::avvia(|r| {
        if r.percorso == "/v1/organizations/api_keys" {
            (200, r#"{"data":[{"id":"apikey_FAKE0001","name":"Chiave di prova A","status":"active"}],"has_more":true,"last_id":"apikey_FAKE0001"}"#.to_owned())
        } else {
            gestore_fixture(r)
        }
    });
    let r = aggiorna(&client(&s), adesso());
    let chiavi = s
        .richieste()
        .into_iter()
        .filter(|q| q.percorso == "/v1/organizations/api_keys")
        .count();
    assert_eq!(chiavi, 10, "dieci richieste, poi si ferma");
    // Un elenco troncato non si spaccia per completo.
    assert!(r.non_disponibili.contains(&Scheda::Chiavi));
    assert!(r.chiavi.is_empty());
}

#[test]
fn pagine_seguite_con_il_cursore() {
    let s = ServerFinto::avvia(|r| {
        if r.percorso == "/v1/organizations/api_keys" {
            return match r.parametro("after_id") {
                None => (200, r#"{"data":[{"id":"apikey_FAKE0001","name":"Chiave di prova A","status":"active"}],"has_more":true,"last_id":"apikey_FAKE0001"}"#.to_owned()),
                Some("apikey_FAKE0001") => (200, r#"{"data":[{"id":"apikey_FAKE0002","name":"Chiave di prova B","status":"active"}],"has_more":false,"last_id":"apikey_FAKE0002"}"#.to_owned()),
                Some(_) => (500, String::new()),
            };
        }
        if r.percorso == "/v1/organizations/analytics/cost"
            && r.parametro("group_by[]") == Some("model")
        {
            return match r.parametro("page") {
                None => (200, r#"{"data":[{"starting_at":"2026-11-01T00:00:00Z","group":{"model":"m1"},"cost":{"amount":"100.000000"}}],"data_refreshed_at":"2026-11-17T08:00:00Z","has_more":true,"next_page":"page_AB=="}"#.to_owned()),
                Some("page_AB==") => (200, r#"{"data":[{"starting_at":"2026-11-01T00:00:00Z","group":{"model":"m2"},"cost":{"amount":"250.000000"}}],"data_refreshed_at":"2026-11-17T08:00:00Z","has_more":false,"next_page":null}"#.to_owned()),
                Some(_) => (500, String::new()),
            };
        }
        gestore_fixture(r)
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.chiavi.len(), 2);
    assert_eq!(r.per_modello.len(), 2);
    assert_eq!(
        r.spesa_mese.unwrap().in_dollari(),
        "3,50 $",
        "il cursore con «=» arriva intatto e le pagine si sommano"
    );
}

#[test]
fn una_scheda_403_non_ferma_le_altre() {
    let s = ServerFinto::avvia(|r| {
        if r.percorso == "/v1/organizations/users" {
            (403, "{}".to_owned())
        } else {
            gestore_fixture(r)
        }
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.non_disponibili, vec![Scheda::WorkspaceMembri]);
    assert_eq!(r.spesa_mese.unwrap().in_dollari(), "412,80 $");
    assert!(r.membri.is_empty());
    assert_eq!(r.errore, None, "una scheda sola non è un errore del giro");
}

#[test]
fn errore_401_chiave_non_valida() {
    let s = ServerFinto::avvia(|_| {
        (
            401,
            r#"{"error":{"message":"invalid x-api-key"}}"#.to_owned(),
        )
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.errore, Some(ErroreApi::ChiaveNonValida));
    assert_eq!(r.spesa_mese, None);
    assert!(r.chiavi.is_empty() && r.token.is_none());
    assert_eq!(r.non_disponibili.len(), 5);
    assert_eq!(
        s.richieste().len(),
        1,
        "con la chiave rifiutata non si insiste"
    );
}

#[test]
fn errori_di_rete_e_di_servizio() {
    let chiuso = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let c = finto(&format!("http://127.0.0.1:{chiuso}"), CHIAVE_FINTA).unwrap();
    assert_eq!(
        c.leggi(&Lettura::Organizzazione).unwrap_err(),
        ErroreApi::Rete
    );

    for (codice, atteso) in [
        (403, ErroreApi::PermessoNegato),
        (429, ErroreApi::TroppeRichieste),
        (500, ErroreApi::Servizio(500)),
        (302, ErroreApi::Servizio(302)),
    ] {
        let s = ServerFinto::avvia(move |_| (codice, "{}".to_owned()));
        assert_eq!(
            client(&s).leggi(&Lettura::Organizzazione).unwrap_err(),
            atteso,
            "{codice}"
        );
        assert_eq!(s.richieste().len(), 1, "nessun reindirizzamento seguito");
    }
    let s = ServerFinto::avvia(|_| (200, "non è json".to_owned()));
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.errore, Some(ErroreApi::Formato));
}

#[test]
fn spesa_insolita_solo_con_dati_completi() {
    let dopo_mezzanotte = Some(Utc.with_ymd_and_hms(2026, 11, 17, 4, 0, 0).unwrap());
    let prima_di_mezzanotte = Some(Utc.with_ymd_and_hms(2026, 11, 16, 20, 0, 0).unwrap());
    let sette = [10; 7];
    assert!(
        spesa_insolita(&riepilogo_con(40, &sette, dopo_mezzanotte), oggi()),
        "40 $ contro media 10 $, dati completi"
    );
    assert!(
        !spesa_insolita(&riepilogo_con(40, &sette, prima_di_mezzanotte), oggi()),
        "dati di ieri incompleti"
    );
    assert!(
        !spesa_insolita(&riepilogo_con(40, &sette, None), oggi()),
        "nessuna freschezza"
    );
    assert!(
        !spesa_insolita(&riepilogo_con(40, &[10, 10, 10], dopo_mezzanotte), oggi()),
        "solo 3 giorni"
    );
    assert!(
        !spesa_insolita(&riepilogo_con(30, &sette, dopo_mezzanotte), oggi()),
        "esattamente 3 volte non basta"
    );
    assert!(
        !spesa_insolita(&riepilogo_con(5, &[0; 7], dopo_mezzanotte), oggi()),
        "media zero: nessun verdetto"
    );
}

#[test]
fn riepilogo_completo_dalle_fixture() {
    let s = ServerFinto::avvia(gestore_fixture);
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.errore, None);
    assert_eq!(
        r.aggiornato_a,
        Some(Utc.with_ymd_and_hms(2026, 11, 17, 8, 2, 11).unwrap())
    );
    assert_eq!(r.spesa_mese.unwrap().in_dollari(), "412,80 $");
    assert_eq!(r.spesa_ieri.unwrap().in_dollari(), "40,00 $");
    assert_eq!(r.giorni.len(), 8);
    assert!(
        r.giorni.windows(2).all(|w| w[0].0 < w[1].0),
        "in ordine di data"
    );
    assert_eq!(
        r.per_modello
            .iter()
            .map(|(m, _)| m.as_str())
            .collect::<Vec<_>>(),
        ["claude-opus-5-5", "claude-sonnet-5-5"]
    );
    assert_eq!(
        r.per_workspace,
        vec![(
            "Workspace di prova".to_owned(),
            importo_da("41280").unwrap()
        )]
    );
    let t = r.token.unwrap();
    assert_eq!(
        (t.ingresso, t.uscita, t.cache_lettura, t.cache_scrittura),
        (1_284_500, 891_000, 3_200_000, 120_000)
    );
    assert_eq!(r.chiamate_mese, Some(128));
    assert_eq!(r.chiavi.len(), 2);
    assert_eq!(r.chiavi[1].stato, "inactive");
    assert_eq!(
        r.chiavi[0].suggerimento, "0001",
        "solo la coda del suggerimento"
    );
    assert!(r.chiavi[0].creata.is_some());
    assert_eq!(r.workspace.len(), 1);
    assert_eq!(r.membri[0].email, "membro@example.com");
    assert_eq!(r.limiti.len(), 2);
    assert_eq!(r.limiti[0].nome, "Claude Sonnet di prova");
    assert_eq!(r.limiti[1].nome, "batch", "senza nome si usa il tipo");
    assert!(
        spesa_insolita(&r, oggi()),
        "40 $ ieri contro 10 $ al giorno, dati di stamattina"
    );
}

#[test]
fn chiamate_del_mese_per_chiave() {
    let s = ServerFinto::avvia(gestore_fixture);
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(
        r.per_chiave,
        vec![
            (
                "Chiave di prova A".to_owned(),
                importo_da("25000").unwrap(),
                Some(90)
            ),
            (
                "Chiave di prova B".to_owned(),
                importo_da("16280").unwrap(),
                Some(38)
            ),
        ]
    );
    // Il valore delle chiavi non c'è mai: solo il nome.
    assert!(!format!("{r:?}").contains("sk-ant"));
}

#[test]
fn senza_scheda_chiavi_si_mostra_l_id() {
    let s = ServerFinto::avvia(|r| {
        if r.percorso == "/v1/organizations/api_keys" {
            (403, "{}".to_owned())
        } else {
            gestore_fixture(r)
        }
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.non_disponibili, vec![Scheda::Chiavi]);
    assert_eq!(r.per_chiave[0].0, "apikey_FAKE0001");
}

#[test]
fn riga_di_costo_senza_importo_leggibile_rende_illeggibile_la_spesa() {
    for sostituto in [r#""amount":"non-un-numero""#, r#""x":"0""#] {
        let s = ServerFinto::avvia(move |r| match fixture(r) {
            Some(c)
                if r.percorso.contains("/analytics/cost")
                    && r.parametro("group_by[]") == Some("model") =>
            {
                (200, c.replacen(r#""amount":"11280.000000""#, sostituto, 1))
            }
            _ => gestore_fixture(r),
        });
        let r = aggiorna(&client(&s), adesso());
        assert!(r.non_disponibili.contains(&Scheda::Spesa), "{sostituto}");
        assert_eq!(r.spesa_mese, None, "mai una somma più piccola del vero");
        assert!(r.per_modello.is_empty());
        assert!(r.token.is_some(), "le altre schede proseguono");
    }
}

/// Le fixture dei giorni senza la riga di `giorno` (data `2026-11-DD`).
fn senza_giorno(corpo: &str, giorno: &str) -> String {
    corpo
        .lines()
        .filter(|l| !l.contains(&format!(r#""starting_at":"2026-11-{giorno}T"#)))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(",\n  ]", "\n  ]")
}

#[test]
fn giorno_senza_riga_con_dati_completi_vale_zero() {
    // 12/11 senza riga; i dati sono di stamattina (17/11 08:02): quel giorno ha speso 0 $.
    let s = ServerFinto::avvia(|r| match fixture(r) {
        Some(c) if r.parametro("time_bucket_width").is_some() => (200, senza_giorno(c, "12")),
        _ => gestore_fixture(r),
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.giorni.len(), 8);
    let dodici = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
    assert_eq!(
        r.giorni
            .iter()
            .find(|(d, _)| *d == dodici)
            .unwrap()
            .1
            .in_dollari(),
        "0,00 $"
    );
    assert!(
        spesa_insolita(&r, oggi()),
        "con un giorno a zero il verdetto c'è ancora"
    );
}

#[test]
fn ieri_senza_riga_con_dati_incompleti_non_si_sa() {
    // Dati fermi a ieri sera (16/11 20:00): ieri non è finito, senza riga non si dice 0.
    let s = ServerFinto::avvia(|r| match fixture(r) {
        Some(c) if r.percorso.contains("/analytics/") => {
            let c = c.replace("2026-11-17T08:02:11Z", "2026-11-16T20:00:00Z");
            if r.parametro("time_bucket_width").is_some() {
                (200, senza_giorno(&c, "16"))
            } else {
                (200, c)
            }
        }
        _ => gestore_fixture(r),
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.spesa_ieri, None);
    assert_eq!(r.giorni.len(), 7, "ieri manca, gli altri 7 sì");
    assert!(!spesa_insolita(&r, oggi()));
}

#[test]
fn senza_nessuna_riga_ma_con_dati_aggiornati_i_giorni_finiti_sono_zero() {
    let s = ServerFinto::avvia(|r| {
        match fixture(r) {
        Some(_) if r.parametro("time_bucket_width").is_some() => {
            (200, r#"{"data":[],"data_refreshed_at":"2026-11-17T08:02:11Z","has_more":false,"next_page":null}"#.to_owned())
        }
        _ => gestore_fixture(r),
    }
    });
    let r = aggiorna(&client(&s), adesso());
    assert_eq!(r.spesa_ieri.unwrap().in_dollari(), "0,00 $");
    assert_eq!(r.giorni.len(), 8);
}
