//! L'arbitro della pillola (ridisegno F4): decide cosa mostra la pillola fra permessi bloccanti,
//! avvisi, fatti e attività. Logica pura: il tempo arriva sempre dal chiamante, nessun timer.
//!
//! Precedenza, senza rotazione: bloccante di qualunque modulo (il più vecchio) → attenzione nuova
//! → fatto non visto → attività del primo modulo (a parità la più recente) → riposo. Con «Non
//! disturbare» passa solo il bloccante; il resto lascia la pillola a riposo, con il puntino.
//!
//! Un'attenzione allarga la pillola per `ALLARGATA_PER`; lo stesso **modulo** non la riallarga
//! prima di `RIALLARGA_DOPO` (il limite è per modulo, non globale).

use crate::attenzione::{Attese, Livello, Voce, ALLARGATA_PER};
use crate::moduli::MODULI;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Dopo quanto lo stesso modulo può riallargare la pillola con un'altra attenzione.
pub const RIALLARGA_DOPO: Duration = Duration::from_secs(10 * 60);
/// Quanto dura la sfumatura verde acqua di un fatto già visto.
pub const SFUMA_FATTO: Duration = Duration::from_millis(2800);

/// Un lavoro in corso di un modulo (per ora le sessioni di Claude Code).
#[derive(Debug, Clone, PartialEq)]
pub struct Attivita {
    pub modulo: &'static str,
    pub id: String,
    pub nome: String,
    pub resto: String,
    pub dal: Option<Instant>,
    pub iniziata: Instant,
}

/// Un lavoro finito. `visto_alle` è l'istante in cui l'utente l'ha guardato (pannello aperto o
/// clic sulla pillola): da lì parte la sfumatura di `SFUMA_FATTO`; `None` = non ancora visto.
#[derive(Debug, Clone, PartialEq)]
pub struct Fatto {
    pub modulo: &'static str,
    pub id: String,
    pub nome: String,
    pub resto: String,
    pub durata: Option<Duration>,
    pub finito: Instant,
    pub visto_alle: Option<Instant>,
}

/// Cosa mostra la pillola.
#[derive(Debug, Clone, PartialEq)]
pub enum Mostra {
    /// A riposo; il puntino è il livello più urgente fra le voci in attesa (nessuna: `None`).
    Riposo { puntino: Option<Livello> },
    /// Permesso che aspetta: `altri` sono gli altri bloccanti oltre a questo.
    Bloccante { voce_id: String, altri: usize },
    Attenzione { voce_id: String },
    /// `visto_alle`: da quando il fatto è visto (inizia la sfumatura verde acqua).
    Fatto { id: String, visto_alle: Option<Instant> },
    /// `altre`: i nomi delle altre attività, dalla più recente (per «+N» e Narrator).
    Attivita { id: String, altre: Vec<String> },
}

/// Tiene la sola memoria che serve: l'ultimo allargamento per modulo.
#[derive(Debug, Default)]
pub struct Arbitro {
    allargate: HashMap<&'static str, Instant>,
}

impl Arbitro {
    /// Un'attenzione è «nuova» se non è vista, ha meno di `ALLARGATA_PER` e il suo modulo non ha
    /// allargato nei `RIALLARGA_DOPO` prima della sua creazione (o è proprio quella mostrata).
    fn e_nuova(&self, attese: &Attese, v: &Voce, ora: Instant) -> bool {
        if v.livello != Livello::Attenzione
            || attese.vista(&v.id)
            || ora.saturating_duration_since(v.creata) >= ALLARGATA_PER
        {
            return false;
        }
        match self.allargate.get(v.fonte.modulo()) {
            None => true,
            Some(&quando) => quando == v.creata || v.creata.saturating_duration_since(quando) >= RIALLARGA_DOPO,
        }
    }

    /// Un'attenzione che la pillola sta mostrando larga adesso (già scelta da `scegli`).
    fn larga_ora(&self, attese: &Attese, v: &Voce, ora: Instant) -> bool {
        self.allargate.get(v.fonte.modulo()) == Some(&v.creata)
            && v.livello == Livello::Attenzione
            && !attese.vista(&v.id)
            && ora.saturating_duration_since(v.creata) < ALLARGATA_PER
    }

    pub fn scegli(
        &mut self,
        attese: &Attese,
        attivita: &[Attivita],
        fatti: &[Fatto],
        non_disturbare: bool,
        ora: Instant,
    ) -> Mostra {
        let ordinate = attese.ordinate();

        // 1. Il bloccante più vecchio, anche in Non disturbare.
        let bloccanti = ordinate.iter().filter(|v| v.livello == Livello::Bloccante);
        if let Some(b) = bloccanti.clone().min_by_key(|v| v.creata) {
            return Mostra::Bloccante { voce_id: b.id.clone(), altri: bloccanti.count() - 1 };
        }

        if !non_disturbare {
            // 2. Un'attenzione nuova: si ricorda l'allargamento del suo modulo.
            if let Some(a) = ordinate.iter().find(|v| self.e_nuova(attese, v, ora)) {
                self.allargate.insert(a.fonte.modulo(), a.creata);
                return Mostra::Attenzione { voce_id: a.id.clone() };
            }

            // 3. Un fatto: prima i non visti, poi quelli che sfumano; i più recenti prima. Una
            //    attività nuova dello stesso modulo lo sostituisce.
            let fatto = fatti
                .iter()
                .filter(|f| f.visto_alle.is_none_or(|v| ora.saturating_duration_since(v) < SFUMA_FATTO))
                .filter(|f| !attivita.iter().any(|a| a.modulo == f.modulo && a.iniziata > f.finito))
                .min_by_key(|f| (f.visto_alle.is_some(), Reverse(f.finito)));
            if let Some(f) = fatto {
                return Mostra::Fatto { id: f.id.clone(), visto_alle: f.visto_alle };
            }

            // 4. Un'attività: il primo modulo (nell'ordine di `MODULI`), a parità la più recente.
            let posto = |a: &Attivita| MODULI.iter().position(|m| m.id == a.modulo).unwrap_or(MODULI.len());
            if let Some(scelta) = attivita.iter().min_by_key(|a| (posto(a), Reverse(a.iniziata))) {
                let mut altre: Vec<&Attivita> = attivita.iter().filter(|a| a.id != scelta.id).collect();
                altre.sort_by_key(|a| Reverse(a.iniziata));
                return Mostra::Attivita {
                    id: scelta.id.clone(),
                    altre: altre.into_iter().map(|a| a.nome.clone()).collect(),
                };
            }
        }

        // 5. Riposo: il puntino è il livello più urgente fra le voci rimaste (`ordinate` parte
        //    dalla più urgente).
        Mostra::Riposo { puntino: ordinate.first().map(|v| v.livello) }
    }

    /// Fra quanto la scelta cambierà da sola (fine dei 20 s dell'attenzione larga, fine della
    /// sfumatura di un fatto); `None` se nulla scade: solo allora serve una sveglia. Va chiamata
    /// **dopo** `scegli`: «larga» è solo l'attenzione che `scegli` ha già scelto.
    pub fn prossimo_cambio(&self, attese: &Attese, fatti: &[Fatto], ora: Instant) -> Option<Duration> {
        let attenzioni = attese
            .ordinate()
            .into_iter()
            .filter(|v| self.larga_ora(attese, v, ora))
            .map(|v| (v.creata + ALLARGATA_PER).saturating_duration_since(ora));
        let fatti = fatti
            .iter()
            .filter_map(|f| f.visto_alle)
            .map(|v| (v + SFUMA_FATTO).saturating_duration_since(ora))
            .filter(|d| !d.is_zero());
        attenzioni.chain(fatti).min()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::attenzione::test::voce;
    use crate::attenzione::Fonte;

    const S: fn(u64) -> Duration = Duration::from_secs;

    fn di(fonte: Fonte, id: &str, livello: Livello, creata: Instant) -> Voce {
        Voce { fonte, ..voce(id, livello, creata) }
    }

    fn attese(voci: Vec<Voce>) -> Attese {
        let mut a = Attese::default();
        for v in voci {
            a.metti(v);
        }
        a
    }

    fn attivita(modulo: &'static str, id: &str, iniziata: Instant) -> Attivita {
        Attivita { modulo, id: id.into(), nome: format!("Nome {id}"), resto: String::new(), dal: None, iniziata }
    }

    fn fatto(modulo: &'static str, id: &str, finito: Instant, visto_alle: Option<Instant>) -> Fatto {
        Fatto {
            modulo,
            id: id.into(),
            nome: format!("Nome {id}"),
            resto: String::new(),
            durata: None,
            finito,
            visto_alle,
        }
    }

    #[test]
    fn bloccante_prima_di_tutto() {
        let t = Instant::now();
        let a = attese(vec![
            di(Fonte::Casa, "att", Livello::Attenzione, t),
            di(Fonte::Claude, "b2", Livello::Bloccante, t + S(5)),
            di(Fonte::Claude, "b1", Livello::Bloccante, t),
        ]);
        let m = Arbitro::default().scegli(
            &a,
            &[attivita("agenti", "x", t)],
            &[fatto("agenti", "f", t, None)],
            false,
            t + S(6),
        );
        assert_eq!(m, Mostra::Bloccante { voce_id: "b1".into(), altri: 1 });
    }

    #[test]
    fn attenzione_larga_20_s_poi_puntino() {
        let t = Instant::now();
        let a = attese(vec![di(Fonte::Casa, "a", Livello::Attenzione, t)]);
        let mut arb = Arbitro::default();
        assert_eq!(arb.scegli(&a, &[], &[], false, t + S(19)), Mostra::Attenzione { voce_id: "a".into() });
        assert_eq!(arb.scegli(&a, &[], &[], false, t + S(20)), Mostra::Riposo { puntino: Some(Livello::Attenzione) });
    }

    #[test]
    fn dieci_minuti_per_modulo_non_globali() {
        let t = Instant::now();
        let mut arb = Arbitro::default();
        let mut a = attese(vec![di(Fonte::Casa, "c1", Livello::Attenzione, t)]);
        assert_eq!(arb.scegli(&a, &[], &[], false, t), Mostra::Attenzione { voce_id: "c1".into() });

        // Un altro modulo, un minuto dopo: allarga lo stesso.
        a.metti(di(Fonte::Pc, "p1", Livello::Attenzione, t + S(60)));
        assert_eq!(arb.scegli(&a, &[], &[], false, t + S(60)), Mostra::Attenzione { voce_id: "p1".into() });

        // Casa di nuovo dopo 3 minuti: solo il puntino.
        a.metti(di(Fonte::Casa, "c2", Livello::Attenzione, t + S(180)));
        assert_eq!(arb.scegli(&a, &[], &[], false, t + S(180)), Mostra::Riposo { puntino: Some(Livello::Attenzione) });

        // Casa dopo oltre 10 minuti dal primo allargamento: allarga.
        let tardi = t + S(10 * 60 + 1);
        a.metti(di(Fonte::Casa, "c3", Livello::Attenzione, tardi));
        assert_eq!(arb.scegli(&a, &[], &[], false, tardi), Mostra::Attenzione { voce_id: "c3".into() });
    }

    #[test]
    fn attenzione_vista_non_allarga() {
        let t = Instant::now();
        let mut a = attese(vec![di(Fonte::Casa, "a", Livello::Attenzione, t)]);
        a.segna_viste();
        let m = Arbitro::default().scegli(&a, &[], &[], false, t + S(1));
        assert_eq!(m, Mostra::Riposo { puntino: Some(Livello::Attenzione) });
    }

    #[test]
    fn fatto_prima_dell_attivita() {
        let t = Instant::now();
        let m = Arbitro::default().scegli(
            &Attese::default(),
            &[attivita("agenti", "x", t)],
            &[fatto("agenti", "f", t + S(5), None)],
            false,
            t + S(6),
        );
        assert_eq!(m, Mostra::Fatto { id: "f".into(), visto_alle: None });
    }

    #[test]
    fn attivita_nuova_sostituisce_il_fatto() {
        let t = Instant::now();
        let m = Arbitro::default().scegli(
            &Attese::default(),
            &[attivita("agenti", "nuova", t + S(10))],
            &[fatto("agenti", "f", t, None)],
            false,
            t + S(11),
        );
        assert_eq!(m, Mostra::Attivita { id: "nuova".into(), altre: vec![] });
    }

    #[test]
    fn fatto_visto_sfuma_e_torna_il_riposo() {
        let t = Instant::now();
        let f = [fatto("agenti", "f", t, Some(t))];
        let mut arb = Arbitro::default();
        let prima = t + SFUMA_FATTO - Duration::from_millis(1);
        assert_eq!(arb.scegli(&Attese::default(), &[], &f, false, prima), Mostra::Fatto { id: "f".into(), visto_alle: Some(t) });
        assert_eq!(arb.scegli(&Attese::default(), &[], &f, false, t + SFUMA_FATTO), Mostra::Riposo { puntino: None });
    }

    #[test]
    fn attivita_la_piu_recente_e_piu_n() {
        let t = Instant::now();
        let at = [attivita("agenti", "a", t), attivita("agenti", "b", t + S(1)), attivita("agenti", "c", t + S(2))];
        let m = Arbitro::default().scegli(&Attese::default(), &at, &[], false, t + S(3));
        assert_eq!(m, Mostra::Attivita { id: "c".into(), altre: vec!["Nome b".into(), "Nome a".into()] });
    }

    #[test]
    fn non_disturbare_lascia_solo_il_bloccante() {
        let t = Instant::now();
        let mut arb = Arbitro::default();
        let at = [attivita("agenti", "x", t)];
        let mut a = attese(vec![di(Fonte::Casa, "a", Livello::Attenzione, t)]);
        assert_eq!(arb.scegli(&a, &at, &[], true, t + S(1)), Mostra::Riposo { puntino: Some(Livello::Attenzione) });
        // L'attenzione arrivata in ND non ha consumato i 10 minuti di Casa: una seconda voce di
        // Casa, 3 minuti dopo e fuori da ND, allarga (se l'allargamento fosse registrato in ND,
        // sarebbe bloccata).
        a.metti(di(Fonte::Casa, "a2", Livello::Attenzione, t + S(180)));
        assert_eq!(arb.scegli(&a, &at, &[], false, t + S(180)), Mostra::Attenzione { voce_id: "a2".into() });

        let b = attese(vec![di(Fonte::Claude, "b", Livello::Bloccante, t), di(Fonte::Casa, "a", Livello::Attenzione, t)]);
        assert_eq!(arb.scegli(&b, &at, &[], true, t + S(3)), Mostra::Bloccante { voce_id: "b".into(), altri: 0 });
    }

    #[test]
    fn prossimo_cambio_solo_se_qualcosa_scade() {
        let t = Instant::now();
        let mut arb = Arbitro::default();
        assert_eq!(arb.prossimo_cambio(&Attese::default(), &[], t), None);

        let a = attese(vec![di(Fonte::Casa, "a", Livello::Attenzione, t)]);
        arb.scegli(&a, &[], &[], false, t);
        assert_eq!(arb.prossimo_cambio(&a, &[], t + S(5)), Some(S(15)));
        // Dopo i 20 s non scade più nulla.
        assert_eq!(arb.prossimo_cambio(&a, &[], t + S(20)), None);

        let f = [fatto("agenti", "f", t, Some(t + S(1)))];
        let vuote = Attese::default();
        assert_eq!(arb.prossimo_cambio(&vuote, &f, t + S(2)), Some(SFUMA_FATTO - S(1)));
        // Un fatto non visto non scade da solo.
        assert_eq!(arb.prossimo_cambio(&vuote, &[fatto("agenti", "g", t, None)], t + S(2)), None);
    }
}
