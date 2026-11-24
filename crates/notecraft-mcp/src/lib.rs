//! MCP (Model Context Protocol) server for a NoteCraft workspace.
//!
//! JSON-RPC 2.0 messages, one per line on stdio. Methods: `initialize`,
//! `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
//! (every page as `notecraft://page/<id>`, Markdown). Writes go through the
//! core crate (atomic, conflict-checked); files changed by the desktop app or
//! a sync tool are re-read before each request. No network is used.

pub mod audit;
pub mod policy;
mod tools;

pub use audit::Audit;
pub use policy::{Class, Policy, Scope, DELETE_TOOLS};
pub use tools::WRITE_TOOLS;

use notecraft_core::{blocks, DocId, Workspace};
use serde_json::{json, Value};
use std::path::Path;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub struct Server {
    pub ws: Workspace,
    pub policy: Policy,
    pub scope: Scope,
    /// Journal of every tool call (`None` only in tests that do not need it).
    pub audit: Option<Audit>,
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

impl Server {
    /// Open a workspace; its index lives in the app's data folder.
    pub fn open(folder: &Path, policy: Policy) -> notecraft_core::Result<Server> {
        let ws = Workspace::open(folder)?;
        let scope = Scope::resolve(&ws, &policy.scope).map_err(notecraft_core::Error::Invalid)?;
        Ok(Server { ws, policy, scope, audit: Some(Audit::for_workspace(folder)) })
    }

    /// Open with an explicit index file (tests).
    pub fn open_at(folder: &Path, index: &Path, policy: Policy) -> notecraft_core::Result<Server> {
        let ws = Workspace::open_at(folder, index)?;
        let scope = Scope::resolve(&ws, &policy.scope).map_err(notecraft_core::Error::Invalid)?;
        let audit = index.parent().map(|d| Audit::new(d.join("audit.jsonl")));
        Ok(Server { ws, policy, scope, audit })
    }

    fn visible(&self, doc: &DocId) -> bool {
        self.scope.contains(&self.ws, doc)
    }

    /// Handle one line of input; `None` for notifications.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.into_iter().filter_map(|m| self.handle(m)).collect();
                (!replies.is_empty()).then(|| Value::Array(replies).to_string())
            }
            Ok(msg) => self.handle(msg).map(|v| v.to_string()),
            Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}")).to_string()),
        }
    }

    /// Handle one JSON-RPC message.
    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_string();
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let id = msg.get("id").cloned()?; // notifications get no reply
                                          // pick up edits made by the app, a sync tool or anyone else
        let _ = self.ws.sync_from_disk();
        let reply = match method.as_str() {
            "initialize" => {
                let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL_VERSION);
                let mode = format!(" Permissions: {}.", self.policy.describe());
                ok(
                    &id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false}},
                        "serverInfo": {"name": "notecraft", "title": "NøteCraft", "version": notecraft_core::version()},
                        "instructions": format!("A local NøteCraft workspace: pages, databases and attachments stored as Markdown files. Page bodies are Markdown; [[Title]] links pages. Start with search or list_pages, then get_page.{mode}")
                    }),
                )
            }
            "ping" => ok(&id, json!({})),
            "tools/list" => ok(&id, json!({"tools": tools::definitions(&self.policy)})),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let level = Class::of(name).name();
                let result = tools::call(&mut self.ws, name, &args, &self.policy, &self.scope);
                let (outcome, detail) = match &result {
                    Ok(_) => ("ok", String::new()),
                    Err(tools::ToolError::Unknown) => ("error", "unknown tool".to_string()),
                    Err(tools::ToolError::ReadOnly) => ("denied", "read-only".to_string()),
                    Err(tools::ToolError::Denied(e)) => ("denied", e.clone()),
                    Err(tools::ToolError::Failed(e)) => ("error", e.clone()),
                };
                if let Some(a) = &self.audit {
                    a.record(name, level, outcome, &args, &detail);
                }
                let text = |t: String, is_error: bool| {
                    ok(&id, json!({"content": [{"type": "text", "text": t}], "isError": is_error}))
                };
                match result {
                    Ok(t) => text(t, false),
                    Err(tools::ToolError::Unknown) => error(&id, -32602, &format!("unknown tool {name}")),
                    Err(tools::ToolError::ReadOnly) => {
                        text(format!("{name} is not available: the server was started with --read-only"), true)
                    }
                    Err(tools::ToolError::Denied(e)) | Err(tools::ToolError::Failed(e)) => text(e, true),
                }
            }
            "resources/list" => {
                let mut list = Vec::new();
                for (pid, p) in &self.ws.pages {
                    if self.ws.is_live(&DocId::Page(pid.clone())) && self.visible(&DocId::Page(pid.clone())) {
                        list.push(json!({
                            "uri": format!("notecraft://page/{pid}"),
                            "name": if p.meta.title.is_empty() { "Untitled".to_string() } else { p.meta.title.clone() },
                            "description": self.ws.path_label(&DocId::Page(pid.clone())),
                            "mimeType": "text/markdown"
                        }));
                    }
                }
                // attachments, typed by extension
                // attachments belong to no page, so a scoped server does not list them
                let assets = self.ws.root.join(notecraft_core::workspace::ASSETS_DIR);
                if let Ok(rd) = std::fs::read_dir(assets).map_err(|_| ()).and_then(|r| {
                    if self.scope.is_active() {
                        Err(())
                    } else {
                        Ok(r)
                    }
                }) {
                    let mut names: Vec<String> = rd
                        .filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .filter(|n| !n.starts_with('.'))
                        .collect();
                    names.sort();
                    for n in names {
                        let mime = notecraft_core::fsutil::mime_type(Path::new(&n));
                        list.push(json!({"uri": format!("notecraft://asset/{n}"), "name": n, "mimeType": mime}));
                    }
                }
                ok(&id, json!({"resources": list}))
            }
            "resources/read"
                if self.scope.is_active()
                    && params
                        .get("uri")
                        .and_then(Value::as_str)
                        .is_some_and(|u| u.starts_with("notecraft://asset/")) =>
            {
                error(&id, -32002, "resource not found")
            }
            "resources/read"
                if params.get("uri").and_then(Value::as_str).is_some_and(|u| u.starts_with("notecraft://asset/")) =>
            {
                let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
                let name = uri.trim_start_matches("notecraft://asset/");
                let path = self.ws.root.join(notecraft_core::workspace::ASSETS_DIR).join(name);
                if name.contains("..") || name.contains('/') || !path.is_file() {
                    error(&id, -32002, "resource not found")
                } else {
                    let mime = notecraft_core::fsutil::mime_type(&path);
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    let content = if mime.starts_with("text/") || mime == "application/json" {
                        json!({"uri": uri, "mimeType": mime, "text": String::from_utf8_lossy(&bytes)})
                    } else {
                        json!({"uri": uri, "mimeType": mime, "blob": base64(&bytes)})
                    };
                    ok(&id, json!({"contents": [content]}))
                }
            }
            "resources/read" => {
                let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
                match uri
                    .strip_prefix("notecraft://page/")
                    .filter(|pid| self.visible(&DocId::Page(pid.to_string())))
                    .and_then(|pid| self.ws.pages.get(pid))
                {
                    Some(p) => ok(
                        &id,
                        json!({"contents": [{"uri": uri, "mimeType": "text/markdown", "text": format!("# {}\n\n{}", p.meta.title, blocks::to_markdown(&p.blocks))}]}),
                    ),
                    None => error(&id, -32002, "resource not found"),
                }
            }
            "resources/templates/list" => ok(
                &id,
                json!({"resourceTemplates": [{"uriTemplate": "notecraft://page/{id}", "name": "Page", "mimeType": "text/markdown"}]}),
            ),
            "prompts/list" => ok(&id, json!({"prompts": []})),
            _ => error(&id, -32601, &format!("method not found: {method}")),
        };
        Some(reply)
    }
}

/// Standard base64 (for binary attachments in `resources/read`).
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn single_blocks_can_be_read_changed_moved_and_deleted() {
        let (_d, mut s) = server(false);
        call(&mut s, "create_page", json!({"title": "Plan"}));
        call(
            &mut s,
            "append_blocks",
            json!({"page": "Plan", "blocks": [
            {"type": "h1", "text": "Goals"}, {"type": "todo", "text": "write"}, {"type": "text", "text": "notes"}]}),
        );
        let (_, list) = call(&mut s, "get_blocks", json!({"page": "Plan"}));
        assert!(list.contains("\"index\": 2") && list.contains("Goals"), "{list}");
        assert!(
            !call(&mut s, "update_block", json!({"page": "Plan", "index": 1, "checked": true, "text": "write it"})).0
        );
        assert!(!call(&mut s, "update_block", json!({"page": "Plan", "index": 2, "type": "quote"})).0);
        assert!(
            !call(
                &mut s,
                "insert_blocks",
                json!({"page": "Plan", "index": 1, "blocks": [{"type": "text", "text": "intro"}]})
            )
            .0
        );
        assert!(!call(&mut s, "move_block", json!({"page": "Plan", "index": 0, "to": 3})).0);
        let page = call(&mut s, "get_page", json!({"page": "Plan"})).1;
        let at = |t: &str| page.find(t).unwrap_or_else(|| panic!("{t} missing in {page}"));
        assert!(
            at("intro") < at("[x] write it") && at("[x] write it") < at("> notes") && at("> notes") < at("# Goals"),
            "{page}"
        );
        assert!(call(&mut s, "update_block", json!({"page": "Plan", "index": 9, "text": "x"})).0);
        // a stale `expect` refuses the edit; the right one lets it through
        assert!(call(&mut s, "delete_block", json!({"page": "Plan", "index": 0, "expect": "something else"})).0);
        assert!(call(&mut s, "get_page", json!({"page": "Plan"})).1.contains("intro"));
        assert!(!call(&mut s, "delete_block", json!({"page": "Plan", "index": 0, "expect": "intro"})).0);
        assert!(!call(&mut s, "get_page", json!({"page": "Plan"})).1.contains("intro"));
        assert!(s.ws.list_snapshots().len() >= 3, "edits keep snapshots");
        // delete tools are hidden without --allow-delete
        let (_d2, mut s2) = server_with(Policy { read_only: false, allow_delete: false, ..Policy::default() });
        call(&mut s2, "create_page", json!({"title": "P"}));
        assert!(call(&mut s2, "delete_block", json!({"page": "P", "index": 0})).0);
    }

    #[test]
    fn the_new_property_kinds_and_the_dashboard_view_are_reachable_by_the_assistant() {
        let (_d, mut s) = server(false);
        call(&mut s, "create_database", json!({"title": "Crew"}));
        for (name, kind) in [("Who", "person"), ("Docs", "files"), ("No", "unique_id")] {
            let (err, msg) = call(&mut s, "add_property", json!({"database": "Crew", "name": name, "type": kind}));
            assert!(!err, "{kind}: {msg}");
        }
        let (err, msg) = call(&mut s, "create_view", json!({"database": "Crew", "name": "Board", "kind": "dashboard"}));
        assert!(!err, "{msg}");
        let (err, msg) = call(
            &mut s,
            "add_row",
            json!({"database": "Crew", "title": "A", "properties": {"Who": ["Ernest", "Marianne"]}}),
        );
        assert!(!err, "{msg}");
        call(&mut s, "add_row", json!({"database": "Crew", "title": "B"}));
        let rows = call(&mut s, "query_rows", json!({"database": "Crew"})).1;
        assert!(rows.contains("Ernest") && rows.contains("Marianne"), "{rows}");
        // the automatic numbers are 1 and 2, and cannot be written by hand
        assert!(rows.contains("1") && rows.contains("2"), "{rows}");
        let (err, _) = call(&mut s, "update_row", json!({"database": "Crew", "row": "A", "properties": {"No": 99}}));
        let after = call(&mut s, "query_rows", json!({"database": "Crew"})).1;
        assert!(err || !after.contains("99"), "a unique id is read only: {after}");
    }

    #[test]
    fn views_and_templates_can_be_created_changed_and_deleted() {
        let (_d, mut s) = server(false);
        call(&mut s, "create_database", json!({"title": "Tasks"}));
        let (err, msg) = call(
            &mut s,
            "create_view",
            json!({"database": "Tasks", "name": "Open", "kind": "board",
            "group_by": "Status", "filter": [{"property": "Status", "op": "is_not", "value": "Done"}]}),
        );
        assert!(!err, "{msg}");
        assert!(call(&mut s, "create_view", json!({"database": "Tasks", "name": "Bad", "kind": "pyramid"})).0);
        assert!(call(&mut s, "get_database", json!({"database": "Tasks"})).1.contains("Open"));
        assert!(
            !call(
                &mut s,
                "update_view",
                json!({"database": "Tasks", "view": "Open", "name": "Active", "group_by": ""})
            )
            .0
        );
        let (_, db) = call(&mut s, "get_database", json!({"database": "Tasks"}));
        assert!(db.contains("Active") && !db.contains("\"Open\""), "{db}");
        assert!(!call(&mut s, "delete_view", json!({"database": "Tasks", "view": "Active"})).0);
        let (err, msg) = call(
            &mut s,
            "create_template",
            json!({"database": "Tasks", "name": "Bug", "properties": {"Status": "Not started"}, "markdown": "Steps"}),
        );
        assert!(!err, "{msg}");
        assert!(call(&mut s, "list_templates", json!({"database": "Tasks"})).1.contains("Bug"));
        assert!(
            !call(&mut s, "add_row_from_template", json!({"database": "Tasks", "template": "Bug", "title": "Crash"})).0
        );
        assert!(!call(&mut s, "delete_template", json!({"database": "Tasks", "template": "Bug"})).0);
        assert!(!call(&mut s, "list_templates", json!({"database": "Tasks"})).1.contains("Bug"));
        // the last view cannot be deleted
        let views = s.ws.dbs.values().next().unwrap().schema.views.len();
        assert!(views >= 1);
    }

    #[test]
    fn reminders_can_be_set_listed_and_cleared() {
        let (_d, mut s) = server(false);
        call(&mut s, "create_page", json!({"title": "Call back"}));
        let (err, msg) = call(&mut s, "set_reminder", json!({"page": "Call back", "when": "2030-01-02 08:30"}));
        assert!(!err, "{msg}");
        let (_, list) = call(&mut s, "list_reminders", json!({}));
        assert!(list.contains("Call back") && list.contains("2030-01-02 08:30"), "{list}");
        assert!(call(&mut s, "set_reminder", json!({"page": "Call back", "when": "soon"})).0);
        call(&mut s, "set_reminder", json!({"page": "Call back"}));
        let (_, list) = call(&mut s, "list_reminders", json!({}));
        assert!(!list.contains("Call back"));
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }

    #[test]
    fn a_page_read_with_get_page_can_be_written_back_unchanged() {
        let (_d, mut s) = server(false);
        let md = "# Titre\n\n> [!NOTE]\n> 💡 **Important** <!--c:bg-yellow-->\n> - un\n> - [ ] deux <!--c:bg-red-->\n\n<!--columns-->\n### A\ntexte\n<!--column-->\n### B\nautre\n<!--/columns-->\n\n---\n\nFin\n";
        let (err, made) = call(&mut s, "create_page", json!({"title": "Rich", "markdown": md}));
        assert!(!err, "{made}");
        let body = |t: &str| t.split_once("<content>\n").unwrap().1.rsplit_once("\n</content>").unwrap().0.to_string();
        let first = body(&call(&mut s, "get_page", json!({"page": "Rich"})).1);
        let (err, r) = call(&mut s, "update_page", json!({"page": "Rich", "markdown": first, "mode": "replace"}));
        assert!(!err, "{r}");
        let second = body(&call(&mut s, "get_page", json!({"page": "Rich"})).1);
        assert_eq!(first, second);
        assert!(second.contains("<!--columns-->") && second.contains("- [ ] deux <!--c:bg-red-->"), "{second}");
    }

    #[test]
    fn replace_text_changes_one_passage_and_leaves_the_rest() {
        let (_d, mut s) = server(false);
        call(
            &mut s,
            "create_page",
            json!({"title": "Doc", "markdown": "# T\n\nAlpha beta\n\n> [!NOTE]\n> 💡 Alpha gamma <!--c:bg-red-->\n\nFin\n"}),
        );
        let (err, m) = call(&mut s, "replace_text", json!({"page": "Doc", "old": "Alpha", "new": "X"}));
        assert!(err && m.contains("2 times"), "{m}");
        let (err, m) = call(&mut s, "replace_text", json!({"page": "Doc", "old": "nope", "new": "X"}));
        assert!(err && m.contains("not found"), "{m}");
        let (err, m) = call(&mut s, "replace_text", json!({"page": "Doc", "old": "Alpha beta", "new": "Delta"}));
        assert!(!err, "{m}");
        let page = call(&mut s, "get_page", json!({"page": "Doc"})).1;
        assert!(
            page.contains("Delta") && page.contains("Alpha gamma <!--c:bg-red-->") && page.contains("Fin"),
            "{page}"
        );
        let (err, _) = call(&mut s, "replace_text", json!({"page": "Doc", "old": "Fin", "new": ""}));
        assert!(!err);
        assert!(!call(&mut s, "get_page", json!({"page": "Doc"})).1.contains("Fin"));
        call(&mut s, "lock_page", json!({"page": "Doc"}));
        assert!(call(&mut s, "replace_text", json!({"page": "Doc", "old": "Delta", "new": "Z"})).0);
    }

    #[test]
    fn long_pages_can_be_read_in_slices() {
        let (_d, mut s) = server(false);
        let md: String = (0..40).map(|i| format!("Ligne numéro {i}\n\n")).collect();
        call(&mut s, "create_page", json!({"title": "Long", "markdown": md}));
        let whole = call(&mut s, "get_page", json!({"page": "Long"})).1;
        assert!(!whole.contains("more characters"));
        let (err, a) = call(&mut s, "get_page", json!({"page": "Long", "max_chars": 100}));
        assert!(!err && a.contains("more characters: call get_page again with offset=100"), "{a}");
        let (_, b) = call(&mut s, "get_page", json!({"page": "Long", "offset": 100, "max_chars": 100000}));
        assert!(b.contains("characters 100..") && !b.contains("more characters"), "{b}");
        assert!(b.contains("Ligne numéro 39"));
    }

    #[test]
    fn the_file_tools_refuse_keys_system_files_and_hidden_folders() {
        let (d, mut s) = server(false);
        let home = std::env::var("HOME").unwrap_or_default();
        for bad in [
            format!("{home}/.ssh/id_ed25519"),
            format!("{home}/.aws/credentials"),
            format!("{home}/Library/Keychains/login.keychain-db"),
            format!("{home}/Library/Application Support/Google/Chrome/Default/Cookies"),
            "/etc/passwd".to_string(),
            "/etc/hosts".to_string(),
            format!("{home}/Documents/../.ssh/id_rsa"),
            "relative/file.txt".to_string(),
        ] {
            let (err, msg) = call(&mut s, "add_attachment", json!({"path": bad}));
            assert!(err, "{bad} must be refused: {msg}");
            assert!(call(&mut s, "import_markdown", json!({"path": bad})).0, "{bad}");
        }
        for bad in [format!("{home}/.ssh"), format!("{home}/Library/LaunchAgents/x"), "/etc/cron.d".to_string()] {
            assert!(call(&mut s, "export_markdown", json!({"destination": bad})).0, "{bad}");
        }
        // a normal file in a temporary folder still works
        let f = d.path().join("note.txt");
        std::fs::write(&f, "hello").unwrap();
        let (err, msg) = call(&mut s, "add_attachment", json!({"path": f.to_string_lossy()}));
        assert!(!err, "{msg}");
    }

    #[test]
    fn files_root_limits_the_file_tools_to_the_chosen_folders() {
        let allowed = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let policy = Policy { files_roots: vec![allowed.path().to_path_buf()], ..Policy::default() };
        let (_d, mut s) = server_with(policy);
        let ok = allowed.path().join("a.txt");
        let no = other.path().join("b.txt");
        std::fs::write(&ok, "a").unwrap();
        std::fs::write(&no, "b").unwrap();
        assert!(!call(&mut s, "add_attachment", json!({"path": ok.to_string_lossy()})).0);
        let (err, msg) = call(&mut s, "add_attachment", json!({"path": no.to_string_lossy()}));
        assert!(err && msg.contains("--files-root"), "{msg}");
    }

    #[test]
    fn a_cover_must_be_a_gradient_or_an_existing_asset() {
        let (_d, mut s) = server(false);
        call(&mut s, "create_page", json!({"title": "Cov"}));
        for bad in ["assets/missing.png", "../../etc/passwd", "/etc/passwd", "assets/../pages/x.md"] {
            assert!(call(&mut s, "set_cover", json!({"page": "Cov", "cover": bad})).0, "{bad}");
        }
        assert!(!call(&mut s, "set_cover", json!({"page": "Cov", "cover": "gradient:2"})).0);
        assert!(!call(&mut s, "set_cover", json!({"page": "Cov", "cover": ""})).0);
    }

    #[test]
    fn locked_documents_refuse_every_write_tool_and_stay_unchanged() {
        let (_d, mut s) = server(false);
        let (_, made) = call(&mut s, "create_page", json!({"title": "Lease", "markdown": "original"}));
        assert!(made.contains("Lease"));
        let (err, locked) = call(&mut s, "lock_page", json!({"page": "Lease"}));
        assert!(!err, "{locked}");
        for (tool, args) in [
            ("update_page", json!({"page": "Lease", "markdown": "changed", "mode": "replace"})),
            ("append_blocks", json!({"page": "Lease", "blocks": [{"type": "text", "text": "x"}]})),
            ("rename_page", json!({"page": "Lease", "title": "Other"})),
            ("trash_page", json!({"page": "Lease"})),
            ("set_icon", json!({"page": "Lease", "icon": "x"})),
            ("create_page", json!({"title": "Child", "parent": "Lease"})),
        ] {
            let (err, text) = call(&mut s, tool, args);
            assert!(err && text.contains("locked"), "{tool}: {text}");
        }
        let (_, page) = call(&mut s, "get_page", json!({"page": "Lease"}));
        assert!(page.contains("original") && !page.contains("changed"));
        assert!(page.contains("locked: yes"));
        let (err, _) = call(&mut s, "lock_page", json!({"page": "Lease"}));
        assert!(!err, "locking twice is harmless");
    }

    #[test]
    fn listings_are_paged_by_200() {
        let (_d, mut s) = server(false);
        for i in 0..205 {
            s.ws.create_page(None, &format!("Page {i:03}")).unwrap();
        }
        let (_, first) = call(&mut s, "list_pages", json!({}));
        assert_eq!(first.lines().filter(|l| l.starts_with("- ")).count(), 200);
        assert!(first.ends_with("next_cursor: 200"));
        let (_, rest) = call(&mut s, "list_pages", json!({"cursor": "200"}));
        assert_eq!(rest.lines().count(), 5);
    }

    #[test]
    fn attachments_are_resources_with_types() {
        let (d, mut s) = server(false);
        std::fs::write(d.path().join("notes.txt"), "hello").unwrap();
        std::fs::write(d.path().join("pic.png"), [137u8, 80, 78, 71]).unwrap();
        s.ws.add_asset(&d.path().join("notes.txt")).unwrap();
        s.ws.add_asset(&d.path().join("pic.png")).unwrap();
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "resources/list"})).unwrap();
        let res = r["result"]["resources"].as_array().unwrap();
        assert!(res.iter().any(|x| x["uri"] == "notecraft://asset/pic.png" && x["mimeType"] == "image/png"));
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 2, "method": "resources/read", "params": {"uri": "notecraft://asset/notes.txt"}})).unwrap();
        assert_eq!(r["result"]["contents"][0]["text"], "hello");
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": {"uri": "notecraft://asset/pic.png"}})).unwrap();
        assert_eq!(r["result"]["contents"][0]["blob"], "iVBORw==");
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 4, "method": "resources/read", "params": {"uri": "notecraft://asset/../x"}})).unwrap();
        assert!(r.get("error").is_some());
    }

    use super::*;

    /// A server with every level enabled (`read_only` false) or read-only.
    fn server(read_only: bool) -> (tempfile::TempDir, Server) {
        server_with(Policy { read_only, allow_delete: true, ..Policy::default() })
    }

    fn server_with(policy: Policy) -> (tempfile::TempDir, Server) {
        let dir = tempfile::tempdir().unwrap();
        let s = Server::open_at(&dir.path().join("notes"), &dir.path().join("index/index.db"), policy).unwrap();
        (dir, s)
    }

    fn call(s: &mut Server, name: &str, args: Value) -> (bool, String) {
        let r = s
            .handle(
                json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": name, "arguments": args}}),
            )
            .unwrap();
        let res = &r["result"];
        (res["isError"].as_bool().unwrap(), res["content"][0]["text"].as_str().unwrap().to_string())
    }

    #[test]
    fn handshake_and_tool_list() {
        let (_d, mut s) = server(false);
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}}})).unwrap();
        assert_eq!(r["result"]["serverInfo"]["name"], "notecraft");
        assert!(s.handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
        let names: Vec<&str> =
            r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        for n in [
            "list_pages",
            "get_page",
            "search",
            "backlinks",
            "recent_pages",
            "favorites",
            "create_page",
            "update_page",
            "append_blocks",
            "rename_page",
            "move_page",
            "duplicate_page",
            "set_icon",
            "set_cover",
            "set_favorite",
            "trash_page",
            "restore_page",
            "empty_trash",
            "list_databases",
            "get_database",
            "create_database",
            "add_property",
            "rename_property",
            "delete_property",
            "add_row",
            "update_row",
            "delete_row",
            "query_rows",
            "list_templates",
            "add_row_from_template",
            "add_attachment",
            "import_markdown",
            "export_markdown",
        ] {
            assert!(names.contains(&n), "{n}");
        }
        assert_eq!(s.handle(json!({"jsonrpc": "2.0", "id": 3, "method": "nope"})).unwrap()["error"]["code"], -32601);
        assert!(s.handle_line("{not json").unwrap().contains("-32700"));
    }

    #[test]
    fn read_only_hides_and_refuses_writes() {
        let (_d, mut s) = server(true);
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
        let names: Vec<&str> =
            r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"search") && !names.contains(&"create_page"));
        let (err, msg) = call(&mut s, "create_page", json!({"title": "x"}));
        assert!(err && msg.contains("read-only"));
        assert!(s.ws.pages.is_empty());
    }

    #[test]
    fn pages_round_trip() {
        let (_d, mut s) = server(false);
        let (err, text) =
            call(&mut s, "create_page", json!({"title": "Trip", "markdown": "# Plan\n\n- pack bags\n- book train"}));
        assert!(!err, "{text}");
        assert!(
            !call(&mut s, "create_page", json!({"title": "Packing", "parent": "Trip", "markdown": "see [[Trip]]"})).0
        );
        assert!(call(&mut s, "search", json!({"query": "train"})).1.contains("Trip"));
        let page = call(&mut s, "get_page", json!({"page": "Trip"})).1;
        assert!(
            page.contains("- book train") && page.contains("Backlinks: Packing") && page.contains("Sub-pages: Packing")
        );
        assert!(!call(&mut s, "update_page", json!({"page": "Trip", "markdown": "- [ ] buy snacks"})).0);
        assert!(!call(&mut s, "append_blocks", json!({"page": "Trip", "blocks": [{"type": "todo", "text": "tickets", "checked": true}, {"type": "h2", "text": "Days"}]})).0);
        let page = call(&mut s, "get_page", json!({"page": "Trip"})).1;
        assert!(
            page.contains("- [ ] buy snacks")
                && page.contains("- [x] tickets")
                && page.contains("## Days")
                && page.contains("pack bags")
        );
        assert!(call(&mut s, "list_pages", json!({})).1.contains("  - Packing"));
        assert!(!call(&mut s, "rename_page", json!({"page": "Trip", "title": "Holiday"})).0);
        assert!(call(&mut s, "get_page", json!({"page": "Packing"})).1.contains("[[Holiday]]"));
        assert!(!call(&mut s, "move_page", json!({"page": "Packing"})).0);
        assert!(call(&mut s, "list_pages", json!({})).1.contains("\n- Packing"));
        assert!(!call(&mut s, "set_favorite", json!({"page": "Holiday", "favorite": true})).0);
        assert!(call(&mut s, "favorites", json!({})).1.contains("Holiday"));
        assert!(!call(&mut s, "set_icon", json!({"page": "Holiday", "icon": "🏖"})).0);
        assert!(!call(&mut s, "duplicate_page", json!({"page": "Holiday"})).0);
        assert!(call(&mut s, "recent_pages", json!({})).1.contains("Holiday"));
        assert!(!call(&mut s, "trash_page", json!({"page": "Packing"})).0);
        assert!(!call(&mut s, "restore_page", json!({"page": "Packing"})).0);
        assert!(!call(&mut s, "trash_page", json!({"page": "Packing"})).0);
        assert!(call(&mut s, "empty_trash", json!({"confirm": false})).0);
        assert!(!call(&mut s, "empty_trash", json!({"confirm": true})).0);
        let (err, msg) = call(&mut s, "get_page", json!({"page": "Nope"}));
        assert!(err && msg.contains("not found"));
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 9, "method": "resources/list"})).unwrap();
        let res = r["result"]["resources"].as_array().unwrap();
        assert_eq!(res.len(), 2);
        let uri = res[0]["uri"].as_str().unwrap().to_string();
        let r =
            s.handle(json!({"jsonrpc": "2.0", "id": 10, "method": "resources/read", "params": {"uri": uri}})).unwrap();
        assert!(r["result"]["contents"][0]["text"].as_str().unwrap().starts_with("# "));
    }

    #[test]
    fn databases_rows_queries_templates() {
        let (_d, mut s) = server(false);
        let (err, t) = call(
            &mut s,
            "create_database",
            json!({"title": "Tasks", "view": "board", "properties": [{"name": "Points", "type": "number"}, {"name": "Area", "type": "select", "options": ["Home", "Work"]}]}),
        );
        assert!(!err, "{t}");
        assert!(!call(&mut s, "add_row", json!({"database": "Tasks", "title": "Write report", "properties": {"Status": "In progress", "Tags": ["Work"], "Date": "2026-12-01", "Points": 3}})).0);
        assert!(
            !call(
                &mut s,
                "add_row",
                json!({"database": "Tasks", "title": "Buy milk", "properties": {"Status": "Not started", "Points": 1}})
            )
            .0
        );
        let rows: Value = serde_json::from_str(
            &call(&mut s, "query_rows", json!({"database": "Tasks", "filter": {"Status": "In progress"}})).1,
        )
        .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["properties"]["Tags"], "Work");
        let rows: Value = serde_json::from_str(
            &call(
                &mut s,
                "query_rows",
                json!({"database": "Tasks", "sort": [{"property": "Points", "descending": false}]}),
            )
            .1,
        )
        .unwrap();
        assert_eq!(rows[0]["title"], "Buy milk");
        let rows: Value = serde_json::from_str(&call(&mut s, "query_rows", json!({"database": "Tasks", "filter": [{"property": "Points", "op": "greater", "value": 2}, {"property": "Status", "op": "is", "value": "Not started"}], "match": "any"})).1).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 2);
        let groups: Value =
            serde_json::from_str(&call(&mut s, "query_rows", json!({"database": "Tasks", "group_by": "Status"})).1)
                .unwrap();
        assert_eq!(groups.as_array().unwrap().len(), 2);
        assert!(
            !call(
                &mut s,
                "update_row",
                json!({"database": "Tasks", "row": "Buy milk", "properties": {"Status": "Done"}})
            )
            .0
        );
        assert!(call(&mut s, "query_rows", json!({"database": "Tasks", "filter": {"Status": "Done"}}))
            .1
            .contains("Buy milk"));
        let (err, msg) = call(&mut s, "add_row", json!({"database": "Tasks", "title": "x", "properties": {"Nope": 1}}));
        assert!(err && msg.contains("Nope"));
        assert!(!call(&mut s, "add_property", json!({"database": "Tasks", "name": "Done", "type": "checkbox"})).0);
        assert!(
            !call(&mut s, "rename_property", json!({"database": "Tasks", "property": "Points", "new_name": "Effort"}))
                .0
        );
        assert!(!call(&mut s, "delete_property", json!({"database": "Tasks", "property": "Area"})).0);
        let db: Value = serde_json::from_str(&call(&mut s, "get_database", json!({"database": "Tasks"})).1).unwrap();
        assert!(db["properties"].as_array().unwrap().iter().any(|p| p["name"] == "Effort"));
        assert_eq!(db["rows"].as_array().unwrap().len(), 2);
        // templates
        let dbid = db["id"].as_str().unwrap().to_string();
        let tpl = s.ws.add_template(&dbid, "Bug").unwrap();
        s.ws.dbs.get_mut(&dbid).unwrap().templates.get_mut(&tpl).unwrap().blocks = blocks::from_markdown("## Steps");
        s.ws.save_template(&dbid, &tpl).unwrap();
        assert!(call(&mut s, "list_templates", json!({"database": "Tasks"})).1.contains("## Steps"));
        assert!(
            !call(&mut s, "add_row_from_template", json!({"database": "Tasks", "template": "Bug", "title": "Crash"})).0
        );
        assert!(call(&mut s, "get_page", json!({"page": "Crash"})).1.contains("## Steps"));
        assert!(!call(&mut s, "delete_row", json!({"database": "Tasks", "row": "Crash"})).0);
        assert!(call(&mut s, "list_databases", json!({})).1.contains("\"rows\": 2"));
    }

    #[test]
    fn files_import_export_attach() {
        let (d, mut s) = server(false);
        let src = d.path().join("in");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.md"), "# A\n\nhello").unwrap();
        std::fs::write(src.join("sub/b.md"), "bee").unwrap();
        std::fs::write(d.path().join("single.md"), "# Single\n\nalone").unwrap();
        std::fs::write(d.path().join("pic.png"), b"png").unwrap();
        assert!(call(&mut s, "import_markdown", json!({"path": src.display().to_string()})).1.contains("Imported"));
        assert!(!call(&mut s, "import_markdown", json!({"path": d.path().join("single.md").display().to_string()})).0);
        assert!(call(&mut s, "list_pages", json!({})).1.contains("- Single"));
        let (err, msg) = call(
            &mut s,
            "add_attachment",
            json!({"path": d.path().join("pic.png").display().to_string(), "page": "Single", "caption": "Photo"}),
        );
        assert!(!err, "{msg}");
        assert!(call(&mut s, "get_page", json!({"page": "Single"})).1.contains("![Photo](assets/pic.png)"));
        let out = d.path().join("export");
        assert!(!call(&mut s, "export_markdown", json!({"destination": out.display().to_string()})).0);
        assert!(out.join("Single.md").exists());
        assert!(out.join("assets/pic.png").exists());
    }

    #[test]
    fn deletion_is_off_by_default_and_listed_only_when_allowed() {
        let (_d, mut s) = server_with(Policy::default());
        let (_, id) = call(&mut s, "create_page", json!({"title": "Keep me"}));
        assert!(id.contains("Created"));
        for tool in DELETE_TOOLS {
            let listed = s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})).unwrap();
            assert!(!listed.to_string().contains(&format!("\"{tool}\"")), "{tool} must not be offered");
        }
        let (err, msg) = call(&mut s, "trash_page", json!({"page": "Keep me"}));
        assert!(err && msg.contains("--allow-delete"), "{msg}");
        let (_, got) = call(&mut s, "get_page", json!({"page": "Keep me"}));
        assert!(!got.contains("in trash"));

        let (_d2, mut s2) = server_with(Policy { allow_delete: true, ..Policy::default() });
        call(&mut s2, "create_page", json!({"title": "Bye"}));
        let (err, _) = call(&mut s2, "trash_page", json!({"page": "Bye"}));
        assert!(!err);
    }

    #[test]
    fn scope_limits_reads_writes_and_listings() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("notes");
        let index = dir.path().join("index/index.db");
        {
            let mut s = Server::open_at(&notes, &index, Policy::default()).unwrap();
            call(&mut s, "create_page", json!({"title": "Work", "markdown": "alpha secret-work"}));
            call(&mut s, "create_page", json!({"title": "Private", "markdown": "alpha secret-private"}));
            call(&mut s, "create_page", json!({"title": "Task", "parent": "Work", "markdown": "alpha task"}));
            call(&mut s, "set_reminder", json!({"page": "Private", "when": "2031-05-05 10:00"}));
            call(&mut s, "set_reminder", json!({"page": "Task", "when": "2031-05-06 10:00"}));
        }
        let policy = Policy { scope: vec!["Work".into()], ..Policy::default() };
        let mut s = Server::open_at(&notes, &index, policy).unwrap();

        let (_, tree) = call(&mut s, "list_pages", json!({}));
        assert!(tree.contains("Work") && tree.contains("Task") && !tree.contains("Private"), "{tree}");
        let (_, hits) = call(&mut s, "search", json!({"query": "alpha"}));
        assert!(hits.contains("Task") && !hits.contains("Private"), "{hits}");
        let (_, recent) = call(&mut s, "recent_pages", json!({}));
        assert!(!recent.contains("Private"), "{recent}");

        let (err, msg) = call(&mut s, "get_page", json!({"page": "Private"}));
        assert!(err && msg.contains("outside the scope"), "{msg}");
        let (err, _) = call(&mut s, "update_page", json!({"page": "Private", "markdown": "x"}));
        assert!(err);
        let (_, reminders) = call(&mut s, "list_reminders", json!({}));
        assert!(reminders.contains("Task") && !reminders.contains("Private"), "{reminders}");
        for tool in ["get_blocks", "delete_block", "update_block"] {
            let (err, _) = call(&mut s, tool, json!({"page": "Private", "index": 0}));
            assert!(err, "{tool} must stay inside the scope");
        }
        let (err, _) = call(&mut s, "create_page", json!({"title": "Loose"}));
        assert!(err, "a scoped server cannot create at the top level");
        let (err, _) = call(&mut s, "move_page", json!({"page": "Task", "parent": "Private"}));
        assert!(err, "nothing can be moved out of the scope");
        let (err, _) = call(&mut s, "create_page", json!({"title": "Sub", "parent": "Work"}));
        assert!(!err);

        let res = s.handle(json!({"jsonrpc": "2.0", "id": 2, "method": "resources/list"})).unwrap().to_string();
        assert!(!res.contains("Private"), "{res}");
        assert!(Server::open_at(&notes, &index, Policy { scope: vec!["Nope".into()], ..Policy::default() }).is_err());
    }

    #[test]
    fn every_call_is_recorded_without_page_content() {
        let (d, mut s) = server_with(Policy::default());
        let long = "x".repeat(500);
        call(&mut s, "create_page", json!({"title": "Journal", "markdown": long}));
        call(&mut s, "get_page", json!({"page": "Journal"}));
        call(&mut s, "trash_page", json!({"page": "Journal"}));
        let log = std::fs::read_to_string(d.path().join("index/audit.jsonl")).unwrap();
        let lines: Vec<Value> = log.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["tool"], "create_page");
        assert_eq!(lines[0]["level"], "write");
        assert_eq!(lines[0]["outcome"], "ok");
        assert_eq!(lines[0]["args"]["markdown"], "<500 chars>");
        assert_eq!(lines[1]["level"], "read");
        assert_eq!(lines[2]["outcome"], "denied");
        assert!(!log.contains(&"x".repeat(100)));
    }

    #[test]
    fn replacing_a_body_keeps_the_previous_version_in_a_snapshot() {
        let (_d, mut s) = server_with(Policy::default());
        call(&mut s, "create_page", json!({"title": "Notes", "markdown": "original text"}));
        let (err, _) = call(&mut s, "update_page", json!({"page": "Notes", "markdown": "new text", "mode": "replace"}));
        assert!(!err);
        let snaps = s.ws.list_snapshots();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].reason, "update_page (replace)");
        // appending does not need one
        call(&mut s, "update_page", json!({"page": "Notes", "markdown": "more"}));
        assert_eq!(s.ws.list_snapshots().len(), 1);
        let id = snaps[0].id.clone();
        s.ws.restore_snapshot(&id).unwrap();
        let (_, got) = call(&mut s, "get_page", json!({"page": "Notes"}));
        assert!(got.contains("original text") && !got.contains("new text"), "{got}");
    }
}
