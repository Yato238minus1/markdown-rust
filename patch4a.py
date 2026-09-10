#!/usr/bin/env python3
"""ui_editor: Link-Klicks, Strg+Hover-Popup, syntect-Code-Highlighting."""
src = open('src/main.rs').read()
def rep(old, new):
    global src
    assert old in src, "FEHLT: " + old[:80].replace("\n", "\\n")
    src = src.replace(old, new, 1)

# ---------- Imports ----------
rep("use rusty_notes::{\n    einstellungen::{Aktion, EinstellungsManager, Einstellungen, Keybind},",
    "use rusty_notes::{\n    code_hervorhebung::CodeHighlighter,\n    einstellungen::{Aktion, EinstellungsManager, Einstellungen, Keybind},")
rep("use std::path::PathBuf;",
    "use std::collections::HashMap;\nuse std::path::PathBuf;\nuse std::sync::LazyLock;")

# ---------- App-Felder für Hover-Popup & Sync-Scroll ----------
rep("""    einst: EinstellungsManager,
    glossar: Option<Glossar>,
    keybind_aufzeichnen: Option<Aktion>,
}""",
"""    einst: EinstellungsManager,
    glossar: Option<Glossar>,
    keybind_aufzeichnen: Option<Aktion>,
    /// Strg+Hover: (Zielnotiz, Bildschirmposition) für das Popup.
    hover_link: Option<(String, egui::Pos2)>,
    /// Sync-Scroll: letzter geteilter Scroll-Stand der Editor-/Vorschau-Ansicht.
    sync_scroll: f32,
}""")
rep("""            einst,
            glossar: None,
            keybind_aufzeichnen: None,
        }""",
"""            einst,
            glossar: None,
            keybind_aufzeichnen: None,
            hover_link: None,
            sync_scroll: 0.0,
        }""")

# ---------- CodeHighlighter als statische Ressource ----------
rep("const GLOSSAR_FARBE: Color32 = Color32::from_rgb(126, 231, 135); // sattes Grün, deutlich von Link-Blau unterscheiden",
    """const GLOSSAR_FARBE: Color32 = Color32::from_rgb(126, 231, 135); // sattes Grün, deutlich von Link-Blau unterscheiden

static CODE_HIGHLIGHTER: LazyLock<CodeHighlighter> = LazyLock::new(CodeHighlighter::default);""")

open('src/main.rs', 'w').write(src)
print("TEIL A OK")
