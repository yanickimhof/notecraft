//! Import an Evernote export (`.enex`): one page per note, with its tags, dates, to-dos, tables,
//! images and attachments. Files of several hundred megabytes are read note by note.

use crate::blocks;
use crate::import_export::{take_title, ImportReport};
use crate::import_html::{html_to_markdown, Res};
use crate::workspace::Workspace;
use crate::{Error, Result};
use base64::Engine;
use md5::{Digest, Md5};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// A single note larger than this is refused (a note with a 300 MB video is not a note).
const MAX_NOTE: usize = 512 * 1024 * 1024;

/// True when the file starts like an Evernote export.
pub fn looks_like_enex(path: &Path) -> bool {
    let mut head = [0u8; 600];
    let n = File::open(path).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
    let s = String::from_utf8_lossy(&head[..n]);
    s.contains("<en-export") || (s.contains("<?xml") && s.contains("en-export"))
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() || from > hay.len() - needle.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

/// `20201231T101500Z` → `2020-12-31T10:15:00Z`
fn enex_date(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() >= 15 && s.as_bytes()[8] == b'T' && s[..8].chars().all(|c| c.is_ascii_digit()) {
        Some(format!("{}-{}-{}T{}:{}:{}Z", &s[0..4], &s[4..6], &s[6..8], &s[9..11], &s[11..13], &s[13..15]))
    } else {
        None
    }
}

fn ext_for(mime: &str) -> &'static str {
    match mime {
        "image/png" => ".png",
        "image/jpeg" | "image/jpg" => ".jpg",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "image/svg+xml" => ".svg",
        "image/heic" => ".heic",
        "application/pdf" => ".pdf",
        "audio/mpeg" => ".mp3",
        "audio/wav" | "audio/x-wav" => ".wav",
        "audio/amr" => ".amr",
        "video/mp4" => ".mp4",
        "text/plain" => ".txt",
        "application/msword" => ".doc",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => ".docx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => ".xlsx",
        _ => "",
    }
}

#[derive(Default)]
struct NoteData {
    title: String,
    created: Option<String>,
    updated: Option<String>,
    tags: Vec<String>,
    source_url: String,
    content: String,
    resources: Vec<(String, String, Vec<u8>)>, // mime, file name, bytes
}

fn child_text(n: roxmltree::Node, name: &str) -> String {
    n.children().find(|c| c.has_tag_name(name)).map(|c| c.text().unwrap_or("").trim().to_string()).unwrap_or_default()
}

fn parse_note(chunk: &str) -> Result<NoteData> {
    let doc = roxmltree::Document::parse(chunk).map_err(|e| Error::Invalid(format!("damaged note: {e}")))?;
    let note = doc.root_element();
    let mut d = NoteData { title: child_text(note, "title"), ..Default::default() };
    d.created = enex_date(&child_text(note, "created"));
    d.updated = enex_date(&child_text(note, "updated"));
    for c in note.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "tag" => {
                if let Some(t) = c.text() {
                    d.tags.push(t.trim().to_string());
                }
            }
            "content" => d.content = c.text().unwrap_or("").to_string(),
            "note-attributes" => d.source_url = child_text(c, "source-url"),
            "resource" => {
                let mime = child_text(c, "mime");
                let data = c.children().find(|x| x.has_tag_name("data")).and_then(|x| x.text()).unwrap_or("");
                let compact: String = data.chars().filter(|ch| !ch.is_whitespace()).collect();
                let bytes = base64::engine::general_purpose::STANDARD.decode(compact.as_bytes()).unwrap_or_default();
                let name = c
                    .children()
                    .find(|x| x.has_tag_name("resource-attributes"))
                    .map(|a| child_text(a, "file-name"))
                    .unwrap_or_default();
                if !bytes.is_empty() {
                    d.resources.push((mime, name, bytes));
                }
            }
            _ => {}
        }
    }
    Ok(d)
}

fn import_note(ws: &mut Workspace, parent: &str, d: NoteData, report: &mut ImportReport) -> Result<String> {
    // store attachments, keyed by the MD5 that <en-media hash="…"> uses
    let mut by_hash: BTreeMap<String, Res> = BTreeMap::new();
    for (i, (mime, name, bytes)) in d.resources.iter().enumerate() {
        let mut file = name.trim().to_string();
        if file.is_empty() {
            file = format!("attachment-{}{}", i + 1, ext_for(mime));
        }
        let path = ws.add_asset_bytes(&file, bytes)?;
        report.assets += 1;
        let hash: String = Md5::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
        by_hash.insert(hash, Res { path, name: file, image: mime.starts_with("image/") && mime != "image/svg+xml" });
    }
    let conv = html_to_markdown(&d.content, &mut |tag, at| {
        if tag == "en-media" {
            let h = at.get("hash")?.to_lowercase();
            return by_hash.get(&h).cloned();
        }
        None
    });
    let mut title = if d.title.trim().is_empty() {
        conv.title.clone().unwrap_or_else(|| "Untitled".into())
    } else {
        d.title.trim().to_string()
    };
    let mut blocks = blocks::from_markdown(&conv.markdown);
    take_title(&mut blocks, &mut title, false);
    // a small line with what Evernote knew about the note
    let mut meta: Vec<String> = Vec::new();
    if !d.tags.is_empty() {
        meta.push(format!("Tags: {}", d.tags.join(", ")));
    }
    if let Some(c) = d.created.as_ref() {
        meta.push(format!("Created {}", &c[..10]));
    }
    if !d.source_url.is_empty() {
        meta.push(format!("Source: {}", d.source_url));
    }
    if !meta.is_empty() {
        let line = format!("*{}*", meta.join(" · "));
        blocks.insert(0, blocks::Block::new(blocks::BlockKind::Paragraph, &line));
    }
    let id = ws.create_page(Some(parent), &title)?;
    let page = ws.pages.get_mut(&id).unwrap();
    page.blocks = blocks;
    if let Some(c) = d.created {
        page.meta.created = c;
    }
    ws.save_page(&id)?;
    Ok(id)
}

/// Import an `.enex` file under `parent`; its notes go under a page named `name` (the notebook).
pub fn import_enex(ws: &mut Workspace, file: &Path, parent: Option<&str>, name: &str) -> Result<ImportReport> {
    let mut f = File::open(file)?;
    let mut report = ImportReport::default();
    let holder = ws.create_page(parent, name)?;
    report.root_page = Some(holder.clone());
    report.pages += 1;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 1 << 20];
    let mut eof = false;
    let mut count = 0usize;
    loop {
        // read until a whole note is in the buffer
        let (start, end) = loop {
            if let Some(s) = find(&buf, b"<note>", 0).or_else(|| find(&buf, b"<note ", 0)) {
                if let Some(e) = find(&buf, b"</note>", s) {
                    break (s, e + 7);
                }
            }
            if eof {
                // no more notes: whatever is left is not one
                buf.clear();
                break (0, 0);
            }
            if buf.len() > MAX_NOTE {
                return Err(Error::Invalid("a note in this file is larger than 512 MB".into()));
            }
            let n = f.read(&mut chunk)?;
            if n == 0 {
                eof = true;
            } else {
                buf.extend_from_slice(&chunk[..n]);
            }
        };
        if end == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&buf[start..end]).to_string();
        buf.drain(..end);
        count += 1;
        match parse_note(&text).and_then(|d| import_note(ws, &holder, d, &mut report)) {
            Ok(_) => report.pages += 1,
            Err(e) => report.skipped.push(format!("note {count}: {e}")),
        }
    }
    if count == 0 {
        return Err(Error::Invalid("no note found in this file: is it an Evernote export (.enex)?".into()));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake image bytes";

    fn sample() -> String {
        let b64 = base64::engine::general_purpose::STANDARD.encode(PNG);
        let hash: String = Md5::digest(PNG).iter().map(|b| format!("{b:02x}")).collect();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE en-export SYSTEM "http://xml.evernote.com/pub/evernote-export4.dtd">
<en-export export-date="20260101T000000Z" application="Evernote" version="10">
<note><title>Groceries</title><created>20201231T101500Z</created><updated>20210101T000000Z</updated>
<tag>home</tag><tag>weekly</tag>
<note-attributes><source-url>https://shop.example/list</source-url></note-attributes>
<content><![CDATA[<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE en-note SYSTEM "http://xml.evernote.com/pub/enml2.dtd">
<en-note><div><b>Buy</b> today</div><div><en-todo checked="true"/>Milk</div><div><en-todo/>Eggs</div><div><br/></div><div><en-media type="image/png" hash="{hash}"/></div><ul><li>Apples</li></ul></en-note>]]></content>
<resource><data encoding="base64">{b64}</data><mime>image/png</mime><resource-attributes><file-name>photo.png</file-name></resource-attributes></resource>
</note>
<note><title>Second</title><content><![CDATA[<en-note>Plain text</en-note>]]></content></note>
</en-export>"#
        )
    }

    #[test]
    fn notes_come_in_with_tags_todos_images_and_dates() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("My Notebook.enex");
        fs::write(&f, sample()).unwrap();
        assert!(looks_like_enex(&f));
        let wsd = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wsd.path()).unwrap();
        let r = import_enex(&mut ws, &f, None, "My Notebook").unwrap();
        assert_eq!(r.pages, 3, "{:?}", r.skipped);
        assert_eq!(r.assets, 1);
        let id = ws.resolve_title("Groceries").unwrap();
        let b = ws.blocks(&id).unwrap();
        let texts: Vec<String> = b.iter().map(|x| x.text.clone()).collect();
        assert!(
            texts.iter().any(|t| t.contains("Tags: home, weekly") && t.contains("Created 2020-12-31")),
            "{texts:?}"
        );
        assert!(
            b.iter().any(|x| matches!(x.kind, blocks::BlockKind::Todo { checked: true }) && x.text == "Milk"),
            "{b:?}"
        );
        assert!(b.iter().any(|x| matches!(x.kind, blocks::BlockKind::Todo { checked: false }) && x.text == "Eggs"));
        assert!(b
            .iter()
            .any(|x| matches!(&x.kind, blocks::BlockKind::Image { path } if path.starts_with("assets/photo"))));
        assert!(b.iter().any(|x| x.kind == blocks::BlockKind::Bulleted && x.text == "Apples"));
        assert!(ws.resolve_title("Second").is_some());
    }

    #[test]
    fn dates_and_a_non_enex_file() {
        assert_eq!(enex_date("20201231T101500Z").as_deref(), Some("2020-12-31T10:15:00Z"));
        assert_eq!(enex_date("nope"), None);
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x.enex");
        fs::write(&f, "<html>hello</html>").unwrap();
        assert!(!looks_like_enex(&f));
        let wsd = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wsd.path()).unwrap();
        assert!(import_enex(&mut ws, &f, None, "x").is_err());
    }

    #[test]
    fn a_damaged_note_is_skipped_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("a.enex");
        fs::write(&f, "<en-export><note><title>Bad</note><note><title>Good</title><content><![CDATA[<en-note>ok</en-note>]]></content></note></en-export>").unwrap();
        let wsd = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wsd.path()).unwrap();
        let r = import_enex(&mut ws, &f, None, "a").unwrap();
        assert!(ws.resolve_title("Good").is_some());
        assert_eq!(r.skipped.len(), 1);
    }
}
