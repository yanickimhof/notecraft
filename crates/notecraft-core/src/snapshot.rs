//! Snapshots: copies of files taken just before something destructive, kept outside the notes.
//!
//! A snapshot lives in `<index folder>/snapshots/<id>/` (next to the search index, never in the
//! notes folder, so a sync tool or an assistant that only reaches the notes cannot touch it):
//! a `manifest.json` and a `files/` tree that mirrors the workspace layout.
//!
//! They are taken automatically before deleting for good, emptying the trash, deleting a database
//! property and by the MCP server before it replaces a page body. Restoring writes the files back
//! (after taking a snapshot of what is there now, so a restore can itself be undone).

use crate::workspace::{DocId, Workspace};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Snapshots kept per workspace; the oldest are removed first.
pub const KEEP: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub id: String,
    pub time: String,
    pub reason: String,
    /// Titles of the documents covered, for display.
    pub items: Vec<String>,
    /// Workspace-relative paths of the files copied.
    pub files: Vec<String>,
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_files(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

/// Copy `files` (under `root`) into a new snapshot below `base`. `None` when none of them exists.
pub fn write_snapshot(
    root: &Path,
    base: &Path,
    files: &[PathBuf],
    items: Vec<String>,
    reason: &str,
) -> Result<Option<SnapshotInfo>> {
    let files: Vec<&PathBuf> = files.iter().filter(|f| f.is_file()).collect();
    if files.is_empty() {
        return Ok(None);
    }
    let stamp: String = crate::now_string().chars().filter(|c| c.is_ascii_digit() || *c == 'T').collect();
    let mut id = stamp.clone();
    let mut n = 1;
    while base.join(&id).exists() {
        id = format!("{stamp}-{n}");
        n += 1;
    }
    let dir = base.join(&id);
    let mut rels = Vec::new();
    for f in files {
        let Ok(rel) = f.strip_prefix(root) else { continue };
        let target = dir.join("files").join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(f, &target)?;
        rels.push(rel.to_string_lossy().to_string());
    }
    let info = SnapshotInfo { id, time: crate::now_string(), reason: reason.to_string(), items, files: rels };
    fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&info)?)?;
    Ok(Some(info))
}

/// All snapshots below `base`, newest first.
pub fn list_snapshots_in(base: &Path) -> Vec<SnapshotInfo> {
    let mut out: Vec<SnapshotInfo> = fs::read_dir(base)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| fs::read(e.path().join("manifest.json")).ok())
                .filter_map(|b| serde_json::from_slice(&b).ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

impl Workspace {
    /// Where this workspace's snapshots are kept.
    pub fn snapshot_dir(&self) -> PathBuf {
        self.index_file.parent().map(|p| p.join("snapshots")).unwrap_or_else(|| PathBuf::from("snapshots"))
    }

    fn files_of(&self, doc: &DocId) -> Vec<PathBuf> {
        let mut out = Vec::new();
        match doc {
            DocId::Page(id) => out.extend(self.pages.get(id).and_then(|p| p.path.clone())),
            DocId::Row(db, r) => out.extend(self.dbs.get(db).and_then(|d| d.rows.get(r)).and_then(|r| r.path.clone())),
            DocId::Database(id) => {
                if let Some(d) = self.dbs.get(id) {
                    collect_files(&d.dir, &mut out);
                }
            }
        }
        out
    }

    /// Copy the files of these documents (and what is inside them) before a destructive change.
    /// Returns the snapshot id, or `None` when there was nothing on disk to copy.
    pub fn snapshot_docs(&self, docs: &[DocId], reason: &str) -> Result<Option<String>> {
        let mut all: Vec<DocId> = Vec::new();
        for d in docs {
            if !all.contains(d) {
                all.push(d.clone());
            }
            for x in self.descendants(d) {
                if !all.contains(&x) {
                    all.push(x);
                }
            }
        }
        let mut files: Vec<PathBuf> = all.iter().flat_map(|d| self.files_of(d)).collect();
        files.sort();
        files.dedup();
        let titles: Vec<String> = docs.iter().map(|d| self.title(d)).collect();
        self.snapshot_files(&files, titles, reason)
    }

    fn snapshot_files(&self, files: &[PathBuf], items: Vec<String>, reason: &str) -> Result<Option<String>> {
        let made = write_snapshot(&self.root, &self.snapshot_dir(), files, items, reason)?;
        if made.is_some() {
            self.prune_snapshots(KEEP);
        }
        Ok(made.map(|i| i.id))
    }

    /// Path of the file behind a document, relative to the workspace root.
    pub fn doc_relative_path(&self, doc: &DocId) -> Option<String> {
        let p = self.files_of(doc).into_iter().next()?;
        Some(p.strip_prefix(&self.root).ok()?.to_string_lossy().to_string())
    }

    /// Put back one file of a snapshot (a single page version). What is there now is snapshotted first.
    pub fn restore_snapshot_file(&mut self, id: &str, rel: &str) -> Result<()> {
        if id.contains('/') || id.contains("..") || rel.contains("..") || Path::new(rel).is_absolute() {
            return Err(Error::Invalid("bad snapshot path".into()));
        }
        let source = self.snapshot_dir().join(id).join("files").join(rel);
        if !source.is_file() {
            return Err(Error::NotFound(format!("{id}/{rel}")));
        }
        let target = self.root.join(rel);
        let name = target.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        self.snapshot_files(std::slice::from_ref(&target), vec![name], &format!("before restoring {id}"))?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &target)?;
        self.reload()
    }

    /// All snapshots, newest first.
    pub fn list_snapshots(&self) -> Vec<SnapshotInfo> {
        list_snapshots_in(&self.snapshot_dir())
    }

    /// Remove the oldest snapshots beyond `keep`.
    pub fn prune_snapshots(&self, keep: usize) {
        let manual = self.list_snapshots().into_iter().filter(|i| i.reason != crate::backup::AUTO_REASON);
        for old in manual.skip(keep) {
            let _ = fs::remove_dir_all(self.snapshot_dir().join(old.id));
        }
    }

    /// Put the files of a snapshot back. What is overwritten is snapshotted first.
    /// Returns the number of files restored.
    pub fn restore_snapshot(&mut self, id: &str) -> Result<usize> {
        if id.contains('/') || id.contains("..") {
            return Err(Error::Invalid("bad snapshot id".into()));
        }
        let dir = self.snapshot_dir().join(id);
        let bytes = fs::read(dir.join("manifest.json")).map_err(|_| Error::NotFound(format!("snapshot {id}")))?;
        let info: SnapshotInfo = serde_json::from_slice(&bytes)?;
        let targets: Vec<PathBuf> = info.files.iter().map(|r| self.root.join(r)).collect();
        self.snapshot_files(&targets, info.items.clone(), &format!("before restoring {id}"))?;
        let mut n = 0;
        for rel in &info.files {
            if rel.contains("..") {
                continue;
            }
            let target = self.root.join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(dir.join("files").join(rel), &target)?;
            n += 1;
        }
        self.reload()?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{Block, BlockKind};

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("idx").join("index.db");
        let w = Workspace::open_at(dir.path().join("notes"), index).unwrap();
        (dir, w)
    }

    #[test]
    fn one_page_version_can_be_restored() {
        let (_d, mut w) = ws();
        let doc = page(&mut w, "Doc", "first");
        let rel = w.doc_relative_path(&doc).expect("path");
        let id = w.snapshot_docs(std::slice::from_ref(&doc), "test").unwrap().expect("snapshot");
        w.blocks_mut(&doc).unwrap()[0].text = "second".into();
        w.save(&doc).unwrap();
        w.restore_snapshot_file(&id, &rel).unwrap();
        assert_eq!(w.blocks(&doc).unwrap()[0].text, "first");
        assert!(w.restore_snapshot_file(&id, "../x.md").is_err());
    }

    fn page(w: &mut Workspace, title: &str, text: &str) -> DocId {
        let id = w.create_page(None, title).unwrap();
        let doc = DocId::Page(id);
        w.blocks_mut(&doc).unwrap().push(Block::new(BlockKind::Paragraph, text));
        w.save(&doc).unwrap();
        doc
    }

    #[test]
    fn deleting_for_good_keeps_a_copy_that_can_be_restored() {
        let (d, mut w) = ws();
        let doc = page(&mut w, "Precious", "do not lose me");
        w.trash_doc(&doc).unwrap();
        w.delete_forever(&doc).unwrap();
        assert!(w.pages.is_empty());

        let snaps = w.list_snapshots();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].reason, "delete forever");
        assert_eq!(snaps[0].items, vec!["Precious".to_string()]);
        // the copy lives outside the notes folder
        assert!(w.snapshot_dir().starts_with(d.path().join("idx")));
        assert!(!w.snapshot_dir().starts_with(&w.root));

        assert_eq!(w.restore_snapshot(&snaps[0].id).unwrap(), 1);
        let back = w.pages.values().next().unwrap();
        assert_eq!(back.meta.title, "Precious");
        assert!(back.blocks.iter().any(|b| b.text == "do not lose me"));
    }

    #[test]
    fn emptying_the_trash_takes_one_snapshot_of_everything_in_it() {
        let (_d, mut w) = ws();
        let a = page(&mut w, "A", "one");
        let b = page(&mut w, "B", "two");
        let keep = page(&mut w, "Keep", "three");
        w.trash_doc(&a).unwrap();
        w.trash_doc(&b).unwrap();
        w.empty_trash().unwrap();
        assert_eq!(w.pages.len(), 1);
        assert!(w.pages.contains_key(keep.id()));
        let snaps = w.list_snapshots();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].reason, "empty trash");
        assert_eq!(snaps[0].files.len(), 2);
        w.restore_snapshot(&snaps[0].id).unwrap();
        assert_eq!(w.pages.len(), 3);
    }

    #[test]
    fn restoring_is_itself_undoable_and_pruning_keeps_the_newest() {
        let (_d, mut w) = ws();
        let doc = page(&mut w, "Draft", "v1");
        let first = w.snapshot_docs(std::slice::from_ref(&doc), "manual").unwrap().unwrap();
        w.blocks_mut(&doc).unwrap()[0].text = "v2".into();
        w.save(&doc).unwrap();
        w.restore_snapshot(&first).unwrap();
        assert_eq!(w.blocks(&doc).unwrap()[0].text, "v1");
        let snaps = w.list_snapshots();
        assert_eq!(snaps.len(), 2, "the restore took a snapshot of v2 first");
        assert!(snaps[0].reason.starts_with("before restoring"));
        w.prune_snapshots(1);
        assert_eq!(w.list_snapshots().len(), 1);
        assert!(w.restore_snapshot("../etc").is_err());
    }

    #[test]
    fn deleting_a_property_snapshots_the_database() {
        let (_d, mut w) = ws();
        let db = w.create_database(None, "Tasks").unwrap();
        let row = w.add_row(&db, None, "Write").unwrap();
        let prop = w.dbs[&db].schema.properties.iter().find(|p| p.name == "Status").unwrap().id.clone();
        w.dbs.get_mut(&db).unwrap().rows.get_mut(&row).unwrap().meta.properties.insert("Status".into(), "Done".into());
        w.save_row(&db, &row).unwrap();
        w.delete_property(&db, &prop).unwrap();
        let snaps = w.list_snapshots();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].reason, "delete property");
        w.restore_snapshot(&snaps[0].id).unwrap();
        assert!(w.dbs[&db].schema.properties.iter().any(|p| p.name == "Status"));
    }
}
