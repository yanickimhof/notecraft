//! Keyboard handling, block operations (split, merge, indent, move), the slash menu,
//! the block menu and the link popup of the editor. Split out of mod.rs without any change
//! in behaviour.

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
            }
            return;
        }
        let shift = ctx.input(|inp| inp.modifiers.shift);
        // ⌥← ⌥→: a word at a time over the visible text; hidden link targets and markers are skipped
        if plain && !no_alt && !shift && consume(Modifiers::ALT, Key::ArrowRight) {
            if visible_after {
                self.focus = Some((bid, Caret::At(inline::step_word(&blocks[i].text, caret.0, true))));
            } else {
                self.focus_neighbor(blocks, i, false);
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
                self.focus_neighbor(blocks, i, true);
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::Start));
                }
            }
            return;
        }
        if plain && no_alt && consume(Modifiers::NONE, Key::ArrowRight) {
            if visible_after {
                self.focus = Some((bid, Caret::At(inline::step_caret(&blocks[i].text, caret.0, true))));
            } else {
                self.focus_neighbor(blocks, i, false);
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::End));
                }
            }
            return;
        }
        if plain && no_alt && consume(Modifiers::NONE, Key::ArrowLeft) {
            if visible_before {
                self.focus = Some((bid, Caret::At(inline::step_caret(&blocks[i].text, caret.0, false))));
            } else {
                self.focus_neighbor(blocks, i, true);
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::Start));
                }
            }
            return;
        }
        if consume(Modifiers::NONE, Key::Tab) {
            self.indent(blocks, i, true, env, out);
            return;
        }
        if consume(Modifiers::SHIFT, Key::Tab) {
            self.indent(blocks, i, false, env, out);
            return;
        }
        let edge_select = (self.at_first_row && consume(Modifiers::SHIFT, Key::ArrowUp))
            || (self.at_last_row && consume(Modifiers::SHIFT, Key::ArrowDown));
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

    /// Blocks covered by the block selection as `[start, end)` (a block brings its children).
    pub(super) fn selected_range(&self, blocks: &[Block]) -> Option<(usize, usize)> {
        let sel = self.selected?;
        let i = blocks.iter().position(|b| b.id == sel)?;
        let j = self.sel_anchor.and_then(|a| blocks.iter().position(|b| b.id == a)).unwrap_or(i);
        let (lo, hi) = (i.min(j), i.max(j));
        Some((lo, blocks::subtree_end(blocks, hi)))
    }

    /// The blocks of a multi-block selection when `i` is one of them.
    pub(super) fn multi_range(&self, blocks: &[Block], i: usize) -> Option<(usize, usize)> {
        self.sel_anchor?;
        self.selected_range(blocks).filter(|(a, b)| (*a..*b).contains(&i))
    }

    /// Turn the selected blocks into `key`, each text block on its own (several lines
    /// become one block for code). A `group_` key puts them together under a new toggle or callout.
    /// False when nothing changed.
    pub(super) fn turn_range(&mut self, blocks: &mut Vec<Block>, i: usize, key: &str) -> bool {
        let multi = self.multi_range(blocks, i);
        if let Some(container) = key.strip_prefix("group_") {
            let Some(kind) = kind_for(container) else { return false };
            let (lo, hi) = multi.unwrap_or((i, blocks::subtree_end(blocks, i)));
            if blocks[lo..hi].iter().any(|b| b.depth + 1 > MAX_DEPTH) {
                return false;
            }
            let depth = blocks[lo..hi].iter().map(|b| b.depth).min().unwrap_or(0);
            for b in &mut blocks[lo..hi] {
                b.depth += 1;
            }
            let head = Block::new(kind, "").with_depth(depth);
            self.focus = Some((head.id, Caret::Start));
            blocks.insert(lo, head);
            self.selected = None;
            self.sel_anchor = None;
            return true;
        }
        let Some(kind) = kind_for(key) else { return false };
        let (lo, hi) = multi.unwrap_or((i, i + 1));
        if matches!(kind, BlockKind::Code { .. })
            && hi - lo > 1
            && blocks[lo..hi].iter().all(|b| b.kind.has_rich_text())
        {
            let depth = blocks[lo].depth;
            let text = blocks[lo..hi].iter().map(|b| b.text.clone()).collect::<Vec<_>>().join("\n");
            blocks.splice(lo..hi, [Block::new(kind, text).with_depth(depth)]);
            self.selected = None;
            self.sel_anchor = None;
            return true;
        }
        let mut changed = false;
        for b in &mut blocks[lo..hi] {
            if b.kind.has_rich_text() || hi - lo == 1 {
                b.kind = kind.clone();
                changed = true;
            }
        }
        changed
    }

    pub(super) fn handle_selection_keys(
        &mut self,
        ctx: &egui::Context,
        blocks: &mut Vec<Block>,
        env: &Env,
        out: &mut Output,
    ) {
        let Some(sel) = self.selected else { return };
        let Some(i) = blocks.iter().position(|b| b.id == sel) else {
            self.selected = None;
            return;
        };
        // don't steal keys from other text fields (title, database cells)
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let consume = |m: Modifiers, k: Key| ctx.input_mut(|inp| inp.consume_key(m, k));
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let (range_start, range_end) = self.selected_range(blocks).unwrap_or((i, blocks::subtree_end(blocks, i)));
        // copy, cut and paste arrive as events (not key presses) on macOS
        let (copy, cut, paste) = ctx.input_mut(|inp| {
            let (mut copy, mut cut, mut paste) = (false, false, None);
            inp.events.retain(|e| match e {
                egui::Event::Copy => {
                    copy = true;
                    false
                }
                egui::Event::Cut => {
                    cut = true;
                    false
                }
                egui::Event::Paste(t) => {
                    paste = Some(t.clone());
                    false
                }
                _ => true,
            });
            (copy, cut, paste)
        });
        if copy || cut {
            ctx.copy_text(blocks::to_markdown(&blocks[range_start..range_end]));
        }
        if let Some(text) = paste {
            let text = super::richpaste::clipboard_markdown().unwrap_or(text);
            let pasted = blocks::from_markdown(&text);
            if !pasted.is_empty() {
                let before = blocks.clone();
                let n = pasted.len();
                for (k, b) in pasted.into_iter().enumerate() {
                    blocks.insert(range_end + k, b);
                }
                self.selected = Some(blocks[range_end].id);
                self.sel_anchor = Some(blocks[range_end + n - 1].id);
                self.record(env.key, &before, None);
                out.changed = true;
            }
        }
        if consume(Modifiers::NONE, Key::Escape) {
            self.slash = None;
            self.block_menu = None;
            self.selected = None;
            self.sel_anchor = None;
            self.block_menu = None;
        } else if cut || consume(Modifiers::NONE, Key::Backspace) || consume(Modifiers::NONE, Key::Delete) {
            let before = blocks.clone();
            blocks.drain(range_start..range_end);
            if blocks.is_empty() {
                blocks.push(Block::paragraph(""));
            }
            let next = range_start.min(blocks.len() - 1);
            self.selected = None;
            self.sel_anchor = None;
            self.block_menu = None;
            self.focus = Some((blocks[next].id, Caret::End));
            self.record(env.key, &before, None);
            out.changed = true;
        } else if consume(Modifiers::NONE, Key::Enter) {
            self.selected = None;
            self.sel_anchor = None;
            self.block_menu = None;
            self.focus = Some((sel, Caret::End));
        } else if consume(Modifiers::SHIFT, Key::ArrowUp) {
            let hidden = blocks::hidden_by_toggles(blocks);
            if let Some(j) = (0..i).rev().find(|j| !hidden[*j]) {
                self.sel_anchor.get_or_insert(sel);
                self.selected = Some(blocks[j].id);
            }
        } else if consume(Modifiers::SHIFT, Key::ArrowDown) {
            let hidden = blocks::hidden_by_toggles(blocks);
            if let Some(j) = (i + 1..blocks.len()).find(|j| !hidden[*j]) {
                self.sel_anchor.get_or_insert(sel);
                self.selected = Some(blocks[j].id);
            }
        } else if let Some(left) = if consume(Modifiers::NONE, Key::ArrowLeft) {
            Some(true)
        } else if consume(Modifiers::NONE, Key::ArrowRight) {
            Some(false)
        } else {
            None
        } {
            // ← or → leaves block selection and puts the caret back in the text:
            // ← at the start of the first selected block, → at the end of the last one
            let editable = |b: &Block| b.kind.has_rich_text() || matches!(b.kind, BlockKind::Code { .. });
            let target = if left {
                (range_start..range_end).find(|j| editable(&blocks[*j]))
            } else {
                (range_start..range_end).rev().find(|j| editable(&blocks[*j]))
            };
            if let Some(j) = target {
                self.selected = None;
                self.sel_anchor = None;
                self.block_menu = None;
                self.focus_later = Some((blocks[j].id, if left { Caret::Start } else { Caret::End }, self.frame + 3));
                ctx.request_repaint();
            }
        } else if consume(Modifiers::NONE, Key::ArrowUp) {
            self.sel_anchor = None;
            let hidden = blocks::hidden_by_toggles(blocks);
            if let Some(j) = (0..i).rev().find(|j| !hidden[*j]) {
                self.selected = Some(blocks[j].id);
            }
        } else if consume(Modifiers::NONE, Key::ArrowDown) {
            self.sel_anchor = None;
            let hidden = blocks::hidden_by_toggles(blocks);
            if let Some(j) = (i + 1..blocks.len()).find(|j| !hidden[*j]) {
                self.selected = Some(blocks[j].id);
            }
        } else if consume(cmd, Key::D) {
            self.duplicate(blocks, i, env, out);
        } else if consume(cmd_shift, Key::ArrowUp) {
            self.move_up_down(blocks, i, true, env, out);
        } else if consume(cmd_shift, Key::ArrowDown) {
            self.move_up_down(blocks, i, false, env, out);
        } else if consume(cmd, Key::Slash) {
            self.slash =
                Some(Slash { block: sel, start: 0, query: String::new(), selected: 0, mode: SlashMode::TurnInto });
        } else if self.slash.as_ref().is_some_and(|s| s.mode == SlashMode::TurnInto) {
            let slash = self.slash.clone().unwrap();
            let items = filter_items("", &SlashMode::TurnInto);
            let n = items.len();
            if consume(Modifiers::NONE, Key::ArrowDown) {
                if let Some(s) = &mut self.slash {
                    s.selected = (s.selected + 1) % n;
                }
            } else if consume(Modifiers::NONE, Key::ArrowUp) {
                if let Some(s) = &mut self.slash {
                    s.selected = (s.selected + n - 1) % n;
                }
            } else if consume(Modifiers::NONE, Key::Enter) {
                let key = items[slash.selected.min(n - 1)].key;
                self.apply_slash(blocks, i, key, &slash, env, out);
            }
        }
    }

    /// Open the address editor for the link under the caret. False when the caret is not in a link.
    pub(super) fn begin_link_edit(&mut self, bid: u64, text: &str, caret: usize) -> bool {
        let Some((label, d0, d1)) = link_target_at(text, caret) else { return false };
        let url = text[d0..d1].replace("%20", " ");
        self.link_edit = Some((bid, caret, caret, url));
        self.link_replace = Some((label, d0, d1));
        true
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

    // -------------------------------------------------------------- slash menu

    pub(super) fn apply_slash(
        &mut self,
        blocks: &mut Vec<Block>,
        i: usize,
        key: &str,
        slash: &Slash,
        env: &Env,
        out: &mut Output,
    ) {
        let before = blocks.clone();
        self.slash = None;
        let bid = blocks[i].id;
        if slash.mode == SlashMode::TurnInto {
            if self.turn_range(blocks, i, key) {
                self.record(env.key, &before, None);
                out.changed = true;
                if self.selected.is_none() && self.focus.is_none() {
                    self.focus = Some((bid, Caret::End));
                }
            }
            self.block_menu = None;
            return;
        }
        // remove the "/query" text
        let len = char_len(&blocks[i].text);
        let caret_end = (slash.start + 1 + char_len(&slash.query)).min(len);
        blocks[i].text = remove_chars(&blocks[i].text, slash.start.min(len), caret_end);
        let empty = blocks[i].text.trim().is_empty();
        if empty {
            blocks[i].text.clear();
        }
        let depth = blocks[i].depth;
        let insert_at = blocks::subtree_end(blocks, i);
        let target = if empty { i } else { insert_at };
        match key {
            "divider" => {
                let at = if empty {
                    blocks[i].kind = BlockKind::Divider;
                    i + 1
                } else {
                    blocks.insert(insert_at, Block::new(BlockKind::Divider, "").with_depth(depth));
                    insert_at + 1
                };
                let p = Block::paragraph("").with_depth(depth);
                self.focus = Some((p.id, Caret::Start));
                blocks.insert(at, p);
            }
            "columns2" | "columns3" => {
                let n = if key == "columns3" { 3 } else { 2 };
                let mut group = vec![Block::new(BlockKind::ColumnsStart, "").with_depth(depth)];
                let mut first_text = None;
                for c in 0..n {
                    if c > 0 {
                        group.push(Block::new(BlockKind::ColumnBreak, "").with_depth(depth));
                    }
                    let p = Block::paragraph("").with_depth(depth);
                    if c == 0 {
                        first_text = Some(p.id);
                    }
                    group.push(p);
                }
                group.push(Block::new(BlockKind::ColumnsEnd, "").with_depth(depth));
                let tail = Block::paragraph("").with_depth(depth);
                group.push(tail);
                self.focus = first_text.map(|id| (id, Caret::Start));
                let at = if empty {
                    blocks.remove(i);
                    i
                } else {
                    insert_at
                };
                for (k, b) in group.into_iter().enumerate() {
                    blocks.insert(at + k, b);
                }
            }
            "page" => out.actions.push(Action::CreateSubpage { index: target, replace: empty }),
            "link" => {
                out.actions.push(Action::LinkToPage { block: bid, at: slash.start });
                self.focus = Some((bid, Caret::At(slash.start)));
            }
            "image" => out.actions.push(Action::PickFile { index: target, replace: empty, image: true }),
            "file" => out.actions.push(Action::PickFile { index: target, replace: empty, image: false }),
            "table" | "board" | "list" => {
                let kind = match key {
                    "board" => ViewKind::Board,
                    "list" => ViewKind::List,
                    _ => ViewKind::Table,
                };
                out.actions.push(Action::CreateDatabase { index: target, replace: empty, kind });
            }
            k => {
                if let Some(kind) = kind_for(k) {
                    let sample = if k == "mermaid" { MERMAID_SAMPLE } else { "" };
                    if empty {
                        blocks[i].kind = kind;
                        blocks[i].text = sample.to_string();
                        self.focus = Some((bid, Caret::Start));
                    } else {
                        let nb = Block::new(kind, sample).with_depth(depth);
                        self.focus = Some((nb.id, Caret::Start));
                        blocks.insert(insert_at, nb);
                    }
                }
            }
        }
        self.record(env.key, &before, None);
        out.changed = true;
    }

    pub(super) fn slash_menu_ui(
        &mut self,
        ctx: &egui::Context,
        blocks: &[Block],
        rects: &[(usize, Rect)],
        env: &Env,
        deferred: &mut Vec<Deferred>,
    ) {
        let Some(slash) = self.slash.clone() else { return };
        let Some(i) = blocks.iter().position(|b| b.id == slash.block) else {
            self.slash = None;
            return;
        };
        let Some((_, rect)) = rects.iter().find(|(j, _)| *j == i) else { return };
        let items = filter_items(&slash.query, &slash.mode);
        let pal = env.pal;
        let screen = ctx.content_rect();
        let mut pos = egui::pos2(rect.min.x + 46.0 + blocks[i].depth as f32 * 26.0, rect.max.y + 4.0);
        // The list can hold 25 entries: let the menu use most of the window (up to 580 px) so that
        // many entries are visible at once, instead of a fixed 320 px strip.
        let max_h = (screen.height() * 0.72).clamp(320.0, 580.0);
        let sections = if slash.query.is_empty() { 4.0 } else { 0.0 };
        let want = max_h.min(items.len().max(1) as f32 * 46.0 + sections * 22.0 + 44.0);
        // Show the menu on the side with the most room. When neither side can hold a usable list,
        // keep a minimum height and let it cover the block rather than shrinking to one entry.
        let min_h = want.min(340.0);
        let below = screen.max.y - 10.0 - pos.y;
        let above = rect.min.y - 4.0 - 10.0;
        // when the menu opens above the block, its bottom edge is pinned to the block
        let mut above_anchor: Option<f32> = None;
        let h = if below >= want {
            want
        } else if above >= want {
            above_anchor = Some(rect.min.y - 4.0);
            want
        } else if below.max(above) >= min_h {
            if below >= above {
                below
            } else {
                above_anchor = Some(rect.min.y - 4.0);
                above
            }
        } else {
            pos.y = (screen.max.y - 10.0 - min_h).max(10.0);
            min_h
        };
        // egui sizes a floating area at most 400 px tall the first time it shows (its default area
        // size), whatever the content asks for: give it the height we want from the start
        let mut area_builder = egui::Area::new(Id::unique("nc-slash-menu"))
            .order(egui::Order::Foreground)
            .default_size(Vec2::new(330.0, h));
        area_builder = match above_anchor {
            Some(y) => area_builder.pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(egui::pos2(pos.x, y)),
            None => area_builder.fixed_pos(pos),
        };
        let area = area_builder.show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                ui.set_width(310.0);
                let title = if slash.mode == SlashMode::TurnInto {
                    tr(env.lang, "turn_into")
                } else {
                    tr(env.lang, "basic_blocks")
                };
                ui.label(RichText::new(title).size(12.0).color(pal.text_secondary));
                if items.is_empty() {
                    ui.label(RichText::new(tr(env.lang, "no_results")).color(pal.text_faint));
                    return;
                }
                egui::ScrollArea::vertical().max_height((h - 44.0).clamp(120.0, 560.0)).show(ui, |ui| {
                    let mut last_section = "basic_blocks";
                    for (k, it) in items.iter().enumerate() {
                        let section = match it.key {
                            "image" | "file" | "bookmark" => "media",
                            "table" | "board" | "list" | "dbview" => "database_views",
                            "group_toggle" | "group_callout" => "block_group",
                            "meeting" => "ai_blocks",
                            _ => "basic_blocks",
                        };
                        if slash.query.is_empty() && section != last_section {
                            ui.add_space(4.0);
                            ui.label(RichText::new(tr(env.lang, section)).size(12.0).color(pal.text_secondary));
                        }
                        last_section = section;
                        let selected = k == slash.selected.min(items.len() - 1);
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
                        if selected {
                            ui.painter().rect_filled(r, CornerRadius::same(6), pal.hover);
                            if ui.input(|i| i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)) {
                                resp.scroll_to_me(None);
                            }
                        }
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(r.shrink2(Vec2::new(6.0, 3.0)))
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        // labels would take the click away from the row
                        child.style_mut().interaction.selectable_labels = false;
                        widgets::glyph_box(&mut child, it.glyph, pal, 36.0);
                        child.vertical(|ui| {
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(tr(env.lang, &format!("blk_{}", it.key))).size(14.0).color(pal.text),
                            );
                            ui.label(
                                RichText::new(tr(env.lang, &format!("desc_{}", it.key)))
                                    .size(11.5)
                                    .color(pal.text_secondary),
                            );
                        });
                        if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                            if let Some(s) = &mut self.slash {
                                s.selected = k;
                            }
                        }
                        if resp.clicked() {
                            deferred.push(Deferred::Slash(i, it.key));
                        }
                    }
                });
            });
        });
        if area.response.clicked_elsewhere() && slash.mode == SlashMode::TurnInto {
            self.slash = None;
        }
    }

    pub(super) fn block_menu_ui(
        &mut self,
        ctx: &egui::Context,
        blocks: &[Block],
        i: usize,
        pos: egui::Pos2,
        env: &Env,
        deferred: &mut Vec<Deferred>,
    ) {
        let pal = env.pal;
        let lang = env.lang;
        let bid = blocks[i].id;
        let opened_now = ctx.cumulative_frame_nr() <= self.block_menu_frame;
        let mut acted = false;
        let mut sub = self.menu_sub;
        let mut query = std::mem::take(&mut self.menu_query);
        let q = query.trim().to_lowercase();
        let screen = ctx.content_rect();
        let pos = egui::pos2(pos.x.min(screen.max.x - 260.0), pos.y.min(screen.max.y - 420.0).max(8.0));
        let area = egui::Area::new(Id::unique("nc-block-menu"))
            .order(egui::Order::Foreground)
            .default_size(Vec2::new(300.0, 640.0))
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(236.0);
                    let search = ui.add(
                        egui::TextEdit::singleline(&mut query)
                            .id(Id::unique("nc-block-menu-search"))
                            .hint_text(tr(lang, "menu_search"))
                            .desired_width(f32::INFINITY),
                    );
                    if opened_now {
                        search.request_focus();
                    }
                    ui.add_space(4.0);
                    // (label, hint, has sub-menu, id)
                    let rows: [(&str, &str, u8, &str); 9] = [
                        (tr(lang, "turn_into"), "", 1, "turn"),
                        (tr(lang, "block_color"), "", 2, "color"),
                        (tr(lang, "block_select"), "Esc", 0, "select"),
                        (tr(lang, "block_copy"), "⌘C", 0, "copy"),
                        (tr(lang, "block_cut"), "⌘X", 0, "cut"),
                        (tr(lang, "duplicate"), "⌘D", 0, "dup"),
                        (tr(lang, "move_up"), "⌘⇧↑", 0, "up"),
                        (tr(lang, "move_down"), "⌘⇧↓", 0, "down"),
                        (tr(lang, "delete"), "⌫", 0, "del"),
                    ];
                    for (label, hint, has_sub, id) in rows {
                        if !q.is_empty() && !label.to_lowercase().contains(&q) {
                            continue;
                        }
                        let right = if has_sub > 0 { "›" } else { hint };
                        let mut btn = egui::Button::new(RichText::new(label).color(pal.text))
                            .right_text(RichText::new(right).color(pal.text_faint).size(12.0))
                            .min_size(Vec2::new(224.0, 28.0));
                        if sub == has_sub && has_sub > 0 {
                            btn = btn.fill(pal.hover);
                        }
                        let r = ui.add(btn);
                        if r.hovered() {
                            sub = has_sub;
                        }
                        if !r.clicked() {
                            continue;
                        }
                        let end = blocks::subtree_end(blocks, i);
                        match id {
                            "turn" | "color" => sub = has_sub,
                            "select" => {
                                self.selected = Some(bid);
                                self.sel_anchor = None;
                                acted = true;
                            }
                            "copy" => {
                                let (a, b) = self.multi_range(blocks, i).unwrap_or((i, end));
                                ctx.copy_text(blocks::to_markdown(&blocks[a..b]));
                                acted = true;
                            }
                            "cut" => {
                                let (a, b) = self.multi_range(blocks, i).unwrap_or((i, end));
                                ctx.copy_text(blocks::to_markdown(&blocks[a..b]));
                                deferred.push(Deferred::Delete(i));
                                acted = true;
                            }
                            "dup" => {
                                deferred.push(Deferred::Duplicate(i));
                                acted = true;
                            }
                            "up" => {
                                deferred.push(Deferred::Move(i, true));
                                acted = true;
                            }
                            "down" => {
                                deferred.push(Deferred::Move(i, false));
                                acted = true;
                            }
                            _ => {
                                deferred.push(Deferred::Delete(i));
                                acted = true;
                            }
                        }
                    }
                });
            });
        let main_rect = area.response.rect;
        // sub-menu to the right of the menu
        let sub_hit = if sub > 0 && q.is_empty() {
            let sub_area = egui::Area::new(Id::unique("nc-block-submenu"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(main_rect.max.x + 4.0, main_rect.min.y))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        if sub == 1 {
                            ui.set_width(240.0);
                            let items = filter_items("", &SlashMode::TurnInto);
                            egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                                for it in items {
                                    let (r, resp) =
                                        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
                                    if resp.hovered() {
                                        ui.painter().rect_filled(r, CornerRadius::same(6), pal.hover);
                                    }
                                    let mut child = ui.new_child(
                                        egui::UiBuilder::new()
                                            .max_rect(r.shrink2(Vec2::new(6.0, 2.0)))
                                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                    );
                                    // labels would take the click away from the row
                                    child.style_mut().interaction.selectable_labels = false;
                                    widgets::glyph_box(&mut child, it.glyph, pal, 22.0);
                                    child.label(
                                        RichText::new(tr(lang, &format!("blk_{}", it.key))).size(14.0).color(pal.text),
                                    );
                                    if resp.clicked() {
                                        deferred.push(Deferred::TurnInto(i, it.key));
                                        acted = true;
                                    }
                                }
                            });
                        } else {
                            ui.set_width(220.0);
                            let current = blocks[i].color.clone();
                            let mut pick: Option<(String, bool)> = None;
                            for bg in [false, true] {
                                let title = if bg { "color_bg_title" } else { "color_text_title" };
                                ui.label(RichText::new(tr(lang, title)).size(12.0).color(pal.text_faint));
                                let default_label = if bg { "color_bg_default" } else { "color_text_default" };
                                let entries = std::iter::once(String::new())
                                    .chain(blocks::BLOCK_COLORS.iter().map(|n| n.to_string()));
                                for name in entries {
                                    let key = match (name.is_empty(), bg) {
                                        (true, _) => String::new(),
                                        (false, true) => format!("bg-{name}"),
                                        (false, false) => name.clone(),
                                    };
                                    let (text, fill) = if key.is_empty() {
                                        (None, None)
                                    } else {
                                        theme::block_color(&key, pal.dark).unwrap_or((None, None))
                                    };
                                    let label = if name.is_empty() {
                                        tr(lang, default_label).to_string()
                                    } else {
                                        format!(
                                            "{} {}",
                                            tr(lang, if bg { "color_bg" } else { "color_text" }),
                                            tr(lang, &format!("color_{name}"))
                                        )
                                    };
                                    let (r, resp) =
                                        ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
                                    if resp.hovered() {
                                        ui.painter().rect_filled(r, CornerRadius::same(6), pal.hover);
                                    }
                                    let sw = Rect::from_min_size(
                                        egui::pos2(r.min.x + 6.0, r.center().y - 10.0),
                                        Vec2::splat(20.0),
                                    );
                                    ui.painter().rect_filled(sw, CornerRadius::same(4), fill.unwrap_or(pal.popup_bg));
                                    ui.painter().rect_stroke(
                                        sw,
                                        CornerRadius::same(4),
                                        Stroke::new(1.0, pal.border),
                                        egui::StrokeKind::Inside,
                                    );
                                    ui.painter().text(
                                        sw.center(),
                                        Align2::CENTER_CENTER,
                                        "A",
                                        FontId::proportional(13.0),
                                        text.unwrap_or(pal.text),
                                    );
                                    ui.painter().text(
                                        egui::pos2(sw.max.x + 8.0, r.center().y),
                                        Align2::LEFT_CENTER,
                                        &label,
                                        FontId::proportional(14.0),
                                        pal.text,
                                    );
                                    let is_current = if key.is_empty() {
                                        !current.split_whitespace().any(|p| p.starts_with("bg-") == bg)
                                    } else {
                                        current.split_whitespace().any(|p| p == key)
                                    };
                                    if is_current {
                                        ui.painter().text(
                                            egui::pos2(r.max.x - 8.0, r.center().y),
                                            Align2::RIGHT_CENTER,
                                            "✓",
                                            FontId::proportional(13.0),
                                            pal.text_secondary,
                                        );
                                    }
                                    if resp.clicked() {
                                        pick = Some((key, bg));
                                    }
                                }
                                ui.add_space(4.0);
                            }
                            if let Some(c) = pick {
                                deferred.push(Deferred::SetColor(i, c.0, c.1));
                                acted = true;
                            }
                        }
                    });
                });
            sub_area.response.clicked_elsewhere()
        } else {
            true
        };
        self.menu_sub = if q.is_empty() { sub } else { 0 };
        self.menu_query = query;
        // the click that opened the menu is "elsewhere" for it: ignore it in that frame
        if acted
            || (!opened_now && area.response.clicked_elsewhere() && sub_hit)
            || ctx.input(|i| i.key_pressed(Key::Escape))
        {
            self.block_menu = None;
            self.menu_sub = 0;
            self.menu_query.clear();
        }
    }

    pub(super) fn apply_deferred(&mut self, blocks: &mut Vec<Block>, d: Deferred, env: &Env, out: &mut Output) {
        let before = blocks.clone();
        match d {
            Deferred::ToggleCheck(i) => {
                if let BlockKind::Todo { checked } = &mut blocks[i].kind {
                    *checked = !*checked;
                }
            }
            Deferred::ToggleOpen(i) => {
                if let BlockKind::Toggle { open } = &mut blocks[i].kind {
                    *open = !*open;
                    let now_open = *open;
                    if now_open && blocks::subtree_end(blocks, i) == i + 1 {
                        let child = Block::paragraph("").with_depth((blocks[i].depth + 1).min(MAX_DEPTH));
                        self.focus = Some((child.id, Caret::Start));
                        blocks.insert(i + 1, child);
                    }
                }
            }
            Deferred::Shortcut(i, kind, strip) => {
                let id = blocks[i].id;
                let text = remove_chars(&blocks[i].text, 0, strip);
                if kind == BlockKind::Divider {
                    blocks[i].kind = BlockKind::Divider;
                    blocks[i].text.clear();
                    let p = Block::paragraph(text).with_depth(blocks[i].depth);
                    self.focus = Some((p.id, Caret::Start));
                    blocks.insert(i + 1, p);
                } else {
                    blocks[i].kind = kind;
                    blocks[i].text = text;
                    self.focus = Some((id, Caret::Start));
                }
                self.slash = None;
            }
            Deferred::PlusAfter(i) => {
                let end = blocks::subtree_end(blocks, i);
                let nb = Block::paragraph("/").with_depth(blocks[i].depth);
                self.focus = Some((nb.id, Caret::End));
                self.slash =
                    Some(Slash { block: nb.id, start: 0, query: String::new(), selected: 0, mode: SlashMode::Insert });
                blocks.insert(end, nb);
            }
            Deferred::AddColumn(s) => {
                if let Some((e, _)) = blocks::column_layout(blocks, s) {
                    let p = Block::paragraph("");
                    self.focus = Some((p.id, Caret::Start));
                    blocks.insert(e, p);
                    blocks.insert(e, Block::new(BlockKind::ColumnBreak, ""));
                }
            }
            Deferred::UnwrapColumns(s) => {
                if let Some((e, _)) = blocks::column_layout(blocks, s) {
                    let mut k = e + 1;
                    while k > s {
                        k -= 1;
                        if matches!(
                            blocks[k].kind,
                            BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd
                        ) {
                            blocks.remove(k);
                        }
                    }
                    if blocks.is_empty() {
                        blocks.push(Block::paragraph(""));
                    }
                }
            }
            Deferred::AppendParagraph => {
                let nb = Block::paragraph("");
                self.focus = Some((nb.id, Caret::Start));
                blocks.push(nb);
            }
            Deferred::SetCalloutIcon(i, e) => blocks[i].kind = BlockKind::Callout { icon: e },
            Deferred::TurnInto(i, key) => {
                let bid = blocks[i].id;
                self.focus = None;
                self.turn_range(blocks, i, key);
                self.selected = None;
                self.sel_anchor = None;
                if self.focus.is_none() {
                    self.focus = Some((bid, Caret::End));
                }
            }
            Deferred::Slash(i, key) => {
                if let Some(s) = self.slash.clone() {
                    self.apply_slash(blocks, i, key, &s, env, out);
                }
                return;
            }
            Deferred::Delete(i) => {
                let (i, end) = self.multi_range(blocks, i).unwrap_or((i, blocks::subtree_end(blocks, i)));
                blocks.drain(i..end);
                if blocks.is_empty() {
                    blocks.push(Block::paragraph(""));
                }
                self.selected = None;
                self.sel_anchor = None;
            }
            Deferred::SetColor(i, color, bg) => {
                // a multi-block selection is coloured as a whole
                let (lo, hi) = match self.selected_range(blocks) {
                    Some((a, b)) if (a..b).contains(&i) => (a, b),
                    _ => (i, blocks::subtree_end(blocks, i)),
                };
                for b in &mut blocks[lo..hi] {
                    if b.kind.has_rich_text() {
                        b.color = blocks::merge_color(&b.color, &color, bg);
                    }
                }
                // the selection highlight would hide the new colour
                self.selected = None;
                self.sel_anchor = None;
            }
            Deferred::Duplicate(i) => {
                self.duplicate(blocks, i, env, out);
                return;
            }
            Deferred::Move(i, up) => {
                self.sel_anchor = None;
                self.selected = Some(blocks[i].id);
                self.move_up_down(blocks, i, up, env, out);
                return;
            }
            Deferred::Open(path) => {
                out.actions.push(Action::OpenFile(path));
                return;
            }
        }
        if *blocks != before {
            self.record(env.key, &before, None);
            out.changed = true;
        }
    }
}

impl EditorState {
    /// ⌘K on a selection: a small field for the link address.
    pub(super) fn link_popup(
        &mut self,
        ctx: &egui::Context,
        blocks: &mut [Block],
        rects: &[(usize, Rect)],
        env: &Env,
        out: &mut Output,
    ) {
        let Some((bid, a, b, mut url)) = self.link_edit.clone() else { return };
        let Some(i) = blocks.iter().position(|x| x.id == bid) else {
            self.link_edit = None;
            return;
        };
        let Some((_, rect)) = rects.iter().find(|(j, _)| *j == i) else { return };
        let mut apply = false;
        let mut unlink = false;
        let mut cancel = ctx.input_mut(|inp| inp.consume_key(Modifiers::NONE, Key::Escape));
        egui::Area::new(Id::unique("nc-link-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(rect.min.x + 46.0, rect.max.y + 4.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut url)
                                .id(Id::unique("nc-link-url"))
                                .desired_width(300.0)
                                .hint_text(tr(env.lang, "link_hint")),
                        );
                        if !r.has_focus() && !r.lost_focus() {
                            r.request_focus();
                        }
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            apply = true;
                        }
                        let ok = ui.button(tr(env.lang, "done"));
                        crate::widgets::a11y(&ok, tr(env.lang, "done"));
                        if ok.clicked() {
                            apply = true;
                        }
                        if self.link_replace.is_some() && ui.button(tr(env.lang, "remove_link")).clicked() {
                            unlink = true;
                        }
                        if ui.button("×").on_hover_text(tr(env.lang, "cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if let (Some((label, d0, d1)), true) = (self.link_replace, apply || unlink) {
            // existing link: change the address, or drop the link and keep its label
            let before = blocks.to_vec();
            let text = blocks[i].text.clone();
            let valid = d1 <= text.len() && label >= 1 && label <= d0 && d0 >= 2;
            if valid {
                blocks[i].text = if unlink || url.trim().is_empty() {
                    format!("{}{}{}", &text[..label - 1], &text[label..d0 - 2], &text[d1 + 1..])
                } else {
                    format!("{}{}{}", &text[..d0], normalize_url(&url).replace(' ', "%20"), &text[d1..])
                };
                self.record(env.key, &before, None);
                out.changed = true;
            }
            self.focus = Some((bid, Caret::At(a)));
            self.link_edit = None;
            self.link_replace = None;
        } else if apply && !url.trim().is_empty() {
            let before = blocks.to_vec();
            let (t, sel) = inline::toggle_wrap(
                &blocks[i].text,
                (a, b),
                "[",
                &format!("]({})", normalize_url(&url).replace(' ', "%20")),
            );
            blocks[i].text = t;
            self.focus = Some((bid, Caret::At(sel.1)));
            self.record(env.key, &before, None);
            out.changed = true;
            self.link_edit = None;
        } else if cancel {
            self.focus = Some((bid, Caret::Select(a, b)));
            self.link_edit = None;
            self.link_replace = None;
        } else {
            self.link_edit = Some((bid, a, b, url));
        }
    }
}
