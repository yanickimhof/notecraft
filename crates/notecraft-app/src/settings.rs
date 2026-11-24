//! The settings window: a side list of sections and, on the right, one page of cards per section.
//!
//! Type scale, used everywhere here: page title 20, body 13, hints and headings 12.

use crate::ai_panel::EngineChoice;
use crate::app::{reveal, NcApp, SHORTCUTS};
use crate::i18n::{tr, Lang};
use crate::icons::{self, Icon};
use crate::mcp_setup::{self, DesktopState};
use crate::theme::ThemeChoice;
use crate::widgets::{card, choice_row, hint, row_divider, section, segmented, setting_row, status_pill};
use eframe::egui::{self, FontId, Id, RichText, TextStyle, Ui, Vec2};
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
    Security,
    Ai,
    Assistants,
    Shortcuts,
    About,
}

const TABS: [(Tab, Icon, &str); 7] = [
    (Tab::General, Icon::General, "set_general"),
    (Tab::Data, Icon::Data, "set_data"),
    (Tab::Security, Icon::Security, "set_security"),
    (Tab::Ai, Icon::Ai, "set_ai"),
    (Tab::Assistants, Icon::Assistants, "set_assistants"),
    (Tab::Shortcuts, Icon::Shortcuts, "set_shortcuts"),
    (Tab::About, Icon::About, "set_about"),
];

fn page_title(ui: &mut Ui, pal: &crate::theme::Palette, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(20.0).strong().color(pal.text));
    if !subtitle.is_empty() {
        ui.add_space(2.0);
        hint(ui, pal, subtitle);
    }
    ui.add_space(14.0);
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
            ui.set_max_width(total - 190.0);
            ui.label(RichText::new(title).size(13.0).color(pal.text));
            ui.add(egui::Label::new(RichText::new(tilde(path)).size(12.0).color(pal.text_secondary)).wrap());
            hint(ui, pal, text);
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr(lang, "show_in_finder")).clicked() {
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
        let height = (ctx.content_rect().height() - 110.0).clamp(420.0, 640.0);
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
            w.inactive.bg_stroke = egui::Stroke::new(1.0, pal.border);
            ui.set_width(800.0);
            ui.set_height(height);
            ui.horizontal_top(|ui| {
                ui.allocate_ui(Vec2::new(190.0, height), |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add_space(4.0);
                        for (tab, icon, key) in TABS {
                            if icons::nav_item(ui, icon, tr(lang, key), self.settings_tab == tab, &pal).clicked() {
                                self.settings_tab = tab;
                            }
                        }
                    });
                });
                ui.add_space(10.0);
                ui.allocate_ui(Vec2::new(580.0, height), |ui| {
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("settings-page", self.settings_tab as u8))
                            .max_height(height - 48.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(560.0);
                                match self.settings_tab {
                                    Tab::General => self.tab_general(ui),
                                    Tab::Data => self.tab_data(ui),
                                    Tab::Security => self.tab_security(ui),
                                    Tab::Ai => self.tab_ai(ui),
                                    Tab::Assistants => self.tab_assistants(ui),
                                    Tab::Shortcuts => self.tab_shortcuts(ui),
                                    Tab::About => self.tab_about(ui),
                                }
                                ui.add_space(12.0);
                            });
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                            ui.set_max_width(560.0);
                            // a centred layout keeps the label in the middle of the button
                            let r = ui.allocate_ui_with_layout(
                                Vec2::new(90.0, 30.0),
                                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                                |ui| ui.button(tr(lang, "done")),
                            );
                            if r.inner.clicked() {
                                close = true;
                            }
                        });
                    });
                });
            });
        });
        if close || modal.should_close() {
            self.settings_open = false;
        }
    }

    // ----------------------------------------------------------------- general

    fn tab_general(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, tr(lang, "set_general"), "");
        section(ui, &pal, tr(lang, "appearance"));
        card(ui, &pal, |ui| {
            setting_row(ui, &pal, tr(lang, "theme"), "", |ui| {
                let opts = [
                    (ThemeChoice::System, tr(lang, "theme_auto").to_string()),
                    (ThemeChoice::Light, tr(lang, "light").to_string()),
                    (ThemeChoice::Dark, tr(lang, "dark").to_string()),
                ];
                segmented(ui, &pal, &mut self.prefs.theme, &opts);
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "text_size"), "", |ui| {
                let opts = [
                    (85u32, "85%".to_string()),
                    (100, "100%".to_string()),
                    (115, "115%".to_string()),
                    (130, "130%".to_string()),
                ];
                segmented(ui, &pal, &mut self.prefs.text_percent, &opts);
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "language"), "", |ui| {
                let opts = [(Lang::En, "English".to_string()), (Lang::Fr, "Français".to_string())];
                segmented(ui, &pal, &mut self.prefs.lang, &opts);
            });
        });
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
        section(ui, &pal, tr(lang, "backups"));
        card(ui, &pal, |ui| self.backup_section(ui));
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

    fn tab_security(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, tr(lang, "set_security"), tr(lang, "set_security_sub"));
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

    fn tab_ai(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, tr(lang, "set_ai"), tr(lang, "set_ai_sub"));
        // model lists change (a model is installed, a new one is released): ask again when the page
        // opens and then at most once a minute while it stays open
        if self.ai.checked.is_none_or(|t| t.elapsed().as_secs() >= 60) {
            self.ai.checked = Some(Instant::now());
            if self.ai.models_loading.is_none() {
                self.ai.models_error = None;
                self.refresh_models();
            }
            if self.ai.claude_loading.is_none() {
                self.refresh_claude_models();
            }
            if self.ai.openai_loading.is_none() {
                self.refresh_openai_models();
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
        section(ui, &pal, tr(lang, "ai_engine"));
        card(ui, &pal, |ui| {
            setting_row(ui, &pal, tr(lang, "ai_engine_pick"), "", |ui| {
                let opts = [
                    (EngineChoice::Ollama, tr(lang, "ai_ollama_short").to_string()),
                    (EngineChoice::Claude, tr(lang, "ai_claude_short").to_string()),
                    (EngineChoice::OpenAi, tr(lang, "ai_openai_short").to_string()),
                ];
                segmented(ui, &pal, &mut self.prefs.ai.engine, &opts);
            });
            ui.add_space(8.0);
            if self.prefs.ai.leaves_this_mac() {
                status_pill(ui, &pal, tr(lang, crate::ai_panel::cloud_note_key(self.prefs.ai.engine)), "orange");
            } else {
                status_pill(ui, &pal, tr(lang, "ai_local_note"), "green");
            }
        });
        ui.add_space(14.0);
        match self.prefs.ai.engine.provider() {
            None => self.ollama_card(ui),
            Some(p) => self.cloud_card(ui, p),
        }
        ui.add_space(14.0);
        self.meeting_card(ui);
    }

    /// Speech model, language and note template for `/meeting` blocks.
    fn meeting_card(&mut self, ui: &mut Ui) {
        use notecraft_meeting::MODELS;
        let pal = self.pal;
        let lang = self.prefs.lang;
        section(ui, &pal, tr(lang, "set_meeting"));
        card(ui, &pal, |ui| {
            for (n, m) in MODELS.iter().enumerate() {
                if n > 0 {
                    row_divider(ui, &pal);
                }
                let chosen = self.prefs.meeting.model == m.id;
                let note = tr(lang, &format!("model_{}", m.note));
                let hint = format!("{note}, {} MB", m.size_mb);
                let installed = m.installed();
                let downloading = self.meeting_view().download.is_some();
                let title = format!("{}{}", m.label, if chosen { "  ✓" } else { "" });
                setting_row(ui, &pal, &title, &hint, |ui| {
                    if installed {
                        if !chosen && ui.button(tr(lang, "choose")).clicked() {
                            self.prefs.meeting.model = m.id.to_string();
                        }
                        if ui.button(tr(lang, "delete")).clicked() {
                            let _ = m.delete();
                        }
                    } else if ui.add_enabled(!downloading, egui::Button::new(tr(lang, "meeting_download"))).clicked() {
                        self.prefs.meeting.model = m.id.to_string();
                        self.meeting_action(crate::meeting::MeetingAction::DownloadModel);
                    }
                });
            }
            row_divider(ui, &pal);
            {
                use notecraft_meeting::models::{parakeet_delete, parakeet_installed, PARAKEET_MB};
                let installed = parakeet_installed();
                let downloading = self.meeting_view().download.is_some();
                let hint_text = format!("{}, {PARAKEET_MB} MB", tr(lang, "parakeet_note"));
                setting_row(ui, &pal, "Parakeet", &hint_text, |ui| {
                    if installed {
                        if ui.button(tr(lang, "delete")).clicked() {
                            let _ = parakeet_delete();
                        }
                    } else if ui.add_enabled(!downloading, egui::Button::new(tr(lang, "meeting_download"))).clicked() {
                        self.meeting_action(crate::meeting::MeetingAction::DownloadParakeet);
                    }
                });
            }
            if let Some((name, frac)) = self.meeting_view().download {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::ProgressBar::new(frac).desired_width(260.0).text(format!("{name} {:.0}%", frac * 100.0)),
                    );
                    if ui.button(tr(lang, "cancel")).clicked() {
                        self.meeting_action(crate::meeting::MeetingAction::CancelDownload);
                    }
                });
            }
            hint(ui, &pal, tr(lang, "meeting_speech_hint"));
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "meeting_source"), tr(lang, "meeting_source_hint"), |ui| {
                let label = |id: &str| match id {
                    "system" => tr(lang, "meeting_source_system"),
                    "both" => tr(lang, "meeting_source_both"),
                    _ => tr(lang, "meeting_source_mic"),
                };
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt("meeting-source-setting")
                        .width(260.0)
                        .selected_text(label(&self.prefs.meeting.source))
                        .show_ui(ui, |ui| {
                            for id in ["mic", "system", "both"] {
                                ui.selectable_value(&mut self.prefs.meeting.source, id.to_string(), label(id));
                            }
                        }),
                    tr(lang, "combo_source"),
                );
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "meeting_microphone"), "", |ui| {
                let (devices, default) = notecraft_meeting::input_devices();
                let default_label = match &default {
                    Some(d) => format!("{} ({d})", tr(lang, "meeting_mic_default")),
                    None => tr(lang, "meeting_mic_default").to_string(),
                };
                let shown = if self.prefs.meeting.microphone.is_empty() {
                    default_label.clone()
                } else {
                    self.prefs.meeting.microphone.clone()
                };
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt("meeting-microphone").width(260.0).selected_text(shown).show_ui(
                        ui,
                        |ui| {
                            ui.selectable_value(&mut self.prefs.meeting.microphone, String::new(), default_label);
                            for d in devices {
                                ui.selectable_value(&mut self.prefs.meeting.microphone, d.clone(), d);
                            }
                        },
                    ),
                    tr(lang, "combo_mic"),
                );
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "meeting_spoken_language"), "", |ui| {
                let opts = [
                    ("auto".to_string(), tr(lang, "meeting_lang_auto").to_string()),
                    ("en".to_string(), tr(lang, "meeting_lang_en").to_string()),
                    ("fr".to_string(), tr(lang, "meeting_lang_fr").to_string()),
                ];
                let current = opts.iter().find(|(k, _)| *k == self.prefs.meeting.language).map(|(_, l)| l.clone());
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt("meeting-language")
                        .width(220.0)
                        .selected_text(current.unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (k, l) in &opts {
                                ui.selectable_value(&mut self.prefs.meeting.language, k.clone(), l);
                            }
                        }),
                    tr(lang, "combo_language"),
                );
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "meeting_engine"), tr(lang, "meeting_engine_sub"), |ui| {
                let label = |id: &str| match id {
                    "whisper" => tr(lang, "engine_whisper"),
                    "parakeet" => tr(lang, "engine_parakeet"),
                    _ => tr(lang, "engine_auto"),
                };
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt("meeting-engine-setting")
                        .width(260.0)
                        .selected_text(label(&self.prefs.meeting.engine))
                        .show_ui(ui, |ui| {
                            for id in ["auto", "whisper", "parakeet"] {
                                ui.selectable_value(&mut self.prefs.meeting.engine, id.to_string(), label(id));
                            }
                        }),
                    tr(lang, "meeting_engine"),
                );
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "meeting_vocabulary"), tr(lang, "meeting_vocabulary_sub"), |_| {});
            ui.add(
                egui::TextEdit::multiline(&mut self.prefs.meeting.vocabulary)
                    .hint_text(tr(lang, "meeting_vocabulary_hint"))
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "meeting_template"), "", |ui| {
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt("meeting-template-setting")
                        .width(220.0)
                        .selected_text(crate::meeting::template_label(lang, &self.prefs.meeting.template))
                        .show_ui(ui, |ui| {
                            for key in notecraft_ai::NOTE_TEMPLATES {
                                let l = crate::meeting::template_label(lang, key);
                                ui.selectable_value(&mut self.prefs.meeting.template, (*key).to_string(), l);
                            }
                        }),
                    tr(lang, "combo_template"),
                );
            });
            if self.prefs.meeting.template == "custom" {
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::multiline(&mut self.prefs.meeting.custom)
                        .hint_text(tr(lang, "meeting_custom_hint"))
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
            }
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "meeting_auto_notes"), "", |ui| {
                ui.checkbox(&mut self.prefs.meeting.auto_notes, "");
            });
            ui.add_space(8.0);
            if self.prefs.ai.leaves_this_mac() {
                status_pill(ui, &pal, tr(lang, "meeting_privacy_cloud"), "orange");
            } else {
                status_pill(ui, &pal, tr(lang, "meeting_privacy_local"), "green");
            }
            hint(ui, &pal, tr(lang, "meeting_consent"));
        });
    }

    fn ollama_card(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let active_local = self.prefs.ai.engine == EngineChoice::Ollama;
        let local_url = notecraft_ai::is_local_url(&self.prefs.ai.ollama_url);
        if (active_local || local_url)
            && self.ai.models.is_empty()
            && self.ai.models_loading.is_none()
            && self.ai.models_error.is_none()
        {
            self.refresh_models();
        }
        let installed = notecraft_ai::ollama_installed();
        let state = if self.ai.models_error.is_some() {
            if installed {
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
        };
        let mut refresh = false;
        section(ui, &pal, tr(lang, "ai_ollama"));
        card(ui, &pal, |ui| {
            setting_row(ui, &pal, "Ollama", tr(lang, "ai_ollama_sub"), |ui| match state {
                OllamaState::Checking => {
                    ui.spinner();
                }
                OllamaState::Ready => status_pill(ui, &pal, tr(lang, "ollama_ready"), "green"),
                OllamaState::NoModel => status_pill(ui, &pal, tr(lang, "ollama_no_model"), "orange"),
                OllamaState::Stopped => status_pill(ui, &pal, tr(lang, "ollama_stopped"), "orange"),
                OllamaState::Missing => status_pill(ui, &pal, tr(lang, "ollama_missing"), "gray"),
            });
            row_divider(ui, &pal);
            match state {
                OllamaState::Missing => {
                    hint(ui, &pal, tr(lang, "ollama_missing_text"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr(lang, "ollama_download")).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab("https://ollama.com/download"));
                        }
                        refresh = ui.button(tr(lang, "ollama_refresh")).clicked();
                    });
                }
                OllamaState::Stopped => {
                    hint(ui, &pal, tr(lang, "ollama_stopped_text"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr(lang, "ollama_launch")).clicked() {
                            let _ = std::process::Command::new("open").args(["-a", "Ollama"]).spawn();
                        }
                        refresh = ui.button(tr(lang, "ollama_refresh")).clicked();
                    });
                }
                OllamaState::NoModel => {
                    hint(ui, &pal, tr(lang, "ai_no_models"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr(lang, "ollama_copy_pull")).clicked() {
                            ui.ctx().copy_text("ollama pull llama3.2".into());
                            self.toast = Some((tr(lang, "copied").into(), Instant::now()));
                        }
                        refresh = ui.button(tr(lang, "ollama_refresh")).clicked();
                    });
                }
                OllamaState::Ready | OllamaState::Checking => {
                    setting_row(ui, &pal, tr(lang, "ai_model"), "", |ui| {
                        refresh = ui.button(tr(lang, "ollama_refresh")).clicked();
                        let current = if self.prefs.ai.ollama_model.is_empty() {
                            "—".to_string()
                        } else {
                            self.prefs.ai.ollama_model.clone()
                        };
                        crate::widgets::name_combo(
                            egui::ComboBox::from_id_salt("ollama-model").selected_text(current).width(180.0).show_ui(
                                ui,
                                |ui| {
                                    for m in self.ai.models.clone() {
                                        ui.selectable_value(&mut self.prefs.ai.ollama_model, m.clone(), m);
                                    }
                                },
                            ),
                            tr(lang, "combo_model"),
                        );
                    });
                }
            }
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "ai_address"), "", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.prefs.ai.ollama_url).desired_width(220.0));
            });
        });
        if refresh {
            self.ai.models_error = None;
            self.refresh_models();
        }
    }

    /// Key and model of a cloud service (Claude or OpenAI): the same three rows for both.
    fn cloud_card(&mut self, ui: &mut Ui, provider: keychain::Provider) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let openai = provider == keychain::Provider::OpenAi;
        let cached = if openai { self.ai.openai_key_status.clone() } else { self.ai.key_status.clone() };
        let status = match cached {
            Some(s) => s,
            None => {
                let s = if keychain::has_key(provider) {
                    keychain::load_key(provider).map(|k| keychain::masked(&k)).unwrap_or_default()
                } else if keychain::load_key(provider).is_some() {
                    "env".to_string()
                } else {
                    String::new()
                };
                if openai {
                    self.ai.openai_key_status = Some(s.clone());
                } else {
                    self.ai.key_status = Some(s.clone());
                }
                s
            }
        };
        let mut save = false;
        let mut remove = false;
        let mut refresh = false;
        let (models, loading, error) = if openai {
            (self.ai.openai_models.clone(), self.ai.openai_loading.is_some(), self.ai.openai_error.clone())
        } else {
            (self.ai.claude_models.clone(), self.ai.claude_loading.is_some(), self.ai.claude_error.clone())
        };
        let title = if openai { "ai_key_title_openai" } else { "ai_key_title" };
        section(ui, &pal, tr(lang, title));
        card(ui, &pal, |ui| {
            setting_row(ui, &pal, tr(lang, "ai_key"), "", |ui| {
                let can = !self.ai.key_input.trim().is_empty();
                if ui.add_enabled(can, egui::Button::new(tr(lang, "save_keychain"))).clicked() {
                    save = true;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.ai.key_input)
                        .password(true)
                        .desired_width(220.0)
                        .hint_text(if openai { "sk-…" } else { "sk-ant-…" }),
                );
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "ai_key_state"), "", |ui| {
                if !status.is_empty()
                    && status != "env"
                    && ui.link(RichText::new(tr(lang, "remove_key")).color(pal.danger)).clicked()
                {
                    remove = true;
                }
                let text = if status.is_empty() {
                    tr(lang, "ai_key_none").to_string()
                } else if status == "env" {
                    tr(lang, if openai { "ai_key_env_openai" } else { "ai_key_env" }).to_string()
                } else {
                    format!("{} · {status}", tr(lang, "ai_key_stored"))
                };
                ui.label(RichText::new(text).color(pal.text_secondary));
            });
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "ai_model"), "", |ui| {
                let chosen = if openai { &mut self.prefs.ai.openai_model } else { &mut self.prefs.ai.claude_model };
                // the account's own list once it answers; until then (no key yet, offline) a short
                // list of well-known models, so the choice is always a menu
                if loading {
                    ui.spinner();
                } else if !models.is_empty() && ui.button(tr(lang, "ollama_refresh")).clicked() {
                    refresh = true;
                }
                let current = chosen.clone();
                let mut all: Vec<String> = if models.is_empty() {
                    let fallback: &[&str] = if openai {
                        notecraft_ai::OPENAI_FALLBACK_MODELS
                    } else {
                        notecraft_ai::CLAUDE_FALLBACK_MODELS
                    };
                    fallback.iter().map(|m| m.to_string()).collect()
                } else {
                    models.clone()
                };
                // a chosen model the list no longer shows stays selectable
                if !all.contains(&current) {
                    all.insert(0, current.clone());
                }
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt(if openai { "openai-model" } else { "claude-model" })
                        .selected_text(current)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for m in all {
                                ui.selectable_value(chosen, m.clone(), m);
                            }
                        }),
                    tr(lang, "combo_model"),
                );
            });
            if let Some(e) = &error {
                hint(ui, &pal, e);
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            hint(ui, &pal, tr(lang, "ai_key_help"));
            ui.hyperlink_to(
                RichText::new(tr(lang, "ai_key_get")).size(12.0),
                if openai {
                    "https://platform.openai.com/api-keys"
                } else {
                    "https://console.anthropic.com/settings/keys"
                },
            );
        });
        if refresh {
            if openai {
                self.refresh_openai_models();
            } else {
                self.refresh_claude_models();
            }
        }
        if save {
            match keychain::save_key(provider, self.ai.key_input.trim()) {
                Ok(()) => {
                    self.ai.key_input.clear();
                    if openai {
                        self.ai.openai_key_status = None;
                        self.refresh_openai_models();
                    } else {
                        self.ai.key_status = None;
                        self.refresh_claude_models();
                    }
                    self.toast = Some((tr(lang, "ai_key_saved").into(), Instant::now()));
                }
                Err(e) => self.toast = Some((e, Instant::now())),
            }
        }
        if remove {
            let _ = keychain::delete_key(provider);
            if openai {
                self.ai.openai_key_status = None;
                self.ai.openai_models.clear();
                self.ai.openai_error = None;
            } else {
                self.ai.key_status = None;
                self.ai.claude_models.clear();
                self.ai.claude_error = None;
            }
        }
    }

    // -------------------------------------------------------------- assistants

    fn tab_assistants(&mut self, ui: &mut Ui) {
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

        section(ui, &pal, tr(lang, "mcp_who"));
        let levels = [
            (McpLevel::Readonly, "mcp_level_ro", "mcp_level_ro_desc"),
            (McpLevel::Standard, "mcp_level_rw", "mcp_level_rw_desc"),
        ];
        for (level, title, desc) in levels {
            if choice_row(ui, &pal, opts.level == level, tr(lang, title), tr(lang, desc)).clicked() {
                opts.level = level;
            }
            ui.add_space(4.0);
        }
        ui.add_space(4.0);
        card(ui, &pal, |ui| {
            // deleting only makes sense when the assistant may write
            let can_delete = opts.level == McpLevel::Standard;
            if !can_delete {
                opts.delete = false;
            }
            ui.add_enabled_ui(can_delete, |ui| {
                ui.checkbox(&mut opts.delete, tr(lang, "mcp_allow_delete"));
            });
            hint(ui, &pal, tr(lang, "mcp_allow_delete_hint"));
            row_divider(ui, &pal);
            setting_row(ui, &pal, tr(lang, "mcp_scope_title"), tr(lang, "mcp_scope_text"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut opts.scope)
                        .desired_width(220.0)
                        .hint_text(tr(lang, "mcp_scope_placeholder")),
                );
            });
        });
        ui.add_space(4.0);
        hint(ui, &pal, tr(lang, "mcp_permissions_hint"));
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
        ui.add_space(14.0);
        section(ui, &pal, tr(lang, "mcp_desktop_title"));
        let (mut add, mut remove) = (false, false);
        card(ui, &pal, |ui| {
            setting_row(ui, &pal, tr(lang, "mcp_desktop_title"), "", |ui| {
                let (text, color) = match state {
                    DesktopState::Connected => (tr(lang, "mcp_desktop_connected"), "green"),
                    DesktopState::Different => (tr(lang, "mcp_desktop_different"), "orange"),
                    DesktopState::NotAdded => (tr(lang, "mcp_desktop_notadded"), "gray"),
                    DesktopState::Missing => (tr(lang, "mcp_desktop_missing"), "gray"),
                };
                status_pill(ui, &pal, text, color);
            });
            ui.add_space(8.0);
            if !bin_exists {
                hint(ui, &pal, &format!("{} cargo build --release -p notecraft-mcp", tr(lang, "mcp_build_first")));
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                let can = bin_exists && state != DesktopState::Missing;
                match state {
                    DesktopState::Connected => {
                        remove = ui.button(tr(lang, "mcp_desktop_remove")).clicked();
                    }
                    DesktopState::Different => {
                        add = ui.add_enabled(can, egui::Button::new(tr(lang, "mcp_desktop_update"))).clicked();
                        remove = ui.button(tr(lang, "mcp_desktop_remove")).clicked();
                    }
                    _ => {
                        add = ui.add_enabled(can, egui::Button::new(tr(lang, "mcp_desktop_add"))).clicked();
                    }
                }
            });
            ui.add_space(6.0);
            hint(ui, &pal, tr(lang, "mcp_desktop_hint"));
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

        ui.add_space(14.0);
        let mut code_add = false;
        section(ui, &pal, tr(lang, "mcp_code_title"));
        card(ui, &pal, |ui| {
            egui::Frame::NONE
                .fill(pal.code_bg)
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&code_cmd).monospace().color(pal.text)).wrap());
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can = bin_exists && mcp_setup::claude_cli().is_some();
                if ui.add_enabled(can, egui::Button::new(tr(lang, "mcp_code_add"))).clicked() {
                    code_add = true;
                }
                if ui.button(tr(lang, "mcp_code_copy")).clicked() {
                    ui.ctx().copy_text(code_cmd.clone());
                    self.toast = Some((tr(lang, "copied").into(), Instant::now()));
                }
            });
            ui.add_space(6.0);
            hint(ui, &pal, tr(lang, "mcp_code_hint"));
        });
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
        page_title(ui, &pal, tr(lang, "shortcuts"), "");
        card(ui, &pal, |ui| {
            egui::Grid::new("shortcuts").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
                for (k, v) in SHORTCUTS {
                    ui.label(RichText::new(*k).monospace().color(pal.text));
                    ui.label(RichText::new(*v).color(pal.text_secondary));
                    ui.end_row();
                }
            });
        });
    }

    // ------------------------------------------------------------------- about

    fn tab_about(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, "NøteCraft", tr(lang, "about_text"));
        let mut copy = false;
        card(ui, &pal, |ui| {
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
