//! Reminders: a window to set one on a page, and a check that fires the ones that are due
//! (a macOS notification and a message in the app).

use crate::app::NcApp;
use eframe::egui::{self, RichText};
use notecraft_core::reminder::{self, Preset};
use std::time::{Duration, Instant};

pub struct ReminderEdit {
    pub page: String,
    pub text: String,
}

#[cfg(target_os = "macos")]
fn notify_os(title: &str, body: &str) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display notification \"{}\" with title \"{}\"", esc(body), esc(title));
    let _ = std::process::Command::new("/usr/bin/osascript").arg("-e").arg(script).spawn();
}

#[cfg(not(target_os = "macos"))]
fn notify_os(_title: &str, _body: &str) {}

impl NcApp {
    /// Open the reminder window for the current page.
    pub fn edit_reminder(&mut self, page: &str) {
        let current =
            self.ws.as_ref().and_then(|w| w.pages.get(page)).map(|p| p.meta.reminder.clone()).unwrap_or_default();
        let text = if current.is_empty() { reminder::preset(Preset::Tomorrow, &jiff::Zoned::now()) } else { current };
        self.reminder_edit = Some(ReminderEdit { page: page.to_string(), text });
    }

    /// Fire the reminders that are due; checked every few seconds, and on start for the ones missed.
    pub fn poll_reminders(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_secs(20));
        if self.last_reminder_check.elapsed() < Duration::from_secs(15) {
            return;
        }
        self.last_reminder_check = Instant::now();
        let now = reminder::now_local();
        let due: Vec<(String, String, String)> = match self.ws.as_ref() {
            Some(ws) => ws.reminders().into_iter().filter(|(_, _, when)| reminder::is_due(when, &now)).collect(),
            None => return,
        };
        for (id, title, _) in due {
            // clear first, so a failure to save cannot make it ring every 15 seconds
            if let Some(ws) = self.ws.as_mut() {
                let _ = ws.set_reminder(&id, "");
            }
            let name = if title.is_empty() { self.t("untitled").to_string() } else { title };
            notify_os("NøteCraft", &name);
            self.notify(format!("⏰ {name}"));
        }
    }

    pub fn reminder_window(&mut self, ctx: &egui::Context) {
        let Some(mut edit) = self.reminder_edit.take() else { return };
        let mut keep = true;
        let mut save = false;
        let mut clear = false;
        let lang = self.prefs.lang;
        egui::Window::new(crate::i18n::tr(lang, "remind_me"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let now = jiff::Zoned::now();
                ui.horizontal(|ui| {
                    for (key, p) in [
                        ("in_one_hour", Preset::InOneHour),
                        ("tomorrow_9", Preset::Tomorrow),
                        ("next_monday_9", Preset::NextMonday),
                    ] {
                        if ui.button(crate::i18n::tr(lang, key)).clicked() {
                            edit.text = reminder::preset(p, &now);
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, "reminder_format")).size(12.0).color(self.pal.text_secondary),
                );
                let r = ui.add(egui::TextEdit::singleline(&mut edit.text).desired_width(220.0));
                let valid = reminder::normalize(&edit.text).is_some();
                if !valid {
                    ui.label(
                        RichText::new(crate::i18n::tr(lang, "reminder_invalid")).size(12.0).color(self.pal.danger),
                    );
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid {
                    save = true;
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, egui::Button::new(crate::i18n::tr(lang, "save"))).clicked() {
                        save = true;
                    }
                    if ui.button(crate::i18n::tr(lang, "reminder_clear")).clicked() {
                        clear = true;
                    }
                    if ui.button(crate::i18n::tr(lang, "cancel")).clicked() {
                        keep = false;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            keep = false;
        }
        if save || clear {
            let when = if clear { String::new() } else { edit.text.clone() };
            if let Some(ws) = self.ws.as_mut() {
                match ws.set_reminder(&edit.page, &when) {
                    Ok(()) => {
                        if !when.is_empty() {
                            let msg = format!("⏰ {}", reminder::normalize(&when).unwrap_or(when));
                            self.notify(msg);
                        }
                    }
                    Err(e) => self.notify(e.to_string()),
                }
            }
            keep = false;
        }
        if keep {
            self.reminder_edit = Some(edit);
        }
    }

    /// Every reminder of the workspace, soonest first; a click opens the page.
    pub fn reminders_list(&mut self, ctx: &egui::Context) {
        if !self.reminders_open {
            return;
        }
        let lang = self.prefs.lang;
        let list = self.ws.as_ref().map(|w| w.reminders()).unwrap_or_default();
        let now = reminder::now_local();
        let mut open_page: Option<String> = None;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new(crate::i18n::tr(lang, "reminders"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                if list.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::tr(lang, "reminders_none"))
                            .size(13.0)
                            .color(self.pal.text_secondary),
                    );
                }
                for (id, title, when) in &list {
                    let late = reminder::is_due(when, &now);
                    ui.horizontal(|ui| {
                        let color = if late { self.pal.danger } else { self.pal.text_secondary };
                        ui.label(RichText::new(when).monospace().size(12.5).color(color));
                        let label = if title.is_empty() { "Untitled" } else { title.as_str() };
                        if ui.link(label).clicked() {
                            open_page = Some(id.clone());
                        }
                    });
                }
                ui.add_space(8.0);
                if ui.button(crate::i18n::tr(lang, "cancel")).clicked() {
                    close = true;
                }
            });
        if let Some(id) = open_page {
            self.navigate(crate::app::Target::Doc(notecraft_core::workspace::DocId::Page(id)));
            close = true;
        }
        if close {
            self.reminders_open = false;
        }
    }
}
