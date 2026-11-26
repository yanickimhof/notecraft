//! Page templates: the sub-pages of the "Templates" page. Each one is an ordinary page that can be written and
//! edited freely; `/template` inserts its blocks in another page. `{{date}}` becomes today's date.

use super::NcApp;
use crate::editor::date_text;
use crate::i18n::Lang;
use notecraft_core::blocks::{self, Block};
use notecraft_core::{workspace::SearchResult, DocId, Workspace};

/// Titles under which the templates page is recognised, in either language.
fn is_templates_title(t: &str) -> bool {
    let t = t.trim().to_lowercase();
    t == "templates" || t == "modèles" || t == "modeles"
}

/// The id of the top level templates page.
fn templates_root(ws: &Workspace) -> Option<String> {
    ws.children(None).into_iter().find_map(|c| match c.doc {
        DocId::Page(id) if is_templates_title(&c.title) => Some(id),
        _ => None,
    })
}

/// The templates that match `query`, as finder results.
pub fn template_results(ws: &Workspace, query: &str) -> Vec<SearchResult> {
    let Some(root) = templates_root(ws) else { return vec![] };
    let q = query.trim().to_lowercase();
    ws.children(Some(&root))
        .into_iter()
        .filter(|c| matches!(c.doc, DocId::Page(_)) && (q.is_empty() || c.title.to_lowercase().contains(&q)))
        .map(|c| SearchResult { doc: c.doc, title: c.title, icon: c.icon, snippet: String::new(), path: String::new() })
        .collect()
}

fn minutes(fr: bool) -> (&'static str, &'static str) {
    if fr {
        (
            "Compte rendu de réunion",
            "## Compte rendu de réunion\n\n**Date :** {{date}}\n\n**Heure :** \n\n**Lieu ou lien :** \n\n**Animateur :** \n\n**Rédacteur :** \n\n### Participants\n\n- \n\n### Excusés\n\n- \n\n### Ordre du jour\n\n1. \n\n### Discussion\n\n- \n\n### Décisions\n\n- \n\n### Actions\n\n- [ ] Action (responsable, échéance)\n\n### Prochaine réunion\n\n- \n",
        )
    } else {
        (
            "Meeting minutes",
            "## Meeting minutes\n\n**Date:** {{date}}\n\n**Time:** \n\n**Location or link:** \n\n**Facilitator:** \n\n**Note taker:** \n\n### Attendees\n\n- \n\n### Apologies\n\n- \n\n### Agenda\n\n1. \n\n### Discussion\n\n- \n\n### Decisions\n\n- \n\n### Actions\n\n- [ ] Action (owner, due date)\n\n### Next meeting\n\n- \n",
        )
    }
}

impl NcApp {
    /// Makes sure the templates page exists, with the standard meeting minutes in it.
    pub fn ensure_templates(&mut self) {
        let fr = self.prefs.lang == Lang::Fr;
        let Some(ws) = self.ws.as_mut() else { return };
        if templates_root(ws).is_some() {
            return;
        }
        let (title, intro) = if fr {
            (
                "Modèles",
                "Chaque sous-page de cette page est un modèle. Écrivez ou modifiez-la comme une page ordinaire, puis insérez-la où vous voulez avec / puis Modèle. {{date}} devient la date du jour.",
            )
        } else {
            (
                "Templates",
                "Each sub-page of this page is a template. Write or edit it like any page, then insert it anywhere with / then Template. {{date}} becomes today's date.",
            )
        };
        let result = (|| -> notecraft_core::Result<()> {
            let root = ws.create_page(None, title)?;
            {
                let p = ws.pages.get_mut(&root).unwrap();
                p.meta.icon = "📋".into();
                p.blocks = vec![Block::paragraph(intro)];
            }
            ws.save_page(&root)?;
            let (name, md) = minutes(fr);
            let sub = ws.create_page(Some(&root), name)?;
            {
                let p = ws.pages.get_mut(&sub).unwrap();
                p.meta.icon = "📝".into();
                p.blocks = blocks::from_markdown(md);
            }
            ws.save_page(&sub)?;
            Ok(())
        })();
        if let Err(e) = result {
            self.notify(e.to_string());
        }
    }

    /// Inserts the blocks of the template page `doc` at `index` of the current page.
    pub fn insert_template(&mut self, doc: DocId, index: usize, replace: bool) {
        let fr = self.prefs.lang == Lang::Fr;
        let Some(ws) = self.ws.as_ref() else { return };
        let Some(src) = ws.blocks(&doc) else { return };
        let md = blocks::to_markdown(src).replace("{{date}}", &date_text("date_short", fr));
        let new = blocks::from_markdown(&md);
        if new.is_empty() {
            return;
        }
        self.insert_blocks(index, replace, new);
        self.flush_save();
    }
}
