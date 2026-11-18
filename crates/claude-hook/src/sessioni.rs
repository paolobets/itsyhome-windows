//! Lo stato delle sessioni di Claude Code, puro (nessun I/O né orologio: l'ora si passa) (spec M3
//! §6.4).

use std::time::{Duration, Instant};

use crate::evento::{EventoHook, TipoEvento};

/// Una sessione silenziosa da tanto sparisce (salvo se aspetta una risposta).
pub const SILENZIO_MASSIMO: Duration = Duration::from_secs(30 * 60);
/// Chi aspetta una risposta scade molto più tardi (il terminale chiuso con la X non manda
/// `SessionEnd`).
pub const ATTESA_MASSIMA_SESSIONE: Duration = Duration::from_secs(2 * 60 * 60);
/// Sessioni tenute al massimo (limite rigido).
pub const MASSIMO_SESSIONI: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stato {
    Lavora,
    TiAspetta,
    Fatto,
    Errore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sessione {
    pub id: String,
    pub cartella: String,
    pub stato: Stato,
    pub strumento: Option<String>,
    /// Da quando lavora (solo se `Lavora`).
    pub lavora_da: Option<Instant>,
    /// Ora dell'ultimo evento.
    pub ultimo: Instant,
    pub errore: Option<String>,
    /// Il `Fatto` è già stato visto dall'utente.
    pub vista: bool,
}

/// Cosa è cambiato. Un evento che non cambia nulla non produce alcun `Cambio`: `ricevi` restituisce
/// un elenco vuoto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cambio {
    Cambiata(String),
    Tolta(String),
}

/// Le sessioni, dalla meno alla più recente per ultimo evento.
#[derive(Debug, Default)]
pub struct Sessioni {
    v: Vec<Sessione>,
}

/// Dopo quanto silenzio la sessione scade.
fn limite_silenzio(s: &Sessione) -> Duration {
    if s.stato == Stato::TiAspetta {
        ATTESA_MASSIMA_SESSIONE
    } else {
        SILENZIO_MASSIMO
    }
}

fn urgenza(s: &Sessione) -> u8 {
    match (s.stato, s.vista) {
        (Stato::TiAspetta, _) => 0,
        (Stato::Errore, _) => 1,
        (Stato::Fatto, false) => 2,
        (Stato::Lavora, _) => 3,
        (Stato::Fatto, true) => 4,
    }
}

impl Sessioni {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applica un evento. Restituisce i cambi: `Tolta` della sessione scalzata dal limite di
    /// `MASSIMO_SESSIONI` (se c'è) e poi `Cambiata`/`Tolta` di quella dell'evento.
    pub fn ricevi(&mut self, e: &EventoHook, ora: Instant) -> Vec<Cambio> {
        let mut cambi = Vec::new();
        let posto = self.v.iter().position(|s| s.sessione_è(&e.sessione));
        if e.tipo == TipoEvento::SessionEnd {
            if let Some(p) = posto {
                self.v.remove(p);
                cambi.push(Cambio::Tolta(e.sessione.clone()));
            }
            return cambi;
        }
        // Notifiche che non dicono nulla di nuovo: nessun cambio (e nessuna sessione creata).
        let notifica = e.notifica.as_deref();
        let ignorata = e.tipo == TipoEvento::Notification
            && !matches!(
                notifica,
                Some("permission_prompt" | "agent_needs_input" | "elicitation_dialog" | "idle_prompt")
            );
        if ignorata {
            if let Some(p) = posto {
                self.tocca(p, ora);
            }
            return cambi;
        }
        // Un `idle_prompt` di una sessione sconosciuta NON la crea: non dice cosa stava facendo
        // e, senza altri eventi, resterebbe lì in attesa.
        let idle = e.tipo == TipoEvento::Notification && notifica == Some("idle_prompt");
        if idle {
            match posto {
                // Un idle di una sessione ignota o già chiusa non dice nulla.
                None => return cambi,
                Some(p) if matches!(self.v[p].stato, Stato::Fatto | Stato::Errore) => {
                    self.tocca(p, ora);
                    return cambi;
                }
                _ => {}
            }
        }

        let mut s = match posto {
            Some(p) => self.v.remove(p),
            None => Sessione {
                id: e.sessione.clone(),
                cartella: String::new(),
                stato: Stato::Lavora,
                strumento: None,
                lavora_da: None,
                ultimo: ora,
                errore: None,
                vista: false,
            },
        };
        s.ultimo = ora;
        if !e.cartella.is_empty() {
            s.cartella = e.cartella.clone();
        }
        let era_lavora = s.stato == Stato::Lavora && posto.is_some();
        match e.tipo {
            TipoEvento::UserPromptSubmit | TipoEvento::PreToolUse => {
                s.stato = Stato::Lavora;
                s.strumento = if e.tipo == TipoEvento::PreToolUse { e.strumento.clone() } else { None };
                s.errore = None;
                if !era_lavora {
                    s.lavora_da = Some(ora);
                }
            }
            TipoEvento::PermissionRequest | TipoEvento::Notification => {
                s.stato = Stato::TiAspetta;
                s.lavora_da = None;
                if e.tipo == TipoEvento::PermissionRequest {
                    s.strumento = e.strumento.clone();
                }
            }
            TipoEvento::Stop => {
                s.stato = Stato::Fatto;
                s.vista = false;
                s.strumento = None;
                s.lavora_da = None;
            }
            TipoEvento::StopFailure => {
                s.stato = Stato::Errore;
                s.errore = e.errore.clone();
                s.strumento = None;
                s.lavora_da = None;
            }
            TipoEvento::SessionEnd => unreachable!("gestito sopra"),
        }
        self.v.push(s);

        // Limite rigido: si toglie la meno recente fra quelle che non aspettano e, se aspettano
        // tutte, la meno recente in assoluto; mai quella appena arrivata (è l'ultima del Vec).
        while self.v.len() > MASSIMO_SESSIONI {
            let ultima = self.v.len() - 1;
            let p = self.v[..ultima].iter().position(|s| s.stato != Stato::TiAspetta).unwrap_or(0);
            let tolta = self.v.remove(p);
            cambi.push(Cambio::Tolta(tolta.id));
        }
        cambi.push(Cambio::Cambiata(e.sessione.clone()));
        cambi
    }

    /// L'utente ha risposto a un permesso dall'assistente: Claude Code esegue (o rinuncia) subito,
    /// ma senza un nuovo evento la sessione resterebbe «ti aspetta». Passa a `Lavora` tenendo lo
    /// strumento; `lavora_da` parte solo se non lavorava già. Sessione sconosciuta o che lavora
    /// già: nessun cambio.
    pub fn segna_al_lavoro(&mut self, id: &str, ora: Instant) -> Vec<Cambio> {
        let Some(p) = self.v.iter().position(|s| s.sessione_è(id)) else { return Vec::new() };
        if self.v[p].stato == Stato::Lavora {
            return Vec::new();
        }
        self.tocca(p, ora);
        let s = self.v.last_mut().expect("appena rimessa in fondo");
        s.stato = Stato::Lavora;
        s.errore = None;
        s.lavora_da = Some(ora);
        vec![Cambio::Cambiata(id.to_owned())]
    }

    /// Un evento che aggiorna solo `ultimo`: la sessione passa in fondo, così la posizione resta
    /// coerente con `ultimo` (limite e parità di `elenco`).
    fn tocca(&mut self, p: usize, ora: Instant) {
        let mut s = self.v.remove(p);
        s.ultimo = ora;
        self.v.push(s);
    }

    /// Per urgenza (TiAspetta, Errore, Fatto non vista, Lavora, Fatto vista); a parità la più
    /// recente prima.
    pub fn elenco(&self) -> Vec<&Sessione> {
        let mut e: Vec<&Sessione> = self.v.iter().rev().collect();
        e.sort_by_key(|s| urgenza(s)); // stabile: l'ordine di recenza resta a parità
        e
    }

    /// Toglie e restituisce le sessioni silenziose da `SILENZIO_MASSIMO`; quelle che aspettano una
    /// risposta solo dopo `ATTESA_MASSIMA_SESSIONE`.
    pub fn scadute(&mut self, ora: Instant) -> Vec<String> {
        let mut tolte = Vec::new();
        self.v.retain(|s| {
            let scaduta = ora.saturating_duration_since(s.ultimo) >= limite_silenzio(s);
            if scaduta {
                tolte.push(s.id.clone());
            }
            !scaduta
        });
        tolte
    }

    /// Fra quanto scade la prossima sessione (zero se già scaduta); `None` se non ce ne sono.
    pub fn prossima_scadenza(&self, ora: Instant) -> Option<Duration> {
        self.v
            .iter()
            .map(|s| limite_silenzio(s).saturating_sub(ora.saturating_duration_since(s.ultimo)))
            .min()
    }

    /// L'utente ha guardato l'elenco: i `Fatto` diventano visti.
    pub fn segna_viste(&mut self) {
        for s in &mut self.v {
            if s.stato == Stato::Fatto {
                s.vista = true;
            }
        }
    }

    pub fn togli(&mut self, id: &str) {
        self.v.retain(|s| s.id != id);
    }

    pub fn lavorano(&self) -> usize {
        self.v.iter().filter(|s| s.stato == Stato::Lavora).count()
    }
}

impl Sessione {
    fn sessione_è(&self, id: &str) -> bool {
        self.id == id
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn ev(id: &str, tipo: TipoEvento) -> EventoHook {
        EventoHook {
            sessione: id.into(),
            tipo,
            cartella: "prog".into(),
            strumento: None,
            notifica: None,
            errore: None,
            riassunto: None,
        }
    }

    fn stato(s: &Sessioni, id: &str) -> Option<Stato> {
        s.elenco().iter().find(|x| x.id == id).map(|x| x.stato)
    }

    fn notifica(id: &str, tipo: &str) -> EventoHook {
        EventoHook { notifica: Some(tipo.into()), ..ev(id, TipoEvento::Notification) }
    }

    #[test]
    fn al_lavoro_dopo_la_risposta_a_un_permesso() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        assert!(s.segna_al_lavoro("a", t).is_empty(), "sconosciuta: nessun cambio");
        let chiede = EventoHook { strumento: Some("Bash".into()), ..ev("a", TipoEvento::PermissionRequest) };
        s.ricevi(&chiede, t);
        assert_eq!(s.elenco()[0].stato, Stato::TiAspetta);
        let dopo = t + Duration::from_secs(9);
        assert_eq!(s.segna_al_lavoro("a", dopo), vec![Cambio::Cambiata("a".into())]);
        let x = s.elenco()[0];
        assert_eq!((x.stato, x.strumento.as_deref(), x.lavora_da, x.ultimo), (Stato::Lavora, Some("Bash"), Some(dopo), dopo));
        // Già al lavoro: niente, e `lavora_da` non riparte.
        assert!(s.segna_al_lavoro("a", dopo + Duration::from_secs(5)).is_empty());
        assert_eq!(s.elenco()[0].lavora_da, Some(dopo));
    }

    #[test]
    fn ciclo_di_una_sessione() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        assert_eq!(s.ricevi(&ev("a", TipoEvento::UserPromptSubmit), t), vec![Cambio::Cambiata("a".into())]);
        assert_eq!(stato(&s, "a"), Some(Stato::Lavora));
        assert_eq!(s.elenco()[0].lavora_da, Some(t));
        let pre = EventoHook { strumento: Some("Edit".into()), ..ev("a", TipoEvento::PreToolUse) };
        s.ricevi(&pre, t + Duration::from_secs(1));
        assert_eq!(s.elenco()[0].strumento.as_deref(), Some("Edit"));
        assert_eq!(s.elenco()[0].lavora_da, Some(t), "lavora_da non riparte se già Lavora");
        s.ricevi(&ev("a", TipoEvento::PermissionRequest), t);
        assert_eq!(stato(&s, "a"), Some(Stato::TiAspetta));
        s.ricevi(&ev("a", TipoEvento::Stop), t);
        assert_eq!(stato(&s, "a"), Some(Stato::Fatto));
        assert!(!s.elenco()[0].vista);
        assert_eq!(s.ricevi(&ev("a", TipoEvento::SessionEnd), t), vec![Cambio::Tolta("a".into())]);
        assert!(s.elenco().is_empty());
    }

    #[test]
    fn idle_dopo_fatto_resta_fatto() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        s.ricevi(&ev("a", TipoEvento::Stop), t);
        assert!(s.ricevi(&notifica("a", "idle_prompt"), t).is_empty());
        assert_eq!(stato(&s, "a"), Some(Stato::Fatto));
        // ma mentre lavora l'idle significa che aspetta
        s.ricevi(&ev("b", TipoEvento::UserPromptSubmit), t);
        s.ricevi(&notifica("b", "idle_prompt"), t);
        assert_eq!(stato(&s, "b"), Some(Stato::TiAspetta));
    }

    #[test]
    fn notifiche_di_attesa_e_altre() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        s.ricevi(&ev("a", TipoEvento::UserPromptSubmit), t);
        assert!(s.ricevi(&notifica("a", "auth_success"), t).is_empty());
        assert_eq!(stato(&s, "a"), Some(Stato::Lavora));
        for tipo in ["permission_prompt", "agent_needs_input", "elicitation_dialog"] {
            s.ricevi(&ev("a", TipoEvento::UserPromptSubmit), t);
            s.ricevi(&notifica("a", tipo), t);
            assert_eq!(stato(&s, "a"), Some(Stato::TiAspetta), "{tipo}");
        }
        // una notifica irrilevante non crea sessioni
        assert!(s.ricevi(&notifica("zzz", "auth_success"), t).is_empty());
        assert_eq!(stato(&s, "zzz"), None);
    }

    #[test]
    fn notifica_dopo_il_permesso_resta_ti_aspetta() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        let p = EventoHook { strumento: Some("Write".into()), ..ev("a", TipoEvento::PermissionRequest) };
        s.ricevi(&p, t);
        s.ricevi(&notifica("a", "permission_prompt"), t + Duration::from_secs(6));
        assert_eq!(stato(&s, "a"), Some(Stato::TiAspetta));
        assert_eq!(s.elenco()[0].strumento.as_deref(), Some("Write"));
    }

    #[test]
    fn errore_dallo_stopfailure() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        let f = EventoHook { errore: Some("rate_limit".into()), ..ev("a", TipoEvento::StopFailure) };
        s.ricevi(&f, t);
        assert_eq!(stato(&s, "a"), Some(Stato::Errore));
        assert_eq!(s.elenco()[0].errore.as_deref(), Some("rate_limit"));
        s.ricevi(&ev("a", TipoEvento::UserPromptSubmit), t);
        assert_eq!(s.elenco()[0].errore, None);
    }

    #[test]
    fn silenziosa_sparisce_dopo_trenta_minuti() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        s.ricevi(&ev("a", TipoEvento::UserPromptSubmit), t);
        s.ricevi(&ev("b", TipoEvento::PermissionRequest), t);
        assert_eq!(s.prossima_scadenza(t), Some(SILENZIO_MASSIMO));
        assert_eq!(s.prossima_scadenza(t + Duration::from_secs(600)), Some(Duration::from_secs(1200)));
        assert!(s.scadute(t + SILENZIO_MASSIMO - Duration::from_secs(1)).is_empty());
        assert_eq!(s.scadute(t + SILENZIO_MASSIMO), vec!["a".to_string()]);
        assert_eq!(stato(&s, "b"), Some(Stato::TiAspetta), "chi aspetta non scade a 30 minuti");
        assert_eq!(s.prossima_scadenza(t + SILENZIO_MASSIMO), Some(ATTESA_MASSIMA_SESSIONE - SILENZIO_MASSIMO));
        assert!(s.scadute(t + ATTESA_MASSIMA_SESSIONE - Duration::from_secs(1)).is_empty());
        assert_eq!(s.scadute(t + ATTESA_MASSIMA_SESSIONE), vec!["b".to_string()]);
        assert_eq!(s.prossima_scadenza(t + ATTESA_MASSIMA_SESSIONE), None);
    }

    #[test]
    fn tetto_rigido_anche_se_aspettano_tutte() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        for i in 0..MASSIMO_SESSIONI {
            s.ricevi(&ev(&format!("s{i}"), TipoEvento::PermissionRequest), t + Duration::from_secs(i as u64));
        }
        let cambi = s.ricevi(&ev("nuova", TipoEvento::PermissionRequest), t + Duration::from_secs(99));
        assert_eq!(cambi, vec![Cambio::Tolta("s0".into()), Cambio::Cambiata("nuova".into())]);
        assert_eq!(s.elenco().len(), MASSIMO_SESSIONI);
        assert_eq!(stato(&s, "nuova"), Some(Stato::TiAspetta));
        assert_eq!(stato(&s, "s0"), None);
    }

    #[test]
    fn evento_che_tocca_solo_ultimo_sposta_in_fondo() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        s.ricevi(&ev("a", TipoEvento::UserPromptSubmit), t);
        s.ricevi(&ev("b", TipoEvento::UserPromptSubmit), t + Duration::from_secs(1));
        // "a" è la più vecchia; una notifica irrilevante la rinfresca
        assert!(s.ricevi(&notifica("a", "auth_success"), t + Duration::from_secs(2)).is_empty());
        for i in 0..(MASSIMO_SESSIONI - 1) {
            s.ricevi(&ev(&format!("n{i}"), TipoEvento::UserPromptSubmit), t + Duration::from_secs(10 + i as u64));
        }
        // 17 sessioni: va via "b" (ora la più vecchia), non "a"
        assert_eq!(stato(&s, "b"), None);
        assert_eq!(stato(&s, "a"), Some(Stato::Lavora));
    }

    #[test]
    fn eventi_fuori_ordine_e_sessioni_limitate() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        s.ricevi(&ev("a", TipoEvento::Stop), t);
        assert_eq!(stato(&s, "a"), Some(Stato::Fatto));
        assert_eq!(s.ricevi(&ev("a", TipoEvento::SessionEnd), t).len(), 1);
        assert!(s.ricevi(&ev("a", TipoEvento::SessionEnd), t).is_empty());

        // la più vecchia (s0) aspetta: si toglie la successiva
        let mut s = Sessioni::new();
        s.ricevi(&ev("s0", TipoEvento::PermissionRequest), t);
        for i in 1..MASSIMO_SESSIONI {
            s.ricevi(&ev(&format!("s{i}"), TipoEvento::UserPromptSubmit), t + Duration::from_secs(i as u64));
        }
        assert_eq!(s.elenco().len(), 16);
        let cambi = s.ricevi(&ev("nuova", TipoEvento::UserPromptSubmit), t + Duration::from_secs(99));
        assert_eq!(cambi, vec![Cambio::Tolta("s1".into()), Cambio::Cambiata("nuova".into())]);
        assert_eq!(s.elenco().len(), 16);
        assert_eq!(stato(&s, "s0"), Some(Stato::TiAspetta));
        assert_eq!(stato(&s, "s1"), None);

        // senza chi aspetta, va via la più vecchia in assoluto
        let mut s = Sessioni::new();
        for i in 0..=MASSIMO_SESSIONI {
            s.ricevi(&ev(&format!("s{i}"), TipoEvento::UserPromptSubmit), t + Duration::from_secs(i as u64));
        }
        assert_eq!(s.elenco().len(), 16);
        assert_eq!(stato(&s, "s0"), None);
    }

    #[test]
    fn ordine_per_urgenza() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        s.ricevi(&ev("lavora", TipoEvento::UserPromptSubmit), t);
        s.ricevi(&ev("aspetta", TipoEvento::PermissionRequest), t);
        s.ricevi(&ev("fatto", TipoEvento::Stop), t);
        let f = EventoHook { errore: Some("x".into()), ..ev("errore", TipoEvento::StopFailure) };
        s.ricevi(&f, t);
        let id = |s: &Sessioni| s.elenco().iter().map(|x| x.id.clone()).collect::<Vec<_>>();
        assert_eq!(id(&s), ["aspetta", "errore", "fatto", "lavora"]);
        s.segna_viste();
        assert_eq!(id(&s), ["aspetta", "errore", "lavora", "fatto"]);
        // a parità, la più recente prima
        s.ricevi(&ev("lavora2", TipoEvento::UserPromptSubmit), t + Duration::from_secs(5));
        assert_eq!(id(&s), ["aspetta", "errore", "lavora2", "lavora", "fatto"]);
    }

    #[test]
    fn togli_e_lavorano() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        s.ricevi(&ev("a", TipoEvento::UserPromptSubmit), t);
        s.ricevi(&ev("b", TipoEvento::Stop), t);
        assert_eq!(s.lavorano(), 1);
        s.togli("a");
        assert_eq!(s.lavorano(), 0);
        assert_eq!(s.elenco().len(), 1);
    }
}
