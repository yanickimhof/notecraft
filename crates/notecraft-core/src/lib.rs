//! NoteCraft core: the UI-free heart of the app.
//!
//! A *workspace* is a plain folder chosen by the user. Plain files are the
//! source of truth:
//!
//! ```text
//! <workspace>/
//!   pages/<slug>-<id>.md       one Markdown file per page (YAML front matter)
//!   databases/<slug>-<id>/     one folder per database
//!     schema.json              properties, views
//!     rows/<slug>-<id>.md      one Markdown file per row
//!     templates/<id>.md        row templates
//!   assets/                    attachments, referenced by relative path
//! ```
//!
//! The SQLite index lives in the app's data folder (see [`sync::index_file_for`])
//! and is rebuilt from the files at any time; the notes folder holds only notes.
//!
//! Modules are deliberately small; the desktop app (`notecraft-app`) and the
//! command line (`notecraft-cli`) both sit on top of [`workspace::Workspace`].
//! A future MCP server or sync layer only needs this crate.

pub mod backup;
pub mod blocks;
pub mod conflict;
pub mod database;
pub mod formula;
pub mod frontmatter;
pub mod fsutil;
pub mod graph;
pub mod history;
pub mod ids;
pub mod import_any;
pub mod import_check;
pub mod import_csv;
pub mod import_docx;
pub mod import_enex;
pub mod import_export;
pub mod import_html;
pub mod import_rich_md;
pub mod import_zip;
pub mod index;
pub mod inline;
pub mod lock;
pub mod log;
pub mod page;
pub mod page_options;
pub mod pdf;
pub mod reminder;
pub mod render;
pub mod sample;
pub mod search;
pub mod second_brain;
pub mod site;
pub mod snapshot;
pub mod solve;
pub mod sync;
pub mod vault;
pub mod watch;
pub mod workspace;

pub use blocks::{Block, BlockKind};
pub use conflict::{Conflict, Resolution};
pub use database::{Database, PropKind, PropValue, Property, Row, View, ViewKind};
pub use page::{Page, PageMeta};
pub use workspace::{DocId, Workspace};

/// Error type used across the core crate.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Yaml(String),
    Json(serde_json::Error),
    Sql(rusqlite::Error),
    NotFound(String),
    Invalid(String),
    /// The document (or one of its parents) is locked.
    Locked(String),
    /// The workspace holds files written by a newer version of the app.
    Newer(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Yaml(e) => write!(f, "front matter error: {e}"),
            Error::Json(e) => write!(f, "JSON error: {e}"),
            Error::Sql(e) => write!(f, "index error: {e}"),
            Error::NotFound(s) => write!(f, "not found: {s}"),
            Error::Invalid(s) => write!(f, "invalid: {s}"),
            Error::Locked(s) => write!(f, "locked: {s}"),
            Error::Newer(s) => write!(f, "{NEWER_FORMAT} {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
impl From<serde_yaml_ng::Error> for Error {
    fn from(e: serde_yaml_ng::Error) -> Self {
        Error::Yaml(e.to_string())
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sql(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Version of the file format (front matter of pages and rows, `schema.json`).
///
/// Every file the app writes carries `format: <n>`; a file without it is format 1. A newer
/// format is only read, never written: opening such a workspace with an older app must not
/// rewrite files it does not fully understand. Raise this number, and add a migration, when
/// the layout of the files changes in a way an older version could misread.
pub const FORMAT_VERSION: u32 = 1;

/// Start of the warning given for a file written by a newer version.
pub const NEWER_FORMAT: &str = "newer format:";

/// Returns the crate version, used by the CLI and the app's About box.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Current time as an RFC 3339 string (UTC, second precision).
pub fn now_string() -> String {
    let now = jiff::Timestamp::now().as_second();
    jiff::Timestamp::from_second(now).map(|t| t.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn now_is_rfc3339() {
        let n = now_string();
        assert!(n.parse::<jiff::Timestamp>().is_ok());
        assert!(n.ends_with('Z') && !n.contains('.'), "{n}");
    }
}
