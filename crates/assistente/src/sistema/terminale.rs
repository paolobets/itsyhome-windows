//! Il terminale di una sessione di Claude Code (M3, Task 10): dal clic su una sessione si porta
//! davanti la finestra in cui gira. Niente tabella TCP (ruling PF19): all'arrivo del primo evento
//! di una sessione il tubo prende la catena dei processi dell'aiutante (aiutante → `claude.exe` →
//! shell → terminale, `Arrivo::catena_mittente`) e il ponte la tiene per sessione; al clic si
//! rilegge l'elenco dei processi, si tengono solo gli anelli della catena il cui PID ha ancora lo
//! STESSO nome di eseguibile (un PID riusato da un altro programma cade), si risale con i genitori
//! e si sceglie la prima finestra visibile di un terminale.
//!
//! Documentazione Microsoft letta prima di scrivere (learn.microsoft.com, 18/11/2026):
//!
//! `CreateToolhelp32Snapshot` (tlhelp32.h, kernel32):
//! - con `TH32CS_SNAPPROCESS` l'istantanea contiene tutti i processi (il secondo argomento è
//!   ignorato); a errore restituisce `INVALID_HANDLE_VALUE`; si chiude con `CloseHandle`;
//! - `Process32FirstW`/`Process32NextW` riempiono `PROCESSENTRY32W` (`dwSize` va impostato prima):
//!   `th32ProcessID`, `th32ParentProcessID` (il PID del genitore alla creazione: se il genitore è
//!   morto il numero può essere stato riusato, per questo la catena si salva subito e al clic si
//!   controlla il nome) e `szExeFile` (solo il nome del file, non il percorso).
//!
//! `GetWindowThreadProcessId`: restituisce il thread che ha creato la finestra (zero se l'handle non
//! vale) e scrive il PID del processo che la possiede. Per Windows Terminal la finestra appartiene
//! a `WindowsTerminal.exe` (la shell è un suo discendente, quindi è un antenato nella catena). Per
//! una console classica la finestra appartiene a `conhost.exe`, che nell'albero è un FIGLIO della
//! shell, non un antenato: per questo le finestre di `conhost.exe`/`OpenConsole.exe` figli di un
//! anello della catena si attribuiscono a quell'anello (`finestre_attribuite`). Sotto Windows
//! Terminal la console finta (ConPTY) ha la finestra nascosta: `IsWindowVisible` la scarta.
//!
//! `EnumWindows` passa le finestre di primo livello; `IsWindowVisible` dice se hanno lo stile
//! visibile; `GetWindow(GW_OWNER)` non nullo = finestra con proprietario (dialoghi, menu): si
//! scartano, resta la finestra principale.
//!
//! `SetForegroundWindow`: un processo può portare in primo piano una finestra solo se, fra
//! l'altro, è il processo in primo piano o ha ricevuto l'ultimo evento di input (altrimenti
//! Windows fa solo lampeggiare il pulsante nella barra). Qui lo si chiama subito dopo il clic o
//! l'Invio di Paolo sul pannello o sulla pillola, cioè mentre l'isola ha l'input: mai da timer o da
//! eventi. `AllowSetForegroundWindow` serve a dare il permesso a ALTRI processi: non ci serve.
//! Una finestra ridotta a icona non torna visibile da sola: `IsIconic` → `ShowWindow(SW_RESTORE)`.
//!
//! Windows Terminal: si porta davanti la finestra, NON la scheda (nessuna API pubblica per
//! scegliere la scheda di un altro processo): la vista lo dice.

/// Un processo dell'elenco di sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processo {
    pub pid: u32,
    pub genitore: u32,
    /// Il nome del file eseguibile («pwsh.exe»).
    pub exe: String,
}

/// I programmi che sono (o ospitano) un terminale.
pub const TERMINALI: [&str; 9] = [
    "WindowsTerminal.exe",
    "OpenConsole.exe",
    "conhost.exe",
    "powershell.exe",
    "pwsh.exe",
    "cmd.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "Code.exe",
];

/// I passi massimi di una catena.
const PASSI_MASSIMI: usize = 16;

pub fn e_terminale(exe: &str) -> bool {
    TERMINALI.iter().any(|t| t.eq_ignore_ascii_case(exe))
}

/// Il processo `pid` e i suoi genitori, dal più vicino (al più 16 passi); si ferma al primo
/// genitore che non c'è più nell'elenco e a ogni ciclo.
pub fn catena(pid: u32, processi: &[Processo]) -> Vec<&Processo> {
    let mut v: Vec<&Processo> = Vec::new();
    let mut corrente = pid;
    while v.len() < PASSI_MASSIMI {
        let Some(p) = processi.iter().find(|p| p.pid == corrente) else {
            break;
        };
        if v.iter().any(|x| x.pid == p.pid) {
            break;
        }
        v.push(p);
        if p.genitore == 0 {
            break;
        }
        corrente = p.genitore;
    }
    v
}

/// Il primo antenato, dal più vicino, con una finestra visibile di primo livello.
pub fn scegli_finestra(catena: &[&Processo], finestre: &[(u32, isize)]) -> Option<isize> {
    catena.iter().find_map(|p| {
        finestre
            .iter()
            .find(|(pid, _)| *pid == p.pid)
            .map(|(_, h)| *h)
    })
}

/// Gli anelli della catena salvata alla prima richiesta che esistono ancora con lo STESSO nome di
/// eseguibile e, dove c'è un anello sopra, con quello stesso come genitore: un PID riusato da un
/// altro programma (nome diverso) o da un'altra istanza dello stesso (genitore diverso) cade.
/// Nell'ordine salvato.
pub fn anelli_validi<'a>(salvata: &[(u32, String)], processi: &'a [Processo]) -> Vec<&'a Processo> {
    salvata
        .iter()
        .enumerate()
        .filter_map(|(i, (pid, nome))| {
            let p = processi.iter().find(|p| p.pid == *pid && p.exe.eq_ignore_ascii_case(nome))?;
            // Anche il genitore di adesso deve essere l'anello che stava sopra: chi ha preso il
            // numero di un processo morto (magari lo stesso programma, un'altra shell) ha quasi
            // sempre un altro genitore.
            match salvata.get(i + 1) {
                Some((genitore, _)) if p.genitore != *genitore => None,
                _ => Some(p),
            }
        })
        .collect()
}

/// Gli anelli validi e, dopo l'ultimo, i suoi genitori nell'elenco di adesso (la catena salvata è
/// lunga al più 6). Poi solo i terminali: il resto (explorer.exe ha una finestra, la barra) non è
/// mai la meta, e un genitore il cui numero è stato riusato non porta a una finestra a caso.
pub fn catena_della_sessione<'a>(
    salvata: &[(u32, String)],
    processi: &'a [Processo],
) -> Vec<&'a Processo> {
    let mut v = anelli_validi(salvata, processi);
    if let Some(ultimo) = v.last().copied() {
        for p in catena(ultimo.pid, processi) {
            if !v.iter().any(|x| x.pid == p.pid) {
                v.push(p);
            }
        }
    }
    v.retain(|p| e_terminale(&p.exe));
    v
}

/// Le finestre come se fossero dei processi della catena: quelle di `conhost.exe`/`OpenConsole.exe`
/// figli di un anello (la console classica) passano all'anello; le altre restano come sono.
pub fn finestre_attribuite(
    catena: &[&Processo],
    processi: &[Processo],
    finestre: &[(u32, isize)],
) -> Vec<(u32, isize)> {
    finestre
        .iter()
        .map(|&(pid, h)| {
            let genitore = processi
                .iter()
                .find(|p| {
                    p.pid == pid
                        && (p.exe.eq_ignore_ascii_case("conhost.exe")
                            || p.exe.eq_ignore_ascii_case("OpenConsole.exe"))
                })
                .map(|p| p.genitore)
                .filter(|g| catena.iter().any(|c| c.pid == *g));
            (genitore.unwrap_or(pid), h)
        })
        .collect()
}

/// La finestra del terminale di una sessione, dalla catena salvata, l'elenco dei processi di adesso
/// e le finestre visibili.
pub fn finestra_della_sessione(
    salvata: &[(u32, String)],
    processi: &[Processo],
    finestre: &[(u32, isize)],
) -> Option<isize> {
    let c = catena_della_sessione(salvata, processi);
    if c.is_empty() {
        return None;
    }
    let f = finestre_attribuite(&c, processi, finestre);
    scegli_finestra(&c, &f)
}

#[cfg(windows)]
#[cfg_attr(test, allow(unused_imports))]
pub use sistema::{finestre_visibili, porta_davanti, processi};

#[cfg(windows)]
mod sistema {
    use super::Processo;
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LPARAM};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
    };

    struct Chiudi(HANDLE);
    impl Drop for Chiudi {
        fn drop(&mut self) {
            // SAFETY: handle valido dell'istantanea, chiuso una volta sola.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// I processi di adesso (`CreateToolhelp32Snapshot`); vuoto se il sistema rifiuta.
    pub fn processi() -> Vec<Processo> {
        let mut v = Vec::new();
        // SAFETY: istantanea chiusa sempre; la struttura parte azzerata con `dwSize` impostato.
        unsafe {
            let h = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if h == INVALID_HANDLE_VALUE {
                return v;
            }
            let h = Chiudi(h);
            let mut e: PROCESSENTRY32W = zeroed();
            e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(h.0, &mut e) != 0;
            while ok {
                let fine = e
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(e.szExeFile.len());
                v.push(Processo {
                    pid: e.th32ProcessID,
                    genitore: e.th32ParentProcessID,
                    exe: String::from_utf16_lossy(&e.szExeFile[..fine]),
                });
                ok = Process32NextW(h.0, &mut e) != 0;
            }
        }
        v
    }

    unsafe extern "system" fn per_finestra(hwnd: isize, lparam: LPARAM) -> i32 {
        // SAFETY: `lparam` è il puntatore al vettore di `finestre_visibili`, vivo per tutta la
        // chiamata a `EnumWindows` (sincrona, stesso thread).
        let v = unsafe { &mut *(lparam as *mut Vec<(u32, isize)>) };
        // SAFETY: funzioni di sola lettura su un handle di finestra (anche se è sparito nel frattempo).
        unsafe {
            if IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER) == 0 {
                let mut pid = 0u32;
                if GetWindowThreadProcessId(hwnd, &mut pid) != 0 {
                    v.push((pid, hwnd));
                }
            }
        }
        1
    }

    /// Le finestre visibili di primo livello e senza proprietario: (PID del processo, handle).
    pub fn finestre_visibili() -> Vec<(u32, isize)> {
        let mut v: Vec<(u32, isize)> = Vec::new();
        // SAFETY: la funzione di ritorno è `extern "system"` e il puntatore al vettore vive più
        // della chiamata, che è sincrona.
        unsafe {
            EnumWindows(
                Some(per_finestra),
                &mut v as *mut Vec<(u32, isize)> as LPARAM,
            );
        }
        v
    }

    /// Porta davanti la finestra (ridotta a icona: prima la ripristina). Da chiamare solo subito
    /// dopo un clic o un Invio dell'utente, quando Windows lo permette. `true` se ha avuto effetto.
    #[cfg_attr(test, allow(dead_code))]
    pub fn porta_davanti(hwnd: isize) -> bool {
        // SAFETY: chiamate su un handle di finestra; con un handle non più valido falliscono e basta.
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd) != 0
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn p(pid: u32, genitore: u32, exe: &str) -> Processo {
        Processo {
            pid,
            genitore,
            exe: exe.into(),
        }
    }

    fn albero() -> Vec<Processo> {
        vec![
            p(10, 20, "claude.exe"),
            p(20, 30, "pwsh.exe"),
            p(30, 40, "WindowsTerminal.exe"),
            p(40, 0, "explorer.exe"),
        ]
    }

    #[test]
    fn catena_dei_genitori() {
        let a = albero();
        let c = catena(10, &a);
        let nomi: Vec<&str> = c.iter().map(|x| x.exe.as_str()).collect();
        assert_eq!(
            nomi,
            [
                "claude.exe",
                "pwsh.exe",
                "WindowsTerminal.exe",
                "explorer.exe"
            ]
        );
        // Ciclo pid → pid (e a due): si ferma.
        let ciclo = vec![p(1, 2, "a.exe"), p(2, 1, "b.exe"), p(3, 3, "c.exe")];
        assert_eq!(catena(1, &ciclo).len(), 2);
        assert_eq!(catena(3, &ciclo).len(), 1);
        // Un genitore che non c'è più chiude la catena; un pid ignoto è una catena vuota.
        assert_eq!(catena(10, &a[..2]).len(), 2);
        assert!(catena(99, &a).is_empty());
    }

    #[test]
    fn catena_lunga_al_piu_sedici_passi() {
        let lunga: Vec<Processo> = (1..=40).map(|i| p(i, i + 1, "x.exe")).collect();
        assert_eq!(catena(1, &lunga).len(), 16);
    }

    #[test]
    fn finestra_del_primo_antenato_con_finestra() {
        let a = albero();
        let c = catena(10, &a);
        // pwsh non ha finestra, WindowsTerminal sì (42); anche explorer ha la sua, più lontana.
        assert_eq!(scegli_finestra(&c, &[(30, 42), (40, 7)]), Some(42));
        // Se ne hanno due, vince il più vicino.
        assert_eq!(scegli_finestra(&c, &[(30, 42), (20, 5)]), Some(5));
    }

    #[test]
    fn nessuna_finestra_nessuna_scelta() {
        let a = albero();
        let c = catena(10, &a);
        assert_eq!(scegli_finestra(&c, &[(999, 1)]), None);
        assert_eq!(scegli_finestra(&c, &[]), None);
        assert_eq!(scegli_finestra(&[], &[(30, 42)]), None);
    }

    #[test]
    fn terminali_riconosciuti() {
        assert!(e_terminale("WindowsTerminal.exe"));
        assert!(e_terminale("windowsterminal.exe"));
        assert!(e_terminale("PWSH.EXE"));
        assert!(!e_terminale("notepad.exe"));
        assert!(!e_terminale("explorer.exe"));
    }

    fn salvata() -> Vec<(u32, String)> {
        vec![
            (5, "isola-hook.exe".into()),
            (10, "claude.exe".into()),
            (20, "pwsh.exe".into()),
            (30, "WindowsTerminal.exe".into()),
        ]
    }

    #[test]
    fn la_catena_salvata_trova_la_finestra() {
        // L'aiutante (5) è morto da un pezzo: cade da sé; il resto è ancora vivo.
        assert_eq!(
            finestra_della_sessione(&salvata(), &albero(), &[(30, 42), (40, 7)]),
            Some(42)
        );
    }

    #[test]
    fn un_pid_riusato_da_un_altro_programma_cade() {
        // Il terminale (30) è stato chiuso e il suo numero è passato a un altro programma, che ha una
        // finestra: non va portata davanti.
        let mut a = albero();
        a[2] = p(30, 40, "notepad.exe");
        assert_eq!(finestra_della_sessione(&salvata(), &a, &[(30, 42)]), None);
        let valide: Vec<u32> = anelli_validi(&salvata(), &a)
            .iter()
            .map(|x| x.pid)
            .collect();
        assert_eq!(valide, [10, 20], "30 ha cambiato eseguibile: scartato");
        // Se a cambiare è un anello in mezzo (20), il terminale sopra resta valido.
        let mut b = albero();
        b[1] = p(20, 30, "chrome.exe");
        let valide: Vec<u32> = anelli_validi(&salvata(), &b)
            .iter()
            .map(|x| x.pid)
            .collect();
        assert_eq!(valide, [10, 30]);
        assert_eq!(
            finestra_della_sessione(&salvata(), &b, &[(30, 42)]),
            Some(42)
        );
    }

    #[test]
    fn un_pid_riusato_dallo_stesso_programma_con_altro_genitore_cade() {
        // La shell (20) è morta e il suo numero è passato a un'altra pwsh.exe, con un altro genitore e
        // una finestra sua (5): non è quella di Claude.
        let mut a = albero();
        a[1] = p(20, 99, "pwsh.exe");
        let valide: Vec<u32> = anelli_validi(&salvata(), &a).iter().map(|x| x.pid).collect();
        assert_eq!(valide, [10, 30], "20 ha un genitore diverso da 30: scartato");
        assert_eq!(finestra_della_sessione(&salvata(), &a, &[(20, 5), (30, 42)]), Some(42));
        // L'ultimo anello salvato non ha un «sopra» da confrontare: resta.
        let sola = vec![(30, "WindowsTerminal.exe".to_owned())];
        assert_eq!(anelli_validi(&sola, &albero()).len(), 1);
    }

    #[test]
    fn il_nome_si_confronta_senza_badare_alle_maiuscole() {
        let mut a = albero();
        a[2] = p(30, 40, "windowsterminal.exe");
        assert_eq!(
            finestra_della_sessione(&salvata(), &a, &[(30, 42)]),
            Some(42)
        );
    }

    #[test]
    fn senza_anelli_validi_nessuna_scelta() {
        assert_eq!(finestra_della_sessione(&[], &albero(), &[(30, 42)]), None);
        assert_eq!(
            finestra_della_sessione(&[(10, "altro.exe".into())], &albero(), &[(30, 42)]),
            None
        );
    }

    #[test]
    fn la_catena_si_estende_in_su_ma_solo_coi_terminali() {
        // Salvata solo fino a claude.exe: si risale con i genitori dell'elenco di adesso.
        let corta = vec![(10, "claude.exe".to_owned())];
        assert_eq!(
            finestra_della_sessione(&corta, &albero(), &[(30, 42)]),
            Some(42)
        );
        // La finestra di explorer.exe (la barra) non è mai la meta.
        assert_eq!(finestra_della_sessione(&corta, &albero(), &[(40, 7)]), None);
    }

    #[test]
    fn la_console_classica_ha_la_finestra_nel_figlio_conhost() {
        // claude → powershell, e conhost.exe (figlio di powershell) possiede la finestra.
        let a = vec![
            p(10, 20, "claude.exe"),
            p(20, 40, "powershell.exe"),
            p(25, 20, "conhost.exe"),
            p(40, 0, "explorer.exe"),
        ];
        let s = vec![
            (10, "claude.exe".to_owned()),
            (20, "powershell.exe".to_owned()),
        ];
        assert_eq!(finestra_della_sessione(&s, &a, &[(25, 99)]), Some(99));
        // Un conhost di un'altra console (genitore fuori catena) non si prende.
        let altra = vec![
            p(10, 20, "claude.exe"),
            p(20, 0, "powershell.exe"),
            p(26, 77, "conhost.exe"),
            p(77, 0, "cmd.exe"),
        ];
        assert_eq!(finestra_della_sessione(&s, &altra, &[(26, 55)]), None);
    }

    #[cfg(windows)]
    #[test]
    fn i_processi_contengono_il_nostro() {
        let miei = processi();
        let io = std::process::id();
        let mio = miei
            .iter()
            .find(|x| x.pid == io)
            .expect("il nostro processo c'è");
        let exe = std::env::current_exe().unwrap();
        let nome = exe.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            mio.exe.eq_ignore_ascii_case(&nome),
            "{} contro {nome}",
            mio.exe
        );
    }

    #[cfg(windows)]
    #[test]
    fn le_finestre_visibili_hanno_handle_e_processo() {
        // Solo lettura: nessuna finestra si tocca. Ogni voce ha un handle e un processo veri.
        for (pid, hwnd) in finestre_visibili() {
            assert!(pid != 0 && hwnd != 0, "finestra senza processo o handle: {pid} {hwnd}");
        }
    }
}
