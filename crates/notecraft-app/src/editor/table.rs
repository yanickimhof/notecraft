//! The table block: cells, rows and columns, keyboard moves between cells.

use super::*;

const GRIP_W: f32 = 14.0;

/// What the table bar applies to the selected text or cells.
#[derive(Clone, Copy)]
enum CellFormat {
    Style(inline::StyleKind),
    Color(Option<&'static str>),
    Fill(Option<&'static str>),
}

impl EditorState {
    fn select_cells(&mut self, ctx: &egui::Context, bid: u64, a: (usize, usize), b: (usize, usize)) {
        self.table_sel = Some((bid, a, b));
        self.table_range = None;
        ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
    }

    /// Apply a format to the text selected in `cell` (or the whole cell when nothing is selected), or to every
    /// cell of `many` when there is no single cell.
    fn apply_cell_format(
        &mut self,
        ctx: &egui::Context,
        cells: &mut [Vec<String>],
        bid: u64,
        cell: Option<(usize, usize)>,
        many: &[(usize, usize)],
        f: CellFormat,
    ) {
        let one = |text: &str, range: Option<(usize, usize)>| -> (String, Option<(usize, usize)>) {
            let (fill, body) = inline::split_fill(text);
            let n = body.chars().count();
            let (a, b) = match range {
                Some((a, b)) if a != b => (a.min(n), b.min(n)),
                _ => {
                    // the whole text without the markers that already wrap it, so a second press undoes the first
                    let mask = inline::marker_mask(body);
                    let first = mask.iter().position(|m| !m);
                    let last = mask.iter().rposition(|m| !m);
                    match (first, last) {
                        (Some(f), Some(l)) => (f, l + 1),
                        _ => (0, n),
                    }
                }
            };
            match f {
                CellFormat::Fill(name) => (inline::set_fill(text, name), None),
                _ if n == 0 => (text.to_string(), None),
                CellFormat::Style(kind) => {
                    let (t, sel) = inline::toggle_style(body, (a, b), kind);
                    (inline::set_fill(&t, fill), Some(sel))
                }
                CellFormat::Color(name) => {
                    let (t, sel) = inline::set_color(body, (a, b), name);
                    (inline::set_fill(&t, fill), Some(sel))
                }
            }
        };
        if let Some((r, c)) = cell {
            let cid = Id::unique(("table-cell", bid, r, c));
            let range = egui::text_edit::TextEditState::load(ctx, cid)
                .and_then(|s| s.cursor.char_range())
                .map(|g| (g.primary.index.0.min(g.secondary.index.0), g.primary.index.0.max(g.secondary.index.0)))
                .filter(|g| g.0 != g.1)
                .or(self.table_range.filter(|t| (t.1, t.2) == (r, c)).map(|t| (t.3, t.4)));
            let (t, sel) = one(&cells[r][c], range);
            cells[r][c] = t;
            self.table_focus = Some((bid, r, c));
            self.table_range_set = sel;
            ctx.request_repaint();
        } else {
            for &(r, c) in many {
                cells[r][c] = one(&cells[r][c], None).0;
            }
        }
    }

    pub(super) fn table_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        cx: &BlockCx,
        sink: &mut Sink,
    ) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let out = &mut *sink.out;
        let pal = env.pal;
        let bid = blocks[i].id;
        let mut cells = notecraft_core::blocks::table_cells(&blocks[i].text);
        let before = cells.clone();
        // typing joins one undo step per burst; formats and structure changes get a step of their own
        let mut structural = false;
        let cols = cells.first().map_or(1, Vec::len);
        let rows = cells.len();
        // column widths follow the content: a short "Date" column stays narrow, a long "Status" one grows
        let col_ws: Vec<f32> = {
            let total = (width - 40.0).max(60.0 * cols as f32);
            let weights: Vec<f32> = (0..cols)
                .map(|c| {
                    let lens: Vec<usize> =
                        cells.iter().map(|r| r.get(c).map_or(0, |t| visible_len(inline::split_fill(t).1))).collect();
                    let avg = lens.iter().sum::<usize>() as f32 / lens.len().max(1) as f32;
                    let max = lens.iter().copied().max().unwrap_or(0) as f32;
                    (0.6 * avg + 0.4 * max.min(120.0)).clamp(8.0, 90.0)
                })
                .collect();
            let sum: f32 = weights.iter().sum();
            // never narrower than the longest word, so words are not cut in the middle
            let char_w = 7.0 * theme::body_scale();
            let mins: Vec<f32> = (0..cols)
                .map(|c| {
                    let longest = cells
                        .iter()
                        .filter_map(|r| r.get(c))
                        .map(|t| inline::strip(inline::split_fill(t).1))
                        .flat_map(|t| t.split_whitespace().map(|w| w.chars().count()).collect::<Vec<_>>())
                        .max()
                        .unwrap_or(0) as f32;
                    (longest * char_w + 26.0).clamp(60.0, total * 0.4)
                })
                .collect();
            let min_sum: f32 = mins.iter().sum();
            if min_sum >= total {
                mins.iter().map(|m| m * total / min_sum).collect()
            } else {
                let extra = total - min_sum;
                (0..cols).map(|c| mins[c] + extra * weights[c] / sum).collect()
            }
        };
        let mut drop_row: Option<usize> = None;
        let mut drop_col: Option<usize> = None;
        let (mut add_row, mut add_col) = (false, false);
        // a new table puts the caret in its first cell
        if self.focus.is_some_and(|(fb, _)| fb == bid) {
            self.focus = None;
            self.table_focus = Some((bid, 0, 0));
        }
        // Keys inside the grid: Tab and Shift+Tab, Enter (cell below), the arrows at the edges of a cell
        // text. Tab or Enter in the last row adds a row.
        let tctx = ui.ctx().clone();
        let focused_cell = (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (r, c)))
            .find(|&(r, c)| tctx.memory(|m| m.has_focus(Id::unique(("table-cell", bid, r, c)))));
        let mut selected_now = false;
        if let Some((r, c)) = focused_cell {
            let cid = Id::unique(("table-cell", bid, r, c));
            let edge: Option<(Id, bool, bool, bool, bool, bool, bool)> =
                tctx.data(|d| d.get_temp(Id::unique(("table-edge", bid))));
            let (first, last, start, end, at_start, at_end) = edge
                .filter(|e| e.0 == cid)
                .map(|e| (e.1, e.2, e.3, e.4, e.5, e.6))
                .unwrap_or((true, true, false, false, false, false));
            let none = Modifiers::NONE;
            let mut target: Option<(usize, usize)> = None;
            // Shift+arrow at the edge of the cell text carries on over the neighbouring cells as a cell selection
            let mut extend: Option<(usize, usize)> = None;
            tctx.input_mut(|i| {
                if first && r > 0 && i.consume_key(Modifiers::SHIFT, Key::ArrowUp) {
                    extend = Some((r - 1, c));
                } else if last && r + 1 < rows && i.consume_key(Modifiers::SHIFT, Key::ArrowDown) {
                    extend = Some((r + 1, c));
                } else if at_start && c > 0 && i.consume_key(Modifiers::SHIFT, Key::ArrowLeft) {
                    extend = Some((r, c - 1));
                } else if at_end && c + 1 < cols && i.consume_key(Modifiers::SHIFT, Key::ArrowRight) {
                    extend = Some((r, c + 1));
                } else if i.consume_key(Modifiers::SHIFT, Key::Tab) {
                    target = if c > 0 {
                        Some((r, c - 1))
                    } else if r > 0 {
                        Some((r - 1, cols - 1))
                    } else {
                        None
                    };
                } else if i.consume_key(none, Key::Tab) {
                    target = Some(if c + 1 < cols { (r, c + 1) } else { (r + 1, 0) });
                } else if !i.modifiers.shift && i.consume_key(none, Key::Enter) {
                    target = Some((r + 1, c));
                } else if first && r > 0 && i.consume_key(none, Key::ArrowUp) {
                    target = Some((r - 1, c));
                } else if last && r + 1 < rows && i.consume_key(none, Key::ArrowDown) {
                    target = Some((r + 1, c));
                } else if start && (c > 0 || r > 0) && i.consume_key(none, Key::ArrowLeft) {
                    target = Some(if c > 0 { (r, c - 1) } else { (r - 1, cols - 1) });
                } else if end && (c + 1 < cols || r + 1 < rows) && i.consume_key(none, Key::ArrowRight) {
                    target = Some(if c + 1 < cols { (r, c + 1) } else { (r + 1, 0) });
                }
            });
            if let Some(to) = extend {
                self.select_cells(&tctx, bid, (r, c), to);
                selected_now = true;
            }
            if let Some((tr, tc)) = target {
                if tr >= rows {
                    add_row = true;
                }
                self.table_focus = Some((bid, tr, tc));
            }
        }
        // Formatting shortcuts act on the text selected in a cell, or on every selected cell.
        let sel_here = self.table_sel.is_some_and(|s| s.0 == bid);
        let sel_rect = self.table_sel.filter(|s| s.0 == bid).map(|(_, a, b)| {
            (
                a.0.min(b.0).min(rows - 1),
                a.1.min(b.1).min(cols - 1),
                a.0.max(b.0).min(rows - 1),
                a.1.max(b.1).min(cols - 1),
            )
        });
        let mut rect_cells: Vec<(usize, usize)> = Vec::new();
        if let Some((r0, c0, r1, c1)) = sel_rect {
            for r in r0..=r1 {
                for c in c0..=c1 {
                    rect_cells.push((r, c));
                }
            }
        }
        if focused_cell.is_some() || sel_here {
            let cmd = Modifiers::COMMAND;
            let kinds = [
                (cmd, Key::B, inline::StyleKind::Bold),
                (cmd, Key::I, inline::StyleKind::Italic),
                (cmd, Key::U, inline::StyleKind::Underline),
                (Modifiers::COMMAND | Modifiers::SHIFT, Key::S, inline::StyleKind::Strike),
                (cmd, Key::E, inline::StyleKind::Code),
            ];
            let mut req: Option<inline::StyleKind> = None;
            tctx.input_mut(|i| {
                for (m, k, s) in kinds {
                    if req.is_none() && i.consume_key(m, k) {
                        req = Some(s);
                    }
                }
            });
            if let Some(kind) = req {
                structural = true;
                self.apply_cell_format(&tctx, &mut cells, bid, focused_cell, &rect_cells, CellFormat::Style(kind));
            }
        }
        if let (Some((_, anchor, head)), None) = (self.table_sel.filter(|s| s.0 == bid), focused_cell) {
            let mut to = head;
            tctx.input_mut(|i| {
                if i.consume_key(Modifiers::SHIFT, Key::ArrowUp) {
                    to.0 = to.0.saturating_sub(1);
                } else if i.consume_key(Modifiers::SHIFT, Key::ArrowDown) {
                    to.0 = (to.0 + 1).min(rows - 1);
                } else if i.consume_key(Modifiers::SHIFT, Key::ArrowLeft) {
                    to.1 = to.1.saturating_sub(1);
                } else if i.consume_key(Modifiers::SHIFT, Key::ArrowRight) {
                    to.1 = (to.1 + 1).min(cols - 1);
                }
            });
            if to != head {
                self.select_cells(&tctx, bid, anchor, to);
            }
        }
        if let (true, None) = (sel_here, focused_cell) {
            let (mut clear, mut esc, mut copy) = (false, false, false);
            tctx.input_mut(|i| {
                clear = i.consume_key(Modifiers::NONE, Key::Backspace) || i.consume_key(Modifiers::NONE, Key::Delete);
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
                copy = i.consume_key(Modifiers::COMMAND, Key::C);
            });
            if clear {
                structural = true;
                for &(r, c) in &rect_cells {
                    let fill = inline::split_fill(&cells[r][c]).0.map(str::to_string);
                    cells[r][c] = inline::set_fill("", fill.as_deref());
                }
            }
            if copy {
                if let Some((r0, c0, r1, c1)) = sel_rect {
                    let tsv: Vec<String> = (r0..=r1)
                        .map(|r| {
                            (c0..=c1)
                                .map(|c| inline::strip(&cells[r][c]).replace(['\t', '\n'], " "))
                                .collect::<Vec<_>>()
                                .join("\t")
                        })
                        .collect();
                    tctx.copy_text(tsv.join("\n"));
                }
            }
            if esc {
                self.table_sel = None;
            }
        }
        // the text selected inside the focused cell (the format bar acts on it)
        if let Some((r, c)) = focused_cell {
            self.table_last = Some((bid, r, c));
            let cid = Id::unique(("table-cell", bid, r, c));
            let rg = egui::text_edit::TextEditState::load(&tctx, cid)
                .and_then(|s| s.cursor.char_range())
                .map(|g| (g.primary.index.0.min(g.secondary.index.0), g.primary.index.0.max(g.secondary.index.0)));
            self.table_range = rg.filter(|g| g.0 != g.1).map(|g| (bid, r, c, g.0, g.1));
            if !selected_now && self.table_sel.is_some_and(|s| s.0 == bid) {
                self.table_sel = None;
            }
        } else if self.table_range.is_some_and(|t| t.0 == bid) {
            let over =
                self.table_bar_rect.zip(tctx.input(|i| i.pointer.interact_pos())).is_some_and(|(r, p)| r.contains(p));
            if !over {
                self.table_range = None;
            }
        }
        let mut cell_rects: Vec<Vec<Rect>> = vec![vec![Rect::NOTHING; cols]; rows];
        let mut grip_clicked: Option<usize> = None;
        let mut strip_clicked: Option<usize> = None;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            egui::ScrollArea::horizontal().id_salt(("table-scroll", bid)).auto_shrink([false, true]).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(GRIP_W);
                        for (c, &cw) in col_ws.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, 16.0), Sense::click());
                            let hot = ui.rect_contains_pointer(r.expand2(Vec2::new(0.0, 4.0)));
                            if hot {
                                ui.painter().text(
                                    r.center(),
                                    Align2::CENTER_CENTER,
                                    "▾",
                                    FontId::proportional(10.0),
                                    pal.text_faint,
                                );
                                if cols > 1 {
                                    let x = Rect::from_center_size(
                                        egui::pos2(r.max.x - 10.0, r.center().y),
                                        Vec2::splat(14.0),
                                    );
                                    ui.painter().text(
                                        x.center(),
                                        Align2::CENTER_CENTER,
                                        "×",
                                        FontId::proportional(10.0),
                                        pal.text_faint,
                                    );
                                    if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| x.contains(p)) {
                                        drop_col = Some(c);
                                        continue;
                                    }
                                }
                            }
                            if resp.clicked() {
                                strip_clicked = Some(c);
                            }
                        }
                    });
                    for (ri, row) in cells.iter_mut().enumerate() {
                        // every cell of a row takes the height of its tallest text
                        let row_base = Base {
                            size: (theme::BODY_SIZE - 1.0) * theme::body_scale(),
                            weight: if ri == 0 { 600.0 } else { 400.0 },
                            color: pal.text,
                            strike: false,
                        };
                        let row_h = row
                            .iter()
                            .enumerate()
                            .map(|(ci, c)| {
                                let job = inline_job(inline::split_fill(c).1, &row_base, pal, col_ws[ci] - 18.0);
                                ui.fonts_mut(|f| f.layout_job(job)).size().y
                            })
                            .fold(row_base.size * 1.5, f32::max);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let (grip, gresp) = ui.allocate_exact_size(Vec2::new(GRIP_W, row_h + 10.0), Sense::click());
                            if ui.rect_contains_pointer(grip) {
                                ui.painter().text(
                                    grip.center(),
                                    Align2::CENTER_CENTER,
                                    "⠿",
                                    FontId::proportional(11.0),
                                    pal.text_faint,
                                );
                            }
                            if gresp.clicked() {
                                grip_clicked = Some(ri);
                            }
                            for (ci, cell) in row.iter_mut().enumerate() {
                                let fill_name = inline::split_fill(cell).0.map(str::to_string);
                                let fill = match fill_name.as_deref() {
                                    Some(n) => pal.option_tokens(n).0,
                                    None if ri == 0 => pal.hover,
                                    None => Color32::TRANSPARENT,
                                };
                                let frame = egui::Frame::NONE
                                    .stroke(Stroke::new(1.0, pal.border))
                                    .fill(fill)
                                    .inner_margin(egui::Margin::symmetric(8, 5));
                                let shown = frame.show(ui, |ui| {
                                    ui.set_min_height(row_h);
                                    // long text wraps inside the cell; a cell stays one line of text
                                    // inline styles (bold, links, colours...) render inside cells too
                                    let cell_base = Base {
                                        size: (theme::BODY_SIZE - 1.0) * theme::body_scale(),
                                        weight: if ri == 0 { 600.0 } else { 400.0 },
                                        color: pal.text,
                                        strike: false,
                                    };
                                    let mut cell_layouter =
                                        |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| -> Arc<egui::Galley> {
                                            let job = inline_job(buf.as_str(), &cell_base, pal, wrap);
                                            ui.fonts_mut(|f| f.layout_job(job))
                                        };
                                    // the text being typed keeps its spaces; the stored cell is trimmed
                                    let cid = Id::unique(("table-cell", bid, ri, ci));
                                    let raw_id = Id::unique(("table-raw", bid, ri, ci));
                                    let body = inline::split_fill(cell).1.to_string();
                                    let mut buf: String = ui
                                        .data(|d| d.get_temp::<String>(raw_id))
                                        .filter(|r| r.trim() == body.as_str())
                                        .unwrap_or_else(|| body.clone());
                                    if self.table_focus == Some((bid, ri, ci)) {
                                        if let Some((a, b)) = self.table_range_set.take() {
                                            set_cursor(ui.ctx(), cid, Caret::Select(a, b), buf.chars().count());
                                        }
                                    }
                                    let o = egui::TextEdit::multiline(&mut buf)
                                        .desired_rows(1)
                                        .return_key(Some(egui::KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)))
                                        .id(cid)
                                        .frame(egui::Frame::NONE)
                                        .desired_width(col_ws[ci] - 18.0)
                                        .layouter(&mut cell_layouter)
                                        .show(ui);
                                    let resp = o.response.clone();
                                    if resp.has_focus() {
                                        if let Some(r) = o.cursor_range {
                                            let here = o.galley.pos_from_cursor(r.primary);
                                            let first = here.min.y <= 1.0;
                                            let last = here.max.y >= o.galley.rect.max.y - 1.0;
                                            let collapsed = r.primary.index.0 == r.secondary.index.0;
                                            let start = collapsed && r.primary.index.0 == 0;
                                            let end = collapsed && r.primary.index.0 >= o.galley.text().chars().count();
                                            let n = o.galley.text().chars().count();
                                            let at_start = r.primary.index.0 == 0;
                                            let at_end = r.primary.index.0 >= n;
                                            ui.data_mut(|d| {
                                                d.insert_temp(
                                                    Id::unique(("table-edge", bid)),
                                                    (cid, first, last, start, end, at_start, at_end),
                                                )
                                            });
                                        }
                                    }
                                    let buf = buf.replace('\r', "");
                                    ui.data_mut(|d| d.insert_temp(raw_id, buf.clone()));
                                    *cell = inline::set_fill(buf.trim(), fill_name.as_deref());
                                    if self.table_focus == Some((bid, ri, ci)) {
                                        resp.request_focus();
                                        self.table_focus = None;
                                    }
                                });
                                cell_rects[ri][ci] = shown.response.rect;
                            }
                            if rows > 1
                                && ui
                                    .add(
                                        egui::Button::new(RichText::new("×").size(15.0).color(pal.text_faint))
                                            .frame(false),
                                    )
                                    .clicked()
                            {
                                drop_row = Some(ri);
                            }
                        });
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("+ {}", tr(env.lang, "table_row"))).size(12.0).color(pal.text_faint),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    add_row = true;
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("+ {}", tr(env.lang, "table_col"))).size(12.0).color(pal.text_faint),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    add_col = true;
                }
            });
        });
        // selecting cells: row grip, column strip, shift-click, drag from one cell to another
        let hit = |p: egui::Pos2| -> Option<(usize, usize)> {
            (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))).find(|&(r, c)| cell_rects[r][c].contains(p))
        };
        if let Some(r) = grip_clicked {
            self.select_cells(&tctx, bid, (r, 0), (r, cols - 1));
        }
        if let Some(c) = strip_clicked {
            self.select_cells(&tctx, bid, (0, c), (rows - 1, c));
        }
        let (pressed, down, shift, pos) = tctx.input(|i| {
            (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers.shift, i.pointer.interact_pos())
        });
        let on_bar = self.table_bar_rect.zip(pos).is_some_and(|(r, p)| r.contains(p));
        if pressed && !on_bar {
            if let Some(at) = pos.and_then(hit) {
                match self.table_last.filter(|l| l.0 == bid) {
                    Some((_, lr, lc)) if shift => self.select_cells(&tctx, bid, (lr, lc), at),
                    _ => {
                        self.table_press = Some((bid, at.0, at.1));
                        if self.table_sel.is_some_and(|s| s.0 == bid) {
                            self.table_sel = None;
                        }
                    }
                }
            }
        }
        if down {
            if let (Some((pb, pr, pc)), Some(at)) = (self.table_press, pos.and_then(hit)) {
                if pb == bid && (pr, pc) != at {
                    self.select_cells(&tctx, bid, (pr, pc), at);
                }
            }
        } else if self.table_press.is_some_and(|p| p.0 == bid) {
            self.table_press = None;
        }
        // an Escape inside a cell selects that cell
        if let Some((r, c)) = focused_cell {
            if tctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.select_cells(&tctx, bid, (r, c), (r, c));
            }
        }
        // selection tint
        let sel_now = self.table_sel.filter(|s| s.0 == bid).map(|(_, a, b)| {
            (
                a.0.min(b.0).min(rows - 1),
                a.1.min(b.1).min(cols - 1),
                a.0.max(b.0).min(rows - 1),
                a.1.max(b.1).min(cols - 1),
            )
        });
        let mut bar_at: Option<Rect> = None;
        if let Some((r0, c0, r1, c1)) = sel_now {
            let area = cell_rects[r0][c0].union(cell_rects[r1][c1]);
            ui.painter().rect_filled(area, 0.0, pal.accent.gamma_multiply(0.16));
            ui.painter().rect_stroke(area, 0.0, Stroke::new(1.5, pal.accent), egui::StrokeKind::Inside);
            bar_at = Some(area);
        } else if let Some((tb, r, c, _, _)) = self.table_range {
            if tb == bid && r < rows && c < cols {
                bar_at = Some(cell_rects[r][c]);
            }
        }
        let mut applied: Option<CellFormat> = None;
        if let Some(at) = bar_at {
            let lang = env.lang;
            let menu = self.table_menu;
            let mut open_menu: Option<u8> = None;
            let area = egui::Area::new(Id::unique(("table-bar", bid)))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(
                    at.min.x.max(4.0),
                    (at.min.y - 40.0 - if menu > 0 { 32.0 } else { 0.0 }).max(4.0),
                ))
                .show(&tctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        if menu > 0 {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let r = ui.button("×").on_hover_text(tr(lang, "color_text_default"));
                                if r.clicked() {
                                    applied =
                                        Some(if menu == 1 { CellFormat::Color(None) } else { CellFormat::Fill(None) });
                                }
                                for name in blocks::BLOCK_COLORS {
                                    let dot = pal.option_tokens(name).2;
                                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                                    let r = r.on_hover_text(tr(lang, &format!("color_{name}")));
                                    ui.painter().circle_filled(rect.center(), if r.hovered() { 9.0 } else { 7.5 }, dot);
                                    if r.clicked() {
                                        applied = Some(if menu == 1 {
                                            CellFormat::Color(Some(name))
                                        } else {
                                            CellFormat::Fill(Some(name))
                                        });
                                    }
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let items: [(RichText, inline::StyleKind, &str); 5] = [
                                (RichText::new("B").strong(), inline::StyleKind::Bold, "fmt_bold"),
                                (RichText::new("I").italics(), inline::StyleKind::Italic, "fmt_italic"),
                                (RichText::new("U").underline(), inline::StyleKind::Underline, "fmt_underline"),
                                (RichText::new("S").strikethrough(), inline::StyleKind::Strike, "fmt_strike"),
                                (RichText::new("</>").monospace(), inline::StyleKind::Code, "fmt_code"),
                            ];
                            for (label, kind, tip) in items {
                                if ui.button(label).on_hover_text(tr(lang, tip)).clicked() {
                                    applied = Some(CellFormat::Style(kind));
                                }
                            }
                            ui.separator();
                            if ui
                                .button(RichText::new("A").color(pal.accent))
                                .on_hover_text(tr(lang, "color_text_title"))
                                .clicked()
                            {
                                open_menu = Some(if menu == 1 { 0 } else { 1 });
                            }
                            if ui.button("▨").on_hover_text(tr(lang, "table_fill")).clicked() {
                                open_menu = Some(if menu == 2 { 0 } else { 2 });
                            }
                        });
                    });
                });
            self.table_bar_rect = Some(area.response.rect);
            if let Some(m) = open_menu {
                self.table_menu = m;
            }
        } else {
            self.table_bar_rect = None;
            self.table_menu = 0;
        }
        if let Some(f) = applied {
            let target = if sel_now.is_some() { None } else { self.table_range.map(|t| (t.1, t.2)) };
            structural = true;
            self.apply_cell_format(&tctx, &mut cells, bid, target, &rect_cells, f);
            if !matches!(f, CellFormat::Style(_)) {
                self.table_menu = 0;
            }
        }
        structural |= drop_row.is_some() || drop_col.is_some() || add_row || add_col;
        if let Some(r) = drop_row {
            cells.remove(r);
        }
        if let Some(c) = drop_col {
            for row in &mut cells {
                row.remove(c);
            }
        }
        if add_row {
            cells.push(vec![String::new(); cols]);
        }
        if add_col {
            for row in &mut cells {
                row.push(String::new());
            }
        }
        if cells != before || blocks[i].text.trim().is_empty() && cells != notecraft_core::blocks::table_cells("") {
            let snapshot = blocks.to_vec();
            blocks[i].text = notecraft_core::blocks::cells_to_text(&cells);
            let group = (!structural).then(|| format!("t{bid}"));
            self.record(env.key, &snapshot, group.as_deref());
            out.changed = true;
        }
        ui.allocate_rect(ui.min_rect(), Sense::hover())
    }
}
