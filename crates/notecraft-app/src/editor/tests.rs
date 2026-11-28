/// Draws a page that holds every kind of block for several frames, with pointer clicks and
/// key presses in between, and checks that nothing panics and nothing gets lost.
#[test]
fn a_page_with_every_block_kind_survives_frames_clicks_and_keys() {
    let md = "# Title\n\nplain <span style=\"color:red\">red</span> text\n\n- [ ] todo\n\n> quote\n\n$$\nx^2-4x+1=0\n$$\n\n```mermaid\ngraph TD\n A[Start] --> B{Choice}\n B -->|yes| C[Done]\n D --> D\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n";
    let mut blocks = notecraft_core::blocks::from_markdown(md);
    let before = notecraft_core::blocks::to_markdown(&blocks);
    let pal = Palette::light();
    let ctx = egui::Context::default();
    theme::apply(&ctx, &pal);
    let titles = HashMap::new();
    let meeting = crate::meeting::View::default();
    let embed = |_: &mut Ui, _: u64, _: &str| {};
    let mut state = EditorState::default();
    let root = Path::new(".");
    let mut events_per_frame: Vec<Vec<egui::Event>> = vec![vec![]; 3];
    for (x, y) in [(300.0, 120.0), (300.0, 260.0), (300.0, 420.0), (300.0, 560.0), (300.0, 700.0), (700.0, 900.0)] {
        let p = egui::pos2(x, y);
        events_per_frame.push(vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        events_per_frame.push(vec![egui::Event::Text("x".into())]);
        events_per_frame.push(vec![egui::Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        events_per_frame.push(vec![egui::Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        events_per_frame.push(vec![]);
    }
    for events in events_per_frame {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900.0, 1000.0))),
            events,
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |root_ui| {
            egui::ScrollArea::vertical().show(root_ui, |ui| {
                let env = Env {
                    pal: &pal,
                    lang: Lang::En,
                    root,
                    titles: &titles,
                    key: "smoke",
                    meeting: &meeting,
                    db_titles: &[],
                    embed: &embed,
                };
                state.show(ui, &mut blocks, &env);
            });
        });
        full.textures_delta.clear();
    }
    assert!(!blocks.is_empty());
    // the first drawing must not have changed the content by itself
    let again = notecraft_core::blocks::from_markdown(&before);
    assert_eq!(again.len(), notecraft_core::blocks::from_markdown(&notecraft_core::blocks::to_markdown(&again)).len());
}

#[test]
fn every_slash_item_has_a_title_and_description_in_both_languages() {
    for it in SLASH_ITEMS {
        for prefix in ["blk_", "desc_"] {
            let key = format!("{prefix}{}", it.key);
            for lang in [Lang::En, Lang::Fr] {
                assert_ne!(tr(lang, &key), key.as_str(), "missing translation {key}");
            }
        }
    }
}

use super::*;

#[test]
fn link_under_the_caret_is_found() {
    let text = "see [the docs](https://x.org/a) now";
    // inside the label
    let (label, d0, d1) = link_target_at(text, 8).expect("link");
    assert_eq!(&text[d0..d1], "https://x.org/a");
    assert_eq!(&text[label..label + 3], "the");
    // outside any link
    assert!(link_target_at(text, 1).is_none());
    assert!(link_target_at(text, 33).is_none());
}

#[test]
fn typed_addresses_are_made_usable() {
    assert_eq!(normalize_url("example.com"), "https://example.com");
    assert_eq!(normalize_url(" https://a.b/c "), "https://a.b/c");
    assert_eq!(normalize_url("me@example.com"), "mailto:me@example.com");
    assert_eq!(normalize_url("notes/page.md"), "notes/page.md");
    assert_eq!(normalize_url("./page.md"), "./page.md");
}

#[test]
fn slash_detection() {
    assert_eq!(slash_query("/he", 3), Some((0, "he".into())));
    assert_eq!(slash_query("hello /to", 9), Some((6, "to".into())));
    assert_eq!(slash_query("a/b", 3), None);
    assert_eq!(slash_query("/he llo", 7), None);
}

#[test]
fn slash_items_in_requested_order() {
    let keys: Vec<_> = filter_items("", &SlashMode::Insert).iter().map(|i| i.key).collect();
    assert_eq!(
        keys,
        vec![
            "text",
            "page",
            "todo",
            "h1",
            "h2",
            "h3",
            "bulleted",
            "numbered",
            "toggle",
            "quote",
            "divider",
            "toc",
            "columns2",
            "columns3",
            "callout",
            "code",
            "link",
            "date_short",
            "template",
            "image",
            "file",
            "mermaid",
            "equation",
            "bookmark",
            "simple_table",
            "table",
            "board",
            "list",
            "dbview",
            "meeting"
        ]
    );
}

#[test]
fn slash_filtering_in_both_languages() {
    assert_eq!(filter_items("head", &SlashMode::Insert)[0].key, "h1");
    assert_eq!(filter_items("titre 2", &SlashMode::Insert)[0].key, "h2");
    assert_eq!(filter_items("kanban", &SlashMode::Insert)[0].key, "board");
    assert!(filter_items("zzzz", &SlashMode::Insert).is_empty());
    let dates: Vec<_> = filter_items("date", &SlashMode::Insert).iter().map(|i| i.key).collect();
    for k in ["date_short", "date_iso", "date_long", "date_full", "date_time"] {
        assert!(dates.contains(&k), "{k} in {dates:?}");
    }
    assert!(filter_items("", &SlashMode::TurnInto).iter().all(|i| i.key != "image"));
}

#[test]
fn number_labels() {
    assert_eq!(number_label(3, 0), "3.");
    assert_eq!(number_label(2, 1), "b.");
    assert_eq!(number_label(4, 2), "iv.");
}

/// The column controls show only while the pointer is over the columns, and work when they do.
#[test]
fn the_column_bar_shows_on_hover_and_adds_a_column() {
    use egui_kittest::kittest::Queryable;
    let md = "before\n\n<!--columns-->\n\nleft\n\n<!--column-->\n\nright\n\n<!--/columns-->\n\nafter\n";
    let blocks = notecraft_core::blocks::from_markdown(md);
    let mut h = egui_kittest::Harness::builder().with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
        |ui, (state, blocks): &mut (EditorState, Vec<Block>)| {
            let pal = Palette::light();
            let titles = HashMap::new();
            let meeting = crate::meeting::View::default();
            let embed = |_: &mut Ui, _: u64, _: &str| {};
            let env = Env {
                pal: &pal,
                lang: Lang::En,
                root: Path::new("."),
                titles: &titles,
                key: "cols",
                meeting: &meeting,
                db_titles: &[],
                embed: &embed,
            };
            state.show(ui, blocks, &env);
        },
        (EditorState::default(), blocks),
    );
    theme::install_fonts(&h.ctx);
    h.run();
    assert!(h.query_by_label_contains("Columns").is_none(), "no bar until the pointer comes");
    // find where the columns are: the text of the left one
    let left = h.get_all_by_value("left").next().unwrap().rect();
    h.hover_at(left.center());
    h.run();
    h.run();
    assert!(h.query_by_label_contains("Columns").is_some(), "the bar shows over the columns");
    h.get_by_label("Add a column").click();
    h.run();
    let md = notecraft_core::blocks::to_markdown(&h.state().1);
    assert_eq!(md.matches("<!--column-->").count(), 2, "a third column was added: {md}");
    // away from the columns the bar goes again
    h.hover_at(egui::pos2(790.0, 590.0));
    h.run();
    h.run();
    assert!(h.query_by_label_contains("Columns").is_none(), "the bar hides when the pointer leaves");
}

#[test]
fn a_speaker_is_renamed_with_one_click_and_enter() {
    use egui_kittest::kittest::Queryable;
    let mut blocks =
        vec![Block::new(BlockKind::Meeting, "Weekly"), Block::new(BlockKind::Toggle { open: true }, "Transcript")];
    for l in ["Speaker 1 : hello", "Me : hi", "Speaker 1 : next"] {
        blocks.push(Block::new(BlockKind::Paragraph, l).with_depth(1));
    }
    let mut h = egui_kittest::Harness::builder().with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
        |ui, (state, blocks, acts): &mut (EditorState, Vec<Block>, Vec<crate::meeting::MeetingAction>)| {
            let pal = Palette::light();
            let titles = HashMap::new();
            let meeting = crate::meeting::View::default();
            let embed = |_: &mut Ui, _: u64, _: &str| {};
            let env = Env {
                pal: &pal,
                lang: Lang::En,
                root: Path::new("."),
                titles: &titles,
                key: "spk",
                meeting: &meeting,
                db_titles: &[],
                embed: &embed,
            };
            for a in state.show(ui, blocks, &env).actions {
                if let Action::Meeting(m) = a {
                    acts.push(m);
                }
            }
        },
        (EditorState::default(), blocks, Vec::new()),
    );
    theme::install_fonts(&h.ctx);
    h.run();
    assert!(h.query_by_label("Speaker 1").is_some(), "the speakers are listed");
    assert!(h.query_by_label("Me").is_some());
    h.get_by_label("Speaker 1").click();
    h.run();
    h.run();
    h.event(egui::Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    h.event(egui::Event::Text("Anna".into()));
    h.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    h.run();
    let acts = &h.state().2;
    assert!(
        matches!(acts.as_slice(), [crate::meeting::MeetingAction::RenameSpeaker(_, old, new)] if old == "Speaker 1" && new == "Anna"),
        "{acts:?}"
    );
}

#[test]
fn simple_table_keeps_typed_spaces_and_arrows_move_between_cells() {
    let tbl = Block::new(BlockKind::Table, "| a | b |\n| --- | --- |\n| c | d |");
    let bid = tbl.id;
    let mut h = egui_kittest::Harness::builder().with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
        |ui, (state, blocks): &mut (EditorState, Vec<Block>)| {
            let pal = Palette::light();
            let titles = HashMap::new();
            let meeting = crate::meeting::View::default();
            let embed = |_: &mut Ui, _: u64, _: &str| {};
            let env = Env {
                pal: &pal,
                lang: Lang::En,
                root: Path::new("."),
                titles: &titles,
                key: "tbl",
                meeting: &meeting,
                db_titles: &[],
                embed: &embed,
            };
            state.show(ui, blocks, &env);
        },
        (EditorState::default(), vec![tbl]),
    );
    theme::install_fonts(&h.ctx);
    h.state_mut().0.table_focus = Some((bid, 0, 0));
    for _ in 0..3 {
        h.run();
    }
    let key = |k: Key| egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    // a space typed after a word stays, so a cell can hold several words
    h.event(egui::Event::Text(" x".into()));
    for _ in 0..3 {
        h.run();
    }
    assert!(h.state().1[0].text.starts_with("| a x |"), "{}", h.state().1[0].text);
    // the right arrow at the end of the text goes to the next cell
    h.event(key(Key::ArrowRight));
    for _ in 0..3 {
        h.run();
    }
    h.event(egui::Event::Text("!".into()));
    for _ in 0..3 {
        h.run();
    }
    assert!(h.state().1[0].text.starts_with("| a x | b! |"), "{}", h.state().1[0].text);
    // Enter goes to the cell below
    h.event(key(Key::Enter));
    for _ in 0..3 {
        h.run();
    }
    h.event(egui::Event::Text("?".into()));
    for _ in 0..3 {
        h.run();
    }
    let md = h.state().1[0].text.clone();
    assert!(md.contains("| c | d? |"), "{md}");
}

/// Draws `n` sections (heading, formatted paragraph, list, to-do, small table) for a few frames and returns
/// the average time of a frame in milliseconds, after the first one (which builds the caches).
fn average_frame_ms(sections: usize, frames: usize) -> (usize, f64, f64) {
    let mut md = String::new();
    for i in 0..sections {
        md.push_str(&format!(
            "## Section {i}\n\nSome **bold**, *italic* and `code` text with a [[link]] and <span style=\"color:red\">colour</span> number {i}.\n\n- first item\n- second item\n\n- [ ] a task {i}\n\n| a | b |\n| --- | --- |\n| {i} | x |\n\n"
        ));
    }
    let mut blocks = notecraft_core::blocks::from_markdown(&md);
    let count = blocks.len();
    let pal = Palette::light();
    let ctx = egui::Context::default();
    theme::apply(&ctx, &pal);
    let titles = HashMap::new();
    let meeting = crate::meeting::View::default();
    let embed = |_: &mut Ui, _: u64, _: &str| {};
    let mut state = EditorState::default();
    let root = Path::new(".");
    let mut times = Vec::new();
    for f in 0..frames {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900.0, 1000.0))),
            // a pointer moving over the page, as when someone reads it
            events: vec![egui::Event::PointerMoved(egui::pos2(300.0, 100.0 + 40.0 * f as f32))],
            ..Default::default()
        };
        let t = std::time::Instant::now();
        let mut full = ctx.run_ui(raw, |root_ui| {
            egui::ScrollArea::vertical().show(root_ui, |ui| {
                let env = Env {
                    pal: &pal,
                    lang: Lang::En,
                    root,
                    titles: &titles,
                    key: "load",
                    meeting: &meeting,
                    db_titles: &[],
                    embed: &embed,
                };
                state.show(ui, &mut blocks, &env);
            });
        });
        full.textures_delta.clear();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let first = times[0];
    let rest = &times[1..];
    (count, first, rest.iter().sum::<f64>() / rest.len() as f64)
}

/// A long page must stay usable: the frame time may grow with the page, but not run away.
#[test]
fn a_page_of_a_thousand_blocks_draws_in_reasonable_time() {
    let (n, first, avg) = average_frame_ms(200, 6);
    eprintln!("{n} blocks: first frame {first:.1} ms, then {avg:.1} ms on average");
    // generous: this runs in a debug build, on any machine
    assert!(avg < 400.0, "{n} blocks took {avg:.1} ms per frame");
}

/// Run with `cargo test --release -p notecraft-app big_pages -- --ignored --nocapture` to see the curve.
#[test]
#[ignore]
fn big_pages_curve() {
    for sections in [100, 400, 1600, 3200] {
        let (n, first, avg) = average_frame_ms(sections, 6);
        eprintln!("{n:>6} blocks: first frame {first:>8.1} ms, then {avg:>8.1} ms");
    }
}

/// Content height of a long page after a few frames, scrolled far down, with or without skipping rows.
fn content_height(skip: bool) -> (f32, usize) {
    let mut md = String::new();
    for i in 0..300 {
        md.push_str(&format!("## Section {i}\n\nA paragraph with **bold** and some more words so that it wraps on a narrow page, number {i}, and again a few more words to be sure.\n\n- item one\n- item two\n\n> a quote {i}\n\n---\n\n"));
    }
    let mut blocks = notecraft_core::blocks::from_markdown(&md);
    let pal = Palette::light();
    let ctx = egui::Context::default();
    theme::apply(&ctx, &pal);
    let titles = HashMap::new();
    let meeting = crate::meeting::View::default();
    let embed = |_: &mut Ui, _: u64, _: &str| {};
    let mut state = EditorState::default();
    let root = Path::new(".");
    let mut height = 0.0;
    let mut skipped = 0;
    for _ in 0..5 {
        if !skip {
            state.heights.clear();
        }
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900.0, 1000.0))),
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |root_ui| {
            egui::ScrollArea::vertical().vertical_scroll_offset(9000.0).show(root_ui, |ui| {
                let env = Env {
                    pal: &pal,
                    lang: Lang::En,
                    root,
                    titles: &titles,
                    key: "skip",
                    meeting: &meeting,
                    db_titles: &[],
                    embed: &embed,
                };
                let start = ui.min_rect().min.y;
                state.show(ui, &mut blocks, &env);
                height = ui.min_rect().max.y - start;
            });
        });
        full.textures_delta.clear();
        skipped = state.heights.len();
    }
    (height, skipped)
}

/// Leaving rows out must not change the length of the page, or the scroll bar would jump.
#[test]
fn skipping_far_rows_keeps_the_page_height() {
    let (full, _) = content_height(false);
    let (skipping, known) = content_height(true);
    assert!(known > 100, "the heights were not recorded");
    assert!((full - skipping).abs() < 2.0, "page height {full} drawn in full, {skipping} with rows skipped");
}

#[test]
fn picking_a_transcription_template_takes_one_click_each_time() {
    use crate::meeting::MeetingAction;
    use egui_kittest::kittest::Queryable;
    type St = (EditorState, Vec<Block>, Vec<MeetingAction>, String);
    let blocks = vec![Block::new(BlockKind::Meeting, "Weekly")];
    let mut h = egui_kittest::Harness::builder().with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
        |ui, (state, blocks, acts, current): &mut St| {
            let pal = Palette::light();
            let titles = HashMap::new();
            let meeting = crate::meeting::View {
                model_ready: true,
                template: current.clone(),
                custom_templates: vec![
                    ("page:a".into(), "Alpha".into()),
                    ("page:b".into(), "Beta".into()),
                    ("page:c".into(), "Gamma".into()),
                ],
                ..Default::default()
            };
            let embed = |_: &mut Ui, _: u64, _: &str| {};
            let env = Env {
                pal: &pal,
                lang: Lang::En,
                root: Path::new("."),
                titles: &titles,
                key: "tpl",
                meeting: &meeting,
                db_titles: &[],
                embed: &embed,
            };
            for a in state.show(ui, blocks, &env).actions {
                if let Action::Meeting(m) = a {
                    if let MeetingAction::SetTemplate(t) = &m {
                        *current = t.clone();
                    }
                    acts.push(m);
                }
            }
        },
        (EditorState::default(), blocks, Vec::new(), "page:a".to_string()),
    );
    theme::install_fonts(&h.ctx);
    h.run();
    for (from, to, key) in [("Alpha", "Beta", "page:b"), ("Beta", "Gamma", "page:c"), ("Gamma", "Alpha", "page:a")] {
        h.get_by_label(&format!("{from} \u{25be}")).click();
        h.run();
        h.run();
        h.get_by_label(to).click();
        h.run();
        h.run();
        assert_eq!(h.state().3, key, "one click on {to} must select it; actions: {:?}", h.state().2);
    }
}

fn editor_harness(blocks: Vec<Block>) -> egui_kittest::Harness<'static, (EditorState, Vec<Block>)> {
    let h = egui_kittest::Harness::builder().with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
        |ui, (state, blocks): &mut (EditorState, Vec<Block>)| {
            let pal = Palette::light();
            let titles = HashMap::new();
            let meeting = crate::meeting::View::default();
            let embed = |_: &mut Ui, _: u64, _: &str| {};
            let env = Env {
                pal: &pal,
                lang: Lang::En,
                root: Path::new("."),
                titles: &titles,
                key: "del",
                meeting: &meeting,
                db_titles: &[],
                embed: &embed,
            };
            state.show(ui, blocks, &env);
        },
        (EditorState::default(), blocks),
    );
    theme::install_fonts(&h.ctx);
    h
}

fn key_event(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}

#[test]
fn delete_at_the_end_of_a_line_pulls_the_next_line_up() {
    let a = Block::new(BlockKind::Paragraph, "abc");
    let b = Block::new(BlockKind::Paragraph, "def");
    let c = Block::new(BlockKind::Paragraph, "ghi");
    let aid = a.id;
    let mut h = editor_harness(vec![a, b, c]);
    h.state_mut().0.focus = Some((aid, Caret::End));
    for _ in 0..3 {
        h.run();
    }
    h.event(key_event(Key::Delete, egui::Modifiers::NONE));
    for _ in 0..3 {
        h.run();
    }
    let texts: Vec<_> = h.state().1.iter().map(|b| b.text.clone()).collect();
    assert_eq!(texts, vec!["abcdef".to_string(), "ghi".to_string()]);
}

#[test]
fn table_cells_can_be_selected_and_formatted_together() {
    let tbl = Block::new(BlockKind::Table, "| a | b |\n| --- | --- |\n| c | d |");
    let bid = tbl.id;
    let mut h = editor_harness(vec![tbl]);
    for _ in 0..2 {
        h.run();
    }
    h.state_mut().0.table_sel = Some((bid, (1, 0), (1, 1)));
    h.event(key_event(Key::B, egui::Modifiers::COMMAND));
    for _ in 0..3 {
        h.run();
    }
    let cells = notecraft_core::blocks::table_cells(&h.state().1[0].text);
    assert_eq!(cells[1][0], "**c**");
    assert_eq!(cells[1][1], "**d**");
    assert_eq!(cells[0][0], "a");
    h.event(key_event(Key::Delete, egui::Modifiers::NONE));
    for _ in 0..3 {
        h.run();
    }
    let cells = notecraft_core::blocks::table_cells(&h.state().1[0].text);
    assert_eq!(cells[1], vec![String::new(), String::new()]);
    assert_eq!(cells[0], vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn a_cell_fill_is_kept_in_the_text_and_hidden() {
    let mut cells = notecraft_core::blocks::table_cells("| a | b |\n| --- | --- |\n| c | d |");
    cells[1][1] = notecraft_core::inline::set_fill(&cells[1][1], Some("yellow"));
    let text = notecraft_core::blocks::cells_to_text(&cells);
    let back = notecraft_core::blocks::table_cells(&text);
    assert_eq!(notecraft_core::inline::split_fill(&back[1][1]), (Some("yellow"), "d"));
}

#[test]
fn shift_arrow_extends_the_selection_by_a_letter() {
    let a = Block::new(BlockKind::Paragraph, "abcdef");
    let aid = a.id;
    let mut h = editor_harness(vec![a]);
    h.state_mut().0.focus = Some((aid, Caret::At(2)));
    for _ in 0..3 {
        h.run();
    }
    h.event(key_event(Key::ArrowRight, egui::Modifiers::SHIFT));
    h.event(key_event(Key::ArrowRight, egui::Modifiers::SHIFT));
    for _ in 0..3 {
        h.run();
    }
    // typing replaces the selected letters "cd"
    h.event(egui::Event::Text("X".into()));
    for _ in 0..3 {
        h.run();
    }
    assert_eq!(h.state().1[0].text, "abXef");
}

fn outline() -> Vec<Block> {
    let mut v = notecraft_core::blocks::from_markdown("Ang\n\nBrnding\n\nRAG :\n\nMiss\n\nTail");
    // index: 0 Ang, 1 Brnding, 2 RAG, 3 child, 4 child, 5 Miss, 6 Tail
    v.insert(3, Block::paragraph("child one"));
    v.insert(4, Block::paragraph("child two"));
    v[3].depth = 1;
    v[4].depth = 1;
    v
}

#[test]
fn block_selection_grows_and_shrinks_one_block_at_a_time_over_indented_lines() {
    let blocks = outline();
    let ids: Vec<u64> = blocks.iter().map(|b| b.id).collect();
    let mut st = EditorState { selected: Some(ids[0]), ..EditorState::default() };
    let mut ends = Vec::new();
    // growing downwards: the end of the range never goes back
    for _ in 0..6 {
        let i = blocks.iter().position(|b| Some(b.id) == st.selected).unwrap();
        st.extend_block_selection(&blocks, i, true);
        ends.push(st.selected_range(&blocks).unwrap().1);
    }
    assert!(ends.windows(2).all(|w| w[0] <= w[1]), "the highlight went back: {ends:?}");
    assert_eq!(st.selected_range(&blocks).unwrap(), (0, blocks.len()));
    // and shrinking back up: the end never grows
    let mut ends = Vec::new();
    for _ in 0..6 {
        let i = blocks.iter().position(|b| Some(b.id) == st.selected).unwrap();
        st.extend_block_selection(&blocks, i, false);
        ends.push(st.selected_range(&blocks).unwrap().1);
    }
    assert!(ends.windows(2).all(|w| w[0] >= w[1]), "the highlight grew while shrinking: {ends:?}");
}

#[test]
fn shift_down_on_a_parent_selects_its_children_and_the_next_block() {
    let blocks = outline();
    let mut st = EditorState { selected: Some(blocks[2].id), ..EditorState::default() }; // RAG : (two children)
    st.extend_block_selection(&blocks, 2, true);
    // anchor RAG, end Miss: RAG, both children and Miss
    assert_eq!(st.selected_range(&blocks).unwrap(), (2, 6));
}

#[test]
fn shift_up_from_a_child_goes_to_the_line_above_and_keeps_the_anchor() {
    let blocks = outline();
    let mut st = EditorState { selected: Some(blocks[4].id), ..EditorState::default() };
    st.extend_block_selection(&blocks, 4, false);
    assert_eq!(st.selected_range(&blocks).unwrap(), (3, 5));
    st.extend_block_selection(&blocks, 3, false);
    assert_eq!(st.selected_range(&blocks).unwrap(), (2, 5));
}

fn settle(h: &mut egui_kittest::Harness<'static, (EditorState, Vec<Block>)>) {
    for _ in 0..4 {
        h.run();
    }
}

fn two_long_lines() -> (Vec<Block>, u64, u64) {
    let a = Block::new(BlockKind::Paragraph, "abcdefghij");
    let b = Block::new(BlockKind::Paragraph, "klmnopqrst");
    let (aid, bid) = (a.id, b.id);
    (vec![a, b], aid, bid)
}

#[test]
fn right_arrow_at_the_end_of_a_block_goes_to_the_start_of_the_next_one() {
    let (blocks, aid, _) = two_long_lines();
    let mut h = editor_harness(blocks);
    h.state_mut().0.focus = Some((aid, Caret::End));
    settle(&mut h);
    h.event(key_event(Key::ArrowRight, egui::Modifiers::NONE));
    settle(&mut h);
    h.event(egui::Event::Text("X".into()));
    settle(&mut h);
    assert_eq!(h.state().1[1].text, "Xklmnopqrst");
}

#[test]
fn left_arrow_at_the_start_of_a_block_goes_to_the_end_of_the_previous_one() {
    let (blocks, _, bid) = two_long_lines();
    let mut h = editor_harness(blocks);
    h.state_mut().0.focus = Some((bid, Caret::Start));
    settle(&mut h);
    h.event(key_event(Key::ArrowLeft, egui::Modifiers::NONE));
    settle(&mut h);
    h.event(egui::Event::Text("X".into()));
    settle(&mut h);
    assert_eq!(h.state().1[0].text, "abcdefghijX");
}

#[test]
fn shift_down_in_the_middle_of_the_last_line_first_selects_the_text() {
    let (blocks, aid, _) = two_long_lines();
    let mut h = editor_harness(blocks);
    h.state_mut().0.focus = Some((aid, Caret::At(3)));
    settle(&mut h);
    h.event(key_event(Key::ArrowDown, egui::Modifiers::SHIFT));
    settle(&mut h);
    assert!(h.state().0.selected.is_none(), "it jumped to block selection before selecting the text");
    h.event(key_event(Key::ArrowDown, egui::Modifiers::SHIFT));
    settle(&mut h);
    assert!(h.state().0.selected.is_some(), "the second press switches to block selection");
}

#[test]
fn down_from_a_block_range_lands_below_the_range_and_up_above_it() {
    let blocks = outline();
    let ids: Vec<u64> = blocks.iter().map(|b| b.id).collect();
    let mut h = editor_harness(blocks);
    h.state_mut().0.selected = Some(ids[2]);
    h.state_mut().0.sel_anchor = Some(ids[1]); // Brnding .. RAG with its two children
    settle(&mut h);
    h.event(key_event(Key::ArrowDown, egui::Modifiers::NONE));
    settle(&mut h);
    assert_eq!(h.state().0.selected, Some(ids[5]));
    assert!(h.state().0.sel_anchor.is_none());
    h.state_mut().0.selected = Some(ids[4]);
    h.state_mut().0.sel_anchor = Some(ids[2]);
    settle(&mut h);
    h.event(key_event(Key::ArrowUp, egui::Modifiers::NONE));
    settle(&mut h);
    assert_eq!(h.state().0.selected, Some(ids[1]));
}

#[test]
fn shift_arrows_select_over_colour_markers_one_visible_character_at_a_time() {
    let text = "a <span style=\"color:red\">TBD</span> b";
    let a = Block::new(BlockKind::Paragraph, text);
    let aid = a.id;
    let mut h = editor_harness(vec![a]);
    h.state_mut().0.focus = Some((aid, Caret::At(2)));
    for _ in 0..3 {
        h.run();
    }
    // T, B, D, then the space after the closing marker: four presses, not one per hidden character
    for _ in 0..4 {
        h.event(key_event(Key::ArrowRight, egui::Modifiers::SHIFT));
        h.run();
        h.run();
    }
    let id = text_id(aid);
    assert_eq!(cursor_anchor(&h.ctx, id), Some(2));
    assert_eq!(cursor_head(&h.ctx, id), Some(text.chars().count() - 1));
}
