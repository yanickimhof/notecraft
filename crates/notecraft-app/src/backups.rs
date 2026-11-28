//! Scheduled backups (settings section) and the version history of a page.

use crate::app::{reveal, NcApp};
use crate::i18n::tr;
use crate::widgets;
use eframe::egui::{self, Id, RichText, Ui};
use notecraft_core::backup;
use notecraft_core::snapshot::SnapshotInfo;
use notecraft_core::workspace::DocId;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

type BackupResult = Result<Option<SnapshotInfo>, String>;

/// The version history window of one page.
pub struct HistoryView {
    doc: DocId,
    rel: String,
    versions: Vec<SnapshotInfo>,
    selected: usize,
    preview: String,
    confirm: bool,
}

#[derive(Default)]
pub struct BackupUi {
    rx: Option<Receiver<BackupResult>>,
    last_run: Option<Instant>,
    /// Time of the last backup that copied something.
    pub last_made: Option<String>,
    pub history: Option<HistoryView>,
    /// Snapshot waiting for a second click on "Restore".
    confirm_restore: Option<String>,
    /// The backup folder was read once since launch (last backup known, old backups thinned).
    scanned: bool,
}

/// Minutes between backups offered in the settings (0 = off).
const CHOICES: [u32; 3] = [60, 360, 1440];

impl NcApp {
    /// Start a backup on a background thread when it is due (or when `force`).
    pub fn poll_backup(&mut self, ctx: &egui::Context, force: bool) {
        let Some(ws) = &self.ws else { return };
        if !self.backup.scanned {
            // the last backup is read from the folder, so it is known after a restart too
            self.backup.scanned = true;
            self.backup.last_made =
                ws.list_snapshots().into_iter().find(|i| i.reason == backup::AUTO_REASON).map(|i| i.time);
            let base = ws.snapshot_dir();
            std::thread::spawn(move || {
                backup::prune_auto(&base);
            });
        }
        if let Some(rx) = &self.backup.rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.backup.rx = None;
                    self.backup.last_run = Some(Instant::now());
                    match result {
                        Ok(Some(info)) => self.backup.last_made = Some(info.time),
                        Ok(None) => {}
                        Err(e) => notecraft_core::log::error(&format!("backup: {e}")),
                    }
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_secs(2)),
                Err(TryRecvError::Disconnected) => self.backup.rx = None,
            }
            return;
        }
        let minutes = match self.prefs.backup_minutes {
            0 => 0,
            1..=59 => 60,
            m => m,
        };
        if minutes == 0 && !force {
            return;
        }
        let due = force
            || match self.backup.last_run {
                // after a launch: only when the last backup on disk is older than the interval
                None => {
                    self.backup_started.elapsed() > Duration::from_secs(20)
                        && self.backup.last_made.as_deref().is_none_or(|t| {
                            t.parse::<jiff::Timestamp>().map_or(true, |t| {
                                jiff::Timestamp::now().as_second() - t.as_second() >= i64::from(minutes) * 60
                            })
                        })
                }
                Some(t) => t.elapsed() >= Duration::from_secs(u64::from(minutes) * 60),
            };
        if !due {
            ctx.request_repaint_after(Duration::from_secs(30));
            return;
        }
        let (root, base) = (ws.root.clone(), ws.snapshot_dir());
        let (tx, rx) = mpsc::channel();
        self.backup.rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(backup::run_backup(&root, &base).map_err(|e| e.to_string()));
        });
        ctx.request_repaint_after(Duration::from_secs(2));
    }

    /// The page behind "Restore": back up now, then the restore points, all visible.
    pub fn restore_page(&mut self, ui: &mut Ui) {
        use crate::widgets::{srow, text_link};
        let pal = self.pal;
        let lang = self.prefs.lang;
        let running = self.backup.rx.is_some();
        let last = match &self.backup.last_made {
            Some(t) => format!("{} {}", tr(lang, "backup_last"), widgets::relative_time(t, lang)),
            None => tr(lang, "backup_none_yet").to_string(),
        };
        self.backup_frequency_row(ui);
        let mut now = false;
        let mut open = false;
        srow(ui, &pal, tr(lang, "backup_now"), if running { tr(lang, "backup_running") } else { &last }, None, |ui| {
            if text_link(ui, &pal, tr(lang, "open")).clicked() {
                open = true;
            }
            if !running && text_link(ui, &pal, tr(lang, "backup_now")).clicked() {
                now = true;
            }
        });
        if now {
            let ctx = ui.ctx().clone();
            self.poll_backup(&ctx, true);
        }
        if open {
            if let Some(ws) = &self.ws {
                reveal(&ws.snapshot_dir());
            }
        }
        ui.add_space(10.0);
        let Some(ws) = &self.ws else { return };
        // backups and the copies kept before a deletion; single page edits live in the page history
        let list: Vec<SnapshotInfo> = ws
            .list_snapshots()
            .into_iter()
            .filter(|i| {
                i.reason == backup::AUTO_REASON
                    || ["delete forever", "empty trash", "delete property"].contains(&i.reason.as_str())
            })
            .take(30)
            .collect();
        if list.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(tr(lang, "backup_none_yet")).size(13.0).color(pal.text_faint));
            });
        }
        let mut restore: Option<String> = None;
        for info in &list {
            let why = if info.reason == backup::AUTO_REASON {
                tr(lang, "backup_auto").to_string()
            } else {
                info.reason.clone()
            };
            let unit = tr(lang, if info.files.len() == 1 { "backup_file" } else { "backup_files" });
            let sub = format!("{why} · {} {unit}", info.files.len());
            let asked = self.backup.confirm_restore.as_deref() == Some(info.id.as_str());
            srow(ui, &pal, &widgets::relative_time(&info.time, lang), &sub, None, |ui| {
                let label = if asked { tr(lang, "backup_confirm") } else { tr(lang, "restore") };
                if text_link(ui, &pal, label).clicked() {
                    if asked {
                        restore = Some(info.id.clone());
                    } else {
                        self.backup.confirm_restore = Some(info.id.clone());
                    }
                }
            });
        }
        if let Some(id) = restore {
            self.backup.confirm_restore = None;
            self.restore_snapshot_now(&id);
        }
    }

    /// How often the automatic backup runs: a click steps to the next choice.
    pub fn backup_frequency_row(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        if self.prefs.backup_minutes == 0 {
            return;
        }
        let label = |m: u32| if m >= 1440 { "24 h".to_string() } else { format!("{} h", m / 60) };
        let opts: Vec<(u32, widgets::Glyph, String)> =
            CHOICES.iter().map(|&m| (m, widgets::Glyph::None, label(m))).collect();
        ui.add_space(8.0);
        widgets::srow(ui, &pal, tr(lang, "backup_every"), "", None, |ui| {
            widgets::cycle(ui, &pal, &mut self.prefs.backup_minutes, &opts, true);
        });
    }

    fn restore_snapshot_now(&mut self, id: &str) {
        let lang = self.prefs.lang;
        let Some(ws) = self.ws.as_mut() else { return };
        match ws.restore_snapshot(id) {
            Ok(n) => {
                self.base = None;
                self.validate_current();
                self.notify(format!("{} ({n})", tr(lang, "backup_restored")));
            }
            Err(e) => self.notify_error("restore", &e),
        }
    }

    /// Open the version history of a document.
    pub fn open_history(&mut self, doc: &DocId) {
        let Some(ws) = &self.ws else { return };
        let Some(rel) = ws.doc_relative_path(doc) else { return };
        let versions = backup::versions_of(&ws.snapshot_dir(), &rel);
        let mut view =
            HistoryView { doc: doc.clone(), rel, versions, selected: 0, preview: String::new(), confirm: false };
        view.preview = self.history_preview(&view);
        self.backup.history = Some(view);
    }

    fn history_preview(&self, view: &HistoryView) -> String {
        let (Some(ws), Some(v)) = (&self.ws, view.versions.get(view.selected)) else { return String::new() };
        let text = backup::version_text(&ws.snapshot_dir(), &v.id, &view.rel).unwrap_or_default();
        // skip the front matter
        let body = match text.strip_prefix("---\n").and_then(|t| t.split_once("\n---\n")) {
            Some((_, rest)) => rest.to_string(),
            None => text,
        };
        body.chars().take(4000).collect()
    }

    pub fn history_ui(&mut self, ctx: &egui::Context) {
        let Some(mut view) = self.backup.history.take() else { return };
        let pal = self.pal;
        let lang = self.prefs.lang;
        let mut close = false;
        let mut restore: Option<String> = None;
        let mut picked: Option<usize> = None;
        // the lists keep a fixed height that fits the window, so a long history scrolls instead of growing
        let list_h = (ctx.content_rect().height() * 0.55).clamp(200.0, 460.0);
        let modal = egui::Modal::new(Id::unique("history")).show(ctx, |ui| {
            ui.set_width(720.0);
            ui.heading(tr(lang, "history"));
            ui.add_space(6.0);
            if view.versions.is_empty() {
                ui.label(RichText::new(tr(lang, "history_empty")).color(pal.text_secondary));
            } else {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        ui.set_height(list_h);
                        egui::ScrollArea::vertical()
                            .id_salt("history-list")
                            .max_height(list_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for (k, v) in view.versions.iter().enumerate() {
                                    let why = if v.reason == backup::AUTO_REASON {
                                        tr(lang, "backup_auto").to_string()
                                    } else {
                                        v.reason.clone()
                                    };
                                    let label = format!("{} · {why}", widgets::relative_time(&v.time, lang));
                                    if ui.selectable_label(view.selected == k, label).clicked() {
                                        picked = Some(k);
                                    }
                                }
                            });
                    });
                    // a fixed-height rule: `ui.separator()` stretches to the whole free height here, which made
                    // the window grow every frame until it left the screen
                    let (rule, _) = ui.allocate_exact_size(egui::vec2(9.0, list_h), egui::Sense::hover());
                    ui.painter().vline(rule.center().x, rule.y_range(), egui::Stroke::new(1.0, pal.border));
                    ui.vertical(|ui| {
                        ui.set_height(list_h);
                        egui::ScrollArea::vertical()
                            .id_salt("history-preview")
                            .max_height(list_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(&view.preview).size(13.0)).wrap());
                            });
                    });
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if !view.versions.is_empty() {
                    let label = if view.confirm { tr(lang, "backup_confirm") } else { tr(lang, "history_restore") };
                    if ui.button(label).clicked() {
                        if view.confirm {
                            restore = view.versions.get(view.selected).map(|v| v.id.clone());
                        } else {
                            view.confirm = true;
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr(lang, "close")).clicked() {
                        close = true;
                    }
                });
            });
        });
        if let Some(k) = picked {
            view.selected = k;
            view.confirm = false;
            view.preview = self.history_preview(&view);
        }
        if let Some(id) = restore {
            if let Some(ws) = self.ws.as_mut() {
                match ws.restore_snapshot_file(&id, &view.rel) {
                    Ok(()) => {
                        self.base = None;
                        self.validate_current();
                        self.notify(tr(lang, "backup_restored").to_string());
                        close = true;
                    }
                    Err(e) => self.notify_error("restore page", &e),
                }
            }
            let _ = &view.doc;
        }
        if !(close || modal.should_close()) {
            self.backup.history = Some(view);
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use eframe::egui;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Height of a modal laid out like the history window, after several frames.
    fn modal_height(with_separator: bool) -> (f32, f32) {
        let heights: Rc<Cell<(f32, f32)>> = Rc::new(Cell::new((0.0, 0.0)));
        let out = heights.clone();
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(840.0, 600.0)).build_ui_state(
            move |ui, _: &mut ()| {
                let ctx = &ui.ctx().clone();
                let list_h = (ctx.content_rect().height() * 0.55).clamp(200.0, 460.0);
                let m = egui::Modal::new(egui::Id::unique("history-test")).show(ctx, |ui| {
                    ui.set_width(720.0);
                    ui.heading("History");
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(220.0);
                            ui.set_height(list_h);
                            egui::ScrollArea::vertical()
                                .id_salt("l")
                                .max_height(list_h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for k in 0..60 {
                                        let _ = ui.selectable_label(false, format!("{k} h ago · replace_text"));
                                    }
                                });
                        });
                        if with_separator {
                            ui.separator();
                        } else {
                            let (rule, _) = ui.allocate_exact_size(egui::vec2(9.0, list_h), egui::Sense::hover());
                            ui.painter().vline(
                                rule.center().x,
                                rule.y_range(),
                                egui::Stroke::new(1.0, egui::Color32::GRAY),
                            );
                        }
                        ui.vertical(|ui| {
                            ui.set_height(list_h);
                            egui::ScrollArea::vertical()
                                .id_salt("p")
                                .max_height(list_h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add(egui::Label::new("long preview text ".repeat(600)).wrap());
                                });
                        });
                    });
                    ui.add_space(8.0);
                    let _ = ui.button("Close");
                });
                let r = m.response.rect;
                out.set((r.min.y, r.height()));
            },
            (),
        );
        for _ in 0..8 {
            h.run();
        }
        heights.get()
    }

    #[test]
    fn the_history_window_keeps_a_stable_size_inside_the_screen() {
        let (top, height) = modal_height(false);
        assert!(top >= 0.0 && height <= 600.0, "top {top} height {height}");
        let (top2, height2) = modal_height(false);
        assert_eq!((top, height), (top2, height2));
    }
}
