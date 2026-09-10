# rusty-notes

A fast, keyboard-driven Markdown note editor for Linux, written in Rust.
Inspired by Obsidian (vault + wikilinks), Zen Notes (minimal chrome), and
Zettlr (writing focus).

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
| `Ctrl+Shift+F` | Vault-wide text search |

## Features

- Live Markdown syntax highlighting while you type
- Live rendered preview (CommonMark + strikethrough + tables) via egui_commonmark
- YAML front matter (title, tags) parsed and shown in preview
- `[[Wikilinks]]` with exact-stem then case-insensitive substring resolution
- Vault-wide full-text search with line numbers
- Create / rename / delete notes (right-click in sidebar), buffers follow renames
- Debounced autosave with dirty indicator in the status bar

## Architecture

```
src/
├── main.rs     UI shell: panels, overlays, shortcuts, autosave, theme
├── vault.rs    Folder scan + note buffers (open/save/create/rename/delete)
├── markdown.rs Front-matter parsing + wikilink extraction/resolution
├── search.rs   Full-text search + fuzzy quick-switcher scoring
└── editor.rs   Byte<->line index + incremental Markdown highlighter
```

All logic modules are pure and unit-tested (27 tests); the UI layer is a thin
shell over them. Tests: `cargo test`.

## License

MIT
