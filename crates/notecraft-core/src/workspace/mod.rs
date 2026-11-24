//! The workspace: loads every page and database from the folder, keeps them
//! in memory, writes changes back atomically and keeps the index current.

use crate::blocks::{self, Block, BlockKind};
use crate::conflict::Conflict;
use crate::database::{Database, PropKind, Property, Row, Schema};
use crate::fsutil::{atomic_write, rel_string, safe_file_name, slugify, unique_path};
use crate::index::{Index, IndexDoc};
use crate::inline;
use crate::page::Page;
use crate::search::fuzzy_score_folded;
use crate::sync::{self, FileStamp};
use crate::{Error, Result};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

mod assets;
mod editing;
mod loading;
mod vault;
use loading::*;
#[cfg(test)]
mod tests;

/// Which document something refers to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DocId {
    Page(String),
    Database(String),
    /// (database id, row id)
    Row(String, String),
}

impl DocId {
    /// Key used in the index: `p:<id>`, `d:<id>`, `r:<db>:<row>`.
    pub fn key(&self) -> String {
        match self {
            DocId::Page(id) => format!("p:{id}"),
            DocId::Database(id) => format!("d:{id}"),
            DocId::Row(db, r) => format!("r:{db}:{r}"),
        }
    }
    pub fn from_key(key: &str) -> Option<DocId> {
        let mut parts = key.splitn(3, ':');
        match (parts.next()?, parts.next(), parts.next()) {
            ("p", Some(id), None) => Some(DocId::Page(id.into())),
            ("d", Some(id), None) => Some(DocId::Database(id.into())),
            ("r", Some(db), Some(r)) => Some(DocId::Row(db.into(), r.into())),
            _ => None,
        }
    }
    /// Id of the page or database (the row id for rows).
    pub fn id(&self) -> &str {
        match self {
            DocId::Page(id) | DocId::Database(id) => id,
            DocId::Row(_, r) => r,
        }
    }
}

/// A sidebar entry.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeItem {
    pub doc: DocId,
    pub title: String,
    pub icon: String,
    pub has_children: bool,
}

/// A quick-finder / search result.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub doc: DocId,
    pub title: String,
    pub icon: String,
    /// Excerpt with `\u{1}`…`\u{2}` around matches (empty for title matches).
    pub snippet: String,
    /// Breadcrumb-style path of the parent, for display.
    pub path: String,
}

pub struct Workspace {
    pub root: PathBuf,
    pub pages: BTreeMap<String, Page>,
    pub dbs: BTreeMap<String, Database>,
    index: Index,
    /// Files that could not be read, shown to the user.
    pub warnings: Vec<String>,
    /// Size and time of every workspace file as this process last saw or wrote it.
    stamps: HashMap<PathBuf, FileStamp>,
    /// The SQLite index file, in the app's data folder (never in the notes folder).
    pub index_file: PathBuf,
    /// Two versions of one document waiting for the person to choose.
    pub conflicts: Vec<Conflict>,
    /// The vault passphrase while the session is unlocked (never written anywhere).
    vault_pass: Option<String>,
}

pub const PAGES_DIR: &str = "pages";
pub const DBS_DIR: &str = "databases";
pub const ASSETS_DIR: &str = "assets";

/// With `NOTECRAFT_PROFILE=1`, print how long the steps of opening a workspace take (stderr).
fn profile(step: &str, since: std::time::Instant) {
    if std::env::var_os("NOTECRAFT_PROFILE").is_some() {
        let rss = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok())
            .map_or(0.0, |kb| kb / 1024.0);
        eprintln!("profile: {step:<32} {:>8.1} ms  rss {rss:>6.0} MB", since.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Key under which the index remembers which files it was built from.
const INDEX_SIGNATURE: &str = "files";

impl Drop for Workspace {
    fn drop(&mut self) {
        self.index.set_meta(INDEX_SIGNATURE, Some(&self.files_signature()));
    }
}

fn now() -> String {
    crate::now_string()
}

impl Workspace {
    // ------------------------------------------------------------------ open

    /// Open a workspace folder (creating the layout if needed) and rebuild
    /// the index, which lives in the app's data folder (see [`sync::index_file_for`]).
    pub fn open(root: impl AsRef<Path>) -> Result<Workspace> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let index_file = sync::index_file_for(&root);
        Self::open_at(root, index_file)
    }

    /// Open with an explicit index file.
    pub fn open_at(root: impl AsRef<Path>, index_file: impl AsRef<Path>) -> Result<Workspace> {
        let root = root.as_ref().to_path_buf();
        let index_file = index_file.as_ref().to_path_buf();
        for d in [PAGES_DIR, DBS_DIR, ASSETS_DIR] {
            fs::create_dir_all(root.join(d))?;
        }
        let mut warnings = Vec::new();
        let mut conflicts = Vec::new();
        let t0 = std::time::Instant::now();
        let pages = load_pages(&root.join(PAGES_DIR), &mut warnings, &mut conflicts);
        profile("load pages", t0);
        let t0 = std::time::Instant::now();
        let mut dbs = load_databases(&root.join(DBS_DIR), &mut warnings, &mut conflicts);
        crate::formula::recompute(&mut dbs);
        profile("load databases", t0);
        let waiting = sync::pending_downloads(&root);
        if waiting > 0 {
            warnings.push(format!("{waiting} file(s) are not downloaded yet (iCloud Drive)"));
        }
        let index = match Index::open_or_rebuild(&index_file) {
            Ok(i) => {
                if let Some(dir) = index_file.parent() {
                    // a note for humans: which workspace this index belongs to
                    let _ = fs::write(dir.join("workspace.txt"), format!("{}\n", root.display()));
                }
                i
            }
            Err(e) => {
                warnings.push(format!("index: {e}; using a temporary index"));
                Index::in_memory()?
            }
        };
        for w in &warnings {
            crate::log::warn(&format!("open {}: {w}", root.display()));
        }
        let mut ws = Workspace {
            root,
            pages,
            dbs,
            index,
            warnings,
            stamps: HashMap::new(),
            index_file,
            conflicts,
            vault_pass: None,
        };
        let t0 = std::time::Instant::now();
        ws.stamps = sync::scan_stamps(&ws.root);
        profile("scan file stamps", t0);
        let t0 = std::time::Instant::now();
        // The index is rebuilt only when the files differ from what it was built from: it stores a
        // signature of the files when the workspace is closed cleanly, and forgets it while open
        // (so a crash always leads to a rebuild).
        let current = ws.index.meta(INDEX_SIGNATURE).is_some_and(|s| s == ws.files_signature())
            && ws.index.count().is_ok_and(|n| n == ws.all_docs().len());
        if current {
            profile("index is up to date", t0);
        } else {
            ws.reindex()?;
            profile("reindex", t0);
        }
        ws.index.set_meta(INDEX_SIGNATURE, None);
        Ok(ws)
    }

    /// Fingerprint of every file (name, size, modification time), independent of their order.
    fn files_signature(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut sum: u64 = 0;
        for (path, stamp) in &self.stamps {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            path.hash(&mut h);
            stamp.len.hash(&mut h);
            stamp.modified.and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).hash(&mut h);
            sum = sum.wrapping_add(h.finish());
        }
        format!("{}:{sum:016x}", self.stamps.len())
    }

    /// Rebuild the SQLite index from the loaded files.
    pub fn reindex(&mut self) -> Result<()> {
        let t0 = std::time::Instant::now();
        let all = self.all_docs();
        profile("  all_docs", t0);
        let t0 = std::time::Instant::now();
        let docs: Vec<IndexDoc> = all.iter().map(|d| self.index_doc(d)).collect();
        profile("  index_doc for every document", t0);
        let t0 = std::time::Instant::now();
        let r = self.index.rebuild(&docs);
        profile("  sqlite rebuild", t0);
        r
    }

    /// Reload everything from disk (after files changed outside the app).
    /// Blocks that did not change keep their in-memory ids, so the editor keeps
    /// its focus and caret in an untouched block.
    pub fn reload(&mut self) -> Result<()> {
        let mut warnings = Vec::new();
        let mut conflicts = Vec::new();
        let mut pages = load_pages(&self.root.join(PAGES_DIR), &mut warnings, &mut conflicts);
        let mut dbs = load_databases(&self.root.join(DBS_DIR), &mut warnings, &mut conflicts);
        for (id, p) in pages.iter_mut() {
            if let Some(old) = self.pages.get(id) {
                blocks::adopt_ids(&old.blocks, &mut p.blocks);
            }
            // an encrypted page stays open if the passphrase is still known
            if let (true, Some(cipher)) = (p.meta.encrypted, p.cipher.clone()) {
                let opened = match self.pages.get(id).and_then(|o| o.key.as_ref()) {
                    Some(k) => crate::vault::open_with(k, &cipher).map(|t| (t, k.clone())).ok(),
                    None => self.vault_pass.as_deref().and_then(|pw| crate::vault::open(pw, &cipher).ok()),
                };
                if let Some((text, key)) = opened {
                    p.blocks = blocks::from_markdown(&text);
                    p.key = Some(key);
                }
            }
        }
        for (id, d) in dbs.iter_mut() {
            if let Some(old) = self.dbs.get(id) {
                for (rid, r) in d.rows.iter_mut() {
                    if let Some(o) = old.rows.get(rid) {
                        blocks::adopt_ids(&o.blocks, &mut r.blocks);
                    }
                }
                for (tid, t) in d.templates.iter_mut() {
                    if let Some(o) = old.templates.get(tid) {
                        blocks::adopt_ids(&o.blocks, &mut t.blocks);
                    }
                }
            }
        }
        self.pages = pages;
        crate::formula::recompute(&mut dbs);
        self.dbs = dbs;
        self.warnings = warnings;
        self.conflicts = conflicts;
        self.stamps = sync::scan_stamps(&self.root);
        self.reindex()
    }

    // --------------------------------------------------------------- lookups

    pub fn exists(&self, doc: &DocId) -> bool {
        match doc {
            DocId::Page(id) => self.pages.contains_key(id),
            DocId::Database(id) => self.dbs.contains_key(id),
            DocId::Row(db, r) => self.dbs.get(db).is_some_and(|d| d.rows.contains_key(r)),
        }
    }

    pub fn title(&self, doc: &DocId) -> String {
        match doc {
            DocId::Page(id) => self.pages.get(id).map(|p| p.meta.title.clone()),
            DocId::Database(id) => self.dbs.get(id).map(|d| d.schema.title.clone()),
            DocId::Row(db, r) => self.dbs.get(db).and_then(|d| d.rows.get(r)).map(|r| r.meta.title.clone()),
        }
        .unwrap_or_default()
    }

    pub fn icon(&self, doc: &DocId) -> String {
        match doc {
            DocId::Page(id) => self.pages.get(id).map(|p| p.meta.icon.clone()),
            DocId::Database(id) => self.dbs.get(id).map(|d| d.schema.icon.clone()),
            DocId::Row(db, r) => self.dbs.get(db).and_then(|d| d.rows.get(r)).map(|r| r.meta.icon.clone()),
        }
        .unwrap_or_default()
    }

    /// Parent of a page or database (rows: their database).
    pub fn parent(&self, doc: &DocId) -> Option<DocId> {
        let pid = match doc {
            DocId::Page(id) => self.pages.get(id)?.meta.parent.clone()?,
            DocId::Database(id) => self.dbs.get(id)?.schema.parent.clone()?,
            DocId::Row(db, _) => return Some(DocId::Database(db.clone())),
        };
        if self.pages.contains_key(&pid) {
            Some(DocId::Page(pid))
        } else if self.dbs.contains_key(&pid) {
            Some(DocId::Database(pid))
        } else {
            None
        }
    }

    pub fn is_trashed(&self, doc: &DocId) -> bool {
        match doc {
            DocId::Page(id) => self.pages.get(id).is_some_and(|p| p.meta.trashed.is_some()),
            DocId::Database(id) => self.dbs.get(id).is_some_and(|d| d.schema.trashed.is_some()),
            DocId::Row(db, r) => self.dbs.get(db).and_then(|d| d.rows.get(r)).is_some_and(|r| r.meta.trashed.is_some()),
        }
    }

    /// Exists and neither it nor an ancestor is in the trash.
    pub fn is_live(&self, doc: &DocId) -> bool {
        let mut cur = Some(doc.clone());
        let mut guard = 0;
        while let Some(d) = cur {
            if !self.exists(&d) || self.is_trashed(&d) {
                return false;
            }
            cur = self.parent(&d);
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        true
    }

    /// Ancestors from the top down, ending with `doc` itself.
    pub fn breadcrumbs(&self, doc: &DocId) -> Vec<DocId> {
        let mut chain = vec![doc.clone()];
        let mut cur = self.parent(doc);
        while let Some(d) = cur {
            if chain.contains(&d) || chain.len() > 64 {
                break;
            }
            cur = self.parent(&d);
            chain.push(d);
        }
        chain.reverse();
        chain
    }

    fn order_of(&self, doc: &DocId) -> f64 {
        match doc {
            DocId::Page(id) => self.pages[id].meta.order,
            DocId::Database(id) => self.dbs[id].schema.order,
            DocId::Row(..) => 0.0,
        }
    }

    /// Live children of a page (or the top level when `parent` is `None`).
    pub fn children(&self, parent: Option<&str>) -> Vec<TreeItem> {
        let mut docs: Vec<DocId> = Vec::new();
        for (id, p) in &self.pages {
            if p.meta.parent.as_deref() == parent && p.meta.trashed.is_none() && !p.meta.archived {
                docs.push(DocId::Page(id.clone()));
            }
        }
        for (id, d) in &self.dbs {
            if d.schema.parent.as_deref() == parent && d.schema.trashed.is_none() {
                docs.push(DocId::Database(id.clone()));
            }
        }
        // top level: also orphans whose parent no longer exists
        if parent.is_none() {
            for (id, p) in &self.pages {
                if let Some(pp) = &p.meta.parent {
                    if !self.pages.contains_key(pp)
                        && !self.dbs.contains_key(pp)
                        && p.meta.trashed.is_none()
                        && !p.meta.archived
                    {
                        docs.push(DocId::Page(id.clone()));
                    }
                }
            }
        }
        // one pass to know which documents have live children, instead of a scan per item
        let mut parents: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for p in self.pages.values() {
            if let (Some(pp), None) = (p.meta.parent.as_deref(), &p.meta.trashed) {
                parents.insert(pp);
            }
        }
        for d in self.dbs.values() {
            if let (Some(pp), None) = (d.schema.parent.as_deref(), &d.schema.trashed) {
                parents.insert(pp);
            }
        }
        let mut keyed: Vec<(f64, String, DocId)> =
            docs.into_iter().map(|d| (self.order_of(&d), self.title(&d).to_lowercase(), d)).collect();
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.1.cmp(&b.1)));
        keyed
            .into_iter()
            .map(|(_, _, d)| TreeItem {
                title: self.title(&d),
                icon: self.icon(&d),
                has_children: parents.contains(d.id()),
                doc: d,
            })
            .collect()
    }

    fn children_ids(&self, parent: &str) -> Vec<DocId> {
        let mut v = Vec::new();
        for (id, p) in &self.pages {
            if p.meta.parent.as_deref() == Some(parent) && p.meta.trashed.is_none() {
                v.push(DocId::Page(id.clone()));
            }
        }
        for (id, d) in &self.dbs {
            if d.schema.parent.as_deref() == Some(parent) && d.schema.trashed.is_none() {
                v.push(DocId::Database(id.clone()));
            }
        }
        v
    }

    /// All descendants (any trash state) of a page or database.
    pub fn descendants(&self, doc: &DocId) -> Vec<DocId> {
        let mut out = Vec::new();
        let mut stack = vec![doc.id().to_string()];
        while let Some(pid) = stack.pop() {
            for (id, p) in &self.pages {
                if p.meta.parent.as_deref() == Some(pid.as_str()) && !out.contains(&DocId::Page(id.clone())) {
                    out.push(DocId::Page(id.clone()));
                    stack.push(id.clone());
                }
            }
            for (id, d) in &self.dbs {
                if d.schema.parent.as_deref() == Some(pid.as_str()) && !out.contains(&DocId::Database(id.clone())) {
                    out.push(DocId::Database(id.clone()));
                    stack.push(id.clone());
                }
            }
        }
        out
    }

    pub fn favorites(&self) -> Vec<TreeItem> {
        let mut v: Vec<DocId> = self
            .pages
            .iter()
            .filter(|(_, p)| p.meta.favorite)
            .map(|(id, _)| DocId::Page(id.clone()))
            .chain(self.dbs.iter().filter(|(_, d)| d.schema.favorite).map(|(id, _)| DocId::Database(id.clone())))
            .filter(|d| self.is_live(d))
            .collect();
        v.sort_by_key(|d| self.title(d).to_lowercase());
        v.into_iter()
            .map(|d| TreeItem {
                title: self.title(&d),
                icon: self.icon(&d),
                has_children: !self.children_ids(d.id()).is_empty(),
                doc: d,
            })
            .collect()
    }

    pub fn is_favorite(&self, doc: &DocId) -> bool {
        match doc {
            DocId::Page(id) => self.pages.get(id).is_some_and(|p| p.meta.favorite),
            DocId::Database(id) => self.dbs.get(id).is_some_and(|d| d.schema.favorite),
            DocId::Row(..) => false,
        }
    }

    /// Items placed directly in the trash, newest first.
    pub fn trash(&self) -> Vec<(DocId, String)> {
        let mut v: Vec<(DocId, String)> = Vec::new();
        for (id, p) in &self.pages {
            if let Some(t) = &p.meta.trashed {
                v.push((DocId::Page(id.clone()), t.clone()));
            }
        }
        for (id, d) in &self.dbs {
            if let Some(t) = &d.schema.trashed {
                v.push((DocId::Database(id.clone()), t.clone()));
            }
            for (rid, r) in &d.rows {
                if let Some(t) = &r.meta.trashed {
                    v.push((DocId::Row(id.clone(), rid.clone()), t.clone()));
                }
            }
        }
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }

    /// Find a live document by title (case-insensitive): pages first.
    pub fn resolve_title(&self, title: &str) -> Option<DocId> {
        let t = title.trim().to_lowercase();
        let pick = |d: DocId| if self.is_live(&d) { Some(d) } else { None };
        self.pages
            .iter()
            .filter(|(_, p)| p.meta.title.to_lowercase() == t)
            .find_map(|(id, _)| pick(DocId::Page(id.clone())))
            .or_else(|| {
                self.dbs
                    .iter()
                    .filter(|(_, d)| d.schema.title.to_lowercase() == t)
                    .find_map(|(id, _)| pick(DocId::Database(id.clone())))
            })
            .or_else(|| {
                self.dbs.iter().find_map(|(db, d)| {
                    d.rows
                        .iter()
                        .filter(|(_, r)| r.meta.title.to_lowercase() == t)
                        .find_map(|(rid, _)| pick(DocId::Row(db.clone(), rid.clone())))
                })
            })
    }

    /// Blocks of a page or row.
    pub fn blocks(&self, doc: &DocId) -> Option<&Vec<Block>> {
        match doc {
            DocId::Page(id) => self.pages.get(id).map(|p| &p.blocks),
            DocId::Row(db, r) => self.dbs.get(db)?.rows.get(r).map(|r| &r.blocks),
            DocId::Database(_) => None,
        }
    }

    pub fn blocks_mut(&mut self, doc: &DocId) -> Option<&mut Vec<Block>> {
        match doc {
            // a locked encrypted page has no readable blocks to edit
            DocId::Page(id) => self.pages.get_mut(id).filter(|p| !p.is_locked()).map(|p| &mut p.blocks),
            DocId::Row(db, r) => self.dbs.get_mut(db)?.rows.get_mut(r).map(|r| &mut r.blocks),
            DocId::Database(_) => None,
        }
    }

    pub fn cover(&self, doc: &DocId) -> String {
        match doc {
            DocId::Page(id) => self.pages.get(id).map(|p| p.meta.cover.clone()),
            DocId::Database(id) => self.dbs.get(id).map(|d| d.schema.cover.clone()),
            DocId::Row(db, r) => self.dbs.get(db).and_then(|d| d.rows.get(r)).map(|r| r.meta.cover.clone()),
        }
        .unwrap_or_default()
    }

    // ---------------------------------------------------------------- saving

    fn page_path(&self, p: &Page) -> PathBuf {
        self.root.join(PAGES_DIR).join(format!("{}-{}.md", slugify(&p.meta.title), p.meta.id))
    }

    fn db_dir(&self, s: &Schema) -> PathBuf {
        self.root.join(DBS_DIR).join(format!("{}-{}", slugify(&s.title), s.id))
    }

    /// Write a page file (renaming it when the title's slug changed). If the
    /// file changed on disk since we read it, our version goes to a conflicted
    /// copy instead (see [`crate::conflict`]).
    pub fn save_page(&mut self, id: &str) -> Result<()> {
        self.ensure_unlocked(&DocId::Page(id.to_string()))?;
        self.write_page(id, false)
    }

    pub(crate) fn write_page(&mut self, id: &str, force: bool) -> Result<()> {
        let (target, current, text) = {
            let p = self.pages.get(id).ok_or_else(|| Error::NotFound(id.into()))?;
            (self.page_path(p), p.path.clone(), p.render()?)
        };
        if !force {
            let clash = self.changed_underneath(current.as_deref()).or_else(|| {
                (current.as_ref() != Some(&target) && target.exists() && self.recorded_stamp(&target).is_none())
                    .then(|| target.clone())
            });
            if let Some(on_disk) = clash {
                return self.divert_to_conflict_copy(&DocId::Page(id.into()), &on_disk, &text);
            }
        }
        let p = self.pages.get_mut(id).unwrap();
        atomic_write(&target, text.as_bytes())?;
        let mut removed = None;
        if let Some(old) = &p.path {
            if old != &target && old.exists() {
                let _ = fs::remove_file(old);
                removed = Some(old.clone());
            }
        }
        p.path = Some(target.clone());
        if let Some(old) = removed {
            self.stamps.remove(&old);
        }
        self.note_written(&target);
        self.reindex_doc(&DocId::Page(id.into()));
        Ok(())
    }

    /// Write `schema.json` (moving the folder when the title's slug changed).
    pub fn save_schema(&mut self, id: &str) -> Result<()> {
        self.ensure_unlocked(&DocId::Database(id.to_string()))?;
        self.write_schema(id, false)
    }

    pub(crate) fn write_schema(&mut self, id: &str, force: bool) -> Result<()> {
        let (target, current) = {
            let d = self.dbs.get(id).ok_or_else(|| Error::NotFound(id.into()))?;
            (self.db_dir(&d.schema), d.dir.join("schema.json"))
        };
        if !force {
            if let Some(on_disk) = self.changed_underneath(Some(&current)) {
                let json = serde_json::to_string_pretty(&self.dbs[id].schema)?;
                return self.divert_to_conflict_copy(&DocId::Database(id.into()), &on_disk, &format!("{json}\n"));
            }
        }
        let d = self.dbs.get_mut(id).unwrap();
        let moved = d.dir != target && d.dir.exists() && !target.exists();
        if moved {
            fs::rename(&d.dir, &target)?;
            for r in d.rows.values_mut().chain(d.templates.values_mut()) {
                if let Some(p) = &r.path {
                    if let Ok(rel) = p.strip_prefix(&d.dir) {
                        r.path = Some(target.join(rel));
                    }
                }
            }
        }
        d.dir = target.clone();
        let json = serde_json::to_string_pretty(&d.schema)?;
        atomic_write(&target.join("schema.json"), format!("{json}\n").as_bytes())?;
        fs::create_dir_all(target.join("rows"))?;
        if moved {
            self.stamps = sync::scan_stamps(&self.root);
        } else {
            self.note_written(&target.join("schema.json"));
        }
        self.reindex_doc(&DocId::Database(id.into()));
        // row titles in the index include nothing of the schema, but
        // property names may have changed the indexed text
        let rows: Vec<String> = self.dbs[id].rows.keys().cloned().collect();
        for r in rows {
            self.reindex_doc(&DocId::Row(id.into(), r));
        }
        crate::formula::recompute(&mut self.dbs);
        Ok(())
    }

    fn row_path(dir: &Path, folder: &str, r: &Row) -> PathBuf {
        dir.join(folder).join(format!("{}-{}.md", slugify(&r.meta.title), r.meta.id))
    }

    pub fn save_row(&mut self, db: &str, row: &str) -> Result<()> {
        self.ensure_unlocked(&DocId::Row(db.to_string(), row.to_string()))?;
        self.write_row(db, row, false)
    }

    pub(crate) fn write_row(&mut self, db: &str, row: &str, force: bool) -> Result<()> {
        let (target, current, text) = {
            let d = self.dbs.get(db).ok_or_else(|| Error::NotFound(db.into()))?;
            let r = d.rows.get(row).ok_or_else(|| Error::NotFound(row.into()))?;
            (Self::row_path(&d.dir, "rows", r), r.path.clone(), r.render()?)
        };
        if !force {
            if let Some(on_disk) = self.changed_underneath(current.as_deref()) {
                return self.divert_to_conflict_copy(&DocId::Row(db.into(), row.into()), &on_disk, &text);
            }
        }
        let d = self.dbs.get_mut(db).unwrap();
        let r = d.rows.get_mut(row).unwrap();
        atomic_write(&target, text.as_bytes())?;
        let mut removed = None;
        if let Some(old) = &r.path {
            if old != &target && old.exists() {
                let _ = fs::remove_file(old);
                removed = Some(old.clone());
            }
        }
        r.path = Some(target.clone());
        if let Some(old) = removed {
            self.stamps.remove(&old);
        }
        self.note_written(&target);
        self.reindex_doc(&DocId::Row(db.into(), row.into()));
        crate::formula::recompute(&mut self.dbs);
        Ok(())
    }

    pub fn save_template(&mut self, db: &str, tpl: &str) -> Result<()> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        let dir = d.dir.clone();
        let r = d.templates.get_mut(tpl).ok_or_else(|| Error::NotFound(tpl.into()))?;
        let target = dir.join("templates").join(format!("{}.md", r.meta.id));
        atomic_write(&target, r.render()?.as_bytes())?;
        r.path = Some(target.clone());
        self.note_written(&target);
        Ok(())
    }

    /// Save whichever document this is.
    pub fn save(&mut self, doc: &DocId) -> Result<()> {
        self.ensure_unlocked(doc)?;
        match doc {
            DocId::Page(id) => self.save_page(id),
            DocId::Database(id) => self.save_schema(id),
            DocId::Row(db, r) => self.save_row(db, r),
        }
    }

    /// Mark a document as edited now (call before `save`).
    pub fn touch(&mut self, doc: &DocId) {
        match doc {
            DocId::Page(id) => {
                if let Some(p) = self.pages.get_mut(id) {
                    p.touch()
                }
            }
            DocId::Database(id) => {
                if let Some(d) = self.dbs.get_mut(id) {
                    d.schema.updated = now()
                }
            }
            DocId::Row(db, r) => {
                if let Some(r) = self.dbs.get_mut(db).and_then(|d| d.rows.get_mut(r)) {
                    r.touch()
                }
            }
        }
    }

    // ------------------------------------------------------------------ trash

    pub fn trash_doc(&mut self, doc: &DocId) -> Result<()> {
        self.ensure_unlocked(doc)?;
        let t = Some(now());
        match doc {
            DocId::Page(id) => self.pages.get_mut(id).map(|p| p.meta.trashed = t),
            DocId::Database(id) => self.dbs.get_mut(id).map(|d| d.schema.trashed = t),
            DocId::Row(db, r) => self.dbs.get_mut(db).and_then(|d| d.rows.get_mut(r)).map(|r| r.meta.trashed = t),
        }
        .ok_or_else(|| Error::NotFound(doc.key()))?;
        self.save(doc)?;
        self.reindex()
    }

    pub fn restore(&mut self, doc: &DocId) -> Result<()> {
        match doc {
            DocId::Page(id) => {
                let parent_gone = {
                    let p = &self.pages[id];
                    p.meta.parent.as_ref().is_some_and(|pp| !self.pages.contains_key(pp) && !self.dbs.contains_key(pp))
                };
                let p = self.pages.get_mut(id).ok_or_else(|| Error::NotFound(id.clone()))?;
                p.meta.trashed = None;
                if parent_gone {
                    p.meta.parent = None;
                }
            }
            DocId::Database(id) => {
                self.dbs.get_mut(id).ok_or_else(|| Error::NotFound(id.clone()))?.schema.trashed = None
            }
            DocId::Row(db, r) => {
                self.dbs
                    .get_mut(db)
                    .and_then(|d| d.rows.get_mut(r))
                    .ok_or_else(|| Error::NotFound(r.clone()))?
                    .meta
                    .trashed = None
            }
        }
        self.save(doc)?;
        self.reindex()
    }

    /// Delete a document and everything under it from disk.
    pub fn delete_forever(&mut self, doc: &DocId) -> Result<()> {
        self.ensure_unlocked(doc)?;
        // a copy outside the notes first: if it cannot be made, nothing is deleted
        self.snapshot_docs(std::slice::from_ref(doc), "delete forever")?;
        self.remove_from_disk(doc)
    }

    fn remove_from_disk(&mut self, doc: &DocId) -> Result<()> {
        let mut all = self.descendants(doc);
        all.push(doc.clone());
        for d in all {
            match &d {
                DocId::Page(id) => {
                    if let Some(p) = self.pages.remove(id) {
                        if let Some(path) = p.path {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
                DocId::Database(id) => {
                    if let Some(db) = self.dbs.remove(id) {
                        if db.dir.starts_with(self.root.join(DBS_DIR)) && db.dir.exists() {
                            fs::remove_dir_all(&db.dir)?;
                        }
                    }
                }
                DocId::Row(db, r) => {
                    if let Some(row) = self.dbs.get_mut(db).and_then(|d| d.rows.remove(r)) {
                        if let Some(path) = row.path {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
            }
        }
        self.stamps = sync::scan_stamps(&self.root);
        self.reindex()
    }

    pub fn empty_trash(&mut self) -> Result<()> {
        let docs: Vec<DocId> = self.trash().into_iter().map(|(d, _)| d).filter(|d| self.exists(d)).collect();
        for d in &docs {
            self.ensure_unlocked(d)?;
        }
        self.snapshot_docs(&docs, "empty trash")?;
        for d in docs {
            if self.exists(&d) {
                self.remove_from_disk(&d)?;
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------- databases

    pub fn create_database(&mut self, parent: Option<&str>, title: &str) -> Result<String> {
        self.ensure_parent_unlocked(parent)?;
        let id = crate::ids::new_id();
        let mut schema = Database::new_schema(id.clone(), title, parent.map(String::from));
        schema.order = self.next_order(parent);
        let dir = self.db_dir(&schema);
        self.dbs.insert(id.clone(), Database { schema, rows: BTreeMap::new(), templates: BTreeMap::new(), dir });
        self.save_schema(&id)?;
        Ok(id)
    }

    /// Add a row, optionally from a template. Returns the row id.
    pub fn add_row(&mut self, db: &str, template: Option<&str>, title: &str) -> Result<String> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        let mut row = Row::new(title);
        if let Some(t) = template.and_then(|t| d.templates.get(t)) {
            row.meta.properties = t.meta.properties.clone();
            row.meta.icon = t.meta.icon.clone();
            row.blocks = t.blocks.iter().map(|b| Block { id: blocks::next_block_id(), ..b.clone() }).collect();
            if title.is_empty() {
                row.meta.title = String::new();
            }
        }
        let id = row.meta.id.clone();
        d.rows.insert(id.clone(), row);
        let numbered = d.assign_unique_ids();
        self.save_row(db, &id)?;
        for other in numbered.into_iter().filter(|r| *r != id) {
            self.save_row(db, &other)?;
        }
        Ok(id)
    }

    pub fn duplicate_row(&mut self, db: &str, row: &str) -> Result<String> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        let src = d.rows.get(row).ok_or_else(|| Error::NotFound(row.into()))?.clone();
        let mut r = Row::new(&src.meta.title);
        r.meta.properties = src.meta.properties.clone();
        // a copy gets its own number, not the one of the original
        for p in d.schema.properties.iter().filter(|p| p.kind == PropKind::UniqueId) {
            r.meta.properties.remove(&p.name);
        }
        r.meta.icon = src.meta.icon.clone();
        r.blocks = src.blocks.clone();
        let id = r.meta.id.clone();
        d.rows.insert(id.clone(), r);
        let numbered = d.assign_unique_ids();
        self.save_row(db, &id)?;
        for other in numbered.into_iter().filter(|r| *r != id) {
            self.save_row(db, &other)?;
        }
        Ok(id)
    }

    pub fn add_template(&mut self, db: &str, name: &str) -> Result<String> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        let t = Row::new(name);
        let id = t.meta.id.clone();
        d.templates.insert(id.clone(), t);
        self.save_template(db, &id)?;
        Ok(id)
    }

    pub fn delete_template(&mut self, db: &str, tpl: &str) -> Result<()> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        if let Some(t) = self.dbs.get_mut(db).and_then(|d| d.templates.remove(tpl)) {
            if let Some(p) = t.path {
                let _ = fs::remove_file(&p);
                self.stamps.remove(&p);
            }
        }
        Ok(())
    }

    pub fn add_property(&mut self, db: &str, name: &str, kind: PropKind) -> Result<String> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        let mut name = name.to_string();
        let mut n = 2;
        while d.schema.prop_by_name(&name).is_some() {
            name = format!("{} {n}", name.trim_end_matches(char::is_numeric).trim_end());
            n += 1;
        }
        let p = Property::new(&name, kind);
        let id = p.id.clone();
        d.schema.properties.push(p);
        for v in d.schema.views.iter_mut() {
            if !v.visible.is_empty() {
                v.visible.push(id.clone());
            }
        }
        let numbered = d.assign_unique_ids();
        self.save_schema(db)?;
        for row in numbered {
            self.save_row(db, &row)?;
        }
        Ok(id)
    }

    /// Rename a property; row files are rewritten with the new key.
    pub fn rename_property(&mut self, db: &str, prop: &str, new_name: &str) -> Result<()> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        if new_name.trim().is_empty() || d.schema.prop_by_name(new_name).is_some() {
            return Err(Error::Invalid("property name is empty or already used".into()));
        }
        let p = d.schema.properties.iter_mut().find(|p| p.id == prop).ok_or_else(|| Error::NotFound(prop.into()))?;
        let old = std::mem::replace(&mut p.name, new_name.to_string());
        let mut changed = Vec::new();
        for (rid, r) in d.rows.iter_mut() {
            if let Some(v) = r.meta.properties.remove(&old) {
                r.meta.properties.insert(new_name.to_string(), v);
                changed.push(rid.clone());
            }
        }
        for t in d.templates.values_mut() {
            if let Some(v) = t.meta.properties.remove(&old) {
                t.meta.properties.insert(new_name.to_string(), v);
            }
        }
        let tpls: Vec<String> = d.templates.keys().cloned().collect();
        self.save_schema(db)?;
        for r in changed {
            self.save_row(db, &r)?;
        }
        for t in tpls {
            self.save_template(db, &t)?;
        }
        Ok(())
    }

    pub fn delete_property(&mut self, db: &str, prop: &str) -> Result<()> {
        self.ensure_unlocked(&DocId::Database(db.to_string()))?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        let Some(pos) = d.schema.properties.iter().position(|p| p.id == prop) else {
            return Ok(());
        };
        if d.schema.properties[pos].kind == PropKind::Title {
            return Err(Error::Invalid("the title property cannot be deleted".into()));
        }
        self.snapshot_docs(&[DocId::Database(db.to_string())], "delete property")?;
        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.into()))?;
        let p = d.schema.properties.remove(pos);
        for v in d.schema.views.iter_mut() {
            v.visible.retain(|x| x != prop);
            v.sorts.retain(|s| s.prop != prop);
            v.filter.remove_prop(prop);
            if v.group_by.as_deref() == Some(prop) {
                v.group_by = None;
            }
        }
        let mut changed = Vec::new();
        for (rid, r) in d.rows.iter_mut() {
            if r.meta.properties.remove(&p.name).is_some() {
                changed.push(rid.clone());
            }
        }
        self.save_schema(db)?;
        for r in changed {
            self.save_row(db, &r)?;
        }
        Ok(())
    }

    /// Display text of a row's property; relations show related titles.
    pub fn display_value(&self, db: &str, row: &Row, prop: &Property) -> String {
        let v = row.value(prop);
        match (prop.kind, &v) {
            (PropKind::Relation, crate::database::PropValue::Multi(ids)) => {
                let target = prop.relation_db.as_deref().unwrap_or(db);
                ids.iter()
                    .map(|id| {
                        self.dbs
                            .get(target)
                            .and_then(|d| d.rows.get(id))
                            .map(|r| r.meta.title.clone())
                            .unwrap_or_else(|| id.clone())
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            _ => v.display(),
        }
    }

    // ----------------------------------------------------------------- search

    /// Quick finder: title matches (fuzzy) first, then full-text hits.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let q = query.trim();
        let mut out: Vec<SearchResult> = Vec::new();
        if q.is_empty() {
            if let Ok(recent) = self.index.recent(limit) {
                for (key, title, icon) in recent {
                    if let Some(doc) = DocId::from_key(&key) {
                        out.push(SearchResult {
                            path: self.path_label(&doc),
                            doc,
                            title,
                            icon,
                            snippet: String::new(),
                        });
                    }
                }
            }
            return out;
        }
        // score every title without copying it; the (slower) liveness check only runs on matches
        let folded = crate::search::fold(q);
        let mut scored: Vec<(i32, DocId)> = Vec::new();
        let mut consider = |title: &str, doc: &dyn Fn() -> DocId| {
            if let Some(s) = fuzzy_score_folded(&folded, title).filter(|s| *s >= 150) {
                scored.push((s, doc()));
            }
        };
        for (id, p) in &self.pages {
            consider(&p.meta.title, &|| DocId::Page(id.clone()));
        }
        for (id, db) in &self.dbs {
            consider(&db.schema.title, &|| DocId::Database(id.clone()));
            for (rid, r) in &db.rows {
                consider(&r.meta.title, &|| DocId::Row(id.clone(), rid.clone()));
            }
        }
        scored.retain(|(_, d)| self.is_live(d));
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| self.title(&a.1).cmp(&self.title(&b.1))));
        for (_, d) in scored.into_iter().take(limit) {
            out.push(SearchResult {
                title: self.title(&d),
                icon: self.icon(&d),
                snippet: String::new(),
                path: self.path_label(&d),
                doc: d,
            });
        }
        if let Ok(hits) = self.index.search(q, limit) {
            for h in hits {
                if let Some(doc) = DocId::from_key(&h.key) {
                    if let Some(existing) = out.iter_mut().find(|r| r.doc == doc) {
                        if existing.snippet.is_empty() && h.snippet.contains('\u{1}') {
                            existing.snippet = h.snippet;
                        }
                        continue;
                    }
                    out.push(SearchResult {
                        path: self.path_label(&doc),
                        doc,
                        title: h.title,
                        icon: h.icon,
                        snippet: h.snippet,
                    });
                }
            }
        }
        out.truncate(limit);
        out
    }

    /// `Parent / Child` label of where a document lives.
    pub fn path_label(&self, doc: &DocId) -> String {
        let mut crumbs = self.breadcrumbs(doc);
        crumbs.pop();
        crumbs.iter().map(|d| self.title(d)).collect::<Vec<_>>().join(" / ")
    }

    /// Documents that mention this one with `[[title]]`.
    pub fn backlinks(&self, doc: &DocId) -> Vec<DocId> {
        let title = self.title(doc);
        if title.trim().is_empty() {
            return vec![];
        }
        self.index
            .backlinks(&title)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|k| DocId::from_key(&k))
            .filter(|d| d != doc)
            .collect()
    }
}

/// Default first block for a new page (an empty paragraph to type in).
pub fn starter_blocks() -> Vec<Block> {
    vec![Block::new(BlockKind::Paragraph, "")]
}

/// Their front matter, with my title (a title typed here wins).
struct PageMetaMerge;
impl PageMetaMerge {
    fn take(theirs: crate::page::PageMeta, my_title: &str) -> crate::page::PageMeta {
        crate::page::PageMeta { title: my_title.to_string(), ..theirs }
    }
}
