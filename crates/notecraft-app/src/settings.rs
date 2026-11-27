//! The settings window: a side list of sections and, on the right, one page of cards per section.
//!
//! Type scale, used everywhere here: page title 20, body 13, hints and headings 12.

mod ai;
mod meeting;
use crate::ai_panel::EngineChoice;
use crate::app::{reveal, NcApp, SHORTCUTS};
use crate::i18n::{tr, Lang};
use crate::mcp_setup::{self, DesktopState};
use crate::theme::ThemeChoice;
use crate::widgets::{card, hint, row_divider, section, setting_row, status_pill, toggle};
use eframe::egui::{self, FontId, Id, RichText, TextStyle, Ui};
use notecraft_ai::keychain;
use notecraft_core::sync;
use notecraft_core::watch::WatchMode;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    General,
    Data,
    Ai,
    Assistants,
    Shortcuts,
    About,
    Backups,
}

/// What is typed (and what is stored, masked) for publishing.
#[derive(Default)]
pub struct PublishCfg {
    pub token_input: String,
    pub open: bool,
    /// The stored token, masked; loaded on first use.
    pub status: Option<String>,
}

fn page_title(ui: &mut Ui, pal: &crate::theme::Palette, _title: &str, subtitle: &str) {
    if !subtitle.is_empty() {
        hint(ui, pal, subtitle);
        ui.add_space(10.0);
    }
}

/// A path with the home folder shortened to `~`.
fn tilde(path: &Path) -> String {
    let s = path.display().to_string();
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s,
    }
}

/// Show the folder (or its closest existing parent) in the Finder.
fn show(path: &Path) {
    let mut p = path;
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent,
            None => return,
        }
    }
    reveal(p);
}

/// One place where files live: what it is, where, and a button to see it in the Finder.
fn file_row(ui: &mut Ui, pal: &crate::theme::Palette, lang: Lang, title: &str, text: &str, path: &Path) {
    ui.horizontal(|ui| {
        let total = ui.available_width();
        ui.vertical(|ui| {
            ui.set_max_width(total - 90.0);
            ui.label(RichText::new(title).size(13.0).color(pal.text));
            ui.add(egui::Label::new(RichText::new(tilde(path)).size(12.0).color(pal.text_secondary)).wrap());
            hint(ui, pal, text);
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::link(ui, pal, tr(lang, "show_short")).clicked() {
                show(path);
            }
        });
    });
}

fn macos_version() -> String {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::consts::OS.to_string())
}

impl NcApp {
    fn system_info(&self) -> String {
        let (pages, dbs) = self.ws.as_ref().map(|w| (w.pages.len(), w.dbs.len())).unwrap_or((0, 0));
        format!(
            "NøteCraft {} ({})\nmacOS {} {}\n{pages} pages, {dbs} databases",
            env!("CARGO_PKG_VERSION"),
            env!("NOTECRAFT_COMMIT"),
            macos_version(),
            std::env::consts::ARCH
        )
    }

    pub fn settings_ui(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let pal = self.pal;
        let lang = self.prefs.lang;
        let mut close = false;
        let height = (ctx.content_rect().height() - 110.0).clamp(420.0, 620.0);
        let modal = egui::Modal::new(Id::unique("settings")).show(ctx, |ui| {
            // one text scale for the whole window
            let styles = &mut ui.style_mut().text_styles;
            styles.insert(TextStyle::Body, FontId::proportional(13.0));
            styles.insert(TextStyle::Button, FontId::proportional(13.0));
            styles.insert(TextStyle::Small, FontId::proportional(12.0));
            styles.insert(TextStyle::Monospace, FontId::monospace(12.0));
            // buttons get a visible body so they read as buttons
            let w = &mut ui.style_mut().visuals.widgets;
            w.inactive.weak_bg_fill = pal.hover;
            w.hovered.weak_bg_fill = pal.active;
            w.inactive.bg_stroke = egui::Stroke::NONE;
            w.hovered.bg_stroke = egui::Stroke::NONE;
            w.active.bg_stroke = egui::Stroke::NONE;
            for st in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                st.corner_radius = egui::CornerRadius::same(8);
                st.expansion = 0.0;
            }
            ui.set_width(480.0);
            ui.set_height(height);
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("settings-page", self.settings_tab as u8))
                    .max_height(height - 44.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(460.0);
                        match self.settings_tab {
                            Tab::General => self.page_main(ui),
                            Tab::Data => {
                                self.sub_header(ui, tr(lang, "set_data_security"));
                                self.tab_data(ui);
                            }
                            Tab::Ai => {
                                self.sub_header(ui, tr(lang, "set_meeting"));
                                self.meeting_card(ui);
                            }
                            Tab::Assistants => {
                                self.sub_header(ui, tr(lang, "set_assistants"));
                                self.tab_assistants(ui);
                            }
                            Tab::Shortcuts => {
                                self.sub_header(ui, tr(lang, "shortcuts"));
                                self.tab_shortcuts(ui);
                            }
                            Tab::About => {
                                self.sub_header(ui, tr(lang, "set_about"));
                                self.tab_about(ui);
                            }
                            Tab::Backups => {
                                self.sub_header(ui, tr(lang, "restore_points"));
                                self.restore_page(ui);
                            }
                        }
                    });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                    ui.set_max_width(460.0);
                    if crate::widgets::link(ui, &pal, tr(lang, "done")).clicked() {
                        close = true;
                    }
                });
            });
        });
        if close || modal.should_close() {
            self.settings_open = false;
            self.settings_tab = Tab::General;
        }
    }

    /// The top of a page opened from the main one: a way back and the page title.
    fn sub_header(&mut self, ui: &mut Ui, title: &str) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        if crate::widgets::text_link(ui, &pal, &format!("‹  {}", tr(lang, "settings"))).clicked() {
            self.settings_tab = Tab::General;
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(title).size(18.0).color(pal.text));
        });
        ui.add_space(10.0);
    }

    // ------------------------------------------------------------- main page

    /// The one page people open: a few lines, then the rest one level down.
    fn page_main(&mut self, ui: &mut Ui) {
        use crate::widgets::{cycle, nav_row, srow, text_link, Glyph};
        let pal = self.pal;
        let lang = self.prefs.lang;
        self.ai_poll(ui);
        ui.add_space(2.0);
        srow(ui, &pal, tr(lang, "theme"), "", None, |ui| {
            let opts = [
                (ThemeChoice::System, Glyph::Half, tr(lang, "theme_auto").to_string()),
                (ThemeChoice::Light, Glyph::Sun, tr(lang, "light").to_string()),
                (ThemeChoice::Dark, Glyph::Moon, tr(lang, "dark").to_string()),
            ];
            cycle(ui, &pal, &mut self.prefs.theme, &opts, false);
        });
        srow(ui, &pal, tr(lang, "text_size"), "", None, |ui| {
            let opts = [
                (85u32, Glyph::Letter(9.0), "85 %".to_string()),
                (100, Glyph::Letter(13.0), "100 %".to_string()),
                (115, Glyph::Letter(17.0), "115 %".to_string()),
                (130, Glyph::Letter(21.0), "130 %".to_string()),
            ];
            cycle(ui, &pal, &mut self.prefs.text_percent, &opts, false);
        });
        srow(ui, &pal, tr(lang, "language"), "", None, |ui| {
            let opts = [
                (Lang::Fr, Glyph::Code("FR"), "Français".to_string()),
                (Lang::En, Glyph::Code("EN"), "English".to_string()),
            ];
            cycle(ui, &pal, &mut self.prefs.lang, &opts, false);
        });
        if let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) {
            srow(ui, &pal, tr(lang, "files_notes"), &tilde(&root), None, |ui| {
                if text_link(ui, &pal, tr(lang, "show_short")).clicked() {
                    show(&root);
                }
            });
        }
        if (1..60).contains(&self.prefs.backup_minutes) {
            self.prefs.backup_minutes = 60;
        }
        let last = match &self.backup.last_made {
            Some(t) => format!("{} {}", tr(lang, "backup_last"), crate::widgets::relative_time(t, lang)),
            None => tr(lang, "backup_none_yet").to_string(),
        };
        let mut on = self.prefs.backup_minutes != 0;
        let mut more = false;
        srow(ui, &pal, tr(lang, "set_daily_backup"), &last, None, |ui| {
            if toggle(ui, &pal, &mut on).changed() {
                self.prefs.backup_minutes = if on { 1440 } else { 0 };
            }
            more = text_link(ui, &pal, tr(lang, "settings")).clicked();
        });
        if more {
            self.settings_tab = Tab::Backups;
        }
        self.ai_row(ui);
        self.publish_row(ui);

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(tr(lang, "set_advanced").to_uppercase()).size(11.0).color(pal.text_faint));
        });
        ui.add_space(2.0);
        if nav_row(ui, &pal, tr(lang, "set_data_security"), "").clicked() {
            self.settings_tab = Tab::Data;
        }
        if nav_row(ui, &pal, tr(lang, "set_meeting"), "").clicked() {
            self.settings_tab = Tab::Ai;
        }
        if nav_row(ui, &pal, tr(lang, "set_assistants"), "").clicked() {
            self.settings_tab = Tab::Assistants;
        }
        if nav_row(ui, &pal, tr(lang, "shortcuts"), "").clicked() {
            self.settings_tab = Tab::Shortcuts;
        }
        if nav_row(ui, &pal, tr(lang, "set_about"), notecraft_core::version()).clicked() {
            self.settings_tab = Tab::About;
        }
    }

    // ------------------------------------------------------------- maintenance

    fn maintenance(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        if self.ws.is_none() {
            return;
        }
        ui.add_space(14.0);
        section(ui, &pal, tr(lang, "set_maintenance"));
        let (mut rebuild, mut reload) = (false, false);
        card(ui, &pal, |ui| {
            setting_row(ui, &pal, tr(lang, "rebuild_index"), tr(lang, "workspace_hint"), |ui| {
                rebuild = ui.button(tr(lang, "rebuild_index_btn")).clicked();
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "reload_files"), tr(lang, "reload_hint"), |ui| {
                reload = ui.button(tr(lang, "reload_btn")).clicked();
            });
        });
        if rebuild || reload {
            if let Some(ws) = self.ws.as_mut() {
                let r = if reload { ws.reload() } else { ws.reindex() };
                match r {
                    Ok(()) => self.toast = Some((tr(lang, "index_rebuilt").into(), Instant::now())),
                    Err(e) => self.toast = Some((e.to_string(), Instant::now())),
                }
            }
            self.validate_current();
        }
    }

    // -------------------------------------------------------------------- data

    fn tab_data(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, tr(lang, "set_data"), tr(lang, "set_data_sub"));
        self.files_card(ui);
        ui.add_space(14.0);
        self.sync_card(ui);
        ui.add_space(14.0);
        self.vault_section(ui);
        self.maintenance(ui);
    }

    fn files_card(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let Some(ws) = &self.ws else { return };
        let root = ws.root.clone();
        let assets = root.join(notecraft_core::workspace::ASSETS_DIR);
        let backups = ws.snapshot_dir();
        let index = ws.index_file.clone();
        let logs = notecraft_core::log::log_dir();
        let (mut switch, mut mv) = (false, false);
        section(ui, &pal, tr(lang, "files_title"));
        card(ui, &pal, |ui| {
            file_row(ui, &pal, lang, tr(lang, "files_notes"), tr(lang, "files_notes_hint"), &root);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                switch = ui.button(tr(lang, "change_workspace")).clicked();
                mv = ui.button(tr(lang, "move_workspace")).clicked();
            });
            ui.add_space(2.0);
            hint(ui, &pal, tr(lang, "move_hint"));
            row_divider(ui, &pal);
            file_row(ui, &pal, lang, tr(lang, "files_assets"), tr(lang, "files_assets_hint"), &assets);
            row_divider(ui, &pal);
            file_row(ui, &pal, lang, tr(lang, "backups"), tr(lang, "files_backups_hint"), &backups);
            row_divider(ui, &pal);
            file_row(ui, &pal, lang, tr(lang, "index_location"), tr(lang, "files_index_hint"), &index);
            row_divider(ui, &pal);
            file_row(ui, &pal, lang, tr(lang, "log_file"), tr(lang, "files_logs_hint"), &logs);
        });
        if switch {
            self.pick_workspace();
        }
        if mv {
            self.move_workspace();
        }
    }

    /// Where a page is published: a Cloudflare account of your own. One line with an icon; the
    /// account ID and the token sit in a small box under the icon, like the key of an AI service.
    fn publish_row(&mut self, ui: &mut Ui) {
        use crate::widgets::{icon_action, srow, text_link, Glyph};
        let pal = self.pal;
        let lang = self.prefs.lang;
        if self.publish_cfg.status.is_none() {
            // only whether a token exists: reading it would make macOS ask for the password
            let tok = if keychain::has_item(keychain::PUBLISH_TOKEN_ACCOUNT) { "stored" } else { "" };
            self.publish_cfg.status = Some(tok.to_string());
        }
        let tok = self.publish_cfg.status.clone().unwrap_or_default();
        let ready = !tok.is_empty();
        let sub = if ready { tr(lang, "publish_sub_ready") } else { tr(lang, "publish_sub_missing") };
        let mut anchor: Option<egui::Response> = None;
        let mut toggle_box = false;
        srow(ui, &pal, tr(lang, "publish_title"), sub, (!ready).then_some(pal.danger), |ui| {
            let r = icon_action(ui, &pal, Glyph::Cloud, tr(lang, "publish_title"), (!ready).then_some(pal.danger));
            toggle_box = r.clicked();
            anchor = Some(r);
        });
        if toggle_box {
            self.publish_cfg.open = !self.publish_cfg.open;
        }
        let Some(anchor) = anchor else { return };
        if !self.publish_cfg.open {
            return;
        }
        let (mut save, mut remove) = (false, false);
        let mut open = true;
        egui::Popup::from_response(&anchor)
            .id(Id::unique("publish-cfg"))
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(260.0);
                crate::widgets::menu_label(ui, &pal, tr(lang, "publish_token"));
                ui.add_space(2.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.publish_cfg.token_input)
                        .password(true)
                        .desired_width(f32::INFINITY)
                        .hint_text(if tok.is_empty() {
                            "…".to_string()
                        } else {
                            "••••••••".to_string()
                        }),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let typed = !self.publish_cfg.token_input.trim().is_empty();
                    if ui.add_enabled(typed, egui::Button::new(tr(lang, "save_keychain"))).clicked() {
                        save = true;
                    }
                    if !tok.is_empty() && text_link(ui, &pal, tr(lang, "remove_key")).clicked() {
                        remove = true;
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new(tr(lang, "publish_text")).size(12.0).color(pal.text_faint));
                ui.hyperlink_to(
                    RichText::new(tr(lang, "publish_get")).size(12.0),
                    "https://dash.cloudflare.com/profile/api-tokens",
                );
            });
        self.publish_cfg.open = open;
        if save {
            let token = self.publish_cfg.token_input.trim().to_string();
            let err: Option<String> = if token.is_empty() {
                None
            } else {
                keychain::save_item(keychain::PUBLISH_TOKEN_ACCOUNT, &token).err()
            };
            match err {
                None => {
                    self.publish_cfg.token_input.clear();
                    self.publish_cfg.status = None;
                    self.publish_cfg.open = false;
                    self.toast = Some((tr(lang, "publish_saved").into(), Instant::now()));
                }
                Some(e) => self.toast = Some((e, Instant::now())),
            }
        }
        if remove {
            let _ = keychain::delete_item(keychain::PUBLISH_TOKEN_ACCOUNT);
            self.publish_cfg.status = None;
        }
    }

    fn sync_card(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let Some(ws) = &self.ws else { return };
        section(ui, &pal, tr(lang, "sync"));
        card(ui, &pal, |ui| {
            hint(ui, &pal, tr(lang, "sync_text"));
            ui.add_space(8.0);
            ui.horizontal(|ui| match ws.synced_folder() {
                Some(kind) => status_pill(ui, &pal, &format!("{} {kind}", tr(lang, "sync_detected")), "green"),
                None => status_pill(ui, &pal, tr(lang, "sync_local_short"), "gray"),
            });
            if ws.git_in_synced_folder() {
                ui.add_space(6.0);
                ui.label(RichText::new(tr(lang, "git_warning")).size(12.0).color(pal.danger));
            }
            let pending = sync::pending_downloads(&ws.root);
            if pending > 0 {
                hint(ui, &pal, &format!("{pending} {}", tr(lang, "icloud_pending")));
            }
            if let Some(w) = &self.watcher {
                let mode = match w.mode() {
                    WatchMode::Events => tr(lang, "watch_events"),
                    WatchMode::PollWatcher | WatchMode::Timer => tr(lang, "watch_polling"),
                };
                hint(ui, &pal, &format!("{}: {mode}", tr(lang, "watcher")));
            }
            if !ws.conflicts.is_empty() {
                ui.label(
                    RichText::new(format!("{} × {}", ws.conflicts.len(), tr(lang, "conflict_title")))
                        .size(12.0)
                        .color(pal.danger),
                );
            }
        });
    }

    // ---------------------------------------------------------------- security

    fn vault_section(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let (count, open) = self
            .ws
            .as_ref()
            .map(|w| (w.pages.values().filter(|p| p.meta.encrypted).count(), w.vault_is_open()))
            .unwrap_or((0, false));
        let mut lock = false;
        section(ui, &pal, tr(lang, "set_vault_title"));
        card(ui, &pal, |ui| {
            hint(ui, &pal, tr(lang, "set_vault_text"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (text, color) = if count == 0 {
                    (tr(lang, "set_vault_none").to_string(), "gray")
                } else if open {
                    (format!("{count} · {}", tr(lang, "set_vault_open")), "orange")
                } else {
                    (format!("{count} · {}", tr(lang, "set_vault_closed")), "green")
                };
                status_pill(ui, &pal, &text, color);
                if open && ui.button(tr(lang, "vault_lock_now")).clicked() {
                    lock = true;
                }
            });
            ui.add_space(6.0);
            hint(ui, &pal, tr(lang, "vault_warning"));
        });
        if lock {
            self.lock_vault_now();
        }
    }

    // ---------------------------------------------------------------------- ai

    fn tab_shortcuts(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        for (i, (keys, desc)) in SHORTCUTS.iter().enumerate() {
            if i == 9 {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(tr(lang, "shortcuts_editing").to_uppercase()).size(11.0).color(pal.text_faint),
                    );
                });
                ui.add_space(2.0);
            }
            crate::widgets::shortcut_row(ui, &pal, keys, desc);
        }
    }

    // ------------------------------------------------------------------- about

    fn tab_about(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, "NøteCraft", tr(lang, "about_text"));
        let mut copy = false;
        let mut demo = false;
        card(ui, &pal, |ui| {
            setting_row(ui, &pal, tr(lang, "demo_watch"), tr(lang, "demo_hint"), |ui| {
                demo = ui.button(tr(lang, "demo_watch")).clicked();
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "version"), "", |ui| {
                ui.label(
                    RichText::new(format!("{} · {}", notecraft_core::version(), env!("NOTECRAFT_COMMIT")))
                        .monospace()
                        .color(pal.text_secondary),
                );
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "about_license_title"), "", |ui| {
                ui.label(RichText::new(tr(lang, "about_license")).color(pal.text_secondary));
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "copy_system_info"), tr(lang, "about_support_hint"), |ui| {
                copy = ui.button(tr(lang, "copy_btn")).clicked();
            });
        });
        ui.add_space(8.0);
        hint(ui, &pal, tr(lang, "about_credits"));
        if demo {
            self.demo_at = Some(0);
        }
        if copy {
            let info = self.system_info();
            ui.ctx().copy_text(info);
            self.notify(tr(lang, "copied"));
        }
    }
}
