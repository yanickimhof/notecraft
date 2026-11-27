use super::*;

fn sample_db() -> Database {
    let mut schema = Database::new_schema("db1".into(), "Tasks", None);
    schema.properties.push(Property::new("Points", PropKind::Number));
    schema.properties.push(Property::new("Done", PropKind::Checkbox));
    let mut db = Database { schema, rows: BTreeMap::new(), templates: BTreeMap::new(), dir: PathBuf::new() };
    let props = db.schema.properties.clone();
    let p = |n: &str| props.iter().find(|p| p.name == n).unwrap().clone();
    for (i, (title, status, pts, tags)) in [
        ("Write report", "In progress", 3.0, vec!["Work"]),
        ("Buy milk", "Not started", 1.0, vec!["Personal"]),
        ("Plan trip", "Done", 5.0, vec!["Personal", "Idea"]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut r = Row::new(title);
        r.meta.created = format!("2026-01-0{}T00:00:00Z", i + 1);
        r.set_value(&p("Status"), PropValue::Select(status.into()));
        r.set_value(&p("Points"), PropValue::Number(pts));
        r.set_value(&p("Tags"), PropValue::Multi(tags.into_iter().map(String::from).collect()));
        r.set_value(&p("Done"), PropValue::Checkbox(status == "Done"));
        db.rows.insert(r.meta.id.clone(), r);
    }
    db
}

fn titles(db: &Database, ids: &[String]) -> Vec<String> {
    ids.iter().map(|i| db.rows[i].meta.title.clone()).collect()
}

#[test]
fn row_file_round_trip() {
    let db = sample_db();
    let row = db.rows.values().next().unwrap();
    let text = row.render().unwrap();
    let back = Row::parse(&text).unwrap();
    assert_eq!(back.meta, row.meta);
}

#[test]
fn sort_by_number_desc() {
    let db = sample_db();
    let pts = db.schema.prop_by_name("Points").unwrap().id.clone();
    let mut v = View::new("t", ViewKind::Table);
    v.sorts.push(Sort { prop: pts, descending: true });
    assert_eq!(titles(&db, &sorted_rows(&db, &v)), vec!["Plan trip", "Write report", "Buy milk"]);
}

#[test]
fn filters_and_or() {
    let db = sample_db();
    let tags = db.schema.prop_by_name("Tags").unwrap().id.clone();
    let pts = db.schema.prop_by_name("Points").unwrap().id.clone();
    let mut v = View::new("t", ViewKind::Table);
    v.filter.rules.push(Filter { prop: tags.clone(), op: FilterOp::Contains, value: "personal".into() });
    v.filter.rules.push(Filter { prop: pts, op: FilterOp::Greater, value: "2".into() });
    assert_eq!(titles(&db, &sorted_rows(&db, &v)), vec!["Plan trip"]);
    v.filter.mode = FilterMode::Or;
    assert_eq!(sorted_rows(&db, &v).len(), 3);
}

#[test]
fn checkbox_and_empty_filters() {
    let db = sample_db();
    let done = db.schema.prop_by_name("Done").unwrap().id.clone();
    let date = db.schema.prop_by_name("Date").unwrap().id.clone();
    let mut v = View::new("t", ViewKind::Table);
    v.filter.rules.push(Filter { prop: done, op: FilterOp::Checked, value: String::new() });
    assert_eq!(titles(&db, &sorted_rows(&db, &v)), vec!["Plan trip"]);
    v.filter.rules = vec![Filter { prop: date, op: FilterOp::IsEmpty, value: String::new() }];
    assert_eq!(sorted_rows(&db, &v).len(), 3);
}

#[test]
fn board_groups_by_status_in_group_order() {
    let db = sample_db();
    let v = View::new("Board", ViewKind::Board);
    let groups = query(&db, &v);
    let keys: Vec<_> = groups.iter().map(|g| g.key.clone()).collect();
    assert_eq!(keys, vec![None, Some("Not started".into()), Some("In progress".into()), Some("Done".into())]);
    assert_eq!(titles(&db, &groups[3].rows), vec!["Plan trip"]);
}

#[test]
fn multi_select_rows_appear_in_each_group() {
    let db = sample_db();
    let tags = db.schema.prop_by_name("Tags").unwrap().id.clone();
    let mut v = View::new("t", ViewKind::List);
    v.group_by = Some(tags);
    let groups = query(&db, &v);
    let personal = groups.iter().find(|g| g.key.as_deref() == Some("Personal")).unwrap();
    assert_eq!(personal.rows.len(), 2);
}

#[test]
fn computed_times() {
    let db = sample_db();
    let mut schema = db.schema.clone();
    schema.properties.push(Property::new("Created", PropKind::CreatedTime));
    let p = schema.prop_by_name("Created").unwrap();
    let r = db.rows.values().next().unwrap();
    assert!(matches!(r.value(p), PropValue::Date(d) if d.starts_with("2026-01")));
}

#[test]
fn number_format() {
    assert_eq!(format_number(3.0), "3");
    assert_eq!(format_number(2.5), "2.5");
}

#[test]
fn chart_counts_and_sums_by_group() {
    let db = sample_db();
    let status = db.schema.prop_by_name("Status").unwrap().id.clone();
    let points = db.schema.prop_by_name("Points").unwrap().id.clone();
    let mut view = View::new("c", ViewKind::Chart);
    view.group_by = Some(status);
    let counts = chart_data(&db, &view);
    assert_eq!(counts.iter().map(|c| c.1).sum::<f64>(), 3.0);
    view.chart.func = ChartFunc::Sum;
    view.chart.value = Some(points);
    let sums = chart_data(&db, &view);
    assert_eq!(sums.iter().map(|c| c.1).sum::<f64>(), 9.0);
    assert_eq!(sums.iter().find(|c| c.0 == "Done").unwrap().1, 5.0);
}

#[test]
fn timeline_orders_rows_and_fixes_bad_ends() {
    let mut db = sample_db();
    let start = Property::new("Start", PropKind::Date);
    let end = Property::new("End", PropKind::Date);
    db.schema.properties.push(start.clone());
    db.schema.properties.push(end.clone());
    let ids: Vec<String> = db.rows.keys().cloned().collect();
    let set = |db: &mut Database, rid: &str, p: &Property, v: &str| {
        db.rows.get_mut(rid).unwrap().set_value(p, PropValue::Date(v.into()));
    };
    set(&mut db, &ids[0], &start, "2026-03-10");
    set(&mut db, &ids[0], &end, "2026-03-20");
    set(&mut db, &ids[1], &start, "2026-03-01T09:00:00");
    set(&mut db, &ids[1], &end, "2026-02-01"); // before the start: one day item
    let mut view = View::new("t", ViewKind::Timeline);
    view.group_by = Some(start.id.clone());
    view.end_by = Some(end.id.clone());
    let (items, undated) = timeline_items(&db, &view);
    assert_eq!(items.len(), 2);
    assert_eq!(undated.len(), 1);
    assert_eq!(items[0].start.to_string(), "2026-03-01");
    assert_eq!(items[0].end, items[0].start);
    assert_eq!(items[1].end.to_string(), "2026-03-20");
}
