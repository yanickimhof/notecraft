//! Presentation mode: a page shown as full-screen slides, split at dividers or at titles and
//! section headings. Arrow keys, space and page keys move; Esc leaves.

use crate::app::NcApp;
use crate::theme::Palette;
use eframe::egui::{self, text::LayoutJob, Color32, FontId, Key, RichText, TextFormat};
use notecraft_core::blocks::{self, Block, BlockKind};
use notecraft_core::inline;

pub struct Present {
    pub blocks: Vec<Block>,
    pub slides: Vec<std::ops::Range<usize>>,
    pub at: usize,
    /// Per slide shrink factor so that a long slide fits the screen (found while drawing).
    pub fit: Vec<f32>,
    /// Zoom chosen with + / - / 0 or Cmd+scroll.
    pub zoom: f32,
    pub title: String,
}

fn job(text: &str, size: f32, color: Color32, pal: &Palette, wrap: f32, bold: bool) -> LayoutJob {
    let mut j = LayoutJob::default();
    j.wrap.max_width = wrap;
    for seg in inline::parse(text) {
        if seg.marker {
            continue;
        }
        let s = &seg.style;
        let mut f = TextFormat {
            font_id: FontId::proportional(size),
            color,
            line_height: Some(size * 1.4),
            ..Default::default()
        };
        if s.bold || bold {
            f.font_id = FontId::proportional(size);
            if crate::theme::has_weight_axis() {
                f.coords = crate::theme::weight(700.0);
            }
        }
        f.italics = s.italic;
        if s.strike {
            f.strikethrough = egui::Stroke::new(2.0, color);
        }
        if s.code {
            f.font_id = FontId::monospace(size * 0.85);
            f.background = pal.code_bg;
        }
        if let Some(c) = &s.color {
            f.color = pal.option_tokens(c).2;
        }
        if s.mention.is_some() || s.link.is_some() {
            f.color = pal.mention;
        }
        j.append(&text[seg.range.clone()], 0.0, f);
    }
    j
}

impl NcApp {
    /// Show the current page as slides.
    pub fn start_presentation(&mut self, ctx: &egui::Context) {
        self.flush_save();
        let Some(crate::app::Target::Doc(doc)) = self.current.clone() else { return };
        // A page is presented the way it is published: the same rendering, opened full width in the
        // browser, where scrolling, zoom (Cmd +/-) and full screen come for free.
        if self.shot.is_none() {
            if let notecraft_core::DocId::Page(id) = &doc {
                let id = id.clone();
                self.pub_preview(&id);
                return;
            }
        }
        let Some(ws) = &self.ws else { return };
        let Some(blocks) = ws.blocks(&doc).cloned() else { return };
        if blocks.is_empty() {
            self.notify(self.t("present_empty").to_string());
            return;
        }
        // the whole page is shown at once and scrolls; no cutting into slides
        let title = ws.title(&doc);
        let slides = vec![0..blocks.len()];
        let fit = vec![1.0; slides.len()];
        self.present = Some(Present { blocks, slides, at: 0, fit, zoom: 1.0, title });
        if self.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
    }

    fn stop_presentation(&mut self, ctx: &egui::Context) {
        self.present = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    }

    /// Draw the slide and handle the keys. Called instead of the normal window while presenting.
    pub fn present_view(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let pal = self.pal;
        let (next, prev, first, last, exit) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowRight)
                    || i.key_pressed(Key::ArrowDown)
                    || i.key_pressed(Key::Space)
                    || i.key_pressed(Key::PageDown)
                    || i.key_pressed(Key::Enter),
                i.key_pressed(Key::ArrowLeft)
                    || i.key_pressed(Key::ArrowUp)
                    || i.key_pressed(Key::PageUp)
                    || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::Escape) || i.key_pressed(Key::Q),
            )
        });
        let zoom_delta = ctx.input(|i| {
            let mut d = 0.0;
            if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                d += 0.1;
            }
            if i.key_pressed(Key::Minus) {
                d -= 0.1;
            }
            if i.modifiers.command {
                d += i.smooth_scroll_delta.y / 600.0;
            }
            d
        });
        let zoom_reset = ctx.input(|i| i.key_pressed(Key::Num0));
        let root = self.ws.as_ref().map(|w| w.root.clone()).unwrap_or_default();
        let Some(p) = self.present.as_mut() else { return };
        let n = p.slides.len();
        if next && p.at + 1 < n {
            p.at += 1;
        }
        if prev && p.at > 0 {
            p.at -= 1;
        }
        if first {
            p.at = 0;
        }
        if last {
            p.at = n - 1;
        }
        if zoom_reset {
            p.zoom = 1.0;
        }
        p.zoom = (p.zoom + zoom_delta).clamp(0.5, 3.0);
        let zoom = p.zoom;
        let title = p.title.clone();
        let heads: Vec<(u8, String)> = p
            .blocks
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::Heading(l) => Some((l, b.text.clone())),
                _ => None,
            })
            .collect();
        let range = p.slides[p.at].clone();
        let blocks: Vec<Block> = p.blocks[range].iter().filter(|b| b.kind != BlockKind::Divider).cloned().collect();
        let at = p.at;
        let _ = (at, &p.fit);
        let new_fit: Option<f32> = None;
        let clicked_next = false;
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(pal.bg)).show(ui, |ui| {
            let rect = ui.max_rect();
            let scale = (rect.height() / 1100.0).clamp(0.7, 1.4) * zoom;
            let width = (rect.width() * 0.78).min(1400.0 * scale);
            let side = ((rect.width() - width) / 2.0).max(0.0);
            let resp = ui.interact(rect, ui.scope_id().with("slide"), egui::Sense::click());
            ui.add_space(rect.height() * 0.05);
            let top = ui.cursor().min.y;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        if !title.is_empty() {
                            ui.label(job(&title, 56.0 * scale, pal.text, &pal, width, true));
                            ui.add_space(20.0 * scale);
                        }
                        let mut i = 0;
                        let mut num = 0;
                        while i < blocks.len() {
                            let b = &blocks[i];
                            if matches!(b.kind, BlockKind::ColumnsStart) {
                                // side by side: gather the columns up to the closing marker
                                let mut cols: Vec<Vec<&Block>> = vec![Vec::new()];
                                i += 1;
                                while i < blocks.len() && !matches!(blocks[i].kind, BlockKind::ColumnsEnd) {
                                    if matches!(blocks[i].kind, BlockKind::ColumnBreak) {
                                        cols.push(Vec::new());
                                    } else if let Some(c) = cols.last_mut() {
                                        c.push(&blocks[i]);
                                    }
                                    i += 1;
                                }
                                i += 1;
                                let n = cols.len().max(1);
                                let cw = (width - 24.0 * scale * (n as f32 - 1.0)) / n as f32;
                                ui.columns(n, |uis| {
                                    for (cui, items) in uis.iter_mut().zip(&cols) {
                                        cui.set_width(cw);
                                        let mut k = 0;
                                        for cb in items {
                                            k = if matches!(cb.kind, BlockKind::Numbered) { k + 1 } else { 0 };
                                            draw_block(cui, cb, k, &pal, scale, cw, &root, &heads);
                                        }
                                    }
                                });
                                continue;
                            }
                            if matches!(b.kind, BlockKind::Numbered) {
                                num += 1;
                            } else {
                                num = 0;
                            }
                            draw_block(ui, b, num, &pal, scale, width, &root, &heads);
                            i += 1;
                        }
                    });
                });
                ui.add_space(rect.height() * 0.3);
            });
            let _ = top;
            let _ = &resp;
            // position, bottom right
            ui.painter().text(
                rect.right_bottom() - egui::vec2(24.0, 20.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("{}%", (zoom * 100.0).round()),
                FontId::proportional(14.0 * scale),
                pal.text_faint,
            );
        });
        if let Some(f) = new_fit {
            if let Some(p) = self.present.as_mut() {
                p.fit[at] = f;
            }
            ctx.request_repaint();
        }
        if clicked_next {
            if let Some(p) = self.present.as_mut() {
                if p.at + 1 < p.slides.len() {
                    p.at += 1;
                }
            }
        }
        if exit {
            self.stop_presentation(&ctx);
        }
        let _ = ui;
    }
}

fn draw_block(
    ui: &mut egui::Ui,
    b: &Block,
    num: usize,
    pal: &Palette,
    scale: f32,
    width: f32,
    root: &std::path::Path,
    heads: &[(u8, String)],
) {
    let body = 24.0 * scale;
    match &b.kind {
        BlockKind::Heading(l) => {
            let size = match l {
                1 => 48.0,
                2 => 38.0,
                _ => 30.0,
            } * scale;
            ui.label(job(&b.text, size, pal.text, pal, width, true));
            ui.add_space(size * 0.3);
        }
        BlockKind::Bulleted | BlockKind::Numbered | BlockKind::Todo { .. } => {
            let mark = match &b.kind {
                BlockKind::Numbered => format!("{num}."),
                BlockKind::Todo { checked: true } => "☑".to_string(),
                BlockKind::Todo { checked: false } => "☐".to_string(),
                _ => "•".to_string(),
            };
            let indent = b.depth as f32 * 40.0 * scale;
            ui.horizontal_top(|ui| {
                ui.add_space(indent);
                ui.label(RichText::new(mark).size(body).color(pal.text_secondary));
                ui.label(job(&b.text, body, pal.text, pal, width - indent - 50.0 * scale, false));
            });
            ui.add_space(body * 0.25);
        }
        BlockKind::Quote => {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new("▌").size(body).color(pal.border));
                ui.label(job(&b.text, body, pal.text_secondary, pal, width - 40.0 * scale, false));
            });
            ui.add_space(body * 0.4);
        }
        BlockKind::Callout { icon } => {
            egui::Frame::NONE.fill(pal.callout_bg).corner_radius(8.0).inner_margin(16.0 * scale).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(icon).size(body));
                    ui.label(job(&b.text, body, pal.text, pal, width - 90.0 * scale, false));
                });
            });
            ui.add_space(body * 0.4);
        }
        BlockKind::Code { lang }
            if lang == "mermaid" && crate::mermaid_more::prepare(ui.painter(), &b.text).is_some() =>
        {
            if let Some(drawn) = crate::mermaid_more::prepare(ui.painter(), &b.text) {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, drawn.size().y + 4.0), egui::Sense::hover());
                // drawn at natural size, centred, clipped to the slide width
                let origin = egui::pos2((rect.center().x - drawn.size().x / 2.0).max(rect.min.x), rect.min.y);
                let painter = ui.painter().with_clip_rect(rect);
                let colors = crate::mermaid::Colors {
                    ink: pal.text,
                    fill: pal.bg,
                    line: pal.text_secondary,
                    soft: pal.text_secondary,
                    bg: pal.code_bg,
                };
                drawn.draw(&painter, origin, &colors);
            }
            ui.add_space(body * 0.4);
        }
        BlockKind::Code { .. } => {
            egui::Frame::NONE.fill(pal.code_bg).corner_radius(8.0).inner_margin(16.0 * scale).show(ui, |ui| {
                ui.label(RichText::new(&b.text).monospace().size(body * 0.8).color(pal.text));
            });
            ui.add_space(body * 0.4);
        }
        BlockKind::Image { path } => {
            let abs = root.join(path);
            if abs.is_file() {
                let w = if (10..100).contains(&b.width) { width * f32::from(b.width) / 100.0 } else { width };
                let mut img = egui::Image::new(format!("file://{}", abs.display())).corner_radius(8.0);
                if let Some((iw, ih)) = crate::editor::image_dims(&abs) {
                    let k = (w / iw as f32).min(640.0 * scale / ih as f32).min(1.0);
                    img = img.fit_to_exact_size(egui::vec2(iw as f32 * k, ih as f32 * k));
                }
                ui.add(img);
            }
            if !b.text.is_empty() {
                ui.label(RichText::new(&b.text).size(body * 0.6).color(pal.text_secondary));
            }
            ui.add_space(body * 0.4);
        }
        BlockKind::Table => {
            let cells = blocks::table_cells(&b.text);
            egui::Grid::new(("slide-table", b.id)).striped(true).spacing([24.0 * scale, 10.0 * scale]).show(ui, |ui| {
                for row in &cells {
                    for c in row {
                        ui.label(job(c, body * 0.8, pal.text, pal, width / row.len().max(1) as f32, false));
                    }
                    ui.end_row();
                }
            });
            ui.add_space(body * 0.4);
        }
        BlockKind::Equation => {
            let lay = crate::math::layout(ui.painter(), &crate::math::parse(&b.text), body * 1.3);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(width, lay.height().max(body) + 16.0), egui::Sense::hover());
            let origin = egui::pos2(rect.center().x - lay.w / 2.0, rect.min.y + 8.0 + crate::math::ascent(&lay));
            crate::math::draw(ui.painter(), &lay, origin, pal.text);
            ui.add_space(body * 0.3);
        }
        BlockKind::Paragraph | BlockKind::Toggle { .. } => {
            if !b.text.trim().is_empty() {
                ui.label(job(&b.text, body, pal.text, pal, width, false));
                ui.add_space(body * 0.4);
            }
        }
        BlockKind::Bookmark { url } => {
            ui.label(RichText::new(format!("{}  {}", b.text, url)).size(body * 0.7).color(pal.mention));
        }
        BlockKind::Toc => {
            for (l, t) in heads {
                ui.horizontal(|ui| {
                    ui.add_space((f32::from(*l) - 1.0) * 24.0 * scale);
                    ui.label(
                        RichText::new(notecraft_core::inline::strip(t)).size(body * 0.85).color(pal.text_secondary),
                    );
                });
            }
            ui.add_space(body * 0.4);
        }
        BlockKind::Database | BlockKind::Meeting => {
            ui.label(RichText::new(format!("▦  {}", b.text)).size(body).color(pal.text_secondary));
            ui.add_space(body * 0.4);
        }
        BlockKind::File { path } => {
            let name = if b.text.is_empty() { path.as_str() } else { b.text.as_str() };
            ui.label(RichText::new(format!("📎  {name}")).size(body * 0.9).color(pal.mention));
            ui.add_space(body * 0.4);
        }
        _ => {}
    }
}
