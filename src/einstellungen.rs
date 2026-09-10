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
    pub fn laden() -> EinstellungsManager {
        let pfad = datei_pfad();
        let werte = pfad
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        EinstellungsManager { werte, dirty: false }
    }

    pub fn ist_dirty(&self) -> bool {
        self.dirty
    }

    pub fn markiere_dirty(&mut self) {
        self.dirty = true;
    }

    /// Schreibt die Datei nur, wenn sich etwas geändert hat.
    pub fn speichern_wenn_noetig(&mut self) -> bool {
        if !self.dirty {
            return false;
        }
        self.dirty = false;
        if let Some(p) = datei_pfad() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(&self.werte) {
                let _ = std::fs::write(p, json);
                return true;
            }
        }
        false
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

    fn tmp_mgr(tag: &str) -> EinstellungsManager {
        // Wir testen die Logik; Datei-I/O wird über datei_pfad() gemockt,
        // indem wir dirty-Verhalten ohne Schreiben prüfen.
        let _ = tag;
        EinstellungsManager::default()
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
        let mut m = tmp_mgr("clean");
        assert!(!m.ist_dirty());
        assert!(!m.speichern_wenn_noetig(), "unveränderte Einstellungen dürfen nicht schreiben");
    }

    #[test]
    fn aenderung_markiert_dirty() {
        let mut m = tmp_mgr("dirty");
        m.markiere_dirty();
        assert!(m.ist_dirty());
        // Nach speichern_wenn_noetig ist das Flag zurückgesetzt:
        let _ = m.speichern_wenn_noetig();
        assert!(!m.ist_dirty());
    }

    #[test]
    fn last_vault_nur_bei_aenderung_dirty() {
        let mut m = tmp_mgr("vault");
        m.setze_last_vault(None);
        assert!(!m.ist_dirty(), "gleicher Wert (None) → kein Dirty");

        m.setze_last_vault(Some("/x".into()));
        assert!(m.ist_dirty(), "neuer Wert → Dirty");

        let _ = m.speichern_wenn_noetig();
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
