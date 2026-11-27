//! Import a folder of Markdown files; export pages, databases or the whole
//! workspace to plain Markdown (no front matter, readable anywhere).

use crate::blocks::{self, Block, BlockKind};
use crate::database::PropKind;
use crate::frontmatter;
use crate::fsutil::{atomic_write, safe_file_name};
use crate::workspace::{DocId, Workspace};
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImportReport {
    pub pages: usize,
    pub assets: usize,
    pub skipped: Vec<String>,
    /// Id of the top page created for the folder.
    pub root_page: Option<String>,
    /// Databases created (a CSV table or a database export).
    pub databases: usize,
    /// Rows created in those databases.
    pub rows: usize,
    /// Things that were imported with a caveat, in plain words.
    pub notes: Vec<String>,
}

/// Exported names may end with a 32-hex id: `Notes 1a2b…`. Drop it.
pub fn clean_title(name: &str) -> String {
    let name = name.trim();
    if let Some((head, tail)) = name.rsplit_once(' ') {
        if tail.len() == 32 && tail.chars().all(|c| c.is_ascii_hexdigit()) {
            return head.trim().to_string();
        }
    }
    name.to_string()
}

pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn percent_encode_path(s: &str) -> String {
    s.replace('%', "%25").replace(' ', "%20")
}

/// Import every `.md` file under `src` as pages under `parent` (or a new
/// top-level page named after the folder when `parent` is `None`). Folders
/// become parent pages; a `Name.md` next to a `Name/` folder becomes that
/// folder's page. Local images and files are copied into `assets/`, and links
/// to other imported `.md` files become `[[mentions]]`.
pub fn import_markdown_folder(ws: &mut Workspace, src: &Path, parent: Option<&str>) -> Result<ImportReport> {
    import_markdown_folder_with(ws, src, parent, &|text, _| text.to_string())
}

/// Hook that rewrites a file's text (given its path) before it is read as Markdown.
pub type Prepare<'a> = dyn Fn(&str, &Path) -> String + 'a;

/// Like [`import_markdown_folder`], with `pre` applied to every file's text first (Obsidian, Bear…).
pub fn import_markdown_folder_with(
    ws: &mut Workspace,
    src: &Path,
    parent: Option<&str>,
    pre: &Prepare,
) -> Result<ImportReport> {
    if !src.is_dir() {
        return Err(Error::Invalid(format!("{} is not a folder", src.display())));
    }
    let mut report = ImportReport::default();
    let folder_title = clean_title(&src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    let root = ws.create_page(parent, &folder_title)?;
    report.root_page = Some(root.clone());
    report.pages += 1;
    let mut created: Vec<(String, PathBuf)> = Vec::new();
    let mut titles: BTreeMap<PathBuf, String> = BTreeMap::new();
    import_dir(ws, src, &root, &mut report, &mut created, &mut titles, pre)?;
    // second pass: links to imported files become mentions
    for (id, file) in &created {
        let dir = file.parent().unwrap_or(src).to_path_buf();
        let page = ws.pages.get_mut(id).unwrap();
        let mut changed = false;
        for b in page.blocks.iter_mut() {
            if b.kind.has_rich_text() {
                if let Some(t) = rewrite_md_links(&b.text, &dir, &titles) {
                    b.text = t;
                    changed = true;
                }
            }
        }
        if changed {
            ws.save_page(id)?;
        }
    }
    Ok(report)
}

fn import_dir(
    ws: &mut Workspace,
    dir: &Path,
    parent: &str,
    report: &mut ImportReport,
    created: &mut Vec<(String, PathBuf)>,
    titles: &mut BTreeMap<PathBuf, String>,
    pre: &Prepare,
) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .collect();
    entries.sort();
    let mut md_pages: BTreeMap<String, String> = BTreeMap::new(); // stem → page id
    for p in entries.iter().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))) {
        match import_file(ws, p, parent, report, pre) {
            Ok(id) => {
                titles.insert(canonical(p), ws.pages[&id].meta.title.clone());
                let stem = p.file_stem().unwrap().to_string_lossy().to_string();
                md_pages.insert(stem, id.clone());
                created.push((id, p.clone()));
                report.pages += 1;
            }
            Err(e) => report.skipped.push(format!("{}: {e}", p.display())),
        }
    }
    for p in entries.iter().filter(|p| p.is_dir()) {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name.to_lowercase().ends_with(".textbundle") || name.to_lowercase().ends_with(".bearnote") {
            // Bear: the note is text.md (or text.markdown) next to its assets/ folder
            let note = ["text.md", "text.markdown", "text.txt"].iter().map(|f| p.join(f)).find(|f| f.is_file());
            if let Some(f) = note {
                let stem = clean_title(&name[..name.rfind('.').unwrap_or(name.len())]);
                let text = String::from_utf8_lossy(&fs::read(&f)?).to_string();
                match import_text(ws, &pre(&text, &f), &stem, p, parent, report) {
                    Ok(id) => {
                        created.push((id, f.clone()));
                        report.pages += 1;
                    }
                    Err(e) => report.skipped.push(format!("{}: {e}", f.display())),
                }
            }
            continue;
        }
        if !contains_markdown(p) {
            continue;
        }
        let pid = match md_pages.get(&name) {
            Some(id) => id.clone(),
            None => {
                let id = ws.create_page(Some(parent), &clean_title(&name))?;
                report.pages += 1;
                id
            }
        };
        import_dir(ws, p, &pid, report, created, titles, pre)?;
    }
    Ok(())
}

/// Import one Markdown file as a page under `parent` (or at the top level).
/// Local images and files it references are copied into `assets/`.
pub fn import_markdown_file(ws: &mut Workspace, file: &Path, parent: Option<&str>) -> Result<String> {
    if !file.is_file() {
        return Err(Error::Invalid(format!("{} is not a file", file.display())));
    }
    let mut report = ImportReport::default();
    match parent {
        Some(p) => import_file(ws, file, p, &mut report, &|t, _| t.to_string()),
        None => {
            // import_file needs a parent; create at the top level, then lift it
            let tmp = ws.create_page(None, "import")?;
            let id = import_file(ws, file, &tmp, &mut report, &|t, _| t.to_string())?;
            if let Some(p) = ws.pages.get_mut(&id) {
                p.meta.parent = None;
            }
            ws.save_page(&id)?;
            ws.delete_forever(&crate::workspace::DocId::Page(tmp))?;
            Ok(id)
        }
    }
}

fn contains_markdown(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok()).any(|e| {
                let p = e.path();
                p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")) || (p.is_dir() && contains_markdown(&p))
            })
        })
        .unwrap_or(false)
}

pub(crate) fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn import_file(
    ws: &mut Workspace,
    path: &Path,
    parent: &str,
    report: &mut ImportReport,
    pre: &Prepare,
) -> Result<String> {
    let text = String::from_utf8_lossy(&fs::read(path)?).to_string();
    let text = pre(&text, path);
    let stem = clean_title(&path.file_stem().unwrap_or_default().to_string_lossy());
    import_text(ws, &text, &stem, path.parent().unwrap_or(Path::new(".")), parent, report)
}

/// Split a leading `# Title` off imported blocks when it repeats the page title.
pub(crate) fn take_title(blocks: &mut Vec<Block>, title: &mut String, trust_first: bool) {
    if let Some(first) = blocks.first() {
        if first.kind == BlockKind::Heading(1) && (trust_first || first.text.trim() == *title) {
            *title = first.text.trim().to_string();
            blocks.remove(0);
        }
    }
}

/// Page formats an exporter links to; they are pages, not attachments.
const PAGE_EXTS: [&str; 5] = ["md", "markdown", "csv", "html", "htm"];

/// The relative path of a link that looks like an attachment (a file name with an extension that is not a page).
/// Absolute paths and `..` are refused: an export must not be able to pull files from elsewhere on the disk.
fn attachment_path(target: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() || target.contains("://") || target.starts_with('#') || target.starts_with("mailto:") {
        return None;
    }
    let rel = percent_decode(target.split(['#', '?']).next().unwrap_or(""));
    let rel_path = Path::new(&rel);
    if rel.is_empty()
        || rel_path.is_absolute()
        || rel_path.components().any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    let ext = rel_path.extension().map(|e| e.to_string_lossy().to_lowercase())?;
    if PAGE_EXTS.contains(&ext.as_str()) || ext.len() > 6 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(rel)
}

/// The attachment a link points to, when it is a file next to the page.
fn attachment_for(dir: &Path, target: &str) -> Option<PathBuf> {
    let p = dir.join(attachment_path(target)?);
    p.is_file().then_some(p)
}

/// A link that looks like an attachment but whose file is not in the export.
fn missing_attachment(dir: &Path, target: &str) -> Option<String> {
    let rel = attachment_path(target)?;
    (!dir.join(&rel).is_file()).then_some(rel)
}

/// `[name](target)` alone in a paragraph.
fn lone_link(text: &str) -> Option<(String, String)> {
    let t = text.trim();
    if t.starts_with("[[") {
        return None;
    }
    let rest = t.strip_prefix('[')?;
    let close = rest.find("](")?;
    let name = &rest[..close];
    if name.contains(['[', ']']) {
        return None;
    }
    let target = rest[close + 2..].strip_suffix(')')?;
    let target = target.trim().trim_start_matches('<').trim_end_matches('>');
    (!target.is_empty() && !target.contains(')')).then(|| (name.to_string(), target.to_string()))
}

/// A line that is only a link to a file (a PDF, a document, a video) is a file block, so it gets copied.
fn promote_local_files(blocks: &mut [Block], dir: &Path) {
    for b in blocks.iter_mut() {
        if b.kind != BlockKind::Paragraph {
            continue;
        }
        if let Some((name, target)) = lone_link(&b.text) {
            if attachment_for(dir, &target).is_some() {
                b.kind = BlockKind::File { path: target };
                b.text = name.trim_start_matches('📎').trim().to_string();
            }
        }
    }
}

/// Report the links to files that are not in the export (the link itself stays in the text).
fn report_missing_files(blocks: &[Block], dir: &Path, report: &mut ImportReport) {
    for b in blocks {
        if !b.kind.has_rich_text() {
            continue;
        }
        let mut rest = b.text.as_str();
        while let Some(pos) = rest.find("](") {
            let tail = rest[pos + 2..].trim_start_matches('<');
            let end = tail.find([')', '>']).unwrap_or(tail.len());
            if let Some(rel) = missing_attachment(dir, &tail[..end]) {
                let note = format!("File not found in the export: {rel}");
                if !report.notes.contains(&note) {
                    report.notes.push(note);
                }
            }
            rest = &tail[end..];
        }
    }
}

/// Files linked from inside a sentence are copied too, and the link now points at the copy.
fn rewrite_inline_files(ws: &mut Workspace, b: &mut Block, dir: &Path, report: &mut ImportReport) -> Result<()> {
    if !b.text.contains("](") {
        return Ok(());
    }
    let text = std::mem::take(&mut b.text);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(pos) = rest.find("](") {
        let (head, tail) = rest.split_at(pos + 2);
        out.push_str(head);
        let wrapped = tail.starts_with('<');
        let (target, after) = if wrapped {
            match tail[1..].find(">)") {
                Some(e) => (&tail[1..1 + e], &tail[1 + e + 2..]),
                None => {
                    rest = tail;
                    continue;
                }
            }
        } else {
            match tail.find(')') {
                Some(e) => (&tail[..e], &tail[e + 1..]),
                None => {
                    rest = tail;
                    continue;
                }
            }
        };
        match attachment_for(dir, target) {
            Some(file) => {
                let rel = ws.add_asset(&file)?;
                report.assets += 1;
                if rel.contains(' ') {
                    out.push_str(&format!("<{rel}>)"));
                } else {
                    out.push_str(&format!("{rel})"));
                }
            }
            None if wrapped => out.push_str(&format!("<{target}>)")),
            None => out.push_str(&format!("{target})")),
        }
        rest = after;
    }
    out.push_str(rest);
    b.text = out;
    Ok(())
}

/// Copy the local images and files that `blocks` point to (relative to `dir`) into the workspace.
/// Links to files inside sentences and lines made only of a file link are copied as well; remote
/// images and files stay links, and the report says so.
pub(crate) fn localize_assets(
    ws: &mut Workspace,
    blocks: &mut [Block],
    dir: &Path,
    report: &mut ImportReport,
) -> Result<()> {
    let can_look = dir != Path::new("/");
    if can_look {
        promote_local_files(blocks, dir);
        report_missing_files(blocks, dir, report);
    }
    for b in blocks.iter_mut() {
        if let BlockKind::Image { path } | BlockKind::File { path } = &mut b.kind {
            if path.contains("://") {
                let shown = path.split(['?', '#']).next().unwrap_or(path);
                report.notes.push(format!("Not copied, still a link to the web (it may stop working): {shown}"));
                continue;
            }
            let local = dir.join(percent_decode(path));
            if local.is_file() {
                *path = ws.add_asset(&local)?;
                report.assets += 1;
            } else {
                report.notes.push(format!("File not found in the export: {}", percent_decode(path)));
            }
            continue;
        }
        if can_look && b.kind.has_rich_text() {
            rewrite_inline_files(ws, b, dir, report)?;
        }
    }
    Ok(())
}

/// Create a page from Markdown `text` named `stem`, with its attachments looked up in `dir`.
pub(crate) fn import_text(
    ws: &mut Workspace,
    text: &str,
    stem: &str,
    dir: &Path,
    parent: &str,
    report: &mut ImportReport,
) -> Result<String> {
    let (yaml, body) = frontmatter::split(text);
    let mut title = stem.to_string();
    let mut icon = String::new();
    if let Some(y) = yaml {
        if let Ok(v) = serde_yaml_ng::from_str::<serde_json::Value>(y) {
            if let Some(t) = v.get("title").and_then(|t| t.as_str()) {
                title = t.to_string();
            }
            if let Some(i) = v.get("icon").and_then(|t| t.as_str()) {
                icon = i.to_string();
            }
        }
    }
    // rich Markdown writes tags (`<callout>`, `<table>`…): turn them into what we read first.
    let converted;
    let body = if crate::import_rich_md::looks_like_rich_markdown(body) {
        let (md, notes) = crate::import_rich_md::convert(body);
        report.notes.extend(notes);
        converted = md;
        converted.as_str()
    } else {
        body
    };
    let mut blocks = blocks::from_markdown(body);
    // A leading `# Title` that repeats the title is the page title, not content.
    take_title(&mut blocks, &mut title, yaml.is_none());
    localize_assets(ws, &mut blocks, dir, report)?;
    let id = ws.create_page(Some(parent), &title)?;
    let page = ws.pages.get_mut(&id).unwrap();
    page.meta.icon = icon;
    page.blocks = blocks;
    ws.save_page(&id)?;
    Ok(id)
}

/// A `<page url=…>Title</page>` tag becomes a link to the page of the same name.
pub(crate) fn resolve_page_refs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("<page url=") {
        let Some(open_end) = rest[i..].find('>').map(|e| i + e + 1) else { break };
        let Some(close) = rest[open_end..].find("</page>").map(|e| open_end + e) else { break };
        let title = rest[open_end..close].trim();
        out.push_str(&rest[..i]);
        if title.is_empty() {
            out.push_str(&rest[i..close + "</page>".len()]);
        } else {
            out.push_str(&format!("[[{title}]]"));
        }
        rest = &rest[close + "</page>".len()..];
    }
    out.push_str(rest);
    out
}

/// Replace the body of an existing page with Markdown (rich or plain). Files named in
/// the text are looked up in `dir` and copied into the workspace. The page keeps its id, parent,
/// icon, cover and children, so links and sub-pages stay intact.
pub fn replace_page_body(ws: &mut Workspace, id: &str, text: &str, dir: &Path) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let text = resolve_page_refs(text);
    let body = if crate::import_rich_md::looks_like_rich_markdown(&text) {
        let (md, notes) = crate::import_rich_md::convert(&text);
        report.notes.extend(notes);
        md
    } else {
        text
    };
    let mut blocks = blocks::from_markdown(&body);
    let mut title = ws.pages.get(id).ok_or_else(|| Error::NotFound(id.to_string()))?.meta.title.clone();
    let kept = title.clone();
    take_title(&mut blocks, &mut title, false);
    localize_assets(ws, &mut blocks, dir, &mut report)?;
    let page = ws.pages.get_mut(id).ok_or_else(|| Error::NotFound(id.to_string()))?;
    page.meta.title = kept;
    page.blocks = blocks;
    page.touch();
    ws.save_page(id)?;
    report.pages = 1;
    Ok(report)
}

/// Same as [`replace_page_body`], for a database row.
pub fn replace_row_body(ws: &mut Workspace, db: &str, row: &str, text: &str, dir: &Path) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let text = resolve_page_refs(text);
    let body = if crate::import_rich_md::looks_like_rich_markdown(&text) {
        let (md, notes) = crate::import_rich_md::convert(&text);
        report.notes.extend(notes);
        md
    } else {
        text
    };
    let mut blocks = blocks::from_markdown(&body);
    let mut title = ws
        .dbs
        .get(db)
        .and_then(|d| d.rows.get(row))
        .ok_or_else(|| Error::NotFound(row.to_string()))?
        .meta
        .title
        .clone();
    let kept = title.clone();
    take_title(&mut blocks, &mut title, false);
    localize_assets(ws, &mut blocks, dir, &mut report)?;
    let r = ws.dbs.get_mut(db).and_then(|d| d.rows.get_mut(row)).ok_or_else(|| Error::NotFound(row.to_string()))?;
    r.meta.title = kept;
    r.blocks = blocks;
    r.touch();
    ws.save_row(db, row)?;
    report.pages = 1;
    Ok(report)
}

/// `[Text](Other%20Page.md)` → `[[Other Page]]` for files we imported.
pub(crate) fn rewrite_md_links(text: &str, dir: &Path, titles: &BTreeMap<PathBuf, String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = text;
    let mut changed = false;
    while let Some(open) = rest.find('[') {
        let after = &rest[open..];
        if after.starts_with("[[") {
            out.push_str(&rest[..open + 2]);
            rest = &rest[open + 2..];
            continue;
        }
        let Some(mid) = after.find("](") else { break };
        let Some(close) = after[mid..].find(')') else { break };
        let target = &after[mid + 2..mid + close];
        let is_md = target.to_lowercase().ends_with(".md") && !target.contains("://");
        if is_md {
            let p = canonical(&dir.join(percent_decode(target)));
            if let Some(t) = titles.get(&p) {
                out.push_str(&rest[..open]);
                out.push_str(&format!("[[{t}]]"));
                rest = &after[mid + close + 1..];
                changed = true;
                continue;
            }
        }
        out.push_str(&rest[..open + mid + close + 1]);
        rest = &after[mid + close + 1..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

// ---------------------------------------------------------------------- export

/// Where each exported document goes, relative to the export folder.
struct Plan {
    paths: BTreeMap<DocId, PathBuf>,
    by_title: BTreeMap<String, DocId>,
}

fn plan(ws: &Workspace, roots: &[DocId], recursive: bool) -> Plan {
    let mut p = Plan { paths: BTreeMap::new(), by_title: BTreeMap::new() };
    fn add(ws: &Workspace, plan: &mut Plan, doc: &DocId, folder: PathBuf, recursive: bool) {
        let name = safe_file_name(&ws.title(doc));
        let mut file = folder.join(format!("{name}.md"));
        let mut n = 2;
        while plan.paths.values().any(|x| x == &file) {
            file = folder.join(format!("{name} {n}.md"));
            n += 1;
        }
        let sub = file.with_extension("");
        plan.paths.insert(doc.clone(), file);
        plan.by_title.entry(ws.title(doc).to_lowercase()).or_insert(doc.clone());
        if let DocId::Database(db) = doc {
            for (rid, r) in &ws.dbs[db].rows {
                if r.meta.trashed.is_none() {
                    add(ws, plan, &DocId::Row(db.clone(), rid.clone()), sub.clone(), false);
                }
            }
        }
        if recursive {
            for c in ws.children(Some(doc.id())) {
                add(ws, plan, &c.doc, sub.clone(), true);
            }
        }
    }
    for r in roots {
        add(ws, &mut p, r, PathBuf::new(), recursive);
    }
    p
}

fn relative(from_file: &Path, to: &Path) -> String {
    let from_dir: Vec<_> = from_file.parent().map(|p| p.components().collect()).unwrap_or_default();
    let to_c: Vec<_> = to.components().collect();
    let common = from_dir.iter().zip(to_c.iter()).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); from_dir.len() - common];
    parts.extend(to_c[common..].iter().map(|c| c.as_os_str().to_string_lossy().to_string()));
    parts.join("/")
}

fn export_text(ws: &Workspace, doc: &DocId, file: &Path, plan: &Plan, assets: &mut Vec<String>) -> String {
    let mut out = String::new();
    let icon = ws.icon(doc);
    let title = ws.title(doc);
    out.push_str(&format!("# {}{}\n\n", if icon.is_empty() { String::new() } else { format!("{icon} ") }, title));
    // properties of a row
    if let DocId::Row(db, r) = doc {
        let d = &ws.dbs[db];
        let row = &d.rows[r];
        for p in &d.schema.properties {
            if p.kind == PropKind::Title {
                continue;
            }
            let v = ws.display_value(db, row, p);
            if !v.is_empty() {
                out.push_str(&format!("- **{}:** {}\n", p.name, v));
            }
        }
        out.push('\n');
    }
    // a database: a table of its rows
    if let DocId::Database(db) = doc {
        let d = &ws.dbs[db];
        let props: Vec<_> = d.schema.properties.iter().collect();
        out.push_str(&format!("| {} |\n", props.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(" | ")));
        out.push_str(&format!("|{}\n", " --- |".repeat(props.len())));
        for (rid, row) in d.rows.iter().filter(|(_, r)| r.meta.trashed.is_none()) {
            let cells: Vec<String> = props
                .iter()
                .map(|p| {
                    let v = ws.display_value(db, row, p).replace('|', "\\|");
                    if p.kind == PropKind::Title {
                        let target = &plan.paths[&DocId::Row(db.clone(), rid.clone())];
                        format!(
                            "[{}]({})",
                            if v.is_empty() { "Untitled".into() } else { v },
                            percent_encode_path(&relative(file, target))
                        )
                    } else {
                        v
                    }
                })
                .collect();
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
        }
        return out;
    }
    let blocks: Vec<Block> = ws
        .blocks(doc)
        .map(|bs| {
            bs.iter()
                .map(|b| {
                    let mut b = b.clone();
                    match &mut b.kind {
                        BlockKind::Image { path } | BlockKind::File { path } if !path.contains("://") => {
                            assets.push(path.clone());
                            let target = Path::new(path.as_str());
                            *path = relative(file, target);
                        }
                        _ => {}
                    }
                    if b.kind.has_rich_text() {
                        b.text = export_mentions(&b.text, file, plan);
                    }
                    b
                })
                .collect()
        })
        .unwrap_or_default();
    out.push_str(&blocks::to_markdown(&blocks));
    // children list for pages
    let kids = ws.children(Some(doc.id()));
    if !kids.is_empty() {
        out.push('\n');
        for k in kids {
            if let Some(t) = plan.paths.get(&k.doc) {
                out.push_str(&format!("- [{}]({})\n", k.title, percent_encode_path(&relative(file, t))));
            }
        }
    }
    out
}

fn export_mentions(text: &str, file: &Path, plan: &Plan) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(close) = after.find("]]") else { break };
        let title = &after[..close];
        out.push_str(&rest[..start]);
        match plan.by_title.get(&title.to_lowercase()).and_then(|d| plan.paths.get(d)) {
            Some(target) => out.push_str(&format!("[{title}]({})", percent_encode_path(&relative(file, target)))),
            None => out.push_str(title),
        }
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    out
}

/// A relative path that stays where it is put: no `..`, no root, no drive.
fn is_safe_relative(p: &str) -> bool {
    let path = Path::new(p);
    !p.is_empty() && !path.is_absolute() && path.components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn write_plan(ws: &Workspace, plan: &Plan, dest: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dest)?;
    let mut written = Vec::new();
    let mut assets = Vec::new();
    for (doc, rel) in &plan.paths {
        let text = export_text(ws, doc, rel, plan, &mut assets);
        let path = dest.join(rel);
        atomic_write(&path, text.as_bytes())?;
        if let DocId::Database(db) = doc {
            let csv = database_csv(ws, db);
            atomic_write(&path.with_extension("csv"), csv.as_bytes())?;
        }
        written.push(path);
    }
    assets.sort();
    assets.dedup();
    let root = ws.root.canonicalize().unwrap_or_else(|_| ws.root.clone());
    for a in assets {
        // a page can name any path: only files that really sit inside the workspace are copied,
        // and always to a place inside `dest`
        if !is_safe_relative(&a) {
            continue;
        }
        let src = ws.asset_path(&a);
        let inside = src.canonicalize().map(|c| c.starts_with(&root)).unwrap_or(false);
        if inside && src.is_file() {
            let target = dest.join(&a);
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(&src, &target)?;
        }
    }
    Ok(written)
}

/// CSV of a database's rows (RFC 4180 quoting).
pub fn database_csv(ws: &Workspace, db: &str) -> String {
    let d = &ws.dbs[db];
    let q = |s: &str| {
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = d.schema.properties.iter().map(|p| q(&p.name)).collect::<Vec<_>>().join(",");
    out.push('\n');
    for row in d.live_rows() {
        let cells: Vec<String> = d.schema.properties.iter().map(|p| q(&ws.display_value(db, row, p))).collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// Export one page (with sub-pages when `recursive`) or database. Returns the main file.
pub fn export_doc(ws: &Workspace, doc: &DocId, dest: &Path, recursive: bool) -> Result<PathBuf> {
    let plan = plan(ws, std::slice::from_ref(doc), recursive);
    write_plan(ws, &plan, dest)?;
    Ok(dest.join(&plan.paths[doc]))
}

/// Export one page as HTML, Word or PDF into `dest` (a folder). Returns the file written.
pub fn export_page_as(ws: &Workspace, doc: &DocId, dest: &Path, format: crate::render::Format) -> Result<PathBuf> {
    let blocks = ws.blocks(doc).ok_or_else(|| crate::Error::NotFound("page".into()))?;
    let title = ws.title(doc);
    let read = |rel: &str| std::fs::read(ws.asset_path(rel)).ok();
    let bytes = crate::render::render(format, &title, blocks, &read);
    let name: String = title.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '-' } else { c }).collect();
    let name = if name.trim().is_empty() { "Untitled".to_string() } else { name.trim().to_string() };
    std::fs::create_dir_all(dest)?;
    let path = dest.join(format!("{name}.{}", format.extension()));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Write the page as an unsent email (`.eml`) into `dest`, ready for the system's mail application.
pub fn email_draft(ws: &Workspace, doc: &DocId, dest: &Path) -> Result<PathBuf> {
    let blocks = ws.blocks(doc).ok_or_else(|| crate::Error::NotFound("page".into()))?;
    let title = ws.title(doc);
    let read = |rel: &str| std::fs::read(ws.asset_path(rel)).ok();
    let eml = crate::render::to_eml(&title, blocks, &read);
    let name: String = title.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '-' } else { c }).collect();
    let name = if name.trim().is_empty() { "Untitled".to_string() } else { name.trim().to_string() };
    std::fs::create_dir_all(dest)?;
    let path = dest.join(format!("{name}.eml"));
    std::fs::write(&path, eml)?;
    Ok(path)
}

/// What an email needs from a page: its subject, the formatted HTML body and the plain-text version.
pub fn email_parts(ws: &Workspace, doc: &DocId) -> Result<(String, String, String)> {
    let blocks = ws.blocks(doc).ok_or_else(|| crate::Error::NotFound("page".into()))?;
    let read = |rel: &str| std::fs::read(ws.asset_path(rel)).ok();
    let subject: String = ws.title(doc).chars().filter(|c| !c.is_control()).collect();
    Ok((subject, crate::render::email_html(blocks, &read), crate::blocks::to_markdown(blocks)))
}

/// Export every live top-level page and database into `dest`.
pub fn export_workspace(ws: &Workspace, dest: &Path) -> Result<usize> {
    let roots: Vec<DocId> = ws.children(None).into_iter().map(|t| t.doc).collect();
    let plan = plan(ws, &roots, true);
    Ok(write_plan(ws, &plan, dest)?.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_lose_export_ids() {
        assert_eq!(clean_title("Notes 0123456789abcdef0123456789abcdef"), "Notes");
        assert_eq!(clean_title("Plain"), "Plain");
    }

    #[test]
    fn import_folder_with_nesting_links_and_images() {
        let src = tempfile::tempdir().unwrap();
        let s = src.path().join("Vault");
        fs::create_dir_all(s.join("Projects/img")).unwrap();
        fs::write(s.join("Home.md"), "# Home\n\nGo to [projects](Projects.md) now.\n").unwrap();
        fs::write(s.join("Projects.md"), "Overview text\n").unwrap();
        fs::write(s.join("Projects/Alpha.md"), "---\ntitle: Alpha project\nicon: 🚀\n---\n\n![shot](img/a%20b.png)\n")
            .unwrap();
        fs::write(s.join("Projects/img/a b.png"), b"png").unwrap();
        let wdir = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wdir.path()).unwrap();
        let rep = import_markdown_folder(&mut ws, &s, None).unwrap();
        assert_eq!(rep.pages, 4);
        assert_eq!(rep.assets, 1);
        let home = ws.pages.values().find(|p| p.meta.title == "Home").unwrap();
        assert_eq!(home.blocks[0].text, "Go to [[Projects]] now.");
        let alpha = ws.pages.values().find(|p| p.meta.title == "Alpha project").unwrap();
        assert_eq!(alpha.meta.icon, "🚀");
        let projects = ws.pages.values().find(|p| p.meta.title == "Projects").unwrap();
        assert_eq!(alpha.meta.parent.as_deref(), Some(projects.meta.id.as_str()));
        match &alpha.blocks[0].kind {
            BlockKind::Image { path } => assert!(ws.asset_path(path).exists()),
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn export_page_tree_and_database() {
        let wdir = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wdir.path()).unwrap();
        let a = ws.create_page(None, "Root").unwrap();
        let b = ws.create_page(Some(&a), "Child").unwrap();
        ws.pages.get_mut(&a).unwrap().blocks.push(Block::paragraph("see [[Child]]"));
        ws.save_page(&a).unwrap();
        ws.pages.get_mut(&b).unwrap().blocks.push(Block::paragraph("child body"));
        ws.save_page(&b).unwrap();
        let db = ws.create_database(Some(&a), "Tasks").unwrap();
        ws.add_row(&db, None, "Write, test").unwrap();
        let out = tempfile::tempdir().unwrap();
        let main = export_doc(&ws, &DocId::Page(a.clone()), out.path(), true).unwrap();
        let text = fs::read_to_string(&main).unwrap();
        assert!(text.starts_with("# Root"));
        assert!(text.contains("[Child](Root/Child.md)"));
        assert!(out.path().join("Root/Child.md").exists());
        let csv = fs::read_to_string(out.path().join("Root/Tasks.csv")).unwrap();
        assert!(csv.contains("\"Write, test\""));
        let n = export_workspace(&ws, out.path().join("all").as_path()).unwrap();
        assert!(n >= 4);
    }

    #[test]
    fn export_never_copies_files_from_outside_the_workspace() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "top secret").unwrap();
        let wdir = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wdir.path()).unwrap();
        let a = ws.create_page(None, "Evil").unwrap();
        let rel_up = format!("../../../../../../../..{}", secret.display());
        for path in [secret.display().to_string(), rel_up, "assets/../../escape.txt".to_string()] {
            ws.pages.get_mut(&a).unwrap().blocks.push(Block::new(BlockKind::File { path }, "x"));
        }
        ws.save_page(&a).unwrap();
        let out = tempfile::tempdir().unwrap();
        let dest = out.path().join("export");
        export_doc(&ws, &DocId::Page(a), &dest, true).unwrap();
        let mut found = Vec::new();
        fn walk(d: &Path, found: &mut Vec<PathBuf>) {
            for e in fs::read_dir(d).unwrap().flatten() {
                if e.path().is_dir() {
                    walk(&e.path(), found);
                } else {
                    found.push(e.path());
                }
            }
        }
        walk(out.path(), &mut found);
        assert!(found.iter().all(|p| p.starts_with(&dest)), "something was written outside the export: {found:?}");
        assert!(found.iter().all(|p| fs::read_to_string(p).map(|t| !t.contains("top secret")).unwrap_or(true)));
        assert_eq!(fs::read_to_string(&secret).unwrap(), "top secret");
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative(Path::new("A/B.md"), Path::new("A/B/C.md")), "B/C.md");
        assert_eq!(relative(Path::new("A/B/C.md"), Path::new("assets/x.png")), "../../assets/x.png");
    }
}

#[cfg(test)]
mod replace_tests {
    use super::*;

    #[test]
    fn sub_pages_point_at_what_the_workspace_has() {
        let t = "<page url=\"https://example.com/p/abc\">\nQuestions au notaire\n</page>";
        let r = resolve_page_refs(t);
        assert!(r.contains("[[Questions au notaire]]"), "{r}");
    }

    #[test]
    fn a_page_body_is_replaced_in_place_with_colours_columns_and_its_files() {
        let dir = std::env::temp_dir().join(format!("nc-rebody-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws_dir = dir.join("ws");
        let src = dir.join("src");
        std::fs::create_dir_all(&ws_dir).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("Acte x.pdf"), b"%PDF fake").unwrap();
        let mut ws = Workspace::open(&ws_dir).unwrap();
        let id = ws.create_page(None, "Page").unwrap();
        let child = ws.create_page(Some(&id), "Child").unwrap();
        let md = "# Page\n\n<callout color=\"yellow_bg\">\nNote\n</callout>\n\n<file source=\"Acte%20x.pdf\" />\n\n<page url=\"https://example.com/p/abc\">\nChild\n</page>\n";
        let rep = replace_page_body(&mut ws, &id, md, &src).unwrap();
        let p = &ws.pages[&id];
        assert_eq!(p.meta.title, "Page");
        assert!(
            p.blocks.iter().any(|b| matches!(&b.kind, BlockKind::File { path } if path.starts_with("assets/"))),
            "{:?}",
            p.blocks
        );
        assert!(p.blocks.iter().any(|b| b.text.contains("[[Child]]")), "{:?}", p.blocks);
        assert_eq!(rep.assets, 1);
        assert_eq!(ws.pages[&child].meta.parent.as_deref(), Some(id.as_str()), "children stay");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
