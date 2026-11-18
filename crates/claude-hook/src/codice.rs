//! Il codice segreto dell'ascoltatore: 32 byte casuali, scritti come 64 cifre esadecimali.
//! Arriva negli hook come `Authorization: Bearer <codice>`.

/// Il codice. Niente `Debug` derivato: scrive solo `CodiceHook(…)`.
#[derive(Clone)]
pub struct CodiceHook([u8; 32]);

impl std::fmt::Debug for CodiceHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodiceHook(…)")
    }
}

fn cifra(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

impl CodiceHook {
    /// Un codice nuovo dalla casualità del sistema.
    pub fn nuovo() -> Result<CodiceHook, String> {
        let mut b = [0u8; 32];
        getrandom::fill(&mut b).map_err(|e| format!("casualità non disponibile: {e}"))?;
        Ok(CodiceHook(b))
    }

    /// Da 64 cifre esadecimali (maiuscole o minuscole); altro è `None`.
    pub fn da_testo(s: &str) -> Option<CodiceHook> {
        let t = s.as_bytes();
        if t.len() != 64 {
            return None;
        }
        let mut b = [0u8; 32];
        for (i, coppia) in t.chunks_exact(2).enumerate() {
            b[i] = (cifra(coppia[0])? << 4) | cifra(coppia[1])?;
        }
        Some(CodiceHook(b))
    }

    /// 64 cifre esadecimali minuscole.
    pub fn come_testo(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// `true` se `autorizzazione` è `Bearer <codice>` (schema senza distinzione di maiuscole).
    /// Il confronto dei 32 byte è a tempo costante: OR degli XOR, nessuna uscita anticipata.
    pub fn verifica(&self, autorizzazione: &str) -> bool {
        let Some((schema, resto)) = autorizzazione.split_once(' ') else {
            return false;
        };
        if !schema.eq_ignore_ascii_case("bearer") {
            return false;
        }
        let Some(altro) = CodiceHook::da_testo(resto) else {
            return false;
        };
        let mut differenza = 0u8;
        for (a, b) in self.0.iter().zip(altro.0.iter()) {
            differenza |= a ^ b;
        }
        differenza == 0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn codice_andata_e_ritorno() {
        let c = CodiceHook::nuovo().expect("casualità");
        let t = c.come_testo();
        assert_eq!(t.len(), 64);
        let r = CodiceHook::da_testo(&t).expect("si rilegge");
        assert_eq!(r.come_testo(), t);
        assert!(CodiceHook::da_testo(&t[..63]).is_none());
        assert!(CodiceHook::da_testo(&format!("{t}0")).is_none());
        assert!(CodiceHook::da_testo(&"zz".repeat(32)).is_none());
        // Due codici nuovi non coincidono.
        assert_ne!(CodiceHook::nuovo().unwrap().come_testo(), t);
    }

    #[test]
    fn verifica_codice() {
        let c = CodiceHook::nuovo().unwrap();
        let t = c.come_testo();
        assert!(c.verifica(&format!("Bearer {t}")));
        assert!(c.verifica(&format!("bearer {t}")));
        assert!(c.verifica(&format!("BEARER {}", t.to_uppercase())));
        // Un byte cambiato.
        let mut cambiato = t.clone().into_bytes();
        cambiato[10] = if cambiato[10] == b'0' { b'1' } else { b'0' };
        let cambiato = String::from_utf8(cambiato).unwrap();
        assert!(!c.verifica(&format!("Bearer {cambiato}")));
        assert!(!c.verifica("Bearer"));
        assert!(!c.verifica("Bearer "));
        assert!(!c.verifica(""));
        assert!(!c.verifica(&format!("Basic {t}")));
        assert!(!c.verifica(&t));
        assert!(!c.verifica(&format!("Bearer  {t}")));
    }

    #[test]
    fn debug_senza_codice() {
        let c = CodiceHook::nuovo().unwrap();
        let d = format!("{c:?}");
        assert!(!d.contains(&c.come_testo()));
        assert!(d.contains("CodiceHook"));
    }
}
