//! Turn a CSV table (a database export, a spreadsheet saved as CSV) into a database.
//!
//! The first column becomes the row title. Every other column gets the most
//! specific property type its values allow (checkbox, number, date, link, email,
//! select, multi-select) and falls back to text, so nothing is ever lost.

use crate::database::{PropKind, Property, Row, SelectOption};
use crate::workspace::Workspace;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Largest table accepted, so a hostile or accidental huge file cannot exhaust memory.
const MAX_CELLS: usize = 5_000_000;

/// What was created.
#[derive(Debug, Clone, Default)]
pub struct CsvImport {
    pub db: String,
    /// Row title and row id, in file order.
    pub rows: Vec<(String, String)>,
}

/// Parse CSV text (RFC 4180 quoting, `,` `;` or tab as separator, BOM and CRLF accepted).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first_line = text.lines().next().unwrap_or("");
    let delim = [',', ';', '\t']
        .into_iter()
        .max_by_key(|d| first_line.matches(*d).count())
        .filter(|d| first_line.contains(*d))
        .unwrap_or(',');
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let mut cells = 0usize;
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            c if c == delim => {
                row.push(std::mem::take(&mut field));
                cells += 1;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
                if cells > MAX_CELLS {
                    break;
                }
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

const EN_MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
const FR_MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

fn month_number(word: &str) -> Option<u32> {
    let w = word.trim_end_matches('.').to_lowercase();
    if w.chars().count() < 3 {
        return None;
    }
    for (i, (en, fr)) in EN_MONTHS.iter().zip(FR_MONTHS.iter()).enumerate() {
        if w == *en || w == *fr || (w.chars().count() == 3 && (en.starts_with(&w) || fr.starts_with(&w))) {
            return Some(i as u32 + 1);
        }
        if w == "sept" && i == 8 {
            return Some(9);
        }
        if (w == "janv" && i == 0) || (w == "févr" && i == 1) || (w == "juil" && i == 6) {
            return Some(i as u32 + 1);
        }
    }
    None
}

fn valid_ymd(y: i32, m: u32, d: u32) -> bool {
    (1000..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// A time like `9:00 AM`, `14:30`, `2:05pm`, `09:30:00Z`, as `HH:MM`.
fn parse_time(s: &str) -> Option<String> {
    let mut s = s.trim().to_lowercase();
    if let Some(i) = s.find('+') {
        s.truncate(i);
    }
    let s = s.trim().trim_end_matches('z').trim();
    let (body, pm, am) = if let Some(b) = s.strip_suffix("pm") {
        (b.trim(), true, false)
    } else if let Some(b) = s.strip_suffix("am") {
        (b.trim(), false, true)
    } else {
        (s, false, false)
    };
    let mut it = body.split(':');
    let mut h: u32 = it.next()?.trim().parse().ok()?;
    let m: u32 = it.next()?.trim().parse().ok()?;
    if h > 23 || m > 59 || ((pm || am) && !(1..=12).contains(&h)) {
        return None;
    }
    if pm && h < 12 {
        h += 12;
    }
    if am && h == 12 {
        h = 0;
    }
    Some(format!("{h:02}:{m:02}"))
}

/// Leading digits of a day word: `3`, `3rd`, `1er`, `23.`.
fn day_number(w: &str) -> Option<u32> {
    let digits: String = w.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &w[digits.len()..];
    if digits.is_empty() || digits.len() > 2 || !matches!(rest, "" | "." | "st" | "nd" | "rd" | "th" | "er" | "e") {
        return None;
    }
    digits.parse().ok()
}

/// Understand the date formats Evernote and spreadsheets write. Returns
/// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`. A range (`A → B`) keeps its start.
pub fn parse_date(raw: &str) -> Option<String> {
    let s = raw.split('→').next().unwrap_or(raw).trim();
    // some CSV exports write `8 octobre 2026 10:31 (UTC+2)`: the zone note is dropped, the local time is kept
    let s = match s.rfind('(') {
        Some(i) if s.ends_with(')') && s[i..].to_lowercase().contains("utc") => s[..i].trim(),
        _ => s,
    };
    let s = s.trim_start_matches('@').trim();
    if s.is_empty() || s.len() > 40 {
        return None;
    }
    let with_time = |ymd: (i32, u32, u32), rest: &str| -> Option<String> {
        if !valid_ymd(ymd.0, ymd.1, ymd.2) {
            return None;
        }
        let date = format!("{:04}-{:02}-{:02}", ymd.0, ymd.1, ymd.2);
        let rest = rest.trim().trim_start_matches(['T', '@']).trim();
        if rest.is_empty() {
            return Some(date);
        }
        parse_time(rest).map(|t| format!("{date}T{t}"))
    };
    // 2026-11-23, 2026/11/23, 2026-11-23T09:30:00Z, 2026-11-23 09:30
    let numeric: Vec<&str> = s.splitn(2, ['T', ' ']).collect();
    for sep in ['-', '/'] {
        let parts: Vec<&str> = numeric[0].split(sep).collect();
        if parts.len() == 3 && parts[0].len() == 4 {
            if let (Ok(y), Ok(m), Ok(d)) = (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
                return with_time((y, m, d), numeric.get(1).copied().unwrap_or(""));
            }
        }
    }
    // 23.11.2026 (day first)
    let parts: Vec<&str> = numeric[0].split('.').collect();
    if parts.len() == 3 && parts[2].len() == 4 {
        if let (Ok(d), Ok(m), Ok(y)) = (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
            return with_time((y, m, d), numeric.get(1).copied().unwrap_or(""));
        }
    }
    // 23/11/2026 only when the day cannot be a month
    let parts: Vec<&str> = numeric[0].split('/').collect();
    if parts.len() == 3 && parts[2].len() == 4 {
        if let (Ok(a), Ok(b), Ok(y)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<i32>()) {
            if a > 12 && b <= 12 {
                return with_time((y, b, a), numeric.get(1).copied().unwrap_or(""));
            }
            if b > 12 && a <= 12 {
                return with_time((y, a, b), numeric.get(1).copied().unwrap_or(""));
            }
        }
    }
    // November 23, 2026 [9:00 AM]   |   Nov 23 2026   |   23 novembre 2026 [14:30]   |   23 Nov 2026
    let words: Vec<&str> = s.split([' ', ',']).filter(|w| !w.is_empty()).collect();
    if words.len() >= 3 {
        let rest = |from: usize| words[from..].join(" ");
        if let Some(m) = month_number(words[0]) {
            if let (Some(d), Ok(y)) = (day_number(words[1]), words[2].parse::<i32>()) {
                return with_time((y, m, d), &rest(3));
            }
        }
        if let Some(m) = month_number(words[1]) {
            if let (Some(d), Ok(y)) = (day_number(words[0]), words[2].parse::<i32>()) {
                return with_time((y, m, d), &rest(3));
            }
        }
    }
    None
}

fn is_yes(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "yes" | "true" | "oui" | "✓" | "☑" | "x" | "checked")
}
fn is_no(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "no" | "false" | "non" | "☐" | "unchecked")
}

fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim().replace(['\u{202f}', '\u{a0}', '\''], "");
    if t.is_empty() || t.len() > 24 || t.starts_with('0') && t.len() > 1 && !t.starts_with("0.") && !t.starts_with("0,")
    {
        return None;
    }
    let t = t.strip_prefix('+').unwrap_or(&t);
    let decimal;
    let t = match t.split_once(',') {
        Some((a, b))
            if !b.contains(',')
                && !a.contains('.')
                && !a.is_empty()
                && !b.is_empty()
                && b.len() != 3
                && a.trim_start_matches('-').chars().all(|c| c.is_ascii_digit())
                && b.chars().all(|c| c.is_ascii_digit()) =>
        {
            decimal = format!("{a}.{b}");
            decimal.as_str()
        }
        _ => t,
    };
    let n: f64 = t.parse().ok()?;
    n.is_finite().then_some(n)
}

const COLORS: [&str; 9] = ["blue", "green", "orange", "purple", "pink", "yellow", "red", "brown", "gray"];

fn options(names: impl IntoIterator<Item = String>) -> Vec<SelectOption> {
    names
        .into_iter()
        .enumerate()
        .map(|(i, name)| SelectOption { name, color: COLORS[i % COLORS.len()].to_string(), group: None })
        .collect()
}

/// The most specific property kind that fits all the (non-empty) values of a column.
fn infer_kind(values: &[&str]) -> PropKind {
    if values.is_empty() {
        return PropKind::Text;
    }
    if values.iter().all(|v| is_yes(v) || is_no(v))
        && values.iter().any(|v| is_yes(v) || is_no(v))
        && !values.is_empty()
    {
        return PropKind::Checkbox;
    }
    if values.iter().all(|v| parse_number(v).is_some()) {
        return PropKind::Number;
    }
    if values.iter().all(|v| parse_date(v).is_some()) {
        return PropKind::Date;
    }
    if values.iter().all(|v| (v.starts_with("http://") || v.starts_with("https://")) && !v.contains(' ')) {
        return PropKind::Url;
    }
    if values.iter().all(|v| v.contains('@') && !v.contains(' ') && v.contains('.')) {
        return PropKind::Email;
    }
    let single: BTreeSet<&str> = values.iter().copied().collect();
    let tokens: Vec<&str> =
        values.iter().flat_map(|v| v.split(", ")).map(str::trim).filter(|t| !t.is_empty()).collect();
    let distinct_tokens: BTreeSet<&str> = tokens.iter().copied().collect();
    let short = tokens.iter().all(|t| t.chars().count() <= 30 && !t.contains('\n'));
    let multi = values.iter().any(|v| v.contains(", "));
    if values.len() >= 3 && short && single.len() <= 12 && single.len() < values.len() && !multi {
        return PropKind::Select;
    }
    if values.len() >= 3 && short && multi && distinct_tokens.len() <= 24 && tokens.len() > distinct_tokens.len() {
        return PropKind::MultiSelect;
    }
    PropKind::Text
}

fn cell_value(kind: PropKind, s: &str) -> Value {
    let s = s.trim();
    if s.is_empty() {
        return if kind == PropKind::Checkbox { Value::Bool(false) } else { Value::Null };
    }
    match kind {
        PropKind::Checkbox => Value::Bool(is_yes(s)),
        PropKind::Number => parse_number(s)
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(s.to_string())),
        PropKind::Date => Value::String(parse_date(s).unwrap_or_else(|| s.to_string())),
        PropKind::MultiSelect => Value::Array(
            s.split(", ").map(|t| Value::String(t.trim().to_string())).filter(|v| v.as_str() != Some("")).collect(),
        ),
        _ => Value::String(s.to_string()),
    }
}

/// Index of the column that should name the rows when column 0 cannot: a yes/no
/// column makes a poor title, so the first plain-text column takes over.
fn title_column(table: &[Vec<String>]) -> Option<usize> {
    let ncols = table.first()?.len();
    let col = |c: usize| -> Vec<&str> {
        table.iter().skip(1).filter_map(|r| r.get(c)).map(|s| s.trim()).filter(|s| !s.is_empty()).collect()
    };
    let first = col(0);
    if first.is_empty() || infer_kind(&first) != PropKind::Checkbox {
        return None;
    }
    (1..ncols).find(|&c| {
        let v = col(c);
        v.len() == table.len().saturating_sub(1) && infer_kind(&v) == PropKind::Text
    })
}

/// Create a database named `title` from CSV `text`, under `parent`.
pub fn import_csv_database(ws: &mut Workspace, parent: Option<&str>, title: &str, text: &str) -> Result<CsvImport> {
    let mut table = parse_csv(text);
    if let Some(tc) = title_column(&table) {
        for rec in table.iter_mut() {
            if tc < rec.len() {
                let cell = rec.remove(tc);
                rec.insert(0, cell);
            }
        }
    }
    let Some(header) = table.first() else {
        return Err(Error::Invalid("the CSV file is empty".into()));
    };
    let ncols = header.len().max(1);
    let db = ws.create_database(parent, title)?;
    // columns: names made unique and non-empty; column 0 is the title
    let mut names: Vec<String> = Vec::new();
    for (i, h) in header.iter().enumerate() {
        let mut n = h.trim().to_string();
        if n.is_empty() {
            n = if i == 0 { "Name".into() } else { format!("Column {}", i + 1) };
        }
        let base = n.clone();
        let mut k = 2;
        while names.contains(&n) {
            n = format!("{base} {k}");
            k += 1;
        }
        names.push(n);
    }
    let body: Vec<&Vec<String>> = table.iter().skip(1).collect();
    let mut props: Vec<Property> = Vec::new();
    for (c, name) in names.iter().enumerate() {
        if c == 0 {
            props.push(Property::new(name, PropKind::Title));
            continue;
        }
        let vals: Vec<&str> =
            body.iter().filter_map(|r| r.get(c)).map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        let kind = infer_kind(&vals);
        let mut p = Property::new(name, kind);
        match kind {
            PropKind::Select => p.options = options(vals.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>()),
            PropKind::MultiSelect => {
                p.options = options(
                    vals.iter().flat_map(|v| v.split(", ")).map(|t| t.trim().to_string()).collect::<BTreeSet<_>>(),
                )
            }
            _ => {}
        }
        props.push(p);
    }
    let kinds: Vec<PropKind> = props.iter().map(|p| p.kind).collect();
    let mut out = CsvImport { db: db.clone(), rows: Vec::new() };
    let mut made: Vec<String> = Vec::new();
    {
        let d = ws.dbs.get_mut(&db).ok_or_else(|| Error::NotFound(db.clone()))?;
        d.schema.properties = props;
        for rec in body {
            let title_cell = rec.first().map(|s| s.trim()).unwrap_or("");
            let title = if title_cell.is_empty() { "Untitled" } else { title_cell };
            let mut row = Row::new(title);
            let mut vals: BTreeMap<String, Value> = BTreeMap::new();
            for c in 1..ncols.min(names.len()) {
                let v = cell_value(kinds[c], rec.get(c).map(String::as_str).unwrap_or(""));
                if !v.is_null() {
                    vals.insert(names[c].clone(), v);
                }
            }
            row.meta.properties = vals;
            out.rows.push((title.to_string(), row.meta.id.clone()));
            made.push(row.meta.id.clone());
            d.rows.insert(row.meta.id.clone(), row);
        }
    }
    ws.save_schema(&db)?;
    for id in &made {
        ws.save_row(&db, id)?;
    }
    Ok(out)
}

/// Turn text columns whose cells name the rows of another of these databases into real relations
/// (exports write a relation as the titles of the related pages). A column converts when at least
/// 80 % of its cells match; the cells that match nothing are dropped. Returns how many columns changed.
pub fn link_relations(ws: &mut Workspace, dbs: &[String]) -> usize {
    use std::collections::BTreeMap;
    let titles: BTreeMap<String, BTreeMap<String, String>> = dbs
        .iter()
        .filter_map(|id| {
            let d = ws.dbs.get(id)?;
            let mut by: BTreeMap<String, String> = BTreeMap::new();
            let mut twice = BTreeSet::new();
            for (rid, r) in &d.rows {
                let title = r.meta.title.trim().to_string();
                if by.insert(title.clone(), rid.clone()).is_some() {
                    twice.insert(title);
                }
            }
            for title in twice {
                by.remove(&title);
            }
            Some((id.clone(), by))
        })
        .collect();
    let tokens = |v: Option<&Value>| -> Vec<String> {
        match v {
            Some(Value::String(s)) => s.split(", ").map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(|x| x.trim().to_string()).collect(),
            _ => Vec::new(),
        }
    };
    let mut converted = 0;
    for db in dbs {
        let Some(d) = ws.dbs.get(db) else { continue };
        let candidates: Vec<String> = d
            .schema
            .properties
            .iter()
            .filter(|p| matches!(p.kind, PropKind::Text | PropKind::Select | PropKind::MultiSelect))
            .map(|p| p.name.clone())
            .collect();
        let mut changed = false;
        for name in candidates {
            let d = &ws.dbs[db];
            let all: Vec<String> = d.rows.values().flat_map(|r| tokens(r.meta.properties.get(&name))).collect();
            if all.is_empty() {
                continue;
            }
            let Some((target, hits)) = titles
                .iter()
                .map(|(t, m)| (t.clone(), all.iter().filter(|x| m.contains_key(*x)).count()))
                .max_by_key(|(_, n)| *n)
            else {
                continue;
            };
            if hits == 0 || hits * 10 < all.len() * 8 {
                continue;
            }
            let map = &titles[&target];
            let d = ws.dbs.get_mut(db).unwrap();
            if let Some(p) = d.schema.properties.iter_mut().find(|p| p.name == name) {
                p.kind = PropKind::Relation;
                p.relation_db = Some(target.clone());
                p.options.clear();
            }
            for r in d.rows.values_mut() {
                let ids: Vec<Value> = tokens(r.meta.properties.get(&name))
                    .iter()
                    .filter_map(|x| map.get(x))
                    .map(|id| Value::String(id.clone()))
                    .collect();
                if ids.is_empty() {
                    r.meta.properties.remove(&name);
                } else {
                    r.meta.properties.insert(name.clone(), Value::Array(ids));
                }
            }
            converted += 1;
            changed = true;
        }
        if changed {
            let _ = ws.save_schema(db);
            let rows: Vec<String> = ws.dbs[db].rows.keys().cloned().collect();
            for id in rows {
                let _ = ws.save_row(db, &id);
            }
        }
    }
    converted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_newlines_and_delimiters() {
        let t = parse_csv("\u{feff}Name;Note\r\n\"a;b\";\"line1\nline2\"\r\nc;\"say \"\"hi\"\"\"\r\n");
        assert_eq!(t, vec![vec!["Name", "Note"], vec!["a;b", "line1\nline2"], vec!["c", "say \"hi\""]]);
        assert_eq!(parse_csv("a,b\n1,2")[1], vec!["1", "2"]);
        assert_eq!(parse_csv("a\tb\n1\t2")[1], vec!["1", "2"]);
        assert!(parse_csv("").is_empty());
    }

    #[test]
    fn dates_in_the_formats_exports_use() {
        assert_eq!(parse_date("2026-11-23").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("2026/11/23").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("November 23, 2026").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("Nov 3 2026").as_deref(), Some("2026-11-03"));
        assert_eq!(parse_date("23 novembre 2026").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("1er mars 2026").as_deref(), Some("2026-03-01"));
        assert_eq!(parse_date("23.11.2026").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("23/11/2026").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("11/23/2026").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("03/04/2026"), None, "ambiguous day and month stay text");
        assert_eq!(parse_date("November 23, 2026 → November 25, 2026").as_deref(), Some("2026-11-23"));
        assert_eq!(parse_date("November 23, 2026 9:30 AM").as_deref(), Some("2026-11-23T09:30"));
        assert_eq!(parse_date("2026-11-23T14:05:00Z").as_deref(), Some("2026-11-23T14:05"));
        assert_eq!(parse_date("hello"), None);
        assert_eq!(parse_date("2026-13-40"), None);
        assert_eq!(parse_date("May 5, 99999"), None);
    }

    #[test]
    fn a_yes_no_first_column_does_not_become_the_title() {
        let csv = "Présent,Nom,Lot\nYes,Ana,1\nNo,Bob,2\n";
        let dir = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(dir.path()).unwrap();
        let r = import_csv_database(&mut ws, None, "T", csv).unwrap();
        assert_eq!(r.rows[0].0, "Ana");
        let d = &ws.dbs[&r.db];
        assert_eq!(d.schema.properties[0].name, "Nom");
        assert!(d.schema.properties.iter().any(|p| p.name == "Présent" && p.kind == PropKind::Checkbox));
    }

    #[test]
    fn column_types_are_inferred() {
        assert_eq!(infer_kind(&["Yes", "No", "Yes"]), PropKind::Checkbox);
        assert_eq!(infer_kind(&["1", "2.5", "-3"]), PropKind::Number);
        assert_eq!(infer_kind(&["007", "12"]), PropKind::Text, "leading zeros keep their text");
        assert_eq!(infer_kind(&["March 3, 2026", "2026-04-01"]), PropKind::Date);
        assert_eq!(infer_kind(&["https://a.ch", "http://b.ch/x"]), PropKind::Url);
        assert_eq!(infer_kind(&["a@b.ch", "c@d.org"]), PropKind::Email);
        assert_eq!(infer_kind(&["Todo", "Done", "Todo", "Done"]), PropKind::Select);
        assert_eq!(infer_kind(&["a, b", "b, c", "a", "c, a"]), PropKind::MultiSelect);
        assert_eq!(infer_kind(&["one", "two", "three"]), PropKind::Text);
    }

    #[test]
    fn text_columns_naming_rows_of_another_database_become_relations() {
        let dir = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(dir.path()).unwrap();
        let people = import_csv_database(&mut ws, None, "People", "Name,Role\nAnn,Dev\nBob,Ops\n").unwrap();
        let tasks =
            import_csv_database(&mut ws, None, "Tasks", "Name,Owner,Status\nShip,\"Ann, Bob\",Todo\nTest,Ann,Done\n")
                .unwrap();
        let n = link_relations(&mut ws, &[people.db.clone(), tasks.db.clone()]);
        assert_eq!(n, 1, "Owner converts, Status and Role stay as they are");
        let d = &ws.dbs[&tasks.db];
        let owner = d.schema.properties.iter().find(|p| p.name == "Owner").unwrap();
        assert_eq!(owner.kind, PropKind::Relation);
        assert_eq!(owner.relation_db.as_deref(), Some(people.db.as_str()));
        let ship = d.rows.values().find(|r| r.meta.title == "Ship").unwrap();
        assert_eq!(ship.meta.properties["Owner"].as_array().unwrap().len(), 2);
        assert!(d.schema.properties.iter().any(|p| p.name == "Status" && p.kind != PropKind::Relation));
    }

    #[test]
    fn french_exports_numbers_and_dates_are_understood() {
        assert_eq!(parse_number("93,9"), Some(93.9));
        assert_eq!(parse_number("0,6"), Some(0.6));
        assert_eq!(parse_number("43"), Some(43.0));
        assert_eq!(parse_number("1,980"), None); // three digits after the comma: a thousands separator
        assert_eq!(parse_date("8 octobre 2026 10:31 (UTC+2)").as_deref(), Some("2026-10-08T10:31"));
        assert_eq!(parse_date("17 novembre 2026 20:18 (UTC+1)").as_deref(), Some("2026-11-17T20:18"));
        assert_eq!(parse_date("17 novembre 2026").as_deref(), Some("2026-11-17"));
    }
}
