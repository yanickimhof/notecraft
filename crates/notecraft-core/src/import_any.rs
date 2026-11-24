//! One entry point for every import: pick a file, a folder or a zip and NoteCraft works out what
//! it is (Evernote, Obsidian, Bear, Joplin, Apple Notes exports, Word, HTML, CSV, Markdown).

use crate::blocks;
use crate::import_docx::{import_docx, looks_like_docx};
use crate::import_enex::{import_enex, looks_like_enex};
use crate::import_export::{
    clean_title, import_markdown_file, import_markdown_folder_with, localize_assets, take_title, ImportReport,
};
use crate::import_html::{html_to_markdown, Res};
use crate::import_zip::{extract_zip, looks_like_zip};
use crate::workspace::Workspace;
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// What an import source turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Evernote,
    Obsidian,
    Bear,
    Joplin,
    Word,
    Html,
    Csv,
    Markdown,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Evernote => "Evernote",
            Source::Obsidian => "Obsidian",
            Source::Bear => "Bear",
            Source::Joplin => "Joplin",
            Source::Word => "Word",
            Source::Html => "HTML",
            Source::Csv => "CSV",
            Source::Markdown => "Markdown",
        }
    }
}

/// A scratch folder that disappears when dropped.
pub(crate) struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Result<Self> {
        let p = std::env::temp_dir().join(format!("notecraft-import-{}", crate::ids::new_id()));
        fs::create_dir_all(&p)?;
        Ok(Scratch(p))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn visible(p: &Path) -> bool {
    !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| visible(p)).collect()).unwrap_or_default();
    v.sort();
    v
}

fn has_ext_deep(dir: &Path, wanted: &[&str], depth: usize) -> bool {
    children(dir).iter().any(|p| {
        if p.is_dir() {
            depth < 6 && has_ext_deep(p, wanted, depth + 1)
        } else {
            wanted.contains(&ext(p).as_str())
        }
    })
}

/// Guess the source of a file or folder (a zip is looked at after it is unpacked).
pub fn detect(path: &Path) -> Result<Source> {
    if path.is_dir() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name.ends_with(".textbundle") || name.ends_with(".bearnote") {
            return Ok(Source::Bear);
        }
        if path.join(".obsidian").is_dir() {
            return Ok(Source::Obsidian);
        }
        if children(path).iter().any(|p| p.is_dir() && matches!(ext(p).as_str(), "textbundle" | "bearnote")) {
            return Ok(Source::Bear);
        }
        if path.join("_resources").is_dir()
            || children(path).iter().any(|p| p.is_dir() && p.join("_resources").is_dir())
        {
            return Ok(Source::Joplin);
        }
        if has_ext_deep(path, &["md", "markdown"], 0) {
            return Ok(Source::Markdown);
        }
        if has_ext_deep(path, &["enex"], 0) {
            return Ok(Source::Evernote);
        }
        if has_ext_deep(path, &["html", "htm"], 0) {
            return Ok(Source::Html);
        }
        if has_ext_deep(path, &["docx"], 0) {
            return Ok(Source::Word);
        }
        if has_ext_deep(path, &["csv"], 0) {
            return Ok(Source::Csv);
        }
        return Err(Error::Invalid(
            "no notes found in this folder (Markdown, HTML, Evernote, Word or CSV files)".into(),
        ));
    }
    if !path.is_file() {
        return Err(Error::Invalid(format!("{} does not exist", path.display())));
    }
    if looks_like_docx(path) {
        return Ok(Source::Word);
    }
    if looks_like_enex(path) {
        return Ok(Source::Evernote);
    }
    match ext(path).as_str() {
        "jex" => Ok(Source::Joplin),
        "html" | "htm" | "xhtml" => Ok(Source::Html),
        "csv" => Ok(Source::Csv),
        "md" | "markdown" | "txt" => Ok(Source::Markdown),
        _ => Err(Error::Invalid(format!("{}: unsupported format (Markdown, HTML, Word, Evernote, CSV, Obsidian, Bear and Joplin exports are supported)", path.display()))),
    }
}

// ---------------------------------------------------------------------------------------- Obsidian

fn percent_encode_path(p: &Path) -> String {
    let mut out = String::new();
    for c in p.to_string_lossy().chars() {
        match c {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '%' => out.push_str("%25"),
            '[' => out.push_str("%5B"),
            ']' => out.push_str("%5D"),
            c => out.push(c),
        }
    }
    out
}

fn index_files(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut m = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((d, depth)) = stack.pop() {
        for p in children(&d) {
            seen += 1;
            if seen > 200_000 {
                return m;
            }
            if p.is_dir() {
                if depth < 20 {
                    stack.push((p, depth + 1));
                }
            } else if !matches!(ext(&p).as_str(), "md" | "markdown") {
                if let Some(n) = p.file_name() {
                    m.entry(n.to_string_lossy().to_lowercase()).or_insert(p);
                }
            }
        }
    }
    m
}

fn callout_icon(kind: &str) -> &'static str {
    match kind {
        "warning" | "caution" | "attention" => "⚠️",
        "info" | "todo" => "ℹ️",
        "note" | "abstract" | "summary" | "tldr" => "📝",
        "danger" | "error" | "bug" | "failure" | "fail" | "missing" => "🚨",
        "success" | "check" | "done" => "✅",
        "question" | "help" | "faq" => "❓",
        "quote" | "cite" => "💬",
        "example" => "🧪",
        _ => "💡",
    }
}

/// Turn Obsidian syntax into plain Markdown that NoteCraft reads.
pub fn obsidian_to_markdown(text: &str, files: &BTreeMap<String, PathBuf>) -> String {
    // comments %% … %%
    let mut s = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(a) = rest.find("%%") {
        s.push_str(&rest[..a]);
        match rest[a + 2..].find("%%") {
            Some(b) => rest = &rest[a + 2 + b + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    s.push_str(rest);
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;
    for line in s.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            in_code = !in_code;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_code {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // callouts: `> [!tip] Title`
        let t = line.trim_start();
        if let Some(r) = t.strip_prefix("> [!").or_else(|| t.strip_prefix(">[!")) {
            if let Some(end) = r.find(']') {
                let kind = r[..end].trim_end_matches(['+', '-']).to_lowercase();
                let title = r[end + 1..].trim_start_matches(['+', '-']).trim();
                let indent = &line[..line.len() - t.len()];
                out.push_str(&format!("{indent}> [!NOTE]\n"));
                out.push_str(&format!(
                    "{indent}> {} {}\n",
                    callout_icon(&kind),
                    if title.is_empty() { &kind } else { title }
                ));
                continue;
            }
        }
        out.push_str(&links(line, files));
        out.push('\n');
    }
    out
}

fn links(line: &str, files: &BTreeMap<String, PathBuf>) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(a) = rest.find("[[") {
        let embed = rest[..a].ends_with('!');
        let before = if embed { &rest[..a - 1] } else { &rest[..a] };
        let Some(b) = rest[a + 2..].find("]]") else { break };
        let inner = &rest[a + 2..a + 2 + b];
        out.push_str(before);
        let (target, alias) = match inner.split_once('|') {
            Some((t, al)) => (t, Some(al)),
            None => (inner, None),
        };
        let page = target.split(['#', '^']).next().unwrap_or("").trim();
        let file_key = Path::new(page).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let is_file = Path::new(page).extension().is_some_and(|e| !e.eq_ignore_ascii_case("md"));
        if embed && is_file {
            match files.get(&file_key) {
                Some(p) => {
                    let image = matches!(ext(p).as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp");
                    let enc = percent_encode_path(p);
                    if image {
                        out.push_str(&format!("\n\n![]({enc})\n\n"));
                    } else {
                        out.push_str(&format!("\n\n[📎 {}]({enc})\n\n", p.file_name().unwrap().to_string_lossy()));
                    }
                }
                None => out.push_str(&format!("`missing file: {page}`")),
            }
        } else if page.is_empty() {
            out.push_str(alias.unwrap_or(target));
        } else {
            let page = page.strip_suffix(".md").unwrap_or(page);
            let page = page.rsplit('/').next().unwrap_or(page);
            out.push_str(&format!("[[{page}]]"));
        }
        rest = &rest[a + 2 + b + 2..];
    }
    out.push_str(rest);
    out
}

// -------------------------------------------------------------------------------------------- HTML

fn data_uri(src: &str) -> Option<(String, Vec<u8>)> {
    use base64::Engine;
    let rest = src.strip_prefix("data:")?;
    let (head, data) = rest.split_once(',')?;
    if !head.contains(";base64") {
        return None;
    }
    let mime = head.split(';').next().unwrap_or("").to_string();
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?;
    Some((mime, bytes))
}

fn mime_ext(m: &str) -> &'static str {
    match m {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

/// Convert one HTML document; local images are looked up under `base` and kept as absolute paths,
/// `data:` images are written into `scratch`.
fn convert_html(html: &str, base: &Path, scratch: &Path) -> (Option<String>, String) {
    let base = crate::import_export::canonical(base);
    let mut n = 0usize;
    let conv = html_to_markdown(html, &mut |tag, at| {
        let key = match tag {
            "img" => "src",
            "a" => "href",
            _ => return None,
        };
        let src = at.get(key)?.trim().to_string();
        if let Some((mime, bytes)) = data_uri(&src) {
            if tag == "a" || bytes.len() > 256 * 1024 * 1024 {
                return None;
            }
            n += 1;
            let name = format!("image-{n}.{}", mime_ext(&mime));
            let p = scratch.join(&name);
            fs::write(&p, bytes).ok()?;
            return Some(Res {
                path: percent_encode_path(&p),
                name: at.get("alt").cloned().unwrap_or_default(),
                image: true,
            });
        }
        if src.contains("://") || src.starts_with('#') || src.starts_with("mailto:") || src.starts_with("data:") {
            return None;
        }
        let rel = crate::import_export::percent_decode(src.split(['?', '#']).next().unwrap_or(""));
        let local = crate::import_export::canonical(&base.join(&rel));
        // only files that live in the folder of the page
        if !local.starts_with(&base) || !local.is_file() {
            return None;
        }
        let image = tag == "img" || matches!(ext(&local).as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp");
        if tag == "a" && ext(&local) == "html" {
            return None;
        }
        Some(Res {
            path: percent_encode_path(&local),
            name: if image {
                at.get("alt").cloned().unwrap_or_default()
            } else {
                local.file_name()?.to_string_lossy().to_string()
            },
            image,
        })
    });
    (conv.title, conv.markdown)
}

/// Import one `.html` file as a page.
pub fn import_html_file(ws: &mut Workspace, file: &Path, parent: Option<&str>) -> Result<ImportReport> {
    let bytes = fs::read(file)?;
    let html = String::from_utf8_lossy(&bytes).to_string();
    let scratch = Scratch::new()?;
    let (doc_title, md) = convert_html(&html, file.parent().unwrap_or(Path::new(".")), &scratch.0);
    let mut report = ImportReport::default();
    let stem = clean_title(&file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let mut title = doc_title.filter(|t| !t.trim().is_empty()).unwrap_or(stem);
    let mut blocks = blocks::from_markdown(&md);
    take_title(&mut blocks, &mut title, false);
    localize_assets(ws, &mut blocks, Path::new("/"), &mut report)?;
    let id = ws.create_page(parent, &title)?;
    ws.pages.get_mut(&id).unwrap().blocks = blocks;
    ws.save_page(&id)?;
    report.pages = 1;
    report.root_page = Some(id);
    Ok(report)
}

fn copy_tree(src: &Path, dst: &Path, depth: usize, count: &mut usize) -> Result<()> {
    fs::create_dir_all(dst)?;
    for p in children(src) {
        *count += 1;
        if *count > 300_000 || depth > 30 {
            return Ok(());
        }
        let Some(name) = p.file_name() else { continue };
        let target = dst.join(name);
        if p.is_dir() {
            copy_tree(&p, &target, depth + 1, count)?;
        } else if matches!(ext(&p).as_str(), "html" | "htm") {
            let html = String::from_utf8_lossy(&fs::read(&p)?).to_string();
            let (title, md) = convert_html(&html, p.parent().unwrap_or(Path::new(".")), dst);
            let md = html_links_to_md(&md);
            let head = title
                .filter(|t| !t.trim().is_empty())
                .map(|t| format!("---\ntitle: {}\n---\n\n", serde_json::to_string(&t).unwrap_or_default()))
                .unwrap_or_default();
            fs::write(target.with_extension("md"), format!("{head}{md}"))?;
        } else {
            fs::copy(&p, &target)?;
        }
    }
    Ok(())
}

/// `[x](Other%20page.html)` → `[x](Other%20page.md)` so page links are recognised.
fn html_links_to_md(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(i) = rest.find("](") {
        out.push_str(&rest[..i + 2]);
        rest = &rest[i + 2..];
        if let Some(e) = rest.find(')') {
            let target = &rest[..e];
            if !target.contains("://") {
                if let Some(t) = target.strip_suffix(".html").or_else(|| target.strip_suffix(".htm")) {
                    out.push_str(t);
                    out.push_str(".md");
                    rest = &rest[e..];
                    continue;
                }
            }
        }
    }
    out.push_str(rest);
    out
}

// ------------------------------------------------------------------------------------------ Joplin

struct Tar<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Tar<'a> {
    type Item = (String, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let h = self.data.get(self.pos..self.pos + 512)?;
            if h.iter().all(|b| *b == 0) {
                return None;
            }
            let field = |a: usize, b: usize| String::from_utf8_lossy(&h[a..b]).trim_matches(['\0', ' ']).to_string();
            let mut name = field(0, 100);
            let prefix = field(345, 500);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
            let size = usize::from_str_radix(field(124, 136).trim(), 8).ok()?;
            let kind = h[156];
            let start = self.pos + 512;
            let end = start.checked_add(size)?;
            let body = self.data.get(start..end)?;
            self.pos = start + size.div_ceil(512) * 512;
            if kind == b'0' || kind == 0 {
                return Some((name, body));
            }
        }
    }
}

/// Import a Joplin `.jex` archive (a tar of notes, notebooks and resources).
pub fn import_jex(ws: &mut Workspace, file: &Path, parent: Option<&str>, name: &str) -> Result<ImportReport> {
    let len = fs::metadata(file)?.len();
    if len > 4 * 1024 * 1024 * 1024 {
        return Err(Error::Invalid("this Joplin archive is larger than 4 GB".into()));
    }
    let data = fs::read(file)?;
    struct Item {
        id: String,
        parent: String,
        title: String,
        body: String,
        kind: u8,
        file_ext: String,
        created: String,
    }
    let mut items: Vec<Item> = Vec::new();
    let mut raw: BTreeMap<String, &[u8]> = BTreeMap::new();
    for (n, body) in (Tar { data: &data, pos: 0 }) {
        let n = n.trim_start_matches("./").to_string();
        if let Some(rid) = n.strip_prefix("resources/") {
            raw.insert(rid.split('.').next().unwrap_or("").to_string(), body);
        } else if n.ends_with(".md") && !n.contains('/') {
            let text = String::from_utf8_lossy(body).to_string();
            // body, blank line, then `key: value` lines; the first line is the title
            let (head, props) = match text.rfind("\n\nid: ") {
                Some(i) => (&text[..i], &text[i + 2..]),
                None => (text.as_str(), ""),
            };
            let get = |k: &str| {
                props.lines().find_map(|l| l.strip_prefix(&format!("{k}: "))).unwrap_or("").trim().to_string()
            };
            let (title, b) = match head.split_once("\n\n") {
                Some((t, b)) => (t.to_string(), b.to_string()),
                None => (head.to_string(), String::new()),
            };
            let kind: u8 = get("type_").parse().unwrap_or(0);
            if kind == 0 {
                continue;
            }
            items.push(Item {
                id: get("id"),
                parent: get("parent_id"),
                title,
                body: b,
                kind,
                file_ext: get("file_extension"),
                created: get("created_time"),
            });
        }
    }
    if items.is_empty() {
        return Err(Error::Invalid("no Joplin notes found in this archive".into()));
    }
    let mut report = ImportReport::default();
    let holder = ws.create_page(parent, name)?;
    report.root_page = Some(holder.clone());
    report.pages += 1;
    // notebooks first (they may nest), then notes
    let mut page_of: BTreeMap<String, String> = BTreeMap::new();
    let mut pending: Vec<&Item> = items.iter().filter(|i| i.kind == 2).collect();
    for _ in 0..30 {
        if pending.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for f in pending {
            let par = if f.parent.is_empty() { Some(holder.clone()) } else { page_of.get(&f.parent).cloned() };
            match par {
                Some(p) => {
                    let id = ws.create_page(Some(&p), if f.title.is_empty() { "Notebook" } else { &f.title })?;
                    page_of.insert(f.id.clone(), id);
                    report.pages += 1;
                }
                None => next.push(f),
            }
        }
        pending = next;
    }
    let scratch = Scratch::new()?;
    let mut assets: BTreeMap<String, String> = BTreeMap::new();
    for r in items.iter().filter(|i| i.kind == 4) {
        if let Some(bytes) = raw.get(&r.id) {
            let fname = format!(
                "{}{}",
                if r.title.is_empty() { &r.id } else { &r.title },
                if r.file_ext.is_empty() || r.title.ends_with(&format!(".{}", r.file_ext)) {
                    String::new()
                } else {
                    format!(".{}", r.file_ext)
                }
            );
            let dir = scratch.0.join(&r.id);
            fs::create_dir_all(&dir)?;
            let safe: String =
                Path::new(&fname).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| r.id.clone());
            let p = dir.join(safe);
            fs::write(&p, bytes)?;
            assets.insert(r.id.clone(), ws.add_asset(&p)?);
            report.assets += 1;
        }
    }
    for note in items.iter().filter(|i| i.kind == 1) {
        let par = page_of.get(&note.parent).cloned().unwrap_or_else(|| holder.clone());
        let mut md = note.body.clone();
        // `![alt](:/resourceid)` → the stored file, on a line of its own
        for (id, path) in &assets {
            let tag = format!("](:/{id})");
            while let Some(close) = md.find(&tag) {
                let open = md[..close].rfind('[').unwrap_or(close);
                let image = md[..open].ends_with('!');
                let start = if image { open - 1 } else { open };
                let label = md[open + 1..close].to_string();
                let repl = if image {
                    format!("\n\n![{label}]({path})\n\n")
                } else {
                    format!("\n\n[📎 {label}]({path})\n\n")
                };
                md.replace_range(start..close + tag.len(), &repl);
            }
        }
        let mut title = if note.title.is_empty() { "Untitled".to_string() } else { note.title.clone() };
        let mut bl = blocks::from_markdown(&md);
        take_title(&mut bl, &mut title, false);
        let id = ws.create_page(Some(&par), &title)?;
        let page = ws.pages.get_mut(&id).unwrap();
        page.blocks = bl;
        if note.created.len() >= 19 && note.created.contains('T') {
            page.meta.created = note.created.clone();
        }
        ws.save_page(&id)?;
        report.pages += 1;
    }
    Ok(report)
}

// ----------------------------------------------------------------------------------------- Driver

fn csv_report(c: crate::import_csv::CsvImport) -> ImportReport {
    ImportReport { databases: 1, rows: c.rows.len(), root_page: Some(c.db), ..Default::default() }
}

fn merge(into: &mut ImportReport, from: ImportReport) {
    into.pages += from.pages;
    into.assets += from.assets;
    into.databases += from.databases;
    into.rows += from.rows;
    into.skipped.extend(from.skipped);
    into.notes.extend(from.notes);
}

fn display_name(p: &Path) -> String {
    let n = p.file_stem().or_else(|| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let n = clean_title(&n);
    if n.is_empty() {
        "Import".to_string()
    } else {
        n
    }
}

/// A folder that holds a single sub-folder is usually the real root (zips do that).
fn descend(dir: &Path) -> PathBuf {
    let mut cur = dir.to_path_buf();
    for _ in 0..3 {
        let kids = children(&cur);
        if kids.len() == 1 && kids[0].is_dir() && !matches!(ext(&kids[0]).as_str(), "textbundle" | "bearnote") {
            cur = kids[0].clone();
        } else {
            break;
        }
    }
    cur
}

fn import_folder(
    ws: &mut Workspace,
    dir: &Path,
    parent: Option<&str>,
    name: &str,
    source: Source,
) -> Result<ImportReport> {
    match source {
        Source::Obsidian => {
            let files = index_files(dir);
            import_markdown_folder_with(ws, dir, parent, &|t, _| obsidian_to_markdown(t, &files))
        }
        Source::Markdown | Source::Joplin | Source::Bear => {
            import_markdown_folder_with(ws, dir, parent, &|t, _| t.to_string())
        }
        Source::Html => import_html_tree(ws, dir, parent),
        Source::Evernote | Source::Word | Source::Csv => {
            // a folder of files: import each one under a page named after the folder
            let holder = ws.create_page(parent, name)?;
            let mut report = ImportReport { pages: 1, root_page: Some(holder.clone()), ..Default::default() };
            let mut stack = vec![(dir.to_path_buf(), holder.clone(), 0usize)];
            while let Some((d, par, depth)) = stack.pop() {
                for p in children(&d) {
                    if p.is_dir() {
                        if depth < 10 {
                            let sub = ws.create_page(Some(&par), &display_name(&p))?;
                            report.pages += 1;
                            stack.push((p, sub, depth + 1));
                        }
                        continue;
                    }
                    let r = match ext(&p).as_str() {
                        "enex" => import_enex(ws, &p, Some(&par), &display_name(&p)),
                        "docx" => import_docx(ws, &p, Some(&par)),
                        "csv" => fs::read(&p).map_err(Error::from).and_then(|b| {
                            crate::import_csv::import_csv_database(
                                ws,
                                Some(&par),
                                &display_name(&p),
                                &String::from_utf8_lossy(&b),
                            )
                            .map(csv_report)
                        }),
                        _ => continue,
                    };
                    match r {
                        Ok(r) => merge(&mut report, r),
                        Err(e) => report.skipped.push(format!("{}: {e}", p.display())),
                    }
                }
            }
            Ok(report)
        }
    }
}

fn import_html_tree(ws: &mut Workspace, dir: &Path, parent: Option<&str>) -> Result<ImportReport> {
    let scratch = Scratch::new()?;
    let name = display_name(dir);
    let mirror = scratch.0.join(&name);
    let mut count = 0usize;
    copy_tree(dir, &mirror, 0, &mut count)?;
    import_markdown_folder_with(ws, &mirror, parent, &|t, _| t.to_string())
}

/// Import whatever `path` is. Returns what it was taken for and a report of the result.
pub fn import_any(ws: &mut Workspace, path: &Path, parent: Option<&str>) -> Result<(Source, ImportReport)> {
    let name = display_name(path);
    if path.is_file() && looks_like_zip(path) && !looks_like_docx(path) {
        let scratch = Scratch::new()?;
        extract_zip(path, &scratch.0)?;
        let root = descend(&scratch.0);
        let source = detect(&root)?;
        // a zip of a single folder is named after the folder inside, not after the zip file
        let name = if root == scratch.0 { name } else { display_name(&root) };
        let report = import_folder(ws, &root, parent, &name, source)?;
        return Ok((source, report));
    }
    let source = detect(path)?;
    let report = if path.is_dir() {
        match source {
            Source::Bear if matches!(ext(path).as_str(), "textbundle" | "bearnote") => {
                // the bundle itself: wrap it so the folder logic sees it as a child
                let scratch = Scratch::new()?;
                let wrapper = scratch.0.join(&name);
                fs::create_dir_all(&wrapper)?;
                let mut count = 0;
                copy_tree(path, &wrapper.join(path.file_name().unwrap()), 0, &mut count)?;
                import_markdown_folder_with(ws, &wrapper, parent, &|t, _| t.to_string())?
            }
            _ => import_folder(ws, path, parent, &name, source)?,
        }
    } else {
        match source {
            Source::Word => import_docx(ws, path, parent)?,
            Source::Evernote => import_enex(ws, path, parent, &name)?,
            Source::Html => import_html_file(ws, path, parent)?,
            Source::Joplin => import_jex(ws, path, parent, &name)?,
            Source::Csv => {
                let text = String::from_utf8_lossy(&fs::read(path)?).to_string();
                csv_report(crate::import_csv::import_csv_database(ws, parent, &name, &text)?)
            }
            _ => {
                let id = import_markdown_file(ws, path, parent)?;
                ImportReport { pages: 1, root_page: Some(id), ..Default::default() }
            }
        }
    };
    let mut report = report;
    write_report_page(ws, source, &mut report)?;
    Ok((source, report))
}

/// One line for a toast or the command line: `Obsidian: 42 pages, 3 databases (120 rows), 15 files`.
pub fn summary(source: Source, r: &ImportReport) -> String {
    let mut parts = vec![format!("{} page{}", r.pages, if r.pages == 1 { "" } else { "s" })];
    if r.databases > 0 {
        parts.push(format!("{} database{} ({} rows)", r.databases, if r.databases == 1 { "" } else { "s" }, r.rows));
    }
    if r.assets > 0 {
        parts.push(format!("{} file{}", r.assets, if r.assets == 1 { "" } else { "s" }));
    }
    let mut s = format!("{}: {}", source.label(), parts.join(", "));
    if !r.skipped.is_empty() {
        s.push_str(&format!(", {} skipped", r.skipped.len()));
    }
    if !r.notes.is_empty() {
        s.push_str(&format!(", {} remark{}", r.notes.len(), if r.notes.len() == 1 { "" } else { "s" }));
    }
    s
}

/// When something was skipped or imported with a caveat, say so on an "Import report" page next to the result.
fn write_report_page(ws: &mut Workspace, source: Source, r: &mut ImportReport) -> Result<()> {
    if r.skipped.is_empty() && r.notes.is_empty() {
        return Ok(());
    }
    let parent = r.root_page.clone().filter(|id| ws.pages.contains_key(id));
    let id = ws.create_page(parent.as_deref(), "Import report")?;
    let mut md = format!("{}\n\n", summary(source, r));
    let mut section = |title: &str, items: &[String]| {
        if !items.is_empty() {
            md.push_str(&format!("## {title}\n\n"));
            for i in items.iter().take(500) {
                md.push_str(&format!("- {}\n", i.replace('\n', " ")));
            }
            if items.len() > 500 {
                md.push_str(&format!("- … and {} more\n", items.len() - 500));
            }
            md.push('\n');
        }
    };
    section("Not imported", &r.skipped);
    section("Imported with a caveat", &r.notes);
    ws.pages.get_mut(&id).unwrap().blocks = blocks::from_markdown(&md);
    ws.save_page(&id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_zip::tests::make_zip;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let d = tempfile::tempdir().unwrap();
        let w = Workspace::open(d.path()).unwrap();
        (d, w)
    }

    #[test]
    fn an_import_done_through_a_second_handle_shows_up_in_the_open_workspace() {
        // the app imports on a worker thread with its own handle, then syncs from disk
        let (d, mut open) = ws();
        let src = tempfile::tempdir().unwrap();
        fs::write(src.path().join("note.md"), "# From elsewhere\n\nHello").unwrap();
        let root = d.path().to_path_buf();
        let path = src.path().join("note.md");
        std::thread::spawn(move || {
            let mut second = Workspace::open(&root).unwrap();
            import_any(&mut second, &path, None).unwrap();
        })
        .join()
        .unwrap();
        open.sync_from_disk().unwrap();
        assert!(open.pages.values().any(|p| p.meta.title == "From elsewhere"));
    }

    #[test]
    fn obsidian_syntax_becomes_plain_markdown() {
        let mut files = BTreeMap::new();
        files.insert("pic.png".to_string(), PathBuf::from("/vault/img/pic.png"));
        let md = "See [[Other note|alias]] and [[Plan#Goals]].\n![[pic.png|300]]\n![[Big Note]]\n%%hidden%%visible\n> [!tip] Remember\n> body\n```\n[[not a link]]\n```\n";
        let out = obsidian_to_markdown(md, &files);
        assert!(out.contains("[[Other note]]") && out.contains("[[Plan]]"), "{out}");
        assert!(out.contains("![](/vault/img/pic.png)"), "{out}");
        assert!(out.contains("[[Big Note]]"), "{out}");
        assert!(out.contains("visible") && !out.contains("hidden"), "{out}");
        assert!(out.contains("> [!NOTE]\n> 💡 Remember"), "{out}");
        assert!(out.contains("[[not a link]]"), "code stays as it is: {out}");
    }

    #[test]
    fn an_obsidian_vault_is_detected_and_imported() {
        let src = tempfile::tempdir().unwrap();
        let v = src.path().join("Vault");
        fs::create_dir_all(v.join(".obsidian")).unwrap();
        fs::create_dir_all(v.join("attachments")).unwrap();
        fs::write(v.join("attachments/pic.png"), b"\x89PNG fake").unwrap();
        fs::write(v.join("A.md"), "# A\n\nLink to [[B|bee]].\n\n![[pic.png]]\n").unwrap();
        fs::write(v.join("B.md"), "Body of B\n").unwrap();
        assert_eq!(detect(&v).unwrap(), Source::Obsidian);
        let (_d, mut w) = ws();
        let (s, r) = import_any(&mut w, &v, None).unwrap();
        assert_eq!(s, Source::Obsidian);
        assert_eq!(r.assets, 1, "{:?} {:?}", r.skipped, r.notes);
        let a = w.resolve_title("A").unwrap();
        let blocks = w.blocks(&a).unwrap();
        assert!(blocks.iter().any(|b| b.text.contains("[[B]]")));
        assert!(blocks
            .iter()
            .any(|b| matches!(&b.kind, blocks::BlockKind::Image { path } if path.starts_with("assets/"))));
    }

    #[test]
    fn a_standalone_html_page_keeps_title_images_and_links() {
        let src = tempfile::tempdir().unwrap();
        fs::write(src.path().join("pic.png"), b"\x89PNG fake").unwrap();
        let html = r#"<html><head><title>My page</title></head><body><h1>My page</h1><p>Hello <b>you</b> <a href="https://x.org">site</a></p>
<img src="pic.png" alt="a picture"><img src="data:image/png;base64,iVBORw0KGgo="><img src="../../etc/passwd"></body></html>"#;
        let f = src.path().join("page.html");
        fs::write(&f, html).unwrap();
        let (_d, mut w) = ws();
        let (s, r) = import_any(&mut w, &f, None).unwrap();
        assert_eq!(s, Source::Html);
        assert_eq!(r.assets, 2, "{:?}", r.notes);
        let id = w.resolve_title("My page").unwrap();
        let b = w.blocks(&id).unwrap();
        assert!(b.iter().any(|x| x.text.contains("**you**") && x.text.contains("[site](https://x.org)")));
    }

    #[test]
    fn a_zip_of_notes_is_unpacked_and_named_after_its_folder() {
        let src = tempfile::tempdir().unwrap();
        let z = src.path().join("export.zip");
        make_zip(&z, &[("Notes/One.md", b"# One\n\nfirst"), ("Notes/Two.md", b"second")]);
        let (_d, mut w) = ws();
        let (s, r) = import_any(&mut w, &z, None).unwrap();
        assert_eq!(s, Source::Markdown);
        assert!(r.pages >= 2, "{r:?}");
        assert!(w.resolve_title("One").is_some() && w.resolve_title("Two").is_some());
    }

    fn tar_entry(name: &str, body: &[u8]) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        let size = format!("{:011o}", body.len());
        h[124..135].copy_from_slice(size.as_bytes());
        h[156] = b'0';
        let mut v = h;
        v.extend_from_slice(body);
        v.resize(512 + body.len().div_ceil(512) * 512, 0);
        v
    }

    #[test]
    fn a_joplin_archive_keeps_notebooks_notes_and_resources() {
        let src = tempfile::tempdir().unwrap();
        let mut tar = Vec::new();
        tar.extend(tar_entry("nb1.md", b"Work\n\nid: nb1\nparent_id: \ntype_: 2"));
        tar.extend(tar_entry("n1.md", b"Plan\n\nSee ![img](:/r1)\n\nid: n1\nparent_id: nb1\ntype_: 1"));
        tar.extend(tar_entry("r1.md", b"pic.png\n\nid: r1\nfile_extension: png\ntype_: 4"));
        tar.extend(tar_entry("resources/r1.png", b"\x89PNG fake"));
        tar.extend(vec![0u8; 1024]);
        let f = src.path().join("Joplin.jex");
        fs::write(&f, tar).unwrap();
        let (_d, mut w) = ws();
        let (s, r) = import_any(&mut w, &f, None).unwrap();
        assert_eq!(s, Source::Joplin);
        assert_eq!(r.assets, 1, "{:?}", r.notes);
        let id = w.resolve_title("Plan").unwrap();
        let crate::workspace::DocId::Page(pid) = &id else { panic!("a page") };
        assert!(w.pages[pid].meta.parent.is_some());
        assert!(w.resolve_title("Work").is_some());
    }

    #[test]
    fn a_bear_bundle_comes_in_with_its_assets() {
        let src = tempfile::tempdir().unwrap();
        let b = src.path().join("Idea.textbundle");
        fs::create_dir_all(b.join("assets")).unwrap();
        fs::write(b.join("text.md"), "# Idea\n\n![](assets/x.png)\n").unwrap();
        fs::write(b.join("assets/x.png"), b"\x89PNG fake").unwrap();
        let (_d, mut w) = ws();
        let (s, r) = import_any(&mut w, &b, None).unwrap();
        assert_eq!(s, Source::Bear);
        assert_eq!(r.assets, 1, "{:?}", r.notes);
        assert!(w.resolve_title("Idea").is_some());
    }

    #[test]
    fn unsupported_and_empty_sources_are_refused_clearly() {
        let src = tempfile::tempdir().unwrap();
        let f = src.path().join("x.bin");
        fs::write(&f, "zz").unwrap();
        let (_d, mut w) = ws();
        assert!(import_any(&mut w, &f, None).is_err());
        assert!(import_any(&mut w, &src.path().join("missing"), None).is_err());
        let empty = src.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(import_any(&mut w, &empty, None).is_err());
    }
}
