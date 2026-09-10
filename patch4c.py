#!/usr/bin/env python3
"""Sync-Scroll: Editor und Vorschau teilen einen Scroll-Anteil (proportional)."""
src = open('src/main.rs').read()
def rep(old, new):
    global src
    assert old in src, "FEHLT: " + old[:80].replace("\n", "\\n")
    src = src.replace(old, new, 1)

# ---------- ui_editor: ScrollArea um Editor + Sync-Logik ----------
rep("""        let editor = egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .layouter(&mut layouter)
            .desired_width(f32::INFINITY)
            .id(editor_id);
        let te_out = editor.show(ui);
        let editor_resp = te_out.response.clone();
        if self.focus_editor_once {
            editor_resp.request_focus();
            self.focus_editor_once = false;
        }

        // ---- Link-Interaktion: Klick + Strg+Hover ----
        self.interagiere_editor_links(&te_out, text.len());

        if te_out.response.changed() {
            if let Some(v) = self.vault.as_mut() {
                let _ = v.set_text(&path, text);
            }
            self.mark_dirty_timer();
        }
    }""",
"""        let editor = egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .layouter(&mut layouter)
            .desired_width(f32::INFINITY)
            .id(editor_id);

        // Sync-Scroll: Editor in ScrollArea mit geteiltem Zustand.
        let scroll_id = egui::Id::new(("editor_scroll", path.clone()));
        let sync_anteil_alt = self.sync_scroll;
        egui::ScrollArea::vertical()
            .id_salt(scroll_id)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let te_out = editor.show(ui);
                let editor_resp = te_out.response.clone();
                if self.focus_editor_once {
                    editor_resp.request_focus();
                    self.focus_editor_once = false;
                }

                // ---- Link-Interaktion: Klick + Strg+Hover ----
                self.interagiere_editor_links(&te_out);

                if editor_resp.changed() {
                    if let Some(v) = self.vault.as_mut() {
                        let _ = v.set_text(&path, text.clone());
                    }
                    self.mark_dirty_timer();
                }
            });

        // Scroll-Stand des Editors lesen (0..1) für die Vorschau.
        let state = egui::scroll_area::State::load(ui.ctx(), scroll_id);
        if let Some(st) = state {
            if st.content_size.y > st.inner_rect.height() {
                self.sync_scroll =
                    (st.offset.y / (st.content_size.y - st.inner_rect.height())).clamp(0.0, 1.0);
            }
        }
    }""")

open('src/main.rs', 'w').write(src)
print("OK")
