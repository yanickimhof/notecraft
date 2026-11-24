//! What an MCP client may do: three levels of tools (read, write, delete) and an optional scope.
//!
//! The policy is fixed when the server starts, by the person who registers it in the client's
//! configuration. A client cannot widen it: there is no tool to change a flag.
//!
//! - `--read-only`: only read tools.
//! - default: read and write tools. Destructive tools stay off.
//! - `--allow-delete`: also the destructive tools ([`DELETE_TOOLS`]).
//! - `--scope <page>` (repeatable): everything the client sees, reads or changes is limited to
//!   those pages and what is inside them.

use crate::tools::{ToolError, WRITE_TOOLS};
use notecraft_core::{DocId, Workspace};

/// Tools that remove or destroy content. Refused unless the server runs with `--allow-delete`.
pub const DELETE_TOOLS: &[&str] =
    &["trash_page", "delete_row", "empty_trash", "delete_property", "delete_block", "delete_view", "delete_template"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Read,
    Write,
    Delete,
}

impl Class {
    pub fn of(tool: &str) -> Class {
        if DELETE_TOOLS.contains(&tool) {
            Class::Delete
        } else if WRITE_TOOLS.contains(&tool) {
            Class::Write
        } else {
            Class::Read
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Class::Read => "read",
            Class::Write => "write",
            Class::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub read_only: bool,
    pub allow_delete: bool,
    /// Pages or databases (id or title) the client is limited to; empty means the whole workspace.
    pub scope: Vec<String>,
    /// Folders the file tools (`add_attachment`, `import_markdown`, `export_markdown`) may touch.
    /// Empty means the default: the home folder without hidden folders and `Library`, the
    /// temporary folders, external volumes and the workspace itself.
    pub files_roots: Vec<std::path::PathBuf>,
}

impl Policy {
    pub fn read_only() -> Policy {
        Policy { read_only: true, ..Policy::default() }
    }

    /// Whether the tool is offered to the client at all.
    pub fn offers(&self, tool: &str) -> bool {
        match Class::of(tool) {
            Class::Read => true,
            Class::Write => !self.read_only,
            Class::Delete => !self.read_only && self.allow_delete,
        }
    }

    /// Refuse a tool the policy does not allow.
    pub fn check(&self, tool: &str) -> Result<(), ToolError> {
        match Class::of(tool) {
            Class::Read => Ok(()),
            Class::Write if self.read_only => Err(ToolError::ReadOnly),
            Class::Delete if self.read_only => Err(ToolError::ReadOnly),
            Class::Delete if !self.allow_delete => Err(ToolError::Denied(format!(
                "{tool} is disabled: it deletes content. Start the server with --allow-delete to enable it."
            ))),
            _ => Ok(()),
        }
    }

    /// Check a path a client gave for reading or writing a file on the computer, and return its
    /// canonical form. A client (or text it was tricked by) must not reach keys, passwords,
    /// cookies or system files through the file tools.
    pub fn check_local_path(
        &self,
        path: &std::path::Path,
        workspace: &std::path::Path,
    ) -> Result<std::path::PathBuf, ToolError> {
        use std::path::{Component, Path, PathBuf};
        let deny = |why: &str| {
            ToolError::Denied(format!("{}: {why}. Put the file in a normal folder under your home folder, or start the server with --files-root <folder>.", path.display()))
        };
        if !path.is_absolute() {
            return Err(ToolError::Failed(format!("{}: give an absolute path", path.display())));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(deny("'..' is not allowed"));
        }
        // a file that does not exist yet (an export target) is judged by its nearest existing folder
        let canon: PathBuf = {
            let mut existing = path.to_path_buf();
            let mut tail: Vec<std::ffi::OsString> = Vec::new();
            while !existing.exists() {
                match (existing.file_name().map(|n| n.to_os_string()), existing.parent()) {
                    (Some(n), Some(p)) => {
                        tail.push(n);
                        existing = p.to_path_buf();
                    }
                    _ => break,
                }
            }
            let mut c = existing.canonicalize().map_err(|_| deny("cannot be resolved"))?;
            for t in tail.iter().rev() {
                c.push(t);
            }
            c
        };
        let canon_ws = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
        if canon.starts_with(&canon_ws) {
            return Ok(canon);
        }
        if !self.files_roots.is_empty() {
            let ok = self.files_roots.iter().any(|r| canon.starts_with(r.canonicalize().unwrap_or_else(|_| r.clone())));
            return if ok { Ok(canon) } else { Err(deny("outside the folders allowed with --files-root")) };
        }
        for tmp in ["/tmp", "/private/tmp", "/var/folders", "/private/var/folders", "/Volumes"] {
            if canon.starts_with(Path::new(tmp)) {
                return Ok(canon);
            }
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).and_then(|h| h.canonicalize().ok());
        let Some(home) = home else { return Err(deny("no home folder to check against")) };
        let Ok(rel) = canon.strip_prefix(&home) else { return Err(deny("outside your home folder")) };
        let mut parts = rel.components();
        if let Some(Component::Normal(first)) = parts.clone().next() {
            if first == "Library" {
                return Err(deny("the Library folder holds keys, cookies and application data"));
            }
        }
        if rel.components().any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().starts_with('.'))) {
            return Err(deny("hidden files and folders (.ssh, .aws, .config...) are off limits"));
        }
        let _ = parts.next();
        Ok(canon)
    }

    pub fn is_scoped(&self) -> bool {
        !self.scope.is_empty()
    }

    /// A short description for the `initialize` instructions and the log.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.read_only {
            parts.push("read-only".to_string());
        } else if self.allow_delete {
            parts.push("read, write and delete".to_string());
        } else {
            parts.push("read and write, deletion disabled".to_string());
        }
        if self.is_scoped() {
            parts.push(format!("limited to: {}", self.scope.join(", ")));
        }
        parts.join("; ")
    }
}

/// The scope resolved to documents; `None` roots mean the whole workspace.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    roots: Vec<DocId>,
}

impl Scope {
    /// Resolve the configured pages; fails when one of them does not exist.
    pub fn resolve(ws: &Workspace, keys: &[String]) -> Result<Scope, String> {
        let mut roots = Vec::new();
        for k in keys {
            let doc = crate::tools::find_any(ws, k).map_err(|_| format!("scope not found: {k}"))?;
            roots.push(doc);
        }
        Ok(Scope { roots })
    }

    pub fn is_active(&self) -> bool {
        !self.roots.is_empty()
    }

    pub fn roots(&self) -> &[DocId] {
        &self.roots
    }

    /// True when there is no scope, or `doc` is a scope root or inside one.
    pub fn contains(&self, ws: &Workspace, doc: &DocId) -> bool {
        if self.roots.is_empty() {
            return true;
        }
        let mut current = Some(doc.clone());
        // The depth guard protects against a corrupted parent loop.
        for _ in 0..64 {
            let Some(d) = current else { return false };
            if self.roots.contains(&d) {
                return true;
            }
            current = ws.parent(&d);
        }
        false
    }
}
