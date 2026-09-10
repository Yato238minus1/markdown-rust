//! Inhalts-basierte Sync-Scroll-Hilfen.
//!
//! `egui_commonmark` gibt keine Quell-Byte→Y-Karte der gerenderten Vorschau
//! frei, daher schätzen wir die gerenderte Höhe pro Markdown-Block selbst ab
//! (mit egui's Font-Messung). Damit lässt sich der *erste sichtbare Quell-Byte*
//! des Editors auf die gleiche Quell-Position in der Vorschau abbilden – statt
//! nur proportional nach Scroll-Tiefe zu gehen (was bei unterschiedlichen
//! Schriftgrößen drifts).
//!
//! WICHTIG: egui_commonmark rendert einen *ganzen Absatz* als EIN Widget und
//! setzt `item_spacing.y` nur *zwischen* Blöcken. Eine Zeile-für-Zeile-Schätzung
//! (mit Spacing pro Quellzeile) drifts daher bei langen/absatzweisen Texten.
//! Wir zerlegen daher in echte Markdown-Blöcke (Absatz, Überschrift, Liste,
//! Code) und messen jeden Block mit egui's Layout (inkl. WRAPPING) – exakt so,
//! wie egui_commonmark ihn rendert.

/// Ein Markdown-Block der Quelle.
enum Block {
    /// Fließtext-Absatz (mehrere Quellzeilen gehören zusammen).
    Absatz,
    /// Überschrift der Stufe 1..6.
    Ueberschrift(usize),
    /// Listeneintrag.
    Liste,
    /// Zitat.
    Zitat,
    /// Code-Zeile innerhalb eines Fences (wird NICHT umgebrochen).
    Code,
    /// Leerzeile (kleiner Abstand).
    Leer,
}

/// Liefert zu jedem Quell-Byte-Offset (Block-Anfang) die geschätzte Y-Position
/// in der gerenderten Vorschau zurück. Die Liste ist nach Byte aufsteigend
/// sortiert und enthält einen abschließenden Eintrag mit der Gesamthöhe.
pub fn build_source_y_map(ctx: &egui::Context, text: &str, content_width: f32) -> Vec<(usize, f32)> {
    let style = ctx.global_style();
    let body_font = style
        .text_styles
        .get(&egui::TextStyle::Body)
        .cloned()
        .unwrap_or_else(|| egui::FontId::new(14.0, egui::FontFamily::Proportional));
    let heading_font = style
        .text_styles
        .get(&egui::TextStyle::Heading)
        .cloned()
        .unwrap_or_else(|| egui::FontId::new(21.0, egui::FontFamily::Proportional));
    let body_size = body_font.size;
    let heading_size = heading_font.size;
    let min_h = body_size;
    let diff = (heading_size - body_size).max(0.0);

    // egui_commonmark Heading-Faktoren (H1=Level0 .. H6=Level5)
    let heading_factors = [1.0, 0.835, 0.668, 0.501, 0.334, 0.167];
    let code_size = (body_size * 0.85).max(11.0);

    // Zeilenabstand zwischen Blöcken (egui item_spacing.y).
    let block_spacing = style.spacing.item_spacing.y.max(2.0);

    // --- Quelle in Blöcke zerlegen ---
    let lines: Vec<&str> = text.split('\n').collect();
    // byte-Offset je Quellzeile
    let mut byte_offsets = Vec::with_capacity(lines.len());
    let mut acc = 0usize;
    for l in &lines {
        byte_offsets.push(acc);
        acc += l.len() + 1; // + '\n'
    }

    // Blöcke als (start_byte, Block, zeilen_indices)
    let mut blocks: Vec<(usize, Block, Vec<usize>)> = Vec::new();
    let mut i = 0;
    let mut im_fence = false;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if im_fence {
            // Code-Zeile (auch die schließende Fence)
            let is_close = trimmed.starts_with("```") || trimmed.starts_with("~~~");
            blocks.push((byte_offsets[i], Block::Code, vec![i]));
            i += 1;
            if is_close {
                im_fence = false;
            }
            continue;
        }
        if line.trim().is_empty() {
            blocks.push((byte_offsets[i], Block::Leer, vec![i]));
            i += 1;
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            im_fence = true;
            blocks.push((byte_offsets[i], Block::Code, vec![i]));
            i += 1;
            continue;
        }
        if let Some(level) = heading_level(trimmed) {
            blocks.push((byte_offsets[i], Block::Ueberschrift(level), vec![i]));
            i += 1;
            continue;
        }
        if is_list_item(trimmed) {
            blocks.push((byte_offsets[i], Block::Liste, vec![i]));
            i += 1;
            continue;
        }
        if trimmed.starts_with(">") {
            blocks.push((byte_offsets[i], Block::Zitat, vec![i]));
            i += 1;
            continue;
        }
        // Absatz: alle folgenden nicht-leeren, nicht-Block-Start-Zeilen sammeln
        let start = i;
        let mut zeilen = vec![i];
        i += 1;
        while i < lines.len() {
            let tl = lines[i].trim();
            let tls = lines[i].trim_start();
            if tl.is_empty()
                || is_list_item(tls)
                || tls.starts_with("```")
                || tls.starts_with("~~~")
                || heading_level(tls).is_some()
                || tls.starts_with(">")
            {
                break;
            }
            zeilen.push(i);
            i += 1;
        }
        blocks.push((byte_offsets[start], Block::Absatz, zeilen));
    }

    // --- Höhen schätzen ---
    let mut map = Vec::new();
    let mut y = 0.0_f32;
    let mut prev_war_absatz = false;
    for (idx, (start, kind, zeilen)) in blocks.iter().enumerate() {
        map.push((*start, y));

        let height = match kind {
            Block::Leer => block_spacing * 0.4,
            Block::Code => {
                let mut h = 0.0_f32;
                for &zi in zeilen {
                    h += measure_line(
                        ctx,
                        lines[zi],
                        egui::FontId::new(code_size, egui::FontFamily::Monospace),
                        content_width,
                        false,
                    );
                }
                h + block_spacing * 0.3
            }
            Block::Ueberschrift(level) => {
                let lvl = (level - 1).min(5);
                let size = if lvl == 0 {
                    heading_size
                } else {
                    min_h + diff * heading_factors[lvl]
                };
                let txt = &lines[zeilen[0]][*level..];
                let h = measure_line(
                    ctx,
                    txt,
                    egui::FontId::new(size, egui::FontFamily::Proportional),
                    content_width,
                    true,
                );
                // egui_commonmark fügt vor einer Überschrift einen Zeilenumbruch
                // (newline) ein -> zusätzlicher Abstand nach oben.
                let _ = prev_war_absatz;
                h + block_spacing + size * 0.2
            }
            Block::Liste | Block::Zitat => {
                let mut joined = String::new();
                for (k, &zi) in zeilen.iter().enumerate() {
                    if k > 0 {
                        joined.push('\n');
                    }
                    joined.push_str(lines[zi].trim_start());
                }
                let h = measure_line(ctx, &joined, body_font.clone(), content_width, true);
                h + block_spacing
            }
            Block::Absatz => {
                let mut joined = String::new();
                for (k, &zi) in zeilen.iter().enumerate() {
                    if k > 0 {
                        joined.push('\n');
                    }
                    joined.push_str(lines[zi]);
                }
                let h = measure_line(ctx, &joined, body_font.clone(), content_width, true);
                h + block_spacing
            }
        };

        y += height;
        prev_war_absatz = matches!(kind, Block::Absatz);
        let _ = idx;
    }
    // Abschließender Eintrag mit Gesamthöhe.
    map.push((acc, y));
    map
}

/// Interpoliert die Y-Position für einen beliebigen Quell-Byte-Offset.
pub fn y_for_byte(map: &[(usize, f32)], byte: usize) -> f32 {
    if map.is_empty() {
        return 0.0;
    }
    let mut lo = 0usize;
    let mut hi = map.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if map[mid].0 <= byte {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let idx = lo.saturating_sub(1);
    if idx + 1 < map.len() {
        let (b0, y0) = map[idx];
        let (b1, y1) = map[idx + 1];
        if b1 > b0 {
            let t = (byte.saturating_sub(b0)) as f32 / (b1 - b0) as f32;
            y0 + t * (y1 - y0)
        } else {
            y0
        }
    } else {
        map[idx].1
    }
}

/// Misst die gerenderte Höhe eines Textes mit egui's Layout.
/// `wrap` aktiviert Zeilenumbruch bei `width` (für Fließtext/Überschriften).
/// Code wird NICHT umgebrochen.
fn measure_line(
    ctx: &egui::Context,
    text: &str,
    font: egui::FontId,
    width: f32,
    wrap: bool,
) -> f32 {
    let mut job = egui::text::LayoutJob::default();
    if wrap {
        job.wrap.max_width = width.max(1.0);
    } else {
        job.wrap.max_width = f32::INFINITY;
    }
    job.append(text, 0.0, egui::text::TextFormat::simple(font, egui::Color32::WHITE));
    ctx.fonts_mut(|f| f.layout_job(job)).size().y
}

fn heading_level(line: &str) -> Option<usize> {
    let mut hashes = 0;
    for c in line.chars() {
        if c == '#' {
            hashes += 1;
        } else {
            break;
        }
    }
    if hashes >= 1 && hashes <= 6 && line[hashes..].starts_with(' ') {
        Some(hashes)
    } else {
        None
    }
}

fn is_list_item(line: &str) -> bool {
    let rest = line.trim_start();
    if rest.is_empty() {
        return false;
    }
    let indent = line.len() - rest.len();
    if indent >= 8 {
        return false;
    }
    if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        return true;
    }
    if let Some(rest2) = rest.strip_prefix("> ") {
        return is_list_item(rest2);
    }
    let mut digits = 0;
    for c in rest.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else {
            break;
        }
    }
    digits >= 1 && digits <= 4 && rest[digits..].starts_with(". ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    fn test_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        let _ = ctx.run(eframe::egui::RawInput::default(), |_ctx| {});
        ctx
    }

    #[test]
    fn empty_text_gives_start_and_end() {
        let ctx = test_ctx();
        let map = build_source_y_map(&ctx, "", 400.0);
        assert_eq!(map.len(), 2);
        assert_eq!(map[0], (0, 0.0));
        assert!(map[1].1 > 0.0);
    }

    #[test]
    fn y_map_is_monotonically_increasing() {
        let ctx = test_ctx();
        let md = "# Titel\n\nEin Absatz mit Text der umgebrochen werden sollte weil er sehr lang ist und ueber die breite hinausgeht.\n- Punkt 1\n- Punkt 2\n```\ncode zeile\n```\n";
        let map = build_source_y_map(&ctx, md, 200.0);
        for w in map.windows(2) {
            assert!(w[1].1 >= w[0].1, "Y nicht monoton: {:?} -> {:?}", w[0], w[1]);
        }
        assert!(map.last().unwrap().1 > 0.0);
    }

    #[test]
    fn y_for_byte_start_is_0() {
        let ctx = test_ctx();
        let md = "a\nbb\nccc\n";
        let map = build_source_y_map(&ctx, md, 400.0);
        assert_eq!(y_for_byte(&map, 0), 0.0);
    }

    #[test]
    fn y_for_byte_last_byte_is_total_height() {
        let ctx = test_ctx();
        let md = "a\nbb\nccc\n";
        let map = build_source_y_map(&ctx, md, 400.0);
        let total = map.last().unwrap().1;
        let last_byte = map.last().unwrap().0;
        let y = y_for_byte(&map, last_byte.saturating_sub(1));
        assert!(y <= total);
        assert!(y >= 0.0);
    }

    #[test]
    fn heading_is_higher_than_body() {
        let ctx = test_ctx();
        let h = build_source_y_map(&ctx, "# Grosse Ueberschrift\n", 400.0);
        let p = build_source_y_map(&ctx, "Kleiner Fliesstext\n", 400.0);
        assert!(h[1].1 > p[1].1, "Heading-Y sollte groesser sein als Paragraph-Y");
    }

    #[test]
    fn paragraph_measured_as_single_block() {
        // Ein Absatz aus 5 kurzen Quellzeilen muss als EIN Block (2 map-Eintraege
        // vor dem End-Eintrag) gezaehlt werden, nicht 5.
        let ctx = test_ctx();
        // Kein abschliessendes '\n', damit kein Extra-Leer-Block entsteht.
        let para = "Zeile eins des Absatzes.\nZeile zwei des Absatzes.\nZeile drei.\nZeile vier.\nZeile fuenf.";
        let map = build_source_y_map(&ctx, para, 400.0);
        // 1 Block (Absatz) + 1 End-Eintrag = 2 Eintraege.
        assert_eq!(map.len(), 2, "Absatz sollte ein Block sein");
    }

    #[test]
    fn real_note_is_monotonic_and_blockwise() {
        // Laedt die echte Testnotiz und prueft die Kern-Eigenschaften der
        // Sync-Schaetzung deterministisch (ohne Live-UI).
        let pfad = std::env::var("CARGO_MANIFEST_DIR").unwrap() + "/../rusty-vault/Lange Notiz.md";
        let text = match std::fs::read_to_string(&pfad) {
            Ok(t) => t,
            Err(_) => return, // Vault ggf. woanders -> Test uebersprungen
        };
        let ctx = test_ctx();
        let map = build_source_y_map(&ctx, &text, 420.0);
        // Monoton steigend.
        for w in map.windows(2) {
            assert!(w[1].1 >= w[0].1, "Y nicht monoton");
        }
        // Mindestens so viele Bloecke wie Abschnitts-Ueberschriften.
        let abschnitte = text.matches("## Abschnitt").count();
        // map.len() - 1 = Anzahl Bloecke (ohne End-Eintrag). Sollte >= Abschnitte sein
        // (jede Ueberschrift ist ein Block, dazu Absaetze/Code/Liste).
        assert!(map.len() - 1 >= abschnitte, "zu wenig Bloecke: {} < {}", map.len()-1, abschnitte);
        // Gesamthoehe positiv und groesser als eine Bildschirmhoehe (tiefes Dokument).
        let total = map.last().unwrap().1;
        assert!(total > 1000.0, "Gesamthoehe zu klein: {}", total);
        // y_for_byte(Map-Mitte) liegt zwischen 0 und total.
        let mid_byte = text.len() / 2;
        let ym = y_for_byte(&map, mid_byte);
        assert!(ym > 0.0 && ym < total, "Mitte ausserhalb: {} / {}", ym, total);
    }
}
