//! The smaller blocks: code, bookmark, equation, table of contents, embedded database.

use super::*;

impl EditorState {
    pub(super) fn code_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        cx: &BlockCx,
        sink: &mut Sink,
        lang: &str,
    ) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let out = &mut *sink.out;
        let pal = env.pal;
        let bid = blocks[i].id;
        let id = text_id(bid);
        self.apply_focus_request(ui.ctx(), &blocks[i], id);
        let mut lang_text = lang.to_string();
        let before = blocks[i].text.clone();
        let r = egui::Frame::NONE
            .fill(pal.code_bg)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 12 })
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(width - 32.0);
                    if lang.trim().eq_ignore_ascii_case("mermaid") {
                        mermaid_view(ui, bid, &blocks[i].text, pal, env.lang);
                    }
                    egui::TextEdit::singleline(&mut lang_text)
                        .id(Id::unique(("nc-lang", bid)))
                        .frame(egui::Frame::NONE)
                        .desired_width(140.0)
                        .font(FontId::proportional(12.0))
                        .text_color(pal.text_secondary)
                        .hint_text(RichText::new("plain text").size(12.0).color(pal.text_faint))
                        .show(ui);
                    let o = egui::TextEdit::multiline(&mut blocks[i].text)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .code_editor()
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .lock_focus(true)
                        .show(ui);
                    if o.response.response.has_focus() {
                        self.focused_block = Some(bid);
                        self.track_rows(&o);
                    }
                });
            });
        if lang_text != *lang {
            let snap = blocks.to_vec();
            blocks[i].kind = BlockKind::Code { lang: lang_text };
            self.record(env.key, &snap, Some(&format!("l{bid}")));
            out.changed = true;
        }
        if blocks[i].text != before {
            let mut snap = blocks.to_vec();
            snap[i].text = before;
            self.record(env.key, &snap, Some(&format!("t{bid}")));
            out.changed = true;
        }
        r.response
    }

    pub(super) fn bookmark_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        cx: &BlockCx,
        sink: &mut Sink,
        url: &str,
    ) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let out = &mut *sink.out;
        let pal = env.pal;
        let bid = blocks[i].id;
        let lang = env.lang;
        let mut committed: Option<String> = None;
        let r = egui::Frame::NONE
            .stroke(Stroke::new(1.0, pal.border))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(width - 26.0);
                if url.is_empty() {
                    let buf = self.bookmark_input.entry(bid).or_default();
                    let te = egui::TextEdit::singleline(buf)
                        .id(Id::unique(("bookmark-url", bid)))
                        .frame(egui::Frame::NONE)
                        .hint_text(RichText::new(tr(lang, "bookmark_hint")).color(pal.text_faint))
                        .desired_width(width - 30.0);
                    let o = ui.add(te);
                    // a new bookmark is ready to receive the link
                    if self.focus.is_some_and(|(fb, _)| fb == bid) {
                        o.request_focus();
                        self.focus = None;
                    }
                    if o.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !buf.trim().is_empty() {
                        let mut u = buf.trim().to_string();
                        if !u.contains("://") {
                            u = format!("https://{u}");
                        }
                        committed = Some(u);
                    }
                } else {
                    let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("").to_string();
                    let title = if blocks[i].text.is_empty() { host } else { blocks[i].text.clone() };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔖").size(16.0));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).size(14.5).strong().color(pal.text));
                            ui.label(RichText::new(url).size(12.0).color(pal.text_faint));
                        });
                    });
                }
            });
        if !url.is_empty() {
            let resp = r.response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                out.actions.push(Action::OpenUrl(url.to_string()));
            }
        }
        if let Some(u) = committed {
            self.bookmark_input.remove(&bid);
            blocks[i].kind = BlockKind::Bookmark { url: u.clone() };
            out.changed = true;
            let (tx, ctx2) = (self.title_tx.clone(), ui.ctx().clone());
            std::thread::spawn(move || {
                if let Some(t) = notecraft_ai::webpage::page_title(&u) {
                    let _ = tx.send((bid, u, t));
                    ctx2.request_repaint();
                }
            });
        }
        r.response
    }

    pub(super) fn equation_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        cx: &BlockCx,
        sink: &mut Sink,
    ) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let out = &mut *sink.out;
        let pal = env.pal;
        let bid = blocks[i].id;
        let lay = cached_layout(ui.ctx(), Id::unique(("nc-eq-layout", bid)), &blocks[i].text, || {
            crate::math::layout(ui.painter(), &crate::math::parse(&blocks[i].text), 24.0)
        });
        ui.vertical(|ui| {
            ui.set_width(width);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, lay.height().max(24.0) + 16.0), Sense::click());
            if blocks[i].text.trim().is_empty() {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    tr(env.lang, "equation_empty"),
                    FontId::proportional(13.0),
                    pal.text_faint,
                );
            } else {
                let origin = egui::pos2(rect.center().x - lay.w / 2.0, rect.min.y + 8.0 + crate::math::ascent(&lay));
                crate::math::draw(ui.painter(), &lay, origin, pal.text);
            }
            if resp.clicked() {
                self.sel_anchor = None;
                self.selected = Some(bid);
            }
            let id = text_id(bid);
            self.apply_focus_request(ui.ctx(), &blocks[i], id);
            let before = blocks[i].text.clone();
            egui::TextEdit::multiline(&mut blocks[i].text)
                .id(id)
                .frame(egui::Frame::NONE)
                .desired_rows(1)
                .desired_width(width)
                .font(FontId::monospace(12.5))
                .text_color(pal.text_secondary)
                .hint_text(RichText::new(tr(env.lang, "equation_hint")).color(pal.text_faint))
                .show(ui);
            if blocks[i].text != before {
                out.changed = true;
            }
            if !blocks[i].text.trim().is_empty() {
                let b = egui::Button::new(
                    RichText::new(format!("∑ {}", tr(env.lang, "equation_solve"))).size(12.0).color(pal.text_secondary),
                )
                .frame(false);
                if ui.add(b).on_hover_text(tr(env.lang, "equation_solve_tip")).clicked() {
                    out.actions.push(Action::SolveEquation(bid));
                }
            }
            resp
        })
        .inner
    }

    pub(super) fn toc_ui(&mut self, ui: &mut Ui, blocks: &mut [Block], cx: &BlockCx) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let pal = env.pal;
        let bid = blocks[i].id;
        let heads: Vec<(u64, u8, String)> = blocks
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::Heading(l) => Some((b.id, l, inline::strip(&b.text))),
                _ => None,
            })
            .collect();
        let r = ui.vertical(|ui| {
            ui.set_width(width);
            ui.style_mut().interaction.selectable_labels = false;
            if heads.is_empty() {
                ui.label(RichText::new(tr(env.lang, "toc_empty")).color(pal.text_faint));
            }
            for (id, level, text) in heads {
                ui.horizontal(|ui| {
                    ui.add_space(f32::from(level - 1) * 18.0);
                    let label = if text.trim().is_empty() { tr(env.lang, "untitled").to_string() } else { text };
                    let resp = ui.add(
                        egui::Label::new(RichText::new(label).color(pal.text_secondary).underline())
                            .sense(Sense::click()),
                    );
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.toc_target = Some(id);
                    }
                });
            }
        });
        let resp = ui.interact(r.response.rect, ui.scope_id().with(("toc", bid)), Sense::click());
        if resp.clicked() && self.toc_target.is_none() {
            self.sel_anchor = None;
            self.selected = Some(bid);
        }
        resp
    }

    pub(super) fn database_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        cx: &BlockCx,
        sink: &mut Sink,
    ) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let out = &mut *sink.out;
        let pal = env.pal;
        let bid = blocks[i].id;
        let lang = env.lang;
        let found = env.titles.get(&blocks[i].text.to_lowercase()).and_then(|(d, _)| match d {
            DocId::Database(id) => Some((id.clone(), d.clone())),
            _ => None,
        });
        egui::Frame::NONE
            .stroke(Stroke::new(1.0, pal.border))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(width - 26.0);
                ui.vertical(|ui| match found {
                    Some((id, doc)) => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("⊞").size(15.0).color(pal.text_faint));
                            ui.label(RichText::new(&blocks[i].text).size(15.0).strong().color(pal.text));
                            if ui.add(egui::Button::new(RichText::new("↗").color(pal.text_faint))).clicked() {
                                out.actions.push(Action::Navigate(doc));
                            }
                        });
                        ui.add_space(4.0);
                        (env.embed)(ui, bid, &id);
                    }
                    None => {
                        ui.label(RichText::new(tr(lang, "dbview_pick")).size(13.0).color(pal.text_faint));
                        if env.db_titles.is_empty() {
                            ui.label(RichText::new(tr(lang, "dbview_none")).color(pal.text_faint));
                        }
                        for t in env.db_titles {
                            if ui.button(format!("⊞  {t}")).clicked() {
                                blocks[i].text = t.clone();
                                out.changed = true;
                            }
                        }
                    }
                });
            });
        ui.add_space(4.0);
        ui.allocate_rect(ui.min_rect(), Sense::hover())
    }
}
