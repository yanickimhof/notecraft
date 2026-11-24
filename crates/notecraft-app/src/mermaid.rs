//! A small Mermaid flowchart renderer (`graph TD` / `flowchart LR`): boxes, rounded boxes, diamonds,
//! circles, arrows, dotted and thick links, link labels. Other diagram types are left as code.

use eframe::egui::{Color32, FontId, Painter, Pos2, Rect, Shape as EShape, Stroke, Vec2};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Down,
    Up,
    Right,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rect,
    Round,
    Stadium,
    Diamond,
    Circle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Solid,
    Dotted,
    Thick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub line: Line,
    pub arrow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    pub dir: Dir,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

// ------------------------------------------------------------------ reading

/// Bigger diagrams are left as code: drawing them every frame would slow the page down.
const MAX_SRC: usize = 20_000;
const MAX_NODES: usize = 150;
const MAX_EDGES: usize = 400;

pub fn parse(src: &str) -> Option<Graph> {
    if src.len() > MAX_SRC {
        return None;
    }
    let mut lines = src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let head = lines.next()?;
    let mut words = head.split_whitespace();
    let kind = words.next()?;
    if kind != "graph" && kind != "flowchart" {
        return None;
    }
    let dir = match words.next().unwrap_or("TD").to_uppercase().as_str() {
        "LR" => Dir::Right,
        "RL" => Dir::Left,
        "BT" => Dir::Up,
        _ => Dir::Down,
    };
    let mut g = Graph { dir, nodes: Vec::new(), edges: Vec::new() };
    let mut index: HashMap<String, usize> = HashMap::new();
    for line in lines {
        for stmt in line.split(';') {
            let stmt = stmt.trim();
            let first = stmt.split_whitespace().next().unwrap_or("");
            if stmt.is_empty()
                || matches!(
                    first,
                    "subgraph" | "end" | "style" | "classDef" | "class" | "click" | "linkStyle" | "direction"
                )
            {
                continue;
            }
            statement(stmt, &mut g, &mut index);
            if g.nodes.len() > MAX_NODES || g.edges.len() > MAX_EDGES {
                return None;
            }
        }
    }
    if g.nodes.is_empty() {
        None
    } else {
        Some(g)
    }
}

struct Scan {
    c: Vec<char>,
    i: usize,
}

impl Scan {
    fn skip_ws(&mut self) {
        while self.c.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    fn rest(&self) -> String {
        self.c[self.i.min(self.c.len())..].iter().collect()
    }
}

fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn clean_label(s: &str) -> String {
    let t = s.trim().trim_matches(|c| "([/\\)]{}".contains(c)).trim();
    let t = t.trim_matches('"').trim_matches('`');
    t.replace("<br/>", " ").replace("<br>", " ").replace("&quot;", "\"").trim().to_string()
}

/// Reads `id`, optionally followed by a bracketed label; returns (id, label, shape).
fn node_at(s: &mut Scan) -> Option<(String, Option<String>, Shape)> {
    s.skip_ws();
    let start = s.i;
    while s.peek().is_some_and(is_id_char) {
        s.i += 1;
    }
    if s.i == start {
        return None;
    }
    let id: String = s.c[start..s.i].iter().collect();
    let open = match s.peek() {
        Some(c @ ('[' | '(' | '{')) => c,
        _ => return Some((id, None, Shape::Rect)),
    };
    let close = match open {
        '[' => ']',
        '(' => ')',
        _ => '}',
    };
    let next = s.c.get(s.i + 1).copied();
    let shape = match (open, next) {
        ('(', Some('(')) => Shape::Circle,
        ('(', Some('[')) => Shape::Stadium,
        ('(', _) => Shape::Round,
        ('{', _) => Shape::Diamond,
        _ => Shape::Rect,
    };
    // find the matching close bracket, ignoring brackets inside quotes
    let mut depth = 0i32;
    let mut quoted = false;
    let mut j = s.i;
    while j < s.c.len() {
        let c = s.c[j];
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
        j += 1;
    }
    if j >= s.c.len() {
        // unclosed bracket: keep the bare id and stop reading this statement
        return Some((id, None, Shape::Rect));
    }
    let mut end = j;
    // swallow the doubled closers of `((x))`, `([x])`, `[[x]]`, `{{x}}`
    while s.c.get(end + 1).is_some_and(|c| ")]}".contains(*c)) {
        end += 1;
    }
    let inner: String = s.c[s.i + 1..j].iter().collect();
    s.i = end + 1;
    Some((id, Some(clean_label(&inner)), shape))
}

/// Reads a link such as `-->`, `-.->`, `==>`, `--- `, `-- text -->`, with an optional `|text|`.
fn edge_at(s: &mut Scan) -> Option<(Line, bool, String)> {
    s.skip_ws();
    let c = s.peek()?;
    if !matches!(c, '-' | '=' | '.' | '<') {
        return None;
    }
    let start = s.i;
    while s.peek().is_some_and(|c| "-=.<>".contains(c)) {
        s.i += 1;
    }
    let mut tok: String = s.c[start..s.i].iter().collect();
    let mut label = String::new();
    // `-- text -->`: an opener with no arrow, then the text, then the real link
    if matches!(tok.as_str(), "--" | "==" | "-.") && s.peek() == Some(' ') {
        let rest = s.rest();
        let closers = ["-->", "---", "==>", "===", ".->", "-.-"];
        if let Some(at) = closers.iter().filter_map(|cl| rest.find(cl)).min() {
            label = rest[..at].trim().to_string();
            let consumed = rest[..at].chars().count();
            s.i += consumed;
            let st = s.i;
            while s.peek().is_some_and(|c| "-=.>".contains(c)) {
                s.i += 1;
            }
            tok = s.c[st..s.i].iter().collect();
        }
    }
    if tok.len() < 2 && !tok.contains('>') {
        return None;
    }
    let line = if tok.contains('.') {
        Line::Dotted
    } else if tok.contains('=') {
        Line::Thick
    } else {
        Line::Solid
    };
    let arrow = tok.contains('>');
    // `-->|text|`
    s.skip_ws();
    if s.peek() == Some('|') {
        let st = s.i + 1;
        let mut j = st;
        while j < s.c.len() && s.c[j] != '|' {
            j += 1;
        }
        if j < s.c.len() {
            label = clean_label(&s.c[st..j].iter().collect::<String>());
            s.i = j + 1;
        }
    }
    Some((line, arrow, label))
}

fn statement(stmt: &str, g: &mut Graph, index: &mut HashMap<String, usize>) {
    let mut s = Scan { c: stmt.chars().collect(), i: 0 };
    let mut register = |g: &mut Graph, id: String, label: Option<String>, shape: Shape| -> usize {
        if let Some(&k) = index.get(&id) {
            if let Some(l) = label {
                g.nodes[k].label = l;
                g.nodes[k].shape = shape;
            }
            k
        } else {
            g.nodes.push(Node { label: label.clone().unwrap_or_else(|| id.clone()), id: id.clone(), shape });
            index.insert(id, g.nodes.len() - 1);
            g.nodes.len() - 1
        }
    };
    let Some((id, label, shape)) = node_at(&mut s) else { return };
    let mut prev = register(g, id, label, shape);
    while let Some((line, arrow, text)) = edge_at(&mut s) {
        let Some((id, label, shape)) = node_at(&mut s) else { break };
        let next = register(g, id, label, shape);
        g.edges.push(Edge { from: prev, to: next, label: text, line, arrow });
        prev = next;
    }
}

// ------------------------------------------------------------------ layout

pub struct Laid {
    galleys: Vec<Arc<eframe::egui::Galley>>,
    centers: Vec<Pos2>,
    sizes: Vec<Vec2>,
    edge_galleys: Vec<Option<Arc<eframe::egui::Galley>>>,
    pub size: Vec2,
}

const LAYER_GAP: f32 = 46.0;
const NODE_GAP: f32 = 34.0;
const MARGIN: f32 = 12.0;

fn layers_of(g: &Graph) -> Vec<usize> {
    let n = g.nodes.len();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &g.edges {
        if e.from != e.to {
            out[e.from].push(e.to);
        }
    }
    // drop back edges found by depth first search, then take the longest path
    let mut state = vec![0u8; n];
    let mut forward: Vec<(usize, usize)> = Vec::new();
    fn dfs(u: usize, out: &[Vec<usize>], state: &mut [u8], forward: &mut Vec<(usize, usize)>) {
        state[u] = 1;
        for &v in &out[u] {
            match state[v] {
                0 => {
                    forward.push((u, v));
                    dfs(v, out, state, forward);
                }
                2 => forward.push((u, v)),
                _ => {} // back edge
            }
        }
        state[u] = 2;
    }
    let mut indeg = vec![0usize; n];
    for e in &g.edges {
        if e.from != e.to {
            indeg[e.to] += 1;
        }
    }
    for u in (0..n).filter(|u| indeg[*u] == 0).chain(0..n) {
        if state[u] == 0 {
            dfs(u, &out, &mut state, &mut forward);
        }
    }
    let mut layer = vec![0usize; n];
    // relax in rounds (the forward edges form a DAG, so n rounds are enough)
    for _ in 0..n {
        let mut changed = false;
        for &(u, v) in &forward {
            if layer[v] < layer[u] + 1 {
                layer[v] = layer[u] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    layer
}

pub fn layout(painter: &Painter, g: &Graph) -> Laid {
    let font = FontId::proportional(14.0);
    let mut galleys = Vec::new();
    let mut sizes = Vec::new();
    for n in &g.nodes {
        let gal = painter.layout(n.label.clone(), font.clone(), Color32::PLACEHOLDER, 190.0);
        let t = gal.size();
        let size = match n.shape {
            Shape::Diamond => Vec2::new(t.x * 1.7 + 28.0, t.y * 1.9 + 14.0),
            Shape::Circle => {
                let d = t.x.max(t.y) + 30.0;
                Vec2::new(d.max(t.x + 26.0), d.max(t.y + 26.0))
            }
            _ => Vec2::new(t.x + 30.0, t.y + 20.0),
        };
        galleys.push(gal);
        sizes.push(size);
    }
    let layer = layers_of(g);
    let depth = layer.iter().copied().max().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); depth];
    for (i, l) in layer.iter().enumerate() {
        rows[*l].push(i);
    }
    // order inside each layer: barycentre sweeps
    let mut pos_in = vec![0.0f32; g.nodes.len()];
    for row in &rows {
        for (k, &i) in row.iter().enumerate() {
            pos_in[i] = k as f32;
        }
    }
    for sweep in 0..6 {
        let down = sweep % 2 == 0;
        let order: Vec<usize> = if down { (1..depth).collect() } else { (0..depth.saturating_sub(1)).rev().collect() };
        for l in order {
            let mut keyed: Vec<(f32, usize)> = rows[l]
                .iter()
                .map(|&i| {
                    let nb: Vec<f32> = g
                        .edges
                        .iter()
                        .filter_map(|e| {
                            if down && e.to == i && layer[e.from] < l {
                                Some(pos_in[e.from])
                            } else if !down && e.from == i && layer[e.to] > l {
                                Some(pos_in[e.to])
                            } else {
                                None
                            }
                        })
                        .collect();
                    let b = if nb.is_empty() { pos_in[i] } else { nb.iter().sum::<f32>() / nb.len() as f32 };
                    (b, i)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            rows[l] = keyed.into_iter().map(|(_, i)| i).collect();
            for (k, &i) in rows[l].iter().enumerate() {
                pos_in[i] = k as f32;
            }
        }
    }
    let horizontal = matches!(g.dir, Dir::Right | Dir::Left);
    // along = the flow direction, across = the other one
    let along = |s: Vec2| if horizontal { s.x } else { s.y };
    let across = |s: Vec2| if horizontal { s.y } else { s.x };
    let across_total: Vec<f32> = rows
        .iter()
        .map(|r| r.iter().map(|&i| across(sizes[i])).sum::<f32>() + NODE_GAP * r.len().saturating_sub(1) as f32)
        .collect();
    let widest = across_total.iter().copied().fold(0.0, f32::max);
    let mut centers = vec![Pos2::ZERO; g.nodes.len()];
    let mut a = MARGIN;
    let mut total_along = 0.0;
    for (l, row) in rows.iter().enumerate() {
        let thick = row.iter().map(|&i| along(sizes[i])).fold(0.0, f32::max);
        let mut c = MARGIN + (widest - across_total[l]) / 2.0;
        for &i in row {
            let w = across(sizes[i]);
            let (x, y) = (c + w / 2.0, a + thick / 2.0);
            centers[i] = if horizontal { Pos2::new(y, x) } else { Pos2::new(x, y) };
            c += w + NODE_GAP;
        }
        a += thick + LAYER_GAP;
        total_along = a - LAYER_GAP + MARGIN;
    }
    let mut size = if horizontal {
        Vec2::new(total_along, widest + 2.0 * MARGIN)
    } else {
        Vec2::new(widest + 2.0 * MARGIN, total_along)
    };
    size = size.max(Vec2::splat(MARGIN * 2.0));
    for c in &mut centers {
        match g.dir {
            Dir::Up => c.y = size.y - c.y,
            Dir::Left => c.x = size.x - c.x,
            _ => {}
        }
    }
    let small = FontId::proportional(12.0);
    let edge_galleys = g
        .edges
        .iter()
        .map(|e| {
            (!e.label.is_empty()).then(|| painter.layout(e.label.clone(), small.clone(), Color32::PLACEHOLDER, 150.0))
        })
        .collect();
    Laid { galleys, centers, sizes, edge_galleys, size }
}

// ------------------------------------------------------------------ drawing

fn border_point(c: Pos2, size: Vec2, shape: Shape, toward: Pos2) -> Pos2 {
    let d = toward - c;
    if d.length() < 0.01 {
        return c;
    }
    let (hw, hh) = (size.x / 2.0, size.y / 2.0);
    let t = match shape {
        Shape::Diamond => 1.0 / (d.x.abs() / hw + d.y.abs() / hh).max(1e-4),
        Shape::Circle => 1.0 / ((d.x / hw).powi(2) + (d.y / hh).powi(2)).sqrt().max(1e-4),
        _ => (hw / d.x.abs().max(1e-4)).min(hh / d.y.abs().max(1e-4)),
    };
    c + d * t.min(1.0)
}

/// Colours used to draw a diagram.
#[derive(Clone, Copy)]
pub struct Colors {
    pub ink: Color32,
    pub fill: Color32,
    pub line: Color32,
    pub soft: Color32,
    pub bg: Color32,
}

/// Paints the diagram with its top left corner at `origin`.
pub fn draw(painter: &Painter, g: &Graph, l: &Laid, origin: Pos2, colors: &Colors) {
    let Colors { ink, fill, line, soft, bg } = *colors;
    let at = |p: Pos2| origin + p.to_vec2();
    for e in &g.edges {
        let (a, b) = (l.centers[e.from], l.centers[e.to]);
        if e.from == e.to {
            continue;
        }
        let p0 = border_point(a, l.sizes[e.from], g.nodes[e.from].shape, b);
        let p1 = border_point(b, l.sizes[e.to], g.nodes[e.to].shape, a);
        let width = if e.line == Line::Thick { 2.6 } else { 1.4 };
        let stroke = Stroke::new(width, line);
        let (mut p0, mut p1) = (at(p0), at(p1));
        // two links in opposite directions are drawn side by side instead of on top of each other
        if g.edges.iter().any(|o| o.from == e.to && o.to == e.from) {
            let d = (p1 - p0).normalized();
            let off = Vec2::new(-d.y, d.x) * 5.0;
            p0 += off;
            p1 += off;
        }
        if e.line == Line::Dotted {
            for s in EShape::dashed_line(&[p0, p1], stroke, 5.0, 4.0) {
                painter.add(s);
            }
        } else {
            painter.line_segment([p0, p1], stroke);
        }
        if e.arrow {
            let d = (p1 - p0).normalized();
            let n = Vec2::new(-d.y, d.x);
            let tip = p1;
            let base = tip - d * 9.0;
            painter.add(EShape::convex_polygon(vec![tip, base + n * 4.5, base - n * 4.5], line, Stroke::NONE));
        }
    }
    for (k, n) in g.nodes.iter().enumerate() {
        let (c, s) = (at(l.centers[k]), l.sizes[k]);
        let rect = Rect::from_center_size(c, s);
        let stroke = Stroke::new(1.4, line);
        match n.shape {
            Shape::Rect => {
                painter.rect_filled(rect, 4.0, fill);
                painter.rect_stroke(rect, 4.0, stroke, eframe::egui::StrokeKind::Inside);
            }
            Shape::Round => {
                painter.rect_filled(rect, 12.0, fill);
                painter.rect_stroke(rect, 12.0, stroke, eframe::egui::StrokeKind::Inside);
            }
            Shape::Stadium => {
                let r = s.y / 2.0;
                painter.rect_filled(rect, r, fill);
                painter.rect_stroke(rect, r, stroke, eframe::egui::StrokeKind::Inside);
            }
            Shape::Circle => {
                painter.add(EShape::ellipse_filled(c, s / 2.0, fill));
                painter.add(EShape::ellipse_stroke(c, s / 2.0, stroke));
            }
            Shape::Diamond => {
                let pts = vec![
                    c + Vec2::new(0.0, -s.y / 2.0),
                    c + Vec2::new(s.x / 2.0, 0.0),
                    c + Vec2::new(0.0, s.y / 2.0),
                    c + Vec2::new(-s.x / 2.0, 0.0),
                ];
                painter.add(EShape::convex_polygon(pts.clone(), fill, stroke));
            }
        }
        let gal = &l.galleys[k];
        painter.galley(c - gal.size() / 2.0, gal.clone(), ink);
    }
    for (k, e) in g.edges.iter().enumerate() {
        let Some(gal) = &l.edge_galleys[k] else { continue };
        let mid = at(l.centers[e.from]).lerp(at(l.centers[e.to]), 0.5);
        let r = Rect::from_center_size(mid, gal.size() + Vec2::new(8.0, 4.0));
        painter.rect_filled(r, 3.0, bg);
        painter.galley(mid - gal.size() / 2.0, gal.clone(), soft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nodes_edges_and_labels() {
        let g = parse("graph TD\n  A[Start] --> B{Choice}\n  B -->|yes| C(Done)\n  B -- no --> D((End))").unwrap();
        assert_eq!(g.dir, Dir::Down);
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.nodes[1].shape, Shape::Diamond);
        assert_eq!(g.nodes[1].label, "Choice");
        assert_eq!(g.nodes[3].shape, Shape::Circle);
        assert_eq!(g.nodes[3].label, "End");
        assert_eq!(g.edges.len(), 3);
        assert_eq!(g.edges[1].label, "yes");
        assert_eq!(g.edges[2].label, "no");
        assert!(g.edges.iter().all(|e| e.arrow));
    }

    #[test]
    fn chains_and_link_styles() {
        let g = parse("flowchart LR\nA --> B --- C; C -.-> D\nD ==> A").unwrap();
        assert_eq!(g.dir, Dir::Right);
        assert_eq!(g.edges.len(), 4);
        assert!(!g.edges[1].arrow);
        assert_eq!(g.edges[2].line, Line::Dotted);
        assert_eq!(g.edges[3].line, Line::Thick);
        assert_eq!(g.nodes.len(), 4);
    }

    #[test]
    fn other_diagrams_and_junk_are_left_alone() {
        assert!(parse("sequenceDiagram\nA->>B: hi").is_none());
        assert!(parse("graph TD").is_none());
        assert!(parse("").is_none());
        let g = parse("graph TD\nA[unclosed --> B\nsubgraph x\nend\nstyle A fill:#f9f").unwrap();
        assert!(!g.nodes.is_empty());
    }

    #[test]
    fn cycles_still_get_layers() {
        let g = parse("graph TD\nA --> B\nB --> C\nC --> A").unwrap();
        let l = layers_of(&g);
        assert_eq!(l, vec![0, 1, 2]);
    }

    fn rng(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    #[test]
    fn hostile_input_never_panics_and_big_graphs_are_refused() {
        let chain: String = (0..5000).map(|i| format!("N{i} --> N{}\n", i + 1)).collect();
        assert!(parse(&format!("graph TD\n{chain}")).is_none());
        let ok: String = (0..100).map(|i| format!("N{i} --> N{}\n", i + 1)).collect();
        let g = parse(&format!("graph LR\n{ok}")).unwrap();
        assert_eq!(layers_of(&g).iter().max(), Some(&100));
        let _ = parse(&format!("graph TD\nA{}", "[".repeat(100_000)));
        let _ = parse(&format!("graph TD\nA -- {} --> B", "x ".repeat(50_000)));
        let parts = [
            "A", "B", "[", "]", "(", ")", "{", "}", "-->", "---", "-.->", "==>", "--", "|", "x", " ", "\n", ";", "\"",
            "é", "&",
        ];
        let mut seed = 0xA0761D6478BD642Fu64;
        for _ in 0..3000 {
            let n = (rng(&mut seed) % 30) as usize;
            let body: String = (0..n).map(|_| parts[(rng(&mut seed) % parts.len() as u64) as usize]).collect();
            let _ = parse(&format!("graph TD\n{body}"));
        }
    }
}
