# Ridisegno F1 — punti rimandati, debiti e prove dal vivo

Raccolti dal registro di lavoro della fetta F1 (23/11/2026) prima di cancellarlo. Da leggere nel piano di F2 e successivi.

## Decisioni del coordinatore

- Ruling: «Consenti» resta neutro come «Nega» (regola M3), non bianco primario come nelle bozze — Paolo ha detto «procedi» senza scegliere, applicata la raccomandazione prudente — costo: un ritocco di stile se Paolo lo vuole bianco.
- Ruling C1: Consenti neutro ovunque (togliere riga 24 del piano; RegolaPulsanti.tipo invariata; test rinominati *_nega_e_consenti_sono_neutri, puntati sui nuovi file dopo che attesa.slint sparisce) — regola M3 — costo: nessuno.
- Ruling C2: testi delle scale accorciati come in preflight.md («Collega Claude», «7 giorni 61 %», «A riposo», «Scollegata», «{n} luci», «{n} link», «Nessun link»; temperature: l'implementatore sceglie un testo che sta nei limiti, provato dal test) — i limiti in pixel sono vincolanti (§13.7) — costo: parole da ritoccare in F2–F3.
- Ruling C3: cfg_attr(not(test), allow(dead_code)) sul modulo moduli dal Task 2 al Task 4; eliminati OraGrande, Registro::config(), fissati, Fonte::nome(); Capsula solo #[cfg(test)]; campi di FuoriMisura stampati esplicitamente — clippy -D warnings — costo: nessuno.
- Ruling C4: il Task 3 dichiara il cambio d'ordine (Agenti primo) in CHANGELOG e aggiorna gli indici del test del dock.
- Ruling C5: regola unica per i token non elencati (neutro→g3/g4, azione→primario, pericolo→errore, vetri→g1/g2/filo); «? 500 : 400» → «? 700 : 500»; il test iniziali_dei_collegamenti si aggiorna, non si perde.
- Ruling C6: Occhietti nasce nel Task 6 in ui/guscio/occhietti.slint, riusato in 7 e 8.
- Ruling C7: tracking mono .07em.
- Ruling C8: «›» disegnato come Path, omesso nel nome per Narrator (§13.10).
- Ruling C9: i file mancanti elencati in preflight.md fanno parte dei rispettivi task (controllare anche pc_stats.rs).
- Ruling C10: tolto il parametro inutile di fascia().
- Ruling C11: «N in attesa ›» e fascia nell'ordine di Tab dopo la voce e prima di carte/tessere (§5.9).
- Ruling C12: fatto non visto = petalo-3 blu; gestione_protocollo::testo_pulito; moduli_aggiorna("pc") alimentato dal punto dove arrivano gli StatPc; camelCase ultimoLancio; ConfigModulo.id con serde(default); la scheda della barra mostra nome_breve; Task 7 sette casi; impostazioni_a_finestra_minima va creato; la rotella sul pulsante volume rispetta l'impostazione volume_rotella esistente.
- Ruling: Task 4 I1 — la riga per la sessione ferma in attesa di un tuo messaggio (non un permesso) ha titolo «Tocca a te» con tono Guarda, non «Ti aspetta» (parola riservata al tono Aspetta del permesso bloccante) — vocabolario chiuso della spec — costo: una parola da ritoccare se Paolo preferisce altro.
- Ruling: Task 6 — la luce che si accende in 1,2 s all'apertura del pannello con un tono acceso resta — l'apertura è una transizione ammessa dai vincoli, nessun costo a riposo — costo: nessuno.
- Ruling: Task 7 C1 — si corregge ORA nel Task 7, non si rimanda (sicurezza M3, spec 17/11 r.97 e dec. 11): il permesso nella Coda si apre col titolo a capo e il dettaglio intero scorrevole (comando in JetBrains Mono NL, sessione visibile), «Consenti» raggiungibile solo dopo aver visto il fondo del testo come in M3; test sostitutivo di titolo_della_voce_espansa_va_a_capo. Il brief chiedeva titolo in ellissi: prevale il vincolo «Sicurezza di Claude invariata» — costo: un po' di lavoro in più nel Task 7, la Panoramica (Task 8) riusa lo stesso dettaglio.
- Ruling: il ritorno dopo la chiusura sopra +2 MB è in gran parte preesistente; si accetta per F1 a condizione che la revisione finale verifichi che non cresca con aperture ripetute (nessuna perdita) — costo se sbagliato: memoria che sale nel tempo.
- Ruling: n1 (fuoco perso se la voce A è sostituita da una B con solo pulsante delicato) e n2 (testo sopra «Apri» nel caso m3) parcheggiati, da correggere all'inizio di F2 prima che qualunque modulo produca voci con un solo pulsante delicato — oggi il caso non è raggiungibile, nessuna seconda ondata ammessa — costo se sbagliato: tastiera persa fino a Tab in un caso oggi impossibile.

## Rimandati, debiti, prove dal vivo

- Task 1: minor (deferred): commento in tab_porta_in_vista_la_soglia_di_spesa su «non trovata = sotto il bordo».
- Task 1: minor (deferred → Task 10): sovrapposizioni g1 su righe g1 (SceltaIcona, ConfermaElimina, popup Scelta) staccate solo dal filo ≈1,2:1.
- Task 1: minor (deferred → Task 10): traccia g4 ≈1,5:1, archi/barre vuote più deboli.
- Task 1: minor (deferred): primario-sopra non nel test dei contrasti; colore_del_token senza limite su alias ciclici.
- Task 1: minor (deferred → Task 10): suggerimento g2 sulla riga Claude in hover g2, staccato solo dal filo; menu-fondo alias di suggerimento.
- Task 2: minor (deferred → carry to Task 6/8 briefs): nome di carta/scheda NON con il componente Etichetta (12 px) ma con token di corpo/tracking da Tema condivisi con MisuraTesto (o componente EtichettaCarta); geometria_schede misura con Stile::Etichetta.
- Task 2: minor (deferred): MARGINE 3 px largo per il mono; «…» non nei testi del confronto; accorcia con massimo < «…»; avviso unused notify-rust preesistente.
- Task 3: minor (deferred → Task 4): test da capo a fondo assistente.json con «claude» → collega → dock (oggi &c.moduli→&[] resterebbe verde); verificare che livelli_voci sia collegato in attese_cambiate; «—» nel nome Narrator; tono calcolato due volte in moduli_nel_pannello.
- Task 3: DEBITO F6: reintrodurre Registro::config() col test «scrive agenti» e portare su agenti il «fissato» di una voce claude; moduli di tipo sbagliato → config di default (rivedere quando moduli si scrive).
- Task 3: minor (deferred): CHANGELOG «non fa più» → «non fa».
- Task 4: minor (deferred): frase_breve confronta stringhe (meglio frase_errore_breve); sensore di dentro senza virgola decimale; dopo «Spegni tutte» il bottone resta fino alla conferma HA; Tono::luce solo test non legato a Tema.luce-tono; asserzione di da_chiuso_non_si_ricalcola che non può fallire.
- Task 4: minor (deferred → final review / F2): claude.rs:384 dice ancora «Ti aspetta» per la riga della vista Agenti con livello Attenzione (tono Guarda) — stesso doppio uso della parola.
- Task 5: minor (deferred): assert !.tmp.exists() non può fallire; collegamenti.aggiungi senza test; ora del lancio da con_claude::adesso_unix (accoppiamento).
- Task 6: minor (deferred): k1/k2 della luce cambiano di colpo fra «ti aspetta» e gli altri toni; foto task-6-foto da cancellare alla chiusura.
- Task 6: minor (deferred → final review): pillola «Volume 40%» senza spazio, diverso da barra/PC «Volume 40 %».
- Task 7: REQUISITO per Task 8 (da rereview-2 n1): la voce della Panoramica che apre un permesso non deve far nascere «Nega» sotto il punto del clic (doppio clic sulla 2ª carta nega senza volerlo), oppure deve ignorare il secondo clic; test.
- Task 7: minor (deferred): nessun test isola il riarmo al fondo di un testo lungo; doppio_clic_su_apri_non_consente prova solo la posizione; minor m1–m3 di task-7-rereview-1.md; testo dell'avviso di prova (attenzione.rs:485) parla ancora di «voce espansa»; un permesso aperto che cambia posto si richiude (sicuro, ma si deve riaprire).
- Task 7: da provare dal vivo (Task 11): entrare nella Coda e cliccare «Consenti» entro 600 ms è ignorato; permesso lungo: Consenti solo dopo il fondo.
- Task 8: minor (deferred → final review): m1 «+N» a 2 px dal «Nega» della riga 0 della Coda; m2 guardia 600 ms sul dal condiviso, Nega ignorato senza segno; m3 indice-nega su un pulsante delicato fragile; m4 `intera` non considera il titolo in ellissi; m6 legame Stile::Etichetta↔Tema.etichetta-* solo commento.
- Task 8: REQUISITO per Task 10: centro-a a tutto il corpo (224) per tutte le viste.
- Task 8: da dire a Paolo: dalla Panoramica un permesso vero mostra sempre «Apri», mai «Consenti» (scelta prudente).
- Task 8: da provare dal vivo (Task 11): Narrator con UIA vera sulle carte; doppio clic reale su «Nega» con due permessi; luce della carta a 125/150 %.
- Task 9: minor (deferred): test doppio clic «{n} avvisi ›»→Coda; «solo se cambia» confronta stringhe e manca il test «non riscrive»; nessun test sulla scala 0,96; animazioni a vuoto uscendo dalla Panoramica.
- Task 9: da provare dal vivo (Task 11) / mostrare a Paolo: Narrator su fascia e avvisi; scala 0,96 a 125/150/175 %; 32 px vuoti fra voce e tessere nell'Orologio.
- Task 10: minor (deferred → F2 vista Agenti): righe «Errore» della vista Agenti in blu (livello Attenzione) mentre la carta le mostra rosa come errore — parola/tono discordanti (preesistente).
- Task 10: minor (deferred): pc.slint:1143 to-uppercase() senza accessible-label; foto Agenti con riga «Ti aspetta» finta; foto Impostazioni «100 %» identiche a 175 %; ramo «testo accorciato» di pulsanti_tagliati mai visto fallire; linter del maiuscolo largo.
- Task 10: da vedere dal vivo (Task 11): sovrapposizioni aperte, anello del fuoco (filo nero dentro i Flickable, 1 px in più) a 125/150 %.
- Ruling: n1 (fuoco perso se la voce A è sostituita da una B con solo pulsante delicato) e n2 (testo sopra «Apri» nel caso m3) parcheggiati, da correggere all'inizio di F2 prima che qualunque modulo produca voci con un solo pulsante delicato — oggi il caso non è raggiungibile, nessuna seconda ondata ammessa — costo se sbagliato: tastiera persa fino a Tab in un caso oggi impossibile.

## Triage della revisione finale

Vedi la sezione «Possono aspettare (F2+)» riportata qui sotto.

### Possono aspettare (F2+)

- **Task 1**: commento in `tab_porta_in_vista_la_soglia_di_spesa`; `primario-sopra` fuori dal test dei contrasti; `colore_del_token` su alias ciclici.
- **Task 2**: `MARGINE` 3 px; «…» fuori dal confronto; `accorcia` con massimo < «…»; avviso `notify-rust` preesistente.
- **Task 3**: **DEBITO F6** (`Registro::config()`, `fissato` da `claude` ad `agenti`, voci illeggibili di `moduli` che azzerano la configurazione). Va nel brief di F6. In F1 nessuno scrive `moduli`, quindi non c'è rischio reale.
- **Task 4**: `frase_breve`, virgola del sensore, bottone «Spegni tutte» fino alla conferma di HA, `Tono::luce` solo test, asserzione che non può fallire in `da_chiuso_non_si_ricalcola`.
- **Task 5**: `!.tmp.exists()` che non può fallire; `collegamenti.aggiungi` senza test; `adesso_unix` preso in prestito da `con_claude`.
- **Task 6**: k1/k2 della luce che cambiano di colpo.
- **Task 7**:
  - m1-m3 di `task-7-rereview-1.md`;
  - n3 `doppio_clic_su_apri_non_consente`: il nome promette più di quanto prova, da rinominare;
  - permesso aperto che cambia posto e si richiude (sicuro).
- **Task 8**:
  - m2 / qui m4: «Nega» muto senza segno → F2, con un aspetto inerte;
  - m4 / qui m5: test titolo↔`MASSIMO_STRUMENTO`;
  - m6: legame `Stile::Etichetta`↔`Tema.etichetta-*`.
- **Task 9**: test del doppio clic «{n} avvisi ›», test «non riscrive» della fascia, test della scala 0,96, animazioni a vuoto uscendo (qui m7).
- **Task 10**:
  - `pc.slint:1143` `to-uppercase()` senza `accessible-label`: da sentire con Narrator dal vivo; se lo legge lettera per lettera va corretto prima del merge;
  - «Errore» blu nella vista Agenti → F2;
  - foto di Agenti e delle Impostazioni a 100 %;
  - ramo di `pulsanti_tagliati`;
  - linter del maiuscolo.
- **Già chiusi in anticipo dal Task 10**: «Tocca a te» in `claude.rs`, «Volume 40 %» nella pillola, testo dell'avviso di prova.

### Requisiti e «da provare dal vivo»: stato

- **REQUISITO Task 7 → Task 8** (nessun «Nega» sotto il clic che apre un permesso dalla Panoramica): **soddisfatto**.
  - «Apri», «+N» e la carta portano a un permesso aperto, il cui «Nega» sta in alto (y 46-78), lontano dai pulsanti della voce (in basso a destra).
  - Resta il margine stretto di «+N» verso le righe dell'elenco (m1).
- **REQUISITO Task 8 → Task 10** (`centro-a` = 224): **soddisfatto** (Task 10, `misure_slint_e_rust`).


## Dopo F2 (24/11/2026)

Stato dei punti di F1 e rimandati nuovi della fetta F2 (registro di lavoro `.superpowers/sdd/2026-11-23-assistente-ridisegno-f2/`). La revisione dell'intera fetta (Task 9 Step 4) e la prova dal vivo con Paolo (Step 5) non sono ancora fatte: i punti che le riguardano sono sotto «Da provare dal vivo» e «Per la revisione finale».

### Punti di F1 chiusi in F2

- **n1** (fuoco perso se la voce A è sostituita da una B con solo pulsante delicato) e **n2** (testo sopra «Apri»): chiusi nel Task 1, con le regole anche per `init`, `vai`, `voce-altre`, `in-attesa` e ↑.
- **Task 8 m2 / «Nega» muto senza segno**: chiuso nel Task 4 (aspetto inerte per i 600 ms, componente `Inerzia`).
- **Task 8 m5 / test titolo↔`MASSIMO_STRUMENTO`**: chiuso nel Task 1 (`titolo_della_voce_con_strumento_al_massimo`).
- **«Errore» blu nella vista Agenti** (Task 4 e Task 10 di F1): chiuso nel Task 5, la sessione in errore è rosa.
- **Task 7: permesso aperto che cambia posto si richiude**: chiuso nel Task 3 (la validità la decide l'id).
- **Task 7: testo dell'avviso di prova che parla di «voce espansa»**: chiuso nel Task 8.
- **Task 7: nessun test isola il riarmo al fondo di un testo lungo**: chiuso per la pagina Agenti (`agenti_fondo_del_testo_riarma`, Task 7).
- **Task 8 m1** («+N» a 2 px dal «Nega» della riga 0 della Coda): affrontato nel Task 3 (i pulsanti del permesso nella Coda sono alti 32 e finiscono 4 px sopra le righe, con test sui rettangoli interi).
- **Task 4 / Task 10: vista Claude e `VoceAperta`**: tolte (Task 6 e Task 3), con i loro test rinati sulle superfici nuove.

### Di F1 non portati in F2 (restano aperti per F3+)

- **Task 8 m6**: legame `Stile::Etichetta`↔`Tema.etichetta-*` solo a commento.
- **Task 9 (minor)**: test del doppio clic «{n} avvisi ›»→Coda, test «non riscrive» della fascia, test della scala 0,96, animazioni a vuoto uscendo dalla Panoramica.
- **Task 2 (minor)**: nome di carta/scheda con token di corpo/tracking condivisi con `MisuraTesto` (o componente `EtichettaCarta`).
- **Task 7 m1–m3 di `task-7-rereview-1.md`**: il file è andato perso; la revisione dell'intera fetta ricontrolla la Coda col permesso da capo.

### Nuovi rimandati di F2

- Task 1: percorsi extra (`init`, `vai`, `voce-altre`, `in-attesa`, ↑) senza test con voce a solo pulsante delicato; asserzioni «Invio non concede» che non possono fallire (usare Ctrl+Invio); la regola C20 riarma senza `Permessi.chiede`; fuoco carta 1→0; `solo_delicato` duplica `solo_consenti`; CHANGELOG «Tab va avanti» non provato.
- Task 2: il test di `chiesto` non prova il fuso (passare il fuso a `permesso_ui`); `testo_notifica` dovrebbe leggere la sessione da `v.permesso.sessione`; costruttore condiviso per l'`EventoHook` di permesso nei test.
- Task 3: passo di 24 px nel test del doppio clic per dimezzare il tempo della suite.
- Task 4: la pillola (altra finestra, solo «Consenti» protetto, senza armamenti) non si mostra inerte: oltre F2 (F4).
- Task 6: helper `tasto`/`etichetta_col_fuoco` ripetuti; voce della casa con `creata` nel futuro nel test; nessun test della nota su due righe con titolo lungo; senza chiave la cartella solo nel dettaglio; filo g4; «Collega» primario.
- Task 8: O1, la regola «una volta sola» di `permesso-chiesto` dipende da `vista_claude` falso a ogni apertura: commento o `debug_assert!` in `pannello_aperto`.
- Task 9: misura a riposo del pannello (con la finestra visibile) più alta di 0,02 punti di CPU di F1 a parità di sessione; da riguardare se cresce.

### Esito della revisione finale (24/11/2026)

La revisione è conforme alla spec. Ha trovato un solo Important, I1: nessun test vedeva «Consenti» spostarsi dentro la vista. È chiuso in 590a0b1 con `consenti_a_posto_fisso_col_testo_che_cambia`, ricontrollato su Opus. Nella stessa ondata sono chiusi:
- **m1:** nella riga col permesso si accorcia prima il nome;
- **m5:** il commento della difesa in più.

Le mutazioni P e O e lo slittamento di 26 px sono tutti uccisi.

Rimandati:
- **m2 (preesistente, F1):** la carta Agenti dice «Tocca a te» con la capsula «Aspetta», mentre la pagina dice «1 ti aspetta / Chiede: …». Da allineare in F3, `moduli/agenti.rs:77`.
- **m3:** CPU del pannello fermo +0,02 punti. L'ipotesi è l'albero più grande, cioè i due `Permesso` sempre vivi. Va misurata prima di F3 con una build usa e getta senza i due `Permesso`. Nella stessa occasione va fatta la misura mancante della pillola a riposo, a schermo sbloccato (`f9-frames.ps1 -Scenario riposo`).
  - **Esito (F3, Task 1, 24/11/2026): ipotesi NON confermata.** Con `permesso.slint` svuotato la CPU di `--apri` scende di 0,016 e 0,010 punti rispetto alla stessa exe misurata subito dopo (0,012 e 0,023 rispetto alla media di quattro prove): non di 0,015 in entrambe, e il rumore fra prove della stessa exe (0,077–0,118 %) è più grande dell'effetto. Nessun rimedio in F3 (toccherebbe il permesso); regola per F3: gli impianti nuovi stanno dentro `if vista == …`. Misurata a PC bloccato; ripetibile a schermo sbloccato.
  - **Pillola a riposo a schermo sbloccato: ancora da misurare** (il PC era bloccato anche nel Task 1 di F3: contati 29 fotogrammi nei primi 5 s e nessuno dopo, cioè pillola non disegnata). Script e build usa e getta pronti nella scratchpad (`f3-frames.ps1`).
- **Test di m1:**
  - nessun test protegge le righe senza permesso dalla regola nuova (mutazione R2b sopravvissuta);
  - la stima di `misura.rs` (13 px meno 3 px di margine, Inter Medium contro il peso di base) non vede un taglio sotto i 3,6 px.
- **Fuori da F2:** la build dei test stampa l'avviso `unused dependency notify-rust` (cargo 1.100), preesistente da `a8d917f`.

### Per la revisione finale

- Task 3: mutazioni non eseguite D2, E, F, G, H, K, L, N, O, P, R (patch in `scratchpad\mut3p\`, `rerev3b.sh`): provare per prime P (posto invisibile di «Consenti» cliccabile) e O (posto di «Nega» che si sposta). Posizioni durante lo scivolamento di 26 px in 280 ms dalla Panoramica non verificate.
- Task 1/4: doppio riarmo possibile (`consenti-compare` del C20 più `claude_riarma`) quando A→B passa da «Apri» a «Consenti»: il conteggio degli `armamenti` lo tiene presente (innocuo).

### Da provare dal vivo con Paolo (Task 9 Step 5)

Il 24/11/2026 Paolo ha installato la build 2cb8de4, ha provato il permesso dalla pagina Agenti e ha dato l'OK per passare a F3. Le altre prove di questo elenco passano alla prova dal vivo di F3, insieme alla foto della Coda col permesso aperto.

- Claude Code vero con un permesso **negato e uno consentito** dalla pagina Agenti (anche arrivandoci col clic sulla pillola), dalla Coda e dalla voce della Panoramica; nella Coda «Consenti» entro 600 ms ignorato (aspetto inerte visibile) e permesso lungo consentito solo dopo il fondo.
- **Doppio clic reale su «Nega» con due permessi** (Panoramica e Agenti); la voce con un solo pulsante delicato (`--prova-avviso`); nomi delle sessioni = titoli delle schede di Claude Code.
- Tastiera e **Narrator con UI Automation vera** su righe, dettaglio, permesso e carte; anello del fuoco nella colonna che scorre e nel testo del permesso a 125/150 %; pagina Agenti a 100 % e 175 %.
- **Tocco su uno schermo touch vero, penna, touchpad** (clic senza muovere il cursore): un tocco non decide un permesso (regola del movimento, Task 7); Esc nel permesso di Agenti chiude il pannello; il riquadro del comando mostra circa due righe alla volta (un testo lungo chiede molti ↓).
- Fuori da F2, nello stesso giro se Paolo vuole: volume, Griglia/Orologio, fascia e avvisi, scala 0,96, luce della carta a 125/150 %, `pc.slint` `to-uppercase()` con Narrator.

### Da dire a Paolo

- «Apri» della voce nella Panoramica porta ancora alla Coda; è la pillola che porta alla pagina Agenti (la decisione 5 riguarda solo la pillola).
- Subito dopo l'apertura dalla pillola un Invio nega (il fuoco è su «Nega»; nei primi 600 ms lo ferma la guardia).
- Esc nel permesso di Agenti chiude il pannello; col touchscreen un tocco non decide un permesso (usa Ctrl+Invio o il mouse): se Paolo usa il touchscreen va rivisto.
- Scelte del coordinatore da confermare o togliere: «Torna all'elenco» come pulsante per il mouse (costo: un pulsante da togliere); l'etichetta sopra il campo secondo lo strumento («TERMINALE», «FILE», «INDIRIZZO», «RICERCA»); senza chiave da amministratore la nota della spesa in chiaro nella testata.
