#!/usr/bin/env python3
"""ui_editor-Kern: syntect im Layouter, TextEditOutput, Link-Klick, Strg+Hover."""
src = open('src/main.rs').read()
start = src.index("    fn ui_editor(&mut self, ui: &mut egui::Ui) {")
ende_marker = "    fn ui_preview(&mut self, ui: &mut egui::Ui) {"
end = src.index(ende_marker)

neu = '''    fn ui_editor(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.active.clone() else {
            return;
        };
        let text_now = self
            .vault
            .as_ref()
            .and_then(|v| v.buffer(&path))
            .map(|b| b.text.clone());
        let Some(mut text) = text_now else {
            ui.weak(t::FILE_GONE);
            return;
        };

        // Glossar-Treffer einmal pro Frame berechnen (Aho-Corasick, schnell).
        let glossar_aktiv = self.einst.werte.glossar_aktiv;
        let glossar_max = self.einst.werte.glossar_max_treffer;
        let min_laenge = self.einst.werte.glossar_min_laenge;
        let schrift_groesse = self.einst.werte.editor_schriftgroesse;
        let glossar_treffer: Vec<glossary::GlossarTreffer> = match (&self.glossar, glossar_aktiv) {
            (Some(g), true) if !g.ist_leer() => {
                let mut treffer: Vec<glossary::GlossarTreffer> = g
                    .finde(&text)
                    .into_iter()
                    .filter(|tr| {
                        let s = &text[tr.start..tr.end];
                        s.chars().count() >= min_laenge
                    })
                    .collect();
                treffer.truncate(glossar_max);
                treffer
            }
            _ => Vec::new(),
        };

        let editor_id = egui::Id::new(("editor", path.clone()));

        let mut layouter =
            move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let txt = buf.as_str();
                let spans = highlight(txt);
                let stuecke: Vec<(usize, usize, glossary::Stil)> =
                    glossary::verschneide(&spans, &glossar_treffer, txt.len());
                let mut job = egui::text::LayoutJob::default();

                // Für CodeBlock-Bereiche: syntect-Färbung pro Block vorbereiten.
                // Cache: (Block-Start, Infostring, Code) -> FarbSpannen.
                let mut code_farben: Vec<(usize, usize, Vec<(usize, usize, [u8; 4])>)> = Vec::new();
                for s in spans.iter() {
                    if s.tok == Tok::CodeBlock {
                        // Block-Inhalt extrahieren: ```info\\n...```
                        let blocktxt = &txt[s.start..s.end];
                        if let Some(zeile_ende) = blocktxt.find('\\n') {
                            let info = blocktxt[3..zeile_ende].trim();
                            let inhalt_start = s.start + zeile_ende + 1;
                            let inhalt = &txt[inhalt_start..s.end];
                            let farben: Vec<(usize, usize, [u8; 4])> = CODE_HIGHLIGHTER
                                .highlight(inhalt, info)
                                .into_iter()
                                .map(|f| (inhalt_start + f.start, inhalt_start + f.end, f.farbe))
                                .collect();
                            code_farben.push((s.start, s.end, farben));
                        }
                    }
                }
                let hat_code = !code_farben.is_empty();

                for (s, e, stil) in stuecke {
                    let farbe;
                    if stil == glossary::Stil::Glossar {
                        farbe = GLOSSAR_FARBE;
                    } else if let glossary::Stil::Token(tok) = stil {
                        if tok != Tok::CodeBlock || !hat_code {
                            farbe = tok_color(tok);
                        } else {
                            // Innerhalb eines Codeblocks: syntect-Farbe suchen.
                            // (Der Abschnitt liegt ganz in einem Block oder teilt ihn.)
                            let mut gefunden = None;
                            for (_bs, _be, farben) in &code_farben {
                                for (fs, fe, f) in farben {
                                    // Überschneidung mit [s, e):
                                    if *fe > s && *fs < e {
                                        // Nimm die Farbe am Abschnitts-Anfang.
                                        if *fs <= s && s < *fe {
                                            gefunden = Some(*f);
                                            break;
                                        }
                                    }
                                }
                                if gefunden.is_some() {
                                    break;
                                }
                            }
                            farbe = gefunden
                                .map(|f| Color32::from_rgba_unmultiplied(f[0], f[1], f[2], f[3]))
                                .unwrap_or_else(|| tok_color(Tok::CodeBlock));
                        }
                    } else {
                        farbe = tok_color(Tok::Plain);
                    }
                    let fmt = egui::TextFormat::simple(
                        egui::FontId::monospace(schrift_groesse),
                        farbe,
                    );
                    job.append(&txt[s..e], 0.0, fmt);
                }
                job.wrap.max_width = wrap_width;
                ui.fonts_mut(|f| f.layout_job(job))
            };

        let editor = egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .layouter(&mut layouter)
            .desired_width(f32::INFINITY)
            .id(editor_id);
        let editor_resp = ui.add(editor);
        if self.focus_editor_once {
            editor_resp.request_focus();
            self.focus_editor_once = false;
        }

        // ---- Link-Interaktion: Klick + Strg+Hover ----
        self.interagiere_editor_links(
            ui,
            editor_resp.clone(),
            &text,
        );

        if editor_resp.changed() {
            if let Some(v) = self.vault.as_mut() {
                let _ = v.set_text(&path, text);
            }
            self.mark_dirty_timer();
        }
    }

    /// Ermittelt den Wikilink unter der Maus: Strg+Hover zeigt ein Popup,
    /// Klick öffnet die Zielnotiz.
    fn interagiere_editor_links(
        &mut self,
        ui: &mut egui::Ui,
        response: egui::Response,
        text: &str,
    ) {
        let Some(hover_pos) = response.hover_pos() else {
            self.hover_link = None;
            return;
        };
        let out = response
            .clone()
            .output_mut(|o| std::mem::take(&mut o.text_edit_output));
        let Some(te_out) = out else { return };

        // Byte-Offset unter der Maus:
        let galley_pos = te_out.galley_pos;
        let rel = egui::vec2(hover_pos.x - galley_pos.x, hover_pos.y - galley_pos.y);
        let ccursor = te_out.galley.cursor_from_pos(rel);
        // CCursor.index zählt Zeichen:
        let byte_pos = char_zu_byte(text, ccursor.index);

        let link = rusty_notes::editor_links::wikilink_an(text, byte_pos);

        // Strg+Hover → Popup merken
        let ctrl = ui.input(|i| i.modifiers.ctrl);
        if ctrl {
            if let Some(l) = &link {
                self.hover_link = Some((l.ziel.clone(), hover_pos));
            } else {
                self.hover_link = None;
            }
        } else {
            self.hover_link = None;
        }

        // Einfacher Klick (ohne Auswahlverschiebung): Link öffnen.
        // Wir nutzen sekundärer Klick (mittlere Maustaste) UND Strg+Klick,
        // damit normale Klicks zum Cursor-Setzen bleiben.
        if response.secondary_clicked() {
            if let Some(l) = link {
                if let Some(pfad) = self.notiz_fuer_wikilink(&l.ziel) {
                    self.pending_link = Some(pfad);
                } else if ui.input(|i| i.modifiers.ctrl) {
                    // Strg+Rechtsklick auf unbekanntes Ziel: neu anlegen?
                    self.status = format!("Ziel '{}' nicht gefunden", l.ziel);
                }
            }
        }
    }

'''
src = src[:start] + neu + src[end:]
open('src/main.rs', 'w').write(src)
print("TEIL B OK")
