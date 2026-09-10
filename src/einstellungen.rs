//! Einstellungen: persistente App-Konfiguration mit Dirty-Tracking.
//!
//! Performance-Design: Die Struktur wird im Speicher gehalten; nur tatsächliche
//! Änderungen (`aendern_*`-Methoden) markieren sie als dirty und triggern beim
//! nächsten `speichern_wenn_noetig()`-Aufruf (einmal pro Frame geprüft) einen
//! einzigen Serialisierungsvorgang. Kein periodisches Schreiben, keine I/O im
//! UI-Pfad.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Einstellungen {
    pub last_vault: Option<String>,
    pub sprache: String, // "de" | "en"
    pub autosave_ms: u64,
    pub vorschau_sichtbar: bool,
    pub glossar_aktiv: bool,
    pub glossar_case_insensitive: bool,
    pub glossar_max_treffer: usize,
}

impl Default for Einstellungen {
    fn default() -> Self {
        Einstellungen {
            last_vault: None,
            sprache: "de".into(),
            autosave_ms: 800,
            vorschau_sichtbar: true,
            glossar_aktiv: true,
            glossar_case_insensitive: true,
            glossar_max_treffer: 500,
        }
    }
}

/// Geladene Einstellungen + Dirty-Flag für performantes Speichern.
#[derive(Debug, Default)]
pub struct EinstellungsManager {
    pub werte: Einstellungen,
    dirty: bool,
}

impl EinstellungsManager {
    /// Laedt aus der Standard-Datei (nur fuer die echte App verwenden).
    pub fn laden() -> EinstellungsManager {
        match datei_pfad() {
            Some(p) => EinstellungsManager::laden_aus(&p),
            None => EinstellungsManager::default(),
        }
    }

    /// Laedt aus einem beliebigen Pfad (Tests: Temp-Datei, keine echten Daten!).
    pub fn laden_aus(pfad: &std::path::Path) -> EinstellungsManager {
        let werte = std::fs::read_to_string(pfad)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        EinstellungsManager {
            werte,
            dirty: false,
        }
    }

    pub fn ist_dirty(&self) -> bool {
        self.dirty
    }

    pub fn markiere_dirty(&mut self) {
        self.dirty = true;
    }

    /// Schreibt die Standard-Datei nur, wenn sich etwas geändert hat.
    pub fn speichern_wenn_noetig(&mut self) -> bool {
        match datei_pfad() {
            Some(p) => self.speichern_wenn_noetig_nach(&p),
            None => false,
        }
    }

    /// Variante mit explizitem Pfad (Tests: Temp-Datei).
    pub fn speichern_wenn_noetig_nach(&mut self, pfad: &std::path::Path) -> bool {
        if !self.dirty {
            return false;
        }
        self.dirty = false;
        if let Some(parent) = pfad.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(&self.werte) {
            Ok(json) => {
                let _ = std::fs::write(pfad, json);
                true
            }
            Err(_) => false,
        }
    }

    /// Setzt last_vault nur bei tatsächlicher Änderung (kein Dirty-Spam).
    pub fn setze_last_vault(&mut self, pfad: Option<String>) {
        if self.werte.last_vault != pfad {
            self.werte.last_vault = pfad;
            self.dirty = true;
        }
    }
}

pub fn datei_pfad() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rusty-notes").join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_pfad(tag: &str) -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rusty-einst-{}-{}-{}", tag, nanos, n));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.json")
    }

    #[test]
    fn standardwerte_sind_deutsch_mit_glossar() {
        let e = Einstellungen::default();
        assert_eq!(e.sprache, "de");
        assert!(e.glossar_aktiv);
        assert!(e.glossar_case_insensitive);
        assert_eq!(e.autosave_ms, 800);
    }

    #[test]
    fn ohne_aenderung_kein_speichern() {
        let pfad = tmp_pfad("clean");
        let mut m = EinstellungsManager::laden_aus(&pfad);
        assert!(!m.ist_dirty());
        assert!(
            !m.speichern_wenn_noetig_nach(&pfad),
            "unveränderte Einstellungen dürfen nicht schreiben"
        );
        assert!(!pfad.exists(), "es darf keine Datei angelegt werden");
    }

    #[test]
    fn aenderung_markiert_dirty_und_schreibt_tempdatei() {
        let pfad = tmp_pfad("dirty");
        let mut m = EinstellungsManager::laden_aus(&pfad);
        m.markiere_dirty();
        assert!(m.ist_dirty());
        assert!(m.speichern_wenn_noetig_nach(&pfad), "dirty → muss schreiben");
        assert!(!m.ist_dirty());
        assert!(pfad.exists());
    }

    #[test]
    fn laden_aus_rundtrip() {
        let pfad = tmp_pfad("roundtrip");
        let mut m = EinstellungsManager::laden_aus(&pfad);
        m.werte.glossar_max_treffer = 77;
        m.markiere_dirty();
        assert!(m.speichern_wenn_noetig_nach(&pfad));

        let m2 = EinstellungsManager::laden_aus(&pfad);
        assert_eq!(m2.werte.glossar_max_treffer, 77);
    }

    #[test]
    fn last_vault_nur_bei_aenderung_dirty() {
        let pfad = tmp_pfad("vault");
        let mut m = EinstellungsManager::laden_aus(&pfad);
        m.setze_last_vault(None);
        assert!(!m.ist_dirty(), "gleicher Wert (None) → kein Dirty");

        m.setze_last_vault(Some("/x".into()));
        assert!(m.ist_dirty(), "neuer Wert → Dirty");

        let _ = m.speichern_wenn_noetig_nach(&pfad);
        m.setze_last_vault(Some("/x".into()));
        assert!(!m.ist_dirty(), "gleicher Wert erneut → kein Dirty");
    }

    #[test]
    fn roundtrip_serialisierung() {
        let e = Einstellungen {
            glossar_max_treffer: 42,
            ..Einstellungen::default()
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Einstellungen = serde_json::from_str(&json).unwrap();
        assert_eq!(back.glossar_max_treffer, 42);
    }
}
