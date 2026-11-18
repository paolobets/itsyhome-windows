//! La chiave da amministratore: la si tiene solo nel portachiavi del sistema (lo fa l'assistente)
//! e qui vive il tempo di una lettura. Mai in `Debug`, mai in un messaggio d'errore, mai in un
//! log: di fuori si vedono soltanto gli ultimi 4 caratteri.

/// Lunghezza massima accettata (le chiavi vere stanno ben sotto).
const LUNGHEZZA_MASSIMA: usize = 256;
/// Prefisso delle chiavi da amministratore della Console.
const PREFISSO: &str = "sk-ant-admin";

/// Una chiave da amministratore già controllata nella forma. Nessun `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct ChiaveAdmin(String);

impl ChiaveAdmin {
    /// Toglie ogni spazio (si incolla spesso con un a-capo), poi controlla: prefisso
    /// `sk-ant-admin`, solo ASCII stampabile, al più 256 caratteri. L'errore è una frase che non
    /// contiene mai ciò che è stato incollato.
    pub fn nuova(s: &str) -> Result<ChiaveAdmin, &'static str> {
        let pulita: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if pulita.is_empty() {
            return Err("Incolla la chiave da amministratore.");
        }
        if pulita.len() > LUNGHEZZA_MASSIMA {
            return Err("La chiave è troppo lunga: non sembra una chiave da amministratore.");
        }
        if !pulita.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("La chiave contiene caratteri non validi.");
        }
        if !pulita.starts_with(PREFISSO) {
            return Err("Serve una chiave da amministratore (comincia per «sk-ant-admin»), non una chiave API normale.");
        }
        Ok(ChiaveAdmin(pulita))
    }

    /// Gli ultimi 4 caratteri, per mostrare «•••• 1a2b» e nient'altro.
    pub fn ultime_quattro(&self) -> String {
        let n = self.0.chars().count();
        self.0.chars().skip(n.saturating_sub(4)).collect()
    }

    /// Il valore per l'intestazione della richiesta (dentro il crate).
    pub(crate) fn segreto(&self) -> &str {
        &self.0
    }

    /// Il valore, SOLO per scriverlo nel portachiavi del sistema: l'unico posto dove la chiave vive.
    pub fn per_il_portachiavi(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ChiaveAdmin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChiaveAdmin(…{})", self.ultime_quattro())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn chiave_admin_valida() {
        assert!(
            ChiaveAdmin::nuova("sk-ant-api03-abcdef").is_err(),
            "una chiave API normale non è da amministratore"
        );
        let c = ChiaveAdmin::nuova(" sk-ant-admin01-abc \n").unwrap();
        assert_eq!(c.segreto(), "sk-ant-admin01-abc", "spazi e a-capo tolti");
        assert_eq!(c.ultime_quattro(), "-abc");
    }

    #[test]
    fn chiave_non_valida_in_altri_modi() {
        assert!(ChiaveAdmin::nuova("").is_err());
        assert!(ChiaveAdmin::nuova("   ").is_err());
        assert!(
            ChiaveAdmin::nuova("sk-ant-admin01-à").is_err(),
            "solo ASCII"
        );
        assert!(
            ChiaveAdmin::nuova(&format!("sk-ant-admin01-{}", "a".repeat(260))).is_err(),
            "troppo lunga"
        );
        assert!(
            ChiaveAdmin::nuova("sk-ant-admin01-a\u{7}b").is_err(),
            "caratteri di controllo"
        );
    }

    #[test]
    fn debug_mostra_solo_le_ultime_quattro() {
        let c = ChiaveAdmin::nuova("sk-ant-admin01-SEGRETISSIMA-1a2b").unwrap();
        assert_eq!(format!("{c:?}"), "ChiaveAdmin(…1a2b)");
        assert!(!format!("{c:?}").contains("SEGRETISSIMA"));
    }
}
