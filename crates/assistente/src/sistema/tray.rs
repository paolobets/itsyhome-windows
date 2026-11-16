//! L'icona nella barra (area di notifica), spenta di serie: l'utente la accende da Impostazioni ›
//! Sistema e da lì si accende o spegne subito, senza riavvio. Ha il menu dell'isola
//! (`sistema::menu`, lo stesso del «⋯» del pannello più «Apri»); clic sinistro = «Apri». Va creata
//! dal ciclo eventi (thread dell'interfaccia).
//!
//! Senza icona l'assistente si ritrova sempre: clic sulla pillola e «⋯», Ctrl+Alt+Maiusc+Spazio
//! (anche a pillola nascosta o ritirata), un secondo avvio dal menu Start (istanza unica). Le
//! notifiche di sistema non dipendono dall'icona (identificativo AUMID registrato a parte).

#[cfg(windows)]
pub use windows::{crea, Tray};

#[cfg(windows)]
mod windows {
    use crate::sistema::menu::MenuIsola;
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    /// Finché vive, l'icona sta nella barra; lasciarla la toglie subito.
    pub struct Tray {
        _icona: TrayIcon,
    }

    fn icona() -> Result<Icon, String> {
        let png = include_bytes!("../../assets/tray.png");
        let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        let (l, a) = img.dimensions();
        Icon::from_rgba(img.into_raw(), l, a).map_err(|e| e.to_string())
    }

    impl Tray {
        /// Il menu nuovo (ricostruito per un altro elenco di monitor).
        pub fn menu(&self, menu: &MenuIsola) {
            self._icona.set_menu(Some(Box::new(menu.menu().clone())));
        }
    }

    /// Crea l'icona col menu dell'isola. `al_clic` (clic sinistro) è chiamata sul ciclo eventi.
    pub fn crea(menu: &MenuIsola, al_clic: fn()) -> Result<Tray, String> {
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                let _ = slint::invoke_from_event_loop(al_clic);
            }
        }));
        let icona = TrayIconBuilder::new()
            .with_tooltip("ItsyHome")
            .with_icon(icona()?)
            .with_menu(Box::new(menu.menu().clone()))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Tray { _icona: icona })
    }
}

#[cfg(test)]
mod test {
    #[cfg(windows)]
    #[test]
    fn l_icona_si_decodifica() {
        let png = include_bytes!("../../assets/tray.png");
        let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).unwrap().into_rgba8();
        assert_eq!(img.dimensions(), (32, 32));
    }
}
