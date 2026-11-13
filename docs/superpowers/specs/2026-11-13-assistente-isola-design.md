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
1. **Pillola piccola** (in alto al centro dello schermo): l'isola *è* la faccia. Pillola nera, due occhi di luce, aura con i colori dei petali HIRIS che scorre lungo il bordo.
2. **Si allarga quando ti cerca** (notifica, richiesta di Claude Code, avviso casa).
3. **Pannello aperto**.

**Umori** (5): tranquillo (aura ferma, solo battito di ciglia), pensa (aura che ruota), ti cerca (aura ambra-rosa che pulsa), contento (occhi ad arco), dorme (luce bassa, occhi a trattino).

**Pannello aperto**
- In alto: occhi al centro, **solo l'ora a destra**, a sinistra niente.
- Centro: le informazioni, in **4 layout** come TwoCode: Orologio, Quadranti, Colori, Tutte le app.
- In basso: **dock di icone centrato** dentro una capsula sottile; la scheda selezionata ha una capsula "vetro" leggera.
- Icone **leggere stile SF Symbols**, monocromatiche (`rgba(236,230,250,.62)`, tratto ~1,55); **il colore solo per lo stato** (regola 60-30-10). Niente piastrelle liquid glass.

**Brand HIRIS**: colori `#c084fc #818cf8 #60a5fa #22d3ee #2dd4bf #e879f9`, fondo `#2a0a4e → #0a0015`, font Inter Tight + JetBrains Mono.

## 4. Requisito di leggerezza — numeri

Misurati negli spike del 13/11 (Windows, CPU media su tutti i core, RAM privata):

| Stato | Misurato | Budget da rispettare |
|---|---|---|
| Pillola ferma | 7,3 MB, 0% CPU | ≤ 15 MB, ≤ 0,1% CPU |
| Pillola che "pensa" (animata) | 7,4 MB, 0,7% | ≤ 1% CPU |
| Pannello aperto, fermo | 10,8 MB | ≤ 30 MB, ~0% CPU |
| Pannello con contenuto web aperto | ~133 MB, 7 processi | accettato **solo mentre è aperto** |
| Dopo aver chiuso il contenuto web | 11,9 MB, 1 processo | ritorno entro +2 MB dal valore prima |

Regole che ne derivano:
- **Si anima solo quando serve**; ciò che non cambia sta fermo (il rendering è software, la CPU cresce con l'area animata).
- Nessun polling: aggiornamenti per eventi (WebSocket HA, hook Claude Code); le statistiche PC si leggono solo a pannello aperto o a intervalli lunghi.
- Le misure vanno rifatte a ogni milestone con lo script di misura (vedi §9).

## 5. Tecnologia (deciso dopo gli spike)

**Modalità mista, in un unico pannello.**
- Interfaccia in **Slint 1.x** con renderer **Skia in modalità software** (`renderer-skia`, backend `winit-skia-software`). Scelto dopo aver confrontato Tauri (122 MB, 7 processi), Slint femtovg, Slint software, Slint Skia GPU.
- Contenuti "da web" (mail HTML; eventualmente anteprima documenti) mostrati con **wry** come **riquadro figlio dentro il pannello** (`build_as_child`), creato quando serve e **distrutto alla chiusura**. Provato: compare nel pannello, apertura 0,45–0,55 s, la memoria torna giù.
- JavaScript disattivato e navigazione bloccata nel riquadro web (le mail sono contenuto ostile).

Vincoli noti:
- Il riquadro web è un rettangolo nativo: Slint non può disegnarci sopra (niente aura o popup sopra la mail); va nascosto durante le animazioni del pannello.
- **Linux**: il riquadro figlio funziona solo su X11. Su Wayland si usa il **piano B**: la mail convertita in testo formattato e mostrata con Slint. Il piano B vale anche ovunque il motore web manchi.
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
3. **Pannello** come da §3: occhi, ora a destra, 4 layout commutabili, dock centrato.
4. **Dati reali nei layout** per le due fonti disponibili in M1: **Casa** (da `casa-core`) e **PC** (CPU, RAM, disco, rete, batteria se c'è).
5. **Umori collegati a eventi reali**: pensa = in connessione/caricamento; ti cerca = notifica HA; contento = azione riuscita; dorme = inattività prolungata; tranquillo = default.
6. **Impostazioni dentro il pannello** (non finestra separata): ambienti Home Assistant (aggiungi/modifica/rimuovi/prova connessione), avvio con il sistema.
7. **Tray**: menu con Apri, Impostazioni, Esci; **istanza unica**; **avvio automatico**.
8. **Componente "riquadro web"** pronto e misurato (apri/chiudi dentro il pannello), anche se nessuna app lo usa ancora.
9. **Misure** nel budget di §4 su Windows; build verificata anche per Linux/macOS dove possibile.

Escluso da M1 (milestone successive, ognuna col suo brainstorming): pagina Casa completa (controlli per tipo di entità, telecamere, ordinamenti), HIRIS, Claude Code, mail, documenti, focus/blocco app, aggiornamenti automatici, licenza freemium.

Ordine proposto delle milestone successive: Casa (porting completo del popup ItsyHome) → Claude Code → HIRIS → focus/blocco app → mail → documenti.

### Precisazioni su M1 (dopo la revisione del costruttore)

- **Sostituisce ItsyHome** (deciso da Paolo: "trasformare ItsyHome"): è la v3 dello stesso prodotto, nello stesso repo, sviluppata su un ramo dedicato; `main` resta la v2.1.2 Tauri finché la v3 non è pronta. Niente doppio supporto Tauri/Slint per `casa-core`. *Scelta mia:* al primo avvio la v3 importa la configurazione della v2 (ambienti, preferiti, ordinamenti, aree nascoste, icone) dal file `itsyhome-config.json` di `tauri-plugin-store` e sposta i token nel portachiavi del sistema.
- **Notifiche in M1** *(scelta mia)*: fonte = `persistent_notification` via WebSocket (già esistente, nessuna porta aperta). Registrazione mobile_app e webhook locale passano alla milestone Casa. All'arrivo: umore "ti cerca" + notifica di sistema come oggi.
- **Casa in M1** *(scelta mia)*: i layout mostrano i **preferiti** (con la regola di ItsyHome; se non ce ne sono, i primi dispositivi in ordine). Comando minimo: un tocco accende/spegne luci e interruttori e attiva le scene; all'esito positivo l'umore diventa "contento". Il resto dei controlli è della milestone Casa.
- **Pillola e schermo** *(scelta mia)*: sta sullo schermo principale, in alto al centro, sempre sopra; **si nasconde quando l'app in primo piano è a schermo intero** (giochi, video, presentazioni) e ricompare all'uscita; resta raggiungibile dalla tray. Cambi di DPI e di monitor gestiti. macOS (notch) e Linux: in M1 si verifica che compili; Linux Wayland non è bersaglio di M1 (la pillola sempre sopra in alto al centro non è garantita lì) — si affronta in una milestone dedicata.
- **Budget con backend** *(scelta mia)*: i limiti del §4 valgono **con Home Assistant connesso** e le entità in memoria. Quindi: runtime tokio ridotto, `rustls` invece di `native-tls`, dipendenze minime (axum non entra in M1), modello entità compatto; scenario di misura con l'HA vero.
- **Registro delle app** *(scelta mia)*: si costruisce in M1, perché ogni milestone successiva ci si aggancia. Dock e "Tutte le app" mostrano solo le app che esistono (Casa, PC) più Impostazioni: niente icone disattivate segnaposto.

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
