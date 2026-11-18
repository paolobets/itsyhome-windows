//! L'eseguibile `isola-hook` vero, lanciato come lo lancia Claude Code (forma exec, JSON su
//! stdin). Qui non c'è isola: la cartella dei test non contiene un `assistente.exe` acceso, quindi
//! l'aiutante deve uscire con 0, senza stdout né stderr, in fretta. I casi con un server (falso o
//! vero) sono nei test di `aiutante.rs` e `tubo.rs`, con nomi di tubo unici.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PERMESSO: &str = r#"{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"C:\x\p","tool_name":"Bash","tool_input":{"command":"ls"}}"#;

fn lancia(argomenti: &[&str], stdin: &[u8]) -> (std::process::Output, Duration) {
    let inizio = Instant::now();
    let mut figlio = Command::new(env!("CARGO_BIN_EXE_isola-hook"))
        .args(argomenti)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("avvio dell'aiutante");
    let mut ingresso = figlio.stdin.take().unwrap();
    let _ = ingresso.write_all(stdin);
    drop(ingresso);
    let uscita = figlio.wait_with_output().unwrap();
    (uscita, inizio.elapsed())
}

#[test]
fn senza_isola_esce_in_silenzio() {
    for (argomenti, stdin) in [
        (&["evento"][..], PERMESSO.as_bytes()),
        (&["evento"][..], &b"non json"[..]),
        (&["evento"][..], &b""[..]),
        (&[][..], PERMESSO.as_bytes()),
        (&["altro"][..], PERMESSO.as_bytes()),
    ] {
        let (u, durata) = lancia(argomenti, stdin);
        assert!(u.status.success(), "{argomenti:?}: {:?}", u.status);
        assert!(
            u.stdout.is_empty(),
            "{argomenti:?}: stdout {:?}",
            String::from_utf8_lossy(&u.stdout)
        );
        assert!(
            u.stderr.is_empty(),
            "{argomenti:?}: stderr {:?}",
            String::from_utf8_lossy(&u.stderr)
        );
        assert!(
            durata < Duration::from_millis(300),
            "{argomenti:?}: {durata:?}"
        );
    }
}

#[test]
fn barra_stampa_solo_la_riga() {
    let stdin = br#"{"session_id":"s1","model":{"display_name":"Opus 5.5"},"context_window":{"used_percentage":42}}"#;
    let (u, durata) = lancia(&["barra"], stdin);
    assert!(u.status.success());
    assert_eq!(
        String::from_utf8_lossy(&u.stdout),
        "Opus 5.5 \u{b7} contesto 42%\n"
    );
    assert!(u.stderr.is_empty());
    assert!(durata < Duration::from_millis(300), "{durata:?}");
}
