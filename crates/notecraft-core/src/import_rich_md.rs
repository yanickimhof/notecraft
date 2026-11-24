//! Rich Markdown (Markdown with XML-like tags, as some assistants and APIs write it) turned into the
//! Markdown NoteCraft reads.
//!
//! Rich Markdown is plain Markdown for simple things and XML-like tags for the rest: `<callout>`,
//! `<table>`, `<toggle>`, `<columns>`, `<span color>`, `<image source>`, `<file source>` and so on.
//! [`convert`] rewrites those into our syntax (callouts as `> [!NOTE]`, toggles as `<details>`,
//! columns and tables of contents as comments, tables as pipe tables, colours as `<!--c:…-->`),
//! and reports what has no equivalent so nothing disappears silently.

use crate::blocks::is_block_color;
use std::collections::BTreeMap;

const KNOWN: &[&str] = &[
    "b",
    "i",
    "s",
    "u",
    "span",
    "mention",
    "date",
    "link-preview",
    "br",
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "quote",
    "callout",
    "toggle",
    "ul",
    "ol",
    "li",
    "todo",
    "equation",
    "table-of-contents",
    "image",
    "pdf",
    "file",
    "audio",
    "video",
    "bookmark",
    "embed",
    "external_object_instance",
    "page",
    "folder",
    "database",
    "data-source",
    "synced-block",
    "synced-block-reference",
    "meeting-notes",
    "meeting-notes-section-notes",
    "meeting-notes-section-summary",
    "meeting-notes-section-transcript",
    "columns",
    "column",
    "tabs",
    "tab",
    "table",
    "colgroup",
    "col",
    "tr",
    "th",
    "td",
    "mail",
    "custom-block",
    "empty-block",
];

/// Tags that never have a closing tag.
const VOID: &[&str] = &["br", "col", "empty-block", "table-of-contents", "date", "link-preview", "custom-block"];

#[derive(Debug, Clone)]
enum Tok {
    Text(String),
    Open { name: String, attrs: BTreeMap<String, String>, selfclose: bool },
    Close(String),
}

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Tag { name: String, attrs: BTreeMap<String, String>, children: Vec<Node> },
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_'
}

/// Read one tag at the start of `s` (which begins with `<`); `None` when it is not one of ours.
fn parse_tag(s: &str) -> Option<(Tok, usize)> {
    let b = s.as_bytes();
    if b.len() < 3 {
        return None;
    }
    let closing = b[1] == b'/';
    let mut i = if closing { 2 } else { 1 };
    let start = i;
    while i < b.len() && is_name_char(b[i]) {
        i += 1;
    }
    if i == start {
        return None;
    }
    let name = &s[start..i];
    if !KNOWN.contains(&name) {
        return None;
    }
    if closing {
        return (b.get(i) == Some(&b'>')).then(|| (Tok::Close(name.to_string()), i + 1));
    }
    if !matches!(b.get(i), Some(b' ' | b'\n' | b'\t' | b'>' | b'/')) {
        return None;
    }
    let mut attrs = BTreeMap::new();
    loop {
        while i < b.len() && matches!(b[i], b' ' | b'\n' | b'\t') {
            i += 1;
        }
        match b.get(i) {
            None => return None,
            Some(b'>') => return Some((Tok::Open { name: name.to_string(), attrs, selfclose: false }, i + 1)),
            Some(b'/') => {
                return (b.get(i + 1) == Some(&b'>'))
                    .then(|| (Tok::Open { name: name.to_string(), attrs, selfclose: true }, i + 2));
            }
            _ => {}
        }
        let ks = i;
        while i < b.len() && !matches!(b[i], b'=' | b' ' | b'\n' | b'\t' | b'>' | b'/') {
            i += 1;
        }
        if i == ks {
            return None;
        }
        let key = s[ks..i].to_string();
        if b.get(i) == Some(&b'=') {
            i += 1;
            let q = *b.get(i)?;
            if q != b'"' && q != b'\'' {
                return None;
            }
            let vs = i + 1;
            let end = s[vs..].find(q as char)? + vs;
            attrs.insert(key, s[vs..end].to_string());
            i = end + 1;
        } else {
            attrs.insert(key, "true".to_string());
        }
    }
}

/// Split into text and tags, leaving code (fenced or inline) untouched.
fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<Tok>| {
        if !buf.is_empty() {
            out.push(Tok::Text(std::mem::take(buf)));
        }
    };
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut line_start = true;
    while i < text.len() {
        if line_start {
            let rest = &text[i..];
            let indent = rest.len() - rest.trim_start_matches(' ').len();
            let body = &rest[indent..];
            let fence_char = body.chars().next().filter(|c| (*c == '`' || *c == '~') && indent <= 3);
            if let Some(fc) = fence_char {
                let run = body.chars().take_while(|c| *c == fc).count();
                if run >= 3 {
                    // copy through the closing fence line
                    let mut j = i;
                    let mut first = true;
                    let mut end = text.len();
                    for line in text[i..].split_inclusive('\n') {
                        if !first {
                            let t = line.trim_start_matches(' ');
                            let n = t.chars().take_while(|c| *c == fc).count();
                            if n >= run && t[n..].trim().is_empty() {
                                end = j + line.len();
                                break;
                            }
                        }
                        first = false;
                        j += line.len();
                    }
                    buf.push_str(&text[i..end]);
                    i = end;
                    line_start = true;
                    continue;
                }
            }
        }
        let c = bytes[i];
        if c == b'`' {
            let run = text[i..].bytes().take_while(|b| *b == b'`').count();
            let ticks = &text[i..i + run];
            if let Some(pos) = text[i + run..].find(ticks) {
                let end = i + run + pos + run;
                buf.push_str(&text[i..end]);
                i = end;
                line_start = false;
                continue;
            }
            buf.push_str(ticks);
            i += run;
            line_start = false;
            continue;
        }
        if c == b'<' {
            if let Some((tok, len)) = parse_tag(&text[i..]) {
                flush(&mut buf, &mut out);
                out.push(tok);
                i += len;
                line_start = false;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap();
        buf.push(ch);
        line_start = ch == '\n';
        i += ch.len_utf8();
    }
    flush(&mut buf, &mut out);
    out
}

fn build(toks: Vec<Tok>) -> Vec<Node> {
    struct Frame {
        name: String,
        attrs: BTreeMap<String, String>,
        children: Vec<Node>,
    }
    let mut stack: Vec<Frame> = vec![Frame { name: String::new(), attrs: BTreeMap::new(), children: Vec::new() }];
    for t in toks {
        match t {
            Tok::Text(s) => stack.last_mut().unwrap().children.push(Node::Text(s)),
            Tok::Open { name, attrs, selfclose } => {
                if selfclose || VOID.contains(&name.as_str()) {
                    stack.last_mut().unwrap().children.push(Node::Tag { name, attrs, children: Vec::new() });
                } else {
                    stack.push(Frame { name, attrs, children: Vec::new() });
                }
            }
            Tok::Close(name) => {
                if !stack.iter().skip(1).any(|f| f.name == name) {
                    stack.last_mut().unwrap().children.push(Node::Text(format!("</{name}>")));
                    continue;
                }
                while stack.len() > 1 {
                    let f = stack.pop().unwrap();
                    let done = f.name == name;
                    stack.last_mut().unwrap().children.push(Node::Tag {
                        name: f.name,
                        attrs: f.attrs,
                        children: f.children,
                    });
                    if done {
                        break;
                    }
                }
            }
        }
    }
    while stack.len() > 1 {
        let f = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(Node::Tag { name: f.name, attrs: f.attrs, children: f.children });
    }
    stack.pop().unwrap().children
}

/// A callout whose icon is glued to its first line (`⚠️ **Title**`): split the pictogram from the text.
/// Only real pictographs count, so a line starting with « or → is left alone.
pub(crate) fn split_leading_emoji(s: &str) -> Option<(String, String)> {
    let is_picto =
        |c: char| matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x2300..=0x23FF);
    let mut chars = s.char_indices().peekable();
    let (_, first) = chars.next()?;
    if !is_picto(first) {
        return None;
    }
    let mut end = first.len_utf8();
    while let Some(&(i, c)) = chars.peek() {
        let joiner = matches!(c as u32, 0xFE0F | 0x200D | 0x1F3FB..=0x1F3FF);
        let after_zwj = s[..i].ends_with('\u{200D}') && is_picto(c);
        if joiner || after_zwj {
            end = i + c.len_utf8();
            chars.next();
        } else {
            break;
        }
    }
    let rest = s[end..].trim();
    if rest.is_empty() {
        return None;
    }
    Some((s[..end].to_string(), rest.to_string()))
}

/// Does this text use rich-Markdown tags (outside code)? Plain Markdown never does, so it is left alone.
pub fn looks_like_rich_markdown(text: &str) -> bool {
    if !text.contains('<') {
        return false;
    }
    tokenize(text).iter().any(|t| match t {
        Tok::Open { name, attrs, .. } => match name.as_str() {
            "callout"
            | "table-of-contents"
            | "toggle"
            | "columns"
            | "column"
            | "mention"
            | "quote"
            | "todo"
            | "equation"
            | "empty-block"
            | "link-preview"
            | "synced-block"
            | "synced-block-reference"
            | "meeting-notes"
            | "tabs"
            | "custom-block"
            | "external_object_instance"
            | "data-source" => true,
            "image" | "pdf" | "file" | "audio" | "video" | "embed" | "bookmark" => attrs.contains_key("source"),
            "page" | "database" | "folder" => attrs.contains_key("url"),
            "span" | "li" => attrs.contains_key("color"),
            "table" => attrs.contains_key("header-row") || attrs.contains_key("fit-page-width"),
            _ => false,
        },
        _ => false,
    })
}

#[derive(Default)]
struct Ctx {
    lossy: BTreeMap<&'static str, usize>,
    notes: Vec<String>,
}

impl Ctx {
    fn lose(&mut self, what: &'static str) {
        *self.lossy.entry(what).or_default() += 1;
    }
}

fn attr<'a>(attrs: &'a BTreeMap<String, String>, k: &str) -> Option<&'a str> {
    attrs.get(k).map(String::as_str)
}

/// `blue_bg` → `bg-blue`, `red` → `red`; anything else is no colour.
fn block_color(attrs: &BTreeMap<String, String>) -> Option<String> {
    let c = attr(attrs, "color")?;
    let name = match c.strip_suffix("_bg") {
        Some(base) => format!("bg-{base}"),
        None => c.to_string(),
    };
    is_block_color(&name).then_some(name)
}

fn color_suffix(attrs: &BTreeMap<String, String>) -> String {
    block_color(attrs).map(|c| format!(" <!--c:{c}-->")).unwrap_or_default()
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "p" | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "quote"
            | "callout"
            | "toggle"
            | "ul"
            | "ol"
            | "li"
            | "todo"
            | "equation"
            | "table-of-contents"
            | "image"
            | "pdf"
            | "file"
            | "audio"
            | "video"
            | "bookmark"
            | "embed"
            | "external_object_instance"
            | "page"
            | "folder"
            | "database"
            | "data-source"
            | "synced-block"
            | "synced-block-reference"
            | "meeting-notes"
            | "meeting-notes-section-notes"
            | "meeting-notes-section-summary"
            | "meeting-notes-section-transcript"
            | "columns"
            | "tabs"
            | "table"
            | "mail"
            | "custom-block"
            | "empty-block"
            | "link-preview"
    )
}

fn single_line(s: &str) -> String {
    s.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Wrap `inner` in a Markdown marker, keeping surrounding spaces outside it.
fn wrap(open: &str, close: &str, inner: &str) -> String {
    let core = inner.trim();
    if core.is_empty() {
        return inner.to_string();
    }
    let lead = &inner[..inner.len() - inner.trim_start().len()];
    let trail = &inner[inner.trim_end().len()..];
    format!("{lead}{open}{core}{close}{trail}")
}

fn link_target(path: &str) -> String {
    if path.contains(' ') || path.contains(')') {
        format!("<{path}>")
    } else {
        path.to_string()
    }
}

/// A display name for a file: the caption, else the `name=` of a file reference, else the last path part.
fn file_name(source: &str, caption: &str) -> String {
    if !caption.trim().is_empty() {
        return single_line(caption);
    }
    if let Some(q) = source.split_once('?').map(|(_, q)| q) {
        for kv in q.split('&') {
            if let Some(v) = kv.strip_prefix("name=") {
                return crate::import_export::percent_decode(v);
            }
        }
    }
    let path = source.split(['?', '#']).next().unwrap_or(source);
    let last = path.rsplit('/').next().unwrap_or(path);
    let name = crate::import_export::percent_decode(last);
    if name.is_empty() {
        "file".to_string()
    } else {
        name
    }
}

fn inline(name: &str, attrs: &BTreeMap<String, String>, children: &[Node], cx: &mut Ctx) -> String {
    match name {
        "b" => wrap("**", "**", &render(children, cx)),
        "i" => wrap("*", "*", &render(children, cx)),
        "s" => wrap("~~", "~~", &render(children, cx)),
        "u" => wrap("<u>", "</u>", &render(children, cx)),
        "br" => "\n".to_string(),
        "span" => {
            let inner = render(children, cx);
            match attr(attrs, "color") {
                Some(c) if c.ends_with("_bg") => {
                    cx.lose("text highlight colours");
                    inner
                }
                Some(c) if is_block_color(c) => wrap(&format!("<span style=\"color:{c}\">"), "</span>", &inner),
                _ => inner,
            }
        }
        "mention" => {
            let label = single_line(&render(children, cx));
            let url = attr(attrs, "url").unwrap_or("");
            if url.starts_with("user://") || url.starts_with("agent://") || url.starts_with("group://") {
                return if label.is_empty() {
                    "@mention".into()
                } else {
                    format!("@{}", label.trim_start_matches('@'))
                };
            }
            if url.starts_with("collection://") || attr(attrs, "type") == Some("database") {
                cx.lose("database mentions kept as text");
                return label;
            }
            match (label.is_empty(), url.is_empty()) {
                (_, true) => label,
                (true, false) => format!("[{url}]({url})"),
                (false, false) => format!("[{label}]({})", link_target(url)),
            }
        }
        "date" => {
            let start = attr(attrs, "start").unwrap_or("");
            let mut s = start.to_string();
            if let Some(t) = attr(attrs, "start-time") {
                s.push(' ');
                s.push_str(t);
            }
            if let Some(end) = attr(attrs, "end") {
                s.push_str(" → ");
                s.push_str(end);
                if let Some(t) = attr(attrs, "end-time") {
                    s.push(' ');
                    s.push_str(t);
                }
            }
            s
        }
        _ => render(children, cx),
    }
}

/// Title (up to the first blank line) and children (the rest) of a container's body.
fn split_body(body: &str) -> (String, String) {
    let t = body.trim_matches('\n');
    match t.find("\n\n") {
        Some(p) => (t[..p].trim().to_string(), t[p + 2..].trim_matches('\n').to_string()),
        None => (t.trim().to_string(), String::new()),
    }
}

fn indent(s: &str, by: &str) -> String {
    s.split('\n')
        .map(|l| if l.trim().is_empty() { String::new() } else { format!("{by}{l}") })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove the indentation (tabs) given to a block's children, so they read as top-level text.
fn dedent(s: &str) -> String {
    let lead = |l: &str| l.chars().take_while(|c| *c == '\t').count();
    let min = s.split('\n').filter(|l| !l.trim().is_empty()).map(lead).min().unwrap_or(0);
    s.split('\n').map(|l| if l.trim().is_empty() { "" } else { &l[min.min(lead(l))..] }).collect::<Vec<_>>().join("\n")
}

/// A text colour written as `{color="red"}` at the end of a line becomes NoteCraft's colour marker.
fn color_suffix_lines(s: &str) -> String {
    let mut in_fence = false;
    let mut out = Vec::new();
    for l in s.split('\n') {
        if l.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        let t = l.trim_end();
        // `{color="x"} <!--c:y-->`: the block already carries a colour, the literal suffix is dropped
        if !in_fence && t.ends_with("-->") {
            if let Some(at) = t.rfind(" {color=\"") {
                if let Some(end) = t[at..].find("\"} <!--c:") {
                    out.push(format!("{}{}", &t[..at], &t[at + end + 2..]));
                    continue;
                }
            }
        }
        if !in_fence && t.ends_with("\"}") {
            if let Some(at) = t.rfind(" {color=\"") {
                let name = &t[at + " {color=\"".len()..t.len() - 2];
                let name = match name.strip_suffix("_bg") {
                    Some(b) => format!("bg-{b}"),
                    None => name.to_string(),
                };
                let head = t[..at].to_string();
                if crate::blocks::is_block_color(&name) {
                    out.push(format!("{head} <!--c:{name}-->"));
                } else {
                    out.push(head);
                }
                continue;
            }
        }
        out.push(l.to_string());
    }
    out.join("\n")
}

/// `[^https://…]` source references are not content.
fn strip_citations(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(at) = rest.find("[^http") {
        match rest[at..].find(']') {
            Some(end) => {
                out.push_str(rest[..at].trim_end_matches(' '));
                rest = &rest[at + end + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// Children indented with tabs (inside lists) use NoteCraft's two spaces.
fn tabs_to_spaces(s: &str) -> String {
    let mut in_fence = false;
    s.split('\n')
        .map(|l| {
            if l.trim_start().starts_with("```") {
                in_fence = !in_fence;
            }
            if in_fence {
                return l.to_string();
            }
            let n = l.chars().take_while(|c| *c == '\t').count();
            format!("{}{}", "  ".repeat(n), &l[n..])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote_lines(s: &str) -> Vec<String> {
    s.trim_matches('\n')
        .split('\n')
        .map(|l| if l.trim().is_empty() { ">".to_string() } else { format!("> {l}") })
        .collect()
}

fn add_suffix_last_line(s: String, suffix: &str) -> String {
    if suffix.is_empty() {
        s
    } else {
        format!("{}{suffix}", s.trim_end())
    }
}

fn list_item(li: &Node, marker: &str, cx: &mut Ctx) -> String {
    let Node::Tag { attrs, children, .. } = li else { return String::new() };
    let (title, kids) = split_body(&render(children, cx));
    let mut lines = title.split('\n');
    let first = lines.next().unwrap_or("").to_string();
    let mut out = format!("{marker}{first}");
    for l in lines {
        out.push('\n');
        out.push_str(&format!("  {l}"));
    }
    out = add_suffix_last_line(out, &color_suffix(attrs));
    if !kids.is_empty() {
        out.push('\n');
        out.push_str(&indent(&kids, "  "));
    }
    out
}

fn table(children: &[Node], cx: &mut Ctx) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for n in children {
        let Node::Tag { name, children: cells, .. } = n else { continue };
        if name != "tr" {
            continue;
        }
        let mut row = Vec::new();
        for c in cells {
            let Node::Tag { name, attrs: cell_attrs, children } = c else { continue };
            if name == "td" || name == "th" {
                let mut text = single_line(&render(children, cx));
                // a text colour on a cell keeps showing; background colours have no cell equivalent
                if let Some(col) = attr(cell_attrs, "color").filter(|c| !c.ends_with("_bg") && *c != "default") {
                    if !text.is_empty() && !text.contains("<span") {
                        text = format!("<span style=\"color:{col}\">{text}</span>");
                    }
                }
                row.push(text.replace('|', "\\|"));
            }
        }
        rows.push(row);
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let mut lines = Vec::new();
    for (i, r) in rows.iter_mut().enumerate() {
        r.resize(width, String::new());
        lines.push(format!("| {} |", r.join(" | ")));
        if i == 0 {
            lines.push(format!("| {} |", vec!["---"; width].join(" | ")));
        }
    }
    lines.join("\n")
}

fn block(name: &str, attrs: &BTreeMap<String, String>, children: &[Node], cx: &mut Ctx) -> String {
    match name {
        "p" => {
            let t = render(children, cx);
            if t.trim().is_empty() {
                "&nbsp;".to_string()
            } else {
                add_suffix_last_line(t.trim_matches('\n').to_string(), &color_suffix(attrs))
            }
        }
        "empty-block" => "&nbsp;".to_string(),
        "h1" | "h2" | "h3" | "h4" => {
            let level = name[1..].parse::<usize>().unwrap_or(1);
            let t = single_line(&render(children, cx));
            format!("{} {}{}", "#".repeat(level), t, color_suffix(attrs))
        }
        "quote" => {
            let lines = quote_lines(&dedent(&render(children, cx)));
            let joined = lines.join("\n");
            add_suffix_last_line(joined, &color_suffix(attrs))
        }
        "callout" => {
            let icon =
                attr(attrs, "icon").filter(|i| i.chars().next().is_some_and(|c| !c.is_alphanumeric() && !c.is_ascii()));
            let mut body = dedent(&render(children, cx));
            // a callout without an icon whose text starts with a pictograph: that pictograph is its icon
            let icon = match icon {
                Some(i) => i.to_string(),
                None => match split_leading_emoji(body.trim_start()) {
                    Some((ic, rest)) => {
                        body = rest;
                        ic
                    }
                    None => "💡".to_string(),
                },
            };
            let suffix = color_suffix(attrs);
            let body_lines: Vec<&str> = body.trim_matches('\n').split('\n').collect();
            // the callout colour closes its header, before any list items: a marker at the very end
            // would read as the colour of the last item
            let head_len = body_lines.iter().position(|l| crate::blocks::is_list_line(l)).unwrap_or(body_lines.len());
            let mut lines: Vec<String> = Vec::new();
            if !suffix.is_empty() && head_len == 0 {
                lines.push(format!("> {icon}{suffix}"));
            }
            for (k, l) in body_lines.iter().enumerate() {
                let l = if !suffix.is_empty()
                    && head_len > 0
                    && Some(k) == body_lines[..head_len].iter().rposition(|x| !x.trim().is_empty())
                {
                    format!("{}{suffix}", l.trim_end())
                } else {
                    (*l).to_string()
                };
                if k == 0 && head_len > 0 {
                    lines.push(format!("> {icon} {l}").trim_end().to_string());
                } else if l.trim().is_empty() {
                    lines.push(">".to_string());
                } else {
                    lines.push(format!("> {l}"));
                }
            }
            format!("> [!NOTE]\n{}", lines.join("\n"))
        }
        "toggle" => {
            if attr(attrs, "heading").is_some() {
                cx.lose("toggle headings turned into plain toggles");
            }
            let (title, kids) = split_body(&render(children, cx));
            let kids = dedent(&kids);
            let mut out = format!("<details>\n<summary>{}</summary>\n", single_line(&title));
            if !kids.is_empty() {
                out.push('\n');
                out.push_str(&kids);
                out.push('\n');
            }
            out.push_str("</details>");
            out
        }
        "ul" | "ol" => {
            let ordered = name == "ol";
            let mut n = attr(attrs, "start").and_then(|s| s.parse::<usize>().ok()).unwrap_or(1);
            let mut items = Vec::new();
            for c in children {
                match c {
                    Node::Tag { name, .. } if name == "li" => {
                        let marker = if ordered { format!("{n}. ") } else { "- ".to_string() };
                        items.push(list_item(c, &marker, cx));
                        n += 1;
                    }
                    Node::Text(t) if t.trim().is_empty() => {}
                    other => {
                        let t = render(std::slice::from_ref(other), cx);
                        if !t.trim().is_empty() {
                            items.push(indent(t.trim(), "  "));
                        }
                    }
                }
            }
            items.join("\n")
        }
        "li" => {
            list_item(&Node::Tag { name: "li".into(), attrs: attrs.clone(), children: children.to_vec() }, "- ", cx)
        }
        "todo" => {
            let mark = if attr(attrs, "checked") == Some("true") { "x" } else { " " };
            format!("- [{mark}] {}{}", single_line(&render(children, cx)), color_suffix(attrs))
        }
        "equation" => {
            let tex: String = children
                .iter()
                .map(|n| match n {
                    Node::Text(t) => t.clone(),
                    _ => String::new(),
                })
                .collect();
            format!("$$\n{}\n$$", tex.trim())
        }
        "table-of-contents" => "<!--toc-->".to_string(),
        "image" => {
            let src = attr(attrs, "source").or(attr(attrs, "src")).unwrap_or("");
            let caption = single_line(&render(children, cx)).replace(['[', ']'], "");
            format!("![{caption}]({})", link_target(src))
        }
        "pdf" | "file" | "audio" | "video" => {
            let src = attr(attrs, "source").or(attr(attrs, "src")).or(attr(attrs, "url")).unwrap_or("");
            let name = file_name(src, &render(children, cx)).replace(['[', ']'], "");
            format!("[📎 {name}]({})", link_target(src))
        }
        "bookmark" | "embed" | "link-preview" | "external_object_instance" => {
            let url = attr(attrs, "source").or(attr(attrs, "url")).unwrap_or("");
            let title = single_line(&render(children, cx)).replace("--", "-");
            if url.is_empty() {
                cx.lose("embedded integrations kept as text");
                return title;
            }
            let url = url.replace("--", "%2D%2D").replace(char::is_whitespace, "%20");
            format!("<!--bookmark:{url} {title}-->")
        }
        "page" | "folder" => {
            let title = single_line(&render(children, cx));
            let url = attr(attrs, "url").unwrap_or("");
            cx.notes.push(
                format!("Sub-page not imported with this page (import it separately): {title} {url}")
                    .trim()
                    .to_string(),
            );
            if url.is_empty() {
                format!("📄 {title}")
            } else {
                format!("📄 [{title}]({url})")
            }
        }
        "database" | "data-source" => {
            let title = single_line(&render(children, cx));
            let url = attr(attrs, "url").unwrap_or("");
            cx.notes.push(
                format!("Database not imported with this page (import it separately): {title} {url}")
                    .trim()
                    .to_string(),
            );
            format!("🗂️ {title}")
        }
        "synced-block" | "synced-block-reference" => {
            cx.lose("synced blocks copied as ordinary blocks");
            render(children, cx).trim_matches('\n').to_string()
        }
        "meeting-notes" => render(children, cx).trim_matches('\n').to_string(),
        "meeting-notes-section-notes" | "meeting-notes-section-summary" => {
            let t = render(children, cx).trim_matches('\n').to_string();
            // an empty notes section is exported as a lone empty paragraph
            if t.trim() == "&nbsp;" {
                String::new()
            } else {
                t
            }
        }
        "meeting-notes-section-transcript" => {
            let t = dedent(render(children, cx).trim_matches('\n'));
            // without the transcript only a placeholder is sent
            if t.trim().is_empty() || t.trim_start().starts_with("Transcript omitted") {
                cx.lose("meeting transcripts (fetch the page with the transcript included)");
                String::new()
            } else {
                format!("<details>\n<summary>Transcript</summary>\n\n{}\n</details>", t.trim())
            }
        }
        "columns" => {
            let mut cols = Vec::new();
            for c in children {
                if let Node::Tag { name, children, .. } = c {
                    if name == "column" {
                        cols.push(dedent(render(children, cx).trim_matches('\n')));
                    }
                }
            }
            if cols.is_empty() {
                return String::new();
            }
            format!("<!--columns-->\n{}\n<!--/columns-->", cols.join("\n<!--column-->\n"))
        }
        "tabs" => {
            cx.lose("tabs turned into toggles");
            let mut out = Vec::new();
            for c in children {
                if let Node::Tag { name, children, .. } = c {
                    if name == "tab" {
                        out.push(block("toggle", &BTreeMap::new(), children, cx));
                    }
                }
            }
            out.join("\n\n")
        }
        "table" => table(children, cx),
        "mail" => {
            cx.lose("e-mail blocks kept as text");
            let subject = attr(attrs, "subject").unwrap_or("");
            let body = render(children, cx);
            format!("✉️ {subject}\n\n{}", body.trim()).trim().to_string()
        }
        "custom-block" => {
            cx.lose("custom blocks (not available outside the source app)");
            String::new()
        }
        _ => render(children, cx),
    }
}

fn render(nodes: &[Node], cx: &mut Ctx) -> String {
    let mut out = String::new();
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Tag { name, attrs, children } if is_block_tag(name) => {
                let b = block(name, attrs, children, cx);
                if b.is_empty() {
                    continue;
                }
                let trimmed = out.trim_end_matches([' ', '\t']).len();
                out.truncate(trimmed);
                if !out.is_empty() && !out.ends_with("\n\n") {
                    if out.ends_with('\n') {
                        out.push('\n');
                    } else {
                        out.push_str("\n\n");
                    }
                }
                out.push_str(&b);
                out.push_str("\n\n");
            }
            Node::Tag { name, attrs, children } => out.push_str(&inline(name, attrs, children, cx)),
        }
    }
    out
}

fn tidy(s: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for l in s.lines() {
        if l.trim().is_empty() {
            blank += 1;
            if blank <= 1 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(l.trim_end());
            out.push('\n');
        }
    }
    out.trim_matches('\n').to_string() + "\n"
}

/// A callout cannot hold an image block, so an image that opens a callout moves just
/// before it (its caption line, repeated from the alt text, is dropped).
fn hoist_callout_images(md: &str) -> String {
    let lines: Vec<&str> = md.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("> [!NOTE]") && i + 1 < lines.len() {
            let mut j = i + 1;
            let mut hoisted: Vec<String> = Vec::new();
            let mut body: Vec<String> = Vec::new();
            while j < lines.len() && lines[j].trim_start().starts_with('>') {
                let l = lines[j].trim_start();
                let inner = l.trim_start_matches('>').trim_start();
                let (icon, rest) = match inner.split_once(' ') {
                    Some((a, b)) if j == i + 1 && !a.starts_with('!') && !a.starts_with('-') => (Some(a), b),
                    _ => (None, inner),
                };
                if rest.starts_with("![") && rest.ends_with(')') && rest.contains("](") {
                    let alt = rest[2..rest.find("](").unwrap_or(2)].to_string();
                    hoisted.push(rest.to_string());
                    if let Some(ic) = icon {
                        body.push(format!("> {ic}"));
                    }
                    if j + 1 < lines.len() && lines[j + 1].trim_start().trim_start_matches('>').trim() == alt.trim() {
                        j += 1;
                    }
                } else {
                    body.push(lines[j].to_string());
                }
                j += 1;
            }
            for h in &hoisted {
                out.push(h.clone());
                out.push(String::new());
            }
            // an icon left alone on its line joins the next line, as callouts are written
            if body.len() > 1
                && body[0].trim_start_matches('>').trim().chars().count() <= 3
                && !body[1].trim_start_matches('>').trim().is_empty()
            {
                let icon = body[0].trim_start_matches('>').trim().to_string();
                let next = body[1].trim_start_matches('>').trim_start().to_string();
                body.splice(0..2, [format!("> {icon} {next}")]);
            }
            out.push(lines[i].to_string());
            out.extend(body);
            i = j;
        } else {
            out.push(lines[i].to_string());
            i += 1;
        }
    }
    out.join("\n")
}

/// Turn rich Markdown into the Markdown NoteCraft reads, plus notes on what could not be kept.
pub fn convert(text: &str) -> (String, Vec<String>) {
    let nodes = build(tokenize(text));
    let mut cx = Ctx::default();
    let body =
        tidy(&hoist_callout_images(&tabs_to_spaces(&color_suffix_lines(&strip_citations(&render(&nodes, &mut cx))))));
    let mut notes = std::mem::take(&mut cx.notes);
    for (what, n) in &cx.lossy {
        notes.push(format!("Rich Markdown: {what} ({n})"));
    }
    (body, notes)
}

/// Parse Markdown that may be rich: tags are converted first, plain Markdown is read as is.
pub fn blocks_from_markdown(md: &str) -> (Vec<crate::blocks::Block>, Vec<String>) {
    if looks_like_rich_markdown(md) {
        let (converted, notes) = convert(md);
        (crate::blocks::from_markdown(&converted), notes)
    } else {
        (crate::blocks::from_markdown(md), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{self, BlockKind};

    fn conv(s: &str) -> String {
        convert(s).0
    }

    #[test]
    fn plain_markdown_is_not_touched() {
        assert!(!looks_like_rich_markdown("# Title\n\nSome **bold** text and `<callout>` in code.\n"));
        assert!(!looks_like_rich_markdown("```\n<callout icon=\"x\">\n```\n"));
        assert!(looks_like_rich_markdown("<callout icon=\"🎯\">\nHi\n</callout>"));
        assert!(looks_like_rich_markdown("<span color=\"red\">x</span>"));
        assert!(!looks_like_rich_markdown("line<br>break and <u>under</u>"));
    }

    #[test]
    fn a_callout_becomes_a_callout_block_with_its_icon_and_colour() {
        let md = conv("<callout icon=\"🎯\" color=\"blue_bg\">\nObjectif de la page\n\nSecond paragraph\n</callout>\n");
        let b = blocks::from_markdown(&md);
        assert_eq!(b.len(), 1, "{md}");
        assert_eq!(b[0].kind, BlockKind::Callout { icon: "🎯".into() });
        assert!(b[0].text.starts_with("Objectif de la page"), "{:?}", b[0].text);
        assert!(b[0].text.contains("Second paragraph"));
        assert_eq!(b[0].color, "bg-blue");
    }

    #[test]
    fn the_table_of_contents_and_headings_survive() {
        let b = blocks::from_markdown(&conv("<table-of-contents />\n\n## 1. Fiche\n\n### Sous-titre\n"));
        assert_eq!(b[0].kind, BlockKind::Toc);
        assert_eq!(b[1].kind, BlockKind::Heading(2));
        assert_eq!(b[2].kind, BlockKind::Heading(3));
    }

    #[test]
    fn an_html_table_becomes_a_table_block_with_every_cell() {
        let md = conv(
            "<table header-row=\"true\">\n<colgroup>\n<col width=\"170\" />\n<col />\n</colgroup>\n<tr>\n<td>\nÉlément\n</td>\n<td>\nDétail\n</td>\n</tr>\n<tr>\n<td>\nMission\n</td>\n<td>\nUn <b>long</b> texte | avec barre<br />et saut\n</td>\n</tr>\n</table>\n",
        );
        let b = blocks::from_markdown(&md);
        assert_eq!(b.len(), 1, "{md}");
        assert_eq!(b[0].kind, BlockKind::Table);
        let cells = blocks::table_cells(&b[0].text);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0], vec!["Élément", "Détail"]);
        assert_eq!(cells[1][0], "Mission");
        assert!(
            cells[1][1].contains("**long**") && cells[1][1].contains("| avec barre") && cells[1][1].contains("saut"),
            "{:?}",
            cells[1][1]
        );
    }

    #[test]
    fn coloured_list_items_keep_their_children_and_colour() {
        let md = conv("<ul>\n<li color=\"blue_bg\">\n<b>A1. Question</b>\n\n- À écouter : réponse\n</li>\n<li>\nB\n</li>\n</ul>\n");
        let b = blocks::from_markdown(&md);
        assert_eq!(b.len(), 3, "{md}");
        assert_eq!(b[0].kind, BlockKind::Bulleted);
        assert_eq!(b[0].text, "**A1. Question**");
        assert_eq!(b[0].color, "bg-blue");
        assert_eq!(
            (b[1].kind.clone(), b[1].depth, b[1].text.as_str()),
            (BlockKind::Bulleted, 1, "À écouter : réponse")
        );
        assert_eq!(b[2].depth, 0);
    }

    #[test]
    fn a_colour_suffix_before_a_block_colour_marker_is_dropped() {
        let out = color_suffix_lines("> 💡 Question {color=\"yellow_bg\"} <!--c:bg-yellow-->");
        assert_eq!(out, "> 💡 Question <!--c:bg-yellow-->");
    }

    #[test]
    fn a_callout_without_icon_takes_the_pictograph_that_starts_its_text() {
        let md = conv("<callout color=\"yellow_bg\">\n⚠️ <b>Careful</b> here\n</callout>\n");
        assert!(md.contains("> ⚠️"), "{md}");
        assert!(!md.contains("💡"), "{md}");
        let plain = conv("<callout>\nJust text\n</callout>\n");
        assert!(plain.contains("> 💡 Just text"), "{plain}");
    }

    #[test]
    fn ordered_lists_toggles_columns_and_todos() {
        let md = conv(
            "<ol>\n<li>\nun\n</li>\n<li>\ndeux\n</li>\n</ol>\n\n<toggle>\nTitre\n\nContenu caché\n</toggle>\n\n<columns>\n<column>\nGauche\n</column>\n<column>\nDroite\n</column>\n</columns>\n\n<todo checked=\"true\">Fait</todo>\n",
        );
        let b = blocks::from_markdown(&md);
        let kinds: Vec<_> = b.iter().map(|x| x.kind.clone()).collect();
        assert_eq!(kinds[0], BlockKind::Numbered, "{md}");
        assert_eq!(kinds[1], BlockKind::Numbered);
        assert_eq!(kinds[2], BlockKind::Toggle { open: false });
        assert_eq!(b[3].text, "Contenu caché");
        assert_eq!(b[3].depth, 1);
        assert!(
            kinds.contains(&BlockKind::ColumnsStart)
                && kinds.contains(&BlockKind::ColumnBreak)
                && kinds.contains(&BlockKind::ColumnsEnd),
            "{md}"
        );
        assert_eq!(b.last().unwrap().kind, BlockKind::Todo { checked: true });
    }

    #[test]
    fn inline_styles_colours_mentions_and_dates() {
        let md = conv("Du <b>gras</b>, <i>italique</i>, <s>barré</s>, <u>souligné</u>, <span color=\"red\">rouge</span>, <span color=\"yellow_bg\">fond</span>, <mention url=\"https://example.com/Plan-0123456789abcdef0123456789abcdef\">Plan</mention> le <date start=\"2026-11-23\" />.\n");
        assert_eq!(
            md.trim(),
            "Du **gras**, *italique*, ~~barré~~, <u>souligné</u>, <span style=\"color:red\">rouge</span>, fond, [Plan](https://example.com/Plan-0123456789abcdef0123456789abcdef) le 2026-11-23."
        );
    }

    #[test]
    fn media_become_images_files_and_bookmarks_with_their_names() {
        let md = conv(
            "<image source=\"Page/shot one.png\">Capture</image>\n\n<pdf source=\"docs/Rapport final.pdf\"></pdf>\n\n<file source=\"docs/note.docx\">Ma note</file>\n\n<bookmark source=\"https://example.com\">Exemple</bookmark>\n",
        );
        let b = blocks::from_markdown(&md);
        assert_eq!(b[0].kind, BlockKind::Image { path: "Page/shot one.png".into() }, "{md}");
        assert_eq!(b[0].text, "Capture");
        assert!(matches!(&b[1].kind, BlockKind::File { path } if path == "docs/Rapport final.pdf"), "{md}");
        assert_eq!(b[1].text, "Rapport final.pdf");
        assert_eq!(b[2].kind, BlockKind::File { path: "docs/note.docx".into() });
        assert_eq!(b[2].text, "Ma note");
        assert_eq!(b[3].kind, BlockKind::Bookmark { url: "https://example.com".into() });
    }

    #[test]
    fn code_is_left_alone_and_unknown_tags_stay_text() {
        let md = conv("<callout icon=\"💡\">\nok\n</callout>\n\n```html\n<callout icon=\"x\">no</callout>\n```\n\nUn <foo> inconnu et `<b>code</b>`.\n");
        assert!(md.contains("<callout icon=\"x\">no</callout>"), "{md}");
        assert!(md.contains("`<b>code</b>`") && md.contains("<foo>"), "{md}");
    }

    #[test]
    fn what_cannot_be_kept_is_reported() {
        let (_, notes) = convert(
            "<page url=\"https://example.com/Sub-1\">Sub</page>\n\n<synced-block>\nCopie\n</synced-block>\n\n<span color=\"blue_bg\">x</span>\n",
        );
        let all = notes.join("\n");
        assert!(all.contains("Sub-page not imported") && all.contains("Sub"), "{all}");
        assert!(all.contains("synced blocks") && all.contains("highlight colours"), "{all}");
    }

    #[test]
    fn equations_and_quotes() {
        let b = blocks::from_markdown(&conv("<equation>E = mc^2</equation>\n\n<quote>\nCitation\n\nSuite\n</quote>\n"));
        assert_eq!(b[0].kind, BlockKind::Equation);
        assert_eq!(b[0].text, "E = mc^2");
        assert_eq!(b[1].kind, BlockKind::Quote);
        assert!(b[1].text.contains("Citation") && b[1].text.contains("Suite"));
    }

    #[test]
    fn an_image_opening_a_callout_moves_before_it() {
        let md = "> [!NOTE]\n> 💡 ![Cap](assets/a.png)\n> Cap\n> Le texte\n";
        let out = hoist_callout_images(md);
        assert!(out.starts_with("![Cap](assets/a.png)\n\n> [!NOTE]\n> 💡 Le texte"), "{out}");
    }

    #[test]
    fn colour_suffix_columns_citations_and_transcript() {
        let src = "Intro {color=\"red\"}\n\n> Quote {color=\"orange_bg\"}\n\n<columns>\n\t<column>\n\t\tLeft\n\t</column>\n\t<column>\n\t\tRight [^https://x.y/z#1]\n\t</column>\n</columns>\n\n<table>\n<tr>\n<td>A</td>\n<td color=\"red\">B</td>\n</tr>\n</table>\n\n<meeting-notes>\n<meeting-notes-section-notes>\n<p />\n</meeting-notes-section-notes>\n<meeting-notes-section-transcript>\nHello there\n</meeting-notes-section-transcript>\n</meeting-notes>\n";
        let (md, _) = convert(src);
        assert!(md.contains("Intro <!--c:red-->"), "{md}");
        assert!(md.contains("> Quote <!--c:bg-orange-->"), "{md}");
        let flat: Vec<&str> = md.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            flat.windows(5).any(|w| w == ["<!--columns-->", "Left", "<!--column-->", "Right", "<!--/columns-->"]),
            "{md}"
        );
        assert!(!md.contains("[^"), "{md}");
        assert!(md.contains("<span style=\"color:red\">B</span>"), "{md}");
        assert!(md.contains("<summary>Transcript</summary>") && md.contains("Hello there"), "{md}");
        assert!(!md.contains("&nbsp;"), "{md}");
    }
}
