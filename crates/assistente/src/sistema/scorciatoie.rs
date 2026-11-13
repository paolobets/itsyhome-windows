//! Scorciatoie globali (`global-hotkey`): Ctrl+Alt+Spazio apre il pannello (e rimette la pillola
//! visibile e sopra: la «via di salvataggio»), Ctrl+Alt+Maiusc+I ritira o rimette la pillola.
//! Mai Ctrl+Alt+<lettera>: sulla tastiera italiana Ctrl+Alt è AltGr (è, @, #, [, ]…).
//! Una scorciatoia già presa da un'altra app diventa una voce «informazione» in «In attesa».
//! Gli eventi arrivano dalla finestra nascosta di `global-hotkey` e si inoltrano al ciclo eventi.

use crate::attenzione::{Fonte, Livello, Pulsante, Voce, PREFISSO_SISTEMA, PULSANTE_HO_CAPITO};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scorciatoia {
    Apri,
    Ritirata,
}

impl Scorciatoia {
    pub const TUTTE: [Scorciatoia; 2] = [Scorciatoia::Apri, Scorciatoia::Ritirata];

    /// Come la si scrive all'utente.
    pub fn tasti(self) -> &'static str {
        match self {
            Scorciatoia::Apri => "Ctrl+Alt+Spazio",
            Scorciatoia::Ritirata => "Ctrl+Alt+Maiusc+I",
        }
    }

    fn cosa_fa(self) -> &'static str {
        match self {
            Scorciatoia::Apri => "aprire il pannello",
            Scorciatoia::Ritirata => "ritirare o rimettere l'isola",
        }
    }

    fn altro_modo(self) -> &'static str {
        match self {
            Scorciatoia::Apri => "Il pannello si apre comunque con un clic sulla pillola o con «Apri» nell'icona vicino all'orologio di Windows.",
            Scorciatoia::Ritirata => "L'isola si ritira comunque con il clic destro sulla pillola o dall'icona vicino all'orologio di Windows.",
        }
    }
}

/// La voce «In attesa» per una scorciatoia non attiva (`occupata`: un'altra app la usa già).
pub fn voce_non_attiva(s: Scorciatoia, occupata: bool, ora: Instant) -> Voce {
    let titolo = if occupata {
        format!("{} è già usata da un'altra app", s.tasti())
    } else {
        format!("{} non è attiva", s.tasti())
    };
    Voce {
        id: format!("{PREFISSO_SISTEMA}scorciatoia-{}", s.tasti()),
        livello: Livello::Informazione,
        fonte: Fonte::Pc,
        titolo,
        dettaglio: format!("La scorciatoia per {} non funziona. {}", s.cosa_fa(), s.altro_modo()),
        pulsanti: vec![Pulsante::new(PULSANTE_HO_CAPITO, "Ho capito")],
        creata: ora,
    }
}

#[cfg(windows)]
pub use windows::{registra, Scorciatoie};

#[cfg(windows)]
mod windows {
    use super::Scorciatoia;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    fn tasto(s: Scorciatoia) -> HotKey {
        match s {
            Scorciatoia::Apri => HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space),
            Scorciatoia::Ritirata => {
                HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT), Code::KeyI)
            }
        }
    }

    /// Tiene registrate le scorciatoie finché vive.
    pub struct Scorciatoie {
        _gestore: GlobalHotKeyManager,
    }

    /// Registra le scorciatoie; `al_tasto` è chiamata sul ciclo eventi. Restituisce anche quelle
    /// non attive, con vero se un'altra app le ha già prese.
    pub fn registra(al_tasto: fn(Scorciatoia)) -> (Option<Scorciatoie>, Vec<(Scorciatoia, bool)>) {
        let gestore = match GlobalHotKeyManager::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("scorciatoie globali non disponibili: {e}");
                return (None, Scorciatoia::TUTTE.iter().map(|s| (*s, false)).collect());
            }
        };
        let mut non_attive = vec![];
        for s in Scorciatoia::TUTTE {
            if let Err(e) = gestore.register(tasto(s)) {
                eprintln!("scorciatoia {} non registrata: {e}", s.tasti());
                non_attive.push((s, matches!(e, global_hotkey::Error::AlreadyRegistered(_))));
            }
        }
        let ids: Vec<(u32, Scorciatoia)> = Scorciatoia::TUTTE.iter().map(|s| (tasto(*s).id(), *s)).collect();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.state() != HotKeyState::Pressed {
                return;
            }
            if let Some((_, s)) = ids.iter().find(|(id, _)| *id == e.id()) {
                let s = *s;
                let _ = slint::invoke_from_event_loop(move || al_tasto(s));
            }
        }));
        (Some(Scorciatoie { _gestore: gestore }), non_attive)
    }
}

/// Fuori da Windows per ora nessuna scorciatoia globale (su Linux servirebbe X11).
#[cfg(not(windows))]
pub struct Scorciatoie;

#[cfg(not(windows))]
pub fn registra(_al_tasto: fn(Scorciatoia)) -> (Option<Scorciatoie>, Vec<(Scorciatoia, bool)>) {
    (None, vec![])
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mai_ctrl_alt_lettera() {
        // Ctrl+Alt+<lettera> è AltGr+<lettera> sulla tastiera italiana: la ritirata ha anche Maiusc.
        assert_eq!(Scorciatoia::Apri.tasti(), "Ctrl+Alt+Spazio");
        assert_eq!(Scorciatoia::Ritirata.tasti(), "Ctrl+Alt+Maiusc+I");
    }

    #[test]
    fn scorciatoia_occupata_diventa_un_informazione() {
        let v = voce_non_attiva(Scorciatoia::Apri, true, Instant::now());
        assert_eq!(v.livello, Livello::Informazione);
        assert_eq!(v.titolo, "Ctrl+Alt+Spazio è già usata da un'altra app");
        assert!(v.id.starts_with(PREFISSO_SISTEMA));
        assert_eq!(v.pulsanti, vec![Pulsante::new(PULSANTE_HO_CAPITO, "Ho capito")]);
        assert!(!crate::attenzione::merita_notifica_di_sistema(&v), "niente toast per questo");
        let v = voce_non_attiva(Scorciatoia::Ritirata, false, Instant::now());
        assert_eq!(v.titolo, "Ctrl+Alt+Maiusc+I non è attiva");
        assert_ne!(v.id, voce_non_attiva(Scorciatoia::Apri, false, Instant::now()).id);
    }
}
