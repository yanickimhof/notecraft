//! Rendering a page to HTML and Word (.docx) here; the PDF layout lives in `pdf.rs`.
//!
//! All three share one intermediate list of [`Item`]s built from the blocks, so a
//! new block kind is handled in one place.

use crate::blocks::{Block, BlockKind};
use crate::inline::{self, Style};

/// Output formats offered by the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Html,
    Docx,
    Pdf,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Html => "html",
            Format::Docx => "docx",
            Format::Pdf => "pdf",
        }
    }
}

/// Render `blocks` in `format`. `read_asset` returns the bytes of a workspace relative file (images).
pub fn render(format: Format, title: &str, blocks: &[Block], read_asset: &dyn Fn(&str) -> Option<Vec<u8>>) -> Vec<u8> {
    match format {
        Format::Html => to_html(title, blocks, read_asset).into_bytes(),
        Format::Docx => to_docx(title, blocks, read_asset),
        Format::Pdf => crate::pdf::to_pdf(title, blocks, read_asset),
    }
}

// ---------------------------------------------------------------------------
// intermediate form

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Marker {
    None,
    Bullet,
    Num(u32),
    Todo(bool),
    Toggle,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    Para,
    Heading(u8),
    Quote,
    Callout(String),
    Code(String),
    Divider,
    Image(String),
    File,
}

#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub(crate) text: String,
    pub(crate) style: Style,
}

#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub(crate) kind: Kind,
    pub(crate) marker: Marker,
    pub(crate) depth: u8,
    pub(crate) runs: Vec<Run>,
    pub(crate) color: String,
}

fn runs_of(text: &str) -> Vec<Run> {
    inline::parse(text)
        .into_iter()
        .filter(|s| !s.marker)
        .map(|s| Run { text: text[s.range].to_string(), style: s.style })
        .collect()
}

fn plain(text: &str) -> Vec<Run> {
    vec![Run { text: text.to_string(), style: Style::default() }]
}

pub(crate) fn items(blocks: &[Block]) -> Vec<Item> {
    let mut counters: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    for b in blocks {
        if b.kind == BlockKind::Toc {
            // the table of contents becomes an indented list of the headings
            for h in blocks {
                if let BlockKind::Heading(l) = h.kind {
                    let l = l.clamp(1, 3);
                    out.push(Item {
                        kind: Kind::Para,
                        marker: Marker::None,
                        depth: l - 1,
                        runs: runs_of(&h.text),
                        color: String::new(),
                    });
                }
            }
            continue;
        }
        if matches!(b.kind, BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd) {
            continue;
        }
        if b.kind == BlockKind::Equation {
            out.push(Item {
                kind: Kind::Code(String::new()),
                marker: Marker::None,
                depth: b.depth,
                runs: plain(&b.text),
                color: String::new(),
            });
            continue;
        }
        if let BlockKind::Bookmark { url } = &b.kind {
            let label = if b.text.is_empty() { url.clone() } else { format!("{} ({url})", b.text) };
            out.push(Item {
                kind: Kind::Para,
                marker: Marker::None,
                depth: b.depth,
                runs: plain(&label),
                color: String::new(),
            });
            continue;
        }
        if b.kind == BlockKind::Table {
            // Exports show a table as one line per row, cells separated by a bar.
            for row in crate::blocks::table_cells(&b.text) {
                out.push(Item {
                    kind: Kind::Para,
                    marker: Marker::None,
                    depth: b.depth,
                    runs: plain(&row.join("   |   ")),
                    color: String::new(),
                });
            }
            continue;
        }
        let d = b.depth as usize;
        counters.truncate(d + 1);
        if matches!(b.kind, BlockKind::Numbered) {
            counters.resize(d + 1, 0);
            counters[d] += 1;
        } else if counters.len() > d {
            counters.truncate(d);
        }
        let marker = match &b.kind {
            BlockKind::Bulleted => Marker::Bullet,
            BlockKind::Numbered => Marker::Num(counters[d]),
            BlockKind::Todo { checked } => Marker::Todo(*checked),
            BlockKind::Toggle { .. } => Marker::Toggle,
            _ => Marker::None,
        };
        let kind = match &b.kind {
            BlockKind::Heading(l) => Kind::Heading((*l).clamp(1, 3)),
            BlockKind::Quote => Kind::Quote,
            BlockKind::Callout { icon } => Kind::Callout(icon.clone()),
            BlockKind::Code { lang } => Kind::Code(lang.clone()),
            BlockKind::Divider => Kind::Divider,
            BlockKind::Image { path } => Kind::Image(path.clone()),
            BlockKind::File { .. } => Kind::File,
            _ => Kind::Para,
        };
        let runs = match &kind {
            Kind::Code(_) | Kind::Divider => plain(&b.text),
            _ => runs_of(&b.text),
        };
        out.push(Item { kind, marker, depth: b.depth, runs, color: b.color.clone() });
    }
    out
}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 32 && c != '\n' && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

// ---------------------------------------------------------------------------
// HTML

pub(crate) fn css_color(name: &str) -> Option<(&'static str, bool)> {
    let (bg, base) = match name.strip_prefix("bg-") {
        Some(n) => (true, n),
        None => (false, name),
    };
    let hex = match (base, bg) {
        ("gray", false) => "#787774",
        ("brown", false) => "#9f6b53",
        ("orange", false) => "#d9730d",
        ("yellow", false) => "#cb912f",
        ("green", false) => "#448361",
        ("blue", false) => "#337ea9",
        ("purple", false) => "#9065b0",
        ("pink", false) => "#c14f8a",
        ("red", false) => "#d44c47",
        ("gray", true) => "#f1f1ef",
        ("brown", true) => "#f4eeee",
        ("orange", true) => "#fbecdd",
        ("yellow", true) => "#fbf3db",
        ("green", true) => "#edf3ec",
        ("blue", true) => "#e7f3f8",
        ("purple", true) => "#f6f3f9",
        ("pink", true) => "#faf1f5",
        ("red", true) => "#fdebec",
        _ => return None,
    };
    Some((hex, bg))
}

/// Text and background colour (hex) of a block colour, which may carry both (`red bg-blue`).
pub(crate) fn css_colors(name: &str) -> (Option<&'static str>, Option<&'static str>) {
    let (mut text, mut bg) = (None, None);
    for part in name.split_whitespace() {
        if let Some((hex, is_bg)) = css_color(part) {
            if is_bg {
                bg = Some(hex);
            } else {
                text = Some(hex);
            }
        }
    }
    (text, bg)
}

fn html_runs(runs: &[Run]) -> String {
    let mut o = String::new();
    for r in runs {
        let mut s = escape(&r.text).replace('\n', "<br>");
        let st = &r.style;
        if st.code {
            s = format!("<code>{s}</code>");
        }
        if st.bold {
            s = format!("<strong>{s}</strong>");
        }
        if st.italic {
            s = format!("<em>{s}</em>");
        }
        if st.underline {
            s = format!("<u>{s}</u>");
        }
        if st.strike {
            s = format!("<s>{s}</s>");
        }
        if let Some(url) = &st.link {
            s = format!("<a href=\"{}\">{s}</a>", escape(url));
        } else if st.mention.is_some() {
            s = format!("<span class=\"mention\">{s}</span>");
        }
        o.push_str(&s);
    }
    o
}

fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n =
            (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}

/// A standalone HTML page (images are embedded).
pub fn to_html(title: &str, blocks: &[Block], read_asset: &dyn Fn(&str) -> Option<Vec<u8>>) -> String {
    let mut body = String::new();
    for it in items(blocks) {
        let mut style = String::new();
        if it.depth > 0 {
            style.push_str(&format!("margin-left:{}em;", f32::from(it.depth) * 1.5));
        }
        let (text_hex, bg_hex) = css_colors(&it.color);
        if let Some(hex) = bg_hex {
            style.push_str(&format!("background:{hex};padding:2px 6px;border-radius:4px;"));
        }
        if let Some(hex) = text_hex {
            style.push_str(&format!("color:{hex};"));
        }
        let attr = if style.is_empty() { String::new() } else { format!(" style=\"{style}\"") };
        let prefix = match &it.marker {
            Marker::None => String::new(),
            Marker::Bullet => "<span class=\"m\">•</span> ".into(),
            Marker::Num(n) => format!("<span class=\"m\">{n}.</span> "),
            Marker::Todo(c) => format!("<span class=\"m\">{}</span> ", if *c { "☑" } else { "☐" }),
            Marker::Toggle => "<span class=\"m\">▸</span> ".into(),
        };
        let text = html_runs(&it.runs);
        match &it.kind {
            Kind::Para => {
                if text.is_empty() && it.marker == Marker::None {
                    body.push_str("<p class=\"empty\">&nbsp;</p>\n");
                } else {
                    body.push_str(&format!("<p{attr}>{prefix}{text}</p>\n"));
                }
            }
            Kind::Heading(l) => body.push_str(&format!("<h{}{attr}>{text}</h{}>\n", l + 1, l + 1)),
            Kind::Quote => body.push_str(&format!("<blockquote{attr}>{text}</blockquote>\n")),
            Kind::Callout(icon) => body.push_str(&format!(
                "<div class=\"callout\"{attr}><span class=\"icon\">{}</span><div>{text}</div></div>\n",
                escape(icon)
            )),
            Kind::Code(lang) => {
                let code: String = it.runs.iter().map(|r| r.text.as_str()).collect();
                body.push_str(&format!("<pre><code class=\"lang-{}\">{}</code></pre>\n", escape(lang), escape(&code)));
            }
            Kind::Divider => body.push_str("<hr>\n"),
            Kind::Image(path) => {
                let caption = it.runs.iter().map(|r| r.text.as_str()).collect::<String>();
                let src = if path.starts_with("http://") || path.starts_with("https://") {
                    Some(path.clone())
                } else {
                    read_asset(path).map(|b| format!("data:{};base64,{}", mime_of(path), base64(&b)))
                };
                match src {
                    Some(src) => body.push_str(&format!(
                        "<figure><img src=\"{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>\n",
                        escape(&src),
                        escape(&caption),
                        escape(&caption)
                    )),
                    None => body.push_str(&format!("<p class=\"missing\">[{}]</p>\n", escape(&caption))),
                }
            }
            Kind::File => body.push_str(&format!("<p{attr}>📎 {text}</p>\n")),
        }
    }
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>{t}</title>\n<style>\n\
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif;max-width:46rem;margin:3rem auto;padding:0 1rem;line-height:1.6;color:#37352f}}\n\
h1{{font-size:2.2rem;margin-bottom:1.5rem}}h2{{margin-top:1.8rem}}h3{{margin-top:1.4rem}}h4{{margin-top:1.2rem}}\n\
p{{margin:.3rem 0}}.m{{display:inline-block;min-width:1.2em}}.empty{{height:1rem}}\n\
code{{background:#f1f1ef;border-radius:3px;padding:0 .3em;font-family:ui-monospace,Menlo,monospace;font-size:.9em}}\n\
pre{{background:#f7f6f3;padding:1rem;border-radius:6px;overflow:auto}}pre code{{background:none;padding:0}}\n\
blockquote{{border-left:3px solid #37352f;margin:.6rem 0;padding-left:1rem}}\n\
.callout{{display:flex;gap:.6rem;background:#f1f1ef;padding:1rem;border-radius:6px;margin:.6rem 0}}\n\
.mention{{background:#f1f1ef;border-radius:3px;padding:0 .3em}}img{{max-width:100%}}figure{{margin:1rem 0}}figcaption{{color:#787774;font-size:.9rem}}\n\
a{{color:#337ea9}}hr{{border:none;border-top:1px solid #e9e9e7;margin:1.2rem 0}}\n\
</style></head><body>\n<h1>{t}</h1>\n{body}</body></html>\n",
        t = escape(title),
    )
}

// ---------------------------------------------------------------------------
// ZIP (stored) and DOCX

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

fn zip_stored(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let nlen = name.len() as u16;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0x00]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0x00]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&nlen.to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn docx_run(text: &str, st: &Style, rel: Option<&str>, size: Option<u32>, bold: bool) -> String {
    let mut props = String::new();
    if bold || st.bold {
        props.push_str("<w:b/>");
    }
    if st.italic {
        props.push_str("<w:i/>");
    }
    if st.strike {
        props.push_str("<w:strike/>");
    }
    if st.code {
        props.push_str("<w:rFonts w:ascii=\"Courier New\" w:hAnsi=\"Courier New\"/>");
    }
    if rel.is_some() {
        props.push_str("<w:color w:val=\"337EA9\"/><w:u w:val=\"single\"/>");
    } else if st.underline {
        props.push_str("<w:u w:val=\"single\"/>");
    }
    if let Some(s) = size {
        props.push_str(&format!("<w:sz w:val=\"{}\"/>", s * 2));
    }
    let mut body = String::new();
    for (k, part) in text.split('\n').enumerate() {
        if k > 0 {
            body.push_str("<w:br/>");
        }
        body.push_str(&format!("<w:t xml:space=\"preserve\">{}</w:t>", escape(part)));
    }
    let run = format!("<w:r><w:rPr>{props}</w:rPr>{body}</w:r>");
    match rel {
        Some(r) => format!("<w:hyperlink r:id=\"{r}\">{run}</w:hyperlink>"),
        None => run,
    }
}

/// A Word document (opens in Word, Pages, LibreOffice).
pub fn to_docx(title: &str, blocks: &[Block], read_asset: &dyn Fn(&str) -> Option<Vec<u8>>) -> Vec<u8> {
    let mut links: Vec<String> = Vec::new();
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut body = String::new();
    let para = |ppr: &str, runs: &str| format!("<w:p><w:pPr>{ppr}</w:pPr>{runs}</w:p>");
    body.push_str(&para("<w:spacing w:after=\"240\"/>", &docx_run(title, &Style::default(), None, Some(26), true)));
    for it in items(blocks) {
        let indent = u32::from(it.depth) * 360;
        let ind = |extra: u32| format!("<w:ind w:left=\"{}\"/>", indent + extra);
        let mut runs = String::new();
        let mut caption_after: Option<String> = None;
        let (size, bold) = match &it.kind {
            Kind::Heading(1) => (Some(20), true),
            Kind::Heading(2) => (Some(16), true),
            Kind::Heading(_) => (Some(13), true),
            Kind::Code(_) => (Some(10), false),
            _ => (None, false),
        };
        let marker = match &it.marker {
            Marker::None => String::new(),
            Marker::Bullet => "•  ".into(),
            Marker::Num(n) => format!("{n}.  "),
            Marker::Todo(c) => format!("{}  ", if *c { "☑" } else { "☐" }),
            Marker::Toggle => "▸  ".into(),
        };
        if !marker.is_empty() {
            runs.push_str(&docx_run(&marker, &Style::default(), None, size, false));
        }
        match &it.kind {
            Kind::Divider => {
                body.push_str(&para(
                    "<w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"BBBBBB\"/></w:pBdr>",
                    "",
                ));
                continue;
            }
            Kind::Image(p) => {
                let cap: String = it.runs.iter().map(|r| r.text.as_str()).collect();
                let bytes = read_asset(p);
                let ext = bytes.as_deref().and_then(|b| {
                    if b.starts_with(b"\x89PNG") {
                        Some("png")
                    } else if b.starts_with(&[0xFF, 0xD8]) {
                        Some("jpeg")
                    } else if b.starts_with(b"GIF8") {
                        Some("gif")
                    } else {
                        None
                    }
                });
                match (bytes, ext) {
                    (Some(b), Some(ext)) => {
                        let (pw, ph) = image_size(&b).unwrap_or((600, 400));
                        let (mut cx, mut cy) = (u64::from(pw.max(1)) * 9525, u64::from(ph.max(1)) * 9525);
                        let scale = (6_000_000.0 / cx as f64).min(8_000_000.0 / cy as f64).min(1.0);
                        cx = (cx as f64 * scale) as u64;
                        cy = (cy as f64 * scale) as u64;
                        let k = media.len();
                        media.push((format!("image{}.{ext}", k + 1), b));
                        runs.push_str(&format!(
                            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{id}\" name=\"Image {id}\" descr=\"{alt}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"image{id}.{ext}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"rIdI{k}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
                            id = k + 1,
                            alt = escape(&cap)
                        ));
                        if !cap.is_empty() {
                            caption_after = Some(cap);
                        }
                    }
                    _ => runs.push_str(&docx_run(
                        &format!("[{}]", if cap.is_empty() { "image" } else { &cap }),
                        &Style::default(),
                        None,
                        None,
                        false,
                    )),
                }
            }
            Kind::File => runs.push_str(&docx_run("📎 ", &Style::default(), None, None, false)),
            Kind::Callout(icon) => runs.push_str(&docx_run(&format!("{icon}  "), &Style::default(), None, None, false)),
            _ => {}
        }
        if !matches!(it.kind, Kind::Image(_)) {
            for r in &it.runs {
                let rel = r.style.link.as_ref().map(|u| {
                    let k = links.iter().position(|x| x == u).unwrap_or_else(|| {
                        links.push(u.clone());
                        links.len() - 1
                    });
                    format!("rIdL{k}")
                });
                runs.push_str(&docx_run(&r.text, &r.style, rel.as_deref(), size, bold));
            }
        }
        let ppr = match &it.kind {
            Kind::Quote => format!(
                "{}<w:pBdr><w:left w:val=\"single\" w:sz=\"18\" w:space=\"8\" w:color=\"37352F\"/></w:pBdr>",
                ind(240)
            ),
            Kind::Callout(_) => format!("{}<w:shd w:val=\"clear\" w:fill=\"F1F1EF\"/>", ind(0)),
            Kind::Code(_) => format!("{}<w:shd w:val=\"clear\" w:fill=\"F7F6F3\"/>", ind(0)),
            _ => ind(0),
        };
        body.push_str(&para(&format!("{ppr}<w:spacing w:after=\"100\"/>"), &runs));
        if let Some(cap) = caption_after {
            let st = Style { italic: true, ..Style::default() };
            body.push_str(&para(
                "<w:jc w:val=\"center\"/><w:spacing w:after=\"160\"/>",
                &docx_run(&cap, &st, None, Some(9), false),
            ));
        }
    }
    let mut rels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for (k, u) in links.iter().enumerate() {
        rels.push_str(&format!(
            "<Relationship Id=\"rIdL{k}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
            escape(u)
        ));
    }
    for (k, (name, _)) in media.iter().enumerate() {
        rels.push_str(&format!(
            "<Relationship Id=\"rIdI{k}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/{name}\"/>"
        ));
    }
    rels.push_str("</Relationships>");
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), content_types.as_bytes().to_vec()),
        ("_rels/.rels".into(), root_rels.as_bytes().to_vec()),
        ("word/document.xml".into(), document.into_bytes()),
        ("word/_rels/document.xml.rels".into(), rels.into_bytes()),
    ];
    for (name, bytes) in media {
        files.push((format!("word/media/{name}"), bytes));
    }
    let refs: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip_stored(&refs)
}

// ---------------------------------------------------------------------------
// shared helpers for images

/// Pixel size of a PNG, JPEG or GIF (read from the file header).
pub(crate) fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() > 24 && b.starts_with(b"\x89PNG") {
        let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
        return Some((w, h));
    }
    if b.len() > 10 && (b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) {
        return Some((u32::from(u16::from_le_bytes([b[6], b[7]])), u32::from(u16::from_le_bytes([b[8], b[9]]))));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let m = b[i + 1];
            if m == 0xFF {
                i += 1;
                continue;
            }
            if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
                let h = u32::from(u16::from_be_bytes([b[i + 5], b[i + 6]]));
                let w = u32::from(u16::from_be_bytes([b[i + 7], b[i + 8]]));
                return Some((w, h));
            }
            if matches!(m, 0xD8 | 0x01) || (0xD0..=0xD7).contains(&m) {
                i += 2;
                continue;
            }
            let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
            i += 2 + len;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::from_markdown;

    fn sample() -> Vec<Block> {
        from_markdown("# Title\n\nSome **bold** and [a link](https://example.com) with é and €.\n\n1. one\n2. two\n\n- [x] done\n\n```rust\nfn main() {}\n```\n\n---\n\n> quoted\n")
    }

    #[test]
    fn html_has_structure_and_escapes() {
        let html = to_html("A & B", &sample(), &|_| None);
        assert!(html.contains("<title>A &amp; B</title>"));
        assert!(html.contains("<h2>Title</h2>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<a href=\"https://example.com\">a link</a>"));
        assert!(html.contains("<span class=\"m\">2.</span>"));
        assert!(html.contains("<blockquote>quoted</blockquote>"));
        assert!(html.contains("fn main() {}"));
    }

    #[test]
    fn table_of_contents_lists_headings() {
        let blocks = from_markdown("<!--toc-->\n\n# One\n\n## Two\n");
        let html = to_html("T", &blocks, &|_| None);
        assert!(html.contains("<p>One</p>"));
        assert!(html.contains("margin-left:1.5em;\">Two</p>"));
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn crc_matches_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn docx_is_a_zip_with_the_document() {
        let bytes = to_docx("T", &sample(), &|_| None);
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("word/document.xml"));
        assert!(text.contains("https://example.com"));
        assert!(text.contains("<w:b/>"));
    }

    const PIXEL_PNG: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    fn pixel() -> Vec<u8> {
        crate::vault::decode(PIXEL_PNG).unwrap()
    }

    #[test]
    fn pdf_is_well_formed_and_paginates() {
        let long: String = "word ".repeat(2000);
        let mut blocks = sample();
        blocks.push(Block::paragraph(&long));
        let short = crate::pdf::to_pdf("Title é", &sample(), &|_| None);
        let bytes = crate::pdf::to_pdf("Title é", &blocks, &|_| None);
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(16)..]).contains("%%EOF"));
        assert!(bytes.len() > short.len(), "long text spans more pages");
    }

    #[test]
    fn images_are_embedded_in_pdf_and_word() {
        let blocks = from_markdown("![A red pixel](assets/p.png)\n");
        let read = |p: &str| (p == "assets/p.png").then(pixel);
        let docx = to_docx("T", &blocks, &read);
        assert!(String::from_utf8_lossy(&docx).contains("word/media/image1.png"));
        let with = crate::pdf::to_pdf("T", &blocks, &read);
        let without = crate::pdf::to_pdf("T", &blocks, &|_| None);
        assert!(String::from_utf8_lossy(&with).contains("/Subtype/Image"));
        assert!(!String::from_utf8_lossy(&without).contains("/Subtype/Image"));
        assert_eq!(image_size(&pixel()), Some((1, 1)));
    }
}
