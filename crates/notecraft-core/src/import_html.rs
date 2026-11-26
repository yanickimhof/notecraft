//! HTML (a saved web page, an Evernote note, an HTML export) to our Markdown.
//!
//! The converter keeps what a notes app keeps: headings, paragraphs, lists (also nested and
//! to-do lists), links, bold, italic, underline, strike, code, quotes, tables and images. Anything
//! else is flattened to its text, so no content is dropped.

use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, ParseOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::collections::BTreeMap;

/// An image or file the converter met, once the caller has stored it.
#[derive(Debug, Clone)]
pub struct Res {
    /// Workspace-relative path, or a web address.
    pub path: String,
    /// Caption (image) or displayed name (file).
    pub name: String,
    pub image: bool,
}

/// Maps an element (`img`, `en-media`, `a` to a local file…) and its attributes to a stored [`Res`].
pub type Resolver<'a> = dyn FnMut(&str, &BTreeMap<String, String>) -> Option<Res> + 'a;

#[derive(Debug, Clone, Default)]
pub struct Converted {
    /// `<title>` of the document, when there is one.
    pub title: Option<String>,
    pub markdown: String,
}

enum B {
    Para(String),
    Item(String),
    Raw(String),
}

struct Conv<'a, 'r> {
    blocks: Vec<B>,
    cur: String,
    todo: Option<bool>,
    lists: Vec<Option<usize>>,
    /// While true, block elements do not break the text (inside a list item or a table cell).
    inline_only: bool,
    pre: bool,
    title: Option<String>,
    resolve: &'a mut Resolver<'r>,
}

fn attrs_of(node: &Handle) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let NodeData::Element { attrs, .. } = &node.data {
        for a in attrs.borrow().iter() {
            m.insert(a.name.local.to_lowercase(), a.value.to_string());
        }
    }
    m
}

fn tag_of(node: &Handle) -> Option<String> {
    match &node.data {
        NodeData::Element { name, .. } => Some(name.local.to_lowercase()),
        _ => None,
    }
}

fn text_content(node: &Handle, out: &mut String, depth: usize) {
    if depth > 200 {
        return;
    }
    match &node.data {
        NodeData::Text { contents } => out.push_str(&contents.borrow()),
        _ => {
            for c in node.children.borrow().iter() {
                text_content(c, out, depth + 1);
            }
        }
    }
}

const SKIP: [&str; 9] = ["script", "style", "noscript", "svg", "template", "iframe", "object", "canvas", "button"];
const BLOCK: [&str; 26] = [
    "p",
    "div",
    "section",
    "article",
    "main",
    "header",
    "footer",
    "aside",
    "figure",
    "figcaption",
    "center",
    "form",
    "fieldset",
    "address",
    "nav",
    "dl",
    "dt",
    "dd",
    "details",
    "summary",
    "body",
    "html",
    "tbody",
    "thead",
    "tfoot",
    "caption",
];

/// The palette colour name closest to a CSS colour (`#d44c47`, `rgb(…)`, `red`), or `None` for the
/// default text colours (black, dark grey, white) that a mail adds to every line.
fn palette_color(css: &str) -> Option<&'static str> {
    let css = css.trim();
    let (r, g, b) = if let Some(hex) = css.strip_prefix('#') {
        let v: Vec<u8> = match hex.len() {
            3 => hex.chars().filter_map(|c| c.to_digit(16)).map(|d| (d * 17) as u8).collect(),
            6 => (0..3).filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()).collect(),
            _ => return None,
        };
        if v.len() != 3 {
            return None;
        }
        (v[0], v[1], v[2])
    } else if let Some(args) = css.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let v: Vec<u8> = args.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).map(|x| x as u8).collect();
        if v.len() < 3 {
            return None;
        }
        (v[0], v[1], v[2])
    } else {
        return match css {
            "red" => Some("red"),
            "green" => Some("green"),
            "blue" => Some("blue"),
            "orange" => Some("orange"),
            "purple" => Some("purple"),
            "pink" => Some("pink"),
            "yellow" => Some("yellow"),
            "brown" => Some("brown"),
            "gray" | "grey" => Some("gray"),
            _ => None,
        };
    };
    let (rf, gf, bf) = (f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    let d = max - min;
    let sat = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    if sat < 0.18 {
        return if (0.38..0.7).contains(&l) { Some("gray") } else { None };
    }
    let h = if max == rf {
        60.0 * (((gf - bf) / d).rem_euclid(6.0))
    } else if max == gf {
        60.0 * ((bf - rf) / d + 2.0)
    } else {
        60.0 * ((rf - gf) / d + 4.0)
    };
    Some(match h {
        h if !(15.0..345.0).contains(&h) => "red",
        h if h < 40.0 => {
            if l < 0.4 {
                "brown"
            } else {
                "orange"
            }
        }
        h if h < 65.0 => "yellow",
        h if h < 170.0 => "green",
        h if h < 255.0 => "blue",
        h if h < 300.0 => "purple",
        _ => "pink",
    })
}

impl<'a, 'r> Conv<'a, 'r> {
    fn push_text(&mut self, s: &str) {
        if self.pre {
            self.cur.push_str(s);
            return;
        }
        for ch in s.chars() {
            if ch.is_whitespace() {
                if !self.cur.is_empty() && !self.cur.ends_with(' ') && !self.cur.ends_with('\n') {
                    self.cur.push(' ');
                }
            } else {
                self.cur.push(ch);
            }
        }
    }

    fn flush(&mut self) {
        let t = self.cur.trim().to_string();
        let todo = self.todo.take();
        self.cur.clear();
        if t.is_empty() {
            return;
        }
        match todo {
            Some(done) => {
                self.blocks.push(B::Item(format!("- [{}] {}", if done { "x" } else { " " }, t.replace('\n', " "))))
            }
            None => self.blocks.push(B::Para(t)),
        }
    }

    fn children(&mut self, node: &Handle, depth: usize) {
        let kids: Vec<Handle> = node.children.borrow().iter().cloned().collect();
        for c in kids {
            self.walk(&c, depth + 1);
        }
    }

    /// Render a subtree as one line of text (cells, list items).
    fn inline_of(&mut self, node: &Handle, depth: usize) -> String {
        let saved_cur = std::mem::take(&mut self.cur);
        let saved_inline = self.inline_only;
        let saved_blocks = std::mem::take(&mut self.blocks);
        self.inline_only = true;
        self.children(node, depth);
        let mut s = std::mem::take(&mut self.cur);
        for b in std::mem::take(&mut self.blocks) {
            match b {
                B::Para(t) | B::Item(t) | B::Raw(t) => {
                    s.push(' ');
                    s.push_str(&t);
                }
            }
        }
        self.blocks = saved_blocks;
        self.cur = saved_cur;
        self.inline_only = saved_inline;
        s.replace('\n', " ").trim().to_string()
    }

    fn wrap(&mut self, node: &Handle, depth: usize, open: &str, close: &str) {
        let start = self.cur.len();
        self.children(node, depth);
        if self.cur.len() <= start {
            return;
        }
        // the children may have flushed the text built so far: then there is nothing left to wrap
        let Some(inner) = self.cur.get(start..).map(str::to_string) else { return };
        let t = inner.trim();
        if t.is_empty() {
            return;
        }
        let lead = &inner[..inner.len() - inner.trim_start().len()];
        let trail = &inner[inner.trim_end().len()..];
        if !self.cur.is_char_boundary(start) {
            return;
        }
        self.cur.truncate(start);
        self.cur.push_str(&format!("{lead}{open}{t}{close}{trail}"));
    }

    fn list(&mut self, node: &Handle, ordered: bool, start: usize, depth: usize) {
        if !self.inline_only {
            self.flush();
        }
        if self.lists.is_empty() {
            self.blocks.push(B::Raw(String::new())); // keeps two lists in a row apart
        }
        self.lists.push(if ordered { Some(start) } else { None });
        let kids: Vec<Handle> = node.children.borrow().iter().cloned().collect();
        for k in kids {
            if tag_of(&k).as_deref() == Some("li") {
                self.li(&k, depth + 1);
            } else {
                self.walk(&k, depth + 1);
            }
        }
        self.lists.pop();
    }

    fn li(&mut self, node: &Handle, depth: usize) {
        self.flush();
        let level = self.lists.len().saturating_sub(1);
        let marker = match self.lists.last_mut() {
            Some(Some(n)) => {
                let m = format!("{n}. ");
                *n += 1;
                m
            }
            _ => "- ".to_string(),
        };
        let saved_inline = self.inline_only;
        self.inline_only = true;
        let kids: Vec<Handle> = node.children.borrow().iter().cloned().collect();
        let mut nested: Vec<Handle> = Vec::new();
        for k in kids {
            match tag_of(&k).as_deref() {
                Some("ul") | Some("ol") => nested.push(k),
                _ => self.walk(&k, depth + 1),
            }
        }
        self.inline_only = saved_inline;
        let text = self.cur.trim().replace('\n', " ");
        let todo = self.todo.take();
        self.cur.clear();
        let marker = match todo {
            Some(done) => format!("- [{}] ", if done { "x" } else { " " }),
            None => marker,
        };
        if !text.is_empty() || !nested.is_empty() {
            self.blocks.push(B::Item(format!("{}{}{}", "  ".repeat(level.min(6)), marker, text)));
        }
        for n in nested {
            self.walk(&n, depth + 1);
        }
    }

    fn table(&mut self, node: &Handle, depth: usize) {
        self.flush();
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut header_row = false;
        self.collect_rows(node, &mut rows, &mut header_row, depth);
        rows.retain(|r| r.iter().any(|c| !c.is_empty()));
        if rows.is_empty() {
            return;
        }
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let fmt = |r: &Vec<String>| {
            let mut cells: Vec<String> = r.iter().map(|c| c.replace('|', "\\|")).collect();
            cells.resize(cols, String::new());
            format!("| {} |", cells.join(" | "))
        };
        let mut lines = vec![fmt(&rows[0])];
        lines.push(format!("|{}|", vec![" --- "; cols].join("|")));
        for r in rows.iter().skip(1) {
            lines.push(fmt(r));
        }
        self.blocks.push(B::Raw(lines.join("\n")));
    }

    fn collect_rows(&mut self, node: &Handle, rows: &mut Vec<Vec<String>>, header: &mut bool, depth: usize) {
        if depth > 100 {
            return;
        }
        let kids: Vec<Handle> = node.children.borrow().iter().cloned().collect();
        for k in kids {
            match tag_of(&k).as_deref() {
                Some("tr") => {
                    let mut cells = Vec::new();
                    let cs: Vec<Handle> = k.children.borrow().iter().cloned().collect();
                    for c in cs {
                        if matches!(tag_of(&c).as_deref(), Some("td") | Some("th")) {
                            if tag_of(&c).as_deref() == Some("th") {
                                *header = true;
                            }
                            cells.push(self.inline_of(&c, depth + 1));
                        }
                    }
                    rows.push(cells);
                }
                Some("table") => {} // nested tables are flattened into their cell's text
                Some(_) => self.collect_rows(&k, rows, header, depth + 1),
                None => {}
            }
        }
    }

    fn quote(&mut self, node: &Handle, depth: usize) {
        self.flush();
        let saved = std::mem::take(&mut self.blocks);
        self.children(node, depth);
        self.flush();
        let inner = std::mem::replace(&mut self.blocks, saved);
        let text = join_blocks(&inner);
        if text.trim().is_empty() {
            return;
        }
        let q: Vec<String> =
            text.lines().map(|l| if l.trim().is_empty() { ">".to_string() } else { format!("> {l}") }).collect();
        self.blocks.push(B::Raw(q.join("\n")));
    }

    fn style_wrap(&mut self, node: &Handle, depth: usize) {
        let style = attrs_of(node).get("style").map(|s| s.to_lowercase().replace(' ', "")).unwrap_or_default();
        let bold = style.contains("font-weight:bold")
            || style.contains("font-weight:700")
            || style.contains("font-weight:600");
        let italic = style.contains("font-style:italic");
        let strike = style.contains("line-through");
        let under = style.contains("underline");
        let mut open = String::new();
        let mut close = String::new();
        let css = style.split(';').find_map(|p| p.strip_prefix("color:")).map(str::to_string);
        let css = css.or_else(|| attrs_of(node).get("color").map(|c| c.to_lowercase()));
        if let Some(name) = css.as_deref().and_then(palette_color) {
            open.push_str(&format!("<span style=\"color:{name}\">"));
            close.insert_str(0, "</span>");
        }
        if bold {
            open.push_str("**");
            close.insert_str(0, "**");
        }
        if italic {
            open.push('*');
            close.insert(0, '*');
        }
        if strike {
            open.push_str("~~");
            close.insert_str(0, "~~");
        }
        if under {
            open.push_str("<u>");
            close.insert_str(0, "</u>");
        }
        if open.is_empty() {
            self.children(node, depth);
        } else {
            self.wrap(node, depth, &open, &close);
        }
    }

    fn walk(&mut self, node: &Handle, depth: usize) {
        if depth > 200 {
            return;
        }
        match &node.data {
            NodeData::Text { contents } => {
                let s = contents.borrow().to_string();
                self.push_text(&s);
            }
            NodeData::Element { .. } => {
                let tag = tag_of(node).unwrap_or_default();
                let tag = tag.as_str();
                if tag == "title" {
                    let mut t = String::new();
                    text_content(node, &mut t, 0);
                    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if self.title.is_none() && !t.is_empty() {
                        self.title = Some(t);
                    }
                    return;
                }
                if SKIP.contains(&tag) || tag == "head" || tag == "meta" || tag == "link" {
                    if tag == "head" {
                        self.children(node, depth);
                    }
                    return;
                }
                match tag {
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        let level = (tag.as_bytes()[1] - b'0').min(3) as usize;
                        if self.inline_only {
                            self.style_wrap(node, depth);
                            return;
                        }
                        self.flush();
                        let t = self.inline_of(node, depth);
                        if !t.is_empty() {
                            self.blocks.push(B::Raw(format!("{} {t}", "#".repeat(level))));
                        }
                    }
                    "ul" => self.list(node, false, 1, depth),
                    "ol" => {
                        let start = attrs_of(node).get("start").and_then(|s| s.parse().ok()).unwrap_or(1);
                        self.list(node, true, start, depth)
                    }
                    "li" => self.li(node, depth),
                    "table" => self.table(node, depth),
                    "pre" => {
                        self.flush();
                        let mut t = String::new();
                        text_content(node, &mut t, 0);
                        let lang = attrs_of(node)
                            .get("class")
                            .and_then(|c| {
                                c.split_whitespace().find_map(|w| w.strip_prefix("language-").map(String::from))
                            })
                            .unwrap_or_default();
                        let t = t.trim_matches('\n').replace("```", "'''");
                        self.blocks.push(B::Raw(format!("```{lang}\n{t}\n```")));
                    }
                    "blockquote" => self.quote(node, depth),
                    "hr" => {
                        self.flush();
                        self.blocks.push(B::Raw("---".into()));
                    }
                    "br" => {
                        if self.inline_only || self.pre {
                            self.push_text(" ");
                        } else {
                            let t = self.cur.trim_end().to_string();
                            self.cur = t;
                            self.cur.push('\n');
                        }
                    }
                    "strong" | "b" => self.wrap(node, depth, "**", "**"),
                    "em" | "i" | "cite" => self.wrap(node, depth, "*", "*"),
                    "u" | "ins" => self.wrap(node, depth, "<u>", "</u>"),
                    "s" | "strike" | "del" => self.wrap(node, depth, "~~", "~~"),
                    "code" | "kbd" | "samp" | "tt" => {
                        let mut t = String::new();
                        text_content(node, &mut t, 0);
                        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                        if !t.is_empty() {
                            self.cur.push_str(&format!("`{}`", t.replace('`', "'")));
                        }
                    }
                    "span" | "font" | "mark" | "label" => self.style_wrap(node, depth),
                    "a" => {
                        let at = attrs_of(node);
                        let href = at.get("href").cloned().unwrap_or_default();
                        let res = if href.is_empty()
                            || href.starts_with("http")
                            || href.starts_with('#')
                            || href.starts_with("mailto:")
                        {
                            None
                        } else {
                            (self.resolve)("a", &at)
                        };
                        if let Some(r) = res {
                            if !self.inline_only {
                                self.flush();
                            }
                            let name =
                                if r.name.is_empty() { at.get("title").cloned().unwrap_or_default() } else { r.name };
                            self.blocks.push(B::Raw(format!("[📎 {name}]({})", r.path)));
                            return;
                        }
                        let keep = href.starts_with("http") || href.starts_with("mailto:");
                        if keep {
                            let start = self.cur.len();
                            self.children(node, depth);
                            let inner = self.cur[start.min(self.cur.len())..].to_string();
                            let t = inner.trim();
                            if !t.is_empty() && !t.contains("](") {
                                self.cur.truncate(start);
                                self.cur.push_str(&format!("[{t}]({href})"));
                            }
                        } else {
                            self.children(node, depth);
                        }
                    }
                    "img" | "en-media" => {
                        let at = attrs_of(node);
                        let r = (self.resolve)(tag, &at);
                        match r {
                            Some(r) => {
                                if !self.inline_only {
                                    self.flush();
                                    let line = if r.image {
                                        format!("![{}]({})", r.name, r.path)
                                    } else {
                                        format!("[📎 {}]({})", r.name, r.path)
                                    };
                                    self.blocks.push(B::Raw(line));
                                } else {
                                    let line = if r.image {
                                        format!("![{}]({})", r.name, r.path)
                                    } else {
                                        format!("[📎 {}]({})", r.name, r.path)
                                    };
                                    self.cur.push_str(&line);
                                }
                            }
                            None => {
                                if let Some(alt) = at.get("alt").filter(|a| !a.is_empty()) {
                                    self.push_text(alt);
                                }
                            }
                        }
                        if tag == "en-media" {
                            self.children(node, depth);
                        }
                    }
                    "en-todo" | "input" => {
                        let at = attrs_of(node);
                        if tag == "en-todo" || at.get("type").map(|t| t == "checkbox").unwrap_or(false) {
                            let done = at.get("checked").map(|c| c != "false").unwrap_or(false);
                            self.todo = Some(done);
                        }
                        // `<en-todo/>` is not a void element for an HTML parser: what follows lands inside it
                        self.children(node, depth);
                    }
                    "en-crypt" => self.push_text("[encrypted text]"),
                    "div" if attrs_of(node).get("class").is_some_and(|c| c.contains("checkbox")) => {
                        let c = attrs_of(node).get("class").cloned().unwrap_or_default();
                        self.todo = Some(c.contains("checkbox-on"));
                    }
                    t if BLOCK.contains(&t) => {
                        if self.inline_only {
                            if !self.cur.is_empty() && !self.cur.ends_with(' ') {
                                self.cur.push(' ');
                            }
                            self.children(node, depth);
                        } else {
                            self.flush();
                            self.children(node, depth);
                            self.flush();
                        }
                    }
                    _ => self.children(node, depth),
                }
            }
            _ => {
                if let NodeData::Document = &node.data {
                    self.children(node, depth);
                }
            }
        }
    }
}

fn join_blocks(blocks: &[B]) -> String {
    let mut out = String::new();
    let mut prev_item = false;
    for b in blocks.iter() {
        let (t, item) = match b {
            B::Para(t) | B::Raw(t) => (t, false),
            B::Item(t) => (t, true),
        };
        if t.is_empty() {
            prev_item = false;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if prev_item && item { "\n" } else { "\n\n" });
        }
        out.push_str(t);
        prev_item = item;
    }
    out
}

/// Convert an HTML document or fragment. `resolve` stores images and attachments and says where they went.
pub fn html_to_markdown(html: &str, resolve: &mut Resolver) -> Converted {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).from_utf8().one(html.as_bytes());
    let mut c = Conv {
        blocks: Vec::new(),
        cur: String::new(),
        todo: None,
        lists: Vec::new(),
        inline_only: false,
        pre: false,
        title: None,
        resolve,
    };
    c.walk(&dom.document, 0);
    c.flush();
    Converted { title: c.title.take(), markdown: join_blocks(&c.blocks) }
}

/// Convert the HTML an app puts on the clipboard (Outlook, Word, a web page) to Markdown, ready to be
/// pasted as blocks: only the copied fragment is kept, Office comments and styles are dropped, and
/// the empty lines and non-breaking spaces Office adds are tidied. Images are not pasted.
pub fn clipboard_html_to_markdown(html: &str) -> String {
    let mut h = html;
    if let (Some(a), Some(b)) = (html.find("<!--StartFragment-->"), html.find("<!--EndFragment-->")) {
        if a + 20 <= b {
            h = &html[a + 20..b];
        }
    }
    // comments, including the Office `<!--[if gte mso 9]> ... <![endif]-->` blocks
    let mut clean = String::with_capacity(h.len());
    let mut rest = h;
    while let Some(a) = rest.find("<!--") {
        clean.push_str(&rest[..a]);
        match rest[a..].find("-->") {
            Some(b) => rest = &rest[a + b + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    clean.push_str(rest);
    let md = html_to_markdown(&clean, &mut |_, _| None).markdown.replace('\u{a0}', " ");
    let mut out = String::new();
    let mut blank = 0;
    for line in md.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlook_clipboard_keeps_structure_and_drops_office_noise() {
        let html = "<html><head><style>p.MsoNormal {margin:0}</style></head><body>\
            <!--StartFragment--><!--[if gte mso 9]><xml><o:OfficeDocumentSettings/></xml><![endif]-->\
            <p class=MsoNormal>Bonjour <b>Marc</b>,<o:p></o:p></p>\
            <p class=MsoNormal><o:p>&nbsp;</o:p></p>\
            <p class=MsoNormal><o:p>&nbsp;</o:p></p>\
            <p class=MsoNormal>Voir <a href=\"https://a.ch\">le plan</a> :</p>\
            <ul><li>un</li><li>deux</li></ul><!--EndFragment--></body></html>";
        assert_eq!(
            clipboard_html_to_markdown(html),
            "Bonjour **Marc**,\n\nVoir [le plan](https://a.ch) :\n\n- un\n- deux"
        );
    }

    #[test]
    fn clipboard_table_becomes_a_markdown_table() {
        let md = clipboard_html_to_markdown(
            "<table><tr><th>Nom</th><th>Prix</th></tr><tr><td>A</td><td>10</td></tr></table>",
        );
        assert!(md.contains("| Nom | Prix |") && md.contains("| A | 10 |"), "{md}");
    }

    fn conv(html: &str) -> String {
        html_to_markdown(html, &mut |_, at| {
            at.get("src").map(|s| Res {
                path: s.clone(),
                name: at.get("alt").cloned().unwrap_or_default(),
                image: true,
            })
        })
        .markdown
    }

    #[test]
    fn headings_paragraphs_and_inline_marks() {
        let md = conv("<h1>Title</h1><p>Some <b>bold</b>, <i>italic</i> and <a href=\"https://a.ch\">a link</a>.</p><h4>Deep</h4>");
        assert_eq!(md, "# Title\n\nSome **bold**, *italic* and [a link](https://a.ch).\n\n### Deep");
    }

    #[test]
    fn nested_and_todo_lists() {
        let md = conv("<ul><li>one<ul><li>inner</li></ul></li><li>two</li></ul><ol><li>a</li><li>b</li></ol><ul><li><input type=\"checkbox\" checked> done</li><li><input type=\"checkbox\"> open</li></ul>");
        assert_eq!(md, "- one\n  - inner\n- two\n\n1. a\n2. b\n\n- [x] done\n- [ ] open");
    }

    #[test]
    fn evernote_todo_lines_and_media() {
        let md = html_to_markdown(
            "<en-note><div><en-todo checked=\"true\"/>Pay rent</div><div><en-todo/>Call Bob</div><div>Text <en-media hash=\"abc\" type=\"image/png\"/></div></en-note>",
            &mut |tag, at| (tag == "en-media" && at.get("hash").is_some_and(|h| h == "abc")).then(|| Res { path: "assets/a.png".into(), name: "a.png".into(), image: true }),
        )
        .markdown;
        assert!(md.contains("- [x] Pay rent"), "{md}");
        assert!(md.contains("- [ ] Call Bob"), "{md}");
        assert!(md.contains("![a.png](assets/a.png)"), "{md}");
    }

    #[test]
    fn tables_code_quotes_rules() {
        let md = conv("<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>x|y</td></tr></table><pre class=\"language-rust\">fn main() {}\n</pre><blockquote><p>wise</p><p>words</p></blockquote><hr>");
        assert!(md.contains("| A | B |\n| --- | --- |\n| 1 | x\\|y |"), "{md}");
        assert!(md.contains("```rust\nfn main() {}\n```"), "{md}");
        assert!(md.contains("> wise\n>\n> words"), "{md}");
        assert!(md.ends_with("---"), "{md}");
    }

    #[test]
    fn styles_scripts_and_whitespace() {
        let md = conv("<html><head><title>T</title><style>p{}</style></head><body><script>alert(1)</script><p>  a \n   b <span style=\"font-weight: bold\">c</span></p></body></html>");
        assert_eq!(md, "a b **c**");
        let t = html_to_markdown("<title> My  page </title><p>x</p>", &mut |_, _| None);
        assert_eq!(t.title.as_deref(), Some("My page"));
    }

    #[test]
    fn hostile_depth_does_not_overflow() {
        let html = format!("{}x{}", "<div>".repeat(5000), "</div>".repeat(5000));
        let md = conv(&html);
        assert!(md.len() < 100);
    }
}
