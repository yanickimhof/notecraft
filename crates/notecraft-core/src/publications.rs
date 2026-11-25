//! Where the record of web publications lives, and a read-only check of it.
//!
//! The record is kept in the app data folder, never inside the notes. The app writes it; other
//! programs (the MCP server) only read it, to refuse removing a page that is still live.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The record file (`publications.json`), next to the indexes.
pub fn store_path() -> PathBuf {
    let base = crate::sync::index_base_dir();
    base.parent().map(|p| p.to_path_buf()).unwrap_or(base).join("publications.json")
}

fn same_dir(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (Path::new(a).canonicalize(), Path::new(b).canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Root page ids of the live publications of a workspace that contain any of `ids`
/// (as their root or as one of their sent pages).
pub fn live_roots_in(record: &serde_json::Value, ws_root: &str, ids: &HashSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(sites) = record.get("sites").and_then(|s| s.as_object()) else { return out };
    for (ws, roots) in sites {
        if !same_dir(ws, ws_root) {
            continue;
        }
        let Some(roots) = roots.as_object() else { continue };
        for (root, entry) in roots {
            let in_pages = entry.get("pages").and_then(|p| p.as_array()).is_some_and(|ps| {
                ps.iter().any(|p| p.get(0).and_then(|v| v.as_str()).is_some_and(|id| ids.contains(id)))
            });
            if (ids.contains(root) || in_pages) && !out.contains(root) {
                out.push(root.clone());
            }
        }
    }
    out
}

/// Same, reading the record file. An unreadable or missing file means nothing is published.
pub fn live_roots(ws_root: &str, ids: &HashSet<String>) -> Vec<String> {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| live_roots_in(&v, ws_root, ids))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_roots_and_sent_pages() {
        let rec = serde_json::json!({"sites": {"/w": {
            "root1": {"pages": [["root1", "t"], ["sub1", "t"]]},
            "other": {"pages": [["x", "t"]]}
        }}});
        let ids: HashSet<String> = ["sub1".to_string()].into();
        assert_eq!(live_roots_in(&rec, "/w", &ids), vec!["root1".to_string()]);
        let ids: HashSet<String> = ["other".to_string()].into();
        assert_eq!(live_roots_in(&rec, "/w", &ids), vec!["other".to_string()]);
        let ids: HashSet<String> = ["nope".to_string()].into();
        assert!(live_roots_in(&rec, "/w", &ids).is_empty());
        let ids: HashSet<String> = ["root1".to_string()].into();
        assert!(live_roots_in(&rec, "/elsewhere", &ids).is_empty());
    }
}
