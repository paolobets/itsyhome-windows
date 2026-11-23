# Assistente "isola" — Ridisegno, fetta F2 (Elenco + dettaglio, pagina Agenti, permesso del guscio) — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** la pagina Agenti passa all'impianto **Elenco + dettaglio** (sessioni a sinistra, riga scelta o permesso a destra, sintesi coi limiti per fornitore in cima), il permesso diventa **un componente del guscio** con tutte le regole di M3 dentro (usato dalla Coda e dalla pagina Agenti), e si chiudono prima i punti parcheggiati da F1 (n1/n2 della voce, «Errore» blu, «Nega» muto senza segno).

**Architecture:** come in F1, **funzioni pure sopra `Ponte`**: `src/moduli/pagina_agenti.rs` calcola righe, sintesi e stato della pagina dai dati che `con_claude.rs` ha già (sessioni, limiti, spesa, permessi in attesa) e `con_claude.rs` li scrive con `aggiorna_modello` solo a pagina aperta. In Slint i modelli di pagina sono **dell'impianto, non del modulo** (`elenco-*` sul `Pannello`, spec §5.10); l'impianto sta in `ui/impianti/elenco_dettaglio.slint`, testata e stati di pagina in `ui/guscio/pagina.slint`, il permesso in `ui/guscio/permesso.slint`. La tastiera resta governata da `tasti` nel pannello, che per il permesso chiama `Permesso.tasto(...)` (schema di `MenuScuro.tasto`): le regole stanno nel componente, il pannello smista soltanto.

**Tech Stack:** Rust 2021, Slint `=1.18.1` (Skia software), `windows-sys 0.52`, chrono; strumenti di prova `i_slint_backend_testing` (`mock_elapsed_time`, `ElementHandle`).

**Spec:** `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (vincolante; **§13 prevale**; §8 «F1 fatta» dice cosa F1 ha deciso diversamente e vale anche qui). Si leggono con lei: `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` (punti portati in F2), `.superpowers/ux/studio/10-tc-isola.md`, `08-fattibilita-sistema.md` (§2.2 modelli degli impianti, §2.3.4 il permesso, §7.3 test da riscrivere), `11-spike.md`; bozza `.superpowers/ux/studio/bozze/tc-isola.html` (`paginaAgenti()` in `tc-isola.js`, misure in `tc-isola.css`: `.sintesi`, `.righe`/`.riga`, `.scheda-nera`, `.lim`, `.kbd`), schermate `bozze/screenshot/tc-agenti.png`, `tc-coda.png`, `tc-griglia-aspetta.png`, `tc-vivo-agenti.png`. Stile: `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1.md`.

## Decisioni di Paolo (23/11, sulle scelte del piano)

1. **Clic su una riga di sessione = la sceglie** e il dettaglio la mostra; il terminale dal pulsante del dettaglio o con Invio sulla riga (come sotto).
2. **Limiti a barra e spesa con «Dettagli» nella sintesi** (come sotto).
3. **«chiesto alle 14:32»** e **solo il nome della cartella** (come sotto).
4. **«Consenti» neutro come «Nega»**: confermato esplicitamente da Paolo (chiude la Ruling «procedi senza scegliere» di F1).
5. **Clic sulla pillola con un permesso → pagina Agenti** con quella sessione scelta e il permesso leggibile nel dettaglio (spec §5.4 «apre il modulo»), non più la Coda col permesso aperto (oggi `DallaPillola::Permesso(indice)` → `apri-coda-su`). Task 8.

## Decisioni prese dal piano (dove spec e bozza lasciano la scelta)

- **File nuovi**: Rust `src/moduli/pagina_agenti.rs`; Slint `ui/guscio/permesso.slint` (`Permesso`), `ui/guscio/pagina.slint` (`TestataPagina` spostata qui da `elenco.slint` e allargata, `StatoPagina`), `ui/impianti/elenco_dettaglio.slint` (`ImpiantoElencoDettaglio`, `RigaModulo`, `DettaglioRiga`). Spariscono `ui/viste/claude.slint`, `VoceAperta` (in `elenco.slint`), le strutture `RigaSessioneUi`/`LimiteClaudeUi` e le proprietà `claude-*` del `Pannello`.
- **La Coda tiene la sua riga** (`RigaElenco` su `VoceAttesaUi`, coi pulsanti) fino a F5 «Coda completa»; la riga generica `RigaModulo` (segno · nome · chi · cosa · valore) nasce qui per Agenti. Il permesso aperto della Coda passa invece **subito** al componente `Permesso`.
- **Niente array dentro le strutture dei modelli** che si confrontano prima di scriverle (`ModelRc` si confronta per puntatore, una riga sembrerebbe sempre cambiata): il dettaglio di una riga ha **3 posti fissi** `d1`/`d2`/`d3`, la sintesi **2 posti di limite** `l1`/`l2` (il secondo fornitore li allarga nella sua milestone).
- **Il permesso nel dettaglio non viaggia nella riga**: la riga porta l'id della voce e il suo indice in `attese`; il dettaglio mostra `attese[attesa]` **solo se** il suo id è ancora quello (mai un permesso diverso da quello della riga).
- **Dati del permesso** (`DatiPermesso`: sessione, strumento, comando, cartella, ora della richiesta) **sulla `Voce`**, non nel registro dei permessi di Claude: il componente del guscio li riceve da qualunque agente. Solo memoria, solo testi già mostrati oggi (riassunto ripulito e nome della sessione); la **cartella resta il solo nome finale** (come oggi, mai il percorso della bozza `~/progetti/my-shop`).
- **«Chiesto alle 14:32»** al posto di «da 42 s» della bozza: un contatore dei secondi vorrebbe un timer (spec §3).
- **Clic su una riga = la sceglie** (il dettaglio la mostra); il terminale si apre dal pulsante del dettaglio o con Invio sulla riga (come oggi). Il pulsante della riga scelta sta **in alto a destra** del dettaglio, i pulsanti del permesso **in basso a destra** (bozza): un clic su «Nega» non trova mai un pulsante nato sotto di sé quando la riga torna «al lavoro».
- **Limiti a barra** nella sintesi (bozza `.lim`: fornitore in grassetto sul primo, nome, barra 44 × 4, «61 %» mono; oltre la soglia dell'utente barra e numero in `attivo`; valore ricordato dopo un riavvio in `testo-tenue` con «dalle 20:29» per Narrator); via gli anelli. **Spesa API** nella sintesi, dopo i limiti, mono («412,80 $») con la nota per Narrator, e **«Dettagli»** come azione secondaria della sintesi solo con la chiave da amministratore.
- **Titolo della sintesi** calcolato da `pagina_agenti::sintesi` («1 ti aspetta» / «2 ti aspettano» se ci sono permessi, altrimenti il titolo della carta); il campo `r1` del contratto arriva in F3, quando le pagine con sintesi diventano quattro, e lo assorbe.
- **Aspetto inerte durante l'armamento** (punto «Nega muto senza segno» di F1): `Pannello.armamenti` è un contatore che Rust incrementa a ogni riarmo; nel componente `property <float> armato-fino: armamenti` con `animate armato-fino { duration: 600ms; easing: linear; }` e `inerte = armato-fino < armamenti - 0.001`. Nessun timer: è un'animazione di 600 ms che parte solo al riarmo, **anche con «Mostra animazioni» spento** (è una soglia di tempo, non un movimento). Rust resta l'unica guardia vera.
- **Le decisioni del permesso passano sempre da `azione-voce`** (guardia di 600 ms su entrambi i pulsanti, `Ponte::azione_voce`), anche nella Coda e nella pagina Agenti; prima la Coda usava `azione`.
- **Un permesso aperto nella Coda che cambia posto resta aperto** (minor di F1 Task 7): il pannello chiede a Rust l'indice dell'id aperto (`indice-voce`) quando le voci cambiano.

## Global Constraints

- Ramo `v3-assistente`; `main` non si tocca. Ogni commit locale con l'ok di Paolo; nessun push, tag o merge senza un suo sì esplicito.
- Testi per l'utente in italiano, verbo in testa sui pulsanti; commenti, commit e CHANGELOG in italiano. Ogni task aggiorna `CHANGELOG.md` (sezione Unreleased v3, «Ridisegno F2») nel proprio commit.
- **Leggerezza** (spec §3): nessuna animazione continua (resta solo il battito degli occhi, decisione di Paolo); animazioni solo durante una transizione (l'aspetto inerte di 600 ms parte solo a un riarmo); **nessun timer nuovo** (la durata delle sessioni usa il timer da un secondo che c'è già, acceso solo se una sessione lavora e la pagina è aperta); le proprietà Slint si scrivono **solo quando il valore cambia** (`aggiorna_modello`, confronto prima di `set_*`); i modelli della pagina si riempiono solo a pagina aperta.
- **Sicurezza di Claude invariata su ogni superficie** (spec §7, M3): fuoco iniziale su «Nega», **solo Ctrl+Invio esatto** consente (Invio mai), armamento di 600 ms (`ARMAMENTO_CONSENTI`) ogni volta che un «Consenti» compare o cambia posto (anche quando il dettaglio passa a un altro permesso), **testo intero e sessione leggibili prima del consenso** («Consenti» solo dopo il fondo del testo), **nessun pulsante che nasce sotto il bersaglio di un clic precedente**, **«Consenti» neutro come «Nega»** (regola M3, confermata da Paolo il 23/11), nessun «Sempre», nessuna risposta senza gesto; il comando **mai** nella notifica di Windows.
- **Nomi delle sessioni** = titolo della scheda di Claude Code (`session_name` dalla barra di stato, `claude::nome_visto`), altrimenti la cartella. Fornitore come **parola** («Claude»), mai come colore.
- **Dati**: nessun dato nuovo su disco in F2; minimizzazione di M3 (nessun testo di prompt, niente di nuovo in `diagnosi.log`); `DatiPermesso` vive solo in memoria.
- **Mai toccare l'Home Assistant vero** né l'isola vera di Paolo nelle prove automatiche e nelle foto; dal vivo solo con Paolo e col suo sì (le azioni che suonano o allarmano mai senza ok).
- Test verdi: `cargo test --workspace`; i test di parti tolte (vista Claude, `VoceAperta`) si cancellano **nello stesso task che toglie la parte**, e le regole che proteggevano (Ctrl+Invio esatto, fuoco su «Nega», consenso dopo il fondo, fuoco mai perso, terminale dalla riga) rinascono sulla superficie nuova **prima** di cancellare le vecchie.
- `cargo clippy --workspace --all-targets -- -D warnings` pulito; codice solo-Windows dietro `#[cfg(windows)]`, nessun codice morto su Linux/macOS (CI sui tre).
- Build e test **solo da PowerShell** con `$env:CARGO_TARGET_DIR='C:\Work\Sviluppo\itsyhome-windows\target\agente'` (una sola cartella condivisa, ripulita a fine fetta); mai scrivere sorgenti da PowerShell 5.1 (solo con gli strumenti di modifica dei file). Rust 1.94 in locale e 1.100 in CI: niente API deprecate in 1.100 né assenti in 1.94. Suite in primo piano.
- Prove e foto su un'**istanza di prova** (copia dell'exe, `ITSYHOME_ASSISTENTE_DIR` su una cartella finta, nessuna casa; per le sessioni `ITSYHOME_AIUTANTE_ATTESO` sull'esempio `sessioni_finte`). **Foto solo della finestra dell'app** (PrintWindow del proprio HWND o `tools/foto-zona.ps1` sulla sua zona), cancellate dopo il confronto. **Input sintetico solo al proprio PID** (UI Automation o `PostMessage` alla propria HWND; mai SendInput, SetCursorPos, mouse_event né clic reali). Processi fermati solo per PID, dopo aver controllato il percorso.
- Colori solo come token di `ui/tema.slint` (`ui_senza_colori_a_mano` resta), simboli sempre SVG o `Path` (mai glifi Unicode, «›» e «$» compresi: il «$» è testo mono nel colore `dollaro`, ammesso perché è un carattere del comando, non un simbolo), testo ≥ 12 px (mono maiuscolo ≥ 11 con tracking .07em), contrasto: parola ≥ 4,5:1, segno e bordo ≥ 3:1. Stato = colore + forma (occhietti) + parola.

## Review Focus

1. **Permesso risolto nel terminale col fuoco sul dettaglio** (fuoco su «Consenti» nella pagina Agenti, la sessione va avanti e la voce sparisce) → il fuoco torna sulla riga della stessa sessione nell'elenco, il Tab seguente non si perde, nessun pulsante della riga scelta nasce dov'era «Nega» (Task 7 `permesso_risolto_col_fuoco_sul_dettaglio`, `agenti_nessun_pulsante_nasce_sotto_il_clic`).
2. **Molte sessioni e testi lunghi** (16 sessioni, titolo di 80 caratteri, comando di 2000 caratteri su una riga sola) → colonna che scorre col fuoco in vista, ellissi nelle righe con il nome intero per Narrator, comando leggibile tutto nel dettaglio e «Consenti» solo dopo il fondo (Task 6 `agenti_testi_lunghi_con_ellissi`, Task 3 `permesso_lungo_su_una_riga_si_legge_tutto`).
3. **Claude scollegato, nessuna sessione, limiti assenti o ricordati, senza chiave da amministratore** → stato della pagina con frase (e «Collega» se scollegato), sintesi senza «0 %» né barre vuote, «Dettagli» assente (Task 5 `limiti_per_fornitore`, `stato_della_pagina`; Task 6 `agenti_stati_vuoti`).
4. **La scelta che arriva su un permesso senza un gesto sul permesso** (↑/↓ sulle righe, una sessione nuova che riordina, la riga col fuoco tolta che passa alla vicina) → «Consenti» si riarma (aspetto inerte), il fuoco non è mai sul consenso, Invio non concede (Task 7 `agenti_dettaglio_che_diventa_permesso_riarma`).
5. **Un permesso senza dati di sessione** (voce di prova `--prova-avviso`, domani un agente che non dà la cartella) → il componente mostra titolo e dettaglio in mono con le stesse regole, nessun campo vuoto a vista (Task 3 `permesso_senza_dati_mostra_il_dettaglio`).

---

### Task 1: Misura di partenza e voce con un solo pulsante delicato (n1, n2)

**Files:**
- Modify: `crates/assistente/ui/pannello.slint` (regole `changed voce-mostrata`, `changed voce-id`, `vai`), `crates/assistente/ui/guscio/voce.slint` (`fine-testo`), `CHANGELOG.md`
- Test: `crates/assistente/src/test_ui.rs`

**Interfaces:**
- Produces: in `Pannello` `pure function bersaglio-iniziale-voce() -> int` — `Bersagli.nega` se `panoramica-el.indice-nega >= 0`; altrimenti `Bersagli.consenti` se l'altro pulsante è «Apri» (delicato e `!voce-intera`); altrimenti `Bersagli.altre` se `voce-altre`; altrimenti `0` se `carte > 0`; altrimenti `-1`. Mai un consenso. Usata da `changed voce-mostrata`, `changed voce-id` (ora anche con `fuoco == Bersagli.nega`) e da `vai("panoramica")`.
- `VoceInAttesa.fine-testo` = `(indice-nega >= 0 ? nega.x : indice-altro >= 0 ? altro.x : width - margine) - 12px`.

- [ ] **Step 1: Misura di partenza** su HEAD d1c2845 (prima di toccare il codice): build release, istanza di prova; `tools\misura.ps1 -Secondi 60` a riposo, con `--apri` (Panoramica ferma) e con la pagina Agenti aperta mentre `sessioni_finte --sessioni 3` lavora (aperta con `PostMessage` di Tab/Invio alla propria finestra). Annotare RAM privata e CPU medie nel rapporto del task: sono il «prima» del Task 9.
- [ ] **Step 2: Test che falliscono**:
  - `voce_sostituita_da_una_con_solo_pulsante_delicato_tiene_il_fuoco` (n1): fuoco su «Nega» della voce A; il modello mette al suo posto B con il solo pulsante delicato e testo lungo → dopo `update_timers_and_animations()` il fuoco è su «Apri»; Tab seguente va alla carta 0. Con B a testo corto («Consenti» visibile) e una seconda voce → fuoco su «+1 in attesa», mai su «Consenti»; senza seconda voce → carta 0.
  - `voce_che_compare_con_solo_pulsante_delicato`: la stessa B che compare su una Panoramica senza voce → stesso bersaglio della regola sopra.
  - `testo_della_voce_non_passa_sotto_apri` (n2): voce con il solo «Apri» → titolo e riquadro del dettaglio finiscono a x ≤ x di «Apri» − 12.
  - `titolo_della_voce_con_strumento_al_massimo` (F1 Task 8 m5): «Claude chiede: » + 12 «W» (`MASSIMO_STRUMENTO`) misurato con `misura::larghezza(_, Stile::Titolo)` sta nella larghezza del titolo della voce con due pulsanti (476 px).
- [ ] **Step 3:** `cargo test -p assistente voce_sostituita voce_che_compare_con testo_della_voce titolo_della_voce_con` → FAIL (n1, n2; l'ultimo può passare già: allora resta come guardia).
- [ ] **Step 4: Implementa** `bersaglio-iniziale-voce()` e `fine-testo`.
- [ ] **Step 5:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 6:** commit `fix(assistente): voce con un solo pulsante delicato, fuoco e testo al loro posto`.

### Task 2: Dati del permesso sulla voce

**Files:**
- Modify: `crates/assistente/src/attenzione.rs` (`DatiPermesso`, campo `Voce.permesso`), `src/claude.rs` (`voce_permesso`), ogni letterale `Voce { … }` (34: `attenzione.rs`, `claude.rs`, `ponte/con_claude.rs`, `ponte/con_claude/collegamento.rs`, `ponte/con_moduli.rs`, `sistema/{batterie_bt,menu,scorciatoie}.rs` → `permesso: None`), `src/ponte/con_claude.rs` (passa `adesso_unix()` a `voce_permesso`), `src/ponte.rs` (`voce_attesa_ui`), `ui/voci.slint` (`PermessoUi`, campo `VoceAttesaUi.permesso`), `CHANGELOG.md`
- Test: `attenzione.rs`, `claude.rs`, `ponte.rs`, `test_ui.rs` (il costruttore `voce_coda` aggiunge `permesso: Default::default()`)

**Interfaces:**
- Produces (Rust): `#[derive(Debug, Clone, PartialEq, Eq, Default)] pub struct DatiPermesso { pub sessione: String /* nome_visto(titolo, cartella) */, pub strumento: String /* strumento_italiano */, pub comando: String /* riassunto ripulito, "" se assente */, pub cartella: String /* nome finale ripulito */, pub alle: i64 /* secondi Unix della richiesta */ }`; `Voce.permesso: Option<DatiPermesso>`. `claude::voce_permesso(id: u64, e: &EventoHook, titolo: &str, ora: Instant, alle: i64) -> Voce` (titolo, dettaglio e pulsanti invariati).
- Produces (Slint): `export struct PermessoUi { sessione: string, strumento: string, comando: string, cartella: string, chiesto: string /* «alle 14:32»; "" senza dati */ }`; `VoceAttesaUi.permesso: PermessoUi` (`sessione == ""` = voce senza dati di permesso). `voce_attesa_ui` lo riempie; `chiesto` con l'ora locale di `alle` (`%H:%M`).

- [ ] **Step 1: Test che falliscono**:
  - `claude::test::voce_permesso_porta_i_dati_ripuliti`: con un riassunto che contiene `\u{202E}`, a capo e invisibili → `comando` ripulito come il titolo; senza riassunto `comando == ""`; `sessione` = titolo della sessione se c'è, altrimenti cartella; `alle` quello passato; titolo e dettaglio uguali a prima.
  - `attenzione::test::la_notifica_di_un_permesso_non_ha_il_comando` allargato: `testo_notifica` della voce di `voce_permesso` non contiene `comando` né il riassunto, contiene il nome della sessione (M3 §10).
  - `ponte::test::voce_attesa_ui_col_permesso`: `permesso.sessione/strumento/comando/cartella` come la voce, `chiesto` comincia con «alle »; una voce senza dati → `PermessoUi` vuoto.
- [ ] **Step 2:** `cargo test -p assistente voce_permesso_porta la_notifica_di_un_permesso voce_attesa_ui_col` → FAIL.
- [ ] **Step 3: Implementa** struttura, campo e passaggio; nessuna superficie cambia ancora.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): dati del permesso sulla voce, per il componente del guscio`.

### Task 3: Componente `Permesso` del guscio, al posto della voce aperta nella Coda

**Files:**
- Create: `crates/assistente/ui/guscio/permesso.slint`
- Modify: `ui/impianti/elenco.slint` (via `VoceAperta`; `ImpiantoElenco` monta `Permesso` con `pulsanti-in-alto: true`), `ui/pannello.slint` (via `coda-visto`, `coda-testo-su/giu` e i rami del permesso aperto in `freccia`/`attiva`/`indietro`: diventano `coda-permesso.tasto(...)`; decisioni su `azione-voce`; `callback indice-voce(string) -> int` e regola su `n-attese` che segue l'id aperto), `src/ponte.rs` (`on_indice_voce`: posizione dell'id nel modello `attese`, -1 se assente), `CHANGELOG.md`
- Test: `test_ui.rs`

**Interfaces:**
- Consumes: `PermessoUi`, `VoceAttesaUi.permesso` (Task 2); `Pulsante`, `RegolaPulsanti`, `Anello`, `Fuoco` (`componenti.slint`), `Capsula` (`guscio/carta.slint`), `Permessi.chiede`.
- Produces: `export component Permesso inherits Rectangle` con
  - `in property <VoceAttesaUi> voce; in property <bool> pulsanti-in-alto; in property <bool> con-fuoco; in property <int> armamenti /* Task 4 */;`
  - `out property <int> fuoco /* -2 testo, 0/1 pulsante */; out property <bool> visto;`
  - `callback decidi(string /* id del pulsante */); callback visto-fondo(); callback lascia(); /* ← dal primo bersaglio o ↑ dal testo in cima: torna a chi lo contiene */`
  - `public function entra()` — fuoco sul primo non delicato («Nega»), altrimenti sul testo; `public function tasto(k: string, ctrl-esatto: bool) -> bool` — ↑/↓ scorrono il testo (36 px), ↓ o → a fondo visto passano ai pulsanti, ←/→ fra i pulsanti (un consenso non visto non si raggiunge), Invio preme solo un pulsante non delicato, **Ctrl+Invio esatto** solo sul delicato dopo il fondo; restituisce vero se ha usato il tasto.
  - Aspetto (bozza `paginaAgenti`): con `voce.permesso.sessione != ""` capsula «Chiede il permesso» (tono `aspetta`, occhietti) e a destra «chiesto {chiesto}» (`testo-tenue` 12); riquadro `nero` r16 con filo: «TERMINALE» (mono 11, .07em, `testo-tenue`), «$» (`dollaro`) + comando (mono 14/20, `testo`, a capo, scorrevole con sfumatura se c'è altro), sotto «{sessione} · {cartella}» (mono 12, `testo-tenue`; una sola se coincidono); senza dati: titolo (Inter 700 14, a capo fino a 3 righe) e `dettaglio` intero in mono 13. In fondo a sinistra «Leggi fino in fondo per consentire» o il tasto `Ctrl Invio` (chip mono 11 r6 `g3`) + «consente». «Nega» e «Consenti» 40 r12 a **posti fissi** (in alto a destra con `pulsanti-in-alto`, altrimenti in basso a destra), neutri entrambi; il posto di «Consenti» c'è anche nascosto (invisibile, inerte, senza nome per Narrator). Narrator: `groupbox` «Ti aspetta · {modulo}: {sessione} chiede: {strumento}» (senza dati: «Ti aspetta · {modulo}: {titolo}»), descrizione = comando e cartella (o `dettaglio`). Un'altra `voce.id` → testo da capo, `visto` falso.

- [ ] **Step 1: Test che falliscono** (le regole di sicurezza prima di togliere `VoceAperta`):
  - Restano verdi così come sono (solo la via d'accesso cambia): `coda_entra_su_nega`, `permesso_aperto_si_legge_intero`, `nega_non_si_sposta_quando_compare_consenti`, `consenso_impossibile_prima_del_fondo_del_testo`, `fondo_del_testo_lungo_riarma_il_consenso`, `dalla_pillola_il_permesso_si_apre_intero`, `coda_nega_e_consenti_sono_neutri`, `coda_consenti_solo_con_ctrl_invio_esatto`, `ultima_voce_tolta_col_fuoco_addosso`, `altre_in_attesa_lontano_dai_nega_della_coda`.
  - `doppio_clic_su_apri_non_consente` rinominato `consenti_non_nasce_sotto_apri` (F1 Task 7 n3: prova la posizione, il nome lo dice).
  - `permesso_mostra_comando_e_cartella`: voce con `PermessoUi` → testi «Chiede il permesso», «$», il comando, «my-shop»; nome per Narrator «Ti aspetta · Agenti: my-shop chiede: Comando».
  - `permesso_senza_dati_mostra_il_dettaglio`: voce di prova (dati vuoti) → titolo e dettaglio, nessun «TERMINALE», stesse regole (Ctrl+Invio solo dopo il fondo).
  - `permesso_lungo_su_una_riga_si_legge_tutto`: comando di 2000 caratteri senza spazi → va a capo nel riquadro (nessun testo oltre il bordo destro), «Consenti» solo dopo ↓ fino in fondo.
  - `permesso_che_cambia_posto_resta_aperto`: permesso aperto alla riga 1, la voce 0 tolta → resta aperto (stesso id), il fuoco resta sul suo bersaglio; un permesso **diverso** al suo posto lo chiude.
  - `decisioni_del_permesso_passano_dalla_guardia`: clic e Invio su «Nega», Ctrl+Invio su «Consenti» nella Coda → eventi su `azione-voce`, nessuno su `azione`.
- [ ] **Step 2:** `cargo test -p assistente permesso_ consenti_non_nasce decisioni_del_permesso` → FAIL.
- [ ] **Step 3: Implementa** il componente e il passaggio della Coda; togli `VoceAperta` e lo stato del permesso aperto dal pannello (resta `coda-aperta`, l'id).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): il permesso è un componente del guscio, usato dalla Coda`.

### Task 4: Aspetto inerte durante l'armamento

**Files:**
- Modify: `ui/guscio/permesso.slint`, `ui/guscio/voce.slint` (`VoceInAttesa`), `ui/pannello.slint` (`in property <int> armamenti`, passato a voce e permesso), `src/ponte/con_claude.rs` (`claude_riarma`, `claude_superficie_compare`: dopo aver spostato `dal`, `Pannello.armamenti += 1` se il pannello c'è), `CHANGELOG.md`
- Test: `test_ui.rs`, `ponte/con_claude.rs`

**Interfaces:**
- Consumes: `Permesso.armamenti` (Task 3).
- Produces: `Pannello.armamenti: int`; nei due componenti `out property <bool> inerte` (tecnica in «Decisioni»): pulsanti con testo `testo-tenue`, niente passaggio, `premuto` ignorato e `accessible-enabled: false` mentre `inerte` (se `accessible-enabled` non c'è in 1.18.1, `accessible-description` «fra un attimo» e il test legge `Pannello.out property <bool> permesso-inerte`). Durata 600 ms anche con `Movimento.ridotto`.

- [ ] **Step 1: Test che falliscono**:
  - `con_claude::test::armamenti_contano_ogni_riarmo`: con un permesso in attesa `claude_superficie_compare` e un `claude_riarma` che sposta il permesso portano `armamenti` +1 ciascuno; senza permessi nessun cambio; nessuna scrittura a pannello chiuso.
  - `test_ui::permesso_inerte_per_600_ms`: Coda col permesso aperto, `set_armamenti(n + 1)` → «Nega» e «Consenti» inerti, un clic non produce eventi; `mock_elapsed_time(610)` + `update_timers_and_animations()` → attivi, il clic produce `azione-voce`.
  - `test_ui::voce_inerte_per_600_ms`: lo stesso sulla voce della Panoramica (F1 Task 8 m2).
  - `test_ui::inerte_anche_col_movimento_ridotto`: con `Movimento.ridotto` vero dopo 300 ms ancora inerte.
- [ ] **Step 2:** `cargo test -p assistente armamenti_contano inerte` → FAIL.
- [ ] **Step 3: Implementa.**
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): «Nega» e «Consenti» si vedono inerti per i 600 ms dell'armamento`.

### Task 5: Modelli della pagina Agenti (funzioni pure)

**Files:**
- Create: `crates/assistente/src/moduli/pagina_agenti.rs`
- Modify: `src/moduli/mod.rs` (`pub mod pagina_agenti;`), `src/claude.rs` (`RigaSessione`: via `stato_testo` e `livello`, al loro posto `stato: Stato`, `vista: bool`, `errore: Option<&'static str>` = `frase_errore_claude`; i test di `righe_sessioni` si aggiornano), `ui/voci.slint` (strutture sotto), `CHANGELOG.md`
- Test: `pagina_agenti.rs`, `claude.rs`

**Interfaces:**
- Consumes: `claude::{RigaSessione, TestoLimite, testo_limite, testo_limite_ricordato, nome_visto}`, `moduli::{Tono, misura::accorcia}`, `DatiPermesso` (Task 2).
- Produces (Slint, `voci.slint`):
  - `export struct VoceDettaglioUi { etichetta: string, valore: string, caldo: bool }`
  - `export struct RigaElencoUi { id: string, tono: TonoModuloUi, parola: string, nome: string, chi: string, cosa: string, valore: string, narratore: string, sotto: string, d1: VoceDettaglioUi, d2: VoceDettaglioUi, d3: VoceDettaglioUi, nota: string, azione-id: string, azione-testo: string, voce: string /* id della voce del permesso, "" */, attesa: int /* suo indice in `attese`, -1 */ }`
  - `export struct LimiteUi { presente: bool, fornitore: string, nome: string, frazione: float, testo: string, caldo: bool, vecchio: bool, narratore: string }`
  - `export struct SintesiUi { tono: TonoModuloUi, titolo: string, sotto: string, l1: LimiteUi, l2: LimiteUi, spesa: string, spesa-nota: string, azione-id: string, azione-testo: string }`
  - `export enum TipoStatoPaginaUi { pieno, vuoto, fuori-servizio, primo-avvio }`; `export struct StatoPaginaUi { tipo: TipoStatoPaginaUi, frase: string, sotto: string, azione-id: string, azione-testo: string }`
- Produces (Rust, `pagina_agenti`): `pub const DETTAGLI: &str = "agenti.dettagli"; pub const PERMESSO: &str = "agenti.permesso";` (riusa `agenti::{TERMINALE, COLLEGA}`);
  - `pub fn riga(r: &RigaSessione, permesso: Option<(&str, i32, &DatiPermesso)>, ripiego: bool, a_finestra: bool) -> RigaElencoUi`
  - `pub fn sintesi(collegato: bool, righe: &[RigaElencoUi], limiti: [Option<(TestoLimite, bool /* ricordato */)>; 2], soglia: u8 /* caldo se frazione × 100 ≥ soglia */, spesa: (&str, &str), con_chiave: bool) -> SintesiUi`
  - `pub fn stato(collegato: bool, righe: usize) -> StatoPaginaUi`

Scala delle righe (tono effettivo, parola = `Tono::parola`, «chi» sempre «Claude»):

| Sessione | Tono | Cosa | Valore | Dettaglio d1 / d2 / d3 | Azione |
|---|---|---|---|---|---|
| con un permesso in attesa | Aspetta | «Chiede: {strumento}» | — | (il dettaglio è il `Permesso`) | `PERMESSO` «Apri il permesso» (solo fino al Task 7) |
| lavora | Attivo | strumento («Modifica») | durata mono («3:12») | Strumento / Da (durata) / Contesto | `TERMINALE` «Apri il terminale» |
| aspetta un tuo messaggio | Guarda | «Tocca a te» | — | Stato «Rispondi nel terminale» / — / Contesto | idem |
| in errore | **Errore** (rosa, non più blu: F1 Task 10) | la frase («Limite raggiunto») | — | Errore (frase) / — / Contesto | idem |
| ha finito, non visto | Fatto (blu, `colore-tono`) | «Ha finito» | — | Stato «Ha finito» / — / Contesto | idem |
| ha finito, visto | Calmo | «Fatto» | — | Stato «Fatto» / — / Contesto | idem |

`nome` = titolo della sessione se c'è, altrimenti la cartella; `sotto` = la cartella se c'è un titolo; Contesto «42 %» con `caldo` oltre `CONTESTO_CALDO`; `nota` = «Apre l'ultimo terminale usato» (ripiego, solo Windows) o «Apre la finestra del terminale, non la scheda»; `narratore` = «{nome intero}, Claude, {per_narrator del tono}, {cosa}[, {valore}]» senza durata se cambia ogni secondo (come oggi: la durata no). Sintesi: scollegato → stato `vuoto` «Collega Claude Code» / «Qui vedrai sessioni, limiti e spesa.» / «Collega»; nessuna sessione → `vuoto` «Nessuna sessione aperta» / «Le sessioni di Claude Code compaiono qui mentre lavorano.» senza azione; titolo «{n} ti aspetta»/«{n} ti aspettano» con permessi (tono Aspetta), altrimenti «{n} al lavoro» (Attivo) o «{n} sessioni»/«1 sessione» (Calmo), `sotto` «{n} sessioni»; limiti: `l1` «Claude» «5 ore», `l2` «» «7 giorni», testo «{p} %» con lo spazio, `caldo` se `p >= soglia`, `vecchio` per il ricordato, `narratore` «Claude, limite 5 ore, 34 %, si azzera alle 14:30» (o «dalle 20:29»); spesa e nota da `claude::testo_spesa`; `azione` `DETTAGLI` «Dettagli» solo con la chiave.

- [ ] **Step 1: Test che falliscono** (in `pagina_agenti::test`):
  - `tono_per_stato`: le sei righe della tabella (tono, parola, cosa, valore, azione).
  - `errore_e_rosa_non_blu`: sessione in errore → `TonoModuloUi::Errore`, parola «Errore».
  - `riga_col_permesso`: `voce` e `attesa` riportati, tono Aspetta, cosa «Chiede: Comando».
  - `nome_e_il_titolo_della_scheda`: con titolo → `nome` = titolo e `sotto` = cartella; senza → `nome` = cartella e `sotto` vuoto; titolo di soli invisibili = assente.
  - `narratore_della_riga` e `nomi_lunghi_interi_per_narrator`: titolo di 80 caratteri intero nel `narratore`.
  - `sintesi_per_stato`: scollegato, nessuna sessione, 2 al lavoro, 1 e 2 permessi (singolare/plurale).
  - `limiti_per_fornitore`: «Claude» solo su `l1`; «61 %»; soglia 60 → `caldo`; ricordato → `vecchio` e «dalle»; limite assente → `presente` falso e nessun «0 %».
  - `spesa_e_dettagli`: senza chiave nessuna azione; con la chiave «Dettagli».
  - `stato_della_pagina`: i tre casi.
  - `claude::test` di `righe_sessioni` aggiornati ai campi nuovi (stessi casi, ordine fermo invariato).
- [ ] **Step 2:** `cargo test -p assistente pagina_agenti righe_sessioni` → FAIL.
- [ ] **Step 3: Implementa** le funzioni pure e i campi di `RigaSessione` (il resto del codice che leggeva `stato_testo`/`livello`, cioè `riga_ui` in `con_claude.rs`, si adatta senza cambiare ciò che la vista di oggi mostra).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito (`#[cfg_attr(not(test), allow(dead_code))]` su `pagina_agenti` fino al Task 6, come la Ruling C3 di F1).
- [ ] **Step 5:** commit `feat(assistente): righe, sintesi e stato della pagina Agenti come funzioni pure`.

### Task 6: Impianto Elenco + dettaglio, la pagina Agenti ci si sposta

**Files:**
- Create: `crates/assistente/ui/guscio/pagina.slint` (`TestataPagina` spostata da `elenco.slint` e allargata, `StatoPagina`), `ui/impianti/elenco_dettaglio.slint` (`ImpiantoElencoDettaglio`, `RigaModulo`, `DettaglioRiga`)
- Delete: `crates/assistente/ui/viste/claude.slint`; in `voci.slint` `RigaSessioneUi`, `LimiteClaudeUi`
- Modify: `ui/impianti/elenco.slint` (importa `TestataPagina`), `ui/pannello.slint` (via le proprietà `claude-*` e le callback `apri-sessione`, `gestisci-claude`, `apri-dettagli-claude`; al loro posto `elenco-sintesi`, `elenco-stato`, `elenco-righe`, `in-out elenco-fuoco`, `elenco-lato`, `callback azione-pagina(string, string, string)`; tastiera della vista `agenti`), `src/ponte/con_claude.rs` (`aggiorna_vista_claude_a` scrive i modelli nuovi con `pagina_agenti`; via `riga_ui`, `limite_ui`, `limite_ricordato_ui`, `limiti_della_vista`; `aggiorna_soglia_claude` ricalcola la sintesi), `src/ponte.rs` (callback; righe riscritte anche in `attese_cambiate` a pagina aperta, perché `attesa` resti giusto), `src/ponte/con_moduli.rs` (`Ponte::azione_pagina(modulo, azione, riga)`: `TERMINALE` → `claude_apri_terminale(riga)`, `COLLEGA` come la carta, `DETTAGLI` → `claude_apri_dettagli`, `PERMESSO` → Coda aperta su quella voce), `CHANGELOG.md`
- Test: `test_ui.rs` (i test della vista Claude riscritti), `ponte/con_claude.rs`

**Interfaces:**
- Consumes: strutture e funzioni del Task 5; `Occhietti` (`guscio/occhietti.slint`), `Pulsante`, `Anello`, `Fuoco`.
- Produces:
  - `TestataPagina { in property <string> titolo, sotto; in property <TonoModuloUi> tono; in property <LimiteUi> l1, l2; in property <string> spesa, spesa-nota, azione-testo; in property <bool> fuoco-azione; callback azione(); }` — alta 40; occhietti del tono (se non calmo/vuoto) + titolo Inter 700 18 + sotto 13 `testo-tenue`; a destra limiti (`.lim` della bozza), spesa mono, «Dettagli» 32 r9 neutro; **il sotto si accorcia per primo, poi il titolo** (ellissi), il gruppo di destra mai.
  - `StatoPagina { in property <StatoPaginaUi> stato; in property <bool> con-fuoco; callback azione(); }` — frase Inter 700 16 + sotto 12 + al più un pulsante, centrati nel corpo sotto la testata.
  - `ImpiantoElencoDettaglio { in property <[RigaElencoUi]> righe; in property <[VoceAttesaUi]> attese; in property <int> fuoco; in property <int> lato /* -1 testata, 0 elenco, 1 dettaglio */; in property <bool> con-fuoco; callback scelta(int); callback azione(string, string); }` — colonna 330 a x 0 da y 48 (righe 40 r12, passo 42, scorrevole, filo di scorrimento fra le colonne come oggi, riga col fuoco portata in vista); dettaglio da x 346 a 752.
  - `RigaModulo` — occhietti del tono · nome (Inter 700 13, ellissi) · chi (12 `testo-tenue`) · cosa (13 `testo-2`, ellissi) · valore (mono 12 a destra); scelta = fondo `g2` + filo; `list-item` con `narratore`, `accessible-item-index/count`.
  - `DettaglioRiga` — capsula (occhietti + parola del tono), nome Inter 700 18 (ellissi), sotto mono 12, le tre voci (etichetta 12 `testo-tenue` + valore 13, `caldo` in `attivo`), nota 12; pulsante dell'azione **in alto a destra** (32 r9, primario bianco: non è un consenso).
  - Tastiera (in `tasti`): ↑/↓ fra le righe (ai bordi si resta; ↑ dalla prima porta all'azione della testata se c'è, ↓ torna), → o Invio su una riga con azione `PERMESSO`/`TERMINALE` → il dettaglio (fuoco sul suo pulsante), Invio sulla riga di una sessione = `TERMINALE` come oggi, ← dal dettaglio torna alla riga, Backspace/Alt+← un livello (dettaglio → elenco → Panoramica), Esc chiude il pannello. Clic su una riga = `elenco-fuoco` su di lei (sceglie).

- [ ] **Step 1: Test che falliscono** (helper `pannello_agenti(stato: StatoPaginaUi, righe: Vec<RigaElencoUi>)` e `riga_agenti(id, nome, tono)` al posto di `pannello_claude`/`riga_claude`; i test di oggi rinascono qui **prima** di cancellare i vecchi):
  - `agenti_stati_vuoti` (da `vista_claude_stati_vuoti`): scollegato → frase e «Collega» (clic, Narrator, Invio → `azione-pagina agenti agenti.collega`); senza sessioni → frase, nessun pulsante.
  - `agenti_il_fuoco_e_un_indice_sempre_valido`, `agenti_righe_da_tastiera` (da `il_fuoco_delle_righe_e_un_indice_sempre_valido`, `righe_da_tastiera`), `agenti_la_colonna_scorre_e_tiene_in_vista_il_fuoco`, `agenti_con_poche_sessioni_niente_scorrimento`.
  - `agenti_dettaglio_segue_il_fuoco`: ↓ → il dettaglio mostra il nome della seconda riga; clic sulla terza → idem, nessun evento verso Rust.
  - `agenti_terminale_dalla_riga_e_dal_dettaglio`: Invio sulla riga e clic su «Apri il terminale» → `azione-pagina agenti agenti.terminale <id>`; clic sulla riga → nessun terminale.
  - `agenti_ultimo_terminale_usato_nel_dettaglio` e `agenti_titolo_e_cartella` (da `l_ultimo_terminale_usato_e_un_icona_con_il_nome`, `la_riga_col_titolo_porta_titolo_e_cartella`).
  - `agenti_sintesi_limiti_e_spesa` (da `vista_claude_spesa_del_mese`, `anelli_nascosti_senza_abbonamento`, `limiti_non_disponibili_spiega_il_motivo`): due limiti col nome per Narrator, nessun limite → nessun elemento di limite, spesa e «Dettagli».
  - `dettagli_da_tastiera_e_da_narrator` e `dettagli_anche_da_scollegato` riscritti sulla testata.
  - `agenti_riga_in_errore_e_rosa`: riga con tono `errore` → occhietti e parola col colore `Tema.errore`, non `guarda`.
  - `agenti_testi_lunghi_con_ellissi` (Review Focus 2): 16 righe con nomi di 80 caratteri e spesa larga → nessun testo oltre la sua colonna, sintesi senza sovrapposizioni (titolo, sotto e gruppo di destra), nome intero per Narrator.
  - `pagine_dentro_il_corpo` aggiornato per la vista `agenti`; `agenti_geometria`: testata y 0–40, elenco x 0–330, dettaglio x 346–752, tutto entro 752 × 224.
  - `con_claude::test::pagina_scritta_solo_se_cambia`: due `aggiorna_vista_claude_a` di fila senza cambi → 0 righe riscritte e nessun `set_elenco_*`.
  - Cancellati qui: `vista_claude_entra_nel_pannello` e gli altri test di `viste/claude.slint` non elencati sopra (rinati nei nomi `agenti_*`).
- [ ] **Step 2:** `cargo test --workspace agenti_ dettagli_ pagine_dentro pagina_scritta` → FAIL.
- [ ] **Step 3: Implementa** impianto, testata, stato di pagina, scrittura dei modelli e tastiera; la riga col permesso per ora porta al permesso aperto della Coda (`PERMESSO`).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito (via il `allow(dead_code)` del Task 5).
- [ ] **Step 5:** foto dell'istanza di prova con `sessioni_finte --sessioni 6` (righe al lavoro, titoli lunghi) a 100 % e 175 %, confronto con `tc-agenti.png` (colonna, testata, limiti); foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): impianto Elenco + dettaglio, la pagina Agenti ci si sposta`.

### Task 7: Il permesso nel dettaglio della pagina Agenti

**Files:**
- Modify: `ui/impianti/elenco_dettaglio.slint` (con una riga che ha `voce` e `attese[attesa].id == voce` il dettaglio è un `Permesso` con `pulsanti-in-alto: false`), `ui/pannello.slint` (tastiera del dettaglio: `agenti-permesso.tasto(...)`, `entra()`; `changed` sull'id del permesso mostrato → `consenti-compare()`; `lascia` → riga; voce sparita col fuoco addosso → fuoco sulla riga), `src/moduli/pagina_agenti.rs` (la riga col permesso non ha più l'azione `PERMESSO`; `PERMESSO` tolto), `crates/claude-hook/examples/sessioni_finte.rs` (`--permesso`: la sessione 0 manda una `PermissionRequest` finta con `aiutante::evento` in un suo thread — strumento `Bash`, riassunto «npm test -- checkout --watch=false», cartella `progetto-a` — e stampa la decisione), `CHANGELOG.md`
- Test: `test_ui.rs`

**Interfaces:**
- Consumes: `Permesso` (Task 3), `armamenti`/`inerte` (Task 4), righe con `voce`/`attesa` (Task 5–6), `azione-voce`.
- Produces: nessuna interfaccia nuova verso Rust: le decisioni vanno su `azione-voce(voce, pulsante)` (guardia di 600 ms), il riarmo su `consenti-compare`.

- [ ] **Step 1: Test che falliscono**:
  - `agenti_permesso_nel_dettaglio`: riga col permesso scelta → «Chiede il permesso», «$», il comando, la cartella, «Nega»; «Consenti» solo dopo il fondo (testo corto: subito).
  - `agenti_entra_su_nega`: → o Invio sulla riga → fuoco vero su «Nega».
  - `agenti_consenti_solo_con_ctrl_invio_esatto`: i 9 casi di `coda_consenti_solo_con_ctrl_invio_esatto` sul dettaglio; solo Ctrl+Invio → `azione-voce … consenti`.
  - `agenti_invio_su_consenti_non_concede`.
  - `agenti_dettaglio_che_diventa_permesso_riarma` (Review Focus 4): ↓ su una riga col permesso, e la riga col fuoco tolta che lascia il posto a una col permesso → un `consenti-compare` per ciascuno; il fuoco non è mai su «Consenti».
  - `agenti_nessun_pulsante_nasce_sotto_il_clic`: il rettangolo dell'azione della riga scelta non tocca quelli di «Nega»/«Consenti»; nessuna riga dell'elenco li tocca.
  - `permesso_risolto_col_fuoco_sul_dettaglio` (Review Focus 1): fuoco su «Consenti», la voce tolta e la riga che torna «al lavoro» → dopo `update_timers_and_animations()` fuoco sulla riga nell'elenco, Tab seguente non si perde.
  - `agenti_permesso_di_un_altro_non_si_mostra`: `attesa` che punta a una voce con un altro id → dettaglio della riga, nessun «Nega».
- [ ] **Step 2:** `cargo test -p assistente agenti_ permesso_risolto` → FAIL.
- [ ] **Step 3: Implementa.**
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto con `sessioni_finte --sessioni 4 --permesso` (permesso in cima) e con `--prova-avviso bloccante --lungo` (Coda), a 100 % e 175 %; confronto con `tc-agenti.png` e `tc-coda.png`; la decisione resta senza risposta (il thread dell'esempio rinuncia da sé); foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): il permesso nel dettaglio della pagina Agenti`.

### Task 8: Dalla pillola al permesso nella pagina Agenti (decisione di Paolo 5)

**Files:**
- Modify: `crates/assistente/src/ponte.rs` (`DallaPillola::Permesso(i32)` → `DallaPillola::Permesso(String /* id della voce */)`; `apertura_dalla_pillola`; `chiedi_permesso(&self, voce: &str)` accanto a `chiedi_vista`), `src/pannello.rs` (il ramo `Permesso` chiede la vista `agenti` e la voce), `src/ponte/con_claude.rs` (`vista_claude(true)`/`aggiorna_vista_claude_a`: alla prima scrittura dopo l'apertura, se c'è una voce chiesta, `elenco-fuoco` sulla riga con quella `voce` e `invoke_entra_nel_permesso()`; la richiesta vale una volta sola, come `vista_richiesta`), `ui/pannello.slint` (`public function entra-nel-permesso()`: zona contenuto, lato dettaglio, `agenti-permesso.entra()`), `CHANGELOG.md`
- Test: `ponte/con_claude.rs`, `test_ui.rs`

**Interfaces:**
- Consumes: righe con `voce`/`attesa` (Task 5–6), `Permesso.entra()` (Task 3), il dettaglio col permesso (Task 7), `claude_superficie_compare`.
- Produces: `DallaPillola::Permesso(String)`; `Ponte::chiedi_permesso(&self, voce: &str)`; `Pannello.entra-nel-permesso()`. Ripiego: un permesso senza riga di sessione (sessione sparita) o di una fonte senza pagina → la Coda col permesso aperto come oggi (`apri-coda-su` e il suo test restano per questo ramo).

Regole di M3 su questa strada, tutte con un test: fuoco iniziale su «Nega» (`entra`); testo intero e sessione nel dettaglio, «Consenti» solo dopo il fondo; **riarmo di 600 ms all'arrivo** (apertura del pannello = superficie che compare, più il cambio di permesso nel dettaglio) con l'aspetto inerte; **nessun pulsante sotto il punto del clic sulla pillola**: «Nega» e «Consenti» del dettaglio stanno in basso a destra del corpo, fuori dalla fascia della riga alta dove nasce il pannello sotto la pillola.

- [ ] **Step 1: Test che falliscono**:
  - `con_claude::test::dalla_pillola_alla_destinazione` aggiornato: i due casi «permesso» e «permesso senza risposta dalla pillola» → `DallaPillola::Permesso(voce_di(1))` (l'id, non più l'indice); gli altri casi invariati (riposo, attività → `agenti`, voce di Casa/PC → la loro pagina, prova → `coda`, puntino → `coda`).
  - `con_claude::test::dal_permesso_della_pillola_la_riga_scelta`: aperta la vista `agenti` con la voce chiesta → `elenco-fuoco` sulla riga di quella sessione (anche se non è la prima), richiesta consumata (una seconda apertura parte dalla prima riga), un riarmo contato in `armamenti`.
  - `test_ui::dalla_pillola_il_permesso_nella_pagina_agenti`: `entra-nel-permesso()` con la riga del permesso scelta → vista `agenti`, fuoco vero su «Nega», «Consenti» assente prima del fondo con un permesso lungo, Invio non concede, Ctrl+Invio esatto solo dopo il fondo.
  - `test_ui::dalla_pillola_nessun_pulsante_sotto_il_clic`: i rettangoli di «Nega» e «Consenti» del dettaglio stanno sotto y = 44 + 48 (fuori dalla riga alta e dalla testata) e non toccano la zona centrale della riga alta dove sta la pillola.
  - `test_ui::dalla_pillola_il_permesso_si_apre_intero` resta (ramo di ripiego sulla Coda).
- [ ] **Step 2:** `cargo test -p assistente dalla_pillola dal_permesso_della` → FAIL.
- [ ] **Step 3: Implementa.**
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): dalla pillola un permesso si apre nella pagina Agenti`.

### Task 9: Chiusura della fetta

**Files:**
- Modify: `CHANGELOG.md` («Ridisegno F2, chiusura»), `README.md` (pagina Agenti: elenco e dettaglio, permesso nella pagina), `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (riga «F2 fatta» nel §8, con ciò che il piano ha deciso diversamente), `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` (punti chiusi segnati; i nuovi rimandati di F2 in coda)

- [ ] **Step 1: Verifica completa**: `cargo test --workspace` (numero annotato; F1 chiudeva a 983), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release -p assistente`.
- [ ] **Step 2: Misure** con `tools\misura.ps1 -Secondi 60` sull'exe release, istanza di prova, stesse condizioni del Task 1 Step 1: a riposo **RAM privata 7,8–8,0 MB (≤ partenza + 0,2) e CPU ≤ 0,05 %**; Panoramica aperta ferma **≤ 30 MB** (F1: 13,9–14,0) e CPU ~0; pagina Agenti aperta con `sessioni_finte --sessioni 3` ≤ 30 MB e CPU non sopra la partenza del Task 1; **10 aperture e chiusure** sulla pagina Agenti (apertura col secondo avvio, `PostMessage` alla propria finestra, Esc) → RAM 15 s dopo ogni chiusura piatta (F1: 10,2–10,5 MB). Fotogrammi a riposo dai `RedrawRequested` di winit (build usa e getta, mai committata): solo il battito degli occhi; nessun fotogramma oltre i 600 ms dopo un riarmo. Cifre nel CHANGELOG come per F1.
- [ ] **Step 3: Confronto visivo** sull'exe finale: `tc-agenti`, `tc-coda`, `tc-griglia-aspetta` (n1/n2), a 100 % e 175 %, solo la finestra dell'app, cancellate dopo.
- [ ] **Step 4: Revisione dell'intera fetta** su **Opus** (codice dei Task 1–8 da d1c2845 + foto affiancate), poi le correzioni in un'ondata sola. Al revisore si dice che i minor m1–m3 di `task-7-rereview-1.md` (F1) non sono più recuperabili (file perso): ricontrolla la Coda col permesso da capo.
- [ ] **Step 5: Prova dal vivo con Paolo** (una riapertura della sua isola, col suo ok), con gli elementi rimasti da F1 che toccano le superfici di F2: Claude Code vero con un permesso **negato e uno consentito** dalla pagina Agenti (anche arrivandoci col clic sulla pillola), dalla Coda e dalla voce della Panoramica; nella Coda «Consenti» entro 600 ms ignorato (aspetto inerte visibile) e permesso lungo consentito solo dopo il fondo; **doppio clic reale su «Nega» con due permessi** (Panoramica e Agenti); la voce con un solo pulsante delicato (`--prova-avviso`); nomi delle sessioni = titoli delle schede di Claude Code; tastiera e **Narrator con UI Automation vera** su righe, dettaglio, permesso e carte; anello del fuoco nella colonna che scorre e nel testo del permesso a 125/150 %; pagina Agenti a 100 % e 175 %. Fuori da F2 ma nello stesso giro se Paolo vuole: volume, Griglia/Orologio, fascia e avvisi, scala 0,96, luce della carta a 125/150 %, `pc.slint` `to-uppercase()` con Narrator. Esito nel CHANGELOG.
- [ ] **Step 6: Pulizia** di `target\agente` (incrementali e artefatti dei giri) e delle foto rimaste.
- [ ] **Step 7:** commit `docs: chiusura del ridisegno F2 (misure, prova dal vivo)` (con l'ok di Paolo; nessun push senza un suo sì).
