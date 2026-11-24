//! A small renderer for the LaTeX subset people use in notes: fractions, roots, sub and superscripts,
//! Greek letters, common operators and big operators with limits. Anything else is shown as typed.

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Stroke};

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Text(String),
    Space(f32),
    Group(Vec<Node>),
    Frac(Vec<Node>, Vec<Node>),
    Sqrt(Vec<Node>),
    Script { base: Box<Node>, sup: Option<Vec<Node>>, sub: Option<Vec<Node>> },
}

const SYMBOLS: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "φ"),
    ("varphi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("cdot", "·"),
    ("times", "×"),
    ("div", "÷"),
    ("pm", "±"),
    ("mp", "∓"),
    ("leq", "≤"),
    ("le", "≤"),
    ("geq", "≥"),
    ("ge", "≥"),
    ("neq", "≠"),
    ("ne", "≠"),
    ("approx", "≈"),
    ("equiv", "≡"),
    ("sim", "∼"),
    ("infty", "∞"),
    ("to", "→"),
    ("rightarrow", "→"),
    ("leftarrow", "←"),
    ("Rightarrow", "⇒"),
    ("Leftarrow", "⇐"),
    ("leftrightarrow", "↔"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("sum", "∑"),
    ("prod", "∏"),
    ("int", "∫"),
    ("oint", "∮"),
    ("in", "∈"),
    ("notin", "∉"),
    ("subset", "⊂"),
    ("cup", "∪"),
    ("cap", "∩"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("cdots", "⋯"),
    ("ldots", "…"),
    ("dots", "…"),
    ("degree", "°"),
    ("circ", "∘"),
    ("angle", "∠"),
    ("neg", "¬"),
    ("land", "∧"),
    ("lor", "∨"),
    ("emptyset", "∅"),
    ("hbar", "ℏ"),
    ("ell", "ℓ"),
    ("propto", "∝"),
    ("perp", "⊥"),
    ("parallel", "∥"),
];

const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "log", "ln", "exp", "lim", "max", "min", "det", "arcsin", "arccos", "arctan", "sinh", "cosh",
    "tanh",
];
const BIG: &[&str] = &["∑", "∏", "∫", "∮"];
const SPACED: &[&str] = &[
    "+", "−", "=", "≤", "≥", "≠", "≈", "≡", "±", "∓", "×", "÷", "→", "←", "⇒", "⇐", "↔", "∈", "∉", "⊂", "∼", "∝", "<",
    ">",
];

/// Longest formula read (characters) and deepest nesting of `{}`: bigger input is shown as typed.
const MAX_LEN: usize = 4000;
const MAX_DEPTH: usize = 40;

pub fn parse(src: &str) -> Vec<Node> {
    let chars: Vec<char> = src.chars().collect();
    if chars.len() > MAX_LEN {
        return vec![Node::Text(src.to_string())];
    }
    let mut p = Parser { c: &chars, i: 0, depth: 0 };
    p.sequence(false)
}

struct Parser<'a> {
    c: &'a [char],
    i: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    fn sequence(&mut self, in_group: bool) -> Vec<Node> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            if c == '}' {
                if in_group {
                    break;
                }
                self.i += 1; // stray brace
                continue;
            }
            if c.is_whitespace() {
                self.i += 1;
                continue;
            }
            if let Some(n) = self.atom_with_scripts() {
                out.push(n);
            }
        }
        out
    }

    fn atom_with_scripts(&mut self) -> Option<Node> {
        let mut base = self.atom()?;
        loop {
            self.skip_spaces();
            let (is_sup, is_sub) = (self.peek() == Some('^'), self.peek() == Some('_'));
            if !is_sup && !is_sub {
                break;
            }
            self.i += 1;
            self.skip_spaces();
            let arg = self.atom().map(|n| vec![n]).unwrap_or_default();
            base = match base {
                Node::Script { base: b, sup, sub } => Node::Script {
                    base: b,
                    sup: if is_sup { Some(arg.clone()) } else { sup },
                    sub: if is_sub { Some(arg) } else { sub },
                },
                other => {
                    Node::Script { base: Box::new(other), sup: is_sup.then(|| arg.clone()), sub: is_sub.then_some(arg) }
                }
            };
        }
        Some(base)
    }

    /// One argument: a `{group}` or a single token.
    fn atom(&mut self) -> Option<Node> {
        self.skip_spaces();
        let c = self.peek()?;
        match c {
            '{' if self.depth >= MAX_DEPTH => {
                self.i += 1;
                Some(Node::Text("{".into()))
            }
            '{' => {
                self.i += 1;
                self.depth += 1;
                let inner = self.sequence(true);
                self.depth -= 1;
                if self.peek() == Some('}') {
                    self.i += 1;
                }
                Some(Node::Group(inner))
            }
            '\\' => {
                self.i += 1;
                let mut name = String::new();
                while let Some(l) = self.peek().filter(|l| l.is_ascii_alphabetic()) {
                    name.push(l);
                    self.i += 1;
                }
                if name.is_empty() {
                    // an escaped character such as `\{`, `\,` or `\\`
                    let e = self.peek()?;
                    self.i += 1;
                    return Some(match e {
                        ',' | ';' | ':' | ' ' => Node::Space(0.25),
                        '!' => Node::Space(0.0),
                        other => Node::Text(other.to_string()),
                    });
                }
                Some(self.command(&name))
            }
            '-' => {
                self.i += 1;
                Some(Node::Text("−".into()))
            }
            '*' => {
                self.i += 1;
                Some(Node::Text("∗".into()))
            }
            '\'' => {
                self.i += 1;
                Some(Node::Text("′".into()))
            }
            c => {
                self.i += 1;
                Some(Node::Text(c.to_string()))
            }
        }
    }

    fn command(&mut self, name: &str) -> Node {
        match name {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.atom().map(|n| vec![n]).unwrap_or_default();
                let b = self.atom().map(|n| vec![n]).unwrap_or_default();
                Node::Frac(a, b)
            }
            "sqrt" => Node::Sqrt(self.atom().map(|n| vec![n]).unwrap_or_default()),
            "left" | "right" | "big" | "Big" | "bigg" | "displaystyle" => Node::Space(0.0),
            "quad" => Node::Space(1.0),
            "qquad" => Node::Space(2.0),
            "text" | "mathrm" | "textbf" | "mathbf" | "operatorname" => match self.atom() {
                Some(Node::Group(g)) => Node::Group(
                    g.into_iter()
                        .map(|n| match n {
                            Node::Text(t) if t.trim().is_empty() => Node::Space(0.25),
                            other => other,
                        })
                        .collect(),
                ),
                Some(n) => n,
                None => Node::Space(0.0),
            },
            f if FUNCTIONS.contains(&f) => Node::Text(f.to_string()),
            s => match SYMBOLS.iter().find(|(k, _)| *k == s) {
                Some((_, v)) => Node::Text((*v).to_string()),
                None => Node::Text(format!("\\{s}")),
            },
        }
    }
}

// ------------------------------------------------------------------ layout

enum Item {
    Text { x: f32, y: f32, s: String, size: f32 },
    Line { a: (f32, f32), b: (f32, f32), w: f32 },
}

/// A laid out piece: origin at the left of its baseline.
pub struct Lay {
    pub w: f32,
    asc: f32,
    desc: f32,
    items: Vec<Item>,
}

impl Lay {
    pub fn height(&self) -> f32 {
        self.asc + self.desc
    }
    fn empty() -> Lay {
        Lay { w: 0.0, asc: 0.0, desc: 0.0, items: Vec::new() }
    }
    fn place(&mut self, other: Lay, dx: f32, dy: f32) {
        self.asc = self.asc.max(other.asc - dy);
        self.desc = self.desc.max(other.desc + dy);
        self.w = self.w.max(dx + other.w);
        for it in other.items {
            self.items.push(match it {
                Item::Text { x, y, s, size } => Item::Text { x: x + dx, y: y + dy, s, size },
                Item::Line { a, b, w } => Item::Line { a: (a.0 + dx, a.1 + dy), b: (b.0 + dx, b.1 + dy), w },
            });
        }
    }
}

pub fn layout(painter: &Painter, nodes: &[Node], size: f32) -> Lay {
    let mut out = Lay::empty();
    let mut x = 0.0;
    let mut prev: Option<&Node> = None;
    for n in nodes {
        // a minus sign with nothing to subtract from is a sign, not an operator: no padding
        let unary = matches!(n, Node::Text(t) if t == "−")
            && match prev {
                None | Some(Node::Space(_)) => true,
                Some(Node::Text(p)) => p == "(" || p == "," || SPACED.contains(&p.as_str()),
                _ => false,
            };
        prev = Some(n);
        let l = if unary { text_padded(painter, "−", size, false) } else { node(painter, n, size) };
        let w = l.w;
        out.place(l, x, 0.0);
        x += w;
    }
    out.w = x;
    out
}

fn node(painter: &Painter, n: &Node, size: f32) -> Lay {
    match n {
        Node::Space(s) => Lay { w: s * size, ..Lay::empty() },
        Node::Group(g) => layout(painter, g, size),
        Node::Text(t) => text(painter, t, size),
        Node::Frac(a, b) => {
            let (top, bot) = (layout(painter, a, size * 0.86), layout(painter, b, size * 0.86));
            let w = top.w.max(bot.w) + size * 0.2;
            let bar = -0.28 * size;
            let gap = 0.14 * size;
            let (ty, by) = (bar - gap - top.desc, bar + gap + bot.asc);
            let (tw, bw) = (top.w, bot.w);
            let mut out = Lay::empty();
            out.place(top, (w - tw) / 2.0, ty);
            out.place(bot, (w - bw) / 2.0, by);
            out.items.push(Item::Line { a: (size * 0.05, bar), b: (w - size * 0.05, bar), w: 1.2 });
            out.w = w;
            out
        }
        Node::Sqrt(inner) => {
            let l = layout(painter, inner, size);
            let rw = size * 0.55;
            let top = -(l.asc + size * 0.18);
            let bot = l.desc + size * 0.05;
            let mid = (top + bot) * 0.55;
            let w = l.w;
            let mut out = Lay::empty();
            out.items.push(Item::Line { a: (0.0, mid), b: (rw * 0.35, bot), w: 1.2 });
            out.items.push(Item::Line { a: (rw * 0.35, bot), b: (rw * 0.75, top), w: 1.2 });
            out.items.push(Item::Line { a: (rw * 0.75, top), b: (rw + w + size * 0.1, top), w: 1.2 });
            out.asc = -top;
            out.desc = bot;
            out.w = rw + w + size * 0.1;
            out.place(l, rw, 0.0);
            out
        }
        Node::Script { base, sup, sub } => {
            let big = matches!(&**base, Node::Text(t) if BIG.contains(&t.as_str()));
            let b = if big { text(painter, base_text(base), size * 1.35) } else { node(painter, base, size) };
            let bw = b.w;
            let mut out = Lay::empty();
            out.place(b, 0.0, 0.0);
            let s = size * 0.7;
            let mut extra = 0.0_f32;
            if let Some(sp) = sup {
                let l = layout(painter, sp, s);
                extra = extra.max(l.w);
                out.place(l, bw + size * 0.04, -0.45 * size - if big { 0.25 * size } else { 0.0 });
            }
            if let Some(sb) = sub {
                let l = layout(painter, sb, s);
                extra = extra.max(l.w);
                out.place(l, bw + size * 0.04, 0.25 * size + if big { 0.3 * size } else { 0.0 });
            }
            out.w = bw + extra + size * 0.06;
            out
        }
    }
}

fn base_text(n: &Node) -> &str {
    match n {
        Node::Text(t) => t,
        _ => "",
    }
}

fn text(painter: &Painter, t: &str, size: f32) -> Lay {
    text_padded(painter, t, size, SPACED.contains(&t))
}

fn text_padded(painter: &Painter, t: &str, size: f32, spaced: bool) -> Lay {
    let galley = painter.layout_no_wrap(t.to_string(), FontId::proportional(size), Color32::WHITE);
    let (w, h) = (galley.size().x, galley.size().y);
    let pad = if spaced { size * 0.22 } else { 0.0 };
    let (asc, desc) = (h * 0.78, h * 0.22);
    Lay { w: w + 2.0 * pad, asc, desc, items: vec![Item::Text { x: pad, y: -asc, s: t.to_string(), size }] }
}

/// Paints `lay` with the left end of its baseline at `origin`.
pub fn draw(painter: &Painter, lay: &Lay, origin: Pos2, color: Color32) {
    for it in &lay.items {
        match it {
            Item::Text { x, y, s, size } => {
                painter.text(
                    Pos2::new(origin.x + x, origin.y + y),
                    Align2::LEFT_TOP,
                    s,
                    FontId::proportional(*size),
                    color,
                );
            }
            Item::Line { a, b, w } => {
                painter.line_segment(
                    [Pos2::new(origin.x + a.0, origin.y + a.1), Pos2::new(origin.x + b.0, origin.y + b.1)],
                    Stroke::new(*w, color),
                );
            }
        }
    }
}

/// Height above the baseline of a laid out piece.
pub fn ascent(lay: &Lay) -> f32 {
    lay.asc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fractions_scripts_and_roots() {
        let n = parse(r"\frac{a+b}{2}");
        assert!(matches!(&n[0], Node::Frac(a, b) if a.len() == 1 && b.len() == 1));
        let n = parse("x^2_i");
        assert!(matches!(&n[0], Node::Script { sup: Some(_), sub: Some(_), .. }));
        let n = parse(r"\sqrt{x}");
        assert!(matches!(&n[0], Node::Sqrt(_)));
        assert_eq!(parse(r"\alpha"), vec![Node::Text("α".into())]);
        assert_eq!(parse(r"\unknown"), vec![Node::Text("\\unknown".into())]);
    }

    #[test]
    fn sums_keep_their_limits() {
        let n = parse(r"\sum_{i=1}^{n} i");
        assert!(matches!(&n[0], Node::Script { base, sup: Some(_), sub: Some(_) } if **base == Node::Text("∑".into())));
        assert_eq!(n.len(), 2);
    }

    #[test]
    fn unbalanced_braces_do_not_panic() {
        let _ = parse(r"\frac{a}{");
        let _ = parse("}}{{x^");
        let _ = parse(r"\");
    }

    /// Small deterministic generator, so the robustness tests need no extra crate.
    fn rng(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    #[test]
    fn hostile_input_never_panics_or_overflows_the_stack() {
        // very deep nesting and very long input
        let deep = "{".repeat(200_000) + "x" + &"}".repeat(200_000);
        let _ = parse(&deep);
        let _ = parse(&r"\frac".repeat(50_000));
        let _ = parse(&"^".repeat(100_000));
        let _ = parse(&"x_".repeat(100_000));
        // random strings
        let alphabet: Vec<&str> =
            vec!["{", "}", "^", "_", "\\", "\\frac", "\\sqrt", "x", "1", " ", "é", "∑", "-", "(", ")"];
        let mut seed = 0x9E3779B97F4A7C15u64;
        for _ in 0..2000 {
            let n = (rng(&mut seed) % 40) as usize;
            let src: String = (0..n).map(|_| alphabet[(rng(&mut seed) % alphabet.len() as u64) as usize]).collect();
            let _ = parse(&src);
        }
    }
}
