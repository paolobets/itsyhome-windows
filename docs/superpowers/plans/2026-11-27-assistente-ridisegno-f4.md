# Ridisegno F4 · pillola — piano di realizzazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** la pillola di isola nel linguaggio di TwoCode (forma con le orecchie, luce dal basso, occhi sempre presenti, tre altezze), con un arbitro unico che decide cosa mostra (bloccante → attenzione nuova → fatto → attività → riposo), «+N», un allargamento ogni 10 minuti per modulo e «Non disturbare».

**Architecture:** la decisione di cosa mostra la pillola diventa una funzione pura nuova (`src/arbitro.rs`), che sostituisce `attenzione::aspetto_pillola` e il pezzo di `pillola::vista` che sceglie la voce; il ponte le passa voci, attività, fatti e lo stato di «Non disturbare», e scrive in Slint solo ciò che cambia. La pillola Slint (`ui/pillola.slint`) si riscrive sulle forme del guscio di F1 (`ContornoIsola`, `Luce`, `Occhi`): via aura, veli, iride e petali. «Non disturbare» è uno stato del guscio (`src/sistema/non_disturbare.rs`) che unisce schermo intero, «Non disturbare» di Windows (solo se c'è un'API documentata) e un interruttore nel menu.

**Tech Stack:** Rust 2021, Slint `=1.18.1` (renderer Skia software), windows-sys, `muda`/`tray-icon` per il menu del vassoio.

**Spec:** `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (§5.2 luce e toni, §5.3 segni, §5.8 pillola, §5.10 contratto, §7 sicurezza, §8 fette, §11 verifica, §13 esiti dello spike). Bozze: `.superpowers/ux/studio/bozze/screenshot/tc-pill-riposo.png`, `tc-pill-lati.png`, `tc-pill-lavora.png`, `tc-pill-casa.png`, `tc-pill-fatto.png`, `tc-pill-permesso.png`, `tc-pill-focus.png`, `tc-colori-pillole.png`.

## Decisioni di Paolo (27/11/2026)

1. **Non disturbare** scatta da solo (app a schermo intero sul monitor della pillola; «Non disturbare» di Windows) **e** c'è un interruttore manuale nel menu «⋯» (e nel menu del vassoio, che ha le stesse voci).
2. **In Non disturbare la pillola resta a riposo**: piccola, con gli occhi e i lati; non si allarga per attività, fatti o avvisi. Solo un permesso bloccante la apre.
3. **Avvisi**: larga per 20 s, poi il puntino accanto agli occhi; lo stesso modulo non la riallarga prima di 10 minuti.
4. **Lati a riposo**: come oggi, scelti in Impostazioni › Isola (catalogo `lati.rs`), ridisegnati in mono 13 a larghezza ferma.

## Decisioni del coordinatore (da confermare a Paolo nel messaggio finale)

- **D1 · Schermo intero**: oggi la pillola **sparisce** a schermo intero (M1, `sistema::Isteresi`). Resta così (niente sopra un video); a schermo intero passa **solo il bloccante**, che fa ricomparire la pillola larga finché c'è. Con «Non disturbare» di Windows o con l'interruttore la pillola si vede, a riposo (decisione 2). *Costo se sbagliata*: cambiare `pillola_visibile` in un punto.
- **D2 · L'interruttore non si salva**: «Non disturbare» manuale vale fino all'uscita da isola (nessun dato nuovo su disco, spec §7). *Costo*: un campo in `Config` se Paolo lo vuole permanente.
- **D3 · Fatto prima dell'attività**: segue la priorità dei toni di §5.3 (fatto > attivo). Un fatto non visto resta nella pillola finché non lo guardi (pannello aperto o clic sulla pillola), ma una **nuova** attività dello stesso modulo lo sostituisce. *Costo*: invertire due righe dell'arbitro.
- **D4 · «Non disturbare» di Windows** si legge solo con un'API documentata da Microsoft (regola: prima la doc). Se il Task 2 non ne trova una, si fa senza: restano schermo intero e interruttore, e si scrive nel rapporto. Niente chiavi di registro non documentate.
- **D5 · «Consenti» neutro come «Nega»** anche nella pillola (regola M3, decisione di F1), non bianco come nella bozza.

## Global Constraints

- Lingua: tutto in italiano (codice, commenti, testi, commit). Commit con `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` e `Claude-Session: https://claude.ai/code/session_014N636cPyugGmahT4Ly1PyC`.
- Compilare solo da PowerShell, con `--target-dir target\agente` (implementatori) o `target\revisore` (revisori); mai scrivere sorgenti da PowerShell o script.
- Misure della pillola (§5.8): altezze **32 / 36 / 40**, orecchie **10 px**, riposo **116 × 32** (solo occhi bianchi), attività **alta 36 a larghezza ferma per tipo**, **«+N» 32 × 28**, bloccante **540 × 40 ferma**.
- Luce dal basso nel colore del tono (`Tema.luce-tono`, componente `ui/guscio/luce.slint`); si anima **1,2 s solo al cambio di tono**, 0 ms col movimento ridotto; a riposo **nessun fotogramma** oltre al battito degli occhi.
- **Nessun timer nuovo** oltre a: il cronometro dell'attività al secondo **solo se mostrato e visibile** (c'è già, `timer_attivita`), le sveglie una tantum calcolate dall'arbitro (`prossimo_cambio`). Nessuna animazione continua.
- Fatto: occhi **^ ^**, luce **blu** (`petalo-3`, `#60a5fa`) finché non guardi, poi **verde acqua** (`petalo-5`, `#2dd4bf`) che **sfuma in 2,8 s** e torna il riposo.
- Precedenza (nessuna rotazione): bloccante di qualunque modulo (il più vecchio) → attenzione nuova → fatto non visto → attività del primo modulo (a parità la più recente) → riposo. In Non disturbare passa solo il bloccante.
- Regole M3 dei permessi **intoccabili**: nessun test dei permessi si cambia o si toglie; «Nega» e «Consenti» neutri, 10 px fra loro, «Consenti» solo se il testo si legge tutto (altrimenti «Apri»), guardia di 600 ms in Rust; il comando **mai** nella notifica di Windows.
- Simboli come `Path` o SVG, mai glifi; contrasti ≥ 4,5:1 per i testi; nomi per Narrator sulla pillola.
- Soglie (§11, come F3): a riposo RAM privata ≤ partenza + 0,2 MB e CPU ≤ 0,05 %; partenza = exe di `97f75ff` rimisurata nello stesso giro a schermo sbloccato.
- Prove desktop: input solo con `PostMessage`/UIA al **proprio** PID; mai sul volume, sulle radio o sull'isola vera di Paolo (`%LOCALAPPDATA%\isola-m3-prova`). Nessun Invio se il primo piano non è la propria finestra.

## Review Focus

1. **Bloccante a schermo intero (D1)**: con un video a tutto schermo e un permesso nuovo, la pillola deve ricomparire larga e sparire di nuovo quando il permesso è deciso; senza permesso resta nascosta. → test in Task 2 (`bloccante_passa_lo_schermo_intero`).
2. **Due moduli che si alternano negli avvisi**: Casa e PC con un avviso ciascuno a 1 minuto di distanza: la pillola si allarga per entrambi (il limite è **per modulo**), ma un secondo avviso di Casa dopo 3 minuti dà solo il puntino. → test in Task 1 (`dieci_minuti_per_modulo_non_globali`).
3. **Fatto e attività dello stesso modulo**: Claude finisce in una sessione e ne riparte un'altra: la pillola passa all'attività nuova, senza restare bloccata sul fatto (D3). → test in Task 1 (`attivita_nuova_sostituisce_il_fatto`).
4. **Clic sulla pillola mentre cambia contenuto**: un clic arriva quando il bloccante è appena stato deciso altrove: niente decisione su un pulsante nato sotto il puntatore (600 ms, stessa regola del pannello). → test in Task 5 (`pillola_inerte_nei_primi_600_ms`).
5. **Testi lunghi a 100 % e 175 %**: nome di sessione di 80 caratteri e comando lungo nel bloccante 540: il nome si accorcia per primo, «+N» e i due pulsanti non si spostano né si sovrappongono. → test in Task 5 (`bloccante_testi_lunghi_pulsanti_fermi`).

---

## Struttura dei file

| File | Responsabilità |
|---|---|
| `crates/assistente/src/arbitro.rs` (nuovo) | Cosa mostra la pillola: funzione pura sopra voci, attività, fatti, «Non disturbare», tempo |
| `crates/assistente/src/attenzione.rs` | Voci: resta; `aspetto_pillola` e `prossimo_cambio_aspetto` passano all'arbitro (tolti) |
| `crates/assistente/src/pillola.rs` | Da `Mostra` dell'arbitro alle proprietà Slint e al ritaglio della finestra |
| `crates/assistente/src/sistema/non_disturbare.rs` (nuovo) | Stato di «Non disturbare»: manuale + Windows + schermo intero |
| `crates/assistente/src/sistema/menu.rs`, `ui/menu.slint` | Voce a spunta «Non disturbare» |
| `crates/assistente/src/sistema/mod.rs`, `sistema/servizi.rs` | Visibilità della pillola (D1) |
| `crates/assistente/src/finestre.rs` | Costanti della finestra e del ritaglio (540 × 40, 116 × 32, orecchie) |
| `crates/assistente/ui/pillola.slint` | La pillola nuova |
| `crates/assistente/ui/guscio/cornice.slint` | `ContornoIsola` riusato dalla pillola (parametri, nessun cambio di aspetto del pannello) |
| `crates/assistente/src/ponte.rs`, `ponte/con_claude.rs` | Attività e fatti di Agenti verso l'arbitro; scrittura della pillola |
| `crates/assistente/src/test_ui.rs` | Prove della pillola sul disegno vero |

---

### Task 1: l'arbitro della pillola (funzione pura)

**Files:**
- Create: `crates/assistente/src/arbitro.rs` (con `mod test` in fondo)
- Modify: `crates/assistente/src/main.rs` (dichiara `mod arbitro;`)

**Interfaces:**
- Consumes: `attenzione::{Attese, Voce, Livello, Fonte}` (`Attese::ordinate`, `Attese::vista` da rendere `pub(crate)`).
- Produces:
  ```rust
  pub const ALLARGATA_PER: Duration = Duration::from_secs(20);          // spostata da attenzione.rs
  pub const RIALLARGA_DOPO: Duration = Duration::from_secs(10 * 60);
  pub const SFUMA_FATTO: Duration = Duration::from_millis(2800);

  #[derive(Debug, Clone, PartialEq)]
  pub struct Attivita { pub modulo: &'static str, pub id: String, pub nome: String, pub resto: String,
                        pub dal: Option<Instant>, pub iniziata: Instant }
  #[derive(Debug, Clone, PartialEq)]
  pub struct Fatto { pub modulo: &'static str, pub id: String, pub nome: String, pub resto: String,
                     pub durata: Option<Duration>, pub finito: Instant, pub visto: bool }

  #[derive(Debug, Clone, PartialEq)]
  pub enum Mostra {
      Riposo { puntino: Option<Livello> },
      Bloccante { voce_id: String, altri: usize },
      Attenzione { voce_id: String },
      Fatto { id: String, visto_alle: Option<Instant> },   // visto_alle: inizia la sfumatura verde acqua
      Attivita { id: String, altre: Vec<String> },         // altre: i nomi per «+N» e Narrator
  }

  #[derive(Debug, Default)]
  pub struct Arbitro { allargate: HashMap<&'static str, Instant> }   // ultimo allargamento per modulo

  impl Arbitro {
      pub fn scegli(&mut self, attese: &Attese, attivita: &[Attivita], fatti: &[Fatto],
                    non_disturbare: bool, ora: Instant) -> Mostra;
      pub fn prossimo_cambio(&self, attese: &Attese, fatti: &[Fatto], ora: Instant) -> Option<Duration>;
  }
  ```
  Il modulo di una voce è `Fonte::modulo()`.

- [ ] **Step 1: scrivere i test che falliscono** (in `arbitro::test`, tempo sempre dal chiamante):
  - `bloccante_prima_di_tutto`: con un'attenzione nuova, un fatto e un'attività, più due bloccanti creati a `t` e `t+5s` → `Mostra::Bloccante { voce_id: <quello di t>, altri: 1 }`.
  - `attenzione_larga_20_s_poi_puntino`: attenzione di Casa creata a `t` → `Attenzione` a `t+19s`, `Riposo { puntino: Some(Attenzione) }` a `t+20s`.
  - `dieci_minuti_per_modulo_non_globali` (Review Focus 2): Casa a `t` (allarga), PC a `t+60s` → `Attenzione` per PC; una seconda voce di Casa a `t+3min` → `Riposo { puntino: Some(Attenzione) }`; una terza voce di Casa a `t+10min+1s` → `Attenzione`.
  - `attenzione_vista_non_allarga`: `attese.segna_viste()` subito dopo la creazione → `Riposo { puntino: … }`.
  - `fatto_prima_dell_attivita` (D3): fatto non visto di `agenti` e attività di `agenti` iniziata **prima** del fatto → `Fatto`.
  - `attivita_nuova_sostituisce_il_fatto` (Review Focus 3): attività di `agenti` iniziata **dopo** `fatto.finito` → `Attivita`.
  - `fatto_visto_sfuma_e_torna_il_riposo`: `visto: true` con `visto_alle = t` → `Fatto { visto_alle: Some(t) }` fino a `t+2,8s` escluso, poi `Riposo`.
  - `attivita_la_piu_recente_e_piu_n`: tre attività (iniziate a `t`, `t+1`, `t+2`) → `Attivita { id: <t+2>, altre: [<nomi delle altre due, dalla più recente>] }`.
  - `non_disturbare_lascia_solo_il_bloccante`: con ND e attenzione nuova + attività → `Riposo { puntino: Some(Attenzione) }`; con ND e un bloccante → `Bloccante`. Un'attenzione arrivata in ND **non** consuma i 10 minuti del suo modulo.
  - `prossimo_cambio_solo_se_qualcosa_scade`: `Some(20 s − trascorsi)` con un'attenzione larga, `Some(2,8 s − trascorsi)` con un fatto che sfuma, `None` a riposo.
- [ ] **Step 2: eseguire e vederli fallire**
  Run: `cargo test -p assistente --target-dir target\agente arbitro::`
  Expected: FAIL (modulo/tipi inesistenti).
- [ ] **Step 3: implementare `Arbitro::scegli` e `prossimo_cambio`** in `arbitro.rs` nell'ordine delle Decisioni. Un'attenzione «nuova» = livello `Attenzione`, non vista, creata da meno di `ALLARGATA_PER`, **e** il suo modulo non ha allargato negli ultimi `RIALLARGA_DOPO` prima della sua creazione; quando la si mostra la prima volta, `allargate[modulo] = voce.creata`. Il puntino è il livello più urgente rimasto fra le voci (`None` senza voci).
- [ ] **Step 4: eseguire e vederli passare**
  Run: `cargo test -p assistente --target-dir target\agente arbitro::`
  Expected: PASS, 10 test.
- [ ] **Step 5: commit** — `feat(assistente): arbitro della pillola (precedenza, 10 minuti per modulo, Non disturbare)`

### Task 2: «Non disturbare» (stato, menu, schermo intero)

**Files:**
- Create: `crates/assistente/src/sistema/non_disturbare.rs`
- Modify: `crates/assistente/src/sistema/mod.rs` (`pillola_visibile`), `sistema/servizi.rs` (giro dei 5 s), `sistema/menu.rs` (`voci_pannello`, `Comando`), `ui/menu.slint` (voce a spunta)

**Interfaces:**
- Produces:
  ```rust
  pub struct NonDisturbare { manuale: bool, windows: bool, schermo_intero: bool }
  impl NonDisturbare {
      pub fn attivo(&self) -> bool;                 // manuale || windows || schermo_intero
      pub fn imposta_manuale(&mut self, acceso: bool);
      pub fn manuale(&self) -> bool;
      pub fn da_windows(&mut self, acceso: bool);
      pub fn da_schermo_intero(&mut self, acceso: bool);
  }
  pub fn windows_attivo() -> Option<bool>;           // None: nessuna API documentata/lettura fallita
  pub fn pillola_visibile(schermo_intero: bool, pannello_aperto: bool, ritirata: bool, bloccante: bool) -> Visibilita;
  ```
  `Comando::NonDisturbare` nel menu, voce a spunta «Non disturbare» fra «Schermo» e «Ritira l'isola».

- [ ] **Step 1: verificare la documentazione Microsoft (D4)** prima di scrivere `windows_attivo`: leggere su learn.microsoft.com `SHQueryUserNotificationState` (`QUERY_USER_NOTIFICATION_STATE`) e ogni API pubblica per lo stato di «Non disturbare»/Focus di Windows 11. Annotare nel commento di testa del file le pagine lette e la data. Se nessuna API documentata dice lo stato di «Non disturbare», `windows_attivo` restituisce `None` su ogni piattaforma e il rapporto lo dice (D4).
- [ ] **Step 2: test che falliscono** in `non_disturbare::test` e `sistema::test`:
  - `attivo_se_una_delle_tre`: ogni combinazione delle tre fonti (8 casi) → `attivo()` vero se almeno una.
  - `bloccante_passa_lo_schermo_intero` (Review Focus 1): `pillola_visibile(true, false, false, true) == Visibilita::Pillola`, `pillola_visibile(true, false, false, false) == Visibilita::Nascosta`; ritirata con bloccante a schermo intero → `Pillola` (il bloccante non si lascia sulla linea).
  - i test esistenti `nascosta_a_schermo_intero`, `pannello_aperto_pillola_nascosta`, `ritirata_mostra_la_linea` restano veri con `bloccante: false`.
  - `menu_ha_non_disturbare_con_la_spunta`: `voci_pannello(…, non_disturbare: true)` contiene la voce «Non disturbare» con `scelto: true`, una volta sola, fra «Schermo» (se c'è) e «Ritira l'isola».
- [ ] **Step 3: eseguire e vederli fallire** — Run: `cargo test -p assistente --target-dir target\agente non_disturbare sistema::test menu` — Expected: FAIL.
- [ ] **Step 4: implementare** `NonDisturbare`, `windows_attivo` (letto nel giro dei 5 s dei servizi, nessun timer nuovo), `pillola_visibile` col bloccante, la voce di menu in `voci_pannello` (unica fonte: vassoio e «⋯») e `Comando::NonDisturbare` che inverte il manuale. Il ponte riceve `non_disturbare(attivo: bool)` e lo passa all'arbitro (Task 6).
- [ ] **Step 5: eseguire e vederli passare** — stesso comando — Expected: PASS.
- [ ] **Step 6: commit** — `feat(assistente): Non disturbare (manuale, schermo intero, Windows se documentato)`

### Task 3: la pillola nuova a riposo (forma, luce, occhi, lati)

**Files:**
- Modify: `crates/assistente/ui/pillola.slint`, `ui/guscio/cornice.slint` (`ContornoIsola` con proprietà `orecchio`, `raggio-basso`; il pannello passa i suoi valori di oggi), `ui/faccia.slint` (`Faccia.larghezza-pillola: 116px`), `src/finestre.rs` (`LARGHEZZA_PILLOLA = 116.0`, `LARGHEZZA_BLOCCANTE = 540.0`, `ALTEZZA_*` 32/36/40, `ORECCHIO` condiviso, ritaglio che comprende le orecchie), `src/lati.rs` (`larghezza_riposo` con le misure nuove), `src/test_ui.rs`

**Interfaces:**
- Consumes: `ContornoIsola`, `Luce`, `Occhi` di F1.
- Produces (proprietà della `Pillola` che i task 4–5 usano): `in property <TonoModuloUi> tono` (luce dal basso), `in property <StatoPillolaUi> stato` (enum nuovo in `voci.slint`: `riposo`, `attivita`, `attenzione`, `fatto`, `bloccante`), `out property <length> larghezza-pillola`, `altezza-pillola`, `larghezza-visibile`, `altezza-visibile` (già letti da `pillola.rs`).

- [ ] **Step 1: test che falliscono** in `test_ui`:
  - `pillola_riposo_116_per_32`: pillola senza lati → `larghezza-pillola == 116px`, `altezza-pillola == 32px`; con due lati la larghezza segue `lati::larghezza_riposo` (test `misure_dei_lati_slint_e_rust` aggiornato alle misure nuove).
  - `misure_slint_e_rust` aggiornato: 116 × 32, 540 × 40, orecchio 10, finestra = 540 + 2 × orecchio + margine.
  - `pillola_riposo_nera_con_orecchie` (disegno vero, `piattaforma_che_disegna`): pixel neri ai due angoli alti fuori dal corpo per 10 px (le orecchie) e trasparenti oltre; nessun pixel dei vecchi petali (`Tema.petalo-1…6` assenti nel rettangolo della pillola a riposo).
  - `luce_solo_col_tono`: `tono: calmo` → nessun pixel colorato sotto gli occhi; `tono: guarda` dopo `fine_arrivo()` → pixel di `Tema.luce-tono(guarda)` nella metà bassa.
- [ ] **Step 2: eseguire e vederli fallire** — Run: `cargo test -p assistente --target-dir target\agente pillola_ misure_` — Expected: FAIL.
- [ ] **Step 3: riscrivere la pillola a riposo**: `ContornoIsola` nero con orecchie di 10 attaccato al bordo alto, `Luce` dentro col `clip`, `Occhi` bianchi, lati mono 13 con l'ingombro del campione (comportamento di oggi). Via `Velo`, `iride`, anello, `intensita-aura`, `battito`, il `Timer` dell'aura e `in-moto` (l'umore «pensa» resta negli occhi). Restano: rotella del volume e barra del volume, ritirata (linea 300 × 5 com'è), nome per Narrator.
- [ ] **Step 4: eseguire e vederli passare**; poi l'intera suite — Run: `cargo test -p assistente --target-dir target\agente` — Expected: PASS (i test delle misure vecchie aggiornati ai numeri nuovi, nessun test dei permessi toccato).
- [ ] **Step 5: commit** — `feat(assistente): pillola a riposo nel linguaggio di TwoCode (orecchie, luce dal basso)`

### Task 4: attività e fatto nella pillola

**Files:**
- Modify: `ui/pillola.slint`, `ui/voci.slint`, `src/pillola.rs`, `src/claude.rs` (produttori), `src/ponte/con_claude.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `arbitro::{Attivita, Fatto, Mostra}` (Task 1), `stato`/`tono` (Task 3).
- Produces:
  ```rust
  pub fn attivita_di_agenti(sessioni: &Sessioni, ora: Instant) -> Vec<arbitro::Attivita>;   // claude.rs
  pub fn fatti_di_agenti(sessioni: &Sessioni, ora: Instant) -> Vec<arbitro::Fatto>;         // claude.rs
  ```
  Proprietà Slint: `in property <image> icona-modulo`, `in property <string> testo-nome` (grassetto), `in property <string> testo-resto` (grigio), `in property <string> cronometro` (mono, vuoto se nessuno), `in property <string> altre` («+2», vuoto se nessuna), `in property <string> nome-altre` (Narrator: «altre 2 attività: blog, my-shop»).

- [ ] **Step 1: test che falliscono**:
  - `claude::test::attivita_e_fatti_dalle_sessioni`: una sessione al lavoro («my-shop», strumento «Modifica», dal `t`) → `Attivita { modulo: "agenti", nome: "my-shop", resto: "Modifica", dal: Some(t), .. }`; una che ha finito non vista → `Fatto { nome: "blog", resto: "ha finito", visto: false, .. }`.
  - `test_ui::pillola_attivita_alta_36_larghezza_ferma`: stato `attivita` → `altezza-pillola == 36px`; la larghezza non cambia fra `cronometro: "0:09"` e `"12:34"` né fra due nomi diversi; con `altre: "+2"` c'è una capsula 32 × 28 a destra e il nome della pillola per Narrator finisce con «, altre 2 attività: …».
  - `test_ui::pillola_fatto_occhi_ad_arco_luce_blu`: stato `fatto`, `tono: fatto`, non visto → occhi ad arco (forma `arco`) e pixel `#60a5fa` nella luce; visto → pixel `#2dd4bf` all'inizio della sfumatura.
- [ ] **Step 2: eseguire e vederli fallire** — Run: `cargo test -p assistente --target-dir target\agente attivita fatto` — Expected: FAIL.
- [ ] **Step 3: implementare** i produttori in `claude.rs` (dati già in `Sessioni`; nessun testo di prompt), le proprietà e il disegno dei due stati (occhi piccoli a sinistra, icona del modulo, nome in grassetto + resto in grigio, cronometro mono a destra, «+N» 32 × 28), la larghezza ferma per tipo come costante in Slint e in `finestre.rs` (test `misure_slint_e_rust`). Il cronometro resta scritto da `timer_attivita` solo se `stato == attivita` e la pillola è visibile.
- [ ] **Step 4: eseguire e vederli passare**, poi l'intera suite — Expected: PASS.
- [ ] **Step 5: commit** — `feat(assistente): attività e fatto nella pillola nuova`

### Task 5: attenzione e bloccante nella pillola

**Files:**
- Modify: `ui/pillola.slint`, `src/pillola.rs` (`pulsanti_pillola` invariata nella regola), `src/ponte.rs` (guardia dei 600 ms per i clic dalla pillola), `src/test_ui.rs`

**Interfaces:**
- Consumes: `Mostra::{Attenzione, Bloccante}` (Task 1), `Voce`, `DatiPermesso`.
- Produces: proprietà `in property <string> sessione` (grassetto), `in property <string> comando` (mono), `in property <bool> inerte` (aspetto inerte per 600 ms, come `Inerzia` di F2 ma nella finestra della pillola: animazione lineare su una proprietà che non si disegna, nessun timer).

- [ ] **Step 1: test che falliscono** (nessun test dei permessi esistente si cambia; questi si aggiungono):
  - `pillola_attenzione_icona_e_fatto`: voce di Casa «Garage aperto» / «da 40 min» → stato `attenzione`, alta 36, icona di Casa, nome in grassetto e resto in grigio, luce del tono `guarda`.
  - `bloccante_540_per_40_fermo`: permesso → `larghezza-pillola == 540px`, `altezza-pillola == 40px`, nessuna animazione dopo `fine_arrivo()` (due foto a 1 s di distanza identiche), «Nega» e «Consenti» neutri (stesso colore di fondo, letto dal disegno come in `consenti_disegnato_neutro_come_nega`).
  - `bloccante_testi_lunghi_pulsanti_fermi` (Review Focus 5): sessione di 80 caratteri e comando di 200 → i rettangoli di «+2», «Nega» e «Consenti» identici a quelli con testi corti; il nome si accorcia prima del comando; testo tagliato → secondo pulsante «Apri».
  - `pillola_inerte_nei_primi_600_ms` (Review Focus 4): un clic su «Nega» della pillola entro 600 ms dalla comparsa del bloccante non produce `azione`; a 600 ms sì (stessa regola del pannello: puntatore mosso sopra il pulsante dopo la comparsa).
- [ ] **Step 2: eseguire e vederli fallire** — Run: `cargo test -p assistente --target-dir target\agente pillola_ bloccante_` — Expected: FAIL.
- [ ] **Step 3: implementare** i due stati sul disegno delle bozze `tc-pill-casa.png` e `tc-pill-permesso.png` (D5: pulsanti neutri), con occhi «sorpresi» (aperti, più alti di 2 px) nel bloccante, e l'aspetto inerte. La decisione resta in Rust (`azione_voce` con la guardia di oggi).
- [ ] **Step 4: eseguire e vederli passare**, poi l'intera suite — Expected: PASS, tutti i test dei permessi di M3 invariati e verdi.
- [ ] **Step 5: commit** — `feat(assistente): attenzione e bloccante nella pillola nuova`

### Task 6: il ponte usa l'arbitro (cablaggio, clic, ritirata)

**Files:**
- Modify: `src/ponte.rs`, `src/ponte/con_claude.rs`, `src/pillola.rs` (`vista` → `da_mostra(m: &Mostra, attese, attivita, fatti) -> VistaPillola`), `src/attenzione.rs` (via `aspetto_pillola`, `prossimo_cambio_aspetto`, `ALLARGATA_PER`), `src/sistema/servizi.rs`

**Interfaces:**
- Consumes: tutto quanto sopra.
- Produces: `Ponte::non_disturbare(&self, attivo: bool)`; `Ponte::apertura_dalla_pillola` apre il pannello sul **modulo dell'elemento mostrato** (bloccante → Agenti con la sessione, come in F2; attenzione → la voce nella Coda; fatto e attività → la pagina del modulo; riposo → Panoramica). Aprire il pannello segna visti i fatti mostrati (`Fatto.visto`).

- [ ] **Step 1: test che falliscono** (in `ponte::test` / `ponte::con_claude::test`):
  - `pillola_segue_l_arbitro`: voce di Casa in attenzione → pillola `attenzione`; dopo 20 s (con `prossimo_cambio`) → riposo col puntino; nessuna sveglia a riposo.
  - `non_disturbare_ferma_l_attivita`: con un'attività di Claude e `non_disturbare(true)` → pillola a riposo; un permesso → bloccante.
  - `clic_sulla_pillola_apre_il_modulo_mostrato`: per ognuno dei cinque `Mostra` la vista aperta è quella delle Interfaces.
  - `linea_della_ritirata_invariata`: colore dal livello più alto e riempimento dall'avanzamento come oggi (`riempimento_linea`).
- [ ] **Step 2: eseguire e vederli fallire** — Run: `cargo test -p assistente --target-dir target\agente ponte::` — Expected: FAIL.
- [ ] **Step 3: implementare** il cablaggio: un solo punto che chiama `Arbitro::scegli` quando cambiano voci, sessioni, ND o scade una sveglia; scrive in Slint solo ciò che cambia (`scrivi_se_cambia`); la sveglia è un `Timer` SingleShot già esistente riusato con `prossimo_cambio`.
- [ ] **Step 4: eseguire la suite intera e clippy**
  Run: `cargo test --workspace --target-dir target\agente` e `cargo clippy --workspace --all-targets --target-dir target\agente -- -D warnings`
  Expected: tutti verdi, clippy pulito.
- [ ] **Step 5: commit** — `feat(assistente): la pillola segue l'arbitro`

### Task 7: misure, fotogrammi, foto e chiusura

**Files:**
- Modify: `CHANGELOG.md`, `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (§8 «F4 fatta»), `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` («Dopo F4»)

- [ ] **Step 1: verifiche** — `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release -p assistente` (target `agente`). Expected: verdi.
- [ ] **Step 2: misure a schermo sbloccato** (controllare prima e dopo con lo script di sblocco): partenza = exe di `97f75ff` nello stesso giro, due prove; riposo RAM ≤ partenza + 0,2 MB, CPU ≤ 0,05 %; pillola in attività (cronometro) CPU non sopra la partenza + 0,01; bloccante fermo 30 s: solo il battito.
- [ ] **Step 3: fotogrammi** (build usa e getta mai committata): riposo = solo battito; cambio di tono = luce per 1,2 s poi nulla; fatto visto = 2,8 s di sfumatura poi nulla; attività = 1 fotogramma al secondo per il cronometro.
- [ ] **Step 4: foto** della sola finestra di prova (`PrintWindow` col contesto DPI per monitor) a 100 % e 175 %, confrontate con le bozze `tc-pill-*.png`; differenze nel CHANGELOG.
- [ ] **Step 5: commit locale** — `docs: misure di chiusura del ridisegno F4`
- [ ] **Step 6 (coordinatore con Paolo): prova dal vivo** sull'isola di Paolo dopo che l'ha chiusa: riposo coi lati, attività di Claude vera, fatto, avviso di prova (`--prova-avviso attenzione`) due volte in 10 minuti, permesso vero (Nega), Non disturbare dal menu e a schermo intero con un permesso; poi chiusura, push con il suo ok, CI sui 3 sistemi.
