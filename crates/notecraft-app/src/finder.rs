//! Quick finder (⌘P / ⌘K): fuzzy title matches plus full-text hits from the
//! SQLite FTS5 index. Also used by "Link to page" to pick a page to mention.

use crate::app::{NcApp, Target};
use crate::i18n::tr;
use eframe::egui::{
    self, text::LayoutJob, Color32, CornerRadius, FontId, Id, Key, Modifiers, RichText, Sense, TextFormat, Vec2,
};
use notecraft_core::DocId;

#[derive(Debug, Clone)]
pub enum FinderMode {
    Jump,
    /// Insert `[[title]]` into `block` of `target` at char `at`.
    Link {
        target: Target,
        block: u64,
        at: usize,
    },
    /// Insert the blocks of a template page at `index` of the current page.
    Template {
        index: usize,
        replace: bool,
    },
}

pub struct Finder {
    pub query: String,
    pub selected: usize,
    pub mode: FinderMode,
    first_frame: bool,
}

impl Finder {
    pub fn new(mode: FinderMode) -> Self {
        Finder { query: String::new(), selected: 0, mode, first_frame: true }
    }
}

/// A snippet with `\u{1}`…`\u{2}` around matches, laid out with highlights.
fn snippet_job(s: &str, normal: Color32, strong: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut on = false;
    for part in s.split(['\u{1}', '\u{2}']) {
        let _ = part;
    }
    let mut buf = String::new();
    let flush = |job: &mut LayoutJob, buf: &mut String, on: bool| {
        if !buf.is_empty() {
            let f = TextFormat {
                font_id: FontId::proportional(12.5),
                color: if on { strong } else { normal },
                background: if on { strong.gamma_multiply(0.12) } else { Color32::TRANSPARENT },
                ..Default::default()
            };
            job.append(&buf.replace('\n', " "), 0.0, f);
            buf.clear();
        }
    };
    for c in s.chars() {
        match c {
            '\u{1}' => {
                flush(&mut job, &mut buf, on);
                on = true;
            }
            '\u{2}' => {
                flush(&mut job, &mut buf, on);
                on = false;
            }
            c => buf.push(c),
        }
    }
    flush(&mut job, &mut buf, on);
    job
}

impl NcApp {
    pub fn finder_ui(&mut self, ctx: &egui::Context) {
        let Some(mut finder) = self.finder.take() else { return };
        let Some(ws) = &self.ws else { return };
        let pal = self.pal;
        let lang = self.prefs.lang;
        let results = match finder.mode {
            FinderMode::Template { .. } => crate::app::template_results(ws, &finder.query),
            _ => ws.search(&finder.query, 24),
        };
        let n = results.len();
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) && n > 0 {
            finder.selected = (finder.selected + 1) % n;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) && n > 0 {
            finder.selected = (finder.selected + n - 1) % n;
        }
        let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let mut chosen: Option<(DocId, String)> = None;
        let screen = ctx.content_rect();
        let width = crate::theme::QUICK_FIND_SIZE[0].min(screen.width() - 40.0);
        let max_list = (crate::theme::QUICK_FIND_SIZE[1] - 70.0).min(screen.height() * 0.7);
        // the click that opened the finder (sidebar button) must not close it in the same frame
        let opened_now = finder.first_frame;
        let mut clicked_outside = false;
        // dim the window behind
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, Id::unique("finder-dim")));
        painter.rect_filled(screen, 0.0, pal.overlay);
        let area = egui::Area::new(Id::unique("finder"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.center().x - width / 2.0, screen.min.y + screen.height() * 0.14))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(0))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("🔍").size(16.0).color(pal.text_faint));
                                let hint = match finder.mode {
                                    FinderMode::Jump => tr(lang, "find_hint"),
                                    FinderMode::Link { .. } => tr(lang, "search_pages"),
                                    FinderMode::Template { .. } => tr(lang, "search_templates"),
                                };
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut finder.query)
                                        .id(Id::unique("finder-input"))
                                        .frame(egui::Frame::NONE)
                                        .font(FontId::proportional(17.0))
                                        .desired_width(width - 60.0)
                                        .hint_text(RichText::new(hint).color(pal.text_faint)),
                                );
                                if finder.first_frame {
                                    r.request_focus();
                                    finder.first_frame = false;
                                }
                                if r.changed() {
                                    finder.selected = 0;
                                }
                            });
                        });
                        let sep = ui.cursor();
                        ui.painter().hline(
                            egui::Rangef::new(sep.min.x, sep.min.x + width),
                            sep.min.y,
                            egui::Stroke::new(1.0, pal.border),
                        );
                        egui::Frame::NONE.inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                            if results.is_empty() {
                                ui.add_space(8.0);
                                ui.label(RichText::new(tr(lang, "no_results")).color(pal.text_faint));
                                ui.add_space(8.0);
                            }
                            egui::ScrollArea::vertical().max_height(max_list).show(ui, |ui| {
                                for (k, r) in results.iter().enumerate() {
                                    let h = if r.snippet.is_empty() { 40.0 } else { 56.0 };
                                    let (rect, resp) =
                                        ui.allocate_exact_size(Vec2::new(width - 12.0, h), Sense::click());
                                    if k == finder.selected {
                                        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.hover);
                                        if ui.input(|i| i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)) {
                                            resp.scroll_to_me(None);
                                        }
                                    }
                                    if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                                        finder.selected = k;
                                    }
                                    let mut child = ui.new_child(
                                        egui::UiBuilder::new()
                                            .max_rect(rect.shrink2(Vec2::new(10.0, 4.0)))
                                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                    );
                                    let icon = if r.icon.is_empty() {
                                        match r.doc {
                                            DocId::Database(_) => "🗂".to_string(),
                                            _ => "📄".to_string(),
                                        }
                                    } else {
                                        r.icon.clone()
                                    };
                                    child.label(RichText::new(icon).size(17.0));
                                    child.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            let title = if r.title.is_empty() {
                                                tr(lang, "untitled").to_string()
                                            } else {
                                                r.title.clone()
                                            };
                                            ui.label(RichText::new(title).size(14.5).color(pal.text));
                                            if !r.path.is_empty() {
                                                ui.label(
                                                    RichText::new(format!("— {}", r.path))
                                                        .size(12.5)
                                                        .color(pal.text_faint),
                                                );
                                            }
                                        });
                                        if !r.snippet.is_empty() {
                                            let mut job = snippet_job(&r.snippet, pal.text_secondary, pal.text);
                                            job.wrap.max_width = width - 80.0;
                                            job.wrap.max_rows = 1;
                                            ui.label(job);
                                        }
                                    });
                                    if resp.clicked() {
                                        chosen = Some((r.doc.clone(), r.title.clone()));
                                    }
                                }
                            });
                        });
                    });
            });
        if area.response.clicked_elsewhere() {
            clicked_outside = true;
        }
        if enter {
            if let Some(r) = results.get(finder.selected) {
                chosen = Some((r.doc.clone(), r.title.clone()));
            }
        }
        if let Some((doc, title)) = chosen {
            match finder.mode.clone() {
                FinderMode::Jump => self.navigate(Target::Doc(doc)),
                FinderMode::Template { index, replace } => self.insert_template(doc, index, replace),
                FinderMode::Link { target, block, at } => {
                    if let Some(ws) = self.ws.as_mut() {
                        let key = target.key();
                        if let Some(blocks) = NcApp::blocks_mut(ws, &target) {
                            if self.editor.insert_text(&key, blocks, block, at, &format!("[[{title}]]")) {
                                self.mark_dirty();
                            }
                        }
                    }
                }
            }
            return;
        }
        if escape || (clicked_outside && !opened_now) {
            return;
        }
        self.finder = Some(finder);
    }
}
