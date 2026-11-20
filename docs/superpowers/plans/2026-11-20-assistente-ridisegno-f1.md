# Assistente "isola" — Ridisegno, fetta F1 (contratto, cornice, Panoramica, Coda minima) — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** isola diventa una piattaforma di moduli con l'aspetto di TwoCode ottimizzato: contratto `Riassunto`/`Registro` per i 4 moduli di oggi (Agenti = Claude, Casa, PC, Collegamenti), pannello nero 800 × 340 con riga alta e barra a capsula, Panoramica Griglia (predefinita) e Orologio, Coda minima, token e caratteri nuovi in tutta l'app; il resto delle pagine resta quello di oggi, ripassato sulla base nera.

**Architecture:** uno strato di **funzioni pure** sopra `Ponte` (spec §5.10, «adesso funzioni pure, non un rifacimento»): `src/moduli/` calcola un `Riassunto` per modulo e un `Registro` lo valida in pixel e scrive i modelli Slint solo se cambia; la colla col `Ponte` sta in `src/ponte/con_moduli.rs` (figlio di `ponte.rs`, come `con_claude.rs`). In Slint la cornice (`ui/guscio/`) possiede misure, barra, Panoramica e pool di fuoco; la Coda usa il primo impianto (`ui/impianti/elenco.slint`). Le pagine Casa, Claude, PC e Collegamenti di oggi restano e ricevono solo token e contrasti.

**Tech Stack:** Rust 2021, Slint `=1.18.1` (Skia software), `ttf-parser =0.25.1` (già nel `Cargo.lock`, per misurare i testi coi caratteri incorporati), `windows-sys 0.52`, chrono.

**Spec:** `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (vincolante; **§13 «Esiti dello spike» prevale** sui punti corrispondenti). Si leggono con lei: `.superpowers/ux/studio/11-spike.md` (tecniche provate), `.superpowers/ux/studio/08-fattibilita-sistema.md` (contratto, pool di fuoco, test da riscrivere §7.3), `.superpowers/ux/studio/10-tc-isola.md` e la bozza `bozze/tc-isola.html|.css|.js` (misure fini: `tc-isola.css`), schermate `bozze/screenshot/tc-*.png`. Lo spike usa e getta (non nel repo) ha gli schemi Slint esatti: `%TEMP%\claude\2b047669b78133cd\1564b5b0-5c1f-4988-87e2-9b15e9fd6dac\scratchpad\spike-ridisegno\ui\spike.slint` (componenti `Luce`, `Posto`, `Scheda`, `Occhietti`, contorno `Path`, regole `changed` del fuoco) e `src/lib.rs` (`ordine_tab`, `prossimo`, `geometria_schede`); i caratteri e le licenze OFL già scaricati stanno nella sua cartella `font\`. Stile: `docs/superpowers/plans/2026-11-18-assistente-grafica.md`, `2026-11-17-assistente-m3-claude.md`.

## Decisioni prese dal piano (dove la spec lascia la scelta al costruttore, §10)

- **File**: Rust in `src/moduli/{mod,misura,registro,panoramica,agenti,casa,pc,collegamenti}.rs` + `src/ponte/con_moduli.rs`; Slint in `ui/guscio/{luce,cornice,carta,panoramica,voce}.slint` e `ui/impianti/elenco.slint`. `src/registro.rs`, `src/layout.rs`, `src/scelte.rs`, `ui/layout/`, `ui/attesa.slint` spariscono nei task indicati.
- **Solo ciò che F1 legge**: `Riassunto` ha in F1 `tono`, `titolo`, `titolo_numerico`, `sec`, `sec_intera`, `azione`; `Identita` ha `id`, `nome`, `nome_breve`, `icona`. `r0`, `r1`, `misura`, `valori`, `sec_extra`, `nome_file`, `avvisi`, `aggiornato_alle`, `impianto` e `delicata` arrivano con la fetta che li usa (F2–F5): niente campi mai letti (`dead_code`, clippy `-D warnings`).
- **Nomi**: l'enum dei toni in Slint è `TonoModuloUi` (esiste già `TonoUi` per gli esiti delle Impostazioni); la scheda della barra è `SchedaBarraUi` (la gestione ha già `SchedaUi`). L'id del modulo e della vista diventa `agenti` (§5.10); la chiave `claude` della configurazione di Claude (`ConfigClaude`) **non** si rinomina: è la sezione propria del fornitore.
- **Nome breve** (capsula presente, §13.5): Agenti «Agenti», Casa «Casa», PC «PC», Collegamenti «Link» *(scelta mia: «COLLEGAMENTI» non sta accanto alla capsula più larga)*.
- **Ricalcolo dei riassunti solo a pannello aperto** (e una volta all'apertura), con lo schema `modelli_vivi`/`modelli_vecchi` che `Ponte` usa già: da chiuso nessuno li guarda (la pillola cambia in F4).
- **Tono effettivo** = il più urgente fra quello del riassunto e il livello delle voci del modulo (bloccante → Aspetta, attenzione → Guarda): i moduli non ripetono nel riassunto ciò che le voci dicono già.
- **Luce dell'isola** = il tono più urgente fra Attivo, Guarda, Aspetta di tutti i moduli (gli altri toni non accendono).
- **Occhi del pannello**: restano il componente `Occhi` di oggi con i token nuovi (gli occhi nuovi arrivano con la pillola in F4, perché i due devono coincidere all'apertura).
- **Voce che ti aspetta in F1**: capsula «Ti aspetta · <modulo>», titolo, prima riga del dettaglio (in mono per i permessi), «Nega» (fuoco) / «Consenti» bianco, «+N in attesa ›». Comando con «$» e cartella a destra arrivano col componente permesso in F2.
- **Consenti è il primario bianco** (spec §5.2 «bianco = primaria», §5.8, schermate `tc-griglia-aspetta`, `tc-coda`): cambia la regola M3 «Nega/Consenti neutri»; tutte le altre protezioni di M3 restano (§7).
- **Meteo nella riga alta**: temperatura + condizione a parole (mono maiuscolo come la data), nessun glifo (§13.10); le icone del meteo non entrano in F1.
- **Ricerca**: pulsante «Ctrl K» presente ma inerte in F1 (spec §8); nome per Narrator «Cerca, in arrivo». Ctrl+K, Ctrl+0…6 e l'esclusione di AltGr con `GetKeyState` (§13.2) arrivano con F5/F6, dove nascono le scorciatoie.

## Global Constraints

- Ramo `v3-assistente`; `main` non si tocca. Ogni commit locale con l'ok di Paolo; nessun push, tag o merge senza un suo sì esplicito.
- Testi per l'utente in italiano, verbo in testa sui pulsanti; commenti, commit e CHANGELOG in italiano. Ogni task aggiorna `CHANGELOG.md` (sezione Unreleased v3, «Ridisegno F1») nel proprio commit.
- **Leggerezza** (spec §3): nessuna animazione continua; animazioni solo durante una transizione (apertura, cambio pagina, capsula 200 ms, azione 120 ms, luce 1,2 s **solo al cambio di tono**; con «Mostra animazioni» spento: dissolvenza o niente); **nessun timer nuovo**; le proprietà Slint si scrivono **solo quando il valore cambia** (`aggiorna_modello`, confronto prima di `set_*`).
- **Sicurezza di Claude invariata** (spec §7): fuoco iniziale su «Nega», solo Ctrl+Invio esatto consente, armamento di 600 ms (`ARMAMENTO_CONSENTI`) anche quando il permesso compare in una superficie nuova (Panoramica, Coda), nessun «Sempre», nessuna risposta senza gesto; il comando del permesso mai nella notifica di Windows. Nessun dato nuovo su disco oltre al contatore dei lanci dei collegamenti e alle scelte del guscio (ordine, fissato, Panoramica).
- **Mai toccare l'Home Assistant vero**: nessuna azione verso la casa nelle prove automatiche o nelle foto; «Spegni tutte» e simili solo contro dati finti; dal vivo solo con Paolo e col suo sì (le azioni che suonano o allarmano mai senza ok).
- Test verdi: `cargo test --workspace`; i test che codificano un testo o una misura cambiati si aggiornano; quelli di parti tolte (layout, quadranti, puntini, riga «In attesa», voce espansa) si cancellano **nello stesso task che toglie la parte**, e le regole che proteggevano (Ctrl+Invio, «Nega», fuoco mai perso) rinascono sulla superficie nuova prima di cancellare le vecchie.
- `cargo clippy --workspace --all-targets -- -D warnings` pulito; il codice solo-Windows dietro `#[cfg(windows)]`, nessun codice morto su Linux/macOS (la CI gira sui tre).
- Build e test **solo da PowerShell** con `$env:CARGO_TARGET_DIR='C:\Work\Sviluppo\itsyhome-windows\target\agente'`; mai scrivere sorgenti da PowerShell 5.1 (solo con gli strumenti di modifica dei file). Rust 1.94 in locale e 1.100 in CI: niente API deprecate in 1.100 né assenti in 1.94.
- Prove dal vivo e foto su un'**istanza di prova** (copia dell'exe, `ITSYHOME_ASSISTENTE_DIR` su una cartella finta, nessuna casa configurata); l'isola vera di Paolo (`%LOCALAPPDATA%\isola-m3-prova`) e `%APPDATA%` non si toccano. **Foto solo della finestra dell'app** (PrintWindow del proprio HWND, o `tools/foto-zona.ps1` solo sulla zona della finestra), cancellate dopo il confronto. **Input sintetico solo al proprio PID** (UI Automation o `PostMessage` alla propria HWND; mai SendInput, SetCursorPos, mouse_event né clic reali). Processi fermati solo per PID, dopo aver controllato il percorso.
- Colori solo come token in `ui/tema.slint` (`ui_senza_colori_a_mano` resta), simboli sempre SVG o `Path` (mai glifi Unicode), testo ≥ 12 px (mono maiuscolo ≥ 11 con tracking .07–.08em), contrasto: parola ≥ 4,5:1, segno e bordo di controllo ≥ 3:1.

## Review Focus

1. **Testi lunghi dell'utente** (nome di un collegamento di 40 «W», titolo di sessione lunghissimo, frase d'errore della casa) → nella carta ellissi, mai sotto il bottone d'azione, testo intero per Narrator e nel suggerimento; il `Registro` non scarta mai un riassunto per colpa di un nome (Task 4 `nomi_lunghi_accorciati_e_interi_per_narrator`, Task 5 `piu_usato_con_nome_lunghissimo`, Task 8 `carta_con_testi_lunghi_ha_ellissi_e_azione_libera`).
2. **Un bersaglio che sparisce col fuoco addosso** (permesso risolto nel terminale mentre il fuoco è su «Consenti», carta che perde l'azione col fuoco sull'azione, ultima voce della Coda tolta) → il fuoco va al più vicino e il tasto seguente non si perde (Task 7 `ultima_voce_tolta_col_fuoco_addosso`, Task 8 `voce_che_sparisce_col_fuoco_addosso`, `azione_che_sparisce_col_fuoco_addosso`).
3. **Primo avvio e dati assenti** (casa non configurata, Claude scollegato, primo campione del PC non arrivato, nessun collegamento) → «—» col motivo o «Da collegare», mai «0» né un vuoto, carte della stessa misura (Task 4 `assenza_non_e_zero`, Task 5 `nessun_collegamento`, Task 8 `carte_tutte_della_stessa_misura`).
4. **Configurazione di prima** (`ultimoLayout: "quadranti"`, chiavi `quadranti`/`massimiQuadranti`, collegamenti senza contatori, ordine con l'id `claude`) → si legge senza errori né perdite degli altri campi, Panoramica Griglia, contatori a 0, `agenti` al posto di `claude` (Task 3 `ordine_con_l_id_claude_diventa_agenti`, Task 5 `configurazione_senza_contatori`, Task 8 `configurazione_coi_quadranti_si_legge`).
5. **Audio assente o cambiato da fuori** (nessuna uscita audio, macOS/Linux, muto premuto dalla tastiera del PC) → pulsante del volume assente con la barra alla stessa geometria; nome per Narrator aggiornato («Volume, muto») senza riscritture a vuoto (Task 6 `volume_assente_geometria_ferma`, `volume_muto_da_fuori_cambia_il_nome`).

---

### Task 1: Caratteri e token nuovi in tutta l'app

**Files:**
- Create: `crates/assistente/assets/font/Inter-Medium.ttf`, `Inter-Bold.ttf`, `JetBrainsMonoNL-Medium.ttf`, `OFL-Inter.txt` (copiati dalla cartella `font\` dello spike; release ufficiali Inter 4.1 e JetBrains Mono 2.304); `OFL-JetBrainsMono.txt` sostituito.
- Delete: `assets/font/InterTight-Regular.ttf`, `InterTight-Medium.ttf`, `JetBrainsMono-Regular.ttf`, `OFL-InterTight.txt`.
- Modify: `crates/assistente/ui/tema.slint` (riscritto), ogni `.slint` di `crates/assistente/ui/**` e `crates/gestione/ui/*.slint` che usa un token tolto, `crates/assistente/ui/componenti.slint` (`Pulsante`: primario bianco con testo nero), `README.md`/`NOTICE` se elencano i caratteri, `CHANGELOG.md`.
- Test: `crates/assistente/src/test_ui.rs`.

**Interfaces:**
- Produces (token di `Tema`, unici nomi usati dai task seguenti): grigi `nero #000000`, `g1 #0f0f11` (carta), `g2 #18181b` (riga scelta), `g3 #242428` (secondario), `g4 #303035` (secondario al passaggio), `filo #232327`, `bordo-ctl #74747c`; testi `testo #ffffff` (t1), `testo-2 #c4c4ca` (t2, sostituisce `testo-ora` e `testo-etichetta`), `testo-tenue #9b9ba3` (t3); toni `attivo` = `caldo-2 #fb923c`, `aspetta` = `caldo-1 #fbbf24`, `guarda` = `petalo-3 #60a5fa`, `fatto` = `petalo-5 #2dd4bf`, `errore` = `caldo-3 #f472b6`, `dollaro #7ce4d7`; luci `luce-attivo rgb(251,137,43)`, `luce-aspetta rgb(220,160,4)`, `luce-guarda rgb(17,121,248)`; occhi `occhio-lavora #fcb67d`, `occhio-chiede #fcd56e`, `occhio-guarda #a2cafc`; `primario #ffffff` + `testo-su-primario #000000`; `velo` = nero all'82 %; `anello` bianco + `anello-esterno` nero (anello di Windows 2 + 1). `public pure function colore-tono(t: TonoModuloUi) -> color` e `luce-tono(t: TonoModuloUi) -> color` arrivano al Task 3 (l'enum nasce lì). `carattere: "Inter"`, `carattere-mono: "JetBrains Mono NL"`.
- I nomi di ruolo esistenti che restano (`testo`, `testo-tenue`, `icona`, `card-fondo`, `scheda-sopra`, raggi, corpi, `sul-pixel`, `scala`, `colore-livello`, `colore-collegamento`, `oltre-soglia`) cambiano solo valore; `bloccante`/`attenzione`/`informazione` diventano alias di `aspetta`/`guarda`/`guarda` (stessa tinta della spec §5.2). Spariscono: `fondo-alto`, `fondo-basso`, `fondo-finestra` (→ `nero`), `petali` (→ `filo`), `bagliore-indaco`, `bagliore-acqua`, `ora-grande` (→ `testo`), `card-vetro`, `card-vetro-sopra` (→ `g1`/`g2`), `corpo` (→ `nero`), `riquadro-pieno`/`riquadro-svanito` (→ `g1`).

- [ ] **Step 1: Misura di partenza** (prima di toccare il codice): build release di HEAD, istanza di prova; `tools\misura.ps1 -Secondi 60` a riposo con la pillola e con `--apri` (pannello aperto fermo). Annotare nel rapporto del task RAM privata media e CPU media: sono il «oggi» del Task 10.
- [ ] **Step 2: Test che falliscono** in `test_ui.rs`:
  - `caratteri_incorporati`: `tema.slint` importa esattamente `Inter-Medium.ttf`, `Inter-Bold.ttf`, `JetBrainsMonoNL-Medium.ttf`; nessun `.slint` contiene «Inter Tight» né `font-weight: 400|600`; `assets/font/` contiene i tre `.ttf` e le due licenze OFL e nient'altro.
  - `contrasti_dei_token`: con `contrasto()` già nel file: `testo-2` e `testo-tenue` ≥ 4,5:1 su `nero`, `g1` e `g2`; `attivo`, `aspetta`, `guarda`, `fatto`, `errore` ≥ 4,5:1 su `nero` e `g1`; `bordo-ctl` ≥ 3:1 su `g1`; `testo-su-primario` ≥ 4,5:1 su `primario`.
  - `finestre_col_peso_500`: `Pannello`, `Pillola`, `FinestraImpostazioni` hanno `default-font-weight` 500 (letto dal sorgente) e `Tema.carattere == "Inter"`.
- [ ] **Step 3:** `cargo test -p assistente caratteri_incorporati contrasti_dei_token finestre_col_peso_500` → FAIL.
- [ ] **Step 4: Implementa**: copia i caratteri e le licenze, riscrivi `tema.slint` coi token sopra (commenti col perché di ogni valore come oggi), `default-font-weight: 500` su ogni `Window` (pannello, pillola, Impostazioni, cornice della gestione), pesi 400 → 500 e 600 → 700 nei sorgenti, ogni token tolto sostituito col suo erede (tabella in Interfaces). Il pannello perde l'anello dei petali e i bagliori (il contorno nuovo arriva al Task 6: per ora fondo `nero`, raggio 34 in basso). Nessun ritocco d'impaginazione qui, salvo i testi che col carattere nuovo uscirebbero dal loro posto (si allarga o si mette `overflow: elide`).
- [ ] **Step 5:** `cargo test --workspace` → PASS (i test di misura che codificano larghezze dei caratteri vecchi si aggiornano ai valori nuovi), `cargo clippy --workspace --all-targets -- -D warnings` → pulito.
- [ ] **Step 6:** commit `feat(assistente): Inter e JetBrains Mono NL, token neri del ridisegno in tutta l'app`.

### Task 2: Misura dei testi in pixel

**Files:**
- Create: `crates/assistente/src/moduli/mod.rs` (per ora solo `pub mod misura;`), `crates/assistente/src/moduli/misura.rs`, `crates/assistente/ui/guscio/misura.slint` (componente `MisuraTesto` per il test di confronto)
- Modify: `crates/assistente/Cargo.toml` (`ttf-parser = "=0.25.1"`, commento: già nel `Cargo.lock` via Slint, stessa versione), `src/main.rs` (`mod moduli;`), `ui/interfaccia.slint` (esporta `MisuraTesto`), `CHANGELOG.md`
- Test: in `misura.rs` e `test_ui.rs`

**Interfaces:**
- Produces:
  - `pub enum Stile { Titolo /* Inter 700 18, -0,27 */, TitoloNumerico /* Mono 500 26, -0,52 */, Riga /* Inter 500 12 */, Etichetta /* Mono 500 11, +0,66 */, Capsula /* Inter 700 11 */, OraGrande /* Inter 700 96, -3 */ }`
  - `pub fn larghezza(testo: &str, stile: Stile) -> f32` — somma degli avanzamenti (`hmtx`) scalati + tracking per carattere; i `Face` si leggono una volta (`thread_local!`/`OnceLock`) dagli stessi byte dei `.ttf` (`include_bytes!`).
  - `pub fn accorcia(testo: &str, stile: Stile, massimo: f32) -> String` — intero se ci sta, altrimenti tagliato a carattere con «…» finale entro `massimo`.
  - `pub const SPAZIO_PIENO: f32 = 154.0; pub const SPAZIO_CON_AZIONE: f32 = 116.0;` (carta 182 − 2 × 14; con il bottone 32 + 6, §13.6). `pub const MARGINE: f32 = 3.0;` (differenza ammessa con la resa di Slint, che applica la crenatura).

- [ ] **Step 1: Test che falliscono**:
  - `misura::test::cifre_mono_uguali`: `larghezza("0", TitoloNumerico) == larghezza("8", …)` per tutte le cifre; `larghezza("88:88:88", TitoloNumerico)` ≈ 121 (± 2).
  - `misura::test::valori_dello_spike`: «Tutto regolare.» Titolo ≈ 125, «4 luci accese» ≈ 112, «AGENTI» Etichetta ≈ 44, «Al lavoro» Capsula ≈ 48 (± 3; valori di `11-spike.md` §4).
  - `misura::test::accorcia_con_ellissi`: 40 «W» in `Riga` con massimo 116 → finisce con «…» e `larghezza(...) <= 116`; «Posta» resta «Posta».
  - `test_ui::misura_rust_come_slint`: per 8 testi e 6 stili, `|larghezza() − MisuraTesto.preferred-width| <= MARGINE` (backend di prova).
- [ ] **Step 2:** `cargo test -p assistente misura` → FAIL (modulo assente).
- [ ] **Step 3: Implementa** `larghezza` e `accorcia` in `misura.rs`; `MisuraTesto inherits Window` con `in property testo`, `in property stile: int` e `out property larghezza: length` (stessi caratteri, pesi e tracking della tabella). Se clippy segnala il componente generato come codice morto fuori dai test, la misura passa come proprietà `out` del `Pannello` (dirlo nel commit).
- [ ] **Step 4:** `cargo test -p assistente misura` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): misura dei testi in pixel coi caratteri incorporati`.

### Task 3: Contratto del modulo, Registro e configurazione del guscio

**Files:**
- Modify: `crates/assistente/src/moduli/mod.rs`, `crates/casa-core/src/config.rs`, `crates/casa-core/tests/config.rs`, `crates/assistente/ui/voci.slint`, `ui/interfaccia.slint`, `ui/tema.slint` (`colore-tono`, `luce-tono`), `ui/pannello.slint` (proprietà `moduli`; vista `claude` → `agenti`), `src/pannello.rs` (`voci_app` dal Registro, icona `agenti` = `lucide/terminal.svg`), `src/ponte.rs` + `src/ponte/con_claude.rs` (`DallaPillola::Vista("agenti")`, `vista_claude(vista == "agenti")`), `src/attenzione.rs` (`Fonte::modulo`), `CHANGELOG.md`
- Create: `crates/assistente/src/moduli/registro.rs`, `crates/assistente/src/ponte/con_moduli.rs`
- Delete: `crates/assistente/src/registro.rs` (i suoi 2 test diventano `registro::test::ordine_predefinito`)
- Test: in `moduli/mod.rs`, `moduli/registro.rs`, `casa-core/tests/config.rs`, `test_ui.rs` (i test che dicono «claude» come id di vista passano ad «agenti»)

**Interfaces:**
- Consumes: `misura::{larghezza, Stile, SPAZIO_PIENO, SPAZIO_CON_AZIONE, MARGINE}` (Task 2).
- Produces (Rust, `crate::moduli`):
  - `pub enum Tono { Vuoto, Calmo, Attivo, Guarda, Aspetta, Fatto, Errore, Fermo }` con `fn priorita(self) -> u8` (Aspetta 5 > Guarda = Errore 4 > Fatto 3 > Attivo 2 > Calmo = Fermo = Vuoto 1), `fn parola(self) -> &'static str` («Aspetta», «Guarda», «Al lavoro», «Fatto», «Errore», «Fermo»; "" per Calmo e Vuoto), `fn per_narrator(self) -> &'static str` («ti aspetta», «guarda presto», «al lavoro», «fatto», «errore», «non risponde»; ""), `fn luce(self) -> bool` (Attivo, Guarda, Aspetta), `fn da_livello(l: Livello) -> Tono` (Bloccante → Aspetta, Attenzione → Guarda, Informazione → Calmo).
  - `pub struct Identita { pub id: &'static str, pub nome: &'static str, pub nome_breve: &'static str, pub icona: &'static str }`; `pub const MODULI: [Identita; 4]` = agenti «Agenti»/«Agenti»/`agenti`, casa «Casa», pc «PC», collegamenti «Collegamenti»/«Link».
  - `pub struct Azione { pub id: &'static str, pub testo: String, pub icona: &'static str }`
  - `pub struct Riassunto { pub tono: Tono, pub titolo: String, pub titolo_numerico: bool, pub sec: String, pub sec_intera: String, pub azione: Option<Azione> }` + `Riassunto::primo_avvio(motivo: &str) -> Riassunto` (titolo «—», sec = motivo, tono Vuoto, nessuna azione).
  - `pub struct FuoriMisura { pub modulo: &'static str, pub campo: &'static str, pub larghezza: f32, pub massimo: f32 }`; `pub fn controlla(id: &'static str, r: &Riassunto) -> Result<(), FuoriMisura>` — `titolo` (Titolo o TitoloNumerico) e `sec` (Riga) ≤ `SPAZIO_CON_AZIONE` se c'è l'azione, altrimenti ≤ `SPAZIO_PIENO`, meno `MARGINE`.
- Produces (`crate::moduli::registro`):
  - `pub struct Registro` (ordine, fissati, ultimo `Riassunto` e livello delle voci per id).
  - `Registro::nuovo(config: &[ConfigModulo]) -> Registro` — ordine dell'utente; id sconosciuti scartati; mancanti aggiunti in fondo nell'ordine di `MODULI`; `"claude"` letto come `"agenti"` (una tantum: il prossimo salvataggio scrive `agenti`).
  - `fn aggiorna(&mut self, id: &'static str, r: Riassunto) -> Result<bool, FuoriMisura>` — `Err` tiene il riassunto di prima; `Ok(true)` solo se è cambiato.
  - `fn livelli_voci(&mut self, per_modulo: &[(&'static str, Livello)]) -> bool` (vero se un tono effettivo cambia).
  - `fn tono_effettivo(&self, id: &str) -> Tono`, `fn tono_isola(&self) -> Tono`, `fn ordine(&self) -> Vec<&'static Identita>`, `fn config(&self) -> Vec<ConfigModulo>`.
  - `fn nome_narrator(&self, posizione: usize) -> String` — «{nome}, {titolo}, {sec_intera}[, {per_narrator}], modulo {i} di {n}[, azione: {azione}]», p. es. «Casa, 4 luci accese, 21,5° dentro, modulo 2 di 4, azione: Spegni tutte».
  - `fn moduli_ui(&self) -> Vec<ModuloUi>` (icone da `pannello::icona`).
  - `fn nome(&self, id: &str) -> &'static str` — il nome del modulo per le voci: `voce_attesa_ui` scrive `sotto` come «{livello} · {nome del modulo di `Fonte::modulo()`}» («Aspetta te · Agenti», non più «Claude Code»; «Prova» per le voci di prova).
- Produces (casa-core): `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)] pub struct ConfigModulo { pub id: String, #[serde(default = "vero")] pub fissato: bool }`; `Config.moduli: Vec<ConfigModulo>` (`#[serde(default)]`; vuoto = ordine del registro, tutti fissati).
- Produces (Slint, `voci.slint`): `export enum TonoModuloUi { vuoto, calmo, attivo, guarda, aspetta, fatto, errore, fermo }`; `export struct ModuloUi { id: string, nome: string, nome-breve: string, icona: image, tono: TonoModuloUi, parola: string, titolo: string, numerico: bool, sec: string, con-azione: bool, azione-testo: string, azione-icona: image, narratore: string, narratore-azione: string }` (`narratore-azione` = «{azione}, {nome}»). `Pannello`: `in property <[ModuloUi]> moduli; in property <TonoModuloUi> tono-isola;`. `Tema.colore-tono(t)` / `Tema.luce-tono(t)` (trasparente per i toni senza luce).
- Produces (`con_moduli.rs`): campi di `Ponte` `registro: RefCell<Registro>`, `modello_moduli: Rc<VecModel<ModuloUi>>`; `pub(super) fn moduli_scrivi(&self)` (riscrive con `aggiorna_modello` + `tono-isola` solo se diverso), `pub fn moduli_nel_pannello(&self, p: &Pannello)`. `Fonte::modulo(self) -> &'static str` («agenti», «casa», «pc», «prova»).

- [ ] **Step 1: Test che falliscono**:
  - `moduli::test::priorita_della_spec`: ordine Aspetta > Guarda = Errore > Fatto > Attivo > Calmo; `parola()` e `per_narrator()` come sopra; `luce()` vera solo per Attivo, Guarda, Aspetta.
  - `moduli::test::capsula_e_nome_breve_stanno_nella_carta`: per ogni `Identita` e ogni tono con parola: `larghezza(nome_breve.to_uppercase(), Etichetta) + 22 + (larghezza(parola, Capsula) + 30) + 6 <= 154` (§13.5); la parola più larga (`Al lavoro`) è il tetto della capsula.
  - `registro::test::ordine_predefinito`: senza configurazione → `agenti, casa, pc, collegamenti`, tutti fissati.
  - `registro::test::ordine_con_l_id_claude_diventa_agenti`: `[collegamenti, claude, casa]` → `collegamenti, agenti, casa, pc`; `config()` scrive `agenti`.
  - `registro::test::rifiuta_oltre_i_limiti`: titolo «WWWWWWWWWWWWWWW» → `Err(FuoriMisura { campo: "titolo", .. })` e il riassunto di prima resta; con azione un titolo da 120 px è rifiutato, senza azione accettato.
  - `registro::test::scrive_solo_se_cambia`: stesso riassunto due volte → `Ok(true)` poi `Ok(false)`.
  - `registro::test::voci_alzano_il_tono`: riassunto Calmo + voce bloccante del modulo → `tono_effettivo` Aspetta; `tono_isola` = Aspetta; tolta la voce → Calmo.
  - `registro::test::nome_per_narrator_composto`: il caso Casa sopra, e Agenti con tono Aspetta → «Agenti, …, ti aspetta, modulo 1 di 4».
  - `casa-core/tests/config.rs::moduli_salvati_e_file_di_prima`: file senza `moduli` → vuoto; salva/rileggi uguale.
  - `test_ui`: `dock_e_puntini_sono_schede_con_lo_stato` aggiornato con l'id `agenti` e il nome «Agenti».
- [ ] **Step 2:** `cargo test --workspace moduli registro config` → FAIL.
- [ ] **Step 3: Implementa** contratto, `Registro`, `ConfigModulo`, `con_moduli.rs`; il dock legge le voci dal `Registro` (`voci_app` diventa «Panoramica» + `registro.ordine()`); rinomina la vista `claude` in `agenti` in `pannello.slint`, `pannello.rs`, `ponte.rs`, `con_claude.rs` e nei test. Nessuna parte visibile cambia oltre al nome «Agenti» e alla sua icona nel dock.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): contratto del modulo, Registro e ordine dei moduli in configurazione`.

### Task 4: Riassunti di Agenti, Casa e PC

**Files:**
- Create: `crates/assistente/src/moduli/agenti.rs`, `moduli/casa.rs`, `moduli/pc.rs`
- Modify: `src/ponte/con_moduli.rs` (ricalcolo e azioni), `src/ponte.rs` (chiamate nei punti «quando cambia»: `ricevi`, `aggiorna_casa_offline`, `attese_cambiate`, `pannello_aperto`/`pannello_chiuso`), `src/ponte/con_claude.rs` (dopo un cambio di sessioni o limiti), `src/pannello.rs` (alla consegna di `StatPc`), `CHANGELOG.md`
- Test: nei tre file nuovi e in `con_moduli.rs`

**Interfaces:**
- Consumes: `Riassunto`, `Azione`, `Tono`, `Registro::aggiorna`, `misura::accorcia` (Task 2–3); `claude::{strumento_italiano, testo_pulito}`, `ponte::{riga_stato, frase_errore, riprovabile, luci_accese}`, `orologio::temperatura_breve`.
- Produces:
  - `agenti::riassunto(collegato: bool, s: &Sessioni, limiti: (Option<&Limite>, Option<&Limite>), ora: Instant) -> Riassunto`
  - `casa::riassunto(stato: &StatoConnessione, menu: Option<&MenuData>, dati_delle: Option<&str>) -> Riassunto`
  - `pc::riassunto(stat: Option<&StatPc>) -> Riassunto` (`None` = campione non ancora pronto)
  - `Ponte::moduli_aggiorna(&self, id: &'static str)` (ricalcola quel riassunto, lo passa al `Registro` e chiama `moduli_scrivi` se è cambiato; a pannello chiuso segna solo `modelli_vecchi`).
  - `Ponte::azione_modulo(&self, modulo: &str, azione: &str)` e `Pannello` `callback azione-modulo(string, string)`: `agenti.terminale` → `claude_apri_terminale(impronta)`, `agenti.collega` → Impostazioni sezione Claude, `casa.spegni` → `spegni_tutte`, `casa.riprova` → `riprova_casa`, `casa.collega` → Impostazioni sulle case (costante `SEZIONE_CASA` accanto a `SEZIONE_CLAUDE` in `finestra_impostazioni.rs`). Id delle azioni come costanti in ciascun modulo.

Scale per stato (testi decisi qui; ognuno entro i limiti in pixel, verificato dal test):

| Modulo | Stato | Tono | Titolo | Riga | Azione |
|---|---|---|---|---|---|
| Agenti | non collegato | Vuoto | «Da collegare» | «Collega Claude Code» | «Collega» |
| | sessioni che lavorano | Attivo | «{n} al lavoro» | limite più alto col nome («Claude 7 giorni 61 %»), senza limiti «{nome} · {strumento}» | «Apri il terminale» (la più recente che lavora) |
| | una sessione in errore | Errore | «Errore» | «{nome}: {frase}» | «Apri il terminale» |
| | fatto non visto | Fatto | «Ha finito» | «{nome}» | «Apri il terminale» |
| | sessioni ferme | Calmo | «{n} sessioni» / «1 sessione» | limite più alto o «Nessun limite letto» | — |
| | nessuna sessione | Calmo | «Nessuna sessione» | limite più alto o «Pronto» | — |
| Casa | non configurata | Vuoto | «Da collegare» | «Collega la casa» | «Collega» |
| | in connessione | Calmo | «Mi collego» | «—» | — |
| | non connessa / errore / token | Fermo | «Non connessa» | frase di `frase_errore` o «Dati delle {hh:mm}» | «Riprova» se `riprovabile`, altrimenti «Collega» |
| | connessa, luci accese | Calmo | «{n} luci accese» / «1 luce accesa» | «{t} dentro, {m} fuori» (quelle che ci sono) | «Spegni tutte» |
| | connessa, luci spente | Calmo | «Luci spente» | come sopra, o «Nessun sensore» | — |
| PC | primo campione non arrivato | Vuoto | «—» | «Lettura in corso» | — |
| | CPU o RAM ≥ 85 % | Guarda | «CPU {x} %» o «RAM {x} %» | l'altra misura | — |
| | batteria ≤ soglia informazione e non in carica | Guarda | «Batteria {x} %» | «CPU {x} %, RAM {y} %» | — |
| | altrimenti | Calmo | «Tutto regolare» | «CPU {x} %, RAM {y} %» | — |

I nomi dell'utente (titolo della sessione o cartella, frase d'errore) entrano nella `sec` accorciati con `misura::accorcia` allo spazio che resta; la `sec_intera` li tiene interi.

- [ ] **Step 1: Test che falliscono** (uno per riga della tabella, più):
  - `agenti::test::scala_per_stato`, `casa::test::scala_per_stato`, `pc::test::scala_per_stato`: tono, titolo, riga e azione di ogni riga della tabella.
  - `{agenti,casa,pc}::test::ogni_gradino_sta_nei_limiti`: per ogni gradino con dati peggiori (numeri a tre cifre, sessione di 60 «W», frase d'errore più lunga di `frase_errore`) `moduli::controlla(id, &r) == Ok(())`.
  - `agenti::test::nomi_lunghi_accorciati_e_interi_per_narrator`: `sec` finisce con «…», `sec_intera` contiene il nome intero.
  - `{casa,pc}::test::assenza_non_e_zero`: senza dati nessun «0» nel titolo né nella riga; PC senza campione → «—».
  - `con_moduli::test::azioni_portano_al_loro_comando`: con `Ponte` di prova (dati finti, nessuna casa vera) `azione_modulo("casa", "casa.spegni")` manda un comando per ogni luce accesa al servizio finto; `agenti.terminale` chiama la stessa funzione del clic su una riga.
  - `con_moduli::test::da_chiuso_non_si_ricalcola`: a pannello chiuso un evento della casa non scrive `modello_moduli`; all'apertura sì, una volta.
- [ ] **Step 2:** `cargo test -p assistente agenti casa pc con_moduli` → FAIL.
- [ ] **Step 3: Implementa** le tre funzioni pure e la colla: `Ponte` chiama `moduli_aggiorna(id)` nei punti elencati (solo se `modelli_vivi`, altrimenti segna `modelli_vecchi`), il `Registro` valida e scrive; un `Err(FuoriMisura)` resta su `eprintln!` (niente in `diagnosi.log`). Il PC riceve il riassunto dal thread dei campioni nella stessa consegna di `StatPc`.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): riassunti di Agenti, Casa e PC con le scale per stato`.

### Task 5: Collegamenti — il più usato col conteggio salvato

**Files:**
- Create: `crates/assistente/src/moduli/collegamenti.rs`
- Modify: `crates/casa-core/src/config.rs` (`Collegamento`), `crates/casa-core/tests/config.rs`, `crates/assistente/src/collegamenti.rs` (`salva`, `ripara`, letterali di `Collegamento`), `src/ponte.rs` (`apri_collegamento`), `src/ponte/con_moduli.rs`, `CHANGELOG.md`
- Test: `moduli/collegamenti.rs`, `collegamenti.rs`, `casa-core/tests/config.rs`, `ponte.rs`

**Interfaces:**
- Consumes: `Riassunto`, `Registro::aggiorna`, `misura::accorcia`, `Ponte::azione_modulo` (Task 3–4).
- Produces:
  - `Collegamento { …, #[serde(default)] pub lanci: u32, #[serde(default)] pub ultimo_lancio: i64 /* secondi Unix */ }`
  - `collegamenti::conta_lancio(config: &mut Config, id: &str, ora: i64) -> bool` (saturante; falso se l'id non c'è)
  - `moduli::collegamenti::piu_usato(c: &[Collegamento]) -> Option<&Collegamento>` — più lanci; a parità il più recente; tutti a zero → il primo nell'ordine dell'utente.
  - `moduli::collegamenti::riassunto(c: &[Collegamento]) -> Riassunto` — nessuno: Vuoto «Nessun collegamento» / «Aggiungine uno» / azione «Aggiungi» (→ Impostazioni › Collegamenti); altrimenti Calmo «{n} collegamenti» / «1 collegamento», riga «Più usato: {nome}», azione «Apri {nome}» con id `collegamenti.apri` (il nome accorciato nel testo del bottone, intero per Narrator).

- [ ] **Step 1: Test che falliscono**:
  - `config.rs::configurazione_senza_contatori`: un `assistente.json` di oggi (collegamenti con solo `id`, `nome`, `url`) si legge con `lanci == 0` e `ultimo_lancio == 0`; salva e rileggi uguale; il file passa per `.tmp` + rename come il resto.
  - `collegamenti::test::lancio_contato`: `conta_lancio` porta 0 → 1 e scrive l'ora; id assente → falso, nulla cambia; `u32::MAX` non trabocca.
  - `collegamenti::test::modifica_e_riparazione_tengono_il_conteggio`: `salva` su un id esistente e `ripara` non azzerano `lanci`; un collegamento tolto se ne va col suo conteggio.
  - `moduli::collegamenti::test::piu_usato_e_parita_al_piu_recente`: (3, t=10) contro (3, t=20) → il secondo; (5, t=1) batte (3, t=99); tutti a zero → il primo.
  - `moduli::collegamenti::test::piu_usato_con_nome_lunghissimo` e `nessun_collegamento`: `controlla(...) == Ok(())`, `sec_intera` col nome intero; elenco vuoto → tono Vuoto, nessun «0».
  - `ponte::test::aprire_conta_e_salva`: con `su_apri_url` finto che riesce, `apri_collegamento("nas")` porta `lanci` a 1 nella configurazione salvata; se il browser fallisce il conteggio non sale.
- [ ] **Step 2:** `cargo test --workspace collegament` → FAIL.
- [ ] **Step 3: Implementa** i campi, `conta_lancio` dentro `casa.aggiorna_config` subito dopo `Ok(())` in `apri_collegamento`, il riassunto e l'azione `collegamenti.apri` (stessa strada di `apri_collegamento`: il pannello si chiude se il browser è partito).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente, casa-core): collegamento più usato col conteggio dei lanci salvato`.

### Task 6: Cornice nera 800 × 340 — isola, riga alta, barra con capsula e volume

**Files:**
- Create: `crates/assistente/ui/guscio/luce.slint` (`Luce`), `ui/guscio/cornice.slint` (`ContornoIsola`, `RigaAlta`, `Barra`, `SchedaBarra`, `PulsanteVolume`, `PulsanteRicerca`), `crates/assistente/src/moduli/panoramica.rs`
- Modify: `ui/pannello.slint` (finestra 820 × 340, corpo 752 × 224 a (34, 44), dock e riga alta sostituiti; il contenuto di oggi — riga «In attesa», layout, puntini, pagine — resta dentro il corpo), `ui/voci.slint` (`SchedaBarraUi`), `src/finestre.rs` (`LARGHEZZA_PANNELLO`, `LARGHEZZA_FINESTRA_PANNELLO = 820.0`, `ALTEZZA_PANNELLO = 340.0`, `ALTEZZA_RIGA_PANNELLO = 44.0`, `SCARTO_OCCHI_PANNELLO_Y`), `src/pannello.rs` (posizione, partenza degli occhi, schede alla `vista-cambiata`), `src/ponte/con_moduli.rs`, `src/ponte.rs` (volume nella barra), `CHANGELOG.md`
- Test: `moduli/panoramica.rs`, `finestre.rs`, `test_ui.rs`

**Interfaces:**
- Consumes: `Registro::ordine`, `tono_effettivo`, `tono_isola`, `Tema.luce-tono` (Task 3); callback esistenti `imposta-volume(float)`, `muto-volume(bool)`, proprietà `volume-disponibile`, `volume`, `volume-muto`.
- Produces:
  - `moduli::panoramica::geometria_schede(nomi: &[&str], attiva: usize) -> Vec<(f32, f32)>` — inattiva 36; attiva 12 + 18 + 8 + `larghezza(nome, Etichetta)` + 14; dopo la Panoramica divisore 1 + 12; passo 2 (schema `spike/src/lib.rs`, ma con la misura del Task 2 invece di 7,48 px fissi).
  - `export struct SchedaBarraUi { id: string, nome: string, icona: image, x: length, larghezza: length, segno: int /* 0 nessuno, 1 ti aspetta, 2 guarda presto */, narratore: string /* «Agenti, ti aspetta, scheda 2 di 5» */ }`; `Pannello`: `in property <[SchedaBarraUi]> schede;`. `Ponte::scrivi_schede(&self, p: &Pannello) -> u64` la riscrive (con `aggiorna_modello`, restituisce le righe riscritte) alla `vista-cambiata` e dopo `moduli_scrivi`; segno e stato dal tono effettivo (Aspetta → 1, Guarda → 2).
  - L'isola prende la luce di `tono-isola` (Task 3) sotto la barra, coi valori di `spike.slint` (`rx-pct .66`, `ry-pct .82`, `cy-pct 1.24`, picco ≤ ~30 % sotto le scritte della barra).
  - `Luce { in property <TonoModuloUi> tono; in property <float> rx-pct, ry-pct, cy-pct, k1, k2, p2, p3; }` — tecnica t1 dello spike (cerchio con `transform-scale-x` dentro un genitore con `clip`), colore e forza animati 1,2 s solo al cambio di tono (0 ms con `Movimento.ridotto`), alfa nel colore.
  - `ContornoIsola`: **un solo `Path`** 820 × 340 con orecchie di 10 e raggi bassi 34 (`spike.slint`, `isola-tracciato`); la luce in un `Rectangle` interno trasparente con `clip` e raggi bassi 34.
  - `PulsanteVolume`: clic = `muto-volume(!volume-muto)`; rotella = `imposta-volume(volume ± 0.02)` per scatto (bloccato a 0..1); `accessible-label` «Volume {n} %» o «Volume, muto»; icona `altoparlante`/`altoparlante-muto`; con `!volume-disponibile` non c'è (`visible: false`, posto vuoto).
  - Riga alta 44: a sinistra data mono maiuscola 12 + meteo (temperatura e condizione a parole, mono 12, `testo-2`), occhi al centro, a destra ora mono 13 (`testo`) e «⋯» 32. Barra 64: ricerca a sinistra (`PulsanteRicerca` inerte, «Ctrl K», lente come `Path`), schede al centro (`SchedaBarra` con capsula 36 r 18 e `animate x, width` 200 ms), volume a destra.

- [ ] **Step 1: Test che falliscono**:
  - `panoramica::test::geometria_delle_schede`: 5 nomi, attiva 0 → la prima larga più di 36, le altre 36; x crescenti senza sovrapposizioni; attiva 2 → la terza larga, la prima 36.
  - `finestre::test::pannello_820_per_340_e_occhi_alla_partenza`: posizione centrata per 820 a scala 1, 1,25, 1,75; gli occhi del pannello partono dove stavano quelli della pillola (`SCARTO_OCCHI_PANNELLO_Y` = 16 − 22).
  - `test_ui::misure_slint_e_rust` aggiornato: larghezza 820, altezza 340, riga 44.
  - `test_ui::barra_con_schede_ricerca_e_volume`: schede con ruolo `Tab` e i nomi del modello; «Cerca, in arrivo» pulsante; «Volume 40 %» pulsante; l'ordine Tab di oggi dal dock arriva a ricerca, volume, «⋯».
  - `test_ui::volume_clic_muto_e_rotella`: clic → `muto-volume(true)`; rotella in su su 0,40 → `imposta-volume(0.42)`; a 1,0 resta 1,0.
  - `test_ui::volume_assente_geometria_ferma`: con `volume-disponibile` falso nessun elemento «Volume…» e le schede hanno la stessa x di prima.
  - `test_ui::volume_muto_da_fuori_cambia_il_nome`: `set_volume_muto(true)` → il nome diventa «Volume, muto».
  - `test_ui::scheda_attiva_ha_il_nome_e_lo_stato`: la scheda Agenti con segno 1 si chiama «Agenti, ti aspetta, scheda 2 di 5» ed è `selected` quando `vista == "agenti"`.
  - `con_moduli::test::schede_riscritte_solo_se_cambiano`: due chiamate di fila a `scrivi_schede` senza cambi → 0 righe riscritte.
- [ ] **Step 2:** `cargo test --workspace panoramica finestre misure_slint barra volume scheda_attiva schede_riscritte` → FAIL.
- [ ] **Step 3: Implementa** la cornice: il vecchio anello e il dock spariscono; le pagine di oggi stanno nel corpo 752 × 224 (le loro coordinate fisse partono da `centro-x`/`centro-y` nuovi; il ritocco fine è del Task 9). I test di geometria vecchi (dock a 38 px, pannello 300) si aggiornano o si cancellano qui.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto dell'istanza di prova a 100 % (Panoramica e una pagina) confrontate con `tc-griglia-4.png` per riga alta e barra; foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): cornice nera 800×340 con orecchie, riga alta, barra a capsula e volume`.

### Task 7: Coda minima sull'impianto Elenco

**Files:**
- Create: `crates/assistente/ui/impianti/elenco.slint` (`ImpiantoElenco`, `RigaElenco`, `TestataPagina`)
- Modify: `ui/pannello.slint` (vista `coda`; la riga «In attesa» di oggi: clic e «+N» aprono la Coda sulla voce; via `VoceEspansa` e la sua tastiera), `ui/attesa.slint` (resta solo `RigaAttesa`), `src/ponte.rs` (`DallaPillola`, `apertura_dalla_pillola`), `src/ponte/con_claude.rs` (armamento all'apertura della Coda), `src/pannello.rs`, `CHANGELOG.md`
- Test: `test_ui.rs`, `ponte.rs`

**Interfaces:**
- Consumes: il modello `attese: [VoceAttesaUi]` (già ordinato: bloccanti, attenzione, informazione; a pari livello la più recente), `Pannello.azione(string, string)`, `claude_superficie_compare`, `Tema.colore-livello`.
- Produces:
  - `ImpiantoElenco { in property <[VoceAttesaUi]> voci; in-out property <int> fuoco-riga; in-out property <int> fuoco-pulsante /* -1 = la riga */; callback azione(string, string); }` — testata «{n} in attesa» (Inter 700 18) + «da {m} moduli» (`testo-tenue`); righe 40: segno del livello (forma + colore: occhietti tondi per bloccante, a mandorla per attenzione, trattino per informazione) + icona del modulo + titolo (Inter 700 14, elide) + «{livello} · {modulo}» + fino a 2 pulsanti (32 r 9; l'ultimo non delicato è primario bianco, «Consenti» compreso); scorre (righe fuori vista portate in vista dal fuoco). Vuota: «Niente in attesa» + «Torna alla Panoramica».
  - Tastiera della Coda: ↑/↓ fra le righe, → ai pulsanti (si entra su «Nega» se c'è, altrimenti sul primo), ← alla riga, Invio sul pulsante (mai su uno delicato), **solo Ctrl+Invio esatto** su «Consenti», Backspace/Alt+← alla Panoramica, Esc chiude.
  - `DallaPillola { Vista(&'static str), Panoramica }` (via `Espandi`): permesso allargato → `Panoramica` (lo mostra la voce del Task 8; fino ad allora la riga «In attesa»), voce allargata di Claude che non è un permesso → `Vista("agenti")`, voce allargata di Casa → `Vista("casa")`, di PC → `Vista("pc")`, pillola col **puntino** → `Vista("coda")`, attività di Claude → `Vista("agenti")`, riposo → `Panoramica`.
  - `Ponte::vista_coda(&self, si: bool)`: all'apertura della Coda con un permesso in attesa riarma «Consenti» (`claude_superficie_compare(Instant::now())`).

- [ ] **Step 1: Test che falliscono** (le regole di sicurezza **prima** di togliere `VoceEspansa`):
  - `coda_consenti_solo_con_ctrl_invio_esatto`: gli stessi 9 casi di `consenti_solo_con_ctrl_invio_esatto` sulla riga della Coda; solo Ctrl+Invio dà `["consenti"]`.
  - `coda_entra_su_nega`: → su una riga di permesso porta il fuoco vero su «Nega».
  - `coda_consenti_e_primario_nega_secondario`: tipi dei due pulsanti (sostituisce `nega_e_consenti_sono_neutri`).
  - `ultima_voce_tolta_col_fuoco_addosso`: fuoco su «Consenti» dell'ultima riga, modello che perde quella riga → fuoco sulla riga precedente dopo `update_timers_and_animations()`; il tasto ↑ seguente funziona.
  - `coda_vuota_ha_lo_stato_vuoto`: «Niente in attesa» e un pulsante «Torna alla Panoramica».
  - `coda_righe_accessibili`: ogni riga ha ruolo e nome «{titolo}, {livello} · {modulo}»; testata letta come testo.
  - `ponte::test::dalla_pillola_alla_destinazione`: i sei casi di `DallaPillola` sopra (sostituisce i test di `Espandi`).
  - `ponte::test::coda_riarma_consenti`: con un permesso in attesa, `vista_coda(true)` sposta l'armamento a `ora + ARMAMENTO_CONSENTI`.
- [ ] **Step 2:** `cargo test -p assistente coda dalla_pillola` → FAIL.
- [ ] **Step 3: Implementa** l'impianto e la vista `coda` (fuoco «per indice» come `claude-fuoco`, regola `changed` sul numero di righe); la riga «In attesa» porta alla Coda; togli `VoceEspansa`, `espandi`, `indice-aperta`, `voce-aperta` e i loro test (`voce_chiusa_ed_espansa`, `titolo_della_voce_espansa_va_a_capo`, `frecce_e_riduci_sono_pulsanti_veri`, `pannello_sul_consenso` e i test che lo usano, ora rifatti sulla Coda).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): Coda minima sull'impianto Elenco, la pillola apre la destinazione`.

### Task 8: Panoramica Griglia, voce che ti aspetta e pool di fuoco

**Files:**
- Create: `crates/assistente/ui/guscio/carta.slint` (`CartaVista`, `Posto`, `Occhietti`, `Capsula`), `ui/guscio/voce.slint` (`VoceInAttesa`), `ui/guscio/panoramica.slint` (`Griglia`, `OrologioGrande`)
- Modify: `ui/pannello.slint` (Panoramica = Griglia; via `RigaAttesa`, `LayoutOrologio`, `LayoutQuadranti`, puntini, `fuoco-luci-el`, zone `attesa`), `ui/impostazioni.slint` (via la sezione «QUADRANTI» e il campo «Massimo»), `src/moduli/panoramica.rs`, `src/ponte.rs` (via `quadranti_ui`, `sensori_quadranti`, `riga_sinistra(Layout, …)` → `riga_sinistra(data, meteo)`, `fonti_dei_quadranti`, modelli e callback dei quadranti), `src/pannello.rs` (via layout e `fonti_della_vista` coi quadranti: dischi, reti e GPU solo nella vista PC), `src/lati.rs` (via `quadranti`, `FonteQuadrante…`; il catalogo resta per i lati della pillola), `crates/casa-core/src/config.rs` (via `ultimo_layout`, `quadranti`, `massimi_quadranti`; `sensori_scelti` solo dai lati), `CHANGELOG.md`
- Delete: `src/layout.rs`, `src/scelte.rs`, `ui/layout/orologio.slint`, `ui/layout/quadranti.slint`, `ui/attesa.slint`
- Test: `moduli/panoramica.rs`, `test_ui.rs`, `casa-core/tests/config.rs`

**Interfaces:**
- Consumes: `ModuloUi`, `moduli`, `tono-isola`, `SchedaBarraUi`, `Luce`, `ContornoIsola` (Task 3, 6), `attese` e la vista `coda` (Task 7), `azione-modulo` (Task 4).
- Produces:
  - Costanti dei bersagli in `panoramica.rs` (le stesse in Slint, controllate da un test): `ID_NEGA = 100`, `ID_CONSENTI = 101`, `ID_ALTRE = 102` («+N in attesa ›»), carta k = `2k`, sua azione = `2k + 1`, `ID_SCHEDA = 200` (+ k), `ID_RICERCA = 300`, `ID_VOLUME = 310`, `ID_MENU = 320`, `ID_IN_ATTESA = 330` («N in attesa ›» della riga alta).
  - `pub struct Composizione { pub voce: bool, pub altre: bool, pub in_attesa: bool, pub azioni: Vec<bool>, pub schede: usize, pub volume: bool }`
  - `pub fn ordine_tab(c: &Composizione) -> Vec<i32>` — voce (Nega → Consenti → «+N») → carte (corpo → azione) → schede → ricerca → volume → «⋯» → «N in attesa ›»; con ≤ 4 moduli nessuna carta è coperta.
  - `pub fn prossimo(ordine: &[i32], attuale: i32, avanti: bool) -> i32` (in giro; fuori elenco: il primo o l'ultimo).
  - `pub fn in_attesa_non_mostrate(voci: usize, voce_mostrata: bool) -> usize`.
  - `Pannello`: `in-out property <int> fuoco: -1; callback prossimo-fuoco(int, bool) -> int;` (Rust calcola da `Composizione`), `in property <int> in-attesa;`.
  - `Posto` (4 posti fissi `p0..p3`, mai dentro `if`/`for`): `Fuoco` del corpo con `accessible-label: modulo.narratore`, bottone d'azione bianco 32 con `visible: modulo.con-azione` (nessun nodo per Narrator senza azione, §13.9) e `opacity` animata 120 ms se `hover || fuoco su corpo o azione`; i `Text` della `CartaVista` con `accessible-role: none`.
  - `CartaVista` 182 × 108 r 20 (spazio 8, 4 per riga, gruppo centrato): etichetta (icona 15 + nome mono 11 maiuscolo; `nome-breve` se c'è la capsula), capsula (occhietti + parola, `Tema.colore-tono`), valore (Inter 700 18 o Mono 500 26 se `numerico`), riga 12; **valore e riga perdono 38 px quando l'azione è visibile** (§13.6); luce dal basso per i toni con luce.
  - `OrologioGrande`: 5 `Text` in celle fisse larghe come la cifra più larga («0», misurata con un `Text` nascosto), Inter 700 96, centrato (§13.8).
  - `VoceInAttesa` (prima voce bloccante di `attese`): 752 × 108 r 20 al posto dell'orologio; capsula «Ti aspetta · {modulo}», titolo Inter 700 18 (elide), prima riga del dettaglio (mono 13 per i permessi), «Nega» (fuoco iniziale) e «Consenti» bianco, «+{n} in attesa ›» → Coda. Regole M3: Invio mai su «Consenti», solo Ctrl+Invio esatto, armamento (la voce che compare chiama `claude_superficie_compare`).
  - Riga alta in Griglia: dopo data e meteo «{n} in attesa ›» se `in_attesa_non_mostrate > 0` (apre la Coda); ora piccola nascosta quando l'orologio grande si vede.

- [ ] **Step 1: Test che falliscono**:
  - `panoramica::test::ordine_tab_della_spec`: senza voce, 4 carte con azione [sì, sì, no, sì], 5 schede, volume → `[0,1,2,3,4,6,7,200..204,300,310,320]`; con voce e «+N» → inizia `[100,101,102,0,…]`; senza volume niente 310; con «N in attesa» 330 in fondo.
  - `panoramica::test::prossimo_in_giro` e `in_attesa_non_mostrate`.
  - `test_ui::panoramica_griglia_con_quattro_moduli`: orologio grande visibile; nessun elemento della griglia sotto y = 276 (fascia della barra).
  - `test_ui::carte_tutte_della_stessa_misura`: con i 4 riassunti di primo avvio, con dati finti calmi e con titoli al limite, le 4 carte sono alla stessa y, larghe 182, alte 108, distanti 190.
  - `test_ui::azione_al_passaggio_o_al_fuoco`: a riposo nessun bottone d'azione visibile; Tab dalla carta 0 porta all'azione 0, poi alla carta 1; la carta PC (senza azione) non ha nodo «…, PC»; Maiusc+Tab torna indietro.
  - `test_ui::carta_con_testi_lunghi_ha_ellissi_e_azione_libera`: con azione visibile il valore e la riga finiscono prima di x = 182 − 14 − 32 − 6.
  - `test_ui::voce_al_posto_dell_orologio`: con una voce bloccante l'orologio non c'è, la voce sì, l'ora piccola torna nella riga alta, il fuoco iniziale è su «Nega»; le carte non si spostano.
  - `test_ui::voce_consenti_solo_con_ctrl_invio_esatto` (i 9 casi) e `voce_invio_su_consenti_non_concede`.
  - `test_ui::voce_che_sparisce_col_fuoco_addosso`: fuoco su «Consenti», voce tolta da Rust → fuoco sulla carta 0 dopo `update_timers_and_animations()`, il Tab seguente va alla sua azione.
  - `test_ui::azione_che_sparisce_col_fuoco_addosso`: fuoco sull'azione della carta Casa, `con-azione` falso → fuoco sul corpo della stessa carta.
  - `test_ui::nomi_per_narrator_dal_modello`: il nome dell'elemento col fuoco è `moduli[k].narratore`; cambiato il modello sotto il fuoco, il nome lo segue.
  - `test_ui::in_attesa_nella_riga_alta`: 3 voci di attenzione → «3 in attesa ›» nella riga alta, clic → vista `coda`.
  - `test_ui::orologio_a_celle_fisse`: con «11:11» e «00:00» la x di ogni cella è la stessa.
  - `casa-core/tests/config.rs::configurazione_coi_quadranti_si_legge`: un file con `ultimoLayout: "quadranti"`, `quadranti`, `massimiQuadranti` si legge, gli altri campi restano, salvato non contiene più quelle chiavi.
- [ ] **Step 2:** `cargo test --workspace panoramica carte_tutte azione_al voce_ orologio_a in_attesa nomi_per configurazione_coi` → FAIL.
- [ ] **Step 3: Implementa** la Griglia sugli schemi dello spike (pool, regole `changed voce-presente` / `changed con-azione` col fuoco al più vicino, `tasti` unico `FocusScope`), la voce e la riga alta; togli layout, quadranti, puntini, riga «In attesa», la sezione «Quadranti» delle Impostazioni e la loro configurazione, coi test che li riguardavano (≈ 25, elenco in 08 §7.3: `quadranti_*`, `ora_piccola_nascosta_solo_nell_orologio`, `il_fuoco_su_piu_n_*`, `da_tre_voci_*`, `il_piu_n_si_raggiunge_*`, `la_riga_luci_*`, `panoramica_col_fuoco_sulle_luci`, `quadrante_gpu`, `selettore_del_quadrante`, `layout_*` …). «N luci accese» porta ancora alla Casa filtrata: è il clic sulla carta Casa con luci accese (`apri-luci` di oggi), non più una riga.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto dell'istanza di prova: Griglia calma, con `sessioni_finte` (Agenti al lavoro, luce arancio) e con `--prova-avviso bloccante` (voce al posto dell'orologio), a 100 % e 175 %; confronto con `tc-griglia-4`, `tc-griglia-aspetta`, `tc-griglia-fuoco`; foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): Panoramica a griglia con voce che ti aspetta e pool di fuoco; via layout e quadranti`.

### Task 9: Panoramica Orologio e Impostazioni › Panoramica

**Files:**
- Modify: `ui/guscio/panoramica.slint` (`Orologio`, `Fascia`, `Tessera` = `CartaVista` alta 84), `ui/guscio/carta.slint` (altezza parametrica 108/84), `ui/pannello.slint` (`in property <int> panoramica`), `ui/impostazioni.slint` (sezione Isola: «PANORAMICA» con la scelta «Griglia» / «Orologio» al posto di «QUADRANTI»), `src/moduli/panoramica.rs` (`fascia`), `src/ponte.rs` + `src/finestra_impostazioni.rs` (scelta e salvataggio), `crates/casa-core/src/config.rs` (`panoramica`), `CHANGELOG.md`
- Test: `moduli/panoramica.rs`, `test_ui.rs`, `casa-core/tests/config.rs`

**Interfaces:**
- Consumes: `Posto`, `CartaVista`, `VoceInAttesa`, `OrologioGrande`, `ordine_tab` (Task 8).
- Produces:
  - `Config.panoramica: String` (`#[serde(default)]`; `""`/`"griglia"` = Griglia, `"orologio"` = Orologio; ogni altro valore = Griglia).
  - `pub enum ModoPanoramica { Griglia, Orologio }` con `fn da_config(s: &str) -> ModoPanoramica` e `fn come_str(self)`; `Pannello.panoramica: int` (0 Griglia, 1 Orologio).
  - `pub fn fascia(voci_aspetta: usize, avvisi: usize, in_attesa_di_dati: usize) -> (String, String)` → testo e nome per Narrator: «Niente ti aspetta» (+ «· {n} avvisi ›» se ce ne sono); al primo avvio «Avvio in corso · {n} moduli in attesa di dati».
  - Orologio: orologio grande, fascia alta 32 senza fondo alla stessa quota sempre, 4 tessere 182 × 84 r 20 centrate con l'azione **sempre** visibile (stessi posti del pool, stesso ordine di Tab); la voce prende il posto dell'orologio (dissolvenza 160 ms, voce da scala 0,96 in 280 ms; 0 ms col movimento ridotto), le tessere restano ferme.
  - «{n} avvisi ›» della fascia è un bersaglio (`ID_IN_ATTESA`, stesso posto nell'ordine di Tab) e apre la Coda.
  - Impostazioni: `in property <int> panoramica-scelta; callback scegli-panoramica(int);` (riga `RigaScelta` «Panoramica» con voci «Griglia», «Orologio»); `Ponte::scegli_panoramica(&self, i: i32)` salva `Config.panoramica` solo se cambia.

- [ ] **Step 1: Test che falliscono**:
  - `panoramica::test::fascia_per_regola`: (0, 0, 0) → «Niente ti aspetta»; (0, 2, 0) → «Niente ti aspetta · 2 avvisi ›»; (0, 0, 3) → «Avvio in corso · 3 moduli in attesa di dati».
  - `panoramica::test::modo_dalla_configurazione`: `""`, `"griglia"`, `"quadranti"`, `"boh"` → Griglia; `"orologio"` → Orologio.
  - `test_ui::panoramica_orologio_con_quattro_tessere`: tessere alte 84 alla stessa y, azioni visibili senza passaggio, fascia presente e alla stessa y con e senza voce.
  - `test_ui::orologio_voce_al_posto_dell_ora`: con la voce l'orologio non c'è, le tessere non si muovono, fuoco su «Nega».
  - `test_ui::impostazioni_scelta_della_panoramica`: la riga «Panoramica» c'è nella sezione Isola, nessuna riga «Quadrante 1»; la scelta chiama `scegli-panoramica(1)`.
  - `ponte::test::panoramica_scelta_salvata`: `scegli_panoramica(1)` scrive `"orologio"` senza toccare gli altri campi; il pannello seguente si apre in Orologio.
- [ ] **Step 2:** `cargo test --workspace fascia modo_dalla panoramica_orologio orologio_voce impostazioni_scelta panoramica_scelta` → FAIL.
- [ ] **Step 3: Implementa** la modalità Orologio sugli stessi posti del pool e la scelta nelle Impostazioni (salvata come le altre scelte dell'isola, scrittura solo se cambia).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto Orologio calma e con `sessioni_finte` a 100 %, confronto con `tc-pan-calma`, `tc-pan-lavora`, `tc-pan-aspetta`; foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): Panoramica Orologio con fascia e tessere, scelta in Impostazioni › Panoramica`.

### Task 10: Passata sulla base nera — pagine di oggi, pillola, Impostazioni, Gestione

**Files:**
- Modify: `ui/viste/{casa,claude,pc,collegamenti}.slint`, `ui/pillola.slint`, `ui/faccia.slint`, `ui/menu.slint`, `ui/componenti.slint`, `ui/impostazioni.slint`, `crates/gestione/ui/{cornice,schede}.slint`, `CHANGELOG.md`
- Test: `test_ui.rs` (e i test della gestione se toccano misure)

**Interfaces:**
- Consumes: i token del Task 1; il corpo 752 × 224 del Task 6. Solo token, contrasti e misure rotte dal carattere nuovo: **nessun ridisegno d'impianto** (le pagine nuove sono F2–F3, la pillola nuova F4).

- [ ] **Step 1: Test che falliscono**:
  - `contrasti_delle_superfici_vecchie`: per ogni coppia testo/fondo usata dalle pagine, dalla pillola, dalle Impostazioni e dalla gestione (elenco nel test: `testo-2` su `g1`, `testo-tenue` su `g2`, `colore-livello(*)` su `nero`, testo dei pulsanti su `g3`, `primario`, segnaposto su `g1`, bordo dei campi su `g1` …) contrasto ≥ 4,5:1 (testo) o ≥ 3:1 (segni e bordi).
  - `pagine_dentro_il_corpo`: nelle viste Casa, Agenti, PC, Collegamenti (dati finti) nessun elemento accessibile esce dal rettangolo del corpo (34, 44, 752, 224).
  - `impostazioni_a_finestra_minima` e `sezione_claude_a_finestra_minima` aggiornati: coi caratteri nuovi nessun pulsante tagliato.
- [ ] **Step 2:** `cargo test --workspace contrasti_delle pagine_dentro a_finestra_minima` → FAIL dove i caratteri o i fondi nuovi rompono qualcosa.
- [ ] **Step 3: Implementa** i ritocchi: fondi delle card su `g1`, righe scelte su `g2`, pulsanti secondo la regola bianco/grigio, anello di Windows (bianco 2 + nero 1), misure corrette dove il testo nuovo non sta, coordinate delle pagine adattate al corpo nuovo. La gestione resta con la sua testata «CONTENUTO ESTERNO».
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto di ciascuna superficie ritoccata (spec §11): pagine Agenti (con `sessioni_finte`), Casa (senza casa: stato vuoto; con dati finti se disponibili negli strumenti di prova), PC, Collegamenti; pillola a riposo, con attività, con avviso, bloccante, ritirata; Coda; Impostazioni (tutte le sezioni); Gestione con `finta_api`; a 100 % e 175 %. Nessun testo tagliato senza ellissi; foto cancellate.
- [ ] **Step 6:** commit `fix(assistente, gestione): pagine, pillola, Impostazioni e gestione sulla base nera`.

### Task 11: Chiusura della fetta

**Files:**
- Modify: `CHANGELOG.md` (sezione «Ridisegno F1» completa, misure), `README.md` (nomi visibili cambiati: «Agenti», Panoramica Griglia/Orologio, niente Quadranti), `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (riga «F1 fatta» nel §8, con ciò che il piano ha deciso diversamente)

- [ ] **Step 1: Verifica completa**: `cargo test --workspace` → tutti verdi (numero annotato); `cargo clippy --workspace --all-targets -- -D warnings` → nessun avviso; `cargo build --release -p assistente` → ok.
- [ ] **Step 2: Misura** con `tools\misura.ps1 -Secondi 60` sull'exe release, istanza di prova, stesse condizioni dello Step 1 del Task 1: a riposo con la pillola **RAM privata ≤ partenza + 2 MB e CPU ≤ 0,05 %**; pannello aperto fermo (`--apri`) **≤ 30 MB privata e CPU ~0 %**; dopo la chiusura del pannello ritorno entro +2 MB. Fotogrammi a riposo contati dai `RedrawRequested` di winit (non dal notificatore di rendering, che con Skia software non viene chiamato): 0 a pannello fermo. Cifre nel CHANGELOG come per M3.
- [ ] **Step 3: Confronto visivo** coi riferimenti approvati: `tc-griglia-4`, `tc-griglia-aspetta`, `tc-pan-lavora`, `tc-stati-misto` (le foto dei Task 6, 8, 9, 10 rifatte sull'exe finale), solo della finestra dell'app, cancellate dopo.
- [ ] **Step 4: Revisione dell'intero ramo** da un revisore su Opus (codice + foto affiancate), poi le correzioni.
- [ ] **Step 5: Prova dal vivo con Paolo** (una sola riapertura della sua isola, col suo ok): casa vera senza azioni che suonano o allarmano; Claude Code vero con un permesso negato e uno consentito, dalla voce della Panoramica e dalla Coda; Tab, frecce, Ctrl+Invio, Esc e Narrator sulla Panoramica e sulla Coda; volume (clic e rotella); scelta Griglia/Orologio; scala 100 % e 175 %. Esito scritto nel CHANGELOG.
- [ ] **Step 6: Pulizia** di `target\agente` (incrementali e artefatti dei giri) e delle foto rimaste.
- [ ] **Step 7:** commit `docs: chiusura del ridisegno F1 (misure, prova dal vivo)` (con l'ok di Paolo; nessun push senza un suo sì).
