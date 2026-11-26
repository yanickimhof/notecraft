//! The table block: cells, rows and columns, keyboard moves between cells.

use super::*;

impl EditorState {
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
        let cols = cells.first().map_or(1, Vec::len);
        let rows = cells.len();
        // column widths follow the content: a short "Date" column stays narrow, a long "Status" one grows
        let col_ws: Vec<f32> = {
            let total = (width - 40.0).max(60.0 * cols as f32);
            let weights: Vec<f32> = (0..cols)
                .map(|c| {
                    let lens: Vec<usize> = cells.iter().map(|r| r.get(c).map_or(0, |t| visible_len(t))).collect();
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
                        .flat_map(|t| t.split_whitespace())
                        .map(|w| w.chars().count())
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
        if let Some((r, c)) = focused_cell {
            let cid = Id::unique(("table-cell", bid, r, c));
            let edge: Option<(Id, bool, bool, bool, bool)> = tctx.data(|d| d.get_temp(Id::unique(("table-edge", bid))));
            let (first, last, start, end) =
                edge.filter(|e| e.0 == cid).map(|e| (e.1, e.2, e.3, e.4)).unwrap_or((true, true, false, false));
            let none = Modifiers::NONE;
            let mut target: Option<(usize, usize)> = None;
            tctx.input_mut(|i| {
                if i.consume_key(Modifiers::SHIFT, Key::Tab) {
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
            if let Some((tr, tc)) = target {
                if tr >= rows {
                    add_row = true;
                }
                self.table_focus = Some((bid, tr, tc));
            }
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            egui::ScrollArea::horizontal().id_salt(("table-scroll", bid)).auto_shrink([false, true]).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (c, &cw) in col_ws.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, 16.0), Sense::click());
                            if cols > 1 && ui.rect_contains_pointer(r.expand2(Vec2::new(0.0, 4.0))) {
                                ui.painter().text(
                                    r.center(),
                                    Align2::CENTER_CENTER,
                                    "×",
                                    FontId::proportional(10.0),
                                    pal.text_faint,
                                );
                                if resp.clicked() {
                                    drop_col = Some(c);
                                }
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
                                let job = inline_job(c, &row_base, pal, col_ws[ci] - 18.0);
                                ui.fonts_mut(|f| f.layout_job(job)).size().y
                            })
                            .fold(row_base.size * 1.5, f32::max);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for (ci, cell) in row.iter_mut().enumerate() {
                                let frame = egui::Frame::NONE
                                    .stroke(Stroke::new(1.0, pal.border))
                                    .fill(if ri == 0 { pal.hover } else { Color32::TRANSPARENT })
                                    .inner_margin(egui::Margin::symmetric(8, 5));
                                frame.show(ui, |ui| {
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
                                    let mut buf: String = ui
                                        .data(|d| d.get_temp::<String>(raw_id))
                                        .filter(|r| r.trim() == cell.as_str())
                                        .unwrap_or_else(|| cell.clone());
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
                                            ui.data_mut(|d| {
                                                d.insert_temp(
                                                    Id::unique(("table-edge", bid)),
                                                    (cid, first, last, start, end),
                                                )
                                            });
                                        }
                                    }
                                    let buf = buf.replace('\r', "");
                                    ui.data_mut(|d| d.insert_temp(raw_id, buf.clone()));
                                    *cell = buf.trim().to_string();
                                    if self.table_focus == Some((bid, ri, ci)) {
                                        resp.request_focus();
                                        self.table_focus = None;
                                    }
                                });
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
            blocks[i].text = notecraft_core::blocks::cells_to_text(&cells);
            out.changed = true;
        }
        ui.allocate_rect(ui.min_rect(), Sense::hover())
    }
}
