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
    /// Zusätzliche Schreibweisen, die denselben Eintrag treffen.
    pub aliase: Vec<String>,
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
    /// AC-Muster-Index -> Index in `eintraege` (Aliase teilen sich den Eintrag).
    muster_zu_eintrag: Vec<usize>,
    case_insensitive: bool,
}

impl Glossar {
    /// Baut den Automaten neu auf (nur bei Vault-Scan oder Einstellungsänderung).
    /// Duplikat-Begriffe: der erste gewinnt. Leere Begriffe werden verworfen.
    pub fn neu(eintraege: Vec<GlossarEintrag>, case_insensitive: bool) -> Glossar {
        let mut gesehen = std::collections::HashSet::new();
        let mut muster: Vec<Vec<u8>> = Vec::new();
        let mut muster_zu_eintrag: Vec<usize> = Vec::new();
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
            let eintrag_idx = gefiltert.len();
            muster.push(bytes);
            muster_zu_eintrag.push(eintrag_idx);
            gefiltert.push(GlossarEintrag {
                begriff: begriff.to_string(),
                aliase: e.aliase.clone(),
                pfad: e.pfad,
            });
            // Aliase als zusätzliche Muster; index zeigt auf denselben Eintrag.
            for alias in &e.aliase {
                let alias = alias.trim();
                if alias.is_empty() {
                    continue;
                }
                let alias_bytes = if case_insensitive {
                    klein_bytes(alias.as_bytes())
                } else {
                    alias.as_bytes().to_vec()
                };
                if !gesehen.insert(alias_bytes.clone()) {
                    continue;
                }
                muster.push(alias_bytes);
                muster_zu_eintrag.push(eintrag_idx);
            }
        }
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&muster)
            .expect("Aho-Corasick mit gültigen Mustern");
        Glossar {
            ac,
            eintraege: gefiltert,
            muster_zu_eintrag,
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
            let muster_idx = m.pattern().as_usize();
            let eintrag_idx = self
                .muster_zu_eintrag
                .get(muster_idx)
                .copied()
                .unwrap_or(muster_idx);
            treffer.push(GlossarTreffer {
                start: m.start(),
                end: m.end(),
                index: eintrag_idx,
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
                    aliase: Vec::new(),
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

// ---------------------------------------------------------------------------
// Verschneidung mit Editor-Highlighting
// ---------------------------------------------------------------------------

/// Stil eines Anzeigebereichs: normales Token oder Glossar-Begriff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stil {
    Token(crate::editor::Tok),
    Glossar,
}

/// Verschneidet kontiguous Highlight-Spans mit Glossar-Treffern: Treffer-
/// Bereiche bekommen `Stil::Glossar`, der Rest behält sein Token. Beide
/// Eingaben müssen aufsteigend sortiert und nichtüberlappend sein.
pub fn verschneide(
    spans: &[crate::editor::Span],
    treffer: &[GlossarTreffer],
    text_len: usize,
) -> Vec<(usize, usize, Stil)> {
    let mut out: Vec<(usize, usize, Stil)> = Vec::new();
    let mut cursor = 0usize; // aktuelle Position im Dokument
    let mut si = 0usize; // Index in spans
    let mut ti = 0usize; // Index in treffer

    while cursor < text_len {
        // Nächster Ereignis-Punkt: Ende des aktuellen Spans oder des Treffers.
        let span_rest = si < spans.len() && spans[si].end > cursor;
        let treffer_rest = ti < treffer.len() && treffer[ti].end > cursor;

        if !span_rest && !treffer_rest {
            // Rest als Plain auffüllen (sollte bei voller Abdeckung nicht passieren)
            out.push((cursor, text_len, Stil::Token(crate::editor::Tok::Plain)));
            break;
        }

        // Grenzen: Ende des Spans, Anfang UND Ende des Treffers.
        let mut grenzen: Vec<usize> = Vec::new();
        if span_rest {
            grenzen.push(spans[si].end);
        }
        if treffer_rest {
            grenzen.push(treffer[ti].end);
            if treffer[ti].start > cursor {
                grenzen.push(treffer[ti].start);
            }
        }
        let next_end = grenzen.iter().copied().min().unwrap_or(text_len);

        let in_treffer = treffer_rest && treffer[ti].start <= cursor;
        let stil = if in_treffer {
            Stil::Glossar
        } else {
            Stil::Token(spans[si].tok)
        };
        out.push((cursor, next_end, stil));

        if span_rest && spans[si].end == next_end {
            si += 1;
        }
        if treffer_rest && treffer[ti].end == next_end {
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
        let treffer = vec![GlossarTreffer { start: 3, end: 7, index: 0 }];
        let out = verschneide(&spans, &treffer, 10);
        assert_eq!(
            out,
            vec![
                (0, 3, Stil::Token(Tok::Plain)),
                (3, 7, Stil::Glossar),
                (7, 10, Stil::Token(Tok::Plain)),
            ]
        );
    }

    #[test]
    fn treffer_ueber_mehrere_spans() {
        let spans = vec![
            span(0, 5, Tok::ListMarker),
            span(5, 12, Tok::Plain),
        ];
        let treffer = vec![GlossarTreffer { start: 3, end: 8, index: 0 }];
        let out = verschneide(&spans, &treffer, 12);
        assert_eq!(
            out,
            vec![
                (0, 3, Stil::Token(Tok::ListMarker)),
                (3, 5, Stil::Glossar),
                (5, 8, Stil::Glossar),
                (8, 12, Stil::Token(Tok::Plain)),
            ]
        );
    }

    #[test]
    fn ohne_treffer_bleibt_alles_gleich() {
        let spans = vec![span(0, 4, Tok::Heading), span(4, 9, Tok::Plain)];
        let out = verschneide(&spans, &[], 9);
        assert_eq!(
            out,
            vec![
                (0, 4, Stil::Token(Tok::Heading)),
                (4, 9, Stil::Token(Tok::Plain)),
            ]
        );
    }

    #[test]
    fn mehrere_treffer_in_einem_span() {
        let spans = vec![span(0, 12, Tok::Plain)];
        let treffer = vec![
            GlossarTreffer { start: 1, end: 4, index: 0 },
            GlossarTreffer { start: 7, end: 10, index: 1 },
        ];
        let out = verschneide(&spans, &treffer, 12);
        assert_eq!(
            out,
            vec![
                (0, 1, Stil::Token(Tok::Plain)),
                (1, 4, Stil::Glossar),
                (4, 7, Stil::Token(Tok::Plain)),
                (7, 10, Stil::Glossar),
                (10, 12, Stil::Token(Tok::Plain)),
            ]
        );
    }
}

#[cfg(test)]
mod glossar_ordner_tests {
    use super::*;

    #[test]
    fn glossareintrag_hat_aliase() {
        let e = GlossarEintrag {
            begriff: "Rust".into(),
            aliase: vec!["rustlang".into(), "RustLang".into()],
            pfad: PathBuf::from("/v/Rust.md"),
        };
        assert_eq!(e.aliase.len(), 2);
    }

    #[test]
    fn aliase_werden_als_muster_indiziert() {
        let gl = Glossar::neu(
            vec![GlossarEintrag {
                begriff: "Rust".into(),
                aliase: vec!["Ferris-Sprache".into()],
                pfad: PathBuf::from("/v/Rust.md"),
            }],
            true,
        );
        // Begriff UND Alias treffen, beide auf denselben Eintrag:
        let t1 = gl.finde("Rust ist toll");
        let t2 = gl.finde("Die Ferris-Sprache ist toll");
        assert_eq!(t1.len(), 1);
        assert_eq!(t2.len(), 1);
        assert_eq!(t1[0].index, t2[0].index);
    }

    #[test]
    fn glossar_ordner_filtert_notizen() {
        use crate::vault::Vault;
        use std::fs;
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("rusty-glossar-{}-{}", 
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), n));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Rust.md"), "# Rust").unwrap();
        fs::create_dir_all(dir.join("Glossar")).unwrap();
        fs::write(dir.join("Glossar/egui.md"), "# egui").unwrap();
        fs::write(dir.join("Willkommen.md"), "# Hi").unwrap();

        let mut v = Vault::open(&dir).unwrap();
        v.scan().unwrap();
        let glossar_notizen: Vec<_> = v
            .notizen_aus(&["Glossar".to_string()])
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(glossar_notizen, vec!["egui.md"]);
    }
}
