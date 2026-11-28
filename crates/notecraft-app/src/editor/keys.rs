//! Keyboard handling of the text and block operations (split, merge, indent, move). Block selection is in
//! `selection.rs`; the slash menu, block menu and link popup are in `menus.rs`.

use super::*;

// ------------------------------------------------------------- keyboard etc.

impl EditorState {
    pub(super) fn handle_keys(
        &mut self,
        ctx: &egui::Context,
        blocks: &mut Vec<Block>,
        i: usize,
        env: &Env,
        out: &mut Output,
    ) {
        let bid = blocks[i].id;
        let id = text_id(bid);
        let kind = blocks[i].kind.clone();
        let is_code = matches!(kind, BlockKind::Code { .. });
        let rich = kind.has_rich_text();
        let consume = |m: Modifiers, k: Key| ctx.input_mut(|inp| inp.consume_key(m, k));
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;

        // slash menu navigation
        if let Some(slash) = self.slash.clone() {
            if slash.block == bid {
                let items = filter_items(&slash.query, &slash.mode);
                if consume(Modifiers::NONE, Key::Escape) {
                    // close the menu, keep the typed text and the caret
                    if slash.mode == SlashMode::Insert {
                        self.dismissed_slash = Some((bid, slash.start));
                    }
                    self.slash = None;
                    let caret = cursor_range(ctx, id).map(|c| c.1).unwrap_or(char_len(&blocks[i].text));
                    self.focus = Some((bid, Caret::At(caret)));
                    return;
                }
                let n = items.len().max(1);
                if consume(Modifiers::NONE, Key::ArrowDown) {
                    if let Some(s) = &mut self.slash {
                        s.selected = (s.selected.min(n - 1) + 1) % n;
                    }
                    return;
                }
                if consume(Modifiers::NONE, Key::ArrowUp) {
                    if let Some(s) = &mut self.slash {
                        s.selected = (s.selected.min(n - 1) + n - 1) % n;
                    }
                    return;
                }
                if consume(Modifiers::NONE, Key::Enter) || consume(Modifiers::NONE, Key::Tab) {
                    if let Some(item) = items.get(slash.selected.min(items.len().saturating_sub(1))) {
                        self.apply_slash(blocks, i, item.key, &slash, env, out);
                    } else {
                        self.slash = None;
                    }
                    return;
                }
            }
        }

        let caret = cursor_range(ctx, id).unwrap_or((0, 0));
        let len = char_len(&blocks[i].text);

        // copying or cutting a part of the text keeps its formatting (bold, colours, links) as HTML
        if rich && caret.0 != caret.1 {
            let (copy, cut) = ctx.input(|inp| {
                (
                    inp.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                    inp.events.iter().any(|e| matches!(e, egui::Event::Cut)),
                )
            });
            if copy || cut {
                ctx.input_mut(|inp| inp.events.retain(|e| !matches!(e, egui::Event::Copy | egui::Event::Cut)));
                let (a, b) = (caret.0.min(caret.1), caret.0.max(caret.1));
                let frag: String = blocks[i].text.chars().skip(a).take(b - a).collect();
                super::richpaste::copy_blocks(ctx, &[Block::paragraph(&frag)]);
                if cut {
                    let before = blocks.clone();
                    let kept: String = blocks[i].text.chars().take(a).chain(blocks[i].text.chars().skip(b)).collect();
                    blocks[i].text = kept;
                    self.focus = Some((bid, Caret::At(a)));
                    self.record(env.key, &before, None);
                    out.changed = true;
                    return;
                }
            }
        }

        // pasting several lines (or Markdown blocks) makes blocks instead of one long block; HTML on the
        // clipboard (Outlook, Word, a web page) keeps its structure and formatting
        if rich {
            let pasted = ctx.input(|inp| {
                inp.events.iter().find_map(|e| match e {
                    egui::Event::Paste(t) => Some(t.clone()),
                    _ => None,
                })
            });
            if let Some(plain) = pasted {
                if let Some(md) = super::richpaste::clipboard_markdown() {
                    let new_blocks = blocks::from_markdown(&md);
                    let simple = new_blocks.len() == 1 && matches!(new_blocks[0].kind, BlockKind::Paragraph);
                    if new_blocks.len() > 1 || (!new_blocks.is_empty() && !simple) {
                        ctx.input_mut(|inp| inp.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                        self.paste_blocks(blocks, i, caret, new_blocks, env, out);
                        return;
                    } else if simple {
                        // one paragraph: its bold, italics and links go in as inline formatting
                        let text = new_blocks[0].text.clone();
                        ctx.input_mut(|inp| {
                            for e in inp.events.iter_mut() {
                                if let egui::Event::Paste(t) = e {
                                    *t = text.clone();
                                }
                            }
                        });
                    }
                } else if plain.trim().contains('\n') {
                    let new_blocks = blocks::from_markdown(&plain);
                    if new_blocks.len() > 1 {
                        ctx.input_mut(|inp| inp.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                        self.paste_blocks(blocks, i, caret, new_blocks, env, out);
                        return;
                    }
                }
            }
        }

        if rich {
            let fmt: &[(Modifiers, Key, inline::StyleKind)] = &[
                (cmd, Key::B, inline::StyleKind::Bold),
                (cmd, Key::I, inline::StyleKind::Italic),
                (cmd, Key::U, inline::StyleKind::Underline),
                (cmd_shift, Key::S, inline::StyleKind::Strike),
                (cmd, Key::E, inline::StyleKind::Code),
            ];
            for (m, k, style) in fmt {
                if consume(*m, *k) {
                    let before = blocks.clone();
                    let (t, sel) = inline::toggle_style(&blocks[i].text, caret, *style);
                    blocks[i].text = t;
                    self.focus = Some((bid, Caret::Select(sel.0, sel.1)));
                    self.record(env.key, &before, None);
                    out.changed = true;
                    return;
                }
            }
            // Cmd+K with a selection asks for a link address
            if caret.0 != caret.1 && consume(cmd, Key::K) {
                self.link_edit = Some((bid, caret.0, caret.1, "https://".into()));
                self.link_replace = None;
                return;
            }
            // Cmd+K inside an existing link edits its address
            if caret.0 == caret.1 && consume(cmd, Key::K) {
                self.begin_link_edit(bid, &blocks[i].text, caret.0);
                return;
            }
        }
        // Cmd+Shift+D writes today's date at the caret, in the format last chosen in the slash menu
        if rich && consume(cmd_shift, Key::D) {
            let text = date_text(self.date_key, env.lang == Lang::Fr);
            let before = blocks.clone();
            let (a, b) = (caret.0.min(caret.1), caret.0.max(caret.1));
            let kept = remove_chars(&blocks[i].text, a, b);
            blocks[i].text = insert_at_char(&kept, a, &text);
            self.focus = Some((bid, Caret::At(a + char_len(&text))));
            self.record(env.key, &before, None);
            out.changed = true;
            return;
        }
        if consume(cmd, Key::D) {
            self.duplicate(blocks, i, env, out);
            return;
        }
        if consume(cmd_shift, Key::ArrowUp) {
            self.move_up_down(blocks, i, true, env, out);
            return;
        }
        if consume(cmd_shift, Key::ArrowDown) {
            self.move_up_down(blocks, i, false, env, out);
            return;
        }
        if consume(cmd, Key::Enter) {
            let before = blocks.clone();
            match &mut blocks[i].kind {
                BlockKind::Todo { checked } => *checked = !*checked,
                BlockKind::Toggle { open } => *open = !*open,
                _ => return,
            }
            self.record(env.key, &before, None);
            out.changed = true;
            return;
        }
        if consume(cmd, Key::Slash) && (rich || is_code) {
            self.slash = Some(Slash {
                block: bid,
                start: caret.0,
                query: String::new(),
                selected: 0,
                mode: SlashMode::TurnInto,
            });
            return;
        }
        // Cmd+A: first the text of the block (native), then, when it is all selected already, every block
        if ctx.input(|inp| inp.modifiers.command && !inp.modifiers.shift && inp.key_pressed(Key::A)) {
            let len = char_len(&blocks[i].text);
            let all_text = len == 0 || cursor_range(ctx, id).is_some_and(|(a, b)| a == 0 && b >= len);
            if all_text && consume(cmd, Key::A) {
                self.select_all_blocks(blocks);
                self.slash = None;
                ctx.memory_mut(|m| m.surrender_focus(id));
                self.focused_block = None;
                return;
            }
        }
        if consume(Modifiers::NONE, Key::Escape) {
            self.sel_anchor = None;
            self.selected = Some(bid);
            self.slash = None;
            ctx.memory_mut(|m| m.surrender_focus(id));
            self.focused_block = None;
            return;
        }
        if is_code {
            // code keeps Enter and Tab; leave it with the arrow keys at its edges
            if self.at_last_row && caret.0 == len && consume(Modifiers::NONE, Key::ArrowDown) {
                self.focus_neighbor(blocks, i, false);
            } else if self.at_first_row && caret.0 == 0 && consume(Modifiers::NONE, Key::ArrowUp) {
                self.focus_neighbor(blocks, i, true);
            } else if blocks[i].text.is_empty() && consume(Modifiers::NONE, Key::Backspace) {
                let before = blocks.clone();
                blocks[i].kind = BlockKind::Paragraph;
                self.focus = Some((bid, Caret::Start));
                self.record(env.key, &before, None);
                out.changed = true;
            }
            return;
        }
        if !rich {
            return;
        }
        if consume(Modifiers::NONE, Key::Enter) {
            self.enter(blocks, i, caret, env, out);
            return;
        }
        // Formatting markers are hidden, so the caret and deletion work on
        // visible characters only.
        let mask = inline::marker_mask(&blocks[i].text);
        let plain = caret.0 == caret.1;
        // `consume_key` ignores extra Alt, so ⌥← ⌥→ (word moves) would be eaten as a single step
        let no_alt = !ctx.input(|inp| inp.modifiers.alt);
        let visible_before = mask[..caret.0.min(mask.len())].iter().any(|m| !m);
        let visible_after = mask[caret.1.min(mask.len())..].iter().any(|m| !m);
        if plain && !visible_before && consume(Modifiers::NONE, Key::Backspace) {
            self.backspace_at_start(blocks, i, env, out);
            return;
        }
        if plain && consume(Modifiers::NONE, Key::Backspace) {
            if let Some((t, c)) = inline::delete_visible(&blocks[i].text, caret.0, true) {
                let mut snap = blocks.clone();
                snap[i].text = blocks[i].text.clone();
                blocks[i].text = t;
                self.focus = Some((bid, Caret::At(c)));
                self.record(env.key, &snap, Some(&format!("t{bid}")));
                out.changed = true;
            }
            return;
        }
        if plain && consume(Modifiers::NONE, Key::Delete) {
            if let Some((t, c)) = inline::delete_visible(&blocks[i].text, caret.0, false) {
                let snap = blocks.clone();
                blocks[i].text = t;
                self.focus = Some((bid, Caret::At(c)));
                self.record(env.key, &snap, Some(&format!("t{bid}")));
                out.changed = true;
            } else if !visible_after {
                self.delete_at_end(blocks, i, env, out);
            }
            return;
        }
        let shift = ctx.input(|inp| {
            inp.modifiers.shift
                || inp.events.iter().any(|e| {
                    matches!(e, egui::Event::Key { key: Key::ArrowLeft | Key::ArrowRight, pressed: true, modifiers, .. } if modifiers.shift)
                })
        });
        // ⌥← ⌥→: a word at a time over the visible text; hidden link targets and markers are skipped
        if plain && !no_alt && !shift && consume(Modifiers::ALT, Key::ArrowRight) {
            if visible_after {
                self.focus = Some((bid, Caret::At(inline::step_word(&blocks[i].text, caret.0, true))));
            } else {
                self.focus_adjacent(blocks, i, false);
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::End));
                }
            }
            return;
        }
        if plain && !no_alt && !shift && consume(Modifiers::ALT, Key::ArrowLeft) {
            if visible_before {
                self.focus = Some((bid, Caret::At(inline::step_word(&blocks[i].text, caret.0, false))));
            } else {
                self.focus_adjacent(blocks, i, true);
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::Start));
                }
            }
            return;
        }
        // (`consume_key` also matches with Shift held, which would move the caret instead of extending the selection)
        if plain && no_alt && !shift && consume(Modifiers::NONE, Key::ArrowRight) {
            if visible_after {
                self.focus = Some((bid, Caret::At(inline::step_caret(&blocks[i].text, caret.0, true))));
            } else {
                self.focus_adjacent(blocks, i, false);
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::End));
                }
            }
            return;
        }
        if plain && no_alt && !shift && consume(Modifiers::NONE, Key::ArrowLeft) {
            if visible_before {
                self.focus = Some((bid, Caret::At(inline::step_caret(&blocks[i].text, caret.0, false))));
            } else {
                self.focus_adjacent(blocks, i, true);
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::Start));
                }
            }
            return;
        }
        // Shift+← Shift+→ (and with ⌥, a word): the selection grows over the visible text only. Left to
        // egui it would walk through every hidden character of a colour, bold or link marker.
        let (alt, command) = ctx.input(|inp| (inp.modifiers.alt, inp.modifiers.command || inp.modifiers.ctrl));
        if shift && !command {
            for (right, key) in [(true, Key::ArrowRight), (false, Key::ArrowLeft)] {
                let mods = if alt { Modifiers::ALT | Modifiers::SHIFT } else { Modifiers::SHIFT };
                if consume(mods, key) {
                    let head = cursor_head(ctx, id).unwrap_or(caret.1);
                    let anchor = cursor_anchor(ctx, id).unwrap_or(head);
                    let text = &blocks[i].text;
                    let to =
                        if alt { inline::step_word(text, head, right) } else { inline::step_caret(text, head, right) };
                    self.focus = Some((bid, Caret::Select(anchor, to)));
                    return;
                }
            }
        }
        if consume(Modifiers::NONE, Key::Tab) {
            self.indent(blocks, i, true, env, out);
            return;
        }
        if consume(Modifiers::SHIFT, Key::Tab) {
            self.indent(blocks, i, false, env, out);
            return;
        }
        // Shift+Up / Shift+Down first extend the text selection to the start / end of the block; only
        // when it already reaches that edge does the next press switch to block selection
        let head = super::cursor_head(ctx, id).unwrap_or(caret.1);
        let anchor = super::cursor_anchor(ctx, id).unwrap_or(caret.0);
        if self.at_first_row && head != 0 && consume(Modifiers::SHIFT, Key::ArrowUp) {
            self.focus = Some((bid, Caret::Select(anchor, 0)));
            return;
        }
        if self.at_last_row && head < len && consume(Modifiers::SHIFT, Key::ArrowDown) {
            self.focus = Some((bid, Caret::Select(anchor, len)));
            return;
        }
        let edge_select = (self.at_first_row && head == 0 && consume(Modifiers::SHIFT, Key::ArrowUp))
            || (self.at_last_row && head >= len && consume(Modifiers::SHIFT, Key::ArrowDown));
        if edge_select {
            // Shift+Up on the first row (Shift+Down on the last) switches to block selection
            self.sel_anchor = Some(bid);
            self.selected = Some(bid);
            self.slash = None;
            self.last_selection = None;
            ctx.memory_mut(|m| m.surrender_focus(id));
            self.focused_block = None;
            return;
        }
        if no_alt && self.at_first_row && consume(Modifiers::NONE, Key::ArrowUp) {
            self.focus_neighbor(blocks, i, true);
            return;
        }
        if no_alt && self.at_last_row && consume(Modifiers::NONE, Key::ArrowDown) {
            self.focus_neighbor(blocks, i, false);
        }
    }

    /// Paste several blocks at the caret: the text before the caret stays in the current block,
    /// the pasted blocks follow it, and the text after the caret ends up in a block of its own.
    pub(super) fn paste_blocks(
        &mut self,
        blocks: &mut Vec<Block>,
        i: usize,
        caret: (usize, usize),
        pasted: Vec<Block>,
        env: &Env,
        out: &mut Output,
    ) {
        let before = blocks.clone();
        let (lo, hi) = (caret.0.min(caret.1), caret.0.max(caret.1));
        let (left, rest) = split_at_char(&blocks[i].text, lo);
        let (_, right) = split_at_char(&rest, hi - lo);
        let depth = blocks[i].depth;
        let kind = blocks[i].kind.clone();
        let mut at = i + 1;
        if left.is_empty() {
            // nothing before the caret: the pasted blocks take the place of the current block
            blocks.remove(i);
            at = i;
        } else {
            blocks[i].text = left;
        }
        let n = pasted.len();
        for (k, mut b) in pasted.into_iter().enumerate() {
            b.depth = b.depth.saturating_add(depth).min(MAX_DEPTH);
            blocks.insert(at + k, b);
        }
        let last = at + n - 1;
        if !right.is_empty() {
            blocks.insert(last + 1, Block::new(kind, right).with_depth(depth));
        }
        self.focus = Some((blocks[last].id, Caret::End));
        self.record(env.key, &before, None);
        out.changed = true;
    }

    /// Open the address editor for the link under the caret. False when the caret is not in a link.
    pub(super) fn begin_link_edit(&mut self, bid: u64, text: &str, caret: usize) -> bool {
        let Some((label, d0, d1)) = link_target_at(text, caret) else { return false };
        let url = text[d0..d1].replace("%20", " ");
        self.link_edit = Some((bid, caret, caret, url));
        self.link_replace = Some((label, d0, d1));
        true
    }

    /// ← or → past the edge of a block: the caret goes to the end of the block before or the start of
    /// the block after (not to the same column, which is what ↑ and ↓ keep).
    pub(super) fn focus_adjacent(&mut self, blocks: &[Block], i: usize, up: bool) {
        self.focus_neighbor(blocks, i, up);
        self.goal = None;
    }

    pub(super) fn focus_neighbor(&mut self, blocks: &[Block], i: usize, up: bool) {
        let hidden = blocks::hidden_by_toggles(blocks);
        let editable = |b: &Block| b.kind.has_rich_text() || matches!(b.kind, BlockKind::Code { .. });
        let j = if up {
            (0..i).rev().find(|j| !hidden[*j] && editable(&blocks[*j]))
        } else {
            (i + 1..blocks.len()).find(|j| !hidden[*j] && editable(&blocks[*j]))
        };
        if let Some(j) = j {
            self.focus = Some((blocks[j].id, if up { Caret::End } else { Caret::Start }));
            // keep the horizontal position: land on the last row going up, the first going down
            self.goal = Some((blocks[j].id, self.caret_x, up, self.frame));
        }
    }

    pub(super) fn enter(
        &mut self,
        blocks: &mut Vec<Block>,
        i: usize,
        caret: (usize, usize),
        env: &Env,
        out: &mut Output,
    ) {
        let before_snapshot = blocks.clone();
        let b = blocks[i].clone();
        let text = remove_chars(&b.text, caret.0, caret.1);
        let (left, right) = split_at_char(&text, caret.0);
        // Enter on an empty list item leaves the list (or moves it out one level)
        if left.is_empty() && right.is_empty() && b.kind.continues_on_enter() {
            if b.depth > 0 {
                let end = blocks::subtree_end(blocks, i);
                for bl in &mut blocks[i..end] {
                    bl.depth -= 1;
                }
            } else {
                blocks[i].kind = BlockKind::Paragraph;
            }
            self.focus = Some((b.id, Caret::Start));
            self.record(env.key, &before_snapshot, None);
            out.changed = true;
            return;
        }
        let next_kind = match &b.kind {
            BlockKind::Bulleted => BlockKind::Bulleted,
            BlockKind::Numbered => BlockKind::Numbered,
            BlockKind::Todo { .. } => BlockKind::Todo { checked: false },
            BlockKind::Toggle { open: false } => BlockKind::Toggle { open: false },
            _ => BlockKind::Paragraph,
        };
        // caret at the start of a non-empty block: open a line above
        if left.is_empty() && !right.is_empty() {
            let kind = if b.kind.continues_on_enter() { next_kind } else { BlockKind::Paragraph };
            blocks.insert(i, Block::new(kind, "").with_depth(b.depth));
            self.focus = Some((b.id, Caret::Start));
            self.record(env.key, &before_snapshot, None);
            out.changed = true;
            return;
        }
        blocks[i].text = left;
        let open_toggle = matches!(b.kind, BlockKind::Toggle { open: true });
        let depth = if open_toggle { (b.depth + 1).min(MAX_DEPTH) } else { b.depth };
        let nb = Block::new(next_kind, right).with_depth(depth);
        let nid = nb.id;
        let at = if open_toggle || b.kind.continues_on_enter() { i + 1 } else { blocks::subtree_end(blocks, i) };
        blocks.insert(at, nb);
        self.focus = Some((nid, Caret::Start));
        self.history(env.key).break_group();
        self.record(env.key, &before_snapshot, None);
        out.changed = true;
    }

    pub(super) fn backspace_at_start(&mut self, blocks: &mut Vec<Block>, i: usize, env: &Env, out: &mut Output) {
        let before = blocks.clone();
        let b = blocks[i].clone();
        if b.kind != BlockKind::Paragraph {
            blocks[i].kind = BlockKind::Paragraph;
            self.focus = Some((b.id, Caret::Start));
        } else if b.depth > 0 {
            let end = blocks::subtree_end(blocks, i);
            for bl in &mut blocks[i..end] {
                bl.depth -= 1;
            }
            self.focus = Some((b.id, Caret::Start));
        } else {
            let hidden = blocks::hidden_by_toggles(blocks);
            let Some(p) = (0..i).rev().find(|j| !hidden[*j]) else { return };
            let prev = blocks[p].clone();
            if prev.kind.has_rich_text() {
                let join = char_len(&prev.text);
                blocks[p].text.push_str(&b.text);
                blocks.remove(i);
                self.focus = Some((prev.id, Caret::At(join)));
            } else if b.text.is_empty() {
                blocks.remove(i);
                if matches!(prev.kind, BlockKind::Code { .. }) {
                    self.focus = Some((prev.id, Caret::End));
                } else {
                    self.sel_anchor = None;
                    self.selected = Some(prev.id);
                }
            } else {
                blocks.remove(p);
                self.focus = Some((b.id, Caret::Start));
            }
        }
        self.record(env.key, &before, None);
        out.changed = true;
    }

    /// Delete at the end of a line: the next line moves up and joins this one.
    pub(super) fn delete_at_end(&mut self, blocks: &mut Vec<Block>, i: usize, env: &Env, out: &mut Output) {
        let hidden = blocks::hidden_by_toggles(blocks);
        let Some(n) = (i + 1..blocks.len()).find(|j| !hidden[*j]) else { return };
        let before = blocks.clone();
        let id = blocks[i].id;
        if blocks[n].kind.has_rich_text() {
            let join = char_len(&blocks[i].text);
            let tail = blocks[n].text.clone();
            blocks[i].text.push_str(&tail);
            blocks.remove(n);
            self.focus = Some((id, Caret::At(join)));
        } else if blocks[i].text.is_empty() {
            // an empty line before a picture, a divider...: the empty line goes
            blocks.remove(i);
            self.sel_anchor = None;
            self.selected = Some(blocks[i.min(blocks.len() - 1)].id);
        } else {
            return;
        }
        self.record(env.key, &before, None);
        out.changed = true;
    }

    pub(super) fn indent(&mut self, blocks: &mut [Block], i: usize, deeper: bool, env: &Env, out: &mut Output) {
        let before = blocks.to_vec();
        let end = blocks::subtree_end(blocks, i);
        if deeper {
            if i == 0 || blocks[i].depth > blocks[i - 1].depth || blocks[i].depth >= MAX_DEPTH {
                return;
            }
            for b in &mut blocks[i..end] {
                b.depth = (b.depth + 1).min(MAX_DEPTH);
            }
        } else {
            if blocks[i].depth == 0 {
                return;
            }
            for b in &mut blocks[i..end] {
                b.depth = b.depth.saturating_sub(1);
            }
        }
        self.record(env.key, &before, None);
        out.changed = true;
    }

    pub(super) fn duplicate(&mut self, blocks: &mut Vec<Block>, i: usize, env: &Env, out: &mut Output) {
        let before = blocks.clone();
        let multi = self.multi_range(blocks, i);
        let (lo, end) = multi.unwrap_or((i, blocks::subtree_end(blocks, i)));
        let copies: Vec<Block> =
            blocks[lo..end].iter().map(|b| Block { id: blocks::next_block_id(), ..b.clone() }).collect();
        let first = copies[0].id;
        let last = copies[copies.len() - 1].id;
        for (k, c) in copies.into_iter().enumerate() {
            blocks.insert(end + k, c);
        }
        if multi.is_some() {
            // the copies are the new selection
            self.selected = Some(first);
            self.sel_anchor = Some(last);
        } else if self.selected.is_some() {
            self.sel_anchor = None;
            self.selected = Some(first);
        } else {
            self.focus = Some((first, Caret::End));
        }
        self.record(env.key, &before, None);
        out.changed = true;
    }

    pub(super) fn move_up_down(&mut self, blocks: &mut Vec<Block>, i: usize, up: bool, env: &Env, out: &mut Output) {
        let before = blocks.clone();
        let bid = blocks[i].id;
        let multi = self.multi_range(blocks, i);
        let (lo, hi) = multi.unwrap_or((i, blocks::subtree_end(blocks, i)));
        let target = if up {
            if lo == 0 {
                return;
            }
            let d = blocks[lo].depth;
            let mut j = lo - 1;
            while j > 0 && blocks[j].depth > d {
                j -= 1;
            }
            j
        } else {
            if hi >= blocks.len() {
                return;
            }
            blocks::subtree_end(blocks, hi)
        };
        if multi.is_some() {
            // the whole selection moves and stays selected
            blocks::move_range(blocks, lo, hi, target);
            self.record(env.key, &before, None);
            out.changed = true;
            return;
        }
        blocks::move_block(blocks, i, target);
        if self.selected.is_some() {
            self.sel_anchor = None;
            self.selected = Some(bid);
        } else {
            self.focus = Some((bid, Caret::End));
        }
        self.record(env.key, &before, None);
        out.changed = true;
    }
}
