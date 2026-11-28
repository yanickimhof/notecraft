//! Importing, attaching, exporting and emailing pages.

use super::*;

impl NcApp {
    /// Copy files into assets/ and insert image or file blocks.
    pub fn attach_files(
        &mut self,
        paths: Vec<PathBuf>,
        index: Option<usize>,
        replace: bool,
        force_image: Option<bool>,
    ) {
        let Some(t) = self.current.clone() else { return };
        if matches!(t, Target::Doc(DocId::Database(_))) {
            return;
        }
        let Some(ws) = self.ws.as_mut() else { return };
        let mut new = Vec::new();
        for p in paths {
            match ws.add_asset(&p) {
                Ok(rel) => {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let image = force_image.unwrap_or_else(|| is_image(&p)) && is_image(&p);
                    new.push(if image {
                        Block::new(BlockKind::Image { path: rel }, "")
                    } else {
                        Block::new(BlockKind::File { path: rel }, name)
                    });
                }
                Err(e) => self.toast = Some((e.to_string(), Instant::now())),
            }
        }
        if new.is_empty() {
            return;
        }
        let len = Self::blocks_mut(ws, &t).map(|b| b.len()).unwrap_or(0);
        let at = index.unwrap_or_else(|| self.editor.drop_hint().map(|i| i + 1).unwrap_or(len));
        self.insert_blocks(at, replace, new);
    }

    /// Pick a file (Markdown, HTML, Word, Evernote, CSV, Joplin, or a zip export) and import it.
    pub fn import_file(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .set_title(self.t("import_file"))
            .add_filter("Notes", &["md", "markdown", "txt", "html", "htm", "docx", "enex", "csv", "zip", "jex"])
            .pick_file()
        else {
            return;
        };
        self.import_path(&file);
    }

    /// Pick a folder (a Markdown folder, an Obsidian vault, an unzipped export…) and import it.
    pub fn import_folder(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title(self.t("import_folder")).pick_folder() else { return };
        self.import_path(&dir);
    }

    /// Import on another thread, into a second handle on the same folder: the window stays responsive,
    /// and the new pages appear through the usual disk watcher when it is done.
    pub(super) fn import_path(&mut self, path: &std::path::Path) {
        if self.importing.is_some() {
            let msg = self.t("import_busy").to_string();
            return self.notify(msg);
        }
        self.flush_save();
        let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) else { return };
        let path = path.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = notecraft_core::Workspace::open(&root)
                .and_then(|mut ws| notecraft_core::import_any::import_any(&mut ws, &path, None))
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        self.importing = Some(rx);
        let msg = self.t("importing").to_string();
        self.notify(msg);
    }

    /// Pick up the end of a running import.
    pub fn poll_import(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.importing else { return };
        let done = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Err(_) => Err("import stopped".to_string()),
        };
        self.importing = None;
        match done {
            Ok((source, rep)) => {
                if let Some(ws) = self.ws.as_mut() {
                    if let Err(e) = ws.sync_from_disk() {
                        self.notify_error("import", &e);
                    }
                }
                let msg = format!(
                    "{} {}",
                    tr(self.prefs.lang, "imported"),
                    notecraft_core::import_any::summary(source, &rep)
                );
                if let Some(root) = rep.root_page {
                    self.navigate(Target::Doc(DocId::Page(root)));
                }
                self.notify(msg);
            }
            Err(e) => self.notify(e),
        }
    }

    pub fn export_doc(&mut self, doc: &DocId) {
        self.flush_save();
        let Some(dir) = rfd::FileDialog::new().set_title(self.t("export_markdown")).pick_folder() else { return };
        let Some(ws) = &self.ws else { return };
        match import_export::export_doc(ws, doc, &dir, true) {
            Ok(path) => {
                reveal(&path);
                let msg = format!("{} {}", self.t("exported"), path.display());
                self.notify(msg);
            }
            Err(e) => self.notify(e.to_string()),
        }
    }

    pub fn export_page_format(&mut self, doc: &DocId, format: notecraft_core::render::Format) {
        let Some(ws) = &self.ws else { return };
        let Some(dir) = rfd::FileDialog::new().set_title(self.t("export_markdown")).pick_folder() else { return };
        match import_export::export_page_as(ws, doc, &dir, format) {
            Ok(path) => {
                reveal(&path);
                let msg = format!("{} {}", self.t("exported"), path.display());
                self.notify(msg);
            }
            Err(e) => self.notify_error("export", &e),
        }
    }

    /// Start an email in the default mail application (Mail, Outlook...) with the page's title as the
    /// subject and the page's text as the body (when short enough); the formatted page is also put on the
    /// clipboard, to paste over it (⌘V).
    pub fn email_page(&mut self, doc: &DocId) {
        self.flush_save();
        let Some(ws) = &self.ws else { return };
        match import_export::email_parts(ws, doc) {
            Ok((subject, html, text)) => {
                let copied =
                    arboard::Clipboard::new().ok().is_some_and(|mut c| c.set_html(html, Some(text.clone())).is_ok());
                #[cfg(target_os = "macos")]
                let ok = std::process::Command::new("open")
                    .arg(mailto_url(&subject, &text))
                    .status()
                    .is_ok_and(|s| s.success());
                #[cfg(not(target_os = "macos"))]
                let ok = false;
                let key = match (copied, ok) {
                    (true, true) => "email_opened",
                    (true, false) => "email_copied",
                    _ => "email_failed",
                };
                let msg = self.t(key).to_string();
                self.notify(msg);
            }
            Err(e) => self.notify_error("export", &e),
        }
    }

    pub fn export_workspace(&mut self) {
        self.flush_save();
        let Some(dir) = rfd::FileDialog::new().set_title(self.t("export_workspace")).pick_folder() else { return };
        let Some(ws) = &self.ws else { return };
        match import_export::export_workspace(ws, &dir) {
            Ok(n) => {
                reveal(&dir);
                let msg = format!("{} {} ({n})", self.t("exported"), dir.display());
                self.notify(msg);
            }
            Err(e) => self.notify(e.to_string()),
        }
    }
}

/// Longest page text (characters) put in the body of the `mailto:` link; mail applications refuse very long links.
const MAILTO_BODY_MAX: usize = 6000;

/// A `mailto:` link with the subject and, when short enough, the page's text as the body.
fn mailto_url(subject: &str, text: &str) -> String {
    let mut url = format!("mailto:?subject={}", percent_encode(subject));
    if !text.trim().is_empty() && text.chars().count() <= MAILTO_BODY_MAX {
        url.push_str("&body=");
        url.push_str(&percent_encode(text.trim()));
    }
    url
}

/// Percent-encoding for a `mailto:` value.
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
