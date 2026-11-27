//! Block selection: the range, growing and shrinking it with the keyboard, turning a range into another
//! type, and the keys that work while blocks (not text) are selected.

use super::*;

impl EditorState {
    /// Blocks covered by the block selection as `[start, end)` (a block brings its children).
    pub(super) fn selected_range(&self, blocks: &[Block]) -> Option<(usize, usize)> {
        let sel = self.selected?;
        let i = blocks.iter().position(|b| b.id == sel)?;
        let j = self.sel_anchor.and_then(|a| blocks.iter().position(|b| b.id == a)).unwrap_or(i);
        let (lo, hi) = (i.min(j), i.max(j));
        Some((lo, blocks::subtree_end(blocks, hi)))
    }

    /// Shift+Up / Shift+Down on a block selection: grow or shrink it by one visible block.
    /// A block carries its children, so growing downwards steps over the whole subtree of the moving
    /// end (stepping onto its first child would shrink the selection instead), and shrinking steps back
    /// over a whole subtree too, so the highlight always changes by one block in the key's direction.
    pub(super) fn extend_block_selection(&mut self, blocks: &[Block], i: usize, down: bool) {
        let Some(sel) = self.selected else { return };
        let hidden = blocks::hidden_by_toggles(blocks);
        let anchor = self.sel_anchor.and_then(|a| blocks.iter().position(|b| b.id == a)).unwrap_or(i);
        let target = if down {
            if i >= anchor {
                (blocks::subtree_end(blocks, i)..blocks.len()).find(|j| !hidden[*j])
            } else {
                (i + 1..blocks.len()).find(|j| !hidden[*j])
            }
        } else if i > anchor {
            // shrinking from below: back to the block whose subtree ended where this one starts
            (anchor..i).rev().find(|j| !hidden[*j]).map(|mut j| {
                while let Some(k) = (anchor..j).rev().find(|k| blocks[*k].depth < blocks[j].depth) {
                    if blocks::subtree_end(blocks, k) <= i {
                        j = k;
                    } else {
                        break;
                    }
                }
                j
            })
        } else {
            (0..i).rev().find(|j| !hidden[*j])
        };
        if let Some(j) = target {
            self.sel_anchor.get_or_insert(sel);
            self.selected = Some(blocks[j].id);
        }
    }

    /// Selects every block of the page (the title is not part of it).
    pub(super) fn select_all_blocks(&mut self, blocks: &[Block]) {
        if let (Some(first), Some(last)) = (blocks.first(), blocks.last()) {
            self.selected = Some(last.id);
            self.sel_anchor = Some(first.id);
        }
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
            super::richpaste::copy_blocks(ctx, &blocks[range_start..range_end]);
        }
        if consume(cmd, Key::A) {
            self.select_all_blocks(blocks);
            return;
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
            self.extend_block_selection(blocks, i, false);
        } else if consume(Modifiers::SHIFT, Key::ArrowDown) {
            self.extend_block_selection(blocks, i, true);
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
            // a range is left from its top edge, a single block from itself
            let multi = self.sel_anchor.take().is_some();
            let from = if multi { range_start } else { i };
            let hidden = blocks::hidden_by_toggles(blocks);
            if let Some(j) = (0..from).rev().find(|j| !hidden[*j]) {
                self.selected = Some(blocks[j].id);
            } else if multi {
                self.selected = Some(blocks[range_start].id);
            }
        } else if consume(Modifiers::NONE, Key::ArrowDown) {
            // a range is left from its bottom edge, a single block from itself
            let multi = self.sel_anchor.take().is_some();
            let from = if multi { range_end } else { i + 1 };
            let hidden = blocks::hidden_by_toggles(blocks);
            if let Some(j) = (from..blocks.len()).find(|j| !hidden[*j]) {
                self.selected = Some(blocks[j].id);
            } else if multi {
                self.selected = Some(blocks[range_end - 1].id);
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
}
