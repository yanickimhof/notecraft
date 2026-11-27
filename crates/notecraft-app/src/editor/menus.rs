//! The slash menu, the block menu, the link popup and the actions that wait for the next frame.

use super::*;

impl EditorState {
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
            k if k.starts_with("date_") => {
                // the date goes in the text, at the place of the "/query"
                let text = date_text(k, env.lang == Lang::Fr);
                self.date_key = SLASH_ITEMS.iter().find(|it| it.key == k).map_or("date_short", |it| it.key);
                let at = slash.start.min(char_len(&blocks[i].text));
                blocks[i].text = insert_at_char(&blocks[i].text, at, &text);
                self.focus = Some((bid, Caret::At(at + char_len(&text))));
            }
            "template" => out.actions.push(Action::InsertTemplate { index: target, replace: empty }),
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
                let list_h = (h - 44.0).clamp(120.0, 560.0);
                egui::ScrollArea::vertical()
                    .max_height(list_h)
                    .min_scrolled_height(list_h.min(items.len() as f32 * 46.0))
                    .show(ui, |ui| {
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
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
                            // "turn into": the block's current type is marked
                            let current_kind =
                                slash.mode == SlashMode::TurnInto && kind_for(it.key).as_ref() == Some(&blocks[i].kind);
                            if current_kind {
                                ui.painter().rect_filled(r, CornerRadius::same(6), pal.accent_soft);
                                ui.painter().text(
                                    r.right_center() - Vec2::new(12.0, 0.0),
                                    Align2::RIGHT_CENTER,
                                    "\u{2713}",
                                    FontId::proportional(14.0),
                                    pal.accent,
                                );
                            }
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
                                    RichText::new(slash_desc(env.lang, it.key)).size(11.5).color(pal.text_secondary),
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
                                super::richpaste::copy_blocks(ctx, &blocks[a..b]);
                                acted = true;
                            }
                            "cut" => {
                                let (a, b) = self.multi_range(blocks, i).unwrap_or((i, end));
                                super::richpaste::copy_blocks(ctx, &blocks[a..b]);
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
                                    if kind_for(it.key).as_ref() == Some(&blocks[i].kind) {
                                        ui.painter().rect_filled(r, CornerRadius::same(6), pal.accent_soft);
                                        ui.painter().text(
                                            r.right_center() - Vec2::new(10.0, 0.0),
                                            Align2::RIGHT_CENTER,
                                            "\u{2713}",
                                            FontId::proportional(14.0),
                                            pal.accent,
                                        );
                                    }
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
                        if !bg {
                            // words coloured one by one would otherwise keep their own colour
                            b.text = notecraft_core::inline::strip_colors(&b.text);
                        }
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
