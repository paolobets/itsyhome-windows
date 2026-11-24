# Changelog

All notable changes to ItsyHome are documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased] — v3 «assistente isola» (ramo `v3-assistente`, milestone M1, M2 e M3; M3 non ancora rilasciata)

> **Nota sul trasporto di M3.** Le sezioni «Task 1», «Task 2», «Task 4», «Task 6» e «Task 7» qui sotto raccontano il primo giro, con hook HTTP su `127.0.0.1`, porta, codice segreto, variabile `ISOLA_HOOK_TOKEN`, `assistente --barra-stato` e «Rigenera il codice». **Nulla di tutto ciò è mai stato rilasciato**: i Task 7A/7B (18/11) lo hanno sostituito con l'aiutante `isola-hook.exe` e un tubo con nome riservato all'utente (spec M3 §10). Le sezioni restano come storia.

### Changed (Ridisegno F3, Task 6: misure del PC, funzioni pure)
- **`moduli/pagina_pc.rs`: misure, righe, sintesi e stato della pagina PC come funzioni pure** (ancora non collegate: le userà il Task 7). Quattro misure a posti fissi: CPU, Memoria («7,7 di 16 GB»), Disco (i GB liberi, «liberi su 476 GB») e Batteria; senza batteria il quarto posto va alla GPU, senza né l'una né l'altra resta vuoto; con la batteria la GPU sta nel dettaglio della CPU. Senza dato il valore è «—» col motivo («Lettura in corso», «Non letta»), mai «0»; CPU e RAM da 85 % in su sono «attivo», la batteria in carica «fatto». Il disco e il quarto posto aspettano le fonti lente (`lente_lette`): la GPU non compare e sparisce. Righe: Wi-Fi e Bluetooth come interruttori («Non presente» se la radio manca, «Cuffie 70 %»), Volume come cursore solo con un'uscita audio, rimandi a Windows; fuori da Windows solo la riga di stato della rete. La sintesi ha «Muto» / «Riattiva l'audio». Nuovi in `voci.slint`: `MisuraUi`, `TipoRigaMisuraUi`, `RigaMisuraUi`; `SOGLIA_CARICO` di `moduli::pc` è `pub(crate)`.

### Changed (Ridisegno F3, Task 5: la pagina Collegamenti sull'impianto Comandi)
- **Collegamenti sull'impianto Comandi** (`moduli/pagina_collegamenti.rs`, funzioni pure): card di tipo lancio (cerchio con l'iniziale sul colore del nome, nome, indirizzo corto; per Narrator «{nome}, {indirizzo corto}, si apre nel browser»), nell'ordine dell'utente, quattro per riga su tutta la larghezza (nessun filtro né scena: la colonna dei filtri non esiste nemmeno per Narrator). La testata ha per titolo `r1.fatto` della carta («2 collegamenti», «Nessun collegamento»), per sotto «Più usato: NAS» o, se l'ultimo lancio non è riuscito, la frase dell'errore (con il tono dell'errore), e «Modifica» come azione secondaria; senza collegamenti lo stato vuoto invita ad «Aggiungere un collegamento». Via `ui/viste/collegamenti.slint` (`VistaCollegamenti`) e dal pannello `collegamenti`, `errore-collegamenti`, `apri-collegamento`, `gestisci-collegamenti`, `fuoco-collegamento` e le loro regole: frecce, Invio, Tab, Esc e Backspace sono quelli dei Comandi (↑ dalla prima riga a «Modifica»).
- **Ponte**: `scrivi_pagina_collegamenti` (solo a pagina aperta, solo ciò che cambia; la card col fuoco resta la stessa per id o passa alla più vicina se è tolta dalle Impostazioni), `tocca_card("collegamenti", id)` apre il collegamento e, se il browser è partito, chiude il pannello dal gancio `chiudi_pannello` (come «Apri {nome}» della carta, che passa dalla stessa `lancia_collegamento`); `azione_pagina("collegamenti", «Modifica» | «Aggiungi un collegamento»)` apre le Impostazioni sulla scheda Collegamenti. L'errore dell'ultimo lancio è un campo del ponte (`errore_lancio`), azzerato da un lancio riuscito e dalla chiusura del pannello: non è `errore_collegamento`, il dettaglio tecnico di Impostazioni › Informazioni.
- **Un solo modello per due pagine (C8)**: passando da Casa a Collegamenti (e viceversa) non compare nessuna card dell'altra, né prima né dopo che Rust riscriva i dati; `comandi_cambio_pagina_senza_card_dell_altra` lo prova e fallisce se si toglie il confronto `comandi-modulo == vista`.
- **`orologio::adesso_unix()`** sta ora in `orologio.rs` (prima in `con_claude.rs`); `casa-core/tests/config.rs`: la prova della scrittura atomica parte da un `.tmp` lasciato a mano e verifica che il salvataggio lo sostituisca (prima l'asserzione non poteva fallire).

### Changed (Ridisegno F3, Task 4b: impianto Comandi, la pagina Casa ci si sposta)
- **Impianto Comandi** (`ui/impianti/comandi.slint`: `ImpiantoComandi`, `FiltroComandi`, `CardComando`, `ChipScena`, `GeometriaComandi`): a sinistra i filtri (172, da y 48; elenco di schede «Filtri» per Narrator, uno solo selezionato, col numero e il filo sopra la prima stanza), a destra card a misura fissa 64 × ~182 in tre colonne da x 188 (quattro senza filtri), la fila «SCENE» (chip 32 r10, quattro per riga) dopo l'ultima riga e la nota sotto, tutto dentro lo scorrimento col filo come in F2. La card è l'interruttore (ruolo `switch` col suo stato, «Accesa · 80 %»), la scena e il lancio sono pulsanti, la sola lettura non ha fondo (ruolo testo; col valore numerico il valore mono 18 è il testo principale); tenue e «Non riuscito» in `Tema.errore`.
- **La pagina Casa sull'impianto**: via `VistaCasa`, `PreferitoUi`, `StanzaUi` e le proprietà della vista vecchia (`preferiti`, `stanze`, `dispositivi`, `luci`, `filtro-luci`, `stanza-aperta`, `stato-casa`, `casa-pronta`, `casa-da-configurare`, `dati-delle`, `casa-riprovabile`, `tocca`, `spegni-tutte`, `riprova-casa`, `apri-stanza`, `chiudi-stanza`) e la zona `stanze`. Al loro posto `comandi-*` (sintesi, stato, filtri, filtro scelto e iniziale, card, scene, nota, fuoco, lato, modulo), `filtro-pagina` e `tocca-card`; la testata comune (`TestataPagina`) con «Spegni tutte» primaria e l'icona «spegni» (power, come la bozza; la stessa nella carta), «Spegni la stanza», «Riprova» o «Collega»; senza la pagina piena lo stato (`StatoPagina`). La carta Casa con l'azione «Spegni tutte» apre il filtro «Luci accese».
- **Tastiera dei Comandi**: i filtri sono il lato 0 del contenuto (niente zona nuova, pre-flight C4) con `comandi-lato-valido`: ↑/↓ nei filtri, Invio sceglie e porta alle card, frecce in griglia, ← dalla prima colonna al filtro scelto, ↑ dalla prima riga all'azione della testata, ↓ dall'ultima riga alle scene; Tab filtri → card → schede; Esc e Backspace prima al filtro iniziale. Un'azione della testata che sparisce col fuoco addosso lo passa alla prima card e, se torna, non se lo riprende.
- **Ponte** (`ponte/con_pagine.rs`): `vista_comandi`, `scrivi_pagina_casa` (solo a pagina aperta, solo ciò che cambia; la card col fuoco resta la stessa per id; un filtro sparito torna all'iniziale col fuoco sui filtri), `filtro_pagina` (un solo ingresso: chiesto a pagina chiusa vale per la prima scrittura, C9), `tocca_card`, `spegni_stanza` (dalle luci di adesso, con l'ottimismo) e `casa_cambiata()` al posto di `ricostruisci_preferiti` in ogni punto (C10). Lasciata la pagina i suoi modelli si svuotano. Via il codice rimasto morto (`ricostruisci_preferiti`, `stanza`, `riempi_stato`, `voce_ui`, `preferiti_ui`, `entita_mostrate`, `stanze_ui`, `dispositivi_stanza`, `testo_stato`, `riga_stato`, `MASSIMO_PREFERITI`).
- **Riconnessione coi dati**: il `r1` di Casa (titolo della testata) in connessione coi dati di prima resta «Scollegata»: prima alternava «Scollegata» e «Mi collego» a ogni tentativo.

### Changed (Ridisegno F3, Task 4a: fondamenta del guscio, nessuna pagina cambia)
- **`EtichettaMono` (m6)**: il nome mono maiuscolo di una scheda, di una carta e delle file (JetBrains Mono NL 500, 11 px, tracking .07em, token `Tema.etichetta-*`) è un componente solo; la misura di Slint dello stile «etichetta» (`MisuraTesto`, stile 3) misura proprio quello, e `etichetta_mono_come_stile_etichetta` lo controlla per «AGENTI», «COLLEGAMENTI», «SCENE».
- **Testata di pagina**: `azione-primaria` (pulsante bianco) e `azione-icona`. L'icona si mostra solo se c'è un'azione primaria con testo, mai in base all'immagine (Agenti e le secondarie passano «nessuna», uno SVG che non disegna nulla, e non cambiano né la misura né il posto del titolo). `Pulsante` impara `icona`/`con-icona` (16 px prima del testo, il pulsante si allarga di 22).
- **Spazio del titolo**: il gruppo di destra della testata non prende più di quel che avanza dopo `SPAZIO_R1` (320 px) o, se più corto, dopo il titolo: se non ci sta si stringono prima le barre dei limiti (fino a sparire, resta il numero), poi l'importo (ellissi); l'azione mai. Prima, con due limiti, la spesa e «Dettagli», un titolo lungo aveva 217 px.
- **Carte riscritte senza motivo**: `ModuloUi.azione_icona` di una carta senza azione era un'`Image` vuota, mai uguale a se stessa in Slint, e `aggiorna_modello` riscriveva la riga a ogni `moduli_scrivi`; ora è l'icona «nessuna».
- **Icone dei filtri della Casa**: `Icone.di-stanza` dà la stella a «Preferiti» (−3), la lampadina a «Luci accese» (−4), la casa a «Tutta la casa» (−5) e «Altro» (−1); prima i codici negativi cadevano tutti sulla casa. Nuove icone Lucide per Rust: `spegni` (`power.svg`, ISC) e `stella`.
- **Helper e contatori comuni**: `tasto` (`impl Into<SharedString>`) ed `etichetta_col_fuoco` sono uno solo in `test_ui.rs`, `pub(crate)`; `scrivi_se_cambia` e il contatore di prova `PROPRIETA_SCRITTE` passano in `ponte.rs`, accanto a `aggiorna_modello` e `RIGHE_SCRITTE`.

### Changed (Ridisegno F3, Task 3: la pagina Casa sull'impianto Comandi, funzioni pure)
- **`moduli/pagina_casa.rs`: filtri, card, scene, sintesi e stato della pagina Casa come funzioni pure** (ancora non collegate: le userà il Task 4). Filtri «Preferiti» (solo se l'utente ne ha scelti: altrimenti la pagina si apre su «Tutta la casa»), «Luci accese» col numero, «Tutta la casa», filo, le stanze nell'ordine salvato coi dispositivi e «Altro» (`casa.altro`); icone delle stanze come indice scelto dall'utente (`FiltroUi.icona` è un `int`, codici negativi per gli altri filtri). Card per genere: luce «Accesa»/«Spenta» con «80 %» se si sa, interruttore, scena, sensore numerico («21,5°», «48 %») e letture (serratura, tapparella, termostato) in sola lettura, «Non disponibile» tenue, «Non riuscito» in errore, nome per Narrator «{nome}, {parola}[, {valore}][, sola lettura]». Le scene sono una fila in fondo in ogni filtro tranne «Luci accese». Sintesi: `r1.fatto` nei filtri della casa con «Spegni tutte» primaria e icona; in una stanza il suo nome, «{a} accese su {n}» e «Spegni la stanza»; a casa scollegata «dati delle 10:42» e «Riprova»/«Collega». Quattro stati di pagina e nota di «Luci accese».
- **Una sola vista ottimistica delle luci (C1).** `ponte::luci_accese_ora(menu, ottimismo)` (e `luci_accese_in` per una stanza) conta le luci come si vedono adesso: una appena spenta esce subito da «Luci accese», dal conteggio, dalla carta e da «Spegni tutte», una appena accesa entra, prima di ogni conferma di Home Assistant. Carta, pagina e `spegni_tutte` partono da lì: `spegni_tutte` non legge più il modello dell'interfaccia (funziona anche a pannello chiuso), `casa::riassunto` riceve `accese: usize`, e dopo un tocco, «Spegni tutte», un esito fallito o un'attesa scaduta la carta di Casa si ricalcola (`moduli_aggiorna("casa")`). Una luce mostrata spenta non tiene la luminosità.
- **`Preferito.luminosita`** dal `brightness` del mapper (già in percento); `casa::frase_breve` diventa `frase_errore_breve(e)` sulla stessa chiave di `frase_errore` (errore del portachiavi).
- **Strutture Slint:** `TipoCardUi`, `CardUi`, `FiltroUi`; `SintesiUi` guadagna `azione-primaria` e `azione-icona` (Agenti: falso e un'icona «nessuna» che non disegna nulla). Un'`Image` vuota di Slint non è uguale nemmeno a un'altra vuota: una riga che la contiene risulterebbe sempre cambiata e sempre riscritta, quindi la sintesi non la usa mai vuota (il test `la_vista_claude_si_scrive_solo_aperta` confronta i campi, non più con `SintesiUi::default()`).

### Changed (Ridisegno F3, Task 2: campo `r1` del contratto e carta Agenti come la pagina)
- **`r1`, il titolo della testata di pagina, entra nel contratto dei moduli.** `Riassunto.r1: R1 { fatto, valore }` (`fatto` ≤ 32 caratteri e ≤ 320 px nello stile Titolo, `valore` ≤ 8; `SPAZIO_R1`, `MASSIMO_FATTO`, `MASSIMO_VALORE` in `moduli/misura.rs`); `controlla` scarta un `r1` fuori misura (`r1.fatto`, `r1.valore`) e il primo avvio dice il motivo anche lì. Ogni modulo lo dà con la sua scala: Agenti «Da collegare» / «A riposo» / «1 sessione» / «{n} sessioni» / «{n} al lavoro» / «1 ti aspetta» / «{n} ti aspettano»; Casa «Da collegare» / «Mi collego» / «Scollegata» / «Luci spente» / «1 luce accesa» / «{n} luci accese»; PC «Tutto regolare» / «CPU {c} %» / «RAM {r} %» / «Batteria {b} %» o il motivo; Collegamenti «Nessun collegamento» / «1 collegamento» / «{n} collegamenti». La sintesi della pagina Agenti prende il titolo da `r1.fatto` (tono e sotto-testo restano dalle righe).
- **m2: la carta Agenti dice il permesso come la pagina.** Con un permesso in attesa la carta diventa «1 ti aspetta» (tono Aspetta, riga «{sessione}: {strumento}», intera per Narrator «{sessione} chiede: {strumento}») o «{n} ti aspettano» / «{n} permessi», e non ha azione: il permesso si decide nella pagina, nulla cambia nelle regole di M3. Il conteggio è delle **sessioni** con un permesso (le stesse righe che la pagina porta in alto, da `permessi_della_pagina`), non dei permessi; la carta li riceve dal più vecchio (ora della richiesta, poi numero della voce) e si ricalcola a ogni cambio delle voci in attesa. Senza permesso la carta resta «Tocca a te» e simili.
- **Il titolo della testata di pagina ha il tracking dello stile Titolo** (`letter-spacing: -0.27px` in `TestataPagina`), così la misura in pixel di `r1.fatto` è giusta.

### Changed (Ridisegno F3, Task 1: misura di partenza)
- **Solo misure, nessun sorgente di prodotto toccato.** Partenza di F3 (cd97bd6, exe release su istanza di prova: cartella finta con Casa finta collegata e 6 preferiti, 6 collegamenti finti, `finto_ha` acceso, `tools\misura.ps1 -Secondi 60`, medie dal secondo 10, RAM = memoria privata, tasti con `PostMessage` al solo PID di prova). **Tutte le misure sono state prese a PC bloccato** (schermata di blocco in primo piano, la pillola si vede solo come finestra 24×24, non si disegna): la riga «a riposo» è quindi senza pillola disegnata e va rifatta a schermo sbloccato (sotto).

  | Scenario (due prove) | RAM privata | CPU media |
  |---|---|---|
  | A riposo | 8,42 e 8,42 MB | 0,004 e 0,002 % (picco 0,11) |
  | `--apri`, Panoramica ferma | 15,14 e 15,46 MB | 0,077 e 0,118 % (picchi 0,44 e 0,78) |
  | Pagina Casa aperta e ferma | 15,47 e 15,43 MB | 0,075 e 0,079 % |
  | Pagina PC aperta e ferma | 17,30 e 17,12 MB | 0,112 e 0,116 % |
  | Pagina Collegamenti aperta e ferma | 15,00 e 15,01 MB | 0,091 e 0,083 % |

  Con le due uscite di controllo di `--apri` (stessa exe, più tardi): 15,13 e 15,13 MB, 0,098 e 0,081 %. Le quattro prove di `--apri` sulla stessa exe vanno da 0,077 a 0,118: il rumore fra una prova e l'altra è più grande di 0,015.
- **m3 (i due `Permesso` sempre vivi): NON confermata.** Con `permesso.slint` svuotato (stessa interfaccia, nessun elemento) `--apri` fa 0,082 e 0,071 % di CPU e 15,31 e 15,09 MB, contro 0,098 e 0,081 % e 15,13 e 15,13 MB della stessa exe di partenza misurata subito dopo (e 0,094 % di media sulle quattro prove): scarto di 0,016 e 0,010 punti, o 0,012 e 0,023 sulla media. Non scende di 0,015 in entrambe le prove con nessun confronto. Niente da rimediare nel Permesso; regola per F3 comunque: gli impianti nuovi stanno dentro `if vista == …` (nessuna istanza fissa viva fuori dalla sua pagina). Anche questa misura è a PC bloccato.
- **Pillola a riposo a schermo sbloccato: NON misurata**, il PC è rimasto bloccato per tutta la prova (controllato fino alle 08:45, con attesa di una decina di minuti). Con la build usa e getta del contatore (mai committata) si sono contati 29 fotogrammi in tutto, tutti nei primi 5 secondi dall'avvio e poi nessuno in 60 s (due prove): è il caso «pillola non disegnata», non il battito degli occhi. Da rifare appena lo schermo è sbloccato.

### Fixed (Ridisegno F2, correzioni della revisione finale)
- **«Consenti» a posto fisso, sorvegliato (I1)**: nuovo test `consenti_a_posto_fisso_col_testo_che_cambia`, per le due istanze del permesso (Coda e dettaglio di Agenti). Il rettangolo intero di «Nega» e «Consenti» è lo stesso con un testo corto, con uno più alto del riquadro e con la stessa voce aggiornata sul posto con un testo più lungo. Prima una modifica che spostasse «Consenti» di 26 px non faceva fallire nessun test. Solo test, nessun codice di prodotto.
- **«Chiede: Comando» non si taglia più nella riga (m1)**: nelle righe con un permesso della pagina Agenti si accorcia prima il nome della sessione (non sotto 48 px) e la cosa resta intera finché c'è spazio; le righe senza permesso sono com'erano. Test `riga_col_permesso_non_taglia_la_cosa`.
- **Commento del controllo in «Consenti» (m5)**: `if (root.mostra(...))` in `permesso.slint` dice di essere una difesa in più (Slint non consegna clic a un elemento invisibile). Solo il commento.

### Changed (Ridisegno F2, chiusura: misure e confronto)
- **Che cosa c'è in F2**: impianto Elenco + dettaglio e pagina Agenti nuova (sintesi coi limiti a barra e la spesa, elenco delle sessioni, dettaglio), il permesso come componente del guscio (usato dalla Coda e dalla pagina Agenti, con le regole di M3 dentro), l'aspetto inerte di «Nega» e «Consenti» per i 600 ms dell'armamento, il clic sulla pillola con un permesso che porta alla pagina Agenti. Chiusi i punti parcheggiati da F1 (n1/n2 della voce, «Errore» rosa, «Nega» muto senza segno): elenco in `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md` («Dopo F2»).
- **Verifiche**: `cargo test --workspace` 1052 passati, 0 falliti, 4 ignorati (F1 chiudeva a 983); `cargo clippy --workspace --all-targets -- -D warnings` pulito; `cargo build --release -p assistente` ok.
- **Misure** (exe release su istanza di prova, cartella finta, nessuna casa, `tools\misura.ps1`, medie dal secondo 10, RAM = memoria privata; partenza = release di d1c2845 misurata nel Task 1, e per confronto la stessa release rimisurata nello stesso giro di prove):

  | | Partenza (Task 1) | Dopo F2 |
  |---|---|---|
  | A riposo (solo pillola), RAM privata | 7,9 MB | **7,8–7,9 MB** (media 7,9) |
  | A riposo, CPU media | 0,02 % | **0,002 %** (picco 0,11 %) |
  | Pannello aperto fermo (`--apri`), RAM privata | 14,1 MB | **14,2–14,5 MB** (due prove: 14,2 e 14,45) |
  | Pannello aperto fermo, CPU media | 0,05 % | **0,08 %** (due prove, picchi 0,3–0,4 %); la release di partenza, rimisurata, 0,056 % |
  | Pagina Agenti aperta, 3 sessioni finte al lavoro, RAM privata | 14,8 MB | **15,2 MB** (14,9–15,5; la partenza rimisurata 15,2) |
  | Pagina Agenti con 3 sessioni, CPU media (dal secondo 15) | 0,30 % | **0,29–0,31 %** (due prove; la partenza rimisurata 0,33 %), picchi 3–3,5 % agli `Stop` delle sessioni |
  | Dopo 10 aperture e chiusure di Agenti (secondo avvio, Esc, 15 s), RAM privata | 10,2–10,5 MB (F1) | **10,5 → 10,8 MB** (cicli 2–10: 10,58–10,80; la partenza rimisurata 10,23 → 10,69) |

  Niente cresce a ogni apertura: il ritorno dopo la chiusura sale di 0,3 MB fra il primo ciclo e l'ultimo, come la partenza (0,5 MB), e resta 2,6–2,9 MB sopra il riposo puro (il pannello lascia in memoria ciò che Skia ha messo in cache, come già in F1). Il pannello fermo costa circa 0,02 punti di CPU più della partenza nelle stesse ore (fra i picchi dei battiti degli occhi); la pagina Agenti non costa più della partenza.
- **Fotogrammi** (contati dai `RedrawRequested` di winit con una build usa e getta, mai committata): con il pannello aperto fermo restano solo il battito degli occhi (circa 12 fotogrammi in 190 ms ogni 4,6 s) e, una volta al minuto, i 2 fotogrammi del cambio dell'ora; con gli occhi che non battono (umore `sleep`) **nessun altro fotogramma** in 25 s. All'arrivo di una voce che ti aspetta (`--prova-avviso bloccante --dopo 15`, a pannello aperto) c'è una sola raffica di 38 fotogrammi che dura 611 ms (600 ms dell'armamento più il fotogramma di chiusura, nel margine di un fotogramma del Task 4) e poi niente fuori dal battito. La pillola a riposo non è stata misurata in questo giro: il PC era bloccato (la pillola non si disegna) e va rifatta con lo schermo sbloccato.
- **Confronto visivo** con `tc-agenti`, `tc-coda` e `tc-griglia-aspetta` (finestra vera dell'app a 175 % e 100 %): composizione, gerarchia, luce dal basso, capsule e anello del fuoco coincidono. Differenze volute o note: «Consenti» è neutro (regola M3), niente Codex, Mail, Musica e Focus (quattro moduli), «chiesto alle 01:54» al posto di «da 42 s», la cartella è il solo nome finale, l'anello del fuoco sta su una scheda del dock a pillola; nell'elenco la parola accanto al nome è tagliata con l'ellissi quando non c'è posto («Chiede: Coma…», il testo intero lo sente Narrator). Le voci con il solo pulsante delicato (n1/n2) non si possono produrre dal vivo (oggi nessun modulo le crea): le provano i test del Task 1. La Coda col permesso aperto non è stata fotografata dal vivo (stesso componente della pagina Agenti, provato dai test).
- **Prova dal vivo con Paolo**: da fare (Task 9, Step 5); l'elenco delle cose da provare è nei rimandati.

### Fixed (Ridisegno F2, Task 8, giro di correzioni 1)
- La guardia dell'arrivo dalla pillola vale solo per il primo cambio del dettaglio: un test prova che, se più tardi la riga col fuoco sparisce e al suo posto arriva quel permesso, il fuoco torna sulla riga (regola del Task 7).
- `entra-nel-permesso(voce)` riceve la voce chiesta ed entra solo se il dettaglio mostra proprio quella; altrimenti il fuoco resta sulla riga.
- Una regola sola per «quale permesso porta ogni riga» (`permessi_della_pagina`), usata dalla pagina e dalla pillola; la richiesta della pillola la consuma solo la prima scrittura della pagina (tolto un secondo svuotamento ridondante), con un test che lo distingue.

### Changed (Ridisegno F2, Task 8: dalla pillola il permesso si apre nella pagina Agenti)
- **Decisione di Paolo 5**: il clic sulla pillola che mostra un permesso apre la pagina Agenti con la riga di quella sessione già scelta (anche se non è la prima) e il fuoco nel permesso del dettaglio, su «Nega»: il comando intero e la sessione si leggono lì prima di decidere. Prima si apriva il permesso nella Coda.
- Regole di M3 invariate su questa strada, ognuna con un test: fuoco iniziale su «Nega»; «Consenti» solo dopo il fondo del testo e solo con Ctrl+Invio esatto (Invio e Spazio non concedono); riarmo di 600 ms all'arrivo, coi pulsanti inerti; un clic col puntatore fermo dall'arrivo non decide (regola del Task 7). «Nega» e «Consenti» stanno in basso a destra del corpo, sotto la riga alta e la testata e lontani dal punto della pillola appena cliccata, in ogni istante dell'arrivo.
- **Ripiego**: un permesso senza riga nella pagina (Claude scollegato, la richiesta non più viva) si apre come prima nella Coda col testo intero. «Apri» della voce nella Panoramica porta ancora alla Coda (la decisione riguarda solo la pillola).
- Interno: `DallaPillola` perde `Copy` e ha `Permesso(String)` (l'id della voce) e `PermessoInCoda(i32)`, decisi in `apertura_dalla_pillola` (`permesso_ha_riga`); `Ponte::chiedi_permesso` chiede la vista e la voce insieme, una volta sola (la consuma la prima scrittura della pagina; la chiusura del pannello la toglie); `Pannello.entra-nel-permesso(voce)`.

### Fixed (Ridisegno F2, Task 7, giro di correzioni 2)
- **Il movimento che conta è solo quello a pulsante del mouse alzato**: premere sul punto fermo, scivolare di un pixel e rilasciare (un secondo clic che trema) faceva decidere «Consenti». Ora la pressione non dà il punto di partenza e i movimenti a pulsante premuto non contano.
- **Il movimento finto che Slint manda dopo ogni rilascio non conta più** (giro di correzioni 3): il punto di partenza si riancora al punto del rilascio. Prima, nello scenario della carta, un clic che trema (premuto fermo, scivolato di un pixel, rilasciato) faceva decidere il clic fermo successivo, e così anche due tocchi in punti appena diversi.
- **Conseguenza voluta sul touch**: un tocco arriva come pressione e rilascio, senza movimenti a dito alzato, quindi tocchi ripetuti (anche in punti diversi) non decidono un permesso. I permessi si decidono con il mouse, la tastiera (Ctrl+Invio esatto; Invio su «Nega») o Narrator. Da provare dal vivo su uno schermo touch vero (il backend di Windows potrebbe mandare altri movimenti).

### Fixed (Ridisegno F2, Task 7, giro di correzioni 1)
- **Nessun clic ripetuto su un permesso nato sotto il puntatore**: con la carta Agenti al 3° o 4° posto della Panoramica, il clic sulla carta apriva la pagina e «Nega» o «Consenti» del permesso scelto nascevano sotto lo stesso punto; un secondo clic lì dopo i 600 ms decideva senza che si fosse guardato. Ora «Nega» e «Consenti» di ogni permesso (Coda, dettaglio di Agenti) e della voce che ti aspetta contano il clic solo se il puntatore si è **mosso** sopra di loro dopo la comparsa (o dopo un'altra voce, o «Consenti» comparso in fondo al testo); basta un pixel. Tastiera (Ctrl+Invio, Invio su «Nega») e Narrator invariati; nessun timer. Il confronto è in coordinate della finestra: lo scivolamento d'arrivo della vista non conta come movimento.
- Un test per un permesso che ne sostituisce un altro col fuoco su «Nega»; la sonda conferma che l'assegnazione in `init` di `Inerzia` si anima (commento corretto); la promessa del Task 4 «non la anticipa mai» corretta qui sotto.

### Changed (Ridisegno F2, Task 7: il permesso nel dettaglio della pagina Agenti)
- **Il permesso si legge e si decide nella pagina Agenti**: la riga di una sessione che chiede un permesso ha per dettaglio il permesso stesso (il componente del guscio della Coda): capsula «Chiede il permesso», «chiesto alle…», «TERMINALE», «$» e il comando intero, la sessione e la cartella, «Nega» e «Consenti» in basso a destra. Non c'è più «Apri il permesso» e non si passa più dalla Coda.
- Regole di M3, invariate anche qui: → o Invio sulla riga portano il fuoco su «Nega» (mai una decisione), ← torna alla riga; «Consenti» solo dopo il fondo del testo e solo con Ctrl+Invio esatto (Invio, Spazio e le altre otto combinazioni non concedono); «Nega» e «Consenti» neutri, a posti fissi; nessun «Sempre». Ogni decisione passa dalla guardia di Rust (`azione-voce`).
- **Riarmo di 600 ms** quando la scelta arriva su un permesso senza un gesto sul permesso (↑/↓, clic su una riga, la pagina che si apre, la riga col fuoco tolta che lascia il posto a un permesso) e quando «Consenti» compare in fondo al testo; i pulsanti si vedono inerti. Se il dettaglio cambia sotto il fuoco (il permesso risolto nel terminale, un permesso al posto del terminale) il fuoco torna sulla riga della sessione, e Tab non si perde.
- Nessun pulsante nasce sotto un clic precedente: i pulsanti del permesso stanno in basso a destra, lontani dalle righe (anche quella appena cliccata) e da «Apri il terminale» in alto. Un permesso che non è quello della riga (un'altra voce al suo posto) non si mostra mai.
- **Un solo `Inerzia`**, fisso nel pannello, per la voce che ti aspetta, i due permessi e ora anche **le righe della Coda col permesso**, il cui «Nega» passa dalla stessa guardia e si vede inerte («Apri» e i pulsanti delle altre voci no). Se il pannello si apre mentre la guardia è ancora in corso (spostata a pannello chiuso) conta un armamento e lo mostra. Il commento di `Inerzia` non promette più la fine «mai prima» della guardia al millisecondo (può finire qualche ms prima, dalla parte sicura).
- Il permesso del guscio creato con la voce già impostata segna il fondo del testo (prima no, dalla parte sicura).
- Prove: `sessioni_finte --permesso` (esempio di `claude-hook`) fa chiedere alla sessione 0 un permesso finto e stampa la decisione. Interno: tolti `pagina_agenti::PERMESSO` e `claude_apri_permesso`; `GeometriaElencoDettaglio` condivide le misure dell'impianto col pannello.

### Fixed (Ridisegno F2, Task 6, giro di correzioni 1)
- **Senza chiave da amministratore la testata della pagina Agenti dice perché non c'è la spesa**: al posto dell'importo, in testo tenue su al più due righe, «Aggiungi una chiave da amministratore nelle Impostazioni» (o «nessun dato» con la chiave ma senza lettura); Narrator la legge come valore di «Spesa API del mese». Prima non si vedeva e non si sentiva.
- Il test delle azioni della pagina ora prova che «Apri il permesso» apre proprio il permesso di quella sessione (anche con un'altra voce davanti nella Coda), da tastiera, col fuoco su «Nega».

### Changed (Ridisegno F2, Task 6: impianto Elenco + dettaglio, la pagina Agenti ci si sposta)
- **La pagina Agenti è nuova** (bozza `tc-agenti`): in alto la testata con la sintesi («1 ti aspetta», «2 al lavoro», «3 sessioni»…), i limiti dell'abbonamento come barre col numero («Claude 5 ore ▬ 34 %», «7 giorni ▬ 61 %», caldo nel colore del lavoro dalla soglia d'avviso, sul numero che si legge), la spesa API del mese in mono e «Dettagli» (solo con la chiave da amministratore); a sinistra le sessioni, tutte, in una colonna che scorre (occhietti del tono, nome, «Claude», cosa fa, durata in mono); a destra il dettaglio della sessione scelta (capsula del tono, nome, cartella, strumento / da quanto / contesto, nota sul terminale) col pulsante **«Apri il terminale»** in alto a destra. La vista Claude di M3 (`ui/viste/claude.slint`, anelli e card) non c'è più.
- **Decisioni di Paolo**: il clic su una riga la sceglie e mostra il suo dettaglio, senza aprire nulla; il terminale si apre dal pulsante del dettaglio o con **Invio sulla riga**. Il nome della sessione è il titolo della scheda di Claude Code, con la cartella sotto nel dettaglio. **La sessione in errore è rosa**, non più blu.
- Tastiera: ↑/↓ fra le righe (la riga scelta resta in vista; ↑ dalla prima porta a «Dettagli», ↓ torna), → al pulsante del dettaglio e ← di nuovo alla riga, Backspace un livello (dettaglio → elenco → Panoramica), Esc chiude. Il fuoco non si perde quando «Dettagli» sparisce. Narrator legge ogni riga come voce d'elenco col nome intero e il suo posto («3 di 5»), e se è quella scelta.
- **Nessun permesso si decide da questa pagina**: la riga di una sessione che chiede un permesso porta, con Invio o col pulsante «Apri il permesso», al permesso aperto della Coda, con tutte le regole di M3 (fuoco su «Nega», «Consenti» solo dopo il fondo del testo e solo con Ctrl+Invio, armamento di 600 ms). Invio sulla riga non decide nulla; Ctrl+Invio nella pagina non fa nulla. Il permesso nel dettaglio arriva col Task 7.
- Stati della pagina: scollegato «Collega Claude Code» con **«Collega»** (come la carta: le Impostazioni sulla scheda Claude, non più «Impostazioni»); nessuna sessione «Nessuna sessione aperta», senza pulsanti. Nessun limite = nessuna barra (mai «0 %»).
- Leggerezza: le righe, la sintesi e lo stato si scrivono solo a pagina aperta e solo se cambiano (anche la riga scelta); un test lo conta dentro `aggiorna_modello` e nella scrittura delle proprietà. Nessun timer nuovo.
- Interno: `TestataPagina` (ora in `ui/guscio/pagina.slint`, con altezza propria: la Coda la tiene a 28 fino a F5) e `StatoPagina`; impianto `ImpiantoElencoDettaglio` con `RigaModulo` e `DettaglioRiga` (`ui/impianti/elenco_dettaglio.slint`). Il pannello ha `elenco-sintesi`, `elenco-stato`, `elenco-righe`, `elenco-fuoco`, `elenco-lato` e una sola callback `azione-pagina(modulo, azione, riga)` (tradotta da `Ponte::azione_pagina`) al posto di `apri-sessione`, `gestisci-claude` e `apri-dettagli-claude`. Tolti `RigaSessioneUi`, `LimiteClaudeUi`, `Tema.oltre-soglia` e `claude::frazione_soglia`.

### Added (Ridisegno F2, Task 5: righe, sintesi e stato della pagina Agenti)
- Nessuna parte visibile cambia ancora (la pagina li mostrerà dal Task 6). Nuovo modulo interno `moduli::pagina_agenti`, funzioni pure: **la riga di una sessione** (tono, parola, nome, cosa, durata, tre campi del dettaglio, azione, nota e nome intero per Narrator), **la sintesi** (titolo, limiti a barra, spesa, «Dettagli») e **lo stato della pagina** (scollegato, nessuna sessione, elenco). Nuove strutture Slint `RigaModuloUi`, `CampoDettaglioUi`, `LimiteUi`, `SintesiUi`, `StatoPaginaUi`/`TipoStatoPaginaUi`, esportate verso Rust.
- **La sessione in errore è rosa** (tono Errore), non più blu come la parola «Errore» sul livello attenzione della vista di F1 (rimandato di F1). Scala delle righe: permesso in attesa «Chiede: Comando» (aspetta), al lavoro con lo strumento e la durata, «Tocca a te» (guarda), la frase dell'errore, «Ha finito» (fatto, blu) e «Fatto» (calmo) già guardato.
- Il nome della riga è il titolo della scheda di Claude Code, con la cartella sotto; senza titolo (o con un titolo di soli invisibili) la cartella. Per Narrator: «{nome intero}, Claude, {tono}, {cosa}», mai la durata.
- Sintesi: «1 ti aspetta» / «2 ti aspettano» coi permessi, altrimenti «N al lavoro» o «1 sessione» / «N sessioni», «A riposo» senza sessioni, «Da collegare» scollegato; sotto il conteggio delle sessioni, al singolare con una, e niente quando il titolo è già il conteggio, senza sessioni o da scollegato. Limiti «Claude 5 ore 61 %» (lo spazio prima del «%») e «7 giorni», con lo stesso numero della carta e della vista di oggi (una sola fonte, `TestoLimite.percento`: anche a 52,5 % tutte dicono 53), caldi dalla soglia dell'utente, il valore ricordato tenue con «dalle 20:29»; un limite assente non c'è (mai «0 %»). «Dettagli» solo con la chiave da amministratore.
- `claude::RigaSessione` porta lo stato vero della sessione (`stato`, `vista`, la frase dell'`errore`) e il contesto come numero, al posto delle parole e del livello già scelti: le parole le sceglie chi mostra la riga. La vista Claude di oggi mostra le stesse cose di prima («Lavora», «Tocca a te», «Fatto», «Errore», «42%»).

### Changed (Ridisegno F2, Task 4: aspetto inerte durante l'armamento)
- **«Nega» e «Consenti» si vedono inerti per i 600 ms dell'armamento** (rimandato m2 di F1: «Nega» era muto senza segno), nel permesso aperto della Coda e nella voce che ti aspetta della Panoramica: testo tenue sul fondo neutro, niente passaggio né mano, il clic e l'azione di Narrator non fanno nulla e Narrator li sente spenti. «Apri» della voce non passa dalla guardia e resta attivo.
- Rust conta i riarmi in `Pannello.armamenti`: +1 se e solo se `dal` (la guardia dei 600 ms) si sposta a pannello aperto, da una sola funzione (`sposta_armamento`); a pannello chiuso nessuna scrittura. La guardia resta in Rust: il pannello la segue (l'aspetto può finire qualche ms prima della guardia, dalla parte sicura: correzione del Task 7).
- Un solo componente, `Inerzia` (`ui/componenti.slint`), per la voce e il permesso: un'animazione lineare di 600 ms su una proprietà che non si disegna, avviata a ogni incremento e poi ferma. Nessun timer, nessuna animazione continua; col movimento ridotto nulla si muove (il cambio d'aspetto è di colpo anche senza), ma i 600 ms restano. Inerti già nell'istante dell'incremento: il permesso aperto nasce inerte (sulla strada vera, pre-flight C2).
- Nuovi test: conteggio dei riarmi nel ponte, inerzia del permesso e della voce per 600 ms, all'apertura, col movimento ridotto, e il colore disegnato (skia senza schermo) del testo inerte.

### Fixed (Ridisegno F2, Task 3, giro di correzioni 1)
- **Nessun pulsante del permesso nasce più sotto un clic precedente**: nella Coda «Nega» e «Consenti» erano alti 40 e coprivano i 4 px alti della prima riga dell'elenco, così un doppio clic sul bordo della riga apriva il permesso e premeva «Nega» o «Consenti» (lo fermava solo la guardia dei 600 ms). Ora nella Coda sono alti 32 e finiscono 4 px sopra le righe. Un test confronta i rettangoli interi di ogni pulsante nato con i bersagli di prima (righe, pulsanti, «Apri» della voce); un altro fa il doppio clic lungo tutto il bordo alto della riga.
- **«Torna all'elenco» torna** per chi usa il mouse, in alto a sinistra nella fascia dei pulsanti, fuori da quella delle righe.
- L'etichetta sopra il testo dice cos'è: «TERMINALE» per un comando, «FILE» per Legge/Modifica/Scrive, «INDIRIZZO» per WebFetch, «RICERCA» per WebSearch, nessuna per gli altri; la sceglie Rust dal nome vero dello strumento (`campo` in `DatiPermesso` e `PermessoUi`). «$» solo per il terminale.
- Il fondo visto vale per l'id del permesso: un altro permesso che arriva nello stesso componente non eredita «Consenti» nemmeno per un istante.
- Narrator legge comando e sessione una volta sola (nella descrizione del gruppo); dopo un clic su «Nega» o «Consenti» l'anello da tastiera si spegne.
- Nuovo test sul colore disegnato (skia senza schermo nel backend di prova): «Consenti» ha il fondo grigio neutro come «Nega».

### Changed (Ridisegno F2, Task 3: il permesso è un componente del guscio, usato dalla Coda)
- **Nuovo componente `Permesso`** (`ui/guscio/permesso.slint`) con tutte le regole di M3 dentro: fuoco iniziale su «Nega», «Consenti» solo dopo il fondo del testo intero (prima il suo posto è vuoto, inerte e senza nome per Narrator), solo Ctrl+Invio esatto concede, Invio preme solo «Nega», «Nega» e «Consenti» neutri e a posti fissi, nessun «Sempre». La tastiera la governa il componente (`entra()`, `tasto()`); ogni decisione esce da `decidi`.
- **Aspetto della bozza della pagina Agenti**: coi dati del permesso capsula «Chiede il permesso», «chiesto alle 14:32» accanto, riquadro nero con «TERMINALE», «$» e il comando in mono (a capo anche una riga sola senza spazi, scorrevole con la sfumatura) e sotto, sempre in vista, «{sessione} · {cartella}» (la cartella solo se c'è e non è la sessione; «$» solo per lo strumento «Comando»). Senza dati (la voce di prova) titolo a capo e dettaglio intero in mono, nessun campo vuoto. In fondo «Leggi fino in fondo per consentire» o il tasto «Ctrl Invio» + «consente». Narrator: «Ti aspetta · Agenti: my-shop chiede: Comando», col comando come descrizione.
- **La Coda usa il componente al posto di `VoceAperta`** (tolta), coi pulsanti in alto. Il componente è un'istanza fissa del pannello, mai dentro un `if` (uno spike ha confermato che un id dentro un `if` non si raggiunge da fuori, e un componente creato all'apertura non vedrebbe il riarmo del Task 4). «Torna all'elenco» non c'è più: si torna con Esc o Backspace.
- **Il permesso aperto che cambia posto resta aperto** (rimandato di F1): la validità la decide l'id (`indice-voce`, risposto da Rust sul modello delle attese), non la riga; si chiude solo se il suo id sparisce o un altro permesso prende il suo posto.
- **Le decisioni su un permesso nella Coda passano dalla guardia dei 600 ms** (`azione-voce`), nel permesso aperto e nella riga; le altre voci restano un'`azione`. `global Permessi` si sposta nel file del componente. I test della Coda ascoltano tutte e due le callback, così i test di Ctrl+Invio esatto possono ancora fallire; `doppio_clic_su_apri_non_consente` diventa `consenti_non_nasce_sotto_apri`.

### Added (Ridisegno F2, Task 2: dati del permesso sulla voce)
- La voce di un permesso di Claude Code porta i suoi dati divisi (`DatiPermesso`: sessione, strumento, comando, cartella, ora della richiesta) e il pannello li riceve in `VoceAttesaUi.permesso` (`PermessoUi`), per il componente del permesso del guscio che arriva nel Task 3. Nessuna superficie cambia ancora.
- Sessione = titolo della scheda di Claude Code, altrimenti la cartella; della cartella solo il nome finale; comando ripulito come il titolo (via invisibili, segni di direzione e a capo), vuoto se Claude non l'ha dato. «Chiesto» è l'ora fissa della richiesta, «alle 14:32» (nessun timer). I dati vivono solo in memoria: niente su disco, nessun testo di prompt.
- Il test della notifica di Windows ora controlla anche il comando diviso: non c'è mai, c'è solo il nome della sessione (M3 §10).

### Fixed (Ridisegno F2, Task 1: voce con un solo pulsante delicato)
- **Il fuoco non si perde più** quando la voce che ti aspetta viene sostituita da una che ha solo il pulsante delicato (rimandato n1 di F1): senza «Nega» il fuoco va su «Apri» (che porta al permesso intero); se quel pulsante è «Consenti» (testo intero nella voce) mai su di lui, ma su «+N in attesa» o sulla prima carta. La stessa regola vale quando la voce compare, tornando alla Panoramica, all'apertura, quando «+N» o «N in attesa» spariscono e con ↑ dalle carte. Senza alcun bersaglio la tastiera resta al pannello (Esc chiude, Tab va avanti).
- Se la stessa voce si aggiorna e il suo testo ora sta tutto, «Apri» diventa «Consenti»: «Consenti» si riarma (600 ms) e, se il fuoco era su «Apri», passa al bersaglio successivo; mai un consenso col fuoco addosso senza averlo scelto.
- **Il testo della voce non passa più sotto «Apri»** (rimandato n2 di F1): con il solo pulsante delicato titolo e riga del dettaglio finiscono 12 px prima di lui.
- Nuovo test: «Claude chiede: » con lo strumento più lungo che la pillola mostra (12 caratteri) sta nel titolo della voce con due pulsanti (476 px).
- Misura di partenza di F2 (release di d1c2845, istanza di prova, `tools\misura.ps1 -Secondi 60`): a riposo 7,9 MB privata e CPU 0,02 %; pannello aperto fermo (`--apri`) 14,1 MB e 0,05 %; pagina Agenti aperta con 3 sessioni finte al lavoro 14,8 MB e 0,30 % (picchi fino a 3,5 % agli `Stop` delle sessioni).

### Changed (Ridisegno F1, Task 1: caratteri e token)
- **Caratteri nuovi in tutta l'app**: Inter (500 e 700) per le parole e JetBrains Mono NL (500) per numeri, ore ed etichette maiuscole (tracking .07em), al posto di Inter Tight e JetBrains Mono; licenze OFL accanto ai file. Ogni finestra (pannello, pillola, Impostazioni, cornice della gestione) parte dal peso 500; i pesi 400 e 600 di prima diventano 500 e 700 (nei ternari «scelto ? 700 : 500»).
- **Token neri**: nero vero come fondo, grigi pieni al posto delle trasparenze (`g1`…`g4`, `filo`, `bordo-ctl`), testi `testo` / `testo-2` / `testo-tenue`, toni `attivo` / `aspetta` / `guarda` / `fatto` / `errore`, luci e occhi dei toni, bianco con testo nero come azione primaria (il pulsante d'azione, l'interruttore acceso). «Consenti» e «Nega» restano neutri (regola di M3). `bloccante` diventa ambra, `attenzione` e `informazione` blu; spariscono i fondi viola, l'anello dei petali, i bagliori del pannello e i token `testo-ora`, `testo-etichetta`, `corpo`, `card-vetro`.
- La colonna della vista PC passa da 240 a 252 px: con Inter «1023,9 / 1023,9 GB» è più largo.

### Added (Ridisegno F1, Task 2: misura dei testi in pixel)
- Nessuna parte visibile cambia. Nuovo modulo interno `moduli::misura`: la larghezza di un testo in pixel (avanzamenti dei caratteri incorporati più tracking) e l'accorciamento con «…», per far stare i testi delle carte nello spazio dato (154 px, 116 con il bottone d'azione). Un test lo confronta con la resa di Slint (differenza ammessa 3 px).

### Changed (Ridisegno F1, Task 3: contratto del modulo, Registro e ordine dei moduli)
- **La vista Claude diventa «Agenti»** (id `agenti`), con l'icona del terminale (Lucide) nel dock. **L'ordine del dock cambia**: dopo la Panoramica viene Agenti, poi Casa, PC e Collegamenti (prima: Casa, Claude, PC, Collegamenti). Sotto le voci «In attesa» di Claude Code si legge il nome del modulo: «Aspetta te · Agenti» (prima «Claude Code»).
- Nuovo contratto interno dei moduli (`moduli`): toni (vuoto, calmo, al lavoro, guarda, aspetta, fatto, errore, fermo) con priorità, parola breve e forma per Narrator; identità dei quattro moduli; riassunto di una carta (titolo, riga, azione) controllato nei limiti in pixel. Il `Registro` tiene l'ordine dei moduli, l'ultimo riassunto (uno fuori misura si scarta e resta quello di prima) e il livello delle voci in attesa che alza il tono della carta; il dock legge l'ordine da qui. Il pannello riceve già le carte e il tono dell'isola, che si mostreranno dal Task 8.
- Configurazione: nuova chiave `moduli` (id e «fissato» per ogni modulo, in camelCase). Un file di prima, senza la chiave, si legge con l'ordine predefinito; l'id `claude` di prima si legge come `agenti`; una voce senza `id` o senza `fissato` non fa rileggere tutta la configurazione come nuova.

### Added (Ridisegno F1, Task 4: riassunti di Agenti, Casa e PC)
- Le carte di Agenti, Casa e PC hanno il loro riassunto (si mostreranno dal Task 8), con una scala per stato e testi che stanno nella carta. **Agenti**: «Da collegare» / «Collega Claude»; «N al lavoro» col limite più alto («7 giorni 61 %») o nome e strumento della sessione; «Tocca a te», «Errore» (la frase dell'errore), «Ha finito» col nome; «N sessioni» o «A riposo» col limite o «Nessun limite letto» / «Pronto». **Casa**: «Da collegare»; «Mi collego»; «Scollegata» con la frase dell'errore o «Dati delle hh:mm»; «N luci» / «1 luce» / «Luci spente» con le temperature di dentro e di fuori (accanto al bottone, se non ci stanno, solo quella di dentro; Narrator le legge tutte e due) o «Nessun sensore». **PC**: «—» / «Lettura in corso» finché il primo campione non è pronto, «—» / «Misure non lette» se né CPU né RAM si leggono; «CPU N %» o «RAM N %» da 85 % in su; «Batteria N %» scarica sotto la soglia; altrimenti «Tutto regolare» con CPU e RAM. Mai «0» al posto di un dato assente.
- Un nome lungo (titolo della sessione) si accorcia con «…» nella carta e resta intero per Narrator; il «—» dell'assenza non si legge più nel nome per Narrator.
- I riassunti si ricalcolano solo quando cambia un loro dato e solo a pannello aperto; da chiuso si rifanno una volta all'apertura (nessun timer). Le voci «In attesa» alzano il tono della carta del loro modulo. Il PC prende il riassunto dallo stesso campione della vista PC.
- Azioni delle carte (pannello `azione-modulo`): «Apri il terminale» porta davanti il terminale della sessione di cui la carta parla, come il clic sulla sua riga; «Collega» apre le Impostazioni su Claude o sulle case; «Spegni tutte» e «Riprova» come nella vista Casa.
- La capsula «Fatto» di una carta è blu (fatto non visto, spec §5.2), non più verde acqua.

### Added (Ridisegno F1, Task 5: Collegamenti, il più usato)
- Ogni collegamento aperto dal pannello conta un lancio: nuovi campi `lanci` e `ultimoLancio` (secondi Unix, camelCase come il resto) nella configurazione locale `assistente.json`. Sono l'unico dato nuovo su disco, mai inviato in rete; un file di prima, senza i campi, si legge con zero; modificare un collegamento o riparare il file non azzera il conteggio, eliminarlo lo porta via. Il lancio si conta solo se il browser è partito; scrittura atomica (`.tmp` poi rename) come sempre.
- La carta dei Collegamenti ha il suo riassunto (si mostrerà dal Task 8): «{n} link» / «1 link» con la riga «Più usato: {nome}» e il bottone «Apri {nome}»; senza collegamenti «Nessun link» / «Aggiungine uno» con «Aggiungi» (apre Impostazioni › Collegamenti). Il più usato è quello con più lanci, a parità il più recente, con tutti a zero il primo dell'ordine scelto. Un nome lunghissimo si accorcia con «…» nella carta e resta intero per Narrator e nel bottone. La carta si aggiorna quando si aggiunge, modifica, sposta, elimina o apre un collegamento (nessun timer).

### Changed (Ridisegno F1, Task 6: cornice nera, riga alta, barra con capsula e volume)
- **Il pannello è un'isola nera 800 × 340 con le orecchie** (finestra 820 × 340, prima 800 × 300), lo stesso in ogni vista: un solo contorno disegnato (raccordi concavi di 10 in alto ai lati, angoli bassi di 34). Dal basso sale la **luce del tono più urgente** fra i moduli (arancio al lavoro, ambra ti aspetta, blu guarda presto; nessuna luce negli altri casi): ferma, si accende e si spegne in 1,2 s solo quando il tono cambia (subito col movimento ridotto).
- **Riga alta 44**: data e meteo in mono maiuscolo a sinistra (solo nella Panoramica), occhi al centro, ora e «⋯» (ora 32 × 32) a destra. Aprendo dalla pillola gli occhi partono da dove stavano i suoi (6 px più in alto) e scendono al loro posto. Il contenuto di oggi (riga «In attesa», layout, puntini, viste) sta nel corpo 752 × 224.
- **Barra bassa al posto del dock**: a sinistra la ricerca («Ctrl K», per ora inerte: Narrator la chiama «Cerca, in arrivo»), al centro le schede (solo l'icona; quella aperta ha in una capsula il nome breve maiuscolo, «LINK» per i Collegamenti, e la capsula scivola in 200 ms cambiando scheda), a destra il volume. Sparisce il suggerimento col nome sopra le icone: Narrator legge il nome intero, lo stato e la posizione («Agenti, ti aspetta, scheda 2 di 5»). Sulle schede non aperte, occhietti ambra tondi per «ti aspetta» e blu a mandorla per «guarda presto» (variante «misto»).
- **Pulsante del volume**: clic (o Invio, o Narrator) = muto sì/no, con l'icona e il nome che lo dicono («Volume 40 %», «Volume, muto», anche quando il muto arriva dalla tastiera del PC; l'altoparlante della vista PC usa lo stesso formato, «Volume 40 %»); la rotella sopra cambia il volume di 2 punti a scatto se è accesa «volume con la rotella» nelle Impostazioni › Isola (la stessa scelta della pillola). Senza uscita audio (e fuori da Windows) il pulsante non c'è e la barra non si sposta.
- **Tastiera**: Tab passa dal contenuto alle schede, poi alla ricerca, al volume (se c'è) e a «⋯». Se l'uscita audio sparisce col fuoco sul volume, il fuoco va sulla ricerca.

### Changed (Ridisegno F1, Task 7: Coda minima, la pillola apre la destinazione)
- **Coda** (vista `coda` del guscio, sull'impianto Elenco): testata «N in attesa» con «da M moduli», righe da 40 con il segno del livello (occhietti tondi per «ti aspetta», a mandorla per «guarda presto», un trattino per «quando vuoi»), l'icona del modulo, il titolo e «livello · modulo», e fino a 2 pulsanti; scorre e porta in vista la riga col fuoco. Vuota: «Niente in attesa» e «Torna alla Panoramica». Per Narrator ogni riga è una voce d'elenco «titolo, livello · modulo» col testo completo come descrizione.
- **Sparisce la voce espansa sul posto**: il clic (o Invio) su una voce della riga «In attesa» e su «+N» apre la Coda su quella voce.
- **Tastiera della Coda**: ↑/↓ fra le righe, → ai pulsanti (si entra su «Nega» se c'è, altrimenti sul primo), ← indietro fino alla riga, Invio sul pulsante (mai su un consenso: sulla riga entra nei pulsanti come →), **«Consenti» solo con Ctrl+Invio esatto** (la riga lo dice: «Ctrl+Invio per consentire»), Backspace o Alt+← alla Panoramica, Esc chiude. Una riga che sparisce col fuoco addosso lo passa alla precedente; se al posto della riga col fuoco arriva un'altra voce il fuoco torna sulla riga, mai sul consenso di un'altra richiesta. Entrare nella Coda con un permesso in attesa riarma «Consenti» (600 ms). «Nega» e «Consenti» restano neutri.
- **Un permesso si legge intero prima di concedere** (regola M3): nella riga della Coda il suo pulsante delicato è «Apri», mai «Consenti»; aperto (clic, «Apri», Invio) prende il posto dell'elenco col titolo a capo (fino a 3 righe), «livello · modulo» e il testo intero in mono (comando e «Sessione: …»), scorrevole con ↑/↓ o la rotella e una sfumatura se c'è altro. **«Consenti» compare solo dopo che il fondo del testo si è visto** («Leggi fino in fondo per consentire» fino ad allora), e anche allora solo col clic o con Ctrl+Invio esatto; il fuoco parte da «Nega»; Esc o Backspace lo richiudono, «Torna all'elenco» (in basso a sinistra) col mouse. Se il permesso sparisce o cambia posto mentre è aperto, si richiude e il fuoco va sulla riga. **«Nega» e «Consenti» stanno a posti fissi in alto a destra dall'apertura**: il posto di «Consenti» resta vuoto e inerte (anche per Narrator) finché non si può concedere, quindi «Nega» non si sposta mai e «Consenti» non nasce dove un attimo prima c'era «Nega» o «Apri» di una riga; aprire un permesso e il comparire di «Consenti» lo riarmano (600 ms), così un doppio clic su «Apri» non concede.
- **«Consenti» si riarma (600 ms) anche quando un permesso cambia posto** nella Coda (una riga che sale sotto il puntatore porta il «Consenti» di un'altra richiesta), non solo quando cambia la prima voce o il bloccante più vecchio.
- **La pillola apre la destinazione**: una voce allargata porta alla pagina del suo modulo (Claude → Agenti, Casa, PC), un permesso alla Coda con quel permesso già aperto (il testo intero: lì porta anche «Apri» della pillola col testo tagliato), il puntino alla Coda, l'attività di Claude ad Agenti, il riposo alla Panoramica.

### Changed (Ridisegno F1, Task 8: Panoramica a griglia, voce che ti aspetta; via layout e quadranti)
- **La Panoramica è una griglia di carte**: in alto l'ora grande (Inter 700 96, cifre in celle ferme: cambiando i minuti nulla balla), sotto le quattro carte dei moduli in una riga (182 × 108, spazio 8, gruppo centrato). Ogni carta ha l'icona e il nome (il nome breve se c'è la capsula), la capsula del tono con gli occhietti, il valore (mono se è un numero) e la riga; la luce sale dal basso per i toni che ce l'hanno. Il bottone d'azione (bianco, tondo 32) si vede al passaggio del mouse o col fuoco sulla carta, e allora valore e riga si accorciano per non finirci sotto. Il clic sulla carta apre la pagina del modulo; la carta Casa con luci accese apre la Casa filtrata (prima era la riga «N luci accese»).
- **La voce che ti aspetta al posto dell'orologio**: la prima voce bloccante (un permesso di Claude) prende il posto dell'ora grande (che torna piccola nella riga alta), con la capsula «Ti aspetta · {modulo}», il titolo, la prima riga del dettaglio (il comando in mono), «Nega» (col fuoco) e «Consenti», neutri tutti e due, e «+N in attesa ›» che apre la Coda. «Consenti» c'è solo se il testo intero si legge nella voce; altrimenti il pulsante è «Apri» e porta al permesso aperto nella Coda, dove «Consenti» compare dopo il fondo del testo. Da tastiera concede solo Ctrl+Invio esatto; la voce che compare riarma «Consenti» (600 ms), e nei 600 ms dopo un cambio della voce i suoi pulsanti si ignorano (un doppio clic su «Nega» non nega anche la richiesta che prende il posto della prima). I pulsanti della voce, «Apri» e «+N» stanno dove la Coda non mette pulsanti: un doppio clic non preme nulla di nato sotto il puntatore.
- **«N in attesa ›» nella riga alta**, dopo data e meteo, quando ci sono voci in attesa e nessuna ti aspetta: apre la Coda.
- **Tastiera della Panoramica**: Tab segue un ordine unico calcolato a parte (voce: «Nega» → «Consenti» → «+N»; «N in attesa ›»; carte: corpo → azione, solo se c'è; schede una per una; ricerca; volume; «⋯»); ←/→ fra voce e carte, ↑/↓ fra voce e carte; Invio apre la carta o fa l'azione. Un bersaglio che sparisce col fuoco addosso (la voce risolta nel terminale, l'azione di una carta che non c'è più) passa il fuoco al più vicino. Per Narrator la carta è un pulsante col nome intero del modulo; i testi della carta non si ripetono e una carta senza azione non ha un pulsante vuoto.
- **Tolti i layout Orologio e Quadranti** (coi puntini, le frecce e la rotella che li cambiavano), la riga «In attesa» della Panoramica e la sezione «QUADRANTI» di Impostazioni › Isola col campo «Massimo». Configurazione: `ultimoLayout`, `quadranti` e `massimiQuadranti` di un file di prima si leggono senza errori (gli altri campi restano) e spariscono al primo salvataggio; i sensori seguiti sono solo quelli dei lati della pillola. Dischi, reti e GPU si leggono solo nella vista PC.

### Added (Ridisegno F1, Task 9: Panoramica Orologio, Impostazioni › Panoramica)
- **Impostazioni › Isola › Panoramica**: «Griglia» (di serie) o «Orologio», salvata in `panoramica` solo se cambia; vale dal pannello seguente. Un file di prima senza la chiave (o con un valore sconosciuto) apre la Griglia.
- **Panoramica Orologio**: l'ora grande, sotto la **fascia** alta 32 senza fondo, sempre alla stessa quota, e le quattro **tessere** 182 × 84 centrate, negli stessi posti e con lo stesso ordine di Tab delle carte della Griglia; il bottone d'azione delle tessere si vede sempre (valore e riga accorciati di conseguenza). La fascia dice per regola «Niente ti aspetta» e, se ci sono voci in attesa non mostrate, «· {n} avvisi ›» (un bersaglio che apre la Coda, al posto di «N in attesa ›» della riga alta, nello stesso posto di Tab: dopo la voce, prima delle tessere); al primo avvio «Avvio in corso · {n} moduli in attesa di dati».
- **La voce che ti aspetta nell'Orologio** prende il posto dell'ora (che va nella riga alta) nello stesso punto della Griglia, con le stesse regole di Claude: «Nega» col fuoco, «Consenti» solo col testo intero e solo con Ctrl+Invio esatto, riarmo di 600 ms. L'ora si dissolve in 160 ms e la voce entra da scala 0,96 in 280 ms (subito col movimento ridotto); le tessere non si muovono e la fascia tace.

### Fixed (Ridisegno F1, Task 10: pagine, pillola, Impostazioni e gestione sulla base nera)
- **Le viste usano tutto il corpo**: Casa, Agenti, PC, Collegamenti e Coda sono alte 224 come la Panoramica (prima 202, sopra la vecchia riga dei puntini).
- **Sovrapposizioni staccate dalle righe**: l'elenco di un selettore, la scelta dell'icona di una stanza e la conferma di un'eliminazione hanno il fondo dei menu (g2, un gradino sopra le righe g1) con l'ombra; la cella sotto il mouse e quella evidenziata usano i gradini del menu. Il suggerimento «Apre l'ultimo terminale usato» è g3, sopra la riga g2 sotto il mouse. Il fondo del menu è un grigio scritto a sé, non più l'alias del suggerimento.
- **Anello del fuoco come quello di Windows**: 2 px bianchi e, fuori, 1 px nero (si vede anche sul bianco del primario).
- **Mono sempre maiuscolo**: il titolo della griglia della Casa («PREFERITI», il nome della stanza), il titolo dei moduli e della scelta dell'icona nelle Impostazioni, le colonne e i gruppi della finestra di gestione; Narrator legge i titoli com'erano.
- **Parole e tinte**: nella vista Agenti chi lavora ha l'arancio del lavoro (prima il verde acqua del «fatto») e la sessione che aspetta un tuo messaggio dice «Tocca a te», come la carta («Ti aspetta» resta del permesso); il nome della pillola durante il volume è «Volume 40 %», con lo spazio come la barra e la vista PC; l'avviso di prova (`--prova-avviso --lungo`) parla del permesso aperto nella Coda.
- La traccia di barre e archi resta il grigio appena sopra la carta delle bozze: un test controlla che il pieno si legga (≥ 3:1) sulla traccia, sulla carta e sul nero.

### Changed (Ridisegno F1, chiusura: misure e confronto)
- **Prova dal vivo (23/11, build b2c07e9 nell'isola di Paolo, casa e Claude Code collegati)**: Paolo ha visto Panoramica e pagine — «mi piace tutto». A riposo 17,5 MB privata (48,2 MB WS) e CPU 0,043 %. Il battito degli occhi a riposo resta (decisione di Paolo). Restano da provare dal vivo con permessi veri (negato e consentito da Panoramica e Coda), Narrator con UI Automation vera, volume, scelta Griglia/Orologio e scala 175 %: elenco in `docs/superpowers/plans/2026-11-20-assistente-ridisegno-f1-rimandati.md`. CI b2c07e9 verde su Windows, Linux e macOS.
- **Verifiche**: `cargo test --workspace` 983 passati, 0 falliti, 4 ignorati; `cargo clippy --workspace --all-targets -- -D warnings` pulito; `cargo build --release -p assistente` ok.
- **Misure** (exe release su istanza di prova, cartella finta, nessuna casa, `tools\misura.ps1 -Secondi 60`; partenza = HEAD a3da924, stesse condizioni):

  | | Partenza | Dopo F1 |
  |---|---|---|
  | A riposo (solo pillola), RAM privata | 7,8 MB | **7,8–8,0 MB** (media 7,8) |
  | A riposo, CPU media | 0,03 % | **0,02 %** (picchi isolati 0,11–0,34 %) |
  | Pannello aperto fermo (`--apri`), RAM privata | 13,0 MB | **13,9–14,0 MB** |
  | Pannello aperto fermo, CPU media | 0,08 % | **0,04 %** (picco 0,44 %) |
  | Dopo la chiusura del pannello (Esc, 15 s), RAM privata | 9,8–10,2 MB | **10,4–10,6 MB** |

  Il ritorno dopo la chiusura è circa 0,3–0,6 MB sopra quello di partenza (e sopra il riposo puro di 7,8 MB, come già prima di F1: il pannello lascia in memoria ciò che Skia ha messo in cache).
- **Fotogrammi** (contati dai `RedrawRequested` di winit con una build usa e getta, mai committata): a riposo la pillola ridisegna solo per il battito di ciglia (ogni 4,6 s una chiusura di 110 ms, circa 12 fotogrammi: 182 in 70 s); con il pannello aperto fermo lo stesso battito degli occhi (circa 13 fotogrammi ogni 4,4 s). Con gli occhi che non battono (umore `sleep` o `happy`) i fotogrammi dopo l'avvio sono **0** sulla pillola e uno solo in 45 s a pannello aperto (oltre all'animazione d'apertura di 0,4 s).
- **Confronto visivo** con `tc-griglia-4`, `tc-griglia-aspetta`, `tc-pan-lavora` e `tc-stati-misto` (le foto dei crate di prova che compilano l'interfaccia del repo): composizione, gerarchia, luce dal basso, capsule e anello del fuoco coincidono. Differenze volute o note: «Consenti» è neutro (regola M3), i moduli sono quattro (niente Mail, Focus, Musica, Agenda), il meteo è testo («9°») e non l'icona della nuvola, gli occhi nelle foto sono bianchi.

### Fixed (Ridisegno F1, ondata finale dopo la revisione della fetta)
- **Movimento ridotto anche nella barra**: con «Mostra animazioni» spento la capsula della scheda attiva, le schede, le loro icone, il gruppo centrato e il divisore saltano al loro posto invece di scivolare per 200 ms (la dissolvenza di 160 ms del nome resta).
- **«+N in attesa ›» più in alto**: nella voce che ti aspetta sta sulla riga della capsula (entrambe centrate a 20 px, «+N» alto 24) e il suo bordo basso resta ad almeno 8 px dal «Nega» della prima riga della Coda che apre: un doppio clic sul bordo, con un po' di deriva, non preme quel «Nega» (prima la distanza era 2 px).
- **Voce con un solo pulsante delicato**: il posto di «Nega», che ha il fuoco iniziale, resta vuoto e il pulsante va sull'altro posto («Apri» se il testo non sta in una riga); anche il posto di «Nega» dice «Apri» per un pulsante delicato. Oggi `voce_permesso` non produce voci così: la regola non dipende più dai dati.
- **Test**: il riarmo di «Consenti» al fondo di un testo lungo ha un test che lo isola (tolto `consenti-compare` da `visto-fondo`, il test fallisce).
- **Memoria**: 10 aperture e chiusure di fila (exe release su istanza di prova, cartella finta, nessuna casa; apertura col secondo avvio, chiusura con Esc): RAM privata 15 s dopo ogni chiusura 10,22 · 10,25 · 10,40 · 10,34 · 10,40 · 10,42 · 10,27 · 10,22 · 10,45 · 10,48 MB, piatta (riposo all'avvio 7,86 MB; a pannello aperto 14,1–14,3 MB). Nessuna crescita da una chiusura all'altra.

### Changed (giro grafico dopo M3, 18–19/11)
- **Un clic, il dettaglio**: dalla pillola con l'attività di Claude (o una sua voce che non è un permesso) il pannello si apre sulla vista Claude. «N luci accese» nella Panoramica apre la Casa filtrata sulle luci accese («Luci accese (N)», un interruttore per luce, «Spegni tutte», «Tutta la casa»; a zero «Nessuna luce accesa»). La riga e il quadrante CASA si raggiungono con ↓ o Tab e per Narrator sono un pulsante solo, grande quanto la riga o la card.
- **Panoramica**: quadranti bordo a bordo con «In attesa» e a 8 px l'uno dall'altro; casa non configurata = invito «Collega la casa ›» (nel quadrante la casa verde acqua al posto dell'arco, una volta sola) che porta alle Impostazioni; con tre o più voci in attesa due card larghe e «+N»; l'arco dei limiti e della spesa si scalda alla soglia dell'utente; «Limite 7 giorni» e «Spesa API del mese» con lo stesso nome ovunque; nel primo minuto il quadrante della spesa dice «prima lettura fra poco». La rotella non cambia più layout sotto il menu «⋯» aperto.
- **Vista Claude**: le sessioni scorrono tutte (via «+N altre»), nell'ordine deciso all'apertura e fermo finché il pannello resta aperto; il filo di scorrimento sta fra le due colonne. Dopo un riavvio i limiti mostrano l'ultimo valore visto, attenuato, con **«dalle 20:29»** («da ieri alle…», «dal 17/11 alle…»), finché non arriva un dato nuovo, e spariscono quando si azzerano; lo stesso valore, attenuato, c'è anche nei quadranti dei limiti. «Contesto N%» si scalda oltre l'80 %; «ultimo terminale usato» è un'icona col suggerimento; senza spesa la nota della card sta al centro.
- **Viste PC e Casa**: un solo stile per le etichette, ritmo 4/8 nelle card, dettagli di RAM e disco interi.
- **Finestra di gestione**: barra di Windows (Snap, riduci, ridimensiona) col filo colorato del pannello; Spesa a gruppi («Per modello», «Ultimi giorni»…) con le strisce alterne che ripartono da ogni gruppo; numeri allineati a destra, un solo anello, il fuoco non sparisce dopo un aggiornamento.
- **Impostazioni**: impaginate con layout veri; anello del fuoco sui campi; sezione Claude in tre blocchi, «Togli» la chiave con conferma, la riga d'errore della soglia di spesa resta in vista; una chiave rifiutata non sposta più il fuoco su «Togli».
- **Parole e collegamenti**: «Casa» per l'utente, «ambiente» sparisce dai testi (dettagli nelle rifiniture grafiche più sotto); ogni collegamento con l'iniziale in un cerchio colorato.
- **Menu «⋯»**: scuro, in Slint, dentro il pannello (stesse voci e stesso ordine del menu dell'icona; frecce, lettere, Invio, Esc che chiude solo il menu, sottomenu «Schermo» con più monitor); funziona anche se il menu dell'icona nella barra non si è potuto creare.
- **Il nome della sessione, non solo la cartella**: due sessioni avviate nella stessa cartella non si chiamano più entrambe «bin». La barra di stato di Claude Code porta `session_name` (il nome di `--name` o `/rename`, altrimenti il titolo che Claude genera da sé; assente se non c'è): `isola-hook barra` lo inoltra come `titolo` (ripulito da controlli, direzione e zero-width, al più 60 caratteri con «…»; isola rifiuta un titolo troppo lungo o sporco come gli altri campi) e la sessione tiene l'ultimo non vuoto. Dove la sessione ha un nome si legge il titolo e, in mancanza, la cartella come prima: righe della vista Claude (il titolo in alto e «· cartella» attenuata nella seconda riga, senza accorciare altro; Narrator dice «titolo, cartella, stato»), voci «In attesa» e notifiche, voce del permesso, nome della pillola per Narrator. Restano fuori `cwd`, trascrizione e workspace.

### Fixed (M3, rifiniture della revisione finale)
- Impostazioni › Claude: la conferma di «Scollega» dice ciò che succede davvero (escono le voci di isola, hook e barra di stato; il resto di `settings.json` resta) e non parla più di un «codice segreto» che non esiste. Il sottotitolo e la nota dell'anteprima non promettono più cose non verificate («Claude Code rilegge da solo»): ora «le sessioni avviate dopo il collegamento lo useranno». Fuori da Windows il sottotitolo dice che il collegamento è solo per Windows.
- «Scollega» con `settings.json` illeggibile ferma comunque l'ascolto di isola e toglie «collegato», e dice che il file non è stato toccato e come rimediare.
- «Il collegamento va aggiornato» non si ripete a ogni avvio dopo «Ho capito» (chi ha tolto le voci di proposito): torna dopo un nuovo collega o scollega.
- Una decisione cliccata quando Claude Code ha già smesso di aspettare lo dice («La risposta è arrivata troppo tardi: rispondi nel terminale»); il tubo aspetta 2 s meno dell'aiutante, così il caso è raro.
- Il contatore diagnostico `claude_permessi` ora conta i permessi ricevuti. Il motivo di un «Nega» per Claude Code è «Negato da isola.».
- Il gancio sul primo piano cerca il terminale solo con Claude Code collegato, e ne ricontrolla l'eseguibile all'uso (un numero di finestra riusato non porta davanti un'altra finestra).
- Tolti il PID dell'aiutante (`Sessione.pid`, `Arrivo.pid_mittente`, nessuno lo leggeva: serve la catena dei processi), il conto condiviso fra test paralleli delle notifiche (ora per thread), la copia del `Riepilogo` per gli avvisi di spesa e gli `allow` di modulo in `notifiche.rs`. Commenti e README allineati al canale protetto.

### M3 — misure (18/11/2026, build release, Windows 11, 12 processori logici)
Condizioni: assistente release in **prova** (`ITSYHOME_ASSISTENTE_DIR`, `claude.collegato = true`, tubo `…-prova`), **senza connessione a una casa** (a differenza di M2): la prova non deve toccare né il Home Assistant vero né la configurazione e il portachiavi veri, e M3 non dipende da HA (hook e API Claude non lo usano). 60 s per stato con `tools/misura.ps1` (RAM = memoria privata, CPU = % dell'intera macchina come nello script). Le sessioni sono quelle finte di `sessioni_finte` (3 sessioni, 1 `PreToolUse` ogni 2 s a sessione, una barra ogni 5 s, `Stop` ogni 30 s).
- **Collegato, nessuna sessione** (thread fermo sul tubo): **5,0 MB**, CPU **0,00 %** (un solo picco di 0,12 % in 60 s).
- **3 sessioni finte per 60 s**: CPU media **0,02–0,04 %** della macchina (circa 0,4 % di un processore: dal registro diagnostico 235 ms di CPU nel primo minuto con 43 eventi e 24 barre, 203 ms nel successivo) contro il budget «≤ 1 %»; RAM **senza crescita** fra il secondo 10 e il 60 delle sessioni (5,9–6,1 MB; 5,0–5,2 MB nell'altra prova). Una prova su quattro ha invece mostrato +5 MB durante le sessioni (da 5 a 11 MB) con ritorno a 8 MB e poi stabile: non ripetuta nelle altre tre, causa non indagata (probabile prima comparsa della pillola in una finestra già pronta).
- **Dopo la fine delle sessioni**: tornata a riposo (5,1 MB, CPU 0,00 %, +0,0 MB) nelle tre prove su quattro; nella quarta 8,3 MB (+3,3 MB, oltre i +2 MB), poi stabile per 90 s.
- **Aiutante, 100 esecuzioni ciascuno** (esempio di prova `aiutante_prova`, lo stesso codice di `isola-hook.exe` ma con tubo e server indicati dall'ambiente, perché l'eseguibile vero non può raggiungere il tubo di prova per scelta; i tempi includono l'avvio del processo): `evento` (un `PreToolUse`) mediana **27,4 ms** (p95 29,0, max 30,1), `barra` mediana **27,3 ms** (p95 29,7, max 30,6), contro i budget 30 e 50 ms. Tutti gli eventi sono arrivati all'assistente (contatori 105 + 105 comprese 5 di riscaldamento).
- **Una lettura API con `finta_api`: NON misurata.** La lettura parte solo se la chiave sta nel portachiavi di prova (`isola:claude-admin-prova`) e scriverla da fuori è stato negato; non si è voluto simulare l'immissione nella finestra. Resta da fare (chiave finta salvata dalla sezione Claude di un'isola di prova, poi 150 s).
- `cargo test --workspace`: 810 test ok (4 ignorati, quelli che leggono il PC vero); `cargo clippy --workspace --all-targets -- -D warnings`: nessun avviso.

### Added (M3, Task 12: limiti e spesa nei quadranti, avvisi a soglia, spec M3 §6.5 e §6.7)
- **Tre nuove fonti per i quadranti della Panoramica** (solo quadranti, mai ai lati della pillola): **«Limite 5 ore»**, **«Limite settimanale»** (arco 0..100 %, con «si azzera alle 14:30») e **«Spesa API del mese»** (arco 0..soglia di spesa se c'è, altrimenti il massimo scritto nel campo «Massimo» della riga o 0..doppio del valore, come le unità ignote; valore breve «413 $», i centesimi stanno nella vista Claude). Leggono solo `limiti()` (già senza i limiti azzerati né non finiti) e l'ultima lettura riuscita delle API: senza dato il quadrante dice **«—»** e non ha arco, mai «0%» né «0 $» (PF9). Si riscrivono da soli quando arriva una barra o una lettura.
- **Avvisi di attenzione ai limiti**: alla soglia «Avvisa al … % del limite» (50..100, predefinita 80) la voce «Limite 5 ore all'82%» / «Limite settimanale al 61%» con «Si azzera alle 14:30.» e «Ho capito». **Una volta per finestra**: il ponte salva la fine della finestra in `limiti_avvisati` (anche dopo un riavvio la stessa finestra non riavvisa; una finestra nuova sì) e **toglie la voce quando la finestra si azzera**, con la sveglia singola dei limiti (che ora c'è anche a vista chiusa, ma solo finché c'è un limite valido: nessun timer ripetuto, nessun timer a riposo).
- **Avvisi di spesa**: con la «Soglia di spesa mensile ($)» scelta (vuota = nessun avviso) la voce di attenzione «Spesa API all'80% della soglia (250,00 $)» e «…al 100%…», **una volta per gradino e per mese UTC** (`spesa_avvisata`, i mesi vecchi si potano); e la voce informativa «Spesa API insolita ieri: 40,00 $» (ieri più di 3 volte la media dei 7 giorni prima), una volta al giorno (`insolita_avvisata`). Mai da dati assenti, falliti, senza data di aggiornamento o fermi al mese scorso. Cambiare la soglia a finestra aperta vale subito sui dati che ci sono.
- **Impostazioni › Claude › Avvisi**: i due campi con l'etichetta sempre visibile; un valore non valido (non un intero da 50 a 100, un importo non positivo) resta nel campo con la frase che dice cosa scrivere e **non si salva**; la soglia di spesa accetta virgola o punto. Con la soglia di spesa assente la riga del quadrante «Spesa API del mese» chiede il suo «Massimo».

### Fixed (M3, Task 11: il fuoco sopravvive agli aggiornamenti)
- Un aggiornamento dei dati a finestra aperta (la lettura arrivata dopo l'apertura, quella delle 3 ore) non toglie più il fuoco: i modelli delle schede e delle righe sono creati una volta e aggiornati sul posto (`set_row_data`, `push`, `remove`, solo per ciò che è cambiato), così la scheda o la riga col fuoco resta la stessa istanza e frecce ed Esc continuano a funzionare. Prima ogni `set_schede` ricreava tutto e il fuoco spariva.
- Fine, PagGiù e le altre frecce raggiungono anche una riga fuori vista (Slint crea solo le righe in vista: prima la si porta in vista, poi prende il fuoco appena nasce).
- L'avviso «Appena aggiornato» non sopravvive alla chiusura della finestra.

### Fixed (M3, Task 11: schede accessibili e rifiniture dalla revisione)
- **Narrator e tastiera nelle schede**: le schede stanno in un `tab-list` e ognuna è un `tab` col fuoco (Tab raggiunge solo quella scelta; ←/→/Home/Fine spostano insieme fuoco e scelta, con posizione e numero); la tabella è una `list` e ogni riga un `list-item` col fuoco la cui etichetta legge le celle con i nomi delle colonne («Nome: …, Stato: …»), ↑/↓/PagSu/PagGiù/Home/Fine passano da una riga all'altra e la riga si porta in vista. Provato con un test Slint nella finestra (ruoli, nomi, scelta che segue i tasti) e con l'albero UIA del processo vero.
- Nessuna cella con `sk-ant-` nemmeno con caratteri invisibili in mezzo (il controllo ora sta dopo `testo_pulito`, dentro `scheda_pulita`); tetti del protocollo: al più 8 schede e 500 caratteri per cella; tolta la proprietà `n` mai usata.
- «Aggiorna»: almeno 60 s fra due richieste dell'utente («Appena aggiornato: riprova fra N s»), e «Lettura in corso…» nella nota finché la lettura dura.
- Ogni processo della finestra ha un numero: una `chiusa` tardiva di un processo vecchio non segna chiusa la finestra nuova, e una lettura finita non riapre mai una finestra chiusa.
- Le chiamate del mese di una chiave si abbinano per id (non per nome); «Dettagli» ha il nome accessibile «Dettagli delle API»; la prova del figlio finto controlla ciò che il figlio riceve davvero.

### Added (M3, Task 11: dettaglio delle API Claude nella finestra di gestione, spec M3 §6.7 e §6.8)
- **«Dettagli»** nella vista Claude (nella card della spesa, solo con la chiave da amministratore): apre la finestra di gestione con cinque **schede native** in sola lettura, **Spesa** (mese, ieri, per modello, chiave e workspace, ultimi giorni), **Token** (ingresso, uscita, cache, chiamate del mese), **Chiavi** (nome, stato in italiano, creata, scade, ultime cifre del suggerimento, chiamate del mese), **Workspace e membri**, **Limiti**. Ogni scheda dice «Aggiornato al … · dati con 4–24 ore di ritardo» (o «Letto alle …»), oppure «Non disponibile con questa chiave»; mai «0 $» al posto di «nessun dato».
- **Mai il valore di una chiave**: dell'API si tiene solo la coda di `partial_key_hint` (api-claude: `InfoChiave` ha ora `creata` e `suggerimento`), e ogni cella che contenesse `sk-ant-` si sostituisce con «—» (provato).
- **Protocollo** (`gestione-protocollo`): `Messaggio::ApriSchede` e `SchedaDati`; la finestra ripassa ogni testo da `testo_pulito` e limita a 200 righe × 6 colonne per scheda. Con `ApriSchede` **nessun riquadro web** (`serve_il_web` è vero solo per `Apri`) e nessuna rete nella finestra.
- **Tastiera e Narrator**: schede `tab` (←/→/Home/Fine), Tab verso tabella e pulsanti, ↑/↓/PagSu/PagGiù nella tabella; nel pannello ↓ dall'ultima riga porta su «Dettagli» e Invio lo apre (anche da scollegato).
- **«Apri la Console»** apre la pagina giusta nel browser: l'assistente accetta dalla finestra solo gli id `console:spesa|uso|limiti|chiavi|chiavi-admin|membri|organizzazione` e `aggiorna`, ogni altro id si ignora; gli indirizzi sono fissi (`platform.claude.com/cost`, `/usage`, `/usage/limits`, `/settings/keys`, `/settings/admin-keys`, `/settings/members`, `/settings/organization`, tutti verificati sul sito il 18/11/2026). In prova la Console non si apre: l'indirizzo va nel log. «Aggiorna» fa una lettura per la strada di sempre (un solo giro alla volta); a finestra aperta i dati nuovi sostituiscono i vecchi senza cambiare scheda. «Togli» la chiave chiude la finestra.
- **Misura dal vivo**: finestra aperta e chiusa 10 volte con Esc, processo sempre raccolto, RAM dell'assistente +0,16 MB (working set).

### Added (M3, Task 10: il clic su una sessione porta davanti il suo terminale, spec M3 §6.6 e §10)
- **«Apri il terminale»**: le voci «Claude ti aspetta» e «Claude ha finito» hanno il pulsante (le voci dei permessi restano con Nega e Consenti, l'errore senza), e il clic o l'Invio su una riga della vista Claude fa lo stesso (`apri-sessione`). Si porta davanti la finestra del terminale della sessione; se era ridotta a icona la si ripristina. Solo subito dopo il gesto dell'utente (regole del primo piano di Windows), mai da timer o da eventi.
- **Come si trova la finestra (PF19, niente tabella TCP)**: il tubo prende la catena dei processi dell'aiutante al primo evento di ogni sessione (aiutante → `claude.exe` → shell → terminale); il ponte la tiene per sessione (16 al più, tolta con la sessione). Al clic si rilegge l'elenco dei processi, si scartano gli anelli il cui PID ha cambiato eseguibile (PID riusato da un altro programma), si risale con i genitori e si sceglie la prima finestra visibile e senza proprietario di un terminale (Windows Terminal, console classica via il suo `conhost.exe`, PowerShell, cmd, WezTerm, Alacritty, VS Code; con Windows Terminal e VS Code si porta davanti la finestra, mai la scheda). Mai una finestra di un programma che non è un terminale (la barra di Explorer non è mai la meta).
- **Ripiego**: senza catena o senza finestra si porta davanti l'ultima finestra di un terminale che è stata in primo piano (la ricorda il gancio del primo piano); la riga della vista dice «ultimo terminale usato» ogni volta che il ripiego può scattare: se la catena non porta a nessuna finestra (calcolato quando la sessione compare e quando si apre la vista, non su un timer) o se l'ultimo clic ha ripiegato. Il PID riusato si scarta anche se è lo stesso programma: l'anello deve avere ancora come genitore quello salvato sopra di lui. Un clic che non trova nulla (o che Windows non porta davanti) mette la voce informativa «Non trovo il terminale di questa sessione», che sparisce da sola dopo 10 s. Con Windows Terminal e VS Code si porta davanti la **finestra, non la scheda** (lo dice la descrizione della riga, solo per loro, a chi usa Narrator). Fuori da Windows il clic apre la vista Claude.
- **Prove**: catena e scelta della finestra sono funzioni pure provate (cicli, 16 passi, PID riusato, console classica); nessun test porta davanti finestre vere. La prova dal vivo con Paolo resta da fare.

### Added (M3, Task 9: vista Claude nel pannello e icona nel dock, spec M3 §6.4, §6.5, §6.7, §6.8)
- **Icona «Claude» nel dock** fra Casa e PC (ordine: Panoramica, Casa, Claude, PC, Collegamenti) e **vista Claude** nel pannello 800 × 300: a sinistra al più **tre sessioni** nell'ordine di urgenza (cartella, stato col colore del suo livello, strumento, durata, contesto %; «+N altre» se ce ne sono di più; «Claude» se la cartella manca), a destra gli anelli **«Limite 5 ore»** e **«Limite settimanale»** con «si azzera alle 14:30» (domani, o il giorno della settimana per le finestre lunghe) e sotto la **spesa API del mese** con «aggiornato alle 14:02» (ora locale).
- **Mai un numero inventato (PF9)**: gli anelli vengono solo da `limiti()` (già senza i limiti azzerati); un limite assente o scaduto non ha l'anello (l'altro posto lo dice) e, senza nessuno dei due, la card dice «Limiti non disponibili». La spesa senza la chiave dice «Aggiungi una chiave da amministratore nelle Impostazioni»; con la chiave ma senza dato «nessun dato».
- **PF18 (api-claude)**: un mese ancora senza righe di costo vale **«0,00 $»** solo se i dati (`data_refreshed_at`) arrivano almeno alla fine del periodo letto; se sono più vecchi o senza data resta «nessun dato».
- **Stati vuoti**: scollegato «Collega Claude Code dalle Impostazioni» con il pulsante «Impostazioni» sotto il messaggio (apre le Impostazioni sulla scheda Claude); collegato senza sessioni «Nessuna sessione aperta». Tastiera: ↑/↓ fra le righe, Invio = `apri-sessione(id)` (l'impronta della sessione, mai il `session_id`; la usa il Task 10). Narrator: righe come pulsanti («cartella, stato»), i due anelli come indicatori con valore e descrizione, la spesa come testo (etichetta, valore e nota); la durata, che cambia ogni secondo, non si legge.
- **Timer (PF10, seconda metà)**: la condizione unica del timer da un secondo (`attivita_serve`) ora è «almeno una sessione lavora **e** (la pillola si vede e non dorme **oppure** la vista Claude è aperta)»: a pannello aperto la pillola è nascosta e la durata scorre lo stesso. La vista non ha timer suoi e a vista chiusa non calcola né scrive nulla; ogni proprietà si riscrive solo se cambia.

### Added (M3, Task 8: API Claude in sola lettura, spec M3 §5 e §6.7)
- **Nuovo crate `api-claude`**: consumi e dati dell'organizzazione Anthropic con una **chiave da amministratore**, che non ha permessi ridotti: la difesa è un **elenco chiuso di 7 percorsi, tutti `GET`** (`/v1/organizations/{me, analytics/cost, analytics/usage, api_keys, workspaces, users, rate_limits}`). Un solo metodo di rete (`leggi`), HTTPS obbligatorio verso `https://api.anthropic.com` con certificato verificato (rustls + ring, radici del sistema, lo stesso `rustls 0.23` di `casa-core`; `ureq 3` senza `webpki-roots` né gzip, nessun reindirizzamento seguito, nessun proxy dall'ambiente, timeout 20 s), un test che impedisce chiamate che scrivono nel sorgente del client. Intestazioni `x-api-key`, `anthropic-version: 2026-10-21`, `anthropic-beta: analytics-usage-cost-2026-08-24` solo su `/analytics/*`.
- **Riepilogo** (`aggiorna`): spesa del mese e di ieri (per modello, chiave, workspace), 8 giorni, token (ingresso, uscita, cache), chiamate del mese anche per chiave, chiavi (nome, stato, scadenza: mai il valore), workspace, membri, limiti di velocità, «aggiornato a» (`data_refreshed_at`). Mese UTC dal primo all'ora intera; pagine seguite al massimo 10 volte (oltre, la scheda è «non disponibile»: un totale parziale non si spaccia per completo); una lettura fallita mette in `non_disponibili` solo la sua scheda; chiave rifiutata (401) o 429 fermano il giro; risultato vuoto o senza `data_refreshed_at` = nessun dato, mai «0 $». `spesa_insolita`: ieri > 3× la media dei 7 giorni prima, solo con dati completi.
- **Impostazioni › Claude › «Chiave da amministratore»**: campo mascherato, «Salva» (la chiave va solo nel portachiavi `isola:claude-admin`; poi «•••• 1a2b» e «Togli»), «Prova» (una sola lettura dell'organizzazione: «Chiave valida» o la frase dell'errore, senza mai la chiave), guida «Crea una chiave da amministratore con scadenza nella Console (Impostazioni › Admin keys)». Riga sotto: ora dell'ultima lettura, spesa del mese, ora dei dati, schede non disponibili.
- **Letture**: thread `api-claude` per ogni lettura (client creato e lasciato andare), prima dopo 60 s dall'avvio e poi ogni 3 ore con un solo `slint::Timer` che esiste **solo con la chiave** (si ferma con «Togli»); il riepilogo torna con `invoke_from_event_loop`. `diagnosi.log`: solo i contatori `api_letture` e `api_errori`.
- **Prova**: con `ITSYHOME_ASSISTENTE_DIR`, `ITSYHOME_API_CLAUDE_BASE` (solo `http://127.0.0.1:<porta>`) e `ITSYHOME_API_CLAUDE_CHIAVE` (solo `finta-…`) l'assistente legge dal server finto `cargo run -p api-claude --example finta_api`; la chiave vera non può finirci e in prova il portachiavi usa un'altra voce (`isola:claude-admin-prova`).

### Fixed (rifiniture del terminale della sessione, dalla seconda revisione del Task 10)
- Un rifiuto di `SetForegroundWindow` è un esito a parte («Windows non ha permesso di portare davanti il terminale: cliccalo nella barra») e **non** segna la sessione come «ultimo terminale usato».
- La rilettura del ripiego all'apertura della vista non prende l'istantanea se non ci sono catene salvate.
- Fuori da Windows un pulsante di una voce con il pannello chiuso lo apre sulla vista Claude (non sull'ultima vista); la CI di Linux e macOS non vede più `voce_terminale_mancante` e l'import `PULSANTE_HO_CAPITO` come codice morto (definizioni sotto `cfg(any(windows, test))`, come il loro unico chiamante).

### Changed (rifiniture grafiche dal controllo del 18/11)
- Il «⋯» (che il carattere non ha e appariva come un quadratino) è scritto «…» nei testi delle scorciatoie e di Impostazioni › Sistema.
- Voce espansa: il titolo si legge intero (a capo, fino a 4 righe) e riga sotto e testo scendono. Sotto le voci c'è il nome per l'utente della fonte («PC», «Casa», «Claude Code», «Prova») invece dell'identificativo interno.
- Vista PC: i dettagli di RAM e disco sono più corti e non si tagliano («10,1 / 15,7 GB»).
- Impostazioni: via il titolo ripetuto in testa (si recuperano 46 px); stati vuoti di Ambienti e Collegamenti col pulsante sotto il messaggio, al centro, come nel pannello; istruzioni di Dispositivi solo con l'elenco pieno; note «solo su Windows» solo fuori da Windows; «Avvisa quando le cuffie…» nella sezione «AVVISI»; il selettore ha una freccia in giù; «Aggiungi un ambiente» / «Aggiungi un collegamento» ovunque.
- Parole: per l'utente la casa è sempre «Casa»; «Casa (Home Assistant)» solo in Impostazioni › Casa, dove si sceglie a cosa collegarsi; «Aggiungi una casa» al posto di «Aggiungi un ambiente»; «ambiente» sparisce dai testi (gli identificativi e il file di configurazione restano). Collegamenti: ognuno ha l'iniziale del nome in un cerchio colorato (colore stabile dal nome, nessuna rete); via «COLLEGAMENTI» e «Si aprono nel browser» dalla testata del pannello.
- «Nega» e «Consenti» sono entrambi neutri (nessuna tinta d'azione), nel pannello e nella pillola; fuoco iniziale su «Nega», Ctrl+Invio e armamento di 600 ms invariati.

### Changed (M3, Task 7B: «Collega» sul canale protetto, spec M3 §10)
- **«Collega Claude Code» ora scrive davvero** (e solo dopo il «sì»): voci `command` in **forma exec** (`"command": "<percorso>\isola-hook.exe"`, `"args": ["evento"]`, nessuna shell) per `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `Notification`, `Stop`, `StopFailure`, `SessionEnd`; `"async": true` esattamente su `UserPromptSubmit`, `Notification`, `Stop`, `StopFailure`, `SessionEnd`; `PreToolUse` sincrono (`timeout` 5) e `PermissionRequest` sincrono (`timeout` 120). Mai `onFailure`, `headers`, `allowedEnvVars`, URL. Barra di stato: `statusLine` = `<percorso con / (o corto 8.3)>/isola-hook.exe barra`; se ce n'è già una di altri non si tocca. Il comando punta all'`isola-hook.exe` accanto a `assistente.exe` (senza l'aiutante non si scrive); in prova il file è `<cartella di prova>\claude-finto\settings.json`.
- **Migrazione**: le vecchie voci HTTP del Task 7 (`http://127.0.0.1:…/isola/v…`) e la barra `--barra-stato` si riconoscono, l'anteprima le elenca («verrà tolta/sostituita») e la scrittura le toglie; «Scollega» le toglie sempre. L'anteprima mostra le voci di isola **come resteranno** e ciò che si sostituisce.
- **Conferma a prova di cambiamenti**: al «Scrivi in settings.json» si **rilegge** il file e si rifà l'unione; se la modifica non è identica a quella letta (voci vecchie comparse, barra di altri, voci tolte…) **non si scrive** e si mostra la nuova anteprima con l'avviso «settings.json è cambiato dopo l'anteprima». Un cambio altrui che non tocca le nostre voci (il modello, ad esempio) si conserva. Senza un'anteprima letta non si scrive.
- **Riserva e scrittura**: `settings.json.isola-bak` si fa solo se il file non ha niente di isola (è il file di prima, e le scritture successive non la sovrascrivono; una riserva presente in sola lettura non ferma la scrittura); il file temporaneo si sincronizza su disco (`sync_all`) prima del `rename`. `togli` non cancella `hooks`, eventi o gruppi che erano già vuoti prima e non riordina nulla. Limite noto: un elenco vuoto di un evento a cui isola si iscrive (o un `hooks: {}`) non si distingue da «assente» dopo collega+scollega e sparisce (per Claude Code è la stessa cosa).
- **All'avvio** una voce *informazione* «Il collegamento con Claude Code va aggiornato» (mai una scrittura) se manca anche una sola delle 7 voci, ha un altro percorso o forma, ci sono voci HTTP vecchie, la barra è cambiata o non c'è più niente di nostro.
- **Interfaccia** (Impostazioni › Claude): via «Rigenera il codice», il riquadro «Copia il codice» e la nota sui terminali da riaprire; nuova nota «Con isola chiusa Claude Code prosegue da solo: i permessi li decidi nel terminale.»; stato «Collegato» (senza porta) e, fuori da Windows, «Disponibile solo su Windows» senza pulsanti.
- **Tolto**: `ambiente_utente` (variabile d'ambiente dell'utente, registro), la voce `isola:claude-hook` del portachiavi, `ConfigClaude.porta` (le vecchie configurazioni con la chiave `porta` si leggono lo stesso), `fuori_lista`/`allowedHttpHookUrls`, `nessun_codice_in_settings`.
- **Dalla revisione del 7A**: (A) l'avviso «canale perso» di un tubo già sostituito o chiuso si ignora (numero di generazione del tubo): non ferma più quello nuovo; (B) il percorso di arresto dopo 50 errori di fila nel creare istanze ha un test vero (creatore di istanze iniettabile) e il test «clienti che spariscono» ha il nome onesto; (C) l'aiutante controlla anche l'**etichetta d'integrità** del tubo (`GetSecurityInfo` + `LABEL_SECURITY_INFORMATION`): assente (medio implicito) o ≥ media sì, sotto il medio o illeggibile no, provata su tubi veri; (D) i test binari dell'aiutante non vanno più in panico se l'esempio non è compilato (saltano con un messaggio).
- **Correzioni dalla CI**: il test di proprietario e DACL del tubo rilegge il descrittore per struttura (EqualSid, DACL protetta, un ACE, accesso completo) invece dell'SDDL testuale, che per l'Administrator integrato usa alias (`LA`); fuori da Windows `Servitore` ha un `Drop` vuoto, così `drop(servitore)` non è un `drop` di un tipo senza `Drop` (clippy `-D warnings`).

### Changed (M3, Task 7A: canale protetto al posto degli hook HTTP, spec M3 §10)
- **Sicurezza**: gli hook HTTP potevano essere risposti da qualunque processo locale che prendesse la porta con isola chiusa (approvare strumenti, iniettare istruzioni). Il trasporto ora è un **tubo con nome di Windows** `\\.\pipe\isola-claude-<SID>` riservato all'utente corrente (SDDL `O:<SID>D:P(A;;GA;;;<SID>)`, `FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, modalità messaggio, al più 18 istanze per 16 connessioni servite), raggiunto dal nuovo aiutante **`isola-hook.exe`** (binario di `claude-hook`, 320 KB, senza Slint né `casa-core`) che Claude Code lancerà come hook `command` (le voci in `settings.json` le rifà il Task 7B).
- **L'aiutante verifica il server prima di scrivere un solo byte**: il **proprietario dell'oggetto tubo** (`GetSecurityInfo`, fissato dal kernel; un altro utente non può darsi il nostro SID) è l'utente corrente → PID dal tubo → processo dello stesso utente (SID del token) e con **integrità almeno media** → immagine `QueryFullProcessImageNameW` = `assistente.exe` nella sua cartella (percorsi canonici, senza maiuscole) → tubo ancora collegato dopo la lettura (riduce, non esclude, un PID riusato: per questo il PID non è mai l'unica prova). Apre il tubo con `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION` (il server non può impersonarlo). La destinazione non si cambia con variabili né argomenti. Ogni errore: uscita 0, niente su stdout né su stderr.
- **Isola verifica il cliente prima di leggere**: stesso controllo, l'eseguibile atteso è `isola-hook.exe` accanto all'assistente (in prova, solo con `ITSYHOME_ASSISTENTE_DIR`, il percorso di `ITSYHOME_AIUTANTE_ATTESO`). Poi un messaggio di al più 64 KB entro 2 s, JSON di `Richiesta` riconvalidato (campi estranei, sessione, lunghezze, cartella senza separatori, riassunto solo nei permessi); altrimenti chiuso e contato come rifiutato. Restano: 16 connessioni (la 17ª si chiude e si conta), sequenza all'accettazione, PID dell'aiutante alla prima richiesta di una sessione, attesa dei permessi (115 s, abbandono se l'aiutante chiude, controllo ogni 250 ms). Allo spegnimento le attese si interrompono subito e tutti i thread si raccolgono: il nome del tubo si riprende al volo.
- **Minimizzazione nell'aiutante**: legge al più 256 KB di stdin e manda solo `RecordEvento` (i campi di `EventoHook`: sessione, tipo, ultimo nome della cartella, strumento, notifica, errore, riassunto del permesso). **Stampa solo** la decisione di un `PermissionRequest` ricevuta da isola verificata (`messaggio::decisione_json`, l'unico testo possibile): per ogni altro evento nessun output (gli hook `async` passano a Claude `additionalContext`/`systemMessage`: per questo non si stampa mai altro).
- `isola-hook barra` sostituisce `assistente --barra-stato` (il ramo in `main.rs` è tolto): riga stampata prima, poi `DatiBarra` per il tubo, 40 ms in tutto.
- Tolti dal percorso di avvio: il codice segreto (`CodiceHook`), la lettura del portachiavi e della variabile `ISOLA_HOOK_TOKEN`, la porta della configurazione, l'ascoltatore TCP/HTTP (`ascoltatore.rs`, `codice.rs`, `mittente.rs`, esempio `prova_permesso`); **«Scrivi in settings.json» è bloccato fino al Task 7B**: risponde «Collegamento in aggiornamento: disponibile nella prossima versione» e non scrive nulla (le voci che sapeva scrivere erano gli hook HTTP e la barra `--barra-stato`); l'anteprima resta in sola lettura. «Rigenera il codice» non ha più effetto (il pulsante lo toglie il Task 7B), «Scollega» toglie ancora le voci, il codice e la variabile rimasti dal Task 7. `PortaOccupata` → `CanaleOccupato`: voce *attenzione* «Collegamento con Claude Code non disponibile» · «Il canale è occupato da un altro programma.». L'esempio `sessioni_finte` parla per il tubo (`--assistente <percorso>`).
- Misura (build release, Windows, 50 lanci per voce, creazione del processo compresa, con isola di prova accesa e `ITSYHOME_DIAGNOSI=1`): `isola-hook evento` con un `PreToolUse` finto **mediana 27,0 ms** (min 26,4, max 36,2; il registro conta `claude_eventi=50`, `claude_rifiutate=0`); `isola-hook barra` **mediana 28,1 ms** (min 27,5, max 33,9; `claude_barre=50`). Riferimento sulla stessa macchina: `hostname.exe` 32,2 ms. Entrambe sotto gli obiettivi (30 e 50 ms). Prova a rovescio dal vivo: lo stesso aiutante copiato in un'altra cartella (senza `assistente.exe` accanto) non consegna nulla; l'esempio `sessioni_finte` (cliente non atteso) è rifiutato da isola.

- Revisione (giro 1): stessi controlli d'utente e integrità anche sul cliente; un'istanza in ascolto andata in errore si scollega e si riusa (il nome non resta libero); dopo 50 errori di fila nel creare istanze il servitore smette e compare la voce «Il canale è occupato da un altro programma.» (`Gestore::canale_perso`) invece di riprovare in silenzio. Per il Task 10 `Arrivo::catena_mittente` porta aiutante → `claude.exe` → terminale (PID e nomi, al più 6) presa mentre l'aiutante è collegato. In prova (`ITSYHOME_ASSISTENTE_DIR`) isola serve `isola-claude-<SID>-prova`: un'isola di prova e quella vera non si contendono il nome; l'aiutante vero usa sempre il nome vero, per le prove c'è l'esempio `aiutante_prova` (stesso corpo, destinazione dall'ambiente) e `sessioni_finte` parla col nome di prova. I test dell'eseguibile non aprono mai il tubo vero. CI: due test resi corretti fuori da Windows (file system sensibile alle maiuscole, tempo della barra senza tubo).

### Added (M3, Task 7: collegamento sicuro a Claude Code)
- *Sostituito dai Task 7A/7B: gli hook `http`, la porta, il codice segreto, la variabile d'ambiente, «Collegato · porta N» e «Rigenera il codice» qui sotto non esistono più; restano l'anteprima, la copia di riserva, la scrittura atomica e «Scollega».*
- Impostazioni › **Claude** (sezione 4; «Sistema» passa alla 5 e «Informazioni» alla 6): stato («Collegato · porta N», «Non collegato», «Da aggiornare»), «Collega Claude Code» → anteprima in un riquadro mono scorrevole con **solo le voci che si aggiungono** a `settings.json`, il file che si scriverebbe, gli avvisi (barra di stato già usata, `allowedHttpHookUrls`/`httpHookAllowedEnvVars` senza isola, `disableAllHooks` attivo) e «Scrivi in settings.json» / «Annulla». Il fuoco parte da «Annulla»; Esc annulla; **niente si scrive prima di «Scrivi»**. «Rigenera il codice» e «Scollega» (con conferma) quando è collegato. Nota fissa: con isola chiusa Claude Code segnala «hook failed» a fine risposta, è innocuo.
- Voci installate: `http` verso `127.0.0.1:<porta>/isola/v1/evento` su `UserPromptSubmit`, `PreToolUse`, `PermissionRequest` (timeout 120), `Notification`, `Stop`, `StopFailure`, `SessionEnd` (timeout 5), senza `matcher` e senza `onFailure`; nessun `SessionStart` né `PostToolUse`. **Il codice non entra mai nel file**: l'header è `Bearer $ISOLA_HOOK_TOKEN` con `allowedEnvVars`; il codice sta nel portachiavi e nella variabile d'ambiente dell'utente (`HKCU\Environment`, solo quel ramo; avviso `WM_SETTINGCHANGE` su un thread a parte). I terminali aperti prima vanno chiusi e riaperti. Fuori da Windows la variabile la imposta l'utente («Imposta ISOLA_HOOK_TOKEN nella tua shell», «Copia il codice»).
- Il file si cambia con garanzie: `settings.json` illeggibile (commenti, virgole finali, non un oggetto, `hooks` malformato) non si riscrive mai; ordine e contenuto altrui conservati (`serde_json` con `preserve_order`); copia di riserva `settings.json.isola-bak`, scrittura su `.tmp` e `rename`; `unisci` due volte = una, `togli(unisci(v)) == v`. La barra di stato si configura solo se manca o è nostra (percorso dell'exe con `/`; con spazi o caratteri della shell, il percorso corto 8.3, altrimenti nessuna barra). La versione delle voci si legge dal percorso `/isola/v1/…` (una sola fonte).
- All'avvio, collegato ma con `settings.json` che non dice più porta, versione o comando della barra: voce informativa «Il collegamento con Claude Code va aggiornato» col pulsante «Impostazioni» (apre la sezione Claude). **Mai scritture da sole.** In prova (`ITSYHOME_ASSISTENTE_DIR`) il file è `<cartella>\claude-finto\settings.json`, la variabile sta in `HKCU\Software\ItsyHomeProva\Environment` e il portachiavi vero non si tocca. Rigenerare il codice riavvia l'ascolto (i permessi in attesa si chiudono come già previsto).
- Prova dal vivo (modalità di prova, file e chiave di registro finti): anteprima senza scritture, collega, riserva identica all'originale, nessun `.tmp`, codice assente dal file, ascolto sulla porta, codice nuovo accettato e vecchio rifiutato (401) dopo «Rigenera», «Scollega» riporta il file al contenuto di partenza e toglie variabile e ascolto. Trovato e corretto dalla prova: togliere la variabile apriva la chiave in sola lettura («Accesso negato»).

### Added (M3, Task 6: barra di stato e limiti dell'abbonamento)
- *Sostituito dai Task 7A/7B: la barra di stato è `isola-hook.exe barra` (dati per il tubo), non più `assistente --barra-stato <porta>` col codice; la logica dei limiti qui sotto vale.*
- `assistente --barra-stato <porta>` è il comando della barra di stato di Claude Code: gira prima di istanza unica, Slint, configurazione e registro; legge al più 64 KB di stdin, stampa «Opus 5.5 · contesto 42% · 5h 24% (↻ 14:30) · 7g 61%» (parti mancanti omesse, limite già azzerato nascosto, senza nulla «Claude») e poi, se `ISOLA_HOOK_TOKEN` c'è, inoltra all'assistente solo `DatiBarra` (sessione, modello, contesto, due limiti): mai cartella, trascrizione, workspace né il codice nella riga. Connessione con scadenza 30 ms, 40 ms in tutto.
- L'assistente riconvalida il corpo sul thread della connessione (stesse regole di `session_id`, percentuali 0..=100, nome del modello ripulito) e tiene i limiti dell'account una finestra alla volta: una barra senza `rate_limits` non cancella quelli noti; un limite scaduto vale come assente (`Ponte::limiti`), senza abbonamento niente anelli. Il contesto è per sessione, al più 16, tolto con la sessione.
- Misura (build release, Windows, 50 esecuzioni, stdin da file di prova con valori finti, via `cmd /c`): mediana 152 ms con porta 1 senza codice, 200 ms con codice (la connessione scade a 30 ms), 160 ms contro un ascoltatore proprio; lo stesso giro con `hostname.exe` dà 75 ms. Il tempo è quasi tutto l'avvio dell'eseguibile (25 MB), non la logica: **la mediana NON rientra nei 50 ms previsti**, Paolo ha accettato la misura (18/11), nessun binario separato.

### Added (M3, Task 5: permessi di Claude Code dalla pillola)
- Un permesso chiesto da Claude Code diventa una voce *bloccante* «Claude chiede: <strumento> <riassunto>» (riassunto ripulito da caratteri invisibili e di direzione, titolo al più 60 caratteri, dettaglio intero con la sessione) con «Nega» e «Consenti». «Consenti» è un pulsante delicato: nel pannello solo Ctrl+Invio o clic, dalla pillola solo se il testo si legge tutto (altrimenti «Apri»). Nessun «Consenti sempre», nessun `updatedPermissions` né `updatedInput`.
- La decisione parte solo da un gesto sulla voce e va solo alla sessione che ha chiesto; un clic tardivo non fa nulla. Il permesso si chiude da sé (senza decisione, voce tolta) quando la stessa sessione manda `PreToolUse`, `UserPromptSubmit`, `Stop`, `StopFailure` o `SessionEnd` (risposta data nel terminale), o quando l'ascoltatore lo abbandona; non con la `Notification` `permission_prompt`, che Claude manda a permesso aperto. Al più 16 permessi in attesa: il 17° ripiega sul terminale. Con un permesso aperto la sessione non mostra anche «Claude ti aspetta».
- Fra più bloccanti la pillola chiama per il più vecchio e aggiunge « +N» (fuori dal testo tagliato, letto anche da Narrator); il pannello resta nell'ordine solito. `PERMESSI_RISOLVIBILI` (esito dello spike) seleziona il ramo B, solo «Apri il terminale».

### Added (M3, Task 4: sessioni di Claude Code nella pillola)
- *Nota: «porta e codice» e il codice nel portachiavi qui sotto sono stati sostituiti dai Task 7A/7B (tubo con nome, nessun codice).*
- Se Claude Code è collegato (`claude.collegato` nella configurazione, con porta e codice) l'assistente ascolta gli hook all'avvio; il codice sta nel portachiavi (`isola:claude-hook`), in prova nella variabile `ISOLA_HOOK_TOKEN` del processo. Porta occupata: voce *attenzione* «Collegamento con Claude Code non disponibile» con «Impostazioni».
- Voci «In attesa» per sessione, con id che non contengono il `session_id` (impronta FNV-1a): «Claude ti aspetta · <cartella>» e «Claude si è fermato · <cartella>» (*attenzione*, errore dell'API in italiano), «Claude ha finito · <cartella>» (*informazione*, sparisce chiudendo il pannello). Una sessione sparisce con `SessionEnd`, dopo 30 minuti di silenzio (2 ore se aspetta) o quando se ne superano 16.
- Occhi della pillola del colore dello stato più urgente (attenzione o informazione). Pillola media «attività dal vivo» «Claude · Modifica · 3:12» (o «Claude ×2 · 3:12»), larga 210 px, a riposo e col puntino; i lati si nascondono. Per Narrator: «ItsyHome, Claude, Modifica, 3 minuti».
- Nessun timer a riposo: una sveglia singola alla prossima scadenza di una sessione; il timer da un secondo del testo di attività gira solo con almeno una sessione che lavora, la pillola visibile e l'umore diverso da «dorme».
- `casa-core`: `Config::claude` (`ConfigClaude`: collegamento, porta, soglie, avvisi già dati); un file di M2 si legge scollegato. Esempio `sessioni_finte` di `claude-hook` per prove e misure.

### Added (M3, Task 3: eventi e stato delle sessioni)
- `claude-hook`: lettura pura degli eventi degli hook (`evento_da_json`) con i soli campi `session_id` (1..=128 caratteri `[A-Za-z0-9_-]`, stesso controllo dell'ascoltatore), `hook_event_name`, `cwd` (ridotto all'ultimo nome, 60 caratteri), `tool_name` (40), `notification_type`, `error`; mai prompt, trascrizione o risposta. `tool_input` si legge solo per `PermissionRequest` e solo per riassumere (`command` per `Bash` e `PowerShell`; `file_path` per `Edit`, `MultiEdit`, `Write`, `Read`, `NotebookEdit`; `url`; `query`), al più 2000 caratteri.
- `Sessioni`: macchina a stati per sessione (Lavora, TiAspetta, Fatto, Errore) con ora passata dal chiamante; elenco per urgenza, scadenza dopo 30 minuti di silenzio (2 ore per chi aspetta una risposta), al più 16 sessioni (si toglie la meno recente fra quelle che non aspettano; `ricevi` restituisce anche il `Tolta` relativo). Una `Notification` `permission_prompt` dopo una `PermissionRequest` lascia la sessione in TiAspetta.

### Added (M3, Task 2: ascoltatore locale blindato)
- *Sostituito dai Task 7A/7B: l'ascoltatore HTTP (e il PID dalla tabella TCP) è stato tolto; ora un tubo con nome verificato nei due sensi (`claude-hook::tubo`), con la stessa difesa a limiti (16 connessioni, 2 s di lettura, 64 KB) e la catena dei processi per il «porta davanti».*
- `claude-hook`: ascoltatore HTTP degli hook di Claude Code, solo su 127.0.0.1 e su una porta precisa (porta occupata = errore esplicito, mai un'altra porta). Codice segreto di 32 byte (`Authorization: Bearer`, confronto a tempo costante, `Debug` senza il codice). Rifiuti prima di leggere il corpo, nell'ordine: oltre 16 connessioni `503`; intestazioni oltre 8 KB o 32 righe `431`; lettura oltre 2 s chiusa; metodo diverso da `POST` `405` (nessuna intestazione CORS); `Origin` `403`; `Host` diverso da `127.0.0.1:<porta>` `403`; `Content-Type` diverso da `application/json` `415`; codice errato `401`; `Transfer-Encoding` o `Content-Length` assente `411`; corpo oltre 256 KB letto e scartato (fino a 1 MiB) poi `413`; percorso sconosciuto `404`; JSON illeggibile `400`. Ogni rifiuto è contato (`Ascoltatore::rifiutate`) e non arriva mai al gestore.
- Eventi: risposta `200 {}` e solo dopo il gestore. `PermissionRequest`: attesa della decisione (115 s al massimo) con controllo ogni 250 ms che il mittente non abbia chiuso; risposta `allow` o `deny` nel formato di Claude Code, `{}` se scaduta o senza decisione.
- Il PID del mittente (tabella TCP di Windows, spostata dall'esempio nella libreria) si cerca una sola volta per `session_id` nuovo (ricordati al più 16), prima di rispondere.
- `casa-core`: contatori diagnostici `claude_eventi`, `claude_rifiutate`, `claude_permessi`, `claude_barre`.

### Added (M3, Task 1: spike del permesso con hook HTTP)
- *Sostituito dai Task 7A/7B: lo spike usava un hook HTTP; il prodotto usa hook `command` con l'aiutante (spec §10). L'esito dello spike (la risposta di `PermissionRequest` decide davvero) vale ancora.*
- Nuovo crate `claude-hook` (per ora solo i percorsi `/isola/v1/evento` e `/isola/v1/barra`) e l'esempio `prova_permesso`, non codice di prodotto: ascolta solo su 127.0.0.1, scrive `impostazioni-prova.json` in una cartella temporanea (mai `~/.claude/settings.json`) con hook `http` per `PermissionRequest` (120 s), `PreToolUse`, `Notification` e `Stop` (5 s), il codice usa-e-getta solo nella variabile `ISOLA_HOOK_TOKEN`. Per ogni richiesta stampa evento, strumento, nomi delle intestazioni, `Origin`, porta e PID del mittente (Windows: tabella TCP) con la catena dei genitori; mai `tool_input`, `prompt`, `cwd`. Sui permessi chiede in console consenti, nega, aspetta o chiudi, e controlla ogni 250 ms se il terminale ha chiuso la connessione. Serve a chiudere P1-P8 della spec M3 §6.3 con Paolo prima del Task 5.

### Fixed (M2, giro finale della revisione)
- Solo Windows: il muto dall'altoparlante della vista PC (clic, Invio, Spazio o Narrator) si vede subito (icona e «Audio disattivato») e si toglie di nuovo con l'altoparlante, alzando il cursore o con la rotella in su. Prima l'eco di Windows del nostro comando veniva scartata e lo stato mostrato restava «non muto»: il PC restava muto senza modo di tornare indietro dal pannello. Il confronto del contesto in `OnNotify` ora ha un test.
- `casa-core`: il cambio di valore di un sensore numerico pubblica i dati della casa solo se il sensore è scelto per un lato o un quadrante (o è mostrato, e allora lo pubblica già la sua sostituzione). Un sensore nascosto dall'utente e non scelto non sveglia più l'interfaccia, come in M1: il suo valore si aggiorna in memoria e parte con la prossima pubblicazione.
- Pillola: in «dorme» anche il lato «Rete» si svuota (l'osservatore della rete è spento, l'icona potrebbe mostrare una rete caduta); il posto resta e la larghezza non cambia.
- Vista PC: il cursore del volume esce dal trascinamento anche quando il sistema annulla il puntatore.
- Con il blocco delle fonti del campionatore rovinato da un panico, le fonti lente restano spente (prima si accendevano tutte, GPU compresa).

### Added (M2, Task 10: utilizzo della GPU nella vista PC)
- Solo Windows: nella vista PC, colonna «Sistema», sotto CPU, RAM e disco, la riga «GPU» con la percentuale e la barra come la CPU (indicatore per Narrator, valore «37%»). Senza dato (fuori da Windows, PC senza contatori della GPU, lettura fallita, primo campione) la riga non c'è, mai «0%». Nulla esce dal pannello 800 × 300 (test `vista_pc_entra_nel_pannello` col caso più pieno e la GPU).
- L'utilizzo è quello del nodo (motore) più occupato fra tutti gli adattatori, come Gestione attività: tempo di esecuzione del nodo fra due letture diviso il tempo trascorso, fuori scala 100 (`pc_stats::gpu_da_tempi`).
- Quadranti configurabili: la fonte «GPU» (scala 0..100 %, icona dello schermo) fra le scelte di Impostazioni › Isola, solo su Windows (nota ««GPU»: Solo su Windows.» sotto i quadranti). Non è fra i lati della pillola.
- Contatori del kernel grafico (`windows-sys` 0.52, feature `Wdk_Graphics_Direct3D`, gdi32.dll), come System Informer: `D3DKMTEnumAdapters2` (gli handle degli adattatori si chiudono subito con `D3DKMTCloseAdapter`), `D3DKMTQueryStatistics` con `D3DKMT_QUERYSTATISTICS_ADAPTER` (numero di nodi) e `D3DKMT_QUERYSTATISTICS_NODE` (`RunningTime` per nodo). `D3DKMTQueryStatistics` è documentato da Microsoft come «Reserved for system use»: posizioni dei campi controllate con `const`, 4 KB di margine dopo la richiesta; se l'API manca o fallisce la GPU è assente (mai un panico). Si tengono solo i nodi che rispondono all'apertura (sul PC di casa un adattatore su tre dice 13 nodi ma li rifiuta). Fonti e vincoli in testa al modulo `gpu` di `pc_stats.rs`.
- Leggerezza: il lettore nasce solo quando la GPU si vede (vista PC, o un quadrante «GPU» nella Panoramica a Quadranti: `Fonti::gpu`) sul thread dei campioni da 2 s già esistente, e si butta appena non si vede più o col pannello; gli handle degli adattatori si chiudono subito dopo l'elenco (lo dice la lettura del codice: nessun test li può contare, vedi sotto). Cambiando vista il thread si sveglia subito; la GPU appena accesa ha il valore dopo 300 ms. Pillola a riposo e pannello su altre viste: nessun campione GPU.
- Cancello di memoria D7 (test `gpu_memoria_dopo_cinque_cicli`, `#[ignore]`, nel processo di test: l'assistente vero era aperto). Prima prova con PDH («GPU Engine»): 5,53 MB prima, 11,54 MB dopo il primo ciclo (+6,00 MB, tabelle che PDH carica e non libera), 11,59 MB dopo cinque, oltre i +2 MB di D7: PDH tolto del tutto. Con D3DKMT: 5,52 MB prima, 5,53 MB dopo il primo ciclo e dopo cinque (+0,01 MB). Il test misura solo la memoria: gli handle D3DKMT non sono contati da `GetProcessHandleCount`, quindi un controllo degli handle non vedrebbe una loro perdita e non c'è.
- Fuori da Windows (macOS, Linux): niente riga GPU, niente scelta «GPU» per i quadranti, niente D3DKMT.

### Added (M2, Task 9: batteria delle cuffie Bluetooth)
- Solo Windows: avviso *informazione* «<nome> al 18%» («Ricarica il dispositivo Bluetooth quando puoi.», con «Ho capito») per un dispositivo Bluetooth **connesso** con la batteria al 20% o meno; la voce sparisce sopra il 25%, a dispositivo disconnesso o con «Ho capito». Uno solo al giorno per dispositivo, anche dopo un riavvio dell'app: `Config::bt_avvisati` (impronta → data, le più vecchie di 7 giorni si tolgono quando si salva un nuovo avviso, non a ogni salvataggio della configurazione: il file resta limitato senza riscrivere la configurazione per questo). Cuffie spente o con un valore di batteria vecchio: nessun avviso.
- Vista PC, colonna «Energia e audio» (la «card del PC»): sotto Batteria e Volume la riga «CUFFIE» con il dispositivo connesso più scarico (nome accorciato, percentuale, glifo della batteria con la tinta *informazione* a 20% o meno; «+1», «+2» se ce ne sono altri e per Narrator l'elenco intero). Senza dispositivi connessi con la batteria leggibile la riga non c'è. Una sola lettura per ingresso nella vista, su un thread di lavoro (mai su quello dell'interfaccia), nessun timer a vista aperta.
- Impostazioni › Isola: interruttore «Avvisa quando le cuffie Bluetooth sono scariche» (acceso di serie; solo su Windows).
- Leggerezza: il thread `batterie-bt` aspetta con `recv_timeout` (nessun sondaggio): prima lettura 60 s dopo l'avvio, poi ogni 15 minuti, solo con l'interruttore acceso; si ferma spegnendolo. A riposo, con l'interruttore acceso (di serie), c'è quindi un thread in più, fermo, e una scansione SetupAPI di tutti i dispositivi ogni 15 minuti (decisione D6); quanto costa in memoria si misura nella prova.
- SetupAPI (`windows-sys`, feature `Win32_Devices_DeviceAndDriverInstallation` e `Win32_Devices_Properties`): `SetupDiGetClassDevsW` con `DIGCF_PRESENT | DIGCF_ALLCLASSES`, `SetupDiGetDevicePropertyW`, `CM_Get_DevNode_Status`. La chiave della batteria Bluetooth e la proprietà «connesso» **non sono documentate da Microsoft**: sola lettura, assenza sopportata (senza la proprietà il dispositivo conta come non connesso). Fonti e vincoli in testa a `sistema/batterie_bt.rs`.
- L'impronta è del contenitore del dispositivo (`DEVPKEY_Device_ContainerId`), uguale per tutti i suoi nodi (audio, telefonia, GATT): la percentuale è la minima fra i nodi connessi e l'impronta non cambia nella giornata, quindi l'avviso resta uno al giorno; due dispositivi con lo stesso nome restano separati.
- Il nome del dispositivo si vede nella vista PC e nel titolo dell'avviso, mai nei log né nei test; si salva solo l'impronta (FNV-1a a 64 bit del contenitore del dispositivo, o dell'id di istanza del nodo se il contenitore manca; l'id contiene l'indirizzo e non si salva mai in chiaro).
- `casa-core`: `Config::avvisi_bt` (di serie vero) e `Config::bt_avvisati` (vuoto per un file di prima).
- Fuori da Windows (macOS, Linux): niente riga «CUFFIE», niente interruttore, niente thread.

### Added (M2, Task 8: Wi-Fi, Bluetooth e icona della rete)
- Vista PC (la «card del PC», decisione di Paolo al posto del pannellino sopra il dock: niente icone nella fascia del dock) in tre colonne: «Sistema» (CPU, RAM, disco; posto per la GPU del Task 10), «Energia e audio» (batteria col glifo, volume e, dal Task 9, le cuffie), «Connessioni». Righe alte 38 px, testi da 12 px in su, colonna del valore larga 120 px (anche il cursore del volume); nulla esce dal pannello 800 × 300 (test `vista_pc_entra_nel_pannello`).
- «Connessioni»: la riga «RETE» con l'icona della rete (cavo, o Wi-Fi con 0–3 tacche: le spente al 30%) e la velocità; sotto, due riquadri «Wi-Fi» e «Bluetooth» (raggio 14; acceso col token `radio-acceso` dal petalo 3; sotto il nome il nome della rete, «Acceso», «Spento» o «Non presente» se manca la radio, non cliccabile) e le righe «Reti Wi-Fi ›» e «Dispositivi Bluetooth ›», che aprono le pagine Wi-Fi e Bluetooth delle Impostazioni di Windows. Per Narrator i riquadri sono interruttori (`switch`, acceso = `checked`, lo stato nella descrizione).
- Un clic o Invio sposta subito l'interruttore; se Windows rifiuta (accesso negato, criteri, modalità aereo, errore) compare in ambra «Non riuscito: apro le Impostazioni di Windows», si apre la pagina del Wi-Fi o del Bluetooth e il riquadro torna allo stato vero (l'ultimo `StateChanged`). Vale anche per il rifiuto silenzioso: se Windows accetta il cambio ma 3 s dopo la radio è ancora com'era (criteri, modalità aereo, interruttore fisico), l'esito è «negato» (`esito_cambio`, test `esito_dopo_la_rilettura`). Un esito che arriva a vista PC già lasciata si ignora (test `esito_a_vista_pc_lasciata_ignorato`). Il rifiuto si prova solo coi test (`rifiuto_mostra_errore_e_stato_reale`): nessun test accende o spegne le radio vere.
- Tastiera della vista PC: Tab e Maiusc+Tab passano da volume, Wi-Fi, Bluetooth, «Reti Wi-Fi ›», «Dispositivi Bluetooth ›» prima di lasciare la vista; frecce nella colonna (← → nella riga, ↑ ↓ fra riquadri e righe); Invio o Spazio commutano o aprono; Esc fa quello che faceva già. Anello del fuoco e fuoco vero su ogni elemento (test `connessioni_da_tastiera`).
- Pillola: il lato «Rete» (Impostazioni › Isola, Solo su Windows) mostra solo l'icona da 16 px (posto riservato anche prima della prima lettura); Narrator legge «ItsyHome, Wi-Fi, 2 tacche su 3» o «rete via cavo». Il nome della rete non compare mai nella pillola né nei log: si legge solo a vista PC aperta.
- WinRT (`windows` 0.62.2, feature `Devices_Radios`, `Networking_Connectivity`, `Foundation`, `Foundation_Collections`, solo `cfg(windows)`): `Radio` (thread `radio`, COM multithread) vive solo a vista PC aperta, con un gestore `StateChanged` per radio, `RequestAccessAsync` alla prima commutazione e `SetStateAsync` uno alla volta; un cambio accettato è confermato dallo `StateChanged` nello stato voluto o, in mancanza, da una sola rilettura 3 s dopo; lasciando la vista o chiudendo il pannello toglie i gestori e libera le radio. `OsservaRete` (thread `rete`) è acceso solo se il lato «Rete» è scelto e la pillola si vede (e non dorme) o se la vista PC è aperta: `NetworkStatusChanged` ed eventi, e le tacche rilette ogni 30 s solo se la connessione è Wi-Fi. Con il lato «Rete» spento il riposo non cambia.
- `browser.rs`: `PaginaWindows` e `apri_pagina` con URI fissi, solo quelli documentati da Microsoft in «Launch Windows Settings» (`ms-settings:network-wifi` per «Reti Wi-Fi ›» e dopo un rifiuto del Wi-Fi, `ms-settings:bluetooth`), via `ShellExecuteW`; `apri` resta solo `http`/`https`. L'URI `ms-availablenetworks:` del brief non ha documentazione Microsoft e non si usa.
- Icone `wifi-0`…`wifi-3`, `cavo`, `bluetooth` (da Lucide «wifi», «ethernet-port», «bluetooth», ISC).
- Fuori da Windows (macOS, Linux): niente riquadri né righe delle radio, niente scelta «Rete» per la pillola; la colonna «Connessioni» mostra solo la velocità della rete.

### Added (M2, Task 7: volume del PC)
- Solo Windows: la rotella sopra la pillola (a riposo o col puntino, non allargata né ritirata) cambia il volume del PC, 2 punti a scatto (`PX_PER_PUNTO` = 30 px logici, da tarare dal vivo); al posto degli occhi compare la barra del volume (40 × 6, traccia e riempimento del colore degli occhi; da muto solo la traccia) e gli occhi tornano 1,2 s dopo l'ultimo cambio con una sveglia singola. Narrator intanto legge «Volume 40%». In su da muto l'audio torna, come coi tasti del volume di Windows. I tasti del volume della tastiera non fanno comparire la barra.
- Vista PC (la «card del PC», decisione di Paolo che sostituisce l'altoparlante nel dock): la riga «VOLUME» nella colonna «Energia e audio» (dal Task 8), impaginata come le altre righe, con l'altoparlante (pulsante «Volume 40%», clic o Narrator = muto sì/no; «Audio disattivato» sotto l'etichetta da muto, subito, senza aspettare Windows) e un cursore orizzontale largo come la colonna del valore (120 px dal Task 8) sempre visibile (slider per Narrator col valore, ±2 punti con le azioni di incremento; clic e trascinamento). Da tastiera il cursore è il contenuto della vista PC: ←/→ ±2 punti, Home/Fine 0/100, Invio o Spazio muto sì/no. La riga c'è solo su Windows con un'uscita audio: senza dispositivo non c'è (non disattivata) e torna appena ne compare uno.
- Core Audio (`windows` 0.62.2, già nel Cargo.lock, solo `cfg(windows)`): un thread `audio` in COM multithread nasce al primo scatto della rotella o alla prima apertura del pannello e resta fermo su `recv()` (nessun sondaggio); `IAudioEndpointVolumeCallback` segue volume e muto cambiati da chiunque, `IMMNotificationClient` il cambio del dispositivo predefinito (cuffie staccate: ci si ricollega al nuovo, senza dispositivo la rotella non fa nulla e la riga sparisce). Le callback di Windows accodano soltanto. Nessuna proprietà si riscrive se il valore (al punto intero) non cambia.
- Rifiniture: una regola sola per il muto, anche col cursore o le frecce della vista PC (alzando il livello sopra quello attuale l'audio torna, abbassandolo il muto resta); il cursore dice a Narrator «Invio: silenzia o riattiva»; i comandi al volume portano un contesto proprio (`pguidEventContext`) e l'eco in `OnNotify` si tratta come conferma, così con la rotella veloce la barra non torna indietro di un passo; per questo ogni nostro comando (rotella, cursore, muto) aggiorna da sé lo stato mostrato (il muto solo dal giro finale della revisione, vedi in testa).
- Impostazioni › Isola: interruttore «Volume con la rotella sulla pillola», acceso di serie; fuori da Windows la riga non c'è.
- Icone `altoparlante.svg` e `altoparlante-muto.svg` (da Lucide «volume-2» e «volume-x», ISC).
- `casa-core`: `Config::volume_rotella` (di serie vero, anche per un file di prima senza la chiave).
- Fuori da Windows (macOS, Linux) il volume non c'è: niente rotella, niente riga nella vista PC, niente interruttore.

### Added (M2, Task 6: scelta dello schermo)
- Solo Windows, solo con più di un monitor: il sottomenu «Schermo» (una voce a spunta per monitor: «Schermo principale», «Schermo 2 · 2560×1440», numerati da sinistra a destra) fra «Informazioni» e «Ritira l'isola», sia nel menu «⋯» del pannello sia in quello dell'icona nella barra. La scelta si salva (`Config.schermo`, il nome del monitor come lo dà winit; `None` = il principale) e sposta subito pillola e pannello aperto. Con un monitor solo il sottomenu non compare. Il clic destro sulla pillola resta senza menu.
- Pillola, pannello, zona della linea della ritirata e controllo dello schermo intero seguono il monitor scelto (tutti passano da `schermo_di` / `monitor_di`). Se il monitor scelto non c'è (scollegato) la pillola sta sul principale e la scelta resta salvata: appena il monitor ricompare, il controllo da 5 s già esistente la riporta lì (nessun codice di sorveglianza nuovo, nessun timer in più).
- Il menu non si ricostruisce a ogni apertura: con `muda` per l'icona nella barra non c'è un evento «prima dell'apertura», quindi si ricostruisce solo quando cambia l'elenco dei monitor (o la spunta), controllato dal giro da 5 s e subito dopo una scelta; il «⋯» lo ricontrolla anche un attimo prima di aprirsi. Una scelta su un indice che non esiste più non fa nulla.
- Il nome del monitor non compare nei log.
- Fuori da Windows (macOS, Linux) non c'è un menu nativo e quindi nessun modo di scegliere lo schermo; `Config.schermo`, se scritta a mano nel file, vale lo stesso.
- `casa-core`: `Config::schermo` (un file di prima, senza la chiave, si legge con `None`).

### Fixed (giro finale della revisione di M1)
- Fuori da Windows si può uscire dall'assistente: Impostazioni › Sistema ha la riga «Esci dall'assistente» con il pulsante «Esci» (su Windows «Esci» resta nel menu «⋯» e nell'icona nella barra).
- Con Home Assistant ancora in avvio la rilettura dei 5 s non si arma più a connessione caduta (come ogni altra riprova).
- CI `v3-build`: parte anche su `main`; voci usate solo da Windows non fanno più fallire clippy (`-D warnings`) su macOS e Ubuntu.

### Added (M2, Task 5: linea della ritirata che si riempie, schermo intero rilevato subito)
- La linea della ritirata ha una traccia neutra e, sopra, il riempimento da sinistra col colore del livello più urgente fra voci e avanzamento; senza avanzamento resta piena come prima. Nuovi `Avanzamento`, `Ponte::metti_avanzamento` / `togli_avanzamento` (pronti per Claude Code e Focus); la frazione si riscrive nell'interfaccia solo se cambiano i pixel riempiti. Flag di prova `--prova-avanzamento <secondi>`.
- Solo Windows: la pillola si accorge dello schermo intero appena cambia la finestra in primo piano (`SetWinEventHook` su `EVENT_SYSTEM_FOREGROUND`, tolto con `UnhookWinEvent` allo spegnimento), senza aspettare il controllo da 5 s; l'isteresi 3 s / 1,5 s è la stessa e il controllo da 5 s resta per l'F11 dei browser. Il gancio non sveglia l'app se la finestra in primo piano non cambia. Fuori da Windows non c'è (resta il controllo da 5 s, che lì non rileva comunque lo schermo intero).
- Esempio `schermo_pieno` per le prove: finestra senza bordi che entra a schermo intero sul monitor scelto appena compare (`--dopo <s>` la ritarda, per riprodurre il caso dell'F11, coperto dal controllo da 5 s).

### Added (M2, Task 4: lati della pillola, sezione «Isola», quadranti configurabili)
- Pillola: a sinistra e a destra degli occhi, a scelta, l'ora, la batteria (glifo e percentuale, solo se il PC ne ha una), CPU, RAM, la temperatura esterna o quella di un sensore della casa («CPU 23%», «14:05», «9°», «21,5°»). Testo 13 px peso 600 nel carattere mono (cifre a larghezza fissa); misure di TwoCode (margine 14, mezzi-occhi 19, distanza 12): con due lati occhi al centro, con uno occhi e valore affiancati, mai oltre i 440 px della finestra. Lati visibili a riposo e col puntino, nascosti da allargata e da ritirata; senza dato il lato resta vuoto (mai «0°»), la CPU illeggibile è «CPU —». Narrator legge «ItsyHome, CPU 23%, batteria 80%». Il pannello aperto dalla pillola con un lato solo fa partire gli occhi da dove stanno.
- Leggerezza: con «niente / niente» (di serie, anche per un file di M1) nessun timer in più. CPU e RAM con un campionatore leggero (`GetSystemTimes` e RAM, niente dischi né rete) ogni 5 s, l'ora con una sveglia al minuto, e solo a pillola visibile (né ritirata, né nascosta, né allargata) e sveglia; la batteria resta sul thread da 60 s di M1; meteo e sensori arrivano coi dati della casa. Un lato si riscrive solo se cambia.
- Impostazioni: nuova sezione «Isola» fra Collegamenti e Sistema («Informazioni» passa alla posizione 5) con «A sinistra degli occhi», «A destra degli occhi», «Meteo da» (Automatico o un'entità `weather.*`) e «Quadrante 1/2/3»; accanto a un sensore in W, kW o con un'unità senza scala naturale il campo «Massimo» dell'arco. Nota: il nome della rete non compare mai nella pillola.
- Nuovo componente `Scelta` (pulsante col valore e «›», elenco in un `PopupWindow`): Invio, Spazio o ↓ aprono, ↑/↓ e Invio scelgono, Esc chiude e il fuoco torna al pulsante; per Narrator una casella combinata con nome e valore.
- Quadranti configurabili: tre quadranti a scelta fra ora, batteria, CPU, RAM, disco, rete in arrivo e in uscita, temperatura esterna e i sensori della casa (vuoto = CPU, RAM e disco di M1), più i sensori preferiti di prima. Arco: % 0–100, °C −10–40, W 0–3000 e kW 0–6 se non si sceglie un altro massimo, altre unità 0–massimo scelto (o il doppio del valore). Il campionatore del pannello legge dischi e rete solo se la vista PC o un quadrante li mostra.
- `casa-core`: `Config` con `lato_sinistro`, `lato_destro`, `quadranti` e `massimi_quadranti`; i sensori scelti per lati e quadranti entrano sempre nell'insieme di `subscribe_entities` (mai un elenco vuoto: la guardia resta in `iscrivi_entita`).
- Ovunque: lati CPU, RAM, ora, batteria e temperature funzionano anche su macOS e Linux (la rete col Task 8, solo su Windows).
- Quadranti: fra i sensori della casa c'è ogni sensore numerico di Home Assistant (temperatura, umidità, potenza in W o kW, energia e produzione FV), da un elenco a parte di `MenuData` (`sensori_numerici`: entity_id, nome, valore, unità) che lascia stanze e preferiti come prima. Il valore di un sensore scelto si aggiorna a ogni cambio senza mappatura completa né letture in più; quello di un sensore non scelto e non mostrato non pubblica nulla da solo (giro finale della revisione).
- La pillola si misura su un campione fisso per tipo («CPU 100%», «00:00», «100%», «-10,5°»; per un sensore il valore più largo della sua unità): «CPU 9%» → «CPU 10%» non la allarga più, la larghezza cambia solo con la scelta.
- Con l'umore «dorme» e la pillola visibile ora, CPU e RAM (e, dal giro finale della revisione, la rete) si svuotano con un solo ridisegno (fermi sarebbero sbagliati) e tornano aggiornati al risveglio; batteria, meteo e sensori restano; la larghezza non cambia.
- Impostazioni › Isola: il campo «Massimo» ha per Narrator il nome del suo quadrante («Quadrante 1, massimo») e, scegliendo un altro sensore, mostra il massimo di quello nuovo.

### Added (M2, Task 3: riga alta con data e meteo, viste che scorrono)
- Riga alta del pannello, a sinistra degli occhi e solo nella Panoramica: nei Quadranti «VEN 13 NOV · 9°» (senza meteo la sola data), nell'Orologio «9° · Nuvoloso» (la data è già grande al centro; senza meteo niente). Nelle app la parte sinistra resta vuota. Per Narrator il testo è per esteso («venerdì 13 novembre, 9 gradi, nuvoloso»). Si riscrive solo se cambia: minuto nuovo, meteo nuovo, altro layout.
- Cambiando vista il contenuto scorre per 280 ms dal lato della scheda scelta (riga alta e dock fermi); il ritorno alla Panoramica (dal dock, con Backspace o Alt+←) va verso la prima scheda. Esc non porta alla Panoramica: un livello alla volta riduce la voce espansa, torna a «Preferiti» da una stanza aperta, poi chiude il pannello. Con «Effetti di animazione» spento niente scorrimento, solo una dissolvenza da 120 ms. Nessun ridisegno fuori dall'animazione.
- `al_prossimo_minuto` passa da `pannello.rs` a `orologio.rs`; condizioni di Home Assistant in italiano e temperatura arrotondata.

### Added (M2, Task 2: meteo da Home Assistant in `casa-core`)
- `MenuData::meteo` (condizione, temperatura, unità) dalla prima `weather.*` disponibile, o da quella scelta con `Config::meteo_entita` (`None` = automatico; una scelta non disponibile dà «nessun meteo», non quello di un'altra); `MenuData::meteo_elenco` per il selettore delle Impostazioni. Le `weather.*` restano fuori da stanze e preferiti.
- Un cambio del meteo si pubblica come una sostituzione (niente mappatura completa, né letture); nel registro diagnostico lo conta `effetto_meteo`. Cambiare `meteo_entita` rimappa solo in memoria.
- Finto HA: `weather.casa` (`cloudy`, 9 °C) e il servizio `weather.imposta` per cambiarlo. Su tutte le piattaforme.

### Added (M2, Task 1: batteria)
- Vista PC: glifo della batteria accanto al valore (verde in carica, tinta normale, blu «informazione» a 20% o meno, ambra «attenzione» a 10% o meno); riempimento proporzionale, solo decorativo (il testo dice già percentuale e stato).
- Voce «Batteria al N%» di livello *informazione* sotto il 20% a batteria (isteresi fino al 22%), con «Ho capito» che la zittisce fino alla ricarica; sotto il 10% resta l'*attenzione* di M1, senza pulsanti. Lettura, tinta e voce valgono anche su macOS e Linux.

### Added
- Assistente sempre presente in alto al centro: pillola con occhi e umori, pannello 800 × 300 (Panoramica Orologio/Quadranti, Casa, PC), Impostazioni in finestra dedicata, finestra di gestione in processo separato (`itsyhome-gestione`).
- Workflow CI `v3-build` (Windows, macOS, Ubuntu): `cargo build -p assistente -p gestione` e `cargo test --workspace`.
- `--prova-gestione-cicli <n>` per misurare aperture e chiusure ripetute della finestra di gestione (un numero non valido ora lo dice).
- `--prova-avviso <livello> --lungo`: titolo che non sta nella pillola e testo da scorrere, per le prove.
- Movimento ridotto: con «Effetti di animazione» spento in Windows niente pulsazioni né scivolamenti.
- CI: `libdbus-1-dev` esplicita su Linux e `cargo clippy --workspace --all-targets -- -D warnings`.

### Changed (revisioni UI/UX e frontend, giro 1)
- «Pensa» solo alla prima connessione e dopo un'azione dell'utente (Salva, Connetti, Riprova), a 20 fps e senza ombra sfocata mentre si muove; le riconnessioni automatiche restano tranquille.
- Permessi: nella pillola, se il testo è tagliato, «Nega» + «Apri» (il consenso si dà dal pannello col testo intero); nel pannello fuoco iniziale su «Nega» e «Consenti» solo con Ctrl+Invio o clic; 10 px fra i pulsanti.
- Casa senza connessione: niente colore di stato, card tenui leggibili, testata «Casa non raggiungibile · dati delle HH:MM» con «Riprova»; vista vuota con «Apri Impostazioni».
- Decimali con la virgola; livelli per l'utente «Aspetta te» / «Guarda presto» / «Quando vuoi»; «Casa» nell'assistente e «Home Assistant» solo dove si configura.
- Pillola: puntino 5 × 5 pieno (attenzione) o ad anello (informazione), pulsanti 26/28 px larghi 64–120, bloccante con bordo da 4 px, anello a riposo al 45%, linea della ritirata 300 × 5 con filo scuro.
- Pannello: Esc riduce prima la voce espansa, testo lungo scorrevole, puntini 24 × 24 nascosti sotto la voce, Orologio centrato, traccia degli archi visibile e icone distinte per CPU, RAM e disco.
- Impostazioni: Informazioni con «© Paolo Bets» e il riquadro AboutSlint (licenza Slint royalty-free 2.0, punto 2(a)), raggiungibile anche dalla voce «Informazioni» del menu della tray, senza «Licenza MIT»; suggerimenti generici e più tenui. AboutSlint porta con sé lo stile dei widget standard di Slint: l'eseguibile dell'assistente passa a circa 24 MB (RAM a riposo invariata).
- Finestra di gestione: adattata all'area di lavoro (al più 780 × 680), contenuto rientrato sotto una linea fidata con l'etichetta «CONTENUTO ESTERNO», titolo ripulito (controlli, direzione, larghezza zero, 200 caratteri), Tab porta nel contenuto.
- Scorciatoie: Ctrl+Alt+Maiusc+Spazio (apri) e Ctrl+Alt+Maiusc+I (ritirata), anche nella spec.

### Changed (revisioni UI/UX e frontend, giro 2: struttura dell'interfaccia)
- Colori solo in `tema.slint`, come token di ruolo (un test fallisce se un `.slint` ne scrive uno a mano); scale fisse di raggi e corpi del testo.
- Componenti comuni (`componenti.slint`: anello del fuoco, pulsante, etichetta; `faccia.slint`: misure, occhi e battito) ed enum Slint al posto delle stringhe di stato.
- Un solo stile di pulsante (raggio 10, 8 nella pillola; neutro senza bordo, azione e pericolo col bordo): nelle Impostazioni i pulsanti non sono più a capsula.
- Misure condivise fra Slint e Rust controllate da `cargo test` (backend di prova di Slint, senza finestre), non più solo all'avvio in debug.
- Accessibilità: il fuoco da tastiera del pannello è un fuoco vero che Narrator legge (schede, voci, card, pulsanti); dock, puntini e sezioni come schede con lo stato; voci espandibili; quadranti e righe PC con nome e valore; pillola come pulsante.
- Sensori dei Quadranti a casa non connessa leggibili (circa 7:1 invece di 2,9:1).
- Vista PC: righe su tutta l'altezza, rete «↓ … · ↑ …» su una riga.
- Gestione: il riquadro web si rimette al suo posto quando la finestra cambia scala o misura; spazio predisposto per mittente e data.
- Spessori sottili (filo del pannello, anello della pillola e del fuoco) arrotondati al pixel fisico a 125% e 175%.

### Fixed
- Dopo la prima apertura del pannello la pillola restava a 16,5–18,5 MB (budget 15) con ~100 handle in più: la CPU letta da sysinfo passava da PDH, che non libera mai le sue tabelle. Su Windows ora si usa `GetSystemTimes`.
- «Informazioni» dalla tray non lascia più la sezione in sospeso se le Impostazioni non si aprono; due letture della CPU senza tempo trascorso tengono il valore di prima invece di «—».

### Misure (14/11/2026, build release, Home Assistant finto, Windows 11, schermo 3840 × 2160 al 175%)

RAM privata e CPU dell'intero albero di processi (`tools/misura.ps1` e campionamento al secondo), ogni stato per almeno 2 minuti.

| Stato | RAM privata | CPU media (max) | Budget |
|---|---|---|---|
| Pillola ferma | 7,8–8,3 MB | 0,014% (0,44) | ≤ 15 MB, ≤ 0,1% |
| Pillola animata («pensa», 25 fps) | 7,9–8,2 MB | 0,58% (1,22) | ≤ 1% |
| Pillola che chiama e pulsa (attenzione)¹ | 9,5–9,7 MB | 0,52% (0,98) | ≤ 1% |
| Pillola ritirata (linea) | 7,9–8,2 MB | 0,008% (0,12) | ≤ 15 MB, ≤ 0,1% |
| Pannello aperto fermo | 13,8 MB (prima della correzione 20,3) | 0,08% (0,44) | ≤ 30 MB, ~0% |
| Pillola dopo 1 / 25 aperture del pannello | 9,96 / 10,53 MB (prima 16,5–18,5) | 0,02% a riposo | entro +2 MB |
| Finestra di gestione aperta (9 processi, WebView2) | 166–178 MB | picco ~14% all'apertura | solo mentre è aperta |
| Assistente dopo 5 cicli della gestione | 8,2 MB, nessun processo figlio vivo | 0% | entro +2 MB |

Thread 6–12 e handle 302–326 per tutta la prova lunga, senza crescita; oggetti GDI/USER tornano a 17/20 dopo ogni chiusura del pannello.

¹ Misurata con una build temporanea (pillola larga per 600 s invece di 20), non quella del ramo: da sola la pillola che chiama pulsa solo 20 s.

### Misure dopo il giro 1 delle revisioni (14/11/2026, stesse condizioni, 5 minuti per stato)

Campioni da 1 s del solo processo dell'assistente; il budget delle animazioni vale sulla media, il 95° percentile dice quanto spesso si va oltre.

| Stato | RAM privata | CPU media | 95° percentile | max | Budget |
|---|---|---|---|---|---|
| Pillola ferma | 8,2–8,3 MB | 0,010% | 0,000% | 0,39% | ≤ 15 MB, ≤ 0,1% |
| Pillola «pensa» (20 fps, senza ombra in moto) | 8,0–8,2 MB | 0,44% | 0,77% | 1,16% | ≤ 1% |
| Pillola che chiama e pulsa¹ | 9,6–9,7 MB | 0,56% | 0,90% | 1,28% | ≤ 1% |

### Misure dopo il giro 2 delle revisioni (14/11/2026, stesse condizioni, 5 minuti per stato)

| Stato | RAM privata | CPU media | 95° percentile | max | Budget |
|---|---|---|---|---|---|
| Pillola ferma | 8,0–8,2 MB | 0,016% | 0,13% | 0,65% | ≤ 15 MB, ≤ 0,1% |
| Pillola ferma e sveglia (`--umore calm`; giro 1 ricostruito, stessa prova: 0,023% / 0,26% / 0,51%) | 8,0–8,2 MB | 0,019% | 0,13% | 0,52% | ≤ 15 MB, ≤ 0,1% |
| Pillola «pensa» | 8,2–8,3 MB | 0,45% | 0,77% | 1,15% | ≤ 1% |
| Pillola che chiama e pulsa¹ | 9,7–9,8 MB | 0,53% | 0,90% | 1,04% | ≤ 1% |

Nel giro 2 la pillola che chiama era salita a 10,6–10,7 MB: i suoi pulsanti usavano il carattere di sistema (un font in più in memoria). Corretto prima di questa misura.

### Changed (Task 14d: dalla prima prova sulla casa vera, 15/11/2026)
- Casa reattiva e leggera: uno `state_changed` aggiorna in memoria solo la sua entità (dal `new_state` dell'evento, come da documentazione di Home Assistant) e si pubblica solo se cambia ciò che l'app mostra; raffiche visibili in una pubblicazione (50 ms, tetto 300 ms). Stati, registri e configurazione si leggono tutti solo alla (ri)connessione, dopo gli eventi `*_registry_updated` (ora seguiti) o con uno stato illeggibile. Prima ogni evento della casa, sensori compresi, faceva rileggere tutto almeno una volta al secondo.
- Memoria: stati mai mostrati senza attributi, risultati delle richieste presi dal messaggio invece che copiati, niente più `persistent_notification/get`.
- Tocco: l'icona cambia subito (stato ottimistico, riusabile per ogni dispositivo); torna com'è con l'errore se il comando fallisce o se in 5 s Home Assistant non conferma.
- Niente icona nella barra di serie (decisione di Paolo): il clic destro sulla pillola apre il menu dell'isola (Apri, Impostazioni, Informazioni, Ritira/Mostra l'isola, Esci), nativo di Windows, sotto la pillola. L'icona si accende o spegne subito da Impostazioni › Sistema › «Mostra icona nella barra». La ritirata ora si sceglie dal menu.
- `finto_ha`: `--raffica <n>`, `--casa-grande <n>`, `--lenta <ms>` per provare una casa che si muove, pesante o lenta.

### Misure del Task 14d (15/11/2026, build release, Home Assistant finto con 7 entità e 7 preferiti, `--umore calm`)

| Stato | Prima (bd02ec2, 2 min) | Dopo (5 min) | Obiettivo |
|---|---|---|---|
| Riposo | 0,022%, 10,6 MB | 0,015% (p95 0,13, max 0,39), 8,0–8,2 MB | ≤ 0,1% |
| Riposo con 20 `state_changed`/s | 0,074%, 10,7–11,2 MB | 0,053% (p95 0,26, max 0,39), 8,3–8,4 MB | ≤ 0,05%, RAM +≤ 1 MB |
| Casa grande (807 entità) con 20/s, 2 min | 0,182%, 15,0–17,5 MB | 0,046%, 11,1–11,3 MB | |
| Pillola che chiama e pulsa¹ | 0,53% (giro 2) | 0,557% (p95 0,91, max 1,43), 9,4–9,5 MB | ≤ 1% |

Tocco su una casa lenta (`--lenta 800`): l'icona cambia sullo schermo 32–51 ms dopo l'invocazione (prima 1150 ms); la conferma arriva dopo circa 880 ms e non cambia più nulla. Con la casa veloce la conferma arriva in circa 60 ms. Sotto lo 0,05% con la raffica non si scende di molto: un client WebSocket minimo che riceve gli stessi eventi e non fa altro costa già circa 140 µs di CPU per messaggio su questo PC.

### Added (Task 14e: stanze e gestione dei dispositivi, 15/11/2026)
- Vista Casa: a sinistra le stanze (ordine salvato, senza le nascoste, «Altro» e «Scene» in fondo), a destra i preferiti; una stanza aperta mostra i suoi dispositivi al posto dei preferiti, come il dettaglio della v2. Luci, interruttori e scene col tocco (stato ottimistico come i preferiti), il resto in sola lettura con lo stato in italiano. Tastiera: Tab fra stanze e card, Esc e «‹ Indietro» chiudono prima la stanza.
- Impostazioni › Dispositivi (la scheda «Accessories» della v2): preferiti in cima con il loro ordine, stanze con ordine, icona (le 60 della v2, ridisegnate con icone Lucide, licenza ISC) e occhio, dispositivi con ordine, occhio e stella; stanze comprimibili. Ogni modifica è salvata subito con le chiavi della v2 e si vede subito nel pannello.

### Changed (Task 14e)
- Gli stati arrivano da `subscribe_entities` con le sole entità mostrate (preferiti, stanze, scene, telecamere, aggiornamenti), non più da `state_changed` di tutta la casa; l'iscrizione si rifà dopo ogni lettura completa e quando cambia ciò che si mostra, mai con l'elenco vuoto (per HA vorrebbe dire «tutte»).
- Prima lettura fallita: nuovi tentativi dopo 2, 4, 8… fino a 60 s, annullati da una lettura riuscita. L'esito di un comando porta il numero del suo tocco. Dopo il menu della pillola il primo piano torna a chi lo aveva solo se è ancora della pillola.
- Un `input_boolean` si commuta col suo servizio; le scene senza ordine salvato vanno per nome.

### Misure del Task 14e (15/11/2026, stesse condizioni del 14d, finto con 25 entità)

| Stato | Task 14d | Task 14e | Obiettivo |
|---|---|---|---|
| Riposo, 5 min | 0,015%, 8,0–8,2 MB | 0,019% (p95 0,13, max 0,52), 7,9–8,1 MB | ≤ 0,1% |
| Raffica 20/s su entità non mostrate, 5 min | 0,053%, 8,3–8,4 MB | 0,022% (p95 0,13, max 0,39), 8,2–8,3 MB | ≤ 0,03% |
| Casa grande (829 entità) + raffica, 2 min | 0,046%, 11,1–11,3 MB | 0,037%, 11,8 MB | |
| Pillola che chiama e pulsa¹, 5 min | 0,557%, 9,4–9,5 MB | 0,340% (p95 0,64, max 1,04), 9,5–9,6 MB | ≤ 1% |

¹ Build temporanea con `ALLARGATA_PER` = 600 s, **senza toast** (prova senza `ITSYHOME_PROVA_NOTIFICHE`, quindi senza AUMID): il confronto col 14d, misurato col toast, non è alla pari; il codice della pillola non è cambiato.

### Fixed (Task 14e, giro di correzioni 1)
- Entità che nascono dopo la lettura (HA appena riavviato, integrazioni lente, ricaricamenti) non restavano più fuori: si seguono anche le voci del registro che l'app mostrerà appena avranno uno stato, un'entità tolta resta seguita fino alla lettura successiva, e se `get_config` dice che HA non è ancora «RUNNING» si rilegge dopo 5 s (arrivato a «RUNNING» HA manda anche `core_config_updated`, che fa rileggere).
- Riconnessione con lettura fallita: si riprova con l'attesa crescente anche se ci sono i dati di prima, e intanto ci si iscrive agli stati.
- Iscrizione agli stati rifiutata da HA scritta nel log; catalogo in un `Arc` (non si copia a ogni cambio); scelta dell'icona chiusa da un clic fuori; le scene con le scintille (come il «✨» della v2), la stella solo per i preferiti.

### Added (Task 14f: collegamenti veloci, 15/11/2026)
- App «Collegamenti» nel dock (icona a catena, Lucide): le pagine web delle gestioni in una griglia di card (nome e indirizzo accorciato), nell'ordine salvato. Un tocco o Invio apre la pagina nel browser predefinito del sistema (Windows `ShellExecuteW` «open», macOS `open`, Linux `xdg-open`), mai nella finestra di gestione; il pannello poi si chiude. Se il browser non parte il pannello resta aperto e lo dice, il dettaglio va in Impostazioni › Informazioni. Stato vuoto con «Aggiungi un collegamento»; «Modifica» nella testata. Entrambi aprono le Impostazioni sulla scheda Collegamenti.
- Impostazioni › Collegamenti (fra Dispositivi e Sistema): aggiungi, modifica, elimina con una conferma dentro la finestra che nomina il collegamento («Eliminare “Proxmox”?», fuoco su «Annulla», Esc annulla), ordine con le frecce su/giù. Nome obbligatorio (al più 60 caratteri, ripulito da controlli e caratteri invisibili); indirizzo solo `http://` o `https://` (senza schema si aggiunge `https://`; schema e host in minuscolo), rifiutati altri schemi (`file:`, `javascript:`, `data:`…), spazi, controlli, caratteri invisibili o di direzione, virgolette e credenziali nell'indirizzo. Controllo al blur sui campi già toccati, errore che sparisce appena corretto. Ogni modifica è salvata subito (`collegamenti` in `assistente.json`, un file senza la chiave si legge) e si vede subito nel pannello; l'indirizzo si ricontrolla anche quando si apre.

Riposo non rimisurato a schermo libero: durante tutta la prova lo schermo era bloccato (pillola nascosta dalla schermata di blocco), la misura di 5 minuti fatta allora (0,0004%, 7,9–8,0 MB) non si confronta con lo 0,019% del 14e. A pannello chiuso il codice nuovo non fa nulla (nessun timer, la vista nasce solo aperta); misura, raffica anti-glitch e foto restano da fare a schermo sbloccato.

### Changed (Task 14f, giro di rifinitura)
- Indirizzi dei collegamenti analizzati con `url` (regole WHATWG del browser) e salvati nella sua forma: ciò che si mostra è ciò che si apre. Nome del sito solo ASCII (un nome internazionale nella forma `xn--`): niente omografi né lettere a larghezza piena che il browser riscriverebbe; IPv6 e nomi controllati davvero, `%` solo con due cifre esadecimali, al più 2048 caratteri.
- Un solo collegamento aperto per tocco (Invio ripetuto o doppio clic non aprono altre schede); configurazione scritta a mano riparata all'avvio (id vuoti o doppi, nomi sporchi).
- Esc a livelli anche nel modulo degli ambienti di Home Assistant: chiude il modulo, non la finestra. Con la conferma di eliminazione aperta le righe sotto sono spente anche per Narrator.

### Fixed (Task 14f, minori del 14e)
- La scelta dell'icona di una stanza non si chiude più con un clic dentro il riquadro ma fuori da un'icona.
- Test di HA in avvio non più instabile.

### Changed (Task 14g: dalla prova di Paolo sulla casa vera, 16/11/2026)
- Vista Casa: «★ Preferiti» (senza numero) è la prima voce dell'elenco, senza l'intestazione «STANZE», selezionata all'apertura; la stanza già aperta non si chiude toccandola di nuovo (si torna con «Preferiti», Esc o Backspace); toccandola tornano i preferiti col titolo «Preferiti». Voci dell'elenco come schede per Narrator (nome, numero, selezionata, posizione). Esc con una stanza aperta torna a «Preferiti», poi chiude il pannello.
- «‹ Indietro» tolto dalla riga alta in tutte le viste: alla Panoramica si torna dal dock, con Backspace o Alt+←.
- Le risposte di Home Assistant (stati e registri) si leggono dal testo nei loro tipi, senza l'albero `Value` intero: meno memoria dopo ogni lettura completa.

### Added (Task 14g)
- Registro diagnostico di soli contatori, spento di serie: con `ITSYHOME_DIAGNOSI=1` ogni 60 s una riga in `diagnosi.log` accanto alla configurazione (al più 1 MB, poi `diagnosi.log.1`): messaggi WebSocket per tipo, eventi per tipo, entità `a/c/r` e loro effetto, pubblicazioni e consegne, riletture per causa, iscrizioni, stato del core, lavoro dell'interfaccia, CPU e memoria privata del processo. Mai entity_id, nomi, valori, token o indirizzi.
- `finto_ha --registro <s>`: un `entity_registry_updated` ogni `s` secondi.

### Misure del Task 14g (16/11/2026, build release, finto HA, `--umore calm`)
| Prova | Risultato |
|---|---|
| Riposo, 5 min, casa piccola (25 entità, 7 preferiti) | **0,016%** (p95 0,11, max 0,44), **8,0–8,2 MB** (14e: 0,019%, 7,9–8,1 MB) |
| Memoria a riposo dal registro, casa piccola / casa grande (807 in più) | 8,4 → **8,3 MB** / 11,7 → **11,2 MB** con le risposte lette dal testo |
| Casa grande + un evento del registro ogni 3 s | 20 riletture complete al minuto, 10,9 MB ricevuti al minuto, 375–484 ms di CPU al minuto, 13,3–13,9 MB |
| Raffica anti-glitch (raffica 20/s + registro ogni secondo, 20 s a 60 fps) | 1199 fotogrammi, **0 difettosi**; controllo negativo (maschera stretta) 300 su 300 |

### Changed (Task 14h: dai contatori della casa vera, 16/11/2026)
- Gli eventi dei registri di Home Assistant si leggono (`action`, id, campi di `changes`, `old_entity_id`): quelli che non toccano ciò che l'app mostra o segue (versioni e connessioni dei dispositivi, opzioni e capacità delle entità, entità non mostrate, stanze nuove o riordinate) si scartano; quelli che cambiano nomi, stanze o visibilità fanno rileggere **i soli tre registri** e rimappare in memoria, senza `get_states` e senza rifare l'iscrizione se le entità seguite non cambiano. Entità nuove o rinominate, entità che diventano visibili col solo stato alleggerito, eventi illeggibili e `core_config_updated` rileggono tutto come prima.
- A pannello chiuso i dati della casa non ricostruiscono più i modelli della vista Casa (la pillola mostra solo umore e voci in attesa): si rifanno una volta all'apertura.
- Registro diagnostico: contatori `registro_scartati`, `rilettura_solo_registri`, `letture_registri`.
- `finto_ha --registro-tipo nome|inutile|nuova`.

### Misure del Task 14h (16/11/2026, build release, finto HA con casa grande da 807 entità in più)
| Prova | Prima (build del 14g) | Dopo |
|---|---|---|
| 10 min, un evento ogni 180 s sul nome di una luce mostrata | 545 KB per evento (lettura completa + iscrizione rifatta, 328 entità), media 182 KB/min; RAM 10,9 → 13,1 MB | **148 KB** per evento (soli registri, nessuna iscrizione), media **50 KB/min**; RAM 11,7 → 12,7 MB |
| 10 min, ogni 180 s versione di un dispositivo + opzioni di un'entità (il caso della casa vera) | 545 KB per evento, media 182 KB/min; RAM 10,7 → 13,8 MB | **scartati**: 350 byte per evento, media **150 byte/min**; RAM **10,8 → 11,0 MB** |
| Una lettura ogni 20 s per 5 min (~14 di fila) | complete: 12,7 MB dopo la prima, 12,5 alla fine | complete: 13,2 → 13,05 MB; soli registri: 11,4 → 12,3 MB (vedi sotto le prove più lunghe) |
| Riposo 5 min, casa piccola, 7 preferiti, `--umore calm` | 14g: 0,016%, 8,0–8,2 MB | **0,015%** (max 0,33), **8,0–8,1 MB** |
| Raffica anti-glitch (raffica 20/s + un evento del registro al secondo, 20 s a 60 fps) | — | soli registri 1199 fotogrammi **0 difettosi**; letture complete 1199 **0 difettosi**; controllo negativo 299 su 299 |

### Fixed (Task 14h, giro di correzioni 1)
- Le entità nascoste dall'utente restano seguite (sono nel catalogo delle Impostazioni): un nome cambiato dall'integrazione (`original_name`) o col dispositivo rinominato (`has_entity_name`, evento con `changes: {}`) arriva senza riconnettersi. Questi eventi, e il nome del dispositivo, fanno rileggere i soli registri.
- Un evento dei registri arrivato mentre una lettura è in volo o in coda non si decide più sui registri vecchi: ciò che dipende dai registri li rilegge dopo (anche durante la prima lettura, senza una seconda lettura completa).
- Dispositivi figli (HA 2026.9+, `parent_device_id`): senza stanza propria le loro entità prendono la stanza del padre, come in HA, invece di finire in «Altro».
- Un pannello che non si riesce a mostrare spegne i modelli della vista Casa.

### Added (Task 14h, giro di correzioni 1)
- Registro diagnostico: `avvio_lettura_ms`, `avvio_consegna_ms`, `avvio_consegna_byte`, `avvio_consegna_cpu_ms` (il primo minuto, scritti una volta), `ui_pensa_ms`, `registro_campo_*` (campi di `changes` da un elenco fisso).

### Limiti noti (Task 14h)
- Il buffer di ricezione del WebSocket (`tungstenite`) resta grande quanto il messaggio più grande ricevuto, per tutta la connessione: sulla casa vera la risposta di `get_states`, circa 1–3 MB, una volta sola.
- Memoria con le letture dei soli registri: ogni lettura fa un picco (indici e mappatura vecchi e nuovi insieme). Con 14 letture di fila (una ogni 12 s) la memoria è salita da 11,9 a 12,9 MB, cioè +1,0 MB dopo la prima, oltre il ±0,5 chiesto. Con ~38 letture di fila (una ogni 8 s, 5,5 min) oscilla fra 11,4 e 13,3 MB senza salire: media 12,1 MB nella prima metà, 12,3 nella seconda, righe al minuto da 12,5 a 12,0 MB. Si assesta, non cresce. Le letture complete di fila (14, una ogni 12 s) restano piatte: 13,6 → 13,3 MB.

### Fixed (Task 14h, giro di correzioni 2)
- Una lettura completa o dei soli registri che fallisce si riprova da sola anche dopo una lettura riuscita (2, 4 … 60 s); prima, una lettura chiesta da un evento (entità nuova o rinominata) si perdeva fino all'evento successivo.
- I nomi che HA riscrive nello stato (`original_name`, `translation_key`, `has_entity_name`, `changes` vuoto, nome del dispositivo proprio) non rileggono più i registri. Restano il nome scelto (`name`) e il nome di un dispositivo padre con figli mostrati.
- Registro diagnostico: contatori anche per `translation_key`, `has_entity_name` e `parent_device_id`.

### Fixed (Task 14h, giro di correzioni 3)
- Il nuovo tentativo di una lettura non si arma più a connessione caduta (l'errore delle letture in volo arriva dopo la caduta): niente tentativi a vuoto durante la riconnessione, e la prima lettura fallita della connessione nuova si riprova dopo 2 s invece che con l'attesa cresciuta intanto.
- Una lettura completa che parte annulla il tentativo armato (niente seconda lettura accodata); il tentativo si arma sempre da capo, mai con una scadenza già passata.

### Changed (Task 14i: decisione di Paolo, 16/11/2026)
- Riga alta del pannello: dopo l'ora il pulsante **«⋯»** («Altre opzioni», 28 × 28, anello da tastiera) apre lo stesso menu nativo del clic destro sulla pillola, senza «Apri» e con «Impostazioni» in testa (Impostazioni, Informazioni, Ritira/Mostra l'isola, Esci), sotto il pulsante e allineato al suo bordo destro dentro l'area di lavoro. Esc lo chiude e il fuoco resta su «⋯»; «Ritira l'isola» da qui chiude il pannello e ritira subito la pillola.
- Dock di sole app: via l'ingranaggio delle Impostazioni. Tab gira fra «In attesa», contenuto, dock e «⋯».
- Pillola (16/11, dopo la prova di Paolo): il clic destro non fa più nulla. Il menu dell'isola sta nel «⋯» del pannello e, se accesa, nella tray; via il codice che serviva solo a quel menu (punto sotto la pillola, ritorno del primo piano).
- Giro di correzioni 1: fuori da Windows, dove il menu nativo non c'è ancora, «⋯» apre direttamente le Impostazioni (senza ingranaggio nel dock era l'unico accesso dal pannello). Da tastiera o da Narrator il menu si apre con la prima voce evidenziata; Invio o Spazio tenuti non scelgono più da soli (filtro dei messaggi solo mentre il menu è aperto). Via l'icona dell'ingranaggio non più usata.
- Misure (release, finto HA, `--umore calm`): riposo 5 min **0,015%** (max 0,33), **8,0–8,2 MB** (14h: 0,015%, 8,0–8,1); raffica anti-glitch 1199 fotogrammi **0 difettosi**, controllo negativo 299 su 299.

---

## [2.1.2] - 2026-03-24

### Fixed
- **App non si avvia se le notifiche push erano registrate** — `tokio::spawn` nel callback `setup()` di Tauri causava panic "no reactor running" perché il main thread non è nel contesto del runtime Tokio. Sostituito con `tauri::async_runtime::spawn`. Il crash si verificava solo con una registrazione `notifRegistration` presente nello store.
- **Webhook server non arrestabile** — il server HTTP per le notifiche push (Axum, porta 7421) veniva avviato senza conservare il `JoinHandle`. Chiamate successive a `notifications_register` lasciavano il vecchio server attivo; il nuovo `bind()` falliva silenziosamente. Fix: `webhook_handle` in `AppState`, vecchio server abortito prima di avviarne uno nuovo.
- **`notifications_unregister` non arrestava il server** — la de-registrazione cancellava il record nello store JSON ma il server HTTP continuava ad ascoltare sulla porta. Fix: `handle.abort()` chiamato su unregister.

---

## [2.1.1] - 2026-03-19

### Fixed
- **Mobile app registration schema validation** — HA `mobile_app/registrations` endpoint requires both `push_token` and `push_url` to be included in the `app_data` inclusion group; registration was failing with 400 Bad Request. Fixed by adding `push_token = device_id` to satisfy the schema validation. Now `notify.mobile_app_itsyhome_<hostname>` service is created correctly in HA automations.

---

## [2.1.0] - 2026-03-19

### Added
- **Push notifications from Home Assistant** — Three notification channels now available:
  1. **WebSocket channel** (sempre attivo): Subscribe to `persistent_notification_created` HA events. Works immediately with any HA automation using `persistent_notification.create` service — no registration required.
  2. **Mobile app registration**: POST to `/api/mobile_app/registrations` creates `notify.mobile_app_itsyhome_<hostname>` service in HA automations, exactly like the official HA companion app. One-click setup from Settings > Notifiche tab.
  3. **Local webhook server** (Axum backend, configurable port default 7421): HA pushes notifications directly to the app when `notify.mobile_app_*` is called in an automation. Requires Windows Firewall rule to allow inbound traffic on the configured port.

- **Notifications Settings tab** — new UI section for notification management:
  - Register/Unregister buttons to manage mobile app registration
  - Port configuration field (default 7421)
  - Status card showing device name, HA service name (`notify.mobile_app_*`), and webhook push URL after registration
  - Test notification button
  - Built-in example HA automation YAML

Example Home Assistant automation using the registered service:
```yaml
alias: Alarm triggered
trigger:
  platform: state
  entity_id: binary_sensor.front_door
  to: 'on'
action:
  - service: notify.mobile_app_itsyhome_MYPC
    data:
      title: "Alarm triggered"
      message: "Motion detected at entrance"
```

---

## [2.0.1] - 2026-03-19

### Fixed
- **App fails on clean installation** — Missing `custom-protocol` Cargo feature meant embedded frontend was not served correctly. `tauri build` adds the feature automatically but direct `cargo build --release` does not. Added `custom-protocol` feature explicitly to `Cargo.toml` so frontend is always served via `tauri://localhost/` regardless of build method.
- **Version number misalignment** — `package.json`, `Cargo.toml`, `tauri.conf.json`, and git tag were not in sync (files at 2.0.0, git tag v2.0.1). All manifests bumped to 2.0.1 for consistency.
- **Build script Node.js PATH** — `build-release.bat` updated with correct Node.js PATH for build pipeline.

---

## [2.0.0] - 2026-03-19 (Patch 2)

### Fixed
- **Camera visibility in popup** — `MenuData.cameras` refactored from `Vec<CameraEntity>` to `Vec<AppEntity>` with `"type":"camera"` in JSON serialization; cameras now render correctly in popup instead of being invisible
- **Entity registry resilience** — `HaEntityEntry.platform` changed from `String` to `Option<String>` to prevent deserialization failure when a single entity has `platform: null` in HA registry
- **Popup positioning drift (follow-up)** — `window_resize` now skips all operations if popup is hidden via `is_visible()` guard, eliminating 30s position drift caused by background JS resize calls; `reposition_from_tray` now uses deterministic `tray_pos` formula instead of reading `outer_pos.x`
- **Drag & drop rewrite** — Complete replacement of HTML5 Drag & Drop API with pointer events (`pointerdown/pointermove/pointerup`) via `setupPointerDrag` helper; WebView2 bug workaround: missing `user-select: none` on `.acc-row` caused text-selection drag to override element drag. Solution: full DnD API bypass with `setPointerCapture`, `elementFromPoint` with `pointerEvents: none`, 5px drag threshold
- **Detail panel background transparency** — Detail panel changed from `rgba(30,30,32,0.92)` (8% transparent) to `rgb(30,30,32)` (opaque); removed `backdrop-filter` from detail panel to prevent blur of internal content; new animation: slide + fade from right (14px translate) with spring easing

### Fixed (Build)
- **Build pipeline order** — `build-release.bat` corrected with proper sequence: 1) Vite build → 2) cargo build → 3) tauri bundle; previous order ran cargo before Vite, causing binary to embed old frontend and fail with "localhost refused connection"

---

## [2.0.0] - 2026-03-17 (Patch)

### Fixed
- **Popup positioning drift** — popup no longer shifts right on repeated tray-icon clicks. Root cause: `window_resize` was calling `set_position` on the hidden window while JS continued running in background; `show_popup` now uses known logical width (300px × DPI scale) instead of `outer_size()` for stable X calculation.
- **Drag & drop areas in Settings** — dragstart guard `closest('.acc-drag')` incorrectly blocked all area drags (e.target is always the header div, never the span); removed the guard.
- **Drag & drop devices** — added `dragover` fallback on the entityList container so drops work even in gaps between rows.
- **Camera visibility** — cameras with `entity_category` set in HA registry were silently filtered; bypass this filter for the camera domain so all cameras always appear.
- **Tray icon right-click menu** — added "Impostazioni" and "Esci da ItsyHome" context menu items (right-click on tray icon).

### Added
- **Area hide/show button** — eye button on each area header in Settings > Accessories allows hiding/showing all entities in an area at once.

---

## [2.0.0] - 2026-03-16 (Initial Release)

### Changed (Breaking)
- **Migrated from Electron 33 to Tauri 2.0** — installer size reduced from ~180 MB to ~8 MB (22x smaller)
- Backend rewritten in **Rust** (tokio async, tokio-tungstenite WebSocket, reqwest HTTP)
- IPC layer migrated from Electron `contextBridge` / `ipcRenderer` to Tauri `invoke` / `listen`
- Bundler changed from `electron-vite` to **Vite 6**
- Persistence migrated from `electron-store` to `tauri-plugin-store`
- Autostart migrated from `app.setLoginItemSettings()` to `tauri-plugin-autostart`

### Added
- **Toast notifications** — visual feedback for every action (success and error)
- **Search / filter** (Ctrl+F or 🔍 button) — find any device in the popup by name
- **Keyboard shortcuts** — Escape (close/back), Ctrl+F (open search)
- **Error reporting** — failed Home Assistant service calls shown as error toast
- Modular frontend architecture: `entities.ts` (per-domain card builders), `sections.ts` (section builders)
- Unified IPC adapter `src/lib/api.ts` — single source of truth for all backend calls

### Performance
| Metric | v1.x (Electron) | v2.0 (Tauri) | Improvement |
|---|---|---|---|
| Installer size | ~180 MB | ~8 MB | 22x smaller |
| RAM (idle) | ~120 MB | ~25 MB | 5x less |
| Cold startup | ~2 s | < 500 ms | 4x faster |

### Removed
- `electron`, `electron-vite`, `electron-builder`, `electron-store` (all replaced by Tauri stack)
- `src/main/` — Electron main process (replaced by `src-tauri/`)
- `src/preload/` — Electron preload scripts (not needed in Tauri)
- `electron.vite.config.ts`, `electron-builder.json`

---

## [1.0.3] - 2025-12-01

### Fixed
- Performance: entity mapping rewritten as O(n) single-pass (was O(n²))
- Memory leak in 2D hue picker canvas — draw functions now stored in `WeakMap`
- `pointercancel` event handled in color picker drag to prevent stuck state

---

## [1.0.2] - 2025-11-15

### Added
- 2D canvas color picker (drag-and-drop, X = hue, Y = saturation/brightness)
- Popup UI redesign with smooth animations (140 ms spring easing)
- Detail side panel for per-room entity controls (slides in from right)
- Dark / light mode auto-detection via `prefers-color-scheme`

---

## [1.0.1] - 2025-11-01

### Added
- Multi-environment support (multiple Home Assistant instances)
- Drag-and-drop reordering of rooms and devices in Settings
- Eye toggle (show/hide) per individual entity
- One-time migration from legacy single `haUrl`/`haToken` to environment list

---

## [1.0.0] - 2025-10-15

### Added
- Initial release — port of ItsyHome macOS (Swift/SwiftUI) to Windows via Electron
- System tray with connected / connecting / error icon states
- Popup menu with Favorites, Rooms (collapsible), Scenes, Cameras sections
- Real-time WebSocket connection to Home Assistant with exponential-backoff reconnect
- 12 entity domains: light, switch, climate, cover, lock, fan, humidifier, valve, sensor, alarm, camera, scene
- Notification badge and update badge in tray and popup header
- Settings window: HA connection, accessories ordering, cameras toggle, launch at login
- Persistent notifications via `persistent_notification/get` HA WS API
- Ping/pong heartbeat (30 s interval, 10 s timeout)
- Single-instance lock
- NSIS installer for Windows x64
