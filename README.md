# rusty-notes

Ein schneller, tastaturgetriebener Markdown-Notiz-Editor in Rust.
Inspiriert von Obsidian (Vault + Wikilinks), Zen Notes (minimale Oberfläche)
und Zettlr (Schreibfokus).

## Wikilinks & Glossar

`[[Ziel]]` und `[[Ziel|Alias]]` werden in der Vorschau als **klickbare
Links** gerendert (über Link-Hooks von egui_commonmark — keine Shell-Aufrufe).
Unbekannte Ziele werden per exakter Stamm-Match, dann Groß-/Klein-insensitivem
Teilstring aufgelöst.

## Glossar (virtuelle Verlinkung)

Angelehnt an das Obsidian-Plugin *Virtual Linker / Glossary*, aber neu
gedacht: Ein **Aho-Corasick-Automat** findet alle Glossar-Begriffe in einem
einzigen Durchlauf (O(Textlänge)), statt jeden Begriff einzeln zu suchen.

Glossar-Quelle ist ein **Unterordner** (Standard `Glossar`, in den
Einstellungen konfigurierbar, mehrere Ordner möglich). Begriffe sind die
Notiz-Namen, zusätzliche Schreibweisen kommen aus dem Front-Matter
(`aliases: [a, b]`). Begriffe werden im Editor **grün** markiert und optional
unter der Vorschau als klickbare Verweise aufgelistet — der Text selbst bleibt
nicht verändert. Code-Blöcke, Inline-Code und bestehende Links werden nie
verlinkt.

### Einstellungen (Auszug)

- Allgemein: Autospeichern-Verzögerung, Vorschau-Standard
- Editor: Schriftgröße
- Glossar: aktiv, Groß-/Kleinschreibung, Vorschau-Liste, max. Treffer,
  Mindestbegriffslänge, Glossar-Unterordner (Zeilenliste)
- Tastenkürzel: alle 12 Aktionen frei belegbar (Aufzeichnung per Klick auf
  „Ändern", dann Taste drücken; Escape bricht ab). Fehlende Keybinds in alten
  Configs werden automatisch mit der Standard-Belegung aufgefüllt.

![status](https://img.shields.io/badge/tests-27%20passing-brightgreen)

## Design goals

- **Speed first**: GPU-rendered (wgpu), incremental syntax highlighting, no
  file locks, debounced autosave. Release build starts instantly.
- **Plain files**: your notes are ordinary `.md` files in a folder ("vault").
  No database, no vendor lock-in.
- **Keyboard-driven**: every core action has a shortcut.

## Build

```sh
cargo build --release
```

Runtime deps: standard Linux graphics stack (Wayland or X11) and `zenity`
for the folder-picker dialog (present on virtually all desktop distros).

## Run

```sh
./target/release/rusty-notes
```

- First start: click **Open Folder** and choose any folder containing `.md`
  files (subfolders are scanned; hidden files/folders are ignored).
- The vault is remembered and reopened on the next start
  (`~/.config/rusty-notes/config.json`).

## Shortcuts

| Keys | Action |
|---|---|
| `Ctrl+O` | Open vault folder |
| `Ctrl+P` | Quick switcher (fuzzy note jump) |
| `Ctrl+K` | Command palette |
| `Ctrl+N` | New note (folders allowed, e.g. `ideas/foo`) |
| `Ctrl+S` | Save now (autosave also runs 0.8 s after typing) |
| `Ctrl+E` | Toggle live preview |
| `Ctrl+Shift+F` | Vault-weite Suche |
| `Ctrl+Shift+S` | Einstellungen |

## Features

- Live Markdown syntax highlighting while you type
- Live rendered preview (CommonMark + strikethrough + tables) via egui_commonmark
- YAML front matter (title, tags) parsed and shown in preview
- `[[Wikilinks]]` with exact-stem then case-insensitive substring resolution
- Vault-wide full-text search with line numbers
- Create / rename / delete notes (right-click in sidebar), buffers follow renames
- Debounced autosave with dirty indicator in the status bar

## Architektur

```
src/
├── main.rs           UI-Shell: Panels, Overlays, Shortcuts, Autosave, Theme
├── lib.rs            Bibliothekskern (für Integrationstests)
├── vault.rs          Ordner-Scan + Puffer + Inhalts-Cache (mtime-entwertet)
├── markdown.rs       Front-Matter + Wikilink-Extraktion/-Auflösung
├── search.rs         Volltextsuche + Fuzzy-Schnellwechsler
├── editor.rs         Byte<->Zeile-Index + inkrementelles Highlighting
├── glossary.rs       Aho-Corasick-Glossar + Span-Verschneidung
├── einstellungen.rs  Persistente Einstellungen (Dirty-Tracking, Keybinds)
└── i18n.rs           Deutsche UI-Texte
tests/bug_tests.rs    Regressionstests für gefundene Bugs
```

Alle Logik-Module sind rein und unit-getestet (52 Tests); die UI ist eine
dünne Schale darüber. `cargo test` läuft alles.

## Windows-Build

Der Code kompiliert für `x86_64-pc-windows-msvc` (geprüft per
`cargo check --target x86_64-pc-windows-msvc`): Der Ordner-Dialog nutzt dort
den nativen Win32-Dialog (rfd), unter Linux zenity.

```sh
rustup target add x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc --all-targets
# Echter Build mit Linker (z.B. cargo-xwin) oder auf einem Windows-System:
cargo build --release
```

## License

MIT
