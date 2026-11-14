//! L'icona nella tray (area di notifica) con il suo menu: «Apri», «Impostazioni»,
//! «Informazioni» (la schermata «About» con AboutSlint, richiesta dalla licenza Slint
//! royalty-free 2.0, §2(a), «accessible from the top level menu»), «Ritira l'isola» / «Mostra
//! l'isola», «Esci». Clic sinistro sull'icona = «Apri». Va creata dal ciclo
//! eventi (thread dell'interfaccia); gli eventi arrivano da `tray-icon` e si inoltrano al ciclo
//! eventi con `invoke_from_event_loop`.

/// Cosa chiede la tray.
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
// Fuori da Windows la tray non c'è: resta per i test.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn testo_ritirata(ritirata: bool) -> &'static str {
    if ritirata {
        "Mostra l'isola"
    } else {
        "Ritira l'isola"
    }
}

#[cfg(windows)]
pub use windows::{crea, Tray};

#[cfg(windows)]
mod windows {
    use super::{testo_ritirata, Comando};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct Tray {
        _icona: TrayIcon,
        ritirata: MenuItem,
    }

    impl Tray {
        pub fn ritirata(&self, attiva: bool) {
            let testo = testo_ritirata(attiva);
            if self.ritirata.text() != testo {
                self.ritirata.set_text(testo);
            }
        }
    }

    fn icona() -> Result<Icon, String> {
        let png = include_bytes!("../../assets/tray.png");
        let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        let (l, a) = img.dimensions();
        Icon::from_rgba(img.into_raw(), l, a).map_err(|e| e.to_string())
    }

    /// Crea l'icona. `al_comando` è chiamata sul ciclo eventi.
    pub fn crea(al_comando: fn(Comando), ritirata: bool) -> Result<Tray, String> {
        let apri = MenuItem::with_id("apri", "Apri", true, None);
        let impostazioni = MenuItem::with_id("impostazioni", "Impostazioni", true, None);
        let informazioni = MenuItem::with_id("informazioni", "Informazioni", true, None);
        let ritira = MenuItem::with_id("ritirata", testo_ritirata(ritirata), true, None);
        let esci = MenuItem::with_id("esci", "Esci", true, None);
        let menu = Menu::new();
        menu.append_items(&[&apri, &impostazioni, &informazioni, &ritira, &PredefinedMenuItem::separator(), &esci])
            .map_err(|e| e.to_string())?;

        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let comando = match e.id().as_ref() {
                "apri" => Comando::Apri,
                "impostazioni" => Comando::Impostazioni,
                "informazioni" => Comando::Informazioni,
                "ritirata" => Comando::Ritirata,
                "esci" => Comando::Esci,
                _ => return,
            };
            let _ = slint::invoke_from_event_loop(move || al_comando(comando));
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                let _ = slint::invoke_from_event_loop(move || al_comando(Comando::Apri));
            }
        }));

        let icona = TrayIconBuilder::new()
            .with_tooltip("ItsyHome")
            .with_icon(icona()?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Tray { _icona: icona, ritirata: ritira })
    }
}

/// Fuori da Windows per ora niente tray (su Linux `tray-icon` richiede GTK).
#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn ritirata(&self, _attiva: bool) {}
}

#[cfg(not(windows))]
pub fn crea(_al_comando: fn(Comando), _ritirata: bool) -> Result<Tray, String> {
    Err("tray non disponibile su questo sistema".into())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn la_voce_della_ritirata_dice_cosa_fara() {
        assert_eq!(testo_ritirata(false), "Ritira l'isola");
        assert_eq!(testo_ritirata(true), "Mostra l'isola");
    }

    #[cfg(windows)]
    #[test]
    fn l_icona_si_decodifica() {
        let png = include_bytes!("../../assets/tray.png");
        let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).unwrap().into_rgba8();
        assert_eq!(img.dimensions(), (32, 32));
    }
}
