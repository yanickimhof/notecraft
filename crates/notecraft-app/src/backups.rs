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
}

/// Minutes between backups offered in the settings (0 = off).
const CHOICES: [u32; 4] = [0, 5, 15, 60];

impl NcApp {
    /// Start a backup on a background thread when it is due (or when `force`).
    pub fn poll_backup(&mut self, ctx: &egui::Context, force: bool) {
        let Some(ws) = &self.ws else { return };
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
        let minutes = self.prefs.backup_minutes;
        if minutes == 0 && !force {
            return;
        }
        let due = force
            || match self.backup.last_run {
                None => self.backup_started.elapsed() > Duration::from_secs(20),
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

    pub fn backup_section(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        ui.label(RichText::new(tr(lang, "backups")).strong().size(14.5).color(pal.text));
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(tr(lang, "backups_text")).size(12.5).color(pal.text_secondary)).wrap());
        ui.add_space(8.0);
        widgets::setting_row(ui, &pal, tr(lang, "backup_every"), "", |ui| {
            let opts: Vec<(u32, String)> = CHOICES
                .iter()
                .map(|&m| (m, if m == 0 { tr(lang, "backup_off").to_string() } else { format!("{m} min") }))
                .collect();
            widgets::segmented(ui, &pal, &mut self.prefs.backup_minutes, &opts);
        });
        widgets::row_divider(ui, &pal);
        let running = self.backup.rx.is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(!running, egui::Button::new(tr(lang, "backup_now"))).clicked() {
                let ctx = ui.ctx().clone();
                self.poll_backup(&ctx, true);
            }
            let last = match &self.backup.last_made {
                Some(t) => format!("{} {}", tr(lang, "backup_last"), widgets::relative_time(t, lang)),
                None => tr(lang, "backup_none_yet").to_string(),
            };
            ui.label(
                RichText::new(if running { tr(lang, "backup_running") } else { &last })
                    .size(12.5)
                    .color(pal.text_secondary),
            );
            if let Some(ws) = &self.ws {
                let dir = ws.snapshot_dir();
                if widgets::icon_button(ui, "↗", tr(lang, "open")).clicked() {
                    reveal(&dir);
                }
            }
        });
        let Some(ws) = &self.ws else { return };
        let list: Vec<SnapshotInfo> = ws.list_snapshots().into_iter().take(15).collect();
        let mut restore: Option<String> = None;
        egui::CollapsingHeader::new(tr(lang, "restore_points")).show(ui, |ui| {
            if list.is_empty() {
                ui.label(RichText::new(tr(lang, "backup_none_yet")).size(12.5).color(pal.text_faint));
            }
            for info in &list {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(widgets::relative_time(&info.time, lang)).size(12.5));
                    let why = if info.reason == backup::AUTO_REASON {
                        tr(lang, "backup_auto").to_string()
                    } else {
                        info.reason.clone()
                    };
                    ui.label(
                        RichText::new(format!("{why} · {} {}", info.files.len(), tr(lang, "backup_files")))
                            .size(12.0)
                            .color(pal.text_faint),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let asked = self.backup.confirm_restore.as_deref() == Some(info.id.as_str());
                        let label = if asked { tr(lang, "backup_confirm") } else { tr(lang, "restore") };
                        if ui.small_button(label).clicked() {
                            if asked {
                                restore = Some(info.id.clone());
                            } else {
                                self.backup.confirm_restore = Some(info.id.clone());
                            }
                        }
                    });
                });
            }
        });
        if let Some(id) = restore {
            self.backup.confirm_restore = None;
            self.restore_snapshot_now(&id);
        }
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
                        egui::ScrollArea::vertical().id_salt("history-list").max_height(360.0).show(ui, |ui| {
                            for (k, v) in view.versions.iter().enumerate() {
                                let why = if v.reason == backup::AUTO_REASON {
                                    tr(lang, "backup_auto").to_string()
                                } else {
                                    v.reason.clone()
                                };
                                let label = format!("{}\n{why}", widgets::relative_time(&v.time, lang));
                                if ui.selectable_label(view.selected == k, label).clicked() {
                                    picked = Some(k);
                                }
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical().id_salt("history-preview").max_height(360.0).show(ui, |ui| {
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
