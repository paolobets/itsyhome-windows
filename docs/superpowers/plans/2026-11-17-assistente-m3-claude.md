# Assistente "isola" — M3 Claude (Claude Code + API) — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** l'assistente dice da solo come stanno le sessioni di Claude Code (lavora / ti aspetta / fatto / errore), lascia risolvere un permesso dalla pillola senza togliere il terminale, mostra i limiti dell'abbonamento e la spesa delle API Claude in sola lettura, in modo sicuro e senza appesantire il PC.

**Architecture:** due crate nuovi senza interfaccia: `claude-hook` (ascoltatore HTTP blindato su 127.0.0.1, eventi, stato delle sessioni, barra di stato, unione sicura in `settings.json`) e `api-claude` (client HTTPS con un elenco chiuso di letture `GET`, modelli e riepiloghi). L'assistente li collega: logica pura in `src/claude.rs`, colla col `Ponte` in `src/ponte/con_claude.rs` (modulo figlio: vede i campi privati senza gonfiare `ponte.rs`), parti di sistema Windows in `src/sistema/` (variabile d'ambiente utente, terminale da portare davanti). I thread di lavoro (ascolto, connessioni, letture API) consegnano all'interfaccia con `slint::invoke_from_event_loop`, sempre **dopo** aver risposto a Claude Code.

**Tech Stack:** Rust 2021, Slint `=1.18.1`, `httparse 1.10` (già nel `Cargo.lock`), `serde_json` (con `raw_value` e `preserve_order`), `getrandom 0.3` (già nel lock), `ureq 3` con rustls/ring (nuovo, da verificare al Task 8), `windows-sys 0.52`, `windows-registry 0.6`, `keyring 3` (via `casa_core::segreti::Segreti`), chrono.

**Spec:** `docs/superpowers/specs/2026-11-17-assistente-m3-claude-design.md` (vincolante), appoggiata a `docs/superpowers/specs/2026-11-13-assistente-isola-design.md` (§3bis livelli e «In attesa», §4 leggerezza, §6 sicurezza, §9 misure). Stile e convenzioni: `docs/superpowers/plans/2026-11-15-assistente-m2-funzioni-twocode.md`.

---

## Fatti verificati sulla documentazione (17/11, prima del piano)

Fonti: `code.claude.com/docs/en/hooks`, `/statusline`, `/settings`; `platform.claude.com/docs/en/manage-claude/usage-cost-analytics-api`, `/admin-api`, `/admin-api-keys`, `/rate-limits-api`, `api/organization/{api_keys,workspaces,users}/list`. Ogni implementatore che usa un campo lo rilegge sulla pagina e cita la pagina nel commento del modulo.

- **Hook HTTP**: `{"type":"http","url","headers","allowedEnvVars","timeout","onFailure"}`; corpo = JSON dell'evento in `POST` con `Content-Type: application/json`; `$VAR`/`${VAR}` nelle intestazioni solo se in `allowedEnvVars` (altrimenti stringa vuota). Timeout predefinito 600 s (30 s su `UserPromptSubmit`). Non 2xx, connessione rifiutata o timeout = errore **non bloccante**, l'azione prosegue (salvo `onFailure:"block"`, mai usato). Risposta 2xx vuota = successo; 2xx con JSON = stesso schema degli hook a comando.
- **`SessionStart` NON accetta hook `http`** (solo `command` e `mcp_tool`). `UserPromptSubmit`, `PreToolUse`, `Stop`, `PermissionRequest`, `Notification`, `StopFailure`, `SessionEnd` sì. → **scostamento dalla spec §6.1**: non ci si iscrive a `SessionStart`; una sessione compare al primo evento (di solito il primo prompt).
- Campi comuni: `session_id`, `cwd`, `hook_event_name`, `transcript_path`, `permission_mode`. `PreToolUse`: `tool_name`, `tool_input`. `PermissionRequest`: `tool_name`, `tool_input` (senza `tool_use_id`), `permission_suggestions`. `Notification`: `message`, `title?`, `notification_type` (`permission_prompt`, `idle_prompt`, `agent_needs_input`, `elicitation_dialog`, …). `Stop`: `last_assistant_message` ecc. (non letti). `StopFailure`: `error` (`rate_limit`, `authentication_failed`, `billing_error`, `server_error`, `max_output_tokens`, `unknown`, …), `error_details?`. `SessionEnd`: `reason` (`clear`, `resume`, `logout`, `prompt_input_exit`, `other`); budget 1,5 s alzato dal `timeout` più alto fino a 60 s.
- Risposta a `PermissionRequest`: `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}` o `"deny"` (con `message` facoltativo). Se il terminale mostri comunque la sua domanda mentre l'hook aspetta **non è scritto**: lo decide lo spike (Task 1).
- Le modifiche a `settings.json` (hook compresi) si applicano alle sessioni aperte senza riavvio. Il file è `%USERPROFILE%\.claude\settings.json`, o `$CLAUDE_CONFIG_DIR\settings.json` se la variabile c'è. Esistono le chiavi `allowedHttpHookUrls` e `httpHookAllowedEnvVars` (liste ammesse): se presenti, le nostre voci devono rientrarci (Task 7). `claude --settings <file>` applica un file per una sola sessione senza toccare gli altri (usato da spike e prove).
- **Barra di stato**: `{"statusLine":{"type":"command","command":…,"padding"?,"refreshInterval"?}}`; JSON su stdin; aggiornamenti per eventi con 300 ms di rimbalzo; uno script ancora in corso viene annullato. Campi: `session_id`, `model.display_name`, `context_window.used_percentage` (può essere `null`), `rate_limits.five_hour|seven_day.used_percentage` (0..100) e `.resets_at` (**secondi epoch Unix**). `rate_limits` c'è solo per abbonati Pro/Max/Ultra e dopo la prima risposta; ogni finestra può mancare e sparisce dopo `resets_at`. Su Windows il comando gira in **Git Bash se installato, altrimenti in PowerShell**: percorsi con `/`.
- **Usage and Cost Analytics API (beta)**: `GET /v1/organizations/analytics/usage` (token e **`requests`**: il numero di chiamate c'è) e `GET /v1/organizations/analytics/cost` (`cost.amount`, `list_amount`, `currency:"USD"`, `requests`; importi **stringhe in centesimi con sei decimali**: `"41280.000000"` = 412,80 $). Intestazioni `x-api-key`, `anthropic-version: 2026-10-21`, `anthropic-beta: analytics-usage-cost-2026-08-24`. Parametri `starting_at`, `ending_at`, `time_bucket_width` (`1m|1h|1d`, con `ending_at` obbligatorio), `group_by[]` (`model`, `api_key_id`, `workspace_id`, …), `order_by`, `limit`, `page`; risposta `data[]` (`starting_at`, `ending_at`, `group`, `usage`|`cost`), `has_more`, `next_page`, `data_refreshed_at` (`null` con `data` vuoto se nulla è pronto). Al massimo 31 giorni per richiesta, dal 1/1/2026; ritardo tipico 4 h, fino a 24 h; valori rivisti fino a ~7 giorni dopo fine mese.
- **Admin API (sola lettura usata)**: `GET /v1/organizations/me`, `/v1/organizations/api_keys` (campi `name`, `status` `active|inactive|archived|expired`, `created_at`, `expires_at`, `partial_key_hint`, `scope`; **nessun «ultimo uso»**), `/v1/organizations/workspaces` (`include_default`, `include_archived`; `name`, `created_at`, `archived_at`), `/v1/organizations/users` (`name`, `email`, `role`, `added_at`), `/v1/organizations/rate_limits` (`group.display_name`, `limits[{type,value}]`). Paginazione `limit` (≤ 1000), `after_id`, `has_more`, `last_id`.
- **Chiave da amministratore Console** (`sk-ant-admin01-…`): **nessuna scelta di permessi, accesso pieno** (verificato: «None to select»), scadenza scelta alla creazione. Le chiavi con ambiti (`read:analytics`) esistono solo per Claude Enterprise. → la difesa è l'elenco chiuso del client; la guida dice di creare una chiave con scadenza.

## Decisioni di Paolo (il piano si esegue coi default)

- **D1 — DECISIONE DI PAOLO (dopo lo spike): permessi nella pillola.** Default: **Nega / Consenti nella pillola** se lo spike conferma i punti P1, P2, P5 del Task 1. Se no: voce bloccante con il solo pulsante «Apri il terminale» e l'ascoltatore risponde subito `200 {}` anche a `PermissionRequest` (spec §6.3, ramo B). Il Task 5 è scritto per entrambi i rami (costante `PERMESSI_RISOLVIBILI`).
- **D2 — DECISIONE DI PAOLO: durata nella pillola media.** Default: «Claude · Modifica · 3:12» con un timer da **1 s acceso solo mentre almeno una sessione lavora e la pillola è visibile e sveglia** (la spec §7 non vuole timer nuovi a riposo: a riposo non c'è). Alternativa: solo minuti («3 min»), timer al minuto.
- **D3 — DECISIONE DI PAOLO: «ultimo uso» delle chiavi.** L'API non lo dà. Default: nella scheda Chiavi la colonna «Chiamate del mese» (analytics `usage` con `group_by[]=api_key_id`). Alternativa: nessuna colonna.
- **D4 — DECISIONE DI PAOLO: mese della spesa.** Default: **mese UTC** (i secchi dell'API sono UTC), detto nell'etichetta («novembre, UTC»).
- **D5 — DECISIONE DI PAOLO: barra di stato con percorso con spazi.** Default: percorso corto 8.3 (`GetShortPathNameW`) che funziona sia in Git Bash sia in PowerShell senza virgolette; se Windows non lo dà, la barra non si installa e Impostazioni › Claude lo dice. Motivo: una riga con virgolette è valida in Bash ma non in PowerShell.

Scelte mie (nel CHANGELOG): porta scelta dal sistema al primo «Collega» (`bind 127.0.0.1:0`) e poi fissa in configurazione; terminale trovato dalla porta del mittente → PID (`GetExtendedTcpTable`) → catena dei genitori; `serde_json` con `preserve_order` nel workspace (l'unione non riordina il `settings.json` di Paolo); ascoltatore che risponde «nessuna decisione» a **115 s**, prima dei 120 s dell'hook.

---

## Global Constraints

- Ramo `v3-assistente`; `main` non si tocca. Commit locali su `v3-assistente` approvati da Paolo; push del ramo per la CI approvato («si carica»); niente tag né `main` senza ok.
- Build e test **solo da PowerShell** (in Git Bash il `link` di Git copre quello di MSVC), con `--target-dir` separato per agente: `cargo test --workspace --target-dir target\agente-<nome>`. **Mai scrivere sorgenti da PowerShell 5.1** (codifica): solo con gli strumenti di modifica dei file.
- Rust **1.94 in locale, 1.100 in CI**: niente API deprecate in 1.100 (es. `AtomicUsize::fetch_update`: usare `compare_exchange` in un ciclo o `fetch_add` con controllo), niente API assenti in 1.94.
- CI su windows/macos/ubuntu: `cargo test --workspace` e `cargo clippy --workspace --all-targets -- -D warnings` verdi. Codice solo Windows dietro `#[cfg(windows)]`; ciò che resta usato solo nei test o solo su Windows con `#[cfg_attr(not(windows), allow(dead_code))]`.
- **Anti-obiettivi della spec §2 come regole**: Claude Code mai bloccato o rallentato dall'assistente (ascoltatore che risponde prima di toccare l'interfaccia, mai `onFailure`); nessun permesso senza gesto esplicito; nessun «Consenti sempre».
- **Segreti**: chiave da amministratore e codice degli hook solo nel portachiavi (`Segreti`, id `isola:claude-admin` e `isola:claude-hook`: `impostazioni::nuovo_id` non produce mai `:`); il codice degli hook anche nella variabile utente `ISOLA_HOOK_TOKEN` (HKCU\Environment), **mai** in `settings.json`, log, messaggi d'errore, test, fixture o chat. La chiave viaggia solo verso `https://api.anthropic.com`.
- **Contenuti delle sessioni**: si tengono solo `session_id`, nome finale della cartella, stato, `tool_name`, riassunto del permesso in attesa (in memoria, finché c'è). Mai `prompt`, `transcript_path`, `tool_input` completi fuori dal permesso, `last_assistant_message`. Nessuna iscrizione a `PostToolUse`. `diagnosi.log` solo contatori.
- **Test**: mai il vero `settings.json`, la vera chiave, il vero portachiavi o HKCU: `settings.json` finto in `tempfile::TempDir`, `MemoriaSegreti`, `AmbienteFinto`, server API finto su 127.0.0.1. Nessun dato personale in test, fixture, log, foto (nomi di cartella finti, `membro@example.com`, id `apikey_finta_1`).
- **Prove dal vivo solo con Paolo**: installazione nel vero `settings.json` (con anteprima), chiave vera, permessi veri. Prima della prova con Paolo si usa `claude --settings <file temporaneo>` e la modalità di prova (`ITSYHOME_ASSISTENTE_DIR`), in cui l'assistente scrive solo in `<cartella di prova>\claude-finto\settings.json` e nella chiave di registro `HKCU\Software\ItsyHomeProva\Environment` (come `sistema::autoavvio`).
- **Leggerezza** (spec generale §4, spec M3 §7): a riposo con collegamento attivo e nessuna sessione, CPU e RAM come M2 (misura M2 annotata nel CHANGELOG); ascoltatore = un thread fermo su `accept`; unici timer nuovi: lettura API ogni 3 h (solo con chiave), durata della pillola media (D2, solo durante il lavoro), sveglia singola per le sessioni silenziose.
- Testi ≥ 12 px, contrasto ≥ 4,5:1, colori solo come token in `ui/tema.slint`, ruoli e nomi per Narrator, tastiera completa, movimento ridotto (come M2).
- Tutto in italiano (interfaccia, commenti, commit, CHANGELOG). Ogni task aggiorna `CHANGELOG.md` (Unreleased v3) nel proprio commit e annota in testa ai moduli le pagine di documentazione usate.

## Review Focus

1. **Assistente chiuso, piantato o lento mentre Claude Code lavora** → Claude Code non aspetta mai: connessione rifiutata subito, 503 immediato oltre 16 connessioni, `200` scritto prima di consegnare all'interfaccia, barra di stato stampata in < 50 ms anche senza assistente (Task 2 `risposta_prima_del_gestore`, `diciassettesima_connessione_503`; Task 6 `assistente_spento_stampa_lo_stesso`).
2. **`settings.json` di Paolo con contenuto imprevisto** (commenti o virgola finale, BOM, chiavi sconosciute, hook altrui sullo stesso evento, barra di stato altrui, file in sola lettura) → nulla si perde, nulla si riordina, se illeggibile non si scrive e lo si dice (Task 7 `json_non_valido_non_si_scrive`, `unisci_conserva_tutto`, `togli_ridà_l_originale`).
3. **Eseguibile spostato, aggiornato o in un percorso con spazi; Git Bash o PowerShell** → la barra funziona o non si installa dicendolo; all'avvio l'assistente propone «Aggiorna» invece di scrivere da solo (Task 7 `comando_barra_con_spazi_usa_il_percorso_corto`, `percorso_cambiato_rilevato`).
4. **Dati API vuoti, parziali o in ritardo** (`data_refreshed_at` nullo, ieri non ancora completo, una scheda 403, 429) → «nessun dato» e «aggiornato a …», mai «0 $» né falsa «spesa insolita»; le altre schede restano (Task 8 `risultato_vuoto_non_e_zero`, `spesa_insolita_solo_con_dati_completi`, `una_scheda_403_non_ferma_le_altre`).
5. **Eventi fuori ordine o anomali** (primo evento `PermissionRequest` o `Stop`, `SessionEnd` doppio, `session_id` strano o lunghissimo, più di 16 sessioni, permesso di una sessione già chiusa) → nessun panico, tabella limitata, voci coerenti (Task 3 `eventi_fuori_ordine_e_sessioni_limitate`; Task 5 `permesso_sparisce_se_la_sessione_va_avanti`).

---

## Struttura dei file

```
Cargo.toml                                   + members "crates/claude-hook", "crates/api-claude"
crates/claude-hook/Cargo.toml                NUOVO: serde, serde_json (raw_value, preserve_order), httparse, getrandom 0.3
crates/claude-hook/src/lib.rs                NUOVO: moduli e costanti dei percorsi (/isola/v1/evento, /isola/v1/barra)
crates/claude-hook/src/codice.rs             NUOVO: CodiceHook (256 bit, confronto a tempo costante)
crates/claude-hook/src/ascoltatore.rs        NUOVO: TcpListener 127.0.0.1, regole di sicurezza, connessioni, attese
crates/claude-hook/src/evento.rs             NUOVO: EventoHook dai campi strettamente necessari
crates/claude-hook/src/sessioni.rs           NUOVO: stato delle sessioni, puro
crates/claude-hook/src/barra.rs              NUOVO: dati e riga della barra di stato, invio all'ascoltatore
crates/claude-hook/src/installa.rs           NUOVO: anteprima, unione, rimozione, scrittura sicura di settings.json
crates/claude-hook/examples/prova_permesso.rs NUOVO: spike §6.3 (Task 1)
crates/claude-hook/examples/sessioni_finte.rs NUOVO: tre sessioni finte per prove e misure (Task 4)
crates/api-claude/Cargo.toml                 NUOVO: serde, serde_json, ureq 3 (rustls), chrono
crates/api-claude/src/{lib,chiave,letture,client,modello,riepilogo}.rs NUOVO
crates/api-claude/tests/fixture/*.json       NUOVO: risposte finte, nessun dato vero
crates/api-claude/examples/finta_api.rs      NUOVO: server finto per le prove dal vivo dell'interfaccia
crates/casa-core/src/config.rs               + ConfigClaude
crates/casa-core/src/diagnosi.rs             + contatori claude_*, api_*
crates/gestione-protocollo/src/lib.rs        + Messaggio::ApriSchede, Scheda
crates/gestione/src/main.rs, ui/schede.slint + schede senza motore web
crates/assistente/src/claude.rs              NUOVO: voci, testi, pillola media, schede (puro)
crates/assistente/src/ponte/con_claude.rs    NUOVO: Ponte ↔ ascoltatore, permessi, barra, API, avvisi
crates/assistente/src/sistema/ambiente_utente.rs NUOVO: variabile ISOLA_HOOK_TOKEN (HKCU, finta nei test)
crates/assistente/src/sistema/terminale.rs   NUOVO: PID dalla porta, catena dei genitori, porta davanti
crates/assistente/src/{attenzione,pillola,registro,lati,finestre,finestra_impostazioni,main}.rs  + Fonte::Claude, attività, dock, quadranti
crates/assistente/ui/viste/claude.slint      NUOVO: vista Claude 800×300
crates/assistente/ui/{pillola,pannello,impostazioni,tema,icone}.slint + attività, tinta occhi, vista, sezione «Claude»
crates/assistente/assets/icone/claude.svg    NUOVO: asterisco a 8 raggi stile SF Symbols (non il logo di Anthropic)
```

---

### Task 1: Spike del permesso (§6.3) da provare con Paolo

**Files:**
- Create: `crates/claude-hook/Cargo.toml`, `crates/claude-hook/src/lib.rs` (solo le costanti dei percorsi), `crates/claude-hook/examples/prova_permesso.rs`
- Modify: `Cargo.toml` (membro), `docs/superpowers/specs/2026-11-17-assistente-m3-claude-design.md` (nuova sezione «Esito dello spike» dopo la prova), `CHANGELOG.md`

**Interfaces:**
- Produces: `pub const PERCORSO_EVENTO: &str = "/isola/v1/evento"; pub const PERCORSO_BARRA: &str = "/isola/v1/barra";` in `claude_hook`. L'esempio non è codice di prodotto: nessun altro task lo usa.

L'esempio (`cargo run -p claude-hook --example prova_permesso -- [--porta N] [--cartella <dir>]`, cartella predefinita `%TEMP%\isola-spike`):
- ascolta solo su `127.0.0.1` (porta data o scelta dal sistema), genera un codice usa-e-getta e scrive `<cartella>\impostazioni-prova.json` con hook `http` per `PermissionRequest` (timeout 120), `PreToolUse`, `Notification`, `Stop` (timeout 5), `Authorization: Bearer $ISOLA_HOOK_TOKEN`, `allowedEnvVars: ["ISOLA_HOOK_TOKEN"]`; stampa i due comandi per Paolo: `$env:ISOLA_HOOK_TOKEN = '<codice>'` e `claude --permission-mode default --settings <cartella>/impostazioni-prova.json` (da lanciare in una cartella già fidata). **Non legge né scrive `~/.claude/settings.json`**.
- per ogni richiesta stampa: ora, `hook_event_name`, `tool_name`, nomi delle intestazioni (non i valori, salvo `Host` e `Content-Type`), presenza di `Origin`, porta del mittente e (Windows) PID del mittente con la catena dei genitori (solo nomi degli eseguibili). Mai `tool_input`, `prompt`, `cwd`.
- su `PermissionRequest` tiene aperta la connessione e chiede in console: `c` consenti, `n` nega, `a` aspetta senza rispondere, `x` chiude senza decisione; mentre aspetta controlla ogni 250 ms (`peek` non bloccante) se il mittente ha chiuso e stampa «connessione chiusa dal terminale dopo N s».

- [ ] **Step 1:** leggere su `code.claude.com/docs/en/hooks` le sezioni «HTTP hook fields», «PermissionRequest» e «PermissionRequest decision control»; annotarle in testa all'esempio.
- [ ] **Step 2:** scrivi l'esempio; `cargo build -p claude-hook --examples` e `cargo clippy -p claude-hook --all-targets -- -D warnings` → OK.
- [ ] **Step 3: prova con Paolo** (lui lancia `claude`, io guardo solo la console dell'esempio). Domande da chiudere, una riga d'esito ciascuna:
  - **P1** con l'hook in attesa il terminale mostra comunque la sua domanda di permesso?
  - **P2** se Paolo risponde nel terminale, la connessione dell'hook si chiude? Dopo quanto?
  - **P3** con `c` lo strumento parte e la domanda nel terminale sparisce?
  - **P4** con `n` lo strumento è negato e Claude lo dice?
  - **P5** con `a` per 120 s, allo scadere il terminale resta usabile e la domanda vale ancora?
  - **P6** con l'esempio spento: Claude Code prosegue senza ritardi visibili?
  - **P7** il PID del mittente porta alla finestra del terminale giusto (Windows Terminal e PowerShell da solo)?
  - **P8** intestazioni: nessun `Origin`, `Host` = `127.0.0.1:<porta>`, `Content-Type: application/json`; senza la variabile impostata l'`Authorization` arriva vuota.
- [ ] **Step 4:** scrivi gli esiti nella spec («Esito dello spike, data»), con la decisione D1 di Paolo. Se P1, P2 o P5 falliscono → ramo B del Task 5.
- [ ] **Step 5:** commit `chore(claude-hook): spike del permesso con hook HTTP` (con l'ok di Paolo).

### Task 2: Ascoltatore blindato

**Files:**
- Create: `crates/claude-hook/src/codice.rs`, `crates/claude-hook/src/ascoltatore.rs`
- Modify: `crates/claude-hook/src/lib.rs`, `crates/claude-hook/Cargo.toml`, `crates/casa-core/src/diagnosi.rs` (contatori `claude_eventi`, `claude_rifiutate`, `claude_permessi`, `claude_barre`), `CHANGELOG.md`
- Test: in `ascoltatore.rs` e `codice.rs` (client `TcpStream` veri su 127.0.0.1, porta 0)

**Interfaces:**
- Produces:
  - `pub struct CodiceHook([u8; 32])` (niente `Debug` derivato: `Debug` scrive `CodiceHook(…)`); `fn nuovo() -> Result<CodiceHook, String>` (`getrandom::fill`); `fn da_testo(s: &str) -> Option<CodiceHook>` (64 cifre esadecimali); `fn come_testo(&self) -> String`; `fn verifica(&self, autorizzazione: &str) -> bool`: schema `Bearer` senza distinzione di maiuscole, poi confronto **a tempo costante** sui 32 byte (OR degli XOR, nessuna uscita anticipata).
  - `pub enum Percorso { Evento, Barra }`; `pub struct Arrivo { pub percorso: Percorso, pub corpo: Vec<u8>, pub porta_mittente: u16 }`.
  - `pub enum Decisione { Consenti, Nega, Nessuna }`; `pub struct AttesaPermesso { pub id: u64, pub decisione: std::sync::mpsc::Receiver<Decisione> }`.
  - `pub trait Gestore: Send + Sync + 'static { fn evento(&self, a: Arrivo); fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso>; fn abbandonato(&self, id: u64); fn barra(&self, a: Arrivo); }` — `permesso` restituisce `None` per rispondere subito (ramo B di D1).
  - `pub struct ConfigAscolto { pub porta: u16, pub codice: CodiceHook, pub attesa_massima: Duration }` con `pub const ATTESA_MASSIMA: Duration = Duration::from_secs(115)`.
  - `pub enum ErroreAvvio { PortaOccupata, Altro(String) }`; `pub fn avvia(c: ConfigAscolto, g: Arc<dyn Gestore>) -> Result<Ascoltatore, ErroreAvvio>`; `Ascoltatore::porta(&self) -> u16`; `Drop` ferma il thread `claude-ascolto` (bandiera + connessione a sé stesso per svegliare `accept`) e lo raccoglie.
  - Costanti: `CORPO_MASSIMO = 256 * 1024`, `SCARTO_MASSIMO = 1024 * 1024`, `INTESTAZIONI_MASSIME = 8 * 1024` (e al più 32 righe), `TEMPO_LETTURA = 2 s`, `CONNESSIONI_MASSIME = 16`.
- Regole, nell'ordine, **prima di leggere il corpo**: oltre 16 connessioni aperte → `503` subito senza leggere; intestazioni oltre 8 KB o 32 righe o lette oltre 2 s → `431`/chiusura; metodo ≠ `POST` (anche `OPTIONS`) → `405` senza intestazioni CORS; qualunque `Origin` → `403`; `Host` ≠ esattamente `127.0.0.1:<porta>` → `403`; `Content-Type` ≠ `application/json` (ammesso `; charset=utf-8`) → `415`; `Authorization` non valida → `401`; `Transfer-Encoding` presente o `Content-Length` assente → `411`; oltre 256 KB → letto e scartato fino a 1 MiB entro i 2 s, poi `413`; percorso sconosciuto → `404`. Ogni rifiuto conta `claude_rifiutate` e non chiama il gestore.
- Percorso `Evento`: si legge solo `hook_event_name` (struct con quel solo campo, `serde_json::from_slice`); JSON non valido → `400`. Se ≠ `PermissionRequest` → si scrive `200 {}` e **poi** `gestore.evento`. Se `PermissionRequest` → `gestore.permesso`; con `Some(attesa)` si aspetta con `recv_timeout(250 ms)` in ciclo, controllando a ogni giro se il mittente ha chiuso (`peek` non bloccante: `Ok(0)` o errore = chiuso → `abbandonato(id)`); `Consenti`/`Nega` → `200` con il JSON della decisione (`"deny"` con `message: "Negato dall'utente dall'assistente ItsyHome."`); `Nessuna` o `attesa_massima` → `200 {}` e `abbandonato(id)`. Percorso `Barra`: `200 {}` e poi `gestore.barra`.

- [ ] **Step 1: test (falliscono)**

```rust
// codice.rs
#[test] fn codice_andata_e_ritorno() // da_testo(c.come_testo()) == Some(c); 63 o 65 cifre, "zz…" → None
#[test] fn verifica_codice() // "Bearer <hex>" e "bearer <hex>" → vero; un byte cambiato, "Bearer", "", "Basic <hex>" → falso
#[test] fn debug_senza_codice() // format!("{c:?}") non contiene c.come_testo()
// ascoltatore.rs
#[test] fn ascolta_solo_su_127_0_0_1() // il socket locale è 127.0.0.1; connettersi a [::1]:porta fallisce
#[test] fn porta_occupata_errore_esplicito() // porta già presa da un TcpListener del test → Err(PortaOccupata), nessuna altra porta
#[test] fn rifiuta_origin() // Origin: http://example.com con tutto il resto valido → 403, gestore mai chiamato
#[test] fn rifiuta_host_diverso() // Host localhost:<porta>, 127.0.0.1:<altra>, assente → 403
#[test] fn rifiuta_options_e_get() // OPTIONS e GET → 405, nessuna Access-Control-* nella risposta
#[test] fn rifiuta_codice_mancante_o_errato() // senza Authorization, Bearer sbagliato → 401
#[test] fn content_type_diverso_415() // text/plain → 415
#[test] fn corpo_oltre_256k_413() // Content-Length 300 KB → 413, gestore mai chiamato
#[test] fn json_malformato_400_senza_panico() // "", "[", "{\"hook_event_name\":", 10 000 "[" annidate → 400; il thread di ascolto è ancora vivo (richiesta valida dopo → 200)
#[test] fn lettura_lenta_chiusa_dopo_2s() // un byte e poi silenzio → connessione chiusa entro 2,5 s
#[test] fn diciassettesima_connessione_503() // 16 PermissionRequest in attesa (gestore che non decide) → la 17ª riceve 503 in < 200 ms
#[test] fn risposta_prima_del_gestore() // gestore.evento che dorme 3 s → il client riceve "200" in < 200 ms
#[test] fn permesso_consenti_risponde_allow() // corpo == {"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}
#[test] fn permesso_nega_risponde_deny() // behavior "deny"
#[test] fn permesso_scaduto_risponde_vuoto() // attesa_massima 300 ms → corpo "{}", abbandonato(id) chiamato
#[test] fn permesso_chiuso_dal_client_abbandonato() // il client chiude → abbandonato(id) entro 1 s
```

- [ ] **Step 2:** `cargo test -p claude-hook --target-dir target\agente-<nome>` → FAIL.
- [ ] **Step 3:** implementa `codice.rs`; poi `ascoltatore.rs` con `httparse::Request` su un buffer di 8 KB, un thread per connessione col contatore atomico (`fetch_add` e `fetch_sub` in un guardiano con `Drop`), `set_read_timeout`/`set_write_timeout` a 2 s.
- [ ] **Step 4:** `cargo test -p claude-hook` e `cargo clippy --workspace --all-targets -- -D warnings` → PASS.
- [ ] **Step 5:** commit `feat(claude-hook): ascoltatore locale blindato` (con l'ok di Paolo).

### Task 3: Eventi e stato delle sessioni (puro)

**Files:**
- Create: `crates/claude-hook/src/evento.rs`, `crates/claude-hook/src/sessioni.rs`
- Modify: `crates/claude-hook/src/lib.rs`, `CHANGELOG.md`

**Interfaces:**
- Produces:
  - `pub enum TipoEvento { UserPromptSubmit, PreToolUse, PermissionRequest, Notification, Stop, StopFailure, SessionEnd }`.
  - `pub struct EventoHook { pub sessione: String, pub tipo: TipoEvento, pub cartella: String, pub strumento: Option<String>, pub notifica: Option<String>, pub errore: Option<String>, pub riassunto: Option<String> }`.
  - `pub enum ErroreEvento { NonValido, Sconosciuto }`; `pub fn evento_da_json(corpo: &[u8]) -> Result<EventoHook, ErroreEvento>`: struttura grezza con soli `session_id`, `hook_event_name`, `cwd`, `tool_name`, `notification_type`, `error`, `tool_input: Option<Box<RawValue>>` (analizzato **solo** per `PermissionRequest`). `session_id` di 1..=128 caratteri `[A-Za-z0-9_-]`, altrimenti `NonValido`. `cartella` = ultimo componente di `cwd` (separatori `/` e `\`), al più 60 caratteri. `strumento` al più 40 caratteri. `riassunto` (solo `PermissionRequest`): `command` per `Bash`; `file_path` per `Edit`, `MultiEdit`, `Write`, `Read`, `NotebookEdit`; `url` per `WebFetch`; `query` per `WebSearch`; altrimenti `None`; al più 2000 caratteri.
  - `pub enum Stato { Lavora, TiAspetta, Fatto, Errore }`; `pub struct Sessione { pub id: String, pub cartella: String, pub stato: Stato, pub strumento: Option<String>, pub lavora_da: Option<Instant>, pub ultimo: Instant, pub errore: Option<String>, pub vista: bool, pub pid: Option<u32> }`.
  - `pub struct Sessioni`; `pub enum Cambio { Nessuno, Cambiata(String), Tolta(String) }`; metodi `ricevi(&mut self, e: &EventoHook, ora: Instant) -> Cambio`, `elenco(&self) -> Vec<&Sessione>` (TiAspetta, Errore, Fatto non vista, Lavora, Fatto vista; a parità l'evento più recente prima), `scadute(&mut self, ora) -> Vec<String>`, `prossima_scadenza(&self, ora) -> Option<Duration>`, `segna_viste(&mut self)`, `togli(&mut self, id)`, `imposta_pid(&mut self, id, pid: u32)`, `lavorano(&self) -> usize`.
  - `pub const SILENZIO_MASSIMO: Duration = Duration::from_secs(30 * 60); pub const MASSIMO_SESSIONI: usize = 16;`
- Transizioni: `UserPromptSubmit` → Lavora (strumento `None`, `lavora_da = ora` se non già Lavora); `PreToolUse` → Lavora con `strumento`; `PermissionRequest` → TiAspetta; `Notification` con `permission_prompt`, `agent_needs_input` o `elicitation_dialog` → TiAspetta, con `idle_prompt` → TiAspetta solo se non Fatto/Errore, gli altri tipi → nessun cambio; `Stop` → Fatto (`vista = false`); `StopFailure` → Errore con `errore`; `SessionEnd` → `Tolta`. Un evento di una sessione sconosciuta la crea (anche `Stop` o `PermissionRequest`), tranne `SessionEnd`. Oltre 16 sessioni si toglie quella col `ultimo` più vecchio fra le non TiAspetta. `scadute`: silenziose da ≥ 30 min e non TiAspetta.

- [ ] **Step 1: test (falliscono)**

```rust
// evento.rs
#[test] fn solo_i_campi_necessari() // JSON con prompt, transcript_path, last_assistant_message → EventoHook senza traccia di quei testi (format!("{e:?}") non li contiene)
#[test] fn cartella_e_l_ultimo_nome() // "C:\\lavoro\\progetto-a" e "/home/x/progetto-a/" → "progetto-a"
#[test] fn riassunto_per_strumento() // Bash {command:"cargo test"} → "cargo test"; Write {file_path:"src/a.rs", content:"…"} → "src/a.rs"; strumento ignoto → None
#[test] fn session_id_strano_rifiutato() // "", 129 caratteri, "a/b", "a b" → NonValido
#[test] fn evento_sconosciuto() // "SubagentStart" → Sconosciuto
// sessioni.rs
#[test] fn ciclo_di_una_sessione() // prompt → Lavora; PreToolUse Edit → Lavora "Edit"; PermissionRequest → TiAspetta; Stop → Fatto; SessionEnd → Tolta
#[test] fn idle_dopo_fatto_resta_fatto() // Stop poi Notification idle_prompt → Fatto
#[test] fn errore_dallo_stopfailure() // StopFailure rate_limit → Errore, errore Some("rate_limit")
#[test] fn silenziosa_sparisce_dopo_trenta_minuti() // Lavora, ora+30 min → scadute == [id]; TiAspetta non scade; prossima_scadenza giusta
#[test] fn eventi_fuori_ordine_e_sessioni_limitate() // primo evento Stop → Fatto; SessionEnd doppio → Nessuno; 17 sessioni → 16, tolta la più vecchia non TiAspetta
#[test] fn ordine_per_urgenza() // [Lavora, TiAspetta, Fatto, Errore] → elenco TiAspetta, Errore, Fatto, Lavora
```

- [ ] **Step 2:** `cargo test -p claude-hook` → FAIL.
- [ ] **Step 3:** implementa.
- [ ] **Step 4:** `cargo test -p claude-hook` → PASS.
- [ ] **Step 5:** commit `feat(claude-hook): eventi e stato delle sessioni` (con l'ok di Paolo).

### Task 4: Sessioni nell'assistente: voci, occhi, pillola media

**Files:**
- Create: `crates/assistente/src/claude.rs`, `crates/assistente/src/ponte/con_claude.rs`, `crates/assistente/assets/icone/claude.svg`, `crates/claude-hook/examples/sessioni_finte.rs`
- Modify: `crates/casa-core/src/config.rs`, `crates/assistente/Cargo.toml` (`claude-hook`), `src/main.rs`, `src/ponte.rs` (`mod con_claude;`, campi, chiamate da `azione`, `pannello_aperto`/`pannello_chiuso`), `src/attenzione.rs` (`Fonte::Claude`), `src/pillola.rs`, `src/finestre.rs`, `ui/pillola.slint`, `ui/icone.slint`, `ui/tema.slint`, `CHANGELOG.md`
- Test: `src/claude.rs`, `src/ponte/con_claude.rs`, `src/test_ui.rs`, `crates/casa-core/tests/config.rs`

**Interfaces:**
- Consumes: `avvia`, `Gestore`, `Arrivo`, `CodiceHook` (Task 2); `evento_da_json`, `Sessioni`, `Stato` (Task 3); `Ponte::metti_voce`, `togli_voce`, `Attese`, `Voce`, `Pulsante` (M1).
- Produces:
  - `Config { …, #[serde(default)] pub claude: ConfigClaude }`; `#[serde(rename_all = "camelCase", default)] pub struct ConfigClaude { pub collegato: bool, pub porta: Option<u16>, pub soglia_limite: u8 /* 80 */, pub soglia_spesa: Option<f64>, pub limiti_avvisati: HashMap<String, i64>, pub spesa_avvisata: HashMap<String, u8>, pub insolita_avvisata: Option<String> }` (`Default` con `soglia_limite: 80`; un file di M2 si legge scollegato).
  - `Fonte::Claude` (`come_stringa` → `"claude"`, icona `claude`).
  - `claude.rs`: `pub const PREFISSO_CLAUDE: &str = "claude.";` `pub fn id_sessione(id: &str) -> String` (impronta FNV-1a 64 bit esadecimale: gli id delle voci non portano il `session_id`); `pub fn voce_sessione(s: &Sessione, ora: Instant) -> Option<Voce>`: TiAspetta → *attenzione* `claude.<impronta>.aspetta`, titolo «Claude ti aspetta · <cartella>»; Errore → *attenzione* `claude.<impronta>.errore`, titolo «Claude si è fermato · <cartella>», dettaglio da `frase_errore_claude`; Fatto non vista → *informazione* `claude.<impronta>.fatto`, titolo «Claude ha finito · <cartella>»; Lavora o Fatto vista → `None`. `pub fn frase_errore_claude(e: &str) -> &'static str` (`rate_limit` → «Limite raggiunto», `authentication_failed` → «Accesso da rifare», `billing_error` → «Problema di fatturazione», `server_error` → «Servizio non disponibile», `max_output_tokens` → «Risposta troppo lunga», altro → «Errore dell'API»). `pub fn strumento_italiano(s: Option<&str>) -> String` (`Bash` «Comando», `Edit`/`MultiEdit` «Modifica», `Write` «Scrive», `Read` «Legge», `Grep`/`Glob` «Cerca», `WebFetch`/`WebSearch` «Web», `Task`/`Agent` «Agente», `None` «Pensa», altro: il nome ripulito, ≤ 12 caratteri). `pub fn durata_breve(d: Duration) -> String` («0:07», «3:12», «1:02:05»). `pub fn testo_attivita(s: &Sessioni, ora: Instant) -> Option<String>`: una sessione Lavora → «Claude · <strumento> · <durata>»; più → «Claude ×N · <durata della più lunga>»; nessuna → `None`. `pub fn tinta_occhi(s: &Sessioni) -> Option<Livello>` (TiAspetta/Errore → Attenzione, Fatto non vista → Informazione).
  - `finestre.rs`: `pub const LARGHEZZA_ATTIVITA: f32 = 210.0`.
  - Pillola: `in property <string> attivita` (vuota = nessuna; mostrata a riposo e col puntino, non da allargata né da ritirata; larga `LARGHEZZA_ATTIVITA`, testo mono 13 px; i lati si nascondono); `in property <LivelloUi> tinta-occhi: LivelloUi.nessuno` (occhi del colore del livello, tranquillo altrimenti). Nome per Narrator: «ItsyHome, Claude, Modifica, 3 minuti».
  - `con_claude.rs`: `pub fn avvia_se_collegato(p: &Ponte)` (chiamata da `main.rs` dopo `ponte::collega`): se `config.claude.collegato` e c'è un codice (`codice_attivo`) e una porta → `claude_hook::ascoltatore::avvia` con `GestoreIsola`; `PortaOccupata` → voce *attenzione* `claude.collegamento`, «Collegamento con Claude Code non disponibile», dettaglio «La porta <n> è usata da un altro programma.», pulsante `PULSANTE_IMPOSTAZIONI`. `pub fn codice_attivo(segreti: &dyn Segreti, in_prova: bool) -> Option<CodiceHook>`: in prova dalla variabile `ISOLA_HOOK_TOKEN` del processo, altrimenti dal portachiavi `isola:claude-hook`. `struct GestoreIsola` (`Gestore`): `evento` → `evento_da_json` sul thread della connessione, poi `invoke_from_event_loop` con il solo `EventoHook`; i rifiuti contano `claude_rifiutate`. Sul Ponte: `claude_sessioni: RefCell<Sessioni>`, `claude_ascolto: RefCell<Option<Ascoltatore>>`, `sveglia_claude: slint::Timer` (singola, alla `prossima_scadenza`), `timer_attivita: slint::Timer` (1 s, D2: acceso solo con `lavorano() > 0`, pillola visibile e umore ≠ dorme; aggiorna `attivita` solo se il testo cambia). Pannello aperto → `segna_viste` non tocca le voci *fatto*; alla chiusura del pannello le voci *fatto* viste spariscono («sfuma»). `main.rs` chiama `con_claude::spegni()` prima di `ponte::spegni()`. `Ponte::azione` passa ogni voce che inizia per `PREFISSO_CLAUDE` a `con_claude::azione(&self, voce, pulsante)` (i task 5, 10 e 12 ci aggiungono i loro pulsanti).
  - `crates/claude-hook/examples/sessioni_finte.rs` (per prove e misure, usato anche dai task 9, 12 e 13): `-- --porta <n> --sessioni 3 --secondi 60`, codice da `ISOLA_HOOK_TOKEN`; per ogni sessione un prompt, un `PreToolUse` ogni 2 s, uno `Stop` ogni 30 s, una barra ogni 5 s; cartelle finte («progetto-a»…).

- [ ] **Step 1: test (falliscono)**

```rust
// claude.rs
#[test] fn voci_per_stato() // TiAspetta → Attenzione «Claude ti aspetta · progetto-a»; Errore rate_limit → dettaglio «Limite raggiunto»; Fatto → Informazione; Lavora → None
#[test] fn id_della_voce_senza_session_id() // voce.id non contiene il session_id
#[test] fn testo_attivita_una_e_piu_sessioni() // Edit da 192 s → "Claude · Modifica · 3:12"; due → "Claude ×2 · 3:12"; nessuna → None
#[test] fn durate() // 7 s "0:07"; 3725 s "1:02:05"
#[test] fn strumenti_in_italiano() // Bash "Comando", None "Pensa", "UnNomeLunghissimoDavvero" → 12 caratteri
// ponte/con_claude.rs (ponte_di_prova)
#[test] fn eventi_diventano_voci() // UserPromptSubmit, Stop → voce informazione «Claude ha finito · progetto-a»; poi SessionEnd → voce tolta
#[test] fn azione_sulle_voci_claude() // voce claude.collegamento + PULSANTE_IMPOSTAZIONI → chiede_impostazioni chiamato
#[test] fn fatto_sparisce_alla_chiusura_del_pannello() // Stop, pannello aperto e chiuso → voce tolta, sessione Fatto vista
#[test] fn scollegato_niente_ascoltatore() // config.claude.collegato false → claude_ascolto None
#[test] fn porta_occupata_da_avviso() // porta presa dal test → voce "claude.collegamento" Attenzione
// test_ui.rs
#[test] fn pillola_media_misure() // attivita non vuota: larghezza-pillola == LARGHEZZA_ATTIVITA, lati non visibili; da allargata l'attività non si vede
// casa-core/tests/config.rs
#[test] fn config_di_m2_scollegata() // JSON senza "claude" → collegato false, soglia_limite 80
```

- [ ] **Step 2:** `cargo test -p assistente claude` → FAIL.
- [ ] **Step 3:** implementa; ogni proprietà Slint si riscrive solo se cambia.
- [ ] **Step 4:** `cargo test --workspace` → PASS (compresi `misure_slint_e_rust` e il test dei colori a mano).
- [ ] **Step 5: prova dal vivo (senza Paolo, nessun file vero):** assistente in prova (`ITSYHOME_ASSISTENTE_DIR`, config con `claude.collegato` e una porta), `ISOLA_HOOK_TOKEN` impostata nel processo; `sessioni_finte` verso la porta dell'assistente: pillola media durante il lavoro, voce *fatto* allo stop, sparita chiudendo il pannello; foto di zona a 100 % e 175 %.
- [ ] **Step 6:** commit `feat(assistente): stato delle sessioni di Claude Code nella pillola` (con l'ok di Paolo).

### Task 5: Permessi dalla pillola

**Files:**
- Modify: `crates/assistente/src/ponte/con_claude.rs`, `src/ponte.rs` (`azione`), `src/claude.rs`, `src/attenzione.rs` (`aspetto_pillola`), `src/pillola.rs`, `ui/pillola.slint`, `CHANGELOG.md`
- Test: `src/ponte/con_claude.rs`, `src/attenzione.rs`, `src/claude.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `Gestore::permesso`, `AttesaPermesso`, `Decisione` (Task 2); `EventoHook.riassunto` (Task 3); `Pulsante::delicato`, `pulsanti_pillola` (M1: col testo tagliato «Consenti» diventa «Apri»).
- Produces:
  - `claude.rs`: `pub const PERMESSI_RISOLVIBILI: bool` (D1: `true` se lo spike è positivo); `pub const PULSANTE_NEGA: &str = "nega"; pub const PULSANTE_CONSENTI: &str = "consenti"; pub const PULSANTE_TERMINALE: &str = "terminale";` `pub fn voce_permesso(id: u64, e: &EventoHook, ora: Instant) -> Voce`: *bloccante*, `Fonte::Claude`, id `claude.permesso.<id>`, titolo «Claude chiede: <strumento italiano> <riassunto ≤ 60 caratteri>», dettaglio = riassunto intero + «\nSessione: <cartella>», tutto passato da `gestione_protocollo::testo_pulito` (via caratteri invisibili e di direzione); pulsanti `[Pulsante::new(PULSANTE_NEGA,"Nega"), Pulsante::delicato(PULSANTE_CONSENTI,"Consenti")]`, oppure col ramo B `[Pulsante::new(PULSANTE_TERMINALE,"Apri il terminale")]`.
  - `attenzione.rs`: fra più bloccanti la pillola mostra **la più vecchia** (`min_by_key(creata)`), il pannello resta nell'ordine di `ordinate`; `pub fn altri_bloccanti(a: &Attese) -> usize`; la pillola aggiunge « +N» al titolo (fuori dal testo tagliato).
  - Ponte: `claude_permessi: RefCell<HashMap<u64, (String /*sessione*/, Sender<Decisione>)>>`. `GestoreIsola::permesso` (thread della connessione): analizza l'evento, crea il canale, consegna `(id, evento, sender)` all'interfaccia, restituisce `Some(AttesaPermesso)`; con `PERMESSI_RISOLVIBILI == false` consegna la voce e restituisce `None`. `azione(voce, PULSANTE_CONSENTI|PULSANTE_NEGA)` → manda la decisione **solo** al mittente di quella voce e toglie la voce; `abbandonato(id)` → toglie la voce. Un `UserPromptSubmit`, `Stop`, `StopFailure` o `SessionEnd` della stessa sessione manda `Nessuna` ai suoi permessi e toglie le voci.
  - Nessun «Consenti sempre», nessun `updatedPermissions`, nessun `updatedInput`.

- [ ] **Step 1: test (falliscono)**

```rust
// ponte/con_claude.rs
#[test] fn consenti_va_solo_a_chi_ha_chiesto() // due permessi di due sessioni; consenti sul primo → primo riceve Consenti, secondo nessun messaggio (try_recv Empty)
#[test] fn nessun_consenso_senza_gesto() // permesso, poi abbandonato(id) → voce tolta, il receiver non ha mai ricevuto Consenti
#[test] fn permesso_sparisce_se_la_sessione_va_avanti() // permesso, poi Stop della stessa sessione → Nessuna, voce tolta; Stop di un'altra sessione → voce resta
#[test] fn permesso_di_sessione_chiusa() // SessionEnd poi PermissionRequest della stessa → la sessione rinasce TiAspetta, voce presente, nessun panico
// claude.rs
#[test] fn riassunto_pulito() // command con "\u{202E}" e "\n" → nessun carattere di direzione, a capo diventati spazi
#[test] fn titolo_permesso_tagliato_a_sessanta() // command di 300 caratteri → titolo ≤ "Claude chiede: Comando " + 60
// attenzione.rs
#[test] fn pillola_col_bloccante_piu_vecchio_e_piu_n() // tre bloccanti → Allargata sul più vecchio; altri_bloccanti == 2
// test_ui.rs
#[test] fn invio_da_solo_non_consente() // pannello con la voce permesso: fuoco iniziale su «Nega»; Invio → azione "nega"; Ctrl+Invio → "consenti"
#[test] fn pillola_mostra_piu_due() // titolo con « +2» visibile
```

- [ ] **Step 2:** `cargo test -p assistente permesso` → FAIL.
- [ ] **Step 3:** implementa (ramo scelto da D1).
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: prova dal vivo con Paolo** (con `claude --settings <file temporaneo>`, `--permission-mode default`): permesso dalla pillola (clic su Consenti e su Nega), dal pannello con Ctrl+Invio, risposta data prima nel terminale (la voce sparisce), due sessioni con permessi insieme («+1»), pillola ritirata che esce per il bloccante, Narrator sui pulsanti.
- [ ] **Step 6:** commit `feat(assistente): permessi di Claude Code dalla pillola` (con l'ok di Paolo).

### Task 6: Barra di stato e limiti dell'abbonamento

**Files:**
- Create: `crates/claude-hook/src/barra.rs`
- Modify: `crates/claude-hook/src/lib.rs`, `crates/assistente/src/main.rs` (ramo `--barra-stato` **prima** dell'istanza unica e di Slint), `src/ponte/con_claude.rs`, `CHANGELOG.md`
- Test: `crates/claude-hook/src/barra.rs`, `src/ponte/con_claude.rs`

**Interfaces:**
- Consumes: `CodiceHook`, `PERCORSO_BARRA`, `Gestore::barra` (Task 2).
- Produces:
  - `pub struct Limite { pub percentuale: f32, pub azzera_alle: i64 }`; `pub struct DatiBarra { pub sessione: String, pub modello: Option<String>, pub contesto: Option<f32>, pub cinque_ore: Option<Limite>, pub sette_giorni: Option<Limite> }` (`Serialize`/`Deserialize`: è anche il corpo inviato all'assistente).
  - `pub const MASSIMO_STDIN: usize = 64 * 1024;` `pub fn dati_da_json(corpo: &[u8]) -> Result<DatiBarra, ErroreEvento>` (solo i campi dei fatti verificati; `null` → `None`; `session_id` con le regole del Task 3).
  - `pub fn riga(d: &DatiBarra, ora: i64, ora_locale: impl Fn(i64) -> String) -> String`: «Opus 5.5 · contesto 42% · 5h 23% (↻ 14:30) · 7g 61%»; le parti mancanti si omettono; un limite con `azzera_alle <= ora` non si mostra; senza nulla «Claude».
  - `pub fn esegui(ingresso: impl Read, uscita: impl Write, porta: u16, codice: Option<&str>, tempo: Duration) -> io::Result<()>`: legge al più 64 KB; analizza; **stampa la riga prima di inviare**; poi, se c'è il codice, `TcpStream::connect_timeout(127.0.0.1:porta, 30 ms)` e scrive un `POST` con il **solo** `DatiBarra` serializzato (mai lo stdin grezzo), senza aspettare la risposta oltre il `tempo` residuo.
  - `main.rs`: `assistente --barra-stato <porta>` → `esegui(stdin, stdout, porta, env ISOLA_HOOK_TOKEN, 40 ms)` ed esce; niente finestre, niente istanza unica, niente configurazione.
  - Ponte: `claude_limiti: RefCell<Option<(Option<Limite>, Option<Limite>)>>` (l'arrivo più recente vince: i limiti sono dell'account), `claude_contesto: RefCell<HashMap<String /*sessione*/, f32>>` per la vista; `pub fn limiti(&self) -> (Option<Limite>, Option<Limite>)` già filtrati per scadenza (li usano i task 9 e 12); un limite scaduto vale come assente; senza abbonamento niente anelli (mai «0%»).

- [ ] **Step 1: test (falliscono)**

```rust
// barra.rs
#[test] fn riga_completa() // fixture con model.display_name "Opus 5.5", used 42.4, 5h 23.5 resets_at T, 7g 61 → "Opus 5.5 · contesto 42% · 5h 24% (↻ 14:30) · 7g 61%"
#[test] fn riga_senza_abbonamento() // senza rate_limits → "Opus 5.5 · contesto 42%"
#[test] fn percentuale_null_ignorata() // context_window.used_percentage null → senza «contesto»
#[test] fn limite_scaduto_non_mostrato() // resets_at < ora → niente «5h»
#[test] fn campi_extra_non_inoltrati() // stdin con cwd, transcript_path, session_name, workspace: il corpo POST ricevuto da un TcpListener del test non contiene quei valori
#[test] fn stdin_oltre_64k() // 100 KB di spazzatura → stampa "Claude", nessun panico
#[test] fn assistente_spento_stampa_lo_stesso() // porta senza ascoltatore → riga stampata, esegui finisce in < 50 ms
#[test] fn senza_codice_non_invia() // codice None → nessuna connessione al TcpListener del test
// ponte/con_claude.rs
#[test] fn limiti_dall_ultimo_arrivo_e_anelli_nascosti_senza() // due barre: vince l'ultima; barra senza rate_limits → limiti None
```

- [ ] **Step 2:** `cargo test -p claude-hook barra` → FAIL.
- [ ] **Step 3:** implementa.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: verifica Windows:** in **release** (`windows_subsystem = "windows"`) `Get-Content fixture.json | .\assistente.exe --barra-stato 1` stampa la riga (stdin e stdout ereditati funzionano anche senza console); `Measure-Command` su 50 esecuzioni: mediana < 50 ms (annotare nel CHANGELOG; se no, fermarsi e dirlo a Paolo prima di cercare altre strade).
- [ ] **Step 6:** commit `feat(assistente): barra di stato di Claude Code e limiti dell'abbonamento` (con l'ok di Paolo).

### Task 7: Collegamento a Claude Code (installazione in `settings.json`)

**Files:**
- Create: `crates/claude-hook/src/installa.rs`, `crates/assistente/src/sistema/ambiente_utente.rs`
- Modify: `crates/claude-hook/Cargo.toml` (`serde_json` `preserve_order`), `crates/assistente/Cargo.toml` (windows-sys `Win32_Storage_FileSystem` per `GetShortPathNameW`), `src/sistema/mod.rs`, `src/ponte/con_claude.rs`, `src/ponte.rs` (`collega_finestra`), `src/finestra_impostazioni.rs`, `ui/impostazioni.slint`, `CHANGELOG.md`
- Test: `crates/claude-hook/src/installa.rs` (tutti su `TempDir`), `src/sistema/ambiente_utente.rs`, `src/ponte/con_claude.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `CodiceHook` (Task 2), `avvia_se_collegato`, `codice_attivo` (Task 4), `Segreti` (casa-core).
- Produces:
  - `installa.rs`: `pub const VERSIONE_VOCI: u32 = 1;` (nell'URL: `http://127.0.0.1:<porta>/isola/v1/evento`); `pub const EVENTI: [&str; 7] = ["UserPromptSubmit","PreToolUse","PermissionRequest","Notification","Stop","StopFailure","SessionEnd"];` (nessun `SessionStart`, nessun `PostToolUse`); ogni voce `{"type":"http","url":…,"headers":{"Authorization":"Bearer $ISOLA_HOOK_TOKEN"},"allowedEnvVars":["ISOLA_HOOK_TOKEN"],"timeout":5}` (120 per `PermissionRequest`), in un gruppo senza `matcher`, mai `onFailure`.
  - `pub struct Piano { pub porta: u16, pub comando_barra: Option<String> }`; `pub enum ErroreInstalla { NonValido(String), Io(String) }`; `pub enum StatoInstallazione { Assente, Presente { porta: u16, versione: u32, comando_barra: Option<String> } }`.
  - `pub fn file_impostazioni(config_dir: Option<&OsStr>, home: &Path) -> PathBuf`; `pub fn leggi(file: &Path) -> Result<Value, ErroreInstalla>` (assente → `{}`; BOM tollerato; non oggetto, commenti, virgole finali → `NonValido`); `pub fn nostre(v: &Value) -> StatoInstallazione` (riconosce le voci dall'URL `http://127.0.0.1:` + `/isola/v` e la barra da `--barra-stato`); `pub fn unisci(v: Value, p: &Piano) -> Value` (toglie le nostre vecchie e mette le nuove in coda, conserva tutto il resto nell'ordine; barra solo se `statusLine` manca o è nostra); `pub fn togli(v: Value) -> Value` (via le nostre; via gruppi, eventi e `hooks` rimasti vuoti per causa nostra); `pub fn anteprima(v: &Value, p: &Piano) -> Anteprima { pub testo: String /* JSON delle sole voci aggiunte */, pub barra_occupata: bool, pub fuori_lista: bool /* allowedHttpHookUrls o httpHookAllowedEnvVars presenti e senza di noi */ }`; `pub fn scrivi_sicuro(file: &Path, v: &Value) -> Result<(), ErroreInstalla>` (copia `settings.json.isola-bak` se il file c'era, `.tmp` + `rename`, indentazione di 2 spazi, ritorno a capo finale); `pub fn comando_barra(exe: &Path, porta: u16, corto: impl Fn(&Path) -> Option<PathBuf>) -> Option<String>` (D5: `/` al posto di `\`; con spazi usa `corto`, senza percorso corto → `None`).
  - `ambiente_utente.rs`: `pub const VARIABILE: &str = "ISOLA_HOOK_TOKEN";` `pub trait AmbienteUtente { fn imposta(&self, nome: &str, valore: &str) -> Result<(), String>; fn togli(&self, nome: &str) -> Result<(), String>; fn leggi(&self, nome: &str) -> Option<String>; }`; `#[cfg(windows)] pub struct RegistroUtente { prova: bool }` (`HKCU\Environment` o, in prova, `HKCU\Software\ItsyHomeProva\Environment`; dopo ogni cambio `SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, "Environment", SMTO_ABORTIFHUNG, 5000)`); `#[cfg(test)] pub struct AmbienteFinto(RefCell<HashMap<String,String>>)`.
  - `con_claude.rs`: `pub fn prepara_collegamento(&self) -> Result<Anteprima, String>` (porta = `config.claude.porta` o `bind 127.0.0.1:0`; codice nuovo se manca; nulla scritto); `pub fn conferma_collegamento(&self, amb: &dyn AmbienteUtente) -> Result<(), String>` (scrive il file, il codice nel portachiavi e nella variabile, `collegato = true`, avvia l'ascoltatore); `pub fn scollega(&self, amb) -> Result<(), String>`; `pub fn rigenera_codice(&self, amb) -> Result<(), String>`; `pub fn controlla_all_avvio(&self)` (collegato ma `nostre` dice porta, versione o comando della barra diversi, o assenti → voce *informazione* `claude.aggiorna` «Il collegamento con Claude Code va aggiornato», pulsante «Impostazioni»; **mai** scrittura senza conferma). In prova il file è `<cartella di prova>\claude-finto\settings.json`.
  - Impostazioni: sezione **«Claude»** all'indice 4 (`["Home Assistant","Dispositivi","Collegamenti","Isola","Claude","Sistema","Informazioni"]`), `SEZIONE_CLAUDE = 4`, `SEZIONE_INFORMAZIONI` da 5 a 6. Contenuto: stato («Collegato · porta 47613», «Non collegato», «Da aggiornare»), «Collega Claude Code» → anteprima in un riquadro mono scorrevole con «Scrivi in settings.json» / «Annulla», nota «I terminali aperti prima vanno chiusi e riaperti.», avviso se `barra_occupata` («La barra di stato è già usata: i limiti dell'abbonamento non saranno disponibili») o `fuori_lista`; «Scollega»; «Rigenera il codice»; fuori da Windows la riga «Imposta ISOLA_HOOK_TOKEN nella tua shell» con «Copia il codice».

- [ ] **Step 0:** rileggere su `code.claude.com/docs/en/settings-reference` le voci `allowedHttpHookUrls`, `httpHookAllowedEnvVars`, `statusLine`, `disableAllHooks`; annotarle in testa a `installa.rs` (se `disableAllHooks` è vero l'anteprima lo dice).
- [ ] **Step 1: test (falliscono)**

```rust
// installa.rs (TempDir, fixture settings con permissions, env, hooks.PreToolUse altrui con matcher "Bash", model)
#[test] fn unisci_conserva_tutto() // ogni chiave e ogni hook altrui restano, nello stesso ordine
#[test] fn unisci_due_volte_come_una() // unisci(unisci(v)) == unisci(v)
#[test] fn togli_ridà_l_originale() // to_string(togli(unisci(v))) == to_string(v)
#[test] fn json_non_valido_non_si_scrive() // file con "// commento" o virgola finale → leggi Err(NonValido); i byte del file sono identici a prima
#[test] fn bom_tollerato() // "\u{feff}{…}" → leggi Ok
#[test] fn riserva_e_scrittura_atomica() // dopo scrivi_sicuro: .isola-bak == vecchi byte; nessun .tmp rimasto
#[test] fn barra_altrui_non_toccata() // statusLine di altri → invariata, anteprima.barra_occupata
#[test] fn nessun_codice_in_settings() // il JSON scritto non contiene il codice; contiene "$ISOLA_HOOK_TOKEN"
#[test] fn eventi_e_timeout() // 7 eventi, nessun SessionStart né PostToolUse; timeout 120 solo per PermissionRequest; nessun "onFailure"
#[test] fn percorso_cambiato_rilevato() // voci di un altro exe o di un'altra porta → nostre() con i valori vecchi
#[test] fn comando_barra_con_spazi_usa_il_percorso_corto() // "C:\\Program Files\\x\\assistente.exe" con corto → "C:/PROGRA~1/x/assistente.exe --barra-stato 47613"; senza corto → None
#[test] fn file_in_claude_config_dir() // CLAUDE_CONFIG_DIR presente → <dir>/settings.json, altrimenti <home>/.claude/settings.json
#[test] fn fuori_lista_segnalato() // allowedHttpHookUrls senza 127.0.0.1 → anteprima.fuori_lista
// ponte/con_claude.rs (MemoriaSegreti, AmbienteFinto, TempDir)
#[test] fn collega_e_scollega() // conferma → file con le voci, variabile e portachiavi con lo stesso codice, collegato; scollega → voci, variabile e codice tolti
#[test] fn rigenera_cambia_codice_ovunque() // nuovo codice in portachiavi e variabile, diverso dal vecchio
#[test] fn all_avvio_niente_scritture() // file con porta diversa → voce claude.aggiorna, file invariato
// test_ui.rs
#[test] fn sezione_claude_e_informazioni_spostata() // sezione 4 «Claude», 6 «Informazioni»
```

- [ ] **Step 2:** `cargo test --workspace` → FAIL.
- [ ] **Step 3:** implementa; poi `cargo test --workspace` una volta col solo `preserve_order` acceso per vedere se qualche test esistente confrontava JSON in ordine alfabetico (se sì, correggere il test, non togliere la funzione).
- [ ] **Step 4:** `cargo test --workspace` e clippy → PASS.
- [ ] **Step 5: prova dal vivo in modalità di prova** (file finto, chiave di registro di prova): collega, anteprima, scollega, rigenera; Narrator e tastiera nella sezione. **Poi con Paolo** sul vero `settings.json`: lui legge l'anteprima e conferma; nuova scheda di Windows Terminal e PowerShell da solo vedono la variabile (verificare se Windows Terminal la ricarica per le nuove schede o va riaperto: scriverlo nella guida); barra di stato in Git Bash e, se Paolo ce l'ha, senza Git Bash.
- [ ] **Step 6:** commit `feat(assistente): collegamento sicuro a Claude Code` (con l'ok di Paolo).

### Task 7A: Canale protetto (tubo con nome e aiutante `isola-hook`)

> Aggiunto il 18/11 dopo la revisione di sicurezza: spec §10. Sostituisce il trasporto HTTP dei task 2, 6 e 7.

**Files:**
- Create: `crates/claude-hook/src/tubo.rs` (server del tubo), `crates/claude-hook/src/verifica.rs` (percorso dell'immagine del processo dall'altra parte del tubo), `crates/claude-hook/src/messaggio.rs` (record minimo e risposta), `crates/claude-hook/src/bin/isola-hook.rs`
- Modify: `crates/claude-hook/src/lib.rs`, `crates/claude-hook/Cargo.toml` (windows-sys: `Win32_System_Pipes`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_System_Threading`, `Win32_Storage_FileSystem`, `Win32_System_IO`), `crates/claude-hook/src/barra.rs` (`esegui` manda per il tubo), `crates/assistente/src/main.rs` (via il ramo `--barra-stato`), `crates/assistente/src/ponte/con_claude.rs` (+ `con_claude/collegamento.rs` dove serve), `CHANGELOG.md`
- Delete: `crates/claude-hook/src/ascoltatore.rs` (TCP/HTTP), `crates/claude-hook/src/codice.rs`, `crates/claude-hook/src/mittente.rs` (tabella TCP) — i test che valgono ancora si spostano su `tubo.rs`.

**Interfaces:**
- Restano con lo stesso nome e significato: `Gestore { evento, permesso -> Option<AttesaPermesso>, abbandonato, barra, rifiutata }`, `Arrivo` (senza `porta_mittente`; `pid_mittente` = PID dell'aiutante; `sequenza` all'accettazione), `AttesaPermesso`, `Decisione`, `ErroreAvvio` (`PortaOccupata` → `CanaleOccupato`).
- `tubo.rs`: `pub fn nome_tubo() -> Result<String, String>` (`\\.\pipe\isola-claude-<SID>`); `pub struct ConfigTubo { pub nome: String, pub cliente_atteso: PathBuf, pub attesa_massima: Duration }`; `pub fn avvia(c: ConfigTubo, g: Arc<dyn Gestore>) -> Result<Servitore, ErroreAvvio>`; `Servitore::rifiutate()`; `Drop` chiude e raccoglie i thread (sveglia con una connessione a sé stesso). Creazione: SDDL che ammette solo il SID dell'utente corrente (`D:P(A;;GA;;;<SID>)`), `FILE_FLAG_FIRST_PIPE_INSTANCE` sulla prima istanza, `PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_REJECT_REMOTE_CLIENTS`, `PIPE_UNLIMITED_INSTANCES` no: **16**. Un thread fermo su `ConnectNamedPipe` (nessun polling); un thread per connessione col contatore come nel Task 2.
- Regole del server, **prima di leggere**: oltre 16 → chiusa; cliente il cui percorso d'immagine non è `cliente_atteso` → `rifiutata` e chiusa. Poi: un messaggio ≤ 64 KB letto entro 2 s, JSON di `messaggio::Richiesta` riconvalidato (stesse regole dei task 3 e 6), altrimenti `rifiutata`.
- `messaggio.rs`: `pub enum Richiesta { Evento(RecordEvento), Barra(DatiBarra) }` con `RecordEvento` = i soli campi di `EventoHook` (§10); `pub fn decisione_json(d: Decisione) -> Option<String>` (l'unico testo che l'aiutante può stampare; `Nessuna` → `None`).
- `verifica.rs` (`cfg(windows)`): `pub fn immagine_server(h) -> Option<PathBuf>`, `pub fn immagine_cliente(h) -> Option<PathBuf>`, `pub fn stesso_file(a: &Path, b: &Path) -> bool` (canonico, senza maiuscole). Fuori da Windows: stub che restituiscono `None`.
- `isola-hook.exe`: `evento` → legge ≤ 256 KB da stdin, costruisce `RecordEvento` (minimizzazione qui), apre il tubo (`CreateFileW` con `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`), **verifica il server** (`assistente.exe` nella sua cartella), manda; per `PermissionRequest` aspetta la risposta ≤ 115 s e stampa `decisione_json`; per gli altri eventi esce subito (≤ 300 ms in tutto) senza stampare. `barra` → come `barra::esegui` (riga prima, poi invio per il tubo, budget 40 ms). Ogni errore: uscita 0 senza output. Nessuna dipendenza da Slint né da `casa-core`.

- [ ] **Step 0:** rileggere su `code.claude.com/docs/en/hooks` forma exec (`args`), `async`, `shell`, uscita vuota = nessuna decisione, timeout; su learn.microsoft.com `CreateNamedPipeW`, `GetNamedPipeServerProcessId`, `GetNamedPipeClientProcessId`, `QueryFullProcessImageNameW`, `ConvertStringSecurityDescriptorToSecurityDescriptorW`. Annotarle in testa a `tubo.rs`.
- [ ] **Step 1: test (falliscono)**

```rust
// tubo.rs (Windows; nome di tubo unico per test)
#[test] fn solo_il_cliente_atteso() // cliente_atteso = l'exe del test → accettato; un altro percorso atteso → rifiutata, gestore mai chiamato
#[test] fn canale_occupato_errore_esplicito() // tubo con lo stesso nome già creato dal test → Err(CanaleOccupato)
#[test] fn messaggio_troppo_grande_o_malformato() // 70 KB, "", "[", 10 000 "[" → rifiutata, nessun panico; dopo, un messaggio valido passa
#[test] fn lettura_lenta_chiusa_dopo_2s()
#[test] fn diciassettesima_connessione_chiusa()
#[test] fn permesso_consenti_nega_scaduto_chiuso() // risposta allow/deny; scaduto → nessuna risposta + abbandonato; cliente chiuso → abbandonato entro 1 s
#[test] fn sequenza_cresce_per_connessione()
// messaggio.rs
#[test] fn decisione_json_unica_uscita() // Consenti/Nega → JSON esatto; Nessuna → None
#[test] fn record_senza_campi_estranei() // stdin con prompt, transcript_path, tool_input.content → il record serializzato non li contiene
// bin isola-hook (test d'integrazione in crates/claude-hook/tests/aiutante.rs)
#[test] fn senza_isola_esce_in_silenzio() // nessun tubo → uscita 0, stdout vuoto, < 300 ms
#[test] fn server_falso_non_riceve_nulla() // tubo creato dal test (immagine ≠ assistente.exe) → il test non legge byte, stdout vuoto
#[test] fn mai_decisioni_fuori_dal_permesso() // server di prova che risponde «Consenti» a un PreToolUse → stdout vuoto
```

- [ ] **Step 2:** `cargo test -p claude-hook` → FAIL.
- [ ] **Step 3:** implementa; poi l'assistente usa `tubo::avvia` (in prova `cliente_atteso` dalla variabile `ITSYHOME_AIUTANTE_ATTESO`, solo con `ITSYHOME_ASSISTENTE_DIR`), toglie codice, porta, portachiavi e variabile del Task 7 dal percorso di avvio; il ramo `--barra-stato` sparisce da `main.rs`.
- [ ] **Step 4:** `cargo test --workspace` e clippy → PASS; misura in release: mediana di 50 `isola-hook evento` con un `PreToolUse` finto e isola di prova accesa < 30 ms; `barra` < 50 ms (annotare nel CHANGELOG).
- [ ] **Step 5:** commit `feat(claude-hook): canale protetto con l'aiutante isola-hook`.

### Task 7B: Collegamento rifatto sul canale protetto

**Files:**
- Modify: `crates/claude-hook/src/installa.rs`, `crates/assistente/src/ponte/con_claude/collegamento.rs` (e `con_claude.rs`), `src/finestra_impostazioni.rs`, `ui/impostazioni.slint`, `crates/assistente/src/sistema/` (via `ambiente_utente.rs` se non serve più), `CHANGELOG.md`, guida/README della sezione Claude se esiste
- Test: `installa.rs`, `con_claude`, `test_ui.rs`

**Interfaces:**
- `installa.rs`: ogni voce = `{"type":"command","command":"<percorso assoluto>\\isola-hook.exe","args":["evento"],"timeout":T}` più `"async": true` per `UserPromptSubmit`, `Notification`, `Stop`, `StopFailure`, `SessionEnd`; `PreToolUse` sincrono `timeout: 5`; `PermissionRequest` sincrono `timeout: 120`; mai `onFailure`, mai `headers`/`allowedEnvVars`. `statusLine` = `{"type":"command","command":"<percorso corto con />/isola-hook.exe barra"}` (D5). `Piano { aiutante: PathBuf, comando_barra: Option<String> }`. `nostre()` riconosce le voci dal nome `isola-hook.exe` + `args[0] == "evento"` **e** le vecchie voci HTTP (`http://127.0.0.1:` + `/isola/v`), che `unisci`/`togli` rimuovono sempre (migrazione). Restano: unione che conserva tutto, riserva, scrittura atomica, anteprima, `barra_occupata`, `hook_disabilitati`; `fuori_lista` non serve più (niente URL) → togliere.
- Collegamento: «Collega» → anteprima → «Scrivi in settings.json» → scrive e avvia il tubo; «Scollega» → toglie le voci; **niente codice, niente variabile, niente «Rigenera»**. Prima di scrivere si **rilegge** il file e si rifà l'unione: se nel frattempo è cambiato rispetto all'anteprima, si mostra la nuova anteprima invece di scrivere. `controlla_all_avvio`: voci assenti, di un altro percorso o vecchie HTTP → voce *informazione* `claude.aggiorna`, mai scrittura.
- Interfaccia: sezione «Claude» senza «Rigenera il codice» né nota sui terminali da riaprire; nota PF14 aggiornata («Con isola chiusa Claude Code prosegue da solo: i permessi li decidi nel terminale»); fuori da Windows «Disponibile solo su Windows».

- [ ] **Step 1: test (falliscono)** — adattare quelli del Task 7 e aggiungere:

```rust
#[test] fn voci_command_in_forma_exec() // command = percorso assoluto dell'aiutante, args ["evento"], nessun headers/allowedEnvVars/url
#[test] fn async_solo_dove_serve() // async true esattamente per i 5 eventi; PreToolUse e PermissionRequest senza async; timeout 5 e 120
#[test] fn vecchie_voci_http_tolte() // file con le voci HTTP del Task 7 → unisci le toglie e mette le nuove; togli lascia il file originale altrui
#[test] fn file_cambiato_dopo_l_anteprima() // anteprima, poi il file cambia → conferma non scrive e restituisce la nuova anteprima
#[test] fn nessun_segreto_scritto() // dopo la conferma: nessuna voce nel portachiavi, nessuna variabile (AmbienteFinto vuoto)
```

- [ ] **Step 2–4:** FAIL → implementa → `cargo test --workspace` e clippy PASS.
- [ ] **Step 5: prova dal vivo in modalità di prova** (file finto), poi **con Paolo** sul vero `settings.json` (lui legge l'anteprima e conferma): stato dal vivo, permesso dalla pillola (Consenti/Nega), risposta nel terminale (cosa succede all'aiutante), isola chiusa (Claude prosegue senza errori visibili?), barra di stato in Git Bash.
- [ ] **Step 6:** commit `feat(assistente): collegamento sul canale protetto`.

### Task 8: API Claude in sola lettura

**Files:**
- Create: `crates/api-claude/` (`Cargo.toml`, `src/{lib,chiave,letture,client,modello,riepilogo}.rs`, `tests/fixture/*.json`, `examples/finta_api.rs`)
- Modify: `Cargo.toml` (membro), `crates/casa-core/src/diagnosi.rs` (`api_letture`, `api_errori`), `crates/assistente/Cargo.toml`, `src/ponte/con_claude.rs`, `ui/impostazioni.slint` (sezione «Claude»), `CHANGELOG.md`
- Test: nei moduli di `api-claude` (server finto `TcpListener` su 127.0.0.1 che serve le fixture e registra metodo e percorso), `src/ponte/con_claude.rs`

**Interfaces:**
- Produces:
  - `chiave.rs`: `pub struct ChiaveAdmin(String)`; `fn nuova(s: &str) -> Result<ChiaveAdmin, &'static str>` (spazi tolti; prefisso `sk-ant-admin`; solo ASCII stampabile; ≤ 256); `fn ultime_quattro(&self) -> String`; `Debug` → `ChiaveAdmin(…1a2b)`; nessun `Display`.
  - `letture.rs`: `pub enum Raggruppa { Nessuno, Modello, Chiave, Workspace }`; `pub enum Lettura { Organizzazione, Costo { da: DateTime<Utc>, a: DateTime<Utc>, per: Raggruppa, giorni: bool, pagina: Option<String> }, Uso { …stessi campi }, Chiavi { dopo: Option<String> }, Workspace { dopo: Option<String> }, Membri { dopo: Option<String> }, LimitiVelocita }`; `fn percorso(&self) -> &'static str`; `fn query(&self) -> Vec<(&'static str, String)>`; `fn analitica(&self) -> bool`; `pub const PERCORSI_AMMESSI: [&str; 7] = ["/v1/organizations/me", "/v1/organizations/analytics/cost", "/v1/organizations/analytics/usage", "/v1/organizations/api_keys", "/v1/organizations/workspaces", "/v1/organizations/users", "/v1/organizations/rate_limits"];` (`Workspace` con `include_default=true`; elenchi con `limit=100`; analitica con `time_bucket_width=1d` e `order_by=time` quando `giorni`).
  - `client.rs`: `pub struct ClientApi`; `pub fn anthropic(chiave: ChiaveAdmin) -> ClientApi` (base fissa `https://api.anthropic.com`, solo HTTPS, nessun reindirizzamento, timeout 20 s); `pub fn finto(base: &str, chiave_finta: &str) -> Result<ClientApi, String>` (solo `http://127.0.0.1:<porta>` e chiave che inizia per `finta-`); `pub fn leggi(&self, l: &Lettura) -> Result<String, ErroreApi>` — **l'unico metodo di rete, sempre `GET`**; intestazioni `x-api-key`, `anthropic-version: 2026-10-21`, `anthropic-beta: analytics-usage-cost-2026-08-24` solo se `analitica()`, `user-agent: ItsyHome-Assistente`. `pub enum ErroreApi { ChiaveNonValida, PermessoNegato, TroppeRichieste, Rete, Servizio(u16), Formato }` con `Display` in italiano che non contiene mai la chiave.
  - `modello.rs`: strutture `serde` con i soli campi dei fatti verificati; `pub struct Importo { pub millesimi: i128 }` (milionesimi di centesimo) con `pub fn importo_da(s: &str) -> Option<Importo>` e `pub fn in_dollari(&self) -> String` («412,80 $», arrotondamento al centesimo).
  - `riepilogo.rs`: `pub struct Riepilogo { pub aggiornato_a: Option<DateTime<Utc>>, pub spesa_mese: Option<Importo>, pub spesa_ieri: Option<Importo>, pub giorni: Vec<(NaiveDate, Importo)>, pub per_modello: Vec<(String, Importo)>, pub per_chiave: Vec<(String, Importo, Option<u64>)>, pub per_workspace: Vec<(String, Importo)>, pub token: Option<Token>, pub chiamate_mese: Option<u64>, pub chiavi: Vec<InfoChiave>, pub workspace: Vec<InfoWorkspace>, pub membri: Vec<InfoMembro>, pub limiti: Vec<GruppoLimiti>, pub non_disponibili: Vec<Scheda> }`; `pub enum Scheda { Spesa, Token, Chiavi, WorkspaceMembri, Limiti }`; `pub fn aggiorna(c: &ClientApi, adesso: DateTime<Utc>) -> Riepilogo` (sequenza chiusa di letture; mese UTC dal primo del mese all'ora intera di `adesso`, al massimo 31 giorni; giorni = gli 8 giorni prima di oggi; pagine seguite al massimo 10 volte; un errore in una lettura mette la sua scheda in `non_disponibili` e le altre proseguono; `data` vuota o `data_refreshed_at` nullo → `None`, mai zero); `pub fn spesa_insolita(r: &Riepilogo, oggi: NaiveDate) -> bool` (ieri > 3 × media dei 7 giorni prima, solo se `aggiornato_a` ≥ fine di ieri, se ci sono 7 giorni e la media > 0).
  - Assistente: chiave nel portachiavi `isola:claude-admin`; Impostazioni › Claude: campo mascherato «Chiave da amministratore», «Salva» (dopo: «•••• 1a2b» e «Togli»), «Prova» (una sola `Lettura::Organizzazione`: «Chiave valida» / frase di `ErroreApi`), guida in una riga «Crea una chiave da amministratore con scadenza nella Console (Impostazioni › Admin keys)». Lettura in un thread `api-claude` creato per ogni lettura (client creato e lasciato andare), all'avvio dopo 60 s e poi `slint::Timer` di **3 h**, solo con la chiave presente; il riepilogo arriva con `invoke_from_event_loop`. In prova: `ITSYHOME_API_CLAUDE_BASE` e `ITSYHOME_API_CLAUDE_CHIAVE` (solo con `ITSYHOME_ASSISTENTE_DIR`) → `ClientApi::finto`; la chiave del portachiavi va **solo** con `ClientApi::anthropic`.

- [ ] **Step 0:** scegliere la versione di `ureq` 3 da docs.rs: rustls con ring (stesso `rustls 0.23` del lock), certificati del sistema come `casa-core` se la feature esiste, `https_only`, `max_redirects(0)`; MSRV ≤ 1.94. Annotare in `Cargo.toml` il perché di ogni feature. Se `ureq` porta un secondo rustls o un provider diverso, fermarsi e dirlo a Paolo.
- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn percorsi_solo_nell_elenco_chiuso() // per ogni variante di Lettura: percorso() ∈ PERCORSI_AMMESSI
#[test] fn client_fa_solo_get() // aggiorna() contro il server finto: ogni richiesta registrata è GET e il suo percorso ∈ PERCORSI_AMMESSI
#[test] fn sorgente_senza_scritture() // include_str!("client.rs") non contiene ".post(", ".put(", ".patch(", ".delete(", "Method::"
#[test] fn intestazioni_giuste() // x-api-key = chiave finta, anthropic-version 2026-10-21, anthropic-beta solo su /analytics/*
#[test] fn chiave_mai_in_debug_ne_errori() // format!("{:?}", chiave) e ogni ErroreApi.to_string() non contengono la chiave
#[test] fn finto_solo_locale_e_chiave_finta() // finto("https://api.anthropic.com", …) Err; finto("http://127.0.0.1:1", "sk-ant-admin01-x") Err
#[test] fn chiave_admin_valida() // "sk-ant-api03-…" Err; " sk-ant-admin01-abc \n" Ok; ultime_quattro "…abc"
#[test] fn importo_centesimi_sei_decimali() // "41280.000000" → "412,80 $"; "0.000000" → "0,00 $"; "abc" → None
#[test] fn risultato_vuoto_non_e_zero() // data [] e data_refreshed_at null → spesa_mese None, aggiornato_a None
#[test] fn mese_dal_primo_in_utc() // adesso 2026-11-17T10:43Z → starting_at 2026-11-01T00:00:00Z, ending_at 2026-11-17T10:00:00Z
#[test] fn pagine_seguite_al_massimo_dieci() // server con has_more sempre vero → 10 richieste, poi si ferma
#[test] fn una_scheda_403_non_ferma_le_altre() // /users 403 → non_disponibili == [WorkspaceMembri], spesa presente
#[test] fn errore_401_chiave_non_valida() // tutte 401 → Riepilogo vuoto, ErroreApi::ChiaveNonValida riportato
#[test] fn spesa_insolita_solo_con_dati_completi() // ieri 40 $, media 10 $, aggiornato_a dopo la mezzanotte UTC → vero; aggiornato_a prima → falso; solo 3 giorni → falso
#[test] fn chiamate_del_mese_per_chiave() // usage con group api_key_id e requests → per_chiave con Some(chiamate) (D3)
```

- [ ] **Step 2:** `cargo test -p api-claude` → FAIL.
- [ ] **Step 3:** implementa; fixture con id finti, nomi «Chiave di prova», `membro@example.com`.
- [ ] **Step 4:** `cargo test --workspace` e clippy → PASS.
- [ ] **Step 5: prova dal vivo:** `cargo run -p api-claude --example finta_api` + assistente in prova: Impostazioni › Claude con la chiave finta, «Prova», lettura che arriva. **Poi con Paolo** la chiave vera: «Prova», una lettura, confronto a occhio con la Console (Paolo legge, nulla nei log); `diagnosi.log` con soli contatori.
- [ ] **Step 6:** commit `feat(api-claude): consumi e dati dell'organizzazione in sola lettura` (con l'ok di Paolo).

### Task 9: Vista Claude nel pannello e icona nel dock

**Files:**
- Create: `crates/assistente/ui/viste/claude.slint`
- Modify: `src/registro.rs`, `src/pannello.rs`, `src/ponte/con_claude.rs`, `src/claude.rs`, `ui/pannello.slint`, `ui/tema.slint`, `CHANGELOG.md`
- Test: `src/registro.rs`, `src/claude.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `Sessioni` (Task 3), limiti (Task 6), `Riepilogo` (Task 8), anello dei quadranti (`ui/layout/quadranti.slint`, M2).
- Produces:
  - Registro: `VoceApp { id: "claude", nome: "Claude", icona: "claude" }` fra `casa` e `pc` (ordine della spec generale §3bis).
  - `claude.rs`: `pub fn righe_sessioni(s: &Sessioni, contesto: &HashMap<String, f32>, ora: Instant) -> (Vec<RigaSessione>, usize /*altre*/)` (al più 3 righe; `RigaSessione { id, cartella, stato_testo /* «Lavora», «Ti aspetta», «Fatto», «Errore» */, livello: Option<Livello>, strumento, durata, contesto: Option<String> }`); `pub fn testo_spesa(r: Option<&Riepilogo>, chiave: bool) -> (String, String)` (valore, «aggiornato alle 14:02 UTC» o «nessun dato»; senza chiave: «Aggiungi una chiave da amministratore nelle Impostazioni»).
  - Vista (800 × 300): a sinistra le righe (cartella, stato col colore del livello, strumento, durata, contesto %), «+N altre» se servono; a destra «Limite 5 ore» e «Limite settimanale» (anelli, «si azzera alle 14:30»), nascosti senza abbonamento con «Limiti non disponibili», sotto la spesa API del mese. Stati vuoti: scollegato → «Collega Claude Code dalle Impostazioni» con «Impostazioni»; nessuna sessione → «Nessuna sessione aperta». Tastiera: ↑/↓ fra le righe, Invio = `callback apri-sessione(string)` (lo usa il Task 10). Durata aggiornata ogni secondo solo a vista aperta con sessioni al lavoro (stesso timer di D2).

- [ ] **Step 1: test (falliscono)**

```rust
// registro.rs
#[test] fn app_disponibili_con_claude() // ids == ["panoramica","casa","claude","pc","collegamenti"] (aggiornare i due test esistenti)
// claude.rs
#[test] fn al_massimo_tre_righe() // 5 sessioni → 3 righe nell'ordine di urgenza, altre == 2
#[test] fn spesa_senza_dati_e_senza_chiave() // None con chiave → "nessun dato"; senza chiave → frase della guida
// test_ui.rs
#[test] fn vista_claude_stati_vuoti() // scollegato: testo e pulsante «Impostazioni»; nessuna sessione: «Nessuna sessione aperta»
#[test] fn anelli_nascosti_senza_abbonamento() // limiti None: nessun elemento «Limite 5 ore»
#[test] fn righe_da_tastiera() // ↓ due volte e Invio → apri-sessione con l'id della terza riga
```

- [ ] **Step 2:** `cargo test -p assistente` → FAIL.
- [ ] **Step 3:** implementa.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: prova dal vivo:** con `sessioni_finte` (Task 4) e `finta_api` (Task 8): vista con 0, 1, 3 e 5 sessioni; Narrator; scorrimento dal dock (M2); foto di zona a 100 % e 175 %.
- [ ] **Step 6:** commit `feat(assistente): vista Claude nel pannello` (con l'ok di Paolo).

### Task 10: Porta davanti il terminale della sessione

**Files:**
- Create: `crates/assistente/src/sistema/terminale.rs`
- Modify: `crates/assistente/Cargo.toml` (windows-sys `Win32_NetworkManagement_IpHelper`, `Win32_Networking_WinSock`, `Win32_System_Diagnostics_ToolHelp`), `src/sistema/mod.rs`, `src/sistema/primo_piano.rs` (ultimo terminale in primo piano), `src/ponte/con_claude.rs`, `src/ponte.rs` (`azione` col pulsante `PULSANTE_TERMINALE`), `CHANGELOG.md`
- Test: `src/sistema/terminale.rs`

**Interfaces:**
- Consumes: `Arrivo.porta_mittente` (Task 2), `Sessioni::imposta_pid` (Task 3), `apri-sessione` (Task 9), esiti P7 dello spike.
- Produces:
  - Puro: `pub struct Processo { pub pid: u32, pub genitore: u32, pub exe: String }`; `pub fn catena(pid: u32, processi: &[Processo]) -> Vec<&Processo>` (al più 16 passi, si ferma ai cicli); `pub fn scegli_finestra(catena: &[&Processo], finestre: &[(u32 /*pid*/, isize /*hwnd*/)]) -> Option<isize>` (il primo antenato, dal più vicino, con una finestra visibile di primo livello); `pub const TERMINALI: [&str; 9] = ["WindowsTerminal.exe","OpenConsole.exe","conhost.exe","powershell.exe","pwsh.exe","cmd.exe","wezterm-gui.exe","alacritty.exe","Code.exe"];` `pub fn e_terminale(exe: &str) -> bool`.
  - Windows: `pub fn pid_da_porta(porta_mittente: u16, porta_ascolto: u16) -> Option<u32>` (`GetExtendedTcpTable(AF_INET, TCP_TABLE_OWNER_PID_CONNECTIONS)`, porte in ordine di rete); `pub fn processi() -> Vec<Processo>` (`CreateToolhelp32Snapshot`); `pub fn finestre_visibili() -> Vec<(u32, isize)>` (`EnumWindows`, `IsWindowVisible`, senza proprietario); `pub fn porta_davanti(hwnd: isize)` (`IsIconic` → `ShowWindow(SW_RESTORE)`, poi `SetForegroundWindow`: lo si chiama subito dopo il clic di Paolo, quando Windows lo permette). Il PID si cerca sul thread della connessione al **primo** evento di ogni sessione e arriva insieme all'`EventoHook`.
  - Ripiego: `primo_piano.rs` ricorda l'ultima finestra in primo piano il cui exe è un terminale; senza PID o finestra, clic → quella, e la riga della vista dice «ultimo terminale usato». Windows Terminal: si porta davanti la finestra, **non la scheda** (lo si dichiara). Fuori da Windows il clic apre la voce nel pannello.

- [ ] **Step 0:** leggere su learn.microsoft.com `GetExtendedTcpTable` (`MIB_TCPROW_OWNER_PID`, porte in ordine di rete), `CreateToolhelp32Snapshot`/`Process32FirstW`, `SetForegroundWindow` (regole del primo piano) e `AllowSetForegroundWindow`; annotare in testa al modulo.
- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn catena_dei_genitori() // claude.exe → pwsh.exe → WindowsTerminal.exe → explorer.exe; ciclo pid→pid → si ferma
#[test] fn finestra_del_primo_antenato_con_finestra() // pwsh senza finestra, WindowsTerminal con hwnd 42 → Some(42)
#[test] fn nessuna_finestra_nessuna_scelta() // → None
#[test] fn terminali_riconosciuti() // "WindowsTerminal.exe" vero (anche in minuscolo), "notepad.exe" falso
#[cfg(windows)] #[test] fn il_proprio_pid_dalla_porta() // TcpListener + TcpStream nel test: pid_da_porta(porta del client, porta del listener) == process::id()
```

- [ ] **Step 2:** `cargo test -p assistente terminale` → FAIL.
- [ ] **Step 3:** implementa.
- [ ] **Step 4:** `cargo test --workspace` → PASS (su Windows anche `il_proprio_pid_dalla_porta`, in CI).
- [ ] **Step 5: prova dal vivo con Paolo:** due sessioni in due finestre (Windows Terminal e PowerShell da solo), clic su *fatto* e su *ti aspetta* dalla vista, dal pannello e dalla voce: arriva davanti la finestra giusta; finestra ridotta a icona → ripristinata. Solo clic di Paolo, nessun input sintetico su finestre altrui.
- [ ] **Step 6:** commit `feat(assistente): clic su una sessione porta davanti il suo terminale` (con l'ok di Paolo).

### Task 11: Dettaglio API nella finestra di gestione

**Files:**
- Create: `crates/gestione/ui/schede.slint`
- Modify: `crates/gestione-protocollo/src/lib.rs`, `crates/gestione/src/main.rs`, `crates/assistente/src/gestione.rs` (`apri_schede`), `src/claude.rs`, `src/ponte/con_claude.rs`, `ui/viste/claude.slint` («Dettagli»), `src/sistema/browser.rs` (pagine della Console), `CHANGELOG.md`
- Test: `crates/gestione-protocollo/src/lib.rs`, `crates/gestione/src/main.rs`, `src/claude.rs`

**Interfaces:**
- Consumes: `Riepilogo`, `Scheda` (Task 8), `Gestione` (M1).
- Produces:
  - Protocollo: `Messaggio::ApriSchede { titolo: String, schede: Vec<SchedaDati> }`; `pub struct SchedaDati { pub nome: String, pub nota: String, pub colonne: Vec<String>, pub righe: Vec<Vec<String>>, pub azione_console: String }` (testi non fidati: la finestra li ripassa da `testo_pulito`; al più 200 righe e 6 colonne per scheda).
  - Gestione: con `ApriSchede` **nessun riquadro web** (`pub fn serve_il_web(m: &Messaggio) -> bool`: vero solo per `Apri`); schede Slint «Spesa», «Token», «Chiavi», «Workspace e membri», «Limiti» (`accessible-role: tab`), tabella scorrevole, «Apri la Console» → `Azione { id: azione_console }`, «Aggiorna» → `Azione { id: "aggiorna" }`.
  - `claude.rs`: `pub fn schede(r: &Riepilogo, adesso_locale: &str) -> Vec<SchedaDati>` (nota «Aggiornato a … · dati con 4–24 ore di ritardo» o «Non disponibile con questa chiave»; Chiavi: nome, stato in italiano, creata, scade, suggerimento `partial_key_hint`, chiamate del mese; mai il valore della chiave).
  - `browser.rs`: `pub enum PaginaConsole { Spesa, Uso, Limiti, ChiaviAdmin, Organizzazione }` con URL fissi `https://platform.claude.com/cost`, `/usage`, `/usage/limits`, `/settings/admin-keys`, `/settings/organization`; l'assistente accetta solo gli id `console:spesa|uso|limiti|chiavi|organizzazione` e `aggiorna`, ogni altro id si ignora.

- [ ] **Step 0:** verificare che `platform.claude.com/settings/keys` e `/settings/members` esistano (dalla Console o dalla doc); se sì aggiungerli a `PaginaConsole`, se no le schede Chiavi e Membri usano `Organizzazione`.
- [ ] **Step 1: test (falliscono)**

```rust
// gestione-protocollo
#[test] fn apri_schede_andata_e_ritorno() // riga_da_messaggio → messaggio_da_riga identico
// gestione
#[test] fn apri_schede_non_chiede_il_web() // serve_il_web(ApriSchede) falso, serve_il_web(Apri) vero
// claude.rs
#[test] fn schede_dal_riepilogo() // fixture del Task 8: 5 schede nell'ordine; la scheda non disponibile ha la nota; nessuna cella contiene "sk-ant-"
#[test] fn azioni_della_console_chiuse() // "console:spesa" → Spesa; "console:altro" e "https://…" → nulla
```

- [ ] **Step 2:** `cargo test --workspace` → FAIL.
- [ ] **Step 3:** implementa.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: prova dal vivo** con `finta_api`: «Dettagli» dalla vista, schede da tastiera, «Apri la Console» apre il browser sulla pagina giusta, «Aggiorna»; misura: finestra aperta e chiusa 10 volte, processo raccolto e RAM dell'assistente entro +2 MB.
- [ ] **Step 6:** commit `feat(gestione): dettaglio delle API Claude in schede` (con l'ok di Paolo).

### Task 12: Quadranti e avvisi (limiti e spesa)

**Files:**
- Modify: `crates/assistente/src/lati.rs`, `src/ponte.rs` (`quadranti_ui`), `src/claude.rs`, `src/ponte/con_claude.rs`, `ui/impostazioni.slint` (sezione «Claude»: soglie), `CHANGELOG.md`
- Test: `src/lati.rs`, `src/claude.rs`, `src/ponte/con_claude.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `Lato`, `scelte`, `scala`, `quadranti_ui` (M2 Task 4); limiti (Task 6); `Riepilogo`, `spesa_insolita` (Task 8); `ConfigClaude` (Task 4).
- Produces:
  - `Lato::Limite5h`, `Lato::Limite7g`, `Lato::SpesaApi` (stringhe `"limite_5h"`, `"limite_7g"`, `"spesa_api"`); solo per i quadranti («Limite 5 ore», «Limite settimanale», «Spesa API del mese»); senza dato il quadrante mostra «—», mai «0%» o «0 $». `scala`: limiti 0..100; spesa 0..`soglia_spesa` se c'è, altrimenti il massimo utente o 0..(valore × 2) come le unità ignote di M2.
  - `claude.rs`: `pub enum Finestra { CinqueOre, SetteGiorni }`; `pub fn avviso_limite(f: Finestra, l: &Limite, soglia: u8, avvisato: Option<i64>, ora: i64) -> Option<Voce>`: `percentuale >= soglia`, `azzera_alle > ora` e `avvisato != Some(azzera_alle)` → *attenzione* `claude.limite.5h`/`.7g`, titolo «Limite 5 ore all'82%», dettaglio «Si azzera alle 14:30.», pulsante «Ho capito»; il ponte salva `azzera_alle` in `limiti_avvisati` (una volta per finestra, anche dopo un riavvio) e toglie la voce quando la finestra si azzera. `pub fn avviso_spesa(spesa: &Importo, soglia: f64, mese: &str, gia: Option<u8>) -> Option<(u8, Voce)>`: 80 % e 100 % (*attenzione* `claude.spesa`, «Spesa API al 100% della soglia (250,00 $)»), una volta per gradino e per mese (`spesa_avvisata`). `pub fn voce_insolita(ieri: &Importo, giorno: NaiveDate) -> Voce` (*informazione* `claude.spesa.insolita`, «Spesa API insolita ieri: 40,00 $»), una volta per giorno (`insolita_avvisata`).
  - Impostazioni › Claude: «Avvisa al … % del limite» (campo 50..100, predefinito 80) e «Soglia di spesa mensile ($)» (vuota = nessuna).

- [ ] **Step 1: test (falliscono)**

```rust
// lati.rs
#[test] fn fonti_claude_andata_e_ritorno() // "limite_5h", "limite_7g", "spesa_api" ↔ varianti; non fra le scelte dei lati della pillola
#[test] fn quadrante_senza_dato_trattino() // limiti None → testo "—"
// claude.rs
#[test] fn avviso_limite_una_volta_per_finestra() // 82 ≥ 80 → Some; avvisato == azzera_alle → None; nuova finestra (azzera_alle diverso) → Some
#[test] fn avviso_limite_scaduto_niente() // azzera_alle <= ora → None
#[test] fn spesa_ottanta_e_cento() // 200/250 → (80, voce); già 80 → None; 250/250 con già 80 → (100, voce); nessuna soglia → mai
// ponte/con_claude.rs
#[test] fn limiti_avvisati_salvati_in_config() // dopo l'avviso config.claude.limiti_avvisati["5h"] == azzera_alle
// test_ui.rs
#[test] fn soglie_nelle_impostazioni() // campo «Avvisa al» con 80; testo "abc" rifiutato senza salvare
```

- [ ] **Step 2:** `cargo test -p assistente` → FAIL.
- [ ] **Step 3:** implementa.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: prova dal vivo:** quadranti con le tre fonti (barra dallo spike o da Claude vero, spesa da `finta_api`); soglia abbassata a 10 % per vedere l'avviso una volta sola; soglia di spesa bassa con `finta_api` → 80 % e 100 %.
- [ ] **Step 6:** commit `feat(assistente): limiti e spesa nei quadranti, avvisi a soglia` (con l'ok di Paolo).

### Task 13: Misure, documentazione e prova finale con Paolo

**Files:**
- Modify: `CHANGELOG.md`, `README` della v3 se esiste la sezione delle funzioni (guida «Collega Claude Code», chiave con scadenza, terminali da riaprire, «Solo su Windows»: variabile d'ambiente e porta davanti)

**Interfaces:**
- Consumes: tutto; `sessioni_finte` (Task 4), `finta_api` (Task 8).
- Produces: solo numeri e documentazione.

- [ ] **Step 1: misure** con `tools/misura.ps1` (60 s per stato, assistente in release, casa vera connessa come in M2), numeri nel CHANGELOG:
  - collegato, nessuna sessione: CPU e RAM come M2 (thread fermo su `accept`);
  - `sessioni_finte` con 3 sessioni: CPU media ≤ 1 % (budget «pensa»), RAM senza crescita fra il secondo 10 e il 60;
  - dopo `sessioni_finte`: ritorno a riposo entro +2 MB;
  - una lettura API con `finta_api`: RAM entro +2 MB dopo la lettura (client rilasciato);
  - `--barra-stato`: mediana di 100 esecuzioni < 50 ms.
- [ ] **Step 2:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, CI verde su windows/macos/ubuntu.
- [ ] **Step 3: revisione** (agenti su Opus): sicurezza di `claude-hook` e `api-claude` (ascoltatore, unione di `settings.json`, chiave), UI/UX della vista, della pillola media e della sezione «Claude».
- [ ] **Step 4: prova finale con Paolo** sui Review Focus 1–5 e su D1–D5: collegamento vero con anteprima, tre sessioni vere, un permesso vero dalla pillola, limiti dell'abbonamento, chiave vera, «Scollega» che lascia `settings.json` com'era (confronto con la copia `.isola-bak`).
- [ ] **Step 5:** commit `docs(assistente): misure e guida di M3` (con l'ok di Paolo).
