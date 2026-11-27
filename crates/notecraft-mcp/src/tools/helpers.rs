//! Argument parsing, lookups, guards and small builders shared by the tools.

use super::*;

pub(super) fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub(super) fn need<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    str_arg(args, key).ok_or_else(|| ToolError::Failed(format!("missing argument {key}")))
}

pub(super) fn limit(args: &Value, default: u64) -> usize {
    args.get("limit").and_then(Value::as_u64).unwrap_or(default).clamp(1, 1000) as usize
}

/// Any document (page, row or database) by id or title.
pub(crate) fn find_any(ws: &Workspace, key: &str) -> Result<DocId, ToolError> {
    if ws.pages.contains_key(key) {
        return Ok(DocId::Page(key.to_string()));
    }
    if ws.dbs.contains_key(key) {
        return Ok(DocId::Database(key.to_string()));
    }
    for (db, d) in &ws.dbs {
        if d.rows.contains_key(key) {
            return Ok(DocId::Row(db.clone(), key.to_string()));
        }
    }
    if let Some(d) = ws.resolve_title(key) {
        return Ok(d);
    }
    // trashed documents by title (for restore_page)
    for (id, p) in &ws.pages {
        if p.meta.title.eq_ignore_ascii_case(key) {
            return Ok(DocId::Page(id.clone()));
        }
    }
    for (id, d) in &ws.dbs {
        if d.schema.title.eq_ignore_ascii_case(key) {
            return Ok(DocId::Database(id.clone()));
        }
    }
    fail(format!("not found: {key}"))
}

/// A page or row (something with a body).
pub(super) fn find_doc(ws: &Workspace, key: &str) -> Result<DocId, ToolError> {
    match find_any(ws, key)? {
        DocId::Database(_) => fail(format!("{key} is a database; use get_database or query_rows")),
        d => Ok(d),
    }
}

pub(super) fn find_page_id(ws: &Workspace, key: &str) -> Result<String, ToolError> {
    match find_any(ws, key)? {
        DocId::Page(id) => Ok(id),
        _ => fail(format!("{key} is not a page")),
    }
}

pub(super) fn find_db(ws: &Workspace, key: &str) -> Result<String, ToolError> {
    if ws.dbs.contains_key(key) {
        return Ok(key.to_string());
    }
    ws.dbs
        .iter()
        .find(|(_, d)| d.schema.title.eq_ignore_ascii_case(key) && d.schema.trashed.is_none())
        .map(|(id, _)| id.clone())
        .ok_or_else(|| ToolError::Failed(format!("database not found: {key}")))
}

pub(super) fn find_row(ws: &Workspace, db: &str, key: &str) -> Result<String, ToolError> {
    let d = &ws.dbs[db];
    if d.rows.contains_key(key) {
        return Ok(key.to_string());
    }
    d.live_rows()
        .find(|r| r.meta.title.eq_ignore_ascii_case(key))
        .map(|r| r.meta.id.clone())
        .ok_or_else(|| ToolError::Failed(format!("row not found: {key}")))
}

pub(super) fn find_prop(ws: &Workspace, db: &str, key: &str) -> Result<Property, ToolError> {
    let s = &ws.dbs[db].schema;
    s.prop(key).or_else(|| s.properties.iter().find(|p| p.name.eq_ignore_ascii_case(key))).cloned().ok_or_else(|| {
        ToolError::Failed(format!(
            "unknown property {key}; properties are {}",
            s.properties.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
        ))
    })
}

pub(super) fn parse_kind(s: &str) -> Result<PropKind, ToolError> {
    let norm = s.trim().to_lowercase().replace(['-', ' '], "_");
    let norm = match norm.as_str() {
        "multiselect" => "multi_select".to_string(),
        "last_edited_time" => "edited_time".to_string(),
        "string" | "rich_text" => "text".to_string(),
        "bool" | "boolean" => "checkbox".to_string(),
        other => other.to_string(),
    };
    serde_json::from_value::<PropKind>(Value::String(norm.clone()))
        .ok()
        .filter(|k| *k != PropKind::Title)
        .ok_or_else(|| ToolError::Failed(format!("unknown property type {s}")))
}

/// Convert a JSON value into a property value for this property.
pub(super) fn to_prop_value(ws: &Workspace, db: &str, p: &Property, v: &Value) -> Result<PropValue, ToolError> {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let list = |v: &Value| -> Vec<String> {
        match v {
            Value::Array(a) => a.iter().map(text).collect(),
            Value::Null => vec![],
            other => text(other).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        }
    };
    Ok(match p.kind {
        PropKind::Number => match v {
            Value::Number(n) => PropValue::Number(n.as_f64().unwrap_or(0.0)),
            Value::Null => PropValue::Empty,
            other => match text(other).trim().replace(',', ".").parse() {
                Ok(n) => PropValue::Number(n),
                Err(_) => return fail(format!("{}: expected a number", p.name)),
            },
        },
        PropKind::Checkbox => PropValue::Checkbox(match v {
            Value::Bool(b) => *b,
            other => matches!(text(other).to_lowercase().as_str(), "true" | "yes" | "1" | "x"),
        }),
        PropKind::Select | PropKind::Status => {
            let t = text(v);
            if !t.is_empty() && p.option(&t).is_none() && p.kind == PropKind::Status {
                let names: Vec<&str> = p.options.iter().map(|o| o.name.as_str()).collect();
                return fail(format!("{}: unknown status {t}; use one of {}", p.name, names.join(", ")));
            }
            PropValue::Select(t)
        }
        PropKind::MultiSelect | PropKind::Person | PropKind::Files => PropValue::Multi(list(v)),
        PropKind::Relation => {
            let target = p.relation_db.clone().unwrap_or_else(|| db.to_string());
            let mut ids = Vec::new();
            for key in list(v) {
                ids.push(find_row(ws, &target, &key)?);
            }
            PropValue::Multi(ids)
        }
        PropKind::Date => PropValue::Date(text(v)),
        PropKind::CreatedTime | PropKind::EditedTime | PropKind::Formula | PropKind::Rollup => {
            return fail(format!("{} is computed and cannot be set", p.name))
        }
        _ => PropValue::Text(text(v)),
    })
}

/// Apply {name: value}; new select options join the schema.
pub(super) fn set_properties(
    ws: &mut Workspace,
    db: &str,
    rid: &str,
    props: &Map<String, Value>,
    template: bool,
) -> Result<(), ToolError> {
    let mut schema_changed = false;
    for (name, v) in props {
        let p = find_prop(ws, db, name)?;
        if p.kind == PropKind::Title {
            let title = v.as_str().unwrap_or_default().to_string();
            let d = ws.dbs.get_mut(db).unwrap();
            let r = if template { d.templates.get_mut(rid) } else { d.rows.get_mut(rid) }.unwrap();
            r.meta.title = title;
            continue;
        }
        let value = to_prop_value(ws, db, &p, v)?;
        if matches!(p.kind, PropKind::Select | PropKind::MultiSelect | PropKind::Person) {
            let names: Vec<String> = match &value {
                PropValue::Select(s) if !s.is_empty() => vec![s.clone()],
                PropValue::Multi(v) => v.clone(),
                _ => vec![],
            };
            let pp = ws.dbs.get_mut(db).unwrap().schema.properties.iter_mut().find(|x| x.id == p.id).unwrap();
            for n in names {
                if pp.option(&n).is_none() {
                    let color = database::COLORS[(pp.options.len() + 1) % database::COLORS.len()].to_string();
                    pp.options.push(database::SelectOption { name: n, color, group: None });
                    schema_changed = true;
                }
            }
        }
        let d = ws.dbs.get_mut(db).unwrap();
        let r = if template { d.templates.get_mut(rid) } else { d.rows.get_mut(rid) }.unwrap();
        r.set_value(&p, value);
    }
    if schema_changed {
        ws.save_schema(db)?;
    }
    Ok(())
}

/// Apply `filter`, `match`, `sort` and `group_by` from the arguments to a view.
pub(super) fn apply_view_spec(ws: &Workspace, db: &str, view: &mut View, args: &Value) -> Result<(), ToolError> {
    match args.get("filter") {
        Some(Value::Object(f)) => {
            for (name, v) in f {
                let p = find_prop(ws, db, name)?;
                let value = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let op = match p.kind {
                    PropKind::Checkbox => {
                        if value == "true" {
                            FilterOp::Checked
                        } else {
                            FilterOp::Unchecked
                        }
                    }
                    PropKind::Select | PropKind::Status | PropKind::Number | PropKind::Date => FilterOp::Is,
                    _ => FilterOp::Contains,
                };
                view.filter.rules.push(database::Filter { prop: p.id.clone(), op, value });
            }
        }
        Some(Value::Array(rules)) => {
            for r in rules {
                let p = find_prop(ws, db, r.get("property").and_then(Value::as_str).unwrap_or(""))?;
                let op_name = r.get("op").and_then(Value::as_str).unwrap_or("is");
                let op: FilterOp = serde_json::from_value(json!(op_name))
                    .map_err(|_| ToolError::Failed(format!("unknown op {op_name}")))?;
                let value = match r.get("value") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Null) | None => String::new(),
                    Some(other) => other.to_string(),
                };
                view.filter.rules.push(database::Filter { prop: p.id.clone(), op, value });
            }
        }
        _ => {}
    }
    if str_arg(args, "match") == Some("any") {
        view.filter.mode = FilterMode::Or;
    }
    if let Some(Value::Array(sorts)) = args.get("sort") {
        view.sorts.clear();
        for s in sorts {
            let p = find_prop(ws, db, s.get("property").and_then(Value::as_str).unwrap_or(""))?;
            view.sorts.push(Sort {
                prop: p.id.clone(),
                descending: s.get("descending").and_then(Value::as_bool).unwrap_or(false),
            });
        }
    }
    if let Some(g) = str_arg(args, "group_by") {
        let p = find_prop(ws, db, g)?;
        view.group_by = Some(p.id.clone());
    }
    Ok(())
}

pub(super) fn block_kind_name(b: &Block) -> &'static str {
    match &b.kind {
        BlockKind::Paragraph => "text",
        BlockKind::Heading(1) => "h1",
        BlockKind::Heading(2) => "h2",
        BlockKind::Heading(_) => "h3",
        BlockKind::Bulleted => "bulleted",
        BlockKind::Numbered => "numbered",
        BlockKind::Todo { .. } => "todo",
        BlockKind::Toggle { .. } => "toggle",
        BlockKind::Quote => "quote",
        BlockKind::Callout { .. } => "callout",
        BlockKind::Code { .. } => "code",
        BlockKind::Divider => "divider",
        BlockKind::Toc => "toc",
        _ => "other",
    }
}

pub(super) fn block_index(args: &Value, key: &str, len: usize, inclusive: bool) -> Result<usize, ToolError> {
    let i = args.get(key).and_then(Value::as_u64).ok_or_else(|| ToolError::Failed(format!("{key} must be a number")))?
        as usize;
    if i > len || (i == len && !inclusive) {
        return fail(format!("{key} {i} is out of range (the page has {len} blocks)"));
    }
    Ok(i)
}

/// Refuse a block edit when the block no longer starts with the text the caller read.
pub(super) fn check_expect(args: &Value, block: &Block) -> Result<(), ToolError> {
    if let Some(e) = args.get("expect").and_then(Value::as_str) {
        if !block.text.trim_start().starts_with(e.trim()) {
            return fail("the block changed since you read it; call get_blocks again");
        }
    }
    Ok(())
}

pub(super) fn find_view_index(ws: &Workspace, db: &str, key: &str) -> Result<usize, ToolError> {
    ws.dbs[db]
        .schema
        .views
        .iter()
        .position(|v| v.id == key || v.name.eq_ignore_ascii_case(key))
        .ok_or_else(|| ToolError::Failed(format!("view not found: {key}")))
}

pub(super) fn apply_view_extras(ws: &Workspace, db: &str, view: &mut View, args: &Value) -> Result<(), ToolError> {
    if let Some(Value::Array(names)) = args.get("visible") {
        view.visible.clear();
        for n in names {
            let p = find_prop(ws, db, n.as_str().unwrap_or(""))?;
            view.visible.push(p.id.clone());
        }
    }
    if let Some(e) = str_arg(args, "end_by") {
        view.end_by = if e.is_empty() { None } else { Some(find_prop(ws, db, e)?.id.clone()) };
    }
    Ok(())
}

pub(super) fn row_json(ws: &Workspace, db: &str, rid: &str) -> Value {
    let d = &ws.dbs[db];
    let r = &d.rows[rid];
    let mut props = Map::new();
    for p in &d.schema.properties {
        if p.kind != PropKind::Title {
            props.insert(p.name.clone(), Value::String(ws.display_value(db, r, p)));
        }
    }
    json!({"id": r.meta.id, "title": r.meta.title, "icon": r.meta.icon, "properties": props, "updated": r.meta.updated})
}

pub(super) fn schema_json(ws: &Workspace, db: &str) -> Value {
    let d = &ws.dbs[db];
    let props: Vec<Value> = d
        .schema
        .properties
        .iter()
        .map(|p| {
            let mut v = json!({"id": p.id, "name": p.name, "type": p.kind.key()});
            if !p.options.is_empty() {
                v["options"] = json!(p
                    .options
                    .iter()
                    .map(|o| json!({"name": o.name, "color": o.color, "group": o.group}))
                    .collect::<Vec<_>>());
            }
            if let Some(r) = &p.relation_db {
                v["relation_database"] = json!(r);
            }
            if !p.formula.is_empty() {
                v["formula"] = json!(p.formula);
            }
            if let Some(r) = &p.rollup {
                v["rollup"] = json!({"relation": r.relation, "property": r.target, "function": r.func.key()});
            }
            v
        })
        .collect();
    let views: Vec<Value> = d.schema.views.iter().map(|v| json!({"id": v.id, "name": v.name, "type": v.kind, "filters": v.filter.count(), "sorts": v.sorts.len()})).collect();
    json!({"id": db, "title": d.schema.title, "icon": d.schema.icon, "properties": props, "views": views})
}

pub(super) fn tree_text(ws: &Workspace, parent: Option<&str>, depth: usize, out: &mut String) {
    for item in ws.children(parent) {
        let kind = if matches!(item.doc, DocId::Database(_)) { " [database]" } else { "" };
        let title = if item.title.is_empty() { "Untitled" } else { &item.title };
        let icon = if item.icon.is_empty() { String::new() } else { format!("{} ", item.icon) };
        out.push_str(&format!("{}- {icon}{title}{kind} (id {})\n", "  ".repeat(depth), item.doc.id()));
        tree_text(ws, Some(item.doc.id()), depth + 1, out);
    }
}

pub(super) fn doc_line(ws: &Workspace, d: &DocId) -> String {
    let kind = match d {
        DocId::Page(_) => "page",
        DocId::Database(_) => "database",
        DocId::Row(..) => "row",
    };
    let path = ws.path_label(d);
    format!(
        "- {} {} ({kind}, id {}){}",
        ws.icon(d),
        ws.title(d),
        d.id(),
        if path.is_empty() { String::new() } else { format!(" in {path}") }
    )
}

pub(super) fn block_from_json(v: &Value) -> Result<Block, ToolError> {
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("text");
    let text = v.get("text").and_then(Value::as_str).unwrap_or("").to_string();
    let kind = match ty {
        "text" | "paragraph" => BlockKind::Paragraph,
        "h1" | "heading_1" => BlockKind::Heading(1),
        "h2" | "heading_2" => BlockKind::Heading(2),
        "h3" | "heading_3" => BlockKind::Heading(3),
        "bulleted" | "bulleted_list" | "bullet" => BlockKind::Bulleted,
        "numbered" | "numbered_list" => BlockKind::Numbered,
        "todo" | "to_do" => BlockKind::Todo { checked: v.get("checked").and_then(Value::as_bool).unwrap_or(false) },
        "toggle" => BlockKind::Toggle { open: true },
        "quote" => BlockKind::Quote,
        "callout" => BlockKind::Callout { icon: v.get("icon").and_then(Value::as_str).unwrap_or("💡").to_string() },
        "code" => BlockKind::Code { lang: v.get("language").and_then(Value::as_str).unwrap_or("").to_string() },
        "divider" => BlockKind::Divider,
        "toc" | "table_of_contents" => BlockKind::Toc,
        other => return fail(format!("unknown block type {other}")),
    };
    let depth = v.get("depth").and_then(Value::as_u64).unwrap_or(0).min(blocks::MAX_DEPTH as u64) as u8;
    Ok(Block::new(kind, text).with_depth(depth))
}

pub(super) fn body_mut<'a>(ws: &'a mut Workspace, doc: &DocId) -> Result<&'a mut Vec<Block>, ToolError> {
    ws.blocks_mut_checked(doc).map_err(ToolError::from)
}

pub(super) fn save_body(ws: &mut Workspace, doc: &DocId) -> Result<(), ToolError> {
    ws.touch(doc);
    ws.save(doc)?;
    Ok(())
}

/// Refuse what is outside the scope the server is limited to. Tools that act on the whole workspace
/// or on the top level are refused when a scope is set, unless they name a place inside it.
pub(super) fn guard_scope(ws: &Workspace, name: &str, args: &Value, scope: &Scope) -> Result<(), ToolError> {
    if !scope.is_active() {
        return Ok(());
    }
    let denied = |what: &str| ToolError::Denied(format!("{what} is outside the scope this server is limited to"));
    let named = |key: &str| str_arg(args, key).is_some_and(|v| !v.is_empty());
    match name {
        "create_page" | "create_database" | "import_markdown" | "move_page" if !named("parent") => {
            return Err(ToolError::Denied(format!("{name}: give a parent inside the scope this server is limited to")));
        }
        "export_markdown" | "add_attachment" if !named("page") => {
            return Err(ToolError::Denied(format!("{name}: give a page inside the scope this server is limited to")));
        }
        "empty_trash" => return Err(ToolError::Denied("empty_trash cannot be limited to a scope".into())),
        _ => {}
    }
    for key in ["page", "parent"] {
        if let Some(k) = str_arg(args, key).filter(|k| !k.is_empty()) {
            if let Ok(doc) = find_any(ws, k) {
                if !scope.contains(ws, &doc) {
                    return Err(denied(k));
                }
            }
        }
    }
    if let Some(db) = str_arg(args, "database").and_then(|k| find_db(ws, k).ok()) {
        let doc = DocId::Database(db);
        if !scope.contains(ws, &doc) {
            return Err(denied(&ws.title(&doc)));
        }
    }
    Ok(())
}

/// Refuse a write aimed at a locked document (or into a locked parent), before anything changes in memory.
pub(super) fn guard_locks(ws: &Workspace, name: &str, args: &Value) -> Result<(), ToolError> {
    if !WRITE_TOOLS.contains(&name) || matches!(name, "export_markdown" | "lock_page" | "set_favorite" | "restore_page")
    {
        return Ok(());
    }
    for key in ["page", "parent"] {
        if let Some(doc) = str_arg(args, key).and_then(|k| find_any(ws, k).ok()) {
            ws.ensure_unlocked(&doc)?;
        }
    }
    if let Some(db) = str_arg(args, "database").and_then(|k| find_db(ws, k).ok()) {
        ws.ensure_unlocked(&DocId::Database(db.clone()))?;
        if let Some(rid) = str_arg(args, "row").and_then(|k| find_row(ws, &db, k).ok()) {
            ws.ensure_unlocked(&DocId::Row(db, rid))?;
        }
    }
    Ok(())
}

pub(super) fn add_options(ws: &mut Workspace, db: &str, pid: &str, options: Option<&Value>) -> Result<(), ToolError> {
    let Some(Value::Array(opts)) = options else { return Ok(()) };
    if let Some(p) = ws.dbs.get_mut(db).unwrap().schema.properties.iter_mut().find(|p| p.id == pid) {
        for (k, o) in opts.iter().enumerate() {
            if let Some(n) = o.as_str() {
                if p.option(n).is_none() {
                    let color = database::COLORS[(k + 1) % database::COLORS.len()].to_string();
                    p.options.push(database::SelectOption { name: n.to_string(), color, group: None });
                }
            }
        }
    }
    ws.save_schema(db)?;
    Ok(())
}

/// Properties and body of a new row; on error the half-made row is removed.
pub(super) fn finish_row(ws: &mut Workspace, db: &str, rid: &str, args: &Value) -> Result<(), ToolError> {
    let result = (|| -> Result<(), ToolError> {
        if let Some(Value::Object(props)) = args.get("properties") {
            set_properties(ws, db, rid, props, false)?;
        }
        if let Some(md) = str_arg(args, "markdown") {
            ws.dbs.get_mut(db).unwrap().rows.get_mut(rid).unwrap().blocks = blocks::from_markdown(md);
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = ws.delete_forever(&DocId::Row(db.to_string(), rid.to_string()));
        return Err(e);
    }
    ws.save_row(db, rid)?;
    Ok(())
}

/// ` Notes: a; b` when the Markdown used tags with no equivalent, else nothing.
pub(super) fn notes_suffix(notes: &[String]) -> String {
    if notes.is_empty() {
        String::new()
    } else {
        format!(" Notes: {}", notes.join("; "))
    }
}

/// Refuse to remove a page that is still published on the web: only a person can unpublish it,
/// from the app, and the page may only go once its link is down.
pub(super) fn ensure_not_published(ws: &Workspace, docs: &[DocId]) -> Result<(), ToolError> {
    let mut ids = std::collections::HashSet::new();
    for d in docs {
        ids.insert(d.id().to_string());
        for x in ws.descendants(d) {
            ids.insert(x.id().to_string());
        }
    }
    let roots = notecraft_core::publications::live_roots(&ws.root.display().to_string(), &ids);
    if roots.is_empty() {
        return Ok(());
    }
    let names: Vec<String> =
        roots.iter().map(|r| ws.pages.get(r).map(|p| p.meta.title.clone()).unwrap_or_else(|| r.clone())).collect();
    Err(ToolError::Failed(format!(
        "still published on the web: {}. Ask the user to unpublish it in NoteCraft first (page menu, Unpublish).",
        names.join(", ")
    )))
}
