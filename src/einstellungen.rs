//! Einstellungen: persistente App-Konfiguration mit Dirty-Tracking.
//!
//! Performance-Design: Die Struktur wird im Speicher gehalten; nur tatsächliche
//! Änderungen (`markiere_dirty`, `setze_*`) triggern beim nächsten
//! `speichern_wenn_noetig()` einen einzigen Serialisierungsvorgang. Kein
//! periodisches Schreiben, kein I/O im UI-Pfad. Keybinds werden als
//! `Vec<(Aktion, Tastencode)>` gehalten; die UI baut daraus einmal pro Frame
//! eine Lookup-Map (Hash, O(1) pro Tastendruck).

use std::path::PathBuf;

/// Alle konfigurierbaren Aktionen (Keybind-Ziele).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Aktion {
    OrdnerOeffnen,
    Schnellwechsler,
    Befehlspalette,
    Speichern,
    NeueNotiz,
    VorschauUmschalten,
    SucheFokussieren,
    Einstellungen,
    NotizSchliessen,
    NaechsteNotiz,
    VorherigeNotiz,
    GlossarUmschalten,
}

impl Aktion {
    pub const ALLE: &'static [(Aktion, &'static str)] = &[
        (Aktion::OrdnerOeffnen, "Ordner öffnen"),
        (Aktion::Schnellwechsler, "Schnellwechsler"),
        (Aktion::Befehlspalette, "Befehlspalette"),
        (Aktion::Speichern, "Speichern"),
        (Aktion::NeueNotiz, "Neue Notiz"),
        (Aktion::VorschauUmschalten, "Vorschau umschalten"),
        (Aktion::SucheFokussieren, "Suche fokussieren"),
        (Aktion::Einstellungen, "Einstellungen"),
        (Aktion::NotizSchliessen, "Notiz schließen"),
        (Aktion::NaechsteNotiz, "Nächste Notiz"),
        (Aktion::VorherigeNotiz, "Vorherige Notiz"),
        (Aktion::GlossarUmschalten, "Glossar umschalten"),
    ];

    pub fn name(self) -> &'static str {
        Aktion::ALLE
            .iter()
            .find(|(a, _)| *a == self)
            .map(|(_, n)| *n)
            .unwrap_or("?")
    }

    /// Stabiler Serialisierungsschlüssel.
    pub fn schluessel(self) -> &'static str {
        match self {
            Aktion::OrdnerOeffnen => "ordner_oeffnen",
            Aktion::Schnellwechsler => "schnellwechsler",
            Aktion::Befehlspalette => "befehlspalette",
            Aktion::Speichern => "speichern",
            Aktion::NeueNotiz => "neue_notiz",
            Aktion::VorschauUmschalten => "vorschau_umschalten",
            Aktion::SucheFokussieren => "suche_fokussieren",
            Aktion::Einstellungen => "einstellungen",
            Aktion::NotizSchliessen => "notiz_schliessen",
            Aktion::NaechsteNotiz => "naechste_notiz",
            Aktion::VorherigeNotiz => "vorherige_notiz",
            Aktion::GlossarUmschalten => "glossar_umschalten",
        }
    }

    pub fn aus_schluessel(s: &str) -> Option<Aktion> {
        Some(match s {
            "ordner_oeffnen" => Aktion::OrdnerOeffnen,
            "schnellwechsler" => Aktion::Schnellwechsler,
            "befehlspalette" => Aktion::Befehlspalette,
            "speichern" => Aktion::Speichern,
            "neue_notiz" => Aktion::NeueNotiz,
            "vorschau_umschalten" => Aktion::VorschauUmschalten,
            "suche_fokussieren" => Aktion::SucheFokussieren,
            "einstellungen" => Aktion::Einstellungen,
            "notiz_schliessen" => Aktion::NotizSchliessen,
            "naechste_notiz" => Aktion::NaechsteNotiz,
            "vorherige_notiz" => Aktion::VorherigeNotiz,
            "glossar_umschalten" => Aktion::GlossarUmschalten,
            _ => return None,
        })
    }
}

/// Eine Tastenkombination, serialisierbar als "Ctrl+Shift+S" o.ä.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Keybind {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// egui-Key-Name, z.B. "S", "P", "F3", "ArrowDown".
    pub taste: String,
}

impl Keybind {
    pub fn neu(ctrl: bool, shift: bool, alt: bool, taste: &str) -> Keybind {
        Keybind {
            ctrl,
            shift,
            alt,
            taste: taste.to_string(),
        }
    }

    pub fn als_text(&self) -> String {
        let mut teile: Vec<String> = Vec::new();
        if self.ctrl {
            teile.push("Strg".to_string());
        }
        if self.shift {
            teile.push("Umschalt".to_string());
        }
        if self.alt {
            teile.push("Alt".to_string());
        }
        teile.push(self.taste.clone());
        teile.join("+")
    }

    pub fn serialisiere(&self) -> String {
        let mut teile: Vec<String> = Vec::new();
        if self.ctrl {
            teile.push("Ctrl".to_string());
        }
        if self.shift {
            teile.push("Shift".to_string());
        }
        if self.alt {
            teile.push("Alt".to_string());
        }
        teile.push(self.taste.clone());
        teile.join("+")
    }

    pub fn deserialisiere(s: &str) -> Option<Keybind> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut taste = String::new();
        for teil in s.split('+') {
            match teil {
                "Ctrl" => ctrl = true,
                "Shift" => shift = true,
                "Alt" => alt = true,
                t => taste = t.to_string(),
            }
        }
        if taste.is_empty() {
            return None;
        }
        Some(Keybind { ctrl, shift, alt, taste })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Einstellungen {
    pub last_vault: Option<String>,
    pub autosave_ms: u64,
    pub vorschau_sichtbar: bool,

    // Glossar
    pub glossar_aktiv: bool,
    pub glossar_case_insensitive: bool,
    pub glossar_max_treffer: usize,
    /// Unterordner, in dem Glossar-Notizen gepflegt werden (relativ zum Vault).
    /// Leer = gesamter Vault. Mehrere Ordner möglich.
    pub glossar_ordner: Vec<String>,
    /// Glossar-Begriffe auch in der Vorschau als klickbare Liste anzeigen.
    pub glossar_vorschau_liste: bool,
    /// Mindestlänge eines Glossar-Begriffs (kürzere werden nie verlinkt).
    pub glossar_min_laenge: usize,

    // Editor
    pub editor_schriftgroesse: f32,
    pub editor_zeilennummern: bool,
    pub editor_zeilenabstand: f32,

    // Keybinds: (Aktionsschlüssel, Bind)
    pub keybinds: Vec<(String, String)>,
}

impl Default for Einstellungen {
    fn default() -> Self {
        Einstellungen {
            last_vault: None,
            autosave_ms: 800,
            vorschau_sichtbar: true,

            glossar_aktiv: true,
            glossar_case_insensitive: true,
            glossar_max_treffer: 500,
            glossar_ordner: vec!["Glossar".to_string()],
            glossar_vorschau_liste: true,
            glossar_min_laenge: 3,

            editor_schriftgroesse: 14.0,
            editor_zeilennummern: false,
            editor_zeilenabstand: 1.0,

            keybinds: standard_keybinds(),
        }
    }
}

/// Standard-Keybinds (deutsche Belegung, Strg statt Ctrl in der Anzeige).
pub fn standard_keybinds() -> Vec<(String, String)> {
    vec![
        (Aktion::OrdnerOeffnen.schluessel().into(), Keybind::neu(true, false, false, "O").serialisiere()),
        (Aktion::Schnellwechsler.schluessel().into(), Keybind::neu(true, false, false, "P").serialisiere()),
        (Aktion::Befehlspalette.schluessel().into(), Keybind::neu(true, false, false, "K").serialisiere()),
        (Aktion::Speichern.schluessel().into(), Keybind::neu(true, false, false, "S").serialisiere()),
        (Aktion::NeueNotiz.schluessel().into(), Keybind::neu(true, false, false, "N").serialisiere()),
        (Aktion::VorschauUmschalten.schluessel().into(), Keybind::neu(true, false, false, "E").serialisiere()),
        (Aktion::SucheFokussieren.schluessel().into(), Keybind::neu(true, true, false, "F").serialisiere()),
        (Aktion::Einstellungen.schluessel().into(), Keybind::neu(true, true, false, "S").serialisiere()),
        (Aktion::NotizSchliessen.schluessel().into(), Keybind::neu(true, false, false, "W").serialisiere()),
        (Aktion::NaechsteNotiz.schluessel().into(), Keybind::neu(true, false, false, "ArrowDown").serialisiere()),
        (Aktion::VorherigeNotiz.schluessel().into(), Keybind::neu(true, false, false, "ArrowUp").serialisiere()),
        (Aktion::GlossarUmschalten.schluessel().into(), Keybind::neu(true, true, false, "G").serialisiere()),
    ]
}

impl Einstellungen {
    pub fn bind_fuer(&self, aktion: Aktion) -> Option<Keybind> {
        let s = aktion.schluessel();
        self.keybinds
            .iter()
            .find(|(a, _)| a == s)
            .and_then(|(_, b)| Keybind::deserialisiere(b))
    }

    pub fn setze_bind(&mut self, aktion: Aktion, bind: Option<Keybind>) {
        let s = aktion.schluessel();
        match bind {
            Some(b) => {
                let serialisiert = b.serialisiere();
                if let Some(eintrag) = self.keybinds.iter_mut().find(|(a, _)| a == s) {
                    if eintrag.1 != serialisiert {
                        eintrag.1 = serialisiert;
                    }
                } else {
                    self.keybinds.push((s.to_string(), serialisiert));
                }
            }
            None => {
                self.keybinds.retain(|(a, _)| a != s);
            }
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
        EinstellungsManager { werte, dirty: false }
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
    fn standardwerte_sind_sinnvoll() {
        let e = Einstellungen::default();
        assert!(e.glossar_aktiv);
        assert_eq!(e.glossar_ordner, vec!["Glossar".to_string()]);
        assert_eq!(e.glossar_min_laenge, 3);
        assert_eq!(e.editor_schriftgroesse, 14.0);
        assert!(e.keybinds.len() >= Aktion::ALLE.len());
    }

    #[test]
    fn keybind_rundtrip() {
        let b = Keybind::neu(true, true, false, "S");
        assert_eq!(b.als_text(), "Strg+Umschalt+S");
        assert_eq!(b.serialisiere(), "Ctrl+Shift+S");
        let back = Keybind::deserialisiere("Ctrl+Shift+S").unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn bind_setzen_und_lesen() {
        let mut e = Einstellungen::default();
        let alt = e.bind_fuer(Aktion::Speichern).unwrap();
        assert_eq!(alt.taste, "S");

        e.setze_bind(Aktion::Speichern, Some(Keybind::neu(true, false, true, "F2")));
        let neu = e.bind_fuer(Aktion::Speichern).unwrap();
        assert_eq!(neu.taste, "F2");
        assert!(neu.alt && neu.ctrl && !neu.shift);

        e.setze_bind(Aktion::GlossarUmschalten, None);
        assert!(e.bind_fuer(Aktion::GlossarUmschalten).is_none());
    }

    #[test]
    fn aktion_schluessel_rundtrip() {
        for (a, name) in Aktion::ALLE {
            assert_eq!(Aktion::aus_schluessel(a.schluessel()), Some(*a), "{}", name);
            assert!(!a.name().is_empty());
        }
        assert!(Aktion::aus_schluessel("gibtsnicht").is_none());
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
    fn laden_aus_rundtrip_mit_neuen_feldern() {
        let pfad = tmp_pfad("roundtrip");
        let mut m = EinstellungsManager::laden_aus(&pfad);
        m.werte.glossar_max_treffer = 77;
        m.werte.glossar_ordner = vec!["Wissen/Glossar".into(), "Begriffe".into()];
        m.werte.editor_schriftgroesse = 17.5;
        m.werte.setze_bind(Aktion::Speichern, Some(Keybind::neu(false, false, false, "F2")));
        m.markiere_dirty();
        assert!(m.speichern_wenn_noetig_nach(&pfad));

        let m2 = EinstellungsManager::laden_aus(&pfad);
        assert_eq!(m2.werte.glossar_max_treffer, 77);
        assert_eq!(m2.werte.glossar_ordner.len(), 2);
        assert_eq!(m2.werte.editor_schriftgroesse, 17.5);
        assert_eq!(m2.werte.bind_fuer(Aktion::Speichern).unwrap().taste, "F2");
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
}
