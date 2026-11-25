//! The window side of publishing: preview, publish, update and unpublish a page, and the one
//! question asked the first time (the name of the web address).

use super::*;
use crate::publish::{self, Msg};
use notecraft_ai::keychain;

/// What the "Publish" menu asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum PubAct {
    Preview,
    Publish(bool),
    Update,
    CopyLink,
    Open,
    Unpublish,
}

/// What was about to happen to a page that is still live on the web.
#[derive(Debug, Clone)]
pub enum GuardAction {
    Trash(DocId),
    Archive(String),
    DeleteForever(DocId),
    EmptyTrash,
}

/// The question asked before such a removal: unpublish first, or leave everything as it is.
pub struct PubGuard {
    pub action: GuardAction,
    /// Root pages of the publications that must come down first.
    pub roots: Vec<String>,
    pub busy: bool,
    pub error: String,
}

/// The first-time question: the name in the web address (`notecraft-pub.<name>.workers.dev`).
pub struct SubdomainDlg {
    pub account: String,
    pub input: String,
    pub error: String,
    pub busy: bool,
}

impl NcApp {
    fn ws_key(&self) -> String {
        self.ws.as_ref().map(|w| w.root.display().to_string()).unwrap_or_default()
    }

    fn lang_code(&self) -> &'static str {
        match self.prefs.lang {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// Open the page as it will look on the web, from a local file. Nothing is sent.
    pub fn pub_preview(&mut self, id: &str) {
        self.flush_save();
        let lang = self.lang_code();
        let Some(ws) = &self.ws else { return };
        match publish::build(ws, id, false, "preview", lang, true) {
            Ok(b) => {
                let Some((_, html)) = b.files.last() else { return };
                let dir = std::env::temp_dir().join("notecraft-preview");
                let _ = std::fs::create_dir_all(&dir);
                let safe: String = id.chars().filter(char::is_ascii_alphanumeric).collect();
                let path = dir.join(format!("{safe}.html"));
                // Only in the local preview: a button and the F key switch the page to full screen.
                const FULL: &str = "<button id=\"nc-full\" title=\"F\" style=\"position:fixed;right:14px;top:12px;z-index:9;padding:6px 12px;border:1px solid #8884;border-radius:8px;background:#fff8;color:inherit;font:13px system-ui;cursor:pointer;backdrop-filter:blur(6px)\">⛶</button><script>(function(){var b=document.getElementById('nc-full');function t(){if(document.fullscreenElement){document.exitFullscreen()}else{document.documentElement.requestFullscreen()}}b.onclick=t;document.addEventListener('keydown',function(e){if((e.key==='f'||e.key==='F')&&!e.metaKey&&!e.ctrlKey){t()}});document.addEventListener('fullscreenchange',function(){b.style.display=document.fullscreenElement?'none':''})})()</script>";
                let page = String::from_utf8_lossy(html).replacen("</body>", &format!("{FULL}</body>"), 1);
                match std::fs::write(&path, page.as_bytes()) {
                    Ok(()) => open_external(&path.display().to_string()),
                    Err(e) => self.notify_error("preview", &e),
                }
            }
            Err(e) if e == "encrypted" => {
                let m = self.t("pub_locked").to_string();
                self.notify(m)
            }
            Err(e) => self.notify_error("preview", &e),
        }
    }

    /// Publish a page (or a page with its sub pages), or send its new version to the same address.
    pub fn pub_start(&mut self, id: &str, with_children: bool) {
        if self.publishing.is_some() {
            let m = self.t("pub_busy").to_string();
            return self.notify(m);
        }
        let Some(token) = keychain::load_item(keychain::PUBLISH_TOKEN_ACCOUNT) else {
            let m = self.t("pub_need_token").to_string();
            return self.notify(m);
        };
        self.flush_save();
        let key = self.ws_key();
        let secret =
            self.pubs.get(&key, id).map(|e| e.secret.clone()).unwrap_or_else(notecraft_ai::publish::new_secret);
        let lang = self.lang_code();
        let Some(ws) = &self.ws else { return };
        let built = match publish::build(ws, id, with_children, &secret, lang, false) {
            Ok(b) => b,
            Err(e) if e == "encrypted" => {
                let m = self.t("pub_locked").to_string();
                return self.notify(m);
            }
            Err(e) => return self.notify_error("publish", &e),
        };
        let skipped = built.skipped.len();
        let cached = self.pubs.setup.clone();
        let root = id.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(publish::run_publish(&token, cached, &root, with_children, &secret, built));
        });
        self.publishing = Some(rx);
        self.pub_retry = Some((id.to_string(), with_children));
        let mut m = self.t("pub_sending").to_string();
        if skipped > 0 {
            m = format!("{m} ({skipped} {})", self.t("pub_skipped"));
        }
        self.notify(m);
    }

    /// Start taking a publication down. True when the job started.
    pub fn pub_unpublish(&mut self, id: &str) -> bool {
        if self.publishing.is_some() {
            let m = self.t("pub_busy").to_string();
            self.notify(m);
            return false;
        }
        let Some(token) = keychain::load_item(keychain::PUBLISH_TOKEN_ACCOUNT) else {
            let m = self.t("pub_need_token").to_string();
            self.notify(m);
            return false;
        };
        let Some(entry) = self.pubs.get(&self.ws_key(), id).cloned() else { return false };
        let cached = self.pubs.setup.clone();
        let root = id.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(publish::run_unpublish(&token, cached, &root, &entry.secret));
        });
        self.publishing = Some(rx);
        self.pub_retry = None;
        let m = self.t("pub_removing").to_string();
        self.notify(m);
        true
    }

    /// Run what the "Publish" menu asked for.
    pub fn pub_act(&mut self, ctx: &egui::Context, id: &str, act: PubAct) {
        let entry = self.pubs.get(&self.ws_key(), id).cloned();
        match act {
            PubAct::Preview => self.pub_preview(id),
            PubAct::Publish(children) => self.pub_start(id, children),
            PubAct::Update => {
                let children = entry.map(|e| e.with_children).unwrap_or(false);
                self.pub_start(id, children);
            }
            PubAct::CopyLink => {
                if let Some(e) = entry {
                    ctx.copy_text(e.url);
                    let m = self.t("pub_link_copied").to_string();
                    self.notify(m);
                }
            }
            PubAct::Open => {
                if let Some(e) = entry {
                    open_external(&e.url);
                }
            }
            PubAct::Unpublish => {
                self.pub_unpublish(id);
            }
        }
    }

    /// Pick up the end of a publishing job.
    pub fn poll_publish(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.publishing else { return };
        let msg = match rx.try_recv() {
            Ok(m) => m,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                return;
            }
            Err(_) => Msg::Failed("publishing stopped".into()),
        };
        self.publishing = None;
        let key = self.ws_key();
        match msg {
            Msg::Published { root, entry, setup } => {
                ctx.copy_text(entry.url.clone());
                self.pubs.setup = Some(setup);
                self.pubs.set(&key, &root, entry);
                self.pubs.save();
                self.pub_retry = None;
                let m = self.t("pub_done").to_string();
                self.notify(m);
            }
            Msg::Unpublished { root } => {
                self.pubs.remove(&key, &root);
                self.pubs.save();
                let m = self.t("pub_removed").to_string();
                self.notify(m);
                self.guard_continue();
            }
            Msg::NeedSubdomain { account } => {
                self.subdomain =
                    Some(SubdomainDlg { account, input: String::new(), error: String::new(), busy: false });
            }
            Msg::SubdomainSet => {
                self.subdomain = None;
                if let Some((id, children)) = self.pub_retry.take() {
                    self.pub_start(&id, children);
                }
            }
            Msg::Failed(e) => {
                if let Some(g) = self.pub_guard.as_mut().filter(|g| g.busy) {
                    // the page stays as it is: nothing is removed while its link may still work
                    g.busy = false;
                    g.error = e;
                } else if let Some(d) = self.subdomain.as_mut() {
                    d.busy = false;
                    d.error = e;
                } else {
                    self.notify_error("publish", &e);
                }
            }
        }
    }

    /// Root pages of the live publications that contain any of these documents.
    fn live_roots_for(&self, docs: &[DocId]) -> Vec<String> {
        let Some(ws) = self.ws.as_ref() else { return Vec::new() };
        let mut ids = std::collections::HashSet::new();
        for d in docs {
            ids.insert(d.id().to_string());
            for x in ws.descendants(d) {
                ids.insert(x.id().to_string());
            }
        }
        let Some(sites) = self.pubs.sites.get(&self.ws_key()) else { return Vec::new() };
        sites
            .iter()
            .filter(|(root, e)| ids.contains(*root) || e.pages.iter().any(|(p, _)| ids.contains(p)))
            .map(|(root, _)| root.clone())
            .collect()
    }

    /// True (and the question is opened) when the action would leave a page live on the web.
    pub fn guard_live(&mut self, action: GuardAction) -> bool {
        let docs: Vec<DocId> = match &action {
            GuardAction::Trash(d) | GuardAction::DeleteForever(d) => vec![d.clone()],
            GuardAction::Archive(id) => vec![DocId::Page(id.clone())],
            GuardAction::EmptyTrash => {
                self.ws.as_ref().map(|w| w.trash().into_iter().map(|(d, _)| d).collect()).unwrap_or_default()
            }
        };
        let roots = self.live_roots_for(&docs);
        if roots.is_empty() {
            return false;
        }
        self.pub_guard = Some(PubGuard { action, roots, busy: false, error: String::new() });
        true
    }

    pub(super) fn do_archive(&mut self, id: &str) {
        self.flush_save();
        let res = self.ws.as_mut().map(|w| w.set_archived(id, true));
        if let Some(Err(e)) = res {
            self.notify(e.to_string());
        }
        self.validate_current();
        let msg = self.t("archived_done").to_string();
        self.notify(msg);
    }

    fn run_guarded(&mut self, action: GuardAction) {
        match action {
            GuardAction::Trash(d) => self.do_trash(&d),
            GuardAction::Archive(id) => self.do_archive(&id),
            GuardAction::DeleteForever(d) => {
                if let Some(Err(e)) = self.ws.as_mut().map(|w| w.delete_forever(&d)) {
                    self.notify(e.to_string());
                }
            }
            GuardAction::EmptyTrash => {
                if let Some(Err(e)) = self.ws.as_mut().map(|w| w.empty_trash()) {
                    self.notify(e.to_string());
                }
            }
        }
    }

    /// A publication just came down: take the next one down, or do what was asked.
    fn guard_continue(&mut self) {
        let Some(mut g) = self.pub_guard.take() else { return };
        if !g.busy {
            self.pub_guard = Some(g);
            return;
        }
        let key = self.ws_key();
        g.roots.retain(|r| self.pubs.get(&key, r).is_some());
        if let Some(root) = g.roots.first().cloned() {
            if !self.pub_unpublish(&root) {
                g.busy = false;
                g.error = self.t("pubg_cannot").to_string();
            }
            self.pub_guard = Some(g);
            return;
        }
        self.run_guarded(g.action);
    }

    /// The question itself: unpublish and go on, or keep the page as it is.
    pub fn pub_guard_window(&mut self, ctx: &egui::Context) {
        let Some(mut g) = self.pub_guard.take() else { return };
        let pal = self.pal;
        let lang = self.prefs.lang;
        let (text, ok) = match g.action {
            GuardAction::Trash(_) => ("pubg_text_trash", "pubg_ok_trash"),
            GuardAction::Archive(_) => ("pubg_text_archive", "pubg_ok_archive"),
            GuardAction::DeleteForever(_) | GuardAction::EmptyTrash => ("pubg_text_delete", "pubg_ok_trash"),
        };
        let titles: Vec<String> = g
            .roots
            .iter()
            .map(|r| {
                self.ws.as_ref().and_then(|w| w.pages.get(r)).map(|p| p.meta.title.clone()).unwrap_or_else(|| r.clone())
            })
            .collect();
        let (mut go, mut cancel) = (false, false);
        egui::Window::new("pub_guard")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(300.0);
                ui.label(RichText::new(tr(lang, "pubg_title")).size(15.0).strong());
                ui.add_space(4.0);
                ui.label(RichText::new(tr(lang, text)).size(13.0).color(pal.text_secondary));
                ui.add_space(8.0);
                for t in &titles {
                    egui::Frame::new()
                        .fill(pal.code_bg)
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(egui::Label::new(RichText::new(t).size(13.0).strong()).truncate());
                        });
                }
                if !g.error.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&g.error).size(12.0).color(pal.danger));
                }
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let main = egui::Button::new(RichText::new(tr(lang, ok)).size(13.0).color(pal.bg))
                        .fill(pal.text)
                        .corner_radius(egui::CornerRadius::same(6));
                    if ui.add_enabled(!g.busy, main).clicked() {
                        go = true;
                    }
                    let keep = egui::Button::new(RichText::new(tr(lang, "pubg_keep")).size(13.0)).frame(false);
                    if ui.add_enabled(!g.busy, keep).clicked() {
                        cancel = true;
                    }
                    if g.busy {
                        ui.spinner();
                    }
                });
            });
        if cancel {
            return;
        }
        if go && self.publishing.is_none() {
            let key = self.ws_key();
            g.roots.retain(|r| self.pubs.get(&key, r).is_some());
            match g.roots.first().cloned() {
                Some(root) => {
                    if self.pub_unpublish(&root) {
                        g.busy = true;
                        g.error.clear();
                    } else {
                        g.error = self.t("pubg_cannot").to_string();
                    }
                }
                None => {
                    self.run_guarded(g.action);
                    return;
                }
            }
        }
        self.pub_guard = Some(g);
    }

    /// The question asked once: the name of the web address.
    pub fn subdomain_window(&mut self, ctx: &egui::Context) {
        let Some(mut dlg) = self.subdomain.take() else { return };
        let pal = self.pal;
        let lang = self.prefs.lang;
        let (mut create, mut cancel) = (false, false);
        egui::Window::new(tr(lang, "sub_title"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new(tr(lang, "sub_text")).size(13.0).color(pal.text_secondary));
                ui.add_space(8.0);
                let r = ui
                    .add(egui::TextEdit::singleline(&mut dlg.input).desired_width(f32::INFINITY).hint_text("my-name"));
                dlg.input = dlg.input.to_lowercase();
                let name = dlg.input.trim();
                ui.label(
                    RichText::new(format!(
                        "https://notecraft-pub.{}.workers.dev",
                        if name.is_empty() { "…" } else { name }
                    ))
                    .size(12.0)
                    .color(pal.text_faint),
                );
                if !dlg.error.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&dlg.error).size(12.0).color(pal.danger));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ok = notecraft_ai::publish::valid_subdomain(name) && !dlg.busy;
                    if ui.add_enabled(ok, egui::Button::new(tr(lang, "sub_create"))).clicked()
                        || (ok && r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                    {
                        create = true;
                    }
                    if ui.button(tr(lang, "cancel")).clicked() {
                        cancel = true;
                    }
                    if dlg.busy {
                        ui.spinner();
                    }
                });
            });
        if cancel {
            self.pub_retry = None;
            return;
        }
        if create && self.publishing.is_none() {
            if let Some(token) = keychain::load_item(keychain::PUBLISH_TOKEN_ACCOUNT) {
                let (account, name) = (dlg.account.clone(), dlg.input.trim().to_string());
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(publish::run_set_subdomain(&token, &account, &name));
                });
                self.publishing = Some(rx);
                dlg.busy = true;
                dlg.error.clear();
            }
        }
        self.subdomain = Some(dlg);
    }
}
