//! Vault: a folder of Markdown notes + open buffers (in-memory edits).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// A single open note: its path plus the in-memory text being edited.
#[derive(Debug)]
pub struct Buffer {
    pub path: PathBuf,
    pub text: String,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct NoteEntry {
    /// Path relative to vault root, using '/' separators.
    pub rel: String,
    /// Absolute path.
    pub abs: PathBuf,
}

#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
    /// Sorted by relative path.
    notes: Vec<NoteEntry>,
    pub buffers: BTreeMap<PathBuf, Buffer>,
    /// Inhalts-Cache für Suche/Glossar: Pfad -> (mtime, Inhalt).
    inhalt_cache: BTreeMap<PathBuf, (std::time::SystemTime, String)>,
}

/// Canonicalize, falling back to the input path when it does not exist yet.
fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

pub fn is_note_file(p: &Path) -> bool {
    let ext_is_md = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("md"))
        .unwrap_or(false);
    if !ext_is_md {
        return false;
    }
    !is_hidden(p)
}

impl Vault {
    pub fn open(root: &Path) -> io::Result<Vault> {
        let root = canon(root);
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("not a directory: {}", root.display()),
            ));
        }
        Ok(Vault {
            root,
            notes: Vec::new(),
            buffers: BTreeMap::new(),
            inhalt_cache: BTreeMap::new(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn notes(&self) -> &[NoteEntry] {
        &self.notes
    }

    /// Rescan the folder tree, skipping hidden entries and non-`.md` files.
    pub fn scan(&mut self) -> io::Result<()> {
        let mut notes = Vec::new();
        for entry in WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_hidden(e.path()))
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() && is_note_file(entry.path()) {
                let abs = entry.path().to_path_buf();
                let rel = abs
                    .strip_prefix(&self.root)
                    .unwrap_or(&abs)
                    .to_string_lossy()
                    .replace('\\', "/");
                notes.push(NoteEntry { rel, abs });
            }
        }
        notes.sort_by(|a, b| a.rel.cmp(&b.rel));
        self.notes = notes;

        // Inhalts-Cache entwerten, wenn Dateien verschwunden oder neuer sind.
        let mut aktuell: BTreeMap<PathBuf, std::time::SystemTime> = BTreeMap::new();
        for n in &self.notes {
            if let Ok(md) = fs::metadata(&n.abs) {
                if let Ok(m) = md.modified() {
                    aktuell.insert(n.abs.clone(), m);
                }
            }
        }
        self.inhalt_cache.retain(|p, (mtime, _)| {
            aktuell.get(p).map(|m| *m <= *mtime).unwrap_or(false)
        });
        Ok(())
    }

    /// Load a note into a buffer (idempotent for already-open notes).
    pub fn open_note(&mut self, abs: &Path) -> io::Result<()> {
        let key = canon(abs);
        if self.buffers.contains_key(&key) {
            return Ok(());
        }
        let text = fs::read_to_string(&key)?;
        self.buffers.insert(key.clone(), Buffer { path: key, text, dirty: false });
        Ok(())
    }

    pub fn buffer(&self, abs: &Path) -> Option<&Buffer> {
        self.buffers.get(&canon(abs))
    }

    pub fn buffer_mut(&mut self, abs: &Path) -> Option<&mut Buffer> {
        let key = canon(abs);
        self.buffers.get_mut(&key)
    }

    /// Inhalt einer Notiz für Suche/Glossar: offener (evtl. geänderter)
    /// Puffer gewinnt; sonst Cache mit mtime-Prüfung; sonst Platte.
    pub fn content_for(&mut self, abs: &Path) -> io::Result<String> {
        let key = canon(abs);
        if let Some(buf) = self.buffers.get(&key) {
            return Ok(buf.text.clone());
        }
        let mtime = fs::metadata(&key)?.modified()?;
        if let Some((cached_mtime, text)) = self.inhalt_cache.get(&key) {
            if *cached_mtime >= mtime {
                return Ok(text.clone());
            }
        }
        let text = fs::read_to_string(&key)?;
        self.inhalt_cache.insert(key, (mtime, text.clone()));
        Ok(text)
    }

    /// Canonical edit path: replaces buffer text and marks it dirty.
    pub fn set_text(&mut self, abs: &Path, text: impl Into<String>) -> io::Result<()> {
        let key = canon(abs);
        let buf = self.buffers.get_mut(&key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no such open buffer")
        })?;
        buf.text = text.into();
        buf.dirty = true;
        // Cache sofort auf den neuen Stand bringen (die Datei ist noch nicht
        // gespeichert, daher UNIX_EPOCH als mtime, bis save() sie setzt).
        self.inhalt_cache
            .insert(key, (std::time::SystemTime::UNIX_EPOCH, buf.text.clone()));
        Ok(())
    }

    /// Persist one buffer to disk and clear its dirty flag.
    pub fn save(&mut self, abs: &Path) -> io::Result<()> {
        let key = canon(abs);
        let buf = self.buffers.get_mut(&key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no such open buffer")
        })?;
        fs::write(&buf.path, &buf.text)?;
        buf.dirty = false;
        if let Ok(m) = fs::metadata(&buf.path).and_then(|md| md.modified()) {
            self.inhalt_cache
                .insert(buf.path.clone(), (m, buf.text.clone()));
        }
        Ok(())
    }

    /// Create an empty note at `rel`, open it, rescan, return its absolute path.
    pub fn create_note(&mut self, rel: &str) -> io::Result<PathBuf> {
        let rel = rel.trim_start_matches('/');
        if rel.is_empty() || rel.split('/').any(|c| c == "..") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad note path"));
        }
        let abs = self.root.join(rel);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)?;
        }
        if !abs.exists() {
            fs::write(&abs, "")?;
        }
        self.scan()?;
        self.open_note(&abs)?;
        Ok(canon(&abs))
    }

    /// Move a note to `new_rel`; an open buffer moves with it.
    pub fn rename_note(&mut self, abs: &Path, new_rel: &str) -> io::Result<PathBuf> {
        let old = canon(abs);
        let new_rel = new_rel.trim_start_matches('/');
        if new_rel.is_empty() || new_rel.split('/').any(|c| c == "..") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad note path"));
        }
        let new_abs = self.root.join(new_rel);
        if let Some(parent) = new_abs.parent() {
            fs::create_dir_all(parent)?;
        }
        // Kein stilles Überschreiben: Ziel darf nicht existieren — außer es
        // ist (kanonisch) die Quelldatei selbst (Groß-/Kleinschreibungs-Fall
        // auf case-insensitiven Dateisystemen).
        if new_abs.exists() && canon(&new_abs) != canon(&old) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("Ziel existiert bereits: {}", new_rel),
            ));
        }
        // Preserve unsaved edits across the move.
        let mut moved = self.buffers.remove(&old);
        fs::rename(&old, &new_abs)?;
        if let Some(mut buf) = moved.take() {
            buf.path = new_abs.clone();
            self.buffers.insert(new_abs.clone(), buf);
        }
        if let Some((m, text)) = self.inhalt_cache.remove(&old) {
            self.inhalt_cache.insert(new_abs.clone(), (m, text));
        }
        self.scan()?;
        Ok(canon(&new_abs))
    }

    /// Delete a note and close its buffer.
    pub fn delete_note(&mut self, abs: &Path) -> io::Result<()> {
        let key = canon(abs);
        self.buffers.remove(&key);
        self.inhalt_cache.remove(&key);
        fs::remove_file(&key)?;
        self.scan()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Unique temp dir per call, cleaned up by caller (OS tmp reaper is fine for tests).
    fn tmpdir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rusty-notes-test-{}-{}-{}", tag, nanos, n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn rels(vault: &Vault) -> Vec<String> {
        vault.notes().iter().map(|n| n.rel.clone()).collect()
    }

    #[test]
    fn scan_finds_md_recursively_ignores_hidden_and_non_md() {
        let dir = tmpdir("scan");
        write_file(&dir.join("a.md"), "# A");
        write_file(&dir.join("sub/b.md"), "# B");
        write_file(&dir.join(".hidden/c.md"), "# C"); // hidden dir
        write_file(&dir.join(".secret.md"), "# S"); // hidden file
        write_file(&dir.join("notes.txt"), "not markdown");

        let mut v = Vault::open(&dir).unwrap();
        v.scan().unwrap();

        assert_eq!(rels(&v), vec!["a.md".to_string(), "sub/b.md".to_string()]);
        assert_eq!(v.root(), dir.as_path());
    }

    #[test]
    fn open_note_loads_text_and_dirty_tracking_works() {
        let dir = tmpdir("dirty");
        write_file(&dir.join("a.md"), "hello");

        let mut v = Vault::open(&dir).unwrap();
        let abs = dir.join("a.md");
        v.open_note(&abs).unwrap();

        let buf = v.buffer(&abs).unwrap();
        assert_eq!(buf.text, "hello");
        assert!(!buf.dirty);

        v.set_text(&abs, "hello world").unwrap();
        assert!(v.buffer(&abs).unwrap().dirty);
    }

    #[test]
    fn save_persists_buffer_and_clears_dirty() {
        let dir = tmpdir("save");
        write_file(&dir.join("a.md"), "old");

        let mut v = Vault::open(&dir).unwrap();
        let abs = dir.join("a.md");
        v.open_note(&abs).unwrap();
        v.buffer_mut(&abs).unwrap().text = "new body".into();
        v.save(&abs).unwrap();

        assert_eq!(fs::read_to_string(&abs).unwrap(), "new body");
        assert!(!v.buffer(&abs).unwrap().dirty);
    }

    #[test]
    fn create_note_creates_empty_file_and_opens_buffer() {
        let dir = tmpdir("create");
        let mut v = Vault::open(&dir).unwrap();
        v.scan().unwrap();

        let created = v.create_note("ideas/new note.md").unwrap();
        assert_eq!(fs::read_to_string(&created).unwrap(), "");
        assert!(v.buffer(&created).is_some());
        v.scan().unwrap();
        assert!(rels(&v).contains(&"ideas/new note.md".to_string()));
    }

    #[test]
    fn rename_note_moves_file_and_keeps_open_buffer() {
        let dir = tmpdir("rename");
        write_file(&dir.join("a.md"), "contents");

        let mut v = Vault::open(&dir).unwrap();
        let abs = dir.join("a.md");
        v.open_note(&abs).unwrap();

        let new_abs = v.rename_note(&abs, "renamed.md").unwrap();
        assert!(!abs.exists());
        assert_eq!(fs::read_to_string(&new_abs).unwrap(), "contents");

        let buf = v.buffer(&new_abs).expect("buffer follows rename");
        assert_eq!(buf.text, "contents");
        assert!(v.buffer(&abs).is_none());
    }

    #[test]
    fn delete_note_removes_file_and_closes_buffer() {
        let dir = tmpdir("delete");
        write_file(&dir.join("a.md"), "bye");

        let mut v = Vault::open(&dir).unwrap();
        let abs = dir.join("a.md");
        v.open_note(&abs).unwrap();

        v.delete_note(&abs).unwrap();
        assert!(!abs.exists());
        assert!(v.buffer(&abs).is_none());
    }
}
