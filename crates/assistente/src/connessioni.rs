//! Le connessioni della pagina PC (M2, Task 8; dal ridisegno F3, Task 7, funzioni pure): la scelta
//! della pagina di Windows dopo l'esito di un cambio e la traduzione dei numeri del pannello. Lo
//! stato delle radio, del rifiuto e delle cuffie lo tiene il ponte (pre-flight C6), che scrive la
//! pagina (`Ponte::scrivi_pagina_pc`); i thread (`radio::Radio`, `radio::OsservaRete`) li accende
//! e spegne lui. Tutto compila e si prova su ogni piattaforma (pre-flight C5).

// Fuori da Windows il modulo c'è solo per i test, che ne usano una parte.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::sistema::browser::PaginaWindows;
use crate::sistema::radio::{EsitoRadio, TipoRadio};

/// La pagina di Windows da aprire dopo l'esito di un cambio. `seguite`: le radio sono ancora
/// seguite (pagina PC aperta); altrimenti l'esito arrivato in ritardo si ignora, senza rifiuto da
/// mostrare né pagina da aprire. `Fatto`: nulla (lo stato vero arriva da `StateChanged`). Rifiutato
/// (`Negato`, `Errore`): la pagina del Wi-Fi o del Bluetooth.
pub fn esito(seguite: bool, tipo: TipoRadio, esito: EsitoRadio) -> Option<PaginaWindows> {
    if !seguite || esito == EsitoRadio::Fatto {
        return None;
    }
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

/// La pagina di un rimando della pagina PC (0 «Reti Wi-Fi», 1 «Dispositivi Bluetooth»).
pub fn pagina_da(i: i32) -> PaginaWindows {
    if i == 0 {
        PaginaWindows::RetiWifi
    } else {
        PaginaWindows::Bluetooth
    }
}
