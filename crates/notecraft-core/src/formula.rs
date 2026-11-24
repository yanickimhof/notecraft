//! Formula and rollup properties.
//!
//! A formula is a small expression over the other properties of the same row, for example
//! `prop("Price") * prop("Qty")` or `if(prop("Done"), "Closed", "Open")`. A rollup summarises a
//! property of the rows reached through a relation (count, sum, average, ...).
//! Both are computed from the data and never stored in the row files.

use crate::database::{Database, PropKind, PropValue, Property, Row, Schema};
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Rollups

/// What a rollup does with the related values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollupFunc {
    /// The values themselves, separated by commas.
    List,
    /// Number of related rows.
    Count,
    /// Number of related rows that have a value.
    Filled,
    /// Number of different values.
    Unique,
    Sum,
    Average,
    Min,
    Max,
    /// Share of checked boxes, in percent.
    Checked,
}

impl RollupFunc {
    pub const ALL: [RollupFunc; 9] = [
        RollupFunc::List,
        RollupFunc::Count,
        RollupFunc::Filled,
        RollupFunc::Unique,
        RollupFunc::Sum,
        RollupFunc::Average,
        RollupFunc::Min,
        RollupFunc::Max,
        RollupFunc::Checked,
    ];

    pub fn key(self) -> &'static str {
        match self {
            RollupFunc::List => "list",
            RollupFunc::Count => "count",
            RollupFunc::Filled => "filled",
            RollupFunc::Unique => "unique",
            RollupFunc::Sum => "sum",
            RollupFunc::Average => "average",
            RollupFunc::Min => "min",
            RollupFunc::Max => "max",
            RollupFunc::Checked => "checked",
        }
    }

    pub fn from_key(k: &str) -> Option<RollupFunc> {
        RollupFunc::ALL.into_iter().find(|f| f.key() == k)
    }

    /// Whether the function needs a property to look at (a plain count does not).
    pub fn needs_target(self) -> bool {
        self != RollupFunc::Count
    }
}

/// Settings of a rollup property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rollup {
    /// Id of a relation property of this database.
    pub relation: String,
    /// Id of a property of the related database (unused by `Count`).
    #[serde(default)]
    pub target: String,
    pub func: RollupFunc,
}

fn rollup_value(rows: &[&Row], target: Option<&Property>, func: RollupFunc) -> PropValue {
    if func == RollupFunc::Count {
        return PropValue::Number(rows.len() as f64);
    }
    let Some(target) = target else { return PropValue::Empty };
    let values: Vec<PropValue> = rows.iter().map(|r| r.value(target)).collect();
    let nums = || -> Vec<f64> {
        values
            .iter()
            .filter_map(|v| match v {
                PropValue::Number(n) => Some(*n),
                PropValue::Text(s) | PropValue::Select(s) => s.trim().parse().ok(),
                _ => None,
            })
            .collect()
    };
    match func {
        RollupFunc::List => {
            let mut out = Vec::new();
            for v in &values {
                match v {
                    PropValue::Multi(m) => out.extend(m.iter().cloned()),
                    v if !v.is_empty() => out.push(v.display()),
                    _ => {}
                }
            }
            if out.is_empty() {
                PropValue::Empty
            } else {
                PropValue::Text(out.join(", "))
            }
        }
        RollupFunc::Filled => PropValue::Number(values.iter().filter(|v| !v.is_empty()).count() as f64),
        RollupFunc::Unique => {
            let mut seen: Vec<String> = Vec::new();
            for v in values.iter().filter(|v| !v.is_empty()) {
                let d = v.display();
                if !seen.contains(&d) {
                    seen.push(d);
                }
            }
            PropValue::Number(seen.len() as f64)
        }
        RollupFunc::Sum => PropValue::Number(nums().iter().sum()),
        RollupFunc::Average => {
            let n = nums();
            if n.is_empty() {
                PropValue::Empty
            } else {
                PropValue::Number(n.iter().sum::<f64>() / n.len() as f64)
            }
        }
        RollupFunc::Min | RollupFunc::Max => {
            let want_max = func == RollupFunc::Max;
            let n = nums();
            if !n.is_empty() {
                let pick = n.iter().copied().fold(if want_max { f64::MIN } else { f64::MAX }, |a, b| {
                    if want_max {
                        a.max(b)
                    } else {
                        a.min(b)
                    }
                });
                return PropValue::Number(pick);
            }
            // dates (and plain text) compare as text, which orders ISO dates correctly
            let mut dates: Vec<String> = values.iter().filter(|v| !v.is_empty()).map(|v| v.display()).collect();
            dates.sort();
            let pick = if want_max { dates.last() } else { dates.first() };
            match (pick, target.kind) {
                (Some(d), PropKind::Date | PropKind::CreatedTime | PropKind::EditedTime) => PropValue::Date(d.clone()),
                (Some(d), _) => PropValue::Text(d.clone()),
                (None, _) => PropValue::Empty,
            }
        }
        RollupFunc::Checked => {
            if values.is_empty() {
                return PropValue::Empty;
            }
            let on = values.iter().filter(|v| matches!(v, PropValue::Checkbox(true))).count();
            PropValue::Number((on as f64 * 100.0 / values.len() as f64).round())
        }
        RollupFunc::Count => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Formula language

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Id(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        match ch {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' | ';' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '"' | '\u{201c}' | '\u{201d}' | '\'' => {
                let close =
                    |x: char| matches!(x, '"' | '\u{201c}' | '\u{201d}') && ch != '\'' || x == '\'' && ch == '\'';
                let mut s = String::new();
                i += 1;
                loop {
                    match c.get(i) {
                        None => return Err("text not closed".into()),
                        Some('\\') if i + 1 < c.len() => {
                            s.push(match c[i + 1] {
                                'n' => '\n',
                                't' => '\t',
                                o => o,
                            });
                            i += 2;
                        }
                        Some(&x) if close(x) => {
                            i += 1;
                            break;
                        }
                        Some(&x) => {
                            s.push(x);
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Str(s));
            }
            d if d.is_ascii_digit() || (d == '.' && c.get(i + 1).is_some_and(|x| x.is_ascii_digit())) => {
                let start = i;
                while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                    i += 1;
                }
                let t: String = c[start..i].iter().collect();
                out.push(Tok::Num(t.parse().map_err(|_| format!("bad number {t}"))?));
            }
            a if a.is_alphabetic() || a == '_' => {
                let start = i;
                while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Id(c[start..i].iter().collect()));
            }
            _ => {
                let two: String = c[i..(i + 2).min(c.len())].iter().collect();
                let op = match two.as_str() {
                    "==" => Some("=="),
                    "!=" => Some("!="),
                    "<=" => Some("<="),
                    ">=" => Some(">="),
                    "&&" => Some("and"),
                    "||" => Some("or"),
                    _ => None,
                };
                if let Some(op) = op {
                    out.push(Tok::Op(op));
                    i += 2;
                    continue;
                }
                let op = match ch {
                    '+' => "+",
                    '-' => "-",
                    '*' => "*",
                    '/' => "/",
                    '%' => "%",
                    '<' => "<",
                    '>' => ">",
                    '=' => "==",
                    '!' => "not",
                    other => return Err(format!("unexpected '{other}'")),
                };
                out.push(Tok::Op(op));
                i += 1;
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Call(String, Vec<Expr>),
    Bin(&'static str, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Id(x)) if x.eq_ignore_ascii_case(w))
            || matches!(self.peek(), Some(Tok::Op(x)) if *x == w)
    }
    fn or(&mut self) -> Result<Expr, String> {
        let mut l = self.and()?;
        while self.word("or") {
            self.i += 1;
            l = Expr::Bin("or", Box::new(l), Box::new(self.and()?));
        }
        Ok(l)
    }
    fn and(&mut self) -> Result<Expr, String> {
        let mut l = self.cmp()?;
        while self.word("and") {
            self.i += 1;
            l = Expr::Bin("and", Box::new(l), Box::new(self.cmp()?));
        }
        Ok(l)
    }
    fn cmp(&mut self) -> Result<Expr, String> {
        let l = self.add()?;
        if let Some(Tok::Op(op)) = self.peek().cloned() {
            if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=") {
                self.i += 1;
                return Ok(Expr::Bin(op, Box::new(l), Box::new(self.add()?)));
            }
        }
        Ok(l)
    }
    fn add(&mut self) -> Result<Expr, String> {
        let mut l = self.mul()?;
        while let Some(Tok::Op(op)) = self.peek().cloned() {
            if op != "+" && op != "-" {
                break;
            }
            self.i += 1;
            l = Expr::Bin(op, Box::new(l), Box::new(self.mul()?));
        }
        Ok(l)
    }
    fn mul(&mut self) -> Result<Expr, String> {
        let mut l = self.unary()?;
        while let Some(Tok::Op(op)) = self.peek().cloned() {
            if !matches!(op, "*" | "/" | "%") {
                break;
            }
            self.i += 1;
            l = Expr::Bin(op, Box::new(l), Box::new(self.unary()?));
        }
        Ok(l)
    }
    fn unary(&mut self) -> Result<Expr, String> {
        if matches!(self.peek(), Some(Tok::Op("-"))) {
            self.i += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.word("not") {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.primary()
    }
    fn primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Str(s)) => Ok(Expr::Str(s)),
            Some(Tok::LParen) => {
                let e = self.or()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(e),
                    _ => Err("missing )".into()),
                }
            }
            Some(Tok::Id(id)) => {
                let low = id.to_lowercase();
                if self.peek() == Some(&Tok::LParen) {
                    self.i += 1;
                    let mut args = Vec::new();
                    if self.peek() == Some(&Tok::RParen) {
                        self.i += 1;
                    } else {
                        loop {
                            args.push(self.or()?);
                            match self.next() {
                                Some(Tok::Comma) => {}
                                Some(Tok::RParen) => break,
                                _ => return Err(format!("missing ) after {id}(")),
                            }
                        }
                    }
                    return Ok(Expr::Call(low, args));
                }
                match low.as_str() {
                    "true" => Ok(Expr::Bool(true)),
                    "false" => Ok(Expr::Bool(false)),
                    _ => Err(format!("unknown word {id}; use prop(\"Name\") to read a property")),
                }
            }
            Some(Tok::Op(o)) => Err(format!("unexpected {o}")),
            Some(Tok::RParen) => Err("unexpected )".into()),
            Some(Tok::Comma) => Err("unexpected ,".into()),
            None => Err("the formula is incomplete".into()),
        }
    }
}

/// Check a formula without a row: `Ok` or the reason it cannot work.
pub fn check(src: &str) -> Result<(), String> {
    parse(src).map(|_| ())
}

fn parse(src: &str) -> Result<Expr, String> {
    if src.trim().is_empty() {
        return Err("empty formula".into());
    }
    let mut p = Parser { t: lex(src)?, i: 0 };
    let e = p.or()?;
    if p.i < p.t.len() {
        return Err("something is left after the end of the formula".into());
    }
    Ok(e)
}

// ---------------------------------------------------------------------------
// Evaluation

#[derive(Debug, Clone, PartialEq)]
enum V {
    Empty,
    Num(f64),
    Str(String),
    Bool(bool),
    Date(String),
}

impl V {
    fn from_prop(v: PropValue) -> V {
        match v {
            PropValue::Empty => V::Empty,
            PropValue::Number(n) => V::Num(n),
            PropValue::Checkbox(b) => V::Bool(b),
            PropValue::Date(d) if d.trim().is_empty() => V::Empty,
            PropValue::Date(d) => V::Date(d),
            PropValue::Text(s) | PropValue::Select(s) => V::Str(s),
            PropValue::Multi(m) => V::Str(m.join(", ")),
        }
    }

    fn into_prop(self) -> PropValue {
        match self {
            V::Empty => PropValue::Empty,
            V::Num(n) => PropValue::Number(n),
            V::Str(s) => PropValue::Text(s),
            V::Bool(b) => PropValue::Checkbox(b),
            V::Date(d) => PropValue::Date(d),
        }
    }

    fn text(&self) -> String {
        match self {
            V::Empty => String::new(),
            V::Num(n) => crate::database::format_number(*n),
            V::Str(s) | V::Date(s) => s.clone(),
            V::Bool(b) => b.to_string(),
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            V::Num(n) => Some(*n),
            V::Bool(b) => Some(*b as u8 as f64),
            V::Str(s) => s.trim().parse().ok(),
            V::Empty => Some(0.0),
            V::Date(_) => None,
        }
    }

    fn truthy(&self) -> bool {
        match self {
            V::Empty => false,
            V::Num(n) => *n != 0.0,
            V::Str(s) => !s.trim().is_empty(),
            V::Bool(b) => *b,
            V::Date(_) => true,
        }
    }

    fn date(&self) -> Option<Date> {
        match self {
            V::Date(s) | V::Str(s) => s.get(..10)?.parse().ok(),
            _ => None,
        }
    }
}

struct Ctx<'a> {
    schema: &'a Schema,
    row: &'a Row,
    /// Formulas being evaluated, to stop a formula that uses itself.
    stack: Vec<String>,
}

fn arg<'a>(a: &'a [V], i: usize, f: &str) -> Result<&'a V, String> {
    a.get(i).ok_or_else(|| format!("{f}() is missing an argument"))
}

fn number(a: &[V], i: usize, f: &str) -> Result<f64, String> {
    arg(a, i, f)?.num().ok_or_else(|| format!("{f}() needs a number"))
}

fn date_text(d: Date) -> String {
    d.to_string()
}

fn eval(e: &Expr, cx: &mut Ctx) -> Result<V, String> {
    match e {
        Expr::Num(n) => Ok(V::Num(*n)),
        Expr::Str(s) => Ok(V::Str(s.clone())),
        Expr::Bool(b) => Ok(V::Bool(*b)),
        Expr::Neg(x) => {
            let v = eval(x, cx)?;
            Ok(V::Num(-v.num().ok_or("a number is expected after -")?))
        }
        Expr::Not(x) => Ok(V::Bool(!eval(x, cx)?.truthy())),
        Expr::Bin(op, l, r) => {
            if *op == "and" || *op == "or" {
                let a = eval(l, cx)?.truthy();
                // short-circuit, as in `if(not empty(x) and ...)`
                if (*op == "and" && !a) || (*op == "or" && a) {
                    return Ok(V::Bool(a));
                }
                return Ok(V::Bool(eval(r, cx)?.truthy()));
            }
            let (a, b) = (eval(l, cx)?, eval(r, cx)?);
            binary(op, a, b)
        }
        Expr::Call(name, args) => call(name, args, cx),
    }
}

fn binary(op: &str, a: V, b: V) -> Result<V, String> {
    match op {
        "+" => {
            if let (Some(d), V::Num(n)) = (a.date(), &b) {
                if matches!(a, V::Date(_)) {
                    let nd = d.checked_add(jiff::Span::new().days(*n as i64)).map_err(|e| e.to_string())?;
                    return Ok(V::Date(date_text(nd)));
                }
            }
            if matches!(a, V::Str(_)) || matches!(b, V::Str(_)) {
                return Ok(V::Str(format!("{}{}", a.text(), b.text())));
            }
            Ok(V::Num(a.num().ok_or("+ needs numbers or text")? + b.num().ok_or("+ needs numbers or text")?))
        }
        "-" => {
            if let (V::Date(_), V::Date(_)) = (&a, &b) {
                return Ok(V::Num(days_between(&a, &b)?));
            }
            if let (V::Date(_), V::Num(n)) = (&a, &b) {
                let d = a.date().ok_or("not a date")?;
                let nd = d.checked_add(jiff::Span::new().days(-(*n as i64))).map_err(|e| e.to_string())?;
                return Ok(V::Date(date_text(nd)));
            }
            Ok(V::Num(a.num().ok_or("- needs numbers")? - b.num().ok_or("- needs numbers")?))
        }
        "*" | "/" | "%" => {
            let (x, y) = (a.num().ok_or("a number is expected")?, b.num().ok_or("a number is expected")?);
            if op != "*" && y == 0.0 {
                return Err("division by zero".into());
            }
            Ok(V::Num(match op {
                "*" => x * y,
                "/" => x / y,
                _ => x % y,
            }))
        }
        _ => {
            let ord = compare(&a, &b);
            let eq = ord == Some(std::cmp::Ordering::Equal);
            Ok(V::Bool(match op {
                "==" => eq,
                "!=" => !eq,
                "<" => ord == Some(std::cmp::Ordering::Less),
                "<=" => matches!(ord, Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)),
                ">" => ord == Some(std::cmp::Ordering::Greater),
                ">=" => matches!(ord, Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)),
                _ => return Err(format!("unknown operator {op}")),
            }))
        }
    }
}

fn compare(a: &V, b: &V) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (V::Empty, V::Empty) => Some(std::cmp::Ordering::Equal),
        (V::Date(x), V::Date(y)) => Some(x.cmp(y)),
        (V::Bool(x), V::Bool(y)) => Some(x.cmp(y)),
        (V::Str(x), V::Str(y)) => Some(x.to_lowercase().cmp(&y.to_lowercase())),
        _ => match (a.num(), b.num()) {
            (Some(x), Some(y)) if !matches!(a, V::Str(_)) || !matches!(b, V::Str(_)) => x.partial_cmp(&y),
            _ => Some(a.text().to_lowercase().cmp(&b.text().to_lowercase())),
        },
    }
}

fn days_between(a: &V, b: &V) -> Result<f64, String> {
    let (x, y) = (a.date().ok_or("not a date")?, b.date().ok_or("not a date")?);
    let span = y.until((jiff::Unit::Day, x)).map_err(|e| e.to_string())?;
    Ok(span.get_days() as f64)
}

fn today() -> Date {
    jiff::Zoned::now().date()
}

fn call(name: &str, args: &[Expr], cx: &mut Ctx) -> Result<V, String> {
    // lazy functions first
    match name {
        "if" => {
            if args.len() < 2 {
                return Err("if() needs a condition and a value".into());
            }
            return if eval(&args[0], cx)?.truthy() {
                eval(&args[1], cx)
            } else if let Some(other) = args.get(2) {
                eval(other, cx)
            } else {
                Ok(V::Empty)
            };
        }
        "prop" => {
            let [Expr::Str(n)] = args else { return Err("prop() takes a property name in quotes".into()) };
            return read_prop(n, cx);
        }
        _ => {}
    }
    let mut a = Vec::with_capacity(args.len());
    for x in args {
        a.push(eval(x, cx)?);
    }
    let num = |i: usize| number(&a, i, name);
    Ok(match name {
        "and" => V::Bool(a.iter().all(|v| v.truthy())),
        "or" => V::Bool(a.iter().any(|v| v.truthy())),
        "not" => V::Bool(!arg(&a, 0, name)?.truthy()),
        "empty" => V::Bool(matches!(arg(&a, 0, name)?, V::Empty) || arg(&a, 0, name)?.text().trim().is_empty()),
        "concat" | "join" => V::Str(a.iter().map(V::text).collect()),
        "length" | "len" => V::Num(arg(&a, 0, name)?.text().chars().count() as f64),
        "upper" => V::Str(arg(&a, 0, name)?.text().to_uppercase()),
        "lower" => V::Str(arg(&a, 0, name)?.text().to_lowercase()),
        "trim" => V::Str(arg(&a, 0, name)?.text().trim().to_string()),
        "contains" => {
            V::Bool(arg(&a, 0, name)?.text().to_lowercase().contains(&arg(&a, 1, name)?.text().to_lowercase()))
        }
        "replace" => V::Str(arg(&a, 0, name)?.text().replace(&arg(&a, 1, name)?.text(), &arg(&a, 2, name)?.text())),
        "format" | "text" => V::Str(arg(&a, 0, name)?.text()),
        "tonumber" | "number" => {
            V::Num(arg(&a, 0, name)?.text().trim().replace(',', ".").parse().map_err(|_| "not a number")?)
        }
        "round" => {
            let d = if a.len() > 1 { num(1)? } else { 0.0 };
            let k = 10f64.powi(d as i32);
            V::Num((num(0)? * k).round() / k)
        }
        "floor" => V::Num(num(0)?.floor()),
        "ceil" => V::Num(num(0)?.ceil()),
        "abs" => V::Num(num(0)?.abs()),
        "sqrt" => V::Num(num(0)?.sqrt()),
        "pow" => V::Num(num(0)?.powf(num(1)?)),
        "min" | "max" | "sum" | "avg" | "average" => {
            let n: Vec<f64> = a.iter().filter_map(V::num).collect();
            if n.is_empty() {
                return Ok(V::Empty);
            }
            V::Num(match name {
                "min" => n.iter().copied().fold(f64::MAX, f64::min),
                "max" => n.iter().copied().fold(f64::MIN, f64::max),
                "sum" => n.iter().sum(),
                _ => n.iter().sum::<f64>() / n.len() as f64,
            })
        }
        "today" => V::Date(date_text(today())),
        "now" => V::Date(crate::now_string()),
        "year" | "month" | "day" => {
            let d = arg(&a, 0, name)?.date().ok_or_else(|| format!("{name}() needs a date"))?;
            V::Num(match name {
                "year" => d.year() as f64,
                "month" => d.month() as f64,
                _ => d.day() as f64,
            })
        }
        "datebetween" => {
            let (x, y) = (arg(&a, 0, name)?, arg(&a, 1, name)?);
            let unit = a.get(2).map(V::text).unwrap_or_else(|| "days".into()).to_lowercase();
            let days = days_between(y, x)?;
            V::Num(match unit.trim_end_matches('s') {
                "day" => days,
                "week" => (days / 7.0).trunc(),
                "month" => (days / 30.4375).trunc(),
                "year" => (days / 365.25).trunc(),
                u => return Err(format!("unknown unit {u}; use days, weeks, months or years")),
            })
        }
        "dateadd" => {
            let d = arg(&a, 0, name)?.date().ok_or("dateAdd() needs a date")?;
            let n = num(1)? as i64;
            let unit = a.get(2).map(V::text).unwrap_or_else(|| "days".into()).to_lowercase();
            let span = match unit.trim_end_matches('s') {
                "day" => jiff::Span::new().days(n),
                "week" => jiff::Span::new().weeks(n),
                "month" => jiff::Span::new().months(n),
                "year" => jiff::Span::new().years(n),
                u => return Err(format!("unknown unit {u}; use days, weeks, months or years")),
            };
            V::Date(date_text(d.checked_add(span).map_err(|e| e.to_string())?))
        }
        other => return Err(format!("unknown function {other}()")),
    })
}

fn read_prop(name: &str, cx: &mut Ctx) -> Result<V, String> {
    let p = cx
        .schema
        .properties
        .iter()
        .find(|p| p.name == name)
        .or_else(|| cx.schema.properties.iter().find(|p| p.name.eq_ignore_ascii_case(name)))
        .ok_or_else(|| format!("no property named \"{name}\""))?;
    if p.kind == PropKind::Formula {
        if cx.stack.contains(&p.id) {
            return Err(format!("\"{name}\" uses itself"));
        }
        cx.stack.push(p.id.clone());
        let r = parse(&p.formula).and_then(|e| eval(&e, cx));
        cx.stack.pop();
        return r.map_err(|e| format!("in \"{name}\": {e}"));
    }
    Ok(V::from_prop(cx.row.value(p)))
}

/// Value of a formula for one row.
pub fn eval_formula(schema: &Schema, row: &Row, prop: &Property) -> PropValue {
    if prop.formula.trim().is_empty() {
        return PropValue::Empty;
    }
    let run = || -> Result<V, String> {
        let e = parse(&prop.formula)?;
        let mut cx = Ctx { schema, row, stack: vec![prop.id.clone()] };
        eval(&e, &mut cx)
    };
    match run() {
        Ok(v) => v.into_prop(),
        Err(e) => PropValue::Text(format!("\u{26a0} {e}")),
    }
}

// ---------------------------------------------------------------------------
// Whole workspace

fn has_computed(db: &Database) -> bool {
    db.schema.properties.iter().any(|p| matches!(p.kind, PropKind::Formula | PropKind::Rollup))
}

fn formulas_pass(dbs: &mut BTreeMap<String, Database>) {
    for db in dbs.values_mut() {
        let props: Vec<Property> =
            db.schema.properties.iter().filter(|p| p.kind == PropKind::Formula).cloned().collect();
        if props.is_empty() {
            continue;
        }
        let results: Vec<(String, Vec<(String, PropValue)>)> = db
            .rows
            .iter()
            .map(|(id, row)| {
                (id.clone(), props.iter().map(|p| (p.id.clone(), eval_formula(&db.schema, row, p))).collect())
            })
            .collect();
        for (id, vals) in results {
            if let Some(row) = db.rows.get_mut(&id) {
                row.computed.extend(vals);
            }
        }
    }
}

fn rollups_pass(dbs: &mut BTreeMap<String, Database>) {
    let mut updates: Vec<(String, String, String, PropValue)> = Vec::new();
    for (db_id, db) in dbs.iter() {
        for p in db.schema.properties.iter().filter(|p| p.kind == PropKind::Rollup) {
            let Some(cfg) = &p.rollup else { continue };
            let Some(rel) = db.schema.prop(&cfg.relation) else { continue };
            let target_db = rel.relation_db.as_deref().unwrap_or(db_id);
            let Some(other) = dbs.get(target_db) else { continue };
            let target = other.schema.prop(&cfg.target);
            for (rid, row) in &db.rows {
                let ids = match row.value(rel) {
                    PropValue::Multi(v) => v,
                    _ => Vec::new(),
                };
                let rows: Vec<&Row> =
                    ids.iter().filter_map(|i| other.rows.get(i)).filter(|r| r.meta.trashed.is_none()).collect();
                updates.push((db_id.clone(), rid.clone(), p.id.clone(), rollup_value(&rows, target, cfg.func)));
            }
        }
    }
    for (db_id, rid, pid, v) in updates {
        if let Some(row) = dbs.get_mut(&db_id).and_then(|d| d.rows.get_mut(&rid)) {
            row.computed.insert(pid, v);
        }
    }
}

/// Recompute every formula and rollup of the workspace. Cheap when there are none.
pub fn recompute(dbs: &mut BTreeMap<String, Database>) {
    if !dbs.values().any(has_computed) {
        return;
    }
    formulas_pass(dbs);
    rollups_pass(dbs);
    // formulas that read rollups, and rollups of formulas
    formulas_pass(dbs);
    rollups_pass(dbs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{Database, Property, Row};

    fn db_with(props: Vec<Property>) -> Database {
        let mut schema = Database::new_schema("d".into(), "D", None);
        schema.properties.extend(props);
        Database { schema, rows: BTreeMap::new(), templates: BTreeMap::new(), dir: std::path::PathBuf::new() }
    }

    fn run(src: &str, rows: &[(&str, &[(&str, serde_json::Value)])]) -> Vec<String> {
        let mut f = Property::new("F", PropKind::Formula);
        f.formula = src.into();
        let mut db = db_with(vec![
            Property::new("Price", PropKind::Number),
            Property::new("Qty", PropKind::Number),
            Property::new("Done", PropKind::Checkbox),
            Property::new("Due", PropKind::Date),
            f.clone(),
        ]);
        for (i, (title, vals)) in rows.iter().enumerate() {
            let mut r = Row::new(title);
            for (k, v) in vals.iter() {
                r.meta.properties.insert(k.to_string(), v.clone());
            }
            db.rows.insert(format!("r{i}"), r);
        }
        let mut dbs = BTreeMap::new();
        dbs.insert("d".to_string(), db);
        recompute(&mut dbs);
        let db = &dbs["d"];
        let p = db.schema.prop(&f.id).unwrap().clone();
        db.rows.values().map(|r| r.value(&p).display()).collect()
    }

    #[test]
    fn arithmetic_and_props() {
        use serde_json::json;
        let out = run(r#"prop("Price") * prop("Qty") + 1"#, &[("a", &[("Price", json!(2.5)), ("Qty", json!(4))])]);
        assert_eq!(out, vec!["11"]);
    }

    #[test]
    fn conditions_and_text() {
        use serde_json::json;
        let out = run(
            r#"if(prop("Done"), "Closed", concat("Open: ", prop("Name")))"#,
            &[("t1", &[("Done", json!(true))]), ("t2", &[])],
        );
        assert_eq!(out, vec!["Closed", "Open: t2"]);
    }

    #[test]
    fn dates() {
        use serde_json::json;
        let out = run(r#"dateBetween(prop("Due"), "2026-01-10", "days")"#, &[("a", &[("Due", json!("2026-01-01"))])]);
        assert_eq!(out, vec!["9"]);
        let out = run(r#"prop("Due") + 3"#, &[("a", &[("Due", json!("2026-01-30"))])]);
        assert_eq!(out, vec!["2026-02-02"]);
    }

    #[test]
    fn errors_are_shown_not_panicked() {
        let out = run(r#"prop("Nope")"#, &[("a", &[])]);
        assert!(out[0].starts_with('\u{26a0}'), "{out:?}");
        let out = run("1 / 0", &[("a", &[])]);
        assert!(out[0].contains("division by zero"));
        assert!(check("1 +").is_err());
        assert!(check("round(2.5)").is_ok());
    }

    #[test]
    fn a_formula_cannot_use_itself() {
        let out = run(r#"prop("F") + 1"#, &[("a", &[])]);
        assert!(out[0].contains("uses itself"), "{out:?}");
    }

    #[test]
    fn rollup_sums_related_rows() {
        use serde_json::json;
        let mut items = db_with(vec![Property::new("Cost", PropKind::Number)]);
        items.schema.id = "items".into();
        for (i, c) in [10.0, 5.5].iter().enumerate() {
            let mut r = Row::new("i");
            r.meta.properties.insert("Cost".into(), json!(c));
            items.rows.insert(format!("i{i}"), r);
        }
        let cost_id = items.schema.prop_by_name("Cost").unwrap().id.clone();
        let mut rel = Property::new("Items", PropKind::Relation);
        rel.relation_db = Some("items".into());
        let mut total = Property::new("Total", PropKind::Rollup);
        total.rollup = Some(Rollup { relation: rel.id.clone(), target: cost_id, func: RollupFunc::Sum });
        let mut count = Property::new("N", PropKind::Rollup);
        count.rollup = Some(Rollup { relation: rel.id.clone(), target: String::new(), func: RollupFunc::Count });
        let (tid, nid) = (total.id.clone(), count.id.clone());
        let mut orders = db_with(vec![rel, total, count]);
        orders.schema.id = "orders".into();
        let mut o = Row::new("o");
        o.meta.properties.insert("Items".into(), json!(["i0", "i1"]));
        orders.rows.insert("o0".into(), o);
        let mut dbs = BTreeMap::new();
        dbs.insert("items".to_string(), items);
        dbs.insert("orders".to_string(), orders);
        recompute(&mut dbs);
        let row = &dbs["orders"].rows["o0"];
        assert_eq!(row.computed[&tid], PropValue::Number(15.5));
        assert_eq!(row.computed[&nid], PropValue::Number(2.0));
    }
}
