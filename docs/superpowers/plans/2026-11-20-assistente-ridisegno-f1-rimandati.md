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

