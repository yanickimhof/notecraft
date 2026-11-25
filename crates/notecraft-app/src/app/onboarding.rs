//! The first launch: a short assistant (language, folder, AI, meetings) before the first workspace.
//! Nothing is downloaded or sent from here unless the person clicks the button that says so.

use super::*;

#[derive(Default)]
pub struct Onboard {
    pub step: u8,
    pub folder: Option<PathBuf>,
}

const STEPS: u8 = 4;

impl NcApp {
    /// The assistant shows for someone who has never opened a workspace here.
    pub(super) fn onboarding_due(&self) -> bool {
        !self.prefs.onboarded && !self.prefs.recent.iter().any(|p| p.is_dir())
    }

    fn default_folder() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents").join("NoteCraft"))
    }

    /// Create the folder and open it: the tour page appears in it.
    fn onboarding_finish(&mut self) {
        self.prefs.onboarded = true;
        let dir = self.onboard.folder.clone().or_else(Self::default_folder);
        match dir {
            Some(d) if std::fs::create_dir_all(&d).is_ok() => self.open_workspace(&d, false),
            _ => self.pick_workspace(),
        }
    }

    pub(super) fn onboarding(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let step = self.onboard.step;
        let mut go: Option<u8> = None;
        let mut finish = false;
        let mut pick = false;
        let mut download = false;
        let view = self.meeting_view();
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.12).max(24.0));
            ui.set_max_width(560.0);
            super::chrome::app_icon(ui, 84.0);
            ui.add_space(6.0);
            // progress dots
            ui.label(
                RichText::new((0..STEPS).map(|i| if i <= step { "●" } else { "○" }).collect::<Vec<_>>().join("  "))
                    .size(11.0)
                    .color(pal.text_faint),
            );
            ui.add_space(14.0);
            let big = |t: &str| egui::Button::new(RichText::new(t).size(15.0)).min_size(Vec2::new(340.0, 40.0));
            let line = |ui: &mut egui::Ui, t: &str| {
                ui.label(RichText::new(t).size(14.0).color(pal.text_secondary));
                ui.add_space(6.0);
            };
            match step {
                0 => {
                    ui.label(RichText::new(self.t("welcome_title")).size(28.0).color(pal.text));
                    ui.add_space(8.0);
                    for k in ["welcome_how1", "welcome_how2", "welcome_how3"] {
                        line(ui, self.t(k));
                    }
                    ui.add_space(6.0);
                    if ui
                        .add(
                            egui::Button::new(RichText::new(format!("▶  {}", self.t("demo_watch"))).size(14.0))
                                .min_size(Vec2::new(340.0, 34.0)),
                        )
                        .clicked()
                    {
                        self.demo_at = Some(0);
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(ui.available_width() / 2.0 - 90.0);
                        ui.radio_value(&mut self.prefs.lang, Lang::En, "English");
                        ui.radio_value(&mut self.prefs.lang, Lang::Fr, "Français");
                    });
                    ui.add_space(14.0);
                    if ui.add(big(self.t("onb_next")).fill(pal.accent_soft)).clicked() {
                        go = Some(1);
                    }
                }
                1 => {
                    ui.label(RichText::new(self.t("onb_folder_title")).size(24.0).color(pal.text));
                    ui.add_space(8.0);
                    line(ui, self.t("onb_folder_text"));
                    ui.add_space(8.0);
                    let chosen = self.onboard.folder.clone().or_else(Self::default_folder);
                    if let Some(c) = &chosen {
                        ui.label(RichText::new(c.display().to_string()).size(13.0).color(pal.text).monospace());
                    }
                    ui.add_space(4.0);
                    line(ui, self.t("welcome_create_hint"));
                    ui.add_space(8.0);
                    if ui.add(big(self.t("onb_next")).fill(pal.accent_soft)).clicked() {
                        go = Some(2);
                    }
                    ui.add_space(6.0);
                    if ui.add(big(self.t("welcome_other"))).clicked() {
                        pick = true;
                    }
                    ui.label(RichText::new(self.t("welcome_other_hint")).size(12.0).color(pal.text_faint));
                }
                2 => {
                    ui.label(RichText::new(self.t("onb_ai_title")).size(24.0).color(pal.text));
                    ui.add_space(8.0);
                    for k in ["onb_ai_1", "onb_ai_2", "onb_ai_3"] {
                        line(ui, self.t(k));
                    }
                    ui.add_space(14.0);
                    if ui.add(big(self.t("onb_next")).fill(pal.accent_soft)).clicked() {
                        go = Some(3);
                    }
                }
                _ => {
                    ui.label(RichText::new(self.t("onb_meet_title")).size(24.0).color(pal.text));
                    ui.add_space(8.0);
                    line(ui, self.t("onb_meet_1"));
                    line(ui, self.t("onb_meet_2"));
                    ui.add_space(8.0);
                    if let Some((name, frac)) = &view.download {
                        ui.add(egui::ProgressBar::new(*frac).desired_width(340.0).text(format!(
                            "{} {name} {:.0}%",
                            self.t("meeting_downloading"),
                            frac * 100.0
                        )));
                    } else if view.model_ready {
                        ui.label(
                            RichText::new(format!("✓ {} {}", view.model_label, self.t("meeting_model_ready")))
                                .color(pal.text),
                        );
                    } else if ui
                        .add(big(&format!(
                            "{} ({}, {} MB)",
                            self.t("onb_meet_download"),
                            view.model_label,
                            view.model_mb
                        )))
                        .clicked()
                    {
                        download = true;
                    }
                    ui.add_space(12.0);
                    if ui.add(big(self.t("onb_finish")).fill(pal.accent_soft)).clicked() {
                        finish = true;
                    }
                }
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 90.0);
                if step > 0 && widgets::text_link(ui, &pal, self.t("onb_back")).clicked() {
                    go = Some(step - 1);
                }
                if step < STEPS - 1 && widgets::text_link(ui, &pal, self.t("onb_skip")).clicked() {
                    finish = true;
                }
            });
        });
        if pick {
            if let Some(dir) = rfd::FileDialog::new().set_title(self.t("open_folder")).pick_folder() {
                if is_too_broad(&dir) {
                    let m = self.t("workspace_too_broad").to_string();
                    self.notify(m);
                } else {
                    self.onboard.folder = Some(dir);
                    go = Some(2);
                }
            }
        }
        if download {
            self.start_model_download();
        }
        if let Some(s) = go {
            self.onboard.step = s;
        }
        if finish {
            self.onboarding_finish();
        }
    }
}
