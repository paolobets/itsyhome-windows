//! Il collegamento a Claude Code dalle Impostazioni (M3, Task 7, spec §5 e §6.1).
//!
//! **Mai una scrittura senza il «sì» dell'utente.** `prepara_collegamento` legge e basta: mostra
//! che cosa verrebbe aggiunto a `settings.json`; solo `conferma_collegamento` (il pulsante «Scrivi
//! in settings.json») scrive, e scrive esattamente il piano che l'utente ha letto. All'avvio
//! `controlla_all_avvio` non scrive mai: al più mette una voce informativa. Il codice segreto non
//! entra mai in `settings.json`: sta nel portachiavi e nella variabile d'ambiente dell'utente
//! (`ISOLA_HOOK_TOKEN`), e gli hook la citano per nome.

use super::{avvia_con, codice_da, SEGRETO_HOOK};
use crate::attenzione::{Fonte, Livello, Pulsante, Voce, PULSANTE_IMPOSTAZIONI};
use crate::claude::ID_AGGIORNA;
use crate::ponte::Ponte;
use crate::sistema::ambiente_utente::{AmbienteUtente, VARIABILE};
use crate::FinestraImpostazioni;
use claude_hook::installa::{self, Anteprima, Piano, StatoInstallazione, VERSIONE_VOCI};
use claude_hook::CodiceHook;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Dove e come si installa: il file di Claude Code, il programma che la barra di stato lancerà, se
/// si è in prova. I test lo costruiscono sopra una cartella temporanea; il programma vero con
/// `Contesto::reale`.
pub struct Contesto {
    pub file: PathBuf,
    pub exe: PathBuf,
    /// In prova (`ITSYHOME_ASSISTENTE_DIR`): file finto, chiave di registro di prova, portachiavi
    /// vero mai toccato.
    pub in_prova: bool,
    /// Il percorso corto 8.3 di un file (sui percorsi con spazi); `None` se non c'è.
    pub corto: fn(&Path) -> Option<PathBuf>,
}

impl Contesto {
    pub fn reale() -> Contesto {
        let cartella_prova = std::env::var_os(crate::avvio::VAR_CARTELLA).filter(|v| !v.is_empty());
        let in_prova = cartella_prova.is_some();
        let file = match &cartella_prova {
            Some(d) => Path::new(d).join("claude-finto").join("settings.json"),
            None => {
                let config = std::env::var_os("CLAUDE_CONFIG_DIR");
                let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
                match (home, config.as_deref().filter(|c| !c.is_empty())) {
                    (Some(h), c) => installa::file_impostazioni(c, &h),
                    (None, Some(c)) => installa::file_impostazioni(Some(c), Path::new("")),
                    // Né casa né cartella di Claude Code: nessun file (le operazioni lo dicono).
                    (None, None) => PathBuf::new(),
                }
            }
        };
        Contesto {
            file,
            exe: std::env::current_exe().unwrap_or_default(),
            in_prova,
            corto: crate::sistema::ambiente_utente::percorso_corto,
        }
    }

    fn controlla(&self) -> Result<(), String> {
        if self.file.as_os_str().is_empty() {
            return Err("Non trovo la cartella di Claude Code (manca la cartella dell'utente).".into());
        }
        Ok(())
    }
}

/// Che cosa l'utente vede prima di confermare.
pub struct Preparazione {
    pub anteprima: Anteprima,
    /// Il file che si scriverebbe.
    pub file: PathBuf,
    /// Il programma sta in un percorso che la shell di Claude Code non può leggere e senza nome
    /// corto: la barra di stato non si configura.
    pub barra_non_scrivibile: bool,
}

impl Preparazione {
    /// Le frasi di avviso, una per riga, nell'ordine in cui vanno lette.
    pub fn avvisi(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.anteprima.hook_disabilitati {
            v.push(
                "«disableAllHooks» è attivo in settings.json: finché resta così, Claude Code non esegue né gli hook né la barra di stato."
                    .to_owned(),
            );
        }
        if self.anteprima.fuori_lista {
            v.push(
                "settings.json limita gli hook HTTP (allowedHttpHookUrls o httpHookAllowedEnvVars) e isola non è nell'elenco: finché non la aggiungi, Claude Code non le manderà nulla."
                    .to_owned(),
            );
        }
        if self.anteprima.barra_occupata {
            v.push("La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili.".to_owned());
        } else if self.barra_non_scrivibile {
            v.push(
                "Il programma sta in un percorso con spazi e senza nome corto: la barra di stato non si configura e i limiti dell'abbonamento non saranno disponibili."
                    .to_owned(),
            );
        }
        v
    }
}

/// Una porta libera scelta dal sistema (solo la prima volta: poi resta quella salvata).
fn porta_libera() -> Result<u16, String> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("Non trovo una porta libera: {e}"))
}

impl Ponte {
    fn piano_per(&self, cx: &Contesto, porta: u16) -> Piano {
        Piano { porta, comando_barra: installa::comando_barra(&cx.exe, porta, cx.corto) }
    }

    /// Legge `settings.json` e prepara il collegamento: la porta (quella salvata o una libera) e
    /// l'anteprima delle voci che si aggiungerebbero. **Non scrive nulla.**
    pub fn prepara_collegamento(&self, cx: &Contesto) -> Result<Preparazione, String> {
        cx.controlla()?;
        let v = installa::leggi(&cx.file).map_err(|e| e.to_string())?;
        let porta = match self.casa.con_config(|c| c.claude.porta) {
            Some(p) => p,
            None => porta_libera()?,
        };
        let piano = self.piano_per(cx, porta);
        let anteprima = installa::anteprima(&v, &piano);
        let barra_non_scrivibile = piano.comando_barra.is_none();
        *self.claude_preparato.borrow_mut() = Some(piano);
        Ok(Preparazione { anteprima, file: cx.file.clone(), barra_non_scrivibile })
    }

    /// L'utente ha letto l'anteprima e ha confermato: ora si scrive. Prima il codice (portachiavi,
    /// variabile), poi il file, poi la configurazione, poi l'ascolto. Se il file non si scrive il
    /// codice nuovo si ritira, così non resta nulla a metà.
    pub fn conferma_collegamento(&self, cx: &Contesto, amb: &dyn AmbienteUtente) -> Result<(), String> {
        cx.controlla()?;
        let piano = self
            .claude_preparato
            .borrow_mut()
            .take()
            .ok_or_else(|| "Prima premi «Collega Claude Code» e leggi che cosa verrà scritto.".to_owned())?;
        // Il file si rilegge ora: se nel frattempo è diventato illeggibile, non si scrive niente.
        let v = installa::leggi(&cx.file).map_err(|e| e.to_string())?;
        let esistente = codice_da(self.segreti.as_ref(), cx.in_prova, amb.leggi(VARIABILE));
        let nuovo = esistente.is_none();
        let codice = match esistente {
            Some(c) => c,
            None => CodiceHook::nuovo()?,
        };
        let testo = codice.come_testo();
        self.metti_codice(cx, amb, &testo)?;
        if let Err(e) = installa::scrivi_sicuro(&cx.file, &installa::unisci(v, &piano)) {
            if nuovo {
                self.ritira_codice(cx, amb);
            }
            return Err(e.to_string());
        }
        self.casa.aggiorna_config(|c| {
            c.claude.porta = Some(piano.porta);
            c.claude.collegato = true;
        });
        self.togli_voce(ID_AGGIORNA);
        avvia_con(self, codice);
        Ok(())
    }

    /// Toglie il collegamento: via le nostre voci da `settings.json` (solo se ce ne sono; il resto
    /// resta), l'ascolto, il codice e la variabile. Un file illeggibile non si riscrive e ferma
    /// tutto.
    pub fn scollega(&self, cx: &Contesto, amb: &dyn AmbienteUtente) -> Result<(), String> {
        cx.controlla()?;
        let v = installa::leggi(&cx.file).map_err(|e| e.to_string())?;
        let senza = installa::togli(v.clone());
        if senza != v {
            installa::scrivi_sicuro(&cx.file, &senza).map_err(|e| e.to_string())?;
        }
        *self.claude_preparato.borrow_mut() = None;
        let ascolto = self.claude_ascolto.borrow_mut().take();
        drop(ascolto);
        self.claude_chiudi_tutti();
        self.casa.aggiorna_config(|c| c.claude.collegato = false);
        self.togli_voce(ID_AGGIORNA);
        self.togli_voce(crate::claude::ID_COLLEGAMENTO);
        // Il codice e la variabile: si prova tutto, si dice il primo errore.
        let mut errore = None;
        if !cx.in_prova {
            if let Err(e) = self.segreti.cancella(SEGRETO_HOOK) {
                errore.get_or_insert(format!("portachiavi: {e}"));
            }
        }
        if amb.gestisce() {
            if let Err(e) = amb.togli(VARIABILE) {
                errore.get_or_insert(e);
            }
        }
        match errore {
            Some(e) => Err(format!("Scollegato, ma non ho potuto cancellare il codice: {e}")),
            None => Ok(()),
        }
    }

    /// Un codice nuovo, ovunque (portachiavi, variabile) e nell'ascolto. `settings.json` non cambia.
    /// I terminali aperti prima hanno ancora il vecchio e finché non si riaprono i loro hook
    /// vengono rifiutati.
    pub fn rigenera_codice(&self, cx: &Contesto, amb: &dyn AmbienteUtente) -> Result<(), String> {
        if !self.casa.con_config(|c| c.claude.collegato) {
            return Err("Claude Code non è collegato.".to_owned());
        }
        let nuovo = CodiceHook::nuovo()?;
        self.metti_codice(cx, amb, &nuovo.come_testo())?;
        // Se la variabile non si scrive `metti_codice` ha già rimesso il portachiavi com'era.
        avvia_con(self, nuovo);
        Ok(())
    }

    /// Scrive il codice nel portachiavi (non in prova) e nella variabile (dove l'app può); se la
    /// seconda scrittura fallisce rimette il portachiavi com'era.
    fn metti_codice(&self, cx: &Contesto, amb: &dyn AmbienteUtente, testo: &str) -> Result<(), String> {
        let precedente = if cx.in_prova { None } else { self.segreti.leggi(SEGRETO_HOOK).ok().flatten() };
        if !cx.in_prova {
            self.segreti.salva(SEGRETO_HOOK, testo).map_err(|e| format!("Non riesco a salvare il codice nel portachiavi: {e}"))?;
        }
        if amb.gestisce() {
            if let Err(e) = amb.imposta(VARIABILE, testo) {
                if !cx.in_prova {
                    match &precedente {
                        Some(p) => {
                            let _ = self.segreti.salva(SEGRETO_HOOK, p);
                        }
                        None => {
                            let _ = self.segreti.cancella(SEGRETO_HOOK);
                        }
                    }
                }
                return Err(format!("Non riesco a impostare la variabile {VARIABILE}: {e}"));
            }
        }
        Ok(())
    }

    /// Ritira un codice appena creato (il file non si è scritto).
    fn ritira_codice(&self, cx: &Contesto, amb: &dyn AmbienteUtente) {
        if !cx.in_prova {
            let _ = self.segreti.cancella(SEGRETO_HOOK);
        }
        if amb.gestisce() {
            let _ = amb.togli(VARIABILE);
        }
    }

    /// Il codice da copiare a mano, dove l'app non scrive la variabile (fuori da Windows).
    pub fn codice_per_la_shell(&self, cx: &Contesto, amb: &dyn AmbienteUtente) -> Option<String> {
        codice_da(self.segreti.as_ref(), cx.in_prova, amb.leggi(VARIABILE)).map(|c| c.come_testo())
    }

    /// Collegato, ma `settings.json` non dice più ciò che questo programma si aspetta (porta,
    /// versione delle voci, comando della barra) o non ha più le nostre voci.
    pub fn collegamento_da_aggiornare(&self, cx: &Contesto) -> bool {
        let (collegato, porta) = self.casa.con_config(|c| (c.claude.collegato, c.claude.porta));
        let (true, Some(porta)) = (collegato, porta) else { return false };
        // Un file che non si legge non si commenta qui: lo dice la sezione Claude alla prima azione.
        let Ok(v) = installa::leggi(&cx.file) else { return false };
        let piano = self.piano_per(cx, porta);
        let atteso = if installa::anteprima(&v, &piano).barra_occupata { None } else { piano.comando_barra };
        !matches!(
            installa::nostre(&v),
            StatoInstallazione::Presente { porta: p, versione, comando_barra }
                if p == porta && versione == VERSIONE_VOCI && comando_barra == atteso
        )
    }

    /// All'avvio: se il collegamento va aggiornato lo si dice con una voce informativa. Non scrive
    /// mai nulla.
    pub fn controlla_all_avvio(&self, cx: &Contesto) {
        if self.collegamento_da_aggiornare(cx) {
            self.metti_voce(
                Voce {
                    id: ID_AGGIORNA.into(),
                    livello: Livello::Informazione,
                    fonte: Fonte::Claude,
                    titolo: "Il collegamento con Claude Code va aggiornato".into(),
                    dettaglio: "Le voci in settings.json non corrispondono più a questo programma. Apri Impostazioni › Claude per aggiornarle: non cambio nulla senza il tuo ok."
                        .into(),
                    pulsanti: vec![Pulsante::new(PULSANTE_IMPOSTAZIONI, "Impostazioni")],
                    creata: Instant::now(),
                },
                true,
            );
        } else {
            self.togli_voce(ID_AGGIORNA);
        }
    }

    /// Dà alla finestra delle Impostazioni lo stato di Claude Code e le sue azioni.
    pub(crate) fn collega_claude(&self, f: &FinestraImpostazioni) {
        let cx = Contesto::reale();
        let amb = crate::sistema::ambiente_utente::ambiente(cx.in_prova);
        f.set_claude_shell(!amb.gestisce());
        self.mostra_stato_claude(f, &cx);
        f.on_claude_prepara(|| {
            if let Some(p) = crate::ponte::ponte() {
                p.claude_prepara_ui();
            }
        });
        f.on_claude_conferma(|| {
            if let Some(p) = crate::ponte::ponte() {
                p.claude_conferma_ui();
            }
        });
        f.on_claude_annulla(|| {
            if let Some(p) = crate::ponte::ponte() {
                *p.claude_preparato.borrow_mut() = None;
            }
        });
        f.on_claude_scollega(|| {
            if let Some(p) = crate::ponte::ponte() {
                p.claude_scollega_ui();
            }
        });
        f.on_claude_rigenera(|| {
            if let Some(p) = crate::ponte::ponte() {
                p.claude_rigenera_ui();
            }
        });
        f.on_claude_codice(|| {
            let Some(p) = crate::ponte::ponte() else { return "".into() };
            let cx = Contesto::reale();
            let amb = crate::sistema::ambiente_utente::ambiente(cx.in_prova);
            p.codice_per_la_shell(&cx, &amb).unwrap_or_default().into()
        });
    }

    fn con_finestra_claude(&self, f: impl FnOnce(&FinestraImpostazioni)) {
        if let Some(finestra) = self.finestra.borrow().upgrade() {
            f(&finestra);
        }
    }

    /// Stato di adesso: «Collegato · porta N», «Non collegato» o «Da aggiornare».
    fn mostra_stato_claude(&self, f: &FinestraImpostazioni, cx: &Contesto) {
        let (collegato, porta) = self.casa.con_config(|c| (c.claude.collegato, c.claude.porta));
        let da_aggiornare = self.collegamento_da_aggiornare(cx);
        let stato = match (collegato, porta) {
            (true, _) if da_aggiornare => "Da aggiornare".to_owned(),
            (true, Some(p)) => format!("Collegato · porta {p}"),
            _ => "Non collegato".to_owned(),
        };
        f.set_claude_stato(stato.into());
        f.set_claude_collegato(collegato);
        f.set_claude_da_aggiornare(da_aggiornare);
        f.set_claude_file(cx.file.display().to_string().into());
    }

    fn esito_claude(&self, f: &FinestraImpostazioni, cx: &Contesto, esito: Result<&str, String>) {
        f.set_claude_passo(0);
        f.set_claude_anteprima("".into());
        f.set_claude_avvisi("".into());
        match esito {
            Ok(testo) => {
                f.set_claude_esito(testo.into());
                f.set_claude_esito_errore(false);
            }
            Err(e) => {
                f.set_claude_esito(e.into());
                f.set_claude_esito_errore(true);
            }
        }
        self.mostra_stato_claude(f, cx);
    }

    fn claude_prepara_ui(&self) {
        let cx = Contesto::reale();
        self.con_finestra_claude(|f| match self.prepara_collegamento(&cx) {
            Ok(prep) => {
                f.set_claude_esito("".into());
                f.set_claude_esito_errore(false);
                f.set_claude_anteprima(prep.anteprima.testo.clone().into());
                f.set_claude_avvisi(prep.avvisi().join("\n").into());
                f.set_claude_file(prep.file.display().to_string().into());
                f.set_claude_passo(1);
            }
            Err(e) => self.esito_claude(f, &cx, Err(e)),
        });
    }

    fn claude_conferma_ui(&self) {
        let cx = Contesto::reale();
        let amb = crate::sistema::ambiente_utente::ambiente(cx.in_prova);
        let esito = self.conferma_collegamento(&cx, &amb);
        self.con_finestra_claude(|f| match esito {
            Ok(()) => self.esito_claude(
                f,
                &cx,
                Ok("Collegato. I terminali aperti prima vanno chiusi e riaperti: solo quelli nuovi hanno il codice."),
            ),
            Err(e) => self.esito_claude(f, &cx, Err(e)),
        });
    }

    fn claude_scollega_ui(&self) {
        let cx = Contesto::reale();
        let amb = crate::sistema::ambiente_utente::ambiente(cx.in_prova);
        let esito = self.scollega(&cx, &amb);
        self.con_finestra_claude(|f| match esito {
            Ok(()) => self.esito_claude(f, &cx, Ok("Scollegato: le voci di isola non sono più in settings.json.")),
            Err(e) => self.esito_claude(f, &cx, Err(e)),
        });
    }

    fn claude_rigenera_ui(&self) {
        let cx = Contesto::reale();
        let amb = crate::sistema::ambiente_utente::ambiente(cx.in_prova);
        let esito = self.rigenera_codice(&cx, &amb);
        self.con_finestra_claude(|f| match esito {
            Ok(()) => self.esito_claude(
                f,
                &cx,
                Ok("Codice rigenerato. I terminali aperti prima vanno chiusi e riaperti: hanno ancora il vecchio."),
            ),
            Err(e) => self.esito_claude(f, &cx, Err(e)),
        });
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::attenzione::Livello;
    use crate::claude::ID_AGGIORNA;
    use crate::ponte::test::ponte_di_prova;
    use crate::sistema::ambiente_utente::{AmbienteFinto, AmbienteUtente, VARIABILE};
        use claude_hook::installa::{self, Piano, StatoInstallazione, VERSIONE_VOCI};
    use tempfile::TempDir;

    fn contesto(d: &TempDir, in_prova: bool) -> Contesto {
        Contesto {
            file: d.path().join("claude").join("settings.json"),
            exe: "C:/x/assistente.exe".into(),
            in_prova,
            corto: |_| None,
        }
    }

    fn scrivi_file(cx: &Contesto, testo: &str) {
        std::fs::create_dir_all(cx.file.parent().unwrap()).unwrap();
        std::fs::write(&cx.file, testo).unwrap();
    }

    const ALTRO: &str = "{\n  \"model\": \"opus\",\n  \"permissions\": { \"allow\": [\"Bash(ls)\"] }\n}\n";

    fn ferma_ascolto(p: &Ponte) {
        let a = p.claude_ascolto.borrow_mut().take();
        drop(a);
    }

    #[test]
    fn collega_e_scollega() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        let amb = AmbienteFinto::default();
        scrivi_file(&cx, ALTRO);

        // L'anteprima non scrive niente: né il file, né la variabile, né il portachiavi.
        let prep = p.prepara_collegamento(&cx).unwrap();
        assert!(prep.anteprima.testo.contains("PermissionRequest"));
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), ALTRO);
        assert_eq!(amb.leggi(VARIABILE), None);
        assert_eq!(amb.scritture.get(), 0);
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap(), None);
        assert!(!p.casa.con_config(|c| c.claude.collegato));

        p.conferma_collegamento(&cx, &amb).unwrap();
        let scritto = std::fs::read_to_string(&cx.file).unwrap();
        let codice = amb.leggi(VARIABILE).expect("variabile impostata");
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap().as_deref(), Some(codice.as_str()), "stesso codice");
        // Il codice non è nel file; il file porta il riferimento alla variabile.
        assert!(!scritto.contains(&codice), "il codice non deve stare in settings.json");
        assert!(scritto.contains("$ISOLA_HOOK_TOKEN"));
        assert!(scritto.contains("\"model\": \"opus\"") && scritto.contains("Bash(ls)"), "il resto resta");
        let (collegato, porta) = p.casa.con_config(|c| (c.claude.collegato, c.claude.porta));
        assert!(collegato);
        let porta = porta.expect("porta salvata");
        assert!(matches!(installa::nostre(&installa::leggi(&cx.file).unwrap()), StatoInstallazione::Presente { porta: q, .. } if q == porta));
        assert_eq!(p.claude_ascolto.borrow().as_ref().map(|a| a.porta()), Some(porta), "ascolta");
        assert!(!p.collegamento_da_aggiornare(&cx));

        p.scollega(&cx, &amb).unwrap();
        let dopo = std::fs::read_to_string(&cx.file).unwrap();
        assert_eq!(installa::nostre(&installa::leggi(&cx.file).unwrap()), StatoInstallazione::Assente);
        assert!(dopo.contains("\"model\": \"opus\"") && dopo.contains("Bash(ls)"), "il resto resta: {dopo}");
        assert!(!dopo.contains("127.0.0.1") && !dopo.contains("hooks"), "le voci di isola sono via: {dopo}");
        assert_eq!(amb.leggi(VARIABILE), None);
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap(), None);
        assert!(!p.casa.con_config(|c| c.claude.collegato));
        assert!(p.claude_ascolto.borrow().is_none(), "non ascolta più");
    }

    #[test]
    fn rigenera_cambia_codice_ovunque() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        let amb = AmbienteFinto::default();
        assert!(p.rigenera_codice(&cx, &amb).is_err(), "non collegato: niente da rigenerare");
        p.prepara_collegamento(&cx).unwrap();
        p.conferma_collegamento(&cx, &amb).unwrap();
        let vecchio = amb.leggi(VARIABILE).unwrap();
        let file_prima = std::fs::read(&cx.file).unwrap();
        let porta = p.claude_ascolto.borrow().as_ref().map(|a| a.porta());
        p.rigenera_codice(&cx, &amb).unwrap();
        let nuovo = amb.leggi(VARIABILE).unwrap();
        assert_ne!(nuovo, vecchio);
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap().as_deref(), Some(nuovo.as_str()));
        assert_eq!(std::fs::read(&cx.file).unwrap(), file_prima, "settings.json non cambia");
        assert_eq!(p.claude_ascolto.borrow().as_ref().map(|a| a.porta()), porta, "ascolta col codice nuovo");
        ferma_ascolto(&p);
    }

    #[test]
    fn all_avvio_niente_scritture() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        let amb = AmbienteFinto::default();
        p.prepara_collegamento(&cx).unwrap();
        p.conferma_collegamento(&cx, &amb).unwrap();
        ferma_ascolto(&p);
        p.controlla_all_avvio(&cx);
        assert!(!p.attese.borrow().contiene(ID_AGGIORNA), "tutto a posto: nessuna voce");

        // Il file ora dice un'altra porta: voce informativa, file e variabile intatti.
        let altra = Piano { porta: 1234, comando_barra: None };
        let v = installa::unisci(installa::togli(installa::leggi(&cx.file).unwrap()), &altra);
        installa::scrivi_sicuro(&cx.file, &v).unwrap();
        let prima = std::fs::read(&cx.file).unwrap();
        let scritture = amb.scritture.get();
        p.controlla_all_avvio(&cx);
        assert_eq!(std::fs::read(&cx.file).unwrap(), prima, "nessuna scrittura senza conferma");
        assert_eq!(amb.scritture.get(), scritture);
        let a = p.attese.borrow();
        let voce = a.voce(ID_AGGIORNA).expect("voce di aggiornamento");
        assert_eq!(voce.livello, Livello::Informazione);
        assert_eq!(voce.titolo, "Il collegamento con Claude Code va aggiornato");
        assert_eq!(voce.pulsanti.len(), 1);
        assert_eq!(voce.pulsanti[0].id, crate::attenzione::PULSANTE_IMPOSTAZIONI);
        drop(a);

        // Anche con le voci sparite dal file, o una versione vecchia, si dice «da aggiornare».
        let senza = installa::togli(installa::leggi(&cx.file).unwrap());
        installa::scrivi_sicuro(&cx.file, &senza).unwrap();
        assert!(p.collegamento_da_aggiornare(&cx));
        assert_eq!(VERSIONE_VOCI, 1);
    }

    #[test]
    fn scollegato_all_avvio_nessuna_voce() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        p.controlla_all_avvio(&cx);
        assert!(!p.attese.borrow().contiene(ID_AGGIORNA));
        assert!(!cx.file.exists(), "nemmeno il file si crea");
    }

    #[test]
    fn file_non_valido_non_si_tocca() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        let amb = AmbienteFinto::default();
        let rotto = "{ // commento\n \"model\": \"opus\" }\n";
        scrivi_file(&cx, rotto);
        assert!(p.prepara_collegamento(&cx).is_err());
        assert!(p.conferma_collegamento(&cx, &amb).is_err(), "senza anteprima non si conferma");
        // Anche forzando la preparazione a monte, il file rotto ferma la conferma prima di ogni scrittura.
        *p.claude_preparato.borrow_mut() = Some(Piano { porta: 1, comando_barra: None });
        assert!(p.conferma_collegamento(&cx, &amb).is_err());
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), rotto);
        assert_eq!(amb.scritture.get(), 0);
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap(), None);
        assert!(!p.casa.con_config(|c| c.claude.collegato));
        assert!(p.scollega(&cx, &amb).is_err(), "anche scollegare non riscrive un file rotto");
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), rotto);
    }

    #[test]
    fn ambiente_che_rifiuta_non_lascia_niente() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        let amb = AmbienteFinto::default();
        scrivi_file(&cx, ALTRO);
        amb.rifiuta.set(true);
        p.prepara_collegamento(&cx).unwrap();
        assert!(p.conferma_collegamento(&cx, &amb).is_err());
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), ALTRO, "il file non cambia");
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap(), None, "il codice nuovo non resta nel portachiavi");
        assert!(!p.casa.con_config(|c| c.claude.collegato));
    }

    #[test]
    fn in_prova_il_portachiavi_vero_non_si_tocca() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, true);
        let amb = AmbienteFinto::default();
        p.prepara_collegamento(&cx).unwrap();
        p.conferma_collegamento(&cx, &amb).unwrap();
        assert!(amb.leggi(VARIABILE).is_some(), "la variabile (di prova) c'è");
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap(), None, "il portachiavi no");
        // Il codice di partenza in prova viene dalla variabile di prova.
        assert!(p.claude_ascolto.borrow().is_some());
        ferma_ascolto(&p);
        p.rigenera_codice(&cx, &amb).unwrap();
        assert_eq!(p.segreti.leggi(SEGRETO_HOOK).unwrap(), None);
        p.scollega(&cx, &amb).unwrap();
        assert_eq!(amb.leggi(VARIABILE), None);
        ferma_ascolto(&p);
    }

    #[test]
    fn senza_ambiente_proprio_il_codice_lo_imposta_l_utente() {
        // Fuori da Windows (`gestisce` falso) la variabile non si scrive: conferma funziona lo stesso.
        struct Nessuno;
        impl AmbienteUtente for Nessuno {
            fn imposta(&self, _: &str, _: &str) -> Result<(), String> {
                Err("solo su Windows".into())
            }
            fn togli(&self, _: &str) -> Result<(), String> {
                Err("solo su Windows".into())
            }
            fn leggi(&self, _: &str) -> Option<String> {
                None
            }
            fn gestisce(&self) -> bool {
                false
            }
        }
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        p.prepara_collegamento(&cx).unwrap();
        p.conferma_collegamento(&cx, &Nessuno).unwrap();
        assert!(p.segreti.leggi(SEGRETO_HOOK).unwrap().is_some(), "il codice sta nel portachiavi");
        assert_eq!(p.codice_per_la_shell(&cx, &Nessuno).unwrap().len(), 64);
        p.scollega(&cx, &Nessuno).unwrap();
        assert!(p.segreti.leggi(SEGRETO_HOOK).unwrap().is_none());
        ferma_ascolto(&p);
    }

    #[test]
    fn avvisi_dell_anteprima_in_italiano() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d, false);
        scrivi_file(
            &cx,
            r#"{ "statusLine": { "type": "command", "command": "altra" }, "allowedHttpHookUrls": ["https://x.example/*"], "disableAllHooks": true }"#,
        );
        let prep = p.prepara_collegamento(&cx).unwrap();
        let avvisi = prep.avvisi().join("\n");
        assert!(avvisi.contains("La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili"), "{avvisi}");
        assert!(avvisi.contains("allowedHttpHookUrls"), "{avvisi}");
        assert!(avvisi.contains("disableAllHooks"), "{avvisi}");
        let pulito = TempDir::new().unwrap();
        let cx2 = contesto(&pulito, false);
        let prep = p.prepara_collegamento(&cx2).unwrap();
        assert!(prep.avvisi().is_empty(), "{:?}", prep.avvisi());
    }
}
