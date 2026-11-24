//! The MCP tools. Pages, rows and databases can be named by id or by exact
//! title. Write tools are listed in [`WRITE_TOOLS`] and refused in read-only mode.

use crate::policy::{Policy, Scope};
use notecraft_core::blocks::{self, Block, BlockKind};
use notecraft_core::database::{self, FilterMode, FilterOp, PropKind, PropValue, Property, Sort, View, ViewKind};
use notecraft_core::{import_export, DocId, Workspace};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub enum ToolError {
    Unknown,
    ReadOnly,
    /// Refused by the server's policy (level or scope), with the reason.
    Denied(String),
    Failed(String),
}

impl From<notecraft_core::Error> for ToolError {
    fn from(e: notecraft_core::Error) -> Self {
        ToolError::Failed(e.to_string())
    }
}

type R = Result<String, ToolError>;

/// Entries per page of a listing (a listing stops at 200).
pub const PAGE_SIZE: usize = 200;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError::Failed(msg.into()))
}

/// Tools that change the workspace (hidden and refused with `--read-only`).
pub const WRITE_TOOLS: &[&str] = &[
    "create_page",
    "update_page",
    "replace_text",
    "append_blocks",
    "insert_blocks",
    "update_block",
    "move_block",
    "delete_block",
    "create_view",
    "update_view",
    "delete_view",
    "create_template",
    "delete_template",
    "rename_page",
    "move_page",
    "duplicate_page",
    "set_icon",
    "set_cover",
    "set_favorite",
    "set_reminder",
    "trash_page",
    "lock_page",
    "restore_page",
    "empty_trash",
    "create_database",
    "add_property",
    "rename_property",
    "delete_property",
    "add_row",
    "update_row",
    "delete_row",
    "add_row_from_template",
    "add_attachment",
    "import_markdown",
    "export_markdown",
];

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    let read_only = !WRITE_TOOLS.contains(&name);
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
        "annotations": {"readOnlyHint": read_only, "destructiveHint": matches!(name, "empty_trash" | "delete_property" | "update_page")}
    })
}

/// Tool descriptions sent to the client.
pub fn definitions(policy: &Policy) -> Vec<Value> {
    let page = json!({"type": "string", "description": "Page or row id, or exact title"});
    let db = json!({"type": "string", "description": "Database id or exact title"});
    let row = json!({"type": "string", "description": "Row id or exact title"});
    let md = json!({"type": "string", "description": "Markdown: # headings, - lists, 1. lists, - [ ] to-dos, > quotes, ``` code, --- dividers, [[Page]] links"});
    let props = json!({"type": "object", "description": "{property name: value}. Values: text, number, true/false, YYYY-MM-DD, option name, or a list for multi-select and relations (row titles or ids)"});
    let all = vec![
        // pages
        tool("list_pages", "The page tree (titles, ids, icons, nesting), databases included, 200 entries per page; pass next_cursor as cursor for more.", json!({"cursor": {"type": "string"}}), &[]),
        tool("get_page", "A page or database row: metadata (id, title, icon, parent, created, updated, favourite), row properties, backlinks and the Markdown body, between <content> and </content> (pass exactly that text to update_page to rewrite it unchanged). Long pages: read them in slices with offset and max_chars (characters of the body).", json!({"page": page, "offset": {"type": "integer", "description": "First body character to return (default 0)"}, "max_chars": {"type": "integer", "description": "Return at most this many body characters"}}), &["page"]),
        tool("search", "Full-text search (titles and bodies, accent-insensitive, prefix matching) over pages, databases and rows, with snippets.", json!({"query": {"type": "string"}, "limit": {"type": "integer", "default": 20}}), &["query"]),
        tool("backlinks", "Pages and rows that link to this page with [[Title]].", json!({"page": page}), &["page"]),
        tool("recent_pages", "Most recently edited pages, databases and rows.", json!({"limit": {"type": "integer", "default": 20}}), &[]),
        tool("favorites", "Pages and databases marked as favourites.", json!({}), &[]),
        tool("create_page", "Create a page with an optional Markdown body.", json!({"title": {"type": "string"}, "markdown": md, "parent": {"type": "string", "description": "Parent page id or title"}, "icon": {"type": "string", "description": "An emoji"}}), &["title"]),
        tool("replace_text", "Replace a passage of the page's Markdown (as get_page shows it) by another one, leaving the rest untouched: the safe way to correct a long page. `old` must occur exactly once unless all is true. A snapshot is taken first.", json!({"page": page, "old": {"type": "string"}, "new": {"type": "string"}, "all": {"type": "boolean", "default": false}}), &["page", "old", "new"]),
        tool("update_page", "Replace or append to the Markdown body of a page or row.", json!({"page": page, "markdown": md, "mode": {"type": "string", "enum": ["replace", "append"], "default": "append"}}), &["page", "markdown"]),
        tool("append_blocks", "Append blocks to a page or row. Each block: {type: text|h1|h2|h3|bulleted|numbered|todo|toggle|quote|callout|code|divider, text, checked?, language?, depth?}.", json!({"page": page, "blocks": {"type": "array", "items": {"type": "object"}}}), &["page", "blocks"]),
        tool("rename_page", "Rename a page, row or database; [[mentions]] of the old title follow.", json!({"page": page, "title": {"type": "string"}}), &["page", "title"]),
        tool("move_page", "Move a page or database under another page (or to the top level), at a position among its siblings.", json!({"page": page, "parent": {"type": "string", "description": "New parent page id or title; omit for the top level"}, "position": {"type": "integer", "description": "0 = first; default last"}}), &["page"]),
        tool("duplicate_page", "Duplicate a page (its blocks, icon and cover) next to it.", json!({"page": page}), &["page"]),
        tool("set_icon", "Set the emoji icon of a page, row or database (empty string removes it).", json!({"page": page, "icon": {"type": "string"}}), &["page", "icon"]),
        tool("set_cover", "Set a cover: gradient:0 … gradient:7, an assets/… path, or empty to remove.", json!({"page": page, "cover": {"type": "string"}}), &["page", "cover"]),
        tool("set_favorite", "Add a page or database to the favourites, or remove it.", json!({"page": page, "favorite": {"type": "boolean"}}), &["page", "favorite"]),
        tool("get_blocks", "The blocks of a page or row, numbered from 0: {index, type, text, depth, checked?, language?}. Use the index with update_block, insert_blocks, move_block and delete_block. Blocks of other kinds (tables, columns, images, meetings...) are listed as type \"other\" and left untouched.", json!({"page": page}), &["page"]),
        tool("insert_blocks", "Insert blocks at a position (index 0 = top; an index equal to the number of blocks appends). Same block format as append_blocks.", json!({"page": page, "index": {"type": "integer"}, "blocks": {"type": "array", "items": {"type": "object"}}}), &["page", "index", "blocks"]),
        tool("update_block", "Change one block in place. Pass `expect` (the start of the block text as read) to be safe against concurrent edits. Give any of: type, text, checked (to-do), language (code), icon (callout), depth. Without type the block keeps its kind. A snapshot is taken first.", json!({"page": page, "index": {"type": "integer"}, "expect": {"type": "string", "description": "Optional guard: the text the block starts with when you read it. The call is refused if the block moved or changed since."}, "type": {"type": "string"}, "text": {"type": "string"}, "checked": {"type": "boolean"}, "language": {"type": "string"}, "icon": {"type": "string"}, "depth": {"type": "integer"}}), &["page", "index"]),
        tool("move_block", "Move one block to another position (index of the block, `to` its new index).", json!({"page": page, "index": {"type": "integer"}, "to": {"type": "integer"}, "expect": {"type": "string", "description": "Optional guard: the text the block starts with when you read it. The call is refused if the block moved or changed since."}}), &["page", "index", "to"]),
        tool("delete_block", "Delete one block (a snapshot is taken first, so a person can restore it). Pass `expect` to refuse if the block is not the one you read.", json!({"page": page, "index": {"type": "integer"}, "expect": {"type": "string", "description": "Optional guard: the text the block starts with when you read it. The call is refused if the block moved or changed since."}}), &["page", "index"]),
        tool("create_view", "Add a view to a database. kind: table, board, list, gallery, calendar, chart, timeline, dashboard. Optional filter, match, sort and group_by as in query_rows; visible: list of property names to show; end_by: end date property of a timeline (group_by is its start).", json!({"database": db, "name": {"type": "string"}, "kind": {"type": "string"}, "filter": {}, "match": {"type": "string"}, "sort": {"type": "array", "items": {"type": "object"}}, "group_by": {"type": "string"}, "visible": {"type": "array", "items": {"type": "string"}}, "end_by": {"type": "string"}}), &["database", "name", "kind"]),
        tool("update_view", "Change a view (by name or id): name, kind, filter (replaces the rules), match, sort, group_by (empty clears), visible, end_by.", json!({"database": db, "view": {"type": "string"}, "name": {"type": "string"}, "kind": {"type": "string"}, "filter": {}, "match": {"type": "string"}, "sort": {"type": "array", "items": {"type": "object"}}, "group_by": {"type": "string"}, "visible": {"type": "array", "items": {"type": "string"}}, "end_by": {"type": "string"}}), &["database", "view"]),
        tool("delete_view", "Delete a view (a database keeps at least one).", json!({"database": db, "view": {"type": "string"}}), &["database", "view"]),
        tool("create_template", "Add a row template to a database, with default properties and a Markdown body.", json!({"database": db, "name": {"type": "string"}, "properties": props, "markdown": {"type": "string"}}), &["database", "name"]),
        tool("delete_template", "Delete a row template (by id or name).", json!({"database": db, "template": {"type": "string"}}), &["database", "template"]),
        tool(
            "lock_page",
            "Lock a page, database or row (and everything inside it): it can no longer be edited, moved or deleted. Only a person can unlock it, in the app.",
            json!({"page": page}),
            &["page"],
        ),
        tool("list_reminders", "List the pages that have a reminder, soonest first (page, id, local date and time).", json!({}), &[]),
        tool(
            "set_reminder",
            "Set or clear the reminder of a page. `when` is a local date and time (YYYY-MM-DD HH:MM; a bare date means 09:00); leave it empty to clear. The app shows a notification when it comes due.",
            json!({"page": page, "when": {"type": "string"}}),
            &["page"],
        ),
        tool("trash_page", "Move a page, row or database (with what is inside) to the trash. Restorable.", json!({"page": page}), &["page"]),
        tool("restore_page", "Restore something from the trash (id or title).", json!({"page": page}), &["page"]),
        tool("empty_trash", "Delete everything in the trash for good.", json!({"confirm": {"type": "boolean", "description": "Must be true"}}), &["confirm"]),
        // databases
        tool("list_databases", "Databases with properties (type, options), views and row counts.", json!({}), &[]),
        tool("get_database", "A database: schema (properties, options, views), templates and all rows with their properties.", json!({"database": db, "limit": {"type": "integer", "default": 200}}), &["database"]),
        tool(
            "create_database",
            "Create a database (title property 'Name', plus Status, Tags and Date by default).",
            json!({"title": {"type": "string"}, "parent": {"type": "string"}, "view": {"type": "string", "enum": ["table", "board", "list"]}, "properties": {"type": "array", "description": "Extra properties: [{name, type, options?: [names]}]", "items": {"type": "object"}}}),
            &["title"],
        ),
        tool("add_property", "Add a property. type: text, number, select, multi_select, status, date, checkbox, url, email, phone, created_time, edited_time, relation, formula, rollup, person, files, unique_id. A formula is an expression such as prop(\"Price\") * prop(\"Qty\"). A rollup summarises a property of related rows: give rollup_relation (a relation property of this database), rollup_property (property of the related database) and rollup_function (list, count, filled, unique, sum, average, min, max, checked).", json!({"database": db, "name": {"type": "string"}, "type": {"type": "string"}, "options": {"type": "array", "items": {"type": "string"}}, "relation_database": {"type": "string"}, "formula": {"type": "string"}, "rollup_relation": {"type": "string"}, "rollup_property": {"type": "string"}, "rollup_function": {"type": "string"}}), &["database", "name", "type"]),
        tool("rename_property", "Rename a property (row files follow).", json!({"database": db, "property": {"type": "string"}, "new_name": {"type": "string"}}), &["database", "property", "new_name"]),
        tool("delete_property", "Delete a property and its values.", json!({"database": db, "property": {"type": "string"}}), &["database", "property"]),
        tool("add_row", "Add a row with properties and an optional Markdown body.", json!({"database": db, "title": {"type": "string"}, "properties": props, "markdown": md}), &["database", "title"]),
        tool("update_row", "Change a row's title, properties, or append Markdown to its body.", json!({"database": db, "row": row, "title": {"type": "string"}, "properties": props, "markdown": md}), &["database", "row"]),
        tool("delete_row", "Move a row to the trash.", json!({"database": db, "row": row}), &["database", "row"]),
        tool(
            "query_rows",
            "Filter, sort and group rows. filter: {name: value} (equal/contains) or [{property, op, value}] with op is, is_not, contains, not_contains, is_empty, is_not_empty, greater, less, checked, unchecked; match: all|any; sort: [{property, descending}]; group_by: property; view: use a saved view.",
            json!({"database": db, "filter": {}, "match": {"type": "string", "enum": ["all", "any"]}, "sort": {"type": "array", "items": {"type": "object"}}, "group_by": {"type": "string"}, "view": {"type": "string"}, "limit": {"type": "integer", "default": 100}}),
            &["database"],
        ),
        tool("list_templates", "Row templates of a database, with their default properties and body.", json!({"database": db}), &["database"]),
        tool("add_row_from_template", "Add a row from a template (by template id or name).", json!({"database": db, "template": {"type": "string"}, "title": {"type": "string"}, "properties": props}), &["database", "template", "title"]),
        // files
        tool("add_attachment", "Copy a file into assets/ and optionally add it to a page as an image or file block.", json!({"path": {"type": "string", "description": "Absolute path of the file"}, "page": {"type": "string", "description": "Page or row to add it to (optional)"}, "caption": {"type": "string"}}), &["path"]),
        tool("import_markdown", "Import notes from a file, folder or zip: Markdown, HTML, Word (.docx), CSV, Evernote (.enex), and exports from Obsidian, Bear, Joplin and other workspace apps (sub-folders become sub-pages; images and files are copied).", json!({"path": {"type": "string"}, "parent": {"type": "string"}}), &["path"]),
        tool("export_markdown", "Export a page (with sub-pages), a database (+CSV) or the whole workspace to plain Markdown in a folder.", json!({"destination": {"type": "string", "description": "Folder to write to"}, "page": {"type": "string", "description": "Page, row or database; omit for the whole workspace"}}), &["destination"]),
    ];
    all.into_iter().filter(|t| policy.offers(t["name"].as_str().unwrap_or(""))).collect()
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn need<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    str_arg(args, key).ok_or_else(|| ToolError::Failed(format!("missing argument {key}")))
}

fn limit(args: &Value, default: u64) -> usize {
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
fn find_doc(ws: &Workspace, key: &str) -> Result<DocId, ToolError> {
    match find_any(ws, key)? {
        DocId::Database(_) => fail(format!("{key} is a database; use get_database or query_rows")),
        d => Ok(d),
    }
}

fn find_page_id(ws: &Workspace, key: &str) -> Result<String, ToolError> {
    match find_any(ws, key)? {
        DocId::Page(id) => Ok(id),
        _ => fail(format!("{key} is not a page")),
    }
}

fn find_db(ws: &Workspace, key: &str) -> Result<String, ToolError> {
    if ws.dbs.contains_key(key) {
        return Ok(key.to_string());
    }
    ws.dbs
        .iter()
        .find(|(_, d)| d.schema.title.eq_ignore_ascii_case(key) && d.schema.trashed.is_none())
        .map(|(id, _)| id.clone())
        .ok_or_else(|| ToolError::Failed(format!("database not found: {key}")))
}

fn find_row(ws: &Workspace, db: &str, key: &str) -> Result<String, ToolError> {
    let d = &ws.dbs[db];
    if d.rows.contains_key(key) {
        return Ok(key.to_string());
    }
    d.live_rows()
        .find(|r| r.meta.title.eq_ignore_ascii_case(key))
        .map(|r| r.meta.id.clone())
        .ok_or_else(|| ToolError::Failed(format!("row not found: {key}")))
}

fn find_prop(ws: &Workspace, db: &str, key: &str) -> Result<Property, ToolError> {
    let s = &ws.dbs[db].schema;
    s.prop(key).or_else(|| s.properties.iter().find(|p| p.name.eq_ignore_ascii_case(key))).cloned().ok_or_else(|| {
        ToolError::Failed(format!(
            "unknown property {key}; properties are {}",
            s.properties.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
        ))
    })
}

fn parse_kind(s: &str) -> Result<PropKind, ToolError> {
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
fn to_prop_value(ws: &Workspace, db: &str, p: &Property, v: &Value) -> Result<PropValue, ToolError> {
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
fn set_properties(
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
fn apply_view_spec(ws: &Workspace, db: &str, view: &mut View, args: &Value) -> Result<(), ToolError> {
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

fn block_kind_name(b: &Block) -> &'static str {
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

fn block_index(args: &Value, key: &str, len: usize, inclusive: bool) -> Result<usize, ToolError> {
    let i = args.get(key).and_then(Value::as_u64).ok_or_else(|| ToolError::Failed(format!("{key} must be a number")))?
        as usize;
    if i > len || (i == len && !inclusive) {
        return fail(format!("{key} {i} is out of range (the page has {len} blocks)"));
    }
    Ok(i)
}

/// Refuse a block edit when the block no longer starts with the text the caller read.
fn check_expect(args: &Value, block: &Block) -> Result<(), ToolError> {
    if let Some(e) = args.get("expect").and_then(Value::as_str) {
        if !block.text.trim_start().starts_with(e.trim()) {
            return fail("the block changed since you read it; call get_blocks again");
        }
    }
    Ok(())
}

fn find_view_index(ws: &Workspace, db: &str, key: &str) -> Result<usize, ToolError> {
    ws.dbs[db]
        .schema
        .views
        .iter()
        .position(|v| v.id == key || v.name.eq_ignore_ascii_case(key))
        .ok_or_else(|| ToolError::Failed(format!("view not found: {key}")))
}

fn apply_view_extras(ws: &Workspace, db: &str, view: &mut View, args: &Value) -> Result<(), ToolError> {
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

fn row_json(ws: &Workspace, db: &str, rid: &str) -> Value {
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

fn schema_json(ws: &Workspace, db: &str) -> Value {
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

fn tree_text(ws: &Workspace, parent: Option<&str>, depth: usize, out: &mut String) {
    for item in ws.children(parent) {
        let kind = if matches!(item.doc, DocId::Database(_)) { " [database]" } else { "" };
        let title = if item.title.is_empty() { "Untitled" } else { &item.title };
        let icon = if item.icon.is_empty() { String::new() } else { format!("{} ", item.icon) };
        out.push_str(&format!("{}- {icon}{title}{kind} (id {})\n", "  ".repeat(depth), item.doc.id()));
        tree_text(ws, Some(item.doc.id()), depth + 1, out);
    }
}

fn doc_line(ws: &Workspace, d: &DocId) -> String {
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

fn block_from_json(v: &Value) -> Result<Block, ToolError> {
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

fn body_mut<'a>(ws: &'a mut Workspace, doc: &DocId) -> Result<&'a mut Vec<Block>, ToolError> {
    ws.blocks_mut_checked(doc).map_err(ToolError::from)
}

fn save_body(ws: &mut Workspace, doc: &DocId) -> Result<(), ToolError> {
    ws.touch(doc);
    ws.save(doc)?;
    Ok(())
}

/// Refuse what is outside the scope the server is limited to. Tools that act on the whole workspace
/// or on the top level are refused when a scope is set, unless they name a place inside it.
fn guard_scope(ws: &Workspace, name: &str, args: &Value, scope: &Scope) -> Result<(), ToolError> {
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
fn guard_locks(ws: &Workspace, name: &str, args: &Value) -> Result<(), ToolError> {
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

/// Run a tool.
/// Runs one tool. A bug in a tool must never take the whole server down, so a panic
/// becomes an error answer and the workspace stays usable.
pub fn call(ws: &mut Workspace, name: &str, args: &Value, policy: &Policy, scope: &Scope) -> R {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call_inner(ws, name, args, policy, scope)))
        .unwrap_or_else(|_| Err(ToolError::Failed("internal error while running this tool".into())))
}

fn call_inner(ws: &mut Workspace, name: &str, args: &Value, policy: &Policy, scope: &Scope) -> R {
    policy.check(name)?;
    guard_scope(ws, name, args, scope)?;
    guard_locks(ws, name, args)?;
    match name {
        // ------------------------------------------------------------ pages
        "list_pages" => {
            let mut out = String::new();
            if scope.is_active() {
                for root in scope.roots() {
                    let icon = ws.icon(root);
                    let icon = if icon.is_empty() { String::new() } else { format!("{icon} ") };
                    let kind = if matches!(root, DocId::Database(_)) { " [database]" } else { "" };
                    out.push_str(&format!("- {icon}{}{kind} (id {})\n", ws.title(root), root.id()));
                    tree_text(ws, Some(root.id()), 1, &mut out);
                }
            } else {
                tree_text(ws, None, 0, &mut out);
            }
            if out.is_empty() {
                return Ok("The workspace is empty.".into());
            }
            // pages of 200 entries with a cursor
            let lines: Vec<&str> = out.lines().collect();
            let start = str_arg(args, "cursor").and_then(|c| c.parse::<usize>().ok()).unwrap_or(0).min(lines.len());
            let end = (start + PAGE_SIZE).min(lines.len());
            let mut page = lines[start..end].join("\n");
            if end < lines.len() {
                page.push_str(&format!("\nnext_cursor: {end}"));
            }
            Ok(page)
        }
        "get_page" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            let mut out = format!("# {} {}\n\n", ws.icon(&doc), ws.title(&doc));
            let (created, updated, fav, parent) = match &doc {
                DocId::Page(id) => {
                    let p = &ws.pages[id];
                    (p.meta.created.clone(), p.meta.updated.clone(), p.meta.favorite, p.meta.parent.clone())
                }
                DocId::Row(db, r) => {
                    let row = &ws.dbs[db].rows[r];
                    (row.meta.created.clone(), row.meta.updated.clone(), false, Some(db.clone()))
                }
                DocId::Database(_) => unreachable!(),
            };
            out.push_str(&format!("id: {}\ncreated: {created}\nupdated: {updated}\n", doc.id()));
            if ws.is_locked(&doc) {
                out.push_str("locked: yes (read-only; only a person can unlock it in the app)\n");
            }
            if let Some(p) = parent {
                out.push_str(&format!("parent: {p} ({})\n", ws.path_label(&doc)));
            }
            if fav {
                out.push_str("favorite: true\n");
            }
            if ws.is_trashed(&doc) {
                out.push_str("in trash: true\n");
            }
            if let DocId::Row(db, rid) = &doc {
                let d = &ws.dbs[db];
                let r = &d.rows[rid];
                for p in d.schema.properties.iter().filter(|p| p.kind != PropKind::Title) {
                    out.push_str(&format!("{}: {}\n", p.name, ws.display_value(db, r, p)));
                }
            }
            let links: Vec<String> = ws.backlinks(&doc).iter().map(|d| ws.title(d)).collect();
            if !links.is_empty() {
                out.push_str(&format!("Backlinks: {}\n", links.join(", ")));
            }
            let kids: Vec<String> = ws.children(Some(doc.id())).iter().map(|t| t.title.clone()).collect();
            if !kids.is_empty() {
                out.push_str(&format!("Sub-pages: {}\n", kids.join(", ")));
            }
            let md = blocks::to_markdown(ws.blocks(&doc).map(|b| b.as_slice()).unwrap_or(&[]));
            // very long pages can be read in slices: offset and max_chars count characters of the body
            let total = md.chars().count();
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let max = args.get("max_chars").and_then(Value::as_u64).map(|m| (m as usize).max(1));
            let (slice, more) = match max {
                Some(m) if offset > 0 || total > m => {
                    let part: String = md.chars().skip(offset).take(m).collect();
                    let left = total.saturating_sub(offset + part.chars().count());
                    (part, left)
                }
                _ => (md, 0),
            };
            if offset > 0 || more > 0 {
                out.push_str(&format!("body: characters {offset}..{} of {total}\n", offset + slice.chars().count()));
            }
            out.push_str("\n<content>\n");
            out.push_str(&slice);
            out.push_str("\n</content>");
            if more > 0 {
                out.push_str(&format!(
                    "\n[{more} more characters: call get_page again with offset={}. Never pass a partial body to update_page with mode replace.]",
                    offset + slice.chars().count()
                ));
            }
            Ok(out)
        }
        "search" => {
            let q = need(args, "query")?;
            let want = limit(args, 20);
            let mut hits = ws.search(q, if scope.is_active() { 1000 } else { want });
            hits.retain(|h| scope.contains(ws, &h.doc));
            hits.truncate(want);
            if hits.is_empty() {
                return Ok("No results.".into());
            }
            Ok(hits
                .iter()
                .map(|h| {
                    let snippet = h.snippet.replace(['\u{1}', '\u{2}'], "**").replace('\n', " ");
                    format!(
                        "{}{}",
                        doc_line(ws, &h.doc),
                        if snippet.is_empty() { String::new() } else { format!(": {snippet}") }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "backlinks" => {
            let doc = find_any(ws, need(args, "page")?)?;
            let mut links = ws.backlinks(&doc);
            links.retain(|d| scope.contains(ws, d));
            Ok(if links.is_empty() {
                "No backlinks.".into()
            } else {
                links.iter().map(|d| doc_line(ws, d)).collect::<Vec<_>>().join("\n")
            })
        }
        "recent_pages" => {
            let want = limit(args, 20);
            let mut r = ws.search("", if scope.is_active() { 1000 } else { want });
            r.retain(|h| scope.contains(ws, &h.doc));
            r.truncate(want);
            Ok(r.iter().map(|h| doc_line(ws, &h.doc)).collect::<Vec<_>>().join("\n"))
        }
        "favorites" => {
            let mut f = ws.favorites();
            f.retain(|t| scope.contains(ws, &t.doc));
            Ok(if f.is_empty() {
                "No favourites.".into()
            } else {
                f.iter().map(|t| doc_line(ws, &t.doc)).collect::<Vec<_>>().join("\n")
            })
        }
        "create_page" => {
            let title = need(args, "title")?;
            let parent = match str_arg(args, "parent") {
                Some(p) => Some(find_page_id(ws, p)?),
                None => None,
            };
            let id = ws.create_page(parent.as_deref(), title)?;
            let page = ws.pages.get_mut(&id).unwrap();
            let mut notes = Vec::new();
            if let Some(md) = str_arg(args, "markdown") {
                let (b, n) = notecraft_core::import_rich_md::blocks_from_markdown(md);
                page.blocks = b;
                notes = n;
            }
            if let Some(icon) = str_arg(args, "icon") {
                page.meta.icon = icon.to_string();
            }
            ws.save_page(&id)?;
            Ok(format!("Created page \"{title}\" (id {id}).{}", notes_suffix(&notes)))
        }
        "replace_text" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            let old = need(args, "old")?;
            let new = args.get("new").and_then(Value::as_str).ok_or_else(|| {
                ToolError::Failed("new is required (use an empty string to delete the passage)".into())
            })?;
            let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
            let md = blocks::to_markdown(ws.blocks(&doc).map(|b| b.as_slice()).unwrap_or(&[]));
            let count = md.matches(old).count();
            if count == 0 {
                return fail("old was not found in the page; copy it exactly from get_page (inside <content>)");
            }
            if count > 1 && !all {
                return fail(format!("old occurs {count} times; make it longer so it is unique, or pass all=true"));
            }
            let changed = if all { md.replace(old, new) } else { md.replacen(old, new, 1) };
            ws.snapshot_docs(std::slice::from_ref(&doc), "replace_text")?;
            *body_mut(ws, &doc)? = blocks::from_markdown(&changed);
            save_body(ws, &doc)?;
            Ok(format!("Replaced {} occurrence(s) in \"{}\".", if all { count } else { 1 }, ws.title(&doc)))
        }
        "update_page" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            let (new, notes) = notecraft_core::import_rich_md::blocks_from_markdown(need(args, "markdown")?);
            let replace = str_arg(args, "mode") == Some("replace");
            if replace {
                ws.snapshot_docs(std::slice::from_ref(&doc), "update_page (replace)")?;
            }
            let b = body_mut(ws, &doc)?;
            if replace {
                *b = new;
            } else {
                b.extend(new);
            }
            save_body(ws, &doc)?;
            Ok(format!("Updated \"{}\".{}", ws.title(&doc), notes_suffix(&notes)))
        }
        "append_blocks" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            let list = args
                .get("blocks")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Failed("blocks must be a list".into()))?;
            let new: Vec<Block> = list.iter().map(block_from_json).collect::<Result<_, _>>()?;
            let n = new.len();
            body_mut(ws, &doc)?.extend(new);
            save_body(ws, &doc)?;
            Ok(format!("Appended {n} block(s) to \"{}\".", ws.title(&doc)))
        }
        "get_blocks" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            let body = ws.blocks(&doc).ok_or_else(|| ToolError::Failed("no such page".into()))?;
            let list: Vec<Value> = body
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let mut v = json!({"index": i, "type": block_kind_name(b), "text": b.text, "depth": b.depth});
                    match &b.kind {
                        BlockKind::Todo { checked } => v["checked"] = json!(checked),
                        BlockKind::Code { lang } => v["language"] = json!(lang),
                        BlockKind::Callout { icon } => v["icon"] = json!(icon),
                        _ => {}
                    }
                    v
                })
                .collect();
            Ok(serde_json::to_string_pretty(&list).unwrap_or_default())
        }
        "insert_blocks" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            let list = args
                .get("blocks")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Failed("blocks must be a list".into()))?;
            let new: Vec<Block> = list.iter().map(block_from_json).collect::<Result<_, _>>()?;
            let n = new.len();
            let body = body_mut(ws, &doc)?;
            let at = block_index(args, "index", body.len(), true)?;
            body.splice(at..at, new);
            save_body(ws, &doc)?;
            Ok(format!("Inserted {n} block(s) at {at} in \"{}\".", ws.title(&doc)))
        }
        "update_block" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            ws.snapshot_docs(std::slice::from_ref(&doc), "update_block")?;
            let body = body_mut(ws, &doc)?;
            let i = block_index(args, "index", body.len(), false)?;
            check_expect(args, &body[i])?;
            let old = body[i].clone();
            if block_kind_name(&old) == "other" {
                return fail("this kind of block cannot be edited with this tool");
            }
            let mut new = if args.get("type").is_some() {
                let mut v = args.clone();
                if v.get("text").is_none() {
                    v["text"] = json!(old.text);
                }
                if v.get("depth").is_none() {
                    v["depth"] = json!(old.depth);
                }
                block_from_json(&v)?
            } else {
                let mut b = old.clone();
                if let Some(t) = str_arg(args, "text") {
                    b.text = t.to_string();
                }
                if let Some(d) = args.get("depth").and_then(Value::as_u64) {
                    b.depth = d.min(blocks::MAX_DEPTH as u64) as u8;
                }
                match &mut b.kind {
                    BlockKind::Todo { checked } => {
                        if let Some(c) = args.get("checked").and_then(Value::as_bool) {
                            *checked = c;
                        }
                    }
                    BlockKind::Code { lang } => {
                        if let Some(l) = str_arg(args, "language") {
                            *lang = l.to_string();
                        }
                    }
                    BlockKind::Callout { icon } => {
                        if let Some(c) = str_arg(args, "icon") {
                            *icon = c.to_string();
                        }
                    }
                    _ => {}
                }
                b
            };
            new.id = old.id;
            new.color = old.color;
            body[i] = new;
            save_body(ws, &doc)?;
            Ok(format!("Updated block {i} of \"{}\".", ws.title(&doc)))
        }
        "move_block" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            ws.snapshot_docs(std::slice::from_ref(&doc), "move_block")?;
            let body = body_mut(ws, &doc)?;
            let from = block_index(args, "index", body.len(), false)?;
            let to = block_index(args, "to", body.len(), false)?;
            check_expect(args, &body[from])?;
            let b = body.remove(from);
            body.insert(to, b);
            save_body(ws, &doc)?;
            Ok(format!("Moved block {from} to {to}."))
        }
        "delete_block" => {
            let doc = find_doc(ws, need(args, "page")?)?;
            ws.snapshot_docs(std::slice::from_ref(&doc), "delete_block")?;
            let body = body_mut(ws, &doc)?;
            let i = block_index(args, "index", body.len(), false)?;
            check_expect(args, &body[i])?;
            body.remove(i);
            save_body(ws, &doc)?;
            Ok(format!("Deleted block {i} of \"{}\".", ws.title(&doc)))
        }
        "create_view" => {
            let db = find_db(ws, need(args, "database")?)?;
            let kind_name = need(args, "kind")?;
            let kind: ViewKind = serde_json::from_value(json!(kind_name.to_lowercase()))
                .map_err(|_| ToolError::Failed(format!("unknown view kind {kind_name}")))?;
            let mut view = View::new(need(args, "name")?, kind);
            apply_view_spec(ws, &db, &mut view, args)?;
            apply_view_extras(ws, &db, &mut view, args)?;
            let id = view.id.clone();
            ws.dbs.get_mut(&db).unwrap().schema.views.push(view);
            ws.save_schema(&db)?;
            Ok(format!("Added the view \"{}\" (id {id}).", need(args, "name")?))
        }
        "update_view" => {
            let db = find_db(ws, need(args, "database")?)?;
            let at = find_view_index(ws, &db, need(args, "view")?)?;
            let mut view = ws.dbs[&db].schema.views[at].clone();
            if let Some(n) = str_arg(args, "name") {
                view.name = n.to_string();
            }
            if let Some(k) = str_arg(args, "kind") {
                view.kind = serde_json::from_value(json!(k.to_lowercase()))
                    .map_err(|_| ToolError::Failed(format!("unknown view kind {k}")))?;
            }
            if args.get("filter").is_some() {
                view.filter.rules.clear();
            }
            let clear_group = matches!(args.get("group_by"), Some(Value::String(s)) if s.is_empty());
            if !clear_group {
                apply_view_spec(ws, &db, &mut view, args)?;
            } else {
                view.group_by = None;
            }
            apply_view_extras(ws, &db, &mut view, args)?;
            ws.dbs.get_mut(&db).unwrap().schema.views[at] = view;
            ws.save_schema(&db)?;
            Ok("Updated the view.".into())
        }
        "delete_view" => {
            let db = find_db(ws, need(args, "database")?)?;
            let at = find_view_index(ws, &db, need(args, "view")?)?;
            if ws.dbs[&db].schema.views.len() <= 1 {
                return fail("a database keeps at least one view");
            }
            let removed = ws.dbs.get_mut(&db).unwrap().schema.views.remove(at);
            ws.save_schema(&db)?;
            Ok(format!("Deleted the view \"{}\".", removed.name))
        }
        "create_template" => {
            let db = find_db(ws, need(args, "database")?)?;
            let tid = ws.add_template(&db, need(args, "name")?)?;
            if let Some(Value::Object(props)) = args.get("properties") {
                set_properties(ws, &db, &tid, props, true)?;
            }
            if let Some(md) = str_arg(args, "markdown") {
                ws.dbs.get_mut(&db).unwrap().templates.get_mut(&tid).unwrap().blocks = blocks::from_markdown(md);
            }
            ws.save_template(&db, &tid)?;
            Ok(format!("Added the template \"{}\" (id {tid}).", need(args, "name")?))
        }
        "delete_template" => {
            let db = find_db(ws, need(args, "database")?)?;
            let key = need(args, "template")?;
            let tid = ws.dbs[&db]
                .templates
                .iter()
                .find(|(id, t)| id.as_str() == key || t.meta.title.eq_ignore_ascii_case(key))
                .map(|(id, _)| id.clone())
                .ok_or_else(|| ToolError::Failed(format!("template not found: {key}")))?;
            ws.delete_template(&db, &tid)?;
            Ok("Deleted the template.".into())
        }
        "lock_page" => {
            let doc = find_any(ws, need(args, "page")?)?;
            ws.set_locked(&doc, true)?;
            Ok(format!("Locked \"{}\". It can only be unlocked from the app.", ws.title(&doc)))
        }
        "rename_page" => {
            let doc = find_any(ws, need(args, "page")?)?;
            let title = need(args, "title")?;
            ws.rename(&doc, title)?;
            Ok(format!("Renamed to \"{title}\"."))
        }
        "move_page" => {
            let doc = find_any(ws, need(args, "page")?)?;
            if matches!(doc, DocId::Row(..)) {
                return fail("rows stay in their database");
            }
            let parent = match str_arg(args, "parent") {
                Some(p) if !p.is_empty() => Some(find_page_id(ws, p)?),
                _ => None,
            };
            let count = ws.children(parent.as_deref()).len();
            let pos = args.get("position").and_then(Value::as_u64).map(|p| p as usize).unwrap_or(count);
            ws.move_to(&doc, parent.as_deref(), pos)?;
            Ok(format!(
                "Moved \"{}\" to {}.",
                ws.title(&doc),
                parent.map(|p| ws.title(&DocId::Page(p))).unwrap_or_else(|| "the top level".into())
            ))
        }
        "duplicate_page" => {
            let id = find_page_id(ws, need(args, "page")?)?;
            let n = ws.duplicate_page(&id)?;
            Ok(format!("Duplicated as id {n}."))
        }
        "set_icon" => {
            let doc = find_any(ws, need(args, "page")?)?;
            ws.set_icon(&doc, need(args, "icon")?)?;
            Ok("Icon set.".into())
        }
        "set_cover" => {
            let doc = find_any(ws, need(args, "page")?)?;
            let cover = need(args, "cover")?.trim();
            // a cover is a gradient or an image already inside the workspace's assets folder
            if !cover.is_empty() && !cover.starts_with("gradient:") {
                let ok = cover.starts_with("assets/")
                    && !cover.split('/').any(|p| p == ".." || p.is_empty())
                    && ws.root.join(cover).is_file();
                if !ok {
                    return fail(format!(
                        "cover must be gradient:0..7 or the path of an existing image under assets/ (got \"{cover}\"); add the file with add_attachment first"
                    ));
                }
            }
            ws.set_cover(&doc, cover)?;
            Ok("Cover set.".into())
        }
        "set_favorite" => {
            let doc = find_any(ws, need(args, "page")?)?;
            let fav = args.get("favorite").and_then(Value::as_bool).unwrap_or(true);
            ws.set_favorite(&doc, fav)?;
            Ok(if fav { "Added to favourites.".into() } else { "Removed from favourites.".into() })
        }
        "list_reminders" => {
            let list: Vec<Value> = ws
                .reminders()
                .into_iter()
                .filter(|(id, _, _)| !scope.is_active() || scope.contains(ws, &DocId::Page(id.clone())))
                .map(|(id, title, when)| json!({"page": title, "id": id, "when": when}))
                .collect();
            Ok(serde_json::to_string_pretty(&list).unwrap_or_default())
        }
        "set_reminder" => {
            let id = find_page_id(ws, need(args, "page")?)?;
            let when = args.get("when").and_then(Value::as_str).unwrap_or("").trim().to_string();
            ws.set_reminder(&id, &when).map_err(|e| ToolError::Failed(e.to_string()))?;
            let title = ws.pages.get(&id).map(|p| p.meta.title.clone()).unwrap_or_default();
            Ok(if when.is_empty() {
                format!("Cleared the reminder of \"{title}\".")
            } else {
                let at = ws.pages.get(&id).map(|p| p.meta.reminder.clone()).unwrap_or_default();
                format!("Reminder for \"{title}\" set to {at}.")
            })
        }
        "trash_page" => {
            let doc = find_any(ws, need(args, "page")?)?;
            let title = ws.title(&doc);
            ws.trash_doc(&doc)?;
            Ok(format!("Moved \"{title}\" to the trash."))
        }
        "restore_page" => {
            let key = need(args, "page")?;
            let doc = ws
                .trash()
                .into_iter()
                .map(|(d, _)| d)
                .find(|d| d.id() == key || ws.title(d).eq_ignore_ascii_case(key))
                .ok_or_else(|| ToolError::Failed(format!("not in the trash: {key}")))?;
            ws.restore(&doc)?;
            Ok(format!("Restored \"{}\".", ws.title(&doc)))
        }
        "empty_trash" => {
            if args.get("confirm").and_then(Value::as_bool) != Some(true) {
                return fail("set confirm: true to delete the trash for good");
            }
            let n = ws.trash().len();
            ws.empty_trash()?;
            Ok(format!("Deleted {n} item(s) for good."))
        }
        // --------------------------------------------------------- databases
        "list_databases" => {
            let mut out = Vec::new();
            for id in ws.dbs.keys() {
                if ws.is_live(&DocId::Database(id.clone())) && scope.contains(ws, &DocId::Database(id.clone())) {
                    let mut v = schema_json(ws, id);
                    v["rows"] = json!(ws.dbs[id].live_rows().count());
                    out.push(v);
                }
            }
            Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
        }
        "get_database" => {
            let db = find_db(ws, need(args, "database")?)?;
            let mut v = schema_json(ws, &db);
            let view = View::new("all", ViewKind::Table);
            let all = database::sorted_rows(&ws.dbs[&db], &view);
            let cap = limit(args, 200);
            v["total_rows"] = json!(all.len());
            v["truncated"] = json!(all.len() > cap);
            v["rows"] = json!(all.iter().take(cap).map(|r| row_json(ws, &db, r)).collect::<Vec<_>>());
            v["templates"] = json!(ws.dbs[&db]
                .templates
                .values()
                .map(|t| json!({"id": t.meta.id, "name": t.meta.title}))
                .collect::<Vec<_>>());
            Ok(serde_json::to_string_pretty(&v).unwrap_or_default())
        }
        "create_database" => {
            let title = need(args, "title")?;
            let parent = match str_arg(args, "parent") {
                Some(p) => Some(find_page_id(ws, p)?),
                None => None,
            };
            let id = ws.create_database(parent.as_deref(), title)?;
            if let Some(kind) = str_arg(args, "view") {
                let k: ViewKind = serde_json::from_value(json!(kind))
                    .map_err(|_| ToolError::Failed(format!("unknown view {kind}")))?;
                if let Some(v) = ws.dbs.get_mut(&id).unwrap().schema.views.first_mut() {
                    v.kind = k;
                    v.name = kind[..1].to_uppercase() + &kind[1..];
                }
                ws.save_schema(&id)?;
            }
            if let Some(Value::Array(props)) = args.get("properties") {
                for p in props {
                    let name = p
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| ToolError::Failed("each property needs a name".into()))?;
                    let kind = parse_kind(p.get("type").and_then(Value::as_str).unwrap_or("text"))?;
                    let pid = ws.add_property(&id, name, kind)?;
                    add_options(ws, &id, &pid, p.get("options"))?;
                }
            }
            Ok(format!("Created database \"{title}\" (id {id})."))
        }
        "add_property" => {
            let db = find_db(ws, need(args, "database")?)?;
            let kind = parse_kind(need(args, "type")?)?;
            let pid = ws.add_property(&db, need(args, "name")?, kind)?;
            add_options(ws, &db, &pid, args.get("options"))?;
            if let Some(rel) = str_arg(args, "relation_database") {
                let target = find_db(ws, rel)?;
                if let Some(p) = ws.dbs.get_mut(&db).unwrap().schema.properties.iter_mut().find(|p| p.id == pid) {
                    p.relation_db = Some(target);
                }
                ws.save_schema(&db)?;
            }
            if let Some(f) = str_arg(args, "formula") {
                notecraft_core::formula::check(f).map_err(|e| ToolError::Failed(format!("formula: {e}")))?;
                if let Some(p) = ws.dbs.get_mut(&db).unwrap().schema.properties.iter_mut().find(|p| p.id == pid) {
                    p.formula = f.to_string();
                }
                ws.save_schema(&db)?;
            }
            if let Some(rel) = str_arg(args, "rollup_relation") {
                let rel = find_prop(ws, &db, rel)?;
                let other = rel.relation_db.clone().unwrap_or_else(|| db.clone());
                let target = match str_arg(args, "rollup_property") {
                    Some(n) => find_prop(ws, &other, n)?.id,
                    None => String::new(),
                };
                let func = str_arg(args, "rollup_function").unwrap_or("count");
                let func = notecraft_core::formula::RollupFunc::from_key(&func.to_lowercase())
                    .ok_or_else(|| ToolError::Failed(format!("unknown rollup_function {func}")))?;
                if let Some(p) = ws.dbs.get_mut(&db).unwrap().schema.properties.iter_mut().find(|p| p.id == pid) {
                    p.rollup = Some(notecraft_core::formula::Rollup { relation: rel.id, target, func });
                }
                ws.save_schema(&db)?;
            }
            Ok(format!("Added property (id {pid})."))
        }
        "rename_property" => {
            let db = find_db(ws, need(args, "database")?)?;
            let p = find_prop(ws, &db, need(args, "property")?)?;
            ws.rename_property(&db, &p.id, need(args, "new_name")?)?;
            Ok("Property renamed.".into())
        }
        "delete_property" => {
            let db = find_db(ws, need(args, "database")?)?;
            let p = find_prop(ws, &db, need(args, "property")?)?;
            ws.delete_property(&db, &p.id)?;
            Ok(format!("Deleted property {}.", p.name))
        }
        "add_row" => {
            let db = find_db(ws, need(args, "database")?)?;
            let title = need(args, "title")?;
            let rid = ws.add_row(&db, None, title)?;
            finish_row(ws, &db, &rid, args)?;
            Ok(format!("Added row \"{title}\" (id {rid})."))
        }
        "add_row_from_template" => {
            let db = find_db(ws, need(args, "database")?)?;
            let key = need(args, "template")?;
            let tpl = ws.dbs[&db]
                .templates
                .iter()
                .find(|(id, t)| id.as_str() == key || t.meta.title.eq_ignore_ascii_case(key))
                .map(|(id, _)| id.clone())
                .ok_or_else(|| ToolError::Failed(format!("template not found: {key}")))?;
            let title = need(args, "title")?;
            let rid = ws.add_row(&db, Some(&tpl), title)?;
            ws.dbs.get_mut(&db).unwrap().rows.get_mut(&rid).unwrap().meta.title = title.to_string();
            finish_row(ws, &db, &rid, args)?;
            Ok(format!("Added row \"{title}\" from template (id {rid})."))
        }
        "update_row" => {
            let db = find_db(ws, need(args, "database")?)?;
            let rid = find_row(ws, &db, need(args, "row")?)?;
            ws.snapshot_docs(&[DocId::Row(db.clone(), rid.clone())], "update_row")?;
            if let Some(Value::Object(props)) = args.get("properties") {
                set_properties(ws, &db, &rid, props, false)?;
            }
            if let Some(md) = str_arg(args, "markdown") {
                ws.dbs.get_mut(&db).unwrap().rows.get_mut(&rid).unwrap().blocks.extend(blocks::from_markdown(md));
            }
            if let Some(t) = str_arg(args, "title") {
                ws.dbs.get_mut(&db).unwrap().rows.get_mut(&rid).unwrap().meta.title = t.to_string();
            }
            ws.touch(&DocId::Row(db.clone(), rid.clone()));
            ws.save_row(&db, &rid)?;
            Ok(format!("Updated row {rid}."))
        }
        "delete_row" => {
            let db = find_db(ws, need(args, "database")?)?;
            let rid = find_row(ws, &db, need(args, "row")?)?;
            ws.trash_doc(&DocId::Row(db, rid.clone()))?;
            Ok(format!("Moved row {rid} to the trash."))
        }
        "query_rows" => {
            let db = find_db(ws, need(args, "database")?)?;
            let schema = ws.dbs[&db].schema.clone();
            let mut view = match str_arg(args, "view") {
                Some(v) => schema
                    .views
                    .iter()
                    .find(|x| x.name.eq_ignore_ascii_case(v) || x.id == v)
                    .cloned()
                    .ok_or_else(|| ToolError::Failed(format!("view not found: {v}")))?,
                None => View::new("query", ViewKind::Table),
            };
            if str_arg(args, "view").is_some() && view.kind == ViewKind::Board && args.get("group_by").is_none() {
                // a board view keeps its grouping
            } else if str_arg(args, "view").is_none() {
                view.group_by = None;
            }
            apply_view_spec(ws, &db, &mut view, args)?;
            let max = limit(args, 100);
            let d = &ws.dbs[&db];
            if view.group_by.is_some() {
                let groups = database::query(d, &view);
                let out: Vec<Value> = groups
                    .iter()
                    .filter(|g| !g.rows.is_empty())
                    .map(|g| json!({"group": g.key, "rows": g.rows.iter().take(max).map(|r| row_json(ws, &db, r)).collect::<Vec<_>>()}))
                    .collect();
                Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
            } else {
                let rows: Vec<Value> =
                    database::sorted_rows(d, &view).iter().take(max).map(|r| row_json(ws, &db, r)).collect();
                Ok(serde_json::to_string_pretty(&rows).unwrap_or_default())
            }
        }
        "list_templates" => {
            let db = find_db(ws, need(args, "database")?)?;
            let d = &ws.dbs[&db];
            let out: Vec<Value> = d
                .templates
                .values()
                .map(|t| {
                    let mut props = Map::new();
                    for p in d.schema.properties.iter().filter(|p| p.kind != PropKind::Title) {
                        let v = t.value(p);
                        if !v.is_empty() {
                            props.insert(p.name.clone(), Value::String(v.display()));
                        }
                    }
                    json!({"id": t.meta.id, "name": t.meta.title, "properties": props, "markdown": blocks::to_markdown(&t.blocks)})
                })
                .collect();
            Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
        }
        // ------------------------------------------------------------- files
        "add_attachment" => {
            let path = policy.check_local_path(&PathBuf::from(need(args, "path")?), &ws.root)?;
            if !path.is_file() {
                return fail(format!("no such file: {}", path.display()));
            }
            let rel = ws.add_asset(&path)?;
            if let Some(p) = str_arg(args, "page") {
                let doc = find_doc(ws, p)?;
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                let caption = str_arg(args, "caption").unwrap_or("").to_string();
                let block = if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") {
                    Block::new(BlockKind::Image { path: rel.clone() }, caption)
                } else {
                    let name = if caption.is_empty() {
                        path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                    } else {
                        caption
                    };
                    Block::new(BlockKind::File { path: rel.clone() }, name)
                };
                body_mut(ws, &doc)?.push(block);
                save_body(ws, &doc)?;
            }
            Ok(format!("Copied to {rel}."))
        }
        "import_markdown" => {
            let path = policy.check_local_path(&PathBuf::from(need(args, "path")?), &ws.root)?;
            let parent = match str_arg(args, "parent") {
                Some(p) => Some(find_page_id(ws, p)?),
                None => None,
            };
            let (source, r) = notecraft_core::import_any::import_any(ws, &path, parent.as_deref())?;
            Ok(format!(
                "Imported {}{}.",
                notecraft_core::import_any::summary(source, &r),
                if r.skipped.is_empty() { String::new() } else { format!("; skipped: {}", r.skipped.join("; ")) }
            ))
        }
        "export_markdown" => {
            let dest = policy.check_local_path(&PathBuf::from(need(args, "destination")?), &ws.root)?;
            match str_arg(args, "page") {
                Some(p) => {
                    let doc = find_any(ws, p)?;
                    let main = import_export::export_doc(ws, &doc, &dest, true)?;
                    Ok(format!("Exported to {}.", main.display()))
                }
                None => {
                    let n = import_export::export_workspace(ws, &dest)?;
                    Ok(format!("Exported {n} file(s) to {}.", dest.display()))
                }
            }
        }
        _ => Err(ToolError::Unknown),
    }
}

fn add_options(ws: &mut Workspace, db: &str, pid: &str, options: Option<&Value>) -> Result<(), ToolError> {
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
fn finish_row(ws: &mut Workspace, db: &str, rid: &str, args: &Value) -> Result<(), ToolError> {
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
fn notes_suffix(notes: &[String]) -> String {
    if notes.is_empty() {
        String::new()
    } else {
        format!(" Notes: {}", notes.join("; "))
    }
}
