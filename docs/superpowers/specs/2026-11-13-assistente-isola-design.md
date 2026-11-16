# Assistente "isola" — documento di design

Data: 13/11/2026 · Stato: bozza da approvare · Autore delle decisioni: Paolo (dove non indicato diversamente)

## 1. Cosa è e perché

ItsyHome oggi è un client di Home Assistant nella system tray di Windows (Tauri 2, v2.1.2).
Paolo vuole trasformarlo in un **assistente del PC**, sempre acceso, che unisca:

- il controllo e le informazioni della **casa** (tutto quello che ItsyHome fa già);
- la chat con **HIRIS** (il suo assistente di casa, progetto proprietario separato);
- le **statistiche del PC** su cui gira;
- il **monitoraggio di Claude Code** (stato, richieste di permesso);
- **mail**, **revisione documenti**, **focus e blocco app**.

L'ispirazione per comportamento e grafica è TwoCode (app Electron closed source, analizzata il 13/11):
**si prendono solo idee, nessun codice** (non è forkabile).

Perché: un unico compagno sul desktop invece di tante app, che si fa notare solo quando serve.

## 2. Obiettivi, non-obiettivi, anti-obiettivi

**Obiettivi**
- Un assistente con un **personaggio** (non una barra informativa).
- Multipiattaforma: Windows, macOS, Linux, scritto in **Rust**.
- **Leggerezza come requisito fondamentale** (vedi §4).
- Tutto in **un unico pannello**: nessuna finestra esterna per le funzioni.

**Non-obiettivi (per ora)**
- Come si apre e come è fatta ogni singola app (casa, HIRIS, mail, …): **fase successiva**, con un brainstorming per app.
- Confronto dettagliato con ItsyHome per macOS: fase successiva.
- Azioni autonome sul PC tipo "Robo, do it" di TwoCode: escluse.

**Anti-obiettivi (sarebbe un fallimento anche se funziona)**
- Un'app che pesa sul PC quando non la si usa.
- Un pannello pieno di colore e icone pesanti: deve restare sobrio, stile iOS/macOS.
- Occhi o personaggio inquietanti (bocciati: occhi col logo HIRIS).

## 3. Il personaggio e la grafica (deciso con le bozze)

Bozze approvate in `.superpowers/brainstorm/…/content/` (non versionate). Riferimento principale: `aperto-dock.html` e `isola-volto-aura.html`.

**Tre momenti**
1. **Pillola piccola** (in alto al centro dello schermo, **attaccata al bordo alto** come il notch di macOS, forma invariata): l'isola *è* la faccia. Pillola nera, due occhi di luce, aura con i colori dei petali HIRIS che scorre lungo il bordo.
2. **Si allarga quando ti cerca** (notifica, richiesta di Claude Code, avviso casa).
3. **Pannello aperto**.

**Umori** (5): tranquillo (aura ferma, solo battito di ciglia), pensa (aura che ruota), ti cerca (aura ambra-rosa che pulsa), contento (occhi ad arco), dorme (luce bassa, occhi a trattino).

**Pannello aperto**
- In alto: occhi al centro, **solo l'ora a destra**, a sinistra niente.
- Misure: **800 × 300** logici, angoli arrotondati, attaccato al bordo alto (deciso da Paolo il 13/11 dopo tre anteprime).
- Centro: le informazioni; per la Panoramica **2 layout** cambiabili al volo (Orologio, Quadranti) — vedi §3bis, che sostituisce i 4 layout iniziali.
- In basso: **dock di sole icone** (solo app) centrato dentro una capsula sottile; la scheda selezionata ha una capsula "vetro" leggera.
- Icone **leggere stile SF Symbols**, monocromatiche (`rgba(236,230,250,.62)`, tratto ~1,55); **il colore solo per lo stato** (regola 60-30-10). Niente piastrelle liquid glass.

**Brand HIRIS**: colori `#c084fc #818cf8 #60a5fa #22d3ee #2dd4bf #e879f9`, fondo `#2a0a4e → #0a0015`, font Inter Tight + JetBrains Mono.

## 3bis. Interfaccia: come si mostrano, si raggiungono e si usano le informazioni

Decisa con Paolo il 13/11 sera, partendo dall'analisi di uno specialista UI/UX (`.superpowers/ux/analisi-architettura-informazioni.md`, riferimenti: TwoCode e gli obiettivi di §2) e dalla bozza `.superpowers/ux/bozza-in-attesa.html` (approvata). Elenco delle decisioni: `.superpowers/ux/decisioni.md`.

**Tre livelli d'uso**
1. **Pillola**: ti dice *che* qualcosa ti cerca e *chi*. Quando ti chiama si allarga con **icona della fonte + una riga + al massimo 2 pulsanti** (es. «Claude chiede: eliminare dist · Nega / Consenti»): se basta un sì/no si risolve lì.
2. **Pannello 800 × 300**: guardare e azioni rapide. In cima, quando c'è qualcosa, la riga **«In attesa»**: le voci di tutte le app con **3 livelli uguali per tutte**: *bloccante* (rosso: aspetta te), *attenzione* (ambra: guarda presto), *informazione* (blu: quando vuoi). Clic su una voce = l'app giusta già su quella cosa; clic sulla pillola che chiama = pannello aperto su quella voce. Riga assente se vuota.
3. **Finestra di gestione**: per lavorare a fondo: **Mail, Documenti, dettaglio Claude Code** (e, come opzione «Apri in finestra», la chat HIRIS lunga). Flusso: avviso → pannello con riassunto e azioni rapide → «Apri» → finestra; chiusa la finestra si torna alla pillola. Stesso stile dell'assistente. Questo rivede la regola iniziale: *nessuna finestra esterna per guardare e per le azioni rapide*; la finestra esiste solo per la gestione approfondita.

**Pannello**
- **Panoramica**: riga «In attesa» + layout **Orologio** o **Quadranti**, cambiabili al volo con **puntini** sotto il contenuto (clic, frecce, rotella); i puntini ci sono solo nella Panoramica. Escono i layout Colori e Tutte le app.
- **Dock: solo app, solo icone** (Panoramica, Casa, Claude, HIRIS, PC, Mail, Documenti, Focus, Impostazioni; ognuna compare quando la sua milestone esiste).
- **Riga alta**: occhi al centro, ora a destra. Nessun «‹ Indietro» (decisione di Paolo, Task 14g): dentro un'app si torna alla Panoramica dal dock, con Esc (un livello alla volta, poi chiude il pannello), Backspace o Alt+←; nella vista Casa «★ Preferiti» è la prima voce dell'elenco delle stanze.
- **Chat HIRIS dentro il pannello** per domande e comandi veloci.
- Pannello **fisso a 800 × 300**: niente allungamento.
- **Impostazioni in una finestra dedicata** (decisa da Paolo il 13/11): finestra Slint nello stesso processo, senza motore web, creata all'apertura e distrutta alla chiusura; non si chiude perdendo il fuoco (serve per incollare il token preso dal browser). Si apre dall'icona del dock (il pannello si chiude) e dalla tray.

**Agire senza errori**
- Permessi di Claude: mai con Invio da solo; da tastiera **Ctrl+Invio**, fuoco iniziale su «Nega».
- Azioni delicate (allarme, serrature): conferma sul pulsante stesso per 4 s.
- Senza connessione a Home Assistant i preferiti diventano **tenui e non cliccabili**.
- Le **scene** si possono mettere tra i preferiti.

**Personaggio (ritocchi approvati)**
- Aura **quasi spenta a riposo**; si accende solo quando significa qualcosa.
- Un *bloccante* si distingue per **forma**, non solo per colore: pillola larga e ferma, bordo più spesso; un avviso normale si allarga per un po' e poi lascia un puntino.
- In «dorme» nessun ridisegno.
- **Modalità «ritirata»**: clic destro (o scorciatoia) → la pillola diventa una linea sottile e i clic passano alle finestre sotto; torna da sola per un *bloccante*.

**Precisazioni dopo la revisione del costruttore (13/11 sera)** *(tutte scelte mie, seguendo l'indicazione di Paolo «seguiamo lo specialista»)*
- **Comportamento della pillola per livello** = tabella §2.6 dell'analisi:
  - *bloccante*: pillola larga **ferma** con bordo più spesso, icona + riga + fino a 2 pulsanti, resta finché non decidi; notifica di sistema sì; esce dalla «ritirata».
  - *attenzione*: si allarga per 20 s con icona + riga, poi torna piccola lasciando un **puntino** sotto gli occhi finché la voce è in attesa; notifica di sistema sì.
  - *informazione*: nessun allargamento; solo il **puntino**; notifica di sistema solo per le notifiche di Home Assistant (come ItsyHome v2).
  - Umore: bloccante e attenzione = «ti cerca»; informazione non cambia umore.
- **Fonti di M1**: notifica persistente HA = *informazione*, pulsante «Archivia» (la archivia anche in HA: azione innocua); batteria PC < 10% scollegata = *attenzione*; Home Assistant irraggiungibile da oltre 5 minuti = *attenzione*, pulsante «Riprova». Nessuna fonte di M1 produce *bloccante*: il meccanismo si costruisce e si prova con un comando di prova (`--prova-avviso <livello>`), la prova vera arriva con Claude Code.
- **Vita di una voce «In attesa»**: sparisce quando la sua causa finisce (notifica archiviata in HA o dal pannello, batteria in carica/sopra soglia, connessione tornata). Il clic su una voce senza app di dettaglio (es. notifica HA in M1) **la espande sul posto** mostrando il testo completo e i pulsanti; nessuna vista nuova.
- **Preferiti in M1**: la Panoramica mostra solo **sintesi** (Orologio: riga di stato; Quadranti: sensori e PC); **accendi/spegni** sta nella **vista Casa** del dock. Le scene possono stare nei preferiti (il mapper non le esclude più); l'editor dei preferiti (stella, ordinamento) è della milestone Casa.
- **Ritirata**: si entra con clic destro sulla pillola, dal menu della tray o con la scorciatoia globale **Ctrl+Alt+Maiusc+I**; si esce dal menu della tray («Mostra l'isola»), con la stessa scorciatoia, o da sola per un *bloccante*. Non sopravvive al riavvio. La scorciatoia globale **Ctrl+Alt+Maiusc+Spazio** apre il pannello (e rimette la pillola visibile). *(14/11: non più Ctrl+Alt+I e Ctrl+Alt+Spazio: con la Maiusc non coincidono con AltGr sulla tastiera italiana e non si scontrano con TwoCode.)*
- **Finestra di gestione**: processo separato con **cornice fidata disegnata in Slint** (stile dell'assistente: titolo, pulsanti) e, quando serve, un'area di **contenuto HTML ostile** (wry, JavaScript spento, navigazione bloccata). Comunicazione col processo principale su stdin/stdout con messaggi JSON (apri contenuto, chiudi, azione scelta). In M1 si costruisce e si misura con una mail di esempio; le interfacce interattive (dettaglio Claude, chat lunga HIRIS) si aggiungono nelle loro milestone sulla stessa cornice.

**Leggibilità** (dall'analisi): testo ≥ 12 px, bersagli del dock 36 × 32, icone spente con contrasto ≥ 3:1, tastiera completa (Esc, frecce, Tab), movimento ridotto rispettato.

## 4. Requisito di leggerezza — numeri

Misurati negli spike del 13/11 (Windows, CPU media su tutti i core, RAM privata):

| Stato | Misurato | Budget da rispettare |
|---|---|---|
| Pillola ferma | 7,3 MB, 0% CPU | ≤ 15 MB, ≤ 0,1% CPU |
| Pillola che "pensa" (animata) | 7,4 MB, 0,7% | ≤ 1% CPU |
| Pannello aperto, fermo | 10,8 MB | ≤ 30 MB, ~0% CPU |
| Finestra di gestione (motore web, processo separato) aperta | ~140 MB, 8 processi (spike «lettore») | accettato **solo mentre è aperta** |
| Dopo aver chiuso la finestra di gestione | 7,3 MB, 1 processo (spike) | ritorno entro +2 MB dal valore prima |

Regole che ne derivano:
- **Si anima solo quando serve**; ciò che non cambia sta fermo (il rendering è software, la CPU cresce con l'area animata).
- Nessun polling: aggiornamenti per eventi (WebSocket HA, hook Claude Code); le statistiche PC si leggono solo a pannello aperto o a intervalli lunghi.
- Le misure vanno rifatte a ogni milestone con lo script di misura (vedi §9).

## 5. Tecnologia (deciso dopo gli spike)

**Modalità mista: pannello Slint + finestra di gestione separata.**
- Interfaccia in **Slint 1.x** con renderer **Skia in modalità software** (`renderer-skia`, backend `winit-skia-software`). Scelto dopo aver confrontato Tauri (122 MB, 7 processi), Slint femtovg, Slint software, Slint Skia GPU.
- Contenuti "da web" (mail HTML, anteprima documenti) mostrati nella **finestra di gestione**: un **processo separato** (wry + tao, come lo spike «lettore»: 0,6 MB di eseguibile, apertura 0,64–0,78 s, ~140 MB solo mentre è aperta, memoria liberata alla chiusura, nessun processo orfano). Su Linux Wayland il riquadro web non è disponibile: la finestra di gestione usa il piano B (testo formattato). Il riquadro web *dentro* il pannello (provato il 13/11) non si usa più.
- JavaScript disattivato e navigazione bloccata per i contenuti delle mail (contenuto ostile); il processo separato non ha accesso a casa, Claude Code o token.

Vincoli noti:
- Dove il motore web manca, **piano B**: la mail convertita in testo formattato e mostrata con Slint.
- Niente vetro sfocato vero (backdrop blur): si usano sfumature e bordi di luce, come nelle bozze.
- Licenza Slint: royalty-free per app desktop anche a pagamento, **con attribuzione** (es. "Made with Slint" nelle informazioni). ItsyHome resta MIT; HIRIS è solo un servizio chiamato via API.

## 6. Architettura (proposta, a grandi linee)

Un solo eseguibile. Workspace Cargo con:
- **`casa-core`**: il backend Rust di ItsyHome estratto e reso indipendente da Tauri (client WebSocket HA, mapper, modelli, ambienti multipli, preferiti/ordinamenti, notifiche push via WebSocket / registrazione mobile app / webhook locale). Oggi l'accoppiamento con Tauri è leggero (`AppHandle.emit` per stato e aggiornamenti, `tauri-plugin-store` per la configurazione): va sostituito con un canale di eventi e un file di configurazione proprio.
- **`assistente`**: l'app Slint (pillola, pannello, layout, dock, impostazioni) + servizi di sistema.
- Moduli successivi come crate o moduli separati: `pc-stats`, `claude-hook`, `hiris`, `mail`, `focus`.

Servizi di sistema che oggi dà Tauri, da sostituire con librerie Rust indipendenti: icona nella tray e menu (`tray-icon`, `muda`), avvio con il sistema, istanza unica, notifiche di sistema, apertura di URL nel browser. Runtime async `tokio` su thread di lavoro; gli aggiornamenti all'interfaccia passano da `slint::invoke_from_event_loop`.

Sicurezza:
- Token di Home Assistant (e futuri: HIRIS, mail) nel **portachiavi del sistema operativo** (crate `keyring`), non in chiaro nel file di configurazione.
- Il server webhook locale resta solo se la funzione push lo richiede, con il segreto già previsto.

## 7. Prima milestone: le fondamenta (M1)

Obiettivo: l'assistente esiste, vive sul desktop, è leggero e mostra la casa e il PC, senza ancora le pagine delle singole app.

Incluso:
1. **Pillola** in alto al centro dello schermo principale, sempre in primo piano, fuori dalla taskbar, con i 5 umori e il battito di ciglia.
2. **Clic sulla pillola → pannello** (animazione di apertura); chiusura con Esc, clic fuori o nuovo clic sulla pillola.
3. **Pannello** come da §3 e §3bis: 800 × 300 al bordo alto, occhi, ora a destra, nessun «‹ Indietro» (si torna dal dock, con Esc, Backspace o Alt+←), Panoramica con riga «In attesa» e layout Orologio/Quadranti con i puntini, dock di sole icone.
4. **Dati reali nei layout** per le due fonti disponibili in M1: **Casa** (da `casa-core`) e **PC** (CPU, RAM, disco, rete, batteria se c'è).
5. **Umori collegati a eventi reali**: pensa = in connessione/caricamento; ti cerca = notifica HA; contento = azione riuscita; dorme = inattività prolungata; tranquillo = default.
6. **Impostazioni in una finestra dedicata** (vedi §3bis): ambienti Home Assistant (aggiungi/modifica/rimuovi/prova connessione), avvio con il sistema.
7. **Tray**: menu con Apri, Impostazioni, Esci; **istanza unica**; **avvio automatico**.
8. **Finestra di gestione** come componente pronto e misurato (processo separato, apri/chiudi, stile dell'assistente), provata con una mail di esempio, anche se nessuna app la usa ancora.
9. **Misure** nel budget di §4 su Windows; build verificata anche per Linux/macOS dove possibile.
10. **Attenzione e sicurezza d'uso** (§3bis) per le fonti di M1: riga «In attesa» con i 3 livelli, pillola che chiama con icona + riga + pulsanti, modalità «ritirata», aura quasi spenta a riposo, bloccante distinto per forma, preferiti tenui senza connessione, scene tra i preferiti.

Escluso da M1 (milestone successive, ognuna col suo brainstorming): pagina Casa completa (controlli per tipo di entità, telecamere, ordinamenti), HIRIS, Claude Code, mail, documenti, focus/blocco app, aggiornamenti automatici, licenza freemium.

Ordine proposto delle milestone successive: Casa (porting completo del popup ItsyHome) → Claude Code → HIRIS → focus/blocco app → mail → documenti.

### Precisazioni su M1 (dopo la revisione del costruttore)

- **Sostituisce ItsyHome** (deciso da Paolo: "trasformare ItsyHome"): è la v3 dello stesso prodotto, nello stesso repo, sviluppata su un ramo dedicato; `main` resta la v2.1.2 Tauri finché la v3 non è pronta. Niente doppio supporto Tauri/Slint per `casa-core`. *Scelta mia:* al primo avvio la v3 importa la configurazione della v2 (ambienti, preferiti, ordinamenti, aree nascoste, icone) dal file `itsyhome-config.json` di `tauri-plugin-store` e sposta i token nel portachiavi del sistema.
- **Notifiche in M1** *(scelta mia)*: fonte = `persistent_notification` via WebSocket (già esistente, nessuna porta aperta). Registrazione mobile_app e webhook locale passano alla milestone Casa. All'arrivo: umore "ti cerca" + notifica di sistema come oggi.
- **Casa in M1** *(scelta mia)*: i layout mostrano i **preferiti** (con la regola di ItsyHome; se non ce ne sono, i primi dispositivi in ordine). Comando minimo: un tocco accende/spegne luci e interruttori e attiva le scene; all'esito positivo l'umore diventa "contento". Il resto dei controlli è della milestone Casa.
- **Pillola e schermo** *(scelta mia)*: sta sullo schermo principale, in alto al centro, sempre sopra; **si nasconde quando l'app in primo piano è a schermo intero** (giochi, video, presentazioni) e ricompare all'uscita; resta raggiungibile dalla tray. Cambi di DPI e di monitor gestiti. macOS (notch) e Linux: in M1 si verifica che compili; Linux Wayland non è bersaglio di M1 (la pillola sempre sopra in alto al centro non è garantita lì) — si affronta in una milestone dedicata.
- **Budget con backend** *(scelta mia)*: i limiti del §4 valgono **con Home Assistant connesso** e le entità in memoria. Quindi: runtime tokio ridotto, `rustls` invece di `native-tls`, dipendenze minime (axum non entra in M1), modello entità compatto; scenario di misura con l'HA vero.
- **Registro delle app** *(scelta mia)*: si costruisce in M1, perché ogni milestone successiva ci si aggancia. Il dock mostra solo le app che esistono (Panoramica, Casa, PC) più Impostazioni: niente icone disattivate segnaposto.
- **Livelli di attenzione in M1** *(scelta mia)*: notifica persistente di HA = *informazione*; batteria del PC sotto il 10% e scollegata = *attenzione*; connessione a Home Assistant persa da oltre 5 minuti = *attenzione*. Allarmi e aperture di sicurezza della casa restano alla milestone Casa. Il meccanismo (registro delle voci in attesa con livello, fonte, testo, azione, pulsanti) è comune e pronto per Claude Code.

## 8. Lasciato al costruttore

- Struttura interna dei crate, nomi dei moduli, organizzazione dei file `.slint`.
- Scelta delle librerie per tray, autostart, istanza unica, notifiche, statistiche PC, purché multipiattaforma e leggere.
- Dettagli di animazione (durate, curve) nel rispetto delle bozze e del budget CPU.
- Formato del file di configurazione.
- Scelte grafiche di dettaglio dei 4 layout con i dati di M1, coerenti con le bozze.

## 9. Come si verifica

- Script di misura con linea del tempo (processi, RAM privata, CPU) come negli spike `C:\Work\Sviluppo\spike-isola\misura-*.ps1`: da portare nel repo in `tools/`.
- Foto di controllo **solo della zona dell'app**, mai dello schermo intero, cancellate dopo la verifica.
- Test automatici per `casa-core` (mapper, store, eventi) e per la logica degli umori.
- Prova dal vivo con il Home Assistant di casa prima di dichiarare chiusa la milestone.
