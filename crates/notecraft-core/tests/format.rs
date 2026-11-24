//! File format versioning: files carry `format:`, newer ones are read but never rewritten.

use notecraft_core::{DocId, Error, Workspace, FORMAT_VERSION};
use std::fs;

fn open(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_at(dir.path().join("notes"), dir.path().join("idx/index.db")).unwrap()
}

#[test]
fn written_files_carry_the_format_and_older_files_still_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = open(&dir);
    let id = ws.create_page(None, "Hello").unwrap();
    let db = ws.create_database(None, "Tasks").unwrap();
    let page_file = ws.pages[&id].path.clone().unwrap();
    assert!(fs::read_to_string(&page_file).unwrap().starts_with(&format!("---\nformat: {FORMAT_VERSION}\n")));
    let schema = fs::read_to_string(ws.dbs[&db].dir.join("schema.json")).unwrap();
    assert!(schema.trim_start().starts_with(&format!("{{\n  \"format\": {FORMAT_VERSION}")), "{schema}");

    // a file written before `format:` existed has none: it is format 1 and opens normally
    let legacy = fs::read_to_string(&page_file).unwrap().replace(&format!("format: {FORMAT_VERSION}\n"), "");
    fs::write(&page_file, legacy).unwrap();
    let mut ws = open(&dir);
    assert!(ws.newer_format_files().is_empty());
    assert!(ws.save(&DocId::Page(id)).is_ok());
}

#[test]
fn a_newer_format_is_read_but_the_workspace_refuses_every_change() {
    let dir = tempfile::tempdir().unwrap();
    let (id, file) = {
        let mut ws = open(&dir);
        let id = ws.create_page(None, "From the future").unwrap();
        let other = ws.create_page(None, "Other").unwrap();
        assert!(ws.pages.contains_key(&other));
        (id.clone(), ws.pages[&id].path.clone().unwrap())
    };
    let text = fs::read_to_string(&file).unwrap().replace("format: 1", "format: 99");
    fs::write(&file, &text).unwrap();

    let mut ws = open(&dir);
    assert_eq!(ws.pages[&id].meta.title, "From the future", "it can still be read");
    assert_eq!(ws.newer_format_files().len(), 1);
    assert!(ws.warnings.iter().any(|w| w.contains("format 99")));

    let doc = DocId::Page(id.clone());
    assert!(matches!(ws.save(&doc), Err(Error::Newer(_))));
    assert!(matches!(ws.create_page(None, "New"), Err(Error::Newer(_))));
    assert!(matches!(ws.rename(&doc, "Renamed"), Err(Error::Newer(_))));
    assert!(matches!(ws.trash_doc(&doc), Err(Error::Newer(_))));
    assert_eq!(fs::read_to_string(&file).unwrap(), text, "the file is untouched");
}
