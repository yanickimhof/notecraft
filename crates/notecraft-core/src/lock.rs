//! Read-only protection: a locked page, database or row cannot be edited, moved,
//! renamed or deleted, and neither can anything inside it.
//!
//! The lock is a plain `locked: true` line in the file's front matter (or in a
//! database's `schema.json`), so it travels with the files and is respected by
//! the app, the CLI and the MCP server alike.

use crate::workspace::{DocId, Workspace};
use crate::{Error, Result};

impl Workspace {
    /// The lock flag of this document itself, ignoring its parents.
    pub fn has_own_lock(&self, doc: &DocId) -> bool {
        match doc {
            DocId::Page(id) => self.pages.get(id).is_some_and(|p| p.meta.locked),
            DocId::Database(id) => self.dbs.get(id).is_some_and(|d| d.schema.locked),
            DocId::Row(db, r) => self.dbs.get(db).and_then(|d| d.rows.get(r)).is_some_and(|r| r.meta.locked),
        }
    }

    /// True when the document or any of its parents is locked.
    pub fn is_locked(&self, doc: &DocId) -> bool {
        self.locking_doc(doc).is_some()
    }

    /// The document that holds the lock protecting `doc` (itself or the nearest locked parent).
    pub fn locking_doc(&self, doc: &DocId) -> Option<DocId> {
        let mut current = Some(doc.clone());
        // The depth guard protects against a corrupted parent loop.
        for _ in 0..64 {
            let d = current?;
            if self.has_own_lock(&d) {
                return Some(d);
            }
            current = self.parent(&d);
        }
        None
    }

    /// Files written by a newer version of the app (they are read, never rewritten).
    pub fn newer_format_files(&self) -> Vec<&str> {
        self.warnings.iter().filter(|w| w.starts_with(crate::NEWER_FORMAT)).map(String::as_str).collect()
    }

    /// Refuse every change while the workspace holds files from a newer version: an older app
    /// could rewrite them and lose what it does not understand.
    pub fn ensure_format_writable(&self) -> Result<()> {
        let n = self.newer_format_files().len();
        if n == 0 {
            return Ok(());
        }
        Err(Error::Newer(format!(
            "{n} file(s) were written by a newer NøteCraft; update the app. The workspace stays read-only until then."
        )))
    }

    /// Fail with `Error::Locked` when `doc` is protected.
    pub fn ensure_unlocked(&self, doc: &DocId) -> Result<()> {
        self.ensure_format_writable()?;
        match self.locking_doc(doc) {
            None => Ok(()),
            Some(holder) if holder == *doc => Err(Error::Locked(self.title(doc))),
            Some(holder) => {
                Err(Error::Locked(format!("{} (inside the locked {})", self.title(doc), self.title(&holder))))
            }
        }
    }

    /// Same check for a parent id given as a string (page or database); `None` means top level.
    pub fn ensure_parent_unlocked(&self, parent: Option<&str>) -> Result<()> {
        self.ensure_format_writable()?;
        let Some(id) = parent else { return Ok(()) };
        if self.pages.contains_key(id) {
            self.ensure_unlocked(&DocId::Page(id.to_string()))
        } else if self.dbs.contains_key(id) {
            self.ensure_unlocked(&DocId::Database(id.to_string()))
        } else {
            Ok(())
        }
    }

    /// Lock or unlock a document. This is the one change a lock itself allows.
    pub fn set_locked(&mut self, doc: &DocId, locked: bool) -> Result<()> {
        match doc {
            DocId::Page(id) => self.pages.get_mut(id).map(|p| p.meta.locked = locked),
            DocId::Database(id) => self.dbs.get_mut(id).map(|d| d.schema.locked = locked),
            DocId::Row(db, r) => self.dbs.get_mut(db).and_then(|d| d.rows.get_mut(r)).map(|r| r.meta.locked = locked),
        }
        .ok_or_else(|| Error::NotFound(doc.key()))?;
        self.touch(doc);
        // Written without the lock check: this is the one change a lock allows.
        match doc {
            DocId::Page(id) => self.write_page(id, false),
            DocId::Database(id) => self.write_schema(id, false),
            DocId::Row(db, r) => self.write_row(db, r, false),
        }
    }

    /// Every document that carries its own lock, for display and audits.
    pub fn locked_docs(&self) -> Vec<DocId> {
        let mut out: Vec<DocId> = Vec::new();
        for (id, p) in &self.pages {
            if p.meta.locked {
                out.push(DocId::Page(id.clone()));
            }
        }
        for (id, d) in &self.dbs {
            if d.schema.locked {
                out.push(DocId::Database(id.clone()));
            }
            for (rid, r) in &d.rows {
                if r.meta.locked {
                    out.push(DocId::Row(id.clone(), rid.clone()));
                }
            }
        }
        out.sort();
        out
    }

    /// Mutable blocks of a document, or `Error::Locked` when it is protected.
    pub fn blocks_mut_checked(&mut self, doc: &DocId) -> Result<&mut Vec<crate::blocks::Block>> {
        self.ensure_unlocked(doc)?;
        self.blocks_mut(doc).ok_or_else(|| Error::NotFound(doc.key()))
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

    fn is_locked_err<T: std::fmt::Debug>(r: Result<T>) -> bool {
        matches!(r, Err(Error::Locked(_)))
    }

    #[test]
    fn a_locked_page_refuses_edits_moves_and_deletion() {
        let (_d, mut w) = ws();
        let id = w.create_page(None, "Contract").unwrap();
        let doc = DocId::Page(id.clone());
        w.set_locked(&doc, true).unwrap();
        assert!(w.is_locked(&doc));
        w.blocks_mut(&doc).unwrap().push(Block::new(BlockKind::Paragraph, "x"));
        assert!(is_locked_err(w.save(&doc)));
        assert!(is_locked_err(w.rename(&doc, "Other")));
        assert!(is_locked_err(w.set_icon(&doc, "🔒")));
        assert!(is_locked_err(w.move_to(&doc, None, 0)));
        assert!(is_locked_err(w.trash_doc(&doc)));
        assert!(is_locked_err(w.delete_forever(&doc)));
        assert!(is_locked_err(w.blocks_mut_checked(&doc)));
        assert_eq!(w.locked_docs(), vec![doc]);
    }

    #[test]
    fn the_lock_covers_everything_inside_and_can_be_lifted() {
        let (_d, mut w) = ws();
        let parent = w.create_page(None, "Archive").unwrap();
        let child = w.create_page(Some(&parent), "Old note").unwrap();
        let (p, c) = (DocId::Page(parent.clone()), DocId::Page(child.clone()));
        w.set_locked(&p, true).unwrap();
        assert!(w.is_locked(&c));
        assert!(!w.has_own_lock(&c));
        assert_eq!(w.locking_doc(&c), Some(p.clone()));
        assert!(is_locked_err(w.rename(&c, "New")));
        assert!(is_locked_err(w.create_page(Some(&parent), "Another")));
        assert!(is_locked_err(w.duplicate_page(&child)));
        w.set_locked(&p, false).unwrap();
        assert!(!w.is_locked(&c));
        w.rename(&c, "New").unwrap();
        w.create_page(Some(&parent), "Another").unwrap();
    }

    #[test]
    fn a_locked_database_protects_its_rows_and_schema() {
        let (_d, mut w) = ws();
        let db = w.create_database(None, "Contracts").unwrap();
        let row = w.add_row(&db, None, "Lease").unwrap();
        w.set_locked(&DocId::Database(db.clone()), true).unwrap();
        let r = DocId::Row(db.clone(), row);
        assert!(w.is_locked(&r));
        assert!(is_locked_err(w.add_row(&db, None, "New")));
        assert!(is_locked_err(w.save(&r)));
        assert!(is_locked_err(w.trash_doc(&r)));
        assert!(is_locked_err(w.add_property(&db, "Amount", crate::database::PropKind::Number)));
    }

    #[test]
    fn renaming_a_page_leaves_mentions_inside_locked_pages_untouched() {
        let (_d, mut w) = ws();
        let target = w.create_page(None, "Budget").unwrap();
        let note = w.create_page(None, "Note").unwrap();
        let frozen = w.create_page(None, "Frozen").unwrap();
        for id in [&note, &frozen] {
            let doc = DocId::Page(id.clone());
            w.blocks_mut(&doc).unwrap().push(Block::new(BlockKind::Paragraph, "see [[Budget]]"));
            w.save(&doc).unwrap();
        }
        w.set_locked(&DocId::Page(frozen.clone()), true).unwrap();
        w.rename(&DocId::Page(target), "Budget 2027").unwrap();
        let text = |id: &String| crate::blocks::to_markdown(w.blocks(&DocId::Page(id.clone())).unwrap());
        assert!(text(&note).contains("[[Budget 2027]]"));
        assert!(text(&frozen).contains("[[Budget]]"));
    }

    #[test]
    fn the_lock_survives_a_reload() {
        let (d, mut w) = ws();
        let id = w.create_page(None, "Vault note").unwrap();
        w.set_locked(&DocId::Page(id.clone()), true).unwrap();
        drop(w);
        let index = d.path().join("idx2").join("index.db");
        let w2 = Workspace::open_at(d.path().join("notes"), index).unwrap();
        assert!(w2.is_locked(&DocId::Page(id)));
    }
}
