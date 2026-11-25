//! Integration tests: start the real `notecraft-mcp` binary and talk to it
//! over stdin/stdout like an MCP client does.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start(workspace: &std::path::Path, index: &std::path::Path, extra: &[&str]) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_notecraft-mcp"))
            .arg("--workspace")
            .arg(workspace)
            .args(extra)
            .env("NOTECRAFT_INDEX_DIR", index)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start notecraft-mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Client { child, stdin, stdout, next: 1 }
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).expect("a JSON reply per line");
        assert_eq!(v["id"], id);
        v
    }

    fn tool(&mut self, name: &str, args: Value) -> (bool, String) {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}));
        (r["result"]["isError"].as_bool().unwrap(), r["result"]["content"][0]["text"].as_str().unwrap().to_string())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn full_session_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("Notes");
    let idx = dir.path().join("indexes");
    let mut c = Client::start(&ws, &idx, &["--allow-delete"]);
    let init = c.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    c.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let tools = c.request("tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().len() >= 33);
    assert!(!c.tool("create_page", json!({"title": "Inbox", "markdown": "- call the plumber"})).0);
    assert!(c.tool("search", json!({"query": "plumber"})).1.contains("Inbox"));
    // the files really are on disk, and the index is not in the notes folder
    let names: Vec<String> = std::fs::read_dir(ws.join("pages"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(names[0].starts_with("inbox-") && names[0].ends_with(".md"));
    assert!(!ws.join(".notecraft").exists());
    assert!(std::fs::read_dir(&idx).unwrap().count() >= 1);
    // an edit made by someone else is seen by the next request
    let path = ws.join("pages").join(&names[0]);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let text = std::fs::read_to_string(&path).unwrap().replace("plumber", "electrician");
    std::fs::write(&path, text).unwrap();
    assert!(c.tool("get_page", json!({"page": "Inbox"})).1.contains("electrician"));
    let r = c.request("resources/list", json!({}));
    assert_eq!(r["result"]["resources"].as_array().unwrap().len(), 1);
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn read_only_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("Notes");
    std::fs::create_dir_all(ws.join("pages")).unwrap();
    std::fs::write(ws.join("pages/hello.md"), "---\nid: h1\ntitle: Hello\n---\n\nworld\n").unwrap();
    let mut c = Client::start(&ws, &dir.path().join("indexes"), &["--read-only"]);
    c.request("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}}));
    let tools = c.request("tools/list", json!({}));
    let names: Vec<&str> =
        tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"get_page") && !names.contains(&"trash_page"));
    assert!(c.tool("get_page", json!({"page": "Hello"})).1.contains("world"));
    let (err, msg) = c.tool("trash_page", json!({"page": "Hello"}));
    assert!(err && msg.contains("read-only"));
    assert!(ws.join("pages/hello.md").exists());
}

/// A page that is still live on the web cannot be trashed through the server, and goes to the
/// trash normally once the record no longer lists it.
#[test]
fn published_page_is_not_trashed() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("Notes");
    std::fs::create_dir_all(ws.join("pages")).unwrap();
    std::fs::write(ws.join("pages/hello.md"), "---\nid: h1\ntitle: Hello\n---\n\nworld\n").unwrap();
    let record = dir.path().join("publications.json");
    let live = json!({"sites": {ws.display().to_string(): {"h1": {"pages": [["h1", "t"]]}}}});
    std::fs::write(&record, live.to_string()).unwrap();
    let mut c = Client::start(&ws, &dir.path().join("indexes"), &["--allow-delete"]);
    c.request("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}}));
    let (err, msg) = c.tool("trash_page", json!({"page": "Hello"}));
    assert!(err && msg.contains("published") && msg.contains("Unpublish"), "{msg}");
    assert!(ws.join("pages/hello.md").exists());
    let (err, _) = c.tool("empty_trash", json!({"confirm": true}));
    assert!(!err);
    std::fs::write(&record, json!({"sites": {}}).to_string()).unwrap();
    let (err, msg) = c.tool("trash_page", json!({"page": "Hello"}));
    assert!(!err, "{msg}");
}
