//! Blocks: the editor's unit of content, and their Markdown form.
//!
//! A page body is a flat list of blocks; nesting is expressed by `depth`
//! (children of a list item or toggle have a greater depth). The Markdown
//! dialect stays readable on GitHub:
//!
//! | block      | Markdown                                    |
//! |------------|---------------------------------------------|
//! | paragraph  | `text`                                      |
//! | headings   | `# `, `## `, `### `                         |
//! | bulleted   | `- item`                                    |
//! | numbered   | `1. item`                                   |
//! | to-do      | `- [ ] task` / `- [x] done`                 |
//! | toggle     | `<details><summary>T</summary>` … `</details>` |
//! | quote      | `> text`                                    |
//! | callout    | `> [!NOTE]` then `> 💡 text`                |
//! | code       | fenced with three backticks and a language  |
//! | divider    | `---`                                       |
//! | image      | `![caption](assets/pic.png)`                |
//! | file       | `[📎 name](assets/doc.pdf)`                 |

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_BLOCK: AtomicU64 = AtomicU64::new(1);

/// A fresh in-memory block id (not persisted; used by the UI for widget ids).
pub fn next_block_id() -> u64 {
    NEXT_BLOCK.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Bulleted,
    Numbered,
    Todo {
        checked: bool,
    },
    Toggle {
        open: bool,
    },
    Quote,
    Callout {
        icon: String,
    },
    Code {
        lang: String,
    },
    Divider,
    /// Table of contents: lists the page's headings (saved as `<!--toc-->`).
    Toc,
    /// A meeting recorded and transcribed on this Mac (saved as `<!--meeting:Title-->`).
    /// `text` is the title; the notes and the transcript are ordinary blocks that follow it.
    Meeting,
    /// A database shown inside the page (saved as `<!--database:Title-->`); `text` is its title.
    Database,
    /// A simple table (saved as a Markdown table); `text` holds its rows, see `table_cells`.
    Table,
    /// A web link shown as a card (saved as `<!--bookmark:URL Title-->`); `text` is the page title.
    Bookmark {
        url: String,
    },
    /// A LaTeX equation shown on its own line (saved between `$$` lines); `text` is the LaTeX.
    Equation,
    /// Start, separator and end of a side-by-side layout (`<!--columns-->`, `<!--column-->`, `<!--/columns-->`).
    ColumnsStart,
    ColumnBreak,
    ColumnsEnd,
    /// `text` is the caption.
    Image {
        path: String,
    },
    /// `text` is the displayed name.
    File {
        path: String,
    },
}

impl BlockKind {
    /// Stable key used by the slash menu and translations.
    pub fn key(&self) -> &'static str {
        match self {
            BlockKind::Paragraph => "text",
            BlockKind::Heading(1) => "h1",
            BlockKind::Heading(2) => "h2",
            BlockKind::Heading(_) => "h3",
            BlockKind::Bulleted => "bulleted",
            BlockKind::Numbered => "numbered",
            BlockKind::Todo { .. } => "todo",
            BlockKind::Toggle { .. } => "toggle",
            BlockKind::Quote => "quote",
            BlockKind::Callout { .. } => "callout",
            BlockKind::Code { .. } => "code",
            BlockKind::Divider => "divider",
            BlockKind::Toc => "toc",
            BlockKind::Meeting => "meeting",
            BlockKind::Database => "dbview",
            BlockKind::Table => "simple_table",
            BlockKind::Bookmark { .. } => "bookmark",
            BlockKind::Equation => "equation",
            BlockKind::ColumnsStart => "columns_start",
            BlockKind::ColumnBreak => "column_break",
            BlockKind::ColumnsEnd => "columns_end",
            BlockKind::Image { .. } => "image",
            BlockKind::File { .. } => "file",
        }
    }

    /// Lists continue on Enter (a new item of the same kind).
    pub fn continues_on_enter(&self) -> bool {
        matches!(self, BlockKind::Bulleted | BlockKind::Numbered | BlockKind::Todo { .. } | BlockKind::Toggle { .. })
    }

    /// Blocks whose text is edited as rich text (not code, divider, media).
    pub fn has_rich_text(&self) -> bool {
        !matches!(
            self,
            BlockKind::Code { .. }
                | BlockKind::Divider
                | BlockKind::Toc
                | BlockKind::Meeting
                | BlockKind::Database
                | BlockKind::Table
                | BlockKind::Bookmark { .. }
                | BlockKind::Equation
                | BlockKind::Image { .. }
                | BlockKind::File { .. }
        )
    }

    fn is_list(&self) -> bool {
        matches!(self, BlockKind::Bulleted | BlockKind::Numbered | BlockKind::Todo { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// In-memory id (not saved).
    pub id: u64,
    pub kind: BlockKind,
    pub text: String,
    pub depth: u8,
    /// Text colour (`red`) or background colour (`bg-red`) of the whole block; empty for none.
    /// Saved as a trailing `<!--c:red-->` comment, which other Markdown tools ignore.
    pub color: String,
    /// Width of an image as a percentage of its column (10 to 99); 0 for the natural size.
    /// Saved as a trailing `<!--w:50-->` comment, which other Markdown tools ignore.
    pub width: u16,
    /// Where an image sits in its column: 0 centre, 1 left, 2 right. Saved in the same comment as the width.
    pub align: u8,
}

/// Colour names a block can take; each also exists with a `bg-` prefix for the background.
pub const BLOCK_COLORS: [&str; 9] = ["gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red"];

/// Whether `name` is a colour a block can carry: `red`, `bg-red`, or both together (`red bg-blue`,
/// one text colour and one background).
pub fn is_block_color(name: &str) -> bool {
    let parts: Vec<&str> = name.split(' ').collect();
    let valid = |n: &str| BLOCK_COLORS.contains(&n.strip_prefix("bg-").unwrap_or(n));
    match parts.as_slice() {
        [a] => valid(a),
        [a, b] => valid(a) && valid(b) && a.starts_with("bg-") != b.starts_with("bg-"),
        _ => false,
    }
}

/// Set the text colour (`bg` false) or the background (`bg` true) of a block colour and keep the
/// other one. `pick` is a colour name (`red`, `bg-red`) or empty to clear that part.
pub fn merge_color(current: &str, pick: &str, bg: bool) -> String {
    let (mut text, mut back) = (String::new(), String::new());
    for p in current.split_whitespace() {
        if p.starts_with("bg-") {
            back = p.to_string();
        } else {
            text = p.to_string();
        }
    }
    if bg {
        back = pick.to_string();
    } else {
        text = pick.to_string();
    }
    [text, back].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
}

impl Block {
    pub fn new(kind: BlockKind, text: impl Into<String>) -> Self {
        Block { id: next_block_id(), kind, text: text.into(), depth: 0, color: String::new(), width: 0, align: 0 }
    }
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Paragraph, text)
    }
    pub fn with_depth(mut self, depth: u8) -> Self {
        self.depth = depth;
        self
    }
}

/// For a `ColumnsStart` at `start`: the index of its matching `ColumnsEnd` and the
/// first index of each column (the block after the start and after each break).
pub fn column_layout(blocks: &[Block], start: usize) -> Option<(usize, Vec<usize>)> {
    if blocks.get(start)?.kind != BlockKind::ColumnsStart {
        return None;
    }
    let mut firsts = vec![start + 1];
    for (k, b) in blocks.iter().enumerate().skip(start + 1) {
        match b.kind {
            BlockKind::ColumnBreak => firsts.push(k + 1),
            BlockKind::ColumnsEnd => return Some((k, firsts)),
            BlockKind::ColumnsStart => return None,
            _ => {}
        }
    }
    None
}

/// Maximum nesting depth.
pub const MAX_DEPTH: u8 = 8;

/// The number shown in front of a numbered item (1-based), counting the run of
/// numbered siblings at the same depth.
pub fn numbered_index(blocks: &[Block], i: usize) -> usize {
    let depth = blocks[i].depth;
    let mut n = 1;
    let mut j = i;
    while j > 0 {
        j -= 1;
        let b = &blocks[j];
        if b.depth > depth {
            continue;
        }
        if b.depth < depth || b.kind != BlockKind::Numbered {
            break;
        }
        n += 1;
    }
    n
}

/// Indices of the blocks hidden because a collapsed toggle above them owns them.
pub fn hidden_by_toggles(blocks: &[Block]) -> Vec<bool> {
    let mut hidden = vec![false; blocks.len()];
    let mut closed_at: Option<u8> = None;
    for (i, b) in blocks.iter().enumerate() {
        if let Some(d) = closed_at {
            if b.depth > d {
                hidden[i] = true;
                continue;
            }
            closed_at = None;
        }
        if let BlockKind::Toggle { open: false } = b.kind {
            closed_at = Some(b.depth);
        }
    }
    hidden
}

/// Index range `[i, end)` of a block and all its children (deeper blocks right after it).
pub fn subtree_end(blocks: &[Block], i: usize) -> usize {
    let d = blocks[i].depth;
    let mut end = i + 1;
    while end < blocks.len() && blocks[end].depth > d {
        end += 1;
    }
    end
}

/// Move the block at `from` (with its children) so it lands before index `to`
/// (an index in the original list, `0..=len`). Returns the new index of the block.
pub fn move_block(blocks: &mut Vec<Block>, from: usize, to: usize) -> usize {
    let end = subtree_end(blocks, from);
    if to >= from && to <= end {
        return from;
    }
    let chunk: Vec<Block> = blocks.drain(from..end).collect();
    let n = chunk.len();
    let insert_at = if to > from { to - n } else { to };
    // Children keep their depth relative to the moved block; the moved block
    // takes the depth of its new neighbourhood (never deeper than the block above + 1).
    let base = chunk[0].depth;
    let above_depth = if insert_at == 0 { None } else { Some(blocks[insert_at - 1].depth) };
    let new_base = match above_depth {
        None => 0,
        Some(d) => base.min(d + 1),
    };
    for (k, mut b) in chunk.into_iter().enumerate() {
        b.depth = b.depth - base + new_base;
        blocks.insert(insert_at + k, b);
    }
    insert_at
}

/// Move the blocks `lo..hi` (a selection, each keeping its depth) so they land before index `to`
/// (an index in the original list). Returns the new index of the first one.
pub fn move_range(blocks: &mut Vec<Block>, lo: usize, hi: usize, to: usize) -> usize {
    if to >= lo && to <= hi {
        return lo;
    }
    let chunk: Vec<Block> = blocks.drain(lo..hi).collect();
    let insert_at = if to > lo { to - chunk.len() } else { to };
    for (k, b) in chunk.into_iter().enumerate() {
        blocks.insert(insert_at + k, b);
    }
    insert_at
}

fn is_column_marker(k: &BlockKind) -> bool {
    matches!(k, BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd)
}

/// Drop block `from` (with its children) beside block `onto`, on its right: both end up side by side
/// as columns, or, when `onto` is already in columns, the block becomes a new column after the one
/// that holds `onto`. Returns the id of the moved block; nothing changes when it cannot be done.
pub fn drop_beside(blocks: &mut Vec<Block>, from: usize, onto: usize) -> Option<u64> {
    let end = subtree_end(blocks, from);
    if (from..end).contains(&onto) || is_column_marker(&blocks[onto].kind) || is_column_marker(&blocks[from].kind) {
        return None;
    }
    let id = blocks[from].id;
    let mut v = blocks.clone();
    let mut chunk: Vec<Block> = v.drain(from..end).collect();
    let onto = if onto > from { onto - chunk.len() } else { onto };
    let enclosing =
        (0..=onto).rev().find_map(|s| column_layout(&v, s).filter(|(e, _)| *e > onto).map(|(e, f)| (s, e, f)));
    let depth = match &enclosing {
        Some((s, ..)) => v[*s].depth,
        None => v[onto].depth,
    };
    let base = chunk[0].depth;
    for b in &mut chunk {
        b.depth = b.depth - base + depth;
    }
    match enclosing {
        Some((_, e, firsts)) => {
            if firsts.len() >= 5 {
                return None;
            }
            let c = firsts.iter().rposition(|f| *f <= onto).unwrap_or(0);
            let last = firsts.get(c + 1).map(|f| f - 1).unwrap_or(e);
            let mut ins = vec![Block::new(BlockKind::ColumnBreak, "").with_depth(depth)];
            ins.extend(chunk);
            v.splice(last..last, ins);
        }
        None => {
            let re = subtree_end(&v, onto);
            let mut ins = vec![Block::new(BlockKind::ColumnBreak, "").with_depth(depth)];
            ins.extend(chunk);
            ins.push(Block::new(BlockKind::ColumnsEnd, "").with_depth(depth));
            v.splice(re..re, ins);
            v.insert(onto, Block::new(BlockKind::ColumnsStart, "").with_depth(depth));
            // something to click on after the columns
            let end_at = v.iter().rposition(|b| b.kind == BlockKind::ColumnsEnd).unwrap_or(0);
            if end_at + 1 == v.len() {
                v.push(Block::paragraph("").with_depth(depth));
            }
        }
    }
    tidy_columns(&mut v);
    *blocks = v;
    Some(id)
}

/// Clean up columns after a block left one: an empty column goes away, and columns left with a
/// single column are unwrapped.
pub fn tidy_columns(blocks: &mut Vec<Block>) {
    let mut s = 0;
    while s < blocks.len() {
        let Some((e, firsts)) = column_layout(blocks, s) else {
            s += 1;
            continue;
        };
        let spans: Vec<(usize, usize)> =
            firsts.iter().enumerate().map(|(c, f)| (*f, firsts.get(c + 1).map(|x| x - 1).unwrap_or(e))).collect();
        let full = spans.iter().filter(|(a, b)| b > a).count();
        if full == spans.len() && full >= 2 {
            s = e + 1;
            continue;
        }
        let mut remove: Vec<usize> = Vec::new();
        if full >= 2 {
            for (c, (a, b)) in spans.iter().enumerate() {
                if b <= a {
                    // its own break marker: before it, or after it for the first column
                    remove.push(if c == 0 { *b } else { *a - 1 });
                }
            }
        } else {
            remove.push(s);
            remove.extend(firsts.iter().skip(1).map(|f| f - 1));
            remove.push(e);
        }
        remove.sort_unstable();
        remove.dedup();
        for k in remove.into_iter().rev() {
            blocks.remove(k);
        }
        if blocks.is_empty() {
            blocks.push(Block::paragraph(""));
        }
    }
}

// ---------------------------------------------------------------------------
// Markdown → blocks

/// A separator row such as `| --- | :-: |`.
fn is_table_separator(line: &str) -> bool {
    line.starts_with('|') && line.contains('-') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// Cells of one Markdown table row, honouring `\|` inside a cell.
fn split_row(line: &str) -> Vec<String> {
    let inner = line.trim().strip_prefix('|').unwrap_or(line.trim());
    let inner = inner.strip_suffix('|').filter(|_| !inner.ends_with("\\|")).unwrap_or(inner);
    let mut cells = vec![String::new()];
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cells.last_mut().unwrap().push('|');
            }
            '|' => cells.push(String::new()),
            c => cells.last_mut().unwrap().push(c),
        }
    }
    cells.into_iter().map(|c| c.trim().to_string()).collect()
}

/// Number of cells in each row of a table block, before the rows are padded to one width. A row
/// that is shorter or longer than the others usually means a cell was split in two on import.
pub(crate) fn table_row_widths(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(i, r)| !(*i == 1 && is_table_separator(r)))
        .map(|(_, r)| split_row(r).len())
        .collect()
}

fn rows_to_cells(rows: &[String]) -> Vec<Vec<String>> {
    let mut cells: Vec<Vec<String>> = rows
        .iter()
        .enumerate()
        .filter(|(i, r)| !(*i == 1 && is_table_separator(r)))
        .map(|(_, r)| split_row(r))
        .collect();
    let width = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
    for r in &mut cells {
        r.resize(width, String::new());
    }
    cells
}

/// The grid of a table block (the first row is the header). A blank block gives an empty 3 by 3 grid.
pub fn table_cells(text: &str) -> Vec<Vec<String>> {
    let rows: Vec<String> = text.lines().map(str::to_string).collect();
    let cells = rows_to_cells(&rows);
    if cells.is_empty() || (cells.len() == 1 && cells[0].iter().all(String::is_empty) && text.trim().is_empty()) {
        return vec![vec![String::new(); 3]; 3];
    }
    cells
}

/// Markdown rows of a grid, with the separator after the header.
pub fn cells_to_text(cells: &[Vec<String>]) -> String {
    let width = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let row = |r: &Vec<String>| {
        let mut parts: Vec<String> = r.iter().map(|c| c.replace('|', "\\|").replace(['\n', '\r'], " ")).collect();
        parts.resize(width, String::new());
        format!("| {} |", parts.join(" | "))
    };
    let mut lines: Vec<String> = Vec::new();
    for (i, r) in cells.iter().enumerate() {
        lines.push(row(r));
        if i == 0 {
            lines.push(format!("| {} |", vec!["---"; width].join(" | ")));
        }
    }
    lines.join("\n")
}

/// A line that starts a list item or a task (`- `, `* `, `+ `, `- [ ] `, `1. `).
pub(crate) fn is_list_line(l: &str) -> bool {
    let t = l.trim_start();
    t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") || strip_list_number(t).is_some()
}

fn indent_of(line: &str) -> (usize, &str) {
    let mut spaces = 0;
    let mut bytes = 0;
    for c in line.chars() {
        match c {
            ' ' => spaces += 1,
            '\t' => spaces += 4,
            _ => break,
        }
        bytes += c.len_utf8();
    }
    (spaces, &line[bytes..])
}

fn strip_list_number(s: &str) -> Option<&str> {
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = &s[digits..];
    rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")).or_else(|| if rest == "." { Some("") } else { None })
}

/// Parse `![alt](path)` or `[text](path)` filling the whole line.
/// `whole_line_link` that also reads a trailing `<!--w:50-->` or `<!--w:50:l-->` (width and place of an image).
fn whole_line_link_width(s: &str) -> Option<(bool, String, String, u16, u8)> {
    let t = s.trim_end();
    if let Some(at) = t.rfind("<!--w:") {
        if let Some(spec) = t[at + 6..].strip_suffix("-->") {
            let (w, a) = spec.split_once(':').unwrap_or((spec, ""));
            if let Ok(n) = w.parse::<u16>() {
                let width = if n == 0 { 0 } else { n.clamp(10, 99) };
                let align = match a {
                    "l" => 1,
                    "r" => 2,
                    _ => 0,
                };
                return whole_line_link(t[..at].trim_end()).map(|(x, y, z)| (x, y, z, width, align));
            }
        }
    }
    whole_line_link(s).map(|(a, b, c)| (a, b, c, 0, 0))
}

fn whole_line_link(s: &str) -> Option<(bool, String, String)> {
    let (is_img, rest) = match s.strip_prefix("![") {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('[')?),
    };
    if !s.ends_with(')') {
        return None;
    }
    let close = rest.find("](")?;
    let text = &rest[..close];
    let url = &rest[close + 2..rest.len() - 1];
    if url.contains(' ') && !url.starts_with('<') {
        return None;
    }
    let url = url.trim_start_matches('<').trim_end_matches('>');
    Some((is_img, text.to_string(), url.to_string()))
}

fn is_local_path(url: &str) -> bool {
    !(url.contains("://") || url.starts_with("mailto:") || url.starts_with('#'))
}

fn split_icon(s: &str) -> (Option<String>, String) {
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        if !first.is_alphanumeric() && !first.is_ascii() {
            // take the whole first "word" as the icon (emoji may be several chars)
            let icon: String = s.split(' ').next().unwrap_or("").to_string();
            let rest = s[icon.len()..].trim_start().to_string();
            return (Some(icon), rest);
        }
    }
    (None, s.to_string())
}

/// Parse a Markdown body into blocks.
pub fn parse_markdown(body: &str) -> Vec<Block> {
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<Block> = Vec::new();
    let mut i = 0;
    // Plain paragraph lines merge until a blank line or another block starts.
    let mut para_open = false;
    let mut details_depth: Vec<u8> = Vec::new();
    // Blank lines separate blocks; each extra pair of blank lines is an empty paragraph.
    let mut blanks = 0usize;
    while i < lines.len() {
        let raw = lines[i];
        if raw.trim().is_empty() {
            blanks += 1;
            para_open = false;
            i += 1;
            continue;
        }
        if blanks >= 3 && !out.is_empty() {
            for _ in 0..(blanks - 1) / 2 {
                out.push(Block::paragraph(""));
            }
        }
        blanks = 0;
        let (spaces, line) = indent_of(raw);
        let mut depth = ((spaces / 2) as u8).min(MAX_DEPTH);
        if let Some(d) = details_depth.last() {
            depth = depth.max(d + 1);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            para_open = false;
            i += 1;
            continue;
        }
        // Fenced code
        if let Some(lang) = trimmed.strip_prefix("```") {
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() {
                let (_, l) = indent_of(lines[i]);
                if l.trim_end() == "```" {
                    break;
                }
                // remove the block's own indentation
                let strip = (depth as usize) * 2;
                let (sp, _) = indent_of(lines[i]);
                let cut = sp.min(strip);
                code.push(lines[i].get(cut..).unwrap_or(""));
                i += 1;
            }
            i += 1; // closing fence
            out.push(Block::new(BlockKind::Code { lang: lang.trim().to_string() }, code.join("\n")).with_depth(depth));
            para_open = false;
            continue;
        }
        // Equation between `$$` lines, or on one line as `$$ ... $$`.
        if let Some(rest) = trimmed.strip_prefix("$$") {
            if let Some(one) = rest.strip_suffix("$$").filter(|_| rest.len() >= 2) {
                out.push(Block::new(BlockKind::Equation, one.trim()).with_depth(depth));
                para_open = false;
                i += 1;
                continue;
            }
            if rest.trim().is_empty() {
                let mut body = Vec::new();
                i += 1;
                while i < lines.len() && indent_of(lines[i]).1.trim_end() != "$$" {
                    body.push(indent_of(lines[i]).1.trim_end());
                    i += 1;
                }
                i += 1; // closing line
                out.push(Block::new(BlockKind::Equation, body.join("\n").trim()).with_depth(depth));
                para_open = false;
                continue;
            }
        }
        // Markdown table: a row, then a separator row (`| --- | --- |`), then more rows.
        if line.starts_with('|') && lines.get(i + 1).is_some_and(|n| is_table_separator(indent_of(n).1.trim())) {
            let mut rows = Vec::new();
            while i < lines.len() {
                let l = indent_of(lines[i]).1.trim();
                if !l.starts_with('|') {
                    break;
                }
                rows.push(l.to_string());
                i += 1;
            }
            let cells = rows_to_cells(&rows);
            out.push(Block::new(BlockKind::Table, cells_to_text(&cells)).with_depth(depth));
            para_open = false;
            continue;
        }
        if trimmed.starts_with("</details>") {
            details_depth.pop();
            para_open = false;
            i += 1;
            continue;
        }
        if trimmed.starts_with("<details") {
            // the summary may be on the same line or the next one
            let open = trimmed.starts_with("<details open");
            let mut title = String::new();
            let mut s = trimmed.to_string();
            if !s.contains("<summary>") && i + 1 < lines.len() {
                i += 1;
                s.push_str(lines[i].trim());
            }
            if let (Some(a), Some(b)) = (s.find("<summary>"), s.find("</summary>")) {
                if b > a {
                    title = s[a + 9..b].to_string();
                }
            }
            // depth: from indentation, ignoring the enclosing details (we are the block)
            let d = ((spaces / 2) as u8).min(MAX_DEPTH).max(details_depth.last().map(|d| d + 1).unwrap_or(0));
            out.push(Block::new(BlockKind::Toggle { open }, title).with_depth(d));
            details_depth.push(d);
            para_open = false;
            i += 1;
            continue;
        }
        let block = if let Some(t) = trimmed.strip_prefix("### ").or_else(|| trimmed.strip_prefix("#### ")) {
            Some(Block::new(BlockKind::Heading(3), t))
        } else if let Some(t) = trimmed.strip_prefix("## ") {
            Some(Block::new(BlockKind::Heading(2), t))
        } else if let Some(t) = trimmed.strip_prefix("# ") {
            Some(Block::new(BlockKind::Heading(1), t))
        } else if matches!(trimmed, "---" | "***" | "___") {
            Some(Block::new(BlockKind::Divider, ""))
        } else if trimmed == "<!--columns-->" {
            Some(Block::new(BlockKind::ColumnsStart, ""))
        } else if trimmed == "<!--column-->" {
            Some(Block::new(BlockKind::ColumnBreak, ""))
        } else if trimmed == "<!--/columns-->" {
            Some(Block::new(BlockKind::ColumnsEnd, ""))
        } else if matches!(trimmed, "<!--toc-->" | "<!-- toc -->") {
            Some(Block::new(BlockKind::Toc, ""))
        } else if let Some(title) = trimmed.strip_prefix("<!--meeting:").and_then(|t| t.strip_suffix("-->")) {
            Some(Block::new(BlockKind::Meeting, title.trim()))
        } else if let Some(rest) = trimmed.strip_prefix("<!--bookmark:").and_then(|t| t.strip_suffix("-->")) {
            let rest = rest.trim();
            let (url, title) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            Some(Block::new(BlockKind::Bookmark { url: url.to_string() }, title.trim()))
        } else if let Some(title) = trimmed.strip_prefix("<!--database:").and_then(|t| t.strip_suffix("-->")) {
            Some(Block::new(BlockKind::Database, title.trim()))
        } else if let Some(t) = ["- [ ] ", "* [ ] ", "- [ ]", "* [ ]"].iter().find_map(|p| {
            if trimmed == p.trim_end() {
                Some("")
            } else {
                trimmed.strip_prefix(p)
            }
        }) {
            Some(Block::new(BlockKind::Todo { checked: false }, t))
        } else if let Some(t) = ["- [x] ", "- [X] ", "* [x] ", "- [x]", "- [X]"].iter().find_map(|p| {
            if trimmed == p.trim_end() {
                Some("")
            } else {
                trimmed.strip_prefix(p)
            }
        }) {
            Some(Block::new(BlockKind::Todo { checked: true }, t))
        } else if let Some(t) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
            .or_else(|| if trimmed == "-" { Some("") } else { None })
        {
            Some(Block::new(BlockKind::Bulleted, t))
        } else if let Some(t) = strip_list_number(trimmed) {
            Some(Block::new(BlockKind::Numbered, t))
        } else if trimmed.starts_with('>') {
            // Quote or callout: gather consecutive `>` lines.
            let mut content = Vec::new();
            let mut is_callout = false;
            let first = trimmed.trim_start_matches('>').trim();
            if first.starts_with("[!") && first.ends_with(']') {
                is_callout = true;
            } else {
                content.push(first.to_string());
            }
            i += 1;
            while i < lines.len() {
                let (_, l) = indent_of(lines[i]);
                let l = l.trim_end();
                if !l.starts_with('>') {
                    break;
                }
                content.push(l.trim_start_matches('>').trim_start().to_string());
                i += 1;
            }
            let text = content.join("\n");
            if is_callout {
                let (icon, rest) = split_icon(&text);
                // list items inside a callout become its children, so tasks stay real tasks
                let mut rest_lines: Vec<String> = rest.split('\n').map(str::to_string).collect();
                let split_at = rest_lines.iter().position(|l| is_list_line(l)).unwrap_or(rest_lines.len());
                // the colour marker closes the header of the callout (its text before the list items):
                // a marker at the end of a list item stays that item's own colour
                let mut callout_color = String::new();
                let marker_line = rest_lines[..split_at].iter().rposition(|l| !l.trim().is_empty()).unwrap_or(0);
                if split_at > 0 || split_at == rest_lines.len() {
                    if let Some(line) = rest_lines.get(marker_line).cloned() {
                        let t = line.trim_end();
                        if let Some(body) = t.strip_suffix("-->") {
                            if let Some(at) = body.rfind("<!--c:") {
                                let name = &body[at + "<!--c:".len()..];
                                if is_block_color(name) && (at == 0 || body[..at].ends_with(' ')) {
                                    callout_color = name.to_string();
                                    rest_lines[marker_line] = body[..at].trim_end().to_string();
                                }
                            }
                        }
                    }
                }
                let rest_lines: Vec<&str> = rest_lines.iter().map(String::as_str).collect();
                // a heading inside a callout is shown as a bold title line
                let head = rest_lines[..split_at]
                    .iter()
                    .map(|l| match l.trim_start().trim_start_matches('#') {
                        t if t.len() < l.trim_start().len() && t.starts_with(' ') && !t.trim().is_empty() => {
                            format!("**{}**", t.trim())
                        }
                        _ => (*l).to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let head = head.trim_end().to_string();
                let mut callout = Block::new(BlockKind::Callout { icon: icon.unwrap_or_else(|| "💡".into()) }, head)
                    .with_depth(depth);
                callout.color = callout_color;
                out.push(callout);
                if split_at < rest_lines.len() {
                    let tail = rest_lines[split_at..].join("\n");
                    for c in parse_markdown(&tail) {
                        let d = c.depth.saturating_add(depth + 1);
                        out.push(c.with_depth(d));
                    }
                }
            } else {
                // a coloured quote carries its colour as a trailing comment, like any other block
                let mut text = text;
                let mut quote_color = String::new();
                if let Some(body) = text.trim_end().strip_suffix("-->") {
                    if let Some(at) = body.rfind(" <!--c:") {
                        let name = &body[at + " <!--c:".len()..];
                        if is_block_color(name) {
                            quote_color = name.to_string();
                            text = body[..at].to_string();
                        }
                    }
                }
                let mut quote = Block::new(BlockKind::Quote, text).with_depth(depth);
                quote.color = quote_color;
                out.push(quote);
            }
            para_open = false;
            continue;
        } else if let Some((is_img, text, url, width, align)) = whole_line_link_width(trimmed) {
            if is_img {
                let mut img = Block::new(BlockKind::Image { path: url }, text);
                img.width = width;
                img.align = align;
                Some(img)
            } else if text.starts_with("📎") || (is_local_path(&url) && url.starts_with("assets/")) {
                let name = text.trim_start_matches("📎").trim().to_string();
                Some(Block::new(BlockKind::File { path: url }, name))
            } else {
                None
            }
        } else {
            None
        };
        match block {
            Some(b) => {
                out.push(b.with_depth(depth));
                para_open = false;
            }
            None => {
                let text = trimmed.trim_end_matches("  ").trim_end_matches('\\');
                if para_open {
                    if let Some(last) = out.last_mut() {
                        last.text.push('\n');
                        last.text.push_str(text);
                    }
                } else {
                    out.push(Block::paragraph(text).with_depth(depth));
                    para_open = true;
                }
            }
        }
        i += 1;
    }
    if blanks >= 2 && !out.is_empty() {
        for _ in 0..blanks / 2 {
            out.push(Block::paragraph(""));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// blocks → Markdown

fn push_lines(out: &mut String, indent: &str, prefix: &str, cont: &str, text: &str) {
    for (k, line) in text.split('\n').enumerate() {
        out.push_str(indent);
        out.push_str(if k == 0 { prefix } else { cont });
        out.push_str(line);
        out.push('\n');
    }
}

/// Serialize blocks to Markdown.
pub fn to_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut open_toggles: Vec<u8> = Vec::new();
    let mut prev: Option<&Block> = None;
    for src in blocks {
        let b = src;
        // the colour travels as a comment at the end of the block's text
        let coloured;
        let b = if !b.color.is_empty()
            && b.kind.has_rich_text()
            && (!b.text.is_empty() || matches!(b.kind, BlockKind::Callout { .. }))
            && is_block_color(&b.color)
        {
            coloured = Block { text: format!("{} <!--c:{}-->", b.text, b.color), ..b.clone() };
            &coloured
        } else {
            b
        };
        while let Some(&d) = open_toggles.last() {
            if b.depth <= d {
                open_toggles.pop();
                out.push_str(&"  ".repeat(d as usize));
                out.push_str("</details>\n");
                prev = None;
            } else {
                break;
            }
        }
        // Separate blocks by a blank line, except consecutive list items.
        if let Some(p) = prev {
            let tight = p.kind.is_list() && b.kind.is_list()
                || (p.kind.is_list() && b.depth > p.depth)
                || matches!(p.kind, BlockKind::Toggle { .. }) && b.depth > p.depth;
            if !tight {
                out.push('\n');
            }
        } else if !out.is_empty() && !out.ends_with("\n\n") && !out.ends_with("</details>\n") {
            out.push('\n');
        }
        let indent = "  ".repeat(b.depth as usize);
        match &b.kind {
            BlockKind::Paragraph => {
                if b.text.is_empty() {
                    // an empty paragraph is a plain blank line
                    out.push('\n');
                } else {
                    let escaped = escape_paragraph(&b.text);
                    let joined = escaped.replace('\n', "  \n");
                    push_lines(&mut out, &indent, "", "", &joined);
                }
            }
            BlockKind::Heading(level) => {
                let hashes = "#".repeat((*level).clamp(1, 3) as usize);
                out.push_str(&format!("{indent}{hashes} {}\n", b.text.replace('\n', " ")));
            }
            BlockKind::Bulleted => push_lines(&mut out, &indent, "- ", "  ", &b.text),
            BlockKind::Numbered => push_lines(&mut out, &indent, "1. ", "   ", &b.text),
            BlockKind::Todo { checked } => {
                let p = if *checked { "- [x] " } else { "- [ ] " };
                push_lines(&mut out, &indent, p, "  ", &b.text)
            }
            BlockKind::Toggle { open } => {
                let attr = if *open { " open" } else { "" };
                out.push_str(&format!("{indent}<details{attr}><summary>{}</summary>\n", b.text.replace('\n', " ")));
                open_toggles.push(b.depth);
            }
            BlockKind::Quote => push_lines(&mut out, &indent, "> ", "> ", &b.text),
            BlockKind::Callout { icon } => {
                out.push_str(&format!("{indent}> [!NOTE]\n"));
                let first = format!("{icon} ");
                push_lines(&mut out, &indent, &format!("> {first}"), "> ", &b.text);
            }
            BlockKind::Code { lang } => {
                out.push_str(&format!("{indent}```{lang}\n"));
                for line in b.text.split('\n') {
                    if !line.is_empty() {
                        out.push_str(&indent);
                    }
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&format!("{indent}```\n"));
            }
            BlockKind::Divider => out.push_str(&format!("{indent}---\n")),
            BlockKind::Toc => out.push_str(&format!("{indent}<!--toc-->\n")),
            BlockKind::Meeting => {
                // A title can never close the comment early.
                let title = b.text.replace("--", "-").replace(['\n', '\r'], " ");
                out.push_str(&format!("{indent}<!--meeting:{}-->\n", title.trim()));
            }
            BlockKind::Equation => {
                out.push_str(&format!("{indent}$$\n"));
                for line in b.text.replace("$$", "$ $").lines() {
                    out.push_str(&format!("{indent}{line}\n"));
                }
                out.push_str(&format!("{indent}$$\n"));
            }
            BlockKind::Bookmark { url } => {
                let url = url.replace("--", "%2D%2D").replace(char::is_whitespace, "%20");
                let title = b.text.replace("--", "-").replace(['\n', '\r'], " ");
                out.push_str(&format!("{indent}<!--bookmark:{url} {}-->\n", title.trim()));
            }
            BlockKind::Table => {
                for line in cells_to_text(&table_cells(&b.text)).split('\n') {
                    out.push_str(&format!("{indent}{line}\n"));
                }
            }
            BlockKind::Database => {
                let title = b.text.replace("--", "-").replace(['\n', '\r'], " ");
                out.push_str(&format!("{indent}<!--database:{}-->\n", title.trim()));
            }
            BlockKind::ColumnsStart => out.push_str(&format!("{indent}<!--columns-->\n")),
            BlockKind::ColumnBreak => out.push_str(&format!("{indent}<!--column-->\n")),
            BlockKind::ColumnsEnd => out.push_str(&format!("{indent}<!--/columns-->\n")),
            BlockKind::Image { path } => {
                let a = match b.align {
                    1 => ":l",
                    2 => ":r",
                    _ => "",
                };
                let w = if (10..100).contains(&b.width) {
                    format!("<!--w:{}{a}-->", b.width)
                } else if !a.is_empty() {
                    format!("<!--w:0{a}-->")
                } else {
                    String::new()
                };
                out.push_str(&format!("{indent}![{}]({}){w}\n", b.text.replace(['[', ']'], ""), link_target(path)))
            }
            BlockKind::File { path } => {
                out.push_str(&format!("{indent}[📎 {}]({})\n", b.text.replace(['[', ']'], ""), link_target(path)))
            }
        }
        prev = Some(src);
    }
    while let Some(d) = open_toggles.pop() {
        out.push_str(&"  ".repeat(d as usize));
        out.push_str("</details>\n");
    }
    out
}

fn link_target(path: &str) -> String {
    if path.contains(' ') {
        format!("<{path}>")
    } else {
        path.to_string()
    }
}

/// A paragraph that would otherwise be read back as another block type gets a
/// backslash in front (`\# not a heading`).
fn escape_paragraph(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let t = line.trim_start();
            let needs = t.starts_with('#')
                || t.starts_with("- ")
                || t.starts_with("* ")
                || t.starts_with("+ ")
                || t.starts_with('>')
                || t.starts_with("```")
                || t.starts_with("<details")
                || t.starts_with("</details")
                || matches!(t, "---" | "***" | "___" | "-")
                || strip_list_number(t).is_some()
                || whole_line_link(t).is_some();
            if needs {
                format!("\\{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo the escaping (and read the `&nbsp;` placeholder of older files) after parsing.
pub fn normalize_parsed(blocks: &mut [Block]) {
    for b in blocks.iter_mut() {
        if b.kind.has_rich_text() {
            if let Some(body) = b.text.strip_suffix("-->") {
                if let Some(at) = body.rfind(" <!--c:") {
                    let name = &body[at + " <!--c:".len()..];
                    if is_block_color(name) {
                        b.color = name.to_string();
                        b.text = body[..at].to_string();
                    }
                }
            }
        }
        if b.kind == BlockKind::Paragraph {
            if b.text == "&nbsp;" {
                b.text.clear();
            } else {
                b.text = b.text.split('\n').map(|l| l.strip_prefix('\\').unwrap_or(l)).collect::<Vec<_>>().join("\n");
            }
        }
    }
}

/// Parse and normalize in one step (what storage uses).
pub fn from_markdown(body: &str) -> Vec<Block> {
    let mut b = parse_markdown(body);
    normalize_parsed(&mut b);
    b
}

/// Give blocks of `new` that equal a block of `old` (same kind, text, depth)
/// that block's in-memory id: first at the same position, then anywhere.
/// Used after re-reading a file so the editor keeps its focus.
pub fn adopt_ids(old: &[Block], new: &mut [Block]) {
    let same = |a: &Block, b: &Block| a.kind == b.kind && a.text == b.text && a.depth == b.depth;
    let mut used = vec![false; old.len()];
    let mut adopted = vec![false; new.len()];
    for (i, n) in new.iter_mut().enumerate() {
        if let Some(o) = old.get(i) {
            if same(o, n) {
                n.id = o.id;
                used[i] = true;
                adopted[i] = true;
            }
        }
    }
    for (i, n) in new.iter_mut().enumerate() {
        if adopted[i] {
            continue;
        }
        if let Some(k) = (0..old.len()).find(|k| !used[*k] && same(&old[*k], n)) {
            n.id = old[k].id;
            used[k] = true;
        }
    }
}

/// Three-way merge of a page body by blocks. `base` is the version last read
/// or written (its ids match `mine`), `mine` has unsaved local edits,
/// `theirs` was just read from disk. Returns `None` when both sides changed
/// the same block (a real conflict).
pub fn merge_blocks(base: &[Block], mine: &[Block], theirs: &[Block]) -> Option<Vec<Block>> {
    let mut theirs: Vec<Block> = theirs.to_vec();
    adopt_ids(base, &mut theirs);
    let base_of = |id: u64| base.iter().find(|b| b.id == id);
    let in_theirs = |id: u64, t: &[Block]| t.iter().position(|b| b.id == id);
    let mut result = theirs.clone();
    // my edits to blocks they left alone
    for m in mine {
        match base_of(m.id) {
            Some(b) if b != m => {
                let pos = in_theirs(m.id, &result)?;
                result[pos] = m.clone()
            }
            _ => {}
        }
    }
    // blocks I deleted (they did not touch them)
    for b in base {
        if !mine.iter().any(|m| m.id == b.id) {
            if let Some(pos) = in_theirs(b.id, &result) {
                result.remove(pos);
            }
        }
    }
    // blocks I added: after the block that precedes them in my version
    for (i, m) in mine.iter().enumerate() {
        if base_of(m.id).is_some() || in_theirs(m.id, &result).is_some() {
            continue;
        }
        let at = (0..i).rev().find_map(|j| in_theirs(mine[j].id, &result).map(|p| p + 1)).unwrap_or(0);
        result.insert(at, m.clone());
    }
    Some(result)
}

/// Plain text of all blocks (for search and previews).
pub fn plain_text(blocks: &[Block]) -> String {
    blocks.iter().map(|b| crate::inline::strip(&b.text)).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Markdown shortcut typed at the start of a paragraph: `# `, `- `, `[] `…
/// Returns the new kind and how many leading chars to remove.
pub fn markdown_shortcut(text: &str) -> Option<(BlockKind, usize)> {
    type KindCtor = fn() -> BlockKind;
    const TABLE: &[(&str, KindCtor)] = &[
        ("### ", || BlockKind::Heading(3)),
        ("## ", || BlockKind::Heading(2)),
        ("# ", || BlockKind::Heading(1)),
        ("[] ", || BlockKind::Todo { checked: false }),
        ("[ ] ", || BlockKind::Todo { checked: false }),
        ("[x] ", || BlockKind::Todo { checked: true }),
        ("- ", || BlockKind::Bulleted),
        ("* ", || BlockKind::Bulleted),
        ("+ ", || BlockKind::Bulleted),
        ("1. ", || BlockKind::Numbered),
        ("> ", || BlockKind::Toggle { open: true }),
        ("\" ", || BlockKind::Quote),
        ("| ", || BlockKind::Quote),
        ("```", || BlockKind::Code { lang: String::new() }),
        ("---", || BlockKind::Divider),
    ];
    for (prefix, make) in TABLE {
        if text.starts_with(prefix) {
            return Some((make(), prefix.chars().count()));
        }
    }
    None
}

/// Split a page into presentation slides: at every divider, or, without dividers, at every
/// heading of the highest level used (title or section). Returns the block ranges; dividers are
/// left out. A page with neither is a single slide.
pub fn slides(blocks: &[Block]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let has_divider = blocks.iter().any(|b| b.kind == BlockKind::Divider);
    let mut start = 0;
    for (i, b) in blocks.iter().enumerate() {
        let cut = if has_divider {
            b.kind == BlockKind::Divider
        } else {
            matches!(b.kind, BlockKind::Heading(l) if l <= 2) && i > start
        };
        if cut {
            if i > start {
                out.push(start..i);
            }
            start = if b.kind == BlockKind::Divider { i + 1 } else { i };
        }
    }
    if start < blocks.len() {
        out.push(start..blocks.len());
    }
    out.retain(|r| blocks[r.clone()].iter().any(|b| b.kind != BlockKind::Divider));
    if out.is_empty() {
        out.push(0..blocks.len());
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn dropping_a_block_beside_another_makes_columns() {
        let mut v = vec![Block::paragraph("a"), Block::paragraph("b"), Block::paragraph("c")];
        let moved = v[2].id;
        assert_eq!(drop_beside(&mut v, 2, 0), Some(moved));
        let kinds: Vec<BlockKind> = v.iter().map(|b| b.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::ColumnsStart,
                BlockKind::Paragraph,
                BlockKind::ColumnBreak,
                BlockKind::Paragraph,
                BlockKind::ColumnsEnd,
                BlockKind::Paragraph
            ]
        );
        assert_eq!((v[1].text.as_str(), v[3].text.as_str(), v[5].text.as_str()), ("a", "c", "b"));
        // beside a block already in columns: a third column
        let b_id = v[5].id;
        assert_eq!(drop_beside(&mut v, 5, 1), Some(b_id));
        let (end, firsts) = column_layout(&v, 0).unwrap();
        assert_eq!(firsts.len(), 3);
        assert_eq!(v[end - 1].text, "c");
        assert_eq!(v[firsts[1]].text, "b");
        // taking the last block out of a column removes that column, then the columns
        let a_pos = v.iter().position(|x| x.text == "a").unwrap();
        let n = v.len();
        move_range(&mut v, a_pos, a_pos + 1, n);
        tidy_columns(&mut v);
        assert_eq!(column_layout(&v, 0).map(|(_, f)| f.len()), Some(2));
        let c_pos = v.iter().position(|x| x.text == "c").unwrap();
        let n = v.len();
        move_range(&mut v, c_pos, c_pos + 1, n);
        tidy_columns(&mut v);
        assert!(v
            .iter()
            .all(|x| !matches!(x.kind, BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd)));
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn a_block_keeps_both_its_text_and_background_colours() {
        assert_eq!(merge_color("bg-blue", "red", false), "red bg-blue");
        assert_eq!(merge_color("red", "bg-blue", true), "red bg-blue");
        assert_eq!(merge_color("red bg-blue", "", true), "red");
        assert_eq!(merge_color("red bg-blue", "green", false), "green bg-blue");
        assert!(is_block_color("red bg-blue") && !is_block_color("red green") && !is_block_color("red  bg-blue"));
        let mut p = Block::paragraph("Hi");
        p.color = "red bg-blue".into();
        let back = from_markdown(&to_markdown(&[p]));
        assert_eq!((back[0].text.as_str(), back[0].color.as_str()), ("Hi", "red bg-blue"));
    }

    #[test]
    fn columns_round_trip_and_layout() {
        let md = "<!--columns-->\n\nleft\n\n<!--column-->\n\nright\n\n<!--/columns-->\n";
        let blocks = from_markdown(md);
        assert_eq!(to_markdown(&blocks), md);
        let (end, firsts) = column_layout(&blocks, 0).expect("layout");
        assert_eq!((end, firsts), (4, vec![1, 3]));
    }

    #[test]
    fn meeting_block_round_trips() {
        let md = "# A\n\n<!--meeting:Weekly sync-->\n\n## Notes\n";
        let blocks = from_markdown(md);
        let m = blocks.iter().find(|b| b.kind == BlockKind::Meeting).expect("meeting block");
        assert_eq!(m.text, "Weekly sync");
        assert!(to_markdown(&blocks).contains("<!--meeting:Weekly sync-->"));
        let mut tricky = Block::new(BlockKind::Meeting, "a --> b");
        tricky.depth = 0;
        let back = from_markdown(&to_markdown(&[tricky]));
        assert_eq!(back[0].kind, BlockKind::Meeting);
    }

    #[test]
    fn toc_block_round_trips() {
        let md = "# A\n\n<!--toc-->\n\n## B\n";
        let blocks = from_markdown(md);
        assert!(blocks.iter().any(|b| b.kind == BlockKind::Toc));
        assert_eq!(to_markdown(&blocks), md);
    }

    use super::*;

    #[test]
    fn image_width_round_trips() {
        let mut b = Block::new(BlockKind::Image { path: "assets/a.png".into() }, "cap");
        b.width = 40;
        let md = to_markdown(&[b]);
        assert!(md.contains("![cap](assets/a.png)<!--w:40-->"), "{md}");
        let back = from_markdown(&md);
        assert_eq!(back[0].width, 40);
        assert_eq!(back[0].kind, BlockKind::Image { path: "assets/a.png".into() });
        assert_eq!(from_markdown("![x](assets/a.png)\n")[0].width, 0);
        let mut c = Block::new(BlockKind::Image { path: "assets/a.png".into() }, "");
        c.align = 2;
        let md = to_markdown(&[c]);
        assert!(md.contains("<!--w:0:r-->"), "{md}");
        let back = from_markdown(&md);
        assert_eq!((back[0].width, back[0].align), (0, 2));
    }

    fn kinds(bs: &[Block]) -> Vec<&'static str> {
        bs.iter().map(|b| b.kind.key()).collect()
    }

    fn strip_ids(bs: &[Block]) -> Vec<(BlockKind, String, u8)> {
        bs.iter().map(|b| (b.kind.clone(), b.text.clone(), b.depth)).collect()
    }

    #[test]
    fn parses_every_block_type() {
        let md = "# One\n\n## Two\n\n### Three\n\nHello **world**\n\n- a\n- b\n  - nested\n1. first\n2. second\n- [ ] todo\n- [x] done\n\n> quote\n\n> [!NOTE]\n> 💡 callout text\n\n```rust\nfn main() {}\n```\n\n---\n\n![cap](assets/a.png)\n\n[📎 doc.pdf](assets/doc.pdf)\n\n<details open><summary>Toggle</summary>\n  inside\n</details>\n";
        let b = from_markdown(md);
        assert_eq!(
            kinds(&b),
            vec![
                "h1", "h2", "h3", "text", "bulleted", "bulleted", "bulleted", "numbered", "numbered", "todo", "todo",
                "quote", "callout", "code", "divider", "image", "file", "toggle", "text"
            ]
        );
        assert_eq!(b[6].depth, 1);
        assert_eq!(b[10].kind, BlockKind::Todo { checked: true });
        assert_eq!(b[12].kind, BlockKind::Callout { icon: "💡".into() });
        assert_eq!(b[12].text, "callout text");
        assert_eq!(b[13].text, "fn main() {}");
        assert_eq!(b[18].depth, 1);
    }

    #[test]
    fn round_trip_is_stable() {
        let blocks = vec![
            Block::new(BlockKind::Heading(1), "Title"),
            Block::paragraph("Line one\nline two"),
            Block::paragraph(""),
            Block::paragraph("# not a heading"),
            Block::new(BlockKind::Bulleted, "item"),
            Block::new(BlockKind::Bulleted, "child").with_depth(1),
            Block::new(BlockKind::Numbered, "n1"),
            Block::new(BlockKind::Todo { checked: true }, "done"),
            Block::new(BlockKind::Toggle { open: false }, "More"),
            Block::paragraph("hidden child").with_depth(1),
            Block::new(BlockKind::Callout { icon: "⚠️".into() }, "Careful"),
            Block::new(BlockKind::Quote, "Q1\nQ2"),
            Block::new(BlockKind::Code { lang: "py".into() }, "x = 1\n\nprint(x)"),
            Block::new(BlockKind::Divider, ""),
            Block::new(BlockKind::Image { path: "assets/my pic.png".into() }, "Caption"),
            Block::new(BlockKind::File { path: "assets/a.pdf".into() }, "a.pdf"),
            Block::paragraph("end"),
        ];
        let md = to_markdown(&blocks);
        let back = from_markdown(&md);
        assert_eq!(strip_ids(&back), strip_ids(&blocks), "markdown was:\n{md}");
        // and serializing again gives the same text
        assert_eq!(to_markdown(&back), md);
    }

    #[test]
    fn empty_paragraphs_are_plain_blank_lines() {
        let blocks = vec![
            Block::paragraph("a"),
            Block::paragraph(""),
            Block::paragraph(""),
            Block::paragraph("b"),
            Block::paragraph(""),
        ];
        let md = to_markdown(&blocks);
        assert!(!md.contains("&nbsp;"), "{md}");
        assert_eq!(strip_ids(&from_markdown(&md)), strip_ids(&blocks), "{md:?}");
        // older files keep working
        assert_eq!(from_markdown("a\n\n&nbsp;\n\nb\n")[1].text, "");
        // two blank lines typed by hand are just a separator
        assert_eq!(from_markdown("a\n\n\nb\n").len(), 2);
    }

    #[test]
    fn ids_are_adopted_and_edits_merge() {
        let base = vec![Block::paragraph("one"), Block::paragraph("two"), Block::paragraph("three")];
        // I am typing in "two"
        let mut mine = base.clone();
        mine[1].text = "two, typed here".into();
        // meanwhile the MCP server appended a block and edited "one"
        let theirs = from_markdown("one, edited elsewhere\n\ntwo\n\nthree\n\nfour\n");
        let merged = merge_blocks(&base, &mine, &theirs).unwrap();
        let texts: Vec<_> = merged.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, vec!["one, edited elsewhere", "two, typed here", "three", "four"]);
        assert_eq!(merged[1].id, base[1].id, "the focused block keeps its id");
        // both sides changed the same block: conflict
        let theirs = from_markdown("one\n\ntwo, changed there\n\nthree\n");
        assert!(merge_blocks(&base, &mine, &theirs).is_none());
        // my new block is kept after its neighbour
        let mut mine2 = base.clone();
        mine2.insert(1, Block::paragraph("inserted"));
        let theirs = from_markdown("one\n\ntwo\n\nthree\n");
        let merged = merge_blocks(&base, &mine2, &theirs).unwrap();
        assert_eq!(merged[1].text, "inserted");
    }

    #[test]
    fn numbering_counts_runs() {
        let b = vec![
            Block::new(BlockKind::Numbered, "a"),
            Block::new(BlockKind::Numbered, "b"),
            Block::new(BlockKind::Bulleted, "c").with_depth(1),
            Block::new(BlockKind::Numbered, "d"),
            Block::paragraph("x"),
            Block::new(BlockKind::Numbered, "e"),
        ];
        assert_eq!(numbered_index(&b, 0), 1);
        assert_eq!(numbered_index(&b, 1), 2);
        assert_eq!(numbered_index(&b, 3), 3);
        assert_eq!(numbered_index(&b, 5), 1);
    }

    #[test]
    fn toggles_hide_children() {
        let b = vec![
            Block::new(BlockKind::Toggle { open: false }, "t"),
            Block::paragraph("c").with_depth(1),
            Block::paragraph("after"),
        ];
        assert_eq!(hidden_by_toggles(&b), vec![false, true, false]);
    }

    #[test]
    fn moving_blocks_carries_children() {
        let mut b = vec![
            Block::paragraph("a"),
            Block::new(BlockKind::Bulleted, "b"),
            Block::new(BlockKind::Bulleted, "b1").with_depth(1),
            Block::paragraph("c"),
        ];
        let at = move_block(&mut b, 1, 4);
        let texts: Vec<_> = b.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "c", "b", "b1"]);
        assert_eq!(at, 2);
        assert_eq!(b[3].depth, 1);
        let at = move_block(&mut b, 2, 0);
        let texts: Vec<_> = b.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["b", "b1", "a", "c"]);
        assert_eq!(at, 0);
    }

    #[test]
    fn shortcuts() {
        assert_eq!(markdown_shortcut("# x"), Some((BlockKind::Heading(1), 2)));
        assert_eq!(markdown_shortcut("[] "), Some((BlockKind::Todo { checked: false }, 3)));
        assert_eq!(markdown_shortcut("hello"), None);
    }

    #[test]
    fn a_colour_on_the_last_item_of_a_callout_stays_with_the_item() {
        let src = "> [!NOTE]\n> 💡 - **A**\n> - **B** <!--c:bg-red-->\n";
        let b = from_markdown(src);
        assert_eq!(b[0].color, "");
        assert_eq!(b.last().unwrap().color, "bg-red");
        let again = from_markdown(&to_markdown(&b));
        let key = |v: &[Block]| {
            v.iter().map(|x| (x.kind.clone(), x.text.clone(), x.depth, x.color.clone())).collect::<Vec<_>>()
        };
        assert_eq!(key(&b), key(&again));
    }

    #[test]
    fn a_coloured_callout_round_trips_with_or_without_a_title_and_items() {
        for head in ["💡 Titre", "💡"] {
            let src = format!("> [!NOTE]\n> {head} <!--c:bg-blue-->\n> - un\n> - deux\n");
            let b = from_markdown(&src);
            assert_eq!(b[0].color, "bg-blue", "{head}");
            let again = from_markdown(&to_markdown(&b));
            let key = |v: &[Block]| {
                v.iter().map(|x| (x.kind.clone(), x.text.clone(), x.depth, x.color.clone())).collect::<Vec<_>>()
            };
            assert_eq!(key(&b), key(&again), "{head}");
        }
        let b = from_markdown("> [!NOTE]\n> 💡 Seul <!--c:bg-green-->\n");
        assert_eq!((b[0].text.as_str(), b[0].color.as_str()), ("Seul", "bg-green"));
    }

    #[test]
    fn block_color_round_trip() {
        let mut p = Block::paragraph("Hello **world**");
        p.color = "bg-yellow".into();
        let mut h = Block::new(BlockKind::Heading(2), "Title");
        h.color = "red".into();
        let mut t = Block::new(BlockKind::Toggle { open: false }, "More");
        t.color = "blue".into();
        let plain = Block::paragraph("plain");
        let md = to_markdown(&[p, h, t, plain]);
        assert!(md.contains("<!--c:bg-yellow-->"), "{md}");
        let back = from_markdown(&md);
        assert_eq!(back.len(), 4, "{md}");
        assert_eq!((back[0].text.as_str(), back[0].color.as_str()), ("Hello **world**", "bg-yellow"));
        assert_eq!((back[1].text.as_str(), back[1].color.as_str()), ("Title", "red"));
        assert_eq!((back[2].text.as_str(), back[2].color.as_str()), ("More", "blue"));
        assert_eq!((back[3].text.as_str(), back[3].color.as_str()), ("plain", ""));
    }

    #[test]
    fn unknown_color_comment_is_left_alone() {
        let back = from_markdown("text <!--c:chartreuse-->\n");
        assert_eq!(back[0].color, "");
        assert!(back[0].text.contains("chartreuse"));
    }

    #[test]
    fn database_block_round_trips() {
        let blocks = from_markdown("# A\n\n<!--database:Tasks-->\n\nafter\n");
        let b = blocks.iter().find(|b| b.kind == BlockKind::Database).expect("database block");
        assert_eq!(b.text, "Tasks");
        assert!(to_markdown(&blocks).contains("<!--database:Tasks-->"));
    }

    #[test]
    fn table_block_round_trips() {
        let md = "# A\n\n| Name | Qty |\n| --- | --- |\n| Milk | 2 |\n| a \\| b | 3 |\n\nafter\n";
        let blocks = from_markdown(md);
        let t = blocks.iter().find(|b| b.kind == BlockKind::Table).expect("table block");
        let cells = table_cells(&t.text);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0], vec!["Name", "Qty"]);
        assert_eq!(cells[2][0], "a | b");
        let out = to_markdown(&blocks);
        assert!(out.contains("| Name | Qty |\n| --- | --- |\n| Milk | 2 |"));
        assert_eq!(to_markdown(&from_markdown(&out)), out);
        assert_eq!(table_cells("").len(), 3);
    }

    #[test]
    fn bookmark_block_round_trips() {
        let blocks = from_markdown("<!--bookmark:https://example.com/a?b=1 Example page-->\n");
        assert_eq!(blocks[0].kind, BlockKind::Bookmark { url: "https://example.com/a?b=1".into() });
        assert_eq!(blocks[0].text, "Example page");
        assert_eq!(to_markdown(&blocks), "<!--bookmark:https://example.com/a?b=1 Example page-->\n");
        let bare = from_markdown("<!--bookmark:https://example.com-->\n");
        assert_eq!(bare[0].text, "");
    }

    #[test]
    fn equation_block_round_trips() {
        let blocks = from_markdown("Before\n\n$$\n\\frac{a}{b} + c^2\n$$\n\n$$x = 1$$\n\nAfter\n");
        let eqs: Vec<&Block> = blocks.iter().filter(|b| b.kind == BlockKind::Equation).collect();
        assert_eq!(eqs.len(), 2);
        assert_eq!(eqs[0].text, "\\frac{a}{b} + c^2");
        assert_eq!(eqs[1].text, "x = 1");
        let md = to_markdown(&blocks);
        assert!(md.contains("$$\n\\frac{a}{b} + c^2\n$$"));
        assert_eq!(to_markdown(&from_markdown(&md)), md);
    }

    #[test]
    fn slides_split_at_dividers_or_top_headings() {
        let md = "# Title\n\nintro\n\n## One\n\na\n\n## Two\n\nb\n";
        let b = from_markdown(md);
        let s = slides(&b);
        assert_eq!(s.len(), 3, "{s:?}");
        let with_div = from_markdown("# T\n\nx\n\n---\n\ny\n\n---\n\nz\n");
        assert_eq!(slides(&with_div).len(), 3);
        let plain = from_markdown("just text\n\nmore");
        assert_eq!(slides(&plain), vec![0..2]);
        assert_eq!(slides(&[]).len(), 1);
    }
}

#[cfg(test)]
mod callout_children_tests {
    use super::*;

    #[test]
    fn callout_list_items_become_children() {
        let b = parse_markdown("> [!NOTE]\n> 💡 **Title**\n>\n> - [ ] one\n> - [x] two\n");
        assert_eq!(b.len(), 3);
        assert!(matches!(b[0].kind, BlockKind::Callout { .. }));
        assert_eq!(b[0].text, "**Title**");
        assert_eq!(b[1].kind, BlockKind::Todo { checked: false });
        assert_eq!((b[1].depth, b[1].text.as_str()), (1, "one"));
        assert_eq!(b[2].kind, BlockKind::Todo { checked: true });
    }

    #[test]
    fn callout_heading_becomes_bold_title_and_colour_stays_on_callout() {
        let b = parse_markdown("> [!NOTE]\n> 💡 ### Head <!--c:yellow-->\n>\n> - item\n");
        let mut b = b;
        normalize_parsed(&mut b);
        assert_eq!(b[0].text, "**Head**");
        assert_eq!(b[0].color, "yellow");
        assert_eq!(b[1].color, "");
    }

    #[test]
    fn callout_with_children_round_trips() {
        let src = "> [!NOTE]\n> 💡 Title\n  - [ ] one\n  - [ ] two\n";
        let b = parse_markdown(src);
        let again = parse_markdown(&to_markdown(&b));
        let key = |v: &[Block]| {
            v.iter().map(|x| (x.kind.clone(), x.text.clone(), x.depth, x.color.clone())).collect::<Vec<_>>()
        };
        assert_eq!(key(&b), key(&again), "{}", to_markdown(&b));
    }

    #[test]
    fn quote_colour_round_trips() {
        let b = parse_markdown("> *Note* <!--c:bg-orange-->\n>\n");
        assert_eq!(b.len(), 1);
        assert!(matches!(b[0].kind, BlockKind::Quote));
        assert_eq!((b[0].text.as_str(), b[0].color.as_str()), ("*Note*", "bg-orange"));
        let again = parse_markdown(&to_markdown(&b));
        assert_eq!(again[0].color, "bg-orange");
    }
}
