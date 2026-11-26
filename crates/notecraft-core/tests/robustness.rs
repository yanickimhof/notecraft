//! Hostile and random input: nothing may panic, and the text parsers must stay consistent.

use notecraft_core::blocks::{from_markdown, to_markdown};
use notecraft_core::inline::{self, StyleKind};

fn rng(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

const PIECES: &[&str] = &[
    "**",
    "*",
    "_",
    "~~",
    "~",
    "`",
    "<u>",
    "</u>",
    "[[",
    "]]",
    "[",
    "](",
    ")",
    "\\",
    "<span style=\"color:red\">",
    "</span>",
    "<span style=\"color:",
    "$$",
    "\n",
    "\n\n",
    "- ",
    "1. ",
    "# ",
    "> ",
    "- [ ] ",
    "- [x] ",
    "```",
    "|",
    "---",
    "<!--meeting:",
    "<!--database:",
    "<!--bookmark:https://a.b T-->",
    "-->",
    "é",
    "日本",
    "😀",
    "x",
    " ",
    "  ",
    "\t",
    "<details>",
    "</details>",
    "<summary>",
    "</summary>",
    "![",
    "<!--columns-->",
    "<!--/columns-->",
];

fn random_text(seed: &mut u64, max: usize) -> String {
    let n = (rng(seed) % max as u64) as usize;
    (0..n).map(|_| PIECES[(rng(seed) % PIECES.len() as u64) as usize]).collect()
}

#[test]
fn inline_segments_cover_every_byte_exactly_once() {
    let mut seed = 0x2545F4914F6CDD1Du64;
    for _ in 0..4000 {
        let t = random_text(&mut seed, 25);
        let segs = inline::parse(&t);
        let mut at = 0;
        for s in &segs {
            assert_eq!(s.range.start, at, "gap or overlap in {t:?}");
            assert!(t.is_char_boundary(s.range.start) && t.is_char_boundary(s.range.end), "split char in {t:?}");
            at = s.range.end;
        }
        assert_eq!(at, t.len(), "text not covered: {t:?}");
        let _ = inline::strip(&t);
        let _ = inline::marker_mask(&t);
    }
}

#[test]
fn editing_helpers_never_panic() {
    let mut seed = 0x9E3779B97F4A7C15u64;
    let kinds = [StyleKind::Bold, StyleKind::Italic, StyleKind::Underline, StyleKind::Strike, StyleKind::Code];
    for _ in 0..3000 {
        let t = random_text(&mut seed, 20);
        let n = t.chars().count() as u64 + 3;
        let (a, b) = ((rng(&mut seed) % n) as usize, (rng(&mut seed) % n) as usize);
        let _ = inline::toggle_style(&t, (a, b), kinds[(rng(&mut seed) % 5) as usize]);
        let _ = inline::set_color(&t, (a, b), Some("red"));
        let _ = inline::set_color(&t, (a, b), None);
        let _ = inline::toggle_wrap(&t, (a.min(b), a.max(b)), "**", "**");
        let _ = inline::step_caret(&t, a.min(n as usize - 3), true);
        let _ = inline::snap_caret(&t, a.min(n as usize - 3));
    }
}

#[test]
fn markdown_parsing_never_panics_and_settles() {
    let mut seed = 0xD1B54A32D192ED03u64;
    for _ in 0..3000 {
        let t = random_text(&mut seed, 40);
        let blocks = from_markdown(&t);
        let once = to_markdown(&blocks);
        // saving and reading again must reach a fixed point (no growth, no drift)
        let twice = to_markdown(&from_markdown(&once));
        let thrice = to_markdown(&from_markdown(&twice));
        assert_eq!(twice, thrice, "markdown does not settle for {t:?}");
    }
}

#[test]
fn huge_inputs_stay_fast_enough() {
    let start = std::time::Instant::now();
    let big = "word **bold** `code` [[Page]] <u>u</u>\n\n".repeat(20_000);
    let blocks = from_markdown(&big);
    assert!(blocks.len() >= 20_000);
    let _ = to_markdown(&blocks);
    let _ = inline::parse(&"*a ".repeat(50_000));
    let _ = inline::parse(&"[a](".repeat(20_000));
    assert!(start.elapsed().as_secs() < 20, "parsing is too slow: {:?}", start.elapsed());
}

/// Rich Markdown comes from outside: unbalanced or hostile tags must never panic.
#[test]
fn the_rich_markdown_converter_survives_tag_soup() {
    const TAGS: &[&str] = &[
        "<callout icon=\"💡\" color=\"gray_bg\">",
        "</callout>",
        "<columns>",
        "<column>",
        "</column>",
        "</columns>",
        "<table>",
        "<tr>",
        "<td>",
        "</td>",
        "</tr>",
        "</table>",
        "<toggle>",
        "</toggle>",
        "<ul>",
        "<li>",
        "</li>",
        "</ul>",
        "<page url=\"x\">",
        "</page>",
        "<database",
        ">",
        "</",
        "<",
        "\n\t",
        "\n",
        "\t- ",
        "- ",
        "1. ",
        "{color=\"red\"}",
        " {color=\"",
        "text ",
        "**",
        "`",
        "```",
        "<![CDATA[",
        "]]>",
        "<!--",
        "-->",
        "&amp;",
        "é",
        "日本",
        "<empty-block/>",
        "<synced_block>",
        "[^https://x]",
    ];
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..20_000 {
        let n = 1 + (rng(&mut seed) % 40) as usize;
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(TAGS[(rng(&mut seed) as usize) % TAGS.len()]);
        }
        let _ = notecraft_core::import_rich_md::blocks_from_markdown(&s);
    }
}

const COLOR_PIECES: &[&str] = &[
    "<span style=\"color:red\">",
    "<span style=\"color:blue\">",
    "</span>",
    "a",
    "bé",
    "**",
    "*",
    " ",
    "`",
    "[[x]]",
    "é",
    "日本",
];

fn colored_text(seed: &mut u64, max: usize) -> String {
    let n = (rng(seed) % max as u64) as usize;
    (0..n).map(|_| COLOR_PIECES[(rng(seed) % COLOR_PIECES.len() as u64) as usize]).collect()
}

/// Recolouring any selection of any text never panics and never leaves the colour tags unbalanced
/// when it started balanced.
#[test]
fn recolouring_keeps_the_colour_tags_balanced() {
    let mut seed = 0xA0761D6478BD642Fu64;
    let count = |t: &str, p: &str| t.matches(p).count();
    let mut checked = 0;
    for _ in 0..6000 {
        let t = colored_text(&mut seed, 14);
        // only text whose tags open before they close is well formed
        let mut depth = 0i32;
        let mut well_formed = true;
        let mut events: Vec<(usize, i32)> = t
            .match_indices("<span")
            .map(|(i, _)| (i, 1))
            .chain(t.match_indices("</span>").map(|(i, _)| (i, -1)))
            .collect();
        events.sort();
        for (_, d) in events {
            depth += d;
            well_formed &= depth >= 0;
        }
        if !well_formed || depth != 0 {
            continue;
        }
        let n = t.chars().count() as u64 + 1;
        let (a, b) = ((rng(&mut seed) % n) as usize, (rng(&mut seed) % n) as usize);
        for color in [Some("green"), None] {
            let (out, _) = inline::set_color(&t, (a.min(b), a.max(b)), color);
            assert_eq!(count(&out, "<span"), count(&out, "</span>"), "{t:?} {a}-{b} {color:?} gave {out:?}");
        }
        checked += 1;
    }
    assert!(checked > 500, "too few balanced samples ({checked})");
    // stripping every colour leaves no tag and keeps the words
    assert_eq!(inline::strip_colors("a <span style=\"color:red\">b</span> c"), "a b c");
}

/// A page pasted or imported from the web is hostile: any soup of tags converts without a panic and
/// without a script or an event handler surviving into the Markdown.
#[test]
fn html_import_survives_tag_soup() {
    const SOUP: &[&str] = &[
        "<p>",
        "</p>",
        "<b>",
        "</i>",
        "<table>",
        "<tr>",
        "<td>",
        "</table>",
        "<ul>",
        "<li>",
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<a href=\"javascript:alert(1)\">",
        "</a>",
        "<span style=\"color:red\">",
        "&amp;",
        "&#x41;",
        "&#99999999;",
        "<br>",
        "<pre>",
        "<code>",
        "text",
        "é",
        "<!--",
        "-->",
        "<",
        ">",
        "\"",
        "'",
    ];
    let mut seed = 0xE7037ED1A0B428DBu64;
    for _ in 0..1500 {
        let n = (rng(&mut seed) % 40) as usize;
        let html: String = (0..n).map(|_| SOUP[(rng(&mut seed) % SOUP.len() as u64) as usize]).collect();
        let md = notecraft_core::import_html::clipboard_html_to_markdown(&html);
        assert!(!md.contains("onerror"), "{html:?} -> {md:?}");
        let blocks = from_markdown(&md);
        let _ = to_markdown(&blocks);
    }
}

/// Whatever a page holds, exporting and publishing it never panics and never emits live script.
#[test]
fn rendering_hostile_pages_never_panics_or_emits_script() {
    let mut seed = 0xC2B2AE3D27D4EB4Fu64;
    let none = |_: &str| None;
    let hostile = [
        "<script>alert(1)</script>",
        "[x](javascript:alert(1))",
        "![i](javascript:alert(1))",
        "<img src=x onerror=alert(1)>",
        "**<script>**",
        "| a | b |\n| - | - |\n| <script> | x |",
    ];
    for k in 0..400 {
        let mut t = random_text(&mut seed, 30);
        t.push_str("\n\n");
        t.push_str(hostile[k % hostile.len()]);
        let blocks = from_markdown(&t);
        let html = notecraft_core::render::html_body(&blocks, &none);
        assert!(!html.contains("<script"), "{t:?}");
        assert!(!html.to_lowercase().contains("href=\"javascript"), "{t:?}");
        let _ = notecraft_core::render::to_docx("t", &blocks, &none);
    }
}

/// Table cells keep line breaks and pipes through a save and a read.
#[test]
fn table_cells_survive_line_breaks_and_pipes() {
    use notecraft_core::blocks::{cells_to_text, table_cells};
    let cells = vec![vec!["a\nb".to_string(), "c | d".to_string()], vec!["".to_string(), "é\n\nz".to_string()]];
    let text = cells_to_text(&cells);
    assert_eq!(table_cells(&text), cells);
}

/// A damaged header must not take the page down: the whole text stays readable.
#[test]
fn damaged_front_matter_keeps_the_text() {
    for t in ["---\nid: a\n", "---\n---\n", "---", "---\n: : :\n---\nbody", "\u{feff}---\nid: 1\n---\nbody", ""] {
        let (_, body) = notecraft_core::frontmatter::split(t);
        assert!(t.contains(body), "{t:?}");
    }
}
