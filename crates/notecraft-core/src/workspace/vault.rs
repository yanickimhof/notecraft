//! Encrypted pages: passphrase, lock and unlock (split out of mod.rs, unchanged).

use super::*;

impl Workspace {
    // ------------------------------------------------------------------ vault

    pub fn is_encrypted(&self, id: &str) -> bool {
        self.pages.get(id).is_some_and(|p| p.meta.encrypted)
    }

    pub fn is_page_locked(&self, id: &str) -> bool {
        self.pages.get(id).is_some_and(|p| p.is_locked())
    }

    /// Whether the passphrase of this session is known (pages unlock without asking again).
    pub fn vault_is_open(&self) -> bool {
        self.vault_pass.is_some()
    }

    /// Whether the workspace already has encrypted pages (so a passphrase exists).
    pub fn has_vault(&self) -> bool {
        self.pages.values().any(|p| p.meta.encrypted)
    }

    /// Check a passphrase against an existing encrypted page (true when there is none yet).
    pub(super) fn passphrase_fits(&self, pass: &str) -> bool {
        match self.pages.values().find_map(|p| p.cipher.as_ref().filter(|_| p.meta.encrypted)) {
            Some(cipher) => crate::vault::open(pass, cipher).is_ok(),
            None => true,
        }
    }

    /// Unlock one encrypted page with the passphrase; the passphrase is kept for the session.
    pub fn unlock_page(&mut self, id: &str, pass: &str) -> Result<()> {
        let p = self.pages.get_mut(id).ok_or_else(|| Error::NotFound(id.into()))?;
        let Some(cipher) = p.cipher.clone().filter(|_| p.meta.encrypted) else { return Ok(()) };
        let (text, key) = crate::vault::open(pass, &cipher)?;
        p.blocks = blocks::from_markdown(&text);
        p.key = Some(key);
        self.vault_pass = Some(pass.to_string());
        Ok(())
    }

    /// Unlock a page with the session passphrase, if known. Returns whether the page is readable.
    pub fn unlock_page_if_known(&mut self, id: &str) -> bool {
        if !self.is_page_locked(id) {
            return true;
        }
        match self.vault_pass.clone() {
            Some(pw) => self.unlock_page(id, &pw).is_ok(),
            None => false,
        }
    }

    /// Encrypt a page: its body becomes unreadable without the passphrase. Earlier plain
    /// copies of the page in the automatic backups are erased. The first encrypted page
    /// sets the passphrase; later ones must use the same.
    pub fn encrypt_page(&mut self, id: &str, pass: &str) -> Result<()> {
        if pass.chars().count() < 8 {
            return Err(Error::Invalid("the passphrase needs at least 8 characters".into()));
        }
        if self.is_encrypted(id) {
            return Ok(());
        }
        if !self.passphrase_fits(pass) {
            return Err(Error::Invalid("wrong passphrase".into()));
        }
        self.ensure_unlocked(&DocId::Page(id.to_string()))?;
        let key = crate::vault::VaultKey::new(pass)?;
        let p = self.pages.get_mut(id).ok_or_else(|| Error::NotFound(id.into()))?;
        p.meta.encrypted = true;
        p.key = Some(key);
        p.touch();
        self.vault_pass = Some(pass.to_string());
        self.save_page(id)?;
        crate::backup::purge_page(&self.snapshot_dir(), id);
        Ok(())
    }

    /// Remove the encryption of an unlocked page (it is stored as plain Markdown again).
    pub fn decrypt_page(&mut self, id: &str) -> Result<()> {
        let p = self.pages.get_mut(id).ok_or_else(|| Error::NotFound(id.into()))?;
        if !p.meta.encrypted {
            return Ok(());
        }
        if p.key.is_none() {
            return Err(Error::Invalid("unlock the page first".into()));
        }
        p.meta.encrypted = false;
        p.key = None;
        p.cipher = None;
        p.touch();
        self.save_page(id)
    }

    /// Forget the passphrase and close every encrypted page. Unsaved text must be saved first.
    pub fn lock_vault(&mut self) {
        self.vault_pass = None;
        for p in self.pages.values_mut().filter(|p| p.meta.encrypted) {
            if let Ok(text) = p.render() {
                p.cipher = crate::frontmatter::split(&text).1.to_string().into();
            }
            p.blocks.clear();
            p.key = None;
        }
        let ids: Vec<String> = self.pages.values().filter(|p| p.meta.encrypted).map(|p| p.meta.id.clone()).collect();
        for id in ids {
            self.reindex_doc(&DocId::Page(id));
        }
    }

    /// Workspace files that changed on disk since this process last read or
    /// wrote them (edits from iCloud Drive, the MCP server, an editor, git).
    pub fn external_changes(&self) -> Vec<PathBuf> {
        sync::diff_stamps(&self.stamps, &sync::scan_stamps(&self.root))
    }

    /// The document a workspace file belongs to (as currently loaded).
    pub fn doc_for_path(&self, path: &Path) -> Option<DocId> {
        for (id, p) in &self.pages {
            if p.path.as_deref() == Some(path) {
                return Some(DocId::Page(id.clone()));
            }
        }
        for (id, d) in &self.dbs {
            if path == d.dir.join("schema.json") {
                return Some(DocId::Database(id.clone()));
            }
            for (rid, r) in &d.rows {
                if r.path.as_deref() == Some(path) {
                    return Some(DocId::Row(id.clone(), rid.clone()));
                }
            }
        }
        None
    }

    /// The file of a page or row has changed on disk while the person has
    /// unsaved edits: merge both block by block (see [`blocks::merge_blocks`]).
    /// `base` is the version last read or written. Returns `true` when merged
    /// and saved, `false` when both sides changed the same block — then the
    /// local version is kept in a conflicted copy and the conflict recorded.
    pub fn merge_external(&mut self, doc: &DocId, base: &[Block]) -> Result<bool> {
        let (path, mine) = match doc {
            DocId::Page(id) => {
                let p = self.pages.get(id).ok_or_else(|| Error::NotFound(id.clone()))?;
                if p.meta.encrypted {
                    // two encrypted versions cannot be merged: keep both
                    let mine_text = p.render()?;
                    if let Some(path) = p.path.clone().filter(|p| p.exists()) {
                        self.divert_to_conflict_copy(doc, &path, &mine_text)?;
                    }
                    return Ok(false);
                }
                (p.path.clone(), p.blocks.clone())
            }
            DocId::Row(db, r) => {
                let row = self.dbs.get(db).and_then(|d| d.rows.get(r)).ok_or_else(|| Error::NotFound(r.clone()))?;
                (row.path.clone(), row.blocks.clone())
            }
            DocId::Database(_) => return Ok(true),
        };
        let Some(path) = path.filter(|p| p.exists()) else {
            self.save(doc)?;
            return Ok(true);
        };
        let text = fs::read_to_string(&path)?;
        let (theirs_blocks, merged) = match doc {
            DocId::Page(_) => {
                let t = Page::parse(&text, "")?;
                let m = blocks::merge_blocks(base, &mine, &t.blocks);
                if let (Some(m), DocId::Page(id)) = (&m, doc) {
                    let p = self.pages.get_mut(id).unwrap();
                    let title = p.meta.title.clone();
                    p.meta = PageMetaMerge::take(t.meta, &title);
                    p.blocks = m.clone();
                }
                (t.blocks.len(), m)
            }
            DocId::Row(db, r) => {
                let t = Row::parse(&text)?;
                let m = blocks::merge_blocks(base, &mine, &t.blocks);
                if let Some(m) = &m {
                    let row = self.dbs.get_mut(db).unwrap().rows.get_mut(r).unwrap();
                    let title = row.meta.title.clone();
                    row.meta = crate::database::RowMeta { title, ..t.meta };
                    row.blocks = m.clone();
                }
                (t.blocks.len(), m)
            }
            DocId::Database(_) => unreachable!(),
        };
        let _ = theirs_blocks;
        match merged {
            Some(_) => {
                self.remember_stamp(&path);
                self.save(doc)?;
                Ok(true)
            }
            None => {
                let mine_text = match doc {
                    DocId::Page(id) => self.pages[id].render()?,
                    DocId::Row(db, r) => self.dbs[db].rows[r].render()?,
                    DocId::Database(_) => unreachable!(),
                };
                self.divert_to_conflict_copy(doc, &path, &mine_text)?;
                Ok(false)
            }
        }
    }

    /// Reload when files changed on disk; returns the changed paths.
    pub fn sync_from_disk(&mut self) -> Result<Vec<PathBuf>> {
        let changed = self.external_changes();
        if !changed.is_empty() {
            self.reload()?;
        }
        Ok(changed)
    }

    /// Hint: the sync service the folder seems to be in (iCloud Drive, kDrive, Dropbox…).
    pub fn synced_folder(&self) -> Option<&'static str> {
        sync::synced_folder_kind(&self.root)
    }

    /// True when the folder is synced and is also a git repository (warned about in Settings).
    pub fn git_in_synced_folder(&self) -> bool {
        self.synced_folder().is_some() && sync::has_git(&self.root)
    }

    pub(crate) fn remember_stamp(&mut self, path: &Path) {
        if let Some(s) = FileStamp::of(path) {
            self.stamps.insert(path.to_path_buf(), s);
        }
    }

    pub(crate) fn forget_stamp(&mut self, path: &Path) {
        self.stamps.remove(path);
    }

    pub(crate) fn recorded_stamp(&self, path: &Path) -> Option<FileStamp> {
        self.stamps.get(path).copied()
    }

    pub(super) fn note_written(&mut self, path: &Path) {
        self.remember_stamp(path);
    }

    /// Write a document even if its file changed on disk (used once the
    /// person chose which version to keep).
    pub(crate) fn force_save(&mut self, doc: &DocId) -> Result<()> {
        match doc {
            DocId::Page(id) => self.write_page(id, true),
            DocId::Database(id) => self.write_schema(id, true),
            DocId::Row(db, r) => self.write_row(db, r, true),
        }
    }

    pub fn name(&self) -> String {
        let name = self.root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Workspace".into());
        // the folder is ASCII (NoteCraft); the name shown is the brand spelling
        if name.eq_ignore_ascii_case("notecraft") {
            "NøteCraft".to_string()
        } else {
            name
        }
    }

    pub(super) fn all_docs(&self) -> Vec<DocId> {
        let mut v: Vec<DocId> = self.pages.keys().map(|k| DocId::Page(k.clone())).collect();
        for (id, db) in &self.dbs {
            v.push(DocId::Database(id.clone()));
            for r in db.rows.keys() {
                v.push(DocId::Row(id.clone(), r.clone()));
            }
        }
        v
    }

    pub(super) fn index_doc(&self, doc: &DocId) -> IndexDoc {
        let (kind, body, links, updated) = match doc {
            DocId::Page(id) => {
                let p = &self.pages[id];
                if p.meta.encrypted {
                    // never put the text of an encrypted page in the search index
                    ("page", String::new(), vec![], p.meta.updated.clone())
                } else {
                    ("page", blocks::plain_text(&p.blocks), mentions_of(&p.blocks), p.meta.updated.clone())
                }
            }
            DocId::Database(id) => {
                let s = &self.dbs[id].schema;
                ("database", s.description.clone(), vec![], s.updated.clone())
            }
            DocId::Row(db, r) => {
                let d = &self.dbs[db];
                let row = &d.rows[r];
                let mut body = String::new();
                for p in &d.schema.properties {
                    if p.kind != PropKind::Title {
                        let v = self.display_value(db, row, p);
                        if !v.is_empty() {
                            body.push_str(&v);
                            body.push('\n');
                        }
                    }
                }
                body.push_str(&blocks::plain_text(&row.blocks));
                ("row", body, mentions_of(&row.blocks), row.meta.updated.clone())
            }
        };
        IndexDoc {
            key: doc.key(),
            kind: kind.into(),
            title: self.title(doc),
            icon: self.icon(doc),
            body,
            trashed: !self.is_live(doc),
            updated,
            links,
        }
    }

    pub(super) fn reindex_doc(&mut self, doc: &DocId) {
        if self.exists(doc) {
            let d = self.index_doc(doc);
            let _ = self.index.upsert(&d);
        } else {
            let _ = self.index.remove(&doc.key());
        }
    }
}
