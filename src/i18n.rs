//! UI strings: English by default, German selectable (Settings → Language).

/// App language. Unknown codes fall back to English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    En,
    De,
}

impl Language {
    pub fn from_code(s: &str) -> Language {
        match s.trim().to_lowercase().as_str() {
            "de" | "deutsch" | "german" => Language::De,
            _ => Language::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::De => "de",
        }
    }
}

/// All user-facing strings for one language.
#[derive(Debug, Clone, Copy)]
pub struct Texts {
    pub open_folder: &'static str,
    pub new_note: &'static str,
    pub search: &'static str,
    pub preview: &'static str,
    pub notes: &'static str,
    pub settings: &'static str,
    pub rename: &'static str,
    pub delete: &'static str,
    pub saved: &'static str,
    pub unsaved: &'static str,
    pub no_matches: &'static str,
    pub no_vault: &'static str,
    pub select_or_create: &'static str,
    pub tagline: &'static str,
    pub open_folder_dots: &'static str,
    pub search_all_notes: &'static str,
    pub type_to_filter: &'static str,
    pub type_a_command: &'static str,
    pub no_matching_notes: &'static str,
    pub quick_switcher: &'static str,
    pub command_palette: &'static str,
    pub ok: &'static str,
    pub cancel: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
    pub confirm: &'static str,
    pub confirm_delete: &'static str,
    pub new_note_prompt: &'static str,
    pub rename_prompt: &'static str,
    pub front_matter: &'static str,
    pub title_label: &'static str,
    pub tags_label: &'static str,
    pub file_gone: &'static str,
    pub no_buffer: &'static str,
    pub note_label: &'static str,
    pub create_note_button: &'static str,
    pub glossary_refs_heading: &'static str,
    // Command palette
    pub cmd_toggle_preview: &'static str,
    pub cmd_new_note: &'static str,
    pub cmd_save: &'static str,
    pub cmd_open_folder: &'static str,
    pub cmd_focus_search: &'static str,
    pub cmd_settings: &'static str,
    // Settings dialog
    pub settings_title: &'static str,
    pub general_heading: &'static str,
    pub editor_heading: &'static str,
    pub glossary_heading: &'static str,
    pub shortcuts_heading: &'static str,
    pub language_label: &'static str,
    pub set_autosave: &'static str,
    pub set_preview: &'static str,
    pub font_size_label: &'static str,
    pub set_glossary: &'static str,
    pub set_glossary_ci: &'static str,
    pub show_refs_below: &'static str,
    pub set_glossary_max: &'static str,
    pub min_term_length: &'static str,
    pub glossary_folders_hint: &'static str,
    pub shortcuts_hint: &'static str,
    pub change_button: &'static str,
    pub set_close: &'static str,
    pub shortcut_hint_open: &'static str,
    pub sync_scroll_on: &'static str,
    pub sync_scroll_off: &'static str,
    pub glossary_on: &'static str,
    pub glossary_off: &'static str,
    // Action names (settings keybind grid)
    pub act_open_folder: &'static str,
    pub act_quick_switcher: &'static str,
    pub act_command_palette: &'static str,
    pub act_save: &'static str,
    pub act_new_note: &'static str,
    pub act_toggle_preview: &'static str,
    pub act_focus_search: &'static str,
    pub act_open_settings: &'static str,
    pub act_close_note: &'static str,
    pub act_next_note: &'static str,
    pub act_prev_note: &'static str,
    pub act_toggle_glossary: &'static str,
    pub act_toggle_sync_scroll: &'static str,
}

pub const EN: Texts = Texts {
    open_folder: "Open folder",
    new_note: "New note",
    search: "Search",
    preview: "Preview",
    notes: "Notes",
    settings: "Settings",
    rename: "Rename",
    delete: "Delete",
    saved: "saved",
    unsaved: "● unsaved",
    no_matches: "No matches",
    no_vault: "No vault open",
    select_or_create: "Select or create a note (Ctrl+N)",
    tagline: "A fast Markdown editor",
    open_folder_dots: "Open folder…",
    search_all_notes: "Search all notes…",
    type_to_filter: "Type to filter notes…",
    type_a_command: "Type a command…",
    no_matching_notes: "No matching notes",
    quick_switcher: "Quick switcher",
    command_palette: "Command palette",
    ok: "OK",
    cancel: "Cancel",
    yes: "Yes",
    no: "No",
    confirm: "Confirm",
    confirm_delete: "Delete this note? This cannot be undone.",
    new_note_prompt: "New note name (folders allowed):",
    rename_prompt: "Rename to:",
    front_matter: "Front matter:",
    title_label: "Title:",
    tags_label: "Tags:",
    file_gone: "(file no longer exists)",
    no_buffer: "(no buffer)",
    note_label: "Note:",
    create_note_button: "Create new note",
    glossary_refs_heading: "Glossary:",
    cmd_toggle_preview: "Toggle preview",
    cmd_new_note: "New note",
    cmd_save: "Save now",
    cmd_open_folder: "Open folder…",
    cmd_focus_search: "Focus search",
    cmd_settings: "Open settings",
    settings_title: "Settings",
    general_heading: "General",
    editor_heading: "Editor",
    glossary_heading: "Glossary",
    shortcuts_heading: "Shortcuts",
    language_label: "Language",
    set_autosave: "Autosave delay (ms)",
    set_preview: "Show preview by default",
    font_size_label: "Font size:",
    set_glossary: "Glossary active (auto-link terms)",
    set_glossary_ci: "Ignore case in glossary",
    show_refs_below: "Show references below the preview",
    set_glossary_max: "Max. glossary hits per note",
    min_term_length: "Minimum term length:",
    glossary_folders_hint: "Glossary subfolders (one per line, empty = whole vault):",
    shortcuts_hint: "Click “Change”, then press a key. Escape cancels.",
    change_button: "Change",
    set_close: "Close",
    shortcut_hint_open: "Ctrl+O",
    sync_scroll_on: "Synced scrolling enabled",
    sync_scroll_off: "Synced scrolling disabled",
    glossary_on: "Glossary enabled",
    glossary_off: "Glossary disabled",
    act_open_folder: "Open folder",
    act_quick_switcher: "Quick switcher",
    act_command_palette: "Command palette",
    act_save: "Save",
    act_new_note: "New note",
    act_toggle_preview: "Toggle preview",
    act_focus_search: "Focus search",
    act_open_settings: "Open settings",
    act_close_note: "Close note",
    act_next_note: "Next note",
    act_prev_note: "Previous note",
    act_toggle_glossary: "Toggle glossary",
    act_toggle_sync_scroll: "Toggle sync scroll",
};

pub const DE: Texts = Texts {
    open_folder: "Ordner öffnen",
    new_note: "Neue Notiz",
    search: "Suche",
    preview: "Vorschau",
    notes: "Notizen",
    settings: "Einstellungen",
    rename: "Umbenennen",
    delete: "Löschen",
    saved: "gespeichert",
    unsaved: "● ungespeichert",
    no_matches: "Keine Treffer",
    no_vault: "Kein Vault geöffnet",
    select_or_create: "Notiz auswählen oder erstellen (Strg+N)",
    tagline: "Ein schneller Markdown-Editor",
    open_folder_dots: "Ordner öffnen…",
    search_all_notes: "Alle Notizen durchsuchen…",
    type_to_filter: "Tippen, um Notizen zu filtern…",
    type_a_command: "Befehl eingeben…",
    no_matching_notes: "Keine passenden Notizen",
    quick_switcher: "Schnellwechsler",
    command_palette: "Befehlspalette",
    ok: "OK",
    cancel: "Abbrechen",
    yes: "Ja",
    no: "Nein",
    confirm: "Bestätigen",
    confirm_delete: "Diese Notiz löschen? Das kann nicht rückgängig gemacht werden.",
    new_note_prompt: "Name der neuen Notiz (Ordner erlaubt):",
    rename_prompt: "Umbenennen in:",
    front_matter: "Front-Matter:",
    title_label: "Titel:",
    tags_label: "Tags:",
    file_gone: "(Datei existiert nicht mehr)",
    no_buffer: "(kein Puffer)",
    note_label: "Notiz:",
    create_note_button: "Neue Notiz erstellen",
    glossary_refs_heading: "Glossar:",
    cmd_toggle_preview: "Vorschau umschalten",
    cmd_new_note: "Neue Notiz",
    cmd_save: "Jetzt speichern",
    cmd_open_folder: "Ordner öffnen…",
    cmd_focus_search: "Suche fokussieren",
    cmd_settings: "Einstellungen öffnen",
    settings_title: "Einstellungen",
    general_heading: "Allgemein",
    editor_heading: "Editor",
    glossary_heading: "Glossar",
    shortcuts_heading: "Tastenkürzel",
    language_label: "Sprache",
    set_autosave: "Autospeichern-Verzögerung (ms)",
    set_preview: "Vorschau standardmäßig anzeigen",
    font_size_label: "Schriftgröße:",
    set_glossary: "Glossar aktiv (Begriffe automatisch verlinken)",
    set_glossary_ci: "Groß-/Kleinschreibung im Glossar ignorieren",
    show_refs_below: "Verweise unter der Vorschau anzeigen",
    set_glossary_max: "Max. Glossar-Treffer pro Notiz",
    min_term_length: "Minimale Begriffslänge:",
    glossary_folders_hint: "Glossar-Unterordner (einer pro Zeile, leer = gesamter Vault):",
    shortcuts_hint: "Klick auf „Ändern“, dann Taste drücken. Escape bricht ab.",
    change_button: "Ändern",
    set_close: "Schließen",
    shortcut_hint_open: "Strg+O",
    sync_scroll_on: "Synchronisiertes Scrollen aktiviert",
    sync_scroll_off: "Synchronisiertes Scrollen deaktiviert",
    glossary_on: "Glossar aktiviert",
    glossary_off: "Glossar deaktiviert",
    act_open_folder: "Ordner öffnen",
    act_quick_switcher: "Schnellwechsler",
    act_command_palette: "Befehlspalette",
    act_save: "Speichern",
    act_new_note: "Neue Notiz",
    act_toggle_preview: "Vorschau umschalten",
    act_focus_search: "Suche fokussieren",
    act_open_settings: "Einstellungen",
    act_close_note: "Notiz schließen",
    act_next_note: "Nächste Notiz",
    act_prev_note: "Vorherige Notiz",
    act_toggle_glossary: "Glossar umschalten",
    act_toggle_sync_scroll: "Sync-Scroll umschalten",
};

/// Pick the table for a language (English default).
pub fn pick(lang: Language) -> &'static Texts {
    match lang {
        Language::En => &EN,
        Language::De => &DE,
    }
}

/// Pick by `"en"` / `"de"` code; unknown codes give English.
pub fn pick_code(code: &str) -> &'static Texts {
    pick(Language::from_code(code))
}

// Status messages (language-aware).
impl Texts {
    pub fn status_vault(&self, p: &str) -> String {
        format!("Vault: {}", p)
    }
    pub fn status_created(&self, r: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Created: {}", r)
        } else {
            format!("Erstellt: {}", r)
        }
    }
    pub fn status_renamed(&self, r: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Renamed: {}", r)
        } else {
            format!("Umbenannt: {}", r)
        }
    }
    pub fn status_deleted(&self) -> String {
        if std::ptr::eq(self, &EN) {
            "Note deleted".into()
        } else {
            "Notiz gelöscht".into()
        }
    }
    pub fn status_saved(&self) -> String {
        if std::ptr::eq(self, &EN) {
            "Saved".into()
        } else {
            "Gespeichert".into()
        }
    }
    pub fn status_note_not_found(&self) -> String {
        if std::ptr::eq(self, &EN) {
            "Note not found".into()
        } else {
            "Notiz nicht gefunden".into()
        }
    }
    pub fn status_open_failed(&self) -> String {
        if std::ptr::eq(self, &EN) {
            "Could not open note".into()
        } else {
            "Notiz konnte nicht geöffnet werden".into()
        }
    }
    pub fn status_opening(&self, p: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Opening {} …", p)
        } else {
            format!("Öffne {} …", p)
        }
    }
    pub fn status_reopen_failed(&self, p: &str, e: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Could not open {}: {}", p, e)
        } else {
            format!("Konnte {} nicht öffnen: {}", p, e)
        }
    }
    pub fn status_open_failed_path(&self, p: &str, e: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Error opening {}: {}", p, e)
        } else {
            format!("Fehler beim Öffnen von {}: {}", p, e)
        }
    }
    pub fn status_create_failed(&self, e: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Create failed: {}", e)
        } else {
            format!("Erstellen fehlgeschlagen: {}", e)
        }
    }
    pub fn status_rename_failed(&self, e: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Rename failed: {}", e)
        } else {
            format!("Umbenennen fehlgeschlagen: {}", e)
        }
    }
    pub fn status_delete_failed(&self, e: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Delete failed: {}", e)
        } else {
            format!("Löschen fehlgeschlagen: {}", e)
        }
    }
    pub fn status_save_failed(&self, e: &str) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Save failed: {}", e)
        } else {
            format!("Speichern fehlgeschlagen: {}", e)
        }
    }
    pub fn status_notes_count(&self, n: usize) -> String {
        if std::ptr::eq(self, &EN) {
            format!("{} notes", n)
        } else {
            format!("{} Notizen", n)
        }
    }
    pub fn status_glossary_hits(&self, n: usize) -> String {
        if std::ptr::eq(self, &EN) {
            format!("Glossary: {} term(s) linked", n)
        } else {
            format!("Glossar: {} Begriff(e) verlinkt", n)
        }
    }
}
