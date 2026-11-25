//! Global shortcuts, top bar, trash window, welcome screen, toasts and drag and drop (split out of mod.rs, unchanged).

use super::*;

impl NcApp {
    // ------------------------------------------------------------- shortcuts

    /// One window at a time: opening one closes the others, so none stays behind another.
    pub fn close_windows(&mut self) {
        self.settings_open = false;
        self.reminders_open = false;
        self.graph_open = false;
        self.trash_open = false;
    }

    pub(super) fn global_keys(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let consume = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if self.ws.is_none() {
            return;
        }
        // before ⌘P: egui lets extra modifiers through, so ⌥⌘P would otherwise open the finder
        if consume(Modifiers::COMMAND | Modifiers::ALT, Key::P) {
            self.start_presentation(ctx);
        }
        if consume(cmd, Key::P) || (!self.editor_has_selection(ctx) && consume(cmd, Key::K)) {
            self.finder = Some(Finder::new(FinderMode::Jump));
        }
        if consume(cmd, Key::Backslash) {
            self.prefs.sidebar_open = !self.prefs.sidebar_open;
        }
        if consume(cmd, Key::N) {
            self.new_page(None);
        }
        if consume(cmd, Key::T) {
            self.new_tab();
        }
        if self.tabs.len() > 1 && consume(cmd, Key::W) {
            let i = self.active_tab;
            self.close_tab(i);
        }
        if consume(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab) {
            self.next_tab(-1);
        } else if consume(Modifiers::CTRL, Key::Tab) {
            self.next_tab(1);
        }
        if consume(cmd, Key::OpenBracket) {
            self.go_back();
        }
        if consume(cmd, Key::CloseBracket) {
            self.go_forward();
        }
        if consume(cmd, Key::J) {
            if self.ai.open {
                self.ai.open = false;
            } else {
                self.open_ai();
            }
        }
        if consume(cmd, Key::Comma) {
            self.close_windows();
            self.settings_open = true;
        }
        // View menu zoom: Cmd+= / Cmd+Plus, Cmd+-, Cmd+0
        let zoom_in = consume(cmd, Key::Equals)
            || consume(cmd, Key::Plus)
            || consume(cmd_shift, Key::Equals)
            || consume(cmd_shift, Key::Plus);
        let zoom_out = consume(cmd, Key::Minus);
        let reset = consume(cmd, Key::Num0);
        if zoom_in {
            self.prefs.zoom = theme::clamp_zoom(self.prefs.zoom + theme::ZOOM_STEP);
        } else if zoom_out {
            self.prefs.zoom = theme::clamp_zoom(self.prefs.zoom - theme::ZOOM_STEP);
        } else if reset {
            self.prefs.zoom = theme::ZOOM_DEFAULT;
        }
        if consume(cmd_shift, Key::L) {
            self.prefs.theme = if self.pal.dark { ThemeChoice::Light } else { ThemeChoice::Dark };
        }
        let redo = consume(cmd_shift, Key::Z) || consume(cmd, Key::Y);
        let undo = !redo && consume(cmd, Key::Z);
        if undo || redo {
            if let Some(t) = self.current.clone() {
                let key = t.key();
                if let Some(ws) = self.ws.as_mut() {
                    if let Some(blocks) = Self::blocks_mut(ws, &t) {
                        let done = if undo { self.editor.undo(&key, blocks) } else { self.editor.redo(&key, blocks) };
                        if done {
                            self.mark_dirty();
                        }
                    }
                }
            }
        }
    }

    pub(super) fn editor_has_selection(&self, ctx: &egui::Context) -> bool {
        let Some(id) = ctx.memory(|m| m.focused()) else { return false };
        egui::text_edit::TextEditState::load(ctx, id)
            .and_then(|s| s.cursor.char_range())
            .is_some_and(|r| r.primary != r.secondary)
    }

    // -------------------------------------------------------------- top bar

    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let drag = ui.interact(ui.max_rect(), Id::unique("topbar-drag"), egui::Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }
        ui.horizontal_centered(|ui| {
            if !self.prefs.sidebar_open {
                // keep clear of the window buttons (traffic-light buttons)
                let pad = if cfg!(target_os = "macos") {
                    theme::window_controls_padding_mac(self.prefs.zoom) / self.prefs.zoom
                } else {
                    0.0
                };
                ui.add_space((pad - 10.0).max(0.0));
                let side = widgets::subtle_button(ui, RichText::new("☰").size(16.0).color(pal.text_secondary), &pal)
                    .on_hover_text(self.t("toggle_sidebar"));
                widgets::a11y(&side, self.t("toggle_sidebar"));
                if side.clicked() {
                    self.prefs.sidebar_open = true;
                }
            }
            let back = ui.add_enabled(
                !self.back.is_empty(),
                egui::Button::new(RichText::new("‹").size(20.0).color(pal.text_secondary)),
            );
            widgets::a11y(&back, self.t("back"));
            if back.on_hover_text(format!("{}  ⌘[", self.t("back"))).clicked() {
                self.go_back();
            }
            let fwd = ui.add_enabled(
                !self.forward.is_empty(),
                egui::Button::new(RichText::new("›").size(20.0).color(pal.text_secondary)),
            );
            widgets::a11y(&fwd, self.t("forward"));
            if fwd.on_hover_text(format!("{}  ⌘]", self.t("forward"))).clicked() {
                self.go_forward();
            }
            // breadcrumbs
            let crumbs: Vec<(Target, String, String)> = match (&self.ws, &self.current) {
                (Some(ws), Some(Target::Doc(d))) => {
                    ws.breadcrumbs(d).into_iter().map(|c| (Target::Doc(c.clone()), ws.title(&c), ws.icon(&c))).collect()
                }
                (Some(ws), Some(Target::Template(db, t))) => {
                    let mut v: Vec<(Target, String, String)> = ws
                        .breadcrumbs(&DocId::Database(db.clone()))
                        .into_iter()
                        .map(|c| (Target::Doc(c.clone()), ws.title(&c), ws.icon(&c)))
                        .collect();
                    let title = ws
                        .dbs
                        .get(db)
                        .and_then(|d| d.templates.get(t))
                        .map(|r| r.meta.title.clone())
                        .unwrap_or_default();
                    v.push((Target::Template(db.clone(), t.clone()), title, "📋".into()));
                    v
                }
                _ => vec![],
            };
            let n = crumbs.len();
            let mut go = None;
            for (k, (target, title, icon)) in crumbs.into_iter().enumerate() {
                if k > 0 {
                    ui.label(RichText::new("/").color(pal.text_faint));
                }
                let title = if title.is_empty() { self.t("untitled").to_string() } else { title };
                let text = if icon.is_empty() { title } else { format!("{icon} {title}") };
                let short: String =
                    if text.chars().count() > 28 { text.chars().take(26).collect::<String>() + "…" } else { text };
                let color = if k + 1 == n { pal.text } else { pal.text_secondary };
                if widgets::subtle_button(ui, RichText::new(short).size(13.5).color(color), &pal).clicked() {
                    go = Some(target);
                }
            }
            if let Some(t) = go {
                self.navigate(t);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                self.ai_button(ui);
                let Some(Target::Doc(doc)) = self.current.clone() else { return };
                let more = widgets::subtle_button(ui, RichText::new("•••").size(13.0).color(pal.text_secondary), &pal);
                widgets::a11y(&more, self.t("a11y_more"));
                let mut export = false;
                let mut history = false;
                let mut present = false;
                let mut remind = false;
                let mut export_as: Option<notecraft_core::render::Format> = None;
                let mut trash = false;
                let mut duplicate = false;
                let mut toggle_lock = false;
                let mut encrypt: Option<bool> = None;
                let mut lock_vault = false;
                let mut look_change: Option<(String, bool, bool)> = None;
                let mut archive = false;
                let mut to_top = false;
                let (look, words) = match (&doc, self.ws.as_ref()) {
                    (DocId::Page(id), Some(w)) => (
                        w.pages.get(id).map(|p| (p.meta.font.clone(), p.meta.small_text, p.meta.full_width)),
                        w.word_count(&doc),
                    ),
                    _ => (None, 0),
                };
                let (encrypted, vault_open) = self
                    .ws
                    .as_ref()
                    .map(|w| (matches!(&doc, DocId::Page(id) if w.is_encrypted(id)), w.vault_is_open()))
                    .unwrap_or((false, false));
                let (own_lock, any_lock) =
                    self.ws.as_ref().map(|w| (w.has_own_lock(&doc), w.is_locked(&doc))).unwrap_or((false, false));
                let mut pub_act: Option<PubAct> = None;
                let pub_info = match (&doc, self.ws.as_ref()) {
                    (DocId::Page(pid), Some(w)) => {
                        let key = w.root.display().to_string();
                        let e = self.pubs.get(&key, pid).cloned();
                        let stale = e.as_ref().is_some_and(|e| crate::publish::is_stale(w, e));
                        let kids = w.children(Some(pid)).iter().any(|c| matches!(c.doc, DocId::Page(_)));
                        Some((e, stale, kids))
                    }
                    _ => None,
                };
                egui::Popup::menu(&more).id(Id::unique("page-more")).show(|ui| {
                    ui.set_min_width(220.0);
                    if let Some((entry, stale, kids)) = &pub_info {
                        let label: egui::WidgetText = if entry.is_some() {
                            RichText::new(format!("{}  ● {}", self.t("publish_menu"), self.t("pub_live")))
                                .color(Color32::from_rgb(46, 160, 100))
                                .into()
                        } else {
                            self.t("publish_menu").into()
                        };
                        ui.menu_button(label, |ui| {
                            if ui.button(self.t("pub_preview")).clicked() {
                                pub_act = Some(PubAct::Preview);
                            }
                            match entry {
                                None => {
                                    if ui.button(self.t("pub_page")).clicked() {
                                        pub_act = Some(PubAct::Publish(false));
                                    }
                                    if *kids && ui.button(self.t("pub_section")).clicked() {
                                        pub_act = Some(PubAct::Publish(true));
                                    }
                                }
                                Some(_) => {
                                    ui.separator();
                                    let update = if *stale {
                                        format!("{} •", self.t("pub_update"))
                                    } else {
                                        self.t("pub_update").to_string()
                                    };
                                    if ui.button(update).clicked() {
                                        pub_act = Some(PubAct::Update);
                                    }
                                    if ui.button(self.t("pub_copy")).clicked() {
                                        pub_act = Some(PubAct::CopyLink);
                                    }
                                    if ui.button(self.t("pub_open")).clicked() {
                                        pub_act = Some(PubAct::Open);
                                    }
                                    if ui.button(self.t("pub_unpublish")).clicked() {
                                        pub_act = Some(PubAct::Unpublish);
                                    }
                                }
                            }
                        });
                    }
                    ui.menu_button(self.t("export_menu"), |ui| {
                        if ui.button(self.t("export_markdown")).clicked() {
                            export = true;
                        }
                        if matches!(doc, DocId::Page(_)) {
                            for (key, f) in [
                                ("export_html", notecraft_core::render::Format::Html),
                                ("export_pdf", notecraft_core::render::Format::Pdf),
                                ("export_docx", notecraft_core::render::Format::Docx),
                            ] {
                                if ui.button(self.t(key)).clicked() {
                                    export_as = Some(f);
                                }
                            }
                        }
                    });
                    if let DocId::Page(pid) = &doc {
                        let when = self
                            .ws
                            .as_ref()
                            .and_then(|w| w.pages.get(pid))
                            .map(|p| p.meta.reminder.clone())
                            .unwrap_or_default();
                        let label = if when.is_empty() { self.t("remind_me").to_string() } else { when.clone() };
                        if ui.button(label).clicked() {
                            remind = true;
                        }
                    }
                    if matches!(doc, DocId::Page(_)) && ui.button(format!("{}  ⌥⌘P", self.t("present"))).clicked() {
                        present = true;
                    }
                    if ui.button(self.t("history")).clicked() {
                        history = true;
                    }
                    if matches!(doc, DocId::Page(_)) && ui.button(self.t("duplicate")).clicked() {
                        duplicate = true;
                    }
                    ui.menu_button(self.t("protect_menu"), |ui| {
                        if any_lock && !own_lock {
                            ui.add_enabled(false, egui::Button::new(self.t("locked_by_parent").to_string()));
                        } else {
                            let label =
                                if own_lock { self.t("unlock").to_string() } else { self.t("lock").to_string() };
                            if ui.button(label).clicked() {
                                toggle_lock = true;
                            }
                        }
                        if matches!(doc, DocId::Page(_)) {
                            let label = if encrypted { "vault_remove" } else { "vault_encrypt_menu" };
                            if ui.button(self.t(label)).clicked() {
                                encrypt = Some(!encrypted);
                            }
                        }
                        if vault_open && ui.button(self.t("vault_lock_now")).clicked() {
                            lock_vault = true;
                        }
                    });
                    if let Some((font, small, wide)) = look.clone() {
                        ui.menu_button(self.t("look_menu"), |ui| {
                            ui.horizontal(|ui| {
                                for (key, value) in
                                    [("font_default", ""), ("font_serif", "serif"), ("font_mono", "mono")]
                                {
                                    let text = RichText::new(self.t(key)).family(match value {
                                        "serif" => egui::FontFamily::Name("serif".into()),
                                        "mono" => egui::FontFamily::Monospace,
                                        _ => egui::FontFamily::Proportional,
                                    });
                                    if ui.selectable_label(font == value, text).clicked() && font != value {
                                        look_change = Some((value.to_string(), small, wide));
                                    }
                                }
                            });
                            if ui.checkbox(&mut { small }, self.t("small_text")).clicked() {
                                look_change = Some((font.clone(), !small, wide));
                            }
                            if ui.checkbox(&mut { wide }, self.t("full_width")).clicked() {
                                look_change = Some((font.clone(), small, !wide));
                            }
                        });
                        ui.menu_button(self.t("organise_menu"), |ui| {
                            if ui.button(self.t("move_top")).clicked() {
                                to_top = true;
                                ui.close();
                            }
                            if ui.button(self.t("archive")).clicked() {
                                archive = true;
                            }
                        });
                        ui.separator();
                    }
                    if let Some(ws) = &self.ws {
                        if ui.button("Copy link  ⌘L").clicked() {
                            ui.ctx().copy_text(format!("[[{}]]", ws.title(&doc)));
                        }
                    }
                    if matches!(doc, DocId::Page(_)) {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!("{words} {}", self.t("words"))).size(12.0).color(pal.text_faint),
                        );
                    }
                    ui.separator();
                    let trash_button = egui::Button::new(RichText::new(self.t("move_to_trash")).color(pal.danger));
                    if ui.add_enabled(!any_lock, trash_button).clicked() {
                        trash = true;
                    }
                });
                if let (Some((font, small, wide)), DocId::Page(id)) = (look_change, &doc) {
                    let res = self.ws.as_mut().map(|w| w.set_page_look(id, &font, small, wide));
                    if let Some(Err(e)) = res {
                        self.notify(e.to_string());
                    }
                }
                if let (true, DocId::Page(id)) = (to_top, &doc) {
                    self.flush_save();
                    if let Some(Err(e)) = self.ws.as_mut().map(|w| w.move_to_top(id)) {
                        self.notify(e.to_string());
                    }
                }
                if let (true, DocId::Page(id)) = (archive, &doc) {
                    self.flush_save();
                    let res = self.ws.as_mut().map(|w| w.set_archived(id, true));
                    if let Some(Err(e)) = res {
                        self.notify(e.to_string());
                    }
                    self.validate_current();
                    let msg = self.t("archived_done").to_string();
                    self.notify(msg);
                }
                if let Some(f) = export_as {
                    self.export_page_format(&doc, f);
                }
                if let (Some(act), DocId::Page(pid)) = (pub_act, &doc) {
                    let ctx = ui.ctx().clone();
                    self.pub_act(&ctx, pid, act);
                }
                if remind {
                    if let DocId::Page(pid) = &doc {
                        self.edit_reminder(pid);
                    }
                }
                if present {
                    let ctx = ui.ctx().clone();
                    self.start_presentation(&ctx);
                }
                if history {
                    self.open_history(&doc);
                }
                if let (Some(on), DocId::Page(id)) = (encrypt, &doc) {
                    if on {
                        self.ask_encrypt(id);
                    } else {
                        self.remove_encryption(id);
                    }
                }
                if lock_vault {
                    self.lock_vault_now();
                }
                if toggle_lock {
                    let res = self.ws.as_mut().map(|w| w.set_locked(&doc, !own_lock));
                    if let Some(Err(e)) = res {
                        self.notify(e.to_string());
                    }
                }
                if any_lock {
                    ui.label(RichText::new("🔒").size(14.0).color(pal.text_secondary))
                        .on_hover_text(self.t("locked_hint"));
                }
                if export {
                    self.export_doc(&doc);
                }
                if duplicate {
                    if let (Some(ws), DocId::Page(id)) = (self.ws.as_mut(), &doc) {
                        if let Ok(n) = ws.duplicate_page(id) {
                            self.navigate(Target::Doc(DocId::Page(n)));
                        }
                    }
                }
                if trash {
                    self.trash_doc(&doc);
                }
                if !matches!(doc, DocId::Row(..)) {
                    let fav = self.ws.as_ref().is_some_and(|w| w.is_favorite(&doc));
                    let star = if fav {
                        RichText::new("★").size(16.0).color(Color32::from_rgb(0xF2, 0xB1, 0x3D))
                    } else {
                        RichText::new("☆").size(16.0).color(pal.text_secondary)
                    };
                    let r = widgets::subtle_button(ui, star, &pal).on_hover_text(if fav {
                        self.t("remove_from_favorites")
                    } else {
                        self.t("add_to_favorites")
                    });
                    widgets::a11y(&r, if fav { self.t("remove_from_favorites") } else { self.t("add_to_favorites") });
                    if r.clicked() {
                        if let Some(ws) = self.ws.as_mut() {
                            let _ = ws.set_favorite(&doc, !fav);
                        }
                    }
                }
                if let Some(ws) = &self.ws {
                    let updated = match &doc {
                        DocId::Page(id) => ws.pages.get(id).map(|p| p.meta.updated.clone()),
                        DocId::Database(id) => ws.dbs.get(id).map(|d| d.schema.updated.clone()),
                        DocId::Row(db, r) => ws.dbs.get(db).and_then(|d| d.rows.get(r)).map(|r| r.meta.updated.clone()),
                    }
                    .unwrap_or_default();
                    let rel = widgets::relative_time(&updated, self.prefs.lang);
                    if !rel.is_empty() {
                        ui.label(RichText::new(format!("{} {rel}", self.t("edited"))).size(12.5).color(pal.text_faint));
                    }
                }
            });
        });
    }

    // --------------------------------------------------------------- dialogs

    pub(super) fn trash_ui(&mut self, ctx: &egui::Context) {
        if !self.trash_open {
            return;
        }
        let pal = self.pal;
        let lang = self.prefs.lang;
        let mut close = false;
        let mut restore = None;
        let mut delete = None;
        let mut empty = false;
        let items: Vec<(DocId, String, String, String)> = self
            .ws
            .as_ref()
            .map(|ws| ws.trash().into_iter().map(|(d, when)| (d.clone(), ws.title(&d), ws.icon(&d), when)).collect())
            .unwrap_or_default();
        let archives: Vec<(String, String, String, String)> =
            self.ws.as_ref().map(|ws| ws.archived()).unwrap_or_default();
        let mut unarchive: Option<String> = None;
        let modal = egui::Modal::new(Id::unique("trash")).show(ctx, |ui| {
            ui.set_width(480.0);
            if !archives.is_empty() {
                ui.heading(tr(lang, "archives"));
                ui.add_space(4.0);
                for (id, title, icon, _) in &archives {
                    ui.horizontal(|ui| {
                        let icon = if icon.is_empty() { "📄" } else { icon.as_str() };
                        let title = if title.is_empty() { tr(lang, "untitled") } else { title.as_str() };
                        ui.label(format!("{icon}  {title}"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(tr(lang, "unarchive")).clicked() {
                                unarchive = Some(id.clone());
                            }
                        });
                    });
                }
                ui.add_space(10.0);
            }
            ui.heading(tr(lang, "trash"));
            ui.add_space(6.0);
            if items.is_empty() {
                ui.label(RichText::new(tr(lang, "trash_empty")).color(pal.text_secondary));
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (d, title, icon, when) in &items {
                    ui.horizontal(|ui| {
                        let icon = if icon.is_empty() { "📄" } else { icon.as_str() };
                        let title = if title.is_empty() { tr(lang, "untitled") } else { title.as_str() };
                        ui.label(format!("{icon}  {title}"));
                        ui.label(RichText::new(widgets::relative_time(when, lang)).size(12.0).color(pal.text_faint));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(RichText::new(tr(lang, "delete_forever")).color(pal.danger)).clicked() {
                                delete = Some(d.clone());
                            }
                            if ui.small_button(tr(lang, "restore")).clicked() {
                                restore = Some(d.clone());
                            }
                        });
                    });
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if !items.is_empty() && ui.button(RichText::new(tr(lang, "empty_trash")).color(pal.danger)).clicked() {
                    empty = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr(lang, "close")).clicked() {
                        close = true;
                    }
                });
            });
        });
        if let Some(ws) = self.ws.as_mut() {
            let r = if let Some(d) = &restore {
                ws.restore(d)
            } else if let Some(d) = &delete {
                ws.delete_forever(d)
            } else if empty {
                ws.empty_trash()
            } else {
                Ok(())
            };
            if let Err(e) = r {
                self.notify(e.to_string());
            }
        }
        if let Some(id) = unarchive {
            if let Some(Err(e)) = self.ws.as_mut().map(|w| w.set_archived(&id, false)) {
                self.notify(e.to_string());
            }
            self.navigate(Target::Doc(DocId::Page(id)));
        }
        if let Some(d) = restore {
            self.navigate(Target::Doc(d));
        }
        if delete.is_some() || empty {
            self.validate_current();
        }
        if close || modal.should_close() {
            self.trash_open = false;
        }
    }

    pub(super) fn welcome(&mut self, ui: &mut egui::Ui) {
        if self.onboarding_due() {
            return self.onboarding(ui);
        }
        let pal = self.pal;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.22).max(40.0));
            app_icon(ui, 96.0);
            ui.add_space(10.0);
            ui.label(RichText::new(self.t("welcome_title")).size(28.0).color(pal.text));
            ui.add_space(4.0);
            ui.label(RichText::new(self.t("welcome_text")).size(15.0).color(pal.text_secondary));
            ui.add_space(22.0);
            ui.add_space(6.0);
            for k in ["welcome_how1", "welcome_how2", "welcome_how3"] {
                ui.label(RichText::new(format!("•  {}", self.t(k))).size(13.5).color(pal.text_secondary));
            }
            ui.add_space(22.0);
            let big = |t: &str| egui::Button::new(RichText::new(t).size(15.0)).min_size(Vec2::new(340.0, 40.0));
            if ui.add(big(self.t("welcome_create")).fill(pal.accent_soft)).clicked() {
                let docs = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents").join("NoteCraft"));
                match docs {
                    Some(d) if std::fs::create_dir_all(&d).is_ok() => self.open_workspace(&d, false),
                    _ => self.pick_workspace(),
                }
            }
            ui.label(RichText::new(self.t("welcome_create_hint")).size(12.0).color(pal.text_faint));
            ui.add_space(12.0);
            if ui
                .add(
                    egui::Button::new(RichText::new(format!("▶  {}", self.t("demo_watch"))).size(14.0))
                        .min_size(Vec2::new(340.0, 34.0)),
                )
                .clicked()
            {
                self.demo_at = Some(0);
            }
            ui.label(RichText::new(self.t("demo_hint")).size(12.0).color(pal.text_faint));
            ui.add_space(14.0);
            if ui.add(big(self.t("welcome_other"))).clicked() {
                self.pick_workspace();
            }
            ui.label(RichText::new(self.t("welcome_other_hint")).size(12.0).color(pal.text_faint));
            let recent: Vec<PathBuf> =
                self.prefs.recent.iter().filter(|p| p.is_dir() && !is_too_broad(p)).cloned().collect();
            let mut forget: Option<PathBuf> = None;
            let mut trash: Option<PathBuf> = None;
            if !recent.is_empty() {
                ui.add_space(18.0);
                ui.label(RichText::new(self.t("recent")).size(13.0).color(pal.text_secondary));
                for p in recent {
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() / 2.0 - 150.0).max(0.0));
                        let open = widgets::subtle_button(
                            ui,
                            RichText::new(p.display().to_string()).size(13.0).color(pal.text),
                            &pal,
                        );
                        if open.clicked() {
                            self.open_workspace(&p, false);
                        }
                        open.context_menu(|ui| {
                            if ui.button(self.t("forget_workspace")).clicked() {
                                forget = Some(p.clone());
                                ui.close();
                            }
                        });
                        if crate::icons::trash_button(ui, &pal).on_hover_text(self.t("trash_one")).clicked() {
                            trash = Some(p.clone());
                        }
                    });
                }
            }
            if let Some(p) = forget {
                self.prefs.recent.retain(|r| r != &p);
            }
            if let Some(p) = trash {
                self.trash_workspace(&p);
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 90.0);
                ui.radio_value(&mut self.prefs.lang, Lang::En, "English");
                ui.radio_value(&mut self.prefs.lang, Lang::Fr, "Français");
            });
        });
    }

    pub(super) fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = self.toast.clone() else { return };
        let age = at.elapsed().as_secs_f32();
        if age > 4.0 {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("toast"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_max_width(screen.width() * 0.6);
                    ui.label(RichText::new(msg).size(13.5));
                });
            });
    }

    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering && self.ws.is_some() {
            let pal = self.pal;
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("drop-overlay")));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(40));
            painter.rect_stroke(screen.shrink(12.0), 12.0, Stroke::new(2.0, pal.accent), egui::StrokeKind::Inside);
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                self.t("drop_files"),
                egui::FontId::proportional(22.0),
                Color32::WHITE,
            );
        }
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| p.is_file()).collect());
        if !dropped.is_empty() {
            self.attach_files(dropped, None, false, None);
        }
    }

    pub(super) fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let Some(shot) = self.shot.as_mut() else { return };
        shot.frames += 1;
        ctx.request_repaint();
        let wait: u32 = std::env::var("NC_SHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
        if shot.frames >= wait && !shot.requested {
            shot.requested = true;
            let path = shot.path.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |img: Arc<egui::ColorImage>| {
                let [w, h] = img.size;
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match image::RgbaImage::from_raw(w as u32, h as u32, img.as_raw().to_vec()) {
                    Some(buf) => {
                        if let Err(e) = buf.save(&path) {
                            eprintln!("could not save screenshot: {e}");
                        }
                    }
                    None => eprintln!("could not build screenshot image"),
                }
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        }
        if shot.frames > wait + 480 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// The NøteCraft icon, shown on the welcome screens.
pub(super) fn app_icon(ui: &mut egui::Ui, size: f32) {
    ui.add(
        egui::Image::from_bytes("bytes://notecraft-icon.png", &include_bytes!("../../assets/demo/icon.png")[..])
            .fit_to_exact_size(Vec2::splat(size))
            .corner_radius(size * 0.22),
    );
}
