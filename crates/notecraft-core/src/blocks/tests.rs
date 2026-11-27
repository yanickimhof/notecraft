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
    let key =
        |v: &[Block]| v.iter().map(|x| (x.kind.clone(), x.text.clone(), x.depth, x.color.clone())).collect::<Vec<_>>();
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
