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
            "Exemple : compte rendu de réunion",
            "## Compte rendu de réunion\n\n**Date :** {{date}}\n\n**Heure :** \n\n**Lieu ou lien :** \n\n**Animateur :** \n\n**Rédacteur :** \n\n### Participants\n\n- \n\n### Excusés\n\n- \n\n### Ordre du jour\n\n1. \n\n### Discussion\n\n- \n\n### Décisions\n\n- \n\n### Actions\n\n- [ ] Action (responsable, échéance)\n\n### Prochaine réunion\n\n- \n",
        )
    } else {
        (
            "Example: meeting minutes",
            "## Meeting minutes\n\n**Date:** {{date}}\n\n**Time:** \n\n**Location or link:** \n\n**Facilitator:** \n\n**Note taker:** \n\n### Attendees\n\n- \n\n### Apologies\n\n- \n\n### Agenda\n\n1. \n\n### Discussion\n\n- \n\n### Decisions\n\n- \n\n### Actions\n\n- [ ] Action (owner, due date)\n\n### Next meeting\n\n- \n",
        )
    }
}

/// Title and explanation of the top level templates page.
fn root_texts(fr: bool) -> (&'static str, &'static str) {
    if fr {
        (
            "Modèles",
            "Chaque sous-page de cette page est un modèle de page. Écrivez ou modifiez-la comme une page ordinaire, puis insérez-la où vous voulez avec / puis Modèle. {{date}} devient la date du jour. Les modèles de transcription (ce que l'IA suit pour rédiger une réunion enregistrée) sont dans la page « Modèles de transcription ».",
        )
    } else {
        (
            "Templates",
            "Each sub-page of this page is a page template. Write or edit it like any page, then insert it anywhere with / then Template. {{date}} becomes today's date. Templates the AI follows to write up a recorded meeting are in the \"Transcription templates\" page.",
        )
    }
}

/// Title and text of the example transcription template.
fn transcription_example(fr: bool) -> (&'static str, &'static str) {
    let text = notecraft_ai::note_structure_text("summary");
    if fr {
        ("Exemple : résumé général", text)
    } else {
        ("Example: general summary", text)
    }
}

/// Writes the example templates: one page template and one transcription template, nothing more. They go only
/// into a section that is created now, so templates the person already has are never duplicated and what they
/// delete does not come back. `first` is true on the very first run; `with_group` asks for the transcription
/// section even later (it is then created empty). Returns the id of the transcription example, if one was written.
fn seed(ws: &mut Workspace, fr: bool, first: bool, with_group: bool) -> notecraft_core::Result<Option<String>> {
    let root = match templates_root(ws) {
        Some(r) => r,
        None => {
            let (title, intro) = root_texts(fr);
            let root = ws.create_page(None, title)?;
            {
                let p = ws.pages.get_mut(&root).unwrap();
                p.meta.icon = "📋".into();
                p.blocks = vec![Block::paragraph(intro)];
            }
            ws.save_page(&root)?;
            if first {
                let (name, md) = minutes(fr);
                let sub = ws.create_page(Some(&root), name)?;
                {
                    let p = ws.pages.get_mut(&sub).unwrap();
                    p.meta.icon = "📝".into();
                    p.blocks = blocks::from_markdown(md);
                }
                ws.save_page(&sub)?;
            }
            root
        }
    };
    let mut example = None;
    if notes_group(ws).is_none() && (first || with_group) {
        let (gtitle, gintro) = group_texts(fr);
        let g = ws.create_page(Some(&root), gtitle)?;
        {
            let p = ws.pages.get_mut(&g).unwrap();
            p.meta.icon = "🎙".into();
            p.blocks = vec![Block::paragraph(gintro)];
        }
        ws.save_page(&g)?;
        if first {
            let (name, text) = transcription_example(fr);
            let sub = ws.create_page(Some(&g), name)?;
            ws.pages.get_mut(&sub).unwrap().blocks = vec![Block::paragraph(text)];
            ws.save_page(&sub)?;
            example = Some(sub);
        }
    }
    Ok(example)
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

    /// Makes sure the templates page exists. The very first time it also writes one example of each kind, in a
    /// section it creates (see [`seed`]); it never adds anything to a section that is already there.
    pub fn ensure_templates(&mut self) {
        self.ensure_templates_with(false);
    }

    /// Same, and the transcription section exists afterwards (empty when it has to be created again).
    pub fn ensure_transcription_group(&mut self) {
        self.ensure_templates_with(true);
    }

    fn ensure_templates_with(&mut self, with_group: bool) {
        self.migrate_group_title();
        if self.prefs.meeting.seed_failed {
            return;
        }
        let fr = self.prefs.lang == Lang::Fr;
        let first = !self.prefs.meeting.notes_seeded;
        let Some(ws) = self.ws.as_mut() else { return };
        if !first && templates_root(ws).is_some() && (!with_group || notes_group(ws).is_some()) {
            return;
        }
        match seed(ws, fr, first, with_group) {
            Ok(example) => {
                self.prefs.meeting.notes_seeded = true;
                // the default choice becomes the example page, so that the menu shows it as selected
                if let Some(id) = example {
                    if self.prefs.meeting.template == "summary" {
                        self.prefs.meeting.template = format!("page:{id}");
                    }
                }
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

    #[test]
    fn only_one_example_of_each_kind_is_written_and_only_into_a_new_section() {
        let (_d, mut w) = ws();
        let id = seed(&mut w, false, true, false).unwrap().expect("the transcription example");
        let root = templates_root(&w).unwrap();
        let group = notes_group(&w).unwrap();
        let page_templates = template_results(&w, "");
        assert_eq!(page_templates.len(), 1, "one page template");
        assert!(page_templates[0].title.starts_with("Example"));
        assert_eq!(note_templates(&w).len(), 1, "one transcription template");
        assert_eq!(note_templates(&w)[0].0, format!("page:{id}"));
        // what the person deletes does not come back
        w.trash_doc(&DocId::Page(id)).unwrap();
        w.trash_doc(&DocId::Page(page_templates[0].doc.id().to_string())).unwrap();
        assert_eq!(seed(&mut w, false, false, false).unwrap(), None);
        assert!(note_templates(&w).is_empty() && template_results(&w, "").is_empty());
        assert_eq!(templates_root(&w), Some(root));
        assert_eq!(notes_group(&w), Some(group));
    }

    #[test]
    fn templates_the_person_already_has_are_never_duplicated() {
        let (_d, mut w) = ws();
        let root = w.create_page(None, "Templates").unwrap();
        let mine = w.create_page(Some(&root), "Meeting notes").unwrap();
        let group = w.create_page(Some(&root), "Transcription templates").unwrap();
        w.create_page(Some(&group), "My own").unwrap();
        // a first run on a workspace that already has both sections writes nothing
        assert_eq!(seed(&mut w, true, true, true).unwrap(), None);
        assert_eq!(w.children(Some(&root)).len(), 2);
        assert_eq!(note_templates(&w).len(), 1);
        assert_eq!(template_results(&w, "")[0].doc.id(), mine);
    }

    #[test]
    fn a_deleted_transcription_section_comes_back_only_when_asked_and_empty() {
        let (_d, mut w) = ws();
        seed(&mut w, false, true, false).unwrap();
        let group = notes_group(&w).unwrap();
        w.trash_doc(&DocId::Page(group)).unwrap();
        assert_eq!(seed(&mut w, false, false, false).unwrap(), None);
        assert_eq!(notes_group(&w), None, "the page templates button does not bring it back");
        assert_eq!(seed(&mut w, false, false, true).unwrap(), None);
        let g = notes_group(&w).expect("asked for explicitly");
        assert!(w.children(Some(&g)).is_empty(), "and it comes back without examples");
    }
}
