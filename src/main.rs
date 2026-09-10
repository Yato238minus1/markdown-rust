//! rusty-notes: a fast, keyboard-driven Markdown note editor.

pub mod editor;
pub mod markdown;
pub mod search;
pub mod vault;

use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Key};
use egui_commonmark::CommonMarkCache;

use crate::editor::{highlight, Tok};

// ---------------------------------------------------------------------------
// Config persistence
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct Config {
    last_vault: Option<String>,
}

impl Config {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("rusty-notes").join("config.json"))
    }

    fn load() -> Config {
        Config::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        if let Some(p) = Config::path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, json);
            }
        }
    }
}

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
}

impl Default for App {
    fn default() -> Self {
        let cfg = Config::load();
        Self {
            vault: None,
            active: None,
            preview_visible: true,
            sidebar_tab: SidebarTab::Notes,
            search_query: String::new(),
            search_results: Vec::new(),
            switcher_query: String::new(),
            switcher_selected: 0,
            overlay: Overlay::None,
            status: "Open a folder to begin (Ctrl+O)".into(),
            dirty_at: None,
            cache: CommonMarkCache::default(),
            pending_link: None,
        }
        // last_vault is applied in `new()` below
        .with_last_vault(cfg.last_vault)
    }
}

impl App {
    fn with_last_vault(mut self, last: Option<String>) -> Self {
        if let Some(v) = last {
            self.status = format!("Opening {} …", v);
            match open_vault(&v) {
                Ok(mut vault) => {
                    vault.scan().ok();
                    let first = vault.notes().first().map(|n| n.abs.clone());
                    self.vault = Some(vault);
                    if let Some(f) = first {
                        self.open_note(f);
                    }
                    self.status = format!("Vault: {}", v);
                }
                Err(e) => {
                    self.status = format!("Could not reopen {}: {}", v, e);
                }
            }
        }
        self
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
                if let Some(f) = first {
                    self.open_note(f);
                }
                self.status = format!("Vault: {}", dir.display());
                Config { last_vault: Some(dir.to_string_lossy().into_owned()) }.save();
            }
            Err(e) => {
                self.status = format!("Failed to open {}: {}", dir.display(), e);
            }
        }
    }

    fn open_note(&mut self, abs: PathBuf) {
        if let Some(v) = self.vault.as_mut() {
            if v.open_note(&abs).is_ok() {
                self.active = Some(abs);
            } else {
                self.status = "Failed to open note".into();
            }
        }
    }

    fn save_active(&mut self) {
        if let (Some(v), Some(active)) = (self.vault.as_mut(), &self.active) {
            match v.save(active) {
                Ok(()) => self.status = "Saved".into(),
                Err(e) => self.status = format!("Save failed: {}", e),
            }
        }
    }

    fn create_note_flow(&mut self) {
        self.overlay = Overlay::Prompt {
            title: "New note name (folders allowed):".into(),
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
            title: "Rename to:".into(),
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
                            self.status = format!("Created {}", rel);
                        }
                        Err(e) => self.status = format!("Create failed: {}", e),
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
                            self.status = format!("Renamed to {}", new_rel);
                        }
                        Err(e) => self.status = format!("Rename failed: {}", e),
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
                            self.status = "Deleted note".into();
                        }
                        Err(e) => self.status = format!("Delete failed: {}", e),
                    }
                }
            }
        }
    }

    fn autosave_tick(&mut self) {
        if let Some(t) = self.dirty_at {
            if t.elapsed() >= std::time::Duration::from_millis(AUTOSAVE_MS as u64) {
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
        if let Some(v) = self.vault.as_ref() {
            let pairs: Vec<(String, String)> = v
                .notes()
                .iter()
                .filter_map(|n| {
                    let content = v
                        .buffer(&n.abs)
                        .map(|b| b.text.clone())
                        .or_else(|| std::fs::read_to_string(&n.abs).ok())?;
                    Some((n.rel.clone(), content))
                })
                .collect();
            self.search_results = search::search_notes(&pairs, &q);
        }
    }

}

const AUTOSAVE_MS: u64 = 800;

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

/// Zenity folder picker via subprocess (no native dialog crates needed).
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

        // Global keyboard shortcuts.
        if let Some(action) = check_shortcuts(&ctx, &self.overlay) {
            match action {
                Shortcut::OpenFolder => self.open_folder_dialog_and_load(),
                Shortcut::QuickSwitcher => {
                    self.overlay = Overlay::Switcher;
                    self.switcher_query.clear();
                    self.switcher_selected = 0;
                }
                Shortcut::CommandPalette => {
                    self.overlay = Overlay::CommandPalette;
                    self.switcher_query.clear();
                    self.switcher_selected = 0;
                }
                Shortcut::Save => self.save_active(),
                Shortcut::NewNote => self.create_note_flow(),
                Shortcut::TogglePreview => self.preview_visible = !self.preview_visible,
                Shortcut::FocusSearch => {
                    self.sidebar_tab = SidebarTab::Search;
                }
            }
        }

        // A wikilink click from the preview pane opens its target.
        if let Some(target) = self.pending_link.take() {
            if target.exists() {
                self.open_note(target);
            } else {
                self.status = "Note not found".into();
            }
        }

        egui::Panel::top("topbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open Folder").clicked() {
                    self.open_folder_dialog_and_load();
                }
                if ui.button("New Note").clicked() {
                    self.create_note_flow();
                }
                if ui.button("Search").clicked() {
                    self.sidebar_tab = SidebarTab::Search;
                }
                if ui
                    .selectable_label(self.preview_visible, "Preview")
                    .clicked()
                {
                    self.preview_visible = !self.preview_visible;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!(
                        "{} notes",
                        self.vault.as_ref().map(|v| v.notes().len()).unwrap_or(0)
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
                                ui.colored_label(Color32::YELLOW, "● unsaved");
                            } else {
                                ui.weak("saved");
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
                    ui.selectable_value(
                        &mut self.sidebar_tab,
                        SidebarTab::Notes,
                        "Notes",
                    );
                    ui.selectable_value(
                        &mut self.sidebar_tab,
                        SidebarTab::Search,
                        "Search",
                    );
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
                    ui.weak("A fast Markdown editor");
                    if ui.button("Open folder…").clicked() {
                        self.open_folder_dialog_and_load();
                    }
                });
                return;
            }
            let has_active = self.active.is_some();
            if !has_active {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.weak("Select or create a note (Ctrl+N)");
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

        // Keep repainting while an autosave is pending so it fires promptly.
        if self.dirty_at.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

enum Shortcut {
    OpenFolder,
    QuickSwitcher,
    CommandPalette,
    Save,
    NewNote,
    TogglePreview,
    FocusSearch,
}

fn check_shortcuts(ctx: &egui::Context, overlay: &Overlay) -> Option<Shortcut> {
    let mut out = None;
    ctx.input(|i| {
        let mods = i.modifiers;
        if !matches!(overlay, Overlay::None) {
            return; // overlays swallow shortcuts
        }
        if mods.ctrl && i.key_pressed(Key::O) {
            out = Some(Shortcut::OpenFolder);
        } else if mods.ctrl && i.key_pressed(Key::P) {
            out = Some(Shortcut::QuickSwitcher);
        } else if mods.ctrl && i.key_pressed(Key::K) {
            out = Some(Shortcut::CommandPalette);
        } else if mods.ctrl && i.key_pressed(Key::S) {
            out = Some(Shortcut::Save);
        } else if mods.ctrl && i.key_pressed(Key::N) {
            out = Some(Shortcut::NewNote);
        } else if mods.ctrl && i.key_pressed(Key::E) {
            out = Some(Shortcut::TogglePreview);
        } else if mods.ctrl && mods.shift && i.key_pressed(Key::F) {
            out = Some(Shortcut::FocusSearch);
        }
    });
    out
}

// ---------------------------------------------------------------------------
// UI pieces
// ---------------------------------------------------------------------------

impl App {
    fn ui_notes_list(&mut self, ui: &mut egui::Ui) {
        let Some(v) = self.vault.as_ref() else {
            ui.weak("No vault open");
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
                    if ui.button("Rename").clicked() {
                        to_rename = Some(n.abs.clone());
                        ui.close();
                    }
                    if ui.button("Delete").clicked() {
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
                message: "Delete this note? This cannot be undone.".into(),
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
                    .hint_text("Search all notes…")
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
                ui.weak("No matches");
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

        let mut layouter =
            |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let txt = buf.as_str();
                let mut job = egui::text::LayoutJob::default();
                for s in highlight(txt) {
                    let fmt = egui::TextFormat::simple(
                        egui::FontId::monospace(14.0),
                        tok_color(s.tok),
                    );
                    job.append(&txt[s.start..s.end], 0.0, fmt);
                }
                job.wrap.max_width = wrap_width;
                ui.fonts_mut(|f| f.layout_job(job))
            };

        let edited = ui
            .add(
                egui::TextEdit::multiline(&mut text)
                    .font(egui::TextStyle::Monospace)
                    .layouter(&mut layouter)
                    .desired_width(f32::INFINITY),
            )
            .changed();

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

        egui::ScrollArea::vertical()
            .id_salt("preview_scroll")
            .show(ui, |ui| {
                if let Some(raw) = fm_raw {
                    let fm = markdown::parse_front_matter(raw);
                    ui.horizontal_wrapped(|ui| {
                        ui.weak("front matter:");
                        if let Some(t) = fm.title {
                            ui.label(format!("title: {}", t));
                        }
                        if !fm.tags.is_empty() {
                            ui.label(format!("tags: {}", fm.tags.join(", ")));
                        }
                    });
                    ui.separator();
                }
                egui_commonmark::CommonMarkViewer::new().show(ui, &mut self.cache, body);
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
                egui::Window::new("Quick Switcher")
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.set_min_width(420.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.switcher_query)
                                .hint_text("Type to filter notes…")
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
                                    ui.weak("No matching notes");
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
                egui::Window::new("Command Palette")
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.set_min_width(360.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.switcher_query)
                                .hint_text("Type a command…")
                                .desired_width(f32::INFINITY),
                        );
                        resp.request_focus();

                        let commands: &[(&str, &'static str)] = &[
                            ("Toggle live preview", "toggle_preview"),
                            ("New note", "new_note"),
                            ("Save now", "save"),
                            ("Open folder…", "open_folder"),
                            ("Focus search", "focus_search"),
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
                            if ui.button("OK").clicked() || enter {
                                submitted = true;
                            }
                            if ui.button("Cancel").clicked() || esc {
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
            Overlay::Confirm { message, action, arg } => {
                let mut keep_open = true;
                let mut result: Option<bool> = None;
                egui::Window::new("Confirm")
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(&ctx, |ui| {
                        ui.label(&message);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Yes").clicked() {
                                result = Some(true);
                            }
                            if ui.button("No").clicked() {
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
            "toggle_preview" => self.preview_visible = !self.preview_visible,
            "new_note" => self.create_note_flow(),
            "save" => self.save_active(),
            "open_folder" => self.open_folder_dialog_and_load(),
            "focus_search" => self.sidebar_tab = SidebarTab::Search,
            _ => {}
        }
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
