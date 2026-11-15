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
    use super::{comando, id, testo_ritirata, voci, Comando};
    use tray_icon::menu::dpi::PhysicalPosition;
    use tray_icon::menu::{ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem};

    pub struct MenuIsola {
        menu: Menu,
        ritirata: MenuItem,
    }

    impl MenuIsola {
        /// Crea il menu sul thread dell'interfaccia. `al_comando` è chiamata sul ciclo eventi.
        pub fn crea(al_comando: fn(Comando), ritirata: bool) -> Result<MenuIsola, String> {
            let menu = Menu::new();
            let mut voce_ritirata = None;
            for (c, testo) in voci(ritirata) {
                let voce = MenuItem::with_id(id(c), testo, true, None);
                if c == Comando::Esci {
                    menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
                }
                menu.append(&voce).map_err(|e| e.to_string())?;
                if c == Comando::Ritirata {
                    voce_ritirata = Some(voce);
                }
            }
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if let Some(c) = comando(e.id().as_ref()) {
                    let _ = slint::invoke_from_event_loop(move || al_comando(c));
                }
            }));
            let ritirata = voce_ritirata.ok_or("voce della ritirata mancante")?;
            Ok(MenuIsola { menu, ritirata })
        }

        pub fn ritirata(&self, attiva: bool) {
            let testo = testo_ritirata(attiva);
            if self.ritirata.text() != testo {
                self.ritirata.set_text(testo);
            }
        }

        /// Lo stesso menu, per l'icona nella barra.
        pub fn menu(&self) -> &Menu {
            &self.menu
        }

        /// Apre il menu nella finestra `hwnd`, col bordo sinistro in alto a `(x, y)` (pixel fisici
        /// dell'area client). Ritorna quando il menu si chiude (scelta, Esc o clic altrove); la
        /// scelta arriva dopo, dal gestore di `crea`.
        pub fn mostra(&self, hwnd: isize, (x, y): (i32, i32)) {
            // SAFETY: hwnd è la finestra viva della pillola, sul thread dell'interfaccia (lo stesso
            // del menu); `muda` lo usa solo per la durata di TrackPopupMenu.
            unsafe {
                self.menu.show_context_menu_for_hwnd(hwnd, Some(PhysicalPosition::new(x, y).into()));
            }
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
    fn ogni_voce_torna_al_suo_comando() {
        for (c, _) in voci(false) {
            assert_eq!(comando(id(c)), Some(c));
        }
        assert_eq!(comando("sconosciuto"), None);
    }
}
