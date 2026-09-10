//! Pure Markdown parsing: YAML front matter + `[[wikilinks]]` extraction.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

static FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```.*?```").unwrap());
static INLINE_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("`[^`\n]+`").unwrap());
static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\[\]|]+)(?:\|[^\[\]]*)?\]\]").unwrap());

/// Parsed front-matter fields we care about (all optional).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub tags: Vec<String>,
    /// Obsidian-kompatible Aliase (`aliases: [a, b]` oder Blockliste).
    pub aliases: Vec<String>,
}

/// Split a document into `(front_matter, body)` when it starts with `---`.
pub fn split_front_matter(text: &str) -> (Option<&str>, &str) {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    if first.trim_end_matches(['\n', '\r']) != "---" {
        return (None, text);
    }
    let rest = &text[first.len()..];
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            let fm = rest[..offset].trim_end_matches(['\n', '\r']);
            let body = &rest[offset + line.len()..];
            return (Some(fm), body);
        }
        offset += line.len();
    }
    (None, text)
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

/// Minimal YAML subset parser: `title:` plus inline `[a, b]` / block-list tags.
pub fn parse_front_matter(raw: &str) -> FrontMatter {
    let mut fm = FrontMatter::default();
    let mut in_tag_block = false;
    let mut in_alias_block = false;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if in_tag_block {
            if let Some(item) = line.strip_prefix("- ") {
                let item = item.trim();
                if !item.is_empty() {
                    fm.tags.push(unquote(item));
                }
                continue;
            }
            in_tag_block = false;
        }
        if in_alias_block {
            if let Some(item) = line.strip_prefix("- ") {
                let item = item.trim();
                if !item.is_empty() {
                    fm.aliases.push(unquote(item));
                }
                continue;
            }
            in_alias_block = false;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            match key.trim() {
                "title" if !value.is_empty() => fm.title = Some(unquote(value)),
                "aliases" => {
                    if value.is_empty() {
                        in_alias_block = true;
                    } else {
                        fm.aliases = unquote(value)
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .split(',')
                            .map(|a| unquote(a.trim()))
                            .filter(|a| !a.is_empty())
                            .collect();
                    }
                }
                "tags" => {
                    if value.is_empty() {
                        in_tag_block = true;
                    } else {
                        fm.tags = unquote(value)
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .split(',')
                            .map(|t| unquote(t.trim()))
                            .filter(|t| !t.is_empty())
                            .collect();
                    }
                }
                _ => {}
            }
        }
    }
    fm
}

/// Extract all `[[link]]` / `[[link|alias]]` targets, skipping code spans/blocks.
pub fn extract_wikilinks(body: &str) -> Vec<String> {
    let scrubbed = FENCE_RE.replace_all(body, "");
    let scrubbed = INLINE_CODE_RE.replace_all(&scrubbed, "");
    WIKILINK_RE
        .captures_iter(&scrubbed)
        .filter_map(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Wandelt `[[Ziel]]` / `[[Ziel|Alias]]` in Markdown-Links mit dem
/// Schema `rusty-note:` um (klickbar in egui_commonmark). Code-Spans und
/// Code-Blöcke bleiben unverändert; bereits existierende `[..](..)`-Links
/// werden nicht angetastet, da `[[` in ihnen nicht als Wikilink zählt.
pub fn wikilinks_to_md_links(text: &str) -> String {
    if !text.contains("[[") {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0usize;
    while i < len {
        match bytes[i] {
            b'`' => {
                // Zaun oder Inline-Code 1:1 kopieren
                if i + 2 < len && &bytes[i..i + 3] == b"```" {
                    let start = i;
                    i += 3;
                    while i + 2 < len && &bytes[i..i + 3] != b"```" {
                        i += 1;
                    }
                    i = (i + 3).min(len);
                    out.push_str(&text[start..i]);
                } else {
                    let start = i;
                    i += 1;
                    while i < len && bytes[i] != b'`' {
                        i += 1;
                    }
                    i = (i + 1).min(len);
                    out.push_str(&text[start..i]);
                }
            }
            b'[' if i + 1 < len && bytes[i + 1] == b'[' => {
                let start = i;
                i += 2;
                let mut target = None;
                while i + 1 < len && &bytes[i..i + 2] != b"]]" {
                    i += 1;
                }
                if i + 1 < len {
                    target = Some(&text[start + 2..i]);
                    i += 2;
                }
                match target {
                    Some(z) => {
                        let z = z.trim();
                        let (target, label) = match z.split_once('|') {
                            Some((t, l)) => (t.trim(), l.trim()),
                            None => (z, z),
                        };
                        out.push('[');
                        out.push_str(label);
                        out.push_str("](<rusty-note:");
                        out.push_str(target);
                        out.push('>');
                        out.push(')');
                    }
                    None => out.push_str(&text[start..i]),
                }
            }
            _ => {
                let start = i;
                while i < len && bytes[i] != b'`' && !(bytes[i] == b'[' && i + 1 < len && bytes[i + 1] == b'[') {
                    i += 1;
                }
                out.push_str(&text[start..i.max(start)]);
            }
        }
    }
    out
}

fn file_stem_str(p: &Path) -> &str {
    p.file_stem().and_then(|s| s.to_str()).unwrap_or("")
}

/// Resolve a wikilink target to an existing note: exact stem match first,
/// then case-insensitive substring fallback.
pub fn resolve_wikilink<'a>(
    links: impl IntoIterator<Item = &'a Path>,
    target: &str,
) -> Option<PathBuf> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    let lower = target.to_lowercase();
    let all: Vec<&Path> = links.into_iter().collect();
    all.iter()
        .find(|p| file_stem_str(p).to_lowercase() == lower)
        .or_else(|| {
            all.iter()
                .find(|p| file_stem_str(p).to_lowercase().contains(&lower))
        })
        .map(|p| (*p).to_path_buf())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_yaml_front_matter_from_body() {
        let doc = "---\ntitle: My Note\ntags: [rust, gui]\n---\n\n# Heading\n";
        let (fm, body) = split_front_matter(doc);
        assert_eq!(fm.unwrap(), "title: My Note\ntags: [rust, gui]");
        assert_eq!(body, "\n# Heading\n");
    }

    #[test]
    fn no_front_matter_returns_whole_text() {
        let doc = "# Just a note\n[[link]]\n";
        let (fm, body) = split_front_matter(doc);
        assert!(fm.is_none());
        assert_eq!(body, doc);
    }

    #[test]
    fn unterminated_delimiter_is_not_front_matter() {
        let doc = "---\nno closing delimiter\n";
        let (fm, body) = split_front_matter(doc);
        assert!(fm.is_none());
        assert_eq!(body, doc);
    }

    #[test]
    fn parses_title_and_inline_and_block_tags() {
        let fm = parse_front_matter("title: Hello World\ntags: [a, b]");
        assert_eq!(fm.title.as_deref(), Some("Hello World"));
        assert_eq!(fm.tags, vec!["a".to_string(), "b".to_string()]);

        let fm2 = parse_front_matter("tags:\n  - x\n  - y");
        assert_eq!(fm2.tags, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn extracts_and_normalizes_wikilinks() {
        let body = "See [[Foo]] and [[Bar/Baz|the baz]]. Not [[broken.";
        let links = extract_wikilinks(body);
        assert_eq!(links, vec!["Foo".to_string(), "Bar/Baz".to_string()]);
    }

    #[test]
    fn wikilinks_inside_code_spans_are_ignored() {
        let body = "`[[NotALink]]` and ```\n[[AlsoNot]]\n```\nbut [[Real]] yes";
        let links = extract_wikilinks(body);
        assert_eq!(links, vec!["Real".to_string()]);
    }

    #[test]
    fn resolves_exact_stem_before_substring() {
        let notes = [
            PathBuf::from("/v/Exact Match.md"),
            PathBuf::from("/v/Pre Exact Match Post.md"),
        ];
        let hit = resolve_wikilink(notes.iter().map(|p| p.as_path()), "Exact Match").unwrap();
        assert_eq!(hit, PathBuf::from("/v/Exact Match.md"));
    }

    #[test]
    fn resolves_substring_fallback_case_insensitive() {
        let notes = [PathBuf::from("/v/Rust GUI Notes.md")];
        let hit =
            resolve_wikilink(notes.iter().map(|p| p.as_path()), "rust gui").expect("substring");
        assert_eq!(hit, PathBuf::from("/v/Rust GUI Notes.md"));
    }
}

#[cfg(test)]
mod alias_und_wikilink_tests {
    use super::*;

    #[test]
    fn aliases_werken_aus_front_matter_gelesen() {
        let fm = parse_front_matter("aliases: [egui, EGUI-Framework]");
        assert_eq!(fm.aliases, vec!["egui".to_string(), "EGUI-Framework".to_string()]);

        let fm2 = parse_front_matter("aliases:\n  - Erster\n  - Zweiter");
        assert_eq!(fm2.aliases, vec!["Erster".to_string(), "Zweiter".to_string()]);
    }

    #[test]
    fn aliases_leer_ohne_angabe() {
        assert!(parse_front_matter("title: x").aliases.is_empty());
    }

    #[test]
    fn wikilinks_werden_zu_klickbaren_md_links() {
        let text = "Siehe [[Rust GUI Notes]] und [[Rust|die Sprache]].";
        let out = wikilinks_to_md_links(text);
        assert!(out.contains("[Rust GUI Notes](<rusty-note:Rust GUI Notes>)"), "{}", out);
        assert!(out.contains("[die Sprache](<rusty-note:Rust>)"), "{}", out);
        assert!(!out.contains("[["));
    }

    #[test]
    fn wikilink_konvertierung_touchiert_code_nicht() {
        let text = "`[[KeinLink]]` und ```\n[[AuchNicht]]\n```\naber [[Doch]].";
        let out = wikilinks_to_md_links(text);
        assert!(out.contains("`[[KeinLink]]`"));
        assert!(out.contains("[[AuchNicht]]"));
        assert!(out.contains("[Doch](<rusty-note:Doch>)"));
    }
}
