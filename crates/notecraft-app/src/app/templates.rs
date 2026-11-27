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

/// The templates that match `query`, as finder results. The transcription templates are not page templates:
/// they live in their own group and are left out.
pub fn template_results(ws: &Workspace, query: &str) -> Vec<SearchResult> {
    let Some(root) = templates_root(ws) else { return vec![] };
    let group = notes_group(ws);
    let q = query.trim().to_lowercase();
    ws.children(Some(&root))
        .into_iter()
        .filter(|c| {
            matches!(c.doc, DocId::Page(_))
                && Some(c.doc.id()) != group.as_deref()
                && (q.is_empty() || c.title.to_lowercase().contains(&q))
        })
        .map(|c| SearchResult { doc: c.doc, title: c.title, icon: c.icon, snippet: String::new(), path: String::new() })
        .collect()
}

/// Titles of the group of transcription templates: the instructions the AI follows to write the notes of a
/// recorded meeting. Not to be confused with the page templates (meeting notes, action tracker, ...) that are
/// pasted into a page to give it a structure.
const GROUP_TITLES: [&str; 3] = ["transcription templates", "modèles de transcription", "modeles de transcription"];

/// Titles the same group had before 0.6.0. They are also the natural title of an ordinary page template, so a
/// page with such a title is the group only when it holds the built-in transcription templates.
const LEGACY_GROUP_TITLES: [&str; 3] = ["meeting notes", "notes de réunion", "notes de reunion"];

/// Beginnings of the explanation the group had before 0.6.0.
const OLD_GROUP_INTROS: [&str; 2] = ["Chaque sous-page est un modèle de notes", "Each sub-page is a notes template"];

fn is_group_title(t: &str) -> bool {
    GROUP_TITLES.contains(&t.trim().to_lowercase().as_str())
}

fn is_legacy_group_title(t: &str) -> bool {
    LEGACY_GROUP_TITLES.contains(&t.trim().to_lowercase().as_str())
}

/// Titles of the built-in transcription templates, in both languages.
fn builtin_labels() -> Vec<&'static str> {
    notecraft_ai::NOTE_TEMPLATES
        .iter()
        .filter(|k| **k != "custom")
        .flat_map(|k| [Lang::En, Lang::Fr].map(|l| crate::meeting::template_label(l, k)))
        .collect()
}

/// The id of the group page that holds the transcription templates, inside the templates page.
pub fn notes_group(ws: &Workspace) -> Option<String> {
    let root = templates_root(ws)?;
    let kids = ws.children(Some(&root));
    if let Some(id) = kids.iter().find_map(|c| match &c.doc {
        DocId::Page(id) if is_group_title(&c.title) => Some(id.clone()),
        _ => None,
    }) {
        return Some(id);
    }
    // a group made before 0.6.0: recognised by the built-in templates it holds, never by its title alone
    let labels = builtin_labels();
    kids.iter()
        .filter_map(|c| match &c.doc {
            DocId::Page(id) if is_legacy_group_title(&c.title) => {
                let n = ws.children(Some(id.as_str())).iter().filter(|g| labels.iter().any(|l| *l == g.title)).count();
                (n > 0).then(|| (n, id.clone()))
            }
            _ => None,
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, id)| id)
}

/// Title and explanation of the group of transcription templates.
fn group_texts(fr: bool) -> (&'static str, &'static str) {
    if fr {
        (
            "Modèles de transcription",
            "Chaque sous-page est un modèle de transcription : écrivez-y la structure et les consignes que l'IA suit pour rédiger le compte rendu d'une réunion enregistrée. Ce ne sont pas des modèles de page. Le modèle se choisit dans le bloc de réunion.",
        )
    } else {
        (
            "Transcription templates",
            "Each sub-page is a transcription template: write the structure and the instructions the AI follows to write up a recorded meeting. These are not page templates. Pick the template in the meeting block.",
        )
    }
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
    /// A transcription group made before 0.6.0 is called "Meeting notes", like an ordinary page template. Gives it
    /// its clear title (and explanation, when it is still the original one) so that the two families read apart.
    pub fn migrate_group_title(&mut self) {
        let fr = self.prefs.lang == Lang::Fr;
        let Some(ws) = self.ws.as_mut() else { return };
        let Some(g) = notes_group(ws) else { return };
        let doc = DocId::Page(g.clone());
        if is_group_title(&ws.title(&doc)) {
            return;
        }
        let (title, intro) = group_texts(fr);
        if ws.rename(&doc, title).is_err() {
            return;
        }
        if let Some(p) = ws.pages.get_mut(&g) {
            if p.blocks.len() == 1 && OLD_GROUP_INTROS.iter().any(|o| p.blocks[0].text.starts_with(o)) {
                p.blocks = vec![Block::paragraph(intro)];
            }
        }
        let _ = ws.save_page(&g);
    }

    /// Makes sure the templates page exists, with the standard meeting minutes in it and the group of note
    /// templates (what the AI follows to write the notes of a recorded meeting).
    pub fn ensure_templates(&mut self) {
        self.migrate_group_title();
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
                            "Chaque sous-page de cette page est un modèle de page. Écrivez ou modifiez-la comme une page ordinaire, puis insérez-la où vous voulez avec / puis Modèle. {{date}} devient la date du jour. Les modèles de transcription (ce que l'IA suit pour rédiger une réunion enregistrée) sont dans la page « Modèles de transcription ».",
                        )
                    } else {
                        (
                            "Templates",
                            "Each sub-page of this page is a page template. Write or edit it like any page, then insert it anywhere with / then Template. {{date}} becomes today's date. Templates the AI follows to write up a recorded meeting are in the \"Transcription templates\" page.",
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
                    let (gtitle, gintro) = group_texts(fr);
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
                // already there in either language: do not add a second copy after a change of language
                let other = crate::meeting::template_label(if fr { Lang::En } else { Lang::Fr }, key);
                if existing.iter().any(|t| t == name || t == other) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let w = Workspace::open(dir.path()).unwrap();
        (dir, w)
    }

    #[test]
    fn an_ordinary_meeting_notes_page_template_is_not_the_transcription_group() {
        let (_d, mut w) = ws();
        let root = w.create_page(None, "Templates").unwrap();
        let page_tpl = w.create_page(Some(&root), "Meeting notes").unwrap();
        assert_eq!(notes_group(&w), None, "a page with that title and no built-in template inside is a page template");
        // a group made before 0.6.0: recognised by the built-in templates it holds
        let old = w.create_page(Some(&root), "Notes de réunion").unwrap();
        w.create_page(Some(&old), "General summary").unwrap();
        assert_eq!(notes_group(&w), Some(old));
        let hits: Vec<String> = template_results(&w, "").into_iter().map(|r| r.doc.id().to_string()).collect();
        assert_eq!(hits, vec![page_tpl], "the finder offers page templates only");
    }

    #[test]
    fn the_transcription_title_is_recognised_in_both_languages() {
        let (_d, mut w) = ws();
        let root = w.create_page(None, "Modèles").unwrap();
        let g = w.create_page(Some(&root), "Modèles de transcription").unwrap();
        assert_eq!(notes_group(&w), Some(g));
        assert!(is_group_title("Transcription templates"));
        assert!(!is_group_title("Meeting notes"));
    }
}
