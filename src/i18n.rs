//! Deutsche UI-Texte (App ist einsprachig deutsch).

pub const OPEN_FOLDER: &str = "Ordner öffnen";
pub const NEW_NOTE: &str = "Neue Notiz";
pub const SEARCH: &str = "Suche";
pub const PREVIEW: &str = "Vorschau";
pub const NOTES: &str = "Notizen";
pub const SETTINGS: &str = "Einstellungen";
pub const RENAME: &str = "Umbenennen";
pub const DELETE: &str = "Löschen";
pub const SAVED: &str = "gespeichert";
pub const UNSAVED: &str = "● ungespeichert";
pub const NO_MATCHES: &str = "Keine Treffer";
pub const NO_VAULT: &str = "Kein Vault geöffnet";
pub const SELECT_OR_CREATE: &str = "Notiz auswählen oder erstellen (Strg+N)";
pub const A_FAST_MARKDOWN_EDITOR: &str = "Ein schneller Markdown-Editor";
pub const OPEN_FOLDER_DOTS: &str = "Ordner öffnen…";
pub const SEARCH_ALL_NOTES: &str = "Alle Notizen durchsuchen…";
pub const TYPE_TO_FILTER: &str = "Tippen, um Notizen zu filtern…";
pub const TYPE_A_COMMAND: &str = "Befehl eingeben…";
pub const NO_MATCHING_NOTES: &str = "Keine passenden Notizen";
pub const QUICK_SWITCHER: &str = "Schnellwechsler";
pub const COMMAND_PALETTE: &str = "Befehlspalette";
pub const OK: &str = "OK";
pub const CANCEL: &str = "Abbrechen";
pub const YES: &str = "Ja";
pub const NO: &str = "Nein";
pub const CONFIRM: &str = "Bestätigen";
pub const CONFIRM_DELETE: &str = "Diese Notiz löschen? Das kann nicht rückgängig gemacht werden.";
pub const NEW_NOTE_PROMPT: &str = "Name der neuen Notiz (Ordner erlaubt):";
pub const RENAME_PROMPT: &str = "Umbenennen in:";
pub const FRONT_MATTER: &str = "Front-Matter:";
pub const TITLE_LABEL: &str = "Titel:";
pub const TAGS_LABEL: &str = "Tags:";
pub const FILE_GONE: &str = "(Datei existiert nicht mehr)";
pub const NO_BUFFER: &str = "(kein Puffer)";

// Statusmeldungen
pub fn status_vault(p: &str) -> String { format!("Vault: {}", p) }
pub fn status_created(r: &str) -> String { format!("Erstellt: {}", r) }
pub fn status_renamed(r: &str) -> String { format!("Umbenannt: {}", r) }
pub fn status_deleted() -> String { "Notiz gelöscht".into() }
pub fn status_saved() -> String { "Gespeichert".into() }
pub fn status_note_not_found() -> String { "Notiz nicht gefunden".into() }
pub fn status_open_failed() -> String { "Notiz konnte nicht geöffnet werden".into() }
pub fn status_opening(p: &str) -> String { format!("Öffne {} …", p) }
pub fn status_reopen_failed(p: &str, e: &str) -> String { format!("Konnte {} nicht öffnen: {}", p, e) }
pub fn status_open_failed_path(p: &str, e: &str) -> String { format!("Fehler beim Öffnen von {}: {}", p, e) }
pub fn status_create_failed(e: &str) -> String { format!("Erstellen fehlgeschlagen: {}", e) }
pub fn status_rename_failed(e: &str) -> String { format!("Umbenennen fehlgeschlagen: {}", e) }
pub fn status_delete_failed(e: &str) -> String { format!("Löschen fehlgeschlagen: {}", e) }
pub fn status_save_failed(e: &str) -> String { format!("Speichern fehlgeschlagen: {}", e) }
pub fn status_notes_count(n: usize) -> String { format!("{} Notizen", n) }
pub fn status_glossar_hits(n: usize) -> String { format!("Glossar: {} Begriff(e) verlinkt", n) }

// Befehlspalette
pub const CMD_TOGGLE_PREVIEW: &str = "Vorschau umschalten";
pub const CMD_NEW_NOTE: &str = "Neue Notiz";
pub const CMD_SAVE: &str = "Jetzt speichern";
pub const CMD_OPEN_FOLDER: &str = "Ordner öffnen…";
pub const CMD_FOCUS_SEARCH: &str = "Suche fokussieren";
pub const CMD_SETTINGS: &str = "Einstellungen öffnen";

// Einstellungen-Dialog
pub const SETTINGS_TITLE: &str = "Einstellungen";
pub const SET_AUTOSAVE: &str = "Autospeichern-Verzögerung (ms)";
pub const SET_PREVIEW: &str = "Vorschau standardmäßig anzeigen";
pub const SET_GLOSSAR: &str = "Glossar aktiv (Begriffe automatisch verlinken)";
pub const SET_GLOSSAR_CI: &str = "Groß-/Kleinschreibung im Glossar ignorieren";
pub const SET_GLOSSAR_MAX: &str = "Max. Glossar-Treffer pro Notiz";
pub const SET_CLOSE: &str = "Schließen";
