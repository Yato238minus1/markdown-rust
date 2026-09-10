//! Editor support: byte<->line mapping and incremental syntax highlighting.

use crate::markdown;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: usize,
    pub col: usize,
}

/// Maps between byte offsets and (line, col) for a fixed text.
pub struct LineIndex {
    /// Byte offset where each line starts; always at least [0].
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        LineIndex { starts, len: text.len() }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.len)
    }

    /// Byte offset of a (line, col). Clamps to the text end.
    pub fn byte_of(&self, lc: LineCol) -> usize {
        let line = lc.line.min(self.starts.len() - 1);
        let start = self.starts[line];
        let end = self.starts.get(line + 1).copied().unwrap_or(self.len);
        start + lc.col.min(end - start)
    }

    pub fn line_col_of(&self, byte: usize) -> LineCol {
        let byte = byte.min(self.len);
        let line = match self.starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        LineCol { line, col: byte - self.starts[line] }
    }
}

/// Semantic token kinds the theme maps to colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Heading,
    BoldItalic,
    InlineCode,
    CodeBlock,
    Quote,
    Link,
    ListMarker,
    FrontMatter,
    Plain,
}

/// One highlighted range: `[start, end)` bytes with kind `tok`.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub tok: Tok,
}

struct SpanBuilder<'a> {
    text: &'a str,
    spans: Vec<Span>,
}

impl<'a> SpanBuilder<'a> {
    fn push(&mut self, start: usize, end: usize, tok: Tok) {
        if end <= start {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.end == start && last.tok == tok {
                last.end = end;
                return;
            }
        }
        self.spans.push(Span { start, end, tok });
    }

    /// Highlight inline constructs (`code`, **bold**, _em_, [[wiki]], [link](url))
    /// within [start, end); anything else becomes Plain.
    fn inline(&mut self, start: usize, end: usize) {
        let bytes = self.text.as_bytes();
        let mut i = start;
        while i < end {
            let b = bytes[i];
            let mut matched = false;
            match b {
                b'`' => {
                    if let Some(close) =
                        self.text[i + 1..end].find('`').map(|p| p + i + 1)
                    {
                        self.push(i, close + 1, Tok::InlineCode);
                        i = close + 1;
                        matched = true;
                    }
                }
                b'*' | b'_' => {
                    let doubled = i + 1 < end && bytes[i + 1] == b;
                    let needle: &[u8] = if doubled { &[b, b] } else { &[b] };
                    let search_from = i + needle.len();
                    if let Some(rel) = find_sub(bytes, needle, search_from, end) {
                        let stop = rel + needle.len();
                        // skip empty emphasis
                        if stop > i + needle.len() {
                            self.push(i, stop, Tok::BoldItalic);
                            i = stop;
                            matched = true;
                        }
                    }
                }
                b'[' | b'!' => {
                    let bracket = if b == b'[' { i } else { i + 1 };
                    if b == b'!' && bracket >= end {
                        // fallthrough to plain below
                    } else if bracket + 1 < end && bytes[bracket + 1] == b'[' {
                        if let Some(close) =
                            self.text[bracket + 2..end].find("]]").map(|p| p + bracket + 2)
                        {
                            self.push(i, close + 2, Tok::Link);
                            i = close + 2;
                            matched = true;
                        }
                    } else if bracket < end && bytes[bracket] == b'[' {
                        if let Some(rb) =
                            self.text[bracket + 1..end].find(']').map(|p| p + bracket + 1)
                        {
                            if rb + 1 < end && bytes[rb + 1] == b'(' {
                                if let Some(rp) =
                                    self.text[rb + 1..end].find(')').map(|p| p + rb + 1)
                                {
                                    self.push(i, rp + 1, Tok::Link);
                                    i = rp + 1;
                                    matched = true;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            if !matched {
                self.push(i, i + 1, Tok::Plain);
                i += 1;
            }
        }
    }
}

fn find_sub(haystack: &[u8], needle: &[u8], from: usize, to: usize) -> Option<usize> {
    if needle.is_empty() || from >= to {
        return None;
    }
    let mut i = from;
    while i + needle.len() <= to {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// Highlight Markdown text into contiguous spans covering [0, text.len()).
pub fn highlight(text: &str) -> Vec<Span> {
    let mut sb = SpanBuilder { text, spans: Vec::new() };

    // Optional front matter block up front.
    let mut pos = 0usize;
    let (fm_raw, body) = markdown::split_front_matter(text);
    if fm_raw.is_some() {
        let fm_end_incl_delim = text.len() - body.len(); // start of "\n" after closing ---
        sb.push(0, fm_end_incl_delim.saturating_sub(1), Tok::FrontMatter);
        pos = fm_end_incl_delim.saturating_sub(1);
    }

    let bytes = text.as_bytes();
    let len = text.len();
    let mut in_fence = false;

    while pos < len {
        let line_end = bytes[pos..len]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| p + pos)
            .unwrap_or(len);
        let line = &text[pos..line_end];

        if is_fence(line) {
            sb.push(pos, line_end, Tok::CodeBlock);
            in_fence = if in_fence {
                let opens_new = line.trim_start()[3..].trim_start().starts_with("```");
                !opens_new
            } else {
                true
            };
        } else if in_fence {
            sb.push(pos, line_end, Tok::CodeBlock);
        } else if line.starts_with('#') && line['#'.len_utf8()..].starts_with([' ', '\t']) {
            sb.push(pos, line_end, Tok::Heading);
        } else if line.starts_with('>') {
            sb.push(pos, line_end, Tok::Quote);
        } else {
            let indent = line.len() - line.trim_start().len();
            let rest = &line[indent..];
            let marker_len = if rest.starts_with("- ")
                || rest.starts_with("* ")
                || rest.starts_with("+ ")
            {
                2
            } else {
                let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
                if digits > 0
                    && rest[digits..].starts_with(". ")
                {
                    digits + 2
                } else {
                    0
                }
            };
            if marker_len > 0 {
                sb.push(pos, pos + indent + marker_len, Tok::ListMarker);
                sb.inline(pos + indent + marker_len, line_end);
            } else {
                sb.inline(pos, line_end);
            }
        }

        if line_end < len {
            sb.push(line_end, line_end + 1, Tok::Plain); // the newline itself
            pos = line_end + 1;
        } else {
            pos = line_end;
        }
    }

    sb.spans
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_maps_offsets_both_ways() {
        let text = "ab\ncd\nef";
        let ix = LineIndex::new(text);
        assert_eq!(ix.line_count(), 3);
        assert_eq!(ix.line_start(0), 0);
        assert_eq!(ix.line_start(1), 3);
        assert_eq!(ix.line_start(2), 6);

        let lc = ix.line_col_of(4); // the 'd'
        assert_eq!(lc, LineCol { line: 1, col: 1 });
        assert_eq!(ix.byte_of(LineCol { line: 1, col: 1 }), 4);
    }

    #[test]
    fn line_index_handles_empty_text_and_clamps() {
        let ix = LineIndex::new("");
        assert_eq!(ix.line_count(), 1);
        assert_eq!(ix.line_col_of(999), LineCol { line: 0, col: 0 });

        let ix2 = LineIndex::new("one\n");
        assert_eq!(ix2.line_count(), 2); // trailing newline opens a new last line
        assert_eq!(ix2.byte_of(LineCol { line: 99, col: 99 }), 4);
    }

    #[test]
    fn highlight_covers_whole_document_exactly_once() {
        let text = "---\ntitle: t\n---\n\n# Head\n\nSome **bold** and `code`.\n\n```rust\nlet x = 1;\n```\n\n> quote\n\n- item\n[link](http://x)\n";
        let spans = highlight(text);
        assert!(!spans.is_empty());
        // spans are contiguous, non-overlapping, cover [0, len)
        let mut cursor = 0usize;
        for s in &spans {
            assert_eq!(s.start, cursor, "gap/overlap at {}", s.start);
            assert!(s.end > s.start);
            cursor = s.end;
        }
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn highlight_recognizes_major_constructs() {
        let text = "# Title\n\n**bold** `code` > not-quote\n- item\n[[Wiki]] plain\n";
        let spans = highlight(text);
        let has = |t: Tok| spans.iter().any(|s| s.tok == t && &text[s.start..s.end] != "");
        assert!(has(Tok::Heading));
        assert!(has(Tok::BoldItalic));
        assert!(has(Tok::InlineCode));
        assert!(has(Tok::ListMarker));

        // heading span covers the marker too
        let head = spans.iter().find(|s| s.tok == Tok::Heading).unwrap();
        assert_eq!(&text[head.start..head.end], "# Title");
    }

    #[test]
    fn highlight_front_matter_block() {
        let text = "---\ntitle: x\n---\nbody";
        let spans = highlight(text);
        let fm: Vec<_> = spans.iter().filter(|s| s.tok == Tok::FrontMatter).collect();
        assert!(!fm.is_empty());
        assert_eq!(&text[fm[0].start..fm[0].end], "---\ntitle: x\n---");
    }

    #[test]
    fn highlight_code_block_content() {
        let text = "```js\nvar x = 1;\n```\nafter";
        let spans = highlight(text);
        let code: Vec<_> = spans
            .iter()
            .filter(|s| s.tok == Tok::CodeBlock && text[s.start..s.end].contains("var"))
            .collect();
        assert_eq!(code.len(), 1);
    }
}
