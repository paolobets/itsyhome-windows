//! Test del servitore sul tubo vero di Windows, con nomi unici (mai quello di isola). Il cliente è
//! questo stesso processo: `cliente_atteso` = l'eseguibile dei test.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::sys;
use super::*;
use crate::evento::TipoEvento;
use crate::messaggio::{Richiesta, Risposta};

#[derive(Clone, Copy)]
enum Modo {
    /// `permesso` risponde `None`.
    Subito,
    /// Decide subito.
    Decidi(Decisione),
    /// Non decide mai (tiene vivo il canale).
    Attendi,
}

struct Registro {
    modo: Modo,
    eventi: Mutex<Vec<Arrivo>>,
    permessi: Mutex<Vec<Arrivo>>,
    abbandonati: Mutex<Vec<u64>>,
    barre: Mutex<Vec<Arrivo>>,
    mittenti: Mutex<Vec<Sender<Decisione>>>,
    prossimo_id: AtomicUsize,
    rifiuti: AtomicUsize,
    persi: AtomicUsize,
}

impl Registro {
    fn nuovo(modo: Modo) -> Arc<Registro> {
        Arc::new(Registro {
            modo,
            eventi: Mutex::new(Vec::new()),
            permessi: Mutex::new(Vec::new()),
            abbandonati: Mutex::new(Vec::new()),
            barre: Mutex::new(Vec::new()),
            mittenti: Mutex::new(Vec::new()),
            prossimo_id: AtomicUsize::new(1),
            rifiuti: AtomicUsize::new(0),
            persi: AtomicUsize::new(0),
        })
    }
    fn chiamate(&self) -> usize {
        self.eventi.lock().unwrap().len()
            + self.permessi.lock().unwrap().len()
            + self.abbandonati.lock().unwrap().len()
            + self.barre.lock().unwrap().len()
    }
}

impl Gestore for Registro {
    fn evento(&self, a: Arrivo) {
        self.eventi.lock().unwrap().push(a);
    }
    fn permesso(&self, a: Arrivo) -> Option<AttesaPermesso> {
        self.permessi.lock().unwrap().push(a);
        if matches!(self.modo, Modo::Subito) {
            return None;
        }
        let id = self.prossimo_id.fetch_add(1, Ordering::SeqCst) as u64;
        let (tx, rx) = std::sync::mpsc::channel();
        if let Modo::Decidi(d) = self.modo {
            tx.send(d).unwrap();
        }
        self.mittenti.lock().unwrap().push(tx);
        Some(AttesaPermesso { id, decisione: rx })
    }
    fn abbandonato(&self, id: u64) {
        self.abbandonati.lock().unwrap().push(id);
    }
    fn barra(&self, a: Arrivo) {
        self.barre.lock().unwrap().push(a);
    }
    fn rifiutata(&self) {
        self.rifiuti.fetch_add(1, Ordering::SeqCst);
    }
    fn canale_perso(&self) {
        self.persi.fetch_add(1, Ordering::SeqCst);
    }
}

/// Quanto il cliente di prova aspetta un'istanza libera (come l'aiutante).
const ATTESA_CLIENTE: Duration = Duration::from_millis(200);

fn exe() -> PathBuf {
    std::env::current_exe().unwrap()
}

fn config(nome: &str, cliente: PathBuf, attesa: Duration) -> ConfigTubo {
    ConfigTubo {
        nome: nome.to_owned(),
        cliente_atteso: cliente,
        attesa_massima: attesa,
    }
}

/// Un servitore con nome unico che accetta questo processo come cliente.
fn avvia_prova(g: &Arc<Registro>, attesa: Duration) -> (Servitore, String) {
    let nome = nome_di_prova();
    let s = avvia(config(&nome, exe(), attesa), g.clone() as Arc<dyn Gestore>).expect("avvio");
    (s, nome)
}

/// Un canale verso il servitore di prova (il server è questo stesso eseguibile).
fn cliente(nome: &str) -> Canale {
    collega(
        &Destinazione {
            nome: nome.to_owned(),
            server_atteso: exe(),
        },
        ATTESA_CLIENTE,
    )
    .expect("collegamento")
}

fn evento(id: &str, tipo: TipoEvento) -> Vec<u8> {
    Richiesta::Evento(EventoHook {
        sessione: id.into(),
        tipo,
        cartella: "progetto".into(),
        strumento: Some("Bash".into()),
        notifica: None,
        errore: None,
        riassunto: (tipo == TipoEvento::PermissionRequest).then(|| "cargo test".into()),
    })
    .in_byte()
    .unwrap()
}

/// Manda e legge la risposta (`None` = chiusa senza risposta). Un servitore che rifiuta può
/// chiudere prima che il messaggio parta, o perfino prima che il cliente finisca di verificarlo.
fn invia(nome: &str, messaggio: &[u8]) -> Option<Risposta> {
    let c = collega(
        &Destinazione {
            nome: nome.to_owned(),
            server_atteso: exe(),
        },
        ATTESA_CLIENTE,
    )?;
    if !c.manda(messaggio, Duration::from_secs(2)) {
        return None;
    }
    c.ricevi(Duration::from_secs(5))
        .and_then(|b| Risposta::da_byte(&b))
}

fn aspetta(mut f: impl FnMut() -> bool, secondi: u64) -> bool {
    let fine = Instant::now() + Duration::from_secs(secondi);
    while Instant::now() < fine {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    f()
}

fn nessuna_chiamata(g: &Registro) {
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(g.chiamate(), 0, "il gestore non doveva essere chiamato");
}

#[test]
fn solo_il_cliente_atteso() {
    // Cliente giusto: accettato.
    let g = Registro::nuovo(Modo::Subito);
    let (s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    assert_eq!(
        invia(&nome, &evento("s1", TipoEvento::PreToolUse)),
        Some(Risposta::Ricevuto)
    );
    assert!(aspetta(|| g.eventi.lock().unwrap().len() == 1, 3));
    assert_eq!(s.rifiutate(), 0);
    // Un altro eseguibile atteso: questo processo non è lui → rifiutata, gestore mai chiamato.
    let g = Registro::nuovo(Modo::Subito);
    let nome = nome_di_prova();
    let altro = exe().with_file_name("isola-hook.exe");
    let s = avvia(
        config(&nome, altro, ATTESA_MASSIMA),
        g.clone() as Arc<dyn Gestore>,
    )
    .unwrap();
    assert_eq!(invia(&nome, &evento("s1", TipoEvento::PreToolUse)), None);
    nessuna_chiamata(&g);
    assert_eq!(s.rifiutate(), 1);
    assert_eq!(
        g.rifiuti.load(Ordering::SeqCst),
        1,
        "il gestore sa del rifiuto"
    );
}

#[test]
fn canale_occupato_errore_esplicito() {
    let nome = nome_di_prova();
    let d = sys::descrittore_solo_utente().unwrap();
    let _occupante = sys::crea_istanza(&sys::larga(&nome), true, &d).expect("tubo del test");
    let g = Registro::nuovo(Modo::Subito);
    let esito = avvia(config(&nome, exe(), ATTESA_MASSIMA), g as Arc<dyn Gestore>);
    assert!(
        matches!(esito, Err(ErroreAvvio::CanaleOccupato)),
        "{:?}",
        esito.err()
    );
}

#[test]
fn nome_non_valido_rifiutato() {
    let g = Registro::nuovo(Modo::Subito);
    let esito = avvia(
        config(r"C:\temp\x", exe(), ATTESA_MASSIMA),
        g as Arc<dyn Gestore>,
    );
    assert!(matches!(esito, Err(ErroreAvvio::Altro(_))));
}

#[test]
fn messaggio_troppo_grande_o_malformato() {
    let g = Registro::nuovo(Modo::Subito);
    let (s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    let grande = vec![b'{'; 70 * 1024];
    let annidato = "[".repeat(10_000);
    for corpo in [
        &grande[..],
        b"",
        b"[",
        annidato.as_bytes(),
        br#"{"Evento":{"sessione":"a b"}}"#,
    ] {
        assert_eq!(
            invia(&nome, corpo),
            None,
            "messaggio di {} byte",
            corpo.len()
        );
    }
    nessuna_chiamata(&g);
    assert!(aspetta(|| s.rifiutate() == 5, 3), "{}", s.rifiutate());
    // Il servitore è vivo: un messaggio valido passa.
    assert_eq!(
        invia(&nome, &evento("s1", TipoEvento::Stop)),
        Some(Risposta::Ricevuto)
    );
    assert!(aspetta(|| g.eventi.lock().unwrap().len() == 1, 3));
}

#[test]
fn lettura_lenta_chiusa_dopo_2s() {
    let g = Registro::nuovo(Modo::Subito);
    let (s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    let c = cliente(&nome);
    let inizio = Instant::now();
    assert_eq!(c.ricevi(Duration::from_secs(5)), None);
    let passato = inizio.elapsed();
    assert!(
        passato < Duration::from_millis(2600),
        "chiusa dopo {passato:?}"
    );
    assert!(
        passato >= Duration::from_millis(1500),
        "chiusa troppo presto: {passato:?}"
    );
    nessuna_chiamata(&g);
    assert_eq!(s.rifiutate(), 1);
}

#[test]
fn diciassettesima_connessione_chiusa() {
    let g = Registro::nuovo(Modo::Attendi);
    let (s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    let mut aperte = Vec::new();
    for i in 0..CONNESSIONI_MASSIME {
        let c = cliente(&nome);
        assert!(c.manda(
            &evento(&format!("s{i}"), TipoEvento::PermissionRequest),
            Duration::from_secs(2)
        ));
        aperte.push(c);
    }
    assert!(aspetta(
        || g.permessi.lock().unwrap().len() == CONNESSIONI_MASSIME,
        5
    ));
    let inizio = Instant::now();
    assert_eq!(invia(&nome, &evento("altra", TipoEvento::PreToolUse)), None);
    assert!(
        inizio.elapsed() < Duration::from_millis(500),
        "{:?}",
        inizio.elapsed()
    );
    assert!(aspetta(|| s.rifiutate() == 1, 2), "{}", s.rifiutate());
    assert!(g.eventi.lock().unwrap().is_empty());
    // Chiuse le attese, si torna a servire.
    drop(aperte);
    assert!(aspetta(
        || g.abbandonati.lock().unwrap().len() == CONNESSIONI_MASSIME,
        5
    ));
    assert_eq!(
        invia(&nome, &evento("altra", TipoEvento::PreToolUse)),
        Some(Risposta::Ricevuto)
    );
}

#[test]
fn permesso_consenti_nega_scaduto_chiuso() {
    let permesso = evento("s1", TipoEvento::PermissionRequest);
    // Consenti / Nega: la risposta, nessun abbandono.
    for (d, r) in [
        (Decisione::Consenti, Risposta::Consenti),
        (Decisione::Nega, Risposta::Nega),
    ] {
        let g = Registro::nuovo(Modo::Decidi(d));
        let (s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
        assert_eq!(invia(&nome, &permesso), Some(r));
        assert!(g.abbandonati.lock().unwrap().is_empty());
        assert_eq!(s.rifiutate(), 0);
    }
    // Nessuna attesa dal gestore: «nessuna» subito.
    let g = Registro::nuovo(Modo::Subito);
    let (_s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    assert_eq!(invia(&nome, &permesso), Some(Risposta::Nessuna));
    // Scaduto: «nessuna» dopo l'attesa e il permesso abbandonato.
    let g = Registro::nuovo(Modo::Attendi);
    let (_s, nome) = avvia_prova(&g, Duration::from_millis(300));
    let inizio = Instant::now();
    assert_eq!(invia(&nome, &permesso), Some(Risposta::Nessuna));
    assert!(
        inizio.elapsed() >= Duration::from_millis(300),
        "{:?}",
        inizio.elapsed()
    );
    assert!(aspetta(
        || g.abbandonati.lock().unwrap().as_slice() == [1],
        3
    ));
    // Cliente che chiude: abbandonato entro 1 s.
    let g = Registro::nuovo(Modo::Attendi);
    let (_s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    let c = cliente(&nome);
    assert!(c.manda(&permesso, Duration::from_secs(2)));
    assert!(aspetta(|| g.permessi.lock().unwrap().len() == 1, 3));
    assert!(g.abbandonati.lock().unwrap().is_empty());
    drop(c);
    assert!(aspetta(
        || g.abbandonati.lock().unwrap().as_slice() == [1],
        1
    ));
}

#[test]
fn sequenza_cresce_per_connessione() {
    let g = Registro::nuovo(Modo::Subito);
    let (_s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    for _ in 0..3 {
        assert_eq!(
            invia(&nome, &evento("s1", TipoEvento::PreToolUse)),
            Some(Risposta::Ricevuto)
        );
    }
    assert!(aspetta(|| g.eventi.lock().unwrap().len() == 3, 3));
    let mut seq: Vec<u64> = g
        .eventi
        .lock()
        .unwrap()
        .iter()
        .map(|a| a.sequenza)
        .collect();
    // Il gestore può essere chiamato in un ordine diverso: conta la sequenza presa all'accettazione.
    seq.sort_unstable();
    seq.dedup();
    assert_eq!(seq.len(), 3, "numeri distinti");
    assert!(
        seq[0] >= 1 && seq.windows(2).all(|w| w[0] < w[1]),
        "{seq:?}"
    );
}

#[test]
fn catena_solo_alla_prima_richiesta_di_una_sessione() {
    let g = Registro::nuovo(Modo::Subito);
    let (_s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    for id in ["uno", "uno", "due"] {
        assert_eq!(
            invia(&nome, &evento(id, TipoEvento::Stop)),
            Some(Risposta::Ricevuto)
        );
    }
    assert!(aspetta(|| g.eventi.lock().unwrap().len() == 3, 3));
    let mut eventi = g.eventi.lock().unwrap().clone();
    eventi.sort_by_key(|e| e.sequenza);
    // Solo alla prima richiesta di una sessione: la catena dell'aiutante presa mentre era
    // collegato (qui: questo processo e i suoi genitori); alle altre, nessuna catena.
    let catene: Vec<&Vec<(u32, String)>> = eventi.iter().map(|e| &e.catena_mittente).collect();
    assert!(catene[1].is_empty());
    for c in [catene[0], catene[2]] {
        assert!(
            !c.is_empty() && c.len() <= crate::verifica::CATENA_MASSIMA,
            "{c:?}"
        );
        assert_eq!(c[0].0, std::process::id());
    }
}

#[test]
fn barra_ricevuta_e_passata_al_gestore() {
    let g = Registro::nuovo(Modo::Subito);
    let (_s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    let d = DatiBarra {
        sessione: "s1".into(),
        modello: Some("Opus".into()),
        contesto: Some(4.0),
        cinque_ore: None,
        sette_giorni: None,
    };
    assert_eq!(
        invia(&nome, &Richiesta::Barra(d.clone()).in_byte().unwrap()),
        Some(Risposta::Ricevuto)
    );
    assert!(aspetta(|| g.barre.lock().unwrap().len() == 1, 3));
    let b = &g.barre.lock().unwrap()[0];
    assert!(matches!(&b.contenuto, Contenuto::Barra(x) if *x == d));
    assert!(b.catena_mittente.is_empty());
    assert!(g.eventi.lock().unwrap().is_empty());
}

#[test]
fn server_sconosciuto_non_riceve_nulla() {
    let g = Registro::nuovo(Modo::Subito);
    let (_s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    // L'aiutante si aspetta un altro eseguibile come server: nessun canale.
    let d = Destinazione {
        nome: nome.clone(),
        server_atteso: exe().with_file_name(NOME_SERVER),
    };
    assert!(collega(&d, ATTESA_CLIENTE).is_none());
    // Il servitore ha visto una connessione chiusa senza messaggio, mai un evento.
    nessuna_chiamata(&g);
    // Tubo assente: nessun canale, subito.
    let inizio = Instant::now();
    assert!(collega(
        &Destinazione {
            nome: nome_di_prova(),
            server_atteso: exe()
        },
        ATTESA_CLIENTE
    )
    .is_none());
    assert!(inizio.elapsed() < Duration::from_millis(100));
}

#[test]
fn drop_ferma_e_libera_il_nome() {
    let g = Registro::nuovo(Modo::Attendi);
    let (s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    // Un permesso in attesa durante lo spegnimento.
    let c = cliente(&nome);
    assert!(c.manda(
        &evento("s1", TipoEvento::PermissionRequest),
        Duration::from_secs(2)
    ));
    assert!(aspetta(|| g.permessi.lock().unwrap().len() == 1, 3));
    let inizio = Instant::now();
    drop(s);
    assert!(
        inizio.elapsed() < Duration::from_secs(1),
        "{:?}",
        inizio.elapsed()
    );
    assert!(aspetta(
        || g.abbandonati.lock().unwrap().as_slice() == [1],
        1
    ));
    drop(c);
    assert!(
        collega(
            &Destinazione {
                nome: nome.clone(),
                server_atteso: exe()
            },
            ATTESA_CLIENTE
        )
        .is_none(),
        "nessuno ascolta più"
    );
    // Il nome si riprende subito.
    let g2 = Registro::nuovo(Modo::Subito);
    let s2 = avvia(
        config(&nome, exe(), ATTESA_MASSIMA),
        g2.clone() as Arc<dyn Gestore>,
    );
    assert!(s2.is_ok(), "{:?}", s2.err());
}

#[test]
fn sddl_solo_l_utente() {
    assert_eq!(
        sys::sddl_solo_utente("S-1-5-21-1-2-3-1001"),
        "O:S-1-5-21-1-2-3-1001D:P(A;;GA;;;S-1-5-21-1-2-3-1001)"
    );
}

/// Il tubo creato ha il proprietario e la DACL attesi, riletti dall'oggetto vero **per struttura**
/// (non con l'SDDL testuale, che per l'Administrator integrato usa alias come `LA`): proprietario =
/// utente corrente, DACL presente e protetta con un solo ACE «consenti», per l'utente e con accesso
/// completo (nessun Everyone, anonimo, SYSTEM o Administrators). E l'aiutante, dalla sua maniglia,
/// rilegge lo stesso proprietario.
#[test]
fn proprietario_e_dacl_riletti_dal_tubo() {
    let nome = nome_di_prova();
    let d = sys::descrittore_solo_utente().unwrap();
    let h = sys::crea_istanza(&sys::larga(&nome), true, &d).expect("tubo del test");
    let s = crate::verifica::struttura_del_tubo(h.0).expect("descrittore riletto");
    assert!(s.proprietario_utente, "{s:?}");
    assert!(s.dacl_presente && s.dacl_protetta, "{s:?}");
    // Un solo ACE, `ACCESS_ALLOWED_ACE_TYPE` (0), per l'utente, con tutti i diritti del file.
    const ACCESSO_COMPLETO: u32 = 0x001F_01FF;
    assert_eq!(s.ace.len(), 1, "{s:?}");
    let (tipo, maschera, utente) = s.ace[0];
    assert_eq!(tipo, 0, "{s:?}");
    assert!(utente, "l'unico ACE non è dell'utente: {s:?}");
    assert_eq!(maschera & ACCESSO_COMPLETO, ACCESSO_COMPLETO, "{s:?}");
    // Lato cliente: la maniglia aperta come l'aiutante rilegge il proprietario.
    let c = sys::apri_cliente(&nome, ATTESA_CLIENTE).expect("cliente");
    assert!(crate::verifica::tubo_dell_utente(c.0));
}

/// Un creatore di istanze che dopo la prima fallisce sempre (risorse finite): dà il via all'arresto
/// del servitore dopo troppi errori di fila.
fn crea_e_poi_fallisce(
    nome: &[u16],
    prima: bool,
    d: &sys::Descrittore,
) -> Result<sys::Maniglia, u32> {
    if prima {
        sys::crea_istanza(nome, true, d)
    } else {
        Err(windows_sys::Win32::Foundation::ERROR_NO_SYSTEM_RESOURCES)
    }
}

/// Il percorso di arresto di M1: 50 errori di fila nel creare le istanze → il servitore smette e
/// lo dice al gestore **una volta sola** (`canale_perso`); non gira a vuoto.
#[test]
fn cinquanta_errori_di_fila_fermano_il_servitore() {
    let g = Registro::nuovo(Modo::Subito);
    let nome = nome_di_prova();
    let s = avvia_con_creatore(
        config(&nome, exe(), ATTESA_MASSIMA),
        g.clone() as Arc<dyn Gestore>,
        crea_e_poi_fallisce,
    )
    .expect("avvio");
    assert_eq!(
        g.persi.load(Ordering::SeqCst),
        0,
        "finché non arriva nessuno non si crea nulla"
    );
    // Un cliente che si collega fa cercare la prossima istanza: fallisce, e fallisce di nuovo.
    let _c = sys::apri_cliente(&nome, ATTESA_CLIENTE).expect("cliente");
    assert!(
        aspetta(|| g.persi.load(Ordering::SeqCst) == 1, 5),
        "canale_perso non è arrivato"
    );
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(g.persi.load(Ordering::SeqCst), 1, "una volta sola");
    // Un servitore fermato si chiude pulito.
    let inizio = Instant::now();
    drop(s);
    assert!(
        inizio.elapsed() < Duration::from_secs(1),
        "{:?}",
        inizio.elapsed()
    );
}

/// Una raffica di clienti che si collegano e spariscono subito non lascia mai il nome libero:
/// dopo la raffica nessuno può creare la «prima istanza» con lo stesso nome e il servitore
/// risponde. (Non è il ramo `Errore` di `connetti`, che non si può provocare da fuori: quello è
/// coperto dalla riga che scollega e rimette in ascolto la stessa istanza.)
#[test]
fn raffica_di_clienti_che_spariscono_lascia_il_nome_occupato() {
    let g = Registro::nuovo(Modo::Subito);
    let (_s, nome) = avvia_prova(&g, ATTESA_MASSIMA);
    let d = sys::descrittore_solo_utente().unwrap();
    for _ in 0..40 {
        if let Some(c) = sys::apri_cliente(&nome, ATTESA_CLIENTE) {
            drop(c);
        }
        assert!(
            sys::crea_istanza(&sys::larga(&nome), true, &d).is_err(),
            "il nome era libero"
        );
    }
    assert_eq!(
        invia(&nome, &evento("s1", TipoEvento::PreToolUse)),
        Some(Risposta::Ricevuto)
    );
}

/// L'etichetta d'integrità riletta da tubi veri: senza etichetta (o media) passa, bassa no. Il
/// cliente dell'aiutante (`collega`) rifiuta un tubo a bassa integrità anche se il resto è in regola.
#[test]
fn etichetta_d_integrita_del_tubo() {
    use crate::verifica::{etichetta_accettabile, etichetta_tubo, Etichetta};
    let sid = crate::verifica::sid_utente().unwrap();
    let base = sys::sddl_solo_utente(&sid);
    // Nessuna etichetta esplicita (come i tubi di isola): `Assente` o un livello almeno medio.
    let d = sys::descrittore_solo_utente().unwrap();
    let h = sys::crea_istanza(&sys::larga(&nome_di_prova()), true, &d).expect("tubo");
    let e = etichetta_tubo(h.0);
    assert!(
        matches!(e, Etichetta::Assente | Etichetta::Livello(_)),
        "{e:?}"
    );
    assert!(etichetta_accettabile(e), "{e:?}");
    // Etichetta media esplicita.
    let d = sys::descrittore_da_sddl(&format!("{base}S:(ML;;NW;;;ME)")).unwrap();
    let h_me = sys::crea_istanza(&sys::larga(&nome_di_prova()), true, &d).expect("tubo medio");
    assert_eq!(etichetta_tubo(h_me.0), Etichetta::Livello(0x2000));
    // Etichetta bassa: letta e rifiutata, anche dalla maniglia del cliente.
    let nome_lw = nome_di_prova();
    let d = sys::descrittore_da_sddl(&format!("{base}S:(ML;;NW;;;LW)")).unwrap();
    let h_lw = sys::crea_istanza(&sys::larga(&nome_lw), true, &d).expect("tubo basso");
    assert_eq!(etichetta_tubo(h_lw.0), Etichetta::Livello(0x1000));
    assert!(!etichetta_accettabile(etichetta_tubo(h_lw.0)));
    let c = sys::apri_cliente(&nome_lw, ATTESA_CLIENTE).expect("cliente");
    assert_eq!(etichetta_tubo(c.0), Etichetta::Livello(0x1000));
    assert!(!crate::verifica::tubo_integrita_ok(c.0));
    drop(c);
    // `collega` verso quel tubo: nessun canale.
    assert!(
        collega(
            &Destinazione {
                nome: nome_lw,
                server_atteso: exe()
            },
            ATTESA_CLIENTE
        )
        .is_none(),
        "un tubo a bassa integrità non è un server valido"
    );
}
