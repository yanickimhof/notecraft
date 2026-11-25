use super::*;

#[test]
fn a_target_key_round_trips() {
    for t in [
        Target::Doc(DocId::Page("a1".into())),
        Target::Doc(DocId::Database("d1".into())),
        Target::Doc(DocId::Row("d1".into(), "r9".into())),
        Target::Template("d1".into(), "t2".into()),
    ] {
        assert_eq!(Target::from_key(&t.key()), Some(t));
    }
    assert_eq!(Target::from_key("x:1"), None);
}

#[test]
fn a_folder_moved_to_the_trash_keeps_its_content_and_a_clash_gets_a_new_name() {
    let base = std::env::temp_dir().join(format!("nc-trash-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let trash = base.join("trash");
    std::fs::create_dir_all(&trash).unwrap();
    for round in 0..2 {
        let ws = base.join("Notes");
        std::fs::create_dir_all(ws.join("pages")).unwrap();
        std::fs::write(ws.join("pages").join("a.md"), "x").unwrap();
        move_into(&ws, &trash).unwrap();
        assert!(!ws.exists(), "round {round}: the folder left its place");
    }
    let kept: Vec<_> = std::fs::read_dir(&trash).unwrap().filter_map(|e| e.ok()).collect();
    assert_eq!(kept.len(), 2, "the second one did not overwrite the first");
    assert!(kept.iter().all(|e| e.path().join("pages").join("a.md").is_file()));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_page_in_the_trash_is_no_longer_a_live_target() {
    let dir = std::env::temp_dir().join(format!("nc-live-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = Workspace::open(&dir).unwrap();
    let parent = ws.create_page(None, "Parent").unwrap();
    let child = ws.create_page(Some(&parent), "Child").unwrap();
    let (p, c) = (Target::Doc(DocId::Page(parent.clone())), Target::Doc(DocId::Page(child.clone())));
    assert!(target_is_live(&ws, &p) && target_is_live(&ws, &c));
    ws.trash_doc(&DocId::Page(parent.clone())).unwrap();
    assert!(!target_is_live(&ws, &p), "the trashed page must not stay open");
    assert!(!target_is_live(&ws, &c), "a page under a trashed one must not stay open either");
    ws.restore(&DocId::Page(parent)).unwrap();
    assert!(target_is_live(&ws, &p) && target_is_live(&ws, &c));
    let gone = Target::Doc(DocId::Page("missing".into()));
    assert!(!target_is_live(&ws, &gone));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_archived_page_is_no_longer_a_live_target() {
    let dir = std::env::temp_dir().join(format!("nc-archived-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = Workspace::open(&dir).unwrap();
    let id = ws.create_page(None, "Old").unwrap();
    let t = Target::Doc(DocId::Page(id.clone()));
    assert!(target_is_live(&ws, &t));
    ws.set_archived(&id, true).unwrap();
    assert!(!target_is_live(&ws, &t), "an archived page must leave the window");
    ws.set_archived(&id, false).unwrap();
    assert!(target_is_live(&ws, &t));
    let _ = std::fs::remove_dir_all(&dir);
}
