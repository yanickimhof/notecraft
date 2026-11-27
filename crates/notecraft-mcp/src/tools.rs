//! The MCP tools. Pages, rows and databases can be named by id or by exact
//! title. Write tools are listed in [`WRITE_TOOLS`] and refused in read-only mode.

use crate::policy::{Policy, Scope};
use notecraft_core::blocks::{self, Block, BlockKind};
use notecraft_core::database::{self, FilterMode, FilterOp, PropKind, PropValue, Property, Sort, View, ViewKind};
use notecraft_core::{import_export, DocId, Workspace};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

mod defs;
mod helpers;

pub use defs::definitions;
pub(crate) use helpers::find_any;
use helpers::*;

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
            let page = ws.pages.get_mut(&id).ok_or_else(|| ToolError::Failed(format!("page not found: {id}")))?;
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
            db_mut(ws, &db)?.schema.views.push(view);
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
            db_mut(ws, &db)?.schema.views[at] = view;
            ws.save_schema(&db)?;
            Ok("Updated the view.".into())
        }
        "delete_view" => {
            let db = find_db(ws, need(args, "database")?)?;
            let at = find_view_index(ws, &db, need(args, "view")?)?;
            if ws.dbs[&db].schema.views.len() <= 1 {
                return fail("a database keeps at least one view");
            }
            let removed = db_mut(ws, &db)?.schema.views.remove(at);
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
                row_mut(ws, &db, &tid, true)?.blocks = blocks::from_markdown(md);
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
            ensure_not_published(ws, std::slice::from_ref(&doc))?;
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
            let trashed: Vec<DocId> = ws.trash().into_iter().map(|(d, _)| d).collect();
            ensure_not_published(ws, &trashed)?;
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
                if let Some(v) = db_mut(ws, &id)?.schema.views.first_mut() {
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
            // Shown in the page it belongs to, not only as a sub-page.
            if let Some(pid) = &parent {
                let doc = DocId::Page(pid.clone());
                if ws.blocks(&doc).is_some() {
                    let t = ws.dbs[&id].schema.title.clone();
                    body_mut(ws, &doc)?.push(Block::new(BlockKind::Database, t));
                    save_body(ws, &doc)?;
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
                if let Some(p) = db_mut(ws, &db)?.schema.properties.iter_mut().find(|p| p.id == pid) {
                    p.relation_db = Some(target);
                }
                ws.save_schema(&db)?;
            }
            if let Some(f) = str_arg(args, "formula") {
                notecraft_core::formula::check(f).map_err(|e| ToolError::Failed(format!("formula: {e}")))?;
                if let Some(p) = db_mut(ws, &db)?.schema.properties.iter_mut().find(|p| p.id == pid) {
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
                if let Some(p) = db_mut(ws, &db)?.schema.properties.iter_mut().find(|p| p.id == pid) {
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
            row_mut(ws, &db, &rid, false)?.meta.title = title.to_string();
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
                row_mut(ws, &db, &rid, false)?.blocks.extend(blocks::from_markdown(md));
            }
            if let Some(t) = str_arg(args, "title") {
                row_mut(ws, &db, &rid, false)?.meta.title = t.to_string();
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
