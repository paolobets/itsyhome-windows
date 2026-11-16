//! Il menu dell'isola: «Apri», «Impostazioni», «Informazioni» (la schermata «About» con
//! AboutSlint, richiesta dalla licenza Slint royalty-free 2.0, §2(a), «accessible from the top
//! level menu»), «Ritira l'isola» / «Mostra l'isola», «Esci».
//!
//! È un menu nativo di Windows (`muda`, lo stesso dell'icona nella barra: TrackPopupMenu). Si
//! apre col clic destro sulla pillola, sotto la pillola, e dall'icona nella barra se l'utente
//! l'ha accesa (Impostazioni › Sistema); Esc o un clic altrove lo chiudono. Un solo menu, con le
//! stesse voci, per tutte e due. Le scelte arrivano dal gestore globale di `muda` e si inoltrano
//! al ciclo eventi con `invoke_from_event_loop`.
//!
//! Dal pulsante «⋯» in alto a destra del pannello (Task 14i) si apre il gemello senza «Apri» (il
//! pannello è già aperto), sotto il pulsante e allineato al suo bordo destro: `muda` lo apre solo
//! allineato a sinistra, quindi lo si apre qui con TrackPopupMenu (allineato a destra, dentro
//! l'area di lavoro) e la scelta torna come comando, senza passare dal gestore di `muda`.
//!
//! La linea della ritirata non apre il menu: lascia passare i clic (non riceve il mouse). Ci si
//! torna tenendo il mouse sulla linea, con Ctrl+Alt+Maiusc+I o Ctrl+Alt+Maiusc+Spazio.
//!
//! Fuori da Windows per ora niente menu (su Linux `muda` richiede GTK): il clic destro non fa
//! nulla, come prima.

/// Cosa chiede il menu (o il clic sull'icona nella barra).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comando {
    Apri,
    Impostazioni,
    /// Le Impostazioni sulla sezione «Informazioni».
    Informazioni,
    /// Ritira la pillola o la rimette (secondo lo stato).
    Ritirata,
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

/// Le voci del menu, in ordine: (comando, testo). Il separatore sta prima di «Esci».
pub fn voci(ritirata: bool) -> [(Comando, &'static str); 5] {
    [
        (Comando::Apri, "Apri"),
        (Comando::Impostazioni, "Impostazioni"),
        (Comando::Informazioni, "Informazioni"),
        (Comando::Ritirata, testo_ritirata(ritirata)),
        (Comando::Esci, "Esci"),
    ]
}

/// Le voci del menu del pannello («⋯»): le stesse, senza «Apri».
#[cfg_attr(not(windows), allow(dead_code))]
pub fn voci_pannello(ritirata: bool) -> [(Comando, &'static str); 4] {
    let [_, impostazioni, informazioni, ritira, esci] = voci(ritirata);
    [impostazioni, informazioni, ritira, esci]
}

/// Cosa c'è a ogni posizione del menu nativo, separatore compreso (`None`): il separatore va
/// prima di «Esci».
// Fuori da Windows il menu non c'è: restano per i test.
#[cfg_attr(not(windows), allow(dead_code))]
fn posizioni(voci: &[(Comando, &str)]) -> Vec<Option<Comando>> {
    let mut p = Vec::with_capacity(voci.len() + 1);
    for (c, _) in voci {
        if *c == Comando::Esci {
            p.push(None);
        }
        p.push(Some(*c));
    }
    p
}

/// Il punto del menu del pannello in pixel fisici dell'area client, dal bordo destro e basso del
/// pulsante «⋯» in pixel logici.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn punto_pannello(destra: f32, basso: f32, scala: f32) -> (i32, i32) {
    ((destra * scala).round() as i32, (basso * scala).round() as i32)
}

/// L'identificativo di una voce nel menu nativo, e ritorno.
// Fuori da Windows il menu non c'è: restano per i test.
#[cfg_attr(not(windows), allow(dead_code))]
fn id(c: Comando) -> &'static str {
    match c {
        Comando::Apri => "apri",
        Comando::Impostazioni => "impostazioni",
        Comando::Informazioni => "informazioni",
        Comando::Ritirata => "ritirata",
        Comando::Esci => "esci",
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn comando(id: &str) -> Option<Comando> {
    voci(false).into_iter().map(|(c, _)| c).find(|c| self::id(*c) == id)
}

#[cfg(windows)]
pub use windows::MenuIsola;

#[cfg(windows)]
mod windows {
    use super::{comando, id, posizioni, testo_ritirata, voci, voci_pannello, Comando};
    use tray_icon::menu::dpi::PhysicalPosition;
    use tray_icon::menu::{ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem};

    pub struct MenuIsola {
        menu: Menu,
        ritirata: MenuItem,
        /// Il gemello del pannello («⋯»): senza «Apri».
        pannello: Menu,
        ritirata_pannello: MenuItem,
        /// Cosa c'è a ogni posizione del menu del pannello (separatore compreso).
        posizioni_pannello: Vec<Option<Comando>>,
    }

    /// Un menu nativo con queste voci (separatore prima di «Esci») e la sua voce della ritirata.
    fn costruisci(voci: &[(Comando, &'static str)]) -> Result<(Menu, MenuItem), String> {
        let menu = Menu::new();
        let mut voce_ritirata = None;
        for &(c, testo) in voci {
            let voce = MenuItem::with_id(id(c), testo, true, None);
            if c == Comando::Esci {
                menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
            }
            menu.append(&voce).map_err(|e| e.to_string())?;
            if c == Comando::Ritirata {
                voce_ritirata = Some(voce);
            }
        }
        let ritirata = voce_ritirata.ok_or("voce della ritirata mancante")?;
        Ok((menu, ritirata))
    }

    impl MenuIsola {
        /// Crea il menu sul thread dell'interfaccia. `al_comando` è chiamata sul ciclo eventi.
        pub fn crea(al_comando: fn(Comando), ritirata: bool) -> Result<MenuIsola, String> {
            let (menu, voce_ritirata) = costruisci(&voci(ritirata))?;
            let voci_pannello = voci_pannello(ritirata);
            let (pannello, ritirata_pannello) = costruisci(&voci_pannello)?;
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if let Some(c) = comando(e.id().as_ref()) {
                    let _ = slint::invoke_from_event_loop(move || al_comando(c));
                }
            }));
            Ok(MenuIsola {
                menu,
                ritirata: voce_ritirata,
                pannello,
                ritirata_pannello,
                posizioni_pannello: posizioni(&voci_pannello),
            })
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

        /// Apre il menu nella finestra `hwnd`, col bordo sinistro in alto a `(x, y)` (pixel fisici
        /// dell'area client). Ritorna quando il menu si chiude (scelta, Esc o clic altrove); la
        /// scelta arriva dopo, dal gestore di `crea`. Vero se è stata scelta una voce.
        pub fn mostra(&self, hwnd: isize, (x, y): (i32, i32)) -> bool {
            // SAFETY: hwnd è la finestra viva della pillola, sul thread dell'interfaccia (lo stesso
            // del menu); `muda` lo usa solo per la durata di TrackPopupMenu.
            unsafe {
                self.menu.show_context_menu_for_hwnd(hwnd, Some(PhysicalPosition::new(x, y).into()))
            }
        }

        /// Apre il menu del pannello nella finestra `hwnd`, col bordo DESTRO in alto a `(x, y)`
        /// (pixel fisici dell'area client), dentro l'area di lavoro. Ritorna quando il menu si
        /// chiude, con la voce scelta (`None`: Esc o clic altrove). Il gestore di `muda` non
        /// riceve nulla: la scelta la porta chi chiama.
        pub fn mostra_dal_pannello(&self, hwnd: isize, (x, y): (i32, i32)) -> Option<Comando> {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetMenuItemCount, GetMenuItemID, SetForegroundWindow, TrackPopupMenu, TPM_RETURNCMD, TPM_RIGHTALIGN,
                TPM_TOPALIGN, TPM_WORKAREA,
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
                TrackPopupMenu(
                    hmenu,
                    TPM_RIGHTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_WORKAREA,
                    punto.x,
                    punto.y,
                    0,
                    hwnd,
                    std::ptr::null(),
                )
            };
            if scelto <= 0 {
                return None;
            }
            // SAFETY: solo letture del menu vivo di `self`.
            let quante = unsafe { GetMenuItemCount(hmenu) };
            (0..quante)
                .find(|&i| unsafe { GetMenuItemID(hmenu, i) } == scelto as u32)
                .and_then(|i| self.posizioni_pannello.get(i as usize).copied().flatten())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn le_voci_del_menu_e_la_ritirata() {
        let testi: Vec<&str> = voci(false).iter().map(|(_, t)| *t).collect();
        assert_eq!(testi, ["Apri", "Impostazioni", "Informazioni", "Ritira l'isola", "Esci"]);
        assert_eq!(voci(true)[3], (Comando::Ritirata, "Mostra l'isola"));
    }

    #[test]
    fn il_menu_del_pannello_senza_apri_con_impostazioni_in_testa() {
        // Task 14i: dal «⋯» del pannello (già aperto) niente «Apri»; il resto come nella pillola.
        let testi: Vec<&str> = voci_pannello(false).iter().map(|(_, t)| *t).collect();
        assert_eq!(testi, ["Impostazioni", "Informazioni", "Ritira l'isola", "Esci"]);
        assert_eq!(voci_pannello(true)[2], (Comando::Ritirata, "Mostra l'isola"));
    }

    #[test]
    fn le_posizioni_contano_il_separatore_prima_di_esci() {
        // Nel menu nativo il separatore occupa una posizione: senza comando.
        assert_eq!(
            posizioni(&voci_pannello(false)),
            [Some(Comando::Impostazioni), Some(Comando::Informazioni), Some(Comando::Ritirata), None, Some(Comando::Esci)]
        );
        assert_eq!(posizioni(&voci(false)).len(), 6);
        assert_eq!(posizioni(&voci(false))[4], None);
    }

    #[test]
    fn il_punto_del_menu_del_pannello_in_pixel_fisici() {
        // Bordo destro e basso del pulsante «⋯» in pixel logici, scala 1,75: arrotondati.
        assert_eq!(punto_pannello(782.0, 30.0, 1.75), (1369, 53));
        assert_eq!(punto_pannello(782.0, 30.0, 1.0), (782, 30));
    }

    #[test]
    fn ogni_voce_torna_al_suo_comando() {
        for (c, _) in voci(false) {
            assert_eq!(comando(id(c)), Some(c));
        }
        assert_eq!(comando("sconosciuto"), None);
    }
}
