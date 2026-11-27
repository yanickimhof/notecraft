//! The card of a `/meeting` block: record button, source / language / microphone / template menus,
//! live state, speakers, errors. It only draws and reports what was asked (`Action::Meeting`).

use super::*;

/// Draws the card of the meeting block `i` and queues what the user asked on `out`.
pub(super) fn meeting_card(
    ui: &mut Ui,
    blocks: &mut [Block],
    i: usize,
    width: f32,
    env: &Env,
    out: &mut Output,
) -> egui::Response {
    let pal = env.pal;
    let bid = blocks[i].id;
    let lang = env.lang;
    let v = env.meeting;
    let loc = crate::meeting::locate(blocks, i);
    let (has_transcript, has_notes) = (loc.transcript.is_some(), !loc.notes.is_empty());
    let recording_here = v.recording.filter(|r| r.0 == bid);
    let finishing_here = v.finishing == Some(bid);
    let writing_here = v.writing.as_ref().filter(|w| w.0 == bid).map(|w| w.1.clone());
    let other_busy = (v.recording.is_some() && recording_here.is_none())
        || (v.finishing.is_some() && !finishing_here)
        || (v.writing.is_some() && writing_here.is_none());
    let mut act: Option<crate::meeting::MeetingAction> = None;
    let r = egui::Frame::NONE
        .fill(pal.callout_bg)
        .stroke(Stroke::new(1.0, pal.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width - 30.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🎙").size(18.0));
                    let te = egui::TextEdit::singleline(&mut blocks[i].text)
                        .hint_text(tr(lang, "meeting_title_hint"))
                        .frame(egui::Frame::NONE)
                        .font(FontId::proportional(16.0))
                        .desired_width(ui.available_width());
                    if ui.add(te).changed() {
                        out.changed = true;
                    }
                });
                ui.add_space(6.0);
                if let Some((_, level, elapsed, ready)) = recording_here {
                    ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 5.0, pal.danger);
                        ui.label(RichText::new(crate::meeting::clock(elapsed)).monospace().color(pal.text));
                        let (bar, _) = ui.allocate_exact_size(Vec2::new(90.0, 6.0), Sense::hover());
                        ui.painter().rect_filled(bar, egui::CornerRadius::same(3), pal.border);
                        let mut fill = bar;
                        fill.set_width(bar.width() * (level * 6.0).clamp(0.0, 1.0));
                        ui.painter().rect_filled(fill, egui::CornerRadius::same(3), pal.accent);
                        if !ready {
                            ui.label(RichText::new(tr(lang, "meeting_loading_model")).size(12.0).color(pal.text_faint));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let stop =
                                egui::Button::new(RichText::new(tr(lang, "meeting_stop")).color(egui::Color32::WHITE))
                                    .fill(pal.danger);
                            if ui.add(stop).clicked() {
                                act = Some(crate::meeting::MeetingAction::Stop);
                            }
                        });
                    });
                    ui.label(RichText::new(tr(lang, "meeting_live_hint")).size(12.0).color(pal.text_faint));
                } else if finishing_here {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(RichText::new(tr(lang, "meeting_finishing")).color(pal.text_secondary));
                    });
                } else if let Some(engine) = &writing_here {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(
                            RichText::new(format!("{} {engine}…", tr(lang, "meeting_writing")))
                                .color(pal.text_secondary),
                        );
                    });
                } else if let Some((name, frac)) = &v.download {
                    ui.horizontal(|ui| {
                        ui.add(egui::ProgressBar::new(*frac).desired_width(220.0).text(format!(
                            "{} {name} {:.0}%",
                            tr(lang, "meeting_downloading"),
                            frac * 100.0
                        )));
                        if ui.button(tr(lang, "cancel")).clicked() {
                            act = Some(crate::meeting::MeetingAction::CancelDownload);
                        }
                    });
                } else if !v.model_ready {
                    ui.label(
                        RichText::new(format!(
                            "{} ({}, {} MB).",
                            tr(lang, "meeting_need_model_card"),
                            v.model_label,
                            v.model_mb
                        ))
                        .color(pal.text_secondary),
                    );
                    ui.add_space(4.0);
                    if ui.button(tr(lang, "meeting_download_start")).clicked() {
                        act = Some(crate::meeting::MeetingAction::Start(bid));
                    }
                } else if other_busy {
                    ui.label(RichText::new(tr(lang, "meeting_busy")).color(pal.text_faint));
                } else {
                    ui.horizontal(|ui| {
                        use crate::widgets::text_link;
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let label =
                            if has_transcript { tr(lang, "meeting_resume") } else { tr(lang, "meeting_record") };
                        let start =
                            egui::Button::new(RichText::new(format!("\u{25cf}  {label}")).size(13.0).color(pal.bg))
                                .fill(pal.text)
                                .stroke(egui::Stroke::NONE)
                                .corner_radius(CornerRadius::same(14))
                                .min_size(egui::vec2(0.0, 28.0));
                        if ui.add(start).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            act = Some(crate::meeting::MeetingAction::Start(bid));
                        }
                        ui.add_space(8.0);
                        let short = |s: &str, n: usize| -> String {
                            if s.chars().count() > n {
                                format!("{}…", s.chars().take(n - 1).collect::<String>())
                            } else {
                                s.to_string()
                            }
                        };
                        // what is recorded
                        let sources = [
                            ("mic".to_string(), tr(lang, "meeting_src_mic").to_string()),
                            ("system".to_string(), tr(lang, "meeting_src_sys").to_string()),
                            ("both".to_string(), tr(lang, "meeting_src_both").to_string()),
                        ];
                        let cur = sources.iter().find(|(k, _)| *k == v.source).map(|(_, l)| l.clone());
                        if let Some(k) = quiet_menu(
                            ui,
                            pal,
                            &cur.unwrap_or_default(),
                            &sources,
                            &v.source,
                            tr(lang, "meeting_source"),
                        ) {
                            act = Some(crate::meeting::MeetingAction::SetSource(k));
                        }
                        // spoken language
                        let langs = [
                            ("auto".to_string(), tr(lang, "meeting_lang_auto").to_string()),
                            ("fr".to_string(), tr(lang, "meeting_lang_fr").to_string()),
                            ("en".to_string(), tr(lang, "meeting_lang_en").to_string()),
                        ];
                        let cur = match v.language.as_str() {
                            "fr" => "FR",
                            "en" => "EN",
                            _ => tr(lang, "meeting_lang_auto_s"),
                        };
                        if let Some(k) =
                            quiet_menu(ui, pal, cur, &langs, &v.language, tr(lang, "meeting_spoken_language"))
                        {
                            act = Some(crate::meeting::MeetingAction::SetLanguage(k));
                        }
                        // microphone
                        if v.source != "system" {
                            let default_label = match &v.default_mic {
                                Some(d) => format!("{} ({})", tr(lang, "meeting_mic_default"), short(d, 28)),
                                None => tr(lang, "meeting_mic_default").to_string(),
                            };
                            let mut mics = vec![(String::new(), default_label)];
                            mics.extend(v.mics.iter().map(|m| (m.clone(), m.clone())));
                            let cur = if v.microphone.is_empty() {
                                tr(lang, "meeting_mic_default").to_string()
                            } else {
                                short(&v.microphone, 20)
                            };
                            if let Some(k) =
                                quiet_menu(ui, pal, &cur, &mics, &v.microphone, tr(lang, "meeting_microphone"))
                            {
                                act = Some(crate::meeting::MeetingAction::SetMicrophone(k));
                            }
                        }
                        // the kind of meeting: how the notes will be written
                        // the templates are the pages of the Transcription templates group
                        let mut tpls: Vec<(String, String)> = if v.custom_templates.is_empty() {
                            // none left: the general summary keeps the block working, nothing else comes back
                            vec![("summary".to_string(), crate::meeting::template_label(lang, "summary").to_string())]
                        } else {
                            v.custom_templates.clone()
                        };
                        let cur = tpls
                            .iter()
                            .find(|(k, _)| *k == v.template)
                            .map(|(_, l)| short(l, 24))
                            .unwrap_or_else(|| short(crate::meeting::template_label(lang, &v.template), 24));
                        tpls.push(("__manage__".to_string(), tr(lang, "meeting_manage_templates").to_string()));
                        if let Some(k) = quiet_menu(ui, pal, &cur, &tpls, &v.template, tr(lang, "meeting_template")) {
                            act = Some(if k == "__manage__" {
                                crate::meeting::MeetingAction::ManageTemplates
                            } else {
                                crate::meeting::MeetingAction::SetTemplate(k)
                            });
                        }
                        if has_transcript {
                            ui.add_space(6.0);
                            let l = if has_notes {
                                tr(lang, "meeting_rewrite_notes")
                            } else {
                                tr(lang, "meeting_write_notes")
                            };
                            if text_link(ui, pal, l).clicked() {
                                act = Some(crate::meeting::MeetingAction::Notes(bid));
                            }
                        }
                        // a small icon in the corner: back to the default settings
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let r = ui
                                .add(
                                    egui::Button::new(RichText::new("\u{21ba}").size(14.0).color(pal.text_faint))
                                        .frame(false),
                                )
                                .on_hover_text(tr(lang, "meeting_reset"));
                            if r.clicked() {
                                act = Some(crate::meeting::MeetingAction::ResetSettings);
                            }
                        });
                    });
                }
                if let (Some(j), true) =
                    (loc.transcript, recording_here.is_none() && !finishing_here && writing_here.is_none())
                {
                    let names = crate::meeting::speakers(blocks, j);
                    if !names.is_empty() {
                        // (name being renamed, text typed so far, the box still has to take the focus)
                        let sid = Id::unique(("rename-speaker", bid));
                        let mut editing: Option<(String, String, bool)> = ui.ctx().data(|d| d.get_temp(sid));
                        ui.add_space(2.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.label(RichText::new(tr(lang, "speakers")).size(12.0).color(pal.text_faint));
                            for name in &names {
                                let mine = editing.as_ref().is_some_and(|e| &e.0 == name);
                                if mine {
                                    let (_, buf, fresh) = editing.as_mut().expect("checked");
                                    let r = ui.add(
                                        egui::TextEdit::singleline(buf)
                                            .desired_width(140.0)
                                            .font(egui::FontId::proportional(12.5)),
                                    );
                                    if std::mem::take(fresh) {
                                        r.request_focus();
                                    }
                                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                                    let escape = ui.input(|i| i.key_pressed(Key::Escape));
                                    if enter {
                                        let (old, new, _) = editing.take().expect("checked");
                                        act = Some(crate::meeting::MeetingAction::RenameSpeaker(bid, old, new));
                                    } else if escape || r.lost_focus() {
                                        editing = None;
                                    }
                                } else {
                                    let chip = egui::Button::new(RichText::new(name).size(12.5).color(pal.accent))
                                        .stroke(egui::Stroke::NONE);
                                    let r = ui
                                        .add(chip)
                                        .on_hover_text(tr(lang, "rename_speaker_hint"))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if r.clicked() {
                                        editing = Some((name.clone(), name.clone(), true));
                                    }
                                }
                            }
                        });
                        ui.ctx().data_mut(|d| match editing {
                            Some(e) => {
                                d.insert_temp(sid, e);
                            }
                            None => d.remove::<(String, String, bool)>(sid),
                        });
                    }
                }
                if let Some(e) = &v.error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(e).size(12.0).color(pal.danger));
                    if v.mic_help && crate::widgets::text_link(ui, pal, tr(lang, "meeting_open_mic_settings")).clicked()
                    {
                        act = Some(crate::meeting::MeetingAction::OpenMicSettings);
                    }
                    if let Some(bad) = &v.silent_device {
                        let others: Vec<&String> = v.mics.iter().filter(|m| *m != bad).collect();
                        if !others.is_empty() {
                            ui.add_space(2.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(tr(lang, "meeting_try_mic")).color(pal.text_secondary));
                                for m in others {
                                    if crate::widgets::text_link(ui, pal, m).clicked() {
                                        act = Some(crate::meeting::MeetingAction::UseMicrophone(m.clone(), bid));
                                    }
                                }
                            });
                        }
                    }
                    if v.audio_help
                        && crate::widgets::text_link(ui, pal, tr(lang, "meeting_open_audio_settings")).clicked()
                    {
                        act = Some(crate::meeting::MeetingAction::OpenAudioSettings);
                    }
                }
            });
        });
    if let Some(a) = act {
        out.actions.push(Action::Meeting(a));
    }
    r.response
}
