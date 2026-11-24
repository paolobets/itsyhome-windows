//! Il registro dei moduli: l'ordine scelto dall'utente, l'ultimo riassunto di ogni modulo e il
//! livello delle sue voci «In attesa». Da qui le schede del dock, le carte e il tono dell'isola.

use super::{controlla, FuoriMisura, Identita, Riassunto, Tono, MODULI};
use crate::attenzione::Livello;
use crate::pannello::icona;
use crate::{ModuloUi, TonoModuloUi};
use casa_core::config::ConfigModulo;

/// L'id di Agenti nei file di prima del ridisegno (la vista si chiamava «Claude»).
const ID_DI_PRIMA: &str = "claude";
/// Il motivo del «—» finché un modulo non ha dato il suo primo riassunto.
const MOTIVO_AVVIO: &str = "In attesa di dati";

/// Un modulo nel registro: chi è, l'ultimo riassunto accettato e la voce «In attesa» più urgente
/// che lo riguarda.
struct Posto {
    identita: &'static Identita,
    riassunto: Riassunto,
    livello: Option<Livello>,
}

impl Posto {
    /// Il tono mostrato: quello del riassunto, alzato dalla voce se la voce è più urgente.
    fn tono(&self) -> Tono {
        let dalla_voce = self.livello.map_or(Tono::Calmo, Tono::da_livello);
        if dalla_voce.priorita() > self.riassunto.tono.priorita() {
            dalla_voce
        } else {
            self.riassunto.tono
        }
    }
}

pub struct Registro {
    posti: Vec<Posto>,
}

/// L'urgenza di un livello, per scegliere fra più voci dello stesso modulo.
fn urgenza(l: Livello) -> u8 {
    Tono::da_livello(l).priorita()
}

impl Registro {
    /// L'ordine scelto dall'utente: gli id sconosciuti (o già visti) si scartano, `claude` vale
    /// `agenti` (una volta sola: nessuno riscrive il file qui, il prossimo salvataggio dell'ordine
    /// scriverà `agenti`), i moduli che mancano vanno in fondo nell'ordine di `MODULI`.
    pub fn nuovo(config: &[ConfigModulo]) -> Registro {
        let mut ordine: Vec<&'static Identita> = Vec::with_capacity(MODULI.len());
        let scelti = config.iter().map(|c| if c.id == ID_DI_PRIMA { "agenti" } else { c.id.as_str() });
        for id in scelti.chain(MODULI.iter().map(|m| m.id)) {
            if let Some(m) = MODULI.iter().find(|m| m.id == id) {
                if !ordine.iter().any(|o| o.id == m.id) {
                    ordine.push(m);
                }
            }
        }
        let posti = ordine
            .into_iter()
            .map(|identita| Posto { identita, riassunto: Riassunto::primo_avvio(MOTIVO_AVVIO), livello: None })
            .collect();
        Registro { posti }
    }

    fn posto(&self, id: &str) -> Option<&Posto> {
        self.posti.iter().find(|p| p.identita.id == id)
    }

    /// Accetta il riassunto di un modulo se i suoi testi stanno nella carta; altrimenti resta
    /// quello di prima. `Ok(true)` solo se è cambiato (da riscrivere).
    pub fn aggiorna(&mut self, id: &'static str, r: Riassunto) -> Result<bool, FuoriMisura> {
        controlla(id, &r)?;
        let Some(posto) = self.posti.iter_mut().find(|p| p.identita.id == id) else { return Ok(false) };
        if posto.riassunto == r {
            return Ok(false);
        }
        posto.riassunto = r;
        Ok(true)
    }

    /// Il livello delle voci «In attesa» di ogni modulo (tutte: un modulo assente non ne ha più);
    /// di più voci dello stesso modulo vale la più urgente. Vero se un tono mostrato cambia.
    pub fn livelli_voci(&mut self, per_modulo: &[(&'static str, Livello)]) -> bool {
        let mut cambiato = false;
        for posto in &mut self.posti {
            let prima = posto.tono();
            posto.livello = per_modulo
                .iter()
                .filter(|(id, _)| *id == posto.identita.id)
                .map(|&(_, l)| l)
                .max_by_key(|&l| urgenza(l));
            cambiato |= posto.tono() != prima;
        }
        cambiato
    }

    /// Il titolo della testata di pagina che il registro tiene per un modulo, per i test che lo
    /// confrontano con quello scritto in pagina.
    #[cfg(test)]
    pub fn r1(&self, id: &str) -> Option<&super::R1> {
        self.posto(id).map(|p| &p.riassunto.r1)
    }

    /// Il tono mostrato da un modulo (riassunto alzato dalle voci); `Vuoto` per un id sconosciuto.
    /// Le carte lo leggono da `moduli_ui`, le schede della barra da qui (`pannello::schede_ui`).
    pub fn tono_effettivo(&self, id: &str) -> Tono {
        self.posto(id).map_or(Tono::Vuoto, Posto::tono)
    }

    /// Il tono più urgente fra tutti i moduli (a parità vince il primo nell'ordine).
    pub fn tono_isola(&self) -> Tono {
        self.posti
            .iter()
            .map(Posto::tono)
            .reduce(|a, t| if t.priorita() > a.priorita() { t } else { a })
            .unwrap_or(Tono::Vuoto)
    }

    /// Quanti moduli non hanno ancora dato il primo riassunto (la fascia dell'Orologio, al primo
    /// avvio: «Avvio in corso · {n} moduli in attesa di dati»). Un «—» con un altro motivo (la
    /// casa da collegare, il PC che legge) è un dato vero, non un'attesa d'avvio.
    pub fn in_attesa_di_dati(&self) -> usize {
        let avvio = Riassunto::primo_avvio(MOTIVO_AVVIO);
        self.posti.iter().filter(|p| p.riassunto == avvio).count()
    }

    pub fn ordine(&self) -> Vec<&'static Identita> {
        self.posti.iter().map(|p| p.identita).collect()
    }

    /// Il nome per Narrator della carta in `posizione` (da 0): «{nome}, {titolo}, {riga intera}
    /// [, {tono}], modulo {i} di {n}[, azione: {azione}]». Il «—» dell'assenza non si legge (il
    /// motivo accanto basta, spec §13.10).
    pub fn nome_narrator(&self, posizione: usize) -> String {
        let Some(posto) = self.posti.get(posizione) else { return String::new() };
        let r = &posto.riassunto;
        let mut parti = vec![posto.identita.nome];
        for testo in [&r.titolo, &r.sec_intera] {
            if !testo.is_empty() && testo != "—" {
                parti.push(testo);
            }
        }
        let tono = posto.tono().per_narrator();
        if !tono.is_empty() {
            parti.push(tono);
        }
        let mut nome = format!("{}, modulo {} di {}", parti.join(", "), posizione + 1, self.posti.len());
        if let Some(a) = &r.azione {
            nome.push_str(", azione: ");
            nome.push_str(&a.testo);
        }
        nome
    }

    /// I moduli per l'interfaccia, nell'ordine.
    pub fn moduli_ui(&self) -> Vec<ModuloUi> {
        self.posti
            .iter()
            .enumerate()
            .map(|(i, posto)| {
                let (m, r, tono) = (posto.identita, &posto.riassunto, posto.tono());
                ModuloUi {
                    id: m.id.into(),
                    nome: m.nome.into(),
                    nome_breve: m.nome_breve.into(),
                    icona: icona(m.icona),
                    tono: tono_ui(tono),
                    parola: tono.parola().into(),
                    titolo: r.titolo.as_str().into(),
                    numerico: r.titolo_numerico,
                    sec: r.sec.as_str().into(),
                    con_azione: r.azione.is_some(),
                    azione_id: r.azione.as_ref().map(|a| a.id).unwrap_or_default().into(),
                    azione_testo: r.azione.as_ref().map(|a| a.testo.as_str()).unwrap_or_default().into(),
                    azione_icona: r.azione.as_ref().map(|a| icona(a.icona)).unwrap_or_default(),
                    narratore: self.nome_narrator(i).into(),
                    narratore_azione: r
                        .azione
                        .as_ref()
                        .map(|a| format!("{}, {}", a.testo, m.nome))
                        .unwrap_or_default()
                        .into(),
                }
            })
            .collect()
    }

    /// Il nome del modulo per le voci «In attesa» («Aspetta te · Agenti»): un id che non è un
    /// modulo (le voci di prova) dice «Prova».
    pub fn nome(&self, id: &str) -> &'static str {
        self.posto(id).map_or("Prova", |p| p.identita.nome)
    }
}

/// Il tono come lo vede l'interfaccia.
pub fn tono_ui(t: Tono) -> TonoModuloUi {
    match t {
        Tono::Vuoto => TonoModuloUi::Vuoto,
        Tono::Calmo => TonoModuloUi::Calmo,
        Tono::Attivo => TonoModuloUi::Attivo,
        Tono::Guarda => TonoModuloUi::Guarda,
        Tono::Aspetta => TonoModuloUi::Aspetta,
        Tono::Fatto => TonoModuloUi::Fatto,
        Tono::Errore => TonoModuloUi::Errore,
        Tono::Fermo => TonoModuloUi::Fermo,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::moduli::misura::{larghezza, Stile, MARGINE, SPAZIO_CON_AZIONE, SPAZIO_PIENO};
    use crate::moduli::{Azione, R1};

    fn config(ids: &[&str]) -> Vec<ConfigModulo> {
        ids.iter().map(|id| ConfigModulo { id: (*id).into(), fissato: true }).collect()
    }

    fn ids(r: &Registro) -> Vec<&'static str> {
        r.ordine().iter().map(|m| m.id).collect()
    }

    fn calmo(titolo: &str, sec: &str) -> Riassunto {
        Riassunto {
            tono: Tono::Calmo,
            titolo: titolo.into(),
            titolo_numerico: false,
            sec: sec.into(),
            sec_intera: sec.into(),
            azione: None,
            r1: R1::default(),
        }
    }

    #[test]
    fn ordine_predefinito() {
        let r = Registro::nuovo(&[]);
        assert_eq!(ids(&r), ["agenti", "casa", "pc", "collegamenti"]);
        let nomi: Vec<_> = r.ordine().iter().map(|m| m.nome).collect();
        assert_eq!(nomi, ["Agenti", "Casa", "PC", "Collegamenti"]);
        // Le Impostazioni e la Panoramica non sono moduli.
        assert!(r.ordine().iter().all(|m| m.id != "impostazioni" && m.id != "panoramica"));
    }

    /// Spec Review Focus 4: un ordine salvato con l'id `claude` di prima si legge con `agenti` al
    /// suo posto; gli id sconosciuti o doppi si scartano, i mancanti vanno in fondo.
    #[test]
    fn ordine_con_l_id_claude_diventa_agenti() {
        let r = Registro::nuovo(&config(&["collegamenti", "claude", "casa"]));
        assert_eq!(ids(&r), ["collegamenti", "agenti", "casa", "pc"]);
        let r = Registro::nuovo(&config(&["pc", "", "meteo", "pc", "agenti", "claude"]));
        assert_eq!(ids(&r), ["pc", "agenti", "casa", "collegamenti"]);
    }

    #[test]
    fn rifiuta_oltre_i_limiti() {
        let mut r = Registro::nuovo(&[]);
        let prima = calmo("4 luci", "21,5° dentro");
        assert_eq!(r.aggiorna("casa", prima.clone()), Ok(true));
        let largo = calmo("WWWWWWWWWWWWWWW", "21,5° dentro");
        match r.aggiorna("casa", largo) {
            Err(FuoriMisura { modulo: "casa", campo: "titolo", .. }) => {}
            altro => panic!("atteso il titolo fuori misura: {altro:?}"),
        }
        assert_eq!(r.nome_narrator(1), "Casa, 4 luci, 21,5° dentro, modulo 2 di 4", "resta il riassunto di prima");

        // Un titolo da circa 120 px: oltre lo spazio con l'azione, dentro quello pieno.
        let mut titolo = String::new();
        while larghezza(&titolo, Stile::Titolo) < 120.0 {
            titolo.push('a');
        }
        let l = larghezza(&titolo, Stile::Titolo);
        assert!(l > SPAZIO_CON_AZIONE - MARGINE && l <= SPAZIO_PIENO - MARGINE, "{titolo} = {l}");
        let mut con_azione = calmo(&titolo, "");
        con_azione.azione = Some(Azione { id: "spegni", testo: "Spegni tutte".into(), icona: "luce" });
        assert!(matches!(r.aggiorna("pc", con_azione), Err(FuoriMisura { campo: "titolo", .. })));
        assert_eq!(r.aggiorna("pc", calmo(&titolo, "")), Ok(true));

        // Anche la riga ha il suo limite.
        let riga_larga = calmo("PC", &"W".repeat(20));
        assert!(matches!(r.aggiorna("pc", riga_larga), Err(FuoriMisura { campo: "sec", .. })));
    }

    /// La fascia dell'Orologio al primo avvio: contano solo i moduli senza il primo riassunto, non
    /// un «—» con un motivo vero (il PC che legge).
    #[test]
    fn moduli_in_attesa_di_dati() {
        let mut r = Registro::nuovo(&[]);
        assert_eq!(r.in_attesa_di_dati(), 4);
        r.aggiorna("pc", Riassunto::primo_avvio("Lettura in corso")).unwrap();
        r.aggiorna("casa", calmo("4 luci", "21,5° dentro")).unwrap();
        assert_eq!(r.in_attesa_di_dati(), 2);
    }

    #[test]
    fn scrive_solo_se_cambia() {
        let mut r = Registro::nuovo(&[]);
        assert_eq!(r.aggiorna("pc", calmo("CPU 12%", "RAM 40%")), Ok(true));
        assert_eq!(r.aggiorna("pc", calmo("CPU 12%", "RAM 40%")), Ok(false));
        assert_eq!(r.aggiorna("pc", calmo("CPU 13%", "RAM 40%")), Ok(true));
    }

    #[test]
    fn voci_alzano_il_tono() {
        let mut r = Registro::nuovo(&[]);
        for m in MODULI {
            r.aggiorna(m.id, calmo("Tutto ok", "")).unwrap();
        }
        assert_eq!(r.tono_isola(), Tono::Calmo);
        assert!(r.livelli_voci(&[("agenti", Livello::Bloccante)]));
        assert_eq!(r.tono_effettivo("agenti"), Tono::Aspetta);
        assert_eq!(r.tono_effettivo("casa"), Tono::Calmo);
        assert_eq!(r.tono_isola(), Tono::Aspetta);
        assert!(!r.livelli_voci(&[("agenti", Livello::Bloccante)]), "stesso tono: niente da riscrivere");
        // Un'informazione non alza il tono (la bloccante è sparita: torna calmo).
        assert!(r.livelli_voci(&[("agenti", Livello::Informazione)]));
        assert_eq!(r.tono_effettivo("agenti"), Tono::Calmo);
        // Più voci dello stesso modulo: vince la più urgente.
        assert!(r.livelli_voci(&[("casa", Livello::Attenzione), ("casa", Livello::Bloccante)]));
        assert_eq!(r.tono_effettivo("casa"), Tono::Aspetta);
        assert!(r.livelli_voci(&[]));
        assert_eq!(r.tono_effettivo("casa"), Tono::Calmo);
        assert_eq!(r.tono_isola(), Tono::Calmo);
        // Un riassunto più urgente della voce resta il suo.
        let mut errore = calmo("Scollegata", "");
        errore.tono = Tono::Errore;
        r.aggiorna("casa", errore).unwrap();
        r.livelli_voci(&[("casa", Livello::Attenzione)]);
        assert_eq!(r.tono_effettivo("casa"), Tono::Errore);
    }

    #[test]
    fn nome_per_narrator_composto() {
        let mut r = Registro::nuovo(&[]);
        let mut casa = calmo("4 luci accese", "21,5° dentro");
        casa.azione = Some(Azione { id: "spegni", testo: "Spegni tutte".into(), icona: "luce" });
        // «4 luci accese» con l'azione sta nei 113 px.
        r.aggiorna("casa", casa).unwrap();
        assert_eq!(r.nome_narrator(1), "Casa, 4 luci accese, 21,5° dentro, modulo 2 di 4, azione: Spegni tutte");
        let mut agenti = calmo("2 sessioni", "7 giorni 61 %");
        agenti.tono = Tono::Aspetta;
        r.aggiorna("agenti", agenti).unwrap();
        assert_eq!(r.nome_narrator(0), "Agenti, 2 sessioni, 7 giorni 61 %, ti aspetta, modulo 1 di 4");
        // Il testo intero per Narrator anche quando la riga è accorciata.
        let mut lungo = calmo("3 link", "Il più usato: Posta…");
        lungo.sec_intera = "Il più usato: Posta dell'ufficio".into();
        r.aggiorna("collegamenti", lungo).unwrap();
        assert_eq!(r.nome_narrator(3), "Collegamenti, 3 link, Il più usato: Posta dell'ufficio, modulo 4 di 4");
        // Il «—» dell'assenza non si legge: solo il motivo.
        assert_eq!(r.nome_narrator(2), "PC, In attesa di dati, modulo 3 di 4");
        let mut collego = calmo("Mi collego", "—");
        collego.sec_intera = "—".into();
        r.aggiorna("casa", collego).unwrap();
        assert_eq!(r.nome_narrator(1), "Casa, Mi collego, modulo 2 di 4");
    }

    #[test]
    fn nome_del_modulo_per_le_voci() {
        let r = Registro::nuovo(&[]);
        assert_eq!(r.nome("agenti"), "Agenti");
        assert_eq!(r.nome("pc"), "PC");
        assert_eq!(r.nome("prova"), "Prova");
    }

    #[test]
    fn moduli_per_l_interfaccia() {
        let mut r = Registro::nuovo(&config(&["casa"]));
        let mut casa = calmo("4 luci accese", "21,5° dentro");
        casa.azione = Some(Azione { id: "spegni", testo: "Spegni tutte".into(), icona: "luce" });
        r.aggiorna("casa", casa).unwrap();
        r.livelli_voci(&[("casa", Livello::Attenzione)]);
        let ui = r.moduli_ui();
        assert_eq!(ui.iter().map(|m| m.id.to_string()).collect::<Vec<_>>(), ["casa", "agenti", "pc", "collegamenti"]);
        let c = &ui[0];
        assert_eq!((c.nome.as_str(), c.nome_breve.as_str()), ("Casa", "Casa"));
        assert_eq!((c.tono, c.parola.as_str()), (TonoModuloUi::Guarda, "Guarda"));
        assert_eq!((c.titolo.as_str(), c.sec.as_str(), c.numerico), ("4 luci accese", "21,5° dentro", false));
        assert!(c.con_azione);
        assert_eq!(c.azione_testo, "Spegni tutte");
        assert_eq!(c.narratore, "Casa, 4 luci accese, 21,5° dentro, guarda presto, modulo 1 di 4, azione: Spegni tutte");
        assert_eq!(c.narratore_azione, "Spegni tutte, Casa");
        assert!(c.icona.size().width > 0, "icona del modulo");
        assert_eq!(ui[3].nome_breve, "Link");
        // Senza dati: «—» col motivo, nessuna azione né parola.
        let a = &ui[1];
        assert_eq!((a.tono, a.titolo.as_str(), a.con_azione, a.parola.as_str()), (TonoModuloUi::Vuoto, "—", false, ""));
        assert!(!a.sec.is_empty(), "il motivo, mai un vuoto");
        assert_eq!(a.narratore_azione, "");
    }
}
