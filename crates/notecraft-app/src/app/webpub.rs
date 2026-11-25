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
        let secret = self
            .pubs
            .get(&key, id)
            .map(|e| e.secret.clone())
            .unwrap_or_else(notecraft_ai::publish::new_secret);
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

    pub fn pub_unpublish(&mut self, id: &str) {
        if self.publishing.is_some() {
            let m = self.t("pub_busy").to_string();
            return self.notify(m);
        }
        let Some(token) = keychain::load_item(keychain::PUBLISH_TOKEN_ACCOUNT) else {
            let m = self.t("pub_need_token").to_string();
            return self.notify(m);
        };
        let Some(entry) = self.pubs.get(&self.ws_key(), id).cloned() else { return };
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
            PubAct::Unpublish => self.pub_unpublish(id),
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
            }
            Msg::NeedSubdomain { account } => {
                self.subdomain = Some(SubdomainDlg { account, input: String::new(), error: String::new(), busy: false });
            }
            Msg::SubdomainSet => {
                self.subdomain = None;
                if let Some((id, children)) = self.pub_retry.take() {
                    self.pub_start(&id, children);
                }
            }
            Msg::Failed(e) => {
                if let Some(d) = self.subdomain.as_mut() {
                    d.busy = false;
                    d.error = e;
                } else {
                    self.notify_error("publish", &e);
                }
            }
        }
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
                let r = ui.add(
                    egui::TextEdit::singleline(&mut dlg.input).desired_width(f32::INFINITY).hint_text("my-name"),
                );
                dlg.input = dlg.input.to_lowercase();
                let name = dlg.input.trim();
                ui.label(
                    RichText::new(format!("https://notecraft-pub.{}.workers.dev", if name.is_empty() { "…" } else { name }))
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
