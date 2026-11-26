//! The sidebar: workspace menu, search and settings, favourites, the page
//! tree with drag-to-reorder and nesting, new page / database, trash.

use crate::app::{NcApp, Target};
use crate::i18n::tr;
use crate::icons::Icon;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, RichText, Sense, Stroke, Ui, Vec2};
use notecraft_core::workspace::TreeItem;
use notecraft_core::DocId;

#[derive(Clone, Debug, PartialEq)]
struct SideDrag(DocId);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Zone {
    Before,
    Inside,
    After,
}

/// A drop decided while drawing the tree, applied afterwards.
struct Drop {
    dragged: DocId,
    target: DocId,
    zone: Zone,
}

const ROW_H: f32 = 28.0;

impl NcApp {
    pub fn sidebar(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        // room for the window buttons (traffic lights) and a drag area
        let top = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), if cfg!(target_os = "macos") { 34.0 } else { 6.0 }),
            Sense::click_and_drag(),
        );
        if top.1.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if cfg!(target_os = "macos") {
            let r = top.0;
            let hide = ui.interact(
                egui::Rect::from_center_size(egui::pos2(r.max.x - 16.0, r.center().y + 2.0), Vec2::splat(24.0)),
                Id::unique("hide-sidebar"),
                Sense::click(),
            );
            crate::widgets::a11y(&hide, tr(lang, "toggle_sidebar"));
            ui.painter().text(
                hide.rect.center(),
                Align2::CENTER_CENTER,
                "«",
                FontId::proportional(16.0),
                if hide.hovered() { pal.text } else { pal.text_faint },
            );
            if hide.on_hover_text(format!("{}  ⌘\\", tr(lang, "toggle_sidebar"))).clicked() {
                self.prefs.sidebar_open = false;
            }
        }
        // workspace switcher
        let name = self.ws.as_ref().map(|w| w.name()).unwrap_or_default();
        let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "N".into());
        let ws_btn = ui.add(
            egui::Button::new(RichText::new(format!("{initial}   {name}  ⌄")).size(14.0).color(pal.text))
                .min_size(Vec2::new(ui.available_width(), 30.0)),
        );
        let mut action: Option<&str> = None;
        let mut open_recent = None;
        let mut forget_recent: Option<std::path::PathBuf> = None;
        let mut trash_recent: Option<std::path::PathBuf> = None;
        let current_root = self.ws.as_ref().map(|w| w.root.clone());
        egui::Popup::menu(&ws_btn).id(Id::unique("ws-menu")).show(|ui| {
            ui.set_min_width(240.0);
            if ui.button(tr(lang, "switch_workspace")).clicked() {
                action = Some("open");
            }
            let recent: Vec<_> = self
                .prefs
                .recent
                .iter()
                .filter(|p| p.is_dir() && !crate::app::is_too_broad(p) && Some(*p) != current_root.as_ref())
                .cloned()
                .collect();
            if !recent.is_empty() {
                ui.separator();
                ui.label(RichText::new(tr(lang, "recent")).size(12.0).color(pal.text_secondary));
                for p in recent {
                    ui.horizontal(|ui| {
                        let open = ui
                            .button(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                            .on_hover_text(p.display().to_string());
                        if open.clicked() {
                            open_recent = Some(p.clone());
                        }
                        open.context_menu(|ui| {
                            if ui.button(tr(lang, "forget_workspace")).clicked() {
                                forget_recent = Some(p.clone());
                                ui.close();
                            }
                        });
                        if crate::icons::trash_button(ui, &pal).on_hover_text(tr(lang, "trash_one")).clicked() {
                            trash_recent = Some(p.clone());
                        }
                    });
                }
            }
            ui.separator();
            if ui.button(tr(lang, "export_workspace")).clicked() {
                action = Some("export");
            }
            if ui.button(tr(lang, "close_workspace")).clicked() {
                action = Some("close");
            }
            if ui.button(tr(lang, "delete_workspace")).clicked() {
                action = Some("trash");
            }
        });
        match action {
            Some("open") => self.pick_workspace(),
            Some("export") => self.export_workspace(),
            Some("close") => {
                self.close_workspace();
                return;
            }
            Some("trash") => {
                if let Some(root) = current_root.clone() {
                    self.trash_workspace(&root);
                }
                return;
            }
            _ => {}
        }
        if let Some(p) = forget_recent {
            self.prefs.recent.retain(|r| r != &p);
        }
        if let Some(p) = trash_recent {
            self.trash_workspace(&p);
        }
        if let Some(p) = open_recent {
            self.open_workspace(&p, false);
        }
        ui.add_space(2.0);
        if nav_item(ui, &pal, Icon::Search, tr(lang, "search"), Some("⌘P")).clicked() {
            self.finder = Some(crate::finder::Finder::new(crate::finder::FinderMode::Jump));
        }
        if nav_item(ui, &pal, Icon::General, tr(lang, "settings"), Some("⌘,")).clicked() {
            self.close_windows();
            self.settings_open = true;
        }
        if nav_item(ui, &pal, Icon::Clock, tr(lang, "reminders"), None).clicked() {
            self.close_windows();
            self.reminders_open = true;
        }
        if nav_item(ui, &pal, Icon::Graph, tr(lang, "graph"), None).clicked() {
            self.close_windows();
            self.graph_open = true;
        }
        let import_resp = nav_item(ui, &pal, Icon::Import, tr(lang, "import"), None);
        let mut import_pick = 0u8;
        egui::Popup::menu(&import_resp).show(|ui| {
            if ui.button(tr(lang, "import_file")).clicked() {
                import_pick = 1;
                ui.close();
            }
            if ui.button(tr(lang, "import_folder")).clicked() {
                import_pick = 2;
                ui.close();
            }
        });
        match import_pick {
            1 => self.import_file(),
            2 => self.import_folder(),
            _ => {}
        }
        if nav_item(ui, &pal, Icon::Plus, tr(lang, "new_page"), Some("⌘N")).clicked() {
            self.new_page(None);
        }
        let has_brain = self.ws.as_ref().is_some_and(|w| w.pages.values().any(|p| p.meta.title == "Second cerveau"));
        if !has_brain && nav_item(ui, &pal, Icon::Plus, tr(lang, "second_brain"), None).clicked() {
            self.flush_save();
            let res = self.ws.as_mut().map(notecraft_core::second_brain::create);
            match res {
                Some(Ok(id)) => self.navigate(Target::Doc(DocId::Page(id))),
                Some(Err(e)) => self.notify_error("second brain", &e),
                None => {}
            }
        }
        ui.add_space(10.0);

        let mut drop: Option<Drop> = None;
        let mut nav: Option<DocId> = None;
        let mut nav_fav = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 40.0).show(
            ui,
            |ui| {
                let favs = self.ws.as_ref().map(|w| w.favorites()).unwrap_or_default();
                if !favs.is_empty() {
                    section(ui, &pal, tr(lang, "favorites"));
                    for f in favs {
                        let r = self.tree_row(ui, &f, 0, "fav", &mut drop, false);
                        if r {
                            nav = Some(f.doc.clone());
                            nav_fav = true;
                        }
                    }
                    ui.add_space(10.0);
                }
                let (_, add_clicked) = section_with_add(ui, &pal, tr(lang, "pages"));
                if add_clicked {
                    self.new_page(None);
                }
                let top = self.ws.as_ref().map(|w| w.children(None)).unwrap_or_default();
                if top.is_empty() {
                    ui.label(RichText::new(tr(lang, "no_pages")).size(13.0).color(pal.text_faint));
                }
                let troot = self.ws.as_ref().and_then(crate::app::templates_root);
                for item in top {
                    // the templates have a section of their own
                    if troot.as_deref() == Some(item.doc.id()) {
                        continue;
                    }
                    self.tree(ui, &item, 0, &mut drop, &mut nav);
                }
                ui.add_space(10.0);
                let (_, add_tpl) = section_with_add(ui, &pal, tr(lang, "templates"));
                if add_tpl {
                    self.ensure_templates();
                    if let Some(root) = self.ws.as_ref().and_then(crate::app::templates_root) {
                        self.new_page(Some(root));
                    }
                }
                if let Some(root) = troot {
                    let group = self.ws.as_ref().and_then(crate::app::notes_group);
                    let kids = self.ws.as_ref().map(|w| w.children(Some(&root))).unwrap_or_default();
                    for k in kids.iter().filter(|k| Some(k.doc.id()) != group.as_deref()) {
                        self.tree(ui, k, 0, &mut drop, &mut nav);
                    }
                    if let Some(g) = group {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            ui.label(RichText::new(tr(lang, "templates_notes")).size(12.0).color(pal.text_faint));
                        });
                        let notes = self.ws.as_ref().map(|w| w.children(Some(&g))).unwrap_or_default();
                        for k in notes {
                            self.tree(ui, &k, 0, &mut drop, &mut nav);
                        }
                    }
                } else {
                    ui.label(RichText::new(tr(lang, "templates_none")).size(12.5).color(pal.text_faint));
                }
            },
        );
        // bottom: trash
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            if nav_item(ui, &pal, Icon::Trash, tr(lang, "trash"), None).clicked() {
                self.close_windows();
                self.trash_open = true;
            }
        });

        if let Some(d) = nav {
            self.fav_nav_doc = if nav_fav { Some(d.clone()) } else { None };
            if ui.ctx().input(|i| i.modifiers.command) {
                self.open_in_new_tab(Target::Doc(d));
            } else {
                self.navigate(Target::Doc(d));
            }
        }
        if let Some(d) = drop {
            self.apply_drop(d);
        }
        // a release anywhere ends a sidebar drag
        if ui.ctx().input(|i| i.pointer.any_released()) && egui::DragAndDrop::has_payload_of_type::<SideDrag>(ui.ctx())
        {
            egui::DragAndDrop::clear_payload(ui.ctx());
        }
    }

    fn tree(&mut self, ui: &mut Ui, item: &TreeItem, depth: usize, drop: &mut Option<Drop>, nav: &mut Option<DocId>) {
        let expanded = self.expanded.contains(&item.doc.key());
        if self.tree_row(ui, item, depth, "tree", drop, true) {
            *nav = Some(item.doc.clone());
        }
        if expanded {
            let kids = self.ws.as_ref().map(|w| w.children(Some(item.doc.id()))).unwrap_or_default();
            if kids.is_empty() && matches!(item.doc, DocId::Page(_)) {
                let pal = self.pal;
                ui.horizontal(|ui| {
                    ui.add_space(30.0 + (depth + 1) as f32 * 14.0);
                    ui.label(RichText::new(tr(self.prefs.lang, "no_pages")).size(12.5).color(pal.text_faint));
                });
            }
            for k in kids {
                self.tree(ui, &k, depth + 1, drop, nav);
            }
        }
    }

    /// One sidebar row. Returns true when clicked.
    fn tree_row(
        &mut self,
        ui: &mut Ui,
        item: &TreeItem,
        depth: usize,
        salt: &str,
        drop: &mut Option<Drop>,
        draggable: bool,
    ) -> bool {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, ROW_H), Sense::click_and_drag());
        let id_key = format!("{salt}:{}", item.doc.key());
        let current = self.current.as_ref() == Some(&Target::Doc(item.doc.clone()))
            && !(salt == "tree" && self.fav_nav_doc.as_ref() == Some(&item.doc));
        let hovered = resp.hovered() || ui.rect_contains_pointer(rect);
        if current {
            ui.painter().rect_filled(rect, CornerRadius::same(6), pal.active);
        } else if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), pal.hover);
        }
        let x = rect.min.x + 4.0 + depth as f32 * 14.0;
        // expand arrow
        let arrow = egui::Rect::from_center_size(egui::pos2(x + 10.0, rect.center().y), Vec2::splat(20.0));
        let expanded = self.expanded.contains(&item.doc.key());
        let ar = ui.interact(arrow, Id::unique(("arrow", &id_key)), Sense::click());
        crate::widgets::a11y(&ar, tr(lang, if expanded { "a11y_collapse" } else { "a11y_expand" }));
        if ar.hovered() {
            ui.painter().rect_filled(arrow, CornerRadius::same(4), pal.active);
        }
        let icon = if item.icon.is_empty() {
            if matches!(item.doc, DocId::Database(_)) {
                "🗂".to_string()
            } else {
                "📄".to_string()
            }
        } else {
            item.icon.clone()
        };
        if hovered && (item.has_children || matches!(item.doc, DocId::Page(_))) {
            ui.painter().text(
                arrow.center(),
                Align2::CENTER_CENTER,
                if expanded { "▾" } else { "▸" },
                FontId::proportional(13.0),
                pal.text_secondary,
            );
        } else {
            ui.painter().text(arrow.center(), Align2::CENTER_CENTER, icon, FontId::proportional(14.0), pal.text);
        }
        if ar.clicked() {
            if expanded {
                self.expanded.remove(&item.doc.key());
            } else {
                self.expanded.insert(item.doc.key());
            }
        }
        let title = if item.title.is_empty() { tr(lang, "untitled").to_string() } else { item.title.clone() };
        resp.widget_info(|| egui::WidgetInfo::selected(egui::accesskit::Role::TreeItem, true, current, &title));
        let right_pad = if hovered { 52.0 } else { 8.0 };
        let text_rect =
            egui::Rect::from_min_max(egui::pos2(x + 26.0, rect.min.y), egui::pos2(rect.max.x - right_pad, rect.max.y));
        let galley = ui.painter().layout(
            title,
            FontId::proportional(14.0),
            if current { pal.text } else { pal.text_secondary.lerp_to_gamma(pal.text, 0.6) },
            f32::INFINITY,
        );
        let painter = ui.painter().with_clip_rect(text_rect);
        painter.galley(egui::pos2(text_rect.min.x, rect.center().y - galley.size().y / 2.0), galley, pal.text);

        let mut clicked = resp.clicked() && !ar.clicked();
        if !hovered {
            // Small status icons on the right; they stack, so a page can be live and locked at once.
            let locked = self.ws.as_ref().is_some_and(|ws| ws.is_locked(&item.doc));
            let live = match (&item.doc, self.ws.as_ref()) {
                (DocId::Page(pid), Some(ws)) => self.pubs.get(&ws.root.display().to_string(), pid).is_some(),
                _ => false,
            };
            let mut x = rect.max.x - 14.0;
            if locked {
                ui.painter().text(
                    egui::pos2(x, rect.center().y),
                    Align2::CENTER_CENTER,
                    "🔒",
                    FontId::proportional(11.0),
                    pal.text_faint,
                );
                x -= 16.0;
            }
            if live {
                ui.painter().circle_filled(egui::pos2(x, rect.center().y), 3.5, Color32::from_rgb(46, 160, 100));
            }
        }
        // hover actions: ⋯ menu and + (add inside)
        if hovered {
            let plus = egui::Rect::from_center_size(egui::pos2(rect.max.x - 14.0, rect.center().y), Vec2::splat(20.0));
            let more = egui::Rect::from_center_size(egui::pos2(rect.max.x - 36.0, rect.center().y), Vec2::splat(20.0));
            let pr = ui.interact(plus, Id::unique(("plus", &id_key)), Sense::click());
            let mr = ui.interact(more, Id::unique(("more", &id_key)), Sense::click());
            crate::widgets::a11y(&pr, tr(lang, "add_subpage"));
            crate::widgets::a11y(&mr, tr(lang, "a11y_more"));
            for (r, g) in [(&pr, "+"), (&mr, "⋯")] {
                if r.hovered() {
                    ui.painter().rect_filled(r.rect, CornerRadius::same(4), pal.active);
                }
                ui.painter().text(
                    r.rect.center(),
                    Align2::CENTER_CENTER,
                    g,
                    FontId::proportional(15.0),
                    pal.text_secondary,
                );
            }
            if pr.clicked() {
                clicked = false;
                if let DocId::Page(p) = &item.doc {
                    self.new_page(Some(p.clone()));
                } else if let DocId::Database(db) = &item.doc {
                    if let Some(ws) = self.ws.as_mut() {
                        if let Ok(r) = ws.add_row(db, None, "") {
                            self.navigate(Target::Doc(DocId::Row(db.clone(), r)));
                        }
                    }
                }
            }
            pr.on_hover_text(tr(lang, "add_subpage"));
            if mr.clicked() {
                clicked = false;
                egui::Popup::open_id(ui.ctx(), Id::unique(("row-menu", &id_key)));
            }
        }
        let menu_id = Id::unique(("row-menu", &id_key));
        let anchor = egui::PopupAnchor::Position(egui::pos2(rect.max.x - 36.0, rect.max.y));
        let mut action: Option<&str> = None;
        egui::Popup::new(menu_id, ui.ctx().clone(), anchor, ui.layer_id())
            .kind(egui::PopupKind::Menu)
            .open_memory(None)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
            .show(|ui| {
                ui.set_min_width(220.0);
                let fav = self.ws.as_ref().is_some_and(|w| w.is_favorite(&item.doc));
                if ui
                    .button(if fav { tr(lang, "remove_from_favorites") } else { tr(lang, "add_to_favorites") })
                    .clicked()
                {
                    action = Some("fav");
                }
                if ui.button(tr(lang, "rename")).clicked() {
                    action = Some("rename");
                }
                if ui.button(tr(lang, "move_page_to")).clicked() {
                    action = Some("move");
                }
                if matches!(item.doc, DocId::Page(_)) && ui.button(tr(lang, "duplicate")).clicked() {
                    action = Some("dup");
                }
                if ui.button(tr(lang, "export_markdown")).clicked() {
                    action = Some("export");
                }
                ui.separator();
                if ui.button(RichText::new(tr(lang, "move_to_trash")).color(pal.danger)).clicked() {
                    action = Some("trash");
                }
            });
        match action {
            Some("fav") => {
                if let Some(ws) = self.ws.as_mut() {
                    let f = ws.is_favorite(&item.doc);
                    let _ = ws.set_favorite(&item.doc, !f);
                }
            }
            Some("dup") => {
                if let (Some(ws), DocId::Page(p)) = (self.ws.as_mut(), &item.doc) {
                    let _ = ws.duplicate_page(p);
                }
            }
            Some("rename") => {
                self.navigate(Target::Doc(item.doc.clone()));
                self.focus_title = true;
            }
            Some("move") => {
                self.move_query.clear();
                self.move_doc = Some(item.doc.clone());
            }
            Some("export") => self.export_doc(&item.doc.clone()),
            Some("trash") => self.trash_doc(&item.doc.clone()),
            _ => {}
        }
        resp.context_menu(|ui| {
            if ui.button(tr(lang, "rename")).clicked() {
                self.navigate(Target::Doc(item.doc.clone()));
                self.focus_title = true;
                ui.close();
            }
            if ui.button(tr(lang, "move_page_to")).clicked() {
                self.move_query.clear();
                self.move_doc = Some(item.doc.clone());
                ui.close();
            }
            if ui.button(tr(lang, "add_subpage")).clicked() {
                if let DocId::Page(p) = &item.doc {
                    self.new_page(Some(p.clone()));
                }
                ui.close();
            }
            if ui.button(RichText::new(tr(lang, "move_to_trash")).color(pal.danger)).clicked() {
                self.trash_doc(&item.doc.clone());
                ui.close();
            }
        });

        // drag and drop
        if draggable {
            if resp.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), SideDrag(item.doc.clone()));
            }
            if let Some(payload) = egui::DragAndDrop::payload::<SideDrag>(ui.ctx()) {
                if let Some(p) = ui.ctx().pointer_interact_pos() {
                    if rect.contains(p) && payload.0 != item.doc {
                        let rel = (p.y - rect.min.y) / rect.height();
                        let zone = if rel < 0.28 {
                            Zone::Before
                        } else if rel > 0.72 || matches!(item.doc, DocId::Database(_)) {
                            Zone::After
                        } else {
                            Zone::Inside
                        };
                        let stroke = Stroke::new(2.0, pal.drop);
                        match zone {
                            Zone::Before => {
                                ui.painter().hline(rect.x_range(), rect.min.y, stroke);
                            }
                            Zone::After => {
                                ui.painter().hline(rect.x_range(), rect.max.y, stroke);
                            }
                            Zone::Inside => {
                                ui.painter().rect_stroke(rect, CornerRadius::same(6), stroke, egui::StrokeKind::Inside);
                            }
                        }
                        if ui.ctx().input(|i| i.pointer.any_released()) {
                            *drop = Some(Drop { dragged: payload.0.clone(), target: item.doc.clone(), zone });
                        }
                    }
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
        }
        clicked
    }

    /// "Move to…": pick the page this one goes under (or the top level).
    pub fn move_dialog(&mut self, ctx: &egui::Context) {
        use crate::widgets::menu_item;
        let Some(doc) = self.move_doc.clone() else { return };
        let pal = self.pal;
        let lang = self.prefs.lang;
        let Some(ws) = &self.ws else {
            self.move_doc = None;
            return;
        };
        let mut banned: Vec<String> = ws.descendants(&doc).iter().map(|d| d.id().to_string()).collect();
        banned.push(doc.id().to_string());
        // every page, in tree order, with its depth
        let mut all: Vec<(String, String, String, usize)> = Vec::new();
        let mut stack: Vec<(Option<String>, usize)> = vec![(None, 0)];
        let mut order: Vec<(Option<String>, usize)> = Vec::new();
        while let Some((parent, depth)) = stack.pop() {
            order.push((parent.clone(), depth));
            let mut kids = ws.children(parent.as_deref());
            kids.reverse();
            for k in kids {
                if let DocId::Page(id) = &k.doc {
                    if !banned.contains(id) {
                        all.push((id.clone(), k.title.clone(), k.icon.clone(), depth));
                        stack.push((Some(id.clone()), depth + 1));
                    }
                }
            }
        }
        let _ = order;
        let q = self.move_query.trim().to_lowercase();
        let mut chosen: Option<Option<String>> = None;
        let modal = egui::Modal::new(Id::unique("move-dialog")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(RichText::new(tr(lang, "move_page_to")).size(15.0).color(pal.text));
            ui.add_space(8.0);
            let te = ui.add(
                egui::TextEdit::singleline(&mut self.move_query)
                    .hint_text(tr(lang, "move_search"))
                    .desired_width(f32::INFINITY),
            );
            te.request_focus();
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                if q.is_empty() && menu_item(ui, &pal, tr(lang, "move_root"), false).clicked() {
                    chosen = Some(None);
                }
                let mut shown = 0;
                for (id, title, icon, depth) in &all {
                    if !q.is_empty() && !title.to_lowercase().contains(&q) {
                        continue;
                    }
                    shown += 1;
                    if shown > 60 {
                        break;
                    }
                    let pad = if q.is_empty() { "    ".repeat((*depth).min(3)) } else { String::new() };
                    let icon = if icon.is_empty() { "📄" } else { icon.as_str() };
                    if menu_item(ui, &pal, &format!("{pad}{icon}  {title}"), false).clicked() {
                        chosen = Some(Some(id.clone()));
                    }
                }
            });
        });
        let mut done = modal.should_close();
        if let Some(target) = chosen {
            if let Some(ws) = self.ws.as_mut() {
                let n = ws.children(target.as_deref()).len();
                if let Some(t) = &target {
                    self.expanded.insert(DocId::Page(t.clone()).key());
                }
                if let Err(e) = ws.move_to(&doc, target.as_deref(), n) {
                    self.notify(e.to_string());
                }
            }
            done = true;
        }
        if done {
            self.move_doc = None;
            self.move_query.clear();
        }
    }

    fn apply_drop(&mut self, d: Drop) {
        let Some(ws) = self.ws.as_mut() else { return };
        let (parent, index) = match d.zone {
            Zone::Inside => {
                let n = ws.children(Some(d.target.id())).len();
                self.expanded.insert(d.target.key());
                (Some(d.target.id().to_string()), n)
            }
            Zone::Before | Zone::After => {
                let parent = ws.parent(&d.target).map(|p| p.id().to_string());
                let sibs: Vec<DocId> =
                    ws.children(parent.as_deref()).into_iter().map(|t| t.doc).filter(|x| *x != d.dragged).collect();
                let pos = sibs.iter().position(|x| *x == d.target).unwrap_or(sibs.len());
                (parent, if d.zone == Zone::After { pos + 1 } else { pos })
            }
        };
        if let Err(e) = ws.move_to(&d.dragged, parent.as_deref(), index) {
            self.notify(e.to_string());
        }
    }
}

fn nav_item(ui: &mut Ui, pal: &crate::theme::Palette, icon: Icon, label: &str, hint: Option<&str>) -> egui::Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, ROW_H), Sense::click());
    crate::widgets::a11y(&resp, label);
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), pal.hover);
    }
    let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.min.x + 14.0, rect.center().y), Vec2::splat(16.0));
    crate::icons::paint(ui.painter(), icon_rect, icon, pal.text_secondary, pal.bg);
    ui.painter().text(
        egui::pos2(rect.min.x + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        pal.text_secondary.lerp_to_gamma(pal.text, 0.5),
    );
    if let Some(h) = hint {
        if resp.hovered() {
            ui.painter().text(
                egui::pos2(rect.max.x - 8.0, rect.center().y),
                Align2::RIGHT_CENTER,
                h,
                FontId::proportional(12.0),
                pal.text_faint,
            );
        }
    }
    resp
}

fn section(ui: &mut Ui, pal: &crate::theme::Palette, label: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(label).size(12.0).color(pal.text_faint));
    ui.add_space(2.0);
}

fn section_with_add(ui: &mut Ui, pal: &crate::theme::Palette, label: &str) -> (egui::Response, bool) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 22.0), Sense::hover());
    ui.painter().text(
        egui::pos2(rect.min.x + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        pal.text_faint,
    );
    let mut clicked = false;
    if ui.rect_contains_pointer(rect) {
        let plus = egui::Rect::from_center_size(egui::pos2(rect.max.x - 14.0, rect.center().y), Vec2::splat(20.0));
        let pr = ui.interact(plus, Id::unique(("section-add", label)), Sense::click());
        crate::widgets::a11y(&pr, label);
        if pr.hovered() {
            ui.painter().rect_filled(plus, CornerRadius::same(4), pal.hover);
        }
        ui.painter().text(plus.center(), Align2::CENTER_CENTER, "+", FontId::proportional(15.0), pal.text_secondary);
        clicked = pr.clicked();
    }
    (resp, clicked)
}
