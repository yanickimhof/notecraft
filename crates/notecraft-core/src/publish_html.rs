//! The web version of a page: the HTML that "Publish" sends and "Preview" opens.
//!
//! It follows the look of the app (same greys, blue accent, block colours, light and dark mode,
//! the page's own font, small text and full width options) so the published page reads like the
//! page on screen. The module only builds text: images and links to other pages are resolved by
//! the caller through [`Hooks`], which keeps the network and the files out of here.

use crate::blocks::{column_layout, table_cells, Block, BlockKind, BLOCK_COLORS};
use crate::render::{escape, runs_of, Run};

/// A link shown above or below the page (breadcrumb, sub page).
#[derive(Debug, Clone, PartialEq)]
pub struct NavLink {
    pub title: String,
    pub icon: String,
    pub url: String,
}

/// The cover of the page.
#[derive(Debug, Clone, PartialEq)]
pub enum Cover {
    None,
    /// One of the eight app gradients.
    Gradient(usize),
    /// URL of an image (already resolved by the caller).
    Image(String),
}

/// What the page looks like and says about itself.
#[derive(Debug, Clone)]
pub struct Options<'a> {
    pub title: &'a str,
    pub icon: &'a str,
    pub cover: Cover,
    /// `""`, `"serif"` or `"mono"`, as in the page settings.
    pub font: &'a str,
    pub small: bool,
    pub wide: bool,
    /// `"fr"` or `"en"` (the few words the page adds itself).
    pub lang: &'a str,
    pub crumbs: Vec<NavLink>,
    pub children: Vec<NavLink>,
    /// Absolute URL of a picture for link previews (optional).
    pub og_image: Option<String>,
}

/// Callbacks that turn page content into URLs.
pub struct Hooks<'a> {
    /// A workspace image path to the URL it will have on the page (`None`: the image is dropped).
    pub image: &'a mut dyn FnMut(&str) -> Option<String>,
    /// The URL of the published page with this title (`None`: not published, the link is plain text).
    pub link: &'a dyn Fn(&str) -> Option<String>,
    /// The grid of a database block (first row is the header), if it can be shown.
    pub database: &'a dyn Fn(&str) -> Option<Vec<Vec<String>>>,
}

const COVERS: [(&str, &str); 8] = [
    ("#F6D365", "#FDA085"),
    ("#A1C4FD", "#C2E9FB"),
    ("#D4FC79", "#96E6A1"),
    ("#FBC2EB", "#A6C1EE"),
    ("#2B5876", "#4E4376"),
    ("#E0C3FC", "#8EC5FC"),
    ("#FFECD2", "#FCB69F"),
    ("#434E5E", "#8E9EAB"),
];

/// (name, light text, dark text, light background, dark background), as in the app.
const COLORS: [(&str, &str, &str, &str, &str); 9] = [
    ("gray", "#9B9A97", "#979A9B", "#EBECED", "#454B4E"),
    ("brown", "#64473A", "#937264", "#E9E5E3", "#434040"),
    ("orange", "#D9730D", "#FFA344", "#FAEBDD", "#594A3A"),
    ("yellow", "#DFAB01", "#FFDC49", "#FBF3DB", "#59563B"),
    ("green", "#0F7B6C", "#4DAB9A", "#DDEDEA", "#354C4B"),
    ("blue", "#0B6E99", "#529CCA", "#DDEBF1", "#364954"),
    ("purple", "#6940A5", "#9A6DD7", "#EAE4F2", "#443F57"),
    ("pink", "#AD1A72", "#E255A1", "#F4DFEB", "#533B4C"),
    ("red", "#E03E3E", "#FF7369", "#FBE4E4", "#594141"),
];

/// Only web and mail links are kept; anything else (`javascript:`, local files) is shown as text.
pub fn safe_url(u: &str) -> Option<String> {
    let t = u.trim();
    let l = t.to_ascii_lowercase();
    if l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:") || l.starts_with('#') {
        Some(t.to_string())
    } else {
        None
    }
}

fn is_marker(k: &BlockKind) -> bool {
    matches!(k, BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd)
}

/// End (exclusive) of the blocks nested under `bs[i]`.
fn child_end(bs: &[Block], i: usize) -> usize {
    let mut j = i + 1;
    while j < bs.len() && bs[j].depth > bs[i].depth && !is_marker(&bs[j].kind) {
        j += 1;
    }
    j
}

fn list_tag(k: &BlockKind) -> Option<&'static str> {
    match k {
        BlockKind::Bulleted => Some("ul"),
        BlockKind::Numbered => Some("ol"),
        BlockKind::Todo { .. } => Some("ul class=\"todo\""),
        _ => None,
    }
}

fn same_list(a: &BlockKind, b: &BlockKind) -> bool {
    matches!(
        (a, b),
        (BlockKind::Bulleted, BlockKind::Bulleted)
            | (BlockKind::Numbered, BlockKind::Numbered)
            | (BlockKind::Todo { .. }, BlockKind::Todo { .. })
    )
}

fn plain_text(text: &str) -> String {
    runs_of(text).iter().map(|r| r.text.as_str()).collect()
}

fn color_classes(color: &str) -> String {
    color
        .split_whitespace()
        .filter_map(|p| {
            let (bg, base) = match p.strip_prefix("bg-") {
                Some(b) => (true, b),
                None => (false, p),
            };
            BLOCK_COLORS.contains(&base).then(|| if bg { format!("bg-{base}") } else { format!("c-{base}") })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct R<'a, 'b> {
    hooks: &'b mut Hooks<'a>,
    headings: Vec<(u8, String)>,
    next_heading: usize,
}

impl R<'_, '_> {
    fn runs(&self, runs: &[Run]) -> String {
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
            if let Some(c) = &st.color {
                let cls = color_classes(c);
                if !cls.is_empty() {
                    s = format!("<span class=\"{cls}\">{s}</span>");
                }
            }
            if let Some(url) = &st.link {
                if let Some(u) = safe_url(url) {
                    s = format!("<a href=\"{}\" rel=\"noopener noreferrer\">{s}</a>", escape(&u));
                }
            } else if let Some(title) = &st.mention {
                s = match (self.hooks.link)(title) {
                    Some(u) => format!("<a class=\"mention\" href=\"{}\">{s}</a>", escape(&u)),
                    None => format!("<span class=\"mention\">{s}</span>"),
                };
            }
            o.push_str(&s);
        }
        o
    }

    fn text(&self, text: &str) -> String {
        self.runs(&runs_of(text))
    }

    fn seq(&mut self, bs: &[Block]) -> String {
        let mut o = String::new();
        let mut i = 0;
        while i < bs.len() {
            let b = &bs[i];
            match &b.kind {
                BlockKind::ColumnsStart => {
                    if let Some((end, firsts)) = column_layout(bs, i) {
                        o.push_str("<div class=\"cols\">");
                        for (k, &a) in firsts.iter().enumerate() {
                            let z = if k + 1 < firsts.len() { firsts[k + 1] - 1 } else { end };
                            let inner = self.seq(&bs[a..z.max(a)]);
                            o.push_str(&format!("<div class=\"col\">{inner}</div>"));
                        }
                        o.push_str("</div>\n");
                        i = end + 1;
                    } else {
                        i += 1;
                    }
                    continue;
                }
                BlockKind::ColumnBreak | BlockKind::ColumnsEnd => {
                    i += 1;
                    continue;
                }
                _ => {}
            }
            if let Some(tag) = list_tag(&b.kind) {
                let name = tag.split(' ').next().unwrap_or("ul");
                o.push_str(&format!("<{tag}>\n"));
                let mut j = i;
                while j < bs.len() && bs[j].depth == b.depth && same_list(&bs[j].kind, &b.kind) {
                    let e = child_end(bs, j);
                    let kids = self.seq(&bs[j + 1..e]);
                    o.push_str(&self.item(&bs[j], &kids));
                    j = e;
                }
                o.push_str(&format!("</{name}>\n"));
                i = j;
                continue;
            }
            let e = child_end(bs, i);
            let kids = self.seq(&bs[i + 1..e]);
            o.push_str(&self.block(b, &kids));
            i = e;
        }
        o
    }

    fn item(&self, b: &Block, kids: &str) -> String {
        let cls = color_classes(&b.color);
        let text = self.text(&b.text);
        let kids = if kids.is_empty() { String::new() } else { format!("<div class=\"kids\">{kids}</div>") };
        match &b.kind {
            BlockKind::Todo { checked } => format!(
                "<li class=\"{}\"><span class=\"box\" role=\"checkbox\" aria-checked=\"{checked}\"></span><div class=\"t {cls}\">{text}{kids}</div></li>\n",
                if *checked { "done" } else { "" },
            ),
            _ => format!("<li class=\"{cls}\">{text}{kids}</li>\n"),
        }
    }

    fn block(&mut self, b: &Block, kids: &str) -> String {
        let cls = color_classes(&b.color);
        let kids_html = if kids.is_empty() { String::new() } else { format!("<div class=\"kids\">{kids}</div>") };
        match &b.kind {
            BlockKind::Paragraph => {
                if b.text.trim().is_empty() && kids.is_empty() {
                    "<p class=\"e\"></p>\n".to_string()
                } else {
                    format!("<p class=\"{cls}\">{}</p>\n{kids_html}", self.text(&b.text))
                }
            }
            BlockKind::Heading(l) => {
                let l = (*l).clamp(1, 3);
                let id = format!("h{}", self.next_heading);
                self.next_heading += 1;
                format!("<h{n} id=\"{id}\" class=\"{cls}\">{}</h{n}>\n{kids_html}", self.text(&b.text), n = l + 1)
            }
            BlockKind::Quote => format!("<blockquote class=\"{cls}\">{}{kids_html}</blockquote>\n", self.text(&b.text)),
            BlockKind::Callout { icon } => {
                let plain = if cls.contains("bg-") { "" } else { " plain" };
                format!(
                    "<div class=\"callout{plain} {cls}\"><span class=\"ci\">{}</span><div class=\"cb\">{}{kids_html}</div></div>\n",
                    escape(icon),
                    self.text(&b.text)
                )
            }
            BlockKind::Code { lang } => {
                let l = if lang.is_empty() { String::new() } else { format!("<span class=\"lang\">{}</span>", escape(lang)) };
                format!("<div class=\"code\">{l}<pre><code>{}</code></pre></div>\n", escape(&b.text))
            }
            BlockKind::Divider => "<hr>\n".to_string(),
            BlockKind::Toggle { open } => format!(
                "<details{} class=\"{cls}\"><summary>{}</summary>{kids_html}</details>\n",
                if *open { " open" } else { "" },
                self.text(&b.text)
            ),
            BlockKind::Toc => {
                let mut s = String::from("<nav class=\"toc\">");
                for (i, (level, text)) in self.headings.iter().enumerate() {
                    s.push_str(&format!("<a class=\"l{level}\" href=\"#h{i}\">{}</a>", escape(text)));
                }
                s.push_str("</nav>\n");
                s
            }
            BlockKind::Meeting => format!("<h3 class=\"meeting\"><span>🎙</span> {}</h3>\n{kids_html}", escape(&b.text)),
            BlockKind::Database => {
                let title = format!("<h3 class=\"dbt\">{}</h3>\n", escape(&b.text));
                match (self.hooks.database)(&b.text) {
                    Some(grid) => format!("{title}{}", self.table(&grid)),
                    None => title,
                }
            }
            BlockKind::Table => self.table(&table_cells(&b.text)),
            BlockKind::Bookmark { url } => {
                let label = if b.text.is_empty() { url.as_str() } else { b.text.as_str() };
                match safe_url(url) {
                    Some(u) => format!(
                        "<a class=\"bookmark\" href=\"{u}\" rel=\"noopener noreferrer\"><strong>{}</strong><span>{u}</span></a>\n",
                        escape(label),
                        u = escape(&u)
                    ),
                    None => format!("<p>{}</p>\n", escape(label)),
                }
            }
            BlockKind::Equation => format!("<div class=\"eq\">{}</div>\n", escape(&b.text)),
            BlockKind::Image { path } => {
                let caption = plain_text(&b.text);
                let src = if path.starts_with("http://") || path.starts_with("https://") {
                    Some(path.clone())
                } else {
                    (self.hooks.image)(path)
                };
                match src {
                    Some(src) => {
                        let cap = if caption.is_empty() { String::new() } else { format!("<figcaption>{}</figcaption>", escape(&caption)) };
                        format!(
                            "<figure><img src=\"{}\" alt=\"{}\" loading=\"lazy\">{cap}</figure>\n",
                            escape(&src),
                            escape(&caption)
                        )
                    }
                    None => String::new(),
                }
            }
            BlockKind::File { .. } => format!("<p class=\"file\">📎 {}</p>\n", self.text(&b.text)),
            BlockKind::Bulleted | BlockKind::Numbered | BlockKind::Todo { .. } => {
                format!("<p>{}</p>\n{kids_html}", self.text(&b.text))
            }
            BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd => String::new(),
        }
    }

    fn table(&self, grid: &[Vec<String>]) -> String {
        let mut s = String::from("<div class=\"tw\"><table>");
        for (r, row) in grid.iter().enumerate() {
            s.push_str("<tr>");
            let tag = if r == 0 { "th" } else { "td" };
            for cell in row {
                s.push_str(&format!("<{tag}>{}</{tag}>", self.text(cell)));
            }
            s.push_str("</tr>");
        }
        s.push_str("</table></div>\n");
        s
    }
}

fn collect_headings(bs: &[Block]) -> Vec<(u8, String)> {
    bs.iter()
        .filter_map(|b| match b.kind {
            BlockKind::Heading(l) => Some((l.clamp(1, 3), plain_text(&b.text))),
            _ => None,
        })
        .collect()
}

fn description(bs: &[Block]) -> String {
    let mut d = String::new();
    for b in bs {
        if matches!(b.kind, BlockKind::Paragraph | BlockKind::Quote | BlockKind::Bulleted | BlockKind::Callout { .. }) {
            let t = plain_text(&b.text);
            let t = t.trim();
            if !t.is_empty() {
                if !d.is_empty() {
                    d.push(' ');
                }
                d.push_str(t);
            }
        }
        if d.chars().count() >= 160 {
            break;
        }
    }
    let mut out: String = d.chars().take(160).collect();
    if d.chars().count() > 160 {
        out.push('…');
    }
    out
}

fn css() -> String {
    let mut light = String::new();
    let mut dark = String::new();
    let mut classes = String::new();
    for (n, lt, dt, lb, db) in COLORS {
        light.push_str(&format!("--{n}:{lt};--{n}-bg:{lb};"));
        dark.push_str(&format!("--{n}:{dt};--{n}-bg:{db};"));
        classes.push_str(&format!(".c-{n}{{color:var(--{n})}}.bg-{n}{{background:var(--{n}-bg)!important;border-radius:4px}}\n"));
    }
    format!(
        ":root{{color-scheme:light dark;--bg:#fff;--text:#2c2c2b;--text2:#7d7a75;--faint:#a19e99;--border:#e6e5e3;--accent:#2783de;--code:#f4f3f3;--callout:#f0efed;--hover:rgba(33,27,23,.05);{light}}}\n\
@media (prefers-color-scheme:dark){{:root{{--bg:#191919;--text:#f0efed;--text2:#ada9a3;--faint:#7d7a75;--border:#383836;--accent:#529cca;--code:#262626;--callout:#383836;--hover:rgba(255,255,255,.056);{dark}}}}}\n\
*{{box-sizing:border-box}}html{{-webkit-text-size-adjust:100%}}\n\
body{{margin:0;background:var(--bg);color:var(--text);font-family:ui-sans-serif,-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,'Apple Color Emoji',Arial,sans-serif;font-size:16px;line-height:1.5;-webkit-font-smoothing:antialiased}}\n\
body.serif{{font-family:ui-serif,'New York',Georgia,'Times New Roman',serif}}body.mono{{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}}body.small{{font-size:14px}}\n\
.cover{{height:30vh;min-height:140px;max-height:280px;width:100%;background-size:cover;background-position:center}}.cover img{{width:100%;height:100%;object-fit:cover;display:block}}\n\
main{{max-width:708px;margin:0 auto;padding:0 24px 80px}}main.wide{{max-width:none;padding:0 clamp(24px,6vw,96px) 80px}}\n\
.crumbs{{padding-top:18px;font-size:.85em;color:var(--text2)}}.crumbs a{{color:var(--text2);text-decoration:none}}.crumbs a:hover{{color:var(--text)}}.crumbs span{{margin:0 .4em;color:var(--faint)}}\n\
.icon{{font-size:64px;line-height:1;margin:-32px 0 8px;position:relative}}.icon.nocover{{margin-top:40px}}.icon img{{width:64px;height:64px;border-radius:6px;object-fit:cover}}\n\
h1.title{{font-size:2.5em;line-height:1.2;font-weight:700;margin:.25em 0 .6em;letter-spacing:-.01em}}\n\
h2{{font-size:1.875em;line-height:1.3;font-weight:600;margin:1.4em 0 .25em}}h3{{font-size:1.5em;line-height:1.3;font-weight:600;margin:1.1em 0 .2em}}h4{{font-size:1.25em;line-height:1.3;font-weight:600;margin:1em 0 .2em}}\n\
p{{margin:.15em 0;padding:3px 2px;white-space:pre-wrap;overflow-wrap:anywhere}}p.e{{min-height:1.5em}}\n\
a{{color:var(--text);text-decoration:underline;text-decoration-color:var(--faint);text-underline-offset:2px}}a:hover{{text-decoration-color:var(--text)}}\n\
.mention{{background:var(--hover);border-radius:4px;padding:0 .3em;text-decoration:none}}\n\
code{{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.85em;background:var(--code);color:#eb5757;border-radius:4px;padding:.15em .4em}}\n\
.code{{position:relative;background:var(--code);border-radius:6px;margin:.5em 0}}.code pre{{margin:0;padding:1em 1.1em;overflow:auto;line-height:1.5}}.code pre code{{background:none;color:var(--text);padding:0;font-size:.85em;white-space:pre}}.lang{{position:absolute;top:6px;right:10px;font-size:.7em;color:var(--faint)}}\n\
blockquote{{margin:.5em 0;padding:.1em 0 .1em 1em;border-left:3px solid var(--text);font-size:1.1em}}\n\
ul,ol{{margin:.1em 0;padding-left:1.6em}}li{{padding:2px 0}}ul.todo{{list-style:none;padding-left:.2em}}ul.todo li{{display:flex;gap:.5em;align-items:flex-start}}\n\
.box{{flex:none;width:1.05em;height:1.05em;margin-top:.28em;border:1.5px solid var(--text);border-radius:3px}}li.done .box{{background:var(--accent);border-color:var(--accent);position:relative}}li.done .box:after{{content:'';position:absolute;left:.3em;top:.08em;width:.3em;height:.6em;border:solid #fff;border-width:0 2px 2px 0;transform:rotate(45deg)}}li.done>.t{{color:var(--text2);text-decoration:line-through}}.t{{min-width:0}}\n\
.kids{{margin-left:1.5em}}details{{margin:.15em 0}}summary{{cursor:pointer;padding:3px 2px}}details>.kids{{margin-left:1.2em}}\n\
.callout{{display:flex;gap:.7em;border-radius:6px;padding:1em 1em 1em .9em;margin:.5em 0;background:var(--callout)}}.callout.plain{{border:1px solid var(--border);background:transparent}}.ci{{font-size:1.25em;line-height:1.3}}.cb{{min-width:0;overflow-wrap:anywhere;white-space:pre-wrap}}\n\
hr{{border:none;border-top:1px solid var(--border);margin:.8em 0}}\n\
figure{{margin:.7em 0}}figure img{{max-width:100%;height:auto;border-radius:4px;display:block;margin:0 auto}}figcaption{{color:var(--text2);font-size:.85em;margin-top:.4em;text-align:center}}\n\
.cols{{display:flex;gap:2em;margin:.3em 0}}.col{{flex:1 1 0;min-width:0}}@media (max-width:640px){{.cols{{flex-direction:column;gap:0}}}}\n\
.tw{{overflow-x:auto;margin:.5em 0}}table{{border-collapse:collapse;width:100%;font-size:.95em}}th,td{{border:1px solid var(--border);padding:.45em .7em;text-align:left;vertical-align:top}}th{{background:var(--hover);font-weight:600}}\n\
.bookmark{{display:block;border:1px solid var(--border);border-radius:6px;padding:.7em 1em;margin:.5em 0;text-decoration:none}}.bookmark:hover{{background:var(--hover)}}.bookmark strong{{display:block}}.bookmark span{{display:block;color:var(--text2);font-size:.8em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}\n\
.eq{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;text-align:center;background:var(--code);border-radius:6px;padding:.8em;margin:.5em 0;overflow-x:auto}}\n\
.toc{{display:flex;flex-direction:column;margin:.5em 0}}.toc a{{color:var(--text2);text-decoration:none;padding:2px 0}}.toc a:hover{{color:var(--text)}}.toc .l2{{padding-left:1em}}.toc .l3{{padding-left:2em}}\n\
.file{{color:var(--text2)}}.sub{{margin-top:2.5em;border-top:1px solid var(--border);padding-top:1em}}.sub h5{{margin:0 0 .5em;font-size:.8em;font-weight:600;color:var(--text2)}}.sub a{{display:flex;gap:.5em;padding:.35em .4em;border-radius:6px;text-decoration:none}}.sub a:hover{{background:var(--hover)}}\n\
footer{{margin-top:4em;text-align:center;font-size:.75em;color:var(--faint)}}\n\
{classes}@media print{{.cover{{height:120px}}footer{{display:none}}}}\n"
    )
}

fn favicon(icon: &str) -> String {
    let e = icon.trim();
    if e.is_empty() || e.chars().count() > 4 || e.contains('/') || e.contains('.') {
        return String::new();
    }
    format!(
        "<link rel=\"icon\" href=\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'%3E%3Ctext y='.9em' font-size='90'%3E{}%3C/text%3E%3C/svg%3E\">\n",
        escape(e)
    )
}

/// The complete HTML document of a page.
pub fn render_page(opts: &Options, blocks: &[Block], hooks: &mut Hooks) -> String {
    let mut r = R { hooks, headings: collect_headings(blocks), next_heading: 0 };
    let body = r.seq(blocks);
    let desc = description(blocks);
    let fr = opts.lang == "fr";

    let cover = match &opts.cover {
        Cover::None => String::new(),
        Cover::Gradient(n) => {
            let (a, b) = COVERS[n % COVERS.len()];
            format!("<div class=\"cover\" style=\"background:linear-gradient(135deg,{a},{b})\"></div>\n")
        }
        Cover::Image(u) => format!("<div class=\"cover\"><img src=\"{}\" alt=\"\"></div>\n", escape(u)),
    };
    let has_cover = opts.cover != Cover::None;
    let icon_html = {
        let i = opts.icon.trim();
        if i.is_empty() {
            String::new()
        } else {
            let nocover = if has_cover { "" } else { " nocover" };
            if i.contains('/') || i.contains('.') {
                match (r.hooks.image)(i) {
                    Some(u) => format!("<div class=\"icon{nocover}\"><img src=\"{}\" alt=\"\"></div>\n", escape(&u)),
                    None => String::new(),
                }
            } else {
                format!("<div class=\"icon{nocover}\">{}</div>\n", escape(i))
            }
        }
    };
    let crumbs = if opts.crumbs.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = opts
            .crumbs
            .iter()
            .map(|c| format!("<a href=\"{}\">{} {}</a>", escape(&c.url), escape(&c.icon), escape(&c.title)))
            .collect();
        format!("<div class=\"crumbs\">{}</div>\n", parts.join("<span>/</span>"))
    };
    let subs = if opts.children.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = opts
            .children
            .iter()
            .map(|c| format!("<a href=\"{}\"><span>{}</span><span>{}</span></a>", escape(&c.url), escape(&c.icon), escape(&c.title)))
            .collect();
        format!("<section class=\"sub\"><h5>Pages</h5>{}</section>\n", items.join(""))
    };
    let mut classes = String::new();
    match opts.font {
        "serif" => classes.push_str("serif "),
        "mono" => classes.push_str("mono "),
        _ => {}
    }
    if opts.small {
        classes.push_str("small");
    }
    let og_image = opts
        .og_image
        .as_ref()
        .map(|u| format!("<meta property=\"og:image\" content=\"{}\">\n<meta name=\"twitter:card\" content=\"summary_large_image\">\n", escape(u)))
        .unwrap_or_default();
    let t = escape(opts.title);
    let d = escape(&desc);
    format!(
        "<!doctype html>\n<html lang=\"{lang}\"><head><meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
<title>{t}</title>\n<meta name=\"description\" content=\"{d}\">\n\
<meta name=\"robots\" content=\"noindex,nofollow,noarchive\">\n<meta name=\"referrer\" content=\"no-referrer\">\n\
<meta property=\"og:type\" content=\"article\"><meta property=\"og:title\" content=\"{t}\">\n<meta property=\"og:description\" content=\"{d}\">\n{og_image}\
{fav}<style>\n{css}</style></head>\n<body class=\"{classes}\">\n{cover}<main class=\"{wide}\">\n{crumbs}{icon_html}<h1 class=\"title\">{t}</h1>\n{body}{subs}\
<footer>{foot}</footer>\n</main>\n</body></html>\n",
        lang = if fr { "fr" } else { "en" },
        fav = favicon(opts.icon),
        css = css(),
        wide = if opts.wide { "wide" } else { "" },
        foot = if fr { "Publié avec NøteCraft" } else { "Published with NøteCraft" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts<'a>() -> Options<'a> {
        Options {
            title: "Plan <1>",
            icon: "🚀",
            cover: Cover::Gradient(1),
            font: "",
            small: false,
            wide: false,
            lang: "fr",
            crumbs: vec![],
            children: vec![],
            og_image: None,
        }
    }

    fn render(blocks: &[Block]) -> String {
        let mut img = |p: &str| Some(format!("a/{}", p.rsplit('/').next().unwrap_or(p)));
        let link = |t: &str| (t == "Other").then(|| "/tok/other/".to_string());
        let db = |_: &str| None;
        let mut hooks = Hooks { image: &mut img, link: &link, database: &db };
        render_page(&opts(), blocks, &mut hooks)
    }

    #[test]
    fn escapes_and_refuses_scripts() {
        let h = render(&[Block::new(
            BlockKind::Paragraph,
            "<script>alert(1)</script> [x](javascript:alert(1)) [ok](https://a.ch)",
        )]);
        assert!(!h.contains("<script>alert"));
        assert!(!h.contains("javascript:"));
        assert!(h.contains("href=\"https://a.ch\""));
        assert!(h.contains("&lt;script&gt;"));
        assert!(h.contains("<title>Plan &lt;1&gt;</title>"));
        assert!(h.contains("noindex"));
    }

    #[test]
    fn lists_toggles_columns_and_mentions() {
        let blocks = vec![
            Block::new(BlockKind::Heading(1), "Intro"),
            Block::new(BlockKind::Bulleted, "a"),
            Block::new(BlockKind::Bulleted, "b").with_depth(1),
            Block::new(BlockKind::Todo { checked: true }, "done"),
            Block::new(BlockKind::Toggle { open: false }, "More"),
            Block::new(BlockKind::Paragraph, "inside").with_depth(1),
            Block::new(BlockKind::ColumnsStart, ""),
            Block::new(BlockKind::Paragraph, "left"),
            Block::new(BlockKind::ColumnBreak, ""),
            Block::new(BlockKind::Paragraph, "right"),
            Block::new(BlockKind::ColumnsEnd, ""),
            Block::new(BlockKind::Paragraph, "see [[Other]] and [[Private]]"),
            Block::new(BlockKind::Image { path: "assets/p.png".into() }, "cap"),
            Block::new(BlockKind::Table, "| A | B |\n|---|---|\n| 1 | 2 |"),
        ];
        let h = render(&blocks);
        assert!(h.contains("<h2 id=\"h0\""));
        assert!(h.contains("<li class=\"\">a<div class=\"kids\"><ul>"));
        assert!(h.contains("li class=\"done\""));
        assert!(h.contains("<summary>More</summary>"));
        assert_eq!(h.matches("class=\"col\"").count(), 2);
        assert!(h.contains("<a class=\"mention\" href=\"/tok/other/\">Other</a>"));
        assert!(h.contains("<span class=\"mention\">Private</span>"));
        assert!(h.contains("src=\"a/p.png\""));
        assert!(h.contains("<th>A</th>"));
        assert!(h.contains("Publié avec NøteCraft"));
    }

    #[test]
    fn description_is_short_plain_text() {
        let d = description(&[Block::new(BlockKind::Paragraph, &"mot ".repeat(80))]);
        assert!(d.chars().count() <= 161);
        assert!(d.ends_with('…'));
    }
}
