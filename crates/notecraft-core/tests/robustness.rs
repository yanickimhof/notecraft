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
