//! Il menu dell'isola: «Apri», «Impostazioni», «Informazioni» (la schermata «About» con
//! AboutSlint, richiesta dalla licenza Slint royalty-free 2.0, §2(a), «accessible from the top
//! level menu»), «Ritira l'isola» / «Mostra l'isola», «Esci».
//!
//! È un menu nativo di Windows (`muda`, TrackPopupMenu). Con tutte le voci si apre dall'icona
//! nella barra se l'utente l'ha accesa (Impostazioni › Sistema); Esc o un clic altrove lo
//! chiudono. Le scelte arrivano dal gestore globale di `muda` e si inoltrano al ciclo eventi con
//! `invoke_from_event_loop`. Il clic destro sulla pillola non lo apre più (16/11, Paolo).
//!
//! Dal pulsante «⋯» in alto a destra del pannello (Task 14i) si apre il gemello senza «Apri» (il
//! pannello è già aperto), sotto il pulsante e allineato al suo bordo destro: `muda` lo apre solo
//! allineato a sinistra, quindi lo si apre qui con TrackPopupMenu (allineato a destra, dentro
//! l'area di lavoro) e la scelta torna come comando, senza passare dal gestore di `muda`.
//!
//! La linea della ritirata non apre il menu: lascia passare i clic (non riceve il mouse). Ci si
//! torna tenendo il mouse sulla linea, con Ctrl+Alt+Maiusc+I o Ctrl+Alt+Maiusc+Spazio.
//!
//! Sottomenu «Schermo» (M2, Task 6): fra «Informazioni» e «Ritira l'isola», una voce a spunta per
//! monitor, SOLO con più di un monitor. Sta in tutti e due i menu (icona nella barra e «⋯»).
//! Con `muda` per l'icona nella barra non c'è un evento «prima dell'apertura», quindi il menu non
//! si ricostruisce a ogni apertura: si ricostruisce quando cambia l'elenco dei monitor (o la
//! spunta), controllato dal giro da 5 s dei servizi e subito dopo una scelta; il «⋯» lo ricontrolla
//! anche un attimo prima di aprirsi. Una scelta fatta nei pochi secondi fra un cambio di monitor e
//! il controllo successivo si legge sull'elenco vero di quel momento (`Comando::Schermo(i)` è un
//! indice): se l'indice non esiste più, non succede nulla.
//!
//! Fuori da Windows per ora niente menu (su Linux `muda` richiede GTK): «⋯» apre direttamente
//! le Impostazioni, e lo schermo non si sceglie da nessuna parte; la preferenza `Config.schermo`,
//! se scritta nel file, vale lo stesso.

use crate::finestre::InfoMonitor;

/// Cosa chiede il menu (o il clic sull'icona nella barra).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comando {
    Apri,
    Impostazioni,
    /// Le Impostazioni sulla sezione «Informazioni».
    Informazioni,
    /// Ritira la pillola o la rimette (secondo lo stato).
    Ritirata,
    /// Sposta pillola e pannello sul monitor di questa posizione dell'elenco mostrato nel menu.
    Schermo(usize),
    Esci,
}

/// Testo della voce della ritirata.
pub fn testo_ritirata(ritirata: bool) -> &'static str {
    if ritirata {
        "Mostra l'isola"
    } else {
        "Ritira l'isola"
    }
}

/// Una voce di un monitor nel sottomenu «Schermo».
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct VoceSchermo {
    pub comando: Comando,
    pub testo: String,
    /// La spunta: il monitor su cui stanno adesso pillola e pannello.
    pub scelto: bool,
}

/// Una riga del menu, prima del separatore e di «Esci».
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Voce {
    Semplice(Comando, String),
    /// Il sottomenu «Schermo»: c'è solo con più di un monitor.
    Schermo(Vec<VoceSchermo>),
}

/// Le voci del sottomenu «Schermo» (testo, spunta) dall'elenco dei monitor: la spunta sul monitor
/// dove stanno pillola e pannello (`scelto`, indice nell'elenco).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn schermi_del_menu(monitor: &[InfoMonitor], scelto: Option<usize>) -> Vec<(String, bool)> {
    (0..monitor.len())
        .map(|i| (crate::finestre::etichetta_monitor(monitor, i), Some(i) == scelto))
        .collect()
}

/// Le voci del menu, in ordine. `schermi`: (testo, spunta) di ogni monitor; il sottomenu «Schermo»
/// c'è solo con più di uno. Il separatore sta prima di «Esci».
#[cfg_attr(not(windows), allow(dead_code))]
pub fn voci(ritirata: bool, schermi: &[(String, bool)]) -> Vec<Voce> {
    let semplice = |c, t: &str| Voce::Semplice(c, t.to_owned());
    let mut v = vec![semplice(Comando::Apri, "Apri"), semplice(Comando::Impostazioni, "Impostazioni"), semplice(Comando::Informazioni, "Informazioni")];
    if schermi.len() > 1 {
        v.push(Voce::Schermo(
            schermi
                .iter()
                .enumerate()
                .map(|(i, (testo, scelto))| VoceSchermo { comando: Comando::Schermo(i), testo: testo.clone(), scelto: *scelto })
                .collect(),
        ));
    }
    v.push(semplice(Comando::Ritirata, testo_ritirata(ritirata)));
    v.push(semplice(Comando::Esci, "Esci"));
    v
}

/// Le voci del menu del pannello («⋯»): le stesse, senza «Apri».
#[cfg_attr(not(windows), allow(dead_code))]
pub fn voci_pannello(ritirata: bool, schermi: &[(String, bool)]) -> Vec<Voce> {
    let mut v = voci(ritirata, schermi);
    v.remove(0);
    v
}

/// Cosa c'è a ogni posizione del menu nativo (primo livello).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
enum Posizione {
    Separatore,
    Comando(Comando),
    /// Il sottomenu «Schermo»: i comandi delle sue righe.
    Sotto(Vec<Comando>),
}

/// Cosa c'è a ogni posizione del menu nativo, separatore compreso: il separatore va prima di
/// «Esci».
// Fuori da Windows il menu non c'è: restano per i test.
#[cfg_attr(not(windows), allow(dead_code))]
fn posizioni(voci: &[Voce]) -> Vec<Posizione> {
    let mut p = Vec::with_capacity(voci.len() + 1);
    for voce in voci {
        match voce {
            Voce::Semplice(c, _) => {
                if *c == Comando::Esci {
                    p.push(Posizione::Separatore);
                }
                p.push(Posizione::Comando(*c));
            }
            Voce::Schermo(righe) => p.push(Posizione::Sotto(righe.iter().map(|r| r.comando).collect())),
        }
    }
    p
}

/// Il punto del menu del pannello in pixel fisici dell'area client, dal bordo destro e basso del
/// pulsante «⋯» in pixel logici.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn punto_pannello(destra: f32, basso: f32, scala: f32) -> (i32, i32) {
    ((destra * scala).round() as i32, (basso * scala).round() as i32)
}

/// Cosa non deve arrivare al menu del pannello aperto da tastiera: l'Invio o lo Spazio ancora
/// tenuti (auto-ripetizione, bit 30 di `lparam`: il tasto era già giù) sceglierebbero da soli la
/// prima voce evidenziata; i loro caratteri (WM_CHAR) non sono iniziali di nessuna voce e il menu
/// farebbe solo il suono d'errore. Una nuova pressione di Invio sceglie come sempre.
#[cfg_attr(not(windows), allow(dead_code))]
fn da_ignorare_nel_menu(messaggio: u32, tasto: usize, lparam: isize) -> bool {
    const WM_KEYDOWN: u32 = 0x100;
    const WM_CHAR: u32 = 0x102;
    const VK_RETURN: usize = 0x0D;
    const VK_SPACE: usize = 0x20;
    let invio_o_spazio = tasto == VK_RETURN || tasto == VK_SPACE;
    match messaggio {
        WM_KEYDOWN => invio_o_spazio && lparam & (1 << 30) != 0,
        WM_CHAR => invio_o_spazio,
        _ => false,
    }
}

/// L'identificativo di una voce nel menu nativo, e ritorno.
// Fuori da Windows il menu non c'è: restano per i test.
#[cfg_attr(not(windows), allow(dead_code))]
fn id(c: Comando) -> String {
    match c {
        Comando::Apri => "apri".into(),
        Comando::Impostazioni => "impostazioni".into(),
        Comando::Informazioni => "informazioni".into(),
        Comando::Ritirata => "ritirata".into(),
        Comando::Schermo(i) => format!("schermo:{i}"),
        Comando::Esci => "esci".into(),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn comando(id: &str) -> Option<Comando> {
    if let Some(i) = id.strip_prefix("schermo:") {
        return i.parse().ok().map(Comando::Schermo);
    }
    [Comando::Apri, Comando::Impostazioni, Comando::Informazioni, Comando::Ritirata, Comando::Esci]
        .into_iter()
        .find(|c| self::id(*c) == id)
}

#[cfg(windows)]
pub use windows::MenuIsola;

#[cfg(windows)]
mod windows {
    use super::{
        comando, da_ignorare_nel_menu, id, posizioni, testo_ritirata, voci, voci_pannello, Comando, Posizione, Voce,
    };
    use tray_icon::menu::{CheckMenuItem, ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    pub struct MenuIsola {
        menu: Menu,
        ritirata: MenuItem,
        /// Il gemello del pannello («⋯»): senza «Apri».
        pannello: Menu,
        ritirata_pannello: MenuItem,
        /// Cosa c'è a ogni posizione del menu del pannello (separatore compreso).
        posizioni_pannello: Vec<Posizione>,
        /// I monitor con cui sono costruiti i menu: (testo, spunta). Con più di uno c'è «Schermo».
        schermi: Vec<(String, bool)>,
    }

    /// Un menu nativo con queste voci (separatore prima di «Esci») e la sua voce della ritirata.
    pub(super) fn costruisci(voci: &[Voce]) -> Result<(Menu, MenuItem), String> {
        let errore = |e: tray_icon::menu::Error| e.to_string();
        let menu = Menu::new();
        let mut voce_ritirata = None;
        for v in voci {
            match v {
                Voce::Semplice(c, testo) => {
                    let voce = MenuItem::with_id(id(*c), testo, true, None);
                    if *c == Comando::Esci {
                        menu.append(&PredefinedMenuItem::separator()).map_err(errore)?;
                    }
                    menu.append(&voce).map_err(errore)?;
                    if *c == Comando::Ritirata {
                        voce_ritirata = Some(voce);
                    }
                }
                Voce::Schermo(righe) => {
                    let sotto = Submenu::new("Schermo", true);
                    for r in righe {
                        sotto.append(&CheckMenuItem::with_id(id(r.comando), &r.testo, true, r.scelto, None)).map_err(errore)?;
                    }
                    menu.append(&sotto).map_err(errore)?;
                }
            }
        }
        let ritirata = voce_ritirata.ok_or("voce della ritirata mancante")?;
        Ok((menu, ritirata))
    }

    /// Filtro dei messaggi del ciclo del menu (WH_MSGFILTER, solo sul thread dell'interfaccia e
    /// solo mentre il menu del pannello è aperto): toglie quello che dice `da_ignorare_nel_menu`.
    unsafe extern "system" fn filtro_menu(codice: i32, wparam: usize, lparam: isize) -> isize {
        use windows_sys::Win32::UI::WindowsAndMessaging::{CallNextHookEx, MSG, MSGF_MENU, WM_NULL};
        if codice == MSGF_MENU as i32 && lparam != 0 {
            // SAFETY: per WH_MSGFILTER `lparam` punta al MSG in esame, valido durante la chiamata.
            let msg = &mut *(lparam as *mut MSG);
            if da_ignorare_nel_menu(msg.message, msg.wParam, msg.lParam) {
                // Il messaggio è già tolto dalla coda: diventa un WM_NULL e il menu non lo vede.
                msg.message = WM_NULL;
                return 1;
            }
        }
        CallNextHookEx(0, codice, wparam, lparam)
    }

    impl MenuIsola {
        /// Crea il menu sul thread dell'interfaccia. `al_comando` è chiamata sul ciclo eventi.
        pub fn crea(al_comando: fn(Comando), ritirata: bool) -> Result<MenuIsola, String> {
            let m = Self::costruito(ritirata, Vec::new())?;
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if let Some(c) = comando(e.id().as_ref()) {
                    let _ = slint::invoke_from_event_loop(move || al_comando(c));
                }
            }));
            Ok(m)
        }

        /// I due menu costruiti per questi monitor.
        fn costruito(ritirata: bool, schermi: Vec<(String, bool)>) -> Result<MenuIsola, String> {
            let (menu, voce_ritirata) = costruisci(&voci(ritirata, &schermi))?;
            let voci_pannello = voci_pannello(ritirata, &schermi);
            let (pannello, ritirata_pannello) = costruisci(&voci_pannello)?;
            Ok(MenuIsola {
                menu,
                ritirata: voce_ritirata,
                pannello,
                ritirata_pannello,
                posizioni_pannello: posizioni(&voci_pannello),
                schermi,
            })
        }

        /// Ricostruisce i due menu se cambiano i monitor o la spunta (`schermi`: testo e spunta di
        /// ognuno). Ritorna `true` se li ha ricostruiti: chi tiene il menu dell'icona nella barra
        /// deve passarle quello nuovo (`menu()`).
        pub fn schermi(&mut self, schermi: Vec<(String, bool)>, ritirata: bool) -> Result<bool, String> {
            if schermi == self.schermi {
                return Ok(false);
            }
            *self = Self::costruito(ritirata, schermi)?;
            Ok(true)
        }

        /// I menu sono costruiti per questi stessi monitor (stessi testi, spunta a parte)?
        pub fn stesso_elenco(&self, schermi: &[(String, bool)]) -> bool {
            self.schermi.len() == schermi.len() && self.schermi.iter().zip(schermi).all(|(a, b)| a.0 == b.0)
        }

        pub fn ritirata(&self, attiva: bool) {
            let testo = testo_ritirata(attiva);
            for voce in [&self.ritirata, &self.ritirata_pannello] {
                if voce.text() != testo {
                    voce.set_text(testo);
                }
            }
        }

        /// Lo stesso menu, per l'icona nella barra.
        pub fn menu(&self) -> &Menu {
            &self.menu
        }

        /// Apre il menu del pannello nella finestra `hwnd`, col bordo DESTRO in alto a `(x, y)`
        /// (pixel fisici dell'area client), dentro l'area di lavoro. Ritorna quando il menu si
        /// chiude, con la voce scelta (`None`: Esc o clic altrove). Il gestore di `muda` non
        /// riceve nulla: la scelta la porta chi chiama. `prima_voce`: aperto da tastiera (o da
        /// Narrator), parte con la prima voce evidenziata, come i menu di Windows aperti da tasto.
        pub fn mostra_dal_pannello(&self, hwnd: isize, (x, y): (i32, i32), prima_voce: bool) -> Option<Comando> {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
            use windows_sys::Win32::System::Threading::GetCurrentThreadId;
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_DOWN;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetMenuItemCount, GetMenuItemID, GetSubMenu, PostMessageW, SetForegroundWindow, SetWindowsHookExW, TrackPopupMenu,
                UnhookWindowsHookEx, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_TOPALIGN, TPM_WORKAREA, WH_MSGFILTER, WM_KEYDOWN,
            };
            let hmenu = self.pannello.hpopupmenu();
            let mut punto = POINT { x, y };
            // SAFETY: hwnd è la finestra viva del pannello e hmenu il menu vivo di `self`, tutti e due
            // sul thread dell'interfaccia; TrackPopupMenu tiene il ciclo dei messaggi finché il
            // menu è aperto e con TPM_RETURNCMD non manda WM_COMMAND a nessuno.
            let scelto = unsafe {
                ClientToScreen(hwnd, &mut punto);
                // TrackPopupMenu vuole la finestra in primo piano (lo è già: il clic era nel pannello).
                SetForegroundWindow(hwnd);
                // Un ↓ in coda alla finestra del pannello (la nostra): lo consuma il ciclo del menu
                // appena aperto ed evidenzia la prima voce.
                if prima_voce {
                    PostMessageW(hwnd, WM_KEYDOWN, VK_DOWN as usize, 0);
                }
                // Il filtro vale solo per questo thread e solo finché il menu è aperto.
                let filtro = SetWindowsHookExW(WH_MSGFILTER, Some(filtro_menu), 0, GetCurrentThreadId());
                let scelto = TrackPopupMenu(
                    hmenu,
                    TPM_RIGHTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_WORKAREA,
                    punto.x,
                    punto.y,
                    0,
                    hwnd,
                    std::ptr::null(),
                );
                if filtro != 0 {
                    UnhookWindowsHookEx(filtro);
                }
                scelto
            };
            if scelto <= 0 {
                return None;
            }
            // SAFETY: solo letture del menu vivo di `self` e dei suoi sottomenu.
            unsafe {
                (0..GetMenuItemCount(hmenu)).find_map(|i| match self.posizioni_pannello.get(i as usize)? {
                    Posizione::Comando(c) => (GetMenuItemID(hmenu, i) == scelto as u32).then_some(*c),
                    Posizione::Sotto(righe) => {
                        let sotto = GetSubMenu(hmenu, i);
                        (0..righe.len() as i32)
                            .find(|&j| GetMenuItemID(sotto, j) == scelto as u32)
                            .map(|j| righe[j as usize])
                    }
                    Posizione::Separatore => None,
                })
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Due monitor: il secondo con la spunta.
    fn due_schermi() -> Vec<(String, bool)> {
        vec![("Schermo principale".into(), false), ("Schermo 2 · 2560×1440".into(), true)]
    }

    /// I testi delle voci di primo livello (il sottomenu come «Schermo»).
    fn testi(voci: &[Voce]) -> Vec<String> {
        voci.iter()
            .map(|v| match v {
                Voce::Semplice(_, t) => t.clone(),
                Voce::Schermo(_) => "Schermo".to_owned(),
            })
            .collect()
    }

    #[test]
    fn le_voci_del_menu_e_la_ritirata() {
        assert_eq!(testi(&voci(false, &[])), ["Apri", "Impostazioni", "Informazioni", "Ritira l'isola", "Esci"]);
        assert_eq!(voci(true, &[])[3], Voce::Semplice(Comando::Ritirata, "Mostra l'isola".into()));
    }

    #[test]
    fn il_menu_del_pannello_senza_apri_con_impostazioni_in_testa() {
        // Task 14i: dal «⋯» del pannello (già aperto) niente «Apri»; il resto come nel menu dell'icona nella barra.
        assert_eq!(testi(&voci_pannello(false, &[])), ["Impostazioni", "Informazioni", "Ritira l'isola", "Esci"]);
        assert_eq!(voci_pannello(true, &[])[2], Voce::Semplice(Comando::Ritirata, "Mostra l'isola".into()));
    }

    #[test]
    fn schermo_solo_con_piu_monitor() {
        // Un monitor solo, o nessuno: niente «Schermo». Con due: il sottomenu fra «Informazioni» e
        // «Ritira l'isola», una voce per monitor e la spunta sul preferito.
        assert!(!testi(&voci(false, &[])).contains(&"Schermo".to_owned()));
        assert!(!testi(&voci(false, &[("Schermo principale".into(), true)])).contains(&"Schermo".to_owned()));
        let v = voci(false, &due_schermi());
        assert_eq!(testi(&v), ["Apri", "Impostazioni", "Informazioni", "Schermo", "Ritira l'isola", "Esci"]);
        assert_eq!(
            v[3],
            Voce::Schermo(vec![
                VoceSchermo { comando: Comando::Schermo(0), testo: "Schermo principale".into(), scelto: false },
                VoceSchermo { comando: Comando::Schermo(1), testo: "Schermo 2 · 2560×1440".into(), scelto: true },
            ])
        );
        // Anche nel menu del pannello.
        assert_eq!(
            testi(&voci_pannello(false, &due_schermi())),
            ["Impostazioni", "Informazioni", "Schermo", "Ritira l'isola", "Esci"]
        );
    }

    #[test]
    fn le_voci_degli_schermi_dai_monitor() {
        use crate::finestre::RettangoloFisico;
        let m = |x, principale| InfoMonitor {
            nome: format!("M{x}"),
            principale,
            area: RettangoloFisico { x, y: 0, larghezza: 2560, altezza: 1400 },
            scala: 1.0,
            larghezza_px: 2560,
            altezza_px: 1440,
        };
        let monitor = [m(0, true), m(2560, false)];
        assert_eq!(
            schermi_del_menu(&monitor, Some(1)),
            [("Schermo principale".to_owned(), false), ("Schermo 2 · 2560×1440".to_owned(), true)]
        );
        assert!(schermi_del_menu(&[], None).is_empty());
    }

    #[test]
    fn le_posizioni_contano_il_separatore_prima_di_esci() {
        use Posizione::{Comando as C, Separatore, Sotto};
        // Nel menu nativo il separatore occupa una posizione: senza comando.
        assert_eq!(
            posizioni(&voci_pannello(false, &[])),
            [C(Comando::Impostazioni), C(Comando::Informazioni), C(Comando::Ritirata), Separatore, C(Comando::Esci)]
        );
        assert_eq!(posizioni(&voci(false, &[])).len(), 6);
        assert_eq!(posizioni(&voci(false, &[]))[4], Separatore);
        // Il sottomenu è una posizione sola, con le sue righe.
        let p = posizioni(&voci(false, &due_schermi()));
        assert_eq!(p.len(), 7);
        assert_eq!(p[3], Sotto(vec![Comando::Schermo(0), Comando::Schermo(1)]));
    }

    /// La traduzione posizione → comando del menu del pannello vale solo se `muda` mette una riga
    /// nativa per voce, separatore compreso, nell'ordine di inserimento: se un aggiornamento la
    /// cambia, questo test lo dice prima che si esegua la voce sbagliata.
    #[cfg(windows)]
    #[test]
    fn il_menu_nativo_ha_le_righe_di_posizioni() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetMenuItemCount, GetMenuItemID, GetMenuState, GetSubMenu, MF_BYPOSITION, MF_SEPARATOR,
        };
        // Senza e con il sottomenu «Schermo» (due monitor).
        for schermi in [Vec::new(), due_schermi()] {
            let voci = voci_pannello(false, &schermi);
            let (menu, _) = super::windows::costruisci(&voci).expect("menu creato");
            let hmenu = tray_icon::menu::ContextMenu::hpopupmenu(&menu);
            let attese = posizioni(&voci);
            // SAFETY: letture del menu vivo appena creato, sullo stesso thread.
            unsafe {
                assert_eq!(GetMenuItemCount(hmenu), attese.len() as i32);
                for (i, attesa) in attese.iter().enumerate() {
                    let separatore = GetMenuState(hmenu, i as u32, MF_BYPOSITION) & MF_SEPARATOR != 0;
                    assert_eq!(separatore, *attesa == Posizione::Separatore, "posizione {i}");
                    match attesa {
                        Posizione::Comando(_) => {
                            assert!(GetMenuItemID(hmenu, i as i32) >= 1000, "voce {i} con un id di comando");
                        }
                        Posizione::Sotto(righe) => {
                            let sotto = GetSubMenu(hmenu, i as i32);
                            assert!(sotto != 0, "la posizione {i} è un sottomenu");
                            assert_eq!(GetMenuItemCount(sotto), righe.len() as i32);
                            for j in 0..righe.len() as i32 {
                                assert!(GetMenuItemID(sotto, j) >= 1000, "riga {j} del sottomenu con un id di comando");
                            }
                        }
                        Posizione::Separatore => {}
                    }
                }
            }
        }
    }

    #[test]
    fn nel_menu_del_pannello_si_ignorano_il_tasto_tenuto_e_i_caratteri() {
        // Giro 1: aperto con Invio, il tasto tenuto (auto-ripetizione) arrivava al menu con la
        // prima voce evidenziata e sceglieva da solo «Impostazioni».
        const WM_KEYDOWN: u32 = 0x100;
        const WM_KEYUP: u32 = 0x101;
        const WM_CHAR: u32 = 0x102;
        let tenuto = 1 << 30;
        assert!(da_ignorare_nel_menu(WM_KEYDOWN, 0x0D, tenuto | 1), "Invio tenuto");
        assert!(da_ignorare_nel_menu(WM_KEYDOWN, 0x20, tenuto | 1), "Spazio tenuto");
        assert!(!da_ignorare_nel_menu(WM_KEYDOWN, 0x0D, 1), "Invio premuto di nuovo sceglie");
        assert!(!da_ignorare_nel_menu(WM_KEYDOWN, 0x28, tenuto | 1), "↓ tenuto scorre le voci");
        assert!(!da_ignorare_nel_menu(WM_KEYDOWN, 0x1B, 1), "Esc chiude");
        assert!(!da_ignorare_nel_menu(WM_KEYUP, 0x0D, tenuto | 1));
        // Il carattere dell'Invio o dello Spazio che ha aperto il menu: nessuna voce lo ha come
        // iniziale, il menu farebbe solo il suono d'errore.
        assert!(da_ignorare_nel_menu(WM_CHAR, 0x0D, 1));
        assert!(da_ignorare_nel_menu(WM_CHAR, 0x20, 1));
        assert!(!da_ignorare_nel_menu(WM_CHAR, 'i' as usize, 1), "le iniziali delle voci restano");
    }

    #[test]
    fn il_punto_del_menu_del_pannello_in_pixel_fisici() {
        // Bordo destro e basso del pulsante «⋯» in pixel logici, scala 1,75: arrotondati.
        assert_eq!(punto_pannello(782.0, 30.0, 1.75), (1369, 53));
        assert_eq!(punto_pannello(782.0, 30.0, 1.0), (782, 30));
    }

    #[test]
    fn ogni_voce_torna_al_suo_comando() {
        let mut tutti: Vec<Comando> = voci(false, &due_schermi())
            .into_iter()
            .flat_map(|v| match v {
                Voce::Semplice(c, _) => vec![c],
                Voce::Schermo(righe) => righe.into_iter().map(|r| r.comando).collect(),
            })
            .collect();
        assert!(tutti.contains(&Comando::Schermo(0)) && tutti.contains(&Comando::Schermo(1)));
        tutti.push(Comando::Schermo(12));
        for c in tutti {
            assert_eq!(comando(&id(c)), Some(c));
        }
        assert_eq!(comando("sconosciuto"), None);
        assert_eq!(comando("schermo:x"), None);
    }
}
