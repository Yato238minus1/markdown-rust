//! Glossary: virtuelle Verlinkungen von Begriffen im Text (Inspiration: das
//! Obsidian-Plugin "Virtual Linker / Glossary"), neu gedacht als
//! Aho-Corasick-Automat: ein Durchlauf über den Text liefert alle Treffer
//! (O(Textlänge + Treffer)), statt jeden Begriff einzeln zu suchen. Das
//! Dokument selbst bleibt unverändert — die Verlinkung ist rein visuell.

use std::path::PathBuf;

use aho_corasick::{AhoCorasick, MatchKind};

/// Ein Glossary-Eintrag: Begriff (Stichwort/Alias) und die Notiz dahinter.
#[derive(Debug, Clone, PartialEq)]
pub struct GlossaryEntry {
    pub term: String,
    /// Zusätzliche Schreibweisen, die denselben Eintrag treffen.
    pub aliases: Vec<String>,
    pub path: PathBuf,
}

/// Ein Treffer im Text: Byte-Range plus Index in die Eintragsliste.
#[derive(Debug, Clone, PartialEq)]
pub struct GlossaryHit {
    pub start: usize,
    pub end: usize,
    pub index: usize,
}

/// Byte-erhaltende Kleinschreibung: A-Z → a-z sowie lateinische
/// Großbuchstaben mit Diakritika (C3 80/C3 82/…/C3 9E, also ÄÖÜ usw.) →
/// ihre Kleinschreibung (+0x20 im zweiten UTF-8-Byte). Die Länge bleibt
/// unverändert, sodass Byte-Offsets zwischen Original und Kopie identisch sind.
fn lowercase_bytes(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        if b == 0xC3 && i + 1 < src.len() {
            let n = src[i + 1];
            // Zweite Bytes 0x80..0x9E (gerade) sind Großbuchstaben À..Þ.
            if (0x80..=0x9E).contains(&n) && n % 2 == 0 {
                out.push(b);
                out.push(n + 0x20);
                i += 2;
                continue;
            }
        }
        out.push(b.to_ascii_lowercase());
        i += 1;
    }
    out
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

#[derive(Debug)]
pub struct Glossary {
    ac: AhoCorasick,
    entries: Vec<GlossaryEntry>,
    /// AC-Muster-Index -> Index in `entries` (Aliase teilen sich den Eintrag).
    pattern_to_entry: Vec<usize>,
    case_insensitive: bool,
}

impl Glossary {
    /// Baut den Automaten neu auf (nur bei Vault-Scan oder Einstellungsänderung).
    /// Duplikat-Begriffe: der erste gewinnt. Leere Begriffe werden verworfen.
    pub fn new(entries: Vec<GlossaryEntry>, case_insensitive: bool) -> Glossary {
        let mut seen = std::collections::HashSet::new();
        let mut patterns: Vec<Vec<u8>> = Vec::new();
        let mut pattern_to_entry: Vec<usize> = Vec::new();
        let mut filtered: Vec<GlossaryEntry> = Vec::new();
        for e in entries {
            let term = e.term.trim();
            if term.is_empty() {
                continue;
            }
            let bytes = if case_insensitive {
                lowercase_bytes(term.as_bytes())
            } else {
                term.as_bytes().to_vec()
            };
            if !seen.insert(bytes.clone()) {
                continue;
            }
            let entry_idx = filtered.len();
            patterns.push(bytes);
            pattern_to_entry.push(entry_idx);
            filtered.push(GlossaryEntry {
                term: term.to_string(),
                aliases: e.aliases.clone(),
                path: e.path,
            });
            // Aliase als zusätzliche Muster; index zeigt auf denselben Eintrag.
            for alias in &e.aliases {
                let alias = alias.trim();
                if alias.is_empty() {
                    continue;
                }
                let alias_bytes = if case_insensitive {
                    lowercase_bytes(alias.as_bytes())
                } else {
                    alias.as_bytes().to_vec()
                };
                if !seen.insert(alias_bytes.clone()) {
                    continue;
                }
                patterns.push(alias_bytes);
                pattern_to_entry.push(entry_idx);
            }
        }
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)
            .expect("Aho-Corasick mit gültigen Mustern");
        Glossary {
            ac,
            entries: filtered,
            pattern_to_entry,
            case_insensitive,
        }
    }

    pub fn entries(&self) -> &[GlossaryEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Alle Begriffs-Treffer in `text` — nur an Wortgrenzen, links-längster
    /// Begriff gewinnt. Treffer innerhalb von Code (`…`, ```…```) und
    /// bestehenden Links ([[…]], […](…)) werden ignoriert.
    pub fn find(&self, text: &str) -> Vec<GlossaryHit> {
        if self.entries.is_empty() || text.is_empty() {
            return Vec::new();
        }

        // Ranges, in denen NICHT verlinkt wird: Code-Spans, Code-Blöcke,
        // Wikilinks, Markdown-Links.
        let protected = protected_ranges(text);
        let bytes = text.as_bytes();
        // Für case-insensitives Suchen: byte-erhaltende Kleinschreibung —
        // gleiche Länge, also stimmen alle Offsets mit dem Original überein.
        let search_bytes: Vec<u8> = if self.case_insensitive {
            lowercase_bytes(bytes)
        } else {
            bytes.to_vec()
        };
        let mut hits = Vec::new();

        for m in self.ac.find_iter(&search_bytes[..]) {
            if !text.is_char_boundary(m.start()) || !text.is_char_boundary(m.end()) {
                continue;
            }
            // Wortgrenzen prüfen (im Original!)
            let left_ok = m.start() == 0 || !is_word_char(bytes[m.start() - 1]);
            let right_ok = m.end() >= bytes.len() || !is_word_char(bytes[m.end()]);
            if !left_ok || !right_ok {
                continue;
            }
            // Geschützte Ranges überspringen
            if is_protected_hit(&protected, m.start(), m.end()) {
                continue;
            }
            let pattern_idx = m.pattern().as_usize();
            let entry_idx = self
                .pattern_to_entry
                .get(pattern_idx)
                .copied()
                .unwrap_or(pattern_idx);
            hits.push(GlossaryHit {
                start: m.start(),
                end: m.end(),
                index: entry_idx,
            });
        }
        hits
    }
}

/// Halboffene Intervalle [start, end) geschützter Ranges.
type Ranges = Vec<(usize, usize)>;

fn is_protected_hit(bereiche: &Ranges, start: usize, end: usize) -> bool {
    bereiche
        .iter()
        .any(|&(s, e)| start < e && end > s)
}

fn protected_ranges(text: &str) -> Ranges {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut bereiche: Ranges = Vec::new();
    let mut i = 0usize;

    while i < len {
        match bytes[i] {
            b'`' => {
                // ```-Zaun oder Inline-Code
                if i + 2 < len && &bytes[i..i + 3] == b"```" {
                    let start = i;
                    i += 3;
                    while i + 2 < len && &bytes[i..i + 3] != b"```" {
                        i += 1;
                    }
                    i = (i + 3).min(len);
                    bereiche.push((start, i));
                } else {
                    let start = i;
                    i += 1;
                    while i < len && bytes[i] != b'`' {
                        i += 1;
                    }
                    i = (i + 1).min(len);
                    bereiche.push((start, i));
                }
            }
            b'[' => {
                if i + 1 < len && bytes[i + 1] == b'[' {
                    // Wikilink
                    let start = i;
                    i += 2;
                    while i + 1 < len && &bytes[i..i + 2] != b"]]" {
                        i += 1;
                    }
                    i = (i + 2).min(len);
                    bereiche.push((start, i));
                } else {
                    // [text](url)
                    let start = i;
                    let mut j = i + 1;
                    while j < len && bytes[j] != b']' {
                        j += 1;
                    }
                    if j + 1 < len && bytes[j + 1] == b'(' {
                        let mut k = j + 2;
                        while k < len && bytes[k] != b')' {
                            k += 1;
                        }
                        i = (k + 1).min(len);
                        bereiche.push((start, i));
                    } else {
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    bereiche
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(terms: &[(&str, &str)]) -> Glossary {
        Glossary::new(
            terms
                .iter()
                .map(|(b, p)| GlossaryEntry {
                    term: b.to_string(),
                    aliases: Vec::new(),
                    path: PathBuf::from(p),
                })
                .collect(),
            true,
        )
    }

    #[test]
    fn findet_begriff_mit_position() {
        let gl = g(&[("Rust", "/v/Rust.md")]);
        let hits = gl.find("Lerne Rust heute.");
        assert_eq!(hits.len(), 1);
        assert_eq!(
            &"Lerne Rust heute."[hits[0].start..hits[0].end],
            "Rust"
        );
        assert_eq!(gl.entries()[0].term, "Rust");
    }

    #[test]
    fn gross_klein_wird_ignoriert() {
        let gl = g(&[("rust", "/v/Rust.md")]);
        assert_eq!(gl.find("RUST und ruSt").len(), 2);
    }

    #[test]
    fn nur_ganze_woerter() {
        let gl = g(&[("Rust", "/v/Rust.md")]);
        // "Trust" (Begriff in der Mitte) und "rusty" (Suffix) dürfen nicht treffen
        assert_eq!(gl.find("Trust rusty Rust").len(), 1);
    }

    #[test]
    fn laengster_begriff_gewinnt() {
        let gl = g(&[("Note", "/v/Note.md"), ("Note Pad", "/v/Note Pad.md")]);
        let hits = gl.find("im Note Pad");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].index, 1, "das längere Muster 'Note Pad' gewinnt");
    }

    #[test]
    fn empty_glossary_and_empty_text() {
        let gl = g(&[]);
        assert!(gl.find("irgendwas").is_empty());
        let gl2 = g(&[("x", "/v/x.md")]);
        assert!(gl2.find("").is_empty());
    }

    #[test]
    fn code_und_links_bleiben_unverlinkt() {
        let gl = g(&[("Rust", "/v/Rust.md")]);
        let text = "`Rust` und [[Rust]] und [Rust](x.md), aber Rust ja.";
        let hits = gl.find(text);
        assert_eq!(hits.len(), 1, "nur das freie 'Rust' trifft");
        assert_eq!(&text[hits[0].start..hits[0].end], "Rust");
        assert!(hits[0].start > text.find(", aber").unwrap());
    }

    #[test]
    fn duplikate_werden_verworfen() {
        let gl = g(&[("Rust", "/v/a.md"), ("Rust", "/v/b.md")]);
        assert_eq!(gl.entries().len(), 1);
        assert_eq!(gl.entries()[0].path, PathBuf::from("/v/a.md"));
    }
}

// ---------------------------------------------------------------------------
// Verschneidung mit Editor-Highlighting
// ---------------------------------------------------------------------------

/// Style eines Anzeigebereichs: normales Token oder Glossary-Begriff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Token(crate::editor::Tok),
    Glossary,
}

/// Verschneidet kontiguous Highlight-Spans mit Glossary-Treffern: Treffer-
/// Ranges bekommen `Style::Glossary`, der Rest behält sein Token. Beide
/// Eingaben müssen aufsteigend sortiert und nichtüberlappend sein.
pub fn intersect(
    spans: &[crate::editor::Span],
    hits: &[GlossaryHit],
    text_len: usize,
) -> Vec<(usize, usize, Style)> {
    let mut out: Vec<(usize, usize, Style)> = Vec::new();
    let mut cursor = 0usize; // aktuelle Position im Dokument
    let mut si = 0usize; // Index in spans
    let mut ti = 0usize; // Index in hits

    while cursor < text_len {
        // Nächster Ereignis-Punkt: Ende des aktuellen Spans oder des Treffers.
        let span_rest = si < spans.len() && spans[si].end > cursor;
        let treffer_rest = ti < hits.len() && hits[ti].end > cursor;

        if !span_rest && !treffer_rest {
            // Rest als Plain auffüllen (sollte bei voller Abdeckung nicht passieren)
            out.push((cursor, text_len, Style::Token(crate::editor::Tok::Plain)));
            break;
        }

        // Grenzen: Ende des Spans, Anfang UND Ende des Treffers.
        let mut grenzen: Vec<usize> = Vec::new();
        if span_rest {
            grenzen.push(spans[si].end);
        }
        if treffer_rest {
            grenzen.push(hits[ti].end);
            if hits[ti].start > cursor {
                grenzen.push(hits[ti].start);
            }
        }
        let next_end = grenzen.iter().copied().min().unwrap_or(text_len);

        let in_treffer = treffer_rest && hits[ti].start <= cursor;
        let stil = if in_treffer {
            Style::Glossary
        } else {
            Style::Token(spans[si].tok)
        };
        out.push((cursor, next_end, stil));

        if span_rest && spans[si].end == next_end {
            si += 1;
        }
        if treffer_rest && hits[ti].end == next_end {
            ti += 1;
        }
        cursor = next_end;
    }
    out
}

#[cfg(test)]
mod verschneide_tests {
    use super::*;
    use crate::editor::{self, Tok};

    fn span(s: usize, e: usize, t: Tok) -> editor::Span {
        editor::Span { start: s, end: e, tok: t }
    }

    #[test]
    fn treffer_in_einem_span_teilen_diesen() {
        let spans = vec![span(0, 10, Tok::Plain)];
        let hits = vec![GlossaryHit { start: 3, end: 7, index: 0 }];
        let out = intersect(&spans, &hits, 10);
        assert_eq!(
            out,
            vec![
                (0, 3, Style::Token(Tok::Plain)),
                (3, 7, Style::Glossary),
                (7, 10, Style::Token(Tok::Plain)),
            ]
        );
    }

    #[test]
    fn treffer_ueber_mehrere_spans() {
        let spans = vec![
            span(0, 5, Tok::ListMarker),
            span(5, 12, Tok::Plain),
        ];
        let hits = vec![GlossaryHit { start: 3, end: 8, index: 0 }];
        let out = intersect(&spans, &hits, 12);
        assert_eq!(
            out,
            vec![
                (0, 3, Style::Token(Tok::ListMarker)),
                (3, 5, Style::Glossary),
                (5, 8, Style::Glossary),
                (8, 12, Style::Token(Tok::Plain)),
            ]
        );
    }

    #[test]
    fn ohne_treffer_bleibt_alles_gleich() {
        let spans = vec![span(0, 4, Tok::Heading), span(4, 9, Tok::Plain)];
        let out = intersect(&spans, &[], 9);
        assert_eq!(
            out,
            vec![
                (0, 4, Style::Token(Tok::Heading)),
                (4, 9, Style::Token(Tok::Plain)),
            ]
        );
    }

    #[test]
    fn mehrere_treffer_in_einem_span() {
        let spans = vec![span(0, 12, Tok::Plain)];
        let hits = vec![
            GlossaryHit { start: 1, end: 4, index: 0 },
            GlossaryHit { start: 7, end: 10, index: 1 },
        ];
        let out = intersect(&spans, &hits, 12);
        assert_eq!(
            out,
            vec![
                (0, 1, Style::Token(Tok::Plain)),
                (1, 4, Style::Glossary),
                (4, 7, Style::Token(Tok::Plain)),
                (7, 10, Style::Glossary),
                (10, 12, Style::Token(Tok::Plain)),
            ]
        );
    }
}

#[cfg(test)]
mod glossar_ordner_tests {
    use super::*;

    #[test]
    fn glossary_entry_has_aliases() {
        let e = GlossaryEntry {
            term: "Rust".into(),
            aliases: vec!["rustlang".into(), "RustLang".into()],
            path: PathBuf::from("/v/Rust.md"),
        };
        assert_eq!(e.aliases.len(), 2);
    }

    #[test]
    fn aliase_werden_als_muster_indiziert() {
        let gl = Glossary::new(
            vec![GlossaryEntry {
                term: "Rust".into(),
                aliases: vec!["Ferris-Sprache".into()],
                path: PathBuf::from("/v/Rust.md"),
            }],
            true,
        );
        // Begriff UND Alias treffen, beide auf denselben Eintrag:
        let t1 = gl.find("Rust ist toll");
        let t2 = gl.find("Die Ferris-Sprache ist toll");
        assert_eq!(t1.len(), 1);
        assert_eq!(t2.len(), 1);
        assert_eq!(t1[0].index, t2[0].index);
    }

    #[test]
    fn glossary_folder_filters_notes() {
        use crate::vault::Vault;
        use std::fs;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("rusty-glossar-{}-{}", 
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), n));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Rust.md"), "# Rust").unwrap();
        fs::create_dir_all(dir.join("Glossary")).unwrap();
        fs::write(dir.join("Glossary/egui.md"), "# egui").unwrap();
        fs::write(dir.join("Willkommen.md"), "# Hi").unwrap();

        let mut v = Vault::open(&dir).unwrap();
        v.scan().unwrap();
        let glossary_notes: Vec<_> = v
            .notes_in(&["Glossary".to_string()])
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(glossary_notes, vec!["egui.md"]);
    }
}
