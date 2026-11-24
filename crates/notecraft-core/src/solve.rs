//! Step by step solver for equations of the first and second degree in one unknown.
//!
//! It reads the LaTeX subset the equation block draws (`\frac`, `^`, `\cdot`, parentheses, implicit
//! products) and returns the working as short labelled LaTeX lines. Anything it cannot do exactly
//! (other functions, several unknowns, degree above two) is reported as [`SolveError::Unsupported`],
//! so the caller can hand the equation to the AI instead.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveError {
    /// No `=` sign, or no unknown to solve for.
    NotAnEquation,
    /// Valid, but outside what the local solver does exactly.
    Unsupported,
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolveError::NotAnEquation => write!(f, "not an equation with one unknown"),
            SolveError::Unsupported => write!(f, "not supported by the local solver"),
        }
    }
}

type Res<T> = Result<T, SolveError>;

/// One line of working: what is done, and the LaTeX to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    pub latex: String,
}

// ------------------------------------------------------------------ rationals

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rat {
    n: i128,
    d: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Rat {
    const ZERO: Rat = Rat { n: 0, d: 1 };
    const ONE: Rat = Rat { n: 1, d: 1 };

    fn new(n: i128, d: i128) -> Res<Rat> {
        if d == 0 {
            return Err(SolveError::Unsupported);
        }
        let g = gcd(n, d).max(1);
        let s = if d < 0 { -1 } else { 1 };
        Ok(Rat { n: s * n / g, d: s * d / g })
    }
    fn int(n: i128) -> Rat {
        Rat { n, d: 1 }
    }
    fn is_zero(self) -> bool {
        self.n == 0
    }
    fn is_neg(self) -> bool {
        self.n < 0
    }
    fn is_int(self) -> bool {
        self.d == 1
    }
    fn neg(self) -> Rat {
        Rat { n: -self.n, d: self.d }
    }
    fn abs(self) -> Rat {
        Rat { n: self.n.abs(), d: self.d }
    }
    fn add(self, o: Rat) -> Res<Rat> {
        let n = self.n.checked_mul(o.d).and_then(|a| o.n.checked_mul(self.d).and_then(|b| a.checked_add(b)));
        let d = self.d.checked_mul(o.d);
        Rat::new(n.ok_or(SolveError::Unsupported)?, d.ok_or(SolveError::Unsupported)?)
    }
    fn sub(self, o: Rat) -> Res<Rat> {
        self.add(o.neg())
    }
    fn mul(self, o: Rat) -> Res<Rat> {
        let n = self.n.checked_mul(o.n).ok_or(SolveError::Unsupported)?;
        let d = self.d.checked_mul(o.d).ok_or(SolveError::Unsupported)?;
        Rat::new(n, d)
    }
    fn div(self, o: Rat) -> Res<Rat> {
        if o.is_zero() {
            return Err(SolveError::Unsupported);
        }
        self.mul(Rat { n: o.d, d: o.n }.normalized()?)
    }
    fn normalized(self) -> Res<Rat> {
        Rat::new(self.n, self.d)
    }
    fn to_f64(self) -> f64 {
        self.n as f64 / self.d as f64
    }
    fn latex(self) -> String {
        if self.is_int() {
            self.n.to_string()
        } else if self.n < 0 {
            format!("-\\frac{{{}}}{{{}}}", -self.n, self.d)
        } else {
            format!("\\frac{{{}}}{{{}}}", self.n, self.d)
        }
    }
    /// The number, in brackets when it is negative (for use inside a product or a power).
    fn paren(self) -> String {
        if self.is_neg() {
            format!("({})", self.latex())
        } else {
            self.latex()
        }
    }
}

fn decimal(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

// ------------------------------------------------------------------ polynomials

#[derive(Debug, Clone, PartialEq, Eq)]
struct Poly(Vec<Rat>);

impl Poly {
    fn constant(r: Rat) -> Poly {
        Poly(vec![r]).trimmed()
    }
    fn var() -> Poly {
        Poly(vec![Rat::ZERO, Rat::ONE])
    }
    fn trimmed(mut self) -> Poly {
        while self.0.last().is_some_and(|c| c.is_zero()) {
            self.0.pop();
        }
        self
    }
    fn degree(&self) -> usize {
        self.0.len().saturating_sub(1)
    }
    fn coef(&self, k: usize) -> Rat {
        self.0.get(k).copied().unwrap_or(Rat::ZERO)
    }
    fn as_const(&self) -> Option<Rat> {
        if self.0.len() <= 1 {
            Some(self.coef(0))
        } else {
            None
        }
    }
    fn add(&self, o: &Poly) -> Res<Poly> {
        let n = self.0.len().max(o.0.len());
        let mut v = Vec::with_capacity(n);
        for k in 0..n {
            v.push(self.coef(k).add(o.coef(k))?);
        }
        Ok(Poly(v).trimmed())
    }
    fn neg(&self) -> Poly {
        Poly(self.0.iter().map(|c| c.neg()).collect())
    }
    fn sub(&self, o: &Poly) -> Res<Poly> {
        self.add(&o.neg())
    }
    fn mul(&self, o: &Poly) -> Res<Poly> {
        if self.0.is_empty() || o.0.is_empty() {
            return Ok(Poly(vec![]));
        }
        let mut v = vec![Rat::ZERO; self.0.len() + o.0.len() - 1];
        for (i, a) in self.0.iter().enumerate() {
            for (j, b) in o.0.iter().enumerate() {
                v[i + j] = v[i + j].add(a.mul(*b)?)?;
            }
        }
        if v.len() > 9 {
            return Err(SolveError::Unsupported);
        }
        Ok(Poly(v).trimmed())
    }
    fn pow(&self, n: u32) -> Res<Poly> {
        let mut out = Poly::constant(Rat::ONE);
        for _ in 0..n {
            out = out.mul(self)?;
        }
        Ok(out)
    }
    fn div(&self, o: &Poly) -> Res<Poly> {
        let c = o.as_const().filter(|c| !c.is_zero()).ok_or(SolveError::Unsupported)?;
        let mut v = Vec::new();
        for a in &self.0 {
            v.push(a.div(c)?);
        }
        Ok(Poly(v).trimmed())
    }
    fn eval(&self, x: Rat) -> Res<Rat> {
        let mut acc = Rat::ZERO;
        for c in self.0.iter().rev() {
            acc = acc.mul(x)?.add(*c)?;
        }
        Ok(acc)
    }
    /// Highest power first: `2x^{2} - 3x + 1`.
    fn latex(&self, v: char) -> String {
        let mut out = String::new();
        for k in (0..=self.degree()).rev() {
            let c = self.coef(k);
            if c.is_zero() {
                continue;
            }
            let neg = c.is_neg();
            let mag = c.abs();
            let body = match k {
                0 => mag.latex(),
                _ => {
                    let coef = if mag == Rat::ONE { String::new() } else { mag.latex() };
                    let pow = if k == 1 { String::new() } else { format!("^{{{k}}}") };
                    format!("{coef}{v}{pow}")
                }
            };
            if out.is_empty() {
                out.push_str(&if neg { format!("-{body}") } else { body });
            } else {
                out.push_str(if neg { " - " } else { " + " });
                out.push_str(&body);
            }
        }
        if out.is_empty() {
            "0".into()
        } else {
            out
        }
    }
}

// ------------------------------------------------------------------ reading

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(Rat),
    Var(char),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Frac,
    Eq,
}

fn tokenize(src: &str) -> Res<Vec<Tok>> {
    let c: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        match ch {
            ' ' | '\t' | '\n' | '$' => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                    i += 1;
                }
                let s: String = c[start..i].iter().collect();
                let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
                if frac.len() > 12 || s.matches('.').count() > 1 {
                    return Err(SolveError::Unsupported);
                }
                let digits: String = format!("{int}{frac}");
                let n: i128 = digits.parse().map_err(|_| SolveError::Unsupported)?;
                out.push(Tok::Num(Rat::new(n, 10_i128.pow(frac.len() as u32))?));
            }
            'a'..='z' | 'A'..='Z' => {
                out.push(Tok::Var(ch));
                i += 1;
            }
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' | '−' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' | '·' | '×' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' | '÷' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '^' => {
                out.push(Tok::Caret);
                i += 1;
            }
            '(' | '[' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' | ']' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '{' => {
                out.push(Tok::LBrace);
                i += 1;
            }
            '}' => {
                out.push(Tok::RBrace);
                i += 1;
            }
            '=' => {
                out.push(Tok::Eq);
                i += 1;
            }
            '\\' => {
                i += 1;
                let start = i;
                while i < c.len() && c[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let name: String = c[start..i].iter().collect();
                match name.as_str() {
                    "frac" | "dfrac" | "tfrac" => out.push(Tok::Frac),
                    "cdot" | "times" => out.push(Tok::Star),
                    "div" => out.push(Tok::Slash),
                    "left" | "right" | "displaystyle" | "quad" | "qquad" => {}
                    "" => i += 1, // `\,` and friends
                    _ => return Err(SolveError::Unsupported),
                }
            }
            _ => return Err(SolveError::Unsupported),
        }
    }
    Ok(out)
}

/// Longest equation read (characters) and deepest nesting of brackets and signs.
const MAX_LEN: usize = 400;
const MAX_DEPTH: usize = 48;

struct Reader<'a> {
    t: &'a [Tok],
    i: usize,
    var: Option<char>,
    depth: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &Tok) -> Res<()> {
        if self.eat(t) {
            Ok(())
        } else {
            Err(SolveError::Unsupported)
        }
    }
    fn starts_atom(&self) -> bool {
        matches!(self.peek(), Some(Tok::Num(_) | Tok::Var(_) | Tok::LParen | Tok::LBrace | Tok::Frac))
    }

    fn expr(&mut self) -> Res<Poly> {
        let mut acc = self.term()?;
        loop {
            if self.eat(&Tok::Plus) {
                acc = acc.add(&self.term()?)?;
            } else if self.eat(&Tok::Minus) {
                acc = acc.sub(&self.term()?)?;
            } else {
                return Ok(acc);
            }
        }
    }

    fn term(&mut self) -> Res<Poly> {
        let mut acc = self.unary()?;
        loop {
            if self.eat(&Tok::Star) {
                acc = acc.mul(&self.unary()?)?;
            } else if self.eat(&Tok::Slash) {
                acc = acc.div(&self.unary()?)?;
            } else if self.starts_atom() {
                acc = acc.mul(&self.unary()?)?;
            } else {
                return Ok(acc);
            }
        }
    }

    fn unary(&mut self) -> Res<Poly> {
        self.depth += 1;
        let r = self.unary_inner();
        self.depth -= 1;
        r
    }

    fn unary_inner(&mut self) -> Res<Poly> {
        if self.depth > MAX_DEPTH {
            return Err(SolveError::Unsupported);
        }
        if self.eat(&Tok::Minus) {
            return Ok(self.unary()?.neg());
        }
        if self.eat(&Tok::Plus) {
            return self.unary();
        }
        let base = self.atom()?;
        if self.eat(&Tok::Caret) {
            let braced = self.eat(&Tok::LBrace);
            let neg = self.eat(&Tok::Minus);
            let n = match self.peek() {
                Some(Tok::Num(r)) if r.is_int() && !neg && r.n <= 8 => r.n as u32,
                _ => return Err(SolveError::Unsupported),
            };
            self.i += 1;
            if braced {
                self.expect(&Tok::RBrace)?;
            }
            return base.pow(n);
        }
        Ok(base)
    }

    fn atom(&mut self) -> Res<Poly> {
        match self.peek().cloned() {
            Some(Tok::Num(r)) => {
                self.i += 1;
                Ok(Poly::constant(r))
            }
            Some(Tok::Var(v)) => {
                self.i += 1;
                match self.var {
                    None => self.var = Some(v),
                    Some(w) if w != v => return Err(SolveError::Unsupported),
                    _ => {}
                }
                Ok(Poly::var())
            }
            Some(Tok::LParen) => {
                self.i += 1;
                let p = self.expr()?;
                self.expect(&Tok::RParen)?;
                Ok(p)
            }
            Some(Tok::LBrace) => {
                self.i += 1;
                let p = self.expr()?;
                self.expect(&Tok::RBrace)?;
                Ok(p)
            }
            Some(Tok::Frac) => {
                self.i += 1;
                self.expect(&Tok::LBrace)?;
                let a = self.expr()?;
                self.expect(&Tok::RBrace)?;
                self.expect(&Tok::LBrace)?;
                let b = self.expr()?;
                self.expect(&Tok::RBrace)?;
                a.div(&b)
            }
            _ => Err(SolveError::Unsupported),
        }
    }
}

// ------------------------------------------------------------------ solving

struct Words {
    fr: bool,
}

impl Words {
    fn t(&self, fr: &str, en: &str) -> String {
        if self.fr { fr } else { en }.to_string()
    }
}

fn step(label: String, latex: String) -> Step {
    Step { label, latex }
}

/// Solve `src` (an equation such as `x^2 - 5x + 6 = 0`), with the working in French when `fr`.
pub fn solve(src: &str, fr: bool) -> Result<Vec<Step>, SolveError> {
    if src.chars().count() > MAX_LEN {
        return Err(SolveError::Unsupported);
    }
    let toks = tokenize(src)?;
    let eq = toks.iter().position(|t| *t == Tok::Eq).ok_or(SolveError::NotAnEquation)?;
    if toks[eq + 1..].contains(&Tok::Eq) {
        return Err(SolveError::Unsupported);
    }
    let mut rd = Reader { t: &toks[..eq], i: 0, var: None, depth: 0 };
    let left = rd.expr()?;
    if rd.i != eq {
        return Err(SolveError::Unsupported);
    }
    let mut rr = Reader { t: &toks[eq + 1..], i: 0, var: rd.var, depth: 0 };
    let right = rr.expr()?;
    if rr.i != toks.len() - eq - 1 {
        return Err(SolveError::Unsupported);
    }
    let v = rr.var.ok_or(SolveError::NotAnEquation)?;
    let w = Words { fr };
    let p = left.sub(&right)?;
    let mut steps =
        vec![step(w.t("Équation de départ", "Starting equation"), src.trim().trim_matches('$').trim().to_string())];
    match p.degree() {
        0 => {
            let c = p.coef(0);
            steps.push(step(
                w.t("Tout passer à gauche et réduire", "Move everything to the left and simplify"),
                format!("{} = 0", c.latex()),
            ));
            let msg = if c.is_zero() {
                w.t("Toujours vrai : tout nombre est solution", "Always true: every number is a solution")
            } else {
                w.t("Jamais vrai : aucune solution", "Never true: there is no solution")
            };
            steps.push(step(msg, if c.is_zero() { format!("{v} \\in \\mathbb{{R}}") } else { "\\emptyset".into() }));
            Ok(steps)
        }
        1 => linear(&p, &left, &right, v, &w, steps),
        2 => quadratic(&p, &left, &right, v, &w, steps),
        _ => Err(SolveError::Unsupported),
    }
}

fn check_step(w: &Words, left: &Poly, right: &Poly, x: Rat, v: char) -> Res<Step> {
    let (l, r) = (left.eval(x)?, right.eval(x)?);
    Ok(step(
        w.t(
            &format!("Vérification : on remplace {v} par {}", x.paren()),
            &format!("Check: replace {v} with {}", x.paren()),
        ),
        format!("{} = {}", l.latex(), r.latex()),
    ))
}

fn linear(p: &Poly, left: &Poly, right: &Poly, v: char, w: &Words, mut steps: Vec<Step>) -> Res<Vec<Step>> {
    let (a, b) = (p.coef(1), p.coef(0));
    steps.push(step(
        w.t("Tout passer à gauche et réduire", "Move everything to the left and simplify"),
        format!("{} = 0", p.latex(v)),
    ));
    if !b.is_zero() {
        steps.push(step(
            w.t(&format!("Isoler le terme en {v}"), &format!("Isolate the {v} term")),
            format!("{} = {}", Poly(vec![Rat::ZERO, a]).trimmed().latex(v), b.neg().latex()),
        ));
    }
    let x = b.neg().div(a)?;
    if a != Rat::ONE {
        steps.push(step(
            w.t(&format!("Diviser les deux membres par {}", a.paren()), &format!("Divide both sides by {}", a.paren())),
            format!("{v} = \\frac{{{}}}{{{}}}", b.neg().latex(), a.latex()),
        ));
    }
    let approx = if x.is_int() { String::new() } else { format!(" \\approx {}", decimal(x.to_f64())) };
    steps.push(step(w.t("Solution", "Solution"), format!("{v} = {}{approx}", x.latex())));
    steps.push(check_step(w, left, right, x, v)?);
    Ok(steps)
}

fn int_sqrt(n: i128) -> Option<i128> {
    if n < 0 {
        return None;
    }
    let r = (n as f64).sqrt().round() as i128;
    (r.saturating_sub(1)..=r + 1).find(|k| k.checked_mul(*k) == Some(n))
}

fn rat_sqrt(r: Rat) -> Option<Rat> {
    Rat::new(int_sqrt(r.n)?, int_sqrt(r.d)?).ok()
}

/// `n = k^2 * m` with `m` free of square factors (for integers up to about 10^12).
fn split_square(n: i128) -> (i128, i128) {
    let (mut k, mut m) = (1, n);
    let mut f = 2;
    while f * f <= m && f <= 1_000_000 {
        while m % (f * f) == 0 {
            m /= f * f;
            k *= f;
        }
        f += 1;
    }
    (k, m)
}

fn quadratic(p: &Poly, left: &Poly, right: &Poly, v: char, w: &Words, mut steps: Vec<Step>) -> Res<Vec<Step>> {
    let (a, b, c) = (p.coef(2), p.coef(1), p.coef(0));
    steps.push(step(
        w.t("Forme standard ax² + bx + c = 0", "Standard form ax² + bx + c = 0"),
        format!("{} = 0", p.latex(v)),
    ));
    steps.push(step(
        w.t("Coefficients", "Coefficients"),
        format!("a = {},\\quad b = {},\\quad c = {}", a.latex(), b.latex(), c.latex()),
    ));
    let four = Rat::int(4);
    let disc = b.mul(b)?.sub(four.mul(a)?.mul(c)?)?;
    steps.push(step(
        w.t("Discriminant Δ = b² − 4ac", "Discriminant Δ = b² − 4ac"),
        format!("\\Delta = {}^{{2}} - 4 \\cdot {} \\cdot {} = {}", b.paren(), a.paren(), c.paren(), disc.latex()),
    ));
    let two_a = a.mul(Rat::int(2))?;
    let sign = disc.n.signum();
    if sign == 0 {
        let x = b.neg().div(two_a)?;
        steps.push(step(
            w.t("Δ = 0 : une solution double", "Δ = 0: one double solution"),
            format!("{v} = \\frac{{-b}}{{2a}} = \\frac{{{}}}{{{}}} = {}", b.neg().latex(), two_a.latex(), x.latex()),
        ));
        steps.push(check_step(w, left, right, x, v)?);
    } else if sign > 0 {
        steps.push(step(
            w.t("Δ > 0 : deux solutions réelles", "Δ > 0: two real solutions"),
            format!(
                "{v} = \\frac{{-b \\pm \\sqrt{{\\Delta}}}}{{2a}} = \\frac{{{} \\pm \\sqrt{{{}}}}}{{{}}}",
                b.neg().latex(),
                disc.latex(),
                two_a.latex()
            ),
        ));
        if let Some(s) = rat_sqrt(disc) {
            steps.push(step(
                w.t(&format!("√Δ = {}", s.latex()), &format!("√Δ = {}", s.latex())),
                format!(
                    "{v}_1 = \\frac{{{} - {}}}{{{}}},\\quad {v}_2 = \\frac{{{} + {}}}{{{}}}",
                    b.neg().latex(),
                    s.latex(),
                    two_a.latex(),
                    b.neg().latex(),
                    s.latex(),
                    two_a.latex()
                ),
            ));
            let x1 = b.neg().sub(s)?.div(two_a)?;
            let x2 = b.neg().add(s)?.div(two_a)?;
            steps.push(step(
                w.t("Solutions", "Solutions"),
                format!("{v}_1 = {},\\quad {v}_2 = {}", x1.latex(), x2.latex()),
            ));
            steps.push(check_step(w, left, right, x1, v)?);
            steps.push(check_step(w, left, right, x2, v)?);
        } else {
            let root = disc.to_f64().sqrt();
            let (x1, x2) = ((-b.to_f64() - root) / two_a.to_f64(), (-b.to_f64() + root) / two_a.to_f64());
            let exact = if disc.is_int() && a.is_int() && b.is_int() {
                let (k, m) = split_square(disc.n);
                let g = gcd(gcd(b.n, k), two_a.n);
                let (bn, kk, d) = (-b.n / g, k / g, two_a.n / g);
                let (bn, kk, d) = if d < 0 { (-bn, kk, -d) } else { (bn, kk, d) };
                let root_tex = if kk == 1 { format!("\\sqrt{{{m}}}") } else { format!("{kk}\\sqrt{{{m}}}") };
                let den = |s: &str| if d == 1 { s.to_string() } else { format!("\\frac{{{s}}}{{{d}}}") };
                let first = if bn == 0 { String::new() } else { bn.to_string() };
                if bn == 0 {
                    Some((format!("-{}", den(&root_tex)), den(&root_tex)))
                } else {
                    Some((den(&format!("{first} - {root_tex}")), den(&format!("{first} + {root_tex}"))))
                }
            } else {
                None
            };
            let line = match exact {
                Some((m1, p1)) => {
                    format!("{v}_1 = {m1} \\approx {},\\quad {v}_2 = {p1} \\approx {}", decimal(x1), decimal(x2))
                }
                None => format!("{v}_1 \\approx {},\\quad {v}_2 \\approx {}", decimal(x1), decimal(x2)),
            };
            steps.push(step(w.t("Solutions (√Δ n'est pas un entier)", "Solutions (√Δ is not an integer)"), line));
        }
    } else {
        let im = disc.abs();
        steps.push(step(w.t("Δ < 0 : aucune solution réelle", "Δ < 0: no real solution"), "\\emptyset".into()));
        let tex = match rat_sqrt(im) {
            Some(s) => {
                let re = b.neg().div(two_a)?;
                let k = s.div(two_a.abs())?;
                let imag = if k == Rat::ONE { "i".to_string() } else { format!("{}i", k.latex()) };
                if re.is_zero() {
                    format!("{v} = \\pm {imag}")
                } else {
                    format!("{v} = {} \\pm {imag}", re.latex())
                }
            }
            None => format!("{v} = \\frac{{{} \\pm i\\sqrt{{{}}}}}{{{}}}", b.neg().latex(), im.latex(), two_a.latex()),
        };
        steps.push(step(w.t("Dans les nombres complexes", "In the complex numbers"), tex));
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn last(src: &str) -> String {
        solve(src, false).unwrap().pop().unwrap().latex
    }
    fn answer(src: &str) -> String {
        let s = solve(src, false).unwrap();
        s.iter()
            .find(|x| x.label.starts_with("Solution"))
            .map(|x| x.latex.clone())
            .unwrap_or_else(|| s.last().unwrap().latex.clone())
    }

    #[test]
    fn solves_linear_equations() {
        assert_eq!(answer("2x + 3 = 7"), "x = 2");
        assert_eq!(answer(r"\frac{x}{2} + 1 = 3"), "x = 4");
        assert_eq!(answer("2(x+1) = x - 3"), "x = -5");
        assert_eq!(answer("3x = 1"), r"x = \frac{1}{3} \approx 0.3333");
        // check line: both sides equal
        let c = last("2x + 3 = 7");
        assert_eq!(c, "7 = 7");
    }

    #[test]
    fn solves_quadratics_with_exact_roots() {
        assert_eq!(answer("x^2 - 5x + 6 = 0"), r"x_1 = 2,\quad x_2 = 3");
        let s = solve("x^{2} + 2x + 1 = 0", false).unwrap();
        assert!(s.iter().any(|x| x.label.contains("double") && x.latex.ends_with("= -1")));
        assert!(answer("2x^2 - 3x - 2 = 0").contains("x_1 = -\\frac{1}{2}"));
    }

    #[test]
    fn irrational_and_complex_roots() {
        let a = answer("x^2 - 2 = 0");
        assert!(a.contains("\\sqrt{2}") && a.contains("1.4142"), "{a}");
        let a = answer("x^2 - 2x - 1 = 0");
        assert!(a.contains("1 - \\sqrt{2}") || a.contains("1 -\\sqrt{2}") || a.contains("\\sqrt{2}"), "{a}");
        let steps = solve("x^2 + 1 = 0", true).unwrap();
        assert!(steps.iter().any(|s| s.label.contains("aucune solution réelle")));
        assert_eq!(steps.last().unwrap().latex, "x = \\pm i");
    }

    #[test]
    fn degenerate_and_unsupported() {
        assert_eq!(last("3x + 1 = 3x + 2"), "\\emptyset");
        assert_eq!(last("2x = 2x"), "x \\in \\mathbb{R}");
        assert_eq!(solve("x^3 = 1", false), Err(SolveError::Unsupported));
        assert_eq!(solve("a + b = c", false), Err(SolveError::Unsupported));
        assert_eq!(solve(r"\sin x = 0", false), Err(SolveError::Unsupported));
        assert_eq!(solve("x + 1", false), Err(SolveError::NotAnEquation));
        assert_eq!(solve("2 = 3", false), Err(SolveError::NotAnEquation));
        assert!(solve("x = ", false).is_err());
        assert!(solve("}{ ^ = (", false).is_err());
    }

    #[test]
    fn french_labels() {
        let s = solve("x^2 - 5x + 6 = 0", true).unwrap();
        assert_eq!(s[0].label, "Équation de départ");
        assert!(s.iter().any(|x| x.label.starts_with("Discriminant")));
    }

    fn rng(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    #[test]
    fn hostile_input_never_panics_or_overflows() {
        let _ = solve(&format!("{}x{} = 1", "(".repeat(100_000), ")".repeat(100_000)), false);
        let _ = solve(&format!("{}x = 1", "-".repeat(100_000)), false);
        let _ = solve(&format!("x = {}", "9".repeat(5_000)), false);
        let _ = solve(&format!("{}x = 1", "99999999999".repeat(8)), false);
        let _ = solve("(x+1)^8 = 0", false);
        let _ = solve("(x+1)^9 = 0", false);
        let _ = solve("x^99999999999999999999 = 0", false);
        let _ = solve("1/0 = x", false);
        let _ = solve("x/0 = 1", false);
        let parts = [
            "x", "2", "3.5", "+", "-", "*", "/", "^", "(", ")", "{", "}", "=", "\\frac", "\\cdot", " ", "é", "x^2", "0",
        ];
        let mut seed = 0xD1B54A32D192ED03u64;
        for _ in 0..4000 {
            let n = (rng(&mut seed) % 14) as usize;
            let src: String = (0..n).map(|_| parts[(rng(&mut seed) % parts.len() as u64) as usize]).collect();
            let _ = solve(&src, true);
        }
    }
}
