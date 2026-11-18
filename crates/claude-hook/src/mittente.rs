//! Chi ha aperto la connessione: il PID del processo che possiede il socket del mittente
//! (tabella TCP di Windows). Fuori da Windows non c'è: sempre `None`.

/// PID del processo con la connessione locale `porta_mittente` -> `porta_nostra`, se si trova.
#[cfg(not(windows))]
pub fn pid_mittente(_porta_mittente: u16, _porta_nostra: u16) -> Option<u32> {
    None
}

/// PID del processo con la connessione locale `porta_mittente` -> `porta_nostra`, se si trova.
#[cfg(windows)]
pub fn pid_mittente(porta_mittente: u16, porta_nostra: u16) -> Option<u32> {
    windows::pid_mittente(porta_mittente, porta_nostra)
}

/// Nomi degli eseguibili dal processo ai suoi genitori (al massimo 8).
#[cfg(windows)]
pub fn catena(pid: u32) -> Vec<String> {
    windows::catena(pid)
}

#[cfg(windows)]
mod windows {
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    const AF_INET: u32 = 2;
    /// 127.0.0.1 come lo scrive la tabella (ordine di rete, letto da un u32 little-endian).
    const LOOPBACK: u32 = u32::from_ne_bytes([127, 0, 0, 1]);
    const ERRORE_BUFFER_PICCOLO: u32 = 122;

    pub fn pid_mittente(porta_mittente: u16, porta_nostra: u16) -> Option<u32> {
        // Le porte della tabella sono in ordine di rete nei 16 bit bassi.
        let porta = |v: u32| u16::from_be((v & 0xFFFF) as u16);
        let mut dimensione: u32 = 0;
        // Prima chiamata: solo la dimensione richiesta.
        // SAFETY: puntatore nullo con dimensione zero è l'uso documentato per chiedere la taglia.
        unsafe {
            GetExtendedTcpTable(null_mut(), &mut dimensione, 0, AF_INET, TCP_TABLE_OWNER_PID_ALL, 0)
        };
        for _ in 0..4 {
            // Vec<u32>: allineamento adatto alla struttura.
            let mut buf = vec![0u32; (dimensione as usize).div_ceil(4) + 16];
            let mut d = (buf.len() * 4) as u32;
            // SAFETY: `buf` è grande `d` byte e vive per tutta la chiamata.
            let esito = unsafe {
                GetExtendedTcpTable(
                    buf.as_mut_ptr().cast(),
                    &mut d,
                    0,
                    AF_INET,
                    TCP_TABLE_OWNER_PID_ALL,
                    0,
                )
            };
            if esito == ERRORE_BUFFER_PICCOLO {
                dimensione = d;
                continue;
            }
            if esito != 0 {
                return None;
            }
            let tabella = buf.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
            // SAFETY: la tabella è stata riempita dal sistema dentro `buf`.
            let righe = unsafe {
                std::slice::from_raw_parts((*tabella).table.as_ptr(), (*tabella).dwNumEntries as usize)
            };
            return righe
                .iter()
                .find(|r| {
                    r.dwLocalAddr == LOOPBACK
                        && r.dwRemoteAddr == LOOPBACK
                        && porta(r.dwLocalPort) == porta_mittente
                        && porta(r.dwRemotePort) == porta_nostra
                })
                .map(|r| r.dwOwningPid);
        }
        None
    }

    pub fn catena(pid: u32) -> Vec<String> {
        let mut processi: Vec<(u32, u32, String)> = Vec::new();
        // SAFETY: istantanea chiusa sempre; la struttura parte azzerata con `dwSize` impostato.
        unsafe {
            let h = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if h == INVALID_HANDLE_VALUE {
                return vec!["(elenco processi non disponibile)".into()];
            }
            let mut e: PROCESSENTRY32W = zeroed();
            e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(h, &mut e) != 0;
            while ok {
                let fine = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                processi.push((
                    e.th32ProcessID,
                    e.th32ParentProcessID,
                    String::from_utf16_lossy(&e.szExeFile[..fine]),
                ));
                ok = Process32NextW(h, &mut e) != 0;
            }
            CloseHandle(h);
        }
        let mut catena = Vec::new();
        let mut corrente = pid;
        for _ in 0..8 {
            let Some((_, padre, nome)) = processi.iter().find(|p| p.0 == corrente) else {
                break;
            };
            catena.push(nome.clone());
            if *padre == 0 || *padre == corrente {
                break;
            }
            corrente = *padre;
        }
        catena
    }
}
