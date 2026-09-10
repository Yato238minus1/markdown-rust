//! Content-anchored sync scrolling between editor and preview.
//!
//! Both panes show the same note with different fonts, wrapping and widgets,
//! so proportional scrolling (fraction * height) drifts. Instead we map source
//! byte offsets to Y positions on BOTH sides and align on a shared anchor
//! byte: the editor's top visible byte is placed at the preview's top.
//!
//! The preview map estimates how `egui_commonmark` renders each Markdown
//! block (measured with egui's own layout, including wrapping). Estimates are
//! exact at block starts; interpolation between them is monotonic, so a small
//! local error never scrambles the order. Empty source lines render as ~zero
//! height in the preview (only inter-block gaps count), images get a fixed
//! heuristic height (constant shift — everything below stays consistent).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use eframe::egui;

/// Byte offset -> Y position, ascending, plus total height.
#[derive(Debug, Clone, Default)]
pub struct SyncMap {
    pub entries: Vec<(usize, f32)>,
    pub total: f32,
}

/// Cached map with the inputs it was built from.
#[derive(Debug, Clone, Default)]
pub struct MapCache {
    pub hash: u64,
    pub width: u32,
    pub fonts: u64,
    pub map: SyncMap,
    /// Extra preview height rendered ABOVE the mapped body (front matter).
    pub extra: f32,
    /// Last seen viewport height (for clamping before first measure).
    pub visible: f32,
}

/// Fast hash to detect text/width/font changes.
pub fn hash_text(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Y position for a source byte (linear interpolation between entries).
pub fn y_for_byte(map: &SyncMap, byte: usize) -> f32 {
    if map.entries.is_empty() {
        return 0.0;
    }
    let mut lo = 0usize;
    let mut hi = map.entries.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if map.entries[mid].0 <= byte {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let idx = lo.saturating_sub(1);
    if idx + 1 < map.entries.len() {
        let (b0, y0) = map.entries[idx];
        let (b1, y1) = map.entries[idx + 1];
        if b1 > b0 {
            let t = byte.saturating_sub(b0) as f32 / (b1 - b0) as f32;
            y0 + t * (y1 - y0)
        } else {
            y0
        }
    } else {
        map.entries[idx].1
    }
}

/// Source byte visible at height `y` (inverse of [`y_for_byte`]).
pub fn byte_at_y(map: &SyncMap, y: f32) -> usize {
    if map.entries.is_empty() {
        return 0;
    }
    let y = y.max(0.0);
    let mut lo = 0usize;
    let mut hi = map.entries.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if map.entries[mid].1 <= y {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let idx = lo.saturating_sub(1);
    if idx + 1 < map.entries.len() {
        let (b0, y0) = map.entries[idx];
        let (b1, y1) = map.entries[idx + 1];
        if y1 > y0 {
            let t = ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
            b0 + (t * (b1 - b0) as f32).round() as usize
        } else {
            b0
        }
    } else {
        map.entries[idx].0
    }
}

/// A source block of the Markdown body.
enum Block {
    Paragraph { lines: Vec<usize> },
    Heading { level: usize, line: usize },
    List { lines: Vec<usize> },
    Quote { lines: Vec<usize> },
    Code { lines: Vec<usize> },
    Table { lines: Vec<usize> },
    Image { lines: Vec<usize> },
    Rule { line: usize },
    Blank,
}

fn body_font(ctx: &egui::Context) -> egui::FontId {
    ctx.global_style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .cloned()
        .unwrap_or_else(|| egui::FontId::new(14.0, egui::FontFamily::Proportional))
}

fn heading_font(ctx: &egui::Context) -> egui::FontId {
    ctx.global_style()
        .text_styles
        .get(&egui::TextStyle::Heading)
        .cloned()
        .unwrap_or_else(|| egui::FontId::new(21.0, egui::FontFamily::Proportional))
}

/// Rendered height of `text` using egui's own layout.
fn measure(
    ctx: &egui::Context,
    text: &str,
    font: &egui::FontId,
    width: f32,
    wrap: bool,
) -> f32 {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if wrap { width.max(1.0) } else { f32::INFINITY };
    job.append(
        text,
        0.0,
        egui::text::TextFormat::simple(font.clone(), egui::Color32::WHITE),
    );
    ctx.fonts_mut(|f| f.layout_job(job)).size().y
}

fn single_row(ctx: &egui::Context, font: &egui::FontId) -> f32 {
    measure(ctx, "Ag", font, f32::INFINITY, false).max(1.0)
}

/// Image height guess: capped at the available width, 16:9-ish, clamped.
/// Constant per image, so everything below stays consistently shifted.
fn image_height(content_width: f32) -> f32 {
    (content_width * 9.0 / 16.0).clamp(120.0, 360.0)
}

/// Split `![alt](url)` segments out of a line; returns (rest, image_count).
fn split_images(s: &str) -> (String, usize) {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut count = 0;
    while let Some(i) = rest.find("![") {
        let after = &rest[i + 2..];
        let end = after
            .find("](")
            .and_then(|e| after[e + 2..].find(')').map(|c| e + 2 + c));
        match end {
            Some(e) => {
                out.push_str(&rest[..i]);
                out.push(' ');
                count += 1;
                rest = &after[e + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    (out, count)
}

fn is_table_delimiter(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| "-:| ".contains(c))
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_'))
}

/// Editor-side map: every raw source line (monospace, wrapped). Empty lines
/// keep a full row so line numbers stay aligned with the TextEdit.
pub fn build_editor_map(
    ctx: &egui::Context,
    text: &str,
    font_size: f32,
    content_width: f32,
) -> SyncMap {
    let font = egui::FontId::monospace(font_size);
    let row = single_row(ctx, &font);
    let mut entries = Vec::new();
    let mut y = 0.0_f32;
    let mut byte = 0usize;
    for line in text.split('\n') {
        entries.push((byte, y));
        y += if line.is_empty() {
            row
        } else {
            measure(ctx, line, &font, content_width, true).max(row)
        };
        byte += line.len() + 1;
    }
    let total = y;
    entries.push((text.len(), total));
    SyncMap { entries, total }
}

/// Preview-side map over the Markdown BODY (without front matter).
pub fn build_preview_map(ctx: &egui::Context, body: &str, content_width: f32) -> SyncMap {
    let font = body_font(ctx);
    let heading = heading_font(ctx);
    let body_size = font.size;
    let heading_size = heading.size;
    let row = single_row(ctx, &font);
    // Inter-block gap: exactly one body row (calibrated against renders).
    let gap = row;
    let spacing = ctx.global_style().spacing.item_spacing.y.max(2.0);
    let code_font = ctx
        .global_style()
        .text_styles
        .get(&egui::TextStyle::Monospace)
        .cloned()
        .unwrap_or_else(|| egui::FontId::new((body_size * 0.85).max(11.0), egui::FontFamily::Monospace));
    // egui_commonmark heading scale (H1 = Heading style .. H6 near body).
    let factors = [1.0, 0.835, 0.668, 0.501, 0.334, 0.167];
    let diff = (heading_size - body_size).max(0.0);

    let lines: Vec<&str> = body.split('\n').collect();
    let mut byte_offsets = Vec::with_capacity(lines.len());
    let mut acc = 0usize;
    for l in &lines {
        byte_offsets.push(acc);
        acc += l.len() + 1;
    }

    // --- split into blocks ---
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0;
    let mut in_fence = false;
    let mut prev_blank_or_code = true;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if in_fence {
            let mut group = vec![i];
            i += 1;
            while i < lines.len() {
                group.push(i);
                let t = lines[i].trim_start();
                i += 1;
                if t.starts_with("```") || t.starts_with("~~~") {
                    break;
                }
            }
            blocks.push(Block::Code { lines: group });
            in_fence = false;
            prev_blank_or_code = true;
            continue;
        }
        if line.trim().is_empty() {
            blocks.push(Block::Blank);
            i += 1;
            prev_blank_or_code = true;
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = true;
            prev_blank_or_code = false;
            continue; // opening fence joins the Code group next iteration
        }
        if let Some(level) = heading_level(trimmed) {
            blocks.push(Block::Heading { level, line: i });
            i += 1;
            prev_blank_or_code = false;
            continue;
        }
        if is_rule(line) {
            blocks.push(Block::Rule { line: i });
            i += 1;
            prev_blank_or_code = false;
            continue;
        }
        if trimmed.starts_with('|') {
            // Table only with a delimiter row, else plain paragraph text.
            let mut group = vec![i];
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim_start().starts_with('|') {
                group.push(j);
                j += 1;
            }
            if group.iter().any(|&k| is_table_delimiter(lines[k])) {
                blocks.push(Block::Table { lines: group });
                i = j;
                prev_blank_or_code = false;
                continue;
            }
        }
        if is_list_item(trimmed) {
            let mut group = vec![i];
            i += 1;
            while i < lines.len() && is_list_item(lines[i].trim_start()) {
                group.push(i);
                i += 1;
            }
            blocks.push(Block::List { lines: group });
            prev_blank_or_code = false;
            continue;
        }
        if trimmed.starts_with('>') {
            let mut group = vec![i];
            i += 1;
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                group.push(i);
                i += 1;
            }
            blocks.push(Block::Quote { lines: group });
            prev_blank_or_code = false;
            continue;
        }
        let (stripped, images) = split_images(line);
        if images > 0 && stripped.trim().is_empty() {
            blocks.push(Block::Image { lines: vec![i] });
            i += 1;
            prev_blank_or_code = false;
            continue;
        }
        if (line.starts_with("    ") || line.starts_with('\t')) && prev_blank_or_code {
            let mut group = vec![i];
            i += 1;
            while i < lines.len()
                && !lines[i].trim().is_empty()
                && (lines[i].starts_with("    ") || lines[i].starts_with('\t'))
            {
                group.push(i);
                i += 1;
            }
            blocks.push(Block::Code { lines: group });
            prev_blank_or_code = true;
            continue;
        }
        // Paragraph: following plain lines belong together.
        let mut group = vec![i];
        i += 1;
        while i < lines.len() {
            let l = lines[i];
            let tls = l.trim_start();
            if l.trim().is_empty()
                || tls.starts_with("```")
                || tls.starts_with("~~~")
                || heading_level(tls).is_some()
                || is_rule(l)
                || is_list_item(tls)
                || tls.starts_with('>')
                || tls.starts_with('|')
                || ((l.starts_with("    ") || l.starts_with('\t')) && prev_blank_or_code)
            {
                break;
            }
            let (s, imgs) = split_images(l);
            if imgs > 0 && s.trim().is_empty() {
                break; // image-only line starts its own block
            }
            group.push(i);
            i += 1;
        }
        blocks.push(Block::Paragraph { lines: group });
        prev_blank_or_code = false;
    }

    // --- measure blocks; one entry per line start for resolution ---
    let push_line = |entries: &mut Vec<(usize, f32)>, line_idx: usize, y: f32| {
        entries.push((byte_offsets[line_idx], y));
    };
    // --- measure blocks ---
    // Calibrated against headless renders of the real renderer:
    // - one body row between blocks (no extra spacing, no trailing gap),
    // - a heading keeps its leading gap even as the first block,
    // - fence markers render as nothing, list items cost row + 0.2*row,
    //   code blocks add one item-spacing padding, tables one double,
    //   quotes pad two rows (frame), rules cost one row mid-document.
    let mut entries: Vec<(usize, f32)> = Vec::new();
    let mut y = 0.0_f32;
    let mut first = true;
    for block in &blocks {
        if matches!(block, Block::Blank) {
            continue;
        }
        let is_heading = matches!(block, Block::Heading { .. });
        if !first || is_heading {
            y += gap;
        }
        first = false;
        match block {
            Block::Blank => {}
            Block::Rule { line } => {
                push_line(&mut entries, *line, y);
                y += row;
            }
            Block::Heading { level, line } => {
                push_line(&mut entries, *line, y);
                let lvl = (level - 1).min(5);
                let size = if lvl == 0 {
                    heading_size
                } else {
                    body_size + diff * factors[lvl]
                };
                let text = lines[*line][(*level).min(lines[*line].len())..].trim();
                y += measure(ctx, text, &egui::FontId::new(size, egui::FontFamily::Proportional), content_width, true).max(row);
            }
            Block::Paragraph { lines: group } => {
                // Soft breaks render as spaces: measure the paragraph JOINED
                // (line-by-line measuring explodes on many short lines),
                // then spread line entries by character count.
                let mut parts: Vec<String> = Vec::with_capacity(group.len());
                let mut images = 0usize;
                for &li in group.iter() {
                    let (stripped, imgs) = split_images(lines[li]);
                    images += imgs;
                    parts.push(
                        stripped.split_whitespace().collect::<Vec<_>>().join(" "),
                    );
                }
                let full = parts.join(" ");
                let text_h =
                    measure(ctx, &full, &font, content_width, true).max(row);
                let img_h = images as f32 * image_height(content_width);
                let total_chars = full.chars().count().max(1) as f32;
                let mut done = 0usize;
                for (k, &li) in group.iter().enumerate() {
                    let frac = done.min(total_chars as usize) as f32 / total_chars;
                    push_line(&mut entries, li, y + (text_h + img_h) * frac);
                    done += parts[k].chars().count() + 1;
                }
                y += text_h + img_h;
            }
            Block::List { lines: group } => {
                for &li in group.iter() {
                    push_line(&mut entries, li, y);
                    let (stripped, images) = split_images(lines[li]);
                    y += measure(ctx, stripped.trim(), &font, content_width, true).max(row)
                        + row * 0.2
                        + images as f32 * image_height(content_width);
                }
            }
            Block::Quote { lines: group } => {
                // Quote lines join with spaces (one paragraph); the frame
                // pads about two rows. Entry per line for resolution.
                let mut joined = String::new();
                for (k, &li) in group.iter().enumerate() {
                    push_line(&mut entries, li, y);
                    if k > 0 {
                        joined.push(' ');
                    }
                    let clean = lines[li]
                        .trim_start()
                        .trim_start_matches(|c| c == '>' || c == ' ');
                    let (stripped, images) = split_images(clean);
                    joined.push_str(stripped.trim());
                    let _ = images;
                }
                let (rest, images) = split_images(&joined);
                y += measure(ctx, rest.trim(), &font, content_width, true).max(row)
                    + gap * 2.0
                    + images as f32 * image_height(content_width);
                // Distribute line entries across the content rows.
                let n = group.len().max(1) as f32;
                let y0 = entries[entries.len() - group.len()].1;
                for (k, &li) in group.iter().enumerate() {
                    let yy = y0 + (y - gap * 2.0 - y0) * k as f32 / n;
                    if let Some(e) = entries.iter_mut().find(|e| e.0 == byte_offsets[li]) {
                        e.1 = yy;
                    }
                }
            }
            Block::Code { lines: group } => {
                // Rendered as a TextEdit: monospace, wrapped, but rows keep
                // the forced body height; the frame adds one item spacing.
                for &li in group {
                    // Fence markers render as nothing; only content rows count.
                    let t = lines[li].trim_start();
                    if group.len() > 1
                        && (t.starts_with("```") || t.starts_with("~~~"))
                    {
                        push_line(&mut entries, li, y);
                        continue;
                    }
                    push_line(&mut entries, li, y);
                    y += measure(ctx, lines[li], &code_font, content_width, true).max(row);
                }
                y += spacing;
            }
            Block::Table { lines: group } => {
                for &li in group {
                    push_line(&mut entries, li, y);
                    y += row;
                }
                y += spacing * 2.0;
            }
            Block::Image { lines: group } => {
                for &li in group {
                    push_line(&mut entries, li, y);
                    let (stripped, images) = split_images(lines[li]);
                    if !stripped.trim().is_empty() {
                        y += measure(ctx, stripped.trim(), &font, content_width, true).max(row);
                    }
                    y += images as f32 * image_height(content_width);
                }
            }
        }
    }
    let total = y.max(0.0);
    entries.push((body.len(), total));
    SyncMap { entries, total }
}

/// Height rendered ABOVE the markdown body (front-matter header + separator).
/// Calibrated: label rows plus one full row for the separator.
pub fn header_extra_height(ctx: &egui::Context, parts: &[&str], content_width: f32) -> f32 {
    if parts.is_empty() {
        return 0.0;
    }
    let font = body_font(ctx);
    let joined = parts.join(" ");
    measure(ctx, &joined, &font, content_width, true) + single_row(ctx, &font)
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

    fn test_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        // Fonts only exist after the first pass; drop its texture deltas.
        let mut out = ctx.run_ui(egui::RawInput::default(), |_ui| {});
        out.textures_delta.clear();
        ctx
    }

    fn assert_monotonic(map: &SyncMap) {
        for w in map.entries.windows(2) {
            assert!(w[1].1 >= w[0].1, "Y not monotonic: {:?} -> {:?}", w[0], w[1]);
        }
        for w in map.entries.windows(2) {
            assert!(w[1].0 >= w[0].0, "bytes not monotonic");
        }
    }

    #[test]
    fn calibrated_against_real_renderer() {
        use std::cell::Cell;
        let ctx = test_ctx();

        // Headless render at fixed outer width; returns (content, inner width).
        fn render_md(ctx: &egui::Context, width: f32, salt: &str, md: &str) -> (f32, f32) {
            let real = Cell::new(0.0f32);
            let inner = Cell::new(0.0f32);
            let mut cache = egui_commonmark::CommonMarkCache::default();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        let o = egui::ScrollArea::vertical()
                            .id_salt(salt)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui_commonmark::CommonMarkViewer::new()
                                    .show(ui, &mut cache, md);
                            });
                        real.set(o.content_size.y);
                        inner.set(o.inner_rect.width());
                    },
                );
            });
            out.textures_delta.clear();
            (real.get(), inner.get())
        }

        // Synthetic doc covering every block type (blank-heavy on purpose).
        let md = "# Title\n\nIntro paragraph with enough words to wrap onto two rows at this width, yes.\n\n## Two\n\n- item one\n- item two\n\n```\ncode a\ncode b\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> quoted words here\n\n---\n\nTail paragraph closes the note.\n";
        // Narrow widths wrap much more: the wrap width must be exact.
        for width in [220.0f32, 400.0] {
            let salt = format!("calib-{width}");
            let (real_total, inner_w) = render_md(&ctx, width, &salt, md);
            // Production builds the map with the measured inner width.
            let map = build_preview_map(&ctx, md, inner_w);
            let ratio = map.total / real_total.max(1.0);
            assert!(inner_w <= width, "inner {inner_w} > outer {width}");
            assert!((ratio - 1.0).abs() < 0.05, "w={width}: total off: est {} real {}", map.total, real_total);

            // Anchor accuracy: real Y of a block start == rendered prefix height.
            // Gaps are leading (emitted with the next block), so a cut that is
            // followed by more content sits one row below its prefix height.
            let mut byte = 0usize;
            let mut prev_blank = true;
            for line in md.split_inclusive('\n') {
                if prev_blank && !line.trim().is_empty() && byte > 0 {
                    let (real_y, _) = render_md(&ctx, width, "calib-at", &md[..byte]);
                    let rest_follows = md[byte..].trim_start_matches('\n').lines().next().is_some_and(|l| !l.trim().is_empty());
                    let row = single_row(&ctx, &body_font(&ctx));
                    let real_anchor = real_y + if rest_follows { row } else { 0.0 };
                    let est_y = y_for_byte(&map, byte);
                    assert!((real_anchor - est_y).abs() <= 5.0, "w={width} anchor @{}: est {} real {}", byte, est_y, real_anchor);
                }
                prev_blank = line.trim().is_empty();
                byte += line.len();
            }
        }

        // Front-matter header: constant shift, must match too.
        let parts = ["Front matter:", "Title: X", "Tags: a, b"];
        let est_extra = header_extra_height(&ctx, &parts, 400.0);
        let real_extra = Cell::new(0.0f32);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(400.0, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let o = egui::ScrollArea::vertical()
                        .id_salt("calib-fm")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.weak(parts[0]);
                                ui.label(format!("{} {}", "Title:", "X"));
                                ui.label(format!("{} {}", "Tags:", "a, b"));
                            });
                            ui.separator();
                        });
                    real_extra.set(o.content_size.y);
                },
            );
        });
        out.textures_delta.clear();
        assert!((real_extra.get() - est_extra).abs() <= 4.0, "fm header: est {} real {}", est_extra, real_extra.get());

        // Best effort on the real long note (skipped when absent).
        let home = std::env::var("HOME").unwrap_or_default();
        let candidates = [
            std::env::var("CARGO_MANIFEST_DIR").unwrap() + "/../rusty-vault/Lange Notiz.md",
            format!("{home}/Documents/rusty-vault/Lange Notiz.md"),
        ];
        let text = candidates.iter().find_map(|p| std::fs::read_to_string(p).ok());
        if let Some(text) = text {
            let (_, body) = crate::markdown::split_front_matter(&text);
            for width in [250.0f32, 400.0] {
                let salt = format!("calib-long-{width}");
                let (real_total, inner_w) = render_md(&ctx, width, &salt, body);
                let map = build_preview_map(&ctx, body, inner_w);
                let ratio = map.total / real_total.max(1.0);
                assert!((ratio - 1.0).abs() < 0.03, "w={width} long note off: est {} real {}", map.total, real_total);
            }
        }
    }

    #[test]
    fn sync_maps_survive_hidpi() {
        // 4K/HiDPI: layout math is in points, so pixels_per_point must not matter.
        let ctx = test_ctx();
        ctx.set_pixels_per_point(2.0);
        let md = "## Head\n\nA paragraph with enough words to wrap onto several rows here.\n\n- one\n- two\n\n```\ncode\n```\n\nTail.\n";
        let width = 300.0;
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let real = std::cell::Cell::new(0.0f32);
        let inner = std::cell::Cell::new(0.0f32);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let o = egui::ScrollArea::vertical()
                        .id_salt("hidpi")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, md);
                        });
                    real.set(o.content_size.y);
                    inner.set(o.inner_rect.width());
                },
            );
        });
        out.textures_delta.clear();
        let map = build_preview_map(&ctx, md, inner.get());
        let ratio = map.total / real.get().max(1.0);
        assert!((ratio - 1.0).abs() < 0.05, "hidpi off: est {} real {}", map.total, real.get());
    }



    #[test]
    fn empty_maps_start_at_zero() {
        let ctx = test_ctx();
        let pm = build_preview_map(&ctx, "", 400.0);
        assert_eq!(pm.entries[0], (0, 0.0));
        let em = build_editor_map(&ctx, "", 14.0, 400.0);
        assert_eq!(em.entries[0], (0, 0.0));
        assert_monotonic(&pm);
        assert_monotonic(&em);
    }

    #[test]
    fn mixed_note_is_monotonic_with_positive_total() {
        let ctx = test_ctx();
        let md = "# Title\n\nA paragraph with text that should wrap because it is very long and exceeds the width.\n\n- item 1\n- item 2\n\n```\ncode line\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n![alt](img.png)\n\n---\n\n> a quote\n\ntail.\n";
        let pm = build_preview_map(&ctx, md, 200.0);
        assert_monotonic(&pm);
        assert!(pm.total > 0.0);
        let em = build_editor_map(&ctx, md, 14.0, 200.0);
        assert_monotonic(&em);
        assert!(em.total > 0.0);
    }

    #[test]
    fn anchors_roundtrip() {
        let ctx = test_ctx();
        let md = "# Title\n\nFirst paragraph here.\n\nSecond paragraph here.\n";
        let pm = build_preview_map(&ctx, md, 400.0);
        for &probe in &[0, md.len() / 2, md.len()] {
            let y = y_for_byte(&pm, probe);
            assert!(y >= 0.0 && y <= pm.total);
            let back = byte_at_y(&pm, y);
            assert!(back <= md.len());
        }
        assert_eq!(y_for_byte(&pm, 0), single_row(&ctx, &body_font(&ctx)));
    }

    #[test]
    fn heading_block_is_taller_than_body_row() {
        let ctx = test_ctx();
        let h = build_preview_map(&ctx, "# Big Heading\n", 400.0);
        let p = build_preview_map(&ctx, "Small body text\n", 400.0);
        assert!(h.total > p.total);
    }

    #[test]
    fn blank_lines_add_no_preview_height() {
        let ctx = test_ctx();
        let one = build_preview_map(&ctx, "text\n", 400.0);
        let many = build_preview_map(&ctx, "text\n\n\n\n", 400.0);
        assert!((many.total - one.total).abs() < 1.0, "{} vs {}", many.total, one.total);
    }

    #[test]
    fn editor_keeps_full_row_for_blank_lines() {
        let ctx = test_ctx();
        let em = build_editor_map(&ctx, "a\n\nb\n", 14.0, 400.0);
        let row = single_row(&ctx, &egui::FontId::monospace(14.0));
        // 4 source lines (trailing '\n' leaves an empty 4th line, like TextEdit).
        assert!((em.total - 4.0 * row).abs() < 2.0, "total {}, row {}", em.total, row);
    }
}
