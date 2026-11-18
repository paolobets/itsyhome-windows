# Assistente "isola" — Giro grafico dopo M3 — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** un'interfaccia chiara e semplice che mostri tutte le informazioni a colpo d'occhio — layout moderno, strutturato, ottimizzato per le dimensioni fisse del pannello (800×300) e per le funzioni — e in cui ogni conteggio o avviso porti al dettaglio giusto con un clic.

**Architecture:** solo interfaccia (Slint) e la colla Rust che la riempie (`src/ponte*.rs`, `src/claude.rs`, `src/lati.rs`, crate `gestione`). Nessun crate nuovo, nessuna dipendenza nuova, nessun timer nuovo a riposo.

**Tech Stack:** Rust 2021, Slint `=1.18.1`, `windows-sys 0.52`.

**Spec:** non c'è una spec a parte: valgono, in quest'ordine, (1) le decisioni di Paolo in `.superpowers/ux/decisioni.md` (1–14, la 14 è di questo giro, con il **principio guida**), (2) i due rapporti dello specialista `.superpowers/ux/rapporto-grafico-1.md` (punti 1–25, i 1–10 e il 20 sono già fatti in a2cb97f) e `.superpowers/ux/rapporto-grafico-claude.md` (punti 31–58), (3) le spec `docs/superpowers/specs/2026-11-13-assistente-isola-design.md` (§3bis, §4 leggerezza, §6 sicurezza) e `2026-11-17-assistente-m3-claude-design.md`. Ogni punto citato qui sotto come «R1-n» o «RC-n» si legge **nel rapporto**, che ha file:riga, cosa e perché; il piano dice solo in quale task cade e cosa ha deciso Paolo dove il rapporto lascia una scelta.

## Global Constraints

- Testi per l'utente in italiano, come il resto; verbo in testa sui pulsanti.
- **Leggerezza** (spec §4): ogni proprietà Slint si riscrive solo se cambia; niente timer nuovi; quelli che servono sono a scatto singolo o accesi solo mentre una vista è aperta. RAM a riposo non deve crescere (misura alla fine).
- **Sicurezza**: i permessi Claude restano come sono (fuoco iniziale su «Nega», Ctrl+Invio, armamento 600 ms, Nega/Consenti neutri). La testata «CONTENUTO ESTERNO» della gestione resta.
- **Casa vera mai toccata nelle prove**: azioni su Home Assistant («Spegni tutte», interruttori) solo nei test con dati finti; nessuna prova contro l'HA vero.
- I test esistenti restano verdi (`cargo test --workspace`); i test che codificano un testo o una misura cambiati si aggiornano, non si cancellano. `cargo clippy --workspace --all-targets -- -D warnings` pulito; il codice solo-Windows non lascia codice morto sugli altri sistemi (la CI gira su Windows, Linux e macOS).
- Compilare solo da PowerShell con `$env:CARGO_TARGET_DIR='C:\Work\Sviluppo\itsyhome-windows\target\agente'`; mai scrivere sorgenti da PowerShell 5.1.
- Foto «prima/dopo» (regole del rapporto: istanza di prova da copia dell'exe, `ITSYHOME_ASSISTENTE_DIR` su cartella finta, solo UI Automation/PostMessage sul proprio PID, mai SendInput/SetCursorPos/mouse_event, PrintWindow della propria finestra, fermare i propri processi per PID dopo averne verificato il percorso). L'isola vera di Paolo (in `%LOCALAPPDATA%\isola-m3-prova`) non si tocca.

## Review Focus

1. **Testi lunghi** (nomi di stanze, luci, cartelle, modelli): con i layout nuovi nulla esce dalla card né si sovrappone; si taglia con «…» solo dove il rapporto lo ammette.
2. **Molti elementi** (10 sessioni Claude, 15 luci accese, 7 collegamenti, 5 voci in attesa): scorrimento o «+N» come deciso, mai elementi irraggiungibili da tastiera.
3. **Zero elementi** (nessuna luce accesa dopo «Spegni tutte», nessuna sessione, nessuna chiave): stato vuoto con un'azione, e la vista filtrata torna sensata.
4. **Fuoco da tastiera** dopo ogni aggiornamento che toglie o riordina righe (vista Claude, Casa filtrata, schede): l'anello non sparisce e il primo tasto non va perso.
5. **Scala 100 % e 175 %**: nessun valore fisso che funzioni solo a una delle due.

---

### Task 1: Il dettaglio a un clic

**Files:** `crates/assistente/src/pannello.rs` (`Apertura::apri`), `src/ponte.rs` (`vista_richiesta`, `pannello_aperto`, riga di stato della Casa ~`:405-425`), `src/ponte/con_claude.rs` (`apri_la_vista_claude`), `ui/viste/casa.slint`, `ui/pannello.slint`, test in `src/test_ui.rs` e nei moduli.

Decisioni di Paolo (decisioni.md §14):
- **Pillola → vista Claude.** Clic sulla pillola mentre mostra l'attività Claude («Claude ×3 · 3:12») **oppure** una voce allargata di fonte Claude che **non** è un permesso → il pannello si apre sulla vista Claude. Un permesso allargato resta com'è oggi (si apre espanso in «In attesa»). Ogni altro caso resta com'è (Panoramica). Il meccanismo `vista_richiesta` oggi è `cfg(any(not(windows), test))`: va reso valido su tutti i sistemi.
- **«N luci accese» → Casa filtrata.** Il clic sulla riga/quadrante che dice «N luci accese» apre la vista Casa filtrata sulle **luci accese**: intestazione «Luci accese (N)», un interruttore per ciascuna, «Spegni tutte» in testata, un filtro per tornare all'elenco completo. Quando l'ultima si spegne: stato vuoto «Nessuna luce accesa» con «Mostra tutta la casa». La vista filtrata segue gli aggiornamenti di stato (una luce spenta altrove sparisce; il fuoco va alla riga più vicina). «Spegni tutte» usa lo stesso percorso di comando degli interruttori esistenti (nessuna API nuova verso HA), senza conferma (spegnere luci non è un'azione delicata secondo la decisione 8), con pulsante neutro, non tinto.

- [ ] Test (falliscono prima): apertura con attività Claude → vista "claude"; con voce Claude non permesso → "claude"; con permesso Claude allargato → panoramica + voce espansa; senza Claude → panoramica. Casa: modello filtrato contiene solo le luci accese, si aggiorna allo spegnimento, stato vuoto a zero; «Spegni tutte» manda un comando per ogni luce accesa (contro il finto).
- [ ] Implementazione, test verdi, clippy pulito.
- [ ] Foto dopo: pillola con attività → pannello su Claude; Casa filtrata (con HA finto se disponibile negli strumenti di prova esistenti, altrimenti solo test_ui).
- [ ] Commit.

### Task 2: Panoramica e pannello

**Files:** `ui/layout/quadranti.slint`, `ui/layout/orologio.slint`, `ui/attesa.slint`, `ui/pannello.slint`, `src/lati.rs`, `src/ponte/con_claude.rs`, `src/claude.rs`.

Punti: **R1-11** (quadranti allineati a «In attesa»), **R1-12** («‹ 1 di 3 › Riduci» con forma di pulsante), **R1-13** (Casa non configurata = invito alle Impostazioni), **R1-14** (ora piccola nascosta nel layout Orologio), **R1-22** (In attesa con ≥3 voci: 2 card larghe + «+N», deciso da Paolo), **RC-36** (pillola senza «·» davanti alla durata), **RC-38** (un nome per limite e spesa: «Limite 7 giorni», «Spesa API del mese» ovunque), **RC-44** (colore caldo alla soglia dell'utente, non all'85 % fisso, anche nei quadranti), **RC-50** (ritmo dei quadranti), **RC-51** (quadrante spesa nel primo minuto; soglia senza centesimi se intera).

- [ ] Test per R1-22 («+N» con 3, 4, 5 voci; fuoco raggiungibile su «+N»), RC-36, RC-38, RC-44 (soglia 70 → caldo a 71), RC-51.
- [ ] Implementazione; test di misura aggiornati.
- [ ] Foto dopo delle foto 01, 02, 03, 08 del rapporto 1 e 49, 50 del rapporto Claude.
- [ ] Commit.

### Task 3: Viste PC e Casa — ritmo e stile delle etichette

**Files:** `ui/viste/pc.slint`, `ui/viste/casa.slint`, `ui/componenti.slint`.

Punti: **R1-15** (un solo stile di etichetta nella vista PC; pesi 400/500), **R1-16** (ritmo 4/8 nelle card Casa e PC).

- [ ] Test di misura aggiornati/aggiunti per i valori nuovi.
- [ ] Foto dopo (04, e Casa con dispositivi se ottenibile con dati finti).
- [ ] Commit.

### Task 4: Vista Claude

**Files:** `ui/viste/claude.slint`, `src/claude.rs`, `src/ponte/con_claude.rs`, `src/ponte/con_claude/api.rs`, configurazione/stato persistente esistente (per RC-56).

Punti: **RC-32** (giorno nell'ora dei dati), **RC-35** (card spesa con margini veri), **RC-37** (niente spazio doppio nelle righe «Fatto»), **RC-48** (un raggio solo), **RC-49** (percentuale che respira nell'anello), e le decisioni di Paolo:
- **RC-52**: la colonna delle sessioni **scorre** (tutte visibili; rotella e frecce; la riga col fuoco resta in vista). Via «+N altre».
- **RC-53**: ordine **fermo finché il pannello è aperto**; si decide all'apertura (per urgenza come oggi); le sessioni nuove in fondo; una sessione chiusa sparisce e il fuoco va alla più vicina.
- **RC-55**: «Contesto N%» caldo oltre l'**80 %**.
- **RC-56**: dopo un riavvio i limiti mostrano l'**ultimo valore salvato**, attenuato, con «visto alle HH:MM» (o «ieri alle…», stessa funzione di RC-32), finché non arriva un dato nuovo; se `resets_at` è passato non si mostra. Il salvataggio usa il file di stato/configurazione già esistente dell'assistente (niente file nuovi se uno adatto c'è), scritto solo quando il valore cambia e al massimo una volta al minuto.
- **RC-57**: «ultimo terminale usato» → un'icona con suggerimento (tooltip/nome accessibile), non il testo su ogni riga.

- [ ] Test: ordine fermo a pannello aperto con cambio di urgenza; sessione nuova in fondo; riordino alla riapertura; RC-56 (valore salvato mostrato attenuato, scaduto non mostrato, sostituito dal dato fresco); RC-55 soglia; RC-32 formati oggi/ieri/data.
- [ ] Implementazione, test verdi.
- [ ] Foto dopo delle 30, 37, 38, 51, 52 del rapporto Claude (con `sessioni_finte`, 8 sessioni per lo scorrimento).
- [ ] Commit.

### Task 5: Finestra di gestione

**Files:** `crates/gestione/ui/cornice.slint`, `crates/gestione/ui/schede.slint`, `crates/gestione/src/main.rs`, `crates/assistente/src/claude.rs` (costruzione delle schede).

Punti: **R1-19 lato gestione** (deciso: barra di Windows — Snap, ridimensiona, riduci — e lo stesso filo colorato del pannello sotto la barra; testata «CONTENUTO ESTERNO» resta sotto), **R1-24** (mail fedele al mittente; contorno e barra di scorrimento scuri), **RC-41** (il fuoco non sparisce mai: riga più vicina dopo un aggiornamento), **RC-42** (un solo anello), **RC-43** (numeri allineati a destra), **RC-45** (parole delle schede), **RC-54** (Spesa a gruppi con intestazione: mese, per modello, per chiave, per workspace, ultimi giorni — al posto della lista piatta con «Voce» ripetuta).

- [ ] Test: RC-41 (fuoco sull'ultima riga, aggiornamento con una riga in meno → fuoco sulla nuova ultima), RC-43 (colonne numeriche marcate), RC-54 (gruppi e intestazioni dal modello).
- [ ] Verifica che la barra di Windows non rompa la scelta di sicurezza della cornice fidata (la pagina esterna non può disegnare sopra la testata).
- [ ] Foto dopo delle 20 (rapporto 1) e 41–48 (rapporto Claude).
- [ ] Commit.

### Task 6: Impostazioni

**Files:** `crates/assistente/ui/impostazioni.slint` (tutte le sezioni), `src/finestra_impostazioni*.rs` se serve, `src/ponte/con_claude/api.rs`.

Punti: **R1-18** (layout veri — `VerticalLayout`/`GridLayout` — al posto delle coordinate fisse, **prima** di tutto il resto di questo task), **R1-17** (anello del fuoco dei campi), **R1-19 lato Impostazioni** (stesso filo colorato del pannello sotto la barra di Windows, come la gestione), **R1-25** (badge Slint più sobrio **solo** se la licenza di Slint in uso lo ammette: verificare sul testo della licenza/FAQ ufficiali e citarlo nel commento; se non è chiaro, non toccarlo e scriverlo nel rapporto), **RC-31, 33, 34, 39, 40, 46, 47** (sezione Claude: pulsanti dell'anteprima visibili, conferma su «Togli», esiti che si cancellano, descrizioni a capo, percorso leggibile, tre blocchi chiari, etichette dei campi).

- [ ] Riscrittura a layout veri con le foto 10–16 e 31–35, 54–58 come riferimento «non deve cambiare niente che non sia voluto»; test di misura aggiornati.
- [ ] Test: conferma «Togli» (Annulla col fuoco); esiti azzerati a collega/scollega/lettura.
- [ ] Foto dopo di tutte le sezioni.
- [ ] Commit.

### Task 7: Parole e collegamenti

**Files:** testi in `ui/**/*.slint` e `src/**/*.rs` dell'assistente, `ui/viste/collegamenti.slint`, `ui/impostazioni.slint`.

Punti: **R1-21** (deciso: «Casa» ovunque per l'utente; «Casa (Home Assistant)» solo nelle Impostazioni dove si sceglie a cosa collegarsi; «Aggiungi una casa»; «ambiente» sparisce dai testi per l'utente — gli identificativi interni restano), **R1-23** (deciso: iniziale del nome in un cerchio colorato per ogni collegamento, colore stabile derivato dal nome dalla tavolozza esistente; via «COLLEGAMENTI» e «Si aprono nel browser» dalla testata).

- [ ] Test: nessun testo per l'utente contiene «ambiente» (cerca nelle stringhe Slint e Rust per l'utente); colore dell'iniziale stabile per lo stesso nome.
- [ ] Foto dopo 05, 09, 12, 16.
- [ ] Commit.

### Task 8: Menu «⋯» scuro in Slint

**Files:** `crates/assistente/src/sistema/menu.rs`, `ui/pannello.slint` (o un componente nuovo in `ui/`), `src/pannello.rs`.

Decisione di Paolo (**RC-58**): il menu «⋯» non è più quello di Windows ma un menu Slint scuro, stesso stile del pannello: stesse voci di oggi nello stesso ordine; tastiera completa (frecce, Invio, Esc che chiude il menu e non il pannello, fuoco che torna a «⋯»); si chiude al clic fuori; nessuna finestra nuova se un popup Slint dentro il pannello basta. Il menu della tray resta quello di Windows.

- [ ] Test: voci e ordine uguali a oggi; Esc chiude solo il menu; ogni voce chiama la stessa azione di prima.
- [ ] Foto dopo del menu aperto.
- [ ] Commit.

### Chiusura

- [ ] Revisione finale dell'intero ramo (Opus) con le foto prima/dopo affiancate.
- [ ] Misura RAM/CPU a riposo dell'istanza di prova prima e dopo il giro (non deve crescere).
- [ ] CHANGELOG (sezione «Giro grafico») e README se cambiano nomi visibili.
- [ ] Prova dal vivo con Paolo (una sola riapertura di isola).
- [ ] Pulizia del disco dopo le compilazioni.
