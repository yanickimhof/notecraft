//! The SQLite index in `.notecraft/index.db`.
//!
//! It is a cache: [`Index::rebuild`] recreates it from the in-memory
//! workspace (loaded from the files) at any time, and deleting the file is
//! always safe. It holds a document table, an FTS5 full-text table and the
//! `[[mention]]` link table used for backlinks.

use crate::Result;
use rusqlite::{params, Connection};
use std::path::Path;

const SCHEMA_VERSION: i64 = 1;

/// Wait this long for a locked database.
pub const BUSY_TIMEOUT_MS: u64 = 5000;

/// One indexed document (page, database or row).
#[derive(Debug, Clone, PartialEq)]
pub struct IndexDoc {
    /// `p:<id>`, `d:<id>` or `r:<db>:<row>`.
    pub key: String,
    pub kind: String,
    pub title: String,
    pub icon: String,
    pub body: String,
    pub trashed: bool,
    pub updated: String,
    /// Lowercased titles this document mentions.
    pub links: Vec<String>,
}

/// A full-text search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub key: String,
    pub title: String,
    pub icon: String,
    /// Matching excerpt; matched words are wrapped in `\u{1}`…`\u{2}`.
    pub snippet: String,
}

pub struct Index {
    conn: Connection,
}

impl Index {
    /// Open (or create) the index file.
    pub fn open(path: &Path) -> Result<Index> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        // a locked index is waited for, not reported at once
        conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS))?;
        // and makes it readable by its owner only: chmodSync(file, 384) = 0o600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        let idx = Index { conn };
        idx.prepare()?;
        Ok(idx)
    }

    /// Open the index; if that fails (damaged file), delete it and start
    /// over. The index is only
    /// a cache of the files, so nothing is lost.
    pub fn open_or_rebuild(path: &Path) -> Result<Index> {
        match Index::open(path) {
            Ok(i) => Ok(i),
            Err(_) => {
                for suffix in ["", "-wal", "-shm"] {
                    let mut p = path.as_os_str().to_owned();
                    p.push(suffix);
                    let _ = std::fs::remove_file(std::path::PathBuf::from(p));
                }
                Index::open(path)
            }
        }
    }

    /// An index kept in memory (tests, read-only folders).
    pub fn in_memory() -> Result<Index> {
        let idx = Index { conn: Connection::open_in_memory()? };
        idx.prepare()?;
        Ok(idx)
    }

    fn prepare(&self) -> Result<()> {
        self.conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let version: i64 = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);
        if version != SCHEMA_VERSION {
            self.conn
                .execute_batch("DROP TABLE IF EXISTS docs; DROP TABLE IF EXISTS fts; DROP TABLE IF EXISTS links; DROP TABLE IF EXISTS meta;")?;
        }
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS docs(
                 key TEXT PRIMARY KEY, kind TEXT NOT NULL, title TEXT NOT NULL,
                 icon TEXT NOT NULL, trashed INTEGER NOT NULL, updated TEXT NOT NULL);
             CREATE VIRTUAL TABLE IF NOT EXISTS fts USING fts5(
                 key UNINDEXED, title, body, tokenize = 'unicode61 remove_diacritics 2');
             CREATE TABLE IF NOT EXISTS links(src TEXT NOT NULL, target TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS links_target ON links(target);
             CREATE INDEX IF NOT EXISTS links_src ON links(src);
             CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        self.conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        Ok(())
    }

    /// A small value stored next to the index (`None` when absent).
    pub fn meta(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0)).ok()
    }

    /// Store a value, or forget it with `None`.
    pub fn set_meta(&self, key: &str, value: Option<&str>) {
        let _ = match value {
            Some(v) => self.conn.execute("INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)", params![key, v]),
            None => self.conn.execute("DELETE FROM meta WHERE key = ?1", params![key]),
        };
    }

    /// Replace the whole index with these documents.
    pub fn rebuild(&mut self, docs: &[IndexDoc]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch("DELETE FROM docs; DELETE FROM fts; DELETE FROM links;")?;
        for d in docs {
            insert(&tx, d)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Insert or update one document.
    pub fn upsert(&mut self, d: &IndexDoc) -> Result<()> {
        let tx = self.conn.transaction()?;
        remove(&tx, &d.key)?;
        insert(&tx, d)?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove(&mut self, key: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        remove(&tx, key)?;
        tx.commit()?;
        Ok(())
    }

    /// Full-text search over titles and bodies (prefix match on every word,
    /// all words required), best matches first. Trashed documents are skipped.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        let Some(q) = fts_query(query) else {
            return Ok(vec![]);
        };
        let mut st = self.conn.prepare(
            "SELECT f.key, d.title, d.icon, snippet(fts, 2, char(1), char(2), '…', 10)
             FROM fts f JOIN docs d ON d.key = f.key
             WHERE fts MATCH ?1 AND d.trashed = 0
             ORDER BY bm25(fts, 0.0, 8.0, 1.0) LIMIT ?2",
        )?;
        let rows = st.query_map(params![q, limit as i64], |r| {
            Ok(Hit { key: r.get(0)?, title: r.get(1)?, icon: r.get(2)?, snippet: r.get(3)? })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Keys of documents that mention a title (`[[title]]`).
    pub fn backlinks(&self, title: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare(
            "SELECT DISTINCT l.src FROM links l JOIN docs d ON d.key = l.src
             WHERE l.target = ?1 AND d.trashed = 0 ORDER BY d.title",
        )?;
        let rows = st.query_map(params![title.to_lowercase()], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Most recently edited documents (the quick finder's empty state).
    pub fn recent(&self, limit: usize) -> Result<Vec<(String, String, String)>> {
        let mut st =
            self.conn.prepare("SELECT key, title, icon FROM docs WHERE trashed = 0 ORDER BY updated DESC LIMIT ?1")?;
        let rows = st.query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn count(&self) -> Result<usize> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM docs", [], |r| r.get::<_, i64>(0))? as usize)
    }
}

fn insert(tx: &rusqlite::Transaction<'_>, d: &IndexDoc) -> Result<()> {
    tx.execute(
        "INSERT INTO docs(key, kind, title, icon, trashed, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![d.key, d.kind, d.title, d.icon, d.trashed as i64, d.updated],
    )?;
    tx.execute("INSERT INTO fts(key, title, body) VALUES (?1, ?2, ?3)", params![d.key, d.title, d.body])?;
    for l in &d.links {
        tx.execute("INSERT INTO links(src, target) VALUES (?1, ?2)", params![d.key, l.to_lowercase()])?;
    }
    Ok(())
}

fn remove(tx: &rusqlite::Transaction<'_>, key: &str) -> Result<()> {
    tx.execute("DELETE FROM docs WHERE key = ?1", params![key])?;
    tx.execute("DELETE FROM fts WHERE key = ?1", params![key])?;
    tx.execute("DELETE FROM links WHERE src = ?1", params![key])?;
    Ok(())
}

/// Turn user input into a safe FTS5 query: every word quoted, prefix-matched.
pub fn fts_query(input: &str) -> Option<String> {
    let words: Vec<String> = input
        .split(|c: char| c.is_whitespace() || c == '"' || c == '*' || c == '^' || c == ':' || c == '(' || c == ')')
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(key: &str, title: &str, body: &str, links: &[&str]) -> IndexDoc {
        IndexDoc {
            key: key.into(),
            kind: "page".into(),
            title: title.into(),
            icon: String::new(),
            body: body.into(),
            trashed: false,
            updated: "2026-01-01T00:00:00Z".into(),
            links: links.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn full_text_search_with_prefixes_and_accents() {
        let mut idx = Index::in_memory().unwrap();
        idx.rebuild(&[
            doc("p:1", "Recettes d'été", "Une tarte aux pêches", &[]),
            doc("p:2", "Work log", "Meeting about the roadmap", &[]),
        ])
        .unwrap();
        let hits = idx.search("peche", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "p:1");
        let hits = idx.search("road", 10).unwrap();
        assert_eq!(hits[0].key, "p:2");
        assert!(hits[0].snippet.contains('\u{1}'));
        assert!(idx.search("   ", 10).unwrap().is_empty());
        // hostile input does not break the query
        assert!(idx.search("\"AND OR (", 10).is_ok());
    }

    #[test]
    fn backlinks_and_updates() {
        let mut idx = Index::in_memory().unwrap();
        idx.rebuild(&[doc("p:1", "A", "", &["B"]), doc("p:2", "B", "", &[])]).unwrap();
        assert_eq!(idx.backlinks("b").unwrap(), vec!["p:1"]);
        idx.upsert(&doc("p:1", "A", "", &[])).unwrap();
        assert!(idx.backlinks("B").unwrap().is_empty());
        idx.remove("p:2").unwrap();
        assert_eq!(idx.count().unwrap(), 1);
    }

    #[test]
    fn trashed_docs_are_hidden() {
        let mut idx = Index::in_memory().unwrap();
        let mut d = doc("p:1", "Secret", "hidden words", &[]);
        d.trashed = true;
        idx.rebuild(&[d]).unwrap();
        assert!(idx.search("hidden", 10).unwrap().is_empty());
    }

    #[test]
    fn index_file_is_created_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("indexes/abc/index.db");
        let mut idx = Index::open(&p).unwrap();
        idx.rebuild(&[doc("p:1", "x", "y", &[])]).unwrap();
        assert!(p.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_damaged_index_is_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("index.db");
        std::fs::write(&p, b"this is not a sqlite file, just garbage bytes......").unwrap();
        assert!(Index::open(&p).is_err());
        let mut idx = Index::open_or_rebuild(&p).unwrap();
        idx.rebuild(&[doc("p:1", "kiwi", "", &[])]).unwrap();
        assert_eq!(idx.search("kiwi", 5).unwrap().len(), 1);
    }
}
