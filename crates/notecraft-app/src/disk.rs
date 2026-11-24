//! Living with changes made elsewhere: another Mac through a synced folder,
//! the MCP server, git or a text editor. The watcher notices files that
//! changed; unchanged blocks keep their identity so the caret stays put; if
//! the open page changed while the person was typing, both edits are merged
//! block by block, and when they touched the same block both versions are
//! kept and a "Conflict" notice lets the person choose.

use crate::app::{NcApp, Target};
use crate::i18n::tr;
use eframe::egui::{self, CornerRadius, RichText, Ui};
use notecraft_core::{DocId, Resolution};
use std::time::Duration;

impl NcApp {
    /// Remember the current document as last read or written.
    pub fn snapshot_base(&mut self) {
        self.base = None;
        let (Some(ws), Some(t)) = (self.ws.as_mut(), self.current.clone()) else { return };
        if let Some(b) = NcApp::blocks_mut(ws, &t) {
            self.base = Some((t, b.clone()));
        }
    }

    /// Check for outside changes (called every frame; cheap when nothing happened).
    pub fn poll_disk(&mut self, ctx: &egui::Context) {
        let Some(w) = self.watcher.as_mut() else { return };
        // events arrive on another thread: keep a slow heartbeat so they are seen
        ctx.request_repaint_after(Duration::from_millis(if w.is_pending() { 150 } else { 700 }));
        if !w.poll() {
            return;
        }
        let lang = self.prefs.lang;
        let Some(ws) = self.ws.as_mut() else { return };
        let changed = ws.external_changes();
        if changed.is_empty() {
            return;
        }
        let current_doc = match &self.current {
            Some(Target::Doc(d)) => Some(d.clone()),
            _ => None,
        };
        let current_changed =
            current_doc.as_ref().is_some_and(|d| changed.iter().any(|p| ws.doc_for_path(p).as_ref() == Some(d)));
        let mut message = None;
        if let Some((dirty_target, _)) = self.dirty.clone() {
            match (&dirty_target, &current_doc) {
                (Target::Doc(d), Some(cur)) if d == cur && current_changed => {
                    // unsaved typing + a change from elsewhere on the same page
                    self.dirty = None;
                    let base = match &self.base {
                        Some((t, b)) if *t == dirty_target => b.clone(),
                        _ => Vec::new(),
                    };
                    let ws = self.ws.as_mut().unwrap();
                    ws.touch(d);
                    match ws.merge_external(d, &base) {
                        Ok(true) => message = Some(tr(lang, "merged_typing").to_string()),
                        Ok(false) => message = Some(tr(lang, "conflict_created").to_string()),
                        Err(e) => message = Some(e.to_string()),
                    }
                }
                _ => self.flush_save(),
            }
        }
        let ws = self.ws.as_mut().unwrap();
        if let Err(e) = ws.reload() {
            message = Some(e.to_string());
        }
        if message.is_none() && current_changed {
            message = Some(tr(lang, "updated_from_disk").to_string());
        }
        self.validate_current();
        self.snapshot_base();
        if let Some(m) = message {
            self.notify(m);
        }
    }

    /// The discreet notice shown on a page that has two versions.
    pub fn conflict_banner(&mut self, ui: &mut Ui, target: &Target) {
        let Target::Doc(doc) = target else { return };
        let Some(ws) = &self.ws else { return };
        let Some(c) = ws.conflict_for(doc).cloned() else { return };
        let pal = self.pal;
        let lang = self.prefs.lang;
        let other_name = c.copy.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let is_db = matches!(doc, DocId::Database(_));
        let mut choice = None;
        ui.horizontal(|ui| {
            ui.add_space(46.0);
            egui::Frame::NONE
                .fill(pal.option_colors("yellow").0)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let title = if is_db { tr(lang, "conflict_db") } else { tr(lang, "conflict_title") };
                        ui.label(RichText::new(format!("⚠  {title}")).size(13.5).color(pal.option_tokens("yellow").1));
                        ui.label(RichText::new(other_name.clone()).size(12.0).color(pal.text_secondary));
                        ui.horizontal(|ui| {
                            if ui.button(tr(lang, "keep_this")).clicked() {
                                choice = Some(Resolution::KeepThis);
                            }
                            if ui.button(tr(lang, "keep_other")).clicked() {
                                choice = Some(Resolution::KeepOther);
                            }
                            if !is_db && ui.button(tr(lang, "keep_both")).clicked() {
                                choice = Some(Resolution::KeepBoth);
                            }
                            let toggle =
                                if self.show_other_version { tr(lang, "hide_other") } else { tr(lang, "show_other") };
                            if ui.button(toggle).clicked() {
                                self.show_other_version = !self.show_other_version;
                            }
                        });
                        if self.show_other_version {
                            let text = ws.other_version_text(doc).unwrap_or_default();
                            egui::ScrollArea::vertical().id_salt("other-version").max_height(220.0).show(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(text).monospace().size(12.5)).wrap());
                            });
                        }
                    });
                });
        });
        ui.add_space(6.0);
        if let Some(how) = choice {
            self.flush_save();
            let ws = self.ws.as_mut().unwrap();
            match ws.resolve_conflict(doc, how) {
                Ok(Some(new_doc)) => {
                    let title = ws.title(&new_doc);
                    self.notify(format!("{} — {title}", tr(lang, "keep_both")));
                }
                Ok(None) => {}
                Err(e) => self.notify(e.to_string()),
            }
            self.show_other_version = false;
            self.snapshot_base();
        }
    }
}
