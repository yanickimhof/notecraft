//! Per-page options: typeface, small text, full width, archive and word count.
//!
//! They are plain lines in the page's front matter, so they travel with the file.

use crate::workspace::{DocId, Workspace};
use crate::{blocks, Error, Result};

/// Typefaces a page can use; the empty string is the default.
pub const FONTS: [&str; 3] = ["", "serif", "mono"];

impl Workspace {
    /// Set the look of a page. Unknown typefaces fall back to the default.
    pub fn set_page_look(&mut self, id: &str, font: &str, small_text: bool, full_width: bool) -> Result<()> {
        let doc = DocId::Page(id.to_string());
        self.ensure_unlocked(&doc)?;
        let p = self.pages.get_mut(id).ok_or_else(|| Error::NotFound(id.to_string()))?;
        p.meta.font = if FONTS.contains(&font) { font.to_string() } else { String::new() };
        p.meta.small_text = small_text;
        p.meta.full_width = full_width;
        self.touch(&doc);
        self.save(&doc)
    }

    /// Take a page out of the sidebar, or put it back. A locked page can still be archived.
    pub fn set_archived(&mut self, id: &str, archived: bool) -> Result<()> {
        let doc = DocId::Page(id.to_string());
        self.pages.get_mut(id).map(|p| p.meta.archived = archived).ok_or_else(|| Error::NotFound(id.to_string()))?;
        self.touch(&doc);
        self.write_page(id, false)
    }

    /// Archived pages that are not in the trash, newest first: (id, title, icon, updated).
    pub fn archived(&self) -> Vec<(String, String, String, String)> {
        let mut v: Vec<_> = self
            .pages
            .values()
            .filter(|p| p.meta.archived && p.meta.trashed.is_none())
            .map(|p| (p.meta.id.clone(), p.meta.title.clone(), p.meta.icon.clone(), p.meta.updated.clone()))
            .collect();
        v.sort_by(|a, b| b.3.cmp(&a.3));
        v
    }

    /// Words in the body of a page (an encrypted page that is not open counts as zero).
    pub fn word_count(&self, doc: &DocId) -> usize {
        let blocks = match doc {
            DocId::Page(id) => self.pages.get(id).map(|p| &p.blocks),
            DocId::Row(db, r) => self.dbs.get(db).and_then(|d| d.rows.get(r)).map(|r| &r.blocks),
            DocId::Database(_) => None,
        };
        blocks.map_or(0, |b| blocks::plain_text(b).split_whitespace().count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ws(name: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (Workspace::open(&dir).unwrap(), dir)
    }

    #[test]
    fn the_look_of_a_page_is_kept_on_disk_and_bad_fonts_are_ignored() {
        let (mut ws, dir) = temp_ws("look");
        let id = ws.create_page(None, "Look").unwrap();
        ws.set_page_look(&id, "serif", true, true).unwrap();
        let again = Workspace::open(&dir).unwrap();
        let m = &again.pages[&id].meta;
        assert_eq!((m.font.as_str(), m.small_text, m.full_width), ("serif", true, true));
        ws.set_page_look(&id, "comic-sans", false, false).unwrap();
        assert_eq!(ws.pages[&id].meta.font, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archived_page_leaves_the_sidebar_keeps_its_children_and_comes_back() {
        let (mut ws, dir) = temp_ws("archive");
        let a = ws.create_page(None, "Old").unwrap();
        let child = ws.create_page(Some(&a), "Child").unwrap();
        ws.set_archived(&a, true).unwrap();
        assert!(ws.children(None).iter().all(|t| t.doc != DocId::Page(a.clone())));
        assert_eq!(ws.archived().len(), 1);
        assert!(ws.is_live(&DocId::Page(child.clone())), "nothing is lost");
        assert!(Workspace::open(&dir).unwrap().pages[&a].meta.archived);
        ws.set_archived(&a, false).unwrap();
        assert!(ws.children(None).iter().any(|t| t.doc == DocId::Page(a.clone())));
        assert!(ws.archived().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn words_are_counted_without_markup() {
        let (mut ws, dir) = temp_ws("words");
        let id = ws.create_page(None, "Count").unwrap();
        ws.pages.get_mut(&id).unwrap().blocks =
            vec![blocks::Block::paragraph("one **two** three"), blocks::Block::paragraph("four")];
        assert_eq!(ws.word_count(&DocId::Page(id)), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
