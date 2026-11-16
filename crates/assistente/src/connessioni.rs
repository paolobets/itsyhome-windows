//! Le connessioni della vista PC (M2, Task 8, solo Windows): che cosa scrivere nel pannello per
//! le radio, per la rete e per l'esito di un cambio. Ogni proprietà si riscrive solo se cambia.
//! I thread (`radio::Radio`, `radio::OsservaRete`) li accende e spegne il ponte; qui solo il
//! pannello, così il rifiuto si prova con un pannello vero nel backend di prova.

// Fuori da Windows il modulo c'è solo per i test, che ne usano una parte.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::sistema::browser::PaginaWindows;
use crate::sistema::radio::{EsitoRadio, StatoRadio, StatoRete, TipoRadio};
use crate::sorgenti_lati::rete_ui;
use crate::Pannello;

/// Gli stati veri delle radio (l'ultimo `StateChanged`): presenti e accese.
pub fn scrivi_radio(p: &Pannello, stati: &[StatoRadio]) {
    let di = |tipo| stati.iter().find(|s| s.tipo == tipo);
    let (wifi, bt) = (di(TipoRadio::WiFi), di(TipoRadio::Bluetooth));
    if !p.get_radio_lette() {
        p.set_radio_lette(true);
    }
    if p.get_wifi_presente() != wifi.is_some() {
        p.set_wifi_presente(wifi.is_some());
    }
    if p.get_bt_presente() != bt.is_some() {
        p.set_bt_presente(bt.is_some());
    }
    let wifi_acceso = wifi.is_some_and(|s| s.acceso);
    if p.get_wifi_acceso() != wifi_acceso {
        p.set_wifi_acceso(wifi_acceso);
    }
    let bt_acceso = bt.is_some_and(|s| s.acceso);
    if p.get_bt_acceso() != bt_acceso {
        p.set_bt_acceso(bt_acceso);
    }
}

/// Le radio non più seguite (vista PC lasciata): i riquadri tornano «non letti».
pub fn dimentica_radio(p: &Pannello) {
    if p.get_radio_lette() {
        p.set_radio_lette(false);
    }
    if p.get_errore_radio() {
        p.set_errore_radio(false);
    }
}

/// L'icona della rete nella vista PC (`None`: non ancora letta).
pub fn scrivi_rete(p: &Pannello, stato: Option<StatoRete>) {
    if p.get_rete_nota() != stato.is_some() {
        p.set_rete_nota(stato.is_some());
    }
    let Some(stato) = stato else { return };
    let (tipo, tacche) = rete_ui(stato);
    if p.get_rete() != tipo {
        p.set_rete(tipo);
    }
    if p.get_tacche() != tacche {
        p.set_tacche(tacche);
    }
}

/// Il nome della rete Wi-Fi (solo a vista PC aperta; vuoto se non si sa).
pub fn scrivi_nome(p: &Pannello, nome: Option<&str>) {
    let nome = nome.unwrap_or_default();
    if p.get_nome_rete() != nome {
        p.set_nome_rete(nome.into());
    }
}

/// L'esito di un cambio. `stati`: l'ultimo elenco letto, `None` se le radio non sono più seguite
/// (vista PC lasciata o pannello chiuso): allora l'esito arrivato in ritardo si ignora, senza
/// riga in ambra né riquadri «Non presente». `Fatto`: nulla (lo stato vero arriva da
/// `StateChanged`). Rifiutato (`Negato`, `Errore`): la riga in ambra, il riquadro di nuovo allo
/// stato vero e la pagina di Windows da aprire.
pub fn esito(p: &Pannello, stati: Option<&[StatoRadio]>, tipo: TipoRadio, esito: EsitoRadio) -> Option<PaginaWindows> {
    let stati = stati?;
    if esito == EsitoRadio::Fatto {
        return None;
    }
    if !p.get_errore_radio() {
        p.set_errore_radio(true);
    }
    scrivi_radio(p, stati);
    Some(match tipo {
        TipoRadio::WiFi => PaginaWindows::ImpostazioniWifi,
        TipoRadio::Bluetooth => PaginaWindows::Bluetooth,
    })
}

/// La radio dal numero del pannello (0 Wi-Fi, 1 Bluetooth).
pub fn tipo_da(i: i32) -> TipoRadio {
    if i == 0 {
        TipoRadio::WiFi
    } else {
        TipoRadio::Bluetooth
    }
}

/// La pagina di una riga del pannello (0 «Reti Wi-Fi ›», 1 «Dispositivi Bluetooth ›»).
pub fn pagina_da(i: i32) -> PaginaWindows {
    if i == 0 {
        PaginaWindows::RetiWifi
    } else {
        PaginaWindows::Bluetooth
    }
}
