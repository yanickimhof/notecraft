//! Conflicts: two versions of one document. They come from sync tools
//! ("Page 2.md" with the same id) or from a save that found the file changed
//! underneath it (our version is then written to a "conflicted copy" file).
//! Nothing is ever overwritten silently: the original stays loaded, the other
//! version waits in its own file (never indexed) until the person chooses.

use crate::blocks;
use crate::database::Row;
use crate::fsutil::atomic_write;
use crate::page::Page;
use crate::sync::{self, FileStamp};
use crate::workspace::{DocId, Workspace};
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub doc: DocId,
    /// The file that is loaded and shown.
    pub original: PathBuf,
    /// The other version, kept in its own file until resolved.
    pub copy: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Keep the version shown; delete the other file.
    KeepThis,
    /// Replace the version shown with the other one.
    KeepOther,
    /// Keep both: the other version becomes its own page or row.
    KeepBoth,
}

impl Workspace {
    /// The first unresolved conflict of a document.
    pub fn conflict_for(&self, doc: &DocId) -> Option<&Conflict> {
        self.conflicts.iter().find(|c| &c.doc == doc)
    }

    /// Text of the other version (its Markdown body, or JSON for a schema).
    pub fn other_version_text(&self, doc: &DocId) -> Option<String> {
        let c = self.conflict_for(doc)?;
        let text = fs::read_to_string(&c.copy).ok()?;
        Some(match doc {
            DocId::Database(_) => text,
            _ => {
                let (_, body) = crate::frontmatter::split(&text);
                body.trim().to_string()
            }
        })
    }

    /// Settle the first conflict of `doc`. With `KeepBoth`, returns the new document.
    pub fn resolve_conflict(&mut self, doc: &DocId, how: Resolution) -> Result<Option<DocId>> {
        let pos = self
            .conflicts
            .iter()
            .position(|c| &c.doc == doc)
            .ok_or_else(|| Error::NotFound(format!("no conflict for {}", doc.key())))?;
        let c = self.conflicts[pos].clone();
        let mut created = None;
        match how {
            Resolution::KeepThis => {}
            Resolution::KeepOther => {
                let text = fs::read_to_string(&c.copy)?;
                match doc {
                    DocId::Page(id) => {
                        let mut other = Page::parse(&text, "")?;
                        other.meta.id = id.clone();
                        let old = self.pages.get(id).ok_or_else(|| Error::NotFound(id.clone()))?;
                        blocks::adopt_ids(&old.blocks, &mut other.blocks);
                        other.path = old.path.clone();
                        self.pages.insert(id.clone(), other);
                        self.force_save(doc)?;
                    }
                    DocId::Row(db, rid) => {
                        let mut other = Row::parse(&text)?;
                        other.meta.id = rid.clone();
                        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.clone()))?;
                        let old = d.rows.get(rid).ok_or_else(|| Error::NotFound(rid.clone()))?;
                        blocks::adopt_ids(&old.blocks, &mut other.blocks);
                        other.path = old.path.clone();
                        d.rows.insert(rid.clone(), other);
                        self.force_save(doc)?;
                    }
                    DocId::Database(db) => {
                        let schema: crate::database::Schema = serde_json::from_str(&text)?;
                        let d = self.dbs.get_mut(db).ok_or_else(|| Error::NotFound(db.clone()))?;
                        d.schema = crate::database::Schema { id: db.clone(), ..schema };
                        self.force_save(doc)?;
                    }
                }
            }
            Resolution::KeepBoth => {
                let text = fs::read_to_string(&c.copy)?;
                match doc {
                    DocId::Page(id) => {
                        let mut other = Page::parse(&text, "")?;
                        other.meta.id = crate::ids::new_id();
                        other.meta.title = format!("{} (conflict copy)", other.meta.title);
                        other.meta.favorite = false;
                        if let Some(o) = self.pages.get(id) {
                            other.meta.parent = o.meta.parent.clone();
                            other.meta.order = o.meta.order + 0.5;
                        }
                        other.path = None;
                        let nid = other.meta.id.clone();
                        self.pages.insert(nid.clone(), other);
                        self.save_page(&nid)?;
                        created = Some(DocId::Page(nid));
                    }
                    DocId::Row(db, _) => {
                        let mut other = Row::parse(&text)?;
                        other.meta.id = crate::ids::new_id();
                        other.meta.title = format!("{} (conflict copy)", other.meta.title);
                        other.path = None;
                        let nid = other.meta.id.clone();
                        self.dbs
                            .get_mut(db)
                            .ok_or_else(|| Error::NotFound(db.clone()))?
                            .rows
                            .insert(nid.clone(), other);
                        self.save_row(db, &nid)?;
                        created = Some(DocId::Row(db.clone(), nid));
                    }
                    DocId::Database(_) => {
                        return Err(Error::Invalid(
                            "two versions of a database schema cannot both be kept; choose one".into(),
                        ))
                    }
                }
            }
        }
        let _ = fs::remove_file(&c.copy);
        self.forget_stamp(&c.copy);
        self.conflicts.remove(pos);
        self.reindex()?;
        Ok(created)
    }

    /// Our version could not be written because `on_disk` changed underneath:
    /// write it next to it as a conflicted copy, load the version on disk,
    /// and record the conflict.
    pub(crate) fn divert_to_conflict_copy(&mut self, doc: &DocId, on_disk: &Path, mine: &str) -> Result<()> {
        let copy = sync::conflict_copy_path(on_disk);
        atomic_write(&copy, mine.as_bytes())?;
        self.remember_stamp(&copy);
        let text = fs::read_to_string(on_disk)?;
        match doc {
            DocId::Page(id) => {
                let mut theirs = Page::parse(&text, "")?;
                theirs.meta.id = id.clone();
                if let Some(old) = self.pages.get(id) {
                    blocks::adopt_ids(&old.blocks, &mut theirs.blocks);
                }
                theirs.path = Some(on_disk.to_path_buf());
                self.pages.insert(id.clone(), theirs);
            }
            DocId::Row(db, rid) => {
                let mut theirs = Row::parse(&text)?;
                theirs.meta.id = rid.clone();
                if let Some(d) = self.dbs.get_mut(db) {
                    if let Some(old) = d.rows.get(rid) {
                        blocks::adopt_ids(&old.blocks, &mut theirs.blocks);
                    }
                    theirs.path = Some(on_disk.to_path_buf());
                    d.rows.insert(rid.clone(), theirs);
                }
            }
            DocId::Database(db) => {
                let schema: crate::database::Schema = serde_json::from_str(&text)?;
                if let Some(d) = self.dbs.get_mut(db) {
                    d.schema = schema;
                }
            }
        }
        self.remember_stamp(on_disk);
        self.conflicts.push(Conflict { doc: doc.clone(), original: on_disk.to_path_buf(), copy });
        self.reindex()
    }

    /// The file we are about to replace changed since we last read or wrote it?
    /// Returns that file. A file deleted elsewhere is simply written again.
    pub(crate) fn changed_underneath(&self, file: Option<&Path>) -> Option<PathBuf> {
        let f = file?;
        let now = FileStamp::of(f)?;
        match self.recorded_stamp(f) {
            Some(known) if known == now => None,
            _ => Some(f.to_path_buf()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::Block;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let w = Workspace::open(dir.path()).unwrap();
        (dir, w)
    }

    fn sleep() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    #[test]
    fn sync_copies_with_the_same_id_are_conflicts_not_duplicates() {
        let (dir, mut w) = ws();
        let a = w.create_page(None, "Notes").unwrap();
        w.pages.get_mut(&a).unwrap().blocks = vec![Block::paragraph("mine")];
        w.save_page(&a).unwrap();
        let path = w.pages[&a].path.clone().unwrap();
        let other = fs::read_to_string(&path).unwrap().replace("mine", "theirs");
        fs::write(dir.path().join("pages/Notes (conflicted copy).md"), other).unwrap();
        let w2 = Workspace::open(dir.path()).unwrap();
        assert_eq!(w2.pages.len(), 1, "never two pages under one id");
        assert_eq!(w2.pages[&a].blocks[0].text, "mine");
        let c = w2.conflict_for(&DocId::Page(a.clone())).unwrap();
        assert_eq!(c.original, path);
        assert_eq!(w2.other_version_text(&DocId::Page(a.clone())).unwrap(), "theirs");
        assert_eq!(w2.search("theirs", 5).len(), 0, "the copy is not indexed");
    }

    #[test]
    fn a_save_never_overwrites_an_outside_change() {
        let (_dir, mut w) = ws();
        let a = w.create_page(None, "Plan").unwrap();
        let doc = DocId::Page(a.clone());
        w.pages.get_mut(&a).unwrap().blocks = vec![Block::paragraph("v1")];
        w.save_page(&a).unwrap();
        let path = w.pages[&a].path.clone().unwrap();
        sleep();
        // another Mac changes the file…
        let theirs = fs::read_to_string(&path).unwrap().replace("v1", "from the other Mac");
        fs::write(&path, &theirs).unwrap();
        // …while we were typing
        w.pages.get_mut(&a).unwrap().blocks = vec![Block::paragraph("my typing")];
        w.save_page(&a).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), theirs, "their file is untouched");
        assert_eq!(w.pages[&a].blocks[0].text, "from the other Mac");
        assert_eq!(w.other_version_text(&doc).unwrap(), "my typing");
        // keep mine
        w.resolve_conflict(&doc, Resolution::KeepOther).unwrap();
        assert_eq!(w.pages[&a].blocks[0].text, "my typing");
        assert!(fs::read_to_string(&path).unwrap().contains("my typing"));
        assert!(w.conflicts.is_empty());
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1, "copy removed");
    }

    #[test]
    fn keep_both_and_keep_this() {
        let (dir, mut w) = ws();
        let a = w.create_page(None, "Ideas").unwrap();
        let doc = DocId::Page(a.clone());
        let path = w.pages[&a].path.clone().unwrap();
        fs::copy(&path, dir.path().join("pages/Ideas 2.md")).unwrap();
        w.reload().unwrap();
        let new = w.resolve_conflict(&doc, Resolution::KeepBoth).unwrap().unwrap();
        assert_eq!(w.title(&new), "Ideas (conflict copy)");
        assert_eq!(w.pages.len(), 2);
        fs::copy(&path, dir.path().join("pages/Ideas (Kopie).md")).unwrap();
        w.reload().unwrap();
        assert!(w.conflict_for(&doc).is_some());
        w.resolve_conflict(&doc, Resolution::KeepThis).unwrap();
        assert!(!dir.path().join("pages/Ideas (Kopie).md").exists());
        assert!(w.conflicts.is_empty());
    }

    #[test]
    fn row_conflicts_on_save() {
        let (_dir, mut w) = ws();
        let db = w.create_database(None, "Tasks").unwrap();
        let r = w.add_row(&db, None, "Task").unwrap();
        let path = w.dbs[&db].rows[&r].path.clone().unwrap();
        sleep();
        fs::write(&path, fs::read_to_string(&path).unwrap().replace("title: Task", "title: Task edited elsewhere"))
            .unwrap();
        w.dbs.get_mut(&db).unwrap().rows.get_mut(&r).unwrap().blocks.push(Block::paragraph("local"));
        w.save_row(&db, &r).unwrap();
        assert_eq!(w.dbs[&db].rows[&r].meta.title, "Task edited elsewhere");
        assert!(w.conflict_for(&DocId::Row(db.clone(), r.clone())).is_some());
        assert_eq!(w.dbs[&db].rows.len(), 1);
    }
}
