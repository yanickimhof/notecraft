//! The image block: loading, caption, resizing.

use super::*;

impl EditorState {
    pub(super) fn image_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        cx: &BlockCx,
        sink: &mut Sink,
        path: &str,
    ) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let deferred = &mut *sink.deferred;
        let out = &mut *sink.out;
        let pal = env.pal;
        let bid = blocks[i].id;
        let abs = env.root.join(path);
        let blocks_width = blocks[i].width;
        let blocks_align = blocks[i].align;
        let mut new_align: Option<u8> = None;
        let mut new_width: Option<u16> = None;
        let mut clear_caption = false;
        let mut size_began = false;
        // width of the picture and its left padding: the caption lines up with it
        let mut geom = (width, 0.0f32);
        let r = ui.vertical(|ui| {
            ui.set_width(width);
            if abs.is_file() {
                let uri = format!("file://{}", abs.display());
                let mut img = egui::Image::new(uri).corner_radius(CornerRadius::same(6)).sense(Sense::click());
                // explicit size from the file header: never depends on the decode state of the loader
                let mut shown = Vec2::new(width, 0.0);
                if let Some((iw, ih)) = image_dims(&abs) {
                    let (iw, ih) = (iw as f32, ih as f32);
                    let pct = blocks_width;
                    let k = if (10..100).contains(&pct) {
                        width * f32::from(pct) / 100.0 / iw
                    } else {
                        (width / iw).min(520.0 / ih).min(1.0)
                    };
                    shown = Vec2::new(iw * k, ih * k);
                    img = img.fit_to_exact_size(shown);
                } else {
                    img = img.max_width(width).max_height(520.0);
                }
                // left, centre or right in the column
                ui.add_space(0.0);
                let pad = match blocks_align {
                    1 => 0.0,
                    2 => (width - shown.x).max(0.0),
                    _ => ((width - shown.x) / 2.0).max(0.0),
                };
                geom = (shown.x, pad);
                ui.horizontal(|ui| {
                    ui.add_space(pad);
                    let resp = ui.add(img);
                    if resp.clicked() {
                        self.sel_anchor = None;
                        self.selected = Some(bid);
                    }
                    if resp.double_clicked() {
                        deferred.push(Deferred::Open(path.to_string()));
                    }
                    resp.context_menu(|ui| {
                        for (a, key) in [(1u8, "img_left"), (0, "img_center"), (2, "img_right")] {
                            if ui.selectable_label(blocks_align == a, tr(env.lang, key)).clicked() {
                                new_align = Some(a);
                                ui.close();
                            }
                        }
                    });
                    // a handle on the corner: drag to resize, double click to go back to the natural size
                    let handle_id = ui.make_persistent_id(("img-size", bid));
                    let corner = if blocks_align == 2 {
                        resp.rect.left_bottom() + egui::vec2(12.0, -12.0)
                    } else {
                        resp.rect.right_bottom() - egui::vec2(12.0, 12.0)
                    };
                    let grab = egui::Rect::from_center_size(corner, egui::vec2(18.0, 18.0));
                    let active = ui.ctx().is_being_dragged(handle_id);
                    if resp.hovered() || self.selected == Some(bid) || active || ui.rect_contains_pointer(grab) {
                        let h = ui.interact(grab, handle_id, Sense::click_and_drag());
                        ui.painter().circle(grab.center(), 6.0, Color32::WHITE, Stroke::new(1.5, pal.accent));
                        let h = h.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                        if h.drag_started() {
                            size_began = true;
                        }
                        if h.dragged() {
                            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                                let w = match blocks_align {
                                    1 => p.x - resp.rect.min.x,
                                    2 => resp.rect.max.x - p.x,
                                    _ => 2.0 * (p.x - resp.rect.center().x),
                                };
                                let pct = (w / width * 100.0).round().clamp(10.0, 100.0) as u16;
                                new_width = Some(if pct >= 100 { 0 } else { pct });
                            }
                        }
                        if h.double_clicked() {
                            new_width = Some(0);
                        }
                    }
                    // the image bar: alignment, size, caption, replace, delete
                    // the bar is a layer above the page, so `rect_contains_pointer` is false on it: use the raw position
                    let pointer = ui.input(|i| i.pointer.hover_pos());
                    let near_bar = self
                        .img_bar
                        .is_some_and(|(b, r)| b == bid && pointer.is_some_and(|p| r.expand(6.0).contains(p)));
                    if resp.hovered() || self.selected == Some(bid) || active || near_bar {
                        let pos = egui::pos2(resp.rect.center().x, resp.rect.min.y + 8.0);
                        let current = if blocks_width == 0 { 100 } else { blocks_width };
                        let area = egui::Area::new(ui.make_persistent_id(("img-bar", bid)))
                            .order(egui::Order::Foreground)
                            .pivot(Align2::CENTER_TOP)
                            .fixed_pos(pos)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        for (a, key) in
                                            [(1u8, "img_bar_left"), (0, "img_bar_center"), (2, "img_bar_right")]
                                        {
                                            if ui.selectable_label(blocks_align == a, tr(env.lang, key)).clicked() {
                                                new_align = Some(a);
                                            }
                                        }
                                        ui.separator();
                                        for pct in [25u16, 50, 75, 100] {
                                            if ui.selectable_label(current == pct, format!("{pct}%")).clicked() {
                                                new_width = Some(if pct >= 100 { 0 } else { pct });
                                            }
                                        }
                                        ui.separator();
                                        // the caption button is a switch: it opens the field, or removes the caption
                                        let has_caption = !blocks[i].text.is_empty() || self.caption_open == Some(bid);
                                        if ui.selectable_label(has_caption, tr(env.lang, "img_caption")).clicked() {
                                            if has_caption {
                                                clear_caption = true;
                                            } else {
                                                self.caption_open = Some(bid);
                                                self.focus = Some((bid, Caret::End));
                                            }
                                        }
                                        if ui.button(tr(env.lang, "img_replace")).clicked() {
                                            out.actions.push(Action::PickFile { index: i, replace: true, image: true });
                                        }
                                        if ui.button(tr(env.lang, "delete")).clicked() {
                                            deferred.push(Deferred::Delete(i));
                                        }
                                    });
                                });
                            });
                        self.img_bar = Some((bid, area.response.rect));
                    } else if self.img_bar.is_some_and(|(b, _)| b == bid) {
                        self.img_bar = None;
                    }
                });
            } else {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 64.0), Sense::hover());
                ui.painter().rect_filled(rect, CornerRadius::same(6), pal.code_bg);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    format!("🖼  {path}"),
                    FontId::proportional(13.0),
                    pal.text_secondary,
                );
            }
            let id = text_id(bid);
            self.apply_focus_request(ui.ctx(), &blocks[i], id);
            let before = blocks[i].text.clone();
            // the caption is optional: shown when it has text, was asked for, or has the caret
            let has_focus = ui.ctx().memory(|m| m.has_focus(id));
            if !blocks[i].text.is_empty() || self.caption_open == Some(bid) || has_focus {
                let cw = geom.0.max(160.0).min(width);
                let (pad, align) = match blocks_align {
                    1 => (0.0, Align2::LEFT_CENTER),
                    2 => ((width - cw).max(0.0), Align2::RIGHT_CENTER),
                    _ => (((width - cw) / 2.0).max(0.0), Align2::CENTER_CENTER),
                };
                ui.horizontal(|ui| {
                    ui.add_space(pad);
                    let o = egui::TextEdit::singleline(&mut blocks[i].text)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .desired_width(cw)
                        .align(align)
                        .font(FontId::proportional(13.0))
                        .text_color(pal.text_secondary)
                        .hint_text(RichText::new(tr(env.lang, "caption")).color(pal.text_faint))
                        .show(ui);
                    if o.response.response.has_focus() {
                        self.focused_block = Some(bid);
                    }
                    if o.response.response.lost_focus() && self.caption_open == Some(bid) {
                        self.caption_open = None;
                    }
                });
            }
            if blocks[i].text != before {
                let mut snap = blocks.to_vec();
                snap[i].text = before;
                self.record(env.key, &snap, Some(&format!("t{bid}")));
                out.changed = true;
            }
        });
        if size_began {
            let snap = blocks.to_vec();
            self.record(env.key, &snap, Some(&format!("w{bid}")));
        }
        if clear_caption {
            let snap = blocks.to_vec();
            self.record(env.key, &snap, None);
            blocks[i].text.clear();
            self.caption_open = None;
            if self.focused_block == Some(bid) {
                self.focused_block = None;
            }
            ui.ctx().memory_mut(|m| m.surrender_focus(text_id(bid)));
            out.changed = true;
        }
        if let Some(a) = new_align {
            if blocks[i].align != a {
                let snap = blocks.to_vec();
                self.record(env.key, &snap, None);
                blocks[i].align = a;
                out.changed = true;
            }
        }
        if let Some(w) = new_width {
            if blocks[i].width != w {
                blocks[i].width = w;
                out.changed = true;
            }
        }
        r.response
    }
}
