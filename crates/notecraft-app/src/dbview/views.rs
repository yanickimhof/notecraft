//! The table, board, gallery, calendar, timeline and list views (split out of mod.rs, unchanged).

use super::*;

impl DbUi {
    // ------------------------------------------------------------------- table

    #[allow(clippy::too_many_arguments)]
    pub(super) fn table(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let schema = ws.dbs[db].schema.clone();
        let props: Vec<Property> = schema.visible_props(view).into_iter().cloned().collect();
        let groups = database::query(&ws.dbs[db], view);
        let grouped = groups.len() > 1 || groups.first().is_some_and(|g| g.key.is_some());
        let gprop = database::group_prop(&schema, view).cloned();
        // column widths: the saved ones, or the live width while a column handle is being dragged
        let mut widths: Vec<f32> = props
            .iter()
            .map(|p| match &self.col_drag {
                Some((id, w)) if *id == p.id => *w,
                _ => col_width(view, p),
            })
            .collect();
        // keyboard: the text cells of the table form a grid the arrow keys, Tab and Enter move across
        let grid: Vec<Vec<Option<Id>>> = groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .map(|rid| props.iter().map(|p| text_cell_id(db, rid, p)).collect())
            .collect();
        self.table_keys(ui, &grid);
        if let Some(rid) = self.focus_row.clone() {
            let at = groups.iter().flat_map(|g| g.rows.iter()).position(|r| *r == rid);
            if let Some(at) = at {
                self.focus_row = None;
                if let Some(id) = grid[at].iter().flatten().next().copied() {
                    ui.memory_mut(|m| m.request_focus(id));
                }
            }
        }
        let total_w: f32 = widths.iter().sum::<f32>() + 40.0;
        egui::ScrollArea::horizontal().id_salt(("tbl", db, &view.id)).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_min_width(total_w);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (ci, p) in props.iter().enumerate() {
                        let w = widths[ci];
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::ZERO, pal.hover);
                        }
                        ui.painter().text(
                            rect.left_center() + Vec2::new(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            format!("{}  {}", kind_glyph(p.kind), p.name),
                            egui::FontId::proportional(13.0),
                            pal.text_secondary,
                        );
                        ui.painter().vline(rect.max.x, rect.y_range(), Stroke::new(1.0, pal.border));
                        // drag the right edge of the header to resize the column
                        let grip = ui.interact(
                            egui::Rect::from_min_max(
                                egui::pos2(rect.max.x - 4.0, rect.min.y),
                                egui::pos2(rect.max.x + 4.0, rect.max.y),
                            ),
                            Id::unique(("col-grip", db, &p.id)),
                            Sense::drag(),
                        );
                        if grip.hovered() || grip.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                            ui.painter().vline(rect.max.x, rect.y_range(), Stroke::new(2.0, pal.accent));
                        }
                        if grip.dragged() {
                            let nw = (w + grip.drag_delta().x).clamp(60.0, 900.0);
                            self.col_drag = Some((p.id.clone(), nw));
                            widths[ci] = nw;
                        }
                        if grip.drag_stopped() {
                            if let Some((pid, nw)) = self.col_drag.take() {
                                edit_view(ws, db, &view.id, actions, |v| {
                                    v.widths.insert(pid, nw);
                                });
                            }
                        }
                        egui::Popup::menu(&resp)
                            .id(Id::unique(("prop-menu", &p.id)))
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                self.property_menu(ui, ws, db, view, p, pal, lang, actions);
                            });
                    }
                    let add = ui.add(
                        egui::Button::new(RichText::new("+").size(16.0).color(pal.text_secondary))
                            .min_size(Vec2::new(36.0, 32.0)),
                    );
                    crate::widgets::a11y(&add, tr(lang, "add_property"));
                    egui::Popup::menu(&add).id(Id::unique(("add-prop", db))).show(|ui| {
                        ui.label(small(tr(lang, "add_property"), pal));
                        for k in PropKind::ADDABLE {
                            if ui.button(format!("{}  {}", kind_glyph(k), kind_name(lang, k))).clicked() {
                                if let Err(e) = ws.add_property(db, kind_name(lang, k), k) {
                                    actions.push(DbAction::Error(e.to_string()));
                                }
                            }
                        }
                    });
                });
                hline(ui, total_w, pal);
                for g in &groups {
                    if grouped {
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            group_label(ui, gprop.as_ref(), g.key.as_deref(), pal, lang);
                            ui.label(small(&g.rows.len().to_string(), pal));
                        });
                        hline(ui, total_w, pal);
                    }
                    for rid in &g.rows {
                        let row_resp = ui.allocate_ui_with_layout(
                            Vec2::new(total_w, 34.0),
                            egui::Layout::left_to_right(egui::Align::TOP),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                for (ci, p) in props.iter().enumerate() {
                                    let w = widths[ci];
                                    let cell = ui.allocate_ui_with_layout(
                                        Vec2::new(w, 34.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_size(Vec2::new(w, 34.0));
                                            ui.set_max_width(w);
                                            ui.add_space(8.0);
                                            if p.kind == PropKind::Title {
                                                self.title_cell(ui, ws, db, rid, w - 14.0, pal, lang, actions);
                                            } else {
                                                cell_editor(
                                                    ui,
                                                    ws,
                                                    db,
                                                    rid,
                                                    p,
                                                    w - 14.0,
                                                    pal,
                                                    lang,
                                                    actions,
                                                    &mut self.new_option,
                                                );
                                            }
                                        },
                                    );
                                    let _ = cell;
                                }
                            },
                        );
                        // column separators span the whole row, which grows with wrapped text
                        let rr = row_resp.response.rect;
                        let mut x = rr.min.x;
                        for w in &widths {
                            x += *w;
                            ui.painter().vline(x, rr.y_range(), Stroke::new(1.0, pal.border));
                        }
                        row_resp.response.context_menu(|ui| {
                            if ui.button(tr(lang, "open_as_page")).clicked() {
                                actions.push(DbAction::Open(DocId::Row(db.to_string(), rid.clone())));
                                ui.close();
                            }
                            if ui.button(tr(lang, "duplicate")).clicked() {
                                let _ = ws.duplicate_row(db, rid);
                                ui.close();
                            }
                            if ui.button(RichText::new(tr(lang, "delete_row")).color(pal.danger)).clicked() {
                                let _ = ws.trash_doc(&DocId::Row(db.to_string(), rid.clone()));
                                ui.close();
                            }
                        });
                        hline(ui, total_w, pal);
                    }
                    let add = ui.add(
                        egui::Button::new(
                            RichText::new(format!("+  {}", tr(lang, "new_row"))).size(13.5).color(pal.text_faint),
                        )
                        .min_size(Vec2::new(0.0, 30.0)),
                    );
                    if add.clicked() {
                        let group = gprop.as_ref().filter(|_| grouped).map(|p| (p.id.clone(), g.key.clone()));
                        self.add_row(ws, db, &view.id, None, group, actions);
                    }
                }
                ui.add_space(6.0);
                let count = database::sorted_rows(&ws.dbs[db], view).len();
                ui.label(small(&format!("{}  {count}", tr(lang, "count")), pal));
            });
        });
    }

    /// Keyboard moves between the text cells: Tab and Shift+Tab along the rows, Enter and the up and
    /// down arrows between rows, left and right arrows at the edges of the text, Escape to leave.
    fn table_keys(&mut self, ui: &mut Ui, grid: &[Vec<Option<Id>>]) {
        let ctx = ui.ctx().clone();
        let Some(f) = ctx.memory(|m| m.focused()) else { return };
        let Some((r, c)) =
            grid.iter().enumerate().find_map(|(r, row)| row.iter().position(|x| *x == Some(f)).map(|c| (r, c)))
        else {
            return;
        };
        let edge: Option<(Id, bool, bool, bool, bool)> = ctx.data(|d| d.get_temp(Id::unique("nc-db-edge")));
        let (first, last, start, end) =
            edge.filter(|e| e.0 == f).map(|e| (e.1, e.2, e.3, e.4)).unwrap_or((true, true, false, false));
        // text cells of the grid in reading order
        let flat: Vec<(usize, usize, Id)> = grid
            .iter()
            .enumerate()
            .flat_map(|(ri, row)| row.iter().enumerate().filter_map(move |(ci, id)| id.map(|id| (ri, ci, id))))
            .collect();
        let at = flat.iter().position(|(ri, ci, _)| (*ri, *ci) == (r, c));
        let vertical = |delta: i32| -> Option<Id> {
            let nr = r as i32 + delta;
            if nr < 0 {
                return None;
            }
            grid.get(nr as usize).and_then(|row| row.get(c).copied().flatten())
        };
        let step = |delta: i32| -> Option<Id> {
            let k = at? as i32 + delta;
            (k >= 0).then(|| flat.get(k as usize).map(|x| x.2)).flatten()
        };
        let none = egui::Modifiers::NONE;
        let mut target: Option<Id> = None;
        let mut leave = false;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                target = step(-1);
            } else if i.consume_key(none, egui::Key::Tab) {
                target = step(1);
            } else if i.consume_key(none, egui::Key::Enter) {
                target = vertical(1);
            } else if first && i.consume_key(none, egui::Key::ArrowUp) {
                target = vertical(-1);
            } else if last && i.consume_key(none, egui::Key::ArrowDown) {
                target = vertical(1);
            } else if start && i.consume_key(none, egui::Key::ArrowLeft) {
                target = step(-1);
            } else if end && i.consume_key(none, egui::Key::ArrowRight) {
                target = step(1);
            } else if i.consume_key(none, egui::Key::Escape) {
                leave = true;
            }
        });
        if leave {
            ctx.memory_mut(|m| m.surrender_focus(f));
        } else if let Some(t) = target {
            ctx.memory_mut(|m| m.request_focus(t));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn title_cell(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        rid: &str,
        w: f32,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let row = &ws.dbs[db].rows[rid];
        let icon = row.meta.icon.clone();
        let mut title = row.meta.title.clone();
        if !icon.is_empty() {
            ui.label(RichText::new(&icon).size(15.0));
        }
        // a long title wraps onto several lines; Shift+Enter is ignored, Enter moves to the row below
        let tid = Id::unique(("title-cell", db, rid));
        let o = egui::TextEdit::multiline(&mut title)
            .id(tid)
            .frame(egui::Frame::NONE)
            .desired_width((w - 60.0).max(40.0))
            .desired_rows(1)
            .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)))
            .font(egui::FontId::proportional(14.5))
            .hint_text(RichText::new(tr(lang, "untitled")).color(pal.text_faint))
            .show(ui);
        note_caret_edges(ui, tid, &o);
        let resp = o.response;
        if resp.changed() {
            title = title.replace('\n', " ");
            if let Some(r) = ws.dbs.get_mut(db).and_then(|d| d.rows.get_mut(rid)) {
                r.meta.title = title;
                r.touch();
            }
            let _ = ws.save_row(db, rid);
        }
        // quiet at rest: the button shows while the pointer is over this cell
        if ui.rect_contains_pointer(ui.max_rect()) {
            let open = ui.add(
                egui::Button::new(RichText::new(tr(lang, "open")).size(11.5).color(pal.text_secondary))
                    .fill(pal.bg)
                    .stroke(Stroke::new(1.0, pal.border))
                    .corner_radius(CornerRadius::same(4)),
            );
            if open.clicked() {
                actions.push(DbAction::Open(DocId::Row(db.to_string(), rid.to_string())));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn property_menu(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        p: &Property,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        ui.set_min_width(230.0);
        let buf = self.rename_buf.entry(format!("prop:{}", p.id)).or_insert_with(|| p.name.clone());
        let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(210.0));
        if resp.lost_focus() && buf.trim() != p.name && !buf.trim().is_empty() {
            let name = buf.trim().to_string();
            if let Err(e) = ws.rename_property(db, &p.id, &name) {
                actions.push(DbAction::Error(e.to_string()));
                *buf = p.name.clone();
            }
        }
        if p.kind != PropKind::Title {
            ui.menu_button(
                format!("{}  {}: {}", kind_glyph(p.kind), tr(lang, "type"), kind_name(lang, p.kind)),
                |ui| {
                    for k in PropKind::ADDABLE {
                        if ui
                            .selectable_label(k == p.kind, format!("{}  {}", kind_glyph(k), kind_name(lang, k)))
                            .clicked()
                        {
                            if let Some(pp) =
                                ws.dbs.get_mut(db).and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id))
                            {
                                pp.kind = k;
                                if k == PropKind::Status && pp.options.is_empty() {
                                    pp.options = database::default_status_options();
                                }
                            }
                            save_schema(ws, db, actions);
                        }
                    }
                },
            );
        }
        if p.kind == PropKind::Relation {
            let dbs: Vec<(String, String)> =
                ws.dbs.iter().map(|(id, d)| (id.clone(), d.schema.title.clone())).collect();
            ui.menu_button(tr(lang, "relation_to"), |ui| {
                for (id, title) in dbs {
                    let cur = p.relation_db.as_deref().unwrap_or(db) == id;
                    if ui.selectable_label(cur, title).clicked() {
                        if let Some(pp) =
                            ws.dbs.get_mut(db).and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id))
                        {
                            pp.relation_db = Some(id);
                        }
                        save_schema(ws, db, actions);
                    }
                }
            });
        }
        if p.kind == PropKind::Formula {
            ui.menu_button(tr(lang, "formula_edit"), |ui| {
                ui.set_width(340.0);
                let mut text = p.formula.clone();
                let r = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .hint_text(RichText::new("prop(\"Price\") * prop(\"Qty\")").color(pal.text_faint)),
                );
                if r.changed() {
                    if let Some(pp) =
                        ws.dbs.get_mut(db).and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id))
                    {
                        pp.formula = text.clone();
                    }
                    save_schema(ws, db, actions);
                }
                if !text.trim().is_empty() {
                    if let Err(e) = notecraft_core::formula::check(&text) {
                        ui.label(RichText::new(e).size(12.0).color(pal.danger));
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new(tr(lang, "formula_help")).size(12.0).color(pal.text_faint));
            });
        }
        if p.kind == PropKind::Rollup {
            rollup_menus(ui, ws, db, p, pal, lang, actions);
        }
        if p.kind.has_options() {
            ui.menu_button(tr(lang, "options"), |ui| options_editor(ui, ws, db, &p.id, pal, lang, actions));
        }
        ui.separator();
        if ui.button(format!("↑  {}", tr(lang, "ascending"))).clicked() {
            let id = p.id.clone();
            edit_view(ws, db, &view.id, actions, |vv| vv.sorts = vec![Sort { prop: id, descending: false }]);
            ui.close();
        }
        if ui.button(format!("↓  {}", tr(lang, "descending"))).clicked() {
            let id = p.id.clone();
            edit_view(ws, db, &view.id, actions, |vv| vv.sorts = vec![Sort { prop: id, descending: true }]);
            ui.close();
        }
        if p.kind != PropKind::Title {
            if ui.button(tr(lang, "hide")).clicked() {
                let all: Vec<String> = ws.dbs[db].schema.properties.iter().map(|x| x.id.clone()).collect();
                let id = p.id.clone();
                edit_view(ws, db, &view.id, actions, |vv| {
                    if vv.visible.is_empty() {
                        vv.visible = all;
                    }
                    vv.visible.retain(|x| x != &id);
                });
                ui.close();
            }
            if ui.button(RichText::new(tr(lang, "delete_property")).color(pal.danger)).clicked() {
                if let Err(e) = ws.delete_property(db, &p.id) {
                    actions.push(DbAction::Error(e.to_string()));
                }
                ui.close();
            }
        }
    }

    // ------------------------------------------------------------------- board

    #[allow(clippy::too_many_arguments)]
    pub(super) fn board(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let schema = ws.dbs[db].schema.clone();
        let Some(gprop) = database::group_prop(&schema, view).cloned() else {
            ui.label(small(tr(lang, "group"), pal));
            return;
        };
        let groups = database::query(&ws.dbs[db], view);
        let props: Vec<Property> = schema
            .visible_props(view)
            .into_iter()
            .filter(|p| p.kind != PropKind::Title && p.id != gprop.id)
            .cloned()
            .collect();
        let mut moved: Option<(String, Option<String>)> = None;
        let column_bg =
            if pal.dark { Color32::from_rgb(0x24, 0x24, 0x27) } else { Color32::from_rgb(0xF7, 0xF7, 0xF5) };
        egui::ScrollArea::horizontal().id_salt(("board", db, &view.id)).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for g in &groups {
                    if g.key.is_none() && g.rows.is_empty() {
                        continue;
                    }
                    let frame = egui::Frame::NONE
                        .inner_margin(egui::Margin::same(8))
                        .corner_radius(CornerRadius::same(8))
                        .fill(column_bg);
                    let (_, dropped) = ui.dnd_drop_zone::<CardDrag, ()>(frame, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(252.0);
                            ui.set_min_height(140.0);
                            ui.horizontal(|ui| {
                                group_label(ui, Some(&gprop), g.key.as_deref(), pal, lang);
                                ui.label(small(&g.rows.len().to_string(), pal));
                            });
                            ui.add_space(4.0);
                            for rid in &g.rows {
                                let id = Id::unique(("card", db, rid, g.key.clone()));
                                let mut clicked = false;
                                ui.dnd_drag_source(id, CardDrag { row: rid.clone() }, |ui| {
                                    let r = egui::Frame::NONE
                                        .fill(pal.popup_bg)
                                        .stroke(Stroke::new(1.0, pal.border))
                                        .corner_radius(CornerRadius::same(6))
                                        .shadow(egui::epaint::Shadow {
                                            offset: [0, 1],
                                            blur: 3,
                                            spread: 0,
                                            color: Color32::from_black_alpha(if pal.dark { 60 } else { 14 }),
                                        })
                                        .inner_margin(egui::Margin::symmetric(10, 8))
                                        .show(ui, |ui| {
                                            ui.vertical(|ui| {
                                                ui.set_width(230.0);
                                                let row = &ws.dbs[db].rows[rid];
                                                let title = if row.meta.title.is_empty() {
                                                    tr(lang, "untitled").to_string()
                                                } else {
                                                    row.meta.title.clone()
                                                };
                                                let icon = if row.meta.icon.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!("{} ", row.meta.icon)
                                                };
                                                ui.label(
                                                    RichText::new(format!("{icon}{title}")).size(14.0).color(pal.text),
                                                );
                                                for p in &props {
                                                    show_value_readonly(ui, ws, db, rid, p, pal);
                                                }
                                            })
                                        });
                                    clicked = r.response.interact(Sense::click()).clicked();
                                });
                                if clicked {
                                    actions.push(DbAction::Open(DocId::Row(db.to_string(), rid.clone())));
                                }
                                ui.add_space(3.0);
                            }
                            let add = ui.add(egui::Button::new(
                                RichText::new(format!("+  {}", tr(lang, "new_row"))).size(13.0).color(pal.text_faint),
                            ));
                            if add.clicked() {
                                self.add_row(ws, db, &view.id, None, Some((gprop.id.clone(), g.key.clone())), actions);
                            }
                        })
                        .inner
                    });
                    if let Some(p) = dropped {
                        moved = Some((p.row.clone(), g.key.clone()));
                    }
                    ui.add_space(4.0);
                }
            });
        });
        if let Some((rid, key)) = moved {
            if let Some(row) = ws.dbs.get_mut(db).and_then(|d| d.rows.get_mut(&rid)) {
                row.set_value(&gprop, group_value(&gprop, key));
                row.touch();
            }
            let _ = ws.save_row(db, &rid);
        }
    }

    // ----------------------------------------------------------------- gallery

    #[allow(clippy::too_many_arguments)]
    pub(super) fn gallery(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let schema = ws.dbs[db].schema.clone();
        let props: Vec<Property> =
            schema.visible_props(view).into_iter().filter(|p| p.kind != PropKind::Title).cloned().collect();
        let rows: Vec<String> = database::query(&ws.dbs[db], view).into_iter().flat_map(|g| g.rows).collect();
        let card_w = 220.0;
        let cols = ((ui.available_width() + 12.0) / (card_w + 12.0)).floor().max(1.0) as usize;
        let mut all: Vec<Option<&String>> = rows.iter().map(Some).collect();
        all.push(None);
        for chunk in all.chunks(cols) {
            ui.horizontal_top(|ui| {
                for rid in chunk {
                    let frame = egui::Frame::NONE
                        .fill(pal.popup_bg)
                        .stroke(Stroke::new(1.0, pal.border))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(12));
                    let r = frame.show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(card_w - 24.0);
                            ui.set_min_height(96.0);
                            match rid {
                                Some(rid) => {
                                    let row = &ws.dbs[db].rows[*rid];
                                    let icon = if row.meta.icon.is_empty() {
                                        "📄".to_string()
                                    } else {
                                        row.meta.icon.clone()
                                    };
                                    let title = if row.meta.title.is_empty() {
                                        tr(lang, "untitled").to_string()
                                    } else {
                                        row.meta.title.clone()
                                    };
                                    ui.label(RichText::new(icon).size(22.0));
                                    ui.label(RichText::new(title).size(14.5).strong().color(pal.text));
                                    for p in &props {
                                        show_value_readonly(ui, ws, db, rid, p, pal);
                                    }
                                }
                                None => {
                                    ui.add_space(30.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new(format!("+  {}", tr(lang, "new_row")))
                                                .size(13.5)
                                                .color(pal.text_faint),
                                        );
                                    });
                                }
                            }
                        });
                    });
                    let resp = r.response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.hovered() {
                        ui.painter().rect_stroke(
                            r.response.rect,
                            CornerRadius::same(8),
                            Stroke::new(1.0, pal.accent),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if resp.clicked() {
                        match rid {
                            Some(rid) => actions.push(DbAction::Open(DocId::Row(db.to_string(), (*rid).clone()))),
                            None => self.add_row(ws, db, &view.id, None, None, actions),
                        }
                    }
                    ui.add_space(4.0);
                }
            });
            ui.add_space(4.0);
        }
    }

    // ---------------------------------------------------------------- calendar

    #[allow(clippy::too_many_arguments)]
    pub(super) fn calendar(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let schema = ws.dbs[db].schema.clone();
        let Some(dprop) = database::date_prop(&schema, view).cloned() else {
            ui.label(small(tr(lang, "no_date_property"), pal));
            return;
        };
        let today = jiff::Zoned::now().date();
        let key = format!("{db}:{}", view.id);
        let (year, month) = *self.cal.entry(key.clone()).or_insert((today.year(), today.month()));
        let first = jiff::civil::date(year, month, 1);
        let months_fr = [
            "janvier",
            "février",
            "mars",
            "avril",
            "mai",
            "juin",
            "juillet",
            "août",
            "septembre",
            "octobre",
            "novembre",
            "décembre",
        ];
        let title = if lang == Lang::Fr {
            format!("{} {year}", months_fr[month as usize - 1])
        } else {
            first.strftime("%B %Y").to_string()
        };
        let mut shift = 0i8;
        let mut go_today = false;
        ui.horizontal(|ui| {
            if ui.button("‹").clicked() {
                shift = -1;
            }
            if ui.button("›").clicked() {
                shift = 1;
            }
            ui.label(RichText::new(title).size(16.0).strong());
            if ui.small_button(tr(lang, "today")).clicked() {
                go_today = true;
            }
        });
        if shift != 0 || go_today {
            let target = if go_today {
                today.first_of_month()
            } else {
                first.checked_add(jiff::Span::new().months(i64::from(shift))).unwrap_or(first)
            };
            self.cal.insert(key, (target.year(), target.month()));
        }
        // rows by day
        let rows: Vec<String> = database::query(&ws.dbs[db], view).into_iter().flat_map(|g| g.rows).collect();
        let mut by_day: HashMap<String, Vec<String>> = HashMap::new();
        for rid in rows {
            if let PropValue::Date(s) = ws.dbs[db].rows[&rid].value(&dprop) {
                if let Some(day) = s.get(..10) {
                    by_day.entry(day.to_string()).or_default().push(rid);
                }
            }
        }
        let weekdays_en = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let weekdays_fr = ["Lun", "Mar", "Mer", "Jeu", "Ven", "Sam", "Dim"];
        let names = if lang == Lang::Fr { weekdays_fr } else { weekdays_en };
        let width = ui.available_width();
        let cell_w = width / 7.0;
        let cell_h = 104.0;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for n in names {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(cell_w, 18.0), Sense::hover());
                ui.painter().text(
                    rect.left_center() + Vec2::new(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    n,
                    egui::FontId::proportional(12.0),
                    pal.text_faint,
                );
            }
        });
        let lead = i64::from(first.weekday().to_monday_zero_offset());
        let days = i64::from(first.days_in_month());
        let weeks = (lead + days + 6) / 7;
        let mut open: Option<String> = None;
        let mut add_on: Option<String> = None;
        for w in 0..weeks {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for d in 0..7 {
                    let n = w * 7 + d - lead + 1;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), Sense::click());
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::ZERO,
                        Stroke::new(0.5, pal.border),
                        egui::StrokeKind::Inside,
                    );
                    if n < 1 || n > days {
                        ui.painter().rect_filled(rect.shrink(0.5), CornerRadius::ZERO, pal.hover.gamma_multiply(0.35));
                        continue;
                    }
                    let date = jiff::civil::date(year, month, n as i8);
                    let iso = date.to_string();
                    let is_today = date == today;
                    if is_today {
                        ui.painter().circle_filled(rect.min + Vec2::new(14.0, 13.0), 10.0, pal.accent);
                    }
                    ui.painter().text(
                        rect.min + Vec2::new(14.0, 13.0),
                        egui::Align2::CENTER_CENTER,
                        n.to_string(),
                        egui::FontId::proportional(12.0),
                        if is_today { Color32::WHITE } else { pal.text_secondary },
                    );
                    let list = by_day.get(&iso).cloned().unwrap_or_default();
                    let mut y = rect.min.y + 28.0;
                    for (k, rid) in list.iter().enumerate() {
                        if k == 3 {
                            ui.painter().text(
                                egui::pos2(rect.min.x + 8.0, y + 6.0),
                                egui::Align2::LEFT_CENTER,
                                format!("+{}", list.len() - 3),
                                egui::FontId::proportional(11.5),
                                pal.text_faint,
                            );
                            break;
                        }
                        let chip_rect =
                            egui::Rect::from_min_size(egui::pos2(rect.min.x + 4.0, y), Vec2::new(cell_w - 8.0, 20.0));
                        let r = ui.interact(chip_rect, Id::unique(("cal-chip", db, rid)), Sense::click());
                        let row = &ws.dbs[db].rows[rid];
                        let t = if row.meta.title.is_empty() {
                            tr(lang, "untitled").to_string()
                        } else {
                            row.meta.title.clone()
                        };
                        ui.painter().rect_filled(
                            chip_rect,
                            CornerRadius::same(4),
                            if r.hovered() { pal.accent_soft } else { pal.hover },
                        );
                        ui.painter().with_clip_rect(chip_rect.shrink2(Vec2::new(4.0, 0.0))).text(
                            chip_rect.left_center() + Vec2::new(4.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            t,
                            egui::FontId::proportional(12.0),
                            pal.text,
                        );
                        if r.clicked() {
                            open = Some(rid.clone());
                        }
                        y += 23.0;
                    }
                    if resp.hovered() && ui.rect_contains_pointer(rect) {
                        ui.painter().text(
                            rect.right_top() + Vec2::new(-12.0, 13.0),
                            egui::Align2::CENTER_CENTER,
                            "+",
                            egui::FontId::proportional(16.0),
                            pal.text_faint,
                        );
                    }
                    if resp.clicked() {
                        add_on = Some(iso);
                    }
                }
            });
        }
        if let Some(rid) = open {
            actions.push(DbAction::Open(DocId::Row(db.to_string(), rid)));
        } else if let Some(iso) = add_on {
            self.add_row(ws, db, &view.id, None, Some((dprop.id.clone(), Some(iso))), actions);
        }
        let _ = ws;
    }

    // ---------------------------------------------------------------- timeline

    #[allow(clippy::too_many_arguments)]
    pub(super) fn timeline(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        use jiff::civil::Date;
        if database::date_prop(&ws.dbs[db].schema, view).is_none() {
            ui.label(small(tr(lang, "no_date_property"), pal));
            return;
        }
        let (items, undated) = database::timeline_items(&ws.dbs[db], view);
        let key = format!("{db}:{}", view.id);
        let zoom = *self.tl_zoom.entry(key.clone()).or_insert(16.0);
        let mut new_zoom = zoom;
        let mut to_today = false;
        ui.horizontal(|ui| {
            if ui.button("−").on_hover_text(tr(lang, "timeline_zoom_out")).clicked() {
                new_zoom = (zoom / 1.4).max(3.0);
            }
            if ui.button("+").on_hover_text(tr(lang, "timeline_zoom_in")).clicked() {
                new_zoom = (zoom * 1.4).min(80.0);
            }
            if ui.small_button(tr(lang, "today")).clicked() {
                to_today = true;
            }
        });
        if new_zoom != zoom {
            self.tl_zoom.insert(key.clone(), new_zoom);
        }
        let zoom = new_zoom;
        let today = jiff::Zoned::now().date();
        let first_day: Date = items.iter().map(|i| i.start).chain([today]).min().unwrap_or(today);
        let last_day: Date = items.iter().map(|i| i.end).chain([today]).max().unwrap_or(today);
        let origin = first_day.checked_add(jiff::Span::new().days(-7)).unwrap_or(first_day);
        let end = last_day.checked_add(jiff::Span::new().days(30)).unwrap_or(last_day);
        let days = i64::from(origin.until(end).map(|s| s.get_days()).unwrap_or(60)).max(30);
        let day_x = |d: Date| -> f32 { origin.until(d).map(|s| s.get_days() as f32).unwrap_or(0.0) * zoom };
        let (header_h, row_h) = (40.0, 28.0);
        let width = days as f32 * zoom;
        let height = header_h + items.len().max(1) as f32 * row_h + 8.0;
        let mut open: Option<String> = None;
        let mut scroll_to: Option<f32> = None;
        if to_today {
            scroll_to = Some((day_x(today) - 120.0).max(0.0));
        }
        let mut area = egui::ScrollArea::horizontal().id_salt(("timeline", &key)).auto_shrink([false, true]);
        if let Some(x) = scroll_to {
            area = area.horizontal_scroll_offset(x);
        }
        area.show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
            let painter = ui.painter_at(rect);
            let months_fr =
                ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
            let months_en = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let names = if lang == Lang::Fr { months_fr } else { months_en };
            // Month labels and week lines.
            let mut d = origin;
            let mut i = 0;
            while d <= end {
                let x = rect.min.x + day_x(d);
                let monday = d.weekday() == jiff::civil::Weekday::Monday;
                if d.day() == 1 || d == origin {
                    if d.day() == 1 {
                        painter.vline(x, rect.min.y..=rect.max.y, Stroke::new(1.0, pal.border));
                    }
                    painter.text(
                        egui::pos2(x + 4.0, rect.min.y + 4.0),
                        egui::Align2::LEFT_TOP,
                        format!("{} {}", names[d.month() as usize - 1], d.year()),
                        egui::FontId::proportional(12.0),
                        pal.text_secondary,
                    );
                } else if monday {
                    painter.vline(x, rect.min.y + header_h..=rect.max.y, Stroke::new(0.5, pal.border));
                }
                if (zoom >= 12.0 && monday) || (zoom >= 30.0) {
                    painter.text(
                        egui::pos2(x + 3.0, rect.min.y + 22.0),
                        egui::Align2::LEFT_TOP,
                        d.day().to_string(),
                        egui::FontId::proportional(11.0),
                        pal.text_faint,
                    );
                }
                d = match d.checked_add(jiff::Span::new().days(1)) {
                    Ok(n) => n,
                    Err(_) => break,
                };
                i += 1;
                if i > 4000 {
                    break;
                }
            }
            painter.hline(rect.x_range(), rect.min.y + header_h, Stroke::new(1.0, pal.border));
            // Today.
            let tx = rect.min.x + day_x(today);
            painter.vline(tx, rect.min.y + header_h - 6.0..=rect.max.y, Stroke::new(1.5, pal.accent));
            // Bars.
            for (n, it) in items.iter().enumerate() {
                let y = rect.min.y + header_h + 4.0 + n as f32 * row_h;
                let x0 = rect.min.x + day_x(it.start);
                let x1 = (rect.min.x + day_x(it.end) + zoom).max(x0 + 8.0);
                let bar = egui::Rect::from_min_max(egui::pos2(x0, y), egui::pos2(x1, y + row_h - 8.0));
                let resp = ui.interact(bar, Id::unique(("tl-bar", db, &it.row)), Sense::click());
                painter.rect_filled(
                    bar,
                    CornerRadius::same(5),
                    if resp.hovered() { pal.accent } else { pal.accent_soft },
                );
                let row = &ws.dbs[db].rows[&it.row];
                let title =
                    if row.meta.title.is_empty() { tr(lang, "untitled").to_string() } else { row.meta.title.clone() };
                let galley = painter.layout_no_wrap(title, egui::FontId::proportional(12.5), pal.text);
                let fits = galley.size().x + 12.0 <= bar.width();
                let pos = if fits {
                    egui::pos2(bar.min.x + 6.0, bar.center().y - galley.size().y / 2.0)
                } else {
                    egui::pos2(bar.max.x + 6.0, bar.center().y - galley.size().y / 2.0)
                };
                painter.galley(pos, galley, pal.text);
                if resp.clicked() {
                    open = Some(it.row.clone());
                }
            }
            if items.is_empty() {
                painter.text(
                    egui::pos2(rect.min.x + 12.0, rect.min.y + header_h + 18.0),
                    egui::Align2::LEFT_CENTER,
                    tr(lang, "chart_empty"),
                    egui::FontId::proportional(13.0),
                    pal.text_faint,
                );
            }
        });
        if !undated.is_empty() {
            ui.add_space(10.0);
            ui.label(small(&format!("{} ({})", tr(lang, "timeline_undated"), undated.len()), pal));
            for rid in &undated {
                let row = &ws.dbs[db].rows[rid];
                let title =
                    if row.meta.title.is_empty() { tr(lang, "untitled").to_string() } else { row.meta.title.clone() };
                if ui.add(egui::Button::new(RichText::new(title).size(13.0).color(pal.text))).clicked() {
                    open = Some(rid.clone());
                }
            }
        }
        if let Some(rid) = open {
            actions.push(DbAction::Open(DocId::Row(db.to_string(), rid)));
        }
    }

    // -------------------------------------------------------------------- list

    #[allow(clippy::too_many_arguments)]
    pub(super) fn list(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let schema = ws.dbs[db].schema.clone();
        let props: Vec<Property> =
            schema.visible_props(view).into_iter().filter(|p| p.kind != PropKind::Title).cloned().collect();
        let groups = database::query(&ws.dbs[db], view);
        let grouped = groups.len() > 1 || groups.first().is_some_and(|g| g.key.is_some());
        let gprop = database::group_prop(&schema, view).cloned();
        for g in &groups {
            if grouped {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    group_label(ui, gprop.as_ref(), g.key.as_deref(), pal, lang);
                    ui.label(small(&g.rows.len().to_string(), pal));
                });
            }
            for rid in &g.rows {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(4), pal.hover);
                }
                let row = &ws.dbs[db].rows[rid];
                let icon = if row.meta.icon.is_empty() { "📄".to_string() } else { row.meta.icon.clone() };
                let title =
                    if row.meta.title.is_empty() { tr(lang, "untitled").to_string() } else { row.meta.title.clone() };
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                // the text of a row is not selectable: it would take the click meant for the row
                child.style_mut().interaction.selectable_labels = false;
                child.label(RichText::new(icon).size(15.0));
                child.label(RichText::new(title).size(14.5).color(pal.text));
                child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for p in props.iter().rev() {
                        show_value_readonly(ui, ws, db, rid, p, pal);
                    }
                });
                if resp.clicked() {
                    actions.push(DbAction::Open(DocId::Row(db.to_string(), rid.clone())));
                }
                ui.painter().hline(rect.x_range(), rect.max.y, Stroke::new(1.0, pal.border));
            }
            let add = ui.add(egui::Button::new(
                RichText::new(format!("+  {}", tr(lang, "new_row"))).size(13.5).color(pal.text_faint),
            ));
            if add.clicked() {
                let group = gprop.as_ref().filter(|_| grouped).map(|p| (p.id.clone(), g.key.clone()));
                self.add_row(ws, db, &view.id, None, group, actions);
            }
        }
    }

    /// Property list on top of a row page (or a template when `template`).
    #[allow(clippy::too_many_arguments)]
    pub fn row_properties(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        rid: &str,
        template: bool,
        pal: &Palette,
        lang: Lang,
    ) -> Vec<DbAction> {
        let mut actions = Vec::new();
        let props: Vec<Property> =
            ws.dbs[db].schema.properties.iter().filter(|p| p.kind != PropKind::Title).cloned().collect();
        egui::Grid::new(("row-props", db, rid)).num_columns(2).spacing(Vec2::new(16.0, 6.0)).min_row_height(30.0).show(
            ui,
            |ui| {
                for p in &props {
                    ui.add_sized(
                        Vec2::new(170.0, 26.0),
                        egui::Label::new(
                            RichText::new(format!("{}   {}", kind_glyph(p.kind), p.name))
                                .size(14.0)
                                .color(pal.text_secondary),
                        )
                        .truncate(),
                    );
                    ui.horizontal(|ui| {
                        if template {
                            template_cell(ui, ws, db, rid, p, pal, lang, &mut actions);
                        } else {
                            cell_editor(ui, ws, db, rid, p, 400.0, pal, lang, &mut actions, &mut self.new_option);
                        }
                    });
                    ui.end_row();
                }
            },
        );
        let add = ui.add(egui::Button::new(
            RichText::new(format!("+  {}", tr(lang, "add_property"))).size(13.5).color(pal.text_faint),
        ));
        egui::Popup::menu(&add).id(Id::unique(("row-add-prop", db))).show(|ui| {
            for k in PropKind::ADDABLE {
                if ui.button(format!("{}  {}", kind_glyph(k), kind_name(lang, k))).clicked() {
                    if let Err(e) = ws.add_property(db, kind_name(lang, k), k) {
                        actions.push(DbAction::Error(e.to_string()));
                    }
                }
            }
        });
        actions
    }
}
