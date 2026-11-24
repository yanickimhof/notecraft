//! Creating, changing, moving and duplicating documents (split out of mod.rs, unchanged).

use super::*;

impl Workspace {
    // --------------------------------------------------------------- editing

    pub(super) fn next_order(&self, parent: Option<&str>) -> f64 {
        self.children(parent).iter().map(|c| self.order_of(&c.doc)).fold(0.0, f64::max) + 1.0
    }

    /// Create a page under `parent` (a page id) or at the top level.
    pub fn create_page(&mut self, parent: Option<&str>, title: &str) -> Result<String> {
        self.ensure_parent_unlocked(parent)?;
        let id = crate::ids::new_id();
        let mut p = Page::new(id.clone(), title, parent.map(String::from));
        p.meta.order = self.next_order(parent);
        self.pages.insert(id.clone(), p);
        self.save_page(&id)?;
        Ok(id)
    }

    /// Rename any document; `[[Old title]]` mentions elsewhere follow.
    pub fn rename(&mut self, doc: &DocId, new_title: &str) -> Result<()> {
        self.ensure_unlocked(doc)?;
        let old = self.title(doc);
        if old == new_title {
            return Ok(());
        }
        match doc {
            DocId::Page(id) => {
                let p = self.pages.get_mut(id).ok_or_else(|| Error::NotFound(id.clone()))?;
                p.meta.title = new_title.to_string();
                p.touch();
            }
            DocId::Database(id) => {
                let d = self.dbs.get_mut(id).ok_or_else(|| Error::NotFound(id.clone()))?;
                d.schema.title = new_title.to_string();
                d.schema.updated = now();
            }
            DocId::Row(db, r) => {
                let row =
                    self.dbs.get_mut(db).and_then(|d| d.rows.get_mut(r)).ok_or_else(|| Error::NotFound(r.clone()))?;
                row.meta.title = new_title.to_string();
                row.touch();
            }
        }
        self.save(doc)?;
        if !old.trim().is_empty() && self.resolve_title(&old).is_none() {
            self.rewrite_mentions(&old, new_title)?;
        }
        Ok(())
    }

    /// Call after a title was edited in place: `[[old]]` mentions follow the
    /// new title unless another document still has the old one.
    pub fn title_changed(&mut self, old: &str, new: &str) -> Result<()> {
        if old == new || old.trim().is_empty() || self.resolve_title(old).is_some() {
            return Ok(());
        }
        self.rewrite_mentions(old, new)
    }

    pub(super) fn rewrite_mentions(&mut self, old: &str, new: &str) -> Result<()> {
        // Locked documents keep their text: a stale mention is better than a rewritten contract.
        let locked_pages: Vec<String> =
            self.pages.keys().filter(|id| self.is_locked(&DocId::Page((*id).clone()))).cloned().collect();
        let locked_dbs: Vec<String> =
            self.dbs.keys().filter(|id| self.is_locked(&DocId::Database((*id).clone()))).cloned().collect();
        let locked_rows: Vec<(String, String)> = self
            .dbs
            .iter()
            .flat_map(|(db, d)| d.rows.keys().map(move |r| (db.clone(), r.clone())))
            .filter(|(db, r)| self.is_locked(&DocId::Row(db.clone(), r.clone())))
            .collect();
        let mut changed: Vec<DocId> = Vec::new();
        for (id, p) in self.pages.iter_mut().filter(|(id, _)| !locked_pages.contains(*id)) {
            if rename_in_blocks(&mut p.blocks, old, new) {
                changed.push(DocId::Page(id.clone()));
            }
        }
        for (db, d) in self.dbs.iter_mut().filter(|(db, _)| !locked_dbs.contains(*db)) {
            for (rid, r) in d.rows.iter_mut() {
                if locked_rows.contains(&(db.clone(), rid.clone())) {
                    continue;
                }
                if rename_in_blocks(&mut r.blocks, old, new) {
                    changed.push(DocId::Row(db.clone(), rid.clone()));
                }
            }
        }
        for d in changed {
            self.save(&d)?;
        }
        Ok(())
    }

    pub fn set_icon(&mut self, doc: &DocId, icon: &str) -> Result<()> {
        self.ensure_unlocked(doc)?;
        match doc {
            DocId::Page(id) => self.pages.get_mut(id).map(|p| p.meta.icon = icon.into()),
            DocId::Database(id) => self.dbs.get_mut(id).map(|d| d.schema.icon = icon.into()),
            DocId::Row(db, r) => {
                self.dbs.get_mut(db).and_then(|d| d.rows.get_mut(r)).map(|r| r.meta.icon = icon.into())
            }
        };
        self.touch(doc);
        self.save(doc)
    }

    pub fn set_cover(&mut self, doc: &DocId, cover: &str) -> Result<()> {
        self.ensure_unlocked(doc)?;
        match doc {
            DocId::Page(id) => self.pages.get_mut(id).map(|p| p.meta.cover = cover.into()),
            DocId::Database(id) => self.dbs.get_mut(id).map(|d| d.schema.cover = cover.into()),
            DocId::Row(db, r) => {
                self.dbs.get_mut(db).and_then(|d| d.rows.get_mut(r)).map(|r| r.meta.cover = cover.into())
            }
        };
        self.touch(doc);
        self.save(doc)
    }

    /// Set (or clear, with an empty string) the reminder of a page. The time must be `YYYY-MM-DD HH:MM`.
    pub fn set_reminder(&mut self, page: &str, when: &str) -> Result<()> {
        let when = if when.is_empty() {
            String::new()
        } else {
            crate::reminder::normalize(when).ok_or_else(|| Error::Invalid(format!("not a date and time: {when}")))?
        };
        let p = self.pages.get_mut(page).ok_or_else(|| Error::Invalid("no such page".into()))?;
        if p.meta.reminder == when {
            return Ok(());
        }
        p.meta.reminder = when;
        self.save_page(page)
    }

    /// Pages with a reminder, soonest first: (page id, title, when). Trashed pages are left out.
    pub fn reminders(&self) -> Vec<(String, String, String)> {
        let mut v: Vec<(String, String, String)> = self
            .pages
            .values()
            .filter(|p| !p.meta.reminder.is_empty() && p.meta.trashed.is_none())
            .map(|p| (p.meta.id.clone(), p.meta.title.clone(), p.meta.reminder.clone()))
            .collect();
        v.sort_by(|a, b| a.2.cmp(&b.2));
        v
    }

    pub fn set_favorite(&mut self, doc: &DocId, fav: bool) -> Result<()> {
        match doc {
            DocId::Page(id) => self.pages.get_mut(id).map(|p| p.meta.favorite = fav),
            DocId::Database(id) => self.dbs.get_mut(id).map(|d| d.schema.favorite = fav),
            DocId::Row(..) => None,
        };
        self.save(doc)
    }

    /// Move a page or database under `new_parent` at position `index` among
    /// its new siblings. Refuses to move something inside itself.
    pub fn move_to(&mut self, doc: &DocId, new_parent: Option<&str>, index: usize) -> Result<()> {
        self.ensure_unlocked(doc)?;
        self.ensure_parent_unlocked(new_parent)?;
        if matches!(doc, DocId::Row(..)) {
            return Err(Error::Invalid("rows stay in their database".into()));
        }
        if let Some(np) = new_parent {
            if np == doc.id() || self.descendants(doc).iter().any(|d| d.id() == np) {
                return Err(Error::Invalid("cannot move a page inside itself".into()));
            }
            if self.dbs.contains_key(np) {
                return Err(Error::Invalid("pages cannot be placed inside a database".into()));
            }
        }
        let mut siblings: Vec<DocId> =
            self.children(new_parent).into_iter().map(|t| t.doc).filter(|d| d != doc).collect();
        let index = index.min(siblings.len());
        siblings.insert(index, doc.clone());
        for (i, d) in siblings.iter().enumerate() {
            let order = i as f64 + 1.0;
            let parent = new_parent.map(String::from);
            let changed = match d {
                DocId::Page(id) => {
                    let p = self.pages.get_mut(id).unwrap();
                    let c = p.meta.order != order || p.meta.parent != parent;
                    p.meta.order = order;
                    p.meta.parent = parent;
                    c
                }
                DocId::Database(id) => {
                    let s = &mut self.dbs.get_mut(id).unwrap().schema;
                    let c = s.order != order || s.parent != parent;
                    s.order = order;
                    s.parent = parent;
                    c
                }
                DocId::Row(..) => false,
            };
            if changed {
                self.save(d)?;
            }
        }
        Ok(())
    }

    /// Duplicate a page (and its blocks) right after the original.
    pub fn duplicate_page(&mut self, id: &str) -> Result<String> {
        let src = self.pages.get(id).ok_or_else(|| Error::NotFound(id.into()))?.clone();
        self.ensure_parent_unlocked(src.meta.parent.as_deref())?;
        let new_id = crate::ids::new_id();
        let mut p = Page::new(new_id.clone(), &src.meta.title, src.meta.parent.clone());
        p.meta.icon = src.meta.icon.clone();
        p.meta.cover = src.meta.cover.clone();
        p.meta.order = src.meta.order + 0.5;
        p.blocks = src.blocks.iter().map(|b| Block { id: blocks::next_block_id(), ..b.clone() }).collect();
        self.pages.insert(new_id.clone(), p);
        self.save_page(&new_id)?;
        // renumber siblings so orders stay whole numbers
        let parent = src.meta.parent.clone();
        let pos =
            self.children(parent.as_deref()).iter().position(|t| t.doc == DocId::Page(new_id.clone())).unwrap_or(0);
        self.move_to(&DocId::Page(new_id.clone()), parent.as_deref(), pos)?;
        Ok(new_id)
    }
}
