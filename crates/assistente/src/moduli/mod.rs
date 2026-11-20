//! Moduli del pannello (ridisegno F1): le carte della Panoramica e le loro regole.
//!
//! Finché i moduli non hanno chiamanti nel programma vero (Task 4) il modulo intero è permesso
//! come codice morto fuori dai test; il Task 4 toglie questo attributo e clippy lo prova.
#![cfg_attr(not(test), allow(dead_code))]

pub mod misura;
