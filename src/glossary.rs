//! Glossar: virtuelle Verlinkungen von Begriffen im Text (Inspiration: das
//! Obsidian-Plugin "Virtual Linker / Glossary"), neu gedacht als
//! Aho-Corasick-Automat: ein Durchlauf über den Text liefert alle Treffer
//! (O(Textlänge + Treffer)), statt jeden Begriff einzeln zu suchen. Das
//! Dokument selbst bleibt unverändert — die Verlinkung ist rein visuell.

use std::path::PathBuf;

use aho_corasick::{AhoCorasick, MatchKind};

/// Ein Glossar-Eintrag: Begriff (Stichwort/Alias) und die Notiz dahinter.
#[derive(Debug, Clone, PartialEq)]
pub struct GlossarEintrag {
    pub begriff: String,
    pub pfad: PathBuf,
}

/// Ein Treffer im Text: Byte-Range plus Index in die Eintragsliste.
#[derive(Debug, Clone, PartialEq)]
pub struct GlossarTreffer {
    pub start: usize,
    pub end: usize,
    pub index: usize,
}

/// Byte-erhaltende Kleinschreibung: A-Z → a-z sowie lateinische
/// Großbuchstaben mit Diakritika (C3 80/C3 82/…/C3 9E, also ÄÖÜ usw.) →
/// ihre Kleinschreibung (+0x20 im zweiten UTF-8-Byte). Die Länge bleibt
/// unverändert, sodass Byte-Offsets zwischen Original und Kopie identisch sind.
fn klein_bytes(src: &[u8]) -> Vec<u8> {
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

fn ist_wortzeichen(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

#[derive(Debug)]
pub struct Glossar {
    ac: AhoCorasick,
    eintraege: Vec<GlossarEintrag>,
    case_insensitive: bool,
}

impl Glossar {
    /// Baut den Automaten neu auf (nur bei Vault-Scan oder Einstellungsänderung).
    /// Duplikat-Begriffe: der erste gewinnt. Leere Begriffe werden verworfen.
    pub fn neu(eintraege: Vec<GlossarEintrag>, case_insensitive: bool) -> Glossar {
        let mut gesehen = std::collections::HashSet::new();
        let mut muster: Vec<Vec<u8>> = Vec::new();
        let mut gefiltert: Vec<GlossarEintrag> = Vec::new();
        for e in eintraege {
            let begriff = e.begriff.trim();
            if begriff.is_empty() {
                continue;
            }
            let bytes = if case_insensitive {
                klein_bytes(begriff.as_bytes())
            } else {
                begriff.as_bytes().to_vec()
            };
            if !gesehen.insert(bytes.clone()) {
                continue;
            }
            muster.push(bytes);
            gefiltert.push(GlossarEintrag {
                begriff: begriff.to_string(),
                pfad: e.pfad,
            });
        }
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&muster)
            .expect("Aho-Corasick mit gültigen Mustern");
        Glossar {
            ac,
            eintraege: gefiltert,
            case_insensitive,
        }
    }

    pub fn eintraege(&self) -> &[GlossarEintrag] {
        &self.eintraege
    }

    pub fn ist_leer(&self) -> bool {
        self.eintraege.is_empty()
    }

    /// Alle Begriffs-Treffer in `text` — nur an Wortgrenzen, links-längster
    /// Begriff gewinnt. Treffer innerhalb von Code (`…`, ```…```) und
    /// bestehenden Links ([[…]], […](…)) werden ignoriert.
    pub fn finde(&self, text: &str) -> Vec<GlossarTreffer> {
        if self.eintraege.is_empty() || text.is_empty() {
            return Vec::new();
        }

        // Bereiche, in denen NICHT verlinkt wird: Code-Spans, Code-Blöcke,
        // Wikilinks, Markdown-Links.
        let geschuetzt = schutz_bereiche(text);
        let bytes = text.as_bytes();
        // Für case-insensitives Suchen: byte-erhaltende Kleinschreibung —
        // gleiche Länge, also stimmen alle Offsets mit dem Original überein.
        let such_text: Vec<u8> = if self.case_insensitive {
            klein_bytes(bytes)
        } else {
            bytes.to_vec()
        };
        let mut treffer = Vec::new();

        for m in self.ac.find_iter(&such_text[..]) {
            // Wortgrenzen prüfen (im Original!)
            let links_ok = m.start() == 0 || !ist_wortzeichen(bytes[m.start() - 1]);
            let rechts_ok = m.end() >= bytes.len() || !ist_wortzeichen(bytes[m.end()]);
            if !links_ok || !rechts_ok {
                continue;
            }
            // Geschützte Bereiche überspringen
            if geschuetzt_treffer(&geschuetzt, m.start(), m.end()) {
                continue;
            }
            treffer.push(GlossarTreffer {
                start: m.start(),
                end: m.end(),
                index: m.pattern().as_usize(),
            });
        }
        treffer
    }
}

/// Halboffene Intervalle [start, end) geschützter Bereiche.
type Bereiche = Vec<(usize, usize)>;

fn geschuetzt_treffer(bereiche: &Bereiche, start: usize, end: usize) -> bool {
    bereiche
        .iter()
        .any(|&(s, e)| start < e && end > s)
}

fn schutz_bereiche(text: &str) -> Bereiche {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut bereiche: Bereiche = Vec::new();
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

    fn g(terms: &[(&str, &str)]) -> Glossar {
        Glossar::neu(
            terms
                .iter()
                .map(|(b, p)| GlossarEintrag {
                    begriff: b.to_string(),
                    pfad: PathBuf::from(p),
                })
                .collect(),
            true,
        )
    }

    #[test]
    fn findet_begriff_mit_position() {
        let gl = g(&[("Rust", "/v/Rust.md")]);
        let treffer = gl.finde("Lerne Rust heute.");
        assert_eq!(treffer.len(), 1);
        assert_eq!(
            &"Lerne Rust heute."[treffer[0].start..treffer[0].end],
            "Rust"
        );
        assert_eq!(gl.eintraege()[0].begriff, "Rust");
    }

    #[test]
    fn gross_klein_wird_ignoriert() {
        let gl = g(&[("rust", "/v/Rust.md")]);
        assert_eq!(gl.finde("RUST und ruSt").len(), 2);
    }

    #[test]
    fn nur_ganze_woerter() {
        let gl = g(&[("Rust", "/v/Rust.md")]);
        // "Trust" (Begriff in der Mitte) und "rusty" (Suffix) dürfen nicht treffen
        assert_eq!(gl.finde("Trust rusty Rust").len(), 1);
    }

    #[test]
    fn laengster_begriff_gewinnt() {
        let gl = g(&[("Note", "/v/Note.md"), ("Note Pad", "/v/Note Pad.md")]);
        let treffer = gl.finde("im Note Pad");
        assert_eq!(treffer.len(), 1);
        assert_eq!(treffer[0].index, 1, "das längere Muster 'Note Pad' gewinnt");
    }

    #[test]
    fn leeres_glossar_und_leerer_text() {
        let gl = g(&[]);
        assert!(gl.finde("irgendwas").is_empty());
        let gl2 = g(&[("x", "/v/x.md")]);
        assert!(gl2.finde("").is_empty());
    }

    #[test]
    fn code_und_links_bleiben_unverlinkt() {
        let gl = g(&[("Rust", "/v/Rust.md")]);
        let text = "`Rust` und [[Rust]] und [Rust](x.md), aber Rust ja.";
        let treffer = gl.finde(text);
        assert_eq!(treffer.len(), 1, "nur das freie 'Rust' trifft");
        assert_eq!(&text[treffer[0].start..treffer[0].end], "Rust");
        assert!(treffer[0].start > text.find(", aber").unwrap());
    }

    #[test]
    fn duplikate_werden_verworfen() {
        let gl = g(&[("Rust", "/v/a.md"), ("Rust", "/v/b.md")]);
        assert_eq!(gl.eintraege().len(), 1);
        assert_eq!(gl.eintraege()[0].pfad, PathBuf::from("/v/a.md"));
    }
}
