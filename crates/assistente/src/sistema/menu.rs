//! Il menu dell'isola: «Apri», «Impostazioni», «Informazioni» (la schermata «About» con
//! AboutSlint, richiesta dalla licenza Slint royalty-free 2.0, §2(a), «accessible from the top
//! level menu»), «Ritira l'isola» / «Mostra l'isola», «Esci».
//!
//! Quello dell'icona nella barra è un menu nativo di Windows (`muda`, TrackPopupMenu) con tutte le
//! voci, se l'utente l'ha accesa (Impostazioni › Sistema); Esc o un clic altrove lo chiudono. Le
//! scelte arrivano dal gestore globale di `muda` e si inoltrano al ciclo eventi con
//! `invoke_from_event_loop`. Il clic destro sulla pillola non lo apre più (16/11, Paolo).
//!
//! Dal pulsante «⋯» in alto a destra del pannello si apre il gemello senza «Apri» (il pannello è
//! già aperto), ma dal Task 8 del giro grafico (RC-58) NON è più nativo: è un menu Slint scuro
//! dentro il pannello (`ui/menu.slint`). Questo modulo gli dà solo le voci (`voci_pannello`, unica
//! fonte per tutti e due i menu: non possono divergere) e riconosce gli id che il menu Slint
//! rimanda (`comando`); la scelta si esegue in `servizi`.
//!
//! La linea della ritirata non apre il menu: lascia passare i clic (non riceve il mouse). Ci si
//! torna tenendo il mouse sulla linea, con Ctrl+Alt+Maiusc+I o Ctrl+Alt+Maiusc+Spazio.
//!
//! Sottomenu «Schermo» (M2, Task 6): fra «Informazioni» e «Ritira l'isola», una voce a spunta per
//! monitor, SOLO con più di un monitor. Sta in tutti e due i menu (icona nella barra e «⋯»).
//! Con `muda` per l'icona nella barra non c'è un evento «prima dell'apertura», quindi il menu non
//! si ricostruisce a ogni apertura: si ricostruisce quando cambia l'elenco dei monitor (o la
//! spunta), controllato dal giro da 5 s dei servizi e subito dopo una scelta; il «⋯» lo ricontrolla
//! anche un attimo prima di aprirsi (e le sue voci si rileggono a ogni apertura). Una scelta fatta
//! nei pochi secondi fra un cambio di monitor e il controllo successivo si legge sull'elenco vero
//! di quel momento (`Comando::Schermo(i)` è un indice): se l'indice non esiste più, non succede
//! nulla.
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

/// Il filo di separazione va prima di «Esci»: in tutti e due i menu (nativo e Slint).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn separatore_prima(voce: &Voce) -> bool {
    matches!(voce, Voce::Semplice(Comando::Esci, _))
}

/// L'identificativo di una voce, nel menu nativo (tray) e nel menu Slint del pannello; e ritorno.
// Fuori da Windows il menu non c'è: restano per i test.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn id(c: Comando) -> String {
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
pub fn comando(id: &str) -> Option<Comando> {
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
    use super::{comando, id, separatore_prima, testo_ritirata, voci, voci_pannello, Comando, Voce};
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    pub struct MenuIsola {
        /// Il menu nativo dell'icona nella barra (con «Apri»). Quello del «⋯» del pannello non è
        /// nativo: lo disegna Slint dalle stesse voci (`voci_pannello`).
        menu: Menu,
        ritirata: MenuItem,
        /// I monitor con cui è costruito: (testo, spunta). Con più di uno c'è «Schermo».
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
                    if separatore_prima(v) {
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

        /// Il menu costruito per questi monitor.
        fn costruito(ritirata: bool, schermi: Vec<(String, bool)>) -> Result<MenuIsola, String> {
            let (menu, voce_ritirata) = costruisci(&voci(ritirata, &schermi))?;
            Ok(MenuIsola { menu, ritirata: voce_ritirata, schermi })
        }

        /// Le voci del menu del «⋯» del pannello (Slint) coi monitor di questo momento: la fonte è
        /// la stessa del menu nativo (`voci_pannello`).
        pub fn voci_pannello(&self, ritirata: bool) -> Vec<Voce> {
            voci_pannello(ritirata, &self.schermi)
        }

        /// Ricostruisce il menu se cambiano i monitor o la spunta (`schermi`: testo e spunta di
        /// ognuno). Ritorna `true` se l'ha ricostruito: chi tiene il menu dell'icona nella barra
        /// deve passarle quello nuovo (`menu()`).
        pub fn schermi(&mut self, schermi: Vec<(String, bool)>, ritirata: bool) -> Result<bool, String> {
            if schermi == self.schermi {
                return Ok(false);
            }
            *self = Self::costruito(ritirata, schermi)?;
            Ok(true)
        }

        /// Il menu è costruito per questi stessi monitor (stessi testi, spunta a parte)?
        pub fn stesso_elenco(&self, schermi: &[(String, bool)]) -> bool {
            self.schermi.len() == schermi.len() && self.schermi.iter().zip(schermi).all(|(a, b)| a.0 == b.0)
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
    fn il_filo_sta_solo_prima_di_esci() {
        // In tutti e due i menu (nativo della tray e Slint del pannello): un solo filo, prima di «Esci».
        for schermi in [Vec::new(), due_schermi()] {
            for v in [voci(false, &schermi), voci_pannello(false, &schermi)] {
                let con_filo: Vec<String> =
                    v.iter().filter(|x| separatore_prima(x)).map(|x| testi(std::slice::from_ref(x))[0].clone()).collect();
                assert_eq!(con_filo, ["Esci"]);
                assert!(separatore_prima(v.last().expect("almeno una voce")), "«Esci» è l'ultima");
            }
        }
    }

    /// Il menu nativo della tray ha una riga per voce più il filo prima di «Esci» (il menu del
    /// pannello non è più nativo, ma la tray sì e non deve cambiare).
    #[cfg(windows)]
    #[test]
    fn il_menu_nativo_della_tray_ha_il_filo_prima_di_esci() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetMenuItemCount, GetMenuState, MF_BYPOSITION, MF_SEPARATOR};
        for schermi in [Vec::new(), due_schermi()] {
            let voci = voci(false, &schermi);
            let (menu, _) = super::windows::costruisci(&voci).expect("menu creato");
            let hmenu = tray_icon::menu::ContextMenu::hpopupmenu(&menu);
            // SAFETY: letture del menu vivo appena creato, sullo stesso thread.
            unsafe {
                assert_eq!(GetMenuItemCount(hmenu), voci.len() as i32 + 1);
                let n = GetMenuItemCount(hmenu) as u32;
                for i in 0..n {
                    let separatore = GetMenuState(hmenu, i, MF_BYPOSITION) & MF_SEPARATOR != 0;
                    assert_eq!(separatore, i == n - 2, "posizione {i}");
                }
            }
        }
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
