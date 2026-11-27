# Assistente "isola" — Ridisegno: piattaforma di moduli con il linguaggio di TwoCode — Design

Data: 20/11/2026. Ramo `v3-assistente` (HEAD 2e63446). Decisa con Paolo nello studio grafico del 19–20/11. Si appoggia
alla spec generale `docs/superpowers/specs/2026-11-13-assistente-isola-design.md` (budget di leggerezza §4, verifica §9)
e alla spec M3 `docs/superpowers/specs/2026-11-17-assistente-m3-claude-design.md` (sicurezza, §10 canale protetto).

**Fonti che valgono come decisioni** (cartella `.superpowers/ux/`): `studio/05-brief-sistema.md` (contratto del modulo,
regole del guscio, risposte di Paolo in fondo), `studio/06-sistema.md` e `studio/07-critica-sistema.md` (le regole
sopravvissute), `studio/08-fattibilita-sistema.md` (contratto in Rust, componenti Slint, fette, spike),
`studio/09-linguaggio-twocode.md` (il linguaggio di TwoCode) e **`studio/10-tc-isola.md` (il disegno approvato)**, con la
bozza `studio/bozze/tc-isola.html` (+ `tc-isola.css`, `tc-isola.js`), `tc-stati.html`, `tc-colori.html` e le schermate
`studio/bozze/screenshot/tc-*.png`. Le misure fini (spazi, pesi, alfa delle luci) si prendono da `tc-isola.css`; questa
spec dice cosa vale e perché, non ripete ogni pixel.

## 1. Perché

Paolo vuole una **UX chiara e semplice che faccia vedere tutto velocemente e agire in un clic, con un layout moderno**
(principio guida del 18/11: «UX chiara e semplice ma vedere tutte le informazioni velocemente; layout moderno, chiaro e
strutturato, ottimizzato per le dimensioni e le funzionalità»). L'app di oggi non lo fa: la Panoramica (Orologio o
Quadranti) non dà un fatto per area e non permette di agire; tutto ha lo stesso peso.

Il secondo motivo è strutturale. isola **è una piattaforma**: oggi ospita 4 moduli (Claude, Casa, PC, Collegamenti),
la roadmap ne porta 12 e oltre (HIRIS, Focus, Mail, Documenti, Agenda, altri agenti…). Paolo ha respinto i disegni a
quattro zone con una domanda: «vedo 4 sezioni ma domani potrebbero essere 5, 6: dovremmo rifare la grafica?». La
risposta di questo ridisegno è no: **un modulo nuovo consegna dati, non grafica**, e la cornice sa impaginare 1, 4, 8 o
12 moduli con le stesse regole.

Il terzo è il gusto. Il punto di partenza visivo è **TwoCode** (isola nera opaca, luce che sale dal basso, bianco =
azione primaria, Inter + mono), che Paolo trova matura e piacevole; la si prende **ottimizzata** per leggibilità,
accessibilità e Windows, con le tinte di HIRIS al posto di quelle di TwoCode.

## 2. Obiettivi, non obiettivi, anti-obiettivi

**Obiettivi**
1. In 1 secondo dalla Panoramica: «tutto tranquillo» oppure «questo ti aspetta, di questo modulo».
2. Le azioni frequenti in ≤ 2 clic da pannello chiuso; il permesso di un agente in 1 (pillola).
3. Un **contratto del modulo** (identità, riassunto a livelli, voci, azione, impianto di pagina) e un **guscio** che
   impagina da solo da 1 a 12+ moduli: aggiungere un modulo non richiede componenti né misure nuove.
4. Il linguaggio visivo di TwoCode ottimizzato (§5.2), uguale in pillola, Panoramica e pagine.
5. Accessibilità di serie per ogni modulo: tastiera completa, anello di fuoco visibile, nomi per Narrator dal contratto.
6. Migrazione a fette che si rilasciano, ciascuna lasciando isola completa e verde (§8).

**Non obiettivi**
- Nuovi moduli (Focus, Mail, Agenda, Musica…): il disegno li prevede e li prova col **modulo di prova**, non li costruisce.
- HIRIS funzionante: esiste solo la riga «Chiedi a HIRIS…» della ricerca, inerte (§5.7).
- Codex e Gemini CLI: il modulo «Agenti» è pronto ad accoglierli, il collegamento vero è una milestone a sé.
- Ridisegno delle finestre **Impostazioni** e **Gestione**: tengono il loro impianto; in F1 ricevono solo i token
  nuovi e le correzioni di contrasto per stare sulla base nera (§8), più la sezione Panoramica/Moduli (§5.7).
- Tema chiaro, dimensione dell'isola 0,85/1/1,18, palette per daltonici come opzione (lo stato è già forma + parola),
  suoni, decorazioni di gioco (cuori, coriandoli, distintivi), moduli caricati a runtime (`ComponentFactory`, 08 §2.4).
- Apertura al passaggio del mouse e occhi che seguono il mouse (vedi §5.2, «Escluso»).

**Anti-obiettivi** (se succede, abbiamo fallito anche se «funziona»)
- Dover ritoccare la grafica per aggiungere, togliere o riordinare un modulo.
- Un modulo che impagina (a capo decisi da lui, testi accorciati a mano, componenti suoi).
- Un testo tagliato in silenzio, senza ellissi e senza il testo intero per Narrator e nel suggerimento.
- Uno stato detto solo dal colore.
- Carte che si spostano mentre Paolo guarda il pannello.
- Pannello o pillola più pesanti di oggi a riposo (§3, §11); un'animazione continua o un timer nuovo.
- Una qualunque protezione del permesso di M3 indebolita; dati delle sessioni scritti o tenuti più di oggi.
- Il fuoco da tastiera perso o Narrator muto quando un elemento appare o sparisce.

## 3. Vincoli

- **Leggerezza** (spec generale §4): nessuna animazione continua; animazioni solo durante una transizione (apertura,
  cambio pagina, cambio di tono); **nessun timer nuovo** (prestito calcolato all'apertura, arbitraggio della pillola per
  confronto di istanti, ricerca a ogni tasto); le proprietà Slint si scrivono **solo quando il valore cambia**
  (`aggiorna_modello`, `ponte.rs`); un modulo spento non costa nulla (niente thread, timer, letture, voci). Riferimento
  di oggi, misurato: ≈ 40 MB di working set, ≈ 14 MB di RAM privata, CPU ≈ 0,03 % a riposo.
- **Accessibilità già costruita, da non perdere**: fuoco da tastiera su ogni bersaglio, Narrator con ruoli e nomi,
  Esc a un livello per volta, nessun «‹ Indietro» (dec. 13), Ctrl+Invio per consentire.
- **Sicurezza di M3 invariata**: permessi, canale protetto, minimizzazione dei dati (§7).
- **Windows prima di tutto**: si disegna e si verifica su Windows 11 a 100–175 %; macOS e Linux compilano (CI verde,
  `clippy -D warnings`), senza verifica visiva.
- **Slint =1.18.1**, renderer Skia software (spec generale §5); niente sfocatura dello sfondo, niente `opacity` per le
  luci (alfa nei colori), una sola ombra per elemento.
- Tutto in italiano; nessun dato personale nel repo, nei test, nelle bozze o nei log.

## 4. Approcci considerati e scartati

| Giro | Che cos'era | Perché è stato scartato |
|---|---|---|
| **Bozze generiche** (13–19/11, `.superpowers/brainstorm/1980-…/content/`: «panoramica-struttura», «cinque-direzioni», «idee-originali» con le **sei idee più originali**, «ridisegno-tre-pagine») | Griglie di tessere, righe, orbite, una frase, corsie | Partivano dalla **forma**, non dalla funzione: gli stessi dati in contenitori diversi, tutte «già viste» (01 §0) |
| **Direzioni 1–4** (studi 01–04: Plancia, Sguardo, Il filo; poi «Il filo» rifinita, con permesso nella riga e Casa a fermate) | Panoramica a **4 zone fisse** Claude · Casa · PC · Collegamenti, un fatto e un'azione per zona, filo dei petali con nodi | **Quattro sezioni scritte a mano**: il quinto modulo avrebbe chiesto una grafica nuova (obiezione di Paolo, 05 §0) |
| **Sistema dei moduli «isola»** (studi 05–07, `bozze/sistema.html`) | Contratto + guscio: fascia «parlata» sotto gli occhi, tessere 182 × 128 sul filo con nodo di tono, compatti, identità viola-nero | Il **contratto e le regole reggono** e restano (§5.10, §5.5, §5.7); l'aspetto no: Paolo voleva partire da TwoCode («non come TwoCode»). Caduti filo, nodi, fumetto e fondo viola |
| **Varianti del giro TwoCode** (studio 10) | Misura di TwoCode 600 × 268 (`tc-pan-600.png`); «un colore per modulo»; segni di stato 1–4 (forme, icone a tratto, occhietti, sola parola) | 600 × 268 tiene solo 3 tessere senza riga; il colore per modulo non regge 12 moduli e confonde lo stato (Paolo intendeva la *disposizione* di «All apps», non i colori); fra i segni Paolo ha scelto la **variante 5 «misto»** |

## 5. Il disegno scelto

### 5.1 Guscio e moduli

Il **guscio** possiede misure, impaginazione, navigazione, Panoramica, pillola, coda «In attesa», ricerca, notifiche,
scorciatoie, tastiera, Narrator e movimento. Il **modulo** consegna dati e dichiarazioni (§5.10), mai impaginazione.
Un solo ordine dei moduli, scelto dall'utente, decide i posti nella Panoramica, nel dock e in «Tutte le app».

I moduli di oggi diventano: **Agenti** (oggi «Claude»: un solo modulo per Claude Code ora, Codex e Gemini CLI poi, con
i limiti per fornitore — decisione di Paolo), **Casa**, **PC**, **Collegamenti**. HIRIS **non è un modulo**: vive nella
cornice come «Chiedi a HIRIS…» unito alla ricerca, Ctrl+K (decisione di Paolo).

### 5.2 Linguaggio visivo: TwoCode ottimizzato (decisione di Paolo)

**Preso da TwoCode.** Isola nera opaca `#000` appesa al bordo alto, con le **orecchie** concave di 10 px e gli angoli
bassi a raggio 34; lo stato è **luce che sale dal basso** (radiale centrata sotto il bordo) nella pillola, nell'isola e
nella carta; **bianco con testo nero = azione primaria, grigio pieno = secondaria**, ovunque; Inter per le parole,
JetBrains Mono per numeri, ore, durate ed etichette maiuscole; **raggi proporzionali all'altezza** (pulsante 32 → 9,
40 → 12, carta 84 → 20, isola → 34); occhi 6 × 10 a ±8,5 con bagliore del loro colore e umore come forma (socchiusi =
lavora, tondi = chiede, ^ ^ = fatto); orologio grande Inter 700; **capsula delle schede** col nome solo sulla scheda
attiva; **cornice ferma** (riga alta + barra bassa: cambiando pagina si muove solo il corpo); «fatto» blu finché non
guardi, poi verde acqua che sfuma in 2,8 s; carte con nome-modulo mono maiuscolo + valore grande + riga (vista «All apps»).

**Ottimizzato (decisione di Paolo, con il perché in 10 §2).**
- Testo ≥ 12 px; mono maiuscolo ≥ 11 px con tracking .07–.08em; data e ora in testata 12–13 px.
- Contrasto AA misurato: t2 `#c4c4ca`, t3 `#9b9ba3` su nero e su `#18181b`; **la luce non passa mai sotto il testo dei
  contenuti** (picco sotto la barra, al più ~30 %; caso peggiore misurato al 50 %: icone 4,4:1, capsula 5,2:1).
- **Stato = colore + forma + parola** (§5.3).
- **Una sola famiglia di grigi**: `#0f0f11` (carta), `#18181b` (riga scelta), `#242428` (secondario), `#303035`
  (secondario al passaggio), filo `#232327`, bordo dei controlli `#74747c` (≥ 3:1).
- **Apertura al clic**, mai al passaggio; anello di fuoco di Windows (bianco 2 + nero 1) su ogni bersaglio; ruoli e nomi
  per Narrator.
- Bersagli ≥ 32 px (frequenti 36–40): schede 36, pulsanti 32/40.
- Occhi **sempre** presenti, anche nelle pillole d'attività (piccoli, scala 0,8).
- Niente pulsante «Sempre» nei permessi; «Nega» col fuoco iniziale; Ctrl+Invio per consentire.

**Colori: tinte di HIRIS, stesso uso del colore di TwoCode (decisione di Paolo).** «Nessun cambiamento all'uso del
colore, ma alle sole tonalità.» Stessi posti, intensità e sfumature; cambia la tinta, presa dai token di
`crates/assistente/ui/tema.slint`. **La luce segue lo stato, non il modulo** (decisione di Paolo): l'identità del
modulo la porta l'icona.

| Stato | Token | Testo/segno | Occhi | Note |
|---|---|---|---|---|
| Al lavoro (anche un timer in corso) | `caldo-2` | `#fb923c` | `#fcb67d` | |
| Ti aspetta | `caldo-1` | `#fbbf24` | `#fcd56e` | |
| Guarda presto / fatto non visto | `petalo-3` | `#60a5fa` | `#a2cafc` | |
| Fatto (visto) | `petalo-5` | `#2dd4bf` | — | sfuma in 2,8 s |
| Errore | `caldo-3` | `#f472b6` | — | non su `caldo-2`, per non confondersi con «al lavoro» |
| «$» del terminale | da `petalo-5` | `#7ce4d7` | — | |

Le luci prendono tinta e saturazione del token con la chiarezza HSL dei valori di TwoCode (valori in `tc-isola.css`
`:root` e in 10 §7, anche quelli di prima per tornare indietro). Contrasti tutti sopra soglia (parola ≥ 4,5, segno ≥ 3,
capsula su luce al 50 % ≥ 4,5). Occhi a riposo bianchi. Schermate: `tc-colori-*.png`.

**Caratteri.** Inter 500 e 700 (il 600 dei pulsanti diventa 700: tetto di due pesi) e **JetBrains Mono NL 500** per
comandi, numeri, ore, durate ed etichette maiuscole (NL perché Slint non spegne le legature). Oggi nel repo ci sono
Inter Tight e JetBrains Mono «normale» (`crates/assistente/assets/font/`): vanno aggiunti i file nuovi con la loro
licenza OFL, e in F1 sostituiscono i vecchi in tutta l'app (una sola famiglia, §8). Slint non ha `tabular-nums`: l'orologio cambia una volta al minuto ed è centrato (accettabile), i
cronometri sono in mono.

**Movimento** (solo durante la transizione; con «Mostra animazioni» di Windows spento: solo dissolvenza o niente).
Isola che cresce: `cubic-bezier(.2,.9,.3,1.06)` 420 ms, chiude `ease-out` 340 ms (Slint non ha molle fisiche). Cambio
pagina: il corpo entra di 26 px dal lato della scheda in 280 ms. Capsula delle schede: `animate x, width` 200 ms
ease-out. Menu: da scala 0,6 in 220 ms `cubic-bezier(.2,.9,.3,1)`. Pressione 0,96 in 100 ms. Luce: dissolvenza 1,2 s
solo al cambio di tono.

**Escluso** (dalla bozza o da TwoCode, per i vincoli): il **«respiro» della luce**. **La luce è ferma e cambia solo
al cambio di stato**, in nessun posto respira (decisione di Paolo, 20/11, che conferma la proposta); occhi che **seguono il
mouse** (servirebbe leggere la posizione globale del puntatore: un timer nuovo); sfocature; ombra esterna dell'isola
(cadrebbe fuori dalla finestra trasparente). Il battito degli occhi resta come oggi.

### 5.3 Stati: vocabolario e segni (variante 5 «misto», decisione di Paolo)

**Toni** (vocabolario chiuso, uguale per ogni modulo): `vuoto` (non configurato / primo avvio), `calmo`, `attivo`
(al lavoro, suona, conta), `guarda` (livello attenzione), `aspetta` (livello bloccante), `fatto`, `errore`, `fermo`
(non raggiungibile, spento, dato vecchio: tenue + età). Priorità per pillola e arbitraggio: aspetta > guarda = errore >
fatto > attivo > calmo.

**Segni (variante 5, `tc-stati.html`, `tc-stati-misto*.png`).**
- **Nelle carte**: capsula con **occhietti d'umore + parola breve** nell'etichetta: «Aspetta», «Guarda», «Al lavoro»,
  «Fatto», «Errore», «Non risponde» (contrasto 5,9–8,4:1). Calmo: nessuna capsula. Le forme lunghe («Guarda presto»,
  «Ti aspetta») stanno nella pillola, nella Coda e nei nomi per Narrator. La capsula c'è in **entrambe** le modalità
  della Panoramica *(scelta mia: la schermata `tc-pan-lavora` mostra solo gli occhietti perché è anteriore alla
  variante 5; una regola sola)*.
- **Nella barra delle schede**: occhietti 15 × 8 **solo** per «ti aspetta» (tondi pieni) e «guarda presto» (a mandorla
  inclinate); nessun segno per «al lavoro» e «fatto». Narrator legge lo stato di ogni scheda.
- **Nella pillola**: nessun segno in più; lo dicono gli occhi veri (umore + colore) e il testo.
- In scala di grigi le due sagome della barra si distinguono a ogni scala; lo stato è comunque detto anche dalla parola.

### 5.4 La cornice: 800 × 340 fisso (decisione di Paolo)

Pannello **800 × 340** per ogni vista (sostituisce 800 × 300). Riga alta 44, barra bassa 64, margini laterali 24, corpo
752 × 224. Le stesse proporzioni di TwoCode; lo spazio in più va in leggibilità, non in altre cose.

- **Riga alta**: a sinistra data mono maiuscola + meteo; al centro gli occhi; a destra l'ora mono 13 (nascosta quando
  l'orologio grande è visibile) e «⋯» (menu, Impostazioni, ritirata, Esci; dec. 13). Il clic destro sulla pillola resta
  tolto (decisione del 16/11; la nota di 10 §2 sul clic destro non la cambia).
- **Barra bassa**: a sinistra **ricerca** (pulsante «Ctrl K», apre il velo di §5.7); al centro le **schede**:
  Panoramica · divisore · fino a **6 moduli** · (modalità Orologio, oltre 6 moduli) «Tutte le app» col totale: mai più
  di 7 posti. Icone sole; la scheda attiva diventa una **capsula 36 r 18 col nome mono maiuscolo** che scivola. A destra
  il **volume**. Schermata: `tc-barre.png` (4 / 6 / 7 / 12 moduli).
- **Pulsante del volume** *(scelta del coordinatore: un gesto per uno, senza un cursore nella barra che ruberebbe
  spazio alle schede)*: clic = muto / audio (lo stato sta nell'icona e nel nome per Narrator: «Volume 40 %» /
  «Volume, muto»); **rotellina sopra il pulsante = volume ±2 %**; il cursore resta nella pagina PC. Senza uscita audio
  o senza supporto del volume (oggi macOS e Linux) il pulsante **non c'è** e il lato destro della barra tiene la sua
  geometria (posto vuoto). Ordine di Tab invariato.
- Clic sulla pillola: apre il pannello sul modulo dell'elemento mostrato (dec. 14 generalizzata).

### 5.5 Panoramica

Due modalità, scelte in **Impostazioni › Panoramica** (decisione di Paolo; niente più cambio al volo con i puntini).
Predefinita **Griglia** (decisione di Paolo, 20/11); Orologio si sceglie in Impostazioni.

**Griglia** (come «All apps» di TwoCode; `tc-griglia-4/-8/-fuoco/-aspetta/-12-p1/-12-p2.png`, confronto
`tc-griglia-confronto.png`):
- **Carte uguali 182 × 108 r 20, spazio 8, 4 per riga**: etichetta (icona 15 + nome mono maiuscolo 11 + capsula di
  stato a destra), **un valore grande** in basso (parole Inter 700 18, numeri e ore JetBrains Mono 26), **una riga**
  12 px. Chi ha tono attivo, guarda o aspetta si accende dal basso. **Tutta la carta apre il modulo.**
- **Azione principale**: bottone tondo bianco 32 nell'angolo in basso a destra, che **compare al passaggio o col
  fuoco** (Tab: carta → sua azione → carta seguente). Con la tastiera c'è sempre e Narrator l'annuncia.
- **Regola dell'orologio**: «le righe libere in alto mostrano l'orologio». Fino a **4 moduli**: orologio grande (Inter
  700 96) + una riga di carte. **Da 5 a 8**: due righe, l'ora va piccola in testata. **Oltre 8**: pagine da 8 a posti
  fissi, **puntini** sopra la barra, rotellina / PagSu / PagGiù / trascinamento per voltare (cambio pagina = la griglia
  scivola di una pagina in 280 ms, solo durante il cambio). L'ultima pagina riempie dalla prima riga; il resto resta
  vuoto. Si apre sempre sulla pagina 1 *(scelta mia: posti fissi, memoria spaziale)*.
- **Voce che ti aspetta**: con ≤ 4 moduli prende il posto dell'orologio; con 5+ **copre la prima riga** (stessa
  altezza 108) finché non rispondi; nessuna carta si sposta; la carta coperta dice «Aspetta» e torna visibile alla
  risposta. Anatomia della voce: capsula «Ti aspetta · <modulo>», titolo che decide, per gli agenti il comando intero in
  mono con «$» e la cartella a destra, «Nega» (fuoco) e «Consenti», «+N in attesa ›» verso la Coda (`tc-griglia-aspetta.png`).
- **Guarda presto e avvisi** in Griglia: capsula sulla carta e occhietti sulla scheda; in più nella riga alta, dopo
  data e meteo, «N in attesa ›» che apre la Coda quando ci sono voci non mostrate *(scelta mia: in Griglia non c'è la
  fascia e la Coda deve restare raggiungibile; una voce «guarda» non copre l'orologio per ore)*.
- Il dock perde «Tutte le app» (la griglia mostra già tutto). Niente prestito: ogni modulo ha già il suo posto.

**Orologio** (l'impianto di 10 §1–2; `tc-pan-calma/-lavora/-aspetta/-6/-12/-fuoco.png`):
- Orologio grande Inter 700 96; sotto, alla stessa quota sempre, la **fascia «parlata»** (alta 32, senza fondo a
  riposo); sotto, **4 tessere 182 × 84 r 20** dei primi 4 moduli fissati, centrate: nome mono 11, titolo 15–16/700,
  riga 12, azione tonda **sempre visibile** (con 4 tessere non c'è il «muro di comandi»).
- **Fascia per regola, non per frasi** (07 §3.1.2): «Niente ti aspetta» + «N avvisi ›» (→ Coda) + le attività che non
  si vedono già nelle tessere né nei compatti; al primo avvio «Avvio in corso · N moduli in attesa di dati». Oltre 4
  moduli la fascia ospita i **compatti** dei non fissati che hanno qualcosa da dire, a **3 posti fissi** (nome breve
  ≤ 8 + valore ≤ 8), «+N» contato in dati, «Tutte le app» (che apre la griglia a pagine).
- **Voce che ti aspetta**: prende il posto dell'orologio (che va in testata); l'orologio si dissolve in 160 ms, la voce
  entra da scala 0,96 in 280 ms; le tessere restano ferme.
- **Prestito di tessera** (decisione di Paolo: isola può scambiare le tessere), con le regole sopravvissute alla critica:
  il posto ospite è **sempre il 4°**; solo se il 4° è calmo; **mai se la voce del modulo è già mostrata nella fascia**;
  segno «in prestito» nell'etichetta della tessera, e chi ha ceduto dice a parole «<nome breve> tornerà qui» nei
  compatti (Narrator: «Mail, ha ceduto il posto ad Agenda»). Si decide **solo all'apertura del pannello**; ritorno alla
  prima apertura dopo che il tono è sceso sotto «guarda» o subito dopo aver aperto la tessera ospite; dal menu della
  tessera «Tieni qui» / «Rimetti …». Con 4 moduli non scatta mai.
- Dock: oltre 6 moduli l'ultima icona è «Tutte le app» col totale.

**Regole comuni.** Le posizioni non dipendono dallo stato: l'urgenza va nella voce, nella capsula, nella luce e nella
pillola, mai nella posizione. Ordine fermo a pannello aperto (dec. 53). La Panoramica di 1 o 3 moduli è centrata e deve
sembrare finita (`s-pan-1`/`s-pan-3` come prova di regola).

### 5.6 Pagine dei moduli

Stessa cornice; ogni pagina comincia con la **riga di sintesi** (fatto R1 a 18/700 + sotto-testo tenue + fino a 2 azioni
a destra) e usa **uno dei 5 impianti** (enum chiuso, nessun componente per un solo modulo):

| Impianto | Corpo | Moduli |
|---|---|---|
| **Elenco** | righe uniformi (segno · nome · chi · cosa · valore), ordine fermo, totale scritto, scorrimento | Coda; futuri Mail, Agenda, Servizi |
| **Elenco + dettaglio** | elenco a sinistra (330), riga scelta o **permesso** a destra | **Agenti** (`tc-agenti.png`) |
| **Comandi** | filtri a sinistra + griglia di card a misura fissa; sola lettura distinta per forma | **Casa** (`tc-casa-luci.png`, `tc-casa-stanza.png`), **Collegamenti** |
| **Misure** | gruppi di valori mono con barra sottile, 0–3 comandi in linea | **PC** (`tc-pc.png`) |
| **Conversazione** | scambi + campo; «Apri in finestra» | HIRIS (solo forma) |

Stati di pagina con la stessa forma per tutti: **vuoto** (frase + 1 azione), **fuori servizio** (frase + età +
«Riprova»), **primo avvio** («—» col motivo, nessuna azione).

- **Agenti.** Righe di sessione col **nome della sessione = titolo della scheda di Claude Code** (`session_name` dalla
  barra di stato, già rilasciato in 2e63446, ripulito come da M3 §10), altrimenti la cartella; colonna del fornitore
  come **parola** («Claude», poi «Codex»), mai come colore; limiti **per fornitore** nella sintesi (oltre la soglia in
  arancio, sempre col numero). La carta/tessera mostra **il limite più alto col suo nome** («Claude 7 giorni 61 %»);
  la seconda riga, solo senza azione, il secondo. La sessione che chiede apre il dettaglio: «TERMINALE», comando in mono
  con «$», cartella, «Ctrl Invio consente», «Nega» (fuoco) / «Consenti».
- **Casa.** Filtri: «★ Preferiti» (prima voce, dec. 13), «Luci accese» (con il numero), «Tutta la casa», poi le
  stanze. **La card è essa stessa l'interruttore** (decisione di Paolo; `accessible-role: switch` con
  `accessible-checked`): accesa = riquadro icona bianco + parola + valore; niente colore e niente interruttore separato.
  Sola lettura (temperatura, umidità): card senza riempimento, valore mono come testo principale, nessun comando. Scene
  in fondo. «Spegni tutte» primario bianco nella sintesi. «N luci accese» porta qui filtrato (dec. 14).
- **PC.** Quattro misure mono 28 con barra sottile, righe di stato; il cursore del volume e i riquadri Wi-Fi/BT di oggi
  diventano tipi dell'impianto Misure; «Muto» secondario.
- **Collegamenti** (decisione di Paolo, 20/11: «più usato con conteggio salvato»). Impianto Comandi con card di tipo
  «lancio». In carta: «N collegamenti · Più usato: …» e azione = **il più usato**, a parità di conteggio il più recente:
  **2 clic** (pillola, azione della carta); gli altri dalla pagina Collegamenti (3 clic). Nessun elenco sul posto
  (opzione (b) scartata). Il conteggio è **dato nuovo su disco**: per ogni collegamento un contatore dei lanci e l'ora
  dell'ultimo lancio, salvati in locale nella configurazione dell'assistente, aggiornati a ogni lancio da isola; solo
  numeri, nulla in rete. Configurazione vecchia senza il campo = 0 (nessuna migrazione da scrivere a mano, ma testata);
  un collegamento tolto perde il suo conteggio. Test: incremento, parità vinta dal più recente, lettura di una
  configurazione senza il campo, salvataggio atomico come il resto della configurazione.

### 5.7 Servizi del guscio

- **Ricerca Ctrl+K e «Chiedi a HIRIS…»** (`tc-cerca.png`): velo col fondo all'82 % (colore con alfa), campo 40 con
  bordo ≥ 3:1; risultati ricalcolati in Rust **a ogni tasto** (≤ 8): moduli, poi oggetti che i moduli espongono
  (dispositivi, collegamenti, sessioni), poi «risposta da un modulo»; **«Invio» solo sulla riga scelta**; **«Chiedi a
  HIRIS: …» sempre ultima, con Ctrl+Invio**; luce bianca tenue (HIRIS non è uno stato). Oggi la riga HIRIS apre
  l'impianto Conversazione con «HIRIS non è ancora disponibile»: nessun dato, nessun processo. Si apre anche
  «cominciando a scrivere» (carattere stampabile senza modificatori, fuori da campi e menu, Spazio escluso).
- **Coda** (`tc-coda.png`): vista del guscio con l'impianto Elenco; livelli a sinistra (Aspetta te / Guarda presto /
  Quando vuoi, stesso componente dei filtri di Casa); voci con segno + icona del modulo + titolo che decide + ≤ 2
  pulsanti + «vai» alla destinazione; più di 2 voci uguali di un modulo diventano una riga. Le voci *informazione*
  vivono solo qui e come contatore. Ci si arriva da «+N in attesa», da «N avvisi ›» / «N in attesa ›», dal puntino
  della pillola.
- **Azione delicata** (allarme, serrature, «Chiudi» del garage; dec. 8): **tieni premuto 4 s** con riempimento, mai con
  Invio, annullata al rilascio.
- **Tutte le app**: la stessa griglia a pagine da 8 della modalità Griglia, con «Gestisci i moduli» → Impostazioni ›
  Moduli. I due ingressi (scheda e compatto) hanno lo stesso nome per Narrator.
- **Impostazioni › Moduli**: per ogni modulo attivo sì/no, ordine (frecce, trascinamento, Alt+↑/↓), in Panoramica
  (fissato), nella pillola, regola delle notifiche (pillola · notifica di Windows · solo pannello · niente; il bloccante
  passa sempre), più la sezione propria del modulo (quelle di oggi: Casa, Claude → Agenti, Collegamenti, Isola).
  **Impostazioni › Panoramica**: Griglia (predefinita) / Orologio, **già in F1**, insieme alla rimozione della sezione
  «Quadranti» *(scelta mia: i quadranti spariscono; il catalogo resta per i lati della pillola)*.

### 5.8 Pillola

Tre altezze come TwoCode (32 / 36 / 40), orecchie di 10 px, luce dal basso nel colore del tono, occhi sempre presenti
(`tc-pill-*.png`).
- **Riposo** 116 × 32, solo occhi bianchi; con i **lati** (ora, batteria, rete del guscio + valori R0 dei moduli,
  catalogo `lati.rs`) mono 13 a larghezza ferma.
- **Attività** alta 36 a larghezza ferma per tipo: occhi piccoli, icona del modulo, testo R0 («my-shop · Modifica»),
  cronometro mono (ridisegnato al secondo **solo** se mostrato e visibile); **«+N»** 32 × 28 a lato per le altre
  attività, nominato per Narrator («altre 2 attività: …»).
- **Bloccante** (permesso) **540 × 40, ferma** *(dalla bozza approvata: progetto + comando mono + «+N» + due pulsanti
  non stanno in 440)*: occhi sorpresi, nome della sessione, comando in mono, «Nega» col fuoco iniziale, «Consenti»
  bianco, Ctrl+Invio; testo tagliato → il secondo pulsante diventa «Apri» (dec. 11).
- **Attenzione**: icona del modulo + fatto, larga per 20 s poi puntino accanto agli occhi; al più un allargamento ogni
  10 min per modulo.
- **Fatto**: occhi ^ ^, luce blu finché non guardi, poi verde acqua che sfuma in 2,8 s e torna il riposo.
- **Precedenza** (nessuna rotazione): bloccante di qualunque modulo → attenzione nuova → attività del primo modulo
  «nella pillola» (a parità la più recente) → riposo. Non disturbare (Focus, schermo intero): passa solo il bloccante.
- **Ritirata**: linea che prende colore dal livello più alto e avanzamento dall'attività mostrata (come oggi).

### 5.9 Tastiera e Narrator

- **Ordine di Tab** (Panoramica): voce o fascia (Nega → Consenti → «+N») → carte/tessere (corpo → azione, per
  ciascuna) → compatti → schede → ricerca → volume → «⋯». Prodotto da una funzione pura in Rust, testata.
- Frecce per muoversi nella griglia; PagSu/PagGiù per le pagine; **Ctrl+0** Panoramica, **Ctrl+1…6** moduli del dock,
  **Ctrl+K** ricerca; Ctrl+Alt (= AltGr) escluso. Solo nel pannello, nessuna scorciatoia globale nuova.
- **Nomi per Narrator composti in Rust dal contratto** (Slint non concatena): «Casa, 4 luci accese, 21,5° dentro,
  modulo 2 di 8, azione: Spegni tutte»; card Casa come interruttore; schede con lo stato («Agenti, ti aspetta»).
- **Pool fisso di bersagli di fuoco** persistenti (mai dentro `if`/`for`) che seguono un indice (08 §5): un elemento
  che sparisce col fuoco addosso lo passa al più vicino. Nelle pagine resta il fuoco «per indice» di oggi.

### 5.10 Il contratto del modulo (Rust → Slint)

Dalla fattibilità (08 §1) con le correzioni della critica (07 §5) e del disegno TwoCode:
- **`Identita`** (statica): `id` (mai mostrato), `nome` ≤ 14, `nome_breve` ≤ 8, `icona` (SVG su griglia 24, tratto
  ~1,8, leggibile a 16/20 px), `impianto: Option<Impianto>` (None = senza pagina). **Niente `tinta`**: la luce è dello
  stato *(scelta mia: il campo di 05 §2.1 resterebbe inutilizzato)*.
- **`Riassunto`** (ricalcolato da una **funzione pura** per modulo quando cambia un suo dato): `tono`, `priorita`,
  `titolo` R2 **≤ 15** (con `titolo_numerico` per il mono 26), `sec` (una riga ≤ 22), `sec_extra` (il guscio la mostra
  solo senza azione e senza misura), `nome_file` (il guscio accorcia nel mezzo), `valori` (per fornitore: il guscio
  mostra il più alto col nome), `misura` 0..1 facoltativa, `azione` 0 o 1 (`id`, verbo + oggetto, icona, `delicata`),
  `r1` (fatto ≤ 32 + valore ≤ 8: sintesi di pagina, «Tutte le app», Narrator), `r0` (frammento ≤ 24 per la pillola e i
  lati), `aggiornato_alle`, `avvisi`. **Scala per stato obbligatoria** (il gradino più basso è un fatto calmo e vero,
  mai vuoto); **assenza ≠ zero** («—» col motivo).
- **`Registro`** (sostituisce `registro.rs`): ordine, attivi, fissati, ultimo riassunto di ciascuno; **rifiuta** testi
  oltre i limiti (test) e **scrive i modelli Slint solo se il riassunto è cambiato**; è l'unico a scrivere `moduli`. Un
  modulo che cambia ogni secondo aggiorna solo l'R0 della pillola.
- **Voci**: restano in `attenzione.rs`; `Fonte` diventa l'id del modulo; la voce guadagna `destinazione` e
  `raggruppa_come`; `Fonte::Prova` diventa il **modulo di prova** per simulare 5, 8, 9 e 12 moduli nei test.
- **Slint**: `ModuloUi` + modelli di indici (`carte`/`tessere`, `compatti`, `dock`), `TonoUi` e `ImpiantoUi` enum
  chiusi, modelli di pagina **uno per impianto, condivisi**, riempiti solo mentre la pagina è aperta. Componenti: `Carta`
  (due altezze: 108 in Griglia e «Tutte le app», 84 in Orologio), `Compatto`, `Fascia`, `VoceInAttesa`, `Barra`,
  `TestataPagina`, `StatoPagina`, i 5 impianti, il **permesso** come componente del guscio con le regole di sicurezza
  dentro. Il guscio calcola le posizioni (indice → riga/colonna/pagina) in Rust: nessun layout che dipende dal contenuto.
- **Adesso funzioni pure sopra `Ponte`**, non un rifacimento; il `trait Modulo` (avvia/ferma/riassunto/azione/cerca/
  pagina) arriva coi moduli nuovi (F7).
- **Id di «Agenti»**: diventa `agenti`, con lettura una tantum della vecchia chiave `claude` nella configurazione
  salvata *(scelta mia: con Codex l'id `claude` mentirebbe; i test della vista Claude si riscrivono comunque in F2)*.

## 6. Decisioni precedenti superate

| Decisione (`decisioni.md` o spec) | Ora |
|---|---|
| Dec. 6 e spec generale §3bis: pannello **800 × 300** | **800 × 340** fisso |
| Dec. 1–2: layout **Orologio e Quadranti** cambiabili al volo con i **puntini** | **Orologio / Griglia** in Impostazioni; i puntini restano solo come indicatore delle pagine della griglia; i quadranti spariscono (anche la loro configurazione del 16/11, dec. 13; il catalogo resta per i lati) |
| Dec. 3: riga «In attesa» in cima, assente se vuota; bloccante rosso, attenzione ambra, informazione blu | Voce più urgente al posto dell'orologio / sulla prima riga; fascia sempre presente in Orologio; colori per stato di §5.2 (aspetta ambra, guarda blu, errore rosa); informazioni solo in Coda |
| Dec. 5: dock solo icone | Resta, ma la scheda attiva mostra il nome nella capsula; al più 6 moduli + «Tutte le app» |
| Dec. 10: pillola 124 × 32 / 400 × 36 / bloccante 440 × 40 | 116 × 32 / 36 a larghezza per tipo / **540 × 40** |
| Dec. 8 e identità «viola-nero» (risposte del 20/11 sul brief 01): aura, fondi viola, petali come decoro | Isola nera opaca, grigi neutri, luce dal basso per stato; i petali restano come **tinte** (tabella §5.2) |
| Spec M3 §6.8: dock con icona «Claude», vista Claude 800 × 300 | Modulo «Agenti», impianto Elenco + dettaglio, 800 × 340 |
| Studio 01 §3.2: ordine fisso Claude · Casa · PC · Collegamenti; fascia che sparisce | Ordine dell'utente; fascia sempre presente (Orologio) |

**Restano**: niente «‹ Indietro» (dec. 13); «⋯» in alto a destra e niente clic destro sulla pillola (16/11); permessi
con Ctrl+Invio e fuoco su «Nega», «Apri» se tagliato (dec. 8, 11); nomi dei livelli «Aspetta te / Guarda presto / Quando
vuoi» (dec. 11); azioni delicate 4 s, ritirata, «dorme» senza ridisegno (dec. 8); Impostazioni e Gestione in finestre
proprie (dec. 9, 14); «★ Preferiti» prima voce di Casa (dec. 13); ogni conteggio porta al dettaglio giusto (dec. 14).

## 7. Sicurezza e dati (M3 invariata; un solo dato nuovo su disco)

- Il permesso è **un componente del guscio** che porta dentro tutte le regole di M3: fuoco iniziale su «Nega», solo
  Ctrl+Invio consente, armamento di 600 ms, nessun «Sempre», nessuna risposta se Paolo non risponde, chiusura al primo
  evento successivo della sessione. Un secondo agente lo usa così com'è e non lo riscrive.
- Canale (M3 §10) e aiutante `isola-hook.exe` non cambiano. Il comando del permesso compare solo nel pannello e nella
  pillola, **mai** nella notifica di Windows; il nome della sessione sì (scelta già approvata in M3 §10).
- **Minimizzazione**: `Riassunto` e modelli contengono solo testi già mostrati oggi; la ricerca lavora in memoria e non
  scrive né registra le parole cercate; la riga HIRIS non manda nulla; niente di nuovo in `diagnosi.log`. **Unico dato
  nuovo su disco**: il conteggio dei lanci e l'ora dell'ultimo lancio di ogni collegamento (§5.6), nella configurazione
  locale dell'assistente, mai trasmesso. Più l'ordine e le impostazioni del guscio per modulo (§5.7), che sono scelte
  dell'utente e non dati d'uso.
- Chiave da amministratore e token di Home Assistant restano nel portachiavi; nessuna superficie nuova verso la rete.

## 8. Migrazione a fette

Ogni fetta lascia isola completa, test e CI verdi, CHANGELOG aggiornato, prova dal vivo prima di chiuderla.

| Fetta | Contenuto | Si vede |
|---|---|---|
| **F0 · spike** | §9 | — |
| **F1 · contratto + cornice + Panoramica + Coda minima** (la più grande) | `Riassunto` e funzioni pure per **Agenti (= Claude), Casa, PC, Collegamenti** con i loro test; contatore dei lanci dei collegamenti (§5.6); `Registro` + configurazione per modulo (ordine, fissato); **i token nuovi sostituiscono quelli di `tema.slint`** (grigi, toni, luci) e caratteri Inter + JetBrains Mono NL; isola nera con orecchie, riga alta, barra con capsula, ricerca inerte e pulsante del volume (§5.4); 800 × 340; Panoramica Griglia (predefinita) e Orologio con 4 moduli (voce al posto dell'orologio); **Impostazioni › Panoramica** (Griglia/Orologio) e via la sezione «Quadranti»; via layout, quadranti, puntini e **la vecchia riga «In attesa»**; **Coda minima** sull'impianto Elenco (lettura delle voci + i loro pulsanti, raggiungibile da «+N in attesa» / «N in attesa ›» e dal puntino della pillola); pool di fuoco e nomi per Narrator; **passata di verifica e ritocco** di pagine di oggi, pillola, Impostazioni e Gestione sulla base nera (solo token e contrasti, nessun ridisegno) | Panoramica nuova, Coda; tutto il resto coerente col nuovo nero |
| **F2 · Elenco (+ dettaglio)** | impianto Elenco + dettaglio (l'Elenco c'è già dalla Coda); Agenti ci si sposta; componente permesso del guscio | pagina Agenti |
| **F3 · Comandi e Misure** | Casa e Collegamenti → Comandi (card-interruttore, sola lettura, scene); PC → Misure | pagine Casa, Collegamenti, PC |
| **F4 · pillola** | pillola nel nuovo linguaggio (misure, occhi, luce ferma, bloccante 540), arbitraggio generale, «+N», 10 min per modulo, Non disturbare | pillola |
| **F5 · servizi del guscio** | Ctrl+K e «comincia a scrivere», risultati da moduli e oggetti, riga HIRIS inerte; Coda completa (livelli a sinistra, raggruppamento, «vai» alla destinazione); azione delicata | ricerca, Coda completa |
| **F6 · scala** | griglia a pagine e puntini, «Tutte le app», dock > 6, compatti e prestito in Orologio, Ctrl+0…6, Impostazioni › Moduli | col modulo di prova finché non arriva il 5° modulo vero |
| **F7 · `trait Modulo`** | avvia/ferma per modulo; primo modulo nuovo come prova del contratto | il modulo nuovo |

F4 può salire subito dopo F1 se Paolo vuole vedere prima la pillola nuova (è indipendente dalle pagine). F6 non dà
nulla di visibile con 4 moduli e può aspettare il quinto.

**F1 fatta (23/11/2026, ramo `v3-assistente`; prova dal vivo con Paolo ancora da fare).** Contratto dei moduli, cornice
nera, barra con ricerca inerte e volume, Panoramica Griglia e Orologio, Coda minima, Impostazioni › Panoramica e passata
sulle pagine vecchie sono dentro; misure di chiusura nel CHANGELOG («Ridisegno F1, chiusura»). Ciò che il piano ha
deciso diversamente da questa spec:
- **«Consenti» neutro come «Nega»** (regola M3), non bianco primario come nelle bozze: il bianco resta all'azione
  primaria (pulsante d'azione, interruttore).
- **L'ordine del dock cambia**: Panoramica, Agenti, Casa, PC, Collegamenti; l'id `claude` di un file di prima si legge
  come `agenti`.
- **Un permesso nella Coda si legge intero prima di concedere**: la riga ha «Apri», mai «Consenti»; aperto, titolo a
  capo e dettaglio scorrevole in mono, «Consenti» solo dopo aver visto il fondo (come in M3). Il brief chiedeva il
  titolo in ellissi: ha prevalso la sicurezza invariata di §7. Il riarmo di 600 ms vale anche quando una riga cambia
  posto.
- **Testi delle scale accorciati** («Collega Claude», «7 giorni 61 %», «A riposo», «Scollegata», «{n} luci», «{n} link»,
  «Nessun link»): i limiti in pixel (§13.7) sono vincolanti, il `Registro` rifiuta ciò che sta fuori misura.
- **«Tocca a te»** (tono Guarda) per la sessione ferma in attesa di un messaggio: «Ti aspetta» è riservata al tono
  Aspetta del permesso bloccante.
- **Fatto non visto = blu** (petalo-3), non più verde acqua; colori dei token non elencati per regola unica (neutro →
  g3/g4, azione → primario, pericolo → errore, vetri → g1/g2/filo); tracking mono .07em; «›» come `Path`, tolto dal
  nome per Narrator.
- **La luce che sale in 1,2 s all'apertura** del pannello con un tono acceso resta (l'apertura è una transizione
  ammessa); a riposo nessun fotogramma.
- **Rimandati**: `Registro::config()` e il «fissato» di una voce `claude` (F6); la ricerca resta inerte (F5).

**F2 fatta (24/11/2026, ramo `v3-assistente`; prova dal vivo con Paolo ancora da fare).** Impianto Elenco + dettaglio, pagina
Agenti (sintesi coi limiti a barra, elenco, dettaglio) e componente `Permesso` del guscio, usato da Coda e Agenti, sono dentro;
si sono chiusi per primi i punti parcheggiati da F1 (n1/n2 della voce, «Errore» rosa, «Nega» inerte durante l'armamento);
misure di chiusura nel CHANGELOG («Ridisegno F2, chiusura»). Ciò che il piano ha deciso diversamente da questa spec:
- **Decisioni di Paolo (23/11)**: il clic su una riga **la sceglie** (il terminale dal pulsante del dettaglio o con Invio);
  **«chiesto alle 14:32»** al posto di «da 42 s» della bozza (un contatore dei secondi vorrebbe un timer, §3) e **solo il nome
  della cartella**; **«Consenti» neutro come «Nega»** confermato; **il clic sulla pillola con un permesso apre la pagina
  Agenti** con quella sessione scelta, non la Coda (la Coda resta la strada di «Apri» della voce nella Panoramica e il ripiego
  se la sessione non ha riga).
- **Il permesso è un componente solo** (`ui/guscio/permesso.slint`) con le regole di M3 dentro; nel pannello ce ne sono **due
  istanze fisse**, mai dentro un `if` (un id dentro un `if` non si raggiunge da fuori), con `visible` legato alla vista. La Coda
  tiene la sua riga (`RigaElenco`) fino a F5; ogni decisione passa da `azione-voce` e dalla guardia di 600 ms di Rust.
- **Aspetto inerte** dei due pulsanti per i 600 ms dell'armamento: un'animazione lineare su una proprietà che non si disegna
  (componente `Inerzia`, uno solo nel pannello), non un timer; vale anche con «Mostra animazioni» spento. La guardia vera resta
  in Rust. **Non vale per la pillola** (altra finestra): resta rimandato.
- **Un clic su «Nega» o «Consenti» vale solo se il puntatore si è mosso sopra il pulsante dopo la sua comparsa** (movimenti a
  pulsante alzato); una superficie nata sotto il puntatore non decide con un clic fermo. Conseguenza accettata: con un
  touchscreen un tocco non decide (restano tastiera e Narrator). È una regola in più rispetto a §7, per entrambi i permessi e
  per la voce.
- **Pagina Agenti**: nome della riga = titolo della scheda di Claude Code (altrimenti la cartella); fornitore come parola
  («Claude»); «caldo» deciso sul numero arrotondato che si legge; senza chiave da amministratore la testata scrive la nota al
  posto della spesa («Aggiungi una chiave da amministratore nelle Impostazioni»), visibile e letta da Narrator; «Dettagli» solo
  con la chiave; «Nessuna sessione aperta» **senza azione**; «Torna all'elenco» resta un pulsante per il mouse; l'etichetta
  del campo dipende dallo strumento («TERMINALE» solo per un comando, «FILE», «INDIRIZZO», «RICERCA») e il «$» solo per il
  comando.
- **Tastiera**: → o Invio sulla riga di un permesso portano il fuoco su «Nega», mai una decisione; Invio sulla riga di una
  sessione apre il terminale; Esc nel permesso di Agenti chiude il pannello; il riquadro del comando mostra circa due righe
  alla volta.
- **Rimandati**: il campo `r1` del contratto (arriva in F3, con le pagine con sintesi); la pillola inerte; elenco completo in
  `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` («Dopo F2»).

**F3 fatta (27/11/2026, ramo `v3-assistente`; prova dal vivo con Paolo fatta).** Impianto Comandi (Casa e Collegamenti) e
impianto Misure (PC) sono dentro, con le vecchie viste tolte; campo `r1` del contratto; carta Agenti allineata («1 ti aspetta»);
misure di chiusura e prova dal vivo nel CHANGELOG («Ridisegno F3»). Ciò che il piano ha deciso diversamente da questa spec:
- **Decisioni di Paolo (24/11)**: «★ Preferiti» solo se c'è un preferito che non è una scena, altrimenti la pagina si apre
  su «Tutta la casa»; le scene in una fila «SCENE» in fondo a ogni filtro tranne «Luci accese» (nessun filtro «Scene»);
  misure del PC CPU · Memoria · Disco (GB liberi) · Batteria, senza batteria la GPU al quarto posto, senza tutte e due il
  posto resta vuoto; misure alte 84 con due file sempre visibili (Wi-Fi · Bluetooth con gli interruttori, Volume · rimandi
  a Windows).
- **Decisioni di Paolo nella prova dal vivo (27/11)**: le **luci accese sono gialle** (bordo, fondo e riquadro dell'icona
  nella tinta della luce; le prese restano col primario); la colonna dei filtri resta **senza etichetta**; i nomi delle card
  restano quelli di Home Assistant.
- **C14(d)**: la carta di un modulo e il suo `r1` possono dire cose diverse fuori dal caso del permesso (la carta è il
  riassunto per la Panoramica, `r1` il titolo della pagina); coincidono solo dove il permesso lo richiede.
- **Guardia di 600 ms anche per le azioni della Casa**: «Spegni tutte» e «Spegni la stanza» da un clic, e il tocco di una
  card, si ignorano nei 600 ms dopo che l'azione o le card nascono sotto il puntatore (azione rumorosa che non deve partire
  per un clic dato a «Riprova»). Il clic ignorato non dà segni.
- **Una sola soglia della batteria scarica** (≤ 20 %) per pillola, carta, pagina PC e riga Bluetooth; la batteria scarica
  è «Guarda». Etichetta «Disco» (non «Disco C:»); la riga Bluetooth dice il nome del dispositivo più scarico, accorciato a
  vista e intero per Narrator. Niente «aggiornato ora» né «piena alle…» della bozza (vorrebbero un orologio, §3).
- **Rimandati**: elenco in `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` («Dopo F3»).

**Scelte del coordinatore sulle fette (20/11).**
- *Coda minima e via la riga «In attesa» già in F1*: le voci non bloccanti devono restare raggiungibili in ogni fetta
  da F1 a F5, così ognuna lascia isola completa; la Coda minima usa l'impianto Elenco, che F2 poi riusa.
- *Token sostituiti in F1, mai due aspetti insieme*: per questo F1 comprende la passata su pagine vecchie, pillola,
  Impostazioni e Gestione (token e contrasti, non impianto). Se il cambio di carattere rompe l'impaginazione di una
  finestra, il costruttore corregge le misure in quella finestra; non si tengono due famiglie di caratteri.
- *Impostazioni › Panoramica in F1*: la scelta Griglia/Orologio arriva con le due modalità, e la sezione «Quadranti»
  sparisce insieme ai quadranti.

## 9. Spike prima del piano (≈ 1 giornata)

Esempio usa e getta `examples/spike_ridisegno.rs` nel crate `assistente`, cartella di compilazione condivisa
`target\revisore`, cancellato alla fine; test sul backend di prova di Slint + finestra fuori schermo con
`Window::take_snapshot()`. Prove di tastiera **solo con messaggi alla propria finestra**, nessun clic reale.
1. **Pool di bersagli**: 1, 4, 8 carte + voce che appare/sparisce col fuoco addosso: il fuoco non si perde, il nome
   per Narrator è quello del modello.
2. **Azione al passaggio o al fuoco** della carta (`animate opacity` 120 ms) e ordine Tab carta → azione → carta.
3. **Tasti**: Ctrl+0…6, Ctrl+K, PagSu/PagGiù, «comincia a scrivere» su tastiera italiana (AltGr escluso, tastierino,
   Bloc Maiusc).
4. **Ellissi e misura** del titolo a 100/125/150/175 %; valore numerico in mono 26 senza cifre tabellari.
5. **Luce dal basso**: radiale in un `Rectangle` con `transform-scale-y` ≈ 1,25 e `clip`, sotto carta, isola e pillola:
   **CPU e RAM** a riposo e durante la dissolvenza col renderer software (è l'unica tecnica grafica nuova).
6. **Orecchie** (`Path` con due archi concavi) nitide a ogni scala; **caratteri** Inter 500/700 + JetBrains Mono NL 500:
   resa e RAM in più.
7. **Capsula che scivola** e cambio pagina della griglia: nessun ridisegno fuori dalla transizione.

Esito atteso: confermare le tecniche di §5.2 e §5.9 (o il ripiego: luce come gradiente lineare dal basso, orecchie come
rettangolo + cerchio) e le cifre di partenza per §11, prima di scrivere il piano di F1.

## 10. Cosa resta al costruttore

- Struttura dei file Rust (`src/moduli/`, `Registro`) e Slint (`componenti`, `impianti/`), e come spezzare
  `pannello.slint`.
- Misure fini dalla bozza (`tc-isola.css`) dove questa spec non le dà; scelta della tecnica della luce dopo lo spike.
- Come leggere «Mostra animazioni» di Windows per il movimento ridotto.
- Quali ritocchi di contrasto e misura servono a Impostazioni, Gestione e pagine vecchie sulla base nera (F1).
- Dove salvare il contatore dei lanci nella configurazione (struttura del campo), entro le regole di §5.6.
- Testi esatti delle scale per stato dei 4 moduli entro i limiti (titolo ≤ 15, riga ≤ 22), con test.
- Riscrittura dei test esistenti (≈ 131 in `test_ui.rs`; 08 §7.3 dice quali cadono e in che fetta).

## 11. Come si verifica

- **Test automatici**: in Rust puro le scale per stato e i `nome_narrator` di ogni modulo, il `Registro` che rifiuta i
  testi oltre i limiti, la composizione della Panoramica con 1/3/4/5/8/9/12 moduli (modulo di prova) in entrambe le
  modalità, la voce che copre la prima riga senza spostare carte, il prestito (4° posto, niente doppioni), l'ordine di
  Tab, l'arbitraggio della pillola, la ricerca, il contatore dei collegamenti (incremento, parità al più recente,
  configurazione senza il campo = 0), il pulsante del volume (clic = muto, rotella ±2 %, assente senza audio con la
  geometria della barra invariata), la Coda minima di F1. Sul backend di prova di Slint: geometria 800 × 340, carte tutte della
  stessa misura, nessun elemento nella fascia della barra, ellissi presenti, fuoco mai perso, regole del permesso
  (Ctrl+Invio esatto, fuoco su «Nega»). CI verde su Windows, macOS, Linux.
- **Confronto visivo**: foto **solo della zona dell'app** (`tools/foto-zona.ps1`), cancellate dopo, confrontate con
  le schermate approvate `tc-griglia-4`, `tc-griglia-8`, `tc-griglia-aspetta`, `tc-pan-lavora`, `tc-agenti`,
  `tc-casa-luci`, `tc-pc`, `tc-pill-permesso`, `tc-stati-misto`. **In F1** in più una foto di ciascuna superficie
  ritoccata sulla base nera: pagine Agenti (Claude), Casa, PC, Collegamenti di oggi, pillola nei suoi stati, Coda
  minima, finestre Impostazioni e Gestione.
- **Leggerezza** con `tools/misura.ps1` (linea del tempo di RAM privata, working set, CPU), stessa procedura della
  misura di oggi (≈ 40 MB WS, ≈ 14 MB privata, CPU ≈ 0,03 %): a riposo con la pillola **RAM privata ≤ +2 MB e CPU
  ≤ 0,05 %**; pannello aperto fermo ≤ 30 MB privata e ~0 % CPU; dopo la chiusura del pannello si torna entro +2 MB
  *(soglie: scelta mia, dentro il budget della spec generale §4)*.
- **Prova dal vivo con Paolo** a fine di ogni fetta: casa vera, Claude Code vero con un permesso (Nega e Consenti),
  tastiera e Narrator, scala 100 % e 175 %. Le azioni che suonano o allarmano si provano solo col suo sì.

## 12. Scelte mie da controllare (riassunto)

Pagina 1 all'apertura; «N in attesa ›» nella riga alta in Griglia; solo «aspetta» copre orologio o prima riga
(«guarda» no); capsula di stato anche in Orologio; occhi che non seguono il mouse; bloccante 540 × 40 dalla bozza;
campo `tinta` tolto; id `agenti` con migrazione della chiave `claude`; sezione «Quadranti» tolta; soglie di misura di §11.

**Scelte del coordinatore** (20/11): Coda minima e via la riga «In attesa» in F1; token nuovi al posto dei vecchi in F1
con passata su pagine vecchie, pillola, Impostazioni e Gestione; pulsante del volume nella barra (§5.4); Impostazioni ›
Panoramica in F1.

**Risposte di Paolo (20/11)** — non restano punti aperti:
1. Panoramica predefinita **Griglia**; Orologio selezionabile in Impostazioni.
2. Collegamenti: **il più usato con conteggio salvato** (contatore locale dei lanci + ora dell'ultimo, §5.6); niente
   elenco sul posto.
3. Luce **ferma**: cambia solo al cambio di stato, nessun respiro da nessuna parte.

## 13. Esiti dello spike (20/11) — correzioni vincolanti

Rapporto completo: `.superpowers/ux/studio/11-spike.md` (§2 decisioni, §3 punti smentiti). Le scelte qui sotto sono
del coordinatore e prevalgono sui punti corrispondenti di questa spec:

1. **Fuoco**: ogni contenitore che fa sparire un bersaglio (voce, posti del pool, velo, menu, ricerca) ha una regola
   esplicita in `changed` che sposta il fuoco sul più vicino; chiudere un velo da Rust lo riporta su `tasti`. Test col
   backend di prova dopo `update_timers_and_animations()`.
2. **AltGr**: escluso leggendo lo stato vero di Alt (`GetKeyState(VK_MENU)`, falso fuori da Windows) prima di
   scorciatoie e «comincia a scrivere». Ctrl+K senza badare alle maiuscole; Ctrl+Maiusc+K vale come Ctrl+K.
3. **Luce dal basso**: cerchio radiale stirato con `transform-scale-x` dentro un genitore con `clip`; animata (1,2 s)
   solo al cambio di tono, alfa nel colore. Niente ripiego lineare (viola il contrasto). Sostituisce «scale-y 1,25».
4. **Contorno di isola e pillola** in un solo `Path` con le orecchie; niente «rettangolo + cerchio».
5. **Etichetta della carta**: con la capsula di stato si mostra il `nome_breve` del modulo; la capsula ha una larghezza
   massima misurata e parole brevi («Aspetta», «Guarda», «Al lavoro», «Fatto», «Fermo» al posto di «Non risponde»).
6. **Azione visibile**: valore e riga si accorciano entrambi dello spazio del bottone; i testi delle scale stanno nei
   limiti anche così.
7. **Limiti dei testi in pixel** (non solo in caratteri), misurati con i caratteri incorporati nel `Registro` e nei test.
8. **Orologio a celle fisse** (cifre centrate in celle larghe quanto la cifra più larga): nessun salto.
9. **Narrator**: i `Text` decorativi delle carte fuori dall'albero; il bottone d'azione di una carta senza azione non
   esiste nemmeno per Narrator.
10. **Simboli** sempre SVG/`Path`, mai glifi Unicode. Caratteri: **Inter 500/700** e **JetBrains Mono NL 500**, dalle
    release ufficiali, al posto di Inter Tight e JetBrains Mono.
