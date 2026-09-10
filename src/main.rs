//! rusty-notes: a fast, keyboard-driven Markdown note editor.

use rusty_notes::{
    einstellungen::{Aktion, EinstellungsManager, Einstellungen, Keybind},
    glossary::{self, Glossar},
    i18n as t, markdown, search, vault,
};

use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Key};
use egui_commonmark::CommonMarkCache;

use rusty_notes::editor::{highlight, Tok};

// ---------------------------------------------------------------------------
// Einstellungen (persistiert ueber EinstellungsManager in der Bibliothek)
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
    Einstellungen {
        /// Arbeitskopie; wird beim Schließen übernommen (Dirty nur bei Diff).
        entwurf: Einstellungen,
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
    einst: EinstellungsManager,
    glossar: Option<Glossar>,
    keybind_aufzeichnen: Option<Aktion>,
}

impl Default for App {
    fn default() -> Self {
        let einst = EinstellungsManager::laden();
        let last_vault = einst.werte.last_vault.clone();
        let vorschau = einst.werte.vorschau_sichtbar;
        Self {
            vault: None,
            active: None,
            preview_visible: vorschau,
            sidebar_tab: SidebarTab::Notes,
            search_query: String::new(),
            search_results: Vec::new(),
            switcher_query: String::new(),
            switcher_selected: 0,
            overlay: Overlay::None,
            status: format!("{} (Strg+O)", t::OPEN_FOLDER),
            dirty_at: None,
            cache: CommonMarkCache::default(),
            pending_link: None,
            focus_editor_once: false,
            einst,
            glossar: None,
            keybind_aufzeichnen: None,
        }
        .with_last_vault(last_vault)
    }
}

impl App {
    fn with_last_vault(mut self, last: Option<String>) -> Self {
        if let Some(v) = last {
            self.status = t::status_opening(&v);
            match open_vault(&v) {
                Ok(mut vault) => {
                    vault.scan().ok();
                    let first = vault.notes().first().map(|n| n.abs.clone());
                    self.vault = Some(vault);
                    self.glossar_erneuern();
                    if let Some(f) = first {
                        self.open_note(f);
                    }
                    self.status = t::status_vault(&v);
                }
                Err(e) => {
                    self.status = t::status_reopen_failed(&v, &e.to_string());
                }
            }
        }
        self
    }

    /// Löst einen Wikilink-Text ([[Ziel]]) auf einen existierenden Notizpfad auf.
    fn notiz_fuer_wikilink(&self, ziel: &str) -> Option<PathBuf> {
        let v = self.vault.as_ref()?;
        let pfade: Vec<PathBuf> = v.notes().iter().map(|n| n.abs.clone()).collect();
        markdown::resolve_wikilink(pfade.iter().map(|p| p.as_path()), ziel)
    }

    /// Glossar aus allen Notiz-Stems neu aufbauen (nur bei Vault-Änderung).
    fn glossar_erneuern(&mut self) {
        if !self.einst.werte.glossar_aktiv {
            self.glossar = None;
            return;
        }
        let ordner = self.einst.werte.glossar_ordner.clone();
        if let Some(v) = self.vault.as_mut() {
            let notizen = v.notizen_aus(&ordner);
            let eintraege: Vec<glossary::GlossarEintrag> = notizen
                .iter()
                .map(|abs| {
                    // Aliase aus Front-Matter der Notiz lesen (Cache!).
                    let aliase = v
                        .content_for(abs)
                        .ok()
                        .map(|c| {
                            markdown::parse_front_matter(
                                markdown::split_front_matter(&c).0.unwrap_or(""),
                            )
                            .aliases
                        })
                        .unwrap_or_default();
                    glossary::GlossarEintrag {
                        begriff: abs
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string(),
                        aliase,
                        pfad: abs.clone(),
                    }
                })
                .collect();
            self.glossar = Some(Glossar::neu(
                eintraege,
                self.einst.werte.glossar_case_insensitive,
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
        match open_vault(&dir.to_string_lossy()) {
            Ok(mut v) => {
                v.scan().ok();
                let first = v.notes().first().map(|n| n.abs.clone());
                self.active = None;
                self.vault = Some(v);
                self.glossar_erneuern();
                if let Some(f) = first {
                    self.open_note(f);
                }
                self.status = t::status_vault(&dir.display().to_string());
                self.einst
                    .setze_last_vault(Some(dir.to_string_lossy().into_owned()));
            }
            Err(e) => {
                self.status =
                    t::status_open_failed_path(&dir.display().to_string(), &e.to_string());
            }
        }
    }

    fn open_note(&mut self, abs: PathBuf) {
        if let Some(v) = self.vault.as_mut() {
            if v.open_note(&abs).is_ok() {
                self.active = Some(abs);
                self.focus_editor_once = true;
            } else {
                self.status = t::status_open_failed();
            }
        }
    }

    fn save_active(&mut self) {
        if let (Some(v), Some(active)) = (self.vault.as_mut(), &self.active) {
            match v.save(active) {
                Ok(()) => self.status = t::status_saved(),
                Err(e) => self.status = t::status_save_failed(&e.to_string()),
            }
        }
    }

    fn create_note_flow(&mut self) {
        self.overlay = Overlay::Prompt {
            title: t::NEW_NOTE_PROMPT.into(),
            value: String::new(),
            action: PromptAction::NewNote,
        };
    }

    fn rename_note_flow(&mut self, path: PathBuf) {
        let default_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        self.overlay = Overlay::Prompt {
            title: t::RENAME_PROMPT.into(),
            value: default_name,
            action: PromptAction::Rename(path),
        };
    }

    fn run_prompt_action(&mut self, action: &PromptAction, value: &str) {
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
                            self.glossar_erneuern();
                            self.status = t::status_created(&rel);
                        }
                        Err(e) => self.status = t::status_create_failed(&e.to_string()),
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
                            self.glossar_erneuern();
                            self.status = t::status_renamed(&new_rel);
                        }
                        Err(e) => self.status = t::status_rename_failed(&e.to_string()),
                    }
                }
            }
        }
    }

    fn run_confirm_action(&mut self, action: &ConfirmAction, arg: &std::path::Path) {
        match action {
            ConfirmAction::Delete => {
                if let Some(v) = self.vault.as_mut() {
                    match v.delete_note(arg) {
                        Ok(()) => {
                            if self.active.as_deref() == Some(arg) {
                                self.active = None;
                            }
                            self.glossar_erneuern();
                            self.status = t::status_deleted();
                        }
                        Err(e) => self.status = t::status_delete_failed(&e.to_string()),
                    }
                }
            }
        }
    }

    fn autosave_tick(&mut self) {
        if let Some(t) = self.dirty_at {
            if t.elapsed() >= std::time::Duration::from_millis(self.einst.werte.autosave_ms) {
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

const GLOSSAR_FARBE: Color32 = Color32::from_rgb(126, 231, 135); // sattes Grün, deutlich von Link-Blau unterscheiden

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
        let ctx = ui.ctx().clone();
        self.autosave_tick();

        // Tastatur-Shortcuts aus den Einstellungen.
        if let Some(aktion) = check_shortcuts(&ctx, &self.einst.werte.clone(), &self.overlay) {
            self.aktion_ausfuehren(aktion);
        }

        // Klick aus der Vorschau: rusty-note:-Schema auf Notizpfad auflösen.
        if let Some(target) = self.pending_link.take() {
            let ziel = if let Some(rest) =
                target.to_str().and_then(|s| s.strip_prefix("rusty-note:"))
            {
                self.notiz_fuer_wikilink(rest)
            } else {
                Some(target)
            };
            match ziel {
                Some(pfad) if pfad.exists() => self.open_note(pfad),
                _ => self.status = t::status_note_not_found(),
            }
        }

        egui::Panel::top("topbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(t::OPEN_FOLDER).clicked() {
                    self.open_folder_dialog_and_load();
                }
                if ui.button(t::NEW_NOTE).clicked() {
                    self.create_note_flow();
                }
                if ui.button(t::SEARCH).clicked() {
                    self.sidebar_tab = SidebarTab::Search;
                }
                if ui
                    .selectable_label(self.preview_visible, t::PREVIEW)
                    .clicked()
                {
                    self.preview_visible = !self.preview_visible;
                    self.einst.werte.vorschau_sichtbar = self.preview_visible;
                    self.einst.markiere_dirty();
                }
                if ui.button(t::SETTINGS).clicked() {
                    self.einstellungen_oeffnen();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(t::status_notes_count(
                        self.vault.as_ref().map(|v| v.notes().len()).unwrap_or(0),
                    ));
                });
            });
        });

        egui::Panel::bottom("statusbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let (Some(v), Some(a)) = (&self.vault, &self.active) {
                        if let Some(b) = v.buffer(a) {
                            if b.dirty {
                                ui.colored_label(Color32::YELLOW, t::UNSAVED);
                            } else {
                                ui.weak(t::SAVED);
                            }
                        }
                    }
                });
            });
        });

        egui::Panel::left("sidebar")
            .default_size(240.0)
            .resizable(true)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.sidebar_tab, SidebarTab::Notes, t::NOTES);
                    ui.selectable_value(&mut self.sidebar_tab, SidebarTab::Search, t::SEARCH);
                });
                ui.separator();
                match self.sidebar_tab {
                    SidebarTab::Notes => self.ui_notes_list(ui),
                    SidebarTab::Search => self.ui_search_panel(ui),
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.vault.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("rusty-notes");
                    ui.weak(t::A_FAST_MARKDOWN_EDITOR);
                    if ui.button(t::OPEN_FOLDER_DOTS).clicked() {
                        self.open_folder_dialog_and_load();
                    }
                });
                return;
            }
            let has_active = self.active.is_some();
            if !has_active {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.weak(t::SELECT_OR_CREATE);
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
                    .show_inside(ui, |ui| {
                        self.ui_preview(ui);
                    });
                // remaining area:
                self.ui_editor(ui);
            }
        });

        self.draw_overlay(&ctx);

        // Einstellungen nur bei tatsächlichen Änderungen wegschreiben.
        if self.einst.speichern_wenn_noetig() {
            // gespeichert — nichts weiter zu tun
        }

        // Keep repainting while an autosave is pending so it fires promptly.
        if self.dirty_at.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// Prüft alle Keybinds aus den Einstellungen gegen den Tastaturzustand.
/// Aufbau der Lookup-Map: O(Keybinds) pro Frame, Lookup O(1) pro Taste.
fn check_shortcuts(
    ctx: &egui::Context,
    werte: &Einstellungen,
    overlay: &Overlay,
) -> Option<Aktion> {
    if !matches!(overlay, Overlay::None) {
        return None; // Overlays schlucken Shortcuts
    }
    let binds: Vec<(Aktion, Keybind)> = Aktion::ALLE
        .iter()
        .filter_map(|(a, _)| werte.bind_fuer(*a).map(|b| (*a, b)))
        .collect();

    ctx.input(|i| {
        let mods = i.modifiers;
        for (aktion, bind) in binds {
            let ctrl_ok = bind.ctrl == mods.ctrl;
            let shift_ok = bind.shift == mods.shift;
            let alt_ok = bind.alt == mods.alt;
            if ctrl_ok && shift_ok && alt_ok {
                if let Some(key) = egui_key(&bind.taste) {
                    if i.key_pressed(key) {
                        return Some(aktion);
                    }
                }
            }
        }
        None
    })
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
        let Some(v) = self.vault.as_ref() else {
            ui.weak(t::NO_VAULT);
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
                    if ui.button(t::RENAME).clicked() {
                        to_rename = Some(n.abs.clone());
                        ui.close();
                    }
                    if ui.button(t::DELETE).clicked() {
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
                message: t::CONFIRM_DELETE.into(),
                action: ConfirmAction::Delete,
                arg: p,
            };
        }
    }

    fn ui_search_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        let changed = ui
            .add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text(t::SEARCH_ALL_NOTES)
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
                ui.weak(t::NO_MATCHES);
            }
        });
    }

    fn ui_editor(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.active.clone() else {
            return;
        };
        let text_now = self
            .vault
            .as_ref()
            .and_then(|v| v.buffer(&path))
            .map(|b| b.text.clone());
        let Some(mut text) = text_now else {
            ui.weak("(file no longer exists)");
            return;
        };

        // Glossar-Treffer einmal pro Frame berechnen (Aho-Corasick, schnell).
        let glossar_aktiv = self.einst.werte.glossar_aktiv;
        let glossar_max = self.einst.werte.glossar_max_treffer;
        let min_laenge = self.einst.werte.glossar_min_laenge;
        let schrift_groesse = self.einst.werte.editor_schriftgroesse;
        let glossar_treffer: Vec<glossary::GlossarTreffer> = match (&self.glossar, glossar_aktiv) {
            (Some(g), true) if !g.ist_leer() => {
                let mut treffer: Vec<glossary::GlossarTreffer> = g
                    .finde(&text)
                    .into_iter()
                    .filter(|tr| tr.end - tr.start >= min_laenge * 2 || {
                        // min_laenge zählt Zeichen; Byte-Länge kann abweichen (UTF-8)
                        let s = &text[tr.start..tr.end];
                        s.chars().count() >= min_laenge
                    })
                    .collect();
                treffer.truncate(glossar_max);
                treffer
            }
            _ => Vec::new(),
        };

        let mut layouter =
            move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let txt = buf.as_str();
                let spans = highlight(txt);
                let stuecke: Vec<(usize, usize, glossary::Stil)> =
                    glossary::verschneide(&spans, &glossar_treffer, txt.len());
                let mut job = egui::text::LayoutJob::default();
                for (s, e, stil) in stuecke {
                    let farbe = match stil {
                        glossary::Stil::Token(tok) => tok_color(tok),
                        glossary::Stil::Glossar => GLOSSAR_FARBE,
                    };
                    let fmt = egui::TextFormat::simple(
                        egui::FontId::monospace(schrift_groesse),
                        farbe,
                    );
                    job.append(&txt[s..e], 0.0, fmt);
                }
                job.wrap.max_width = wrap_width;
                ui.fonts_mut(|f| f.layout_job(job))
            };

        let editor = egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .layouter(&mut layouter)
            .desired_width(f32::INFINITY);
        let editor_resp = ui.add(editor);
        if self.focus_editor_once {
            editor_resp.request_focus();
            self.focus_editor_once = false;
        }
        let edited = editor_resp.changed();

        if edited {
            if let Some(v) = self.vault.as_mut() {
                let _ = v.set_text(&path, text);
            }
            self.mark_dirty_timer();
        }
    }

    fn ui_preview(&mut self, ui: &mut egui::Ui) {
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
                let (ziel, _) = z.split_once('|').unwrap_or((z.as_str(), ""));
                let ziel = ziel.trim().to_string();
                self.notiz_fuer_wikilink(&ziel).map(|p| (ziel, p))
            })
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("preview_scroll")
            .show(ui, |ui| {
                if let Some(raw) = fm_raw {
                    let fm = markdown::parse_front_matter(raw);
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(t::FRONT_MATTER);
                        if let Some(titel) = fm.title {
                            ui.label(format!("{} {}", t::TITLE_LABEL, titel));
                        }
                        if !fm.tags.is_empty() {
                            ui.label(format!("{} {}", t::TAGS_LABEL, fm.tags.join(", ")));
                        }
                    });
                    ui.separator();
                }
                // Wikilinks zu klickbaren Links umschreiben; Ziele als Hooks
                // registrieren, damit Klicks keine Shell auslösen.
                let konvertiert = markdown::wikilinks_zu_md_links(body);
                for (ziel, _) in &ziel_pfade {
                    self.cache.add_link_hook(format!("rusty-note:{}", ziel));
                }
                egui_commonmark::CommonMarkViewer::new().show(ui, &mut self.cache, &konvertiert);

                // Geklickte Hooks abfragen:
                for (ziel, pfad) in &ziel_pfade {
                    let schema = format!("rusty-note:{}", ziel);
                    if self.cache.get_link_hook(&schema) == Some(true) {
                        self.cache.remove_link_hook(&schema);
                        self.pending_link = Some(pfad.clone());
                    }
                }

                // Klickbare Glossar-Verweise (virtuelle Links dieser Notiz):
                if let Some(g) = &self.glossar {
                    if self.einst.werte.glossar_aktiv
                        && self.einst.werte.glossar_vorschau_liste
                        && !g.ist_leer()
                    {
                        let mut treffer = g.finde(body);
                        treffer.truncate(self.einst.werte.glossar_max_treffer);
                        if !treffer.is_empty() {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.weak("Glossar:");
                            ui.horizontal_wrapped(|ui| {
                                // Duplikate nach Eintrags-Index zusammenfassen
                                let mut gesehen: Vec<usize> = Vec::new();
                                for tr in &treffer {
                                    if !gesehen.contains(&tr.index) {
                                        gesehen.push(tr.index);
                                    }
                                }
                                gesehen.sort_unstable();
                                let eintraege = g.eintraege();
                                for idx in gesehen {
                                    let e = &eintraege[idx];
                                    if ui
                                        .link(egui::RichText::new(&e.begriff).underline())
                                        .clicked()
                                    {
                                        self.pending_link = Some(e.pfad.clone());
                                    }
                                }
                            });
                        }
                    }
                }
            });
    }
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

impl App {
    fn draw_overlay(&mut self, ctx: &egui::Context) {
        match std::mem::replace(&mut self.overlay, Overlay::None) {
            Overlay::None => {}
            Overlay::Switcher => {
                let mut keep_open = true;
                let mut done = false;
                egui::Window::new(t::QUICK_SWITCHER)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.set_min_width(420.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.switcher_query)
                                .hint_text(t::TYPE_TO_FILTER)
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
                                    ui.weak(t::NO_MATCHING_NOTES);
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
                egui::Window::new(t::COMMAND_PALETTE)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.set_min_width(360.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.switcher_query)
                                .hint_text(t::TYPE_A_COMMAND)
                                .desired_width(f32::INFINITY),
                        );
                        resp.request_focus();

                        let commands: &[(&str, &'static str)] = &[
                            (t::CMD_TOGGLE_PREVIEW, "toggle_preview"),
                            (t::CMD_NEW_NOTE, "new_note"),
                            (t::CMD_SAVE, "save"),
                            (t::CMD_OPEN_FOLDER, "open_folder"),
                            (t::CMD_FOCUS_SEARCH, "focus_search"),
                            (t::CMD_SETTINGS, "open_settings"),
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
                            if ui.button(t::OK).clicked() || enter {
                                submitted = true;
                            }
                            if ui.button(t::CANCEL).clicked() || esc {
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
            Overlay::Einstellungen { mut entwurf } => {
                let mut keep_open = true;
                let mut schliessen = false;
                let mut aufzeichnen: Option<Aktion> = None;
                egui::Window::new(t::SETTINGS_TITLE)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 40.0])
                    .default_size([540.0, 640.0])
                    .resizable(true)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.set_min_width(490.0);

                            ui.heading("Allgemein");
                            ui.checkbox(&mut entwurf.vorschau_sichtbar, t::SET_PREVIEW);
                            ui.horizontal(|ui| {
                                ui.label(t::SET_AUTOSAVE);
                                ui.add(
                                    egui::DragValue::new(&mut entwurf.autosave_ms)
                                        .speed(100)
                                        .range(0..=10_000)
                                        .suffix(" ms"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.heading("Editor");
                            ui.horizontal(|ui| {
                                ui.label("Schriftgröße:");
                                ui.add(
                                    egui::DragValue::new(&mut entwurf.editor_schriftgroesse)
                                        .speed(0.5)
                                        .range(8.0..=32.0)
                                        .suffix(" px"),
                                );
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.heading("Glossar");
                            ui.checkbox(&mut entwurf.glossar_aktiv, t::SET_GLOSSAR);
                            ui.add_enabled_ui(entwurf.glossar_aktiv, |ui| {
                                ui.checkbox(
                                    &mut entwurf.glossar_case_insensitive,
                                    t::SET_GLOSSAR_CI,
                                );
                                ui.checkbox(
                                    &mut entwurf.glossar_vorschau_liste,
                                    "Verweise unter der Vorschau anzeigen",
                                );
                                ui.horizontal(|ui| {
                                    ui.label(t::SET_GLOSSAR_MAX);
                                    ui.add(
                                        egui::DragValue::new(&mut entwurf.glossar_max_treffer)
                                            .speed(10)
                                            .range(10..=10_000),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Minimale Begriffslänge:");
                                    ui.add(
                                        egui::DragValue::new(&mut entwurf.glossar_min_laenge)
                                            .range(1..=30),
                                    );
                                });
                                ui.label("Glossar-Unterordner (einer pro Zeile, leer = gesamter Vault):");
                                let mut ordner_text = entwurf.glossar_ordner.join("\n");
                                let resp = ui.add(
                                    egui::TextEdit::multiline(&mut ordner_text)
                                        .desired_rows(3)
                                        .desired_width(240.0),
                                );
                                if resp.changed() {
                                    entwurf.glossar_ordner = ordner_text
                                        .lines()
                                        .map(|l| l.trim().trim_matches('/').to_string())
                                        .filter(|l| !l.is_empty())
                                        .collect();
                                }
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.heading("Tastenkürzel");
                            ui.label("Klick auf \u{201e}\u{c4}ndern\u{201c}, dann Taste drücken. Escape bricht ab.");
                            egui::Grid::new("keybind_grid")
                                .num_columns(3)
                                .spacing([12.0, 4.0])
                                .show(ui, |ui| {
                                    for (aktion, name) in Aktion::ALLE {
                                        let aktuell = entwurf
                                            .bind_fuer(*aktion)
                                            .map(|b| b.als_text())
                                            .unwrap_or_else(|| "\u{2013}".into());
                                        ui.label(*name);
                                        ui.monospace(aktuell);
                                        if ui.button("\u{c4}ndern").clicked() {
                                            aufzeichnen = Some(*aktion);
                                        }
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(8.0);
                            if ui.button(t::SET_CLOSE).clicked() {
                                schliessen = true;
                            }
                        });
                    });

                // Tastenaufzeichnung:
                if let Some(aktion) = aufzeichnen {
                    self.keybind_aufzeichnen = Some(aktion);
                }
                if let Some(aktion) = self.keybind_aufzeichnen {
                    let erfasst = ctx.input(|i| {
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
                    if let Some((mods, key)) = erfasst {
                        if key != Key::Escape {
                            entwurf.setze_bind(
                                aktion,
                                Some(Keybind::neu(mods.ctrl, mods.shift, mods.alt, key_name(key))),
                            );
                        }
                        self.keybind_aufzeichnen = None;
                    } else {
                        self.keybind_aufzeichnen = Some(aktion);
                    }
                }

                if schliessen {
                    if entwurf != self.einst.werte {
                        let glossar_neu = entwurf.glossar_aktiv
                            != self.einst.werte.glossar_aktiv
                            || entwurf.glossar_case_insensitive
                                != self.einst.werte.glossar_case_insensitive
                            || entwurf.glossar_ordner != self.einst.werte.glossar_ordner
                            || entwurf.glossar_min_laenge != self.einst.werte.glossar_min_laenge;
                        self.einst.werte = entwurf.clone();
                        self.einst.markiere_dirty();
                        if glossar_neu {
                            self.glossar_erneuern();
                        }
                    }
                    self.keybind_aufzeichnen = None;
                    keep_open = false;
                }
                self.overlay = if keep_open {
                    Overlay::Einstellungen { entwurf }
                } else {
                    Overlay::None
                };
            }
            Overlay::Confirm { message, action, arg } => {
                let mut keep_open = true;
                let mut result: Option<bool> = None;
                egui::Window::new(t::CONFIRM)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.label(&message);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button(t::YES).clicked() {
                                result = Some(true);
                            }
                            if ui.button(t::NO).clicked() {
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
                self.einst.werte.vorschau_sichtbar = self.preview_visible;
                self.einst.markiere_dirty();
            }
            "new_note" => self.create_note_flow(),
            "save" => self.save_active(),
            "open_folder" => self.open_folder_dialog_and_load(),
            "focus_search" => self.sidebar_tab = SidebarTab::Search,
            "open_settings" => self.einstellungen_oeffnen(),
            _ => {}
        }
    }

    fn einstellungen_oeffnen(&mut self) {
        self.overlay = Overlay::Einstellungen {
            entwurf: self.einst.werte.clone(),
        };
    }

    /// Führt eine Aktion aus (Keybinds, Befehlspalette).
    fn aktion_ausfuehren(&mut self, aktion: Aktion) {
        match aktion {
            Aktion::OrdnerOeffnen => self.open_folder_dialog_and_load(),
            Aktion::Schnellwechsler => {
                self.overlay = Overlay::Switcher;
                self.switcher_query.clear();
                self.switcher_selected = 0;
            }
            Aktion::Befehlspalette => {
                self.overlay = Overlay::CommandPalette;
                self.switcher_query.clear();
                self.switcher_selected = 0;
            }
            Aktion::Speichern => self.save_active(),
            Aktion::NeueNotiz => self.create_note_flow(),
            Aktion::VorschauUmschalten => {
                self.preview_visible = !self.preview_visible;
                self.einst.werte.vorschau_sichtbar = self.preview_visible;
                self.einst.markiere_dirty();
            }
            Aktion::SucheFokussieren => self.sidebar_tab = SidebarTab::Search,
            Aktion::Einstellungen => self.einstellungen_oeffnen(),
            Aktion::NotizSchliessen => self.active = None,
            Aktion::NaechsteNotiz => self.naechste_notiz(1),
            Aktion::VorherigeNotiz => self.naechste_notiz(-1),
            Aktion::GlossarUmschalten => {
                self.einst.werte.glossar_aktiv = !self.einst.werte.glossar_aktiv;
                self.einst.markiere_dirty();
                self.glossar_erneuern();
                self.status = if self.einst.werte.glossar_aktiv {
                    "Glossar aktiviert".into()
                } else {
                    "Glossar deaktiviert".into()
                };
            }
        }
    }

    /// Springt in der sortierten Notizliste vor/zurück.
    fn naechste_notiz(&mut self, richtung: i32) {
        let Some(v) = self.vault.as_ref() else {
            return;
        };
        let notizen: Vec<PathBuf> = v.notes().iter().map(|n| n.abs.clone()).collect();
        if notizen.is_empty() {
            return;
        }
        let idx = self
            .active
            .as_ref()
            .and_then(|a| notizen.iter().position(|p| p == a))
            .map(|i| i as i32)
            .unwrap_or(-richtung);
        let neu = (idx + richtung).rem_euclid(notizen.len() as i32) as usize;
        let ziel = notizen[neu].clone();
        self.open_note(ziel);
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
