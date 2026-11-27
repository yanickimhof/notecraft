//! Application state, navigation, saving, global shortcuts, dialogs and the
//! top bar. The sidebar, page view and quick finder live in their own modules
//! as further `impl NcApp` blocks.

use crate::dbview::{DbAction, DbUi};
use crate::editor::{Action, EditorState};
use crate::finder::{Finder, FinderMode};
use crate::i18n::{tr, Lang};
use crate::theme::{self, Palette, ThemeChoice};
use crate::widgets;
use eframe::egui::{self, Color32, Id, Key, Modifiers, RichText, Stroke, Vec2};
use notecraft_core::blocks::{Block, BlockKind};
use notecraft_core::database::ViewKind;
use notecraft_core::{import_export, sample, DocId, Workspace};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

mod chrome;
mod files;
mod onboarding;
mod system;
mod tabs;
mod templates;
mod webpub;
pub use system::*;
pub use templates::{note_template_text, note_templates, template_results};
pub(crate) use templates::{notes_group, templates_root};
pub use webpub::{GuardAction, PubAct, PubGuard, SubdomainDlg};
/// A page, database or template that exists and is not in the trash, nor under something that is.
/// It is the only kind a tab or the open page may show: a trashed page must close, not stay on screen.
pub(crate) fn target_is_live(ws: &Workspace, t: &Target) -> bool {
    match t {
        // a page that is archived, or sits under an archived one, is not shown in the window
        Target::Doc(d) => {
            ws.is_live(d)
                && !ws
                    .breadcrumbs(d)
                    .iter()
                    .any(|a| matches!(a, DocId::Page(id) if ws.pages.get(id).is_some_and(|p| p.meta.archived)))
        }
        Target::Template(db, id) => {
            ws.is_live(&DocId::Database(db.clone())) && ws.dbs.get(db).is_some_and(|d| d.templates.contains_key(id))
        }
    }
}

#[cfg(test)]
mod open_tests;
#[cfg(test)]
mod tab_key_tests;

/// Preferences kept by eframe between launches (not in the workspace).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeChoice,
    pub lang: Lang,
    pub last_workspace: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub sidebar_open: bool,
    pub expanded: Vec<String>,
    /// Page zoom (1.0, steps of 0.1, 0.5–4.9).
    pub zoom: f32,
    /// Body text size in percent (85, 100, 115, 130), on top of the page zoom.
    pub text_percent: u32,
    /// AI engine choices (the API key is never here: it lives in the Keychain).
    pub ai: crate::ai_panel::AiPrefs,
    /// Meeting notes: speech model, language and note template.
    pub meeting: crate::meeting::MeetingPrefs,
    /// Minutes between automatic backups (0 = off).
    pub backup_minutes: u32,
    /// The first launch assistant was completed or skipped.
    pub onboarded: bool,
    /// The person asked not to see the demo window at start up again.
    pub demo_seen: bool,
    /// Open tabs of the last session (`Target::key`), with the active one and the workspace they belong to.
    pub tabs: Vec<String>,
    pub active_tab: usize,
    pub tabs_workspace: Option<PathBuf>,
}

/// French when the Mac's first preferred language is French, English otherwise.
fn system_lang() -> Lang {
    let out = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output();
    match out {
        Ok(o)
            if String::from_utf8_lossy(&o.stdout)
                .lines()
                .find(|l| l.contains('"'))
                .is_some_and(|l| l.contains("\"fr")) =>
        {
            Lang::Fr
        }
        _ => Lang::En,
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: ThemeChoice::System,
            lang: system_lang(),
            last_workspace: None,
            recent: Vec::new(),
            sidebar_open: true,
            expanded: Vec::new(),
            zoom: theme::ZOOM_DEFAULT,
            text_percent: 100,
            ai: crate::ai_panel::AiPrefs::default(),
            meeting: crate::meeting::MeetingPrefs::default(),
            backup_minutes: 1440,
            onboarded: false,
            demo_seen: false,
            tabs: Vec::new(),
            active_tab: 0,
            tabs_workspace: None,
        }
    }
}

/// What the main area shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Doc(DocId),
    /// (database id, template id)
    Template(String, String),
}

impl Target {
    pub fn key(&self) -> String {
        match self {
            Target::Doc(d) => d.key(),
            Target::Template(db, t) => format!("t:{db}:{t}"),
        }
    }

    /// The inverse of `key`.
    pub fn from_key(key: &str) -> Option<Target> {
        if let Some(rest) = key.strip_prefix("t:") {
            let (db, id) = rest.split_once(':')?;
            return Some(Target::Template(db.into(), id.into()));
        }
        DocId::from_key(key).map(Target::Doc)
    }
}

/// Command-line options.
#[derive(Debug, Default, Clone)]
pub struct Args {
    pub screenshot: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub dark: bool,
    /// Title of a page or database to open first.
    pub open: Option<String>,
    /// For databases opened with `open`: show this view kind (table, board, list).
    pub view: Option<String>,
    /// Open a panel at start: "ai" or "settings" (for checks and screenshots).
    pub panel: Option<String>,
    /// Scroll the page to this offset (screenshots only).
    pub scroll: Option<f32>,
}

pub struct Shot {
    pub path: PathBuf,
    pub frames: u32,
    pub requested: bool,
}

pub struct NcApp {
    pub prefs: Prefs,
    pub ws: Option<Workspace>,
    pub current: Option<Target>,
    pub back: Vec<Target>,
    pub forward: Vec<Target>,
    /// Open tabs; the active one always mirrors `current`.
    pub tabs: Vec<Target>,
    /// The slide show, while there is one.
    pub present: Option<crate::present::Present>,
    /// Start the presentation on the first frame, at this slide (screenshots only).
    pub present_pending: bool,
    pub onboard: onboarding::Onboard,
    /// Page of the demo window when it is open.
    pub demo_at: Option<usize>,
    pub demo_auto_done: bool,
    pub present_at: usize,
    /// Fixed scroll offset of the page view (screenshots only).
    pub shot_scroll: Option<f32>,
    pub reminder_edit: Option<crate::remind::ReminderEdit>,
    pub reminders_open: bool,
    pub graph_open: bool,
    pub graph: Option<crate::graph_view::GraphState>,
    /// A running import (result of the worker thread).
    pub importing: Option<
        std::sync::mpsc::Receiver<
            Result<(notecraft_core::import_any::Source, notecraft_core::import_export::ImportReport), String>,
        >,
    >,
    pub last_reminder_check: Instant,
    pub active_tab: usize,
    pub editor: EditorState,
    pub dbui: DbUi,
    pub finder: Option<Finder>,
    /// Menu bar icon (macOS); `None` elsewhere and in screenshot mode.
    tray: Option<crate::tray::Tray>,
    pub dirty: Option<(Target, Instant)>,
    pub title_before: Option<(Target, String)>,
    pub focus_title: bool,
    pub toast: Option<(String, Instant)>,
    pub settings_open: bool,
    pub trash_open: bool,
    #[allow(dead_code)]
    pub settings_tab: crate::settings::Tab,
    pub vault: crate::vault_ui::VaultUi,
    pub backup: crate::backups::BackupUi,
    pub backup_started: Instant,
    pub show_backlinks: bool,
    pub expanded: HashSet<String>,
    /// The page just opened from the Favourites list: the page tree neither opens up nor lights up for it.
    pub fav_nav_doc: Option<DocId>,
    /// The page being moved (the "Move to…" dialog is open) and what was typed to find the new parent.
    pub move_doc: Option<DocId>,
    pub move_query: String,
    pub pal: Palette,
    pub shot: Option<Shot>,
    pub icon_picker: bool,
    /// Notices files changed by someone else (another Mac, the MCP server, git…).
    pub watcher: Option<notecraft_core::watch::Watcher>,
    /// The current document as last read or written (for merging outside edits).
    pub base: Option<(Target, Vec<Block>)>,
    pub ai: crate::ai_panel::AiState,
    pub publish_cfg: crate::settings::PublishCfg,
    /// What is published, and where (kept in the app data folder, not in the notes).
    pub pubs: crate::publish::Store,
    pub publishing: Option<std::sync::mpsc::Receiver<crate::publish::Msg>>,
    /// The page to publish again once the web address name is set.
    pub pub_retry: Option<(String, bool)>,
    pub subdomain: Option<SubdomainDlg>,
    /// Asked when a removal would leave a page live on the web.
    pub pub_guard: Option<PubGuard>,
    pub meeting: crate::meeting::MeetingState,
    pub show_other_version: bool,
}

impl NcApp {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        // zoom follows our own rules (below), not egui's built-in steps
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut prefs: Prefs = if args.screenshot.is_some() {
            Prefs::default()
        } else {
            cc.storage.and_then(|s| eframe::get_value(s, "prefs")).unwrap_or_default()
        };
        if args.dark {
            prefs.theme = ThemeChoice::Dark;
        }
        // folders that are gone, or too broad to be a workspace (the home folder), were never shown and could never be removed
        prefs.recent.retain(|p| p.is_dir() && !is_too_broad(p));
        if prefs.last_workspace.as_ref().is_some_and(|p| is_too_broad(p)) {
            prefs.last_workspace = None;
        }
        let tray_labels = [
            tr(prefs.lang, "tray_open"),
            tr(prefs.lang, "tray_new_page"),
            tr(prefs.lang, "tray_search"),
            tr(prefs.lang, "tray_quit"),
        ];
        let tray = if args.screenshot.is_some() { None } else { crate::tray::Tray::new(&cc.egui_ctx, tray_labels) };
        let mut app = NcApp {
            expanded: prefs.expanded.iter().cloned().collect(),
            fav_nav_doc: None,
            move_doc: None,
            move_query: String::new(),
            prefs,
            ws: None,
            current: None,
            back: Vec::new(),
            tabs: Vec::new(),
            present: None,
            present_pending: false,
            onboard: onboarding::Onboard::default(),
            demo_at: None,
            demo_auto_done: false,
            present_at: 0,
            shot_scroll: args.scroll,
            reminder_edit: None,
            reminders_open: false,
            graph_open: false,
            graph: None,
            importing: None,
            last_reminder_check: Instant::now() - std::time::Duration::from_secs(60),
            active_tab: 0,
            forward: Vec::new(),
            editor: EditorState::default(),
            dbui: DbUi::default(),
            finder: None,
            tray,
            dirty: None,
            title_before: None,
            focus_title: false,
            toast: None,
            settings_open: false,
            trash_open: false,
            settings_tab: Default::default(),
            vault: Default::default(),
            backup: Default::default(),
            backup_started: Instant::now(),
            show_backlinks: false,
            pal: Palette::light(),
            shot: args.screenshot.clone().map(|path| Shot { path, frames: 0, requested: false }),
            icon_picker: false,
            watcher: None,
            base: None,
            ai: crate::ai_panel::AiState::default(),
            publish_cfg: Default::default(),
            pubs: crate::publish::Store::load(),
            publishing: None,
            pub_retry: None,
            subdomain: None,
            pub_guard: None,
            meeting: crate::meeting::MeetingState::default(),
            show_other_version: false,
        };
        let folder = if let Some(w) = args.workspace.clone() {
            Some(w)
        } else if app.shot.is_some() {
            // a fresh sample workspace in the temp folder, so a check never touches real notes
            let parent = std::env::temp_dir().join(format!("notecraft-screenshot-{}", std::process::id()));
            let dir = parent.join("My notes");
            let _ = std::fs::remove_dir_all(&parent);
            Some(dir)
        } else {
            app.prefs.last_workspace.clone().filter(|p| p.is_dir())
        };
        if let Some(f) = folder {
            let sample = app.shot.is_some() && args.workspace.is_none();
            app.open_workspace(&f, sample);
        }
        if app.shot.is_none() && args.open.is_none() {
            app.restore_tabs();
        }
        if let (Some(title), Some(ws)) = (&args.open, &app.ws) {
            if let Some(doc) = ws.resolve_title(title) {
                if let (DocId::Database(db), Some(kind)) = (&doc, &args.view) {
                    if let Some(v) =
                        ws.dbs[db].schema.views.iter().find(|v| format!("{:?}", v.kind).eq_ignore_ascii_case(kind))
                    {
                        app.dbui.active.insert(db.clone(), v.id.clone());
                    }
                }
                app.current = Some(Target::Doc(doc));
            }
        }
        match args.panel.as_deref() {
            Some("ai") => app.open_ai(),
            Some("settings") => app.settings_open = true,
            Some("graph") => app.graph_open = true,
            Some("tabs") => {
                // for the pictures: three tabs, the first one active
                let docs: Vec<Target> = ["Getting started", "Reading list", "Projects"]
                    .iter()
                    .filter_map(|t| app.ws.as_ref().and_then(|w| w.resolve_title(t)))
                    .map(Target::Doc)
                    .collect();
                if let Some(first) = docs.first().cloned() {
                    app.current = Some(first);
                    app.sync_tabs();
                    for d in docs.into_iter().skip(1) {
                        app.tabs.push(d);
                    }
                }
            }
            Some("remind") => {
                if let Some(Target::Doc(DocId::Page(id))) = app.current.clone() {
                    app.edit_reminder(&id);
                }
            }
            Some(p) if p.starts_with("demo") => {
                app.demo_at = Some(p.strip_prefix("demo:").and_then(|n| n.parse().ok()).unwrap_or(0));
                app.demo_auto_done = true;
            }
            Some(p) if p.starts_with("present") => {
                app.present_at = p.strip_prefix("present:").and_then(|n| n.parse().ok()).unwrap_or(0);
                app.present_pending = true;
            }
            _ => {}
        }
        app
    }

    pub fn t(&self, key: &str) -> &'static str {
        tr(self.prefs.lang, key)
    }

    /// Show an error to the person and keep a line about it in the local log.
    pub fn notify_error(&mut self, what: &str, e: &dyn std::fmt::Display) {
        notecraft_core::log::error(&format!("{what}: {e}"));
        let msg = format!("{}: {e}", self.t("error"));
        self.notify(msg);
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    // ------------------------------------------------------------- workspace

    pub fn open_workspace(&mut self, path: &Path, with_sample: bool) {
        if is_too_broad(path) {
            let msg = self.t("workspace_too_broad").to_string();
            self.notify(msg);
            return;
        }
        self.flush_save();
        match Workspace::open(path) {
            Ok(mut ws) => {
                if with_sample {
                    if let Err(e) = sample::populate(&mut ws) {
                        self.notify_error("workspace", &e);
                    }
                } else {
                    // a brand new, empty workspace opens on a tour of everything the app does
                    let fr = matches!(self.prefs.lang, Lang::Fr);
                    if let Err(e) = notecraft_core::tour::populate(&mut ws, fr) {
                        self.notify_error("workspace", &e);
                    }
                }
                let first = ws.children(None).first().map(|t| t.doc.clone());
                if !ws.warnings.is_empty() {
                    self.notify(ws.warnings.join("\n"));
                }
                self.ws = Some(ws);
                self.migrate_group_title();
                self.back.clear();
                self.forward.clear();
                self.tabs.clear();
                self.active_tab = 0;
                self.current = first.map(Target::Doc);
                self.editor = EditorState::default();
                self.watcher = self.ws.as_ref().map(|w| notecraft_core::watch::Watcher::new(&w.root));
                self.snapshot_base();
                if self.shot.is_none() {
                    self.prefs.last_workspace = Some(path.to_path_buf());
                    self.prefs.recent.retain(|p| p != path);
                    self.prefs.recent.insert(0, path.to_path_buf());
                    self.prefs.recent.truncate(6);
                }
            }
            Err(e) => self.notify_error("action", &e),
        }
    }

    pub fn pick_workspace(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title(self.t("open_folder")).pick_folder() {
            self.open_workspace(&dir, false);
        }
    }

    /// Copy the whole workspace into a folder the person picks, then work in the copy.
    pub fn move_workspace(&mut self) {
        let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) else { return };
        let Some(parent) = rfd::FileDialog::new().set_title(self.t("move_workspace")).pick_folder() else { return };
        let name = root.file_name().map(|n| n.to_owned()).unwrap_or_else(|| "NoteCraft".into());
        let dest = parent.join(name);
        if dest.starts_with(&root) {
            let msg = self.t("move_inside").to_string();
            return self.notify(msg);
        }
        if dest.exists() {
            let msg = self.t("move_exists").to_string();
            return self.notify(msg);
        }
        self.flush_save();
        match notecraft_core::sync::copy_tree(&root, &dest) {
            Ok(()) => {
                self.open_workspace(&dest, false);
                let msg = self.t("moved_ok").to_string();
                self.notify(msg);
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dest);
                self.notify(e.to_string());
            }
        }
    }

    /// Move a workspace folder to the Trash after asking, then forget it. Only a folder that looks like
    /// a workspace is moved, and never the home folder or Desktop, Documents, Downloads.
    pub fn trash_workspace(&mut self, path: &Path) {
        if is_too_broad(path) || !path.join(notecraft_core::workspace::PAGES_DIR).is_dir() {
            let msg = self.t("trash_not_ws").to_string();
            return self.notify(msg);
        }
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(self.t("trash_title"))
            .set_description(format!("{}\n\n{}", self.t("trash_text"), path.display()))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if answer != rfd::MessageDialogResult::Yes {
            return;
        }
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if self.ws.as_ref().is_some_and(|w| w.root.canonicalize().unwrap_or_else(|_| w.root.clone()) == canon) {
            self.close_workspace();
        }
        let index = notecraft_core::sync::index_file_for(path);
        match move_to_trash(path) {
            Ok(()) => {
                // the search index is only a cache of this folder
                for suffix in ["", "-wal", "-shm"] {
                    let mut f = index.as_os_str().to_owned();
                    f.push(suffix);
                    let _ = std::fs::remove_file(std::path::PathBuf::from(f));
                }
                self.prefs.recent.retain(|p| p != path);
                let msg = self.t("trashed_ok").to_string();
                self.notify(msg);
            }
            Err(e) => {
                let what = self.t("trash_failed");
                self.notify_error(what, &e);
            }
        }
    }

    pub fn close_workspace(&mut self) {
        self.flush_save();
        self.watcher = None;
        self.ws = None;
        self.current = None;
        self.tabs.clear();
        self.active_tab = 0;
        self.prefs.last_workspace = None;
    }

    /// Make sure the current target still exists (after trash/delete).
    pub(crate) fn validate_current(&mut self) {
        let live = |app: &Self| match (&app.ws, &app.current) {
            (Some(ws), Some(t)) => target_is_live(ws, t),
            _ => true,
        };
        if live(self) {
            return;
        }
        // the page of the active tab went away (archived, trashed, deleted): with other tabs open,
        // that tab closes and the neighbour shows; with a single tab it moves to the first page
        if self.tabs.len() > 1 {
            let i = self.active_tab.min(self.tabs.len() - 1);
            self.close_tab(i);
            if live(self) {
                return;
            }
        }
        let Some(ws) = &self.ws else { return };
        self.current = ws.children(None).first().map(|t| Target::Doc(t.doc.clone()));
    }

    // ---------------------------------------------------------------- saving

    pub fn mark_dirty(&mut self) {
        if let Some(t) = self.current.clone() {
            if let Some((d, _)) = &self.dirty {
                if *d != t {
                    self.flush_save();
                }
            }
            self.dirty = Some((t, Instant::now()));
        }
    }

    pub fn flush_save(&mut self) {
        let Some((t, _)) = self.dirty.take() else { return };
        let Some(ws) = self.ws.as_mut() else { return };
        let conflicts_before = ws.conflicts.len();
        let r = match &t {
            Target::Doc(d) => {
                ws.touch(d);
                ws.save(d)
            }
            Target::Template(db, tpl) => ws.save_template(db, tpl),
        };
        let new_conflict = ws.conflicts.len() > conflicts_before;
        if let Err(e) = r {
            self.notify_error("save", &e);
        } else if new_conflict {
            // the file changed elsewhere: our version was kept beside it
            self.notify(tr(self.prefs.lang, "conflict_created"));
        }
        if self.current.as_ref() == Some(&t) {
            self.snapshot_base();
        }
    }

    pub fn blocks_mut<'a>(ws: &'a mut Workspace, t: &Target) -> Option<&'a mut Vec<Block>> {
        match t {
            Target::Doc(d) => ws.blocks_mut(d),
            Target::Template(db, tpl) => ws.dbs.get_mut(db)?.templates.get_mut(tpl).map(|r| &mut r.blocks),
        }
    }

    // --------------------------------------------------------------- actions

    fn unique_title(ws: &Workspace, base: &str) -> String {
        let mut title = base.to_string();
        let mut n = 2;
        while ws.resolve_title(&title).is_some() {
            title = format!("{base} {n}");
            n += 1;
        }
        title
    }

    pub fn new_page(&mut self, parent: Option<String>) {
        self.flush_save();
        let untitled = self.t("untitled");
        let Some(ws) = self.ws.as_mut() else { return };
        let title = Self::unique_title(ws, untitled);
        let _ = title;
        match ws.create_page(parent.as_deref(), "") {
            Ok(id) => {
                if let Some(p) = &parent {
                    self.expanded.insert(DocId::Page(p.clone()).key());
                }
                self.navigate(Target::Doc(DocId::Page(id)));
                self.focus_title = true;
            }
            Err(e) => self.notify(e.to_string()),
        }
    }

    pub fn new_database(&mut self, parent: Option<String>, kind: ViewKind) -> Option<String> {
        self.flush_save();
        let lang = self.prefs.lang;
        let ws = self.ws.as_mut()?;
        let title = Self::unique_title(ws, tr(lang, "new_database"));
        match ws.create_database(parent.as_deref(), &title) {
            Ok(id) => {
                if let Some(d) = ws.dbs.get_mut(&id) {
                    if let Some(v) = d.schema.views.first_mut() {
                        v.kind = kind;
                        v.name = tr(
                            lang,
                            match kind {
                                ViewKind::Table => "view_table",
                                ViewKind::Board => "view_board",
                                ViewKind::List => "view_list",
                                ViewKind::Gallery => "view_gallery",
                                ViewKind::Calendar => "view_calendar",
                                ViewKind::Chart => "view_chart",
                                ViewKind::Timeline => "view_timeline",
                                ViewKind::Dashboard => "view_dashboard",
                            },
                        )
                        .to_string();
                    }
                }
                let _ = ws.save_schema(&id);
                // three empty rows, as a starting point
                for _ in 0..3 {
                    let _ = ws.add_row(&id, None, "");
                }
                Some(id)
            }
            Err(e) => {
                self.notify(e.to_string());
                None
            }
        }
    }

    fn insert_blocks(&mut self, index: usize, replace: bool, new: Vec<Block>) {
        let Some(t) = self.current.clone() else { return };
        let key = t.key();
        let Some(ws) = self.ws.as_mut() else { return };
        let Some(blocks) = Self::blocks_mut(ws, &t) else { return };
        let before = blocks.clone();
        let index = index.min(blocks.len());
        if replace && index < blocks.len() {
            blocks.remove(index);
        }
        let depth =
            blocks.get(index.saturating_sub(1)).map(|b| b.depth).unwrap_or(0).min(if index == 0 { 0 } else { 8 });
        let _ = depth;
        for (k, b) in new.into_iter().enumerate() {
            blocks.insert(index + k, b);
        }
        self.editor.record(&key, &before, None);
        self.mark_dirty();
    }

    pub fn handle_editor_actions(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Navigate(d) => self.navigate(Target::Doc(d)),
                Action::CreateMentioned(title) => {
                    self.flush_save();
                    let parent = match &self.current {
                        Some(Target::Doc(DocId::Page(p))) => Some(p.clone()),
                        _ => None,
                    };
                    if let Some(ws) = self.ws.as_mut() {
                        match ws.create_page(parent.as_deref(), &title) {
                            Ok(id) => self.navigate(Target::Doc(DocId::Page(id))),
                            Err(e) => self.notify(e.to_string()),
                        }
                    }
                }
                Action::OpenUrl(u) => open_external(&u),
                Action::SolveEquation(bid) => self.solve_equation(bid),
                Action::OpenFile(rel) => {
                    if let Some(ws) = &self.ws {
                        // only files inside the workspace: a `../` link must not reach elsewhere
                        let root = ws.root.canonicalize().unwrap_or_else(|_| ws.root.clone());
                        if let Ok(p) = ws.root.join(&rel).canonicalize() {
                            if p.starts_with(&root) {
                                open_external(&p.display().to_string());
                            }
                        }
                    }
                }
                Action::CreateSubpage { index, replace } => {
                    let parent = match &self.current {
                        Some(Target::Doc(DocId::Page(p))) => Some(p.clone()),
                        _ => None,
                    };
                    let untitled = self.t("untitled");
                    let Some(ws) = self.ws.as_mut() else { continue };
                    let title = Self::unique_title(ws, untitled);
                    match ws.create_page(parent.as_deref(), &title) {
                        Ok(id) => {
                            // the new page is listed in the Sub-pages section of this page: no link in the body
                            let _ = (index, replace);
                            self.flush_save();
                            self.navigate(Target::Doc(DocId::Page(id)));
                            self.focus_title = true;
                        }
                        Err(e) => self.notify(e.to_string()),
                    }
                }
                Action::InsertTemplate { index, replace } => {
                    self.ensure_templates();
                    self.finder = Some(Finder::new(FinderMode::Template { index, replace }));
                }
                Action::LinkToPage { block, at } => {
                    if let Some(t) = self.current.clone() {
                        self.finder = Some(Finder::new(FinderMode::Link { target: t, block, at }));
                    }
                }
                Action::PickFile { index, replace, image } => {
                    let mut dlg = rfd::FileDialog::new();
                    if image {
                        dlg = dlg.add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]);
                    }
                    if let Some(path) = dlg.pick_file() {
                        self.attach_files(vec![path], Some(index), replace, Some(image));
                    }
                }
                Action::Meeting(a) => self.meeting_action(a),
                Action::CreateDatabase { index, replace, kind } => {
                    let parent = match &self.current {
                        Some(Target::Doc(DocId::Page(p))) => Some(p.clone()),
                        _ => None,
                    };
                    if let Some(id) = self.new_database(parent, kind) {
                        let title = self.ws.as_ref().map(|w| w.dbs[&id].schema.title.clone()).unwrap_or_default();
                        // the database is shown in the page itself (and stays editable there)
                        self.insert_blocks(
                            index,
                            replace,
                            vec![Block::new(BlockKind::Database, title), Block::paragraph("")],
                        );
                        self.flush_save();
                    }
                }
            }
        }
    }

    pub fn handle_db_actions(&mut self, actions: Vec<DbAction>) {
        for a in actions {
            match a {
                DbAction::Open(d) => self.navigate(Target::Doc(d)),
                DbAction::EditTemplate(db, t) => self.navigate(Target::Template(db, t)),
                DbAction::Error(e) => self.notify(e),
            }
        }
    }

    pub fn trash_doc(&mut self, doc: &DocId) {
        // a page still live on the web goes to the trash only once unpublished
        if self.guard_live(GuardAction::Trash(doc.clone())) {
            return;
        }
        self.do_trash(doc);
    }

    pub(super) fn do_trash(&mut self, doc: &DocId) {
        self.flush_save();
        if let Some(ws) = self.ws.as_mut() {
            if let Err(e) = ws.trash_doc(doc) {
                self.notify(e.to_string());
            }
        }
        self.validate_current();
    }
}

/// With `NC_PROFILE=1`, every frame slower than 12 ms is reported on stderr (a tool for finding slow paths).
struct FrameTimer(Option<std::time::Instant>);

impl FrameTimer {
    fn start() -> Self {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let on = *ON.get_or_init(|| std::env::var_os("NC_PROFILE").is_some());
        FrameTimer(on.then(std::time::Instant::now))
    }
}

impl Drop for FrameTimer {
    fn drop(&mut self) {
        if let Some(t) = self.0 {
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            if ms > 12.0 {
                eprintln!("[profile] frame {ms:.1} ms");
            }
        }
    }
}

pub const SHORTCUTS: &[(&str, &str)] = &[
    ("⌘P / ⌘K", "Quick find"),
    ("⌘N", "New page"),
    ("⌘\\", "Show/hide sidebar"),
    ("⌘[  ⌘]", "Back / Forward"),
    ("⌘T  ⌘W", "New tab / Close tab"),
    ("⌃Tab  ⌃⇧Tab", "Next / Previous tab"),
    ("⌥⌘P", "Present the current page"),
    ("⌘J", "AI assistant"),
    ("⌘Z  ⌘⇧Z", "Undo / Redo"),
    ("⌘B ⌘I ⌘U", "Bold / Italic / Underline"),
    ("⌘⇧S  ⌘E", "Strikethrough / Inline code"),
    ("⌘K (selection)", "Link"),
    ("⌘D", "Duplicate block"),
    ("⌘⇧↑ ⌘⇧↓", "Move block"),
    ("⌘/", "Turn into"),
    ("⌘↵", "Check to-do / open toggle"),
    ("Esc", "Select block"),
    ("⌥← ⌥→", "Move by word"),
    ("⇧↓ (selection)", "Select several blocks; ← or → returns to the text"),
    ("Tab  ⇧Tab", "Indent / Outdent"),
    ("⇧↵", "Line break"),
    ("⌘⇧L", "Light / dark mode"),
    ("⌘=  ⌘-  ⌘0", "Zoom in / out / actual size"),
    ("⌘,", "Settings"),
];

impl NcApp {
    /// Handle a pick in the menu bar menu.
    fn poll_tray(&mut self, ctx: &egui::Context) {
        use crate::tray::TrayAction;
        let Some(action) = self.tray.as_ref().and_then(|t| t.poll()) else { return };
        if action == TrayAction::Quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        if self.ws.is_none() {
            return;
        }
        match action {
            TrayAction::NewPage => self.new_page(None),
            TrayAction::Search => self.finder = Some(Finder::new(FinderMode::Jump)),
            TrayAction::Open | TrayAction::Quit => {}
        }
    }
}

impl eframe::App for NcApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let _timer = FrameTimer::start();
        notecraft_ai::set_allow_remote_ollama(self.prefs.ai.allow_remote_ollama);
        notecraft_net::set_lockdown(self.prefs.ai.network_lockdown);
        // the built-in note templates become editable pages, once
        if self.ws.is_some() && !self.prefs.meeting.notes_seeded && !self.prefs.meeting.seed_failed {
            self.ensure_templates();
        }
        let ctx = ui.ctx().clone();
        let dark = match self.prefs.theme {
            ThemeChoice::Dark => true,
            ThemeChoice::Light => false,
            ThemeChoice::System => ctx.input(|i| i.raw.system_theme) == Some(egui::Theme::Dark),
        };
        if dark != self.pal.dark || ctx.cumulative_frame_nr() == 0 {
            self.pal = if dark { Palette::dark() } else { Palette::light() };
        }
        theme::apply(&ctx, &self.pal);
        let z = theme::clamp_zoom(self.prefs.zoom);
        if (ctx.zoom_factor() - z).abs() > 1e-3 {
            ctx.set_zoom_factor(z);
        }
        if self.present.is_none() {
            self.global_keys(&ctx);
        }
        self.poll_tray(&ctx);
        self.poll_disk(&ctx);
        self.poll_ai(&ctx);
        self.poll_meeting(&ctx);
        self.poll_backup(&ctx, false);
        if self.present_pending {
            self.present_pending = false;
            self.start_presentation(&ctx);
            let at = self.present_at;
            if let Some(p) = self.present.as_mut() {
                p.at = at.min(p.slides.len().saturating_sub(1));
            }
        }
        self.poll_reminders(&ctx);
        self.poll_import(&ctx);
        self.poll_publish(&ctx);
        self.subdomain_window(&ctx);
        self.pub_guard_window(&ctx);
        self.demo_window(&ctx);
        self.handle_dropped_files(&ctx);

        if self.present.is_some() {
            self.present_view(ui);
            self.screenshot_tick(&ctx);
            return;
        }
        if self.ws.is_none() {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(self.pal.bg)).show(ui, |ui| self.welcome(ui));
        } else {
            if self.prefs.sidebar_open {
                egui::Panel::left("sidebar")
                    .resizable(true)
                    .default_size(theme::SIDEBAR_WIDTH)
                    .size_range(200.0..=420.0)
                    .show_separator_line(false)
                    .frame(egui::Frame::NONE.fill(self.pal.sidebar).inner_margin(egui::Margin {
                        left: 8,
                        right: 8,
                        top: 0,
                        bottom: 8,
                    }))
                    .show(ui, |ui| self.sidebar(ui));
            }
            egui::Panel::top("topbar")
                .exact_size(44.0)
                .show_separator_line(false)
                .frame(egui::Frame::NONE.fill(self.pal.bg).inner_margin(egui::Margin {
                    left: 10,
                    right: 6,
                    top: 4,
                    bottom: 0,
                }))
                .show(ui, |ui| self.top_bar(ui));
            self.sync_tabs();
            if self.tabs.len() > 1 {
                egui::Panel::top("tabbar")
                    .exact_size(30.0)
                    .show_separator_line(false)
                    .frame(egui::Frame::NONE.fill(self.pal.bg).inner_margin(egui::Margin {
                        left: 10,
                        right: 6,
                        top: 0,
                        bottom: 2,
                    }))
                    .show(ui, |ui| self.tab_bar(ui));
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(self.pal.bg)).show(ui, |ui| self.main_view(ui));
        }

        self.finder_ui(&ctx);
        self.reminder_window(&ctx);
        self.reminders_list(&ctx);
        self.graph_window(&ctx);
        self.ai_panel_ui(&ctx);
        self.settings_ui(&ctx);
        self.move_dialog(&ctx);
        self.trash_ui(&ctx);
        self.history_ui(&ctx);
        self.vault_ui(&ctx);
        self.toast_ui(&ctx);

        // debounced saving
        if let Some((_, at)) = &self.dirty {
            if at.elapsed().as_millis() > 600 {
                self.flush_save();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(650));
            }
        }
        self.prefs.expanded = self.expanded.iter().cloned().collect();
        self.screenshot_tick(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.flush_save();
        if self.shot.is_none() {
            self.remember_tabs();
            eframe::set_value(storage, "prefs", &self.prefs);
        }
    }
}

/// Lowercased title → (doc, icon) for every live document (mentions).
pub fn title_map(ws: &Workspace) -> HashMap<String, (DocId, String)> {
    let mut m = HashMap::new();
    for (id, d) in &ws.dbs {
        for (rid, r) in &d.rows {
            if r.meta.trashed.is_none() && !r.meta.title.is_empty() {
                m.insert(r.meta.title.to_lowercase(), (DocId::Row(id.clone(), rid.clone()), r.meta.icon.clone()));
            }
        }
    }
    for (id, d) in &ws.dbs {
        if d.schema.trashed.is_none() {
            m.insert(d.schema.title.to_lowercase(), (DocId::Database(id.clone()), d.schema.icon.clone()));
        }
    }
    for (id, p) in &ws.pages {
        if p.meta.trashed.is_none() && !p.meta.title.is_empty() {
            m.insert(p.meta.title.to_lowercase(), (DocId::Page(id.clone()), p.meta.icon.clone()));
        }
    }
    m
}
