//! The main area: cover, icon, title, backlinks, row properties, the block
//! editor, and database views.

use crate::app::{title_map, NcApp, Target};
use crate::editor::Env;
use crate::i18n::tr;
use crate::theme::{self, COVERS};
use crate::widgets;
use eframe::egui::{self, CornerRadius, FontId, Id, Key, Modifiers, RichText, Sense, Ui, Vec2};
use notecraft_core::DocId;

const GUTTER: f32 = 46.0;

impl NcApp {
    pub fn main_view(&mut self, ui: &mut Ui) {
        let Some(target) = self.current.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(self.t("no_pages")).color(self.pal.text_faint));
            });
            return;
        };
        let pal = self.pal;
        let is_db = matches!(target, Target::Doc(DocId::Database(_)));
        let mut area = egui::ScrollArea::vertical().id_salt(("main-scroll", target.key())).auto_shrink([false, false]);
        if let Some(y) = self.shot_scroll {
            area = area.vertical_scroll_offset(y);
        }
        area.show(
            ui,
            |ui| {
                let avail = ui.available_width();
                // cover across the whole width
                let cover = self.cover_of(&target);
                if !cover.is_empty() {
                    self.cover_ui(ui, &target, &cover, avail);
                } else {
                    ui.add_space(52.0);
                }
                let wide = matches!(&target, Target::Doc(DocId::Page(id)) if self.ws.as_ref().and_then(|w| w.pages.get(id)).is_some_and(|p| p.meta.full_width));
                let max_w = if is_db {
                    1180.0
                } else if wide {
                    avail
                } else {
                    theme::PAGE_MAX_WIDTH
                };
                let col = (avail - 96.0).clamp(320.0, max_w);
                let left = ((avail - col) / 2.0 - GUTTER).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(left);
                    ui.vertical(|ui| {
                        ui.set_width(col + GUTTER);
                        self.conflict_banner(ui, &target);
                        self.header_ui(ui, &target, !cover.is_empty());
                        match &target {
                            Target::Doc(DocId::Database(db)) => {
                                let db = db.clone();
                                ui.horizontal_top(|ui| {
                                    ui.add_space(GUTTER);
                                    ui.vertical(|ui| {
                                        ui.set_width(col);
                                        let lang = self.prefs.lang;
                                        let locked = self.is_target_locked(&target);
                                        if locked {
                                            self.lock_banner(ui);
                                        }
                                        if let Some(ws) = self.ws.as_mut() {
                                            let actions = ui
                                                .add_enabled_ui(!locked, |ui| self.dbui.show(ui, ws, &db, &pal, lang))
                                                .inner;
                                            self.handle_db_actions(actions);
                                        }
                                    });
                                });
                            }
                            Target::Doc(DocId::Page(id)) if self.ws.as_ref().is_some_and(|w| w.is_page_locked(id)) => {
                                let id = id.clone();
                                self.locked_page_ui(ui, &id);
                            }
                            _ => {
                                if let Target::Doc(DocId::Row(db, r)) = &target {
                                    let locked = self.is_target_locked(&target);
                                    ui.add_enabled_ui(!locked, |ui| self.row_props_ui(ui, db, r, false));
                                }
                                if let Target::Template(db, t) = &target {
                                    self.row_props_ui(ui, db, t, true);
                                }
                                self.editor_ui(ui, &target);
                            }
                        }
                        ui.add_space(120.0);
                    });
                });
            },
        );
    }

    fn cover_of(&self, t: &Target) -> String {
        let Some(ws) = &self.ws else { return String::new() };
        match t {
            Target::Doc(d) => ws.cover(d),
            Target::Template(..) => String::new(),
        }
    }

    fn cover_ui(&mut self, ui: &mut Ui, target: &Target, cover: &str, width: f32) {
        let pal = self.pal;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 200.0), Sense::hover());
        if let Some(n) = cover.strip_prefix("gradient:").and_then(|n| n.parse::<usize>().ok()) {
            let (a, b) = COVERS[n % COVERS.len()];
            theme::paint_gradient(ui.painter(), rect, a, b);
        } else if let Some(ws) = &self.ws {
            let abs = ws.root.join(cover);
            if abs.is_file() {
                // cover the banner: scale to fill, crop the overflow around the centre
                let base = egui::Image::new(format!("file://{}", abs.display()));
                let mut uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                if let Ok(poll) = base.load_for_size(ui.ctx(), rect.size()) {
                    if let Some(sz) = poll.size() {
                        if sz.x > 0.0 && sz.y > 0.0 {
                            let scale = (rect.width() / sz.x).max(rect.height() / sz.y);
                            let fw = (rect.width() / (sz.x * scale)).min(1.0);
                            let fh = (rect.height() / (sz.y * scale)).min(1.0);
                            uv = egui::Rect::from_min_max(
                                egui::pos2((1.0 - fw) / 2.0, (1.0 - fh) / 2.0),
                                egui::pos2((1.0 + fw) / 2.0, (1.0 + fh) / 2.0),
                            );
                        }
                    }
                }
                base.uv(uv).fit_to_exact_size(rect.size()).maintain_aspect_ratio(false).paint_at(ui, rect);
            } else {
                ui.painter().rect_filled(rect, 0.0, pal.code_bg);
            }
        }
        if resp.hovered() || ui.rect_contains_pointer(rect) {
            let btn_area =
                egui::Rect::from_min_size(egui::pos2(rect.max.x - 330.0, rect.max.y - 40.0), Vec2::new(320.0, 30.0));
            let mut child = ui.new_child(
                egui::UiBuilder::new().max_rect(btn_area).layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            let style_btn = |t: &str| {
                egui::Button::new(RichText::new(t).size(12.5).color(pal.text))
                    .fill(pal.popup_bg.gamma_multiply(0.92))
                    .corner_radius(CornerRadius::same(4))
            };
            let mut new_cover = None;
            if child.add(style_btn(self.t("remove"))).clicked() {
                new_cover = Some(String::new());
            }
            if child.add(style_btn(self.t("cover_image"))).clicked() {
                if let Some(p) =
                    rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
                {
                    if let Some(ws) = self.ws.as_mut() {
                        if let Ok(rel) = ws.add_asset(&p) {
                            new_cover = Some(rel);
                        }
                    }
                }
            }
            if child.add(style_btn(self.t("change_cover"))).clicked() {
                let n =
                    cover.strip_prefix("gradient:").and_then(|n| n.parse::<usize>().ok()).map(|n| n + 1).unwrap_or(0);
                new_cover = Some(format!("gradient:{}", n % COVERS.len()));
            }
            if let (Some(c), Target::Doc(d), Some(ws)) = (new_cover, target, self.ws.as_mut()) {
                let _ = ws.set_cover(d, &c);
            }
        }
    }

    /// Icon, "add icon / add cover" controls, title, backlinks.
    fn header_ui(&mut self, ui: &mut Ui, target: &Target, has_cover: bool) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let Some(ws) = &self.ws else { return };
        let (title, icon, doc) = match target {
            Target::Doc(d) => (ws.title(d), ws.icon(d), Some(d.clone())),
            Target::Template(db, t) => (
                ws.dbs.get(db).and_then(|x| x.templates.get(t)).map(|r| r.meta.title.clone()).unwrap_or_default(),
                String::new(),
                None,
            ),
        };
        if let Target::Template(..) = target {
            ui.horizontal(|ui| {
                ui.add_space(GUTTER);
                egui::Frame::NONE
                    .fill(pal.accent_soft)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("📋  {}", tr(lang, "editing_template"))).size(13.0).color(pal.text),
                        );
                    });
            });
            ui.add_space(8.0);
        }
        let header_top = ui.cursor().min;
        // icon
        let mut set_icon: Option<String> = None;
        if !icon.is_empty() {
            if has_cover {
                ui.add_space(-46.0);
            }
            ui.horizontal(|ui| {
                ui.add_space(GUTTER - 6.0);
                let r = ui.add(egui::Button::new(RichText::new(&icon).size(60.0)).min_size(Vec2::splat(84.0)));
                if r.clicked() {
                    self.icon_picker = !self.icon_picker;
                }
                if self.icon_picker {
                    let mut chosen = None;
                    let a = egui::Area::new(Id::unique("icon-picker"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(r.rect.left_bottom())
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                chosen = widgets::emoji_grid(ui, &pal, tr(lang, "remove"), true);
                            });
                        });
                    if let Some(c) = chosen {
                        set_icon = Some(c);
                        self.icon_picker = false;
                    } else if a.response.clicked_elsewhere() && !r.clicked() {
                        self.icon_picker = false;
                    }
                }
            });
        }
        // add icon / add cover (on hover)
        let hover_zone = egui::Rect::from_min_size(header_top, Vec2::new(ui.available_width(), 130.0));
        let show_controls = ui.rect_contains_pointer(hover_zone) && doc.is_some() && !self.is_target_locked(target);
        let mut add_cover = false;
        ui.horizontal(|ui| {
            ui.add_space(GUTTER);
            ui.set_min_height(26.0);
            if show_controls {
                if icon.is_empty()
                    && widgets::subtle_button(
                        ui,
                        RichText::new(format!("☺  {}", tr(lang, "add_icon"))).size(13.0).color(pal.text_faint),
                        &pal,
                    )
                    .clicked()
                {
                    let n = (title.len() + 7) % theme::EMOJI.len();
                    set_icon = Some(theme::EMOJI[n].to_string());
                }
                if !has_cover
                    && widgets::subtle_button(
                        ui,
                        RichText::new(format!("▨  {}", tr(lang, "add_cover"))).size(13.0).color(pal.text_faint),
                        &pal,
                    )
                    .clicked()
                {
                    add_cover = true;
                }
            }
        });
        if let (Some(i), Some(d)) = (&set_icon, &doc) {
            if let Some(ws) = self.ws.as_mut() {
                let _ = ws.set_icon(d, i);
            }
        }
        if add_cover {
            if let (Some(d), Some(ws)) = (&doc, self.ws.as_mut()) {
                let n = (title.chars().count() * 3 + 1) % COVERS.len();
                let _ = ws.set_cover(d, &format!("gradient:{n}"));
            }
        }
        // title
        self.title_ui(ui, target, &title);
        // backlinks
        if let (Some(d), Some(ws)) = (&doc, &self.ws) {
            let links = ws.backlinks(d);
            if !links.is_empty() {
                let n = links.len();
                let label = format!("↙  {n} {}", if n == 1 { tr(lang, "backlink") } else { tr(lang, "backlinks") });
                let mut go = None;
                ui.horizontal(|ui| {
                    ui.add_space(GUTTER);
                    if widgets::subtle_button(ui, RichText::new(label).size(13.0).color(pal.text_secondary), &pal)
                        .clicked()
                    {
                        self.show_backlinks = !self.show_backlinks;
                    }
                });
                if self.show_backlinks {
                    for l in links {
                        let t = ws.title(&l);
                        let i = ws.icon(&l);
                        ui.horizontal(|ui| {
                            ui.add_space(GUTTER + 12.0);
                            let icon = if i.is_empty() { "📄".to_string() } else { i };
                            if widgets::subtle_button(
                                ui,
                                RichText::new(format!("{icon}  {t}")).size(13.5).color(pal.text).underline(),
                                &pal,
                            )
                            .clicked()
                            {
                                go = Some(l.clone());
                            }
                        });
                    }
                }
                if let Some(g) = go {
                    self.navigate(Target::Doc(g));
                }
            }
        }
        ui.add_space(6.0);
    }

    /// The "this page is locked" notice shown above read-only content.
    fn lock_banner(&self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        ui.horizontal(|ui| {
            ui.add_space(GUTTER);
            egui::Frame::NONE
                .fill(pal.accent_soft)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("🔒  {}", tr(lang, "locked_banner"))).size(13.0).color(pal.text));
                });
        });
        ui.add_space(8.0);
    }

    fn is_target_locked(&self, target: &Target) -> bool {
        match (&self.ws, target) {
            (Some(ws), Target::Doc(d)) => ws.is_locked(d),
            _ => false,
        }
    }

    fn title_ui(&mut self, ui: &mut Ui, target: &Target, title: &str) {
        let pal = self.pal;
        let id = Id::unique(("page-title", target.key()));
        let mut text = title.to_string();
        let locked = self.is_target_locked(target);
        let enter = ui.memory(|m| m.has_focus(id)) && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let mut f = egui::TextFormat {
                font_id: FontId::proportional(38.0),
                color: pal.text,
                line_height: Some(48.0),
                ..Default::default()
            };
            if theme::has_weight_axis() {
                f.coords = theme::weight(720.0);
            }
            let mut job = egui::text::LayoutJob::single_section(buf.as_str().to_string(), f);
            job.wrap.max_width = wrap;
            job.first_row_min_height = 48.0;
            ui.fonts_mut(|fo| fo.layout_job(job))
        };
        let resp = ui
            .horizontal(|ui| {
                ui.add_space(GUTTER);
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .id(id)
                        .interactive(!locked)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .layouter(&mut layouter)
                        .hint_text(RichText::new(self.t("untitled")).size(38.0).color(pal.text_faint)),
                )
            })
            .inner;
        if self.focus_title {
            resp.request_focus();
            self.focus_title = false;
        }
        if resp.gained_focus() {
            self.title_before = Some((target.clone(), title.to_string()));
        }
        let text = text.replace('\n', " ");
        if text != title {
            if let Some(ws) = self.ws.as_mut() {
                match target {
                    Target::Doc(DocId::Page(p)) => {
                        if let Some(pg) = ws.pages.get_mut(p) {
                            pg.meta.title = text.clone();
                        }
                    }
                    Target::Doc(DocId::Database(d)) => {
                        if let Some(db) = ws.dbs.get_mut(d) {
                            db.schema.title = text.clone();
                        }
                    }
                    Target::Doc(DocId::Row(d, r)) => {
                        if let Some(row) = ws.dbs.get_mut(d).and_then(|x| x.rows.get_mut(r)) {
                            row.meta.title = text.clone();
                        }
                    }
                    Target::Template(d, t) => {
                        if let Some(row) = ws.dbs.get_mut(d).and_then(|x| x.templates.get_mut(t)) {
                            row.meta.title = text.clone();
                        }
                    }
                }
            }
            self.mark_dirty();
        }
        if resp.lost_focus() || enter {
            if let Some((t, old)) = self.title_before.take() {
                if t == *target && old != text {
                    self.flush_save();
                    if let Some(ws) = self.ws.as_mut() {
                        if let Err(e) = ws.title_changed(&old, &text) {
                            self.notify(e.to_string());
                        }
                    }
                }
            }
        }
        if enter {
            ui.memory_mut(|m| m.surrender_focus(id));
            if let Some(ws) = self.ws.as_mut() {
                if let Some(blocks) = NcApp::blocks_mut(ws, target) {
                    if blocks.is_empty() {
                        blocks.push(notecraft_core::Block::paragraph(""));
                    }
                    self.editor.focus_first(blocks);
                }
            }
        }
    }

    fn row_props_ui(&mut self, ui: &mut Ui, db: &str, rid: &str, template: bool) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let Some(ws) = self.ws.as_mut() else { return };
        if !ws.dbs.contains_key(db) {
            return;
        }
        let actions = ui
            .horizontal_top(|ui| {
                ui.add_space(GUTTER);
                ui.vertical(|ui| self.dbui.row_properties(ui, ws, db, rid, template, &pal, lang)).inner
            })
            .inner;
        self.handle_db_actions(actions);
        ui.add_space(4.0);
        let c = ui.cursor();
        ui.painter().hline(
            egui::Rangef::new(c.min.x + GUTTER, c.min.x + ui.available_width()),
            c.min.y,
            egui::Stroke::new(1.0, pal.border),
        );
        ui.add_space(12.0);
    }

    fn editor_ui(&mut self, ui: &mut Ui, target: &Target) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let key = target.key();
        let (font, small) = match target {
            Target::Doc(DocId::Page(id)) => self
                .ws
                .as_ref()
                .and_then(|w| w.pages.get(id))
                .map_or((String::new(), false), |p| (p.meta.font.clone(), p.meta.small_text)),
            _ => (String::new(), false),
        };
        theme::set_page_style(&font, small);
        theme::set_text_percent(self.prefs.text_percent);
        self.refresh_mics();
        let meeting_view = self.meeting_view();
        let Some(ws0) = self.ws.as_ref() else { return };
        let locked = matches!(target, Target::Doc(d) if ws0.is_locked(d));
        if locked {
            self.lock_banner(ui);
        }
        let Some(ws) = self.ws.as_mut() else { return };
        let Some(blocks_ref) = NcApp::blocks_mut(ws, target) else { return };
        let mut blocks = std::mem::take(blocks_ref);
        let titles = title_map(ws);
        let root = ws.root.clone();
        let mut db_titles: Vec<String> =
            ws.dbs.values().filter(|d| d.schema.trashed.is_none()).map(|d| d.schema.title.clone()).collect();
        db_titles.sort();
        let ws_cell = std::cell::RefCell::new(&mut *ws);
        let dbui_cell = std::cell::RefCell::new(&mut self.dbui);
        let db_actions = std::cell::RefCell::new(Vec::new());
        let embed = |ui: &mut Ui, bid: u64, db: &str| {
            let mut ws = ws_cell.borrow_mut();
            if !ws.dbs.contains_key(db) {
                return;
            }
            let mut dbui = dbui_cell.borrow_mut();
            ui.push_id(("dbview", bid, db), |ui| {
                let acts = dbui.show(ui, &mut ws, db, &pal, lang);
                db_actions.borrow_mut().extend(acts);
            });
        };
        let env = Env {
            pal: &pal,
            lang,
            root: &root,
            titles: &titles,
            key: &key,
            meeting: &meeting_view,
            db_titles: &db_titles,
            embed: &embed,
        };
        if locked {
            // Read-only: the editor works on a throwaway copy, so nothing typed or dragged can reach the page.
            let mut shadow = blocks.clone();
            let _ = self.editor.show(ui, &mut shadow, &env);
            if let Some(ws) = self.ws.as_mut() {
                if let Some(b) = NcApp::blocks_mut(ws, target) {
                    *b = blocks;
                }
            }
            return;
        }
        let out = self.editor.show(ui, &mut blocks, &env);
        if let Some(ws) = self.ws.as_mut() {
            if let Some(b) = NcApp::blocks_mut(ws, target) {
                *b = blocks;
            }
        }
        let embedded = db_actions.take();
        if !embedded.is_empty() {
            self.handle_db_actions(embedded);
        }
        if out.changed {
            self.mark_dirty();
        }
        if !out.actions.is_empty() {
            self.handle_editor_actions(out.actions);
        }
    }
}
