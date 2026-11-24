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
        let Some(ws) = &self.ws else { return };
        let Some(blocks) = ws.blocks(&doc).cloned() else { return };
        if blocks.is_empty() {
            self.notify(self.t("present_empty").to_string());
            return;
        }
        let slides = blocks::slides(&blocks);
        self.present = Some(Present { blocks, slides, at: 0 });
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
        let range = p.slides[p.at].clone();
        let blocks: Vec<Block> = p.blocks[range].iter().filter(|b| b.kind != BlockKind::Divider).cloned().collect();
        let at = p.at;
        let mut clicked_next = false;
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(pal.bg)).show(ui, |ui| {
            let rect = ui.max_rect();
            let scale = (rect.height() / 800.0).clamp(0.8, 2.5);
            let width = (rect.width() * 0.78).min(1400.0 * scale);
            let side = ((rect.width() - width) / 2.0).max(0.0);
            let resp = ui.interact(rect, ui.scope_id().with("slide"), egui::Sense::click());
            if resp.clicked() {
                clicked_next = true;
            }
            ui.add_space(rect.height() * 0.1);
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    let mut num = 0;
                    for b in &blocks {
                        if matches!(b.kind, BlockKind::Numbered) {
                            num += 1;
                        } else {
                            num = 0;
                        }
                        draw_block(ui, b, num, &pal, scale, width, &root);
                    }
                });
            });
            // position, bottom right
            ui.painter().text(
                rect.right_bottom() - egui::vec2(24.0, 20.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("{} / {}", at + 1, n),
                FontId::proportional(14.0 * scale),
                pal.text_faint,
            );
        });
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

fn draw_block(ui: &mut egui::Ui, b: &Block, num: usize, pal: &Palette, scale: f32, width: f32, root: &std::path::Path) {
    let body = 28.0 * scale;
    match &b.kind {
        BlockKind::Heading(l) => {
            let size = match l {
                1 => 60.0,
                2 => 46.0,
                _ => 36.0,
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
                ui.add(
                    egui::Image::new(format!("file://{}", abs.display())).max_width(width).max_height(480.0 * scale),
                );
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
        _ => {}
    }
}
