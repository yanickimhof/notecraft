//! Looks for the damage a lossy import leaves behind, so it is found at once and not weeks later.
//!
//! A lossy export (Markdown & CSV) drops the icon, the cover, the columns and the colours, and it
//! can cut a table row in two when a cell holds several paragraphs. The converters fix what they
//! can; this module checks the result page by page and names what still looks wrong:
//!
//! * a table row that ended up as plain text, or a table whose rows have different widths;
//! * rich-Markdown tags (`<columns>`, `<callout>`, `<span color>`…) left in the text;
//! * an image or attachment whose file is not in the workspace;
//! * a `[[link]]` to a page that does not exist;
//! * side-by-side columns that do not open and close properly.
//!
//! `notecraft verify <workspace>` prints the findings (exit code 2 when there are some) and
//! `notecraft import` / `rebody` print them right after the import.

use crate::blocks::{self, BlockKind};
use crate::inline;
use crate::page::Page;
use crate::workspace::Workspace;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// A line such as `| a | b |` that is not inside a table.
    StrayTableRow,
    /// Rows of one table with a different number of cells.
    RaggedTable,
    /// Rich-Markdown or HTML tags that should have been converted.
    RawMarkup,
    /// An image or file that is not in the workspace (or still points to a remote reference).
    MissingFile,
    /// A `[[link]]` to a title that no page has.
    DanglingLink,
    /// Columns that are not opened, separated or closed properly.
    BrokenColumns,
    /// A callout kept on the default icon whose text starts with its real icon.
    GluedCalloutIcon,
    /// A link to a `.md` file that no longer exists as a link to a page.
    DeadFileLink,
    /// A citation such as `[[1]](…)` left as a link.
    CitationLink,
    ColourSuffix,
}

impl Problem {
    pub fn label(self) -> &'static str {
        match self {
            Problem::StrayTableRow => "stray-table-row",
            Problem::RaggedTable => "ragged-table",
            Problem::RawMarkup => "raw-markup",
            Problem::MissingFile => "missing-file",
            Problem::DanglingLink => "dangling-link",
            Problem::BrokenColumns => "broken-columns",
            Problem::GluedCalloutIcon => "glued-callout-icon",
            Problem::DeadFileLink => "dead-file-link",
            Problem::CitationLink => "citation-link",
            Problem::ColourSuffix => "colour-suffix",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub page_id: String,
    pub page_title: String,
    pub problem: Problem,
    pub detail: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}] {}: {}", self.page_title, self.page_id, self.problem.label(), self.detail)
    }
}

/// Tags that only exist in rich Markdown or HTML; none should survive an import.
const RAW_TAGS: [&str; 11] = [
    "<columns",
    "<column ",
    "<column>",
    "<callout",
    "<span color",
    "<image ",
    "<table",
    "<tr>",
    "<td>",
    "<quote",
    "<page url",
];

fn short(text: &str) -> String {
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 60 {
        format!("{}…", one.chars().take(60).collect::<String>())
    } else {
        one
    }
}

/// `[[12]](…)` or `[12](…)` with only digits as the label: a citation.
fn has_citation_link(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'[' {
            let mut j = i + 1;
            while j < b.len() && b[j] == b'[' {
                j += 1;
            }
            let k = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > k && text[j..].starts_with("]](") {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// The target of the first `](…​.md)` link that is not a web address.
fn dead_md_link(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        let end = after.find(')').unwrap_or(after.len());
        let target = &after[..end];
        if target.to_lowercase().ends_with(".md") && !target.contains("://") {
            return Some(target.to_string());
        }
        rest = after;
    }
    None
}

/// Everything that looks wrong on one page.
pub fn check_page(ws: &Workspace, page: &Page) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    let mut add = |problem: Problem, detail: String| {
        out.push(Finding { page_id: page.meta.id.clone(), page_title: page.meta.title.clone(), problem, detail })
    };
    let mut depth = 0u32;
    let mut columns_ok = true;
    for (i, b) in page.blocks.iter().enumerate() {
        let n = i + 1;
        let verbatim = matches!(b.kind, BlockKind::Code { .. } | BlockKind::Equation);
        match &b.kind {
            BlockKind::Table => {
                let widths = blocks::table_row_widths(&b.text);
                if let (Some(lo), Some(hi)) = (widths.iter().min(), widths.iter().max()) {
                    if lo != hi {
                        let row = widths.iter().position(|w| w != hi).map(|r| r + 1).unwrap_or(1);
                        add(Problem::RaggedTable, format!("block {n}: rows have {lo} to {hi} cells (see row {row})"));
                    }
                }
            }
            BlockKind::Image { path } | BlockKind::File { path } => {
                if !path.contains("://") && !ws.root.join(path).is_file() {
                    add(Problem::MissingFile, format!("block {n}: {path} is not in the workspace"));
                }
            }
            BlockKind::ColumnsStart => {
                if depth > 0 {
                    columns_ok = false;
                    add(Problem::BrokenColumns, format!("block {n}: columns started inside columns"));
                }
                depth += 1;
            }
            BlockKind::ColumnBreak => {
                if depth == 0 {
                    columns_ok = false;
                    add(Problem::BrokenColumns, format!("block {n}: a column break outside columns"));
                }
            }
            BlockKind::ColumnsEnd => {
                if depth == 0 {
                    columns_ok = false;
                    add(Problem::BrokenColumns, format!("block {n}: columns closed but never opened"));
                } else {
                    depth -= 1;
                }
            }
            _ => {}
        }
        if verbatim || matches!(b.kind, BlockKind::Table) {
            continue;
        }
        let t = b.text.trim_start();
        if t.starts_with('|') && t.matches('|').count() >= 2 {
            add(Problem::StrayTableRow, format!("block {n}: {}", short(&b.text)));
        }
        if let Some(tag) = RAW_TAGS.iter().find(|tag| b.text.contains(**tag)) {
            add(Problem::RawMarkup, format!("block {n}: {tag} in “{}”", short(&b.text)));
        }
        if let BlockKind::Callout { icon } = &b.kind {
            if icon == "💡" && crate::import_rich_md::split_leading_emoji(&b.text).is_some() {
                add(Problem::GluedCalloutIcon, format!("block {n}: icon is inside the text ({})", short(&b.text)));
            }
        }
        if b.text.contains("{color=\"") {
            add(Problem::ColourSuffix, format!("block {n}: {}", short(&b.text)));
        }
        if has_citation_link(&b.text) {
            add(Problem::CitationLink, format!("block {n}: {}", short(&b.text)));
        }
        if let Some(target) = dead_md_link(&b.text) {
            add(Problem::DeadFileLink, format!("block {n}: link to {target}"));
        }
        if b.kind.has_rich_text() {
            for m in inline::mentions(&b.text) {
                if ws.resolve_title(&m).is_none() {
                    add(Problem::DanglingLink, format!("block {n}: [[{m}]] matches no page"));
                }
            }
        }
    }
    if depth > 0 && columns_ok {
        add(Problem::BrokenColumns, "columns opened but never closed".to_string());
    }
    out
}

/// Check every page of the workspace.
pub fn check_workspace(ws: &Workspace) -> Vec<Finding> {
    ws.pages.values().flat_map(|p| check_page(ws, p)).collect()
}

/// One line per problem kind, for a summary after an import.
pub fn summary(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "no problem found".to_string();
    }
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for f in findings {
        *counts.entry(f.problem.label()).or_default() += 1;
    }
    let parts: Vec<String> = counts.iter().map(|(k, v)| format!("{v} {k}")).collect();
    format!("{} problem(s): {}", findings.len(), parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::Block;
    use std::fs;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path()).unwrap();
        (dir, ws)
    }

    fn page_with(ws: &mut Workspace, md: &str) -> Page {
        let id = ws.create_page(None, "Test").unwrap();
        ws.pages.get_mut(&id).unwrap().blocks = blocks::from_markdown(md);
        ws.pages[&id].clone()
    }

    fn kinds(f: &[Finding]) -> Vec<Problem> {
        f.iter().map(|f| f.problem).collect()
    }

    #[test]
    fn a_clean_page_has_no_finding() {
        let (_d, mut ws) = ws();
        let p = page_with(&mut ws, "# T\n\nText with **bold**.\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n");
        assert!(check_page(&ws, &p).is_empty());
        assert_eq!(summary(&[]), "no problem found");
    }

    #[test]
    fn a_table_row_left_as_text_is_found() {
        let (_d, mut ws) = ws();
        let mut p = page_with(&mut ws, "Intro\n");
        p.blocks.push(Block::new(BlockKind::Paragraph, "| 3 | Salon | Sud |".to_string()));
        assert_eq!(kinds(&check_page(&ws, &p)), vec![Problem::StrayTableRow]);
    }

    #[test]
    fn a_table_with_a_short_row_is_found() {
        let (_d, mut ws) = ws();
        let mut p = page_with(&mut ws, "Intro\n");
        p.blocks.push(Block::new(BlockKind::Table, "| a | b |\n| --- | --- |\n| 1 | 2 |\n| 3 |".to_string()));
        let f = check_page(&ws, &p);
        assert_eq!(kinds(&f), vec![Problem::RaggedTable], "{f:?}");
        assert!(f[0].detail.contains("row 3"), "{}", f[0]);
    }

    #[test]
    fn rich_tags_left_in_the_text_are_found_but_not_in_code() {
        let (_d, mut ws) = ws();
        let mut p = page_with(&mut ws, "<callout color=\"red_bg\">x</callout>\n\n<span color=\"red\">y</span>\n");
        p.blocks.push(Block::new(BlockKind::Code { lang: String::new() }, "<columns>".to_string()));
        let f = check_page(&ws, &p);
        assert!(!f.is_empty() && f.iter().all(|f| f.problem == Problem::RawMarkup), "{f:?}");
        assert!(f.iter().all(|f| !f.detail.contains("block 3")), "code is left alone: {f:?}");
    }

    #[test]
    fn a_missing_or_unconverted_file_is_found() {
        let (d, mut ws) = ws();
        let mut p = page_with(&mut ws, "Intro\n");
        p.blocks.push(Block::new(BlockKind::Image { path: "assets/gone.png".into() }, "x".to_string()));
        p.blocks.push(Block::new(
            BlockKind::File { path: "assets/f.pdf".into() },
            "f.pdf".to_string(),
        ));
        assert_eq!(kinds(&check_page(&ws, &p)), vec![Problem::MissingFile, Problem::MissingFile]);
        fs::create_dir_all(d.path().join("assets")).unwrap();
        fs::write(d.path().join("assets/gone.png"), b"png").unwrap();
        assert_eq!(kinds(&check_page(&ws, &p)), vec![Problem::MissingFile], "only the unconverted file is left");
    }

    #[test]
    fn a_link_to_no_page_is_found() {
        let (_d, mut ws) = ws();
        let p = page_with(&mut ws, "See [[Nowhere]] and [[Test]].\n");
        let f = check_page(&ws, &p);
        assert_eq!(kinds(&f), vec![Problem::DanglingLink], "{f:?}");
        assert!(f[0].detail.contains("Nowhere"));
    }

    #[test]
    fn columns_must_open_and_close() {
        let (_d, mut ws) = ws();
        let ok = page_with(&mut ws, "<!--columns-->\n\nA\n\n<!--column-->\n\nB\n\n<!--/columns-->\n");
        assert!(check_page(&ws, &ok).is_empty());
        let open = page_with(&mut ws, "<!--columns-->\n\nA\n");
        assert_eq!(kinds(&check_page(&ws, &open)), vec![Problem::BrokenColumns]);
        let stray = page_with(&mut ws, "A\n\n<!--/columns-->\n");
        assert_eq!(kinds(&check_page(&ws, &stray)), vec![Problem::BrokenColumns]);
    }

    // ---- the import itself: what an export sends must come out clean -----------------------------

    #[test]
    fn a_callout_with_its_icon_glued_to_the_text_is_found() {
        let (_d, mut ws) = ws();
        let p = page_with(&mut ws, "> [!NOTE]\n> 💡 ⚠️ Careful\n");
        assert!(kinds(&check_page(&ws, &p)).contains(&Problem::GluedCalloutIcon));
        let p = page_with(&mut ws, "> [!NOTE]\n> Plain text\n");
        assert!(!kinds(&check_page(&ws, &p)).contains(&Problem::GluedCalloutIcon));
    }

    #[test]
    fn a_citation_link_and_a_dead_md_link_are_found() {
        let (_d, mut ws) = ws();
        let p = page_with(&mut ws, "See [[3]](a/b.md) and [file](other.md) and [web](https://x.ch/a.md).\n");
        let k = kinds(&check_page(&ws, &p));
        assert!(k.contains(&Problem::CitationLink), "{k:?}");
        assert!(k.contains(&Problem::DeadFileLink), "{k:?}");
        let p = page_with(&mut ws, "Fine [1] and [web](https://x.ch/a.md).\n");
        let k = kinds(&check_page(&ws, &p));
        assert!(!k.contains(&Problem::CitationLink) && !k.contains(&Problem::DeadFileLink), "{k:?}");
    }

    #[test]
    fn rich_markdown_keeps_columns_colours_and_files() {
        let src = tempfile::tempdir().unwrap();
        fs::write(src.path().join("shot one.png"), b"png").unwrap();
        let md = "# Page\n\n<columns>\n<column ratio=\"50\">\n<image source=\"shot%20one.png\">Cap</image>\n</column>\n<column ratio=\"50\">\n<callout color=\"brown_bg\">\n### Heading\n\nBody with <span color=\"red\">red</span> text\n</callout>\n</column>\n</columns>\n\n| A | B |\n| --- | --- |\n| <span color=\"red\">x</span> | 2 |\n";
        let (_d, mut ws) = ws();
        let id = ws.create_page(None, "Page").unwrap();
        crate::import_export::replace_page_body(&mut ws, &id, md, src.path()).unwrap();
        let page = &ws.pages[&id];
        let f = check_page(&ws, page);
        assert!(f.is_empty(), "{f:?}\n{:?}", page.blocks);
        let k: Vec<&BlockKind> = page.blocks.iter().map(|b| &b.kind).collect();
        assert!(
            k.contains(&&BlockKind::ColumnsStart)
                && k.contains(&&BlockKind::ColumnBreak)
                && k.contains(&&BlockKind::ColumnsEnd),
            "{k:?}"
        );
        assert!(page.blocks.iter().any(|b| matches!(b.kind, BlockKind::Image { .. })), "{k:?}");
        let call = page.blocks.iter().find(|b| matches!(b.kind, BlockKind::Callout { .. })).expect("a callout");
        assert_eq!(call.color, "bg-brown", "{call:?}");
    }
}
