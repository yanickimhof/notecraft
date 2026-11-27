//! Tool descriptions sent to the client.

use super::*;

pub(super) fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
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
