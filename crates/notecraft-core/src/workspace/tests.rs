use super::*;
use crate::database::{PropValue, ViewKind};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let w = Workspace::open(dir.path()).unwrap();
    (dir, w)
}

#[test]
fn layout_is_created_and_the_index_lives_elsewhere() {
    let (dir, w) = ws();
    for d in ["pages", "databases", "assets"] {
        assert!(dir.path().join(d).is_dir(), "{d}");
    }
    assert!(!dir.path().join(".notecraft").exists());
    assert!(!dir.path().join(".gitignore").exists(), "the notes folder holds only notes");
    assert!(w.index_file.exists());
    assert!(!w.index_file.starts_with(dir.path()));
}

#[test]
fn pages_persist_and_reload() {
    let (dir, mut w) = ws();
    let a = w.create_page(None, "Alpha").unwrap();
    let b = w.create_page(Some(&a), "Beta").unwrap();
    w.pages.get_mut(&b).unwrap().blocks.push(Block::paragraph("hello [[Alpha]]"));
    w.save_page(&b).unwrap();
    let w2 = Workspace::open(dir.path()).unwrap();
    assert_eq!(w2.pages.len(), 2);
    assert_eq!(w2.pages[&b].meta.parent.as_deref(), Some(a.as_str()));
    assert_eq!(w2.children(Some(&a)).len(), 1);
    assert_eq!(w2.backlinks(&DocId::Page(a.clone())), vec![DocId::Page(b.clone())]);
    let crumbs = w2.breadcrumbs(&DocId::Page(b));
    assert_eq!(crumbs.len(), 2);
}

#[test]
fn rename_updates_file_and_mentions() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "Old name").unwrap();
    let b = w.create_page(None, "Other").unwrap();
    w.pages.get_mut(&b).unwrap().blocks.push(Block::paragraph("see [[Old name]]"));
    w.save_page(&b).unwrap();
    let old_path = w.pages[&a].path.clone().unwrap();
    w.rename(&DocId::Page(a.clone()), "New name").unwrap();
    assert!(!old_path.exists());
    assert!(w.pages[&a].path.as_ref().unwrap().to_string_lossy().contains("new-name"));
    assert_eq!(w.pages[&b].blocks[0].text, "see [[New name]]");
    assert_eq!(w.backlinks(&DocId::Page(a)).len(), 1);
}

#[test]
fn reorder_and_no_cycles() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "A").unwrap();
    let b = w.create_page(None, "B").unwrap();
    let c = w.create_page(None, "C").unwrap();
    w.move_to(&DocId::Page(c.clone()), None, 0).unwrap();
    let titles: Vec<_> = w.children(None).into_iter().map(|t| t.title).collect();
    assert_eq!(titles, vec!["C", "A", "B"]);
    w.move_to(&DocId::Page(b.clone()), Some(&a), 0).unwrap();
    assert!(w.move_to(&DocId::Page(a.clone()), Some(&b), 0).is_err());
}

#[test]
fn trash_restore_and_delete() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "Parent").unwrap();
    let b = w.create_page(Some(&a), "Child words").unwrap();
    w.pages.get_mut(&b).unwrap().blocks.push(Block::paragraph("zebra"));
    w.save_page(&b).unwrap();
    assert_eq!(w.search("zebra", 10).len(), 1);
    w.trash_doc(&DocId::Page(a.clone())).unwrap();
    assert!(!w.is_live(&DocId::Page(b.clone())));
    assert!(w.search("zebra", 10).is_empty());
    assert_eq!(w.trash().len(), 1);
    w.restore(&DocId::Page(a.clone())).unwrap();
    assert!(w.is_live(&DocId::Page(b.clone())));
    w.trash_doc(&DocId::Page(a.clone())).unwrap();
    let path = w.pages[&b].path.clone().unwrap();
    w.empty_trash().unwrap();
    assert!(w.pages.is_empty());
    assert!(!path.exists());
}

#[test]
fn favorites_listed() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "Fav").unwrap();
    w.set_favorite(&DocId::Page(a), true).unwrap();
    assert_eq!(w.favorites().len(), 1);
}

#[test]
fn databases_rows_templates_and_properties() {
    let (dir, mut w) = ws();
    let db = w.create_database(None, "Tasks").unwrap();
    let tpl = w.add_template(&db, "Bug").unwrap();
    let status = w.dbs[&db].schema.prop_by_name("Status").unwrap().clone();
    {
        let t = w.dbs.get_mut(&db).unwrap().templates.get_mut(&tpl).unwrap();
        t.set_value(&status, PropValue::Select("In progress".into()));
        t.blocks.push(Block::paragraph("Steps to reproduce"));
    }
    w.save_template(&db, &tpl).unwrap();
    let r = w.add_row(&db, Some(&tpl), "Crash on start").unwrap();
    assert_eq!(w.dbs[&db].rows[&r].value(&status), PropValue::Select("In progress".into()));
    assert_eq!(w.dbs[&db].rows[&r].blocks.len(), 1);
    w.rename_property(&db, &status.id, "State").unwrap();
    assert!(w.dbs[&db].rows[&r].meta.properties.contains_key("State"));
    let num = w.add_property(&db, "Points", PropKind::Number).unwrap();
    w.dbs.get_mut(&db).unwrap().schema.views.push(crate::database::View::new("Board", ViewKind::Board));
    w.save_schema(&db).unwrap();
    // reload from disk
    let w2 = Workspace::open(dir.path()).unwrap();
    let d2 = &w2.dbs[&db];
    assert_eq!(d2.rows.len(), 1);
    assert_eq!(d2.templates.len(), 1);
    assert_eq!(d2.schema.views.len(), 2);
    assert!(d2.schema.prop(&num).is_some());
    assert_eq!(w2.search("crash", 5)[0].doc, DocId::Row(db.clone(), r.clone()));
    // folder rename with title
    let mut w3 = w2;
    w3.rename(&DocId::Database(db.clone()), "Bugs").unwrap();
    assert!(w3.dbs[&db].dir.to_string_lossy().contains("bugs-"));
    assert!(w3.dbs[&db].rows[&r].path.as_ref().unwrap().exists());
    w3.delete_property(&db, &num).unwrap();
    assert!(w3.dbs[&db].schema.prop(&num).is_none());
}

#[test]
fn assets_are_copied() {
    let (dir, mut w) = ws();
    let src = dir.path().join("photo.PNG");
    fs::write(&src, b"img").unwrap();
    let rel = w.add_asset(&src).unwrap();
    assert_eq!(rel, "assets/photo.png");
    // Same content: reused, not duplicated.
    let rel2 = w.add_asset(&src).unwrap();
    assert_eq!(rel2, rel);
    // Different content with the same name: numbered.
    fs::write(&src, b"other").unwrap();
    let rel3 = w.add_asset(&src).unwrap();
    assert_eq!(rel3, "assets/photo 2.png");
    assert!(w.asset_path(&rel).exists());
}

#[test]
fn index_can_be_deleted_and_rebuilt() {
    let (dir, mut w) = ws();
    let a = w.create_page(None, "Kiwi").unwrap();
    w.pages.get_mut(&a).unwrap().blocks.push(Block::paragraph("banana"));
    w.save_page(&a).unwrap();
    let index = w.index_file.clone();
    drop(w);
    fs::remove_file(&index).unwrap();
    let w2 = Workspace::open(dir.path()).unwrap();
    assert_eq!(w2.search("banana", 5).len(), 1);
}

#[test]
fn external_edits_are_noticed_and_own_writes_are_not() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "Alpha").unwrap();
    w.pages.get_mut(&a).unwrap().blocks.push(Block::paragraph("one"));
    w.save_page(&a).unwrap();
    assert!(w.external_changes().is_empty());
    // someone else edits the file
    let path = w.pages[&a].path.clone().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let text = fs::read_to_string(&path).unwrap().replace("one", "two, from elsewhere");
    fs::write(&path, text).unwrap();
    let changed = w.external_changes();
    assert_eq!(changed, vec![path.clone()]);
    assert_eq!(w.doc_for_path(&path), Some(DocId::Page(a.clone())));
    w.sync_from_disk().unwrap();
    assert_eq!(w.pages[&a].blocks[0].text, "two, from elsewhere");
    assert!(w.external_changes().is_empty());
}

#[test]
fn reload_keeps_ids_of_unchanged_blocks() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "Live").unwrap();
    w.pages.get_mut(&a).unwrap().blocks = vec![Block::paragraph("first"), Block::paragraph("second")];
    w.save_page(&a).unwrap();
    let first_id = w.pages[&a].blocks[0].id;
    let path = w.pages[&a].path.clone().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&path, fs::read_to_string(&path).unwrap().replace("second", "second, edited elsewhere")).unwrap();
    w.sync_from_disk().unwrap();
    assert_eq!(w.pages[&a].blocks[0].id, first_id);
    assert_eq!(w.pages[&a].blocks[1].text, "second, edited elsewhere");
}

#[test]
fn doc_keys_round_trip() {
    for d in [DocId::Page("a".into()), DocId::Database("b".into()), DocId::Row("c".into(), "d".into())] {
        assert_eq!(DocId::from_key(&d.key()), Some(d));
    }
}
#[test]
fn encrypted_pages_hide_their_text_and_unlock_with_the_passphrase() {
    let (dir, mut w) = ws();
    let id = w.create_page(None, "Journal").unwrap();
    w.pages.get_mut(&id).unwrap().blocks = blocks::from_markdown("Le code est zorglub42");
    w.save_page(&id).unwrap();
    assert!(w.encrypt_page(&id, "short").is_err(), "passphrase too short");
    w.encrypt_page(&id, "correct horse battery").unwrap();
    let file = w.pages[&id].path.clone().unwrap();
    let on_disk = std::fs::read_to_string(&file).unwrap();
    assert!(on_disk.contains("encrypted: true") && on_disk.contains("Journal"));
    assert!(!on_disk.contains("zorglub42"));
    assert!(w.search("zorglub42", 10).is_empty(), "never indexed");
    // a saved edit stays encrypted
    w.pages.get_mut(&id).unwrap().blocks = blocks::from_markdown("Nouveau secret xyzzy7");
    w.save_page(&id).unwrap();
    assert!(!std::fs::read_to_string(&file).unwrap().contains("xyzzy7"));
    // reopen: locked until unlocked
    let mut w2 = Workspace::open(dir.path()).unwrap();
    assert!(w2.is_page_locked(&id));
    assert!(w2.pages[&id].blocks.is_empty());
    assert!(w2.unlock_page(&id, "wrong passphrase").is_err());
    w2.unlock_page(&id, "correct horse battery").unwrap();
    assert_eq!(blocks::plain_text(&w2.pages[&id].blocks), "Nouveau secret xyzzy7");
    // locking again forgets everything, and saving a locked page keeps the ciphertext
    w2.lock_vault();
    assert!(w2.is_page_locked(&id) && !w2.vault_is_open());
    w2.save_page(&id).unwrap();
    w2.unlock_page(&id, "correct horse battery").unwrap();
    assert_eq!(blocks::plain_text(&w2.pages[&id].blocks), "Nouveau secret xyzzy7");
    // decrypting restores a plain page
    w2.decrypt_page(&id).unwrap();
    assert!(std::fs::read_to_string(&file).unwrap().contains("xyzzy7"));
}

#[test]
fn encrypting_erases_plain_copies_from_the_backups() {
    let (_dir, mut w) = ws();
    let id = w.create_page(None, "Secret").unwrap();
    w.pages.get_mut(&id).unwrap().blocks = blocks::from_markdown("plainmarker99");
    w.save_page(&id).unwrap();
    crate::backup::run_backup(&w.root, &w.snapshot_dir()).unwrap();
    let base = w.snapshot_dir();
    let has_plain = |base: &Path| {
        let mut found = false;
        let mut stack = vec![base.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if std::fs::read_to_string(&p).is_ok_and(|t| t.contains("plainmarker99")) {
                    found = true;
                }
            }
        }
        found
    };
    assert!(has_plain(&base));
    w.encrypt_page(&id, "a long enough phrase").unwrap();
    assert!(!has_plain(&base));
}

#[test]
fn the_same_passphrase_is_required_for_every_encrypted_page() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "A").unwrap();
    let b = w.create_page(None, "B").unwrap();
    w.encrypt_page(&a, "first passphrase").unwrap();
    w.lock_vault();
    assert!(w.encrypt_page(&b, "another passphrase").is_err());
    w.encrypt_page(&b, "first passphrase").unwrap();
}

#[test]
fn a_reminder_is_kept_on_the_page_and_survives_a_reload() {
    let d = tempfile::tempdir().unwrap();
    let mut ws = Workspace::open(d.path()).unwrap();
    let a = ws.create_page(None, "Call the bank").unwrap();
    let b = ws.create_page(None, "Renew passport").unwrap();
    ws.set_reminder(&a, "2026-12-02").unwrap();
    ws.set_reminder(&b, "2026-12-01 08:30").unwrap();
    assert!(ws.set_reminder(&b, "soon").is_err());
    let ws = Workspace::open(d.path()).unwrap();
    let r = ws.reminders();
    assert_eq!(r.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), ["Renew passport", "Call the bank"]);
    assert_eq!(r[1].2, "2026-12-02 09:00");
    let mut ws = ws;
    ws.set_reminder(&a, "").unwrap();
    assert_eq!(ws.reminders().len(), 1);
}

#[test]
fn unique_ids_are_given_in_order_never_reused_and_not_copied() {
    let (dir, mut w) = ws();
    let db = w.create_database(None, "Tickets").unwrap();
    let a = w.add_row(&db, None, "A").unwrap();
    let b = w.add_row(&db, None, "B").unwrap();
    let p = w.add_property(&db, "No", PropKind::UniqueId).unwrap();
    let prop = w.dbs[&db].schema.properties.iter().find(|x| x.id == p).unwrap().clone();
    // rows that existed before the property are numbered by age
    let na = w.dbs[&db].rows[&a].value(&prop).display();
    let nb = w.dbs[&db].rows[&b].value(&prop).display();
    assert_eq!((na.as_str(), nb.as_str()), ("1", "2"));
    // a new row continues after the highest number, a copy gets its own number
    let c = w.add_row(&db, None, "C").unwrap();
    let d = w.duplicate_row(&db, &a).unwrap();
    assert_eq!(w.dbs[&db].rows[&c].value(&prop).display(), "3");
    assert_eq!(w.dbs[&db].rows[&d].value(&prop).display(), "4");
    // deleting the last row does not free its number for the others, and the number is read-only
    w.dbs.get_mut(&db).unwrap().rows.get_mut(&a).unwrap().set_value(&prop, PropValue::Number(99.0));
    assert_eq!(w.dbs[&db].rows[&a].value(&prop).display(), "1");
    // numbers survive a reload
    let w2 = Workspace::open(dir.path()).unwrap();
    assert_eq!(w2.dbs[&db].rows[&c].value(&prop).display(), "3");
}

#[test]
fn person_and_files_properties_hold_lists_and_survive_a_reload() {
    let (dir, mut w) = ws();
    let db = w.create_database(None, "Docs").unwrap();
    let who = w.add_property(&db, "Owner", PropKind::Person).unwrap();
    let files = w.add_property(&db, "Attachments", PropKind::Files).unwrap();
    let r = w.add_row(&db, None, "Row").unwrap();
    let get = |w: &Workspace, id: &str| w.dbs[&db].schema.properties.iter().find(|x| x.id == id).unwrap().clone();
    let (who, files) = (get(&w, &who), get(&w, &files));
    let row = w.dbs.get_mut(&db).unwrap().rows.get_mut(&r).unwrap();
    row.set_value(&who, PropValue::Multi(vec!["Ernest".into(), "Marianne".into()]));
    row.set_value(&files, PropValue::Multi(vec!["assets/a.pdf".into()]));
    w.save_row(&db, &r).unwrap();
    let w2 = Workspace::open(dir.path()).unwrap();
    let row = &w2.dbs[&db].rows[&r];
    assert_eq!(row.value(&who), PropValue::Multi(vec!["Ernest".into(), "Marianne".into()]));
    assert_eq!(row.value(&files), PropValue::Multi(vec!["assets/a.pdf".into()]));
}

#[test]
fn nested_filter_groups_combine_with_and_or_and_old_files_still_load() {
    use crate::database::{Filter, FilterGroup, FilterMode, FilterOp};
    let (_dir, mut w) = ws();
    let db = w.create_database(None, "Items").unwrap();
    let n = w.add_property(&db, "N", PropKind::Number).unwrap();
    let t = w.add_property(&db, "Kind", PropKind::Text).unwrap();
    let n_prop = w.dbs[&db].schema.properties.iter().find(|p| p.id == n).unwrap().clone();
    let t_prop = w.dbs[&db].schema.properties.iter().find(|p| p.id == t).unwrap().clone();
    let mut ids = Vec::new();
    for (title, num, kind) in [("a", 1.0, "x"), ("b", 5.0, "x"), ("c", 5.0, "y"), ("d", 9.0, "y")] {
        let r = w.add_row(&db, None, title).unwrap();
        let row = w.dbs.get_mut(&db).unwrap().rows.get_mut(&r).unwrap();
        row.set_value(&n_prop, PropValue::Number(num));
        row.set_value(&t_prop, PropValue::Text(kind.into()));
        ids.push(r);
    }
    let rule = |p: &str, op, v: &str| Filter { prop: p.to_string(), op, value: v.to_string() };
    // Kind is x  OR  (Kind is y AND N > 5)  ->  a, b, d
    let group = FilterGroup {
        mode: FilterMode::Or,
        rules: vec![rule(&t, FilterOp::Is, "x")],
        groups: vec![FilterGroup {
            mode: FilterMode::And,
            rules: vec![rule(&t, FilterOp::Is, "y"), rule(&n, FilterOp::Greater, "5")],
            groups: vec![],
        }],
    };
    let d = &w.dbs[&db];
    let passing: Vec<&str> = ids
        .iter()
        .filter(|r| crate::database::row_passes(&d.schema, &d.rows[*r], &group))
        .map(|r| d.rows[r].meta.title.as_str())
        .collect();
    assert_eq!(passing, vec!["a", "b", "d"]);
    // an empty nested group changes nothing
    let mut with_empty = group.clone();
    with_empty.groups.push(FilterGroup::default());
    assert!(ids.iter().all(|r| {
        crate::database::row_passes(&d.schema, &d.rows[r], &with_empty)
            == crate::database::row_passes(&d.schema, &d.rows[r], &group)
    }));
    assert_eq!(group.count(), 3);
    // a view written before nested groups existed still parses and writes back without a groups key
    let old: FilterGroup = serde_json::from_str(r#"{"mode":"or","rules":[]}"#).unwrap();
    assert!(old.groups.is_empty());
    assert!(!serde_json::to_string(&old).unwrap().contains("groups"));
}

#[test]
fn dashboard_totals_follow_the_view_filters() {
    use crate::database::{dashboard_data, Filter, FilterOp, View};
    let (_dir, mut w) = ws();
    let db = w.create_database(None, "Sales").unwrap();
    let amount = w.add_property(&db, "Amount", PropKind::Number).unwrap();
    let paid = w.add_property(&db, "Paid", PropKind::Checkbox).unwrap();
    let get = |w: &Workspace, id: &str| w.dbs[&db].schema.properties.iter().find(|x| x.id == id).unwrap().clone();
    let (amount, paid) = (get(&w, &amount), get(&w, &paid));
    for (t, n, p) in [("a", 100.0, true), ("b", 50.0, false), ("c", 10.0, true)] {
        let r = w.add_row(&db, None, t).unwrap();
        let row = w.dbs.get_mut(&db).unwrap().rows.get_mut(&r).unwrap();
        row.set_value(&amount, PropValue::Number(n));
        row.set_value(&paid, PropValue::Checkbox(p));
    }
    let mut view = View::new("d", ViewKind::Dashboard);
    let all = dashboard_data(&w.dbs[&db], &view);
    assert_eq!(all.total, 3);
    assert_eq!((all.numbers[0].sum, all.numbers[0].min, all.numbers[0].max), (160.0, 10.0, 100.0));
    assert_eq!(all.checks, vec![("Paid".to_string(), 2)]);
    view.filter.rules.push(Filter { prop: paid.id.clone(), op: FilterOp::Checked, value: String::new() });
    let paid_only = dashboard_data(&w.dbs[&db], &view);
    assert_eq!((paid_only.total, paid_only.numbers[0].sum), (2, 110.0));
}

#[test]
fn the_link_graph_follows_mentions_and_ignores_trashed_pages() {
    let (_dir, mut w) = ws();
    let a = w.create_page(None, "Alpha").unwrap();
    let b = w.create_page(None, "Beta").unwrap();
    let c = w.create_page(None, "Gamma").unwrap();
    let _lonely = w.create_page(None, "Lonely").unwrap();
    w.pages.get_mut(&a).unwrap().blocks.push(Block::paragraph("see [[Beta]] and [[beta]]"));
    w.pages.get_mut(&b).unwrap().blocks.push(Block::paragraph("back to [[Alpha]], also [[Gamma]]"));
    w.pages.get_mut(&c).unwrap().blocks.push(Block::paragraph("nothing [[Missing]]"));
    let g = w.link_graph();
    let names: Vec<&str> = g.titles.iter().map(|t| t.as_str()).collect();
    assert!(!names.contains(&"Lonely"));
    assert_eq!(g.nodes.len(), 3);
    // Alpha-Beta counts once although both link to each other, plus Beta-Gamma
    assert_eq!(g.edges.len(), 2);
    let beta = names.iter().position(|n| *n == "Beta").unwrap();
    assert_eq!(g.degree[beta], 2);
}

#[test]
fn pages_with_the_same_position_and_title_keep_a_stable_order() {
    let (_dir, mut w) = ws();
    let root = w.create_page(None, "Group").unwrap();
    let mut ids: Vec<String> = (0..6).map(|_| w.create_page(Some(&root), "Same").unwrap()).collect();
    ids.sort();
    for _ in 0..20 {
        let got: Vec<String> = w.children(Some(&root)).into_iter().map(|c| c.doc.id().to_string()).collect();
        assert_eq!(got, ids, "the id breaks the tie, in the same order every time");
    }
}
