//! Il percorso corto 8.3 di un file (M3, Task 7B): serve al comando della barra di stato di Claude
//! Code, che passa da una shell e non ammette spazi nel percorso dell'aiutante.

/// Il percorso corto 8.3 di un file che esiste (`C:\PROGRA~1\…`), o `None` (volume senza nomi corti,
/// file assente).
#[cfg(windows)]
pub fn percorso_corto(p: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let largo: Vec<u16> = p
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `largo` è terminato da zero; il buffer è lungo quanto dichiarato.
    unsafe {
        let n = GetShortPathNameW(largo.as_ptr(), std::ptr::null_mut(), 0);
        if n == 0 {
            return None;
        }
        let mut buf = vec![0u16; n as usize];
        let scritti = GetShortPathNameW(largo.as_ptr(), buf.as_mut_ptr(), n);
        if scritti == 0 || scritti >= n {
            return None;
        }
        Some(std::ffi::OsString::from_wide(&buf[..scritti as usize]).into())
    }
}

/// Fuori da Windows non ci sono nomi corti.
#[cfg(not(windows))]
pub fn percorso_corto(_p: &std::path::Path) -> Option<std::path::PathBuf> {
    None
}

#[cfg(all(test, windows))]
mod test {
    use super::*;

    #[test]
    fn percorso_corto_di_un_file_che_non_esiste_e_none() {
        assert_eq!(
            percorso_corto(std::path::Path::new(r"C:\questo\non\esiste\proprio.exe")),
            None
        );
    }
}
