//! L'aiutante lanciato come lo lancia Claude Code (forma exec, JSON su stdin).
//!
//! Mai il tubo vero: l'eseguibile `isola-hook` riceve solo ingressi che non arrivano ad aprire un
//! tubo (JSON illeggibile, comandi sconosciuti, barra senza dati); i casi con un JSON valido
//! passano dall'esempio `aiutante_prova` (stesso corpo, `aiutante::principale`) con un nome di tubo
//! unico dato nell'ambiente. I casi con un server (falso o vero) sono nei test di `aiutante.rs` e
//! `tubo.rs`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PERMESSO: &str = r#"{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"C:\\x\\p","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
const BARRA: &str = r#"{"session_id":"s1","model":{"display_name":"Opus 5.5"},"context_window":{"used_percentage":42}}"#;

/// L'esempio di prova, compilato da `cargo test` accanto ai test (`…/debug/examples`). Con
/// `cargo test --test aiutante` da solo gli esempi non si compilano: `None`, e i test che ne hanno
/// bisogno si saltano con un messaggio chiaro (`cargo test -p claude-hook` o `--workspace` li compila).
fn aiutante_prova() -> Option<PathBuf> {
    // La cartella del profilo (`…/debug`) è una delle cartelle sopra questo eseguibile di test
    // (`deps` o `build/<crate>/<hash>/out` a seconda della versione di cargo).
    let nome = format!("aiutante_prova{}", std::env::consts::EXE_SUFFIX);
    let test = std::env::current_exe().unwrap();
    let trovato = test
        .ancestors()
        .map(|d| d.join("examples").join(&nome))
        .find(|p| p.exists());
    if trovato.is_none() {
        eprintln!(
            "SALTATO: manca examples/{nome} sopra {} (non compilato: lancia `cargo test -p claude-hook` o `cargo build -p claude-hook --examples` prima)",
            test.display()
        );
    }
    trovato
}

/// Un nome di tubo che nessuno serve.
fn tubo_unico() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(r"\\.\pipe\isola-prova-binario-{}-{t}", std::process::id())
}

fn lancia(
    exe: &std::path::Path,
    argomenti: &[&str],
    stdin: &[u8],
    tubo: Option<&str>,
) -> (std::process::Output, Duration) {
    let inizio = Instant::now();
    let mut c = Command::new(exe);
    c.args(argomenti)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c.env_remove("ISOLA_PROVA_TUBO")
        .env_remove("ISOLA_PROVA_SERVER");
    if let Some(t) = tubo {
        c.env("ISOLA_PROVA_TUBO", t)
            .env("ISOLA_PROVA_SERVER", exe.with_file_name("assistente.exe"));
    }
    let mut figlio = c.spawn().expect("avvio dell'aiutante");
    let mut ingresso = figlio.stdin.take().unwrap();
    let _ = ingresso.write_all(stdin);
    drop(ingresso);
    let uscita = figlio.wait_with_output().unwrap();
    (uscita, inizio.elapsed())
}

fn silenzioso(u: &std::process::Output, durata: Duration, caso: &str) {
    assert!(u.status.success(), "{caso}: {:?}", u.status);
    assert!(
        u.stdout.is_empty(),
        "{caso}: stdout {:?}",
        String::from_utf8_lossy(&u.stdout)
    );
    assert!(
        u.stderr.is_empty(),
        "{caso}: stderr {:?}",
        String::from_utf8_lossy(&u.stderr)
    );
    assert!(durata < Duration::from_millis(300), "{caso}: {durata:?}");
}

#[test]
fn eseguibile_vero_in_silenzio_senza_aprire_il_tubo() {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_isola-hook"));
    for (argomenti, stdin) in [
        (&["evento"][..], &b"non json"[..]),
        (&["evento"][..], &b""[..]),
        (
            &["evento"][..],
            &br#"{"session_id":"s1","hook_event_name":"SubagentStart"}"#[..],
        ),
        (&[][..], PERMESSO.as_bytes()),
        (&["altro"][..], PERMESSO.as_bytes()),
    ] {
        let (u, durata) = lancia(&exe, argomenti, stdin, None);
        silenzioso(&u, durata, &format!("{argomenti:?}"));
    }
    // Barra senza dati validi: «Claude» e nessun invio.
    let (u, _) = lancia(&exe, &["barra"], b"non json", None);
    assert!(u.status.success());
    assert_eq!(String::from_utf8_lossy(&u.stdout), "Claude\n");
    assert!(u.stderr.is_empty());
}

#[test]
fn senza_isola_esce_in_silenzio() {
    let Some(exe) = aiutante_prova() else { return };
    let tubo = tubo_unico();
    let (u, durata) = lancia(&exe, &["evento"], PERMESSO.as_bytes(), Some(&tubo));
    silenzioso(&u, durata, "permesso, tubo assente");
    // Senza destinazione (variabili assenti): come isola spenta.
    let (u, durata) = lancia(&exe, &["evento"], PERMESSO.as_bytes(), None);
    silenzioso(&u, durata, "permesso, nessuna destinazione");
}

#[test]
fn barra_stampa_solo_la_riga() {
    let Some(exe) = aiutante_prova() else { return };
    let tubo = tubo_unico();
    let (u, durata) = lancia(&exe, &["barra"], BARRA.as_bytes(), Some(&tubo));
    assert!(u.status.success());
    assert_eq!(
        String::from_utf8_lossy(&u.stdout),
        "Opus 5.5 \u{b7} contesto 42%\n"
    );
    assert!(u.stderr.is_empty());
    assert!(durata < Duration::from_millis(300), "{durata:?}");
}
