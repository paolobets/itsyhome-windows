# Assistente "isola" — M3 Claude (Claude Code + API) — Design

Data: 17/11/2026. Decisa con Paolo nel brainstorming del 17/11. Si appoggia alla spec generale
`docs/superpowers/specs/2026-11-13-assistente-isola-design.md` (livelli d'uso, «In attesa», budget di leggerezza §4,
sicurezza §7) e all'analisi di TwoCode `.superpowers/ux/twocode-funzioni.md` (§1.7, righe 10–14 della tabella).

## 1. Perché

Paolo lavora con Claude Code **da terminale, fino a 3 sessioni insieme**, e usa anche le **API Claude** a consumo.
Oggi per sapere se una sessione ha finito o lo aspetta deve girare tra i terminali, e per sapere quanto spende o
quanto limite gli resta deve aprire la Console o claude.ai. L'assistente deve dirglielo da solo, senza appesantire
il PC, e — richiesta esplicita di Paolo — **in modo sicuro**: «di questi tempi la sicurezza è fondamentale».
Deve essere **migliore di TwoCode**, che fa la stessa cosa ma scrive da solo nelle impostazioni di Claude e non
guarda le API.

**Risposte di Paolo dopo la revisione del costruttore (17/11)**
- È **amministratore di un'organizzazione nella Console Anthropic**: la chiave da amministratore si può creare.
- Claude Code gira **solo su Windows** (Windows Terminal / PowerShell). WSL è fuori.
- **«Per me, pronto per altri»**: si costruisce per Paolo, ma senza scorciatoie che impediscano di darlo agli
  utenti di ItsyHome dopo. Quindi si gestiscono comunque: utente senza abbonamento (nessun `rate_limits` → anelli
  nascosti, non «0%»), utente senza chiave da amministratore (parte API spenta, nessun errore), barra di stato
  già occupata (§6.1), percorso dell'eseguibile che cambia dopo un aggiornamento (all'avvio l'assistente controlla
  che il comando in `settings.json` punti a sé e, se no, propone di aggiornarlo). La guida per terzi e la licenza
  restano fuori da M3.

## 2. Obiettivi, non obiettivi, anti-obiettivi

**Obiettivi**
1. Stato delle sessioni di Claude Code (fino a 3, ma senza un tetto rigido): lavora / ti aspetta / ha finito / errore.
2. Richiesta di permesso risolvibile nella pillola (Nega / Consenti), con il terminale sempre usabile.
3. Limiti dell'abbonamento: 5 ore e settimanale (percentuale e ora di azzeramento), con avviso a soglia.
4. Clic su «fatto» → porta davanti il terminale di quella sessione (al meglio possibile, vedi §6.6).
5. Consumi delle API Claude e dati amministrativi dell'organizzazione: spesa, token, chiamate se disponibili,
   chiavi API, workspace, membri, limiti di velocità. **Solo lettura** (scelta di Paolo).

**Non obiettivi**
- Nessuna chat con Claude dentro l'assistente; nessuna chiamata ai modelli (Messages API).
- Nessuna modifica all'organizzazione Anthropic (creare/revocare chiavi, membri, workspace): per quello c'è il
  pulsante «Apri la Console».
- Niente sessioni web (claude.ai/code) o app desktop: gli hook valgono solo per Claude Code locale.
- Niente lettura dei file di registro di Claude Code (`~/.claude/projects/*.jsonl`): formato non documentato.

**Anti-obiettivi** (se succede, abbiamo fallito anche se «funziona»)
- Claude Code rallentato, bloccato o in errore perché l'assistente è chiuso, piantato o lento.
- Un permesso concesso senza un gesto esplicito di Paolo.
- Segreti (codice degli hook, chiave da amministratore) in chiaro su disco, nei log, in chat o nel repo.
- Contenuti delle sessioni (prompt, comandi, file) scritti su disco o tenuti in memoria più del necessario.
- L'assistente che diventa una porta d'ingresso: qualcosa dalla rete o da una pagina web che gli fa fare cose.
- Budget di leggerezza (§4 della spec generale) superato a riposo.

## 3. Approcci considerati

> *Superato dal §10 (18/11): l'approccio 1 (hook HTTP su 127.0.0.1) non è mai stato rilasciato; ora si usa l'aiutante `isola-hook.exe` (hook `command`) e un tubo con nome riservato all'utente. Il «✔» di questa tabella è storico.*

| | Approccio | Pro | Contro |
|---|---|---|---|
| **1 ✔** | **Hook HTTP locali** (`"type": "http"`) verso un piccolo ascoltatore dell'assistente su 127.0.0.1 + barra di stato che richiama per un attimo l'assistente | Per eventi, nessun processo in più; Claude Code va avanti se l'assistente è spento (errore non bloccante, documentato); è il modo di TwoCode | Serve un ascoltatore locale da blindare |
| 2 | Hook a comando: un eseguibile lanciato a ogni evento | Nessun ascoltatore | Un avvio per ogni strumento usato: con 3 sessioni centinaia all'ora — contro il requisito di leggerezza |
| 3 | Leggere i registri di Claude Code | Non tocca le impostazioni | Non può rispondere ai permessi; formato non documentato, fragile |

Scelto da Paolo: **1**, «in ambito migliorato rispetto a TwoCode», sicuro. Per le API non ci sono alternative serie:
lettura periodica con la chiave da amministratore, solo in lettura (Paolo ha scelto la strada «solo guardare» tra
solo guardare / qualche azione sicura / gestione completa).

## 4. Fatti verificati sulla documentazione (17/11)

> *Superato dal §10 (18/11) per i fatti sull'hook `http`: non lo si usa più (nessun TLS né verifica del server); valgono i fatti sull'hook `command` e sulla risposta di `PermissionRequest`.*

- **Hook HTTP** (`code.claude.com/docs/en/hooks`): `{"type":"http","url":…,"headers":{…},"allowedEnvVars":[…],"timeout":…}`;
  il corpo è il JSON dell'evento in POST. Le intestazioni possono usare `$VAR` solo se elencate in `allowedEnvVars`.
  Timeout predefinito 600 s. **Indirizzo irraggiungibile, risposta non 2xx o timeout = errore non bloccante**
  (l'azione prosegue), salvo `"onFailure": "block"` che **non useremo mai**.
  Per `PermissionRequest` la risposta è `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny"}}}`.
- **Barra di stato** (`code.claude.com/docs/en/statusline`): è un comando, riceve JSON su stdin con
  `session_id`, `model.display_name`, `context_window.used_percentage`,
  `rate_limits.five_hour.used_percentage|resets_at`, `rate_limits.seven_day.used_percentage|resets_at`.
  Non può essere un indirizzo HTTP.
- **Consumi API** (Usage and Cost Analytics API, beta): `/v1/organizations/analytics/usage` e `/cost`, chiave da
  amministratore (`sk-ant-admin…`), dati con **4–24 ore di ritardo**, al massimo 31 giorni per richiesta.
  Altri endpoint amministrativi in sola lettura: chiavi API, workspace, membri, report dei limiti di velocità.
- **Correzioni dalla stesura del piano (17/11):** `SessionStart` non accetta hook `http` → non ci si iscrive,
  la sessione compare al primo evento; l'elenco delle chiavi API **non ha «ultimo uso»** → al suo posto «chiamate
  del mese» per chiave (l'API analytics dà `requests`); la chiave da amministratore della Console **non ha
  permessi ridotti** → la difesa resta l'elenco chiuso di letture, e la guida consiglia una chiave con scadenza.
- Le impostazioni di Claude Code di Paolo oggi **non hanno né hook né barra di stato** (verificato 17/11, solo le chiavi).

## 5. Modello delle minacce (cosa proteggiamo, da chi)

> *Superato dal §10 (18/11) per tutte le righe sul trasporto HTTP (porta, codice segreto, `Origin`/`Host`, porta occupata); le minacce vigenti sono nella tabella «Minacce aggiornate» del §10. Restano valide le righe su permessi, contenuti, chiave da amministratore e malware locale.*

| Minaccia | Difesa |
|---|---|
| Altre macchine in rete | Ascolto **solo su 127.0.0.1**, mai 0.0.0.0 né IPv6 globale |
| Pagina web nel browser che chiama 127.0.0.1 (CSRF, DNS rebinding) | Codice segreto obbligatorio in `Authorization` (il browser non può aggiungerlo cross-origin senza preflight); rifiuto di `OPTIONS` e di qualunque richiesta con intestazione `Origin`; `Host` deve essere esattamente `127.0.0.1:<porta>`; solo `POST` con `Content-Type: application/json` |
| Programma locale che occupa la porta prima dell'assistente e riceve gli eventi | All'avvio, se la porta è occupata da altri, l'assistente **non** usa un'altra porta in silenzio: mostra un avviso *attenzione* «Collegamento con Claude Code non disponibile» e il collegamento resta spento. Il codice segreto è solo un permesso a mandare eventi all'assistente: non apre altro |
| Evento falso che mostra un finto permesso | Serve il codice segreto; in più la risposta «Consenti» va solo alla connessione che ha chiesto, quindi un falso non ottiene nulla su una sessione vera |
| Permesso concesso per sbaglio | Mai con Invio da solo; **Ctrl+Invio**, fuoco iniziale su «Nega» (già nella spec); nessun «Consenti sempre»; la pillola mostra strumento + comando/percorso (riassunto), il testo completo nel pannello; se Paolo non risponde l'assistente **non risponde** e decide il terminale |
| Corpo enorme o malformato | Limite 256 KB per richiesta (oltre: 413, letto e scartato), timeout di lettura 2 s, al massimo 16 connessioni aperte (§6.3); parser JSON che non va mai in panico |
| Fuga di contenuti delle sessioni | Si tengono solo i campi necessari (id sessione, nome della cartella, stato, nome dello strumento, riassunto breve del permesso in attesa); prompt, comandi completi e risposte degli strumenti non si salvano mai, nemmeno nei log. Non ci si iscrive a `PostToolUse` (corpi grandi, non servono). `diagnosi.log` solo contatori |
| Furto della chiave da amministratore | Solo nel **portachiavi del sistema** (`keyring`, come il token HA); campo mascherato nelle Impostazioni, dopo il salvataggio si vedono solo le ultime 4 cifre; mai nei log, mai nei messaggi d'errore; trasmessa solo a `https://api.anthropic.com` con verifica del certificato (rustls) |
| Chiave da amministratore usata per modificare | Il client API dell'assistente ha un **elenco chiuso** di percorsi, tutti `GET`; un test verifica che non esista nessun altro metodo/percorso. Se la Console permette una chiave con permessi ridotti (sola lettura / analytics), la guida dice di usare quella — **da verificare** sulla doc al momento del piano |
| Malware che gira come Paolo | Fuori portata: può già leggere `~/.claude` e il portachiavi dell'utente. Non lo dichiariamo protetto |

> *Superato dal §10 (18/11): non esistono più il codice segreto, la variabile `ISOLA_HOOK_TOKEN`, «Rigenera il codice» né il «compromesso dichiarato» qui sotto; il canale è un tubo riservato all'utente, verificato in entrambi i sensi.*

Il **codice segreto degli hook** (256 bit casuali) sta nel portachiavi; Claude Code lo legge da una variabile
d'ambiente utente (`ISOLA_HOOK_TOKEN`, impostata in HKCU\Environment su Windows) tramite `allowedEnvVars`, così
**non compare in `settings.json`**, che si copia e si mostra facilmente. *(Scelta mia: costa che i terminali aperti
prima dell'installazione vanno riaperti; la guida lo dice.)* Pulsante «Rigenera il codice» nelle Impostazioni.

**Compromesso dichiarato.** Claude Code deve poter leggere il codice, quindi da qualche parte è in chiaro per
l'utente (la variabile sta nel registro utente ed è ereditata dai processi dei terminali). È accettato perché il
codice dà **un solo potere**: mandare eventi all'assistente, che al massimo mostra un finto avviso (nessun permesso
vero si può concedere così, vedi sopra). L'anti-obiettivo «segreti mai in chiaro» vale pieno per la chiave da
amministratore e per il token di Home Assistant; per il codice degli hook vale «mai in `settings.json`, nei log,
in chat o nel repo». La barra di stato prende il codice dalla stessa variabile.

## 6. Come funziona

### 6.1 Collegamento a Claude Code (installazione)

> *Superato dal §10 (18/11): niente indirizzo `http://127.0.0.1:<porta>/…` né variabile d'ambiente; le voci sono hook `command` che lanciano `isola-hook.exe` e «Scollega» toglie solo le voci di `settings.json`. Resta valido l'anteprima prima di scrivere, la copia di riserva e la scrittura atomica.*
- Impostazioni › nuova sezione **«Claude»** › «Collega Claude Code». Prima di scrivere, l'assistente **mostra
  esattamente cosa aggiunge** a `~/.claude/settings.json` e chiede conferma. Miglioria rispetto a TwoCode, che
  scrive da solo all'avvio.
- Scrittura sicura: copia di riserva (`settings.json.isola-bak`), unione che **conserva tutte le altre chiavi**,
  scrittura atomica (file temporaneo + rinomina). Le voci nostre sono riconoscibili dall'indirizzo
  (`http://127.0.0.1:<porta>/isola/v1/…`) e portano un numero di versione per gli aggiornamenti.
- Se esiste già una barra di stato **non nostra**, non la si tocca: i limiti restano non disponibili e lo si dice.
- «Scollega Claude Code» toglie solo le voci nostre e la variabile d'ambiente.
- Eventi a cui ci si iscrive (`SessionStart` escluso: non accetta hook `http`, vedi §4): `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`,
  `Notification`, `Stop`, `StopFailure`, `SessionEnd`. Timeout: 5 s per tutti tranne `PermissionRequest`
  (vedi §6.3). Nessun `onFailure`.

### 6.2 Ascoltatore

> *Superato dal §10 (18/11): niente porta fissa né `accept` su TCP; un thread attende sul tubo con nome (`WaitForMultipleObjects`), solo con il collegamento attivo.*
- Un thread che dorme su `accept` (nessun polling), avviato solo se il collegamento è attivo. Porta fissa scelta
  all'installazione e salvata in configurazione *(scelta mia: una porta che cambia a ogni avvio costringerebbe a
  riscrivere `settings.json`)*.
- Risponde subito `200 {}` a tutti gli eventi tranne `PermissionRequest`; la logica di stato lavora dopo la
  risposta, così Claude Code non aspetta mai l'interfaccia.

### 6.3 Permessi

> *Superato dal §10 (18/11) solo nel trasporto: «la connessione HTTP» è ora la connessione sul tubo con l'aiutante (attesa dell'aiutante ≤ 115 s, del tubo 2 s meno); niente `503` ma connessione chiusa oltre 16. Il resto (voce bloccante, armamento, chiusura al primo evento) vale.*
- `PermissionRequest` → voce **bloccante** «Claude chiede: <strumento> <riassunto>» con Nega / Consenti
  (pillola larga 440, pulsanti 78×28 come da analisi TwoCode). La pillola esce dalla ritirata.
- La connessione HTTP resta aperta finché Paolo risponde; timeout del hook **120 s** *(scelta mia)*: allo scadere
  l'assistente chiude senza decisione e il permesso segue il flusso normale del terminale.
- Se Paolo risponde nel terminale prima, Claude Code **non chiude** la richiesta: la tiene aperta fino al timeout
  (120 s). La voce sparisce quindi al **primo evento successivo della stessa sessione** (`PreToolUse`,
  `Notification`, `Stop`…) o alla chiusura della connessione, quale arriva prima; una risposta tardiva
  dell'assistente su quella connessione è ignorata da Claude Code. *(Esito dello spike, sotto.)*
- **Esito dello spike, 18/11/2026** (Task 1, esempio `prova_permesso`, Claude Code in PowerShell su Windows,
  modo «manual»):
  - **P1 ✅** con l'hook in attesa il terminale mostra comunque la sua domanda e accetta la risposta: «vince il primo».
  - **P2 ⚠️** rispondendo nel terminale la connessione dell'hook **resta aperta** fino ai 120 s, poi Claude la
    chiude: la voce si toglie con l'evento successivo della sessione (regola sopra), non con la chiusura.
  - **P3 ✅** «Consenti» dall'ascoltatore → lo strumento parte, il terminale scrive «Allowed by PermissionRequest
    hook» e non mostra la sua domanda.
  - **P4 ✅** «Nega» → lo strumento non parte (file non creato).
  - **P5 ✅** nessuna risposta per 120 s → Claude chiude, la domanda resta nel terminale e vale.
  - **P6 ✅ con un costo**: ascoltatore spento → la domanda di permesso compare subito, senza ritardi; però dopo la
    risposta il terminale mostra «Stop hook failed». Con isola chiusa ogni fine risposta mostra quell'errore:
    è il prezzo degli hook HTTP; per non vederlo si usa «Scollega» (§6.1). Da dire a Paolo.
  - **P7 ✅ parziale** PID del mittente trovato ogni volta, catena `claude.exe ← powershell.exe ← explorer.exe`
    (PowerShell da solo). Windows Terminal non provato: resta il ripiego «ultima finestra di terminale» (PF2).
  - **P8 ✅** nessun `Origin`, `Host = 127.0.0.1:<porta>`, `Content-Type: application/json`, `Authorization`
    corretta. Il caso «variabile non impostata» non è stato provato: il test automatico del Task 2 lo copre.
  - Altri fatti visti: ogni `PermissionRequest` è preceduto da un `PreToolUse` dello stesso strumento; circa 6 s
    dopo arriva una `Notification` (il «ti aspetta»); su Windows i comandi di sola lettura dello strumento
    `PowerShell` partono senza chiedere permesso.
  - **Decisione D1: ramo A** — Nega / Consenti nella pillola (P1 e P5 passano; P2 coperto dalla regola
    dell'evento successivo).
- **Più permessi insieme** (fino a 3 sessioni) *(scelta mia)*: la pillola ne mostra uno alla volta, il più vecchio,
  con «+1»/«+2» accanto; risolto uno compare il successivo. Nel pannello ci sono tutti come voci bloccanti
  separate, ognuna con i suoi pulsanti. Limite di connessioni aperte: **16** (permessi in attesa + eventi + barre
  di stato); oltre, le nuove ricevono subito 503 e Claude Code prosegue da solo.
- Con la modalità automatica di Claude Code (attiva da Paolo) i permessi saranno rari: il valore principale è lo stato.

### 6.4 Stato delle sessioni
- Per sessione (`session_id`): cartella (solo il nome finale), stato, strumento corrente, inizio del lavoro,
  ultimo evento. Stati: *lavora* (prompt inviato / strumento in uso), *ti aspetta* (permesso o `Notification`),
  *fatto* (`Stop`), *errore* (`StopFailure`), chiusa (`SessionEnd` → sparisce).
- Sessione senza eventi da 30 minuti e non «ti aspetta» → sparisce *(scelta mia: terminale chiuso con la croce
  non manda `SessionEnd`)*.
- **Occhi della pillola**: colore dello stato più urgente tra le sessioni (palette esistente dei livelli);
  *fatto* resta finché Paolo non guarda (apre il pannello o clicca), poi sfuma.
- **Pillola media** «attività dal vivo» mentre almeno una sessione lavora: «Claude · Modifica · 3:12»
  (larga ~210). Con più sessioni: il numero («Claude ×2»).
- *ti aspetta* = voce **attenzione** in «In attesa»; *errore* = **attenzione**; *fatto* = **informazione**.

### 6.5 Limiti dell'abbonamento (barra di stato)

> *Superato dal §10 (18/11): la barra di stato è `isola-hook.exe barra`, non più `assistente --barra-stato`; l'aiutante stampa la riga e manda i dati per il tubo.*
- `assistente --barra-stato`: legge stdin (max 64 KB), estrae solo i campi di §4, li manda all'ascoltatore
  (stesso codice segreto, letto dall'ambiente), stampa una riga utile e esce. Deve chiudere in **< 50 ms** e non
  inizializzare né finestre né istanza unica. Se l'assistente è spento stampa comunque la riga.
- Riga stampata *(scelta mia)*: `Opus 5.5 · contesto 42% · 5h 23% (↻ 14:30) · 7g 61%`.
- Anello **«Limite 5 ore»** e **«Limite settimanale»** selezionabili nei quadranti (come W/FV in M2); avviso
  **attenzione** alla soglia (predefinita 80%, cambiabile), una volta per finestra di limite.

### 6.6 Porta davanti il terminale

> *Superato dal §10 (18/11): il PID c'è, è quello dell'aiutante, e il terminale si trova risalendo i processi genitori (`claude.exe` → terminale).*
- Clic su una sessione *fatto* o *ti aspetta* → porta davanti la finestra del terminale. Gli hook HTTP non danno il
  PID: il costruttore trova il modo più affidabile (es. titolo della finestra che Claude Code imposta, cartella
  della sessione, processo `claude` figlio del terminale). Se non è affidabile, si porta davanti l'ultima finestra
  di terminale usata e lo si dichiara. Solo Windows; altrove il clic apre il dettaglio.

### 6.7 Consumi e dati amministrativi delle API
- Impostazioni › Claude › «Chiave da amministratore» (campo mascherato, «Prova» che fa una sola lettura).
- Lettura **ogni 3 ore** mentre l'assistente è acceso, più «Aggiorna» nel dettaglio *(scelta mia: i dati
  arrivano con 4–24 ore di ritardo, leggere più spesso non dà nulla)*. Client HTTPS creato per la lettura e
  rilasciato dopo.
- Dati: spesa del mese e di ieri (per modello, per chiave, per workspace), token (ingresso, uscita, cache),
  **numero di chiamate se l'API lo fornisce** (da verificare; se no si mostra solo ciò che c'è, senza stime),
  chiavi API (nome, stato, ultimo uso — mai il valore), workspace, membri, limiti di velocità.
- Sempre indicato «aggiornato a …» (il `data_refreshed_at` dell'API); un risultato vuoto non è «0 $».
- Avvisi: soglia di spesa mensile scelta da Paolo (nessuna predefinita) → **attenzione** all'80% e al 100%;
  giorno con spesa > 3× la media degli ultimi 7 → **informazione** «spesa insolita» *(scelte mie)*.
- Quadrante **«Spesa API del mese»** selezionabile.

### 6.8 Dove si vede
- **Dock**: compare l'icona **Claude** (prevista dalla spec generale).
- **Vista Claude nel pannello** (800×300): a sinistra le sessioni (fino a 3 righe: cartella, stato, strumento,
  durata; clic = porta davanti), a destra due anelli dei limiti e la spesa API del mese con «aggiornato a».
- **Finestra di gestione** (cornice Slint esistente, nessun contenuto web): dettaglio API — schede Spesa, Token,
  Chiavi, Workspace e membri, Limiti — con «Apri la Console» per ogni modifica.

## 7. Vincoli

> *Superato dal §10 (18/11): l'ascoltatore non è più multipiattaforma né su `accept`/TCP (è un tubo con nome, solo Windows) e non c'è variabile d'ambiente; il resto dei vincoli vale.*

- Budget §4 della spec generale: a riposo l'ascoltatore è un thread fermo su `accept`; **a riposo** nessun timer
  nuovo oltre alla lettura API ogni 3 ore. Timer accesi solo quando c'è qualcosa (precisazione del 17/11 dal
  piano): durata del lavoro al secondo solo mentre una sessione lavora e la pillola o la vista Claude si vedono;
  una sveglia singola per far sparire le sessioni silenziose, solo se ci sono sessioni. Misura con lo script di §9 della spec generale prima della chiusura.
- Rust 1.94 in locale e 1.100 in CI; CI su Windows, macOS, Linux verde (`clippy -D warnings`).
- Ascoltatore, barra di stato e API multipiattaforma dove costa poco; variabile d'ambiente e «porta davanti» solo
  Windows (fuori Windows la guida spiega come impostare la variabile).
- Tutto in italiano. Nessun dato personale (nomi di cartelle reali, email dei membri) nel repo o nei log.
- Prove dal vivo **solo con Paolo**: l'installazione in `settings.json` vero, la chiave vera e i permessi veri
  si provano insieme a lui. Nei test automatici: `settings.json` finto in cartella temporanea, server API finto.
- Verificare sulla doc ufficiale ogni endpoint e campo prima di usarlo.

## 8. Cosa resta al costruttore

- Libreria HTTP per l'ascoltatore (minima, sincrona o su tokio già presente in `casa-core`) e per le API (rustls).
- Come trovare la finestra del terminale (§6.6).
- Struttura dei moduli (`claude-hook` come da §6 della spec generale, più un modulo `api-claude`).
- Disegno fine della vista Claude e delle schede della finestra di gestione, nello stile esistente.

## 9. Scelte mie da controllare (riassunto)

> *Superato dal §10 (18/11): «porta fissa» e «codice segreto via variabile d'ambiente» non esistono più; le altre scelte valgono.*

Porta fissa; codice segreto via variabile d'ambiente invece che in `settings.json`; timeout del permesso 120 s;
sessione silenziosa sparisce dopo 30 min; lettura API ogni 3 ore; soglie spesa 80/100% e «spesa insolita» 3×;
testo della riga nella barra di stato; nessun `PostToolUse`; permessi multipli in coda nella pillola (uno alla
volta, «+N»), tutti nel pannello; limite 16 connessioni; codice degli hook in variabile d'ambiente come
compromesso dichiarato (§5).

## 10. Revisione del 18/11/2026: canale protetto (sostituisce il trasporto HTTP)

**Perché.** Una revisione di sicurezza sul Task 7, verificata sulla doc ufficiale degli hook, ha trovato un buco
di progetto. La risposta di un hook `http` è letta con lo stesso schema dei `command`: chi risponde può dire
`permissionDecision: "allow"` a un `PreToolUse`, `allow` a un `PermissionRequest`, `decision: "block"` con un
motivo a uno `Stop` o `additionalContext` a un `UserPromptSubmit` (istruzioni nascoste a Claude). Gli hook HTTP non
hanno TLS né verifica del server. Se isola è chiusa o bloccata, **qualunque processo locale** (anche di un altro
utente) può mettersi in ascolto su `127.0.0.1:<porta>`, ricevere il codice in chiaro e rispondere al posto di
isola; con isola accesa, su Windows una porta senza `SO_EXCLUSIVEADDRUSE` si può perfino rubare. Il §5 copriva
«un programma che occupa la porta» solo dal lato di isola, non dal lato di Claude Code. **Paolo ha scelto il
canale protetto** (18/11).

**Come funziona ora.**
- **Aiutante `isola-hook.exe`**: un piccolo eseguibile (binario del crate `claude-hook`, senza Slint, ~1 MB)
  accanto a `assistente.exe`. Claude Code lo lancia come hook `command` in **forma exec**
  (`"command": "<percorso>\isola-hook.exe", "args": ["evento"]`): nessuna shell, nessun problema di virgolette o
  spazi nel percorso.
- **Tubo con nome di Windows** `\.\pipe\isola-claude-<SID dell'utente>`, creato da isola con:
  descrittore di sicurezza che ammette **solo l'utente corrente**; `FILE_FLAG_FIRST_PIPE_INSTANCE` (se il nome è
  già preso da altri, isola **non** si collega e mostra *attenzione* «Collegamento con Claude Code non
  disponibile: il canale è occupato da un altro programma»); `PIPE_REJECT_REMOTE_CLIENTS`; al più 16 istanze.
- **L'aiutante verifica il server prima di mandare qualsiasi cosa**: `GetNamedPipeServerProcessId` →
  percorso dell'immagine del processo (`QueryFullProcessImageNameW`) che deve essere **esattamente**
  `assistente.exe` nella stessa cartella dell'aiutante (confronto su percorsi canonici, senza distinzione di
  maiuscole). Se il tubo manca o il server non è isola, l'aiutante **esce in silenzio** (codice 0, nessun output):
  Claude Code prosegue come se l'hook non ci fosse e i permessi li decide il terminale.
- **Isola verifica il cliente** allo stesso modo (`GetNamedPipeClientProcessId` → deve essere l'aiutante della
  stessa cartella): non è una difesa contro malware dell'utente (fuori portata, §5), ma toglie di mezzo
  i falsi eventi di programmi qualunque.
- **Minimizzazione nell'aiutante**: legge al più 256 KB da stdin, estrae solo i campi di §6.4/§6.3
  (`session_id` validato, `hook_event_name`, ultimo nome della cartella, `tool_name`, `notification_type`,
  `error`, riassunto del permesso ≤ 2000 caratteri) e manda **solo quelli** (≤ 64 KB). Isola li riconvalida.
- **Uscita dell'aiutante**: **solo** per `PermissionRequest`, e solo con una risposta di isola, stampa esattamente
  `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny",…}}}`. Per
  ogni altro evento non stampa nulla: nel codice dell'aiutante non esiste un modo di emettere
  `permissionDecision`, `additionalContext` o `decision: "block"`.
- **Eventi**: `PreToolUse` e `PermissionRequest` **sincroni** (il `PreToolUse` deve arrivare prima del suo
  permesso, per la regola di chiusura di §6.3; l'aiutante non stampa nulla ed esce entro 300 ms);
  `PermissionRequest` con `timeout: 120` e attesa dell'aiutante ≤ 115 s; `UserPromptSubmit`, `Notification`,
  `Stop`, `StopFailure`, `SessionEnd` con **`"async": true`** (Claude Code non aspetta e non legge l'output).
- **Barra di stato**: `statusLine` = `<percorso corto>/isola-hook.exe barra` (forma shell: serve il percorso 8.3
  se ci sono spazi, D5); l'aiutante stampa la riga e manda `DatiBarra` per il tubo. Il ramo
  `assistente --barra-stato` del Task 6 si toglie.
- **Si tolgono**: il codice degli hook, la voce `isola:claude-hook` del portachiavi, la variabile
  `ISOLA_HOOK_TOKEN` in `HKCU\Environment` (e il «compromesso dichiarato» di §5), «Rigenera il codice», la porta
  in configurazione, l'ascoltatore TCP. Restano: tratto `Gestore`, limiti (16 istanze, 2 s di lettura), numero di
  sequenza all'accettazione, sessioni, permessi nella pillola con tutte le loro protezioni.
- **«Porta davanti» (§6.6)**: il PID del cliente è l'aiutante; il terminale si trova risalendo i genitori
  (aiutante → `claude.exe` → terminale), senza tabella TCP.
- **Fuori da Windows**: niente tubo; la sezione Claude dice «Disponibile solo su Windows». La CI compila gli stub.

**Minacce aggiornate (sostituiscono le righe HTTP di §5).**

| Minaccia | Difesa |
|---|---|
| Altre macchine in rete | `PIPE_REJECT_REMOTE_CLIENTS`; nessuna porta di rete aperta |
| Pagina web nel browser | Un browser non può aprire un tubo con nome: superficie eliminata |
| Programma che si finge isola (isola chiusa, o tubo creato prima) | L'aiutante verifica il percorso dell'eseguibile del server; se non è `assistente.exe` accanto a lui non manda nulla e non stampa nulla |
| Programma che si finge Claude Code (falsi eventi) | Tubo riservato all'utente; isola verifica che il cliente sia l'aiutante della sua cartella; dati riconvalidati |
| Risposta che comanda Claude | L'aiutante sa emettere solo la decisione di un `PermissionRequest`, solo su risposta di isola verificata; gli eventi asincroni non hanno output letto |
| Malware che gira come Paolo | Fuori portata (invariato) |

**Da verificare dal vivo con Paolo** (come lo spike del 18/11): forma exec e `async` su Windows; cosa succede al
processo dell'aiutante se Paolo risponde nel terminale (ucciso? → isola vede la chiusura del tubo); tempi
dell'aiutante (obiettivo: mediana < 30 ms per `PreToolUse`, < 50 ms per la barra).
