//! rusty-notes: a fast, keyboard-driven Markdown note editor.

use rusty_notes::{
    code_highlight::CodeHighlighter,
    settings::{Action, SettingsManager, Settings, Keybind},
    glossary::{self, Glossary},
    i18n::{self, Language, Texts},
    markdown, search, vault,
};

use std::path::PathBuf;
use std::sync::LazyLock;

use eframe::egui;
use egui::{Color32, Key};
use egui_commonmark::CommonMarkCache;

use rusty_notes::editor::{highlight, Tok};

// ---------------------------------------------------------------------------
// Settings (persistiert ueber SettingsManager in der Bibliothek)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
enum SidebarTab {
    Notes,
    Search,
}

enum Overlay {
    None,
    Switcher,
    CommandPalette,
    Prompt {
        title: String,
        value: String,
        action: PromptAction,
    },
    Confirm {
        message: String,
        action: ConfirmAction,
        arg: PathBuf,
    },
    Settings {
        /// Arbeitskopie; wird beim Schließen übernommen (Dirty nur bei Diff).
        draft: Settings,
    },
}

#[derive(Clone)]
enum PromptAction {
    NewNote,
    Rename(PathBuf),
}

#[derive(Clone)]
enum ConfirmAction {
    Delete,
}

struct App {
    vault: Option<vault::Vault>,
    active: Option<PathBuf>,
    preview_visible: bool,
    sidebar_tab: SidebarTab,
    search_query: String,
    search_results: Vec<search::SearchHit>,
    switcher_query: String,
    switcher_selected: usize,
    overlay: Overlay,
    status: String,
    dirty_at: Option<std::time::Instant>,
    cache: CommonMarkCache,
    pending_link: Option<PathBuf>,
    focus_editor_once: bool,
    settings: SettingsManager,
    glossary: Option<Glossary>,
    recording_keybind: Option<Action>,
    /// Strg+Hover: (Zielnotiz, Bildschirmposition) für das Popup.
    hover_link: Option<(String, egui::Pos2)>,
    /// Sync-Scroll: letzter geteilter Scroll-Stand der Editor-/Vorschau-Ansicht.
    sync_scroll: f32,
    /// Letzter Sync-Anteil, um Editor-Änderungen zu erkennen (Some = geändert).
    sync_last: Option<f32>,
    /// True im Frame nach einer Editor-Scroll-Änderung.
    sync_changed: bool,
    /// Letzte gemessene Vorschau-Maße (content, visible) für Ziel-Offset-Berechnung.
    preview_measure: PreviewMeasure,
}

#[derive(Default)]
struct PreviewMeasure {
    last: Option<(f32, f32)>,
}

impl Default for App {
    fn default() -> Self {
        let settings = SettingsManager::load();
        let last_vault = settings.values.last_vault.clone();
        let preview_on = settings.values.preview_visible;
        Self {
            vault: None,
            active: None,
            preview_visible: preview_on,
            sidebar_tab: SidebarTab::Notes,
            search_query: String::new(),
            search_results: Vec::new(),
            switcher_query: String::new(),
            switcher_selected: 0,
            overlay: Overlay::None,
            status: format!("{} ({})", i18n::EN.open_folder, i18n::EN.shortcut_hint_open),
            dirty_at: None,
            cache: CommonMarkCache::default(),
            pending_link: None,
            focus_editor_once: false,
            settings,
            glossary: None,
            recording_keybind: None,
            hover_link: None,
            sync_scroll: 0.0,
            sync_last: None,
            sync_changed: false,
            preview_measure: PreviewMeasure::default(),
        }
        .with_last_vault(last_vault)
    }
}

impl App {
    /// UI strings for the language saved in settings (English default).
    fn texts(&self) -> &'static Texts {
        i18n::pick_code(&self.settings.values.language)
    }

    fn with_last_vault(mut self, last: Option<String>) -> Self {
        let txt = self.texts();
        if let Some(v) = last {
            self.status = txt.status_opening(&v);
            match open_vault(&v) {
                Ok(mut vault) => {
                    vault.scan().ok();
                    let first = vault.notes().first().map(|n| n.abs.clone());
                    self.vault = Some(vault);
                    self.refresh_glossary();
                    if let Some(f) = first {
                        self.open_note(f);
                    }
                    self.status = txt.status_vault(&v);
                }
                Err(e) => {
                    self.status = txt.status_reopen_failed(&v, &e.to_string());
                }
            }
        }
        self
    }

    /// Löst einen Wikilink-Text ([[Ziel]]) auf einen existierenden Notizpfad auf.
    fn note_for_wikilink(&self, target: &str) -> Option<PathBuf> {
        let v = self.vault.as_ref()?;
        let pfade: Vec<PathBuf> = v.notes().iter().map(|n| n.abs.clone()).collect();
        markdown::resolve_wikilink(pfade.iter().map(|p| p.as_path()), target)
    }

    /// Glossary aus allen Notiz-Stems neu aufbauen (nur bei Vault-Änderung).
    fn refresh_glossary(&mut self) {
        if !self.settings.values.glossary_enabled {
            self.glossary = None;
            return;
        }
        let folders = self.settings.values.glossary_folders.clone();
        if let Some(v) = self.vault.as_mut() {
            let notes = v.notes_in(&folders);
            let entries: Vec<glossary::GlossaryEntry> = notes
                .iter()
                .map(|abs| {
                    // Aliase aus Front-Matter der Notiz lesen (Cache!).
                    let aliases = v
                        .content_for(abs)
                        .ok()
                        .map(|c| {
                            markdown::parse_front_matter(
                                markdown::split_front_matter(&c).0.unwrap_or(""),
                            )
                            .aliases
                        })
                        .unwrap_or_default();
                    glossary::GlossaryEntry {
                        term: abs
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string(),
                        aliases,
                        path: abs.clone(),
                    }
                })
                .collect();
            self.glossary = Some(Glossary::new(
                entries,
                self.settings.values.glossary_case_insensitive,
            ));
        }
    }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        set_theme(&cc.egui_ctx);
        Self::default()
    }

    fn open_folder_dialog_and_load(&mut self) {
        if let Some(dir) = pick_folder_dialog("Open Vault Folder") {
            self.load_vault_at(std::path::PathBuf::from(dir));
        }
    }

    fn load_vault_at(&mut self, dir: PathBuf) {
        let txt = self.texts();
        match open_vault(&dir.to_string_lossy()) {
            Ok(mut v) => {
                v.scan().ok();
                let first = v.notes().first().map(|n| n.abs.clone());
                self.active = None;
                self.vault = Some(v);
                self.refresh_glossary();
                if let Some(f) = first {
                    self.open_note(f);
                }
                self.status = txt.status_vault(&dir.display().to_string());
                self.settings
                    .set_last_vault(Some(dir.to_string_lossy().into_owned()));
            }
            Err(e) => {
                self.status =
                    txt.status_open_failed_path(&dir.display().to_string(), &e.to_string());
            }
        }
    }

    fn open_note(&mut self, abs: PathBuf) {
        let txt = self.texts();
        if let Some(v) = self.vault.as_mut() {
            if v.open_note(&abs).is_ok() {
                self.active = Some(abs);
                self.focus_editor_once = true;
            } else {
                self.status = txt.status_open_failed();
            }
        }
    }

    fn save_active(&mut self) {
        let txt = self.texts();
        if let (Some(v), Some(active)) = (self.vault.as_mut(), &self.active) {
            match v.save(active) {
                Ok(()) => self.status = txt.status_saved(),
                Err(e) => self.status = txt.status_save_failed(&e.to_string()),
            }
        }
    }

    fn create_note_flow(&mut self) {
        let txt = self.texts();
        self.overlay = Overlay::Prompt {
            title: txt.new_note_prompt.into(),
            value: String::new(),
            action: PromptAction::NewNote,
        };
    }

    fn rename_note_flow(&mut self, path: PathBuf) {
        let txt = self.texts();
        let default_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        self.overlay = Overlay::Prompt {
            title: txt.rename_prompt.into(),
            value: default_name,
            action: PromptAction::Rename(path),
        };
    }

    fn run_prompt_action(&mut self, action: &PromptAction, value: &str) {
        let txt = self.texts();
        let name = value.trim();
        if name.is_empty() {
            return;
        }
        match action {
            PromptAction::NewNote => {
                if let Some(v) = self.vault.as_mut() {
                    let rel = if name.ends_with(".md") {
                        name.to_string()
                    } else {
                        format!("{}.md", name)
                    };
                    match v.create_note(&rel) {
                        Ok(path) => {
                            self.active = Some(path);
                            self.focus_editor_once = true;
                            self.refresh_glossary();
                            self.status = txt.status_created(&rel);
                        }
                        Err(e) => self.status = txt.status_create_failed(&e.to_string()),
                    }
                }
            }
            PromptAction::Rename(old_path) => {
                if let Some(v) = self.vault.as_mut() {
                    let old_rel = old_path.strip_prefix(v.root()).unwrap_or(old_path);
                    let parent = old_rel.parent().and_then(|p| p.to_str()).unwrap_or("");
                    let new_rel = if parent.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}/{}", parent, name)
                    };
                    let new_rel = if new_rel.ends_with(".md") {
                        new_rel
                    } else {
                        format!("{}.md", new_rel)
                    };
                    match v.rename_note(old_path, &new_rel) {
                        Ok(new_abs) => {
                            if self.active.as_deref() == Some(old_path.as_path()) {
                                self.active = Some(new_abs.clone());
                            }
                            self.refresh_glossary();
                            self.status = txt.status_renamed(&new_rel);
                        }
                        Err(e) => self.status = txt.status_rename_failed(&e.to_string()),
                    }
                }
            }
        }
    }

    fn run_confirm_action(&mut self, action: &ConfirmAction, arg: &std::path::Path) {
        let txt = self.texts();
        match action {
            ConfirmAction::Delete => {
                if let Some(v) = self.vault.as_mut() {
                    match v.delete_note(arg) {
                        Ok(()) => {
                            if self.active.as_deref() == Some(arg) {
                                self.active = None;
                            }
                            self.refresh_glossary();
                            self.status = txt.status_deleted();
                        }
                        Err(e) => self.status = txt.status_delete_failed(&e.to_string()),
                    }
                }
            }
        }
    }

    fn autosave_tick(&mut self) {
        if let Some(t) = self.dirty_at {
            if t.elapsed() >= std::time::Duration::from_millis(self.settings.values.autosave_ms) {
                self.dirty_at = None;
                self.save_active();
            }
        }
    }

    fn mark_dirty_timer(&mut self) {
        self.dirty_at = Some(std::time::Instant::now());
    }

    fn collect_search(&mut self) {
        self.search_results.clear();
        let q = self.search_query.trim().to_lowercase();
        if q.is_empty() {
            return;
        }
        if let Some(v) = self.vault.as_mut() {
            let pfade: Vec<(PathBuf, String)> = v
                .notes()
                .iter()
                .map(|n| (n.abs.clone(), n.rel.clone()))
                .collect();
            let mut pairs: Vec<(String, String)> = Vec::with_capacity(pfade.len());
            for (abs, rel) in &pfade {
                if let Ok(content) = v.content_for(abs) {
                    pairs.push((rel.clone(), content));
                }
            }
            self.search_results = search::search_notes(&pairs, &q);
        }
    }

}

const GLOSSARY_COLOR: Color32 = Color32::from_rgb(126, 231, 135); // sattes Grün, deutlich von Link-Blau unterscheiden

static CODE_HIGHLIGHTER: LazyLock<CodeHighlighter> = LazyLock::new(CodeHighlighter::default);

fn tok_color(tok: Tok) -> Color32 {
    match tok {
        Tok::Heading => Color32::from_rgb(133, 176, 255),
        Tok::BoldItalic => Color32::from_rgb(236, 201, 137),
        Tok::InlineCode => Color32::from_rgb(163, 221, 163),
        Tok::CodeBlock => Color32::from_rgb(152, 206, 206),
        Tok::Quote => Color32::from_rgb(154, 154, 168),
        Tok::Link => Color32::from_rgb(142, 183, 250),
        Tok::ListMarker => Color32::from_rgb(203, 143, 232),
        Tok::FrontMatter => Color32::from_rgb(138, 138, 153),
        Tok::Plain => Color32::from_rgb(220, 223, 228),
    }
}

fn open_vault(root: &str) -> std::io::Result<vault::Vault> {
    vault::Vault::open(std::path::Path::new(root))
}

// ---------------------------------------------------------------------------
// Zenity folder picker (subprocess; no native dialog crates needed)
// ---------------------------------------------------------------------------

/// Ordner-Auswahl: unter Windows der native Dateidialog (rfd/Win32),
/// sonst (Linux/BSD) zenity als leichtgewichtiger Subprozess.
#[cfg(windows)]
fn pick_folder_dialog(title: &str) -> Option<String> {
    rfd::FileDialog::new()
        .set_title(title)
        .pick_folder()
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(not(windows))]
fn pick_folder_dialog(title: &str) -> Option<String> {
    let _ = title;
    let out = std::process::Command::new("zenity")
        .args(["--file-selection", "--directory"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

fn set_theme(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(Color32::from_rgb(220, 223, 228));
    ctx.set_global_style(style);
}

// ---------------------------------------------------------------------------
// eframe integration
// ---------------------------------------------------------------------------

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let txt = self.texts();
        let ctx = ui.ctx().clone();
        self.autosave_tick();

        // Tastatur-Shortcuts aus den Settings.
        if let Some(action) = check_shortcuts(&ctx, &self.settings.values.clone(), &self.overlay) {
            self.run_action(action);
        }

        // Klick aus der Vorschau: rusty-note:-Schema auf Notizpfad auflösen.
        if let Some(target) = self.pending_link.take() {
            let target = if let Some(rest) =
                target.to_str().and_then(|s| s.strip_prefix("rusty-note:"))
            {
                self.note_for_wikilink(rest)
            } else {
                Some(target)
            };
            match target {
                Some(pfad) if pfad.exists() => self.open_note(pfad),
                _ => self.status = txt.status_note_not_found(),
            }
        }

        egui::Panel::top("topbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(txt.open_folder).clicked() {
                    self.open_folder_dialog_and_load();
                }
                if ui.button(txt.new_note).clicked() {
                    self.create_note_flow();
                }
                if ui.button(txt.search).clicked() {
                    self.sidebar_tab = SidebarTab::Search;
                }
                if ui
                    .selectable_label(self.preview_visible, txt.preview)
                    .clicked()
                {
                    self.preview_visible = !self.preview_visible;
                    self.settings.values.preview_visible = self.preview_visible;
                    self.settings.mark_dirty();
                }
                if ui.button(txt.settings).clicked() {
                    self.open_settings();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(txt.status_notes_count(
                        self.vault.as_ref().map(|v| v.notes().len()).unwrap_or(0),
                    ));
                });
            });
        });

        egui::Panel::bottom("statusbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let (Some(v), Some(a)) = (&self.vault, &self.active) {
                        if let Some(b) = v.buffer(a) {
                            if b.dirty {
                                ui.colored_label(Color32::YELLOW, txt.unsaved);
                            } else {
                                ui.weak(txt.saved);
                            }
                        }
                    }
                });
            });
        });

        egui::Panel::left("sidebar")
            .default_size(240.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.sidebar_tab, SidebarTab::Notes, txt.notes);
                    ui.selectable_value(&mut self.sidebar_tab, SidebarTab::Search, txt.search);
                });
                ui.separator();
                match self.sidebar_tab {
                    SidebarTab::Notes => self.ui_notes_list(ui),
                    SidebarTab::Search => self.ui_search_panel(ui),
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.vault.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("rusty-notes");
                    ui.weak(txt.tagline);
                    if ui.button(txt.open_folder_dots).clicked() {
                        self.open_folder_dialog_and_load();
                    }
                });
                return;
            }
            let has_active = self.active.is_some();
            if !has_active {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.weak(txt.select_or_create);
                });
                return;
            }

            let show_editor_only = !self.preview_visible;
            let show_preview_only = false; // future: full-preview mode

            if show_preview_only {
                self.ui_preview(ui);
            } else if show_editor_only {
                self.ui_editor(ui);
            } else {
                egui::Panel::right("preview_panel")
                    .resizable(true)
                    .default_size(420.0)
                    .show(ui, |ui| {
                        self.ui_preview(ui);
                    });
                // remaining area:
                self.ui_editor(ui);
            }
        });

        self.draw_overlay(&ctx);

        // ---- Strg+Hover-Popup über Editor-Links ----
        if let Some((target, pos)) = self.hover_link.clone() {
            let preview_lines: Option<(PathBuf, Vec<String>)> = self.hover_preview(&target);
            egui::Area::new(egui::Id::new("hover_popup"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos + egui::vec2(16.0, 20.0))
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(280.0);
                        match preview_lines.as_ref() {
                            Some((_, lines)) => {
                                ui.horizontal(|ui| {
                                    ui.weak(txt.note_label);
                                    ui.label(target.clone());
                                });
                                for line in lines {
                                    ui.small(line.to_string());
                                }
                            }
                            None => {
                                ui.colored_label(
                                    Color32::ORANGE,
                                    format!("'{}' — {}", target, txt.no_matches),
                                );
                                if ui.small_button(txt.create_note_button).clicked() {
                                    if let Some(v) = self.vault.as_mut() {
                                        let rel = if target.ends_with(".md") {
                                            target.clone()
                                        } else {
                                            format!("{}.md", target)
                                        };
                                        match v.create_note(&rel) {
                                            Ok(created) => {
                                                self.active = Some(created);
                                                self.focus_editor_once = true;
                                                self.refresh_glossary();
                                                self.status = txt.status_created(&rel);
                                            }
                                            Err(e) => {
                                                self.status = txt.status_create_failed(&e.to_string())
                                            }
                                        }
                                    }
                                    self.hover_link = None;
                                }
                            }
                        }
                    });
                });
        }


        // Settings nur bei tatsächlichen Änderungen wegschreiben.
        if self.settings.save_if_needed() {
            // gespeichert — nichts weiter zu tun
        }

        // Keep repainting while an autosave is pending so it fires promptly.
        if self.dirty_at.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Kein Zusatzbedarf; Shortcuts laufen in ui().
    }
}

/// Prüft alle Keybinds aus den Settings gegen den Tastaturzustand.
/// Aufbau der Lookup-Map: O(Keybinds) pro Frame, Lookup O(1) pro Taste.
fn check_shortcuts(
    ctx: &egui::Context,
    values: &Settings,
    overlay: &Overlay,
) -> Option<Action> {
    if !matches!(overlay, Overlay::None) {
        return None; // Overlays schlucken Shortcuts
    }
    let binds: Vec<(Action, Keybind)> = Action::ALL
        .iter()
        .filter_map(|a| values.binding_for(*a).map(|b| (*a, b)))
        .collect();

    ctx.input(|i| {
        let mods = i.modifiers;
        for (action, bind) in binds {
            let ctrl_ok = bind.ctrl == mods.ctrl;
            let shift_ok = bind.shift == mods.shift;
            let alt_ok = bind.alt == mods.alt;
            if ctrl_ok && shift_ok && alt_ok {
                if let Some(key) = egui_key(&bind.key) {
                    if i.key_pressed(key) {
                        return Some(action);
                    }
                }
            }
        }
        None
    })
}

/// Zeichen-Offset -> Byte-Offset (UTF-8-sicher, clampend).
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

/// Serialisierter Name eines egui-Keys.
fn key_name(key: Key) -> &'static str {
    match key {
        Key::A => "A", Key::B => "B", Key::C => "C", Key::D => "D",
        Key::E => "E", Key::F => "F", Key::G => "G", Key::H => "H",
        Key::I => "I", Key::J => "J", Key::K => "K", Key::L => "L",
        Key::M => "M", Key::N => "N", Key::O => "O", Key::P => "P",
        Key::Q => "Q", Key::R => "R", Key::S => "S", Key::T => "T",
        Key::U => "U", Key::V => "V", Key::W => "W", Key::X => "X",
        Key::Y => "Y", Key::Z => "Z",
        Key::F1 => "F1", Key::F2 => "F2", Key::F3 => "F3", Key::F4 => "F4",
        Key::F5 => "F5", Key::F6 => "F6", Key::F7 => "F7", Key::F8 => "F8",
        Key::F9 => "F9", Key::F10 => "F10", Key::F11 => "F11", Key::F12 => "F12",
        Key::ArrowDown => "ArrowDown", Key::ArrowUp => "ArrowUp",
        Key::ArrowLeft => "ArrowLeft", Key::ArrowRight => "ArrowRight",
        Key::Enter => "Enter", Key::Escape => "Escape",
        Key::Tab => "Tab", Key::Space => "Space",
        _ => "Unbekannt",
    }
}

/// egui-Key aus dem serialisierten Namen.
fn egui_key(name: &str) -> Option<Key> {
    Some(match name {
        "A" => Key::A, "B" => Key::B, "C" => Key::C, "D" => Key::D,
        "E" => Key::E, "F" => Key::F, "G" => Key::G, "H" => Key::H,
        "I" => Key::I, "J" => Key::J, "K" => Key::K, "L" => Key::L,
        "M" => Key::M, "N" => Key::N, "O" => Key::O, "P" => Key::P,
        "Q" => Key::Q, "R" => Key::R, "S" => Key::S, "T" => Key::T,
        "U" => Key::U, "V" => Key::V, "W" => Key::W, "X" => Key::X,
        "Y" => Key::Y, "Z" => Key::Z,
        "F1" => Key::F1, "F2" => Key::F2, "F3" => Key::F3, "F4" => Key::F4,
        "F5" => Key::F5, "F6" => Key::F6, "F7" => Key::F7, "F8" => Key::F8,
        "F9" => Key::F9, "F10" => Key::F10, "F11" => Key::F11, "F12" => Key::F12,
        "ArrowDown" => Key::ArrowDown, "ArrowUp" => Key::ArrowUp,
        "ArrowLeft" => Key::ArrowLeft, "ArrowRight" => Key::ArrowRight,
        "Enter" => Key::Enter, "Escape" => Key::Escape,
        "Tab" => Key::Tab, "Space" => Key::Space,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// UI pieces
// ---------------------------------------------------------------------------

impl App {
    fn ui_notes_list(&mut self, ui: &mut egui::Ui) {
        let txt = self.texts();
        let Some(v) = self.vault.as_ref() else {
            ui.weak(txt.no_vault);
            return;
        };

        ui.add_space(4.0);
        let mut to_open: Option<PathBuf> = None;
        let mut to_rename: Option<PathBuf> = None;
        let mut to_delete: Option<PathBuf> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for n in v.notes() {
                let is_active = self.active.as_deref() == Some(n.abs.as_path());
                let label = format!("{}", n.rel);
                let resp = ui.selectable_label(is_active, label);
                if resp.clicked() {
                    to_open = Some(n.abs.clone());
                }
                resp.context_menu(|ui| {
                    if ui.button(txt.rename).clicked() {
                        to_rename = Some(n.abs.clone());
                        ui.close();
                    }
                    if ui.button(txt.delete).clicked() {
                        to_delete = Some(n.abs.clone());
                        ui.close();
                    }
                });
            }
        });

        if let Some(p) = to_open {
            self.open_note(p);
        }
        if let Some(p) = to_rename {
            self.rename_note_flow(p);
        }
        if let Some(p) = to_delete {
            self.overlay = Overlay::Confirm {
                message: txt.confirm_delete.into(),
                action: ConfirmAction::Delete,
                arg: p,
            };
        }
    }

    fn ui_search_panel(&mut self, ui: &mut egui::Ui) {
        let txt = self.texts();
        ui.add_space(4.0);
        let changed = ui
            .add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text(txt.search_all_notes)
                    .desired_width(f32::INFINITY),
            )
            .changed();
        if changed {
            self.collect_search();
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let results = self.search_results.clone();
            for hit in results {
                let label = format!("{}:{}  {}", hit.rel, hit.line_no + 1, hit.line_text);
                if ui
                    .add(
                        egui::Label::new(egui::RichText::new(label).small())
                            .wrap_mode(egui::TextWrapMode::Truncate)
                            .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    if let Some(v) = self.vault.as_ref() {
                        let abs = v.root().join(&hit.rel);
                        self.open_note(abs);
                    }
                }
            }
            if self.search_results.is_empty() && !self.search_query.trim().is_empty() {
                ui.weak(txt.no_matches);
            }
        });
    }

    fn ui_editor(&mut self, ui: &mut egui::Ui) {
        let txt = self.texts();
        let Some(path) = self.active.clone() else {
            return;
        };
        let text_now = self
            .vault
            .as_ref()
            .and_then(|v| v.buffer(&path))
            .map(|b| b.text.clone());
        let Some(mut text) = text_now else {
            ui.weak(txt.file_gone);
            return;
        };

        // Glossary-Treffer einmal pro Frame berechnen (Aho-Corasick, schnell).
        let glossary_enabled = self.settings.values.glossary_enabled;
        let glossary_max = self.settings.values.glossary_max_hits;
        let min_len = self.settings.values.glossary_min_len;
        let font_size = self.settings.values.editor_font_size;
        let glossary_hits: Vec<glossary::GlossaryHit> = match (&self.glossary, glossary_enabled) {
            (Some(g), true) if !g.is_empty() => {
                let mut hits: Vec<glossary::GlossaryHit> = g
                    .find(&text)
                    .into_iter()
                    .filter(|tr| {
                        let s = &text[tr.start..tr.end];
                        s.chars().count() >= min_len
                    })
                    .collect();
                hits.truncate(glossary_max);
                hits
            }
            _ => Vec::new(),
        };

        let editor_id = egui::Id::new(("editor", path.clone()));

        let mut layouter =
            move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let txt = buf.as_str();
                let spans = highlight(txt);
                let pieces: Vec<(usize, usize, glossary::Style)> =
                    glossary::intersect(&spans, &glossary_hits, txt.len());
                let mut job = egui::text::LayoutJob::default();

                // Codeblöcke mit syntect einfärben. Dank korrigierter
                // Fence-Spans ist jeder Block EINE zusammenhängende Span
                // "```info\n...```". Inhalt = nach erster Zeile bis vor "```".
                let mut code_farben: Vec<(usize, usize, [u8; 4])> = Vec::new();
                for s in spans.iter() {
                    if s.tok != Tok::CodeBlock {
                        continue;
                    }
                    let block = &txt[s.start..s.end];
                    let Some(zeile_ende) = block.find('\n') else {
                        continue; // einzeiliger Fence ohne Inhalt
                    };
                    let info = block[3..zeile_ende].trim();
                    // Inhalt: nach Infostring bis vor dem schließenden ``` .
                    // Die Fence-Span endet exakt auf dem schließenden "```" (siehe
                    // editor.rs), also drei Bytes vor block.ende.
                    let fence_ende = block.len().saturating_sub(3);
                    let inhalt = &block[zeile_ende + 1..fence_ende]
                        .strip_suffix('\n')
                        .unwrap_or(&block[zeile_ende + 1..fence_ende]);
                    let basis = s.start + zeile_ende + 1;
                    if info.is_empty() {
                        continue; // ohne Sprache: Standardfarbe belassen
                    }
                    for f in CODE_HIGHLIGHTER.highlight(inhalt, info) {
                        code_farben.push((basis + f.start, basis + f.end, f.farbe));
                    }
                }

                // Stückelung mit Code-Farbgrenzen verschneiden:
                let mut final_stuecke: Vec<(usize, usize, Color32)> = Vec::new();
                for (s, e, stil) in pieces {
                    if stil != glossary::Style::Token(Tok::CodeBlock) {
                        let farbe = match stil {
                            glossary::Style::Glossary => GLOSSARY_COLOR,
                            glossary::Style::Token(tok) => tok_color(tok),
                        };
                        final_stuecke.push((s, e, farbe));
                        continue;
                    }
                    // CodeBlock-Abschnitt in syntect-Farben zerlegen:
                    let mut cursor = s;
                    while cursor < e {
                        // Passende Farbspanne am cursor finden:
                        let mut naechste_grenze = e;
                        let mut farbe = tok_color(Tok::CodeBlock);
                        for (fs, fe, f) in &code_farben {
                            if *fs <= cursor && cursor < *fe {
                                farbe = Color32::from_rgba_unmultiplied(f[0], f[1], f[2], f[3]);
                                naechste_grenze = (*fe).min(e);
                                break;
                            }
                        }
                        if naechste_grenze == e && farbe == tok_color(Tok::CodeBlock) {
                            // cursor liegt zwischen syntect-Spans: bis zur nächsten Spanne
                            let mut grenze = e;
                            for (fs, _fe, f) in &code_farben {
                                if *fs > cursor && *fs < grenze {
                                    grenze = *fs;
                                    farbe = Color32::from_rgba_unmultiplied(f[0], f[1], f[2], f[3]);
                                }
                            }
                            naechste_grenze = grenze;
                            if grenze == e {
                                farbe = tok_color(Tok::CodeBlock);
                            } else {
                                // Farbe der kommenden Spanne übernehmen:
                                for (fs, fe, f) in &code_farben {
                                    if *fs == grenze {
                                        farbe = Color32::from_rgba_unmultiplied(f[0], f[1], f[2], f[3]);
                                        naechste_grenze = (*fe).min(e);
                                        break;
                                    }
                                }
                            }
                        }
                        if naechste_grenze > cursor {
                            final_stuecke.push((cursor, naechste_grenze, farbe));
                            cursor = naechste_grenze;
                        } else {
                            break; // Sicherheit gegen Endlosschleife
                        }
                    }
                }

                for (s, e, farbe) in final_stuecke {
                    let fmt = egui::TextFormat::simple(
                        egui::FontId::monospace(font_size),
                        farbe,
                    );
                    job.append(&txt[s..e], 0.0, fmt);
                }
                job.wrap.max_width = wrap_width;
                ui.fonts_mut(|f| f.layout_job(job))
            };

        let text_len = text.len();
        let editor = egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .layouter(&mut layouter)
            .desired_width(f32::INFINITY)
            .id(editor_id);

        // Sync-Scroll: Editor in ScrollArea; der Stand wird als Anteil 0..1
        // gespeichert und von ui_preview auf die Vorschau übertragen.
        let scroll_id = egui::Id::new(("editor_scroll", path.clone()));
        let mut geaendert = false;
        let scroll_out = egui::ScrollArea::vertical()
            .id_salt(scroll_id)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let te_out = editor.show(ui);
                let editor_resp = te_out.response.clone();
                if self.focus_editor_once {
                    editor_resp.request_focus();
                    self.focus_editor_once = false;
                }

                // ---- Link-Interaktion: Klick + Strg+Hover ----
                self.interact_editor_links(&te_out, text_len);

                geaendert = editor_resp.changed();
            });

        if geaendert {
            if let Some(v) = self.vault.as_mut() {
                let _ = v.set_text(&path, text);
            }
            self.mark_dirty_timer();
        } else if !text.is_empty() {
            // text zurückgeben, damit der Puffer konsistent bleibt (kein Op nötig)
        }

        let anteil_vor_frame = self.sync_last;
        let visible = scroll_out.inner_rect.height();
        if scroll_out.content_size.y > visible && visible > 0.0 {
            self.sync_scroll =
                (scroll_out.state.offset.y / (scroll_out.content_size.y - visible))
                    .clamp(0.0, 1.0);
        }
        self.sync_changed =
            anteil_vor_frame.map_or(true, |vor| (vor - self.sync_scroll).abs() > 0.001);
        self.sync_last = Some(self.sync_scroll);
    }

    /// Sammelt Notizpfad + erste Zeilen für das Hover-Popup (kein UI-Borrow).
    fn hover_preview(&self, target: &str) -> Option<(PathBuf, Vec<String>)> {
        let v = self.vault.as_ref()?;
        let pfade: Vec<PathBuf> = v.notes().iter().map(|n| n.abs.clone()).collect();
        let pfad = markdown::resolve_wikilink(pfade.iter().map(|p| p.as_path()), target)?;
        let inhalt = v
            .buffer(&pfad)
            .map(|b| b.text.clone())
            .or_else(|| std::fs::read_to_string(&pfad).ok())?;
        let (_fm, body) = markdown::split_front_matter(&inhalt);
        let zeilen: Vec<String> = body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(4)
            .map(|l| l.trim_start_matches('#').trim().to_string())
            .collect();
        Some((pfad, zeilen))
    }

    /// Ermittelt den Wikilink unter der Maus: Strg+Hover zeigt ein Popup,
    /// Klick öffnet die Zielnotiz.
    fn interact_editor_links(
        &mut self,
        te_out: &egui::widgets::text_edit::TextEditOutput,
        text_len: usize,
    ) {
        let _ = text_len;
        let text = te_out.galley.text();
        let response = &te_out.response;

        let Some(hover_pos) = response.hover_pos() else {
            self.hover_link = None;
            return;
        };

        // Byte-Offset unter der Maus:
        let galley_pos = te_out.galley_pos;
        let rel = egui::vec2(hover_pos.x - galley_pos.x, hover_pos.y - galley_pos.y);
        let ccursor = te_out.galley.cursor_from_pos(rel);
        let byte_pos = char_to_byte(text, ccursor.index.into());

        let link = rusty_notes::editor_links::wikilink_at(text, byte_pos);

        // Strg+Hover → Popup merken
        let ctrl = response.ctx.input(|i| i.modifiers.ctrl);
        if ctrl {
            if let Some(l) = &link {
                self.hover_link = Some((l.target.clone(), hover_pos));
            } else {
                self.hover_link = None;
            }
        } else {
            self.hover_link = None;
        }

        // Strg+Klick oder Mittelklick öffnet den Link unter der Maus.
        let clicked = response.clicked_by(egui::PointerButton::Secondary)
            || (ctrl && response.clicked_by(egui::PointerButton::Primary));
        if clicked {
            if let Some(l) = link {
                if let Some(pfad) = self.note_for_wikilink(&l.target) {
                    self.pending_link = Some(pfad);
                } else {
                    self.status = format!("Ziel '{}' nicht gefunden", l.target);
                }
            }
        }
    }

    fn ui_preview(&mut self, ui: &mut egui::Ui) {
        let txt = self.texts();
        let Some(path) = self.active.clone() else {
            return;
        };
        let Some(v) = self.vault.as_ref() else {
            return;
        };
        let Some(buf) = v.buffer(&path) else {
            ui.weak("(no buffer)");
            return;
        };

        let (fm_raw, body) = markdown::split_front_matter(&buf.text);

        // Wikilink-Ziele einmalig auflösen (vor dem UI-Block, kein Borrow-Konflikt).
        let ziele: Vec<String> = markdown::extract_wikilinks(body);
        let ziel_pfade: Vec<(String, PathBuf)> = ziele
            .iter()
            .filter_map(|z| {
                let (target, _) = z.split_once('|').unwrap_or((z.as_str(), ""));
                let target = target.trim().to_string();
                self.note_for_wikilink(&target).map(|p| (target, p))
            })
            .collect();

        // Sync-Scroll: Vorschau übernimmt den Anteil des Editors, solange aktiv
        // und der Editor seit letztem Frame seinen Anteil geändert hat.
        let anteil = self.sync_scroll;
        let mut ziel_offset = None;
        if self.settings.values.sync_scroll && self.sync_changed {
            if let Some((content, visible)) = self.preview_measure.last {
                ziel_offset = Some(anteil * (content - visible).max(0.0));
            }
        }
        let mut preview_builder = egui::ScrollArea::vertical().id_salt("preview_scroll");
        if let Some(target) = ziel_offset {
            preview_builder = preview_builder.vertical_scroll_offset(target);
        }
        let scroll_out = preview_builder
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(raw) = fm_raw {
                    let fm = markdown::parse_front_matter(raw);
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(txt.front_matter);
                        if let Some(title) = fm.title {
                            ui.label(format!("{} {}", txt.title_label, title));
                        }
                        if !fm.tags.is_empty() {
                            ui.label(format!("{} {}", txt.tags_label, fm.tags.join(", ")));
                        }
                    });
                    ui.separator();
                }
                // Wikilinks zu klickbaren Links umschreiben; Ziele als Hooks
                // registrieren, damit Klicks keine Shell auslösen.
                let konvertiert = markdown::wikilinks_to_md_links(body);
                for (target, _) in &ziel_pfade {
                    self.cache.add_link_hook(format!("rusty-note:{}", target));
                }
                egui_commonmark::CommonMarkViewer::new().show(ui, &mut self.cache, &konvertiert);

                // Geklickte Hooks abfragen:
                for (target, pfad) in &ziel_pfade {
                    let schema = format!("rusty-note:{}", target);
                    if self.cache.get_link_hook(&schema) == Some(true) {
                        self.cache.remove_link_hook(&schema);
                        self.pending_link = Some(pfad.clone());
                    }
                }

                // Klickbare Glossary-Verweise (virtuelle Links dieser Notiz):
                if let Some(g) = &self.glossary {
                    if self.settings.values.glossary_enabled
                        && self.settings.values.glossary_preview_list
                        && !g.is_empty()
                    {
                        let mut hits = g.find(body);
                        hits.truncate(self.settings.values.glossary_max_hits);
                        if !hits.is_empty() {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.weak(txt.glossary_refs_heading);
                            ui.horizontal_wrapped(|ui| {
                                // Duplikate nach Eintrags-Index zusammenfassen
                                let mut seen: Vec<usize> = Vec::new();
                                for tr in &hits {
                                    if !seen.contains(&tr.index) {
                                        seen.push(tr.index);
                                    }
                                }
                                seen.sort_unstable();
                                let entries = g.entries();
                                for idx in seen {
                                    let e = &entries[idx];
                                    if ui
                                        .link(egui::RichText::new(&e.term).underline())
                                        .clicked()
                                    {
                                        self.pending_link = Some(e.path.clone());
                                    }
                                }
                            });
                        }
                    }
                }
            });

        // Maße für den nächsten Sync-Merker:
        self.preview_measure.last = Some((
            scroll_out.content_size.y,
            scroll_out.inner_rect.height(),
        ));
    }
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

impl App {
    fn draw_overlay(&mut self, ctx: &egui::Context) {
        let txt = self.texts();
        match std::mem::replace(&mut self.overlay, Overlay::None) {
            Overlay::None => {}
            Overlay::Switcher => {
                let mut keep_open = true;
                let mut done = false;
                egui::Window::new(txt.quick_switcher)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.set_min_width(420.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.switcher_query)
                                .hint_text(txt.type_to_filter)
                                .desired_width(f32::INFINITY),
                        );
                        resp.request_focus();

                        let stems: Vec<String> = self
                            .vault
                            .as_ref()
                            .map(|v| {
                                v.notes()
                                    .iter()
                                    .map(|n| n.rel.trim_end_matches(".md").to_string())
                                    .collect()
                            })
                            .unwrap_or_default();
                        let ranked = search::quick_switcher(
                            stems.iter().map(|s| s.as_str()),
                            &self.switcher_query,
                        );

                        let (enter, esc, up, down) = ui.input(|i| {
                            (
                                i.key_pressed(Key::Enter),
                                i.key_pressed(Key::Escape),
                                i.key_pressed(Key::ArrowUp),
                                i.key_pressed(Key::ArrowDown),
                            )
                        });

                        if down {
                            self.switcher_selected =
                                (self.switcher_selected + 1).min(ranked.len().saturating_sub(1));
                        }
                        if up {
                            self.switcher_selected = self.switcher_selected.saturating_sub(1);
                        }

                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for (i, cand) in ranked.iter().enumerate().take(50) {
                                    let selected = i == self.switcher_selected;
                                    if ui.selectable_label(selected, &cand.rel).clicked()
                                        || (selected && enter)
                                    {
                                        if let Some(v) = self.vault.as_ref() {
                                            let abs = v
                                                .root()
                                                .join(format!("{}.md", cand.rel));
                                            self.pending_link = Some(abs);
                                        }
                                        done = true;
                                    }
                                }
                                if ranked.is_empty() {
                                    ui.weak(txt.no_matching_notes);
                                }
                            });

                        if esc || done {
                            keep_open = false;
                        }
                    });
                if !keep_open {
                    self.overlay = Overlay::None;
                } else {
                    self.overlay = Overlay::Switcher;
                }
            }
            Overlay::CommandPalette => {
                let mut keep_open = true;
                let mut executed: Option<&'static str> = None;
                egui::Window::new(txt.command_palette)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.set_min_width(360.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.switcher_query)
                                .hint_text(txt.type_a_command)
                                .desired_width(f32::INFINITY),
                        );
                        resp.request_focus();

                        let commands: &[(&str, &'static str)] = &[
                            (txt.cmd_toggle_preview, "toggle_preview"),
                            (txt.cmd_new_note, "new_note"),
                            (txt.cmd_save, "save"),
                            (txt.cmd_open_folder, "open_folder"),
                            (txt.cmd_focus_search, "focus_search"),
                            (txt.cmd_settings, "open_settings"),
                        ];
                        let ranked =
                            search::quick_switcher(commands.iter().map(|(n, _)| *n), &self.switcher_query);
                        let (enter, esc) = ui.input(|i| {
                            (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape))
                        });

                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for cand in ranked.iter().take(20) {
                                    if ui.selectable_label(false, &cand.rel).clicked()
                                        || (enter && cand.rel == ranked.first().map(|c| c.rel.clone()).unwrap_or_default() && !ranked.is_empty())
                                    {
                                        executed = commands
                                            .iter()
                                            .find(|(n, _)| *n == cand.rel)
                                            .map(|(_, id)| *id);
                                    }
                                }
                            });
                        if esc {
                            keep_open = false;
                        }
                    });

                if let Some(id) = executed {
                    self.run_command(id);
                    keep_open = false;
                }
                if !keep_open {
                    self.overlay = Overlay::None;
                } else {
                    self.overlay = Overlay::CommandPalette;
                }
            }
            Overlay::Prompt { title, mut value, action } => {
                let mut keep_open = true;
                let mut submitted = false;
                egui::Window::new(&title)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.set_min_width(380.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut value)
                                .desired_width(f32::INFINITY),
                        );
                        resp.request_focus();
                        let (enter, esc) = ui.input(|i| {
                            (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape))
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button(txt.ok).clicked() || enter {
                                submitted = true;
                            }
                            if ui.button(txt.cancel).clicked() || esc {
                                keep_open = false;
                            }
                        });
                    });
                if submitted {
                    self.run_prompt_action(&action, &value);
                }
                self.overlay = if keep_open && !submitted {
                    Overlay::Prompt { title, value, action }
                } else {
                    Overlay::None
                };
            }
            Overlay::Settings { mut draft } => {
                let mut keep_open = true;
                let mut close_dialog = false;
                let mut recording: Option<Action> = None;
                // Dialog follows the draft language for instant preview.
                let txt = i18n::pick_code(&draft.language);
                egui::Window::new(txt.settings_title)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 40.0])
                    .default_size([540.0, 640.0])
                    .resizable(true)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.set_min_width(490.0);

                            ui.heading(txt.general_heading);
                            ui.checkbox(&mut draft.preview_visible, txt.set_preview);
                            ui.horizontal(|ui| {
                                ui.label(txt.set_autosave);
                                ui.add(
                                    egui::DragValue::new(&mut draft.autosave_ms)
                                        .speed(100)
                                        .range(0..=10_000)
                                        .suffix(" ms"),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(txt.language_label);
                                egui::ComboBox::from_id_salt("language")
                                    .selected_text(if draft.language == "de" {
                                        "Deutsch"
                                    } else {
                                        "English"
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut draft.language,
                                            "en".to_string(),
                                            "English",
                                        );
                                        ui.selectable_value(
                                            &mut draft.language,
                                            "de".to_string(),
                                            "Deutsch",
                                        );
                                    });
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.heading(txt.editor_heading);
                            ui.horizontal(|ui| {
                                ui.label(txt.font_size_label);
                                ui.add(
                                    egui::DragValue::new(&mut draft.editor_font_size)
                                        .speed(0.5)
                                        .range(8.0..=32.0)
                                        .suffix(" px"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.heading(txt.glossary_heading);
                            ui.checkbox(&mut draft.glossary_enabled, txt.set_glossary);
                            ui.add_enabled_ui(draft.glossary_enabled, |ui| {
                                ui.checkbox(
                                    &mut draft.glossary_case_insensitive,
                                    txt.set_glossary_ci,
                                );
                                ui.checkbox(
                                    &mut draft.glossary_preview_list,
                                    txt.show_refs_below,
                                );
                                ui.horizontal(|ui| {
                                    ui.label(txt.set_glossary_max);
                                    ui.add(
                                        egui::DragValue::new(&mut draft.glossary_max_hits)
                                            .speed(10)
                                            .range(10..=10_000),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(txt.min_term_length);
                                    ui.add(
                                        egui::DragValue::new(&mut draft.glossary_min_len)
                                            .range(1..=30),
                                    );
                                });
                                ui.label(txt.glossary_folders_hint);
                                let mut folders_text = draft.glossary_folders.join("\n");
                                let resp = ui.add(
                                    egui::TextEdit::multiline(&mut folders_text)
                                        .desired_rows(3)
                                        .desired_width(240.0),
                                );
                                if resp.changed() {
                                    draft.glossary_folders = folders_text
                                        .lines()
                                        .map(|l| l.trim().trim_matches('/').to_string())
                                        .filter(|l| !l.is_empty())
                                        .collect();
                                }
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.heading(txt.shortcuts_heading);
                            ui.label(txt.shortcuts_hint);
                            egui::Grid::new("keybind_grid")
                                .num_columns(3)
                                .spacing([12.0, 4.0])
                                .show(ui, |ui| {
                                    let dialog_lang =
                                        Language::from_code(&draft.language);
                                    for action in Action::ALL {
                                        let current = draft
                                            .binding_for(*action)
                                            .map(|b| b.display_in(dialog_lang))
                                            .unwrap_or_else(|| "\u{2013}".into());
                                        ui.label(action.label(txt));
                                        ui.monospace(current);
                                        if ui.button(txt.change_button).clicked() {
                                            recording = Some(*action);
                                        }
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(8.0);
                            if ui.button(txt.set_close).clicked() {
                                close_dialog = true;
                            }
                        });
                    });

                // Tastenaufzeichnung:
                if let Some(action) = recording {
                    self.recording_keybind = Some(action);
                }
                if let Some(action) = self.recording_keybind {
                    let captured = ctx.input(|i| {
                        let mods = i.modifiers;
                        for key in [
                            Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G,
                            Key::H, Key::I, Key::J, Key::K, Key::L, Key::M, Key::N,
                            Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U,
                            Key::V, Key::W, Key::X, Key::Y, Key::Z,
                            Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6,
                            Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12,
                            Key::ArrowDown, Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight,
                            Key::Enter, Key::Escape, Key::Tab, Key::Space,
                        ] {
                            if i.key_pressed(key) {
                                return Some((mods, key));
                            }
                        }
                        None
                    });
                    if let Some((mods, key)) = captured {
                        if key != Key::Escape {
                            draft.set_binding(
                                action,
                                Some(Keybind::new(mods.ctrl, mods.shift, mods.alt, key_name(key))),
                            );
                        }
                        self.recording_keybind = None;
                    } else {
                        self.recording_keybind = Some(action);
                    }
                }

                if close_dialog {
                    if draft != self.settings.values {
                        let glossar_neu = draft.glossary_enabled
                            != self.settings.values.glossary_enabled
                            || draft.glossary_case_insensitive
                                != self.settings.values.glossary_case_insensitive
                            || draft.glossary_folders != self.settings.values.glossary_folders
                            || draft.glossary_min_len != self.settings.values.glossary_min_len;
                        self.settings.values = draft.clone();
                        self.settings.mark_dirty();
                        if glossar_neu {
                            self.refresh_glossary();
                        }
                    }
                    self.recording_keybind = None;
                    keep_open = false;
                }
                self.overlay = if keep_open {
                    Overlay::Settings { draft }
                } else {
                    Overlay::None
                };
            }
            Overlay::Confirm { message, action, arg } => {
                let mut keep_open = true;
                let mut result: Option<bool> = None;
                egui::Window::new(txt.confirm)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.label(&message);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button(txt.yes).clicked() {
                                result = Some(true);
                            }
                            if ui.button(txt.no).clicked() {
                                result = Some(false);
                            }
                        });
                    });
                if let Some(yes) = result {
                    if yes {
                        self.run_confirm_action(&action, &arg);
                    }
                    keep_open = false;
                }
                self.overlay = if keep_open {
                    Overlay::Confirm { message, action, arg }
                } else {
                    Overlay::None
                };
            }
        }
    }

    fn run_command(&mut self, id: &str) {
        match id {
            "toggle_preview" => {
                self.preview_visible = !self.preview_visible;
                self.settings.values.preview_visible = self.preview_visible;
                self.settings.mark_dirty();
            }
            "new_note" => self.create_note_flow(),
            "save" => self.save_active(),
            "open_folder" => self.open_folder_dialog_and_load(),
            "focus_search" => self.sidebar_tab = SidebarTab::Search,
            "open_settings" => self.open_settings(),
            _ => {}
        }
    }

    fn open_settings(&mut self) {
        self.overlay = Overlay::Settings {
            draft: self.settings.values.clone(),
        };
    }

    /// Führt eine Action aus (Keybinds, Befehlspalette).
    fn run_action(&mut self, action: Action) {
        let txt = self.texts();
        match action {
            Action::OpenFolder => self.open_folder_dialog_and_load(),
            Action::QuickSwitcher => {
                self.overlay = Overlay::Switcher;
                self.switcher_query.clear();
                self.switcher_selected = 0;
            }
            Action::CommandPalette => {
                self.overlay = Overlay::CommandPalette;
                self.switcher_query.clear();
                self.switcher_selected = 0;
            }
            Action::Save => self.save_active(),
            Action::NewNote => self.create_note_flow(),
            Action::TogglePreview => {
                self.preview_visible = !self.preview_visible;
                self.settings.values.preview_visible = self.preview_visible;
                self.settings.mark_dirty();
            }
            Action::FocusSearch => self.sidebar_tab = SidebarTab::Search,
            Action::OpenSettings => self.open_settings(),
            Action::CloseNote => self.active = None,
            Action::NextNote => self.step_note(1),
            Action::PrevNote => self.step_note(-1),
            Action::ToggleSyncScroll => {
                self.settings.values.sync_scroll = !self.settings.values.sync_scroll;
                self.settings.mark_dirty();
                self.status = if self.settings.values.sync_scroll {
                    txt.sync_scroll_on.into()
                } else {
                    txt.sync_scroll_off.into()
                };
            }
            Action::ToggleGlossary => {
                self.settings.values.glossary_enabled = !self.settings.values.glossary_enabled;
                self.settings.mark_dirty();
                self.refresh_glossary();
                self.status = if self.settings.values.glossary_enabled {
                    txt.glossary_on.into()
                } else {
                    txt.glossary_off.into()
                };
            }
        }
    }

    /// Springt in der sortierten Notizliste vor/zurück.
    fn step_note(&mut self, direction: i32) {
        let Some(v) = self.vault.as_ref() else {
            return;
        };
        let notes: Vec<PathBuf> = v.notes().iter().map(|n| n.abs.clone()).collect();
        if notes.is_empty() {
            return;
        }
        let idx = self
            .active
            .as_ref()
            .and_then(|a| notes.iter().position(|p| p == a))
            .map(|i| i as i32)
            .unwrap_or(-direction);
        let next = (idx + direction).rem_euclid(notes.len() as i32) as usize;
        let target = notes[next].clone();
        self.open_note(target);
    }
}

fn main() -> Result<(), eframe::Error> {
    let mut opts = eframe::NativeOptions::default();
    opts.viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([720.0, 480.0])
        .with_title("rusty-notes");

    eframe::run_native(
        "rusty-notes",
        opts,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
