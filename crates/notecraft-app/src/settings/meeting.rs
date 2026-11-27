//! Settings, meeting card: speech engine, models and templates.

use super::*;

impl NcApp {
    /// Speech model, language and note template for `/meeting` blocks: icons for choices, one line each.
    pub(super) fn meeting_card(&mut self, ui: &mut Ui) {
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
        // show a template that exists, not a built-in key left over from before
        self.prefs.meeting.template = self.effective_template();
        srow(ui, &pal, tr(lang, "meeting_template"), "", None, |ui| {
            // the transcription templates are the pages of their group; when none is left the general summary
            // keeps the block working
            let pages = self.ws.as_ref().map(crate::app::note_templates).unwrap_or_default();
            let mut opts: Vec<(String, Glyph, String)> = if pages.is_empty() {
                vec![("summary".to_string(), Glyph::None, crate::meeting::template_label(lang, "summary").to_string())]
            } else {
                pages.into_iter().map(|(k, t)| (k, Glyph::None, t)).collect()
            };
            if self.prefs.meeting.template == "custom" && opts.iter().all(|(k, _, _)| k != "custom") {
                opts.push((
                    "custom".to_string(),
                    Glyph::None,
                    crate::meeting::template_label(lang, "custom").to_string(),
                ));
            }
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
}
