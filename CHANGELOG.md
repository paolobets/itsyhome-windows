# Changelog

All notable changes to ItsyHome are documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased] — v3 «assistente isola» (ramo `v3-assistente`, milestone M1 e M2)

### Changed (rifiniture grafiche dal controllo del 18/11)
- Il «⋯» (che il carattere non ha e appariva come un quadratino) è scritto «…» nei testi delle scorciatoie e di Impostazioni › Sistema.
- Voce espansa: il titolo si legge intero (a capo, fino a 4 righe) e riga sotto e testo scendono. Sotto le voci c'è il nome per l'utente della fonte («PC», «Casa», «Claude Code», «Prova») invece dell'identificativo interno.
- Vista PC: i dettagli di RAM e disco sono più corti e non si tagliano («10,1 / 15,7 GB»).
- Impostazioni: via il titolo ripetuto in testa (si recuperano 46 px); stati vuoti di Ambienti e Collegamenti col pulsante sotto il messaggio, al centro, come nel pannello; istruzioni di Dispositivi solo con l'elenco pieno; note «solo su Windows» solo fuori da Windows; «Avvisa quando le cuffie…» nella sezione «AVVISI»; il selettore ha una freccia in giù; «Aggiungi un ambiente» / «Aggiungi un collegamento» ovunque.
- «Nega» e «Consenti» sono entrambi neutri (nessuna tinta d'azione), nel pannello e nella pillola; fuoco iniziale su «Nega», Ctrl+Invio e armamento di 600 ms invariati.

### Added (M3, Task 7: collegamento sicuro a Claude Code)
- Impostazioni › **Claude** (sezione 4; «Sistema» passa alla 5 e «Informazioni» alla 6): stato («Collegato · porta N», «Non collegato», «Da aggiornare»), «Collega Claude Code» → anteprima in un riquadro mono scorrevole con **solo le voci che si aggiungono** a `settings.json`, il file che si scriverebbe, gli avvisi (barra di stato già usata, `allowedHttpHookUrls`/`httpHookAllowedEnvVars` senza isola, `disableAllHooks` attivo) e «Scrivi in settings.json» / «Annulla». Il fuoco parte da «Annulla»; Esc annulla; **niente si scrive prima di «Scrivi»**. «Rigenera il codice» e «Scollega» (con conferma) quando è collegato. Nota fissa: con isola chiusa Claude Code segnala «hook failed» a fine risposta, è innocuo.
- Voci installate: `http` verso `127.0.0.1:<porta>/isola/v1/evento` su `UserPromptSubmit`, `PreToolUse`, `PermissionRequest` (timeout 120), `Notification`, `Stop`, `StopFailure`, `SessionEnd` (timeout 5), senza `matcher` e senza `onFailure`; nessun `SessionStart` né `PostToolUse`. **Il codice non entra mai nel file**: l'header è `Bearer $ISOLA_HOOK_TOKEN` con `allowedEnvVars`; il codice sta nel portachiavi e nella variabile d'ambiente dell'utente (`HKCU\Environment`, solo quel ramo; avviso `WM_SETTINGCHANGE` su un thread a parte). I terminali aperti prima vanno chiusi e riaperti. Fuori da Windows la variabile la imposta l'utente («Imposta ISOLA_HOOK_TOKEN nella tua shell», «Copia il codice»).
- Il file si cambia con garanzie: `settings.json` illeggibile (commenti, virgole finali, non un oggetto, `hooks` malformato) non si riscrive mai; ordine e contenuto altrui conservati (`serde_json` con `preserve_order`); copia di riserva `settings.json.isola-bak`, scrittura su `.tmp` e `rename`; `unisci` due volte = una, `togli(unisci(v)) == v`. La barra di stato si configura solo se manca o è nostra (percorso dell'exe con `/`; con spazi o caratteri della shell, il percorso corto 8.3, altrimenti nessuna barra). La versione delle voci si legge dal percorso `/isola/v1/…` (una sola fonte).
- All'avvio, collegato ma con `settings.json` che non dice più porta, versione o comando della barra: voce informativa «Il collegamento con Claude Code va aggiornato» col pulsante «Impostazioni» (apre la sezione Claude). **Mai scritture da sole.** In prova (`ITSYHOME_ASSISTENTE_DIR`) il file è `<cartella>\claude-finto\settings.json`, la variabile sta in `HKCU\Software\ItsyHomeProva\Environment` e il portachiavi vero non si tocca. Rigenerare il codice riavvia l'ascolto (i permessi in attesa si chiudono come già previsto).
- Prova dal vivo (modalità di prova, file e chiave di registro finti): anteprima senza scritture, collega, riserva identica all'originale, nessun `.tmp`, codice assente dal file, ascolto sulla porta, codice nuovo accettato e vecchio rifiutato (401) dopo «Rigenera», «Scollega» riporta il file al contenuto di partenza e toglie variabile e ascolto. Trovato e corretto dalla prova: togliere la variabile apriva la chiave in sola lettura («Accesso negato»).

### Added (M3, Task 6: barra di stato e limiti dell'abbonamento)
- `assistente --barra-stato <porta>` è il comando della barra di stato di Claude Code: gira prima di istanza unica, Slint, configurazione e registro; legge al più 64 KB di stdin, stampa «Opus 5.5 · contesto 42% · 5h 24% (↻ 14:30) · 7g 61%» (parti mancanti omesse, limite già azzerato nascosto, senza nulla «Claude») e poi, se `ISOLA_HOOK_TOKEN` c'è, inoltra all'assistente solo `DatiBarra` (sessione, modello, contesto, due limiti): mai cartella, trascrizione, workspace né il codice nella riga. Connessione con scadenza 30 ms, 40 ms in tutto.
- L'assistente riconvalida il corpo sul thread della connessione (stesse regole di `session_id`, percentuali 0..=100, nome del modello ripulito) e tiene i limiti dell'account una finestra alla volta: una barra senza `rate_limits` non cancella quelli noti; un limite scaduto vale come assente (`Ponte::limiti`), senza abbonamento niente anelli. Il contesto è per sessione, al più 16, tolto con la sessione.
- Misura (build release, Windows, 50 esecuzioni, stdin da file di prova con valori finti, via `cmd /c`): mediana 152 ms con porta 1 senza codice, 200 ms con codice (la connessione scade a 30 ms), 160 ms contro un ascoltatore proprio; lo stesso giro con `hostname.exe` dà 75 ms. Il tempo è quasi tutto l'avvio dell'eseguibile (25 MB), non la logica: **la mediana NON rientra nei 50 ms previsti**, Paolo ha accettato la misura (18/11), nessun binario separato.

### Added (M3, Task 5: permessi di Claude Code dalla pillola)
- Un permesso chiesto da Claude Code diventa una voce *bloccante* «Claude chiede: <strumento> <riassunto>» (riassunto ripulito da caratteri invisibili e di direzione, titolo al più 60 caratteri, dettaglio intero con la sessione) con «Nega» e «Consenti». «Consenti» è un pulsante delicato: nel pannello solo Ctrl+Invio o clic, dalla pillola solo se il testo si legge tutto (altrimenti «Apri»). Nessun «Consenti sempre», nessun `updatedPermissions` né `updatedInput`.
- La decisione parte solo da un gesto sulla voce e va solo alla sessione che ha chiesto; un clic tardivo non fa nulla. Il permesso si chiude da sé (senza decisione, voce tolta) quando la stessa sessione manda `PreToolUse`, `UserPromptSubmit`, `Stop`, `StopFailure` o `SessionEnd` (risposta data nel terminale), o quando l'ascoltatore lo abbandona; non con la `Notification` `permission_prompt`, che Claude manda a permesso aperto. Al più 16 permessi in attesa: il 17° ripiega sul terminale. Con un permesso aperto la sessione non mostra anche «Claude ti aspetta».
- Fra più bloccanti la pillola chiama per il più vecchio e aggiunge « +N» (fuori dal testo tagliato, letto anche da Narrator); il pannello resta nell'ordine solito. `PERMESSI_RISOLVIBILI` (esito dello spike) seleziona il ramo B, solo «Apri il terminale».

### Added (M3, Task 4: sessioni di Claude Code nella pillola)
- Se Claude Code è collegato (`claude.collegato` nella configurazione, con porta e codice) l'assistente ascolta gli hook all'avvio; il codice sta nel portachiavi (`isola:claude-hook`), in prova nella variabile `ISOLA_HOOK_TOKEN` del processo. Porta occupata: voce *attenzione* «Collegamento con Claude Code non disponibile» con «Impostazioni».
- Voci «In attesa» per sessione, con id che non contengono il `session_id` (impronta FNV-1a): «Claude ti aspetta · <cartella>» e «Claude si è fermato · <cartella>» (*attenzione*, errore dell'API in italiano), «Claude ha finito · <cartella>» (*informazione*, sparisce chiudendo il pannello). Una sessione sparisce con `SessionEnd`, dopo 30 minuti di silenzio (2 ore se aspetta) o quando se ne superano 16.
- Occhi della pillola del colore dello stato più urgente (attenzione o informazione). Pillola media «attività dal vivo» «Claude · Modifica · 3:12» (o «Claude ×2 · 3:12»), larga 210 px, a riposo e col puntino; i lati si nascondono. Per Narrator: «ItsyHome, Claude, Modifica, 3 minuti».
- Nessun timer a riposo: una sveglia singola alla prossima scadenza di una sessione; il timer da un secondo del testo di attività gira solo con almeno una sessione che lavora, la pillola visibile e l'umore diverso da «dorme».
- `casa-core`: `Config::claude` (`ConfigClaude`: collegamento, porta, soglie, avvisi già dati); un file di M2 si legge scollegato. Esempio `sessioni_finte` di `claude-hook` per prove e misure.

### Added (M3, Task 3: eventi e stato delle sessioni)
- `claude-hook`: lettura pura degli eventi degli hook (`evento_da_json`) con i soli campi `session_id` (1..=128 caratteri `[A-Za-z0-9_-]`, stesso controllo dell'ascoltatore), `hook_event_name`, `cwd` (ridotto all'ultimo nome, 60 caratteri), `tool_name` (40), `notification_type`, `error`; mai prompt, trascrizione o risposta. `tool_input` si legge solo per `PermissionRequest` e solo per riassumere (`command` per `Bash` e `PowerShell`; `file_path` per `Edit`, `MultiEdit`, `Write`, `Read`, `NotebookEdit`; `url`; `query`), al più 2000 caratteri.
- `Sessioni`: macchina a stati per sessione (Lavora, TiAspetta, Fatto, Errore) con ora passata dal chiamante; elenco per urgenza, scadenza dopo 30 minuti di silenzio (2 ore per chi aspetta una risposta), al più 16 sessioni (si toglie la meno recente fra quelle che non aspettano; `ricevi` restituisce anche il `Tolta` relativo). Una `Notification` `permission_prompt` dopo una `PermissionRequest` lascia la sessione in TiAspetta.

### Added (M3, Task 2: ascoltatore locale blindato)
- `claude-hook`: ascoltatore HTTP degli hook di Claude Code, solo su 127.0.0.1 e su una porta precisa (porta occupata = errore esplicito, mai un'altra porta). Codice segreto di 32 byte (`Authorization: Bearer`, confronto a tempo costante, `Debug` senza il codice). Rifiuti prima di leggere il corpo, nell'ordine: oltre 16 connessioni `503`; intestazioni oltre 8 KB o 32 righe `431`; lettura oltre 2 s chiusa; metodo diverso da `POST` `405` (nessuna intestazione CORS); `Origin` `403`; `Host` diverso da `127.0.0.1:<porta>` `403`; `Content-Type` diverso da `application/json` `415`; codice errato `401`; `Transfer-Encoding` o `Content-Length` assente `411`; corpo oltre 256 KB letto e scartato (fino a 1 MiB) poi `413`; percorso sconosciuto `404`; JSON illeggibile `400`. Ogni rifiuto è contato (`Ascoltatore::rifiutate`) e non arriva mai al gestore.
- Eventi: risposta `200 {}` e solo dopo il gestore. `PermissionRequest`: attesa della decisione (115 s al massimo) con controllo ogni 250 ms che il mittente non abbia chiuso; risposta `allow` o `deny` nel formato di Claude Code, `{}` se scaduta o senza decisione.
- Il PID del mittente (tabella TCP di Windows, spostata dall'esempio nella libreria) si cerca una sola volta per `session_id` nuovo (ricordati al più 16), prima di rispondere.
- `casa-core`: contatori diagnostici `claude_eventi`, `claude_rifiutate`, `claude_permessi`, `claude_barre`.

### Added (M3, Task 1: spike del permesso con hook HTTP)
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
