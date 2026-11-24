//! The "second brain" starter: a small tree of pages (how it works, inbox, projects,
//! areas, references, context for an assistant, weekly review) created on demand.

use crate::import_export::replace_page_body;
use crate::workspace::Workspace;
use crate::Result;
use std::path::Path;

const PAGES: [&str; 7] = [
    include_str!("../templates/second-brain/Mode d'emploi.md"),
    include_str!("../templates/second-brain/Inbox.md"),
    include_str!("../templates/second-brain/Projets.md"),
    include_str!("../templates/second-brain/Domaines.md"),
    include_str!("../templates/second-brain/Références.md"),
    include_str!("../templates/second-brain/Contexte pour Claude.md"),
    include_str!("../templates/second-brain/Revue hebdomadaire (modèle).md"),
];

fn title_of(md: &str) -> String {
    md.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or("Page").trim().to_string()
}

/// Create the starter tree at the top of the sidebar and return the id of its root page.
pub fn create(ws: &mut Workspace) -> Result<String> {
    let root = ws.create_page(None, "Second cerveau")?;
    ws.set_icon(&crate::workspace::DocId::Page(root.clone()), "🧠")?;
    let intro = format!("# Second cerveau\n\nCommence par [[{}]].\n", title_of(PAGES[0]));
    replace_page_body(ws, &root, &intro, Path::new("."))?;
    for md in PAGES {
        let id = ws.create_page(Some(&root), &title_of(md))?;
        replace_page_body(ws, &id, md, Path::new("."))?;
    }
    ws.move_to_top(&root)?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starter_lands_first_with_seven_children() {
        let dir = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(dir.path()).unwrap();
        ws.create_page(None, "Existing").unwrap();
        let root = create(&mut ws).unwrap();
        assert_eq!(ws.children(None)[0].doc.id(), root);
        assert_eq!(ws.children(Some(&root)).len(), 7);
    }
}
