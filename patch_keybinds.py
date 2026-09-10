#!/usr/bin/env python3
"""Keybind-System: check_shortcuts über Einstellungen; neue Aktionen verdrahten."""
src = open('src/main.rs').read()
def rep(old, new):
    global src
    assert old in src, "FEHLT: " + old[:80].replace("\n", "\\n")
    src = src.replace(old, new, 1)

# ---------- Shortcut-Enum ersetzen: Aktion direkt ----------
rep("""enum Shortcut {
    OpenFolder,
    QuickSwitcher,
    CommandPalette,
    Save,
    NewNote,
    TogglePreview,
    FocusSearch,
    Einstellungen,
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
        } else if mods.ctrl && mods.shift && i.key_pressed(Key::S) {
            out = Some(Shortcut::Einstellungen);
        }
    });
    out
}""",
"""/// Prüft alle Keybinds aus den Einstellungen gegen den Tastaturzustand.
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
}""")

# ---------- Aufrufseite + Dispatch ----------
rep("""        // Global keyboard shortcuts.
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
                Shortcut::Einstellungen => self.einstellungen_oeffnen(),
            }
        }""",
"""        // Tastatur-Shortcuts aus den Einstellungen.
        if let Some(aktion) = check_shortcuts(&ctx, &self.einst.werte.clone(), &self.overlay) {
            self.aktion_ausfuehren(aktion);
        }""")

# ---------- aktion_ausfuehren als Methode ----------
rep("""    fn einstellungen_oeffnen(&mut self) {
        self.overlay = Overlay::Einstellungen {
            entwurf: self.einst.werte.clone(),
        };
    }""",
"""    fn einstellungen_oeffnen(&mut self) {
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
    }""")

open('src/main.rs', 'w').write(src)
print("OK")
