//! Il collegamento a Claude Code dalle Impostazioni (M3, Task 7 e 7B, spec §6.1 e §10).
//!
//! **Mai una scrittura senza il «sì» dell'utente.** `prepara_collegamento` legge e basta: mostra
//! che cosa resterebbe in `settings.json` e che cosa di isola già presente verrebbe sostituito; solo
//! `conferma_collegamento` (il pulsante «Scrivi in settings.json») scrive, e **solo se la modifica è
//! identica a quella letta**: prima di scrivere rilegge il file e rifà l'unione, e se nel frattempo
//! è cambiato qualcosa (comprese le voci vecchie da togliere o la barra di stato) non scrive e
//! mostra la nuova anteprima. All'avvio `controlla_all_avvio` non scrive mai: al più mette una
//! voce informativa.
//!
//! Il canale è il tubo protetto con l'aiutante `isola-hook.exe` (spec §10): nessun codice, nessuna
//! variabile d'ambiente, nessuna porta, nessun segreto nel portachiavi. Le voci di `settings.json`
//! lanciano l'aiutante accanto a questo programma.

use super::NOME_AIUTANTE;
use crate::attenzione::{Fonte, Livello, Pulsante, Voce, PULSANTE_IMPOSTAZIONI};
use crate::claude::ID_AGGIORNA;
use crate::ponte::Ponte;
use crate::FinestraImpostazioni;
use claude_hook::installa::{self, Anteprima, Piano, Scrittura, Stato};
use claude_hook::tubo::ConfigTubo;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Dove e come si installa: il file di Claude Code, il programma accanto a cui sta l'aiutante, e
/// il tubo da aprire dopo la scrittura. I test lo costruiscono sopra una cartella temporanea; il
/// programma vero con `Contesto::reale`.
pub struct Contesto {
    pub file: PathBuf,
    pub exe: PathBuf,
    /// Il percorso corto 8.3 di un file (sui percorsi con spazi); `None` se non c'è.
    pub corto: fn(&Path) -> Option<PathBuf>,
    /// Il tubo da aprire dopo la scrittura (`None`: non si apre, per i test e per chi non ha il nome).
    pub tubo: Option<ConfigTubo>,
}

impl Contesto {
    /// In prova (`ITSYHOME_ASSISTENTE_DIR`) il file è `<cartella di prova>\claude-finto\settings.json`:
    /// il vero `settings.json` non si tocca.
    pub fn reale() -> Contesto {
        let cartella_prova = std::env::var_os(crate::avvio::VAR_CARTELLA).filter(|v| !v.is_empty());
        let file = match &cartella_prova {
            Some(d) => Path::new(d).join("claude-finto").join("settings.json"),
            None => {
                let config = std::env::var_os("CLAUDE_CONFIG_DIR");
                let home = std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .map(PathBuf::from);
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
            corto: crate::sistema::percorso::percorso_corto,
            tubo: super::config_tubo(),
        }
    }

    /// `isola-hook.exe` accanto a questo programma: ciò che le voci lanciano.
    fn aiutante(&self) -> PathBuf {
        self.exe.with_file_name(NOME_AIUTANTE)
    }

    fn piano(&self) -> Piano {
        let aiutante = self.aiutante();
        let comando_barra = installa::comando_barra(&aiutante, self.corto);
        Piano {
            aiutante,
            comando_barra,
        }
    }

    fn controlla(&self) -> Result<(), String> {
        if self.file.as_os_str().is_empty() {
            return Err(
                "Non trovo la cartella di Claude Code (manca la cartella dell'utente).".into(),
            );
        }
        Ok(())
    }

    /// Come `controlla`, e l'aiutante c'è (scrivere voci che lanciano un file che non esiste
    /// farebbe fallire ogni hook).
    fn controlla_per_scrivere(&self) -> Result<(), String> {
        self.controlla()?;
        let a = self.aiutante();
        if !self.piano().percorso_valido() {
            return Err(format!(
                "Il percorso di {NOME_AIUTANTE} non è un testo UTF-8 valido e non si può scrivere in settings.json: {}",
                a.display()
            ));
        }
        if !a.is_file() {
            return Err(format!(
                "Non trovo {NOME_AIUTANTE} accanto al programma ({}): reinstalla isola.",
                a.display()
            ));
        }
        Ok(())
    }
}

/// Il messaggio quando, fra l'anteprima e «Scrivi», il file è cambiato in modo che conta.
pub const FILE_CAMBIATO_DOPO_L_ANTEPRIMA: &str =
    "settings.json è cambiato dopo l'anteprima: controlla di nuovo ciò che verrà scritto prima di confermare.";

/// Che cosa l'utente vede prima di confermare.
pub struct Preparazione {
    pub anteprima: Anteprima,
    /// Il file che si scriverebbe.
    pub file: PathBuf,
    /// Il programma sta in un percorso che la shell di Claude Code non può leggere e senza nome
    /// corto: la barra di stato non si configura.
    pub barra_non_scrivibile: bool,
    /// Questa è la seconda anteprima: il file è cambiato dopo la prima (`FILE_CAMBIATO_DOPO_L_ANTEPRIMA`).
    pub cambiato: bool,
}

impl Preparazione {
    /// Le frasi di avviso, una per riga, nell'ordine in cui vanno lette.
    pub fn avvisi(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.cambiato {
            v.push(FILE_CAMBIATO_DOPO_L_ANTEPRIMA.to_owned());
        }
        for s in &self.anteprima.sostituite {
            v.push(format!("Già in settings.json · {s}"));
        }
        if self.anteprima.hook_disabilitati {
            v.push(
                "«disableAllHooks» è attivo in settings.json: finché resta così, Claude Code non esegue né gli hook né la barra di stato."
                    .to_owned(),
            );
        }
        if self.anteprima.barra_occupata {
            v.push(
                "La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili."
                    .to_owned(),
            );
        } else if self.barra_non_scrivibile {
            v.push(
                "Il programma sta in un percorso con spazi e senza nome corto: la barra di stato non si configura e i limiti dell'abbonamento non saranno disponibili."
                    .to_owned(),
            );
        }
        v
    }
}

/// L'esito di «Scrivi in settings.json».
pub enum Esito {
    /// Scritto: il collegamento c'è e il tubo si è aperto (o ha la sua voce d'errore).
    Scritto,
    /// Il file è cambiato dopo l'anteprima: **nulla è stato scritto**; ecco la nuova anteprima.
    Cambiato(Preparazione),
}

impl Ponte {
    fn preparazione(&self, cx: &Contesto, anteprima: Anteprima, cambiato: bool) -> Preparazione {
        Preparazione {
            anteprima,
            file: cx.file.clone(),
            barra_non_scrivibile: cx.piano().comando_barra.is_none(),
            cambiato,
        }
    }

    /// Legge `settings.json` e prepara il collegamento: l'anteprima delle voci di isola come
    /// resteranno e di ciò che si sostituisce. **Non scrive nulla.**
    pub fn prepara_collegamento(&self, cx: &Contesto) -> Result<Preparazione, String> {
        cx.controlla_per_scrivere()?;
        let v = installa::leggi(&cx.file).map_err(|e| e.to_string())?;
        let anteprima = installa::anteprima(&v, &cx.piano());
        *self.claude_preparato.borrow_mut() = Some(anteprima.clone());
        Ok(self.preparazione(cx, anteprima, false))
    }

    /// «Scrivi in settings.json»: rilegge il file, rifà l'unione e la confronta con l'anteprima che
    /// l'utente ha letto. Uguale → scrive (con la riserva alla prima volta), segna il collegamento
    /// e apre il tubo. Diversa → **non scrive** e restituisce la nuova anteprima. L'anteprima si
    /// consuma a ogni conferma: senza una letta non si scrive.
    pub fn conferma_collegamento(&self, cx: &Contesto) -> Result<Esito, String> {
        cx.controlla_per_scrivere()?;
        let Some(letta) = self.claude_preparato.borrow_mut().take() else {
            return Err("Prima leggi l'anteprima di ciò che verrà scritto.".into());
        };
        let v = installa::leggi(&cx.file).map_err(|e| e.to_string())?;
        let piano = cx.piano();
        let ora = installa::anteprima(&v, &piano);
        if ora != letta {
            *self.claude_preparato.borrow_mut() = Some(ora.clone());
            return Ok(Esito::Cambiato(self.preparazione(cx, ora, true)));
        }
        // Ultimo controllo dentro la scrittura: se il file cambia fra la lettura e il `rename` non si
        // scrive e si mostra la nuova anteprima (resta solo la finestra di microsecondi del rename).
        let unito = installa::unisci(v.clone(), &piano);
        match installa::scrivi_se_invariato(&cx.file, &v, &unito).map_err(|e| e.to_string())? {
            Scrittura::Scritto => {}
            Scrittura::Cambiato => {
                let v = installa::leggi(&cx.file).map_err(|e| e.to_string())?;
                let nuova = installa::anteprima(&v, &piano);
                *self.claude_preparato.borrow_mut() = Some(nuova.clone());
                return Ok(Esito::Cambiato(self.preparazione(cx, nuova, true)));
            }
        }
        self.casa.aggiorna_config(|c| c.claude.collegato = true);
        self.togli_voce(ID_AGGIORNA);
        if let Some(tubo) = cx.tubo.clone() {
            super::avvia_con(self, tubo);
        }
        Ok(Esito::Scritto)
    }

    /// Toglie il collegamento: via le nostre voci da `settings.json` (solo se ce ne sono; il resto
    /// resta) e il tubo. Un file illeggibile non si riscrive e ferma tutto.
    pub fn scollega(&self, cx: &Contesto) -> Result<(), String> {
        cx.controlla()?;
        let v = installa::leggi(&cx.file).map_err(|e| e.to_string())?;
        let senza = installa::togli(v.clone());
        if senza != v {
            installa::scrivi_sicuro(&cx.file, &senza).map_err(|e| e.to_string())?;
        }
        *self.claude_preparato.borrow_mut() = None;
        self.claude_ferma_tubo();
        self.casa.aggiorna_config(|c| c.claude.collegato = false);
        self.togli_voce(ID_AGGIORNA);
        self.togli_voce(crate::claude::ID_COLLEGAMENTO);
        Ok(())
    }

    /// Collegato, ma `settings.json` non dice più ciò che questo programma si aspetta: voci
    /// mancanti (anche una sola), di un altro percorso o forma, vecchie HTTP, barra cambiata, o
    /// nessuna voce nostra.
    pub fn collegamento_da_aggiornare(&self, cx: &Contesto) -> bool {
        if !self.casa.con_config(|c| c.claude.collegato) {
            return false;
        }
        // Un file che non si legge non si commenta qui: lo dice la sezione Claude alla prima azione.
        let Ok(v) = installa::leggi(&cx.file) else {
            return false;
        };
        installa::stato(&v, &cx.piano()) != Stato::Aggiornato
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
        f.set_claude_disponibile(cfg!(windows));
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
    }

    fn con_finestra_claude(&self, f: impl FnOnce(&FinestraImpostazioni)) {
        if let Some(finestra) = self.finestra.borrow().upgrade() {
            f(&finestra);
        }
    }

    /// Stato di adesso: «Collegato», «Non collegato», «Da aggiornare» o «Disponibile solo su Windows».
    fn mostra_stato_claude(&self, f: &FinestraImpostazioni, cx: &Contesto) {
        let collegato = self.casa.con_config(|c| c.claude.collegato);
        let da_aggiornare = self.collegamento_da_aggiornare(cx);
        let stato = match (cfg!(windows), collegato) {
            (false, _) => "Disponibile solo su Windows",
            (true, true) if da_aggiornare => "Da aggiornare",
            (true, true) => "Collegato",
            (true, false) => "Non collegato",
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

    fn mostra_anteprima_claude(&self, f: &FinestraImpostazioni, prep: &Preparazione) {
        f.set_claude_esito("".into());
        f.set_claude_esito_errore(false);
        f.set_claude_anteprima(prep.anteprima.testo.clone().into());
        f.set_claude_avvisi(prep.avvisi().join("\n").into());
        f.set_claude_file(prep.file.display().to_string().into());
        f.set_claude_passo(1);
    }

    fn claude_prepara_ui(&self) {
        let cx = Contesto::reale();
        self.con_finestra_claude(|f| match self.prepara_collegamento(&cx) {
            Ok(prep) => self.mostra_anteprima_claude(f, &prep),
            Err(e) => self.esito_claude(f, &cx, Err(e)),
        });
    }

    fn claude_conferma_ui(&self) {
        let cx = Contesto::reale();
        let esito = self.conferma_collegamento(&cx);
        self.con_finestra_claude(|f| match esito {
            Ok(Esito::Scritto) => self.esito_claude(f, &cx, Ok("Collegato.")),
            // Nulla scritto: si resta all'anteprima, aggiornata, con l'avviso in cima.
            Ok(Esito::Cambiato(prep)) => self.mostra_anteprima_claude(f, &prep),
            Err(e) => self.esito_claude(f, &cx, Err(e)),
        });
    }

    fn claude_scollega_ui(&self) {
        let cx = Contesto::reale();
        let esito = self.scollega(&cx);
        self.con_finestra_claude(|f| match esito {
            Ok(()) => self.esito_claude(
                f,
                &cx,
                Ok("Scollegato: le voci di isola non sono più in settings.json."),
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
    use serde_json::{json, Value};
    use tempfile::TempDir;

    /// Un contesto sopra una cartella temporanea: `app\assistente.exe` e `app\isola-hook.exe` (vuoti,
    /// bastano a esistere), il file finto in `claude\settings.json`, nessun tubo da aprire.
    fn contesto(d: &TempDir) -> Contesto {
        let app = d.path().join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("assistente.exe"), b"").unwrap();
        std::fs::write(app.join(NOME_AIUTANTE), b"").unwrap();
        Contesto {
            file: d.path().join("claude").join("settings.json"),
            exe: app.join("assistente.exe"),
            corto: |_| None,
            tubo: None,
        }
    }

    #[cfg(windows)]
    fn tubo_di_prova() -> ConfigTubo {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        ConfigTubo {
            nome: format!(
                r"\\.\pipe\isola-prova-collegamento-{}-{}-{t}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ),
            cliente_atteso: std::env::current_exe().unwrap(),
            attesa_massima: claude_hook::ATTESA_MASSIMA,
        }
    }

    /// Il tubo da aprire nel test di conferma: uno vero (con nome unico) su Windows, nessuno altrove.
    #[cfg(windows)]
    fn tubo_per_prova() -> Option<ConfigTubo> {
        Some(tubo_di_prova())
    }

    #[cfg(not(windows))]
    fn tubo_per_prova() -> Option<ConfigTubo> {
        None
    }

    fn scrivi_file(cx: &Contesto, testo: &str) {
        std::fs::create_dir_all(cx.file.parent().unwrap()).unwrap();
        std::fs::write(&cx.file, testo).unwrap();
    }

    fn letto(cx: &Contesto) -> Value {
        installa::leggi(&cx.file).unwrap()
    }

    fn bak(cx: &Contesto) -> PathBuf {
        cx.file.with_file_name("settings.json.isola-bak")
    }

    const ALTRO: &str =
        "{\n  \"model\": \"opus\",\n  \"permissions\": { \"allow\": [\"Bash(ls)\"] }\n}\n";

    /// Il file con le voci HTTP del Task 7 (come le scriveva) e la vecchia barra.
    fn file_del_task_7() -> String {
        let voce = r#"{ "hooks": [ { "type": "http", "url": "http://127.0.0.1:4321/isola/v1/evento", "headers": { "Authorization": "Bearer $ISOLA_HOOK_TOKEN" }, "allowedEnvVars": ["ISOLA_HOOK_TOKEN"], "timeout": 5 } ] }"#;
        let eventi = installa::EVENTI
            .iter()
            .map(|e| format!("\"{e}\": [{voce}]"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{ "model": "opus", "hooks": {{ {eventi} }}, "statusLine": {{ "type": "command", "command": "C:/x/assistente.exe --barra-stato 4321" }} }}"#
        )
    }

    /// Collega davvero, passando da anteprima e conferma.
    fn collega(p: &Ponte, cx: &Contesto) {
        p.prepara_collegamento(cx).unwrap();
        assert!(matches!(p.conferma_collegamento(cx), Ok(Esito::Scritto)));
    }

    #[test]
    fn conferma_scrive_solo_dopo_l_anteprima_e_conserva_il_resto() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = Contesto {
            tubo: tubo_per_prova(),
            ..contesto(&d)
        };
        scrivi_file(&cx, ALTRO);
        // L'anteprima non scrive nulla.
        let prep = p.prepara_collegamento(&cx).unwrap();
        assert!(
            prep.anteprima.testo.contains("PermissionRequest")
                && prep.anteprima.testo.contains("isola-hook.exe")
        );
        assert_eq!(
            std::fs::read_to_string(&cx.file).unwrap(),
            ALTRO,
            "file intatto"
        );
        assert!(!bak(&cx).exists() && !p.casa.con_config(|c| c.claude.collegato));
        assert!(p.claude_ascolto.borrow().is_none());
        // La conferma scrive: voci in forma exec, il resto com'era, riserva col file di prima.
        assert!(matches!(p.conferma_collegamento(&cx), Ok(Esito::Scritto)));
        let v = letto(&cx);
        assert_eq!(v["model"], "opus");
        assert_eq!(v["permissions"], json!({ "allow": ["Bash(ls)"] }));
        let aiutante = cx.aiutante();
        assert_eq!(
            v["hooks"]["PermissionRequest"][0]["hooks"][0]["command"],
            json!(aiutante.to_string_lossy())
        );
        assert_eq!(installa::stato(&v, &cx.piano()), Stato::Aggiornato);
        assert_eq!(std::fs::read_to_string(bak(&cx)).unwrap(), ALTRO);
        assert!(p.casa.con_config(|c| c.claude.collegato));
        assert!(
            p.claude_preparato.borrow().is_none(),
            "l'anteprima è consumata"
        );
        #[cfg(windows)]
        {
            assert!(p.claude_ascolto.borrow().is_some(), "il tubo si è aperto");
            super::super::spegni();
        }
        assert!(!p.collegamento_da_aggiornare(&cx));
    }

    #[test]
    fn conferma_senza_anteprima_non_scrive() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, ALTRO);
        assert!(p.conferma_collegamento(&cx).is_err());
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), ALTRO);
        // Anche una conferma ripetuta dopo una riuscita: l'anteprima è consumata.
        collega(&p, &cx);
        let prima = std::fs::read(&cx.file).unwrap();
        assert!(p.conferma_collegamento(&cx).is_err());
        assert_eq!(std::fs::read(&cx.file).unwrap(), prima);
        // Senza file: nemmeno lo si crea.
        let (_s2, p2, _pillola2, d2) = ponte_di_prova();
        let cx2 = contesto(&d2);
        assert!(p2.conferma_collegamento(&cx2).is_err());
        assert!(!cx2.file.exists());
    }

    #[test]
    fn file_cambiato_dopo_l_anteprima() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, ALTRO);
        let prima = p.prepara_collegamento(&cx).unwrap();
        assert!(!prima.anteprima.barra_occupata && prima.anteprima.sostituite.is_empty());
        // Nel frattempo arrivano le vecchie voci HTTP di isola e la sua vecchia barra.
        let cambiato = file_del_task_7();
        scrivi_file(&cx, &cambiato);
        let esito = p.conferma_collegamento(&cx).unwrap();
        let Esito::Cambiato(nuova) = esito else {
            panic!("doveva non scrivere")
        };
        assert_eq!(
            std::fs::read_to_string(&cx.file).unwrap(),
            cambiato,
            "nulla scritto"
        );
        assert!(!bak(&cx).exists() && !p.casa.con_config(|c| c.claude.collegato));
        assert!(nuova.cambiato && nuova.avvisi()[0] == FILE_CAMBIATO_DOPO_L_ANTEPRIMA);
        assert!(
            nuova.anteprima.sostituite.len() >= 8,
            "le voci vecchie e la barra si vedono: {:?}",
            nuova.anteprima.sostituite
        );
        // Con la nuova anteprima in mano, la seconda conferma scrive.
        assert!(matches!(p.conferma_collegamento(&cx), Ok(Esito::Scritto)));
        let t = std::fs::read_to_string(&cx.file).unwrap();
        assert!(
            !t.contains("127.0.0.1")
                && !t.contains("--barra-stato")
                && t.contains("isola-hook.exe"),
            "{t}"
        );
    }

    #[test]
    fn una_modifica_altrui_che_non_cambia_la_nostra_scrive_e_la_conserva() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, ALTRO);
        p.prepara_collegamento(&cx).unwrap();
        // L'utente ha cambiato il modello: la nostra modifica è la stessa, si scrive e il cambio resta.
        scrivi_file(&cx, &ALTRO.replace("opus", "sonnet"));
        assert!(matches!(p.conferma_collegamento(&cx), Ok(Esito::Scritto)));
        assert_eq!(letto(&cx)["model"], "sonnet");
    }

    #[test]
    fn migrazione_dalle_voci_http() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, &file_del_task_7());
        let prep = p.prepara_collegamento(&cx).unwrap();
        assert_eq!(
            prep.anteprima
                .sostituite
                .iter()
                .filter(|s| s.contains("HTTP"))
                .count(),
            7
        );
        let avvisi = prep.avvisi().join("\n");
        assert!(
            avvisi.contains("Già in settings.json") && avvisi.contains("verrà tolta"),
            "{avvisi}"
        );
        assert!(matches!(p.conferma_collegamento(&cx), Ok(Esito::Scritto)));
        let t = std::fs::read_to_string(&cx.file).unwrap();
        assert!(
            !t.contains("http") && !t.contains("Bearer") && !t.contains("--barra-stato"),
            "{t}"
        );
        assert_eq!(installa::stato(&letto(&cx), &cx.piano()), Stato::Aggiornato);
        // Il file aveva già voci nostre: nessuna riserva (non è il file di prima di isola).
        assert!(!bak(&cx).exists());
    }

    #[test]
    fn nessun_segreto_scritto() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, ALTRO);
        collega(&p, &cx);
        // Cosa controlla e cosa no: solo ciò che il codice di isola può scrivere (nessuna voce nel
        // portachiavi di prova; nel file nessun codice, variabile, URL o intestazione). Non può
        // provare l'assenza di scritture nel registro (`HKCU\Environment`): il modulo che le faceva
        // non esiste più, e questo lo garantisce la revisione del codice, non il test.
        // Né nel portachiavi (nessuna voce), né nel file (nessun codice né riferimento a una variabile).
        assert_eq!(p.segreti.leggi("isola:claude-hook").unwrap(), None);
        let t = std::fs::read_to_string(&cx.file).unwrap();
        for vietato in [
            "ISOLA_HOOK_TOKEN",
            "Bearer",
            "headers",
            "allowedEnvVars",
            "127.0.0.1",
            "localhost",
        ] {
            assert!(!t.contains(vietato), "{vietato} in {t}");
        }
        // Scollegare non tocca nemmeno il portachiavi: nessuna scrittura né cancellazione.
        p.scollega(&cx).unwrap();
        assert_eq!(p.segreti.leggi("isola:claude-hook").unwrap(), None);
    }

    #[test]
    fn scollega_toglie_le_voci_e_lascia_il_resto() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, ALTRO);
        collega(&p, &cx);
        assert!(!p.collegamento_da_aggiornare(&cx));
        p.scollega(&cx).unwrap();
        let dopo = std::fs::read_to_string(&cx.file).unwrap();
        assert!(!installa::ha_nostre(&letto(&cx)));
        assert!(
            dopo.contains("\"model\": \"opus\"") && dopo.contains("Bash(ls)"),
            "il resto resta: {dopo}"
        );
        assert!(
            !dopo.contains("isola-hook") && !dopo.contains("hooks"),
            "le voci di isola sono via: {dopo}"
        );
        assert!(!p.casa.con_config(|c| c.claude.collegato));
        assert!(p.claude_ascolto.borrow().is_none());
        // La riserva resta quella di prima di isola.
        assert_eq!(std::fs::read_to_string(bak(&cx)).unwrap(), ALTRO);
    }

    #[test]
    fn scollega_ferma_il_tubo_e_gli_avvisi_vecchi_non_riaprono_nulla() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, ALTRO);
        collega(&p, &cx);
        let prima = p.claude_generazione.get();
        p.scollega(&cx).unwrap();
        assert!(
            p.claude_generazione.get() > prima,
            "il numero del tubo sale"
        );
        p.claude_canale_perso(prima);
        assert!(!p.attese.borrow().contiene(crate::claude::ID_COLLEGAMENTO));
    }

    #[test]
    fn all_avvio_niente_scritture() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, "{}");
        collega(&p, &cx);
        p.controlla_all_avvio(&cx);
        assert!(
            !p.attese.borrow().contiene(ID_AGGIORNA),
            "tutto a posto: nessuna voce"
        );

        let controlla = |contenuto: Value, perche: &str| {
            installa::scrivi_sicuro(&cx.file, &contenuto).unwrap();
            let prima = std::fs::read(&cx.file).unwrap();
            let riserva = bak(&cx).exists();
            p.controlla_all_avvio(&cx);
            assert_eq!(
                std::fs::read(&cx.file).unwrap(),
                prima,
                "{perche}: nessuna scrittura senza conferma"
            );
            assert_eq!(bak(&cx).exists(), riserva);
            let a = p.attese.borrow();
            let voce = a
                .voce(ID_AGGIORNA)
                .unwrap_or_else(|| panic!("{perche}: voce di aggiornamento"));
            assert_eq!(voce.livello, Livello::Informazione, "{perche}");
            assert_eq!(voce.titolo, "Il collegamento con Claude Code va aggiornato");
            assert_eq!(voce.pulsanti.len(), 1);
            assert_eq!(
                voce.pulsanti[0].id,
                crate::attenzione::PULSANTE_IMPOSTAZIONI
            );
        };
        let buono = letto(&cx);
        // Un altro percorso.
        let altro = Piano {
            aiutante: PathBuf::from(r"D:\vecchio\isola-hook.exe"),
            comando_barra: None,
        };
        controlla(
            installa::unisci(installa::togli(buono.clone()), &altro),
            "altro percorso",
        );
        // Un evento mancante.
        let mut manca = buono.clone();
        manca["hooks"]
            .as_object_mut()
            .unwrap()
            .shift_remove("SessionEnd");
        controlla(manca, "evento mancante");
        // Forma diversa (async tolto).
        let mut forma = buono.clone();
        forma["hooks"]["Stop"][0]["hooks"][0]
            .as_object_mut()
            .unwrap()
            .shift_remove("async");
        controlla(forma, "forma diversa");
        // Vecchie voci HTTP.
        controlla(
            serde_json::from_str(&file_del_task_7()).unwrap(),
            "voci HTTP",
        );
        // Tutto sparito.
        controlla(installa::togli(buono), "voci sparite");
        // E rimesso a posto la voce se ne va.
        let cx2 = contesto(&d);
        assert!(p.collegamento_da_aggiornare(&cx2));
        collega(&p, &cx2);
        p.controlla_all_avvio(&cx2);
        assert!(!p.attese.borrow().contiene(ID_AGGIORNA));
    }

    #[test]
    fn scollegato_all_avvio_nessuna_voce() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        p.controlla_all_avvio(&cx);
        assert!(!p.attese.borrow().contiene(ID_AGGIORNA));
        assert!(!cx.file.exists(), "nemmeno il file si crea");
    }

    #[test]
    fn file_non_valido_non_si_tocca() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        let rotto = "{ // commento\n \"model\": \"opus\" }\n";
        scrivi_file(&cx, rotto);
        assert!(p.prepara_collegamento(&cx).is_err());
        assert!(
            p.conferma_collegamento(&cx).is_err(),
            "senza anteprima non si conferma"
        );
        // Anche forzando un'anteprima a monte, il file rotto ferma la conferma prima di ogni scrittura.
        *p.claude_preparato.borrow_mut() = Some(installa::anteprima(&json!({}), &cx.piano()));
        assert!(p.conferma_collegamento(&cx).is_err());
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), rotto);
        assert!(!bak(&cx).exists());
        assert!(!p.casa.con_config(|c| c.claude.collegato));
        assert!(
            p.scollega(&cx).is_err(),
            "anche scollegare non riscrive un file rotto"
        );
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), rotto);
    }

    #[test]
    fn senza_aiutante_non_si_scrive() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(&cx, ALTRO);
        std::fs::remove_file(cx.aiutante()).unwrap();
        let e = p.prepara_collegamento(&cx).err().expect("errore");
        assert!(e.contains("isola-hook.exe"), "{e}");
        assert!(p.conferma_collegamento(&cx).is_err());
        assert_eq!(std::fs::read_to_string(&cx.file).unwrap(), ALTRO);
    }

    #[test]
    fn avvisi_dell_anteprima_in_italiano() {
        let (_s, p, _pillola, d) = ponte_di_prova();
        let cx = contesto(&d);
        scrivi_file(
            &cx,
            r#"{ "statusLine": { "type": "command", "command": "altra" }, "disableAllHooks": true }"#,
        );
        let prep = p.prepara_collegamento(&cx).unwrap();
        let avvisi = prep.avvisi().join("\n");
        assert!(
            avvisi.contains(
                "La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili"
            ),
            "{avvisi}"
        );
        assert!(avvisi.contains("disableAllHooks"), "{avvisi}");
        assert!(
            !avvisi.contains("allowedHttpHookUrls"),
            "niente più elenchi di URL"
        );
        let pulito = TempDir::new().unwrap();
        let cx2 = contesto(&pulito);
        let prep = p.prepara_collegamento(&cx2).unwrap();
        assert!(prep.avvisi().is_empty(), "{:?}", prep.avvisi());
    }
}
