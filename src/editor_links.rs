//! Editor-Interaktion: Link-Erkennung unter dem Cursor/der Maus.

/// Ein Wikilink im Text: Ziel und Byte-Range.
#[derive(Debug, Clone, PartialEq)]
pub struct WikiLinkRange {
    pub target: String,
    pub start: usize,
    pub end: usize,
}

/// Findet den `[[Link]]`, der `byte_pos` enthält (oder None).
/// `[[Ziel|Alias]]`: die Range umfasst alles, das Ziel ist der Teil vor `|`.
pub fn wikilink_at(text: &str, byte_pos: usize) -> Option<WikiLinkRange> {
    if text.is_empty() || byte_pos > text.len() {
        return None;
    }
    // Position auf ein Zeichen innerhalb setzen (byte_pos kann auf einer
    // UTF-8-Grenze liegen; wir suchen die Umgebung).
    let bytes = text.as_bytes();

    // 1) Wikilink: das letzte "[[", dessen "]]" hinter byte_pos liegt.
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find("[[") {
        let open = search_from + rel;
        match text[open + 2..].find("]]") {
            Some(close_rel) => {
                let close = open + 2 + close_rel + 2;
                if byte_pos >= open && byte_pos < close {
                    let inner = &text[open + 2..close - 2];
                    let target = inner.split('|').next().unwrap_or(inner).trim().to_string();
                    return Some(WikiLinkRange {
                        target,
                        start: open,
                        end: close,
                    });
                }
                if open + 2 > byte_pos && open > byte_pos {
                    break; // weiter hinten liegende Links können nicht mehr treffen
                }
                search_from = open + 2;
            }
            None => break,
        }
    }

    // 2) Markdown-Link [Text](Ziel): '(' kann vor ODER knapp hinter pos liegen,
    // solange pos innerhalb [Text] liegt.
    if byte_pos <= bytes.len() {
        let paren_opt = text[..byte_pos.min(text.len())].rfind('(').or_else(|| {
            // '(' direkt nach der Klammer?
            let rest = &text[byte_pos..];
            if rest.starts_with('(') {
                Some(byte_pos)
            } else {
                rest.find('(').map(|p| byte_pos + p).filter(|&p| {
                    // nur wenn zwischen pos und ( nur Text+']' liegt
                    text[byte_pos..p].chars().all(|c| c != '\n')
                })
            }
        });
        if let Some(paren) = paren_opt {
            let paren = paren;
            if paren > 0 && bytes[paren - 1] == b']' {
                // '[' zum passenden ']' suchen
                if let Some(bracket_rel) = text[..paren - 1].rfind('[') {
                    let bracket = bracket_rel;
                    // Ziel: von paren+1 bis ')'
                    if let Some(close_paren_rel) = text[paren + 1..].find(')') {
                        let close_paren = paren + 1 + close_paren_rel + 1;
                        if byte_pos >= bracket && byte_pos < close_paren {
                            let target = text[paren + 1..close_paren - 1]
                                .trim()
                                .trim_start_matches("rusty-note:")
                                .to_string();
                            return Some(WikiLinkRange {
                                target,
                                start: bracket,
                                end: close_paren,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_link_at_position() {
        let text = "Siehe [[Rust GUI Notes]] heute.";
        // Position auf "GUI" (byte 12)
        let pos = text.find("GUI").unwrap();
        let b = wikilink_at(text, pos).unwrap();
        assert_eq!(b.target, "Rust GUI Notes");
        assert_eq!(&text[b.start..b.end], "[[Rust GUI Notes]]");
    }

    #[test]
    fn alias_link_returns_target() {
        let text = "[[Ziel|Alias]]";
        let b = wikilink_at(text, 3).unwrap();
        assert_eq!(b.target, "Ziel");
        assert_eq!(&text[b.start..b.end], "[[Ziel|Alias]]");
    }

    #[test]
    fn outside_links_is_none() {
        assert!(wikilink_at("kein link hier", 4).is_none());
        assert!(wikilink_at("[[offen", 3).is_none());
        assert!(wikilink_at("", 0).is_none());
    }

    #[test]
    fn edge_position_counts() {
        let text = "[[A]]";
        assert!(wikilink_at(text, 0).is_some(), "auf '['");
        assert!(wikilink_at(text, 4).is_some(), "auf ']'");
        assert!(wikilink_at(text, 5).is_none(), "direkt danach");
    }

    #[test]
    fn markdown_link_is_detected_too() {
        let text = "klick [Text](Andere.md) bitte";
        let pos = text.find("Text").unwrap();
        let b = wikilink_at(text, pos).unwrap();
        assert_eq!(b.start, text.find("[Text]").unwrap());
        assert!(b.target == "Andere.md");
    }
}
