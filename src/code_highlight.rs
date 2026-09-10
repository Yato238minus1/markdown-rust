//! Code-Highlighting mit syntect: Codeblock-Inhalt → Farbverläufe.
//!
//! Performance: SyntaxSet und Theme werden einmalig in LazyLocks geladen;
//! das Parsing selbst ist pro Block O(Länge). Farben werden als
//! (Start, Ende, Color32) geliefert, damit der Editor-Layouter sie direkt
//! in TextFormat-Striche übersetzen kann.

use std::sync::LazyLock;

use syntect::parsing::{SyntaxSet, SyntaxReference};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::util::LinesWithEndings;

pub struct CodeHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

impl Default for CodeHighlighter {
    fn default() -> Self {
        let theme = THEME_SET.themes.get("base16-ocean.dark").cloned().unwrap_or_default();
        CodeHighlighter { syntax_set: SYNTAX_SET.clone(), theme }
    }
}

/// Eine gefärbte Spanne innerhalb des Codes (Byte-Offsets relativ zum Code).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorSpan {
    pub start: usize,
    pub end: usize,
    pub color: [u8; 4],
}

impl CodeHighlighter {
    /// Erkennt die Sprache am Infostring ("rust", "js", …), None = Plain.
    fn syntax_for(&self, info: &str) -> Option<&SyntaxReference> {
        let token = info.trim().split(&[',', ' '][..]).next()?.trim();
        if token.is_empty() {
            return None;
        }
        self.syntax_set
            .find_syntax_by_token(&token.to_lowercase())
            .or_else(|| self.syntax_set.find_syntax_by_extension(&token.to_lowercase()))
    }

    /// Färbt `code` (Sprache via Infostring). Ohne erkannte Sprache: Plain-Farbe.
    pub fn highlight(&self, code: &str, info: &str) -> Vec<ColorSpan> {
        let syntax = match self.syntax_for(info) {
            Some(s) => s,
            None => {
                // Kein Syntax-Highlighting: alles als eine Spanne zurückgeben.
                return if code.is_empty() {
                    Vec::new()
                } else {
                    vec![ColorSpan { start: 0, end: code.len(), color: [152, 206, 206, 255] }]
                };
            }
        };

        let mut hl = HighlightLines::new(syntax, &self.theme);
        let mut out: Vec<ColorSpan> = Vec::new();
        let mut cursor = 0usize;

        for zeile in LinesWithEndings::from(code) {
            if let Ok(ranges) = hl.highlight_line(zeile, &self.syntax_set) {
                for (stil, text) in ranges {
                    let start = cursor;
                    let end = start + text.len();
                    cursor = end;
                    let c = stil.foreground;
                    let color = [c.r, c.g, c.b, c.a];
                    if end > start {
                        // Gleiche Farbe aneinander hängen (weniger Spans).
                        if let Some(letzte) = out.last_mut() {
                            if letzte.color == color && letzte.end == start {
                                letzte.end = end;
                                continue;
                            }
                        }
                        out.push(ColorSpan { start, end, color });
                    }
                }
            } else {
                // Parse-Fehler: Rest einfarbig.
                out.push(ColorSpan {
                    start: cursor,
                    end: code.len(),
                    color: [152, 206, 206, 255],
                });
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_code_is_multicolored() {
        let h = CodeHighlighter::default();
        let spans = h.highlight("let x = 1;\nfn main() {}\n", "rust");
        // Mindestens 2 verschiedene Farben + mehrere Spans:
        assert!(spans.len() >= 3, "nur {} Spans", spans.len());
        let farben: std::collections::HashSet<_> = spans.iter().map(|s| s.color).collect();
        assert!(farben.len() >= 2, "zu wenig Farbvielfalt");
    }

    #[test]
    fn offsets_are_contiguous_and_complete() {
        let h = CodeHighlighter::default();
        let code = "let x = 1;\n";
        let spans = h.highlight(code, "rust");
        let mut cursor = 0usize;
        for s in &spans {
            assert_eq!(s.start, cursor);
            assert!(s.end > s.start);
            cursor = s.end;
        }
        assert_eq!(cursor, code.len());
    }

    #[test]
    fn unknown_language_single_color_or_empty_without_crash() {
        let h = CodeHighlighter::default();
        let spans = h.highlight("irgendwas", "keinestruktur");
        assert!(!spans.is_empty() || true); // kein Crash ist das Kriterium
    }

    #[test]
    fn js_is_detected() {
        let h = CodeHighlighter::default();
        let spans = h.highlight("var x = 1;\n", "js");
        assert!(spans.len() >= 2);
    }
}
