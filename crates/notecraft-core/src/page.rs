//! Pages: one Markdown file each, with YAML front matter.

use crate::blocks::{self, Block};
use crate::vault::VaultKey;
use crate::{frontmatter, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Front matter of a page file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PageMeta {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// `gradient:<n>` or a relative path such as `assets/cover.jpg`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cover: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    /// Parent page id (or database id for nothing; `None` = top level).
    #[serde(default)]
    pub parent: Option<String>,
    /// Position among siblings in the sidebar.
    #[serde(default)]
    pub order: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    /// When set, the page is in the trash (the date it was trashed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed: Option<String>,
    /// Protected against edits, moves and deletion (it also protects everything inside it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    /// The body is encrypted with the vault passphrase (see [`crate::vault`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub encrypted: bool,
    /// Local date and time to be reminded about this page, `YYYY-MM-DD HH:MM`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reminder: String,
    /// Typeface of the body: empty (default), `serif` or `mono`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub font: String,
    /// Smaller body text.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub small_text: bool,
    /// Use the whole width of the window instead of a reading column.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub full_width: bool,
    /// Out of the sidebar but not in the trash; the page and what is inside it stay intact.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub meta: PageMeta,
    pub blocks: Vec<Block>,
    /// File the page was loaded from (absolute); `None` until first save.
    pub path: Option<PathBuf>,
    /// The encrypted body as stored in the file (only for encrypted pages).
    pub cipher: Option<String>,
    /// Key of an unlocked encrypted page; `None` while it is locked.
    pub key: Option<VaultKey>,
}

impl Page {
    /// An encrypted page whose passphrase has not been entered yet (its blocks are empty).
    pub fn is_locked(&self) -> bool {
        self.meta.encrypted && self.key.is_none()
    }

    pub fn new(id: String, title: &str, parent: Option<String>) -> Self {
        let now = crate::now_string();
        Page {
            meta: PageMeta {
                id,
                title: title.to_string(),
                created: now.clone(),
                updated: now,
                parent,
                ..Default::default()
            },
            blocks: Vec::new(),
            path: None,
            cipher: None,
            key: None,
        }
    }

    /// Parse a page file. Files without front matter get a fresh id and use
    /// `fallback_title`.
    pub fn parse(text: &str, fallback_title: &str) -> Result<Page> {
        let (yaml, body) = frontmatter::split(text);
        let mut meta: PageMeta = match yaml {
            Some(y) if !y.trim().is_empty() => serde_yaml_ng::from_str(y)?,
            _ => PageMeta::default(),
        };
        if meta.id.is_empty() {
            meta.id = crate::ids::new_id();
        }
        if meta.title.is_empty() {
            meta.title = fallback_title.to_string();
        }
        if meta.created.is_empty() {
            meta.created = crate::now_string();
        }
        if meta.updated.is_empty() {
            meta.updated = meta.created.clone();
        }
        if meta.encrypted {
            return Ok(Page { meta, blocks: Vec::new(), path: None, cipher: Some(body.to_string()), key: None });
        }
        Ok(Page { meta, blocks: blocks::from_markdown(body), path: None, cipher: None, key: None })
    }

    /// Render the page file text.
    pub fn render(&self) -> Result<String> {
        let yaml = format!("format: {}\n{}", crate::FORMAT_VERSION, serde_yaml_ng::to_string(&self.meta)?);
        let body = if self.meta.encrypted {
            match &self.key {
                Some(key) => key.seal(&blocks::to_markdown(&self.blocks))?,
                None => self.cipher.clone().unwrap_or_default(),
            }
        } else {
            blocks::to_markdown(&self.blocks)
        };
        Ok(frontmatter::join(&yaml, &body))
    }

    pub fn touch(&mut self) {
        self.meta.updated = crate::now_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::BlockKind;

    #[test]
    fn page_round_trip() {
        let mut p = Page::new("abc".into(), "Hello: world", Some("parent1".into()));
        p.meta.icon = "📘".into();
        p.blocks.push(Block::new(BlockKind::Heading(1), "Head"));
        p.blocks.push(Block::paragraph("Body with [[Other]]"));
        let text = p.render().unwrap();
        assert!(text.starts_with("---\nformat: 1\nid: abc\n"));
        let back = Page::parse(&text, "x").unwrap();
        assert_eq!(back.meta, p.meta);
        assert_eq!(back.blocks.len(), 2);
        assert_eq!(back.blocks[1].text, "Body with [[Other]]");
    }

    #[test]
    fn plain_markdown_gets_defaults() {
        let p = Page::parse("Just text", "Fallback").unwrap();
        assert_eq!(p.meta.title, "Fallback");
        assert!(!p.meta.id.is_empty());
    }
}
