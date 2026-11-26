//! Inline formatting inside a block's text, stored as Markdown:
//! `**bold**`, `*italic*`, `<u>underline</u>`, `~~strike~~` (or `~strike~`),
//! `` `code` ``, `[text](url)`, `[[Page title]]` mentions and coloured text
//! (`<span style="color:red">…</span>`).
//!
//! [`parse`] splits a text into segments, each either a *marker* (syntax the
//! editor dims or hides) or *content* with a style. Ranges are byte ranges.

use std::ops::Range;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
    pub mention: Option<String>,
    /// Text colour name (`<span style="color:red">…</span>`).
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub range: Range<usize>,
    pub marker: bool,
    pub style: Style,
}

fn push(out: &mut Vec<Segment>, range: Range<usize>, marker: bool, style: &Style) {
    if range.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.marker == marker && last.style == *style && last.range.end == range.start {
            last.range.end = range.end;
            return;
        }
    }
    out.push(Segment { range, marker, style: style.clone() });
}

/// Split `text` into styled segments covering every byte exactly once.
pub fn parse(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    parse_range(text, 0, text.len(), &Style::default(), &mut out);
    out
}

const COLOR_OPEN: &str = "<span style=\"color:";

fn find_from(text: &str, from: usize, end: usize, pat: &str) -> Option<usize> {
    text.get(from..end)?.find(pat).map(|i| i + from)
}

fn parse_range(text: &str, start: usize, end: usize, base: &Style, out: &mut Vec<Segment>) {
    let bytes = text.as_bytes();
    let mut style = base.clone();
    let mut i = start;
    while i < end {
        let rest = &text[i..end];
        let c = rest.chars().next().unwrap();
        let clen = c.len_utf8();
        // escapes
        if c == '\\' && i + 1 < end && (bytes[i + 1] as char).is_ascii_punctuation() {
            push(out, i..i + 1, true, &style);
            push(out, i + 1..i + 2, false, &style);
            i += 2;
            continue;
        }
        // inline code
        if c == '`' {
            if let Some(close) = find_from(text, i + 1, end, "`") {
                let mut s = style.clone();
                s.code = true;
                push(out, i..i + 1, true, &s);
                push(out, i + 1..close, false, &s);
                push(out, close..close + 1, true, &s);
                i = close + 1;
                continue;
            }
        }
        // mention
        if rest.starts_with("[[") {
            if let Some(close) = find_from(text, i + 2, end, "]]") {
                let title = &text[i + 2..close];
                if !title.is_empty() && !title.contains('\n') {
                    let mut s = style.clone();
                    s.mention = Some(title.to_string());
                    push(out, i..i + 2, true, &s);
                    push(out, i + 2..close, false, &s);
                    push(out, close..close + 2, true, &s);
                    i = close + 2;
                    continue;
                }
            }
        }
        // link [text](url)
        if c == '[' {
            if let Some(mid) = find_from(text, i + 1, end, "](") {
                if let Some(close) = find_from(text, mid + 2, end, ")") {
                    let url = &text[mid + 2..close];
                    if !url.contains(' ') && !text[i + 1..mid].contains('[') {
                        let mut s = style.clone();
                        s.link = Some(url.to_string());
                        push(out, i..i + 1, true, &s);
                        parse_range(text, i + 1, mid, &s, out);
                        push(out, mid..close + 1, true, &s);
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        // text colour
        if let Some(tail) = rest.strip_prefix(COLOR_OPEN) {
            if let Some(gt) = tail.find("\">") {
                let name = &tail[..gt];
                let open_len = COLOR_OPEN.len() + gt + 2;
                if !name.is_empty()
                    && name.len() <= 12
                    && name.chars().all(|c| c.is_ascii_lowercase())
                    && find_from(text, i + open_len, end, "</span>").is_some()
                {
                    style.color = Some(name.to_string());
                    push(out, i..i + open_len, true, &style);
                    i += open_len;
                    continue;
                }
            }
        }
        if rest.starts_with("</span>") && style.color != base.color {
            push(out, i..i + 7, true, &style);
            style.color = base.color.clone();
            i += 7;
            continue;
        }
        // underline
        if rest.starts_with("<u>") {
            style.underline = true;
            push(out, i..i + 3, true, &style);
            i += 3;
            continue;
        }
        if rest.starts_with("</u>") && style.underline {
            push(out, i..i + 4, true, &style);
            style.underline = base.underline;
            i += 4;
            continue;
        }
        // bold
        if rest.starts_with("**") {
            if style.bold && !base.bold {
                push(out, i..i + 2, true, &style);
                style.bold = false;
                i += 2;
                continue;
            } else if !style.bold && find_from(text, i + 2, end, "**").is_some() {
                style.bold = true;
                push(out, i..i + 2, true, &style);
                i += 2;
                continue;
            }
        }
        // strike ~~ or ~
        if c == '~' {
            let len = if rest.starts_with("~~") { 2 } else { 1 };
            let pat = &rest[..len];
            if style.strike && !base.strike {
                push(out, i..i + len, true, &style);
                style.strike = false;
                i += len;
                continue;
            } else if !style.strike && find_from(text, i + len, end, pat).is_some() {
                style.strike = true;
                push(out, i..i + len, true, &style);
                i += len;
                continue;
            }
        }
        // italic * or _
        if c == '*' || c == '_' {
            let prev_alnum = i > 0 && text[..i].chars().last().is_some_and(|p| p.is_alphanumeric());
            let next_alnum = rest[1..].chars().next().is_some_and(|n| n.is_alphanumeric());
            let pat = if c == '*' { "*" } else { "_" };
            if style.italic && !base.italic && (c == '*' || !next_alnum) {
                push(out, i..i + 1, true, &style);
                style.italic = false;
                i += 1;
                continue;
            } else if !style.italic
                && (c == '*' || !prev_alnum)
                && rest.len() > 1
                && !rest[1..].starts_with(' ')
                && find_from(text, i + 1, end, pat).is_some()
            {
                style.italic = true;
                push(out, i..i + 1, true, &style);
                i += 1;
                continue;
            }
        }
        push(out, i..i + clen, false, &style);
        i += clen;
    }
}

/// Visible text without markers.
pub fn strip(text: &str) -> String {
    parse(text).into_iter().filter(|s| !s.marker).map(|s| &text[s.range]).collect()
}

/// Titles of all `[[mentions]]` in a text.
pub fn mentions(text: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for s in parse(text) {
        if let Some(m) = s.style.mention {
            if !s.marker && !v.contains(&m) {
                v.push(m);
            }
        }
    }
    v
}

/// Replace `[[old]]` with `[[new]]` (case-insensitive on `old`). Returns the
/// new text when something changed.
pub fn rename_mentions(text: &str, old: &str, new: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        if let Some(close) = after.find("]]") {
            let inner = &after[..close];
            out.push_str(&rest[..start]);
            if inner.eq_ignore_ascii_case(old) || inner.to_lowercase() == old.to_lowercase() {
                out.push_str("[[");
                out.push_str(new);
                out.push_str("]]");
                changed = true;
            } else {
                out.push_str(&rest[start..start + 2 + close + 2]);
            }
            rest = &after[close + 2..];
        } else {
            break;
        }
    }
    out.push_str(rest);
    changed.then_some(out)
}

fn char_to_byte(text: &str, ci: usize) -> usize {
    text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len())
}

fn byte_to_char(text: &str, bi: usize) -> usize {
    text[..bi.min(text.len())].chars().count()
}

/// Toggle a formatting marker around a selection given in *char* indices.
/// Returns the new text and the new selection (chars).
pub fn toggle_wrap(text: &str, sel: (usize, usize), open: &str, close: &str) -> (String, (usize, usize)) {
    let (a, b) = if sel.0 <= sel.1 { sel } else { (sel.1, sel.0) };
    let ba = char_to_byte(text, a);
    let bb = char_to_byte(text, b);
    let ol = open.chars().count();
    let cl = close.chars().count();
    // markers just outside the selection → remove them
    if ba >= open.len()
        && text[..ba].ends_with(open)
        && text[bb..].starts_with(close)
        && !(open == "*" && text[..ba].ends_with("**") && !text[..ba].ends_with("***"))
    {
        let mut s = String::new();
        s.push_str(&text[..ba - open.len()]);
        s.push_str(&text[ba..bb]);
        s.push_str(&text[bb + close.len()..]);
        return (s, (a - ol, b - ol));
    }
    // markers just inside the selection → remove them
    let inner = &text[ba..bb];
    if inner.len() >= open.len() + close.len() && inner.starts_with(open) && inner.ends_with(close) {
        let mut s = String::new();
        s.push_str(&text[..ba]);
        s.push_str(&inner[open.len()..inner.len() - close.len()]);
        s.push_str(&text[bb..]);
        return (s, (a, b - ol - cl));
    }
    let mut s = String::new();
    s.push_str(&text[..ba]);
    s.push_str(open);
    s.push_str(inner);
    s.push_str(close);
    s.push_str(&text[bb..]);
    (s, (a + ol, b + ol))
}

/// The style at a char position (used to show active formatting).
pub fn style_at(text: &str, char_pos: usize) -> Style {
    let bi = char_to_byte(text, char_pos);
    for s in parse(text) {
        if s.range.contains(&bi) || (s.range.end == bi && bi == text.len()) {
            return s.style;
        }
    }
    Style::default()
}

/// Segment (content) under a char position, if it is a link or a mention.
pub fn link_at(text: &str, char_pos: usize) -> Option<Style> {
    let bi = char_to_byte(text, char_pos);
    parse(text)
        .into_iter()
        .find(|s| s.range.contains(&bi) && (s.style.link.is_some() || s.style.mention.is_some()))
        .map(|s| s.style)
}

/// Convert between char and byte offsets (exposed for the editor).
pub fn char_offset_to_byte(text: &str, ci: usize) -> usize {
    char_to_byte(text, ci)
}
pub fn byte_offset_to_char(text: &str, bi: usize) -> usize {
    byte_to_char(text, bi)
}

// ---------------------------------------------------------------------------
// WYSIWYG editing: markers are stored in the text but never shown, so the
// caret skips them, deleting never breaks them, and styles combine.

/// For each char of `text`: is it part of a formatting marker?
pub fn marker_mask(text: &str) -> Vec<bool> {
    let mut mask = Vec::with_capacity(text.len());
    let segs = parse(text);
    let mut seg = 0;
    for (bi, _) in text.char_indices() {
        while seg < segs.len() && segs[seg].range.end <= bi {
            seg += 1;
        }
        mask.push(segs.get(seg).is_some_and(|s| s.marker && s.range.contains(&bi)));
    }
    mask
}

/// Move the caret one visible character left or right, jumping over markers.
/// Moving right stops before closing markers, so typing continues the style
/// of the character to the left (as in most editors).
pub fn step_caret(text: &str, pos: usize, right: bool) -> usize {
    let mask = marker_mask(text);
    let n = mask.len();
    let mut p = pos.min(n);
    if right {
        while p < n && mask[p] {
            p += 1;
        }
        if p < n {
            p += 1;
        }
    } else {
        while p > 0 && mask[p - 1] {
            p -= 1;
        }
        p = p.saturating_sub(1);
    }
    p
}

/// Move the caret one word left or right (⌥← ⌥→), as if the markers were not there.
/// Going right it ends before closing markers, like `step_caret`.
pub fn step_word(text: &str, pos: usize, right: bool) -> usize {
    let mask = marker_mask(text);
    let ch: Vec<char> = text.chars().collect();
    let n = mask.len();
    let word = |i: usize| ch[i].is_alphanumeric() || ch[i] == '_';
    let mut p = pos.min(n);
    if right {
        while p < n && (mask[p] || !word(p)) {
            p += 1;
        }
        while p < n && (mask[p] || word(p)) {
            p += 1;
        }
        while p > pos && p > 0 && mask[p - 1] {
            p -= 1;
        }
    } else {
        while p > 0 && (mask[p - 1] || !word(p - 1)) {
            p -= 1;
        }
        while p > 0 && (mask[p - 1] || word(p - 1)) {
            p -= 1;
        }
        while p < n && mask[p] {
            p += 1;
        }
    }
    p
}

/// A caret strictly inside a run of markers moves to the end of the run.
pub fn snap_caret(text: &str, pos: usize) -> usize {
    let mask = marker_mask(text);
    let n = mask.len();
    let mut p = pos.min(n);
    if p > 0 && p < n && mask[p - 1] && mask[p] {
        while p < n && mask[p] {
            p += 1;
        }
    }
    p
}

/// (opener, closer) pairs removed when nothing is left between them.
const EMPTY_PAIRS: [(&str, &str); 7] =
    [("<u>", "</u>"), ("**", "**"), ("~~", "~~"), ("`", "`"), ("*", "*"), ("~", "~"), ("_", "_")];

fn remove_empty_pairs(chars: &mut Vec<char>, at: usize) -> usize {
    // returns how many chars were removed before `at`
    let mut at = at;
    let mut removed_before = 0;
    loop {
        let mut changed = false;
        for (o, c) in EMPTY_PAIRS {
            let ol = o.chars().count();
            let cl = c.chars().count();
            if at >= ol && at + cl <= chars.len() {
                let left: String = chars[at - ol..at].iter().collect();
                let right: String = chars[at..at + cl].iter().collect();
                if left == o && right == c {
                    chars.drain(at..at + cl);
                    chars.drain(at - ol..at);
                    at -= ol;
                    removed_before += ol;
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            return removed_before;
        }
    }
}

/// Delete the visible character before (`backward`) or after the caret,
/// keeping the formatting markers around it and dropping formats that end
/// up empty. Returns the new text and caret, or `None` when there is nothing
/// visible to delete on that side.
pub fn delete_visible(text: &str, pos: usize, backward: bool) -> Option<(String, usize)> {
    let mask = marker_mask(text);
    let mut chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let pos = pos.min(n);
    if backward {
        let mut q = pos;
        while q > 0 && mask[q - 1] {
            q -= 1;
        }
        if q == 0 {
            return None;
        }
        chars.remove(q - 1);
        let removed = remove_empty_pairs(&mut chars, q - 1);
        let caret = if removed > 0 { q - 1 - removed } else { pos - 1 };
        Some((chars.into_iter().collect(), caret))
    } else {
        let mut q = pos;
        while q < n && mask[q] {
            q += 1;
        }
        if q == n {
            return None;
        }
        chars.remove(q);
        let removed = remove_empty_pairs(&mut chars, q);
        Some((chars.into_iter().collect(), pos.min(q - removed)))
    }
}

/// Every colour tag still open at the end of `s`, outermost first.
fn open_color_tags(s: &str) -> Vec<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if rest.starts_with(COLOR_OPEN) {
            if let Some(gt) = rest.find("\">") {
                stack.push(rest[..gt + 2].to_string());
                i += gt + 2;
                continue;
            }
        }
        if rest.starts_with("</span>") {
            stack.pop();
            i += 7;
            continue;
        }
        i += rest.chars().next().map_or(1, |c| c.len_utf8());
    }
    stack
}

/// `text` without any colour span (the tags go, the words stay).
pub fn strip_colors(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix(COLOR_OPEN) {
            if let Some(gt) = tail.find("\">") {
                rest = &tail[gt + 2..];
                continue;
            }
        }
        if let Some(tail) = rest.strip_prefix("</span>") {
            rest = tail;
            continue;
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Colour spans with nothing inside.
fn drop_empty_spans(s: &str) -> String {
    let mut s = s.to_string();
    while let Some(at) = s.find(COLOR_OPEN) {
        let Some(gt) = s[at..].find("\">") else { break };
        let end = at + gt + 2;
        if s[end..].starts_with("</span>") {
            s.replace_range(at..end + 7, "");
        } else {
            break;
        }
    }
    s
}

/// Moves the edges of a byte range out of the colour tags they fall inside: the start goes back to the
/// start of the tag, the end forward to the end of it.
fn snap_out_of_color_tags(text: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let len = if rest.starts_with(COLOR_OPEN) {
            rest.find("\">").map(|g| g + 2)
        } else if rest.starts_with("</span>") {
            Some(7)
        } else {
            None
        };
        match len {
            Some(l) => {
                let (t0, t1) = (at, at + l);
                if start > t0 && start < t1 {
                    start = t0;
                }
                if end > t0 && end < t1 {
                    end = t1;
                }
                at = t1;
            }
            None => at += rest.chars().next().map_or(1, |c| c.len_utf8()),
        }
    }
    (start, end.max(start))
}

/// Colour (a name such as `red`, or `None` for the default) on a selection of chars.
/// Colours already inside the selection are replaced; a span that the selection cuts is
/// closed before it and reopened after it. Returns the new text and selection.
pub fn set_color(text: &str, sel: (usize, usize), color: Option<&str>) -> (String, (usize, usize)) {
    let (a, b) = if sel.0 <= sel.1 { sel } else { (sel.1, sel.0) };
    // a selection edge never cuts through a colour tag: it moves out of it
    let (ba, bb) = snap_out_of_color_tags(text, char_to_byte(text, a), char_to_byte(text, b));
    let mut before = text[..ba].to_string();
    let mut after = text[bb..].to_string();
    let inner = strip_colors(&text[ba..bb]);
    // spans open before the selection stop at it; spans still open at its end start again after it
    // (the selection may cut several nested spans)
    let mut stack = open_color_tags(&before);
    for _ in 0..stack.len() {
        before.push_str("</span>");
    }
    let mut rest = &text[ba..bb];
    while !rest.is_empty() {
        if rest.starts_with(COLOR_OPEN) {
            if let Some(gt) = rest.find("\">") {
                stack.push(rest[..gt + 2].to_string());
                rest = &rest[gt + 2..];
                continue;
            }
        } else if rest.starts_with("</span>") {
            stack.pop();
            rest = &rest[7..];
            continue;
        }
        rest = &rest[rest.chars().next().map_or(1, |c| c.len_utf8())..];
    }
    after = format!("{}{after}", stack.concat());
    let before = drop_empty_spans(&before);
    let after = drop_empty_spans(&after);
    let (open, close) = match color {
        Some(c) if !inner.is_empty() => (format!("{COLOR_OPEN}{c}\">"), "</span>".to_string()),
        _ => (String::new(), String::new()),
    };
    let start = before.chars().count() + open.chars().count();
    let end = start + inner.chars().count();
    (format!("{before}{open}{inner}{close}{after}"), (start, end))
}

/// Inline styles that can be toggled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
}

impl StyleKind {
    fn markers(self) -> (&'static str, &'static str) {
        match self {
            StyleKind::Bold => ("**", "**"),
            StyleKind::Italic => ("*", "*"),
            StyleKind::Underline => ("<u>", "</u>"),
            StyleKind::Strike => ("~~", "~~"),
            StyleKind::Code => ("`", "`"),
        }
    }
    fn active(self, s: &Style) -> bool {
        match self {
            StyleKind::Bold => s.bold,
            StyleKind::Italic => s.italic,
            StyleKind::Underline => s.underline,
            StyleKind::Strike => s.strike,
            StyleKind::Code => s.code,
        }
    }
}

const TOKENS: [&str; 10] = ["</u>", "<u>", "**", "~~", "[[", "]]", "*", "~", "`", "_"];

fn token_kind(t: &str) -> Option<StyleKind> {
    Some(match t {
        "**" => StyleKind::Bold,
        "*" | "_" => StyleKind::Italic,
        "~~" | "~" => StyleKind::Strike,
        "`" => StyleKind::Code,
        "<u>" | "</u>" => StyleKind::Underline,
        _ => return None,
    })
}

/// Split a run of marker chars into tokens: (start char, token).
fn tokenize(chars: &[char], from: usize, to: usize) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = from;
    while i < to {
        let rest: String = chars[i..to].iter().collect();
        let t = TOKENS
            .iter()
            .find(|t| rest.starts_with(**t))
            .map(|t| t.to_string())
            .unwrap_or_else(|| chars[i].to_string());
        let len = t.chars().count();
        out.push((i, t));
        i += len;
    }
    out
}

/// Toggle a style on a selection (char indices), combining with the styles
/// already there: bold + italic + strike + code nest correctly, code always
/// innermost. Returns the new text and selection.
pub fn toggle_style(text: &str, sel: (usize, usize), kind: StyleKind) -> (String, (usize, usize)) {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let (a, b) = if sel.0 <= sel.1 { (sel.0.min(n), sel.1.min(n)) } else { (sel.1.min(n), sel.0.min(n)) };
    let (open, close) = kind.markers();
    let (ol, cl) = (open.chars().count(), close.chars().count());
    let mask = marker_mask(text);
    let splice = |parts: &[(usize, usize, &str)]| -> String {
        // parts: (from, to, replacement) sorted, non-overlapping
        let mut out = String::new();
        let mut i = 0;
        for (f, t, r) in parts {
            out.extend(&chars[i..*f]);
            out.push_str(r);
            i = *t;
        }
        out.extend(&chars[i..]);
        out
    };
    if a == b {
        let t = splice(&[(a, a, &format!("{open}{close}"))]);
        return (t, (a + ol, a + ol));
    }
    let visible: Vec<usize> = (a..b).filter(|i| !mask[*i]).collect();
    let (Some(&first), Some(&last)) = (visible.first(), visible.last()) else {
        let t = splice(&[(a, a, open), (b, b, close)]);
        return (t, (a + ol, b + ol));
    };
    let first_style = style_at(text, first);
    let last_style = style_at(text, last);
    // marker runs touching the selection, limited to markers that belong to it
    let mut ls = a;
    while ls > 0 && mask[ls - 1] {
        ls -= 1;
    }
    let mut re = b;
    while re < n && mask[re] {
        re += 1;
    }
    let left: Vec<(usize, String)> = tokenize(&chars, ls, a);
    let right: Vec<(usize, String)> = tokenize(&chars, b, re);
    let mut left_in: Vec<(usize, String)> = Vec::new();
    for (p, t) in left.iter().rev() {
        match token_kind(t) {
            Some(k) if k.active(&first_style) => left_in.insert(0, (*p, t.clone())),
            _ => break,
        }
    }
    let mut right_in: Vec<(usize, String)> = Vec::new();
    for (p, t) in right.iter() {
        match token_kind(t) {
            Some(k) if k.active(&last_style) => right_in.push((*p, t.clone())),
            _ => break,
        }
    }
    let active = kind.active(&first_style) && kind.active(&last_style);
    if active {
        let lo = left_in
            .iter()
            .rev()
            .find(|(_, t)| token_kind(t) == Some(kind) && (kind != StyleKind::Underline || t == "<u>"));
        let rc =
            right_in.iter().find(|(_, t)| token_kind(t) == Some(kind) && (kind != StyleKind::Underline || t == "</u>"));
        if let (Some((lp, lt)), Some((rp, rt))) = (lo, rc) {
            let llen = lt.chars().count();
            let rlen = rt.chars().count();
            let t = splice(&[(*lp, lp + llen, ""), (*rp, rp + rlen, "")]);
            return (t, (a - llen, b - llen));
        }
        // the style comes from further away: split it around the selection
        let t = splice(&[(a, a, close), (b, b, open)]);
        return (t, (a + cl, b + cl));
    }
    let (l, r) = if kind == StyleKind::Code {
        (a, b)
    } else {
        (
            left_in.first().map(|(p, _)| *p).unwrap_or(a),
            right_in.last().map(|(p, t)| p + t.chars().count()).unwrap_or(b),
        )
    };
    let t = splice(&[(l, l, open), (r, r, close)]);
    (t, (a + ol, b + ol))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(text: &str) -> Vec<(String, Style)> {
        parse(text).into_iter().filter(|s| !s.marker).map(|s| (text[s.range].to_string(), s.style)).collect()
    }

    #[test]
    fn segments_cover_text() {
        let t = "a **b** *c* `d` [e](http://x) [[F]] <u>g</u> ~~h~~ ~i~ \\*j";
        let segs = parse(t);
        let mut pos = 0;
        for s in &segs {
            assert_eq!(s.range.start, pos);
            pos = s.range.end;
        }
        assert_eq!(pos, t.len());
    }

    #[test]
    fn styles_are_detected() {
        let c = content("**b** *i* `c` ~~s~~ ~t~ <u>u</u>");
        let find = |w: &str| c.iter().find(|(t, _)| t == w).unwrap().1.clone();
        assert!(find("b").bold);
        assert!(find("i").italic);
        assert!(find("c").code);
        assert!(find("s").strike);
        assert!(find("t").strike);
        assert!(find("u").underline);
    }

    #[test]
    fn links_and_mentions() {
        let c = content("see [site](https://a.b) and [[My Page]]");
        assert!(c.iter().any(|(t, s)| t == "site" && s.link.as_deref() == Some("https://a.b")));
        assert!(c.iter().any(|(t, s)| t == "My Page" && s.mention.as_deref() == Some("My Page")));
        assert_eq!(mentions("[[A]] [[B]] [[A]]"), vec!["A", "B"]);
    }

    #[test]
    fn stray_markers_are_literal() {
        assert_eq!(strip("2 * 3 = 6"), "2 * 3 = 6");
        assert_eq!(strip("snake_case_name"), "snake_case_name");
        assert_eq!(strip("**bold** text"), "bold text");
    }

    #[test]
    fn toggle_wrap_adds_and_removes() {
        let (t, sel) = toggle_wrap("hello world", (6, 11), "**", "**");
        assert_eq!(t, "hello **world**");
        assert_eq!(sel, (8, 13));
        let (t2, sel2) = toggle_wrap(&t, sel, "**", "**");
        assert_eq!(t2, "hello world");
        assert_eq!(sel2, (6, 11));
        let (t3, _) = toggle_wrap("é", (0, 1), "`", "`");
        assert_eq!(t3, "`é`");
    }

    #[test]
    fn rename_mentions_works() {
        assert_eq!(rename_mentions("go to [[Old]] now", "old", "New").as_deref(), Some("go to [[New]] now"));
        assert_eq!(rename_mentions("nothing", "a", "b"), None);
    }

    fn styles_of(t: &str) -> Vec<(String, Style)> {
        content(t)
    }

    #[test]
    fn caret_skips_hidden_markers() {
        let t = "a**b**c";
        assert_eq!(marker_mask(t), vec![false, true, true, false, true, true, false]);
        assert_eq!(step_caret(t, 1, true), 4, "over the opener and onto after b");
        assert_eq!(step_caret(t, 4, true), 7);
        assert_eq!(step_caret(t, 7, false), 6);
        assert_eq!(step_caret(t, 3, false), 0);
        assert_eq!(snap_caret(t, 2), 3);
        assert_eq!(snap_caret(t, 3), 3);
    }

    #[test]
    fn word_steps_skip_hidden_markers_and_link_targets() {
        let t = "see [the docs](https://x.org/a/b) now";
        let len = t.chars().count();
        // going right: end of "see", end of "docs" (before the hidden target), end of "now"
        let a = step_word(t, 0, true);
        assert_eq!(a, 3);
        let b = step_word(t, a, true);
        assert_eq!(&t[..b], "see [the");
        let c = step_word(t, b, true);
        assert_eq!(&t[..c], "see [the docs");
        let d = step_word(t, c, true);
        assert_eq!(d, len);
        // going left from the end walks back over the same words and never gets stuck in the target
        let mut p = len;
        let mut seen = vec![p];
        for _ in 0..6 {
            let q = step_word(t, p, false);
            if q == p {
                break;
            }
            p = q;
            seen.push(p);
        }
        assert_eq!(*seen.last().unwrap(), 0, "{seen:?}");
        assert!(seen.len() <= 5, "{seen:?}");
        // bold markers are transparent too
        assert_eq!(step_word("a **bc** d", 0, true), 1);
        assert_eq!(step_word("a **bc** d", 1, true), 6, "end of bc, before the closing markers");
    }

    #[test]
    fn deleting_keeps_formats_intact() {
        assert_eq!(delete_visible("**ab**", 4, true), Some(("**a**".to_string(), 3)));
        assert_eq!(delete_visible("x**b**", 6, true), Some(("x".to_string(), 1)), "empty bold removed");
        assert_eq!(delete_visible("**b**", 2, true), None);
        assert_eq!(delete_visible("a**b**", 1, false), Some(("a".to_string(), 1)));
        assert_eq!(delete_visible("<u>x</u>y", 4, true), Some(("y".to_string(), 0)));
    }

    #[test]
    fn styles_stack_and_toggle_off() {
        let mut t = "hello".to_string();
        let mut sel = (0, 5);
        for k in [StyleKind::Bold, StyleKind::Italic, StyleKind::Code, StyleKind::Strike, StyleKind::Underline] {
            let (nt, ns) = toggle_style(&t, sel, k);
            t = nt;
            sel = ns;
            assert_eq!(strip(&t), "hello", "{t}");
        }
        let c = styles_of(&t);
        assert_eq!(c.len(), 1, "{t}: {c:?}");
        let s = &c[0].1;
        assert!(s.bold && s.italic && s.code && s.strike && s.underline, "{t}");
        // and off again, one by one
        let (t2, sel2) = toggle_style(&t, sel, StyleKind::Bold);
        let s2 = &styles_of(&t2)[0].1;
        assert!(!s2.bold && s2.italic && s2.code && s2.strike && s2.underline, "{t2}");
        let (t3, _) = toggle_style(&t2, sel2, StyleKind::Code);
        let s3 = &styles_of(&t3)[0].1;
        assert!(!s3.code && s3.italic, "{t3}");
        assert_eq!(strip(&t3), "hello");
    }

    #[test]
    fn bold_around_code_not_inside() {
        let (t, sel) = toggle_style("`x`", (1, 2), StyleKind::Bold);
        assert_eq!(t, "**`x`**");
        assert_eq!(sel, (3, 4));
        let (t, _) = toggle_style("**a**b", (5, 6), StyleKind::Italic);
        assert_eq!(t, "**a***b*");
        let c = styles_of(&t);
        assert!(c[0].1.bold && !c[0].1.italic && c[1].1.italic && !c[1].1.bold, "{t} {c:?}");
    }

    #[test]
    fn partial_toggle_off_splits() {
        let (t, _) = toggle_style("**abc**", (3, 4), StyleKind::Bold);
        assert_eq!(strip(&t), "abc");
        let c = styles_of(&t);
        assert!(c.iter().any(|(s, st)| s == "b" && !st.bold), "{t}");
        assert!(c.iter().any(|(s, st)| s == "a" && st.bold), "{t}");
    }

    #[test]
    fn underline_is_parsed() {
        let c = styles_of("<u>under</u>");
        assert_eq!(c.len(), 1);
        assert!(c[0].1.underline);
    }

    #[test]
    fn text_colour_round_trips_and_changes() {
        let (t, sel) = set_color("hello big world", (6, 9), Some("red"));
        assert_eq!(t, "hello <span style=\"color:red\">big</span> world");
        assert_eq!(strip(&t), "hello big world");
        let seg = parse(&t);
        assert!(seg
            .iter()
            .any(|s| !s.marker && s.style.color.as_deref() == Some("red") && &t[s.range.clone()] == "big"));
        // selection is exactly the coloured text: change it, then reset it
        let (t2, sel2) = set_color(&t, sel, Some("blue"));
        assert_eq!(t2, "hello <span style=\"color:blue\">big</span> world");
        let (t3, _) = set_color(&t2, sel2, None);
        assert_eq!(t3, "hello big world");
        // colouring over a bold word keeps the bold markers
        let (t4, _) = set_color("a **b** c", (2, 7), Some("green"));
        assert_eq!(t4, "a <span style=\"color:green\">**b**</span> c");
        // an unclosed or odd tag is plain text, never a panic
        let _ = parse("<span style=\"color:red\">no close");
        let _ = parse("<span style=\"color:RED!\">x</span>");
        let _ = set_color("", (0, 0), Some("red"));
    }

    #[test]
    fn a_link_wrapped_in_extra_brackets_is_still_a_link() {
        let t = "Pieces on [[KDRIVE](https://k.example/s/1)] - updated 18 June";
        assert!(mentions(t).is_empty(), "{:?}", mentions(t));
        let shown = strip(t);
        assert!(shown.contains("KDRIVE") && !shown.contains("https"), "{shown}");
        let links: Vec<_> = parse(t).into_iter().filter(|s| s.style.link.is_some()).collect();
        assert!(
            !links.is_empty(),
            "no link found in {:?}",
            parse(t).iter().map(|s| (&t[s.range.clone()], s.marker)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn recolouring_a_line_replaces_the_colours_inside_and_keeps_tags_balanced() {
        let t = "a <span style=\"color:red\">b c</span> d";
        let (out, _) = set_color(t, (0, 27), Some("blue"));
        assert_eq!(out.matches("<span").count(), out.matches("</span>").count());
        assert_eq!(out, "<span style=\"color:blue\">a b</span><span style=\"color:red\"> c</span> d");
        let (all, _) = set_color(t, (0, t.chars().count()), Some("green"));
        assert_eq!(all, "<span style=\"color:green\">a b c d</span>");
        assert_eq!(strip_colors(t), "a b c d");
    }
}
