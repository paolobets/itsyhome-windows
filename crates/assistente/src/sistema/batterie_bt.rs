//! La batteria dei dispositivi Bluetooth (cuffie, tastiere…) (M2, Task 9). Le decisioni pure
//! (chi avvisare, la voce, la riga della vista PC) stanno qui e si provano ovunque; la lettura
//! con SetupAPI e il thread `batterie-bt` esistono solo su Windows.
//!
//! Documentazione letta prima di scrivere (learn.microsoft.com, 16/11/2026), con i vincoli che
//! questo modulo rispetta:
//!
//! - `SetupDiGetClassDevsW` (windows/win32/api/setupapi/nf-setupapi-setupdigetclassdevsw):
//!   con `ClassGuid` e `Enumerator` nulli e `DIGCF_ALLCLASSES | DIGCF_PRESENT` elenca tutti i
//!   dispositivi presenti (esempio 2 della pagina). Dà `INVALID_HANDLE_VALUE` in caso d'errore;
//!   l'insieme va sempre liberato con `SetupDiDestroyDeviceInfoList`. Nessun vincolo di thread
//!   né di COM: si usa dal thread di lavoro.
//! - `SetupDiGetDevicePropertyW` (…/nf-setupapi-setupdigetdevicepropertyw): da Windows Vista;
//!   `Flags` deve essere 0; il buffer deve contenere tutto il valore, altrimenti
//!   `ERROR_INSUFFICIENT_BUFFER`; una proprietà che il dispositivo non ha dà `ERROR_NOT_FOUND`
//!   (qui: «niente valore», mai un errore). Il tipo si controlla sempre (`DEVPROP_TYPE_*`).
//! - Nome amichevole: `DEVPKEY_Device_FriendlyName` ({a45c254e-…}, pid 14, stringa;
//!   devpkey-device-friendlyname). Id di istanza: `DEVPKEY_Device_InstanceId` (pid 256, stringa,
//!   devpkey-device-instanceid); contiene l'indirizzo del dispositivo, quindi si salva solo come
//!   impronta (`impronta`).
//! - `DEVPKEY_Device_ContainerId` ({8c7ed206-…}, pid 2, `DEVPROP_TYPE_GUID`; devpkey-device-containerid):
//!   identifica il dispositivo fisico, uguale per tutti i suoi nodi: l'impronta si ricava da lui
//!   (vedi `unifica`). Senza contenitore si ripiega sull'id di istanza del nodo.
//! - `CM_Get_DevNode_Status` (…/cfgmgr32/nf-cfgmgr32-cm_get_devnode_status): `pulStatus` con i
//!   flag `DN_*` (qui `DN_STARTED`), `ulFlags` 0; restituisce `CR_SUCCESS` o un errore `CR_*`.
//!
//! **Non documentato da Microsoft** (ricerca su learn.microsoft.com senza risultati, quindi
//! sola lettura e il codice ne sopporta l'assenza):
//! - La chiave della batteria Bluetooth {104EA319-6EE2-4701-BD47-8DDBF425BBE5}, pid 2, tipo
//!   `DEVPROP_TYPE_BYTE` (percentuale 0–100). Un dispositivo che non la espone, un tipo diverso
//!   o un valore oltre 100 non producono nessuna lettura.
//! - Che cosa distingue un dispositivo associato ma **non connesso** (Review Focus 5): un
//!   dispositivo associato resta presente (`DIGCF_PRESENT`) e il suo nodo può restare avviato
//!   (`DN_STARTED`) anche a cuffie spente, quindi `DN_STARTED` da solo non basta. Si richiede
//!   anche la proprietà booleana {83DA6326-97A6-4088-9453-A1923F573B29}, pid 15 («connesso»,
//!   anch'essa non documentata, `DEVPROP_TYPE_BOOLEAN`). Se manca o non è vera il dispositivo
//!   conta come **non connesso**: nel dubbio nessun avviso (un valore di batteria vecchio di
//!   cuffie spente non deve mai far suonare niente). **Da confermare dal vivo con Paolo.**
//!
//! Leggerezza: il thread `batterie-bt` aspetta con `recv_timeout` (nessun sondaggio, nessuna CPU):
//! prima lettura 60 s dopo l'avvio, poi ogni 15 minuti; si ferma quando l'interruttore si spegne
//! (cade il `Sender`). La riga «CUFFIE» della vista PC fa una sola lettura per ingresso nella
//! vista, su un thread di lavoro. Nomi e indirizzi non vanno mai nei log né nei test.

// Fuori da Windows il modulo c'è solo per i test, che ne usano una parte.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::attenzione::{Fonte, Livello, Pulsante, Voce, PREFISSO_BT, PULSANTE_HO_CAPITO};
use crate::pc_stats::TintaBatteria;
use crate::Pannello;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Sotto o a questa percentuale (dispositivo connesso) si avvisa.
pub const SOGLIA_BT: u8 = 20;
/// La voce già presente resta fino a questa percentuale (isteresi): sopra, sparisce.
pub const SOGLIA_BT_FINE: u8 = 25;
/// Ogni quanto si rilegge, a interruttore acceso.
pub const PERIODO_BT: Duration = Duration::from_secs(15 * 60);
/// Dopo quanto dall'avvio la prima lettura.
pub const PRIMA_LETTURA_BT: Duration = Duration::from_secs(60);
/// Le impronte avvisate da più di tanti giorni si tolgono a ogni salvataggio.
pub const GIORNI_IMPRONTE: i64 = 7;
/// Oltre tanti caratteri il nome nella riga della vista PC si accorcia con «…».
pub const NOME_MASSIMO: usize = 16;

/// Un dispositivo Bluetooth con la batteria leggibile. `nome` si vede solo nella vista PC e nel
/// titolo dell'avviso: mai nei log. `impronta` è l'unica cosa che si salva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetturaBt {
    pub impronta: String,
    pub nome: String,
    pub percentuale: u8,
    pub connesso: bool,
}

/// FNV-1a a 64 bit dell'id di istanza, in 16 cifre esadecimali minuscole: l'id contiene
/// l'indirizzo del dispositivo e non si salva mai in chiaro.
pub fn impronta(id_istanza: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id_istanza.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// L'id della voce di un dispositivo.
pub fn id_voce(impronta: &str) -> String {
    format!("{PREFISSO_BT}{impronta}")
}

/// Chi avvisare: connessi, a `SOGLIA_BT` o meno, non ancora avvisati oggi (`oggi` come
/// «2026-11-15»; `avvisati`: impronta → data dell'ultimo avviso, salvata in configurazione).
pub fn da_avvisare(letture: &[LetturaBt], avvisati: &HashMap<String, String>, oggi: &str) -> Vec<LetturaBt> {
    letture
        .iter()
        .filter(|l| l.connesso && l.percentuale <= SOGLIA_BT)
        .filter(|l| avvisati.get(&l.impronta).map(String::as_str) != Some(oggi))
        .cloned()
        .collect()
}

/// Le voci già presenti da togliere: dispositivo non più connesso, sparito dalle letture o sopra
/// `SOGLIA_BT_FINE`. `presenti`: gli id delle voci in attesa che cominciano per `PREFISSO_BT`.
pub fn da_togliere(letture: &[LetturaBt], presenti: &[String]) -> Vec<String> {
    presenti
        .iter()
        .filter(|id| {
            let Some(imp) = id.strip_prefix(PREFISSO_BT) else { return false };
            !letture.iter().any(|l| l.impronta == imp && l.connesso && l.percentuale <= SOGLIA_BT_FINE)
        })
        .cloned()
        .collect()
}

/// La voce di un dispositivo scarico: *informazione*, con «Ho capito».
pub fn voce_bt(l: &LetturaBt, ora: Instant) -> Voce {
    Voce {
        id: id_voce(&l.impronta),
        livello: Livello::Informazione,
        fonte: Fonte::Pc,
        titolo: format!("{} al {}%", l.nome, l.percentuale),
        dettaglio: "Ricarica il dispositivo Bluetooth quando puoi.".into(),
        pulsanti: vec![Pulsante::new(PULSANTE_HO_CAPITO, "Ho capito")],
        creata: ora,
    }
}

/// Le impronte avvisate da più di `GIORNI_IMPRONTE` giorni (e quelle con una data illeggibile)
/// si tolgono: il file non cresce per sempre.
pub fn pota_avvisati(avvisati: &mut HashMap<String, String>, oggi: &str) {
    let Ok(oggi) = chrono::NaiveDate::parse_from_str(oggi, "%Y-%m-%d") else { return };
    avvisati.retain(|_, data| {
        chrono::NaiveDate::parse_from_str(data, "%Y-%m-%d")
            .is_ok_and(|d| (oggi - d).num_days() <= GIORNI_IMPRONTE)
    });
}

/// Più nodi dello stesso dispositivo (audio, telefonia, GATT…) portano la stessa impronta, perché
/// `leggi` la ricava dal contenitore (`DEVPKEY_Device_ContainerId`) e non dal singolo nodo: ne
/// resta una lettura, connessa se lo è almeno un nodo, con la percentuale minima fra i nodi
/// connessi (se nessuno lo è, fra tutti). L'impronta non dipende da quale nodo è più basso, quindi
/// non cambia durante la giornata (profili diversi riportano la batteria a passi diversi) e
/// l'avviso resta uno al giorno. Due dispositivi distinti con lo stesso nome hanno contenitori
/// diversi e restano separati. L'ordine di prima si mantiene.
pub fn unifica(letture: Vec<LetturaBt>) -> Vec<LetturaBt> {
    let mut fuori: Vec<LetturaBt> = Vec::new();
    for l in letture {
        let Some(x) = fuori.iter_mut().find(|x| x.impronta == l.impronta) else {
            fuori.push(l);
            continue;
        };
        // Percentuale: la minima fra i connessi; un connesso vince su un non connesso.
        if (l.connesso, std::cmp::Reverse(l.percentuale)) > (x.connesso, std::cmp::Reverse(x.percentuale)) {
            x.percentuale = l.percentuale;
            x.connesso = l.connesso;
        }
    }
    fuori
}

/// La riga «CUFFIE» della vista PC: il dispositivo connesso con la batteria più bassa.
#[derive(Debug, Clone, PartialEq)]
pub struct RigaCuffie {
    /// Il nome, accorciato se serve.
    pub nome: String,
    pub percentuale: u8,
    /// Gli altri dispositivi connessi con la batteria leggibile («+1»).
    pub altri: usize,
    pub tinta: TintaBatteria,
    /// Per Narrator: tutti i dispositivi, per nome intero e percentuale.
    pub descrizione: String,
}

/// Accorcia a `massimo` caratteri con «…» (mai a metà di un carattere).
pub fn accorcia(nome: &str, massimo: usize) -> String {
    if nome.chars().count() <= massimo {
        return nome.to_string();
    }
    let testa: String = nome.chars().take(massimo.saturating_sub(1)).collect();
    format!("{}…", testa.trim_end())
}

/// La riga da mostrare, `None` se nessun dispositivo connesso ha la batteria leggibile.
pub fn riga_cuffie(letture: &[LetturaBt]) -> Option<RigaCuffie> {
    let mut connessi: Vec<&LetturaBt> = letture.iter().filter(|l| l.connesso).collect();
    connessi.sort_by_key(|l| l.percentuale);
    let primo = connessi.first()?;
    let descrizione = connessi.iter().map(|l| format!("{} al {}%", l.nome, l.percentuale)).collect::<Vec<_>>().join(", ");
    Some(RigaCuffie {
        nome: accorcia(&primo.nome, NOME_MASSIMO),
        percentuale: primo.percentuale,
        altri: connessi.len() - 1,
        tinta: if primo.percentuale <= SOGLIA_BT { TintaBatteria::Informazione } else { TintaBatteria::Normale },
        descrizione,
    })
}

/// Scrive la riga nel pannello (`None`: la riga non c'è). Ogni proprietà si riscrive solo se
/// cambia.
pub fn scrivi_cuffie(p: &Pannello, riga: Option<&RigaCuffie>) {
    if p.get_cuffie_presenti() != riga.is_some() {
        p.set_cuffie_presenti(riga.is_some());
    }
    let Some(r) = riga else { return };
    let dettaglio = if r.altri > 0 { format!("{} +{}", r.nome, r.altri) } else { r.nome.clone() };
    if p.get_cuffie_dettaglio() != dettaglio.as_str() {
        p.set_cuffie_dettaglio(dettaglio.into());
    }
    let testo = format!("{}%", r.percentuale);
    if p.get_cuffie_testo() != testo.as_str() {
        p.set_cuffie_testo(testo.into());
    }
    let frazione = f32::from(r.percentuale) / 100.0;
    if p.get_cuffie_frazione() != frazione {
        p.set_cuffie_frazione(frazione);
    }
    let tinta = crate::pannello::tinta_ui(r.tinta);
    if p.get_cuffie_tinta() != tinta {
        p.set_cuffie_tinta(tinta);
    }
    if p.get_cuffie_descrizione() != r.descrizione.as_str() {
        p.set_cuffie_descrizione(r.descrizione.clone().into());
    }
}

/// Il thread `batterie-bt`: aspetta (`recv_timeout`, nessun sondaggio), legge alla prima
/// scadenza e poi a ogni periodo, e si ferma appena il gestore cade (interruttore spento,
/// uscita dal programma) o `consegna` dice che nessuno ascolta più.
pub struct SorveglianzaBt {
    // Cadendo, sveglia il thread con `Disconnected` e lo fa finire.
    _ferma: mpsc::Sender<()>,
}

impl SorveglianzaBt {
    pub fn avvia_con(
        prima: Duration,
        periodo: Duration,
        leggi: impl Fn() -> Vec<LetturaBt> + Send + 'static,
        consegna: impl Fn(Vec<LetturaBt>) -> bool + Send + 'static,
    ) -> SorveglianzaBt {
        let (ferma, rx) = mpsc::channel::<()>();
        let avviato = std::thread::Builder::new().name("batterie-bt".into()).spawn(move || {
            let mut attesa = prima;
            // Un `send` non c'è mai: solo `Timeout` (leggi) o `Disconnected` (esci).
            while let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(attesa) {
                if !consegna(leggi()) {
                    return;
                }
                attesa = periodo;
            }
        });
        if let Err(e) = avviato {
            eprintln!("thread batterie-bt non avviato: {e}");
        }
        SorveglianzaBt { _ferma: ferma }
    }

    /// Con la lettura vera di Windows: prima a 60 s, poi ogni 15 minuti.
    #[cfg(windows)]
    pub fn avvia(consegna: impl Fn(Vec<LetturaBt>) -> bool + Send + 'static) -> SorveglianzaBt {
        SorveglianzaBt::avvia_con(PRIMA_LETTURA_BT, PERIODO_BT, leggi, consegna)
    }
}

/// Una lettura sola su un thread di lavoro (la riga «CUFFIE» entrando nella vista PC): mai sul
/// thread dell'interfaccia. `consegna` riceve il risultato su quel thread di lavoro.
#[cfg(windows)]
pub fn leggi_una_volta(consegna: impl FnOnce(Vec<LetturaBt>) + Send + 'static) {
    let avviato = std::thread::Builder::new().name("cuffie-bt".into()).spawn(move || consegna(leggi()));
    if let Err(e) = avviato {
        eprintln!("thread cuffie-bt non avviato: {e}");
    }
}

/// Tutti i dispositivi presenti con la batteria leggibile, un solo giro di SetupAPI (pochi ms).
/// Errori (nessun accesso, nessun dispositivo) = elenco vuoto: mai un panico.
#[cfg(windows)]
pub fn leggi() -> Vec<LetturaBt> {
    use std::mem::{size_of, zeroed};
    use std::ptr::null;
    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_Status, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDevicePropertyW, CR_SUCCESS, DIGCF_ALLCLASSES, DIGCF_PRESENT, DN_STARTED, HDEVINFO, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Devices::Properties::{
        DEVPKEY_Device_ContainerId, DEVPKEY_Device_FriendlyName, DEVPKEY_Device_InstanceId, DEVPROPKEY, DEVPROPTYPE,
        DEVPROP_TYPE_BOOLEAN, DEVPROP_TYPE_BYTE, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING,
    };
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    // Non documentate da Microsoft (vedi l'intestazione del modulo).
    const BATTERIA: DEVPROPKEY =
        DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };
    const CONNESSO: DEVPROPKEY =
        DEVPROPKEY { fmtid: GUID::from_u128(0x83da6326_97a6_4088_9453_a1923f573b29), pid: 15 };

    /// I byte di una proprietà del tipo atteso (`None`: manca, tipo diverso, buffer piccolo).
    unsafe fn proprieta(
        set: HDEVINFO,
        dati: &SP_DEVINFO_DATA,
        chiave: &DEVPROPKEY,
        tipo_atteso: DEVPROPTYPE,
        buffer: &mut [u8],
    ) -> Option<usize> {
        let mut tipo: DEVPROPTYPE = 0;
        let mut dimensione: u32 = 0;
        let ok = SetupDiGetDevicePropertyW(
            set,
            dati,
            chiave,
            &mut tipo,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            &mut dimensione,
            0,
        );
        (ok != 0 && tipo == tipo_atteso && dimensione as usize <= buffer.len()).then_some(dimensione as usize)
    }

    unsafe fn stringa(set: HDEVINFO, dati: &SP_DEVINFO_DATA, chiave: &DEVPROPKEY) -> Option<String> {
        let mut buffer = [0u8; 1024];
        let n = proprieta(set, dati, chiave, DEVPROP_TYPE_STRING, &mut buffer)?;
        let unita: Vec<u16> = buffer[..n].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let fine = unita.iter().position(|&u| u == 0).unwrap_or(unita.len());
        Some(String::from_utf16_lossy(&unita[..fine]))
    }

    let mut letture = Vec::new();
    // SAFETY: chiamate a SetupAPI con puntatori a variabili locali vive per tutta la chiamata;
    // l'insieme dei dispositivi si libera sempre prima di uscire.
    unsafe {
        let set = SetupDiGetClassDevsW(null(), null(), 0, DIGCF_PRESENT | DIGCF_ALLCLASSES);
        if set == INVALID_HANDLE_VALUE as HDEVINFO {
            return letture;
        }
        let mut dati: SP_DEVINFO_DATA = zeroed();
        dati.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
        let mut indice = 0;
        while SetupDiEnumDeviceInfo(set, indice, &mut dati) != 0 {
            indice += 1;
            let mut b = [0u8; 4];
            let Some(1) = proprieta(set, &dati, &BATTERIA, DEVPROP_TYPE_BYTE, &mut b) else { continue };
            if b[0] > 100 {
                continue;
            }
            let Some(id) = stringa(set, &dati, &DEVPKEY_Device_InstanceId) else { continue };
            // L'impronta è del contenitore (il dispositivo fisico), uguale per tutti i suoi nodi; senza
            // contenitore (assente o tutto zero) si ripiega sull'id del nodo.
            let mut g = [0u8; 16];
            let contenitore = proprieta(set, &dati, &DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID, &mut g)
                .filter(|&n| n == 16 && g.iter().any(|&x| x != 0));
            let chiave = match contenitore {
                Some(_) => format!("contenitore:{g:02x?}"),
                None => id,
            };
            let nome = stringa(set, &dati, &DEVPKEY_Device_FriendlyName).filter(|n| !n.trim().is_empty());
            let (mut stato, mut problema) = (0u32, 0u32);
            let avviato = CM_Get_DevNode_Status(&mut stato, &mut problema, dati.DevInst, 0) == CR_SUCCESS
                && stato & DN_STARTED != 0;
            let mut c = [0u8; 4];
            let dichiarato = proprieta(set, &dati, &CONNESSO, DEVPROP_TYPE_BOOLEAN, &mut c).is_some_and(|n| n == 1 && c[0] != 0);
            letture.push(LetturaBt {
                impronta: impronta(&chiave),
                nome: nome.unwrap_or_else(|| "Dispositivo Bluetooth".into()),
                percentuale: b[0],
                connesso: avviato && dichiarato,
            });
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    unifica(letture)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lettura(imp: &str, nome: &str, percentuale: u8, connesso: bool) -> LetturaBt {
        LetturaBt { impronta: imp.into(), nome: nome.into(), percentuale, connesso }
    }

    #[test]
    fn impronta_stabile_e_senza_mac() {
        let id = "BTHENUM\\{0000110B-0000-1000-8000-00805F9B34FB}_VID&0001_PID&0002\\7&AA:BB:CC:DD:EE:FF";
        let a = impronta(id);
        assert_eq!(a, impronta(id), "uguale a ogni chiamata");
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "16 cifre esadecimali minuscole");
        assert!(!a.contains("AA"), "niente indirizzo in chiaro");
        assert_ne!(a, impronta("BTHENUM\\altro"), "id diversi, impronte diverse");
        // Valore noto di FNV-1a a 64 bit per la stringa vuota e per «a».
        assert_eq!(impronta(""), "cbf29ce484222325");
        assert_eq!(impronta("a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn avvisa_sotto_venti() {
        let niente = HashMap::new();
        assert_eq!(da_avvisare(&[lettura("a", "Cuffie finte", 18, true)], &niente, "2026-11-15").len(), 1);
        assert_eq!(da_avvisare(&[lettura("a", "Cuffie finte", 20, true)], &niente, "2026-11-15").len(), 1, "20% compreso");
        assert!(da_avvisare(&[lettura("a", "Cuffie finte", 21, true)], &niente, "2026-11-15").is_empty());
    }

    #[test]
    fn dispositivo_non_connesso_non_avvisa() {
        let niente = HashMap::new();
        assert!(da_avvisare(&[lettura("a", "Cuffie finte", 5, false)], &niente, "2026-11-15").is_empty());
    }

    #[test]
    fn una_volta_al_giorno_anche_dopo_il_riavvio() {
        let l = [lettura("a", "Cuffie finte", 10, true)];
        let avvisati: HashMap<String, String> = [("a".to_string(), "2026-11-15".to_string())].into();
        assert!(da_avvisare(&l, &avvisati, "2026-11-15").is_empty(), "già avvisato oggi");
        assert_eq!(da_avvisare(&l, &avvisati, "2026-11-16").len(), 1, "il giorno dopo di nuovo");
    }

    #[test]
    fn voce_bt_informazione_con_ho_capito() {
        let v = voce_bt(&lettura("0123456789abcdef", "Cuffie finte", 18, true), Instant::now());
        assert_eq!(v.livello, Livello::Informazione);
        assert_eq!(v.fonte, Fonte::Pc);
        assert_eq!(v.id, "pc.bt.0123456789abcdef");
        assert_eq!(v.titolo, "Cuffie finte al 18%");
        assert_eq!(v.dettaglio, "Ricarica il dispositivo Bluetooth quando puoi.");
        assert_eq!(v.pulsanti, vec![Pulsante::new(PULSANTE_HO_CAPITO, "Ho capito")]);
    }

    #[test]
    fn la_voce_sparisce_sopra_venticinque_o_senza_connessione() {
        let presenti = vec![id_voce("a"), id_voce("b"), id_voce("c"), id_voce("d")];
        let letture = [
            lettura("a", "Uno finto", 23, true),  // dentro l'isteresi: resta
            lettura("b", "Due finto", 26, true),  // sopra il 25%: via
            lettura("c", "Tre finto", 10, false), // disconnesso: via
        ]; // «d» non c'è più: via
        assert_eq!(da_togliere(&letture, &presenti), vec![id_voce("b"), id_voce("c"), id_voce("d")]);
    }

    #[test]
    fn avvisati_vecchi_di_sette_giorni_si_tolgono() {
        let mut m: HashMap<String, String> = [
            ("oggi", "2026-11-15"),
            ("sette", "2026-11-08"),
            ("otto", "2026-11-07"),
            ("rotta", "boh"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        pota_avvisati(&mut m, "2026-11-15");
        let mut chiavi: Vec<_> = m.keys().cloned().collect();
        chiavi.sort();
        assert_eq!(chiavi, ["oggi", "sette"]);
    }

    #[test]
    fn nodi_dello_stesso_dispositivo_diventano_uno() {
        // Stessa impronta (stesso contenitore): una lettura, connessa, con la percentuale minima
        // fra i nodi connessi (il non connesso, più basso, non conta).
        let u = unifica(vec![
            lettura("a", "Cuffie finte", 30, false),
            lettura("a", "Cuffie finte", 42, true),
            lettura("a", "Cuffie finte", 40, true),
            lettura("c", "Tastiera finta", 90, true),
        ]);
        assert_eq!(u.len(), 2);
        assert_eq!((u[0].impronta.as_str(), u[0].percentuale, u[0].connesso), ("a", 40, true));
    }

    #[test]
    fn due_dispositivi_con_lo_stesso_nome_restano_due() {
        let u = unifica(vec![lettura("a", "Cuffie finte", 50, true), lettura("b", "Cuffie finte", 15, true)]);
        assert_eq!(u.len(), 2, "contenitori diversi: due dispositivi");
        assert_eq!(riga_cuffie(&u).unwrap().altri, 1);
    }

    /// Lo scambio fra nodi dello stesso dispositivo (HFP a passi di 10%, GATT al punto) non cambia
    /// l'impronta: la voce resta e nello stesso giorno non nasce un secondo avviso.
    #[test]
    fn scambio_fra_nodi_nessun_secondo_avviso() {
        let oggi = "2026-11-15";
        let prima = unifica(vec![lettura("a", "Cuffie finte", 20, true), lettura("a", "Cuffie finte", 18, true)]);
        assert_eq!(prima[0].percentuale, 18);
        let da_avvisare_prima = da_avvisare(&prima, &HashMap::new(), oggi);
        assert_eq!(da_avvisare_prima.len(), 1);
        // L'avviso dato è registrato; il giro dopo l'altro nodo è il più basso.
        let avvisati: HashMap<String, String> = [(da_avvisare_prima[0].impronta.clone(), oggi.to_string())].into();
        let dopo = unifica(vec![lettura("a", "Cuffie finte", 10, true), lettura("a", "Cuffie finte", 17, true)]);
        assert_eq!(dopo[0].impronta, prima[0].impronta, "stessa impronta");
        assert!(da_avvisare(&dopo, &avvisati, oggi).is_empty(), "nessun secondo avviso");
        assert!(da_togliere(&dopo, &[id_voce(&prima[0].impronta)]).is_empty(), "la voce non si toglie");
    }

    #[test]
    fn riga_cuffie_il_piu_basso_con_gli_altri() {
        let l = [
            lettura("a", "Tastiera finta", 80, true),
            lettura("b", "Cuffie finte con un nome molto lungo", 18, true),
            lettura("c", "Mouse finto", 5, false), // non connesso: fuori dalla riga
            lettura("d", "Mouse finto due", 50, true),
        ];
        let r = riga_cuffie(&l).unwrap();
        assert_eq!(r.percentuale, 18);
        assert_eq!(r.altri, 2);
        assert_eq!(r.tinta, TintaBatteria::Informazione);
        assert!(r.nome.chars().count() <= NOME_MASSIMO && r.nome.ends_with('…'), "nome accorciato: {}", r.nome);
        assert_eq!(
            r.descrizione,
            "Cuffie finte con un nome molto lungo al 18%, Mouse finto due al 50%, Tastiera finta al 80%"
        );
        let sola = riga_cuffie(&[lettura("a", "Cuffie finte", 60, true)]).unwrap();
        assert_eq!((sola.altri, sola.tinta), (0, TintaBatteria::Normale));
        assert_eq!(sola.nome, "Cuffie finte", "nome corto intero");
    }

    #[test]
    fn senza_connessi_la_riga_non_c_e() {
        assert!(riga_cuffie(&[]).is_none());
        assert!(riga_cuffie(&[lettura("a", "Cuffie finte", 50, false)]).is_none());
    }

    /// Il thread legge dopo la prima attesa, poi a ogni periodo, e finisce quando il gestore
    /// cade: nessuna lettura dopo.
    #[test]
    fn il_thread_legge_e_si_ferma_col_gestore() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let letture = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel::<usize>();
        let l = letture.clone();
        let s = SorveglianzaBt::avvia_con(
            Duration::from_millis(10),
            Duration::from_millis(10),
            move || {
                l.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            },
            move |_| tx.send(1).is_ok(),
        );
        rx.recv_timeout(Duration::from_secs(5)).expect("prima lettura");
        rx.recv_timeout(Duration::from_secs(5)).expect("seconda lettura");
        drop(s);
        std::thread::sleep(Duration::from_millis(100));
        let dopo = letture.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(letture.load(Ordering::SeqCst), dopo, "fermo dopo lo spegnimento");
    }

    /// Solo leggere: nessun nome né identificativo stampato.
    #[cfg(windows)]
    #[test]
    #[ignore = "legge i dispositivi veri del PC"]
    fn legge_senza_errori() {
        let n = leggi().len();
        // Il numero basta a dire che la lettura è finita.
        assert!(n < 1000);
        eprintln!("dispositivi Bluetooth con batteria: {n}");
    }
}
