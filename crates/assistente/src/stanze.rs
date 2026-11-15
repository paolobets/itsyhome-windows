//! Le icone delle stanze: lo stesso insieme di 60 della v2 (`src/shared/roomIcons.ts`, ramo
//! `main`), con la stessa icona predefinita scelta dal nome della stanza.
//!
//! La v2 le disegnava come emoji e le salvava così in `areaIcons`: la configurazione continua a
//! tenere l'emoji (importata dalla v2 e scritta dalla v3 nello stesso modo), mentre l'interfaccia
//! mostra al suo posto un'icona a contorno di Lucide (licenza ISC,
//! `assets/icone/lucide/LICENSE-lucide.txt`), nel colore neutro delle icone come il resto
//! dell'assistente. L'ordine qui è quello di `Icone.stanze` in `ui/icone.slint`.

/// (emoji salvata in configurazione, nome per Narrator e per il suggerimento).
pub const ICONE: [(&str, &str); 60] = [
    // Zona giorno
    ("🛋️", "Divano"),
    ("🏠", "Casa"),
    ("🏡", "Casa con giardino"),
    ("🏘️", "Case"),
    ("🏰", "Castello"),
    ("🏖️", "Spiaggia"),
    // Cucina e pranzo
    ("🍳", "Pentola"),
    ("🍽️", "Posate"),
    ("☕", "Caffè"),
    ("🥂", "Calici"),
    ("🫕", "Zuppiera"),
    ("🍪", "Biscotto"),
    // Notte
    ("🛏️", "Letto matrimoniale"),
    ("🌙", "Luna"),
    ("🧸", "Giocattoli"),
    ("👶", "Neonato"),
    ("😴", "Letto singolo"),
    ("🌟", "Stella"),
    // Bagno
    ("🚿", "Doccia"),
    ("🛁", "Vasca"),
    ("🪥", "Gocce"),
    ("🪞", "Specchio"),
    ("🧴", "Flacone"),
    ("🧼", "Pulito"),
    // Studio
    ("🖥️", "Monitor"),
    ("💻", "Portatile"),
    ("📚", "Libreria"),
    ("📝", "Quaderno"),
    ("🎓", "Tocco"),
    ("🔬", "Microscopio"),
    // Giardino ed esterno
    ("🌳", "Albero"),
    ("🌿", "Foglia"),
    ("🪴", "Germoglio"),
    ("☀️", "Sole"),
    ("🌺", "Fiore"),
    ("🌻", "Girasole"),
    // Svago
    ("🎮", "Videogiochi"),
    ("📺", "Televisore"),
    ("🎬", "Ciak"),
    ("🎭", "Teatro"),
    ("🎵", "Musica"),
    ("🎸", "Chitarra"),
    // Sport e benessere
    ("💪", "Muscoli"),
    ("🏋️", "Pesi"),
    ("🧘", "Benessere"),
    ("🏊", "Piscina"),
    ("🚴", "Bicicletta"),
    ("⚽", "Pallone"),
    // Servizi e ripostigli
    ("🚗", "Auto"),
    ("🚪", "Porta"),
    ("📦", "Scatola"),
    ("🫧", "Lavatrice"),
    ("🧹", "Spazzola"),
    ("🔧", "Chiave inglese"),
    // Varie
    ("🔒", "Lucchetto"),
    ("💡", "Lampadina"),
    ("🔑", "Chiave"),
    ("🕯️", "Fiamma"),
    ("🌡️", "Termometro"),
    ("💧", "Goccia"),
];

/// Un'emoji senza il selettore di variante (U+FE0F): «🛋️» e «🛋» sono la stessa icona.
fn base(emoji: &str) -> String {
    emoji.chars().filter(|&c| c != '\u{FE0F}').collect()
}

/// La posizione di un'emoji nell'insieme (`None` se non è fra le 60).
pub fn indice(emoji: &str) -> Option<usize> {
    let cercata = base(emoji);
    ICONE.iter().position(|(e, _)| base(e) == cercata)
}

/// L'icona predefinita di una stanza dal suo nome, come `roomIcon` della v2 (nomi italiani e
/// inglesi; «studio» è lo studio/ufficio).
pub fn predefinita(nome: &str) -> &'static str {
    let n = nome.to_lowercase();
    let ha = |parole: &[&str]| parole.iter().any(|p| n.contains(p));
    if ha(&["living", "soggiorno", "salotto"]) {
        "🛋️"
    } else if ha(&["kitchen", "cucina"]) {
        "🍳"
    } else if ha(&["master", "main bedroom"]) {
        "🌙"
    } else if ha(&["cameretta", "nursery", "baby"]) {
        "🧸"
    } else if ha(&["bedroom", "guest", "camera da letto", "camera"]) {
        "🛏️"
    } else if ha(&["bath", "bagno", "bagnetto", "wc", "toilette"]) {
        "🚿"
    } else if ha(&["office", "ufficio", "studio"]) {
        "🖥️"
    } else if ha(&["hall", "corridor", "entryway", "ingresso", "corridoio", "disimpegno", "entrata"]) {
        "🚪"
    } else if ha(&["garage", "box"]) {
        "🚗"
    } else if ha(&["garden", "yard", "giardino", "esterno"]) {
        "🌳"
    } else if ha(&["terrace", "balcony", "patio", "terrazzo", "terrazza", "balcone"]) {
        "☀️"
    } else if ha(&["gym", "fitness", "palestra"]) {
        "💪"
    } else if ha(&["pool", "spa", "piscina"]) {
        "🏊"
    } else if ha(&["laundry", "lavanderia"]) {
        "🫧"
    } else if ha(&["dining", "pranzo", "sala pranzo", "tinello"]) {
        "🍽️"
    } else if ha(&["attic", "basement", "cantina", "ripostiglio", "mansarda", "soffitta"]) {
        "📦"
    } else {
        "🏠"
    }
}

/// L'icona di una stanza: quella scelta (salvata in `areaIcons`) se è fra le 60, altrimenti la
/// predefinita dal nome.
pub fn icona(scelta: Option<&str>, nome: &str) -> usize {
    scelta.and_then(indice).or_else(|| indice(predefinita(nome))).unwrap_or(1)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sessanta_icone_tutte_diverse() {
        let mut viste = std::collections::HashSet::new();
        for (e, nome) in ICONE {
            assert!(viste.insert(base(e)), "{e} due volte");
            assert!(!nome.is_empty());
        }
        assert_eq!(viste.len(), 60);
    }

    #[test]
    fn predefinita_come_la_v2() {
        assert_eq!(predefinita("Soggiorno"), "🛋️");
        assert_eq!(predefinita("Cucina"), "🍳");
        assert_eq!(predefinita("Cameretta"), "🧸", "prima di «camera»");
        assert_eq!(predefinita("Camera da letto"), "🛏️");
        assert_eq!(predefinita("Bagno"), "🚿");
        assert_eq!(predefinita("Studio"), "🖥️");
        assert_eq!(predefinita("Ingresso"), "🚪");
        assert_eq!(predefinita("Terrazzo"), "☀️");
        assert_eq!(predefinita("Soffitta"), "📦");
        assert_eq!(predefinita("Sala hobby"), "🏠");
        // Ogni predefinita è fra le 60.
        for nome in ["Soggiorno", "Garage", "Giardino", "Palestra", "Piscina", "Lavanderia", "Tinello"] {
            assert!(indice(predefinita(nome)).is_some(), "{nome}");
        }
    }

    #[test]
    fn la_scelta_vince_e_il_selettore_non_conta() {
        assert_eq!(icona(Some("🎮"), "Cucina"), indice("🎮").unwrap());
        // La v2 salvava a volte l'emoji senza il selettore di variante.
        assert_eq!(icona(Some("🛋"), "Cucina"), 0);
        // Un'emoji che non è fra le 60: vale la predefinita.
        assert_eq!(icona(Some("🦄"), "Cucina"), indice("🍳").unwrap());
        assert_eq!(icona(None, "Sala hobby"), 1);
    }
}
