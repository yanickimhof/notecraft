//! The settings window: a side list of sections and, on the right, one page of cards per section.
//!
//! Type scale, used everywhere here: page title 20, body 13, hints and headings 12.

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum OllamaState {
    Checking,
    Ready,
    NoModel,
    Stopped,
    Missing,
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

    /// Model lists change (a model is installed, a new one is released): ask again when the page
    /// opens and then at most once a minute while it stays open.
    fn ai_poll(&mut self, ui: &mut Ui) {
        // model lists change (a model is installed, a new one is released): ask again when the page
        // opens and then at most once a minute while it stays open
        if self.ai.checked.is_none_or(|t| t.elapsed().as_secs() >= 60) {
            self.ai.checked = Some(Instant::now());
            if self.ai.models_loading.is_none() {
                self.ai.models_error = None;
                self.refresh_models();
            }
            // the cloud lists need the keys: ask once per session, not every minute, so that
            // macOS does not ask for the password again and again
            if !self.ai.cloud_checked {
                self.ai.cloud_checked = true;
                if self.ai.claude_loading.is_none() && keychain::is_available(keychain::Provider::Claude) {
                    self.refresh_claude_models();
                }
                if self.ai.openai_loading.is_none() && keychain::is_available(keychain::Provider::OpenAi) {
                    self.refresh_openai_models();
                }
                if self.ai.grok_loading.is_none() && keychain::is_available(keychain::Provider::Grok) {
                    self.refresh_grok_models();
                }
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
    }

    /// Speech model, language and note template for `/meeting` blocks: icons for choices, one line each.
    fn meeting_card(&mut self, ui: &mut Ui) {
        use crate::widgets::{cycle, icon_action, icon_choice, srow, srow_tip, Glyph};
        use notecraft_meeting::MODELS;
        let pal = self.pal;
        let lang = self.prefs.lang;
        let downloading = self.meeting_view().download.is_some();
        let faint = Some(pal.text_faint);
        for m in MODELS.iter() {
            let chosen = self.prefs.meeting.model == m.id;
            let sub = format!("{}, {} MB", tr(lang, &format!("model_{}", m.note)), m.size_mb);
            let installed = m.installed();
            srow(ui, &pal, m.label, &sub, None, |ui| {
                if installed {
                    if icon_action(ui, &pal, Glyph::Trash, tr(lang, "delete"), None).clicked() {
                        let _ = m.delete();
                    }
                    if icon_choice(ui, &pal, Glyph::Check, chosen, tr(lang, "choose")).clicked() && !chosen {
                        self.prefs.meeting.model = m.id.to_string();
                    }
                } else if icon_action(
                    ui,
                    &pal,
                    Glyph::Download,
                    tr(lang, "meeting_download"),
                    downloading.then_some(pal.text_faint),
                )
                .clicked()
                    && !downloading
                {
                    self.prefs.meeting.model = m.id.to_string();
                    self.meeting_action(crate::meeting::MeetingAction::DownloadModel);
                }
            });
        }
        {
            use notecraft_meeting::models::{parakeet_delete, parakeet_installed, PARAKEET_MB};
            let installed = parakeet_installed();
            let sub = format!("{}, {PARAKEET_MB} MB", tr(lang, "parakeet_short"));
            srow_tip(ui, &pal, "Parakeet", &sub, None, tr(lang, "parakeet_note"), |ui| {
                if installed {
                    if icon_action(ui, &pal, Glyph::Trash, tr(lang, "delete"), None).clicked() {
                        let _ = parakeet_delete();
                    }
                } else if icon_action(
                    ui,
                    &pal,
                    Glyph::Download,
                    tr(lang, "meeting_download"),
                    faint.filter(|_| downloading),
                )
                .clicked()
                    && !downloading
                {
                    self.meeting_action(crate::meeting::MeetingAction::DownloadParakeet);
                }
            });
        }
        if let Some((name, frac)) = self.meeting_view().download {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.add(egui::ProgressBar::new(frac).desired_width(300.0).text(format!("{name} {:.0}%", frac * 100.0)));
                if crate::widgets::text_link(ui, &pal, tr(lang, "cancel")).clicked() {
                    self.meeting_action(crate::meeting::MeetingAction::CancelDownload);
                }
            });
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // what is recorded: the microphone, the system audio, or both
        srow_tip(ui, &pal, tr(lang, "meeting_source"), "", None, tr(lang, "meeting_source_hint"), |ui| {
            for (id, g, k) in [
                ("both", Glyph::Both, "meeting_source_both"),
                ("system", Glyph::Speaker, "meeting_source_system"),
                ("mic", Glyph::Mic, "meeting_source_mic"),
            ] {
                if icon_choice(ui, &pal, g, self.prefs.meeting.source == id, tr(lang, k)).clicked() {
                    self.prefs.meeting.source = id.to_string();
                }
            }
        });
        // the microphone: a click steps to the next one
        srow(ui, &pal, tr(lang, "meeting_microphone"), "", None, |ui| {
            let (devices, default) = notecraft_meeting::input_devices();
            let default_label = match &default {
                Some(d) => format!("{} ({d})", tr(lang, "meeting_mic_default")),
                None => tr(lang, "meeting_mic_default").to_string(),
            };
            let short = |s: &str| -> String {
                if s.chars().count() > 26 {
                    format!("{}…", s.chars().take(25).collect::<String>())
                } else {
                    s.to_string()
                }
            };
            let mut opts: Vec<(String, Glyph, String)> = vec![(String::new(), Glyph::Mic, short(&default_label))];
            for d in devices {
                opts.push((d.clone(), Glyph::Mic, short(&d)));
            }
            cycle(ui, &pal, &mut self.prefs.meeting.microphone, &opts, true);
        });
        srow(ui, &pal, tr(lang, "meeting_spoken_language"), "", None, |ui| {
            let opts = [
                ("auto".to_string(), Glyph::Code("AUTO"), tr(lang, "meeting_lang_auto").to_string()),
                ("en".to_string(), Glyph::Code("EN"), tr(lang, "meeting_lang_en").to_string()),
                ("fr".to_string(), Glyph::Code("FR"), tr(lang, "meeting_lang_fr").to_string()),
            ];
            cycle(ui, &pal, &mut self.prefs.meeting.language, &opts, false);
        });
        srow_tip(ui, &pal, tr(lang, "meeting_engine"), "", None, tr(lang, "meeting_engine_sub"), |ui| {
            let opts = [
                ("auto".to_string(), Glyph::Code("AUTO"), tr(lang, "engine_auto").to_string()),
                ("whisper".to_string(), Glyph::Code("Whisper"), tr(lang, "engine_whisper").to_string()),
                ("parakeet".to_string(), Glyph::Code("Parakeet"), tr(lang, "engine_parakeet").to_string()),
            ];
            cycle(ui, &pal, &mut self.prefs.meeting.engine, &opts, false);
        });
        srow(ui, &pal, tr(lang, "meeting_template"), "", None, |ui| {
            let opts: Vec<(String, Glyph, String)> = notecraft_ai::NOTE_TEMPLATES
                .iter()
                .map(|k| ((*k).to_string(), Glyph::None, crate::meeting::template_label(lang, k).to_string()))
                .collect();
            cycle(ui, &pal, &mut self.prefs.meeting.template, &opts, true);
        });
        if self.prefs.meeting.template == "custom" {
            ui.add(
                egui::TextEdit::multiline(&mut self.prefs.meeting.custom)
                    .hint_text(tr(lang, "meeting_custom_hint"))
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
        }
        srow(ui, &pal, tr(lang, "meeting_auto_notes"), "", None, |ui| {
            toggle(ui, &pal, &mut self.prefs.meeting.auto_notes);
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(tr(lang, "meeting_vocabulary")).size(13.0).color(pal.text));
        });
        ui.add_space(2.0);
        ui.add(
            egui::TextEdit::multiline(&mut self.prefs.meeting.vocabulary)
                .hint_text(tr(lang, "meeting_vocabulary_hint"))
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.add_space(10.0);
        let cloud = self.prefs.ai.leaves_this_mac();
        let note = tr(lang, if cloud { "meeting_privacy_cloud" } else { "meeting_privacy_local" });
        ui.horizontal_wrapped(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(note).size(12.0).color(if cloud { pal.danger } else { pal.text_faint }));
        });
        ui.horizontal_wrapped(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(tr(lang, "meeting_consent")).size(12.0).color(pal.text_faint));
        });
    }

    // ---------------------------------------------------------------------- ai

    fn ollama_state(&mut self) -> OllamaState {
        let active_local = self.prefs.ai.engine == EngineChoice::Ollama;
        let local_url = notecraft_ai::is_local_url(&self.prefs.ai.ollama_url);
        if (active_local || local_url)
            && self.ai.models.is_empty()
            && self.ai.models_loading.is_none()
            && self.ai.models_error.is_none()
        {
            self.refresh_models();
        }
        if self.ai.models_error.is_some() {
            if notecraft_ai::ollama_installed() {
                OllamaState::Stopped
            } else {
                OllamaState::Missing
            }
        } else if self.ai.models_loading.is_some() && self.ai.models.is_empty() {
            OllamaState::Checking
        } else if self.ai.models.is_empty() {
            OllamaState::NoModel
        } else {
            OllamaState::Ready
        }
    }

    fn key_status_slot(&mut self, p: keychain::Provider) -> &mut Option<String> {
        match p {
            keychain::Provider::Claude => &mut self.ai.key_status,
            keychain::Provider::OpenAi => &mut self.ai.openai_key_status,
            keychain::Provider::Grok => &mut self.ai.grok_key_status,
        }
    }

    /// What is stored for a cloud service: a masked key, "env" (taken from the environment) or nothing.
    fn key_status(&mut self, provider: keychain::Provider) -> String {
        if let Some(s) = self.key_status_slot(provider).clone() {
            return s;
        }
        let s = if keychain::has_key(provider) {
            "stored".to_string()
        } else if keychain::is_available(provider) {
            "env".to_string()
        } else {
            String::new()
        };
        *self.key_status_slot(provider) = Some(s.clone());
        s
    }

    /// The models a cloud service offers: its own list once it answers, a short known one until then.
    fn provider_models(&self, p: keychain::Provider) -> Vec<String> {
        let (list, fallback): (&Vec<String>, &[&str]) = match p {
            keychain::Provider::Claude => (&self.ai.claude_models, notecraft_ai::CLAUDE_FALLBACK_MODELS),
            keychain::Provider::OpenAi => (&self.ai.openai_models, notecraft_ai::OPENAI_FALLBACK_MODELS),
            keychain::Provider::Grok => (&self.ai.grok_models, notecraft_ai::GROK_FALLBACK_MODELS),
        };
        if list.is_empty() {
            fallback.iter().map(|m| m.to_string()).collect()
        } else {
            list.clone()
        }
    }

    /// The AI lines. First: where the text goes, as icons (this Mac, Claude, OpenAI, Grok), with the
    /// key of a cloud service. Second: the model, in a small menu.
    fn ai_row(&mut self, ui: &mut Ui) {
        use crate::widgets::{icon_action, icon_choice, model_menu, srow, text_link, Glyph};
        let pal = self.pal;
        let lang = self.prefs.lang;
        let engine = self.prefs.ai.engine;
        let provider = engine.provider();
        let ostate = if provider.is_none() { self.ollama_state() } else { OllamaState::Ready };
        let key = provider.map(|p| self.key_status(p)).unwrap_or_default();
        let (sub, bad) = match provider {
            None => match ostate {
                OllamaState::Ready | OllamaState::Checking => (tr(lang, "ai_local_note"), false),
                OllamaState::NoModel => (tr(lang, "ollama_no_model"), true),
                OllamaState::Stopped => (tr(lang, "ollama_stopped"), true),
                OllamaState::Missing => (tr(lang, "ollama_missing"), true),
            },
            Some(_) if key.is_empty() => (tr(lang, "ai_key_missing"), true),
            Some(_) => (tr(lang, crate::ai_panel::cloud_note_key(engine)), false),
        };
        let mut models: Vec<String> = match provider {
            None => self.ai.models.clone(),
            Some(p) => self.provider_models(p),
        };
        let current = self.prefs.ai.model().to_string();
        if !current.is_empty() && !models.contains(&current) && !models.is_empty() {
            models.insert(0, current.clone());
        }

        let mut pick: Option<EngineChoice> = None;
        let mut key_resp: Option<egui::Response> = None;
        let mut open_key = false;
        srow(ui, &pal, tr(lang, "ai_title"), sub, bad.then_some(pal.danger), |ui| {
            // right to left: the key, then the services from the last to the first
            if provider.is_some() {
                let tint = key.is_empty().then_some(pal.danger);
                let r = icon_action(ui, &pal, Glyph::Key, tr(lang, "ai_key"), tint);
                open_key = r.clicked();
                key_resp = Some(r);
            }
            for (e, g, k) in [
                (EngineChoice::Grok, Glyph::Grok, "ai_grok_short"),
                (EngineChoice::OpenAi, Glyph::OpenAi, "ai_openai_short"),
                (EngineChoice::Claude, Glyph::Sparkle, "ai_claude_short"),
                (EngineChoice::Ollama, Glyph::Laptop, "ai_ollama_short"),
            ] {
                if icon_choice(ui, &pal, g, engine == e, tr(lang, k)).clicked() {
                    pick = Some(e);
                }
            }
        });

        let ready = provider.is_some() || matches!(ostate, OllamaState::Ready | OllamaState::Checking);
        let mut model: Option<String> = None;
        let (mut launch, mut download) = (false, false);
        let mut pull: Option<&str> = None;
        srow(ui, &pal, tr(lang, "ai_model"), "", None, |ui| {
            if ready {
                if !models.is_empty() {
                    model = model_menu(ui, &pal, &current, &models);
                }
            } else {
                match ostate {
                    OllamaState::Missing => download = text_link(ui, &pal, tr(lang, "ollama_download")).clicked(),
                    OllamaState::Stopped => launch = text_link(ui, &pal, tr(lang, "ollama_launch")).clicked(),
                    _ => {
                        if let Some(job) = &self.ai.pull {
                            use std::sync::atomic::Ordering;
                            let (d, t) = (job.done.load(Ordering::Relaxed), job.total.load(Ordering::Relaxed));
                            let pct = (d * 100).checked_div(t).map_or("…".to_string(), |v| format!(" {v}%"));
                            ui.label(format!("{} {}{}", tr(lang, "ollama_pulling"), job.model, pct));
                            if text_link(ui, &pal, tr(lang, "cancel")).clicked() {
                                job.cancel.store(true, Ordering::Relaxed);
                            }
                        } else {
                            for (m, size) in [("qwen2.5:7b", "4.7 GB"), ("llama3.2", "2 GB")] {
                                if text_link(ui, &pal, &format!("{} {m} ({size})", tr(lang, "ollama_get"))).clicked() {
                                    pull = Some(m);
                                }
                            }
                        }
                    }
                }
            }
        });

        if let Some(e) = pick {
            self.prefs.ai.engine = e;
            match e.provider() {
                Some(p) => {
                    // a service without a key cannot answer: ask for the key right away
                    if self.key_status(p).is_empty() {
                        self.ai.key_open = true;
                    }
                }
                None => {
                    self.ai.models_error = None;
                    self.refresh_models();
                }
            }
        }
        if let Some(m) = model {
            self.prefs.ai.set_model(m);
        }
        if open_key {
            self.ai.key_open = !self.ai.key_open;
        }
        if download {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://ollama.com/download"));
        }
        if let Some(m) = pull {
            self.start_pull(m);
        }
        if launch {
            let _ = std::process::Command::new("open").args(["-a", "Ollama"]).spawn();
            self.ai.models_error = None;
        }

        // the key, in a small box under its icon
        let Some(p) = self.prefs.ai.engine.provider() else {
            self.ai.key_open = false;
            return;
        };
        let Some(anchor) = key_resp else { return };
        if !self.ai.key_open {
            return;
        }
        let (placeholder, env_key, get_url) = match p {
            keychain::Provider::Claude => ("sk-ant-…", "ai_key_env", "https://console.anthropic.com/settings/keys"),
            keychain::Provider::OpenAi => ("sk-…", "ai_key_env_openai", "https://platform.openai.com/api-keys"),
            keychain::Provider::Grok => ("xai-…", "ai_key_env_grok", "https://console.x.ai"),
        };
        let status = self.key_status(p);
        let mut open = true;
        let (mut save, mut remove) = (false, false);
        egui::Popup::from_response(&anchor)
            .id(Id::unique("ai-key"))
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(260.0);
                crate::widgets::menu_label(ui, &pal, tr(lang, "ai_key"));
                ui.add_space(2.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.ai.key_input)
                        .password(true)
                        .desired_width(f32::INFINITY)
                        .hint_text(placeholder),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.ai.key_input.trim().is_empty(), egui::Button::new(tr(lang, "save_keychain")))
                        .clicked()
                    {
                        save = true;
                    }
                    if !status.is_empty() && status != "env" && text_link(ui, &pal, tr(lang, "remove_key")).clicked() {
                        remove = true;
                    }
                });
                let line = if status.is_empty() {
                    tr(lang, "ai_key_missing").to_string()
                } else if status == "env" {
                    tr(lang, env_key).to_string()
                } else {
                    tr(lang, "ai_key_stored").to_string()
                };
                ui.add_space(4.0);
                ui.label(RichText::new(line).size(12.0).color(pal.text_faint));
                ui.hyperlink_to(RichText::new(tr(lang, "ai_key_get")).size(12.0), get_url);
            });
        self.ai.key_open = open;
        if save {
            match keychain::save_key(p, self.ai.key_input.trim()) {
                Ok(()) => {
                    self.ai.key_input.clear();
                    self.ai.key_open = false;
                    *self.key_status_slot(p) = None;
                    self.refresh_models_for(p);
                    self.toast = Some((tr(lang, "ai_key_saved").into(), Instant::now()));
                }
                Err(e) => self.toast = Some((e, Instant::now())),
            }
        }
        if remove {
            let _ = keychain::delete_key(p);
            self.forget_models_for(p);
        }
    }

    // -------------------------------------------------------------- assistants

    /// The MCP server: what it is in one line, two switches, and the two apps it can be connected to.
    fn tab_assistants(&mut self, ui: &mut Ui) {
        use crate::widgets::{srow, text_link};
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, tr(lang, "set_assistants"), tr(lang, "set_assistants_sub"));
        let Some(ws) = &self.ws else { return };
        let bin = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("notecraft-mcp")))
            .unwrap_or_else(|| "notecraft-mcp".into());
        let root = ws.root.display().to_string();
        let bin_s = bin.display().to_string();
        let bin_exists = bin.exists();
        let opts_id = egui::Id::unique("mcp-options");
        let mut opts: McpOptions = ui.data_mut(|d| d.get_temp(opts_id)).unwrap_or_default();

        // two switches: may the apps edit, and may they delete
        let mut write = opts.level == McpLevel::Standard;
        srow(ui, &pal, tr(lang, "mcp_write_title"), tr(lang, "mcp_write_hint"), None, |ui| {
            toggle(ui, &pal, &mut write);
        });
        opts.level = if write { McpLevel::Standard } else { McpLevel::Readonly };
        if !write {
            opts.delete = false;
        }
        srow(ui, &pal, tr(lang, "mcp_allow_delete"), tr(lang, "mcp_allow_delete_hint"), None, |ui| {
            ui.add_enabled_ui(write, |ui| {
                toggle(ui, &pal, &mut opts.delete);
            });
        });
        ui.data_mut(|d| d.insert_temp(opts_id, opts.clone()));

        let mut extra: Vec<String> = Vec::new();
        match opts.level {
            McpLevel::Readonly => extra.push("--read-only".into()),
            McpLevel::Standard if opts.delete => extra.push("--allow-delete".into()),
            McpLevel::Standard => {}
        }
        if !opts.scope.trim().is_empty() {
            extra.push("--scope".into());
            extra.push(opts.scope.trim().to_string());
        }
        let flags: String =
            extra.iter().map(|a| if a.starts_with("--") { format!(" {a}") } else { format!(" \"{a}\"") }).collect();
        let code_cmd = format!("claude mcp add --scope user notecraft -- \"{bin_s}\" --workspace \"{root}\"{flags}");
        let mut args = vec!["--workspace".to_string(), root];
        args.extend(extra);

        // Claude Desktop: read the configuration at most once a second
        let now = ui.input(|i| i.time);
        let cache_id = egui::Id::unique("mcp-desktop-state");
        let key = format!("{bin_s}|{}", args.join("|"));
        let cached: Option<(String, DesktopState, f64)> = ui.data(|d| d.get_temp(cache_id));
        let state = match cached {
            Some((k, s, t)) if k == key && now - t < 1.0 => s,
            _ => {
                let s = mcp_setup::state(&bin_s, &args);
                ui.data_mut(|d| d.insert_temp(cache_id, (key.clone(), s, now)));
                s
            }
        };

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(tr(lang, "mcp_apps").to_uppercase()).size(11.0).color(pal.text_faint));
        });
        ui.add_space(2.0);
        let (mut add, mut remove) = (false, false);
        let desktop_sub = match state {
            DesktopState::Connected | DesktopState::Different => tr(lang, "mcp_desktop_connected"),
            DesktopState::NotAdded => tr(lang, "mcp_desktop_notadded"),
            DesktopState::Missing => tr(lang, "mcp_desktop_missing"),
        };
        let can = bin_exists && state != DesktopState::Missing;
        srow(ui, &pal, tr(lang, "mcp_desktop_title"), desktop_sub, None, |ui| match state {
            DesktopState::Connected => remove = text_link(ui, &pal, tr(lang, "mcp_desktop_remove")).clicked(),
            DesktopState::Different => {
                remove = text_link(ui, &pal, tr(lang, "mcp_desktop_remove")).clicked();
                add = text_link(ui, &pal, tr(lang, "mcp_desktop_update")).clicked();
            }
            _ => {
                ui.add_enabled_ui(can, |ui| {
                    add = text_link(ui, &pal, tr(lang, "mcp_connect")).clicked();
                });
            }
        });
        let (mut code_add, mut code_copy) = (false, false);
        let cli = mcp_setup::claude_cli().is_some();
        srow(ui, &pal, tr(lang, "mcp_code_title"), if cli { "" } else { tr(lang, "mcp_code_missing") }, None, |ui| {
            ui.add_enabled_ui(bin_exists && cli, |ui| {
                code_add = text_link(ui, &pal, tr(lang, "mcp_connect")).clicked();
            });
            code_copy = text_link(ui, &pal, tr(lang, "mcp_code_copy")).clicked();
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(10.0);
            let note = if bin_exists {
                tr(lang, "mcp_footer").to_string()
            } else {
                format!("{} cargo build --release -p notecraft-mcp", tr(lang, "mcp_build_first"))
            };
            ui.label(RichText::new(note).size(12.0).color(pal.text_faint));
        });

        if add {
            match mcp_setup::add(&bin_s, &args) {
                Ok(()) => self.notify(tr(lang, "mcp_desktop_done")),
                Err(e) => self.notify(e),
            }
            ui.data_mut(|d| d.remove::<(String, DesktopState, f64)>(cache_id));
        }
        if remove {
            match mcp_setup::remove() {
                Ok(()) => self.notify(tr(lang, "mcp_desktop_removed")),
                Err(e) => self.notify(e),
            }
            ui.data_mut(|d| d.remove::<(String, DesktopState, f64)>(cache_id));
        }
        if code_copy {
            ui.ctx().copy_text(code_cmd.clone());
            self.toast = Some((tr(lang, "copied").into(), Instant::now()));
        }
        if code_add {
            match mcp_setup::add_to_claude_code(&bin_s, &args) {
                Ok(()) => self.notify(tr(lang, "mcp_code_done")),
                Err(e) => self.notify(e),
            }
        }
    }

    // --------------------------------------------------------------- shortcuts

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

/// What an assistant connected through MCP may do (see `docs/mcp.md`).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum McpLevel {
    Readonly,
    /// The server default: read and write, deletion tools stay off unless ticked.
    #[default]
    Standard,
}

#[derive(Clone, Default)]
struct McpOptions {
    level: McpLevel,
    delete: bool,
    scope: String,
}
