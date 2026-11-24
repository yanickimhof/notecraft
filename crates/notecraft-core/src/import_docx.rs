//! Import a Word document (`.docx`): headings, paragraphs, bold, italic, underline, strike, links,
//! bulleted and numbered lists (nested), tables and images.

use crate::blocks;
use crate::import_export::{localize_assets, take_title, ImportReport};
use crate::workspace::Workspace;
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAX_XML: u64 = 256 * 1024 * 1024;
const MAX_MEDIA: u64 = 128 * 1024 * 1024;

/// True when the file is a zip that holds `word/document.xml`.
pub fn looks_like_docx(path: &Path) -> bool {
    if !crate::import_zip::looks_like_zip(path) {
        return false;
    }
    File::open(path)
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok())
        .is_some_and(|mut z| z.by_name("word/document.xml").is_ok())
}

fn read_entry(z: &mut zip::ZipArchive<File>, name: &str, max: u64) -> Option<Vec<u8>> {
    let e = z.by_name(name).ok()?;
    if e.size() > max {
        return None;
    }
    let mut v = Vec::new();
    e.take(max).read_to_end(&mut v).ok()?;
    Some(v)
}

fn read_text(z: &mut zip::ZipArchive<File>, name: &str) -> Option<String> {
    read_entry(z, name, MAX_XML).map(|b| String::from_utf8_lossy(&b).to_string())
}

fn wattr<'a>(n: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attribute((W, name))
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// `<w:b/>` is on; `<w:b w:val="0"/>` or `false` is off.
fn flag(rpr: Option<roxmltree::Node>, name: &str) -> bool {
    rpr.and_then(|r| child(r, name)).is_some_and(|n| !matches!(wattr(n, "val"), Some("0" | "false" | "none")))
}

#[derive(Clone, Default, PartialEq)]
struct Fmt {
    b: bool,
    i: bool,
    u: bool,
    s: bool,
    link: Option<String>,
}

enum Seg {
    Text(String, Fmt),
    Image(String, String),
}

struct Doc<'a> {
    rels: BTreeMap<String, String>,
    styles: BTreeMap<String, String>,
    /// (numId, level) → true when the level is a bullet
    bullets: BTreeMap<(String, u32), bool>,
    zip: &'a mut zip::ZipArchive<File>,
    ws: &'a mut Workspace,
    report: &'a mut ImportReport,
    images: usize,
}

fn parse_rels(xml: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Ok(d) = roxmltree::Document::parse(xml) {
        for r in d.descendants().filter(|n| n.has_tag_name("Relationship")) {
            if let (Some(id), Some(t)) = (r.attribute("Id"), r.attribute("Target")) {
                m.insert(id.to_string(), t.to_string());
            }
        }
    }
    m
}

fn parse_styles(xml: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Ok(d) = roxmltree::Document::parse(xml) {
        for s in d.descendants().filter(|n| n.is_element() && n.tag_name().name() == "style") {
            if let (Some(id), Some(name)) = (wattr(s, "styleId"), child(s, "name").and_then(|n| wattr(n, "val"))) {
                m.insert(id.to_string(), name.to_lowercase());
            }
        }
    }
    m
}

fn parse_numbering(xml: &str) -> BTreeMap<(String, u32), bool> {
    let mut out = BTreeMap::new();
    let Ok(d) = roxmltree::Document::parse(xml) else { return out };
    let mut abs: BTreeMap<String, BTreeMap<u32, bool>> = BTreeMap::new();
    for a in d.descendants().filter(|n| n.is_element() && n.tag_name().name() == "abstractNum") {
        let Some(id) = wattr(a, "abstractNumId") else { continue };
        let mut lv = BTreeMap::new();
        for l in a.children().filter(|c| c.is_element() && c.tag_name().name() == "lvl") {
            let level = wattr(l, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0);
            let fmt = child(l, "numFmt").and_then(|n| wattr(n, "val")).unwrap_or("bullet");
            lv.insert(level, fmt == "bullet" || fmt == "none");
        }
        abs.insert(id.to_string(), lv);
    }
    for n in d.descendants().filter(|n| n.is_element() && n.tag_name().name() == "num") {
        let (Some(num), Some(a)) = (wattr(n, "numId"), child(n, "abstractNumId").and_then(|c| wattr(c, "val"))) else {
            continue;
        };
        if let Some(lv) = abs.get(a) {
            for (level, bullet) in lv {
                out.insert((num.to_string(), *level), *bullet);
            }
        }
    }
    out
}

impl Doc<'_> {
    fn image(&mut self, rid: &str) -> Option<String> {
        let target = self.rels.get(rid)?.clone();
        let path = if let Some(t) = target.strip_prefix('/') { t.to_string() } else { format!("word/{target}") };
        if self.images >= 5000 {
            return None;
        }
        let bytes = read_entry(self.zip, &path, MAX_MEDIA)?;
        let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
        let p = self.ws.add_asset_bytes(&name, &bytes).ok()?;
        self.images += 1;
        self.report.assets += 1;
        Some(p)
    }

    fn collect(&mut self, node: roxmltree::Node, link: Option<&str>, segs: &mut Vec<Seg>) {
        for c in node.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "r" => self.run(c, link, segs),
                "hyperlink" => {
                    let l = c
                        .attribute((R, "id"))
                        .and_then(|id| self.rels.get(id))
                        .filter(|t| t.starts_with("http") || t.starts_with("mailto:"))
                        .cloned();
                    self.collect(c, l.as_deref().or(link), segs);
                }
                "ins" | "smartTag" | "sdt" | "sdtContent" | "fldSimple" | "customXml" => self.collect(c, link, segs),
                _ => {}
            }
        }
    }

    fn run(&mut self, r: roxmltree::Node, link: Option<&str>, segs: &mut Vec<Seg>) {
        let rpr = child(r, "rPr");
        let fmt = Fmt {
            b: flag(rpr, "b"),
            i: flag(rpr, "i"),
            u: flag(rpr, "u"),
            s: flag(rpr, "strike"),
            link: link.map(String::from),
        };
        let mut text = String::new();
        for c in r.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "t" => text.push_str(c.text().unwrap_or("")),
                "tab" => text.push(' '),
                "br" | "cr" => {
                    if wattr(c, "type") != Some("page") {
                        text.push('\n');
                    }
                }
                "noBreakHyphen" => text.push('-'),
                "drawing" | "pict" => {
                    if !text.is_empty() {
                        segs.push(Seg::Text(std::mem::take(&mut text), fmt.clone()));
                    }
                    let rid = c.descendants().find_map(|d| {
                        d.attribute((R, "embed"))
                            .or_else(|| (d.tag_name().name() == "imagedata").then(|| d.attribute((R, "id"))).flatten())
                    });
                    let alt = c.descendants().find_map(|d| d.attribute("descr")).unwrap_or("").to_string();
                    if let Some(p) = rid.and_then(|id| self.image(id)) {
                        segs.push(Seg::Image(p, alt));
                    }
                }
                _ => {}
            }
        }
        if !text.is_empty() {
            segs.push(Seg::Text(text, fmt));
        }
    }
}

fn render(segs: &[Seg]) -> String {
    // merge neighbours that share a format, then wrap
    let mut merged: Vec<(String, Fmt)> = Vec::new();
    for s in segs {
        if let Seg::Text(t, f) = s {
            match merged.last_mut() {
                Some((lt, lf)) if lf == f => lt.push_str(t),
                _ => merged.push((t.clone(), f.clone())),
            }
        }
    }
    let mut out = String::new();
    for (t, f) in merged {
        let core = t.trim();
        if core.is_empty() {
            out.push_str(&t);
            continue;
        }
        let lead = &t[..t.len() - t.trim_start().len()];
        let trail = &t[t.trim_end().len()..];
        let mut s = core.replace('\n', "  \n");
        if f.s {
            s = format!("~~{s}~~");
        }
        if f.u && f.link.is_none() {
            s = format!("<u>{s}</u>");
        }
        if f.i {
            s = format!("*{s}*");
        }
        if f.b {
            s = format!("**{s}**");
        }
        if let Some(l) = &f.link {
            s = format!("[{s}]({l})");
        }
        out.push_str(lead);
        out.push_str(&s);
        out.push_str(trail);
    }
    out.trim().to_string()
}

fn para(d: &mut Doc, p: roxmltree::Node, out: &mut Vec<String>) {
    let ppr = child(p, "pPr");
    let style_id = ppr.and_then(|x| child(x, "pStyle")).and_then(|x| wattr(x, "val")).unwrap_or("");
    let style = d.styles.get(style_id).cloned().unwrap_or_else(|| style_id.to_lowercase());
    let mut segs = Vec::new();
    d.collect(p, None, &mut segs);
    // text and images in order; an image ends the paragraph so far
    let mut run: Vec<Seg> = Vec::new();
    let mut pieces: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<Seg>, pieces: &mut Vec<String>| {
        let t = render(run);
        run.clear();
        if !t.is_empty() {
            pieces.push(t);
        }
    };
    for s in segs {
        match s {
            Seg::Image(path, alt) => {
                flush(&mut run, &mut pieces);
                out.push(format!("![{alt}]({path})"));
            }
            t => {
                run.push(t);
            }
        }
    }
    flush(&mut run, &mut pieces);
    for text in pieces {
        let heading = if style == "title" {
            Some(1)
        } else if let Some(n) = style.strip_prefix("heading ").or_else(|| style.strip_prefix("titre ")) {
            n.trim().parse::<usize>().ok().map(|n| n.clamp(1, 3))
        } else {
            None
        };
        if let Some(level) = heading {
            out.push(format!("{} {}", "#".repeat(level), text.replace("  \n", " ")));
            continue;
        }
        let num = ppr.and_then(|x| child(x, "numPr"));
        if let Some(np) = num {
            let level: u32 = child(np, "ilvl").and_then(|n| wattr(n, "val")).and_then(|v| v.parse().ok()).unwrap_or(0);
            let id = child(np, "numId").and_then(|n| wattr(n, "val")).unwrap_or("0").to_string();
            if id != "0" {
                let bullet = d.bullets.get(&(id, level)).copied().unwrap_or(true);
                out.push(format!(
                    "{}{}{}",
                    "  ".repeat(level.min(6) as usize),
                    if bullet { "- " } else { "1. " },
                    text.replace("  \n", " ")
                ));
                continue;
            }
        }
        if style.contains("quote") {
            out.push(format!("> {text}"));
        } else {
            out.push(text);
        }
    }
}

fn table(d: &mut Doc, t: roxmltree::Node, out: &mut Vec<String>) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for tr in t.children().filter(|c| c.is_element() && c.tag_name().name() == "tr") {
        let mut cells = Vec::new();
        for tc in tr.children().filter(|c| c.is_element() && c.tag_name().name() == "tc") {
            let mut parts = Vec::new();
            for p in tc.children().filter(|c| c.is_element() && c.tag_name().name() == "p") {
                let mut segs = Vec::new();
                d.collect(p, None, &mut segs);
                let s = render(&segs);
                if !s.is_empty() {
                    parts.push(s.replace("  \n", " "));
                }
            }
            cells.push(parts.join(" ").replace('|', "\\|"));
        }
        rows.push(cells);
    }
    rows.retain(|r| r.iter().any(|c| !c.is_empty()));
    if rows.is_empty() {
        return;
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1);
    let line = |r: &Vec<String>| {
        let mut c = r.clone();
        c.resize(cols, String::new());
        format!("| {} |", c.join(" | "))
    };
    let mut lines = vec![line(&rows[0]), format!("|{}|", vec![" --- "; cols].join("|"))];
    lines.extend(rows.iter().skip(1).map(line));
    out.push(lines.join("\n"));
}

/// Import one `.docx` as a page under `parent`. Returns the page id in `root_page`.
pub fn import_docx(ws: &mut Workspace, file: &Path, parent: Option<&str>) -> Result<ImportReport> {
    let f = File::open(file)?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| Error::Invalid(format!("not a Word document: {e}")))?;
    let xml = read_text(&mut zip, "word/document.xml")
        .ok_or_else(|| Error::Invalid("not a Word document (no word/document.xml)".into()))?;
    let rels = read_text(&mut zip, "word/_rels/document.xml.rels").map(|x| parse_rels(&x)).unwrap_or_default();
    let styles = read_text(&mut zip, "word/styles.xml").map(|x| parse_styles(&x)).unwrap_or_default();
    let bullets = read_text(&mut zip, "word/numbering.xml").map(|x| parse_numbering(&x)).unwrap_or_default();
    let core_title = read_text(&mut zip, "docProps/core.xml").and_then(|x| {
        roxmltree::Document::parse(&x).ok().and_then(|d| {
            d.descendants()
                .find(|n| n.tag_name().name() == "title")
                .and_then(|n| n.text().map(|t| t.trim().to_string()))
        })
    });
    let doc = roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions { allow_dtd: false, ..Default::default() },
    )
    .map_err(|e| Error::Invalid(format!("damaged Word document: {e}")))?;
    let mut report = ImportReport::default();
    let mut lines: Vec<String> = Vec::new();
    {
        let mut d = Doc { rels, styles, bullets, zip: &mut zip, ws, report: &mut report, images: 0 };
        let body = doc
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "body")
            .ok_or_else(|| Error::Invalid("empty Word document".into()))?;
        for n in body.children().filter(|c| c.is_element()) {
            match n.tag_name().name() {
                "p" => para(&mut d, n, &mut lines),
                "tbl" => table(&mut d, n, &mut lines),
                "sdt" => {
                    for p in n.descendants().filter(|x| x.is_element() && x.tag_name().name() == "p") {
                        para(&mut d, p, &mut lines);
                    }
                }
                _ => {}
            }
        }
    }
    // list items stay together, everything else is a paragraph apart
    let mut md = String::new();
    let is_item = |l: &str| l.trim_start().starts_with("- ") || l.trim_start().starts_with("1. ");
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            md.push_str(if is_item(l) && is_item(&lines[i - 1]) { "\n" } else { "\n\n" });
        }
        md.push_str(l);
    }
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into());
    let core_title = core_title.filter(|t| !t.is_empty());
    let from_file_name = core_title.is_none();
    let mut title = core_title.unwrap_or(stem);
    let mut blocks = blocks::from_markdown(&md);
    // without a title in the file's properties, a leading heading is the better title than the file name
    take_title(&mut blocks, &mut title, from_file_name);
    localize_assets(ws, &mut blocks, file.parent().unwrap_or(Path::new(".")), &mut report)?;
    let id = ws.create_page(parent, &title)?;
    ws.pages.get_mut(&id).unwrap().blocks = blocks;
    ws.save_page(&id)?;
    report.pages = 1;
    report.root_page = Some(id);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_zip::tests::make_zip;

    const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
<w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Report</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">Plain </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve"> italic</w:t></w:r><w:hyperlink r:id="rId5"><w:r><w:t>site</w:t></w:r></w:hyperlink></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>First</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Nested</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>Step</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><a:blip r:embed="rId9"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
</w:body></w:document>"#;

    const RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId5" Type="x" Target="https://example.org" TargetMode="External"/>
<Relationship Id="rId9" Type="x" Target="media/image1.png"/></Relationships>"#;

    const STYLES: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:styleId="Heading1"><w:name w:val="heading 1"/></w:style></w:styles>"#;

    const NUMBERING: &str = r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>
<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#;

    #[test]
    fn a_word_file_comes_in_with_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("My report.docx");
        make_zip(
            &f,
            &[
                ("word/document.xml", DOC.as_bytes()),
                ("word/_rels/document.xml.rels", RELS.as_bytes()),
                ("word/styles.xml", STYLES.as_bytes()),
                ("word/numbering.xml", NUMBERING.as_bytes()),
                ("word/media/image1.png", b"\x89PNG fake"),
            ],
        );
        assert!(looks_like_docx(&f));
        let wsd = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wsd.path()).unwrap();
        let r = import_docx(&mut ws, &f, None).unwrap();
        assert_eq!(r.assets, 1);
        let id = ws.resolve_title("Report").expect("the Heading 1 becomes the title");
        let b = ws.blocks(&id).unwrap();
        let find = |t: &str| b.iter().find(|x| x.text.contains(t)).unwrap_or_else(|| panic!("{t} in {b:?}"));
        assert!(find("bold").text.contains("**bold**") && find("bold").text.contains("*italic*"));
        assert!(find("site").text.contains("[site](https://example.org)"));
        assert_eq!(find("First").kind, blocks::BlockKind::Bulleted);
        assert_eq!(find("Nested").depth, 1);
        assert_eq!(find("Step").kind, blocks::BlockKind::Numbered);
        assert_eq!(find("A").kind, blocks::BlockKind::Table);
        assert!(b.iter().any(|x| matches!(&x.kind, blocks::BlockKind::Image { path } if path.starts_with("assets/"))));
    }

    #[test]
    fn something_that_is_not_word_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x.docx");
        make_zip(&f, &[("hello.txt", b"hi")]);
        assert!(!looks_like_docx(&f));
        let wsd = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(wsd.path()).unwrap();
        assert!(import_docx(&mut ws, &f, None).is_err());
        std::fs::write(&f, "plain").unwrap();
        assert!(import_docx(&mut ws, &f, None).is_err());
    }
}
