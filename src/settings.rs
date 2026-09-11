//! Settings: persistente App-Konfiguration mit Dirty-Tracking.
//!
//! Performance-Design: Die Struktur wird im Speicher gehalten; nur tatsächliche
//! Änderungen (`mark_dirty`, `setze_*`) triggern beim nächsten
//! `save_if_needed()` einen einzigen Serialisierungsvorgang. Kein
//! periodisches Schreiben, kein I/O im UI-Pfad. Keybinds werden als
//! `Vec<(Action, Tastencode)>` gehalten; die UI baut daraus einmal pro Frame
//! eine Lookup-Map (Hash, O(1) pro Tastendruck).

use std::path::PathBuf;

use crate::i18n::{Language, Texts};

/// Alle konfigurierbaren Aktionen (Keybind-Ziele).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    OpenFolder,
    QuickSwitcher,
    CommandPalette,
    Save,
    NewNote,
    TogglePreview,
    FocusSearch,
    OpenSettings,
    CloseNote,
    NextNote,
    PrevNote,
    ToggleGlossary,
    ToggleSyncScroll,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::OpenFolder,
        Action::QuickSwitcher,
        Action::CommandPalette,
        Action::Save,
        Action::NewNote,
        Action::TogglePreview,
        Action::FocusSearch,
        Action::OpenSettings,
        Action::CloseNote,
        Action::NextNote,
        Action::PrevNote,
        Action::ToggleGlossary,
        Action::ToggleSyncScroll,
    ];

    /// Display name in the current UI language.
    pub fn label(self, txt: &Texts) -> &'static str {
        match self {
            Action::OpenFolder => txt.act_open_folder,
            Action::QuickSwitcher => txt.act_quick_switcher,
            Action::CommandPalette => txt.act_command_palette,
            Action::Save => txt.act_save,
            Action::NewNote => txt.act_new_note,
            Action::TogglePreview => txt.act_toggle_preview,
            Action::FocusSearch => txt.act_focus_search,
            Action::OpenSettings => txt.act_open_settings,
            Action::CloseNote => txt.act_close_note,
            Action::NextNote => txt.act_next_note,
            Action::PrevNote => txt.act_prev_note,
            Action::ToggleGlossary => txt.act_toggle_glossary,
            Action::ToggleSyncScroll => txt.act_toggle_sync_scroll,
        }
    }

    /// Stable serialization key (English). `from_key` also accepts the
    /// legacy German keys so old configs keep working.
    pub fn key(self) -> &'static str {
        match self {
            Action::OpenFolder => "open_folder",
            Action::QuickSwitcher => "quick_switcher",
            Action::CommandPalette => "command_palette",
            Action::Save => "save",
            Action::NewNote => "new_note",
            Action::TogglePreview => "toggle_preview",
            Action::FocusSearch => "focus_search",
            Action::OpenSettings => "open_settings",
            Action::CloseNote => "close_note",
            Action::NextNote => "next_note",
            Action::PrevNote => "prev_note",
            Action::ToggleGlossary => "toggle_glossary",
            Action::ToggleSyncScroll => "toggle_sync_scroll",
        }
    }

    /// Legacy German key (pre-English configs). Used as a lookup fallback
    /// so old `config.json` files keep their custom keybinds.
    pub fn legacy_key(self) -> &'static str {
        match self {
            Action::OpenFolder => "ordner_oeffnen",
            Action::QuickSwitcher => "schnellwechsler",
            Action::CommandPalette => "befehlspalette",
            Action::Save => "speichern",
            Action::NewNote => "neue_notiz",
            Action::TogglePreview => "vorschau_umschalten",
            Action::FocusSearch => "suche_fokussieren",
            Action::OpenSettings => "einstellungen",
            Action::CloseNote => "notiz_schliessen",
            Action::NextNote => "naechste_notiz",
            Action::PrevNote => "vorherige_notiz",
            Action::ToggleGlossary => "glossar_umschalten",
            Action::ToggleSyncScroll => "sync_scroll_umschalten",
        }
    }

    pub fn from_key(s: &str) -> Option<Action> {
        Some(match s {
            "open_folder" | "ordner_oeffnen" => Action::OpenFolder,
            "quick_switcher" | "schnellwechsler" => Action::QuickSwitcher,
            "command_palette" | "befehlspalette" => Action::CommandPalette,
            "save" | "speichern" => Action::Save,
            "new_note" | "neue_notiz" => Action::NewNote,
            "toggle_preview" | "vorschau_umschalten" => Action::TogglePreview,
            "focus_search" | "suche_fokussieren" => Action::FocusSearch,
            "open_settings" | "einstellungen" => Action::OpenSettings,
            "close_note" | "notiz_schliessen" => Action::CloseNote,
            "next_note" | "naechste_notiz" => Action::NextNote,
            "prev_note" | "vorherige_notiz" => Action::PrevNote,
            "toggle_glossary" | "glossar_umschalten" => Action::ToggleGlossary,
            "toggle_sync_scroll" | "sync_scroll_umschalten" => Action::ToggleSyncScroll,
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
    pub key: String,
}

impl Keybind {
    pub fn new(ctrl: bool, shift: bool, alt: bool, key: &str) -> Keybind {
        Keybind {
            ctrl,
            shift,
            alt,
            key: key.to_string(),
        }
    }

    /// Display form, e.g. "Ctrl+Shift+S" (EN) or "Strg+Umschalt+S" (DE).
    pub fn display_in(&self, lang: Language) -> String {
        let (ctrl, shift) = match lang {
            Language::En => ("Ctrl", "Shift"),
            Language::De => ("Strg", "Umschalt"),
        };
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push(ctrl.to_string());
        }
        if self.shift {
            parts.push(shift.to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        parts.push(self.key.clone());
        parts.join("+")
    }

    pub fn serialize(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        parts.push(self.key.clone());
        parts.join("+")
    }

    pub fn deserialize(s: &str) -> Option<Keybind> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key = String::new();
        for part in s.split('+') {
            match part {
                "Ctrl" => ctrl = true,
                "Shift" => shift = true,
                "Alt" => alt = true,
                t => key = t.to_string(),
            }
        }
        if key.is_empty() {
            return None;
        }
        Some(Keybind {
            ctrl,
            shift,
            alt,
            key,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub last_vault: Option<String>,
    pub autosave_ms: u64,
    pub preview_visible: bool,

    // Glossar
    pub glossary_enabled: bool,
    pub glossary_case_insensitive: bool,
    pub glossary_max_hits: usize,
    /// Unterordner, in dem Glossar-Notizen gepflegt werden (relativ zum Vault).
    /// Leer = gesamter Vault. Mehrere Ordner möglich.
    pub glossary_folders: Vec<String>,
    /// Glossar-Begriffe auch in der Vorschau als klickbare Liste anzeigen.
    pub glossary_preview_list: bool,
    /// Mindestlänge eines Glossar-Begriffs (kürzere werden nie verlinkt).
    pub glossary_min_len: usize,

    /// Synchronisiertes Scrollen zwischen Editor und Vorschau.
    pub sync_scroll: bool,

    // Editor
    pub editor_font_size: f32,
    pub editor_line_numbers: bool,
    pub editor_line_spacing: f32,

    // Vorschau
    /// Schriftgröße der gerenderten Markdown-Vorschau (Body). Wird auf egui's
    /// TextStyle::Body/Heading angewendet, damit egui_commonmark sie nutzt.
    pub preview_font_size: f32,

    /// UI language code: "en" (default) or "de".
    pub language: String,

    // Keybinds: (action key, bind)
    pub keybinds: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            last_vault: None,
            autosave_ms: 800,
            preview_visible: true,

            glossary_enabled: true,
            glossary_case_insensitive: true,
            glossary_max_hits: 500,
            glossary_folders: vec!["Glossar".to_string()],
            glossary_preview_list: true,
            glossary_min_len: 3,

            sync_scroll: true,
            editor_font_size: 14.0,
            editor_line_numbers: false,
            editor_line_spacing: 1.0,
            preview_font_size: 14.0,
            language: "en".to_string(),

            keybinds: default_keybinds(),
        }
    }
}

/// Standard-Keybinds (deutsche Belegung, Strg statt Ctrl in der Anzeige).
pub fn default_keybinds() -> Vec<(String, String)> {
    vec![
        (
            Action::OpenFolder.key().into(),
            Keybind::new(true, false, false, "O").serialize(),
        ),
        (
            Action::QuickSwitcher.key().into(),
            Keybind::new(true, false, false, "P").serialize(),
        ),
        (
            Action::CommandPalette.key().into(),
            Keybind::new(true, false, false, "K").serialize(),
        ),
        (
            Action::Save.key().into(),
            Keybind::new(true, false, false, "S").serialize(),
        ),
        (
            Action::NewNote.key().into(),
            Keybind::new(true, false, false, "N").serialize(),
        ),
        (
            Action::TogglePreview.key().into(),
            Keybind::new(true, false, false, "E").serialize(),
        ),
        (
            Action::FocusSearch.key().into(),
            Keybind::new(true, true, false, "F").serialize(),
        ),
        (
            Action::OpenSettings.key().into(),
            Keybind::new(true, true, false, "S").serialize(),
        ),
        (
            Action::CloseNote.key().into(),
            Keybind::new(true, false, false, "W").serialize(),
        ),
        (
            Action::NextNote.key().into(),
            Keybind::new(true, false, false, "ArrowDown").serialize(),
        ),
        (
            Action::PrevNote.key().into(),
            Keybind::new(true, false, false, "ArrowUp").serialize(),
        ),
        (
            Action::ToggleGlossary.key().into(),
            Keybind::new(true, true, false, "G").serialize(),
        ),
        (
            Action::ToggleSyncScroll.key().into(),
            Keybind::new(true, true, false, "Y").serialize(),
        ),
    ]
}

impl Settings {
    pub fn binding_for(&self, action: Action) -> Option<Keybind> {
        self.keybinds
            .iter()
            .find(|(a, _)| a == action.key() || a == action.legacy_key())
            .and_then(|(_, b)| Keybind::deserialize(b))
    }

    pub fn set_binding(&mut self, action: Action, bind: Option<Keybind>) {
        let s = action.key();
        match bind {
            Some(b) => {
                let serialized = b.serialize();
                if let Some(entry) = self.keybinds.iter_mut().find(|(a, _)| a == s) {
                    if entry.1 != serialized {
                        entry.1 = serialized;
                    }
                } else {
                    self.keybinds.push((s.to_string(), serialized));
                }
            }
            None => {
                self.keybinds.retain(|(a, _)| a != s);
            }
        }
    }
}

/// Geladene Settings + Dirty-Flag für performantes Save.
#[derive(Debug, Default)]
pub struct SettingsManager {
    pub values: Settings,
    dirty: bool,
}

impl SettingsManager {
    /// Laedt aus der Standard-Datei (nur fuer die echte App verwenden).
    pub fn load() -> SettingsManager {
        match config_path() {
            Some(p) => SettingsManager::load_from(&p),
            None => SettingsManager::default(),
        }
    }

    /// Laedt aus einem beliebigen Pfad (Tests: Temp-Datei, keine echten Daten!).
    /// Fehlende Keybinds (z.B. alte Configs) werden mit Standards aufgefüllt.
    pub fn load_from(path: &std::path::Path) -> SettingsManager {
        // Fehlende Felder sollen die Standardwerte erhalten (serde::default),
        // daher: Defaults load und vorhandene Datei-Werte drüberlegen.
        let file: Option<serde_json::Value> = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let mut values = Settings::default();
        if let Some(serde_json::Value::Object(map)) = file {
            // Field-by-field override (keybinds only when non-empty).
            // Each field accepts its legacy German key so old configs survive.
            let bool_for =
                |en: &str, de: &str| map.get(en).or(map.get(de)).and_then(|v| v.as_bool());
            let u64_for = |en: &str, de: &str| map.get(en).or(map.get(de)).and_then(|v| v.as_u64());
            let f64_for = |en: &str, de: &str| map.get(en).or(map.get(de)).and_then(|v| v.as_f64());
            if let Some(x) = map.get("autosave_ms").and_then(|v| v.as_u64()) {
                values.autosave_ms = x;
            }
            if let Some(x) = bool_for("preview_visible", "vorschau_sichtbar") {
                values.preview_visible = x;
            }
            if let Some(x) = bool_for("glossary_enabled", "glossar_aktiv") {
                values.glossary_enabled = x;
            }
            if let Some(x) = bool_for("glossary_case_insensitive", "glossar_case_insensitive") {
                values.glossary_case_insensitive = x;
            }
            if let Some(x) = u64_for("glossary_max_hits", "glossar_max_treffer") {
                values.glossary_max_hits = x as usize;
            }
            if let Some(x) = u64_for("glossary_min_len", "glossar_min_laenge") {
                values.glossary_min_len = x as usize;
            }
            if let Some(x) = bool_for("glossary_preview_list", "glossar_vorschau_liste") {
                values.glossary_preview_list = x;
            }
            if let Some(x) = f64_for("editor_font_size", "editor_schriftgroesse") {
                values.editor_font_size = x as f32;
            }
            if let Some(x) = map.get("editor_line_numbers").and_then(|v| v.as_bool()) {
                values.editor_line_numbers = x;
            }
            if let Some(x) = map.get("editor_line_spacing").and_then(|v| v.as_f64()) {
                values.editor_line_spacing = x as f32;
            }
            if let Some(x) = f64_for("preview_font_size", "vorschau_schriftgroesse") {
                values.preview_font_size = x as f32;
            }
            if let Some(x) = map
                .get("sync_scroll")
                .or(map.get("sync_scroll_aktiv"))
                .and_then(|v| v.as_bool())
            {
                values.sync_scroll = x;
            }
            if let Some(x) = map.get("language").and_then(|v| v.as_str()) {
                values.language = x.to_string();
            }
            if let Some(v) = map.get("glossary_folders").or(map.get("glossar_ordner")) {
                if let Some(arr) = v.as_array() {
                    values.glossary_folders = arr
                        .iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect();
                }
            }
            if let Some(v) = map.get("last_vault") {
                values.last_vault = v.as_str().map(String::from);
            }
            if let Some(v) = map.get("keybinds") {
                if let Some(arr) = v.as_array() {
                    if !arr.is_empty() {
                        values.keybinds = arr
                            .iter()
                            .filter_map(|x| {
                                let a = x.get(0)?.as_str()?.to_string();
                                let b = x.get(1)?.as_str()?.to_string();
                                Some((a, b))
                            })
                            .collect();
                    }
                }
            }
        }
        SettingsManager {
            values,
            dirty: false,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Schreibt die Standard-Datei nur, wenn sich etwas geändert hat.
    pub fn save_if_needed(&mut self) -> bool {
        match config_path() {
            Some(p) => self.save_if_needed_to(&p),
            None => false,
        }
    }

    /// Variante mit explizitem Pfad (Tests: Temp-Datei).
    pub fn save_if_needed_to(&mut self, path: &std::path::Path) -> bool {
        if !self.dirty {
            return false;
        }
        self.dirty = false;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(&self.values) {
            Ok(json) => {
                let _ = std::fs::write(path, json);
                true
            }
            Err(_) => false,
        }
    }

    /// Setzt last_vault nur bei tatsächlicher Änderung (kein Dirty-Spam).
    pub fn set_last_vault(&mut self, path: Option<String>) {
        if self.values.last_vault != path {
            self.values.last_vault = path;
            self.dirty = true;
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rusty-notes").join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> std::path::PathBuf {
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
    fn defaults_are_sensible() {
        let e = Settings::default();
        assert!(e.glossary_enabled);
        assert_eq!(e.glossary_folders, vec!["Glossar".to_string()]);
        assert_eq!(e.glossary_min_len, 3);
        assert_eq!(e.editor_font_size, 14.0);
        assert!(e.keybinds.len() >= Action::ALL.len());
    }

    #[test]
    fn keybind_roundtrip() {
        let b = Keybind::new(true, true, false, "S");
        assert_eq!(b.display_in(Language::En), "Ctrl+Shift+S");
        assert_eq!(b.display_in(Language::De), "Strg+Umschalt+S");
        assert_eq!(b.serialize(), "Ctrl+Shift+S");
        let back = Keybind::deserialize("Ctrl+Shift+S").unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn set_and_get_binding() {
        let mut e = Settings::default();
        let alt = e.binding_for(Action::Save).unwrap();
        assert_eq!(alt.key, "S");

        e.set_binding(Action::Save, Some(Keybind::new(true, false, true, "F2")));
        let updated = e.binding_for(Action::Save).unwrap();
        assert_eq!(updated.key, "F2");
        assert!(updated.alt && updated.ctrl && !updated.shift);

        e.set_binding(Action::ToggleGlossary, None);
        assert!(e.binding_for(Action::ToggleGlossary).is_none());
    }

    #[test]
    fn action_key_roundtrip() {
        for a in Action::ALL {
            assert_eq!(Action::from_key(a.key()), Some(*a), "{}", a.key());
            assert!(!a.label(&crate::i18n::EN).is_empty());
        }
        assert!(Action::from_key("nope").is_none());
    }

    #[test]
    fn no_write_without_changes() {
        let path = tmp_path("clean");
        let mut m = SettingsManager::load_from(&path);
        assert!(!m.is_dirty());
        assert!(
            !m.save_if_needed_to(&path),
            "unveränderte Settings dürfen nicht schreiben"
        );
        assert!(!path.exists(), "es darf keine Datei angelegt werden");
    }

    #[test]
    fn change_marks_dirty_and_writes_tempfile() {
        let path = tmp_path("dirty");
        let mut m = SettingsManager::load_from(&path);
        m.mark_dirty();
        assert!(m.is_dirty());
        assert!(m.save_if_needed_to(&path), "dirty → muss schreiben");
        assert!(!m.is_dirty());
        assert!(path.exists());
    }

    #[test]
    fn load_roundtrip_with_new_fields() {
        let path = tmp_path("roundtrip");
        let mut m = SettingsManager::load_from(&path);
        m.values.glossary_max_hits = 77;
        m.values.glossary_folders = vec!["Wissen/Glossar".into(), "Begriffe".into()];
        m.values.editor_font_size = 17.5;
        m.values
            .set_binding(Action::Save, Some(Keybind::new(false, false, false, "F2")));
        m.mark_dirty();
        assert!(m.save_if_needed_to(&path));

        let m2 = SettingsManager::load_from(&path);
        assert_eq!(m2.values.glossary_max_hits, 77);
        assert_eq!(m2.values.glossary_folders.len(), 2);
        assert_eq!(m2.values.editor_font_size, 17.5);
        assert_eq!(m2.values.binding_for(Action::Save).unwrap().key, "F2");
    }

    #[test]
    fn legacy_german_config_loads() {
        let path = tmp_path("legacy");
        std::fs::write(
            &path,
            r#"{"glossar_aktiv": false, "vorschau_sichtbar": false, "glossar_max_treffer": 42, "sync_scroll_aktiv": false, "keybinds": [["speichern", "Ctrl+S"]]}"#,
        )
        .unwrap();
        let m = SettingsManager::load_from(&path);
        assert!(!m.values.glossary_enabled);
        assert!(!m.values.preview_visible);
        assert_eq!(m.values.glossary_max_hits, 42);
        assert!(!m.values.sync_scroll);
        assert_eq!(m.values.language, "en");
        // legacy German keybind key still resolves
        assert_eq!(m.values.binding_for(Action::Save).unwrap().key, "S");
    }

    #[test]
    fn last_vault_dirty_only_on_change() {
        let path = tmp_path("vault");
        let mut m = SettingsManager::load_from(&path);
        m.set_last_vault(None);
        assert!(!m.is_dirty(), "gleicher Wert (None) → kein Dirty");

        m.set_last_vault(Some("/x".into()));
        assert!(m.is_dirty(), "neuer Wert → Dirty");

        let _ = m.save_if_needed_to(&path);
        m.set_last_vault(Some("/x".into()));
        assert!(!m.is_dirty(), "gleicher Wert erneut → kein Dirty");
    }
}

#[cfg(test)]
mod keybind_default_tests {
    use super::*;

    #[test]
    fn empty_keybinds_get_defaults() {
        let path = std::env::temp_dir().join(format!(
            "rusty-kb-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"autosave_ms": 500, "keybinds": []}"#).unwrap();

        let m = SettingsManager::load_from(&path);
        assert_eq!(m.values.autosave_ms, 500);
        assert!(
            m.values.binding_for(Action::QuickSwitcher).is_some(),
            "leere keybinds müssen Standard-Belegung erhalten"
        );
        assert!(m.values.binding_for(Action::Save).is_some());
    }
}

#[cfg(test)]
mod sync_scroll_tests {
    use super::*;

    #[test]
    fn sync_scroll_action_serializable() {
        // Schlüssel-Round-Trip
        let a = Action::ToggleSyncScroll;
        let s = a.key();
        assert_eq!(s, "toggle_sync_scroll");
        assert_eq!(Action::from_key(&s), Some(a));

        // Standard-Keybind existiert und ist Ctrl+Shift+Y
        let kb = default_keybinds()
            .into_iter()
            .find(|(name, _)| name == "toggle_sync_scroll")
            .expect("Sync-Scroll-Default-Keybind vorhanden");
        let bind = &kb.1;
        assert!(bind.contains("Ctrl") && bind.contains("Shift") && bind.contains("Y"));
    }

    #[test]
    fn sync_scroll_defaults_true() {
        assert!(Settings::default().sync_scroll);
    }
}
