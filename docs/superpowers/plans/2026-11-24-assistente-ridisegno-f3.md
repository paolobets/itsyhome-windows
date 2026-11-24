# Assistente "isola" — Ridisegno, fetta F3 (Comandi e Misure: pagine Casa, Collegamenti, PC) — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Piano (24/11/2026).** Le quattro domande in fondo hanno avuto risposta da Paolo il 24/11: ha scelto la proposta raccomandata in tutti e quattro i casi (A, A, A, C). Il conteggio dei lanci resta un dato interno, senza cifra sulle card: è una scelta del coordinatore.

**Goal:** Casa e Collegamenti passano all'impianto **Comandi** (filtri a sinistra, griglia di card a misura fissa: la card è l'interruttore, sola lettura distinta per forma, scene, card «lancio»), il PC all'impianto **Misure** (quattro misure mono 28 con barra, righe con 0–3 comandi in linea); il contratto riceve il campo **`r1`**, che dà il titolo alla testata di ogni pagina; prima si misura il pannello fermo (m3 di F2) e si allinea la carta Agenti alla pagina (m2).

**Architecture:** come in F2, **funzioni pure sopra `Ponte`**: `src/moduli/pagina_casa.rs`, `pagina_collegamenti.rs` e `pagina_pc.rs` calcolano filtri, card, misure, righe, sintesi e stato della pagina dai dati che il ponte ha già (menu di HA con lo stato ottimistico, configurazione dei collegamenti, campioni del PC, radio, volume, cuffie); un file nuovo del ponte, `src/ponte/con_pagine.rs`, li scrive con `aggiorna_modello` **solo a pagina aperta e solo se cambiano**. In Slint i modelli sono **dell'impianto, non del modulo** (`comandi-*` condivisi da Casa e Collegamenti, `misure-*` del PC: spec §5.10, 08 §2.2); gli impianti stanno in `ui/impianti/comandi.slint` e `ui/impianti/misure.slint`, la testata resta `TestataPagina` (`ui/guscio/pagina.slint`), allargata all'azione primaria con l'icona. La tastiera resta di `tasti` nel pannello, per indice (`comandi-fuoco`, `misure-fuoco`), con una funzione `freccia-*`/`attiva-*` per impianto come `freccia-agenti` di F2. Spariscono `ui/viste/casa.slint`, `ui/viste/collegamenti.slint`, `ui/viste/pc.slint` e le proprietà del pannello che servivano solo a loro.

**Tech Stack:** Rust 2021, Slint `=1.18.1` (Skia software), `windows-sys 0.52`, chrono, ttf-parser (`moduli::misura`); strumenti di prova `i_slint_backend_testing` (`mock_elapsed_time`, `ElementHandle`); per le prove a mano `casa-core --example finto_ha`, `claude-hook --example sessioni_finte`, `tools\misura.ps1`, `tools\foto-zona.ps1`.

**Spec:** `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (vincolante; **§13 prevale**; contano §5.6 Casa, PC, Collegamenti, impianti Comandi e Misure e stati di pagina, §5.10 contratto e `r1`, §7, §8 riga F3 e note «F1 fatta» / «F2 fatta», §11). Si leggono con lei: `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` («Dopo F2», «Esito della revisione finale»), `.superpowers/ux/studio/11-spike.md`, `08-fattibilita-sistema.md` (§2.2 modelli degli impianti, §7.3 test da riscrivere: Casa e Collegamenti ≈ 17, PC ≈ 10), `05-brief-sistema.md` (R1: fatto ≤ 32, valore ≤ 8), `07-critica-sistema.md` §5.A (la testata usa il fatto R1); bozza `.superpowers/ux/studio/bozze/tc-isola.html` (`paginaCasaLuci()`, `paginaCasaStanza()`, `paginaPC()`, `filtri()`, `card()` in `tc-isola.js`; misure in `tc-isola.css`: `.sintesi`, `.filtri`/`.filtro`, `.dopo-filtri`, `.cards`/`.card`, `.scene`/`.chip`, `.misure`/`.misura`), schermate `bozze/screenshot/tc-casa-luci.png`, `tc-casa-stanza.png`, `tc-pc.png`. Stile del piano: `docs/superpowers/plans/2026-11-23-assistente-ridisegno-f2.md`.

## Rimandati portati in F3

| Punto | Da dove | Task |
|---|---|---|
| **m3**: CPU del pannello fermo +0,02 punti (due `Permesso` sempre vivi?) e pillola a riposo a schermo sbloccato mai misurata | F2, esito della revisione finale | 1 |
| **m2**: la carta Agenti dice «Tocca a te» con la capsula «Aspetta» mentre la pagina dice «1 ti aspetta / Chiede: …» (`moduli/agenti.rs:77`) | F2, esito della revisione finale | 2 |
| Campo **`r1`** del contratto (la sintesi di Agenti lo assorbe) | F2, «F2 fatta» | 2 |
| Dopo «Spegni tutte» il bottone resta fino alla conferma di HA | F1 Task 4 | 3 |
| `frase_breve` confronta stringhe; asserzione di `da_chiuso_non_si_ricalcola` che non può fallire | F1 Task 4 | 3 |
| Helper di prova `tasto`/`etichetta_col_fuoco` ripetuti; `Stile::Etichetta`↔`Tema.etichetta-*` legati solo da un commento; nome di carta/scheda coi token condivisi (`EtichettaMono`) | F2 Task 6; F1 Task 8 m6 e Task 2 | 4 |
| `!.tmp.exists()` che non può fallire; azione «Aggiungi» della carta senza test; ora del lancio presa da `con_claude::adesso_unix` | F1 Task 5 | 5 |
| `pc.slint` `to-uppercase()` senza nome per Narrator | F1 Task 10 | 7 (la vista sparisce; le etichette nuove hanno il nome intero) |
| Prove dal vivo rimaste da F2 e foto della Coda col permesso aperto | F2, «Da provare dal vivo» | 8 |

Non presi (restano nei rimandati, nessuna superficie di F3 li tocca): test del doppio clic «{n} avvisi ›», test della scala 0,96, animazioni a vuoto uscendo dalla Panoramica (F1 Task 9); taglio sotto i 3,6 px che `misura.rs` non vede nella riga di Agenti (F2 m1); pillola inerte (F4).

## Decisioni prese dal piano (dove spec e bozza lasciano la scelta)

- **File nuovi**: Rust `src/moduli/pagina_casa.rs`, `pagina_collegamenti.rs`, `pagina_pc.rs`, `src/ponte/con_pagine.rs`; Slint `ui/impianti/comandi.slint` (`ImpiantoComandi`, `FiltroComandi`, `CardComando`, `GeometriaComandi`), `ui/impianti/misure.slint` (`ImpiantoMisure`, `CartaMisura`, `RigaMisura`). `EtichettaMono` nasce in `ui/componenti.slint`. Spariscono `ui/viste/{casa,collegamenti,pc}.slint`, le strutture `PreferitoUi`, `StanzaUi` e `StatPcUi` (anche dall'`export` di `ui/interfaccia.slint` e di `pannello.slint`) e `stat_ui`; `CollegamentoUi` resta, perché lo usano le Impostazioni.
- **Niente array dentro le strutture** confrontate prima di scriverle (regola di F2): i gruppi di 08 §2.2 diventano modelli piatti. Comandi: `comandi-filtri`, `comandi-card`, `comandi-scene`. Misure: `misure-valori` (sempre **4 posti**, `presente` falso = posto vuoto) e `misure-righe`.
- **Un solo modello per impianto, condiviso**: Casa e Collegamenti scrivono gli stessi `comandi-*`; il ponte sa quale pagina li possiede (`Ponte::pagina_comandi: Cell<Option<&'static str>>`) e li riscrive interi al cambio di pagina.
- **`r1` = `R1 { fatto, valore }`** nel `Riassunto`. `fatto` ≤ 32 caratteri **e** ≤ `SPAZIO_R1` = 320 px in `Stile::Titolo` (la testata più piena, Agenti con due limiti, la spesa e «Dettagli», lascia al titolo circa 330 px). `valore` ≤ 8 caratteri; in F3 nessuna superficie lo mostra (lo leggeranno compatti e «Tutte le app» in F6), ma ogni scala lo riempie e il `Registro` lo controlla: Agenti il limite più alto («61 %»), Casa la temperatura di dentro («21,5°»), PC la CPU («12 %»), Collegamenti vuoto. `Riassunto::primo_avvio(motivo)` ha `r1.fatto = motivo` (mai «—»). La testata di una pagina mostra `r1.fatto` sul filtro iniziale; su una stanza mostra il nome della stanza.
- **m2**: con un permesso in attesa la carta Agenti è quella della bozza Griglia (`T.agentiAspetta` di `tc-isola.js`): tono Aspetta, titolo «1 ti aspetta» / «{n} ti aspettano» (lo stesso `r1.fatto` della pagina), riga «{nome}: {strumento}» (il nome si accorcia, `riga_con_nome`; per Narrator «{nome} chiede: {strumento}»), con più permessi «{n} permessi»; **nessuna azione** (la voce che ti aspetta ha già «Nega» e «Apri»; la carta apre la pagina Agenti). «Tocca a te» resta solo per la sessione ferma senza permesso.
- **Card a misura fissa** (bozza `.card`): 64 r16, spazio 8; colonne 3 con i filtri (griglia da x 188, larga 564) e 4 senza (Collegamenti, larga 752): la card è larga circa 182 in entrambi i casi, come la carta della Panoramica. Anatomia: riquadro icona 32 r10 (`g3`; acceso: bianco con icona nera), nome Inter 700 13 (il 600 della bozza diventa 700: tetto di due pesi), sotto la parola 12 `testo-2` e, dopo « · », il valore mono 12 (Slint non mescola caratteri in un `Text`: due `Text`). **Sola lettura**: nessun riempimento, solo il filo; col valore numerico il valore mono 18 è il testo principale e il nome sta sotto. **Lancio** (Collegamenti): il cerchio con l'iniziale di oggi al posto del riquadro, nome, indirizzo accorciato sotto. Tenue (non disponibile, casa scollegata) = colori con alfa 0,85 come oggi; errore = «Non riuscito» in `Tema.errore` (oggi `caldo-2`, che dal ridisegno vuol dire «al lavoro»).
- **La card è l'interruttore** (spec §5.6): `accessible-role: switch` con `accessible-checked` per luci e interruttori; scena e lancio `button`; sola lettura `text` col nome «{nome}, {valore}, sola lettura». Si comandano solo luci, interruttori (`switch`, `input_boolean`) e scene, **come oggi** (`comando_per`): serrature, allarme, tapparelle e termostati restano sola lettura fino all'azione delicata di F5.
- **Luminosità** nella card («Accesa · 80 %», bozza): `Preferito` guadagna `luminosita: Option<u8>` dal `LightEntity.brightness` del mapper, che è già in percento (`casa-core/src/ha/mapper.rs:706`).
- **Testata di Casa**: «Spegni tutte» primaria bianca con l'icona (spec), «Spegni la stanza» secondaria in una stanza (bozza); «Riprova» o «Collega» secondarie a casa scollegata. Le luci accese si contano **con lo stato ottimistico** (pagina e carta): dopo «Spegni tutte» il pulsante sparisce subito e non resta fino alla conferma di HA (rimandato di F1). Niente «aggiornato ora» della bozza: servirebbe un orologio che lo rinfresca (spec §3); a casa scollegata il sotto dice «dati delle 10:42».
- **Testata del PC**: titolo `r1.fatto` («Tutto regolare»), «Muto» / «Riattiva l'audio» secondaria solo con un'uscita audio; niente «aggiornato 2 s fa» (stessa ragione).
- **Testata di Collegamenti**: titolo `r1.fatto` («6 collegamenti»: qui il nome intero, nella carta «6 link»), sotto «Più usato: NAS», oppure l'errore dell'ultima apertura al suo posto; «Modifica» secondaria (le Impostazioni sulla scheda Collegamenti). Le card nell'**ordine scelto dall'utente** (fermo), senza il conteggio a vista *(scelta del coordinatore: il conteggio resta un dato interno che decide il «più usato»)*.
- **La nota sotto le card** del filtro «Luci accese» (bozza: «Le luci spente non compaiono in questo filtro.»; senza luci: «Nessuna luce accesa: le luci spente non compaiono in questo filtro.») è `comandi-nota`, scritta da Rust.
- **Stati di pagina** (spec §5.6) con `StatoPagina` di F2: Casa non configurata → vuoto «Collega la casa» / «Qui accendi luci e scene di Home Assistant.» / «Collega»; in connessione → primo avvio «Mi collego alla casa», nessuna azione; scollegata senza dati → fuori servizio con la frase di `frase_errore` («Casa non raggiungibile»), sotto «Nessun dato», «Riprova» (o «Collega» se non riprovabile); connessa senza dispositivi → vuoto «Nessun dispositivo da mostrare» / «Aggiungi dei dispositivi alle stanze in Home Assistant.» senza azione. Collegamenti senza collegamenti → vuoto «Nessun collegamento» / «Le pagine delle tue gestioni a portata di clic: si aprono nel browser.» / «Aggiungi un collegamento». PC prima del primo campione → primo avvio «Lettura in corso».
- **Filtri di Casa** *(Paolo, 24/11, Domanda 1)*: «★ Preferiti» (solo se l'utente ne ha scelti), «Luci accese» col numero, «Tutta la casa», filo, le stanze nell'ordine salvato col numero dei dispositivi; «Altro» per le entità senza stanza come oggi. Apertura dal dock su «★ Preferiti», o su «Tutta la casa» senza preferiti scelti; dalla carta con luci accese su «Luci accese» (dec. 14, oggi `mostra-luci`). Nei filtri che mescolano le stanze il nome della card è quello di HA, senza aggiungere la stanza.
- **Scene** *(Paolo, 24/11, Domanda 2)*: fila «SCENE» (mono 11) di chip 32 r10 `g3` con l'icona `Icone.scena` (HA non dà un'icona per scena) dopo le card, dentro lo scorrimento, in ogni filtro tranne «Luci accese»; le scene sono di tutta la casa (il mapper non le lega a una stanza); sparisce il gruppo «Scene» fra i filtri.
- **Misure del PC** *(Paolo, 24/11, Domanda 3)*: quattro posti CPU · Memoria · Disco · Batteria; senza batteria il quarto è la GPU, senza tutte e due il posto resta vuoto (posti fissi). Disco come nella bozza: valore «182 GB», riga «liberi su 476 GB», barra = spazio occupato. CPU: la riga dice la GPU quando la GPU non ha un posto suo («GPU 37 %»), altrimenti niente (i «8 core, 2,1 GHz» della bozza non li legge nessuno oggi). Batteria: «In carica», «A batteria», «Carica» (l'ora di carica piena della bozza non c'è); barra in `fatto` mentre carica. CPU o RAM ≥ 85 % (`SOGLIA_CARICO` della carta): barra e valore in `attivo`.
- **Righe del PC** *(Paolo, 24/11, Domanda 4: proposta C)*: misure alte **84** (etichetta, valore e barra; la riga piccola accanto al valore, a destra, accorciata con ellissi e intera per Narrator) e sotto **due file di righe da 36 in due colonne**, tutte a vista: Wi-Fi (nome della rete e velocità, interruttore in linea) · Bluetooth (le cuffie e la loro batteria se ci sono, interruttore in linea); Volume (cursore e «40 %») · Windows (rimandi «Reti Wi-Fi» e «Bluetooth»). Fuori da Windows: solo la riga della rete, in sola lettura. Un rifiuto di Windows scrive «Non riuscito: apro le Impostazioni di Windows» nella riga, in `attivo`.
- **Il cursore del volume legge `Pannello.volume`/`volume-muto`** (restano: li usa anche la barra), non il modello: il trascinamento non deve aspettare Rust.
- **Bersagli che spariscono col fuoco addosso** (spec §13.1): Rust tiene `comandi-fuoco` sulla stessa card per id (come `elenco-fuoco` di F2) o sulla più vicina; un filtro scelto che sparisce (stanza nascosta in HA) torna al filtro iniziale; in Slint una regola `changed` per ogni modello che cala.
- **Richiesta di filtro dalla carta**: `Ponte::filtro_richiesto` come `permesso_richiesto` di F2, consumato dalla prima scrittura dopo l'apertura della pagina.

## Global Constraints

- Ramo `v3-assistente` (HEAD di partenza `fba35e4`, F1 e F2 chiuse); `main` non si tocca. Ogni commit locale con l'ok di Paolo; nessun push, tag o merge senza un suo sì esplicito.
- **Sorgenti solo con gli strumenti di modifica dei file (Edit/Write)**: mai scritti da PowerShell 5.1 né da una shell.
- Build e test **solo da PowerShell** con `$env:CARGO_TARGET_DIR='C:\Work\Sviluppo\itsyhome-windows\target\agente'` (una sola cartella condivisa, ripulita a fine fetta); suite in primo piano. Rust 1.94 in locale e 1.100 in CI: niente API deprecate in 1.100 né assenti in 1.94.
- Testi per l'utente in italiano, verbo in testa sui pulsanti; commenti, commit e CHANGELOG in italiano. **Ogni task aggiorna `CHANGELOG.md` (sezione Unreleased v3, «Ridisegno F3, Task N: …») nel proprio commit.**
- **Leggerezza** (spec §3): **nessun timer nuovo e nessuna animazione continua** (resta solo il battito degli occhi); animazioni solo durante una transizione; **le proprietà Slint si scrivono solo quando il valore cambia** (`aggiorna_modello`, `scrivi_se_cambia`); i modelli di pagina si riempiono **solo a pagina aperta**; i campioni del PC restano quelli di oggi (uno ogni 2 s a pannello aperto); radio e cuffie si leggono solo a pagina PC aperta, come oggi.
- **Mai l'Home Assistant vero** nelle prove automatiche, nelle misure e nelle foto: test con dati finti (`ponte::test::{luce, interruttore, scena, sensore, serratura, menu}`), prove a mano col server `finto_ha`. La casa vera solo nella prova dal vivo, con Paolo e col suo sì.
- **Azioni rumorose mai** senza l'ok di Paolo (scene, prese, Wi-Fi e Bluetooth dal vivo, volume alto); un'entità «unavailable» non è un comando fallito.
- **Sicurezza di Claude invariata** (spec §7, M3): le regole dei permessi non cambiano su nessuna superficie (fuoco su «Nega», solo Ctrl+Invio esatto, armamento di 600 ms, testo intero prima del consenso, nessun pulsante sotto il clic, nessun «Sempre»); **«Consenti» neutro come «Nega»**. F3 tocca Agenti solo nelle parole della carta e nel titolo della sintesi.
- **Dati**: nessun dato nuovo su disco (il conteggio dei lanci c'è già da F1); niente di nuovo in `diagnosi.log`; il nome della rete Wi-Fi solo a pagina PC aperta, mai nella pillola né nei log (come oggi).
- Test verdi: `cargo test --workspace`; i test delle viste tolte si cancellano **nello stesso task che toglie la vista**, dopo che le regole che proteggevano (fuoco mai perso, Esc e Backspace a livelli, nome per Narrator, card toccabile solo se connessa, Invio apre il collegamento, Wi-Fi/BT da tastiera) sono rinate sull'impianto nuovo.
- `cargo clippy --workspace --all-targets -- -D warnings` pulito; codice solo-Windows dietro `#[cfg(windows)]`, nessun codice morto su Linux/macOS (CI sui tre).
- Prove e foto su un'**istanza di prova** (copia dell'exe nella scratchpad, `ITSYHOME_ASSISTENTE_DIR` su una cartella finta preparata da `finto_ha prepara`, mai `%APPDATA%`). **Foto solo della finestra dell'app** (PrintWindow del proprio HWND o `tools\foto-zona.ps1`), cancellate dopo il confronto. **Input sintetico solo al proprio PID** (UI Automation o `PostMessage` alla propria HWND; mai SendInput, SetCursorPos, mouse_event né clic reali). Processi fermati solo per PID, dopo aver controllato il percorso; mai `taskkill` per nome.
- Colori solo come token di `ui/tema.slint` (`ui_senza_colori_a_mano` resta), simboli sempre SVG o `Path` (mai glifi Unicode: «★», «›», «↓/↑» compresi; le icone nuove da Lucide, ISC, in `assets/icone/lucide/`), testo ≥ 12 px (mono maiuscolo ≥ 11 con tracking .07em), contrasto: parola ≥ 4,5:1, segno e bordo ≥ 3:1. Stato = colore + forma + parola.

## Review Focus

1. **Dati che cambiano sotto il fuoco a pagina aperta** (una luce spenta esce da «Luci accese» col fuoco addosso, una stanza nascosta in HA sparisce mentre è il filtro scelto, la casa si scollega, un collegamento tolto dalle Impostazioni) → il fuoco va alla card o al filtro più vicino, Tab non si perde, il nome per Narrator è quello nuovo (Task 4 `comandi_card_tolta_col_fuoco_addosso`, `comandi_filtro_sparito_torna_all_iniziale`; Task 5 `collegamenti_tolto_col_fuoco_addosso`).
2. **Molti elementi e testi lunghi** (40 dispositivi in una stanza, 30 stanze, 20 scene, 60 collegamenti, nomi di 80 caratteri, nome della rete di 32) → griglia e filtri che scorrono col fuoco in vista, ellissi, nome intero per Narrator, nulla oltre la sua colonna (Task 4 `comandi_molte_card_e_nomi_lunghi`, Task 7 `misure_testi_lunghi_nei_posti`).
3. **Tocchi ripetuti e pulsanti che spariscono** (doppio tocco su una card, «Spegni tutte» premuto e poi un secondo clic nello stesso punto) → un solo comando per tocco con lo stato ottimistico, il pulsante sparisce subito e al suo posto non nasce nulla di cliccabile (Task 3 `spegni_tutte_conta_con_l_ottimismo`, Task 4 `spegni_tutte_non_lascia_bersagli_sotto_il_clic`).
4. **PC senza dati o senza hardware** (primo campione, nessuna batteria né GPU, nessuna uscita audio, Wi-Fi o Bluetooth assenti, fuori da Windows) → «—» col motivo, mai «0 %», posti fissi, nessun comando morto raggiungibile da tastiera (Task 6 `misure_assenti_non_sono_zero`, Task 7 `misure_senza_radio_ne_volume`).
5. **Aggiornamenti continui a pagina aperta** (campioni ogni 2 s, raffiche di `state_changed` di HA) → si riscrivono solo le righe cambiate, scorrimento e fuoco non saltano, nessun fotogramma se nulla cambia (Task 4 `pagina_casa_scritta_solo_se_cambia`, Task 7 `pagina_pc_scritta_solo_se_cambia`).

---

### Task 1: Misura di partenza (m3) e pillola a riposo a schermo sbloccato

**Files:**
- Modify: `CHANGELOG.md`, `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` (esito di m3 sotto «Esito della revisione finale»)
- Usa e getta, mai committati: `crates/assistente/ui/guscio/permesso.slint` (stessa interfaccia, corpo vuoto), un contatore dei `RedrawRequested` di winit su pillola e pannello (come nel Task 9 di F2)

**Interfaces:** nessuna. È il «prima» di tutte le misure del Task 8.

- [ ] **Step 1: Build e istanza di prova** su HEAD `fba35e4`: `cargo build --release -p assistente`; copia dell'exe in `scratchpad\prova-f3\bin`, cartella finta preparata da `cargo run -p casa-core --example finto_ha -- prepara <cartella> <porta> --preferiti`, server `finto_ha <porta>` acceso, sei collegamenti finti (`https://…esempio.lan/`) scritti nel suo `assistente.json` prima del primo avvio.
- [ ] **Step 2: Misure** con `tools\misura.ps1 -Secondi 60`, dal tool PowerShell (mai da un `powershell -Command` nel tool Bash: la pillola non si disegna, trappola del Task 9 di F2), due prove ciascuna: a riposo; `--apri` (Panoramica ferma); pagina Casa, pagina PC e pagina Collegamenti aperte e ferme (aperte con `PostMessage` di Tab/frecce/Invio alla propria finestra). Annotare RAM privata e CPU medie dal secondo 10.
- [ ] **Step 3: m3**: la versione usa e getta di `permesso.slint` (stessa interfaccia: proprietà, callback, `entra()`, `tasto()` che restituisce falso; nessun elemento dentro), build in `target\agente`, `--apri` misurato due volte; poi `git checkout -- crates/assistente/ui/guscio/permesso.slint` e `git status` pulito. **Confermata** se la CPU media scende di almeno 0,015 punti in entrambe le prove.
- [ ] **Step 4: Pillola a riposo a schermo sbloccato**: build usa e getta col contatore dei fotogrammi, 60 s a riposo con lo schermo sbloccato (se il PC è bloccato si rimanda e lo si dice: a schermo bloccato la pillola non si disegna). Atteso: solo il battito degli occhi; RAM e CPU come la riga «a riposo». Sorgenti ripristinati, `git status` pulito.
- [ ] **Step 5: CHANGELOG** («Ridisegno F3, Task 1: misura di partenza») con la tabella; nei rimandati l'esito di m3. Se m3 è confermata, nessun rimedio in F3 (toccherebbe il permesso): la cifra e una proposta vanno al coordinatore, e la regola per F3 è che gli impianti nuovi stanno dentro `if vista == …` (nessuna istanza fissa viva fuori dalla sua pagina).
- [ ] **Step 6:** pulizia di `target\agente\release` della build usa e getta (`cargo build --release` la rifà pulita); commit `docs: misura di partenza di F3 (m3, pillola a riposo)`.

### Task 2: Campo `r1` del contratto e carta Agenti allineata alla pagina (m2)

**Files:**
- Modify: `crates/assistente/src/moduli/mod.rs` (`R1`, `Riassunto.r1`, `primo_avvio`, `controlla`), `src/moduli/misura.rs` (`SPAZIO_R1`), `src/moduli/{agenti,casa,pc,collegamenti}.rs` (`r1` in ogni gradino), `src/moduli/pagina_agenti.rs` (`sintesi` prende il titolo da `r1`), `src/moduli/registro.rs` (test), `src/ponte/con_moduli.rs` (`riassunto_di("agenti")` passa i permessi; `azione_modulo` usa la stessa sessione), `src/ponte/con_claude.rs` (`permessi_della_pagina` diventa `pub(super)`; `aggiorna_vista_claude_a` passa `r1`), `src/ponte.rs` (`attese_cambiate` chiama anche `moduli_aggiorna("agenti")`), `CHANGELOG.md`
- Test: gli stessi file

**Interfaces:**
- Produces: `#[derive(Debug, Clone, PartialEq, Eq, Default)] pub struct R1 { pub fatto: String, pub valore: String }`; `Riassunto.r1: R1`; in `misura.rs` `pub const SPAZIO_R1: f32 = 320.0;` e `pub const MASSIMO_FATTO: usize = 32; pub const MASSIMO_VALORE: usize = 8;`; `controlla` rifiuta anche `campo: "r1.fatto"` (caratteri o pixel in `Stile::Titolo`) e `"r1.valore"` (caratteri).
- Produces: `agenti::riassunto(collegato: bool, s: &Sessioni, limiti: (Option<&Limite>, Option<&Limite>), permessi: &[(&str /* impronta della sessione */, &DatiPermesso)] /* dal più vecchio */) -> Riassunto`; `agenti::sessione_dell_azione(s: &Sessioni, permessi: &[(&str, &DatiPermesso)]) -> Option<&Sessione>` (con permessi: nessuna, la carta non ha azione); `pagina_agenti::sintesi(collegato: bool, r1: &R1, righe: &[RigaModuloUi], limiti: …, soglia: u8, spesa: (&str, &str), con_chiave: bool) -> SintesiUi` (titolo = `r1.fatto`; il sotto resta calcolato dalle righe).

Scala `r1.fatto` (il titolo della testata): Agenti «Da collegare» / «A riposo» / «1 sessione» / «{n} sessioni» / «{n} al lavoro» / «1 ti aspetta» / «{n} ti aspettano» (la stessa precedenza di oggi: permessi, poi chi lavora, poi il numero); Casa «Da collegare» / «Mi collego» / «Scollegata» / «Luci spente» / «1 luce accesa» / «{n} luci accese»; PC «Tutto regolare» / «CPU {c} %» / «RAM {r} %» / «Batteria {b} %» / il motivo del primo avvio; Collegamenti «Nessun collegamento» / «1 collegamento» / «{n} collegamenti».

- [ ] **Step 1: Test che falliscono**:
  - `moduli::test::r1_nei_limiti_del_registro`: un `R1` di 33 caratteri, uno di 32 «W» (oltre 320 px) e un valore di 9 caratteri → `Err(FuoriMisura { campo: "r1.fatto" | "r1.valore", .. })`; `primo_avvio("Lettura in corso").r1.fatto == "Lettura in corso"`.
  - In ognuno dei quattro moduli `ogni_gradino_sta_nei_limiti` esteso a `r1` (coi dati peggiori di oggi) e `scala_per_stato` con il `r1.fatto` atteso della tabella; `assenza_non_e_zero` vale anche per `r1`.
  - `agenti::test::carta_col_permesso_come_la_pagina` (m2): una sessione `TiAspetta` con un permesso → tono `Aspetta`, titolo «1 ti aspetta», riga «prog-a: Comando», `sec_intera` «prog-a chiede: Comando», azione assente; due permessi → «2 ti aspettano», «2 permessi»; la stessa sessione senza permesso → «Tocca a te» (Guarda); titolo e riga misurati entro `SPAZIO_PIENO - MARGINE`.
  - `pagina_agenti::test::titolo_della_sintesi_e_r1`: per scollegato, nessuna sessione, 2 al lavoro, 1 e 2 permessi, `sintesi(…).titolo == agenti::riassunto(…).r1.fatto`.
  - `con_moduli::test::carta_agenti_segue_i_permessi`: pannello aperto, `claude_permesso` di una sessione → la carta 0 dice «1 ti aspetta» con la capsula «Aspetta»; tolto il permesso → torna «Tocca a te» o «1 al lavoro».
- [ ] **Step 2:** `cargo test -p assistente r1_ ogni_gradino scala_per_stato carta_col_permesso titolo_della_sintesi carta_agenti_segue` → FAIL.
- [ ] **Step 3: Implementa** (ogni letterale `Riassunto { … }` riceve `r1`: `agenti.rs` `fisso`/`con_nome`, `casa.rs` `carta`, `pc.rs` `carta`, `collegamenti.rs`, `mod.rs` `primo_avvio`, `registro.rs` test `calmo`).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): campo r1 del contratto; la carta Agenti dice il permesso come la pagina`.

### Task 3: Casa sull'impianto Comandi, funzioni pure

**Files:**
- Create: `crates/assistente/src/moduli/pagina_casa.rs`
- Modify: `src/moduli/mod.rs` (`pub mod pagina_casa;`), `src/moduli/casa.rs` (`riassunto` riceve `accese: usize`; `frase_breve` diventa `frase_errore_breve(e: &str) -> &'static str` sulla stessa chiave di `frase_errore`), `src/ponte.rs` (`Preferito.luminosita`; `preferito` diventa `pub(crate)`; `pub fn luci_accese_ora(menu: &MenuData, ottimismo: &Ottimismo<bool>) -> Vec<Preferito>`; dopo un tocco `moduli_aggiorna("casa")`), `src/ponte/con_moduli.rs` (`riassunto_di("casa")` passa `luci_accese_ora(…).len()`; il test `da_chiuso_non_si_ricalcola` conta le scritture con `RIGHE_SCRITTE`), `ui/voci.slint` (strutture sotto), `CHANGELOG.md`
- Test: `pagina_casa.rs`, `casa.rs`, `ponte.rs`, `con_moduli.rs`

**Interfaces:**
- Produces (Slint, `voci.slint`):
  - `export enum TipoCardUi { interruttore, scena, lettura, lancio }`
  - `export struct CardUi { id: string, tipo: TipoCardUi, genere: GenereUi, nome: string, stato: string, valore: string, acceso: bool, toccabile: bool, tenue: bool, errore: bool, iniziale: string, tinta: int, narratore: string }`
  - `export struct FiltroUi { id: string, nome: string, icona: image, numero: string, filo: bool /* filo sopra */, narratore: string }`
  - `SintesiUi` guadagna `azione-primaria: bool` e `azione-icona: image` (Agenti: falso e vuota).
- Produces (Rust, `pagina_casa`): `pub const FILTRO_PREFERITI: &str = "casa.preferiti"; pub const FILTRO_LUCI: &str = "casa.luci"; pub const FILTRO_TUTTA: &str = "casa.tutta"; pub const SPEGNI_STANZA: &str = "casa.spegni-stanza";` (una stanza è il suo `area_id`; riusa `casa::{SPEGNI, RIPROVA, COLLEGA}`);
  - `pub fn filtri(menu: &MenuData, accese: usize) -> Vec<FiltroUi>`
  - `pub fn filtro_iniziale(menu: &MenuData) -> &'static str` e `pub fn filtro_valido(menu: &MenuData, scelto: &str) -> String` (sparito → iniziale)
  - `pub fn del_filtro(menu: &MenuData, filtro: &str) -> Vec<Preferito>` (le card, scene escluse) e `pub fn scene(menu: &MenuData, filtro: &str) -> Vec<Preferito>` (vuote in «Luci accese»)
  - `pub fn card(p: &Preferito, connesso: bool) -> CardUi`
  - `pub fn sintesi(stato: &StatoConnessione, r1: &R1, filtro: &str, menu: Option<&MenuData>, accese: usize, dati_delle: Option<&str>) -> SintesiUi`
  - `pub fn stato(stato: &StatoConnessione, menu: Option<&MenuData>) -> StatoPaginaUi` e `pub fn nota(filtro: &str, card: usize) -> &'static str`

Card (tipo, parola, valore): luce accesa «Accesa» e «80 %» se c'è la luminosità; luce spenta «Spenta»; interruttore «Acceso»/«Spento»; sensore numerico → `lettura`, valore «21,5°»/«48 %» grande e la parola = il nome; altra lettura (tapparella, serratura, termostato) → `lettura` senza valore, parola = `stato_in_lettura`; non disponibile «Non disponibile», tenue; errore «Non riuscito», `errore`. `narratore`: «{nome}, {parola}[, {valore}]» (sola lettura: «…, sola lettura»). Filtri: `numero` «4» per «Luci accese» e per le stanze (dispositivi), vuoto per Preferiti e Tutta la casa; `filo` sulla prima stanza; `narratore` «Soggiorno, 6 dispositivi» / «Luci accese, 4» / «Preferiti». Sintesi: tabella delle «Decisioni» (titolo `r1.fatto` sui filtri della casa, il nome della stanza e «{a} accese su {n}» o «{n} dispositivi» in una stanza; azioni come lì).

- [ ] **Step 1: Test che falliscono** (in `pagina_casa::test`, coi costruttori di `ponte::test`):
  - `filtri_e_ordine`: con preferiti → `[Preferiti, Luci accese, Tutta la casa, Stanza 0, Stanza 1]`, filo sulla prima stanza, numeri; senza preferiti nessun «Preferiti» e `filtro_iniziale == FILTRO_TUTTA`.
  - `filtro_sparito_torna_all_iniziale`: `filtro_valido(menu, "a9") == filtro_iniziale(menu)`.
  - `card_per_genere`: le righe della tabella (tipo, parola, valore, `acceso`, `toccabile`, `tenue`, `narratore`); una serratura è `lettura` e non `toccabile`; scollegata → tutte tenui e non toccabili.
  - `luci_accese_solo_accese_e_scene_fuori`: `del_filtro(FILTRO_LUCI)` = luci accese e disponibili nell'ordine delle stanze; `scene(FILTRO_LUCI)` vuoto, `scene(FILTRO_TUTTA)` = le scene.
  - `sintesi_per_stato`: connessa con 4 accese → «4 luci accese», «Spegni tutte» primaria con l'icona; 0 accese → nessuna azione; stanza con 2 luci accese su 4 dispositivi → «Stanza 0», «2 accese su 4», «Spegni la stanza» secondaria; scollegata con dati → «Scollegata», «dati delle 10:42», «Riprova».
  - `stato_della_pagina`: i quattro casi delle «Decisioni».
  - `casa::test::scala_per_stato` aggiornato (`accese` passato) e `ponte::test::spegni_tutte_conta_con_l_ottimismo` (Review Focus 3): dopo `spegni_tutte` con due luci accese, `luci_accese_ora` è vuoto e `casa::riassunto` non ha più l'azione, prima di ogni conferma di HA; un secondo `spegni_tutte` non manda comandi.
  - `con_moduli::test::da_chiuso_non_si_ricalcola`: l'asserzione «modello fermo» conta `RIGHE_SCRITTE == 0`.
- [ ] **Step 2:** `cargo test -p assistente pagina_casa casa::test spegni_tutte_conta da_chiuso` → FAIL.
- [ ] **Step 3: Implementa** (`#[cfg_attr(not(test), allow(dead_code))]` su `pagina_casa` fino al Task 4, come la Ruling C3 di F1).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): filtri, card, sintesi e stato della pagina Casa come funzioni pure`.

### Task 4: Impianto Comandi, la pagina Casa ci si sposta

**Files:**
- Create: `crates/assistente/ui/impianti/comandi.slint`, `crates/assistente/src/ponte/con_pagine.rs`, `assets/icone/lucide/power.svg` (ISC)
- Delete: `crates/assistente/ui/viste/casa.slint`; in `voci.slint` `PreferitoUi`, `StanzaUi`
- Modify: `ui/interfaccia.slint` (via `PreferitoUi` dall'`export`), `ui/componenti.slint` (`EtichettaMono`), `ui/guscio/misura.slint` (lo stile 3 misura un `EtichettaMono`), `ui/guscio/cornice.slint` e `ui/guscio/carta.slint` (nome della scheda e della carta con `EtichettaMono`), `ui/guscio/pagina.slint` (`TestataPagina`: `azione-primaria`, `azione-icona`), `ui/pannello.slint` (via `preferiti`, `stanze`, `dispositivi`, `luci`, `filtro-luci`, `stanza-aperta`, `stato-casa`, `casa-pronta`, `casa-da-configurare`, `dati-delle`, `casa-riprovabile`, `apri-stanza`, `chiudi-stanza`, `tocca`, `spegni-tutte`, `riprova-casa` e le loro regole; al loro posto i modelli sotto; `Zona.stanze` diventa `Zona.filtri`; `scegli-carta` apre «Luci accese» se l'azione della carta Casa è `casa.spegni`), `src/ponte.rs` (via `ricostruisci_preferiti`, `stanza`, i modelli `preferiti`/`stanze`/`dispositivi`/`luci` e `riempi_stato` per le proprietà tolte; `scrivi_se_cambia` spostata qui da `con_claude.rs`, `pub(super)`), `src/ponte/con_claude.rs` (`vista_cambiata` chiama `vista_comandi`), `src/ponte/con_moduli.rs` (`azione_pagina` per Casa), `src/pannello.rs` (icone `stella`, `spegni`), `CHANGELOG.md`
- Test: `test_ui.rs` (i test della vista Casa rinascono qui; helper comuni sotto), `ponte/con_pagine.rs`, `ponte/con_moduli.rs`

**Interfaces:**
- Consumes: strutture e funzioni del Task 3; `TestataPagina`, `StatoPagina`, `Fuoco`, `Anello`, `Pulsante`, `Icone.di-genere`.
- Produces (Slint):
  - `EtichettaMono inherits Text` — JetBrains Mono NL 500, `Tema.etichetta-corpo`, `Tema.etichetta-spaziatura`, maiuscolo, `accessible-role: none` (il nome lo dà chi la contiene).
  - `GeometriaComandi { out property <length> largo-filtri: 172px; x-griglia: 188px; alto-card: 64px; spazio: 8px; }`
  - `ImpiantoComandi { in property <[FiltroUi]> filtri; in property <int> filtro; in property <[CardUi]> card; in property <[CardUi]> scene; in property <string> nota; in property <int> fuoco /* card, o chip con lato 2, o filtro con lato 0 */; in property <int> lato /* -1 testata, 0 filtri, 1 card, 2 scene */; in property <bool> con-fuoco; out property <int> colonne /* 3 con filtri, 4 senza */; callback scegli-filtro(int); callback tocca(string); }` — filtri x 0 da y 48, larghi 172, voci 32 r10 (fondo `Tema.scheda-attiva` sulla scelta), `tab-list`/`tab` con `accessible-item-selected/index/count`; griglia da x 188 (0 senza filtri) in un `Flickable` che sporge di 5 px per l'anello; la fila «SCENE» (`EtichettaMono` + chip) dopo l'ultima riga di card, dentro lo scorrimento; la nota 12 `testo-tenue` sotto le card; filo di scorrimento come in F2.
  - `CardComando { in property <CardUi> card; in property <bool> anello; callback tocca(); }` — le quattro forme delle «Decisioni».
  - Pannello: `in property <SintesiUi> comandi-sintesi; in property <StatoPaginaUi> comandi-stato; in property <[FiltroUi]> comandi-filtri; in-out property <int> comandi-filtro; in property <[CardUi]> comandi-card; in property <[CardUi]> comandi-scene; in property <string> comandi-nota; in-out property <int> comandi-fuoco; in-out property <int> comandi-lato; callback filtro-pagina(string /* modulo */, string /* filtro */); callback tocca-card(string /* modulo */, string /* id */);` (le azioni della testata e dello stato passano da `azione-pagina` di F2).
  - Tastiera (`freccia-comandi`, `attiva-comandi`): nei filtri ↑/↓, Invio o Spazio sceglie e porta alle card, → alle card; nelle card le frecce in griglia, ← dalla prima colonna al filtro scelto, ↑ dalla prima riga all'azione della testata se c'è, ↓ dall'ultima riga alle scene; nelle scene ←/→, ↑ torna alle card; Invio o Spazio tocca una card toccabile (sola lettura: nulla) o una scena; Backspace e Alt+← dal filtro scelto a quello iniziale, poi alla Panoramica; Esc lo stesso, poi chiude il pannello. Clic su un filtro = scelta (`filtro-pagina`); clic su una card = `tocca-card`.
- Produces (Rust, `con_pagine.rs`): `Ponte::vista_comandi(&self, vista: &str)` (apre o chiude la pagina, `pagina_comandi`); `Ponte::scrivi_pagina_casa(&self)` (solo se `pagina_comandi == Some("casa")`: filtro valido, card con ottimismo ed errori, scene, sintesi, stato, nota, fuoco sulla stessa card per id); `Ponte::filtro_pagina(&self, modulo: &str, filtro: &str)`; `Ponte::tocca_card(&self, modulo: &str, id: &str)`; `Ponte::chiedi_filtro(&self, filtro: &str)` (consumato alla prima scrittura). `azione_pagina("casa", casa::SPEGNI | SPEGNI_STANZA | RIPROVA | COLLEGA, "")`; `Ponte::spegni_stanza(&self, area: &str)` come `spegni_tutte`, sulle luci accese della stanza.
- Helper di prova in `test_ui.rs`, `pub(crate)` per tutti i test UI (anche `con_moduli`, `con_pagine`): `tasto(p, k)`, `etichetta_col_fuoco(p)`, `pannello_comandi(modulo, filtri, card, scene)`, `card_casa(id, nome, tipo)`, `filtro(id, nome)`.

- [ ] **Step 1: Test che falliscono** (i test della vista Casa rinascono qui **prima** di cancellarla):
  - `comandi_filtri_accessibili` (da `stanze_della_casa_accessibili`, `preferiti_prima_voce_delle_stanze`): `tab-list` «Filtri», un solo filtro selezionato, indice e totale, clic → `filtro-pagina casa a0`, nessun «Indietro».
  - `comandi_card_interruttore` (spec §5.6): ruolo `Switch`, `accessible-checked` = acceso, nome «Divano, accesa, 80 %»; clic e Invio → `tocca-card casa light.divano`; card di sola lettura senza fondo (`background` trasparente) e ruolo `Text`, Invio non manda nulla; tenue non toccabile.
  - `comandi_da_tastiera` (da `stanza_da_tastiera_ed_esc_a_livelli`, `preferiti_da_tastiera`, `tab_fra_stanze_card_e_dock`, `casa_filtrata_da_tastiera_senza_perdere_il_fuoco`): il giro delle frecce delle «Interfaces», Tab filtri → card → schede, Esc e Backspace a livelli.
  - `comandi_scene_in_fondo`: fila «SCENE» sotto l'ultima riga, ↓ dall'ultima riga sulla prima scena, Invio → `tocca-card casa scene.sera`; nessuna fila nel filtro «Luci accese».
  - `comandi_card_tolta_col_fuoco_addosso` (Review Focus 1): fuoco sulla card 3 di 4, il modello cala a 3 → dopo `update_timers_and_animations()` fuoco sulla card 2, Tab seguente non si perde.
  - `comandi_molte_card_e_nomi_lunghi` (Review Focus 2): 40 card con nomi di 80 caratteri e 30 filtri → la card col fuoco e il filtro col fuoco in vista, nessun testo oltre la sua card, nome intero per Narrator.
  - `comandi_geometria`: testata y 0–40, filtri x 0–172, card 64 alte e larghe 182 ± 1 con 3 colonne da x 188, tutto entro 752 × 224; `pagine_dentro_il_corpo` aggiornato per la vista `casa`.
  - `spegni_tutte_non_lascia_bersagli_sotto_il_clic` (Review Focus 3): «Spegni tutte» primaria bianca con l'icona; tolta dal modello la sintesi senza azione, nel suo rettangolo non c'è nessun elemento cliccabile.
  - `la_carta_casa_con_luci_accese_apre_luci_accese` (da `la_carta_casa_con_luci_accese_apre_la_casa_filtrata`): clic sulla carta con azione `casa.spegni` → vista `casa` e `filtro-pagina casa casa.luci`.
  - `stati_della_pagina_casa` (da `casa_filtrata_senza_connessione_dice_lo_stato`, `casa_filtrata_senza_luci_accese_ha_lo_stato_vuoto`): vuoto con «Collega» (Invio → `azione-pagina casa casa.collega`), primo avvio senza pulsanti, fuori servizio con «Riprova».
  - `etichetta_mono_come_stile_etichetta` (m6): `MisuraTesto` stile 3 (ora un `EtichettaMono`) e `misura::larghezza(_, Stile::Etichetta)` per «AGENTI», «COLLEGAMENTI», «SCENE» entro `MARGINE`.
  - `con_pagine::test::pagina_casa_scritta_solo_se_cambia` (Review Focus 5): due `scrivi_pagina_casa` di fila → `RIGHE_SCRITTE` invariato; a pagina chiusa un evento di HA non scrive nulla; `filtro_pagina` con un filtro sparito → filtro iniziale.
  - `con_pagine::test::comandi_filtro_sparito_torna_all_iniziale` (Review Focus 1): stanza scelta nascosta dal menu nuovo → `comandi-filtro` sul filtro iniziale e il fuoco sui filtri.
  - Cancellati qui: i test della vista Casa elencati sopra e `card_casa_ritmo_4_8`, `backspace_esce_dal_filtro_poi_dalla_casa` (rinati nei nomi `comandi_*`).
- [ ] **Step 2:** `cargo test --workspace comandi_ spegni_tutte_non la_carta_casa stati_della_pagina_casa etichetta_mono pagina_casa_scritta` → FAIL.
- [ ] **Step 3: Implementa** impianto, testata allargata, scrittura dei modelli, tastiera; via il `allow(dead_code)` del Task 3.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto dell'istanza di prova con `finto_ha` (filtro «Luci accese» e una stanza con sensori e scene) a 100 % e 175 %, confronto con `tc-casa-luci.png` e `tc-casa-stanza.png`; nessun comando mandato da questa prova; foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): impianto Comandi, la pagina Casa ci si sposta`.

### Task 5: Collegamenti sull'impianto Comandi

**Files:**
- Create: `crates/assistente/src/moduli/pagina_collegamenti.rs`
- Delete: `crates/assistente/ui/viste/collegamenti.slint`
- Modify: `src/moduli/mod.rs`, `ui/impianti/comandi.slint` (forma `lancio` di `CardComando`, griglia a 4 colonne senza filtri), `ui/pannello.slint` (via `collegamenti`, `errore-collegamenti`, `apri-collegamento`, `gestisci-collegamenti`, `fuoco-collegamento` e le loro regole), `src/pannello.rs` (via `on_apri_collegamento`/`on_gestisci_collegamenti`: la chiusura dopo un lancio passa dal gancio `chiudi_pannello` del ponte, come `azione_modulo`), `src/ponte/con_pagine.rs` (`scrivi_pagina_collegamenti`, `tocca_card("collegamenti", id)`), `src/ponte/con_moduli.rs` (`azione_pagina("collegamenti", MODIFICA | AGGIUNGI, "")`), `src/ponte.rs` (`scrivi_errore_collegamento` scrive il sotto della sintesi), `src/orologio.rs` (`pub fn adesso_unix() -> i64` spostata qui da `con_claude.rs`; i chiamanti la prendono da qui), `crates/casa-core/tests/config.rs` (riga 400), `CHANGELOG.md`
- Test: `pagina_collegamenti.rs`, `test_ui.rs`, `con_pagine.rs`, `con_moduli.rs`, `casa-core/tests/config.rs`

**Interfaces:**
- Consumes: `CardUi`, `TipoCardUi::Lancio`, `ImpiantoComandi`, `Ponte::vista_comandi` (Task 3–4); `collegamenti::{indirizzo_corto, iniziale, tono, piu_usato}`.
- Produces: `pagina_collegamenti::MODIFICA: &str = "collegamenti.modifica"`; `pub fn card(c: &Collegamento) -> CardUi` (tipo `lancio`, `stato` = indirizzo corto, `narratore` «{nome}, {indirizzo corto}, si apre nel browser»); `pub fn sintesi(c: &[Collegamento], r1: &R1, errore: Option<&str>) -> SintesiUi`; `pub fn stato(n: usize) -> StatoPaginaUi`.

- [ ] **Step 1: Test che falliscono**:
  - `pagina_collegamenti::test::card_e_sintesi`: card nell'ordine dell'utente; titolo `r1.fatto` «2 collegamenti», sotto «Più usato: NAS», «Modifica» secondaria; con l'errore il sotto è l'errore; nessun collegamento → stato vuoto con «Aggiungi un collegamento» (`AGGIUNGI`).
  - `collegamenti_card_accessibili`, `collegamenti_da_tastiera`, `collegamenti_vuota`, `collegamenti_dice_se_non_si_e_aperto` (da `vista_collegamenti_*`): nome per Narrator, descrizione «Si apre nel browser», 4 colonne, frecce in griglia, ↑ dalla prima riga a «Modifica», Invio → `tocca-card collegamenti nas`; l'errore nel sotto della testata.
  - `collegamenti_tolto_col_fuoco_addosso` (Review Focus 1): fuoco sull'ultimo, il modello cala → fuoco sul nuovo ultimo.
  - `con_pagine::test::lancio_dalla_pagina`: `tocca_card("collegamenti", "nas")` → URL al gancio di prova (`su_apri_url`), +1 lancio, pannello chiuso; browser che non parte → pannello aperto, sotto con l'errore, nessun lancio in più.
  - `con_moduli::test::aggiungi_della_carta_apre_le_impostazioni` (rimandato F1 Task 5): `azione_modulo("collegamenti", AGGIUNGI)` → il gancio `su_apri_impostazioni` chiamato una volta.
  - `casa-core/tests/config.rs`: l'asserzione sul `.tmp` diventa una che può fallire (il file `.tmp` creato a mano prima del salvataggio non c'è più dopo).
  - `iniziali_dei_collegamenti_leggibili_sul_loro_cerchio` resta (il cerchio non cambia).
- [ ] **Step 2:** `cargo test --workspace collegamenti_ pagina_collegamenti lancio_dalla aggiungi_della_carta` → FAIL.
- [ ] **Step 3: Implementa**; cancella i test `vista_collegamenti_*` rinati sopra.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto della pagina Collegamenti dell'istanza di prova (sei collegamenti finti) a 100 % e 175 %, accanto alla foto della Casa del Task 4 (stessa card); nessun browser aperto; foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): la pagina Collegamenti sull'impianto Comandi`.

### Task 6: Misure del PC, funzioni pure

**Files:**
- Create: `crates/assistente/src/moduli/pagina_pc.rs`
- Modify: `src/moduli/mod.rs`, `ui/voci.slint` (strutture sotto), `CHANGELOG.md`
- Test: `pagina_pc.rs`

**Interfaces:**
- Consumes: `pc_stats::{StatPc, LetturaBatteria, formatta_usato_totale, formatta_velocita}`, `moduli::pc` (`SOGLIA_CARICO` diventa `pub(crate)`), `R1`.
- Produces (Slint): `export struct MisuraUi { presente: bool, id: string, icona: image, etichetta: string /* «CPU», «Memoria»: l'aspetto maiuscolo lo fa Slint */, valore: string, unita: string, frazione: float, dettaglio: string, tono: TonoModuloUi /* calmo bianco, attivo oltre la soglia, fatto in carica */, narratore: string }`; `export enum TipoRigaMisuraUi { stato, interruttore, cursore, rimandi }`; `export struct RigaMisuraUi { id: string, tipo: TipoRigaMisuraUi, icona: image, nome: string, valore: string, acceso: bool, attiva: bool, errore: string, narratore: string }`.
- Produces (Rust, `pagina_pc`): `pub const MUTO: &str = "pc.muto";` e gli id delle righe `WIFI`, `BLUETOOTH`, `VOLUME`, `RIMANDI`, `RETE`;
  - `pub fn misure(stat: Option<&StatPc>, cpu_pronta: bool) -> [MisuraUi; 4]`
  - `pub struct RighePc<'a> { pub windows: bool, pub velocita: Option<String>, pub nome_rete: Option<&'a str>, pub wifi: Option<(bool /* presente */, bool /* acceso */)>, pub bluetooth: Option<(bool, bool)>, pub cuffie: Option<(&'a str /* nome */, u8 /* % */)>, pub volume: Option<(f32, bool /* muto */)>, pub errore_radio: bool }`
  - `pub fn righe(r: &RighePc) -> Vec<RigaMisuraUi>`; `pub fn sintesi(r1: &R1, volume: Option<(f32, bool)>) -> SintesiUi`; `pub fn stato(stat: Option<&StatPc>, cpu_pronta: bool) -> StatoPaginaUi`

Misure (posto: etichetta · valore · unità · dettaglio · frazione): CPU · «12» · «%» · «GPU 37 %» se la GPU non ha un posto suo · cpu/100; Memoria · «48» · «%» · «7,7 di 16 GB» · ram; Disco C: · «182» · «GB» (liberi) · «liberi su 476 GB» · occupato; Batteria · «84» · «%» · «In carica»/«A batteria»/«Carica» · %/100, tono `fatto` in carica; GPU (solo senza batteria) · «37» · «%» · «» · gpu/100. Senza dato il valore è «—», l'unità e la barra non ci sono, il dettaglio dice il motivo («Lettura in corso», «Non letta»). `narratore` «CPU, 12 %, GPU 37 %» (mai l'etichetta maiuscola). Righe (Domanda 4, proposta C): Wi-Fi `interruttore` col nome della rete e la velocità «↓ 480 Mb/s» (frecce disegnate: il testo è «480 Mb/s giù, 12 Mb/s su» per Narrator, le icone le mette Slint), Bluetooth `interruttore` («Cuffie 70 %» se ci sono, altrimenti «Acceso»/«Spento»/«Non presente»), Volume `cursore`, Windows `rimandi`; fuori da Windows solo `RETE` `stato`.

- [ ] **Step 1: Test che falliscono** (in `pagina_pc::test`):
  - `misure_per_posto`: i cinque posti della tabella; con batteria la GPU va nel dettaglio della CPU; senza batteria la GPU è il quarto posto; senza tutte e due il quarto ha `presente` falso.
  - `misure_assenti_non_sono_zero` (Review Focus 4): `misure(None, false)` → quattro «—» col motivo, nessun «0»; CPU non pronta → «—» solo lì; disco totale 0 → «—».
  - `soglia_e_carica`: CPU 90 → tono `attivo`; batteria in carica → `fatto`.
  - `righe_per_hardware`: Windows con tutto; senza Wi-Fi (`presente` falso → «Non presente», `attiva` falsa); senza volume nessuna riga `cursore`; fuori da Windows una sola riga `stato`; errore → testo del rifiuto.
  - `sintesi_e_stato`: «Muto» con l'audio, «Riattiva l'audio» da muto, nessuna azione senza uscita audio; prima del campione stato primo avvio «Lettura in corso».
  - `narratore_senza_maiuscole`: nessun `narratore` è tutto maiuscolo.
- [ ] **Step 2:** `cargo test -p assistente pagina_pc` → FAIL.
- [ ] **Step 3: Implementa** (`allow(dead_code)` fino al Task 7).
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** commit `feat(assistente): misure, righe, sintesi e stato della pagina PC come funzioni pure`.

### Task 7: Impianto Misure, la pagina PC ci si sposta

**Files:**
- Create: `crates/assistente/ui/impianti/misure.slint`, `assets/icone/lucide/{headphones,battery,cpu}.svg` (ISC; se `Icone.chip` e `Icone.memoria` bastano si usano quelle)
- Delete: `crates/assistente/ui/viste/pc.slint`; `StatPcUi` in `voci.slint` e nell'`export` di `ui/interfaccia.slint` e `pannello.slint`; `stat_ui` e i suoi test in `src/pannello.rs`
- Modify: `ui/pannello.slint` (via `pc`, `connessioni-disponibili`, `radio-lette`, `wifi-presente`, `wifi-acceso`, `bt-presente`, `bt-acceso`, `nome-rete`, `rete-nota`, `rete`, `tacche`, `errore-radio`, `cuffie-*`, `fuoco-pc`, `pc-ha`, `pc-successivo`, `pc-precedente`, `commuta`; al loro posto i modelli sotto; restano `volume`, `volume-muto`, `volume-disponibile`, `imposta-volume`, `muto-volume`, `commuta-radio`, `apri-pagina`), `src/pannello.rs` (il campione va a `Ponte::campione_pc`), `src/connessioni.rs` e `src/sistema/batterie_bt.rs` (`scrivi_radio`, `dimentica_radio`, `scrivi_rete`, `scrivi_nome`, `scrivi_cuffie` non scrivono più il pannello: aggiornano lo stato del ponte e chiamano `scrivi_pagina_pc`; `esito` resta la scelta della pagina di Windows), `src/ponte.rs` (`vista_pc` invariata nel comportamento; `commuta_radio` scrive subito la riga col valore voluto), `src/ponte/con_pagine.rs` (`scrivi_pagina_pc`, `campione_pc`), `src/ponte/con_moduli.rs` (`azione_pagina("pc", MUTO, "")`), `CHANGELOG.md`
- Test: `test_ui.rs` (i test della vista PC rinascono qui), `con_pagine.rs`

**Interfaces:**
- Consumes: strutture e funzioni del Task 6; `EtichettaMono` (Task 4); `TestataPagina`, `StatoPagina`.
- Produces (Slint): `CartaMisura { in property <MisuraUi> misura; }` (alta 84 con la proposta C: etichetta `EtichettaMono` con icona 15, valore mono 28 + unità 14 `testo-2`, dettaglio 12 a destra del valore con ellissi, barra 4 r2 su `Tema.traccia`; `groupbox` col `narratore`); `RigaMisura { in property <RigaMisuraUi> riga; in property <float> volume; in property <bool> muto; in property <int> fuoco /* -1, 0 il comando, 1 il secondo rimando */; callback commuta(); callback apri(int); callback imposta(float); callback cambia-muto(bool); }` (36, icona 16, nome 13, valore mono 12 a destra; l'interruttore 40 × 20 WinUI, `switch`; il cursore e l'altoparlante del `RigaVolume` di oggi; i rimandi come collegamenti); `ImpiantoMisure { in property <[MisuraUi]> misure; in property <[RigaMisuraUi]> righe; in property <int> fuoco; in property <bool> con-fuoco; in property <float> volume; in property <bool> volume-muto; callbacks come `RigaMisura`, con l'indice della riga }` — misure da y 48 a 4 colonne di 182, righe da y 140 in due colonne da 372.
- Pannello: `in property <SintesiUi> misure-sintesi; in property <StatoPaginaUi> misure-stato; in property <[MisuraUi]> misure-valori; in property <[RigaMisuraUi]> misure-righe; in-out property <int> misure-fuoco /* bersaglio: riga × 2 + comando */;` tastiera `freccia-misure`/`attiva-misure`: ←/→ e ↑/↓ fra i bersagli che ci sono (righe con `attiva`), sul cursore ←/→ ±2 punti e Home/Fine 0/100, Invio o Spazio commuta, apre la pagina di Windows o fa muto sì/no; ↑ dalla prima fila all'azione della testata.
- Produces (Rust): `Ponte::campione_pc(&self, stat: StatPc, cpu_pronta: bool)` (tiene l'ultimo, scrive le misure solo a pagina aperta e solo se cambiano, aggiorna la carta come oggi); `Ponte::scrivi_pagina_pc(&self)`.

- [ ] **Step 1: Test che falliscono** (i test della vista PC rinascono qui **prima** di cancellarla):
  - `misure_dicono_nome_e_valore` (da `la_vista_pc_dice_nome_e_valore`, e F1 Task 10): ogni carta è un `groupbox` col nome intero («Memoria, 48 %, 7,7 di 16 GB»); nessun nome per Narrator è tutto maiuscolo.
  - `misure_geometria` (da `vista_pc_entra_nel_pannello`, `vista_pc_un_solo_stile_e_ritmo_4_8`): quattro carte 182 × 84 a y 48, due file di righe da 36 entro 752 × 224, testi ≥ 12 px; `pagine_dentro_il_corpo` per la vista `pc`.
  - `righe_wifi_e_bluetooth` (da `riquadri_wifi_e_bluetooth`): ruolo `Switch` e `checked`, Invio e clic → `commuta-radio(0, …)`; «Non presente» non raggiungibile da tastiera.
  - `riga_volume_da_tastiera`: ←/→ ±2 punti su `imposta-volume`, Home/Fine, Invio → `muto-volume`; il cursore segue `volume` senza aspettare il modello.
  - `riga_cuffie` (da `riga_cuffie_nella_vista_pc`) e `rimandi_a_windows`: Invio sul secondo rimando → `apri-pagina(1)`.
  - `misure_senza_radio_ne_volume` (Review Focus 4): fuori da Windows e senza audio nessun bersaglio di comando, Tab passa dalla testata alle schede senza fermarsi su nulla di morto.
  - `misure_testi_lunghi_nei_posti` (Review Focus 2): nome della rete di 32 caratteri, «1023,9 di 1023,9 GB» → ellissi nel posto, nome intero per Narrator.
  - `con_pagine::test::pagina_pc_scritta_solo_se_cambia` (Review Focus 5): due `campione_pc` uguali → `RIGHE_SCRITTE` invariato; a pagina chiusa un campione aggiorna solo la carta; aprendo la pagina l'ultimo campione è già scritto.
  - `con_pagine::test::esito_a_pagina_pc_lasciata_ignorato` (da `esito_a_vista_pc_lasciata_ignorato`).
  - Cancellati qui: i test della vista PC elencati sopra e quelli di `stat_ui` in `pannello.rs` (le cifre rinascono in `pagina_pc::test`).
- [ ] **Step 2:** `cargo test --workspace misure_ righe_wifi riga_volume riga_cuffie rimandi_a pagina_pc_scritta esito_a_pagina` → FAIL.
- [ ] **Step 3: Implementa**; via il `allow(dead_code)` del Task 6.
- [ ] **Step 4:** `cargo test --workspace` → PASS; clippy pulito.
- [ ] **Step 5:** foto della pagina PC dell'istanza di prova a 100 % e 175 %, confronto con `tc-pc.png` (le misure più basse della proposta C sono una differenza voluta); nessun interruttore toccato; foto cancellate.
- [ ] **Step 6:** commit `feat(assistente): impianto Misure, la pagina PC ci si sposta`.

### Task 8: Chiusura della fetta

**Files:**
- Modify: `CHANGELOG.md` («Ridisegno F3, chiusura»), `README.md` (pagine Casa, Collegamenti e PC), `docs/superpowers/specs/2026-11-20-assistente-ridisegno-design.md` (riga «F3 fatta» nel §8, con ciò che il piano ha deciso diversamente e le risposte di Paolo), `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` (punti chiusi segnati; sezione «Dopo F3» coi nuovi rimandati)

- [ ] **Step 1: Verifica completa**: `cargo test --workspace` (numero annotato; F2 chiudeva a 1052), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release -p assistente`.
- [ ] **Step 2: Misure** con `tools\misura.ps1 -Secondi 60` sull'exe release, istanza di prova e `finto_ha`, stesse condizioni del Task 1: a riposo **RAM privata ≤ partenza + 0,2 MB e CPU ≤ 0,05 %**; Panoramica ferma **≤ 30 MB** e CPU non sopra la partenza; pagine Casa, Collegamenti e PC aperte e ferme ≤ 30 MB e CPU non sopra la partenza del Task 1 (+0,01 di tolleranza, due prove); **10 aperture e chiusure** sulla pagina PC (radio che nascono e muoiono) e 10 sulla pagina Casa → RAM 15 s dopo ogni chiusura piatta. Fotogrammi a riposo dai `RedrawRequested` (build usa e getta, mai committata): a pagina ferma solo il battito; un tocco su una card e un campione del PC non lasciano fotogrammi dopo la loro transizione. Cifre nel CHANGELOG come per F2.
- [ ] **Step 3: Confronto visivo** sull'exe finale: `tc-casa-luci`, `tc-casa-stanza`, `tc-pc`, la pagina Collegamenti, e la Coda col permesso aperto rimasta da F2, a 100 % e 175 %, solo la finestra dell'app, cancellate dopo.
- [ ] **Step 4: Revisione dell'intera fetta** su **Opus** (codice dei Task 1–7 da `fba35e4` + foto affiancate), poi le correzioni in un'ondata sola.
- [ ] **Step 5: Prova dal vivo con Paolo** (la sua isola, una riapertura, col suo ok): casa vera su Casa (filtri, una luce accesa e spenta dalla card, «Spegni tutte» e «Spegni la stanza», una scena **solo se Paolo la sceglie**, sola lettura, casa scollegata e «Riprova»); Collegamenti (un lancio, il più usato che cambia nella carta); PC (misure, Wi-Fi e Bluetooth **solo col suo ok**, volume, cuffie); la carta Agenti con un permesso («1 ti aspetta»); tastiera e **Narrator con UI Automation vera** su filtri, card-interruttore, sola lettura, misure e righe; anello del fuoco nella griglia che scorre a 125/150 %; pagine a 100 % e 175 %. Con loro le prove rimaste da F2: un permesso negato e uno consentito dalla pagina Agenti (anche dalla pillola), dalla Coda e dalla voce della Panoramica; «Consenti» entro 600 ms ignorato; permesso lungo consentito solo dopo il fondo; doppio clic reale su «Nega» con due permessi; tocco, penna e touchpad che non decidono un permesso; Esc nel permesso di Agenti; nomi delle sessioni = titoli delle schede. Esito nel CHANGELOG.
- [ ] **Step 6: Pulizia** di `target\agente` (incrementali e artefatti dei giri), delle foto e dell'istanza di prova; `finto_ha -- pulisci` per il token finto nel portachiavi.
- [ ] **Step 7:** commit `docs: chiusura del ridisegno F3 (misure, prova dal vivo)` (con l'ok di Paolo; nessun push senza un suo sì).

---

## Auto-revisione

- **Copertura della spec**: §5.6 Casa (filtri con «★ Preferiti» prima, «Luci accese» col numero, «Tutta la casa», stanze; card-interruttore `switch`; sola lettura senza riempimento; scene; «Spegni tutte» primaria; «N luci accese» porta qui filtrato) → Task 3–4; Collegamenti (card «lancio», più usato, 3 clic per gli altri, conteggio già su disco) → Task 5; PC (quattro misure mono 28 con barra, righe di stato, cursore e Wi-Fi/BT come tipi dell'impianto, «Muto» secondario) → Task 6–7, con la sola deroga dell'altezza 84 (Domanda 4); stati di pagina vuoto / fuori servizio / primo avvio → Task 3, 5, 6; §5.10 `r1` → Task 2; §7 invariata e nessun dato nuovo → Global Constraints; §11 test, confronto visivo, misure, prova dal vivo → Task 8; §13.1 fuoco riparato, §13.7 limiti in pixel (`r1`), §13.10 simboli SVG → Task 2, 4, 7.
- **Coerenza dei nomi**: `R1`, `CardUi`, `FiltroUi`, `MisuraUi`, `RigaMisuraUi`, `TipoCardUi`, `TipoRigaMisuraUi`, `comandi-*`, `misure-*`, `vista_comandi`, `scrivi_pagina_casa/collegamenti/pc`, `campione_pc`, `filtro_pagina`, `tocca_card`, `chiedi_filtro` sono definiti una volta (Task 2, 3, 4, 6, 7) e usati con lo stesso nome dopo. Le strutture esistenti si chiamano come nel codice (`RigaModuloUi`, `CampoDettaglioUi`, `SintesiUi`, `StatoPaginaUi`), non come nel piano di F2.
- **Review Focus**: ogni riga ha il suo test nel task che possiede il codice.
- **Rischi noti**: il Task 4 è il più grande (impianto, pagina, tastiera, via la vista Casa); se la revisione lo trova troppo largo si divide fra impianto con le card (4a) e tastiera con scene (4b). Le risposte di Paolo alle domande 1–4 toccano solo `pagina_casa::filtri`/`scene` e le costanti di `GeometriaComandi`/`ImpiantoMisure`.

## Domande per Paolo (risposte del 24/11: 1A, 2A, 3A, 4C)

1. **Casa: da dove si parte e cosa fa «★ Preferiti» se non ne hai scelti?** (spec: «★ Preferiti» prima voce; la bozza approvata non ce l'ha)
   - **A (raccomandata)**: «★ Preferiti» c'è solo se hai scelto dei preferiti, ed è lì che la pagina si apre; senza preferiti si apre su «Tutta la casa». Niente preferiti «finti».
   - B: «★ Preferiti» c'è sempre; senza preferiti dice «Nessun preferito» con «Scegli i preferiti» (Impostazioni › Dispositivi).
   - C: come oggi: senza preferiti scelti mostra i primi 8 dispositivi delle stanze.
2. **Scene: dove stanno?** (il mapper non lega le scene a una stanza)
   - **A (raccomandata)**: fila «SCENE» di chip in fondo alle card, come nella bozza, in ogni filtro tranne «Luci accese»; sparisce il filtro «Scene».
   - B: la fila solo in «★ Preferiti» e «Tutta la casa», non nelle stanze.
   - C: come oggi: un filtro «Scene» in fondo all'elenco, con le scene come card.
3. **PC: quali quattro misure?**
   - **A (raccomandata)**: CPU, Memoria, Disco (GB liberi, come la bozza), Batteria; senza batteria la GPU; senza tutte e due il quarto posto resta vuoto.
   - B: come A, ma il disco in percento occupato come oggi.
   - C: CPU, Memoria, Disco, GPU sempre; la batteria passa fra le righe.
4. **PC: Wi-Fi, Bluetooth, volume e «Reti Wi-Fi ›» sotto le misure** (con le misure alte 116 della bozza resta posto per una sola fila di righe)
   - A: misure come la bozza (116) e una fila sola a vista (Wi-Fi e Bluetooth con l'interruttore); volume e rimandi in una seconda fila che si raggiunge scorrendo.
   - B: misure come la bozza e solo le due righe della bozza; il volume resta nella barra (rotella) e «Muto» nella testata, i rimandi a Windows spariscono dalla pagina.
   - **C (raccomandata)**: misure più basse (84: la riga piccola va accanto al valore) e due file a vista senza scorrimento: Wi-Fi · Bluetooth con l'interruttore, Volume · rimandi a Windows.
