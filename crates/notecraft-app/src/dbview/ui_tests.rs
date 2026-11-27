//! The database controls driven like a person would: real clicks on a headless egui, no window.

use super::*;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

fn props() -> Vec<Property> {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::open(dir.path().join("w")).unwrap();
    let db = ws.create_database(None, "T").unwrap();
    ws.add_property(&db, "Done", PropKind::Checkbox).unwrap();
    ws.dbs[&db].schema.properties.clone()
}

fn filter_harness(group: database::FilterGroup) -> Harness<'static, (database::FilterGroup, Vec<Property>, bool)> {
    Harness::builder().build_ui_state(
        |ui, (group, props, changed)| filter_group_ui(ui, group, props, Lang::En, "t", 0, changed),
        (group, props(), false),
    )
}

fn rule(p: &Property) -> database::Filter {
    database::Filter { prop: p.id.clone(), op: FilterOp::for_kind(p.kind)[0], value: String::new() }
}

#[test]
fn filter_groups_can_be_added_nested_two_deep_and_no_deeper() {
    let p = props();
    let mut h = filter_harness(database::FilterGroup { rules: vec![rule(&p[0])], ..Default::default() });
    h.run();
    assert_eq!(h.get_all_by_label("+  Add filter group").count(), 1);
    h.get_by_label("+  Add filter group").click();
    h.run();
    assert_eq!(h.state().0.groups.len(), 1);
    assert!(h.state().2, "the change is reported so the view is saved");
    assert_eq!(h.get_all_by_label("+  Add filter group").count(), 2, "root and first level");
    // second level: through the button of the nested group, drawn before the root's own buttons
    h.get_all_by_label("+  Add filter group").next().unwrap().click();
    h.run();
    assert_eq!(h.state().0.groups[0].groups.len(), 1);
    assert_eq!(h.get_all_by_label("+  Add filter group").count(), 2, "the deepest level offers no more groups");
    assert_eq!(h.get_all_by_label("Group").count(), 2);
}

#[test]
fn the_and_or_switch_appears_with_two_entries_and_changes_the_mode() {
    let p = props();
    let mut h = filter_harness(database::FilterGroup { rules: vec![rule(&p[0])], ..Default::default() });
    h.run();
    assert!(h.query_by_label("Or").is_none(), "a single rule needs no switch");
    h.get_by_label("+  Add a filter").click();
    h.run();
    assert_eq!(h.state().0.rules.len(), 2);
    assert_eq!(h.state().0.mode, FilterMode::And);
    h.get_by_label("Or").click();
    h.run();
    assert_eq!(h.state().0.mode, FilterMode::Or);
}

#[test]
fn removing_a_nested_group_leaves_the_others_alone() {
    let p = props();
    let inner = database::FilterGroup { rules: vec![rule(&p[0])], ..Default::default() };
    let group =
        database::FilterGroup { rules: vec![rule(&p[0])], groups: vec![inner.clone(), inner], ..Default::default() };
    let mut h = filter_harness(group);
    h.run();
    assert_eq!(h.state().0.groups.len(), 2);
    // the root rule's remove button comes first, then for each group its header's and its rule's
    let crosses: Vec<_> = h.get_all_by_label("Remove").collect();
    assert_eq!(crosses.len(), 5, "1 root rule + 2 headers + 2 nested rules");
    crosses[1].click();
    h.run();
    assert_eq!(h.state().0.groups.len(), 1);
    assert_eq!(h.state().0.rules.len(), 1);
}

#[test]
fn the_dashboard_shows_counts_sums_and_ticked_boxes() {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::open(dir.path().join("w")).unwrap();
    let db = ws.create_database(None, "Sales").unwrap();
    let amount = ws.add_property(&db, "Amount", PropKind::Number).unwrap();
    let done = ws.add_property(&db, "Paid", PropKind::Checkbox).unwrap();
    for (i, n) in ["10", "20", "30"].iter().enumerate() {
        let id = ws.add_row(&db, None, &format!("r{i}")).unwrap();
        let d = ws.dbs.get_mut(&db).unwrap();
        let (pa, pd) = (
            d.schema.properties.iter().find(|p| p.id == amount).unwrap().clone(),
            d.schema.properties.iter().find(|p| p.id == done).unwrap().clone(),
        );
        let row = d.rows.get_mut(&id).unwrap();
        row.set_value(&pa, notecraft_core::database::PropValue::Number(n.parse().unwrap()));
        row.set_value(&pd, notecraft_core::database::PropValue::Checkbox(i < 2));
    }
    let view = ws.dbs[&db].schema.views[0].clone();
    let pal = Palette::light();
    let mut h = Harness::new_ui_state(
        move |ui, ws: &mut Workspace| dashboard::dashboard(ui, ws, &db, &view, &pal, Lang::En),
        ws,
    );
    h.run();
    assert!(h.query_by_label("Rows").is_some());
    assert!(h.query_by_label("3").is_some(), "three rows");
    assert!(h.query_by_label("Amount · Total").is_some());
    assert!(h.query_by_label("60").is_some(), "10 + 20 + 30");
    assert!(h.query_by_label_contains("Average 20").is_some());
    assert!(h.query_by_label_contains("Min 10").is_some() && h.query_by_label_contains("Max 30").is_some());
}

fn cell_harness(
    kind: PropKind,
    value: Option<notecraft_core::database::PropValue>,
) -> (Harness<'static, (Workspace, Vec<DbAction>, String)>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::open(dir.path().join("w")).unwrap();
    let db = ws.create_database(None, "Crew").unwrap();
    let pid = ws.add_property(&db, "Field", kind).unwrap();
    let rid = ws.add_row(&db, None, "Row").unwrap();
    let prop = ws.dbs[&db].schema.properties.iter().find(|p| p.id == pid).unwrap().clone();
    if let Some(v) = value {
        ws.dbs.get_mut(&db).unwrap().rows.get_mut(&rid).unwrap().set_value(&prop, v);
    }
    let h = Harness::builder().with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
        move |ui, (ws, actions, new_option): &mut (Workspace, Vec<DbAction>, String)| {
            let pal = Palette::light();
            cell_editor_for_test(ui, ws, &db, &rid, &prop, &pal, actions, new_option);
        },
        (ws, Vec::new(), String::new()),
    );
    (h, dir)
}

fn cell_editor_for_test(
    ui: &mut Ui,
    ws: &mut Workspace,
    db: &str,
    rid: &str,
    p: &Property,
    pal: &Palette,
    actions: &mut Vec<DbAction>,
    new_option: &mut String,
) {
    cells::cell_editor(ui, ws, db, rid, p, 200.0, pal, Lang::En, actions, new_option);
}

#[test]
fn a_unique_id_shows_its_number_and_offers_nothing_to_edit() {
    let (mut h, _d) = cell_harness(PropKind::UniqueId, None);
    h.run();
    assert!(h.query_by_label("#1").is_some(), "the first row is number 1");
    assert_eq!(h.query_all_by_role(egui::accesskit::Role::Button).count(), 0, "nothing to click");
    assert_eq!(h.query_all_by_role(egui::accesskit::Role::TextInput).count(), 0);
}

#[test]
fn files_show_one_chip_each_and_a_plus_to_add_more() {
    let v = notecraft_core::database::PropValue::Multi(vec!["assets/report.pdf".into(), "assets/photo.png".into()]);
    let (mut h, _d) = cell_harness(PropKind::Files, Some(v));
    h.run();
    assert!(h.query_by_label_contains("report.pdf").is_some() && h.query_by_label_contains("photo.png").is_some());
    assert!(
        h.query_by_label("Add files").is_some() || h.query_by_label_contains("Add file").is_some(),
        "the + button is named"
    );
}

#[test]
fn people_toggle_like_a_multiple_choice_and_a_select_stays_single() {
    let mut person = props()[0].clone();
    person.kind = PropKind::Person;
    let v = cells::toggle_option(&person, &["Ernest".into()], "Marianne");
    assert_eq!(v, notecraft_core::database::PropValue::Multi(vec!["Ernest".into(), "Marianne".into()]));
    let v = cells::toggle_option(&person, &["Ernest".into(), "Marianne".into()], "Ernest");
    assert_eq!(v, notecraft_core::database::PropValue::Multi(vec!["Marianne".into()]));
    person.kind = PropKind::Select;
    assert_eq!(
        cells::toggle_option(&person, &["Ernest".into()], "Marianne"),
        notecraft_core::database::PropValue::Select("Marianne".into())
    );
}

#[test]
fn a_menu_stays_open_while_a_dropdown_inside_it_is_used() {
    let mut h = Harness::builder().build_ui_state(
        |ui, choice: &mut usize| {
            let btn = ui.button("Menu");
            crate::widgets::sticky_popup(&btn, Id::unique("t-menu"), 200.0, |ui| {
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt("pick").selected_text(format!("choice {choice}")).show_ui(ui, |ui| {
                        for n in 0..3 {
                            if ui.selectable_label(*choice == n, format!("option {n}")).clicked() {
                                *choice = n;
                            }
                        }
                    }),
                    "pick",
                );
            });
        },
        0usize,
    );
    h.run();
    h.get_by_label("Menu").click();
    h.run();
    for n in [2usize, 1, 2] {
        h.get_by_label("pick").click();
        h.run();
        h.get_by_label(&format!("option {n}")).click();
        h.run();
        assert_eq!(*h.state(), n, "the option is taken");
        assert!(h.query_by_label("pick").is_some(), "the menu is still open after choosing");
    }
}
