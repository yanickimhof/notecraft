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
pub fn templates_root(ws: &Workspace) -> Option<String> {
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
        .filter(|c| matches!(c.doc, DocId::Page(_)) && !is_notes_title(&c.title) && (q.is_empty() || c.title.to_lowercase().contains(&q)))
        .map(|c| SearchResult { doc: c.doc, title: c.title, icon: c.icon, snippet: String::new(), path: String::new() })
        .collect()
}

/// Title of the group of note templates (the instructions the AI follows to write meeting notes).
fn is_notes_title(t: &str) -> bool {
    let t = t.trim().to_lowercase();
    t == "meeting notes" || t == "notes de réunion" || t == "notes de reunion"
}

/// The id of the group page that holds the note templates, inside the templates page.
pub fn notes_group(ws: &Workspace) -> Option<String> {
    let root = templates_root(ws)?;
    ws.children(Some(&root)).into_iter().find_map(|c| match c.doc {
        DocId::Page(id) if is_notes_title(&c.title) => Some(id),
        _ => None,
    })
}

/// The note templates written by the user: (key, title). The key is `page:<id>`.
pub fn note_templates(ws: &Workspace) -> Vec<(String, String)> {
    let Some(group) = notes_group(ws) else { return vec![] };
    ws.children(Some(&group))
        .into_iter()
        .filter_map(|c| match c.doc {
            DocId::Page(id) => Some((format!("page:{id}"), c.title)),
            _ => None,
        })
        .collect()
}

/// The text of the note template `key` (`page:<id>`), as Markdown.
pub fn note_template_text(ws: &Workspace, key: &str) -> Option<String> {
    let id = key.strip_prefix("page:")?;
    let blocks = ws.blocks(&DocId::Page(id.to_string()))?;
    let md = blocks::to_markdown(blocks);
    (!md.trim().is_empty()).then_some(md)
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
    /// Makes sure the templates page exists, with the standard meeting minutes in it and the group of note
    /// templates (what the AI follows to write the notes of a recorded meeting).
    pub fn ensure_templates(&mut self) {
        let fr = self.prefs.lang == Lang::Fr;
        let Some(ws) = self.ws.as_mut() else { return };
        if templates_root(ws).is_some() && notes_group(ws).is_some() && self.prefs.meeting.notes_seeded
            || self.prefs.meeting.seed_failed
        {
            return;
        }
        let result = (|| -> notecraft_core::Result<()> {
            let root = match templates_root(ws) {
                Some(r) => r,
                None => {
                    let (title, intro) = if fr {
                        (
                            "Modèles",
                            "Chaque sous-page de cette page est un modèle de page. Écrivez ou modifiez-la comme une page ordinaire, puis insérez-la où vous voulez avec / puis Modèle. {{date}} devient la date du jour. Les modèles de notes de réunion sont dans la page « Notes de réunion ».",
                        )
                    } else {
                        (
                            "Templates",
                            "Each sub-page of this page is a page template. Write or edit it like any page, then insert it anywhere with / then Template. {{date}} becomes today's date. Templates for meeting notes are in the \"Meeting notes\" page.",
                        )
                    };
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
                    root
                }
            };
            let group = match notes_group(ws) {
                Some(g) => g,
                None => {
                    let (gtitle, gintro) = if fr {
                        (
                            "Notes de réunion",
                            "Chaque sous-page est un modèle de notes : écrivez-y la structure et les consignes que l'IA suit pour rédiger les notes d'une réunion enregistrée. Modifiez-les librement. Le modèle se choisit dans le bloc de réunion.",
                        )
                    } else {
                        (
                            "Meeting notes",
                            "Each sub-page is a notes template: write the structure and the instructions the AI follows to write the notes of a recorded meeting. Edit them freely. Pick the template in the meeting block.",
                        )
                    };
                    let g = ws.create_page(Some(&root), gtitle)?;
                    {
                        let p = ws.pages.get_mut(&g).unwrap();
                        p.meta.icon = "🎙".into();
                        p.blocks = vec![Block::paragraph(gintro)];
                    }
                    ws.save_page(&g)?;
                    g
                }
            };
            // the built-in note templates become pages, so that they can be edited like the others
            let lang = if fr { Lang::Fr } else { Lang::En };
            let existing: Vec<String> = ws.children(Some(&group)).into_iter().map(|c| c.title).collect();
            for key in notecraft_ai::NOTE_TEMPLATES.iter().filter(|k| **k != "custom") {
                let name = crate::meeting::template_label(lang, key);
                if existing.iter().any(|t| t == name) {
                    continue;
                }
                let sub = ws.create_page(Some(&group), name)?;
                {
                    let p = ws.pages.get_mut(&sub).unwrap();
                    p.blocks = vec![Block::paragraph(notecraft_ai::note_structure_text(key))];
                }
                ws.save_page(&sub)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.prefs.meeting.notes_seeded = true;
                self.prefs.meeting.template = self.effective_template();
            }
            Err(e) => {
                self.prefs.meeting.seed_failed = true;
                self.notify(e.to_string());
            }
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
