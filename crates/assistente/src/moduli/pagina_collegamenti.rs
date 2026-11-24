//! La pagina Collegamenti sull'impianto Comandi (ridisegno F3, Task 5): le card dei lanci, la
//! sintesi e lo stato come funzioni pure. Il ponte le chiama coi collegamenti della configurazione
//! e scrive i modelli solo a pagina aperta, solo se cambiano (`ponte::con_pagine`).

use super::collegamenti::{piu_usato, AGGIUNGI};
use super::registro::tono_ui;
use super::{Tono, R1};
use crate::collegamenti::{indirizzo_corto, iniziale, tono};
use crate::pannello::icona;
use crate::{CardUi, SintesiUi, StatoPaginaUi, TipoCardUi, TipoStatoPaginaUi};
use casa_core::config::Collegamento;

/// «Modifica»: l'azione della testata, le Impostazioni sulla scheda Collegamenti.
pub const MODIFICA: &str = "collegamenti.modifica";

/// La card di un collegamento: il cerchio con l'iniziale sul colore del nome, il nome, sotto
/// l'indirizzo accorciato. Si tocca sempre (il browser decide se parte). Per Narrator: «{nome},
/// {indirizzo corto}, si apre nel browser».
pub fn card(c: &Collegamento) -> CardUi {
    let corto = indirizzo_corto(&c.url);
    CardUi {
        id: c.id.as_str().into(),
        tipo: TipoCardUi::Lancio,
        nome: c.nome.as_str().into(),
        narratore: format!("{}, {corto}, si apre nel browser", c.nome).into(),
        stato: corto.into(),
        toccabile: true,
        iniziale: iniziale(&c.nome).into(),
        tinta: tono(&c.nome),
        ..Default::default()
    }
}

/// La sintesi in cima alla pagina. Il titolo è `r1.fatto` (lo stesso della carta); il sotto è
/// l'errore dell'ultimo lancio se c'è (il browser non è partito), altrimenti «Più usato: {nome}».
/// «Modifica» (secondaria, senza icona) solo se c'è almeno un collegamento. Con l'errore il tono è
/// quello dell'errore: la parola sta nel sotto, il segno negli occhi della testata.
pub fn sintesi(c: &[Collegamento], r1: &R1, errore: Option<&str>) -> SintesiUi {
    let errore = errore.filter(|e| !e.is_empty());
    let (tono, sotto) = match (piu_usato(c), errore) {
        (_, Some(e)) => (Tono::Errore, e.to_owned()),
        (Some(primo), None) => (Tono::Calmo, format!("Più usato: {}", primo.nome)),
        (None, None) => (Tono::Vuoto, String::new()),
    };
    SintesiUi {
        tono: tono_ui(tono),
        titolo: r1.fatto.as_str().into(),
        sotto: sotto.into(),
        azione_id: if c.is_empty() { "" } else { MODIFICA }.into(),
        azione_testo: if c.is_empty() { "" } else { "Modifica" }.into(),
        azione_primaria: false,
        // Mai l'`Image` vuota (non è uguale a se stessa: la sintesi sarebbe sempre «cambiata»).
        azione_icona: icona("nessuna"),
        ..Default::default()
    }
}

/// Lo stato della pagina: senza collegamenti un invito con «Aggiungi un collegamento»; altrimenti
/// la pagina piena.
pub fn stato(n: usize) -> StatoPaginaUi {
    if n > 0 {
        return StatoPaginaUi { tipo: TipoStatoPaginaUi::Pieno, ..Default::default() };
    }
    StatoPaginaUi {
        tipo: TipoStatoPaginaUi::Vuoto,
        frase: "Le tue gestioni a portata di clic".into(),
        sotto: "Le pagine si aprono nel browser.".into(),
        azione_id: AGGIUNGI.into(),
        azione_testo: "Aggiungi un collegamento".into(),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::moduli::collegamenti::riassunto;
    use crate::TonoModuloUi;

    fn col(id: &str, nome: &str, url: &str, lanci: u32, ultimo: i64) -> Collegamento {
        Collegamento { id: id.into(), nome: nome.into(), url: url.into(), lanci, ultimo_lancio: ultimo }
    }

    fn due() -> Vec<Collegamento> {
        vec![col("proxmox", "Proxmox", "https://esempio.lan:8006", 1, 5), col("nas", "NAS", "http://nas.esempio.lan/admin?x=1", 4, 9)]
    }

    /// Le card nell'ordine dell'utente, col nome per Narrator; il titolo della sintesi è `r1.fatto`
    /// della carta, il sotto il più usato (o l'errore dell'ultimo lancio), «Modifica» secondaria;
    /// senza collegamenti lo stato vuoto invita ad aggiungerne uno.
    #[test]
    fn card_e_sintesi() {
        let c = due();
        let carte: Vec<CardUi> = c.iter().map(card).collect();
        let visti: Vec<(&str, &str, &str, &str)> = carte.iter().map(|k| (k.id.as_str(), k.nome.as_str(), k.stato.as_str(), k.narratore.as_str())).collect();
        assert_eq!(
            visti,
            [
                ("proxmox", "Proxmox", "esempio.lan:8006", "Proxmox, esempio.lan:8006, si apre nel browser"),
                ("nas", "NAS", "nas.esempio.lan/admin", "NAS, nas.esempio.lan/admin, si apre nel browser"),
            ]
        );
        assert!(carte.iter().all(|k| k.tipo == TipoCardUi::Lancio && k.toccabile && !k.tenue && !k.errore && !k.acceso));
        assert_eq!((carte[0].iniziale.as_str(), carte[1].iniziale.as_str()), ("P", "N"));
        assert_eq!((carte[0].tinta, carte[1].tinta), (tono("Proxmox"), tono("NAS")));

        let r1 = riassunto(&c).r1;
        let s = sintesi(&c, &r1, None);
        assert_eq!((s.titolo.as_str(), s.sotto.as_str(), s.tono), ("2 collegamenti", "Più usato: NAS", TonoModuloUi::Calmo));
        assert_eq!((s.azione_id.as_str(), s.azione_testo.as_str(), s.azione_primaria), (MODIFICA, "Modifica", false));
        assert_eq!(s.titolo.as_str(), riassunto(&c).r1.fatto, "il titolo è quello della carta");

        // L'errore dell'ultimo lancio prende il posto del sotto; uno vuoto no.
        let e = "«NAS» non si è aperto: il dettaglio è in Impostazioni › Informazioni";
        let s = sintesi(&c, &r1, Some(e));
        assert_eq!((s.titolo.as_str(), s.sotto.as_str(), s.tono), ("2 collegamenti", e, TonoModuloUi::Errore));
        assert_eq!(s.azione_testo.as_str(), "Modifica", "«Modifica» resta");
        assert_eq!(sintesi(&c, &r1, Some("")).sotto.as_str(), "Più usato: NAS");

        // Nessun collegamento: titolo a parole, nessuna azione nella testata, stato vuoto con
        // «Aggiungi un collegamento».
        let r1 = riassunto(&[]).r1;
        let s = sintesi(&[], &r1, None);
        assert_eq!((s.titolo.as_str(), s.sotto.as_str(), s.tono, s.azione_testo.as_str()), ("Nessun collegamento", "", TonoModuloUi::Vuoto, ""));
        let v = stato(0);
        assert_eq!((v.tipo, v.azione_id.as_str(), v.azione_testo.as_str()), (TipoStatoPaginaUi::Vuoto, AGGIUNGI, "Aggiungi un collegamento"));
        assert_eq!(stato(1).tipo, TipoStatoPaginaUi::Pieno);
        assert_eq!(stato(1).azione_id.as_str(), "");
    }

    /// Un collegamento solo dice «1 collegamento» nel titolo; con 60 collegamenti il titolo resta
    /// nello spazio della testata.
    #[test]
    fn titolo_della_sintesi_nelle_scale() {
        let uno = [col("a", "NAS", "https://nas.lan", 0, 0)];
        assert_eq!(sintesi(&uno, &riassunto(&uno).r1, None).titolo.as_str(), "1 collegamento");
        let molti: Vec<Collegamento> = (0..60).map(|i| col(&format!("c{i}"), &format!("Nome {i}"), "https://x.lan", 0, 0)).collect();
        assert_eq!(sintesi(&molti, &riassunto(&molti).r1, None).titolo.as_str(), "60 collegamenti");
    }
}
