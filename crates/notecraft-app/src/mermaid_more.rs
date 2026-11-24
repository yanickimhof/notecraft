//! The Mermaid diagrams besides flowcharts: sequence diagrams, pie charts and state diagrams
//! (drawn as flowcharts). Anything else stays a code block.

use crate::mermaid::{self, Colors, Graph, Laid};
use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape as EShape, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    Filled,
    Open,
    Cross,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Msg {
        from: usize,
        to: usize,
        text: String,
        dashed: bool,
        head: Head,
    },
    Note {
        a: usize,
        b: usize,
        text: String,
    },
    /// `loop`, `alt`, `else`, `end`... shown as a small label so the structure stays readable.
    Marker(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub parts: Vec<(String, String)>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pie {
    pub title: String,
    pub slices: Vec<(String, f64)>,
    pub show_data: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    Sequence(Sequence),
    Pie(Pie),
}

/// A diagram ready to draw: a flowchart with its layout, or another kind with its size.
pub enum Drawn {
    Flow(Graph, Laid),
    Other(Diagram, Vec2),
}

impl Drawn {
    pub fn size(&self) -> Vec2 {
        match self {
            Drawn::Flow(_, l) => l.size,
            Drawn::Other(_, s) => *s,
        }
    }

    pub fn draw(&self, painter: &Painter, origin: Pos2, colors: &Colors) {
        match self {
            Drawn::Flow(g, l) => mermaid::draw(painter, g, l, origin, colors),
            Drawn::Other(d, size) => draw(painter, d, *size, origin, colors),
        }
    }
}

/// Parse and lay out any supported Mermaid source.
pub fn prepare(painter: &Painter, src: &str) -> Option<Drawn> {
    if let Some(g) = mermaid::parse(src) {
        let laid = mermaid::layout(painter, &g);
        return Some(Drawn::Flow(g, laid));
    }
    if let Some(flow) = state_as_flowchart(src) {
        if let Some(g) = mermaid::parse(&flow) {
            let laid = mermaid::layout(painter, &g);
            return Some(Drawn::Flow(g, laid));
        }
    }
    let d = parse(src)?;
    let size = measure(painter, &d);
    Some(Drawn::Other(d, size))
}

fn lines_of(src: &str) -> impl Iterator<Item = &str> {
    src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("%%"))
}

pub fn parse(src: &str) -> Option<Diagram> {
    let first = lines_of(src).next()?.to_ascii_lowercase();
    if first.starts_with("sequencediagram") {
        parse_sequence(src).map(Diagram::Sequence)
    } else if first.starts_with("pie") {
        parse_pie(src).map(Diagram::Pie)
    } else {
        None
    }
}

/// `stateDiagram-v2` written as the flowchart that draws the same thing.
fn state_as_flowchart(src: &str) -> Option<String> {
    let mut it = lines_of(src);
    if !it.next()?.to_ascii_lowercase().starts_with("statediagram") {
        return None;
    }
    let mut out = String::from("graph LR\n");
    for l in it {
        let Some((from, rest)) = l.split_once("-->") else { continue };
        let (to, label) = match rest.split_once(':') {
            Some((t, lab)) => (t.trim(), lab.trim()),
            None => (rest.trim(), ""),
        };
        let from = from.trim();
        let node = |s: &str, as_target: bool| -> String {
            if s == "[*]" {
                if as_target {
                    "end_state((◉))".to_string()
                } else {
                    "start_state((●))".to_string()
                }
            } else {
                let id: String = s.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
                format!("{id}([{s}])")
            }
        };
        let link = if label.is_empty() { "-->".to_string() } else { format!("-->|{label}|") };
        out.push_str(&format!("{} {} {}\n", node(from, false), link, node(to, true)));
    }
    Some(out)
}

fn parse_pie(src: &str) -> Option<Pie> {
    let mut pie = Pie { title: String::new(), slices: Vec::new(), show_data: false };
    let mut it = lines_of(src);
    let head = it.next()?;
    let rest = head[3..].trim();
    let (rest, show) = match rest.strip_prefix("showData") {
        Some(r) => (r.trim(), true),
        None => (rest, false),
    };
    pie.show_data = show;
    if let Some(t) = rest.strip_prefix("title") {
        pie.title = t.trim().to_string();
    }
    for l in it {
        if let Some(t) = l.strip_prefix("title") {
            pie.title = t.trim().to_string();
        } else if l.eq_ignore_ascii_case("showdata") {
            pie.show_data = true;
        } else if let Some((label, value)) = l.rsplit_once(':') {
            let label = label.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            if let Ok(v) = value.trim().parse::<f64>() {
                if v >= 0.0 && !label.is_empty() {
                    pie.slices.push((label, v));
                }
            }
        }
    }
    if pie.slices.is_empty() {
        None
    } else {
        Some(pie)
    }
}

const ARROWS: [(&str, bool, Head); 8] = [
    ("-->>", true, Head::Filled),
    ("->>", false, Head::Filled),
    ("--x", true, Head::Cross),
    ("-x", false, Head::Cross),
    ("--)", true, Head::Open),
    ("-)", false, Head::Open),
    ("-->", true, Head::None),
    ("->", false, Head::None),
];

fn parse_sequence(src: &str) -> Option<Sequence> {
    let mut seq = Sequence { parts: Vec::new(), items: Vec::new() };
    let part = |seq: &mut Sequence, id: &str, label: Option<&str>| -> usize {
        if let Some(i) = seq.parts.iter().position(|p| p.0 == id) {
            if let Some(l) = label {
                seq.parts[i].1 = l.to_string();
            }
            return i;
        }
        seq.parts.push((id.to_string(), label.unwrap_or(id).to_string()));
        seq.parts.len() - 1
    };
    for l in lines_of(src).skip(1) {
        let lower = l.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("participant ").or_else(|| lower.strip_prefix("actor ")) {
            let offset = l.len() - rest.len();
            let rest = &l[offset..];
            match rest.split_once(" as ") {
                Some((id, label)) => part(&mut seq, id.trim(), Some(label.trim())),
                None => part(&mut seq, rest.trim(), None),
            };
            continue;
        }
        if lower.starts_with("note ") {
            let Some((who, text)) = l[5..].split_once(':') else { continue };
            let who_l = who.to_ascii_lowercase();
            let names = |s: &str| -> Vec<String> { s.split(',').map(|n| n.trim().to_string()).collect() };
            let (a, b) = if let Some(r) = who_l.strip_prefix("over ") {
                let ns = names(&who[who.len() - r.len()..]);
                let a = part(&mut seq, &ns[0], None);
                let b = ns.get(1).map(|n| part(&mut seq, n, None)).unwrap_or(a);
                (a, b)
            } else if let Some(r) = who_l.strip_prefix("left of ").or_else(|| who_l.strip_prefix("right of ")) {
                let i = part(&mut seq, who[who.len() - r.len()..].trim(), None);
                (i, i)
            } else {
                continue;
            };
            seq.items.push(Item::Note { a: a.min(b), b: a.max(b), text: text.trim().to_string() });
            continue;
        }
        if ["loop", "alt", "opt", "par", "else", "and", "end", "critical", "break", "rect"]
            .iter()
            .any(|k| lower == *k || lower.starts_with(&format!("{k} ")))
        {
            seq.items.push(Item::Marker(l.to_string()));
            continue;
        }
        if ["autonumber", "title", "activate", "deactivate", "create", "destroy", "box"]
            .iter()
            .any(|k| lower.starts_with(k))
        {
            continue;
        }
        // a message: From<arrow>To: text
        let Some((head, text)) = l.split_once(':') else { continue };
        let mut found = None;
        for (arrow, dashed, h) in ARROWS {
            if let Some(pos) = head.find(arrow) {
                found = Some((pos, arrow.len(), dashed, h));
                break;
            }
        }
        let Some((pos, len, dashed, h)) = found else { continue };
        let from = head[..pos].trim().trim_end_matches(['+', '-']).trim();
        let to = head[pos + len..].trim().trim_start_matches(['+', '-']).trim();
        if from.is_empty() || to.is_empty() {
            continue;
        }
        let (f, t) = (part(&mut seq, from, None), part(&mut seq, to, None));
        seq.items.push(Item::Msg { from: f, to: t, text: text.trim().to_string(), dashed, head: h });
    }
    if seq.parts.is_empty() {
        None
    } else {
        Some(seq)
    }
}

const FONT: f32 = 13.0;
const ROW: f32 = 40.0;
const HEAD_H: f32 = 30.0;

fn text_size(painter: &Painter, text: &str) -> Vec2 {
    painter.layout_no_wrap(text.to_string(), FontId::proportional(FONT), Color32::BLACK).size()
}

/// Horizontal distance between two neighbouring lifelines.
fn spacing(painter: &Painter, s: &Sequence) -> f32 {
    let longest = s
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Msg { text, from, to, .. } if from != to => Some(text_size(painter, text).x),
            _ => None,
        })
        .fold(0.0_f32, f32::max);
    let labels = s.parts.iter().map(|p| text_size(painter, &p.1).x + 28.0).fold(0.0_f32, f32::max);
    (longest + 40.0).max(labels + 20.0).clamp(150.0, 360.0)
}

pub fn measure(painter: &Painter, d: &Diagram) -> Vec2 {
    match d {
        Diagram::Sequence(s) => {
            let sp = spacing(painter, s);
            let w = sp * (s.parts.len().saturating_sub(1)) as f32 + 160.0;
            Vec2::new(w, HEAD_H * 2.0 + 30.0 + ROW * s.items.len() as f32)
        }
        Diagram::Pie(p) => {
            let legend = p.slices.iter().map(|(l, _)| text_size(painter, l).x).fold(0.0_f32, f32::max) + 110.0;
            Vec2::new(260.0 + legend, 250.0 + if p.title.is_empty() { 0.0 } else { 30.0 })
        }
    }
}

const SLICES: [Color32; 8] = [
    Color32::from_rgb(79, 129, 189),
    Color32::from_rgb(214, 126, 59),
    Color32::from_rgb(98, 164, 106),
    Color32::from_rgb(180, 84, 112),
    Color32::from_rgb(138, 110, 190),
    Color32::from_rgb(196, 170, 66),
    Color32::from_rgb(74, 167, 178),
    Color32::from_rgb(130, 130, 130),
];

fn arrow_head(painter: &Painter, tip: Pos2, dir: Vec2, head: Head, color: Color32) {
    let n = Vec2::new(-dir.y, dir.x);
    match head {
        Head::Filled => {
            let base = tip - dir * 10.0;
            painter.add(EShape::convex_polygon(vec![tip, base + n * 4.5, base - n * 4.5], color, Stroke::NONE));
        }
        Head::Open => {
            let base = tip - dir * 9.0;
            painter.line_segment([tip, base + n * 5.0], Stroke::new(1.4, color));
        }
        Head::Cross => {
            let c = tip - dir * 5.0;
            painter.line_segment([c - dir * 4.0 - n * 4.0, c + dir * 4.0 + n * 4.0], Stroke::new(1.6, color));
            painter.line_segment([c - dir * 4.0 + n * 4.0, c + dir * 4.0 - n * 4.0], Stroke::new(1.6, color));
        }
        Head::None => {}
    }
}

fn dashed(painter: &Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    painter.extend(EShape::dashed_line(&[a, b], stroke, 6.0, 4.0));
}

pub fn draw(painter: &Painter, d: &Diagram, size: Vec2, origin: Pos2, colors: &Colors) {
    let Colors { ink, fill, line, soft, bg } = *colors;
    let font = FontId::proportional(FONT);
    match d {
        Diagram::Sequence(s) => {
            let sp = spacing(painter, s);
            let x_of = |i: usize| origin.x + 80.0 + sp * i as f32;
            let top = origin.y;
            let body_top = top + HEAD_H;
            let bottom = origin.y + size.y - HEAD_H;
            for (i, (_, label)) in s.parts.iter().enumerate() {
                let x = x_of(i);
                dashed(painter, Pos2::new(x, body_top), Pos2::new(x, bottom), Stroke::new(1.0, soft));
                for y in [top, bottom] {
                    let w = text_size(painter, label).x + 24.0;
                    let r = Rect::from_min_size(Pos2::new(x - w / 2.0, y), Vec2::new(w, HEAD_H));
                    painter.rect(r, 5.0, fill, Stroke::new(1.3, line), eframe::egui::StrokeKind::Inside);
                    painter.text(r.center(), Align2::CENTER_CENTER, label, font.clone(), ink);
                }
            }
            let mut y = body_top + ROW * 0.7;
            for item in &s.items {
                match item {
                    Item::Msg { from, to, text, dashed: dash, head } => {
                        let (x0, x1) = (x_of(*from), x_of(*to));
                        let stroke = Stroke::new(1.4, line);
                        if from == to {
                            // a message to itself loops back beside the lifeline
                            let pts = [
                                Pos2::new(x0, y - 8.0),
                                Pos2::new(x0 + 34.0, y - 8.0),
                                Pos2::new(x0 + 34.0, y + 8.0),
                                Pos2::new(x0 + 2.0, y + 8.0),
                            ];
                            for w in pts.windows(2) {
                                if *dash {
                                    dashed(painter, w[0], w[1], stroke);
                                } else {
                                    painter.line_segment([w[0], w[1]], stroke);
                                }
                            }
                            arrow_head(painter, pts[3], Vec2::new(-1.0, 0.0), *head, line);
                            painter.text(Pos2::new(x0 + 40.0, y), Align2::LEFT_CENTER, text, font.clone(), ink);
                        } else {
                            let dir = Vec2::new((x1 - x0).signum(), 0.0);
                            let (a, b) = (Pos2::new(x0, y), Pos2::new(x1, y));
                            if *dash {
                                dashed(painter, a, b, stroke);
                            } else {
                                painter.line_segment([a, b], stroke);
                            }
                            arrow_head(painter, b, dir, *head, line);
                            painter.text(
                                Pos2::new((x0 + x1) / 2.0, y - 6.0),
                                Align2::CENTER_BOTTOM,
                                text,
                                font.clone(),
                                ink,
                            );
                        }
                    }
                    Item::Note { a, b, text } => {
                        let w = (x_of(*b) - x_of(*a)).max(text_size(painter, text).x + 20.0).max(110.0);
                        let cx = (x_of(*a) + x_of(*b)) / 2.0;
                        let r = Rect::from_center_size(Pos2::new(cx, y), Vec2::new(w, 26.0));
                        painter.rect(r, 3.0, bg, Stroke::new(1.0, soft), eframe::egui::StrokeKind::Inside);
                        painter.text(r.center(), Align2::CENTER_CENTER, text, font.clone(), ink);
                    }
                    Item::Marker(t) => {
                        painter.text(
                            Pos2::new(origin.x + 6.0, y),
                            Align2::LEFT_CENTER,
                            t,
                            FontId::proportional(11.5),
                            soft,
                        );
                    }
                }
                y += ROW;
            }
        }
        Diagram::Pie(p) => {
            let title_h = if p.title.is_empty() { 0.0 } else { 30.0 };
            if !p.title.is_empty() {
                painter.text(
                    Pos2::new(origin.x + 130.0, origin.y + 12.0),
                    Align2::CENTER_CENTER,
                    &p.title,
                    FontId::proportional(15.0),
                    ink,
                );
            }
            let total: f64 = p.slices.iter().map(|s| s.1).sum();
            let c = Pos2::new(origin.x + 130.0, origin.y + title_h + 115.0);
            let r = 100.0;
            if total > 0.0 {
                let mut a0 = -std::f32::consts::FRAC_PI_2;
                for (i, (_, v)) in p.slices.iter().enumerate() {
                    let span = (*v / total) as f32 * std::f32::consts::TAU;
                    if span <= 0.0 {
                        continue;
                    }
                    let steps = ((span / 0.05).ceil() as usize).max(1);
                    let mut mesh = eframe::egui::epaint::Mesh::default();
                    for s in 0..steps {
                        let (x0, x1) = (a0 + span * s as f32 / steps as f32, a0 + span * (s + 1) as f32 / steps as f32);
                        let base = mesh.vertices.len() as u32;
                        for pt in [c, c + Vec2::angled(x0) * r, c + Vec2::angled(x1) * r] {
                            mesh.vertices.push(eframe::egui::epaint::Vertex {
                                pos: pt,
                                uv: eframe::egui::epaint::WHITE_UV,
                                color: SLICES[i % SLICES.len()],
                            });
                        }
                        mesh.indices.extend([base, base + 1, base + 2]);
                    }
                    painter.add(EShape::mesh(mesh));
                    a0 += span;
                }
                painter.circle_stroke(c, r, Stroke::new(1.5, fill));
            }
            let mut y = origin.y + title_h + 30.0;
            for (i, (label, v)) in p.slices.iter().enumerate() {
                let sw = Rect::from_min_size(Pos2::new(origin.x + 260.0, y - 6.0), Vec2::splat(12.0));
                painter.rect_filled(sw, 3.0, SLICES[i % SLICES.len()]);
                let pct = if total > 0.0 { v / total * 100.0 } else { 0.0 };
                let txt =
                    if p.show_data { format!("{label}  {v}  ({pct:.0}%)") } else { format!("{label}  ({pct:.0}%)") };
                painter.text(Pos2::new(origin.x + 280.0, y), Align2::LEFT_CENTER, txt, font.clone(), ink);
                y += 24.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_diagram_is_read_with_aliases_arrows_notes_and_blocks() {
        let src = "sequenceDiagram\n  participant A as Alice\n  actor B as Bob\n  A->>B: Hello\n  B-->>A: Hi\n  A-)B: async\n  B--xA: lost\n  Note over A,B: shared\n  loop every day\n  A->>A: think\n  end";
        let Some(Diagram::Sequence(s)) = parse(src) else { panic!("not a sequence") };
        assert_eq!(s.parts, vec![("A".into(), "Alice".into()), ("B".into(), "Bob".into())]);
        let heads: Vec<Head> =
            s.items.iter().filter_map(|i| if let Item::Msg { head, .. } = i { Some(*head) } else { None }).collect();
        assert_eq!(heads, vec![Head::Filled, Head::Filled, Head::Open, Head::Cross, Head::Filled]);
        assert!(matches!(s.items[1], Item::Msg { dashed: true, .. }));
        assert!(s.items.iter().any(|i| matches!(i, Item::Note { a: 0, b: 1, .. })));
        assert_eq!(s.items.iter().filter(|i| matches!(i, Item::Marker(_))).count(), 2);
        // participants that are never declared appear in order of first use
        let Some(Diagram::Sequence(s2)) = parse("sequenceDiagram\nX->>Y: hi") else { panic!() };
        assert_eq!(s2.parts.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(), vec!["X", "Y"]);
    }

    #[test]
    fn a_pie_chart_reads_title_slices_and_ignores_bad_lines() {
        let src = "pie showData title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85.5\n  \"Bad\" : x\n  nonsense";
        let Some(Diagram::Pie(p)) = parse(src) else { panic!("not a pie") };
        assert_eq!(p.title, "Pets");
        assert!(p.show_data);
        assert_eq!(p.slices, vec![("Dogs".into(), 386.0), ("Cats".into(), 85.5)]);
        assert!(parse("pie\n").is_none());
    }

    #[test]
    fn other_sources_are_not_taken_for_these_diagrams() {
        assert!(parse("graph TD\nA-->B").is_none());
        assert!(parse("").is_none());
        assert!(parse("gantt\ntitle x").is_none());
    }

    #[test]
    fn a_state_diagram_becomes_a_flowchart() {
        let src = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : start\n  Busy --> [*]";
        let flow = state_as_flowchart(src).unwrap();
        let g = mermaid::parse(&flow).expect("flowchart");
        assert!(g.edges.len() >= 3);
        assert!(g.nodes.iter().any(|n| n.label == "Idle"));
    }
}
