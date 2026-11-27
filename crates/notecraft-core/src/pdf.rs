//! PDF export: A4 pages, embedded fonts (Lato and Inconsolata, SIL OFL), pictures,
//! clickable links, bookmarks-free but numbered pages. Layout is done here, the file
//! itself is written by `krilla`.

use crate::blocks::Block;
use crate::render::{css_colors, image_size, items, Item, Kind, Marker, Run};
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font as KFont, TextDirection};
use krilla::{action, annotation, Data, Document};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 60.0;
const BOTTOM: f32 = 70.0;

type Rgb = (u8, u8, u8);
const INK: Rgb = (0x37, 0x35, 0x2f);
const GREY: Rgb = (0x78, 0x77, 0x74);
const LINK: Rgb = (0x33, 0x7e, 0xa9);
const CODE_BG: Rgb = (0xf7, 0xf6, 0xf3);
const CALLOUT_BG: Rgb = (0xf1, 0xf1, 0xef);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum F {
    Regular,
    Bold,
    Italic,
    BoldItalic,
    Mono,
}

const FONT_DATA: [(F, &[u8]); 5] = [
    (F::Regular, include_bytes!("../fonts/Lato-Regular.ttf")),
    (F::Bold, include_bytes!("../fonts/Lato-Bold.ttf")),
    (F::Italic, include_bytes!("../fonts/Lato-Italic.ttf")),
    (F::BoldItalic, include_bytes!("../fonts/Lato-BoldItalic.ttf")),
    (F::Mono, include_bytes!("../fonts/Inconsolata.ttf")),
];

static FACES: LazyLock<HashMap<F, ttf_parser::Face<'static>>> = LazyLock::new(|| {
    FONT_DATA.iter().filter_map(|(f, d)| ttf_parser::Face::parse(d, 0).ok().map(|face| (*f, face))).collect()
});

fn has_glyph(c: char, f: F) -> bool {
    c.is_whitespace() || FACES.get(&f).is_some_and(|face| face.glyph_index(c).is_some())
}

/// Characters the font cannot draw (emoji, rare scripts) are left out instead of showing boxes.
fn clean(s: &str, f: F) -> String {
    s.chars()
        .filter(|&c| has_glyph(c, f) && !c.is_control() || c == ' ')
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect()
}

fn char_w(c: char, f: F, size: f32) -> f32 {
    let Some(face) = FACES.get(&f) else { return size * 0.5 };
    let upm = f32::from(face.units_per_em());
    face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map_or(size * 0.3, |a| f32::from(a) / upm * size)
}

fn text_w(s: &str, f: F, size: f32) -> f32 {
    s.chars().map(|c| char_w(c, f, size)).sum()
}

fn hex_rgb(hex: &str) -> Rgb {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    (p(0), p(2), p(4))
}

enum Cmd {
    Text { x: f32, y: f32, f: F, size: f32, color: Rgb, text: String },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgb },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, w: f32, color: Rgb },
    Image { x: f32, y: f32, w: f32, h: f32, idx: usize },
    Link { x: f32, y: f32, w: f32, h: f32, url: String },
}

struct Word {
    text: String,
    f: F,
    color: Rgb,
    link: Option<String>,
    underline: bool,
    strike: bool,
    code: bool,
    w: f32,
}

struct Layout {
    pages: Vec<Vec<Cmd>>,
    cur: Vec<Cmd>,
    /// Distance from the top of the page.
    y: f32,
    images: Vec<krilla::image::Image>,
}

impl Layout {
    fn new() -> Self {
        Layout { pages: Vec::new(), cur: Vec::new(), y: MARGIN, images: Vec::new() }
    }

    fn ensure(&mut self, h: f32) {
        if self.y + h > PAGE_H - BOTTOM && self.y > MARGIN {
            self.pages.push(std::mem::take(&mut self.cur));
            self.y = MARGIN;
        }
    }

    fn gap(&mut self, h: f32) {
        if self.y > MARGIN {
            self.y += h;
        }
    }

    /// Break `words` into lines no wider than `avail`.
    fn wrap(words: Vec<Word>, avail: f32, size: f32) -> Vec<Vec<Word>> {
        let mut lines: Vec<Vec<Word>> = vec![Vec::new()];
        let mut used = 0.0;
        for mut w in words {
            if w.text == "\n" {
                lines.push(Vec::new());
                used = 0.0;
                continue;
            }
            let visible = w.w - if w.text.ends_with(' ') { char_w(' ', w.f, size) } else { 0.0 };
            if used + visible > avail && used > 0.0 {
                lines.push(Vec::new());
                used = 0.0;
            }
            if visible > avail {
                // a single word wider than the line: cut it
                let mut head = String::new();
                let mut acc = 0.0;
                let mut rest = String::new();
                for c in w.text.chars() {
                    let cw = char_w(c, w.f, size);
                    if rest.is_empty() && (acc + cw <= avail || head.is_empty()) {
                        acc += cw;
                        head.push(c);
                    } else {
                        rest.push(c);
                    }
                }
                let tail = Word { text: rest.clone(), w: text_w(&rest, w.f, size), ..clone_word(&w) };
                w.text = head;
                w.w = acc;
                lines.last_mut().expect("line").push(w);
                lines.push(Vec::new());
                used = 0.0;
                if !tail.text.is_empty() {
                    let mut rest_lines = Self::wrap(vec![tail], avail, size);
                    let last = rest_lines.pop().unwrap_or_default();
                    lines.extend(rest_lines);
                    used = last.iter().map(|x| x.w).sum();
                    *lines.last_mut().expect("line") = last;
                }
                continue;
            }
            used += w.w;
            lines.last_mut().expect("line").push(w);
        }
        lines
    }

    fn paragraph(&mut self, words: Vec<Word>, x: f32, width: f32, size: f32, lead: f32, deco: &Deco) {
        let lines = Self::wrap(words, width, size);
        let n = lines.len();
        for (k, line) in lines.into_iter().enumerate() {
            self.ensure(lead);
            let top = self.y;
            if let Some(bg) = deco.bg {
                let up = if k == 0 { deco.pad } else { 0.0 };
                let down = if k + 1 == n { deco.pad } else { 0.0 };
                self.cur.push(Cmd::Rect {
                    x: x - deco.pad_x,
                    y: top - up,
                    w: width + 2.0 * deco.pad_x,
                    h: lead + up + down,
                    color: bg,
                });
            }
            if let Some(bar) = deco.bar {
                self.cur.push(Cmd::Rect { x: x - 11.0, y: top, w: 2.5, h: lead, color: bar });
            }
            let base = top + lead * 0.5 + size * 0.34;
            if k == 0 {
                if let Some(m) = &deco.marker {
                    self.marker(m, x, base, size, top, lead);
                }
            }
            let mut cx = x;
            for w in line {
                if w.code {
                    self.cur.push(Cmd::Rect {
                        x: cx - 1.0,
                        y: base - size * 0.95,
                        w: w.w + 2.0,
                        h: size * 1.25,
                        color: CODE_BG,
                    });
                }
                self.cur.push(Cmd::Text { x: cx, y: base, f: w.f, size, color: w.color, text: w.text.clone() });
                let visible = w.w - if w.text.ends_with(' ') { char_w(' ', w.f, size) } else { 0.0 };
                if w.underline || w.link.is_some() {
                    self.cur.push(Cmd::Line {
                        x1: cx,
                        y1: base + 1.6,
                        x2: cx + visible,
                        y2: base + 1.6,
                        w: 0.6,
                        color: w.color,
                    });
                }
                if w.strike {
                    self.cur.push(Cmd::Line {
                        x1: cx,
                        y1: base - size * 0.3,
                        x2: cx + visible,
                        y2: base - size * 0.3,
                        w: 0.6,
                        color: w.color,
                    });
                }
                if let Some(url) = w.link {
                    self.cur.push(Cmd::Link { x: cx, y: top, w: visible, h: lead, url });
                }
                cx += w.w;
            }
            self.y += lead;
        }
    }

    fn marker(&mut self, m: &Marker, x: f32, base: f32, size: f32, top: f32, lead: f32) {
        let mx = x - 16.0;
        match m {
            Marker::None => {}
            Marker::Bullet => {
                let r = size * 0.14;
                self.cur.push(Cmd::Rect { x: mx + 5.0, y: base - size * 0.38 - r, w: r * 2.0, h: r * 2.0, color: INK });
            }
            Marker::Num(n) => {
                let t = format!("{n}.");
                let w = text_w(&t, F::Regular, size);
                self.cur.push(Cmd::Text { x: x - 5.0 - w, y: base, f: F::Regular, size, color: GREY, text: t });
            }
            Marker::Todo(done) => {
                let s = size * 0.8;
                let y0 = top + (lead - s) / 2.0;
                if *done {
                    self.cur.push(Cmd::Rect { x: mx, y: y0, w: s, h: s, color: LINK });
                    let p = [(0.2, 0.52), (0.42, 0.74), (0.8, 0.28)];
                    for k in 0..2 {
                        self.cur.push(Cmd::Line {
                            x1: mx + s * p[k].0,
                            y1: y0 + s * p[k].1,
                            x2: mx + s * p[k + 1].0,
                            y2: y0 + s * p[k + 1].1,
                            w: 1.2,
                            color: (255, 255, 255),
                        });
                    }
                } else {
                    for (a, b, c, d) in [(0.0, 0.0, s, 0.0), (s, 0.0, s, s), (s, s, 0.0, s), (0.0, s, 0.0, 0.0)] {
                        self.cur.push(Cmd::Line {
                            x1: mx + a,
                            y1: y0 + b,
                            x2: mx + c,
                            y2: y0 + d,
                            w: 0.8,
                            color: GREY,
                        });
                    }
                }
            }
            Marker::Toggle => {
                let s = size * 0.3;
                let cy = base - size * 0.34;
                for (a, b, c, d) in [(0.0, -s, s * 1.6, 0.0), (s * 1.6, 0.0, 0.0, s)] {
                    self.cur.push(Cmd::Line {
                        x1: mx + 3.0 + a,
                        y1: cy + b,
                        x2: mx + 3.0 + c,
                        y2: cy + d,
                        w: 1.0,
                        color: GREY,
                    });
                }
            }
        }
    }
}

fn clone_word(w: &Word) -> Word {
    Word {
        text: w.text.clone(),
        f: w.f,
        color: w.color,
        link: w.link.clone(),
        underline: w.underline,
        strike: w.strike,
        code: w.code,
        w: w.w,
    }
}

#[derive(Default)]
struct Deco {
    bg: Option<Rgb>,
    bar: Option<Rgb>,
    marker: Option<Marker>,
    pad: f32,
    pad_x: f32,
}

fn words_of(runs: &[Run], base: F, size: f32, color: Rgb) -> Vec<Word> {
    let mut out = Vec::new();
    for r in runs {
        let f = if r.style.code {
            F::Mono
        } else {
            match (base == F::Bold || r.style.bold, r.style.italic || base == F::Italic) {
                (true, true) => F::BoldItalic,
                (true, false) => F::Bold,
                (false, true) => F::Italic,
                _ => F::Regular,
            }
        };
        let col = if r.style.link.is_some() { LINK } else { color };
        for (k, line) in r.text.split('\n').enumerate() {
            if k > 0 {
                out.push(Word {
                    text: "\n".into(),
                    f,
                    color: col,
                    link: None,
                    underline: false,
                    strike: false,
                    code: false,
                    w: 0.0,
                });
            }
            for piece in line.split_inclusive(' ') {
                let text = clean(piece, f);
                if text.is_empty() {
                    continue;
                }
                out.push(Word {
                    w: text_w(&text, f, size),
                    text,
                    f,
                    color: col,
                    link: r.style.link.clone(),
                    underline: r.style.underline,
                    strike: r.style.strike,
                    code: r.style.code,
                });
            }
        }
    }
    out
}

fn item_color(it: &Item) -> (Rgb, Option<Rgb>) {
    let (text, bg) = css_colors(&it.color);
    (text.map_or(INK, hex_rgb), bg.map(hex_rgb))
}

fn load_image(bytes: &[u8]) -> Option<krilla::image::Image> {
    let data: Data = bytes.to_vec().into();
    if bytes.starts_with(b"\x89PNG") {
        krilla::image::Image::from_png(data, true).ok()
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        krilla::image::Image::from_jpeg(data, true).ok()
    } else if bytes.starts_with(b"GIF8") {
        krilla::image::Image::from_gif(data, true).ok()
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        krilla::image::Image::from_webp(data, true).ok()
    } else {
        None
    }
}

/// Lay the items out page by page.
fn layout(title: &str, blocks: &[Block], read_asset: &dyn Fn(&str) -> Option<Vec<u8>>) -> Layout {
    let mut l = Layout::new();
    let width = PAGE_W - 2.0 * MARGIN;
    l.paragraph(words_of(&[plain(title)], F::Bold, 26.0, INK), MARGIN, width, 26.0, 33.0, &Deco::default());
    l.y += 14.0;
    for it in items(blocks) {
        let indent = f32::from(it.depth) * 20.0;
        let x = MARGIN + indent;
        let w = width - indent;
        let (color, bg) = item_color(&it);
        match &it.kind {
            Kind::Para => {
                if it.runs.iter().all(|r| r.text.is_empty()) && matches!(it.marker, Marker::None) {
                    l.y += 8.0;
                    continue;
                }
                let (marker, dx) = list_marker(&it.marker);
                let deco = Deco { bg, marker, pad: 2.0, pad_x: 4.0, ..Deco::default() };
                l.paragraph(words_of(&it.runs, F::Regular, 11.0, color), x + dx, w - dx, 11.0, 16.5, &deco);
                l.y += 3.5;
            }
            Kind::Heading(n) => {
                let (size, before) = match n {
                    1 => (22.0, 16.0),
                    2 => (17.0, 12.0),
                    _ => (14.0, 8.0),
                };
                l.gap(before);
                l.ensure(size * 1.3 + 30.0);
                l.paragraph(words_of(&it.runs, F::Bold, size, color), x, w, size, size * 1.3, &Deco::default());
                l.y += 4.0;
            }
            Kind::Quote => {
                let deco = Deco { bar: Some(INK), ..Deco::default() };
                l.paragraph(words_of(&it.runs, F::Regular, 12.0, color), x + 14.0, w - 14.0, 12.0, 18.0, &deco);
                l.y += 4.0;
            }
            Kind::Callout(icon) => {
                let mut runs = it.runs.clone();
                let icon_ok = !icon.is_empty() && icon.chars().all(|c| has_glyph(c, F::Regular));
                if icon_ok {
                    runs.insert(0, Run { text: format!("{icon}  "), style: Default::default() });
                }
                l.y += 3.0;
                let deco = Deco { bg: Some(bg.unwrap_or(CALLOUT_BG)), pad: 8.0, pad_x: 10.0, ..Deco::default() };
                l.paragraph(words_of(&runs, F::Regular, 11.0, color), x + 12.0, w - 24.0, 11.0, 16.5, &deco);
                l.y += 12.0;
            }
            Kind::Code(_) => {
                let code: String = it.runs.iter().map(|r| r.text.as_str()).collect();
                let mut ws = Vec::new();
                for (k, line) in code.split('\n').enumerate() {
                    if k > 0 {
                        ws.push(Word {
                            text: "\n".into(),
                            f: F::Mono,
                            color: INK,
                            link: None,
                            underline: false,
                            strike: false,
                            code: false,
                            w: 0.0,
                        });
                    }
                    let text = clean(line, F::Mono);
                    if !text.is_empty() {
                        ws.push(Word {
                            w: text_w(&text, F::Mono, 9.5),
                            text,
                            f: F::Mono,
                            color: INK,
                            link: None,
                            underline: false,
                            strike: false,
                            code: false,
                        });
                    }
                }
                l.y += 3.0;
                let deco = Deco { bg: Some(CODE_BG), pad: 8.0, pad_x: 10.0, ..Deco::default() };
                l.paragraph(ws, x + 12.0, w - 24.0, 9.5, 14.0, &deco);
                l.y += 12.0;
            }
            Kind::Table(_) => {}
            Kind::Divider => {
                l.ensure(20.0);
                l.y += 8.0;
                l.cur.push(Cmd::Line { x1: x, y1: l.y, x2: x + w, y2: l.y, w: 0.7, color: (0xdd, 0xdd, 0xd9) });
                l.y += 10.0;
            }
            Kind::Image(path) => {
                let caption: String = it.runs.iter().map(|r| r.text.as_str()).collect();
                let img = read_asset(path).and_then(|b| image_size(&b).zip(load_image(&b)));
                match img {
                    Some(((pw, ph), image)) if pw > 0 && ph > 0 => {
                        let max_h = PAGE_H - MARGIN - BOTTOM - 40.0;
                        let mut dw = w.min(pw as f32 * 0.75);
                        let mut dh = dw * ph as f32 / pw as f32;
                        if dh > max_h {
                            dh = max_h;
                            dw = dh * pw as f32 / ph as f32;
                        }
                        l.ensure(dh + 24.0);
                        let idx = l.images.len();
                        l.images.push(image);
                        l.y += 4.0;
                        l.cur.push(Cmd::Image { x: x + (w - dw) / 2.0, y: l.y, w: dw, h: dh, idx });
                        l.y += dh + 6.0;
                        if !caption.is_empty() {
                            let ws = words_of(&[plain(&caption)], F::Italic, 9.5, GREY);
                            l.paragraph(ws, x, w, 9.5, 13.0, &Deco::default());
                        }
                        l.y += 6.0;
                    }
                    _ => {
                        let text = format!("[image{}{caption}]", if caption.is_empty() { "" } else { ": " });
                        l.paragraph(
                            words_of(&[plain(&text)], F::Italic, 10.0, GREY),
                            x,
                            w,
                            10.0,
                            14.0,
                            &Deco::default(),
                        );
                        l.y += 3.0;
                    }
                }
            }
            Kind::File => {
                l.paragraph(words_of(&it.runs, F::Regular, 11.0, color), x, w, 11.0, 16.5, &Deco::default());
                l.y += 3.5;
            }
        }
    }
    l.pages.push(std::mem::take(&mut l.cur));
    l
}

fn plain(text: &str) -> Run {
    Run { text: text.to_string(), style: Default::default() }
}

fn list_marker(m: &Marker) -> (Option<Marker>, f32) {
    match m {
        Marker::None => (None, 0.0),
        other => (Some(other.clone()), 20.0),
    }
}

fn fill(c: Rgb) -> Fill {
    Fill { paint: rgb::Color::new(c.0, c.1, c.2).into(), ..Default::default() }
}

/// Render a page as a PDF.
pub fn to_pdf(title: &str, blocks: &[Block], read_asset: &dyn Fn(&str) -> Option<Vec<u8>>) -> Vec<u8> {
    let l = layout(title, blocks, read_asset);
    let fonts: HashMap<F, KFont> =
        FONT_DATA.iter().filter_map(|(f, d)| KFont::new(Arc::new(d.to_vec()).into(), 0).map(|k| (*f, k))).collect();
    let mut doc = Document::new();
    let total = l.pages.len();
    for (n, cmds) in l.pages.iter().enumerate() {
        let mut page = doc.start_page_with(PageSettings::new(Size::from_wh(PAGE_W, PAGE_H).expect("page size")));
        let mut links = Vec::new();
        {
            let mut s = page.surface();
            for c in cmds {
                match c {
                    Cmd::Rect { x, y, w, h, color } => {
                        if let Some(r) = Rect::from_xywh(*x, *y, *w, *h) {
                            let mut pb = PathBuilder::new();
                            pb.push_rect(r);
                            if let Some(p) = pb.finish() {
                                s.set_stroke(None);
                                s.set_fill(Some(fill(*color)));
                                s.draw_path(&p);
                            }
                        }
                    }
                    Cmd::Line { x1, y1, x2, y2, w, color } => {
                        let mut pb = PathBuilder::new();
                        pb.move_to(*x1, *y1);
                        pb.line_to(*x2, *y2);
                        if let Some(p) = pb.finish() {
                            s.set_fill(None);
                            s.set_stroke(Some(Stroke {
                                paint: rgb::Color::new(color.0, color.1, color.2).into(),
                                width: *w,
                                ..Default::default()
                            }));
                            s.draw_path(&p);
                        }
                    }
                    Cmd::Text { x, y, f, size, color, text } => {
                        if let Some(font) = fonts.get(f) {
                            s.set_stroke(None);
                            s.set_fill(Some(fill(*color)));
                            s.draw_text(Point::from_xy(*x, *y), font.clone(), *size, text, false, TextDirection::Auto);
                        }
                    }
                    Cmd::Image { x, y, w, h, idx } => {
                        if let (Some(img), Some(sz)) = (l.images.get(*idx), Size::from_wh(*w, *h)) {
                            s.push_transform(&krilla::geom::Transform::from_translate(*x, *y));
                            s.draw_image(img.clone(), sz);
                            s.pop();
                        }
                    }
                    Cmd::Link { x, y, w, h, url } => links.push((*x, *y, *w, *h, url.clone())),
                }
            }
            // footer: title on the left, page number on the right
            let foot = PAGE_H - 36.0;
            let short: String = clean(title, F::Regular).chars().take(60).collect();
            if let Some(font) = fonts.get(&F::Regular) {
                s.set_stroke(None);
                s.set_fill(Some(fill(GREY)));
                s.draw_text(Point::from_xy(MARGIN, foot), font.clone(), 8.5, &short, false, TextDirection::Auto);
                let num = format!("{} / {}", n + 1, total);
                let nw = text_w(&num, F::Regular, 8.5);
                s.draw_text(
                    Point::from_xy(PAGE_W - MARGIN - nw, foot),
                    font.clone(),
                    8.5,
                    &num,
                    false,
                    TextDirection::Auto,
                );
            }
            s.finish();
        }
        for (x, y, w, h, url) in links {
            if let Some(r) = Rect::from_xywh(x, y, w, h) {
                let act = action::LinkAction::new(url);
                page.add_annotation(annotation::LinkAnnotation::new(r, annotation::Target::Action(act.into())).into());
            }
        }
        page.finish();
    }
    doc.set_metadata(Metadata::new().title(title.to_string()).creator("NøteCraft".to_string()));
    doc.finish().unwrap_or_default()
}
