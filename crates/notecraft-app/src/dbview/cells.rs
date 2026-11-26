//! Cell editing and value helpers for database views.

use super::*;

pub(super) fn group_value(p: &Property, key: Option<String>) -> PropValue {
    match (p.kind, key) {
        (_, None) => PropValue::Empty,
        (PropKind::Checkbox, Some(k)) => PropValue::Checkbox(k == "true"),
        (PropKind::MultiSelect | PropKind::Person, Some(k)) => PropValue::Multi(vec![k]),
        (PropKind::Date, Some(k)) => PropValue::Date(k),
        (_, Some(k)) => PropValue::Select(k),
    }
}

pub(super) fn col_width(view: &View, p: &Property) -> f32 {
    view.widths.get(&p.id).copied().unwrap_or(match p.kind {
        PropKind::Title => 260.0,
        PropKind::Checkbox => 90.0,
        PropKind::Number => 110.0,
        PropKind::Date | PropKind::CreatedTime | PropKind::EditedTime => 160.0,
        _ => 180.0,
    })
}

/// Id of the text field of a cell, when the cell is a text field the keyboard can move between.
pub(super) fn text_cell_id(db: &str, rid: &str, p: &Property) -> Option<Id> {
    match p.kind {
        PropKind::Title => Some(Id::unique(("title-cell", db, rid))),
        PropKind::Text | PropKind::Url | PropKind::Email | PropKind::Phone | PropKind::Number => {
            Some(Id::unique(("cell", db, rid, &p.id)))
        }
        _ => None,
    }
}

/// Remember where the caret sits in a text cell (first or last line, start or end of the text), so that
/// the arrow keys can leave the cell only at its edges. Read back by the table on the next frame.
pub(super) fn note_caret_edges(ui: &mut Ui, id: Id, o: &egui::text_edit::TextEditOutput) {
    if !o.response.has_focus() {
        return;
    }
    let Some(r) = o.cursor_range else { return };
    let here = o.galley.pos_from_cursor(r.primary);
    let first = here.min.y <= 1.0;
    let last = here.max.y >= o.galley.rect.max.y - 1.0;
    let collapsed = r.primary.index.0 == r.secondary.index.0;
    let start = collapsed && r.primary.index.0 == 0;
    let end = collapsed && r.primary.index.0 >= o.galley.text().chars().count();
    ui.data_mut(|d| d.insert_temp(Id::unique("nc-db-edge"), (id, first, last, start, end)));
}

pub(super) fn hline(ui: &mut Ui, w: f32, pal: &Palette) {
    let c = ui.cursor().min;
    ui.painter().hline(egui::Rangef::new(c.x, c.x + w), c.y, Stroke::new(1.0, pal.border));
}

pub(super) fn group_label(ui: &mut Ui, prop: Option<&Property>, key: Option<&str>, pal: &Palette, lang: Lang) {
    let Some(p) = prop else { return };
    match key {
        None => {
            ui.label(
                RichText::new(format!("{} {}", tr(lang, "no_value"), p.name)).size(13.5).color(pal.text_secondary),
            );
        }
        Some(k) if p.kind == PropKind::Checkbox => {
            ui.label(
                RichText::new(format!("{}  {}", if k == "true" { "☑" } else { "☐" }, p.name))
                    .size(13.5)
                    .color(pal.text),
            );
        }
        Some(k) => {
            let color = p.option(k).map(|o| o.color.clone()).unwrap_or_else(|| "default".into());
            if p.kind == PropKind::Status {
                status_chip(ui, pal, k, &color);
            } else {
                chip(ui, pal, k, &color);
            }
        }
    }
}

/// Compact read-only value (board cards, list rows).
pub(super) fn show_value_readonly(ui: &mut Ui, ws: &Workspace, db: &str, rid: &str, p: &Property, pal: &Palette) {
    let row = &ws.dbs[db].rows[rid];
    match row.value(p) {
        PropValue::Select(s) if !s.is_empty() => {
            let color = p.option(&s).map(|o| o.color.clone()).unwrap_or_default();
            if p.kind == PropKind::Status {
                status_chip(ui, pal, &s, &color);
            } else {
                chip(ui, pal, &s, &color);
            }
        }
        PropValue::Multi(items) if p.kind.is_multi_choice() => {
            ui.horizontal_wrapped(|ui| {
                for s in items {
                    let color = p.option(&s).map(|o| o.color.clone()).unwrap_or_default();
                    chip(ui, pal, &s, &color);
                }
            });
        }
        PropValue::Checkbox(b) => {
            if b {
                ui.label(RichText::new(format!("☑ {}", p.name)).size(12.5).color(pal.text_secondary));
            }
        }
        v if !v.is_empty() => {
            let text = ws.display_value(db, row, p);
            let text = if p.kind.is_computed() { short_date(&text) } else { text };
            ui.label(RichText::new(text).size(12.5).color(pal.text_secondary));
        }
        _ => {}
    }
}

pub(super) fn short_date(s: &str) -> String {
    s.parse::<jiff::Timestamp>()
        .map(|t| t.to_zoned(jiff::tz::TimeZone::system()).strftime("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| s.to_string())
}

pub(super) fn set_row_value(
    ws: &mut Workspace,
    db: &str,
    rid: &str,
    p: &Property,
    v: PropValue,
    actions: &mut Vec<DbAction>,
) {
    if let Some(row) = ws.dbs.get_mut(db).and_then(|d| d.rows.get_mut(rid)) {
        row.set_value(p, v);
        row.touch();
    }
    if let Err(e) = ws.save_row(db, rid) {
        actions.push(DbAction::Error(e.to_string()));
    }
}

/// Editor for one property value of a row.
#[allow(clippy::too_many_arguments)]
pub fn cell_editor(
    ui: &mut Ui,
    ws: &mut Workspace,
    db: &str,
    rid: &str,
    p: &Property,
    w: f32,
    pal: &Palette,
    lang: Lang,
    actions: &mut Vec<DbAction>,
    new_option: &mut String,
) {
    let Some(row) = ws.dbs.get(db).and_then(|d| d.rows.get(rid)) else { return };
    let value = row.value(p);
    let id = Id::unique(("cell", db, rid, &p.id));
    match p.kind {
        PropKind::Title => {}
        PropKind::Formula | PropKind::Rollup => {
            let t = value.display();
            let (shown, col) = if t.is_empty() {
                ("\u{2014}".to_string(), pal.text_faint)
            } else if t.starts_with('\u{26a0}') {
                (t, pal.danger)
            } else {
                (t, pal.text)
            };
            ui.add(egui::Label::new(RichText::new(shown).color(col)).truncate());
        }
        PropKind::Text | PropKind::Url | PropKind::Email | PropKind::Phone | PropKind::Number => {
            let mut text = value.display();
            let link = matches!(p.kind, PropKind::Url | PropKind::Email) && !value.is_empty();
            let width = if link { w - 26.0 } else { w };
            let hint = RichText::new(tr(lang, "empty")).color(pal.text_faint);
            let r = if p.kind == PropKind::Text {
                // long text wraps onto several lines instead of being cut; Shift+Enter adds a line break
                let o = egui::TextEdit::multiline(&mut text)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .desired_width(width)
                    .desired_rows(1)
                    .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)))
                    .hint_text(hint)
                    .show(ui);
                note_caret_edges(ui, id, &o);
                o.response
            } else {
                let o = egui::TextEdit::singleline(&mut text)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .desired_width(width)
                    .hint_text(hint)
                    .show(ui);
                note_caret_edges(ui, id, &o);
                o.response
            };
            if r.changed() {
                let v = if p.kind == PropKind::Number {
                    match text.trim().replace(',', ".").parse::<f64>() {
                        Ok(n) => PropValue::Number(n),
                        Err(_) if text.trim().is_empty() => PropValue::Empty,
                        Err(_) => return,
                    }
                } else {
                    PropValue::Text(text)
                };
                set_row_value(ws, db, rid, p, v, actions);
            }
            if link && crate::widgets::icon_button(ui, "↗", tr(lang, "open")).clicked() {
                let target =
                    if p.kind == PropKind::Email { format!("mailto:{}", value.display()) } else { value.display() };
                crate::app::open_external(&target);
            }
        }
        PropKind::Checkbox => {
            let mut b = matches!(value, PropValue::Checkbox(true));
            if ui.checkbox(&mut b, "").changed() {
                set_row_value(ws, db, rid, p, PropValue::Checkbox(b), actions);
            }
        }
        PropKind::Date => {
            let current = match &value {
                PropValue::Date(s) => s.get(..10).and_then(|d| d.parse::<jiff::civil::Date>().ok()),
                _ => None,
            };
            match current {
                Some(mut d) => {
                    let before = d;
                    ui.add(
                        egui_extras::DatePickerButton::new(&mut d)
                            .id_salt(&format!("date-{db}-{rid}-{}", p.id))
                            .calendar_week(false),
                    );
                    if d != before {
                        set_row_value(ws, db, rid, p, PropValue::Date(d.to_string()), actions);
                    }
                    if ui.rect_contains_pointer(ui.max_rect())
                        && crate::widgets::icon_button(ui, "×", tr(lang, "clear")).clicked()
                    {
                        set_row_value(ws, db, rid, p, PropValue::Empty, actions);
                    }
                }
                None => {
                    if ui
                        .add(egui::Button::new(RichText::new(tr(lang, "pick_date")).size(13.0).color(pal.text_faint)))
                        .clicked()
                    {
                        let today = jiff::Zoned::now().date();
                        set_row_value(ws, db, rid, p, PropValue::Date(today.to_string()), actions);
                    }
                }
            }
        }
        PropKind::UniqueId => {
            let n = value.display();
            let text = if n.is_empty() { "—".to_string() } else { format!("#{n}") };
            ui.label(RichText::new(text).size(13.0).color(pal.text_secondary));
        }
        PropKind::Files => {
            let files: Vec<String> = match &value {
                PropValue::Multi(v) => v.clone(),
                _ => vec![],
            };
            let mut next = files.clone();
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (i, f) in files.iter().enumerate() {
                    let name = std::path::Path::new(f)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| f.clone());
                    let r = ui.add(egui::Button::new(RichText::new(format!("📎 {name}")).size(12.5).color(pal.text)));
                    if r.clicked() {
                        crate::app::open_external(&ws.root.join(f).display().to_string());
                    }
                    r.context_menu(|ui| {
                        if ui.button(tr(lang, "remove")).clicked() {
                            next.remove(i);
                            changed = true;
                            ui.close();
                        }
                    });
                }
                if crate::widgets::icon_button(ui, "+", tr(lang, "add_files")).clicked() {
                    if let Some(paths) = rfd::FileDialog::new().set_title(tr(lang, "add_files")).pick_files() {
                        for path in paths {
                            match ws.add_asset(&path) {
                                Ok(rel) => {
                                    next.push(rel);
                                    changed = true;
                                }
                                Err(e) => actions.push(DbAction::Error(e.to_string())),
                            }
                        }
                    }
                }
            });
            if changed {
                set_row_value(ws, db, rid, p, PropValue::Multi(next), actions);
            }
        }
        PropKind::CreatedTime | PropKind::EditedTime => {
            ui.label(RichText::new(short_date(&value.display())).size(13.0).color(pal.text_secondary));
        }
        PropKind::Select | PropKind::Status | PropKind::MultiSelect | PropKind::Person => {
            let selected: Vec<String> = match &value {
                PropValue::Select(s) if !s.is_empty() => vec![s.clone()],
                PropValue::Multi(v) => v.clone(),
                _ => vec![],
            };
            let r = ui
                .horizontal(|ui| {
                    ui.set_min_width(w.min(220.0));
                    if selected.is_empty() {
                        ui.add(
                            egui::Label::new(RichText::new(tr(lang, "empty")).size(13.0).color(pal.text_faint))
                                .selectable(false)
                                .sense(Sense::hover()),
                        );
                    }
                    // pills are drawn only: a click anywhere in the cell, pill included, opens the menu
                    for s in &selected {
                        let color = p.option(s).map(|o| o.color.clone()).unwrap_or_default();
                        crate::widgets::pill(ui, pal, s, &color, p.kind == PropKind::Status);
                    }
                })
                .response
                .interact(Sense::click());
            let label = format!(
                "{}: {}",
                p.name,
                if selected.is_empty() { tr(lang, "empty").to_string() } else { selected.join(", ") }
            );
            crate::widgets::a11y(&r, &label);
            egui::Popup::menu(&r)
                .id(id.with("menu"))
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_min_width(230.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(new_option).hint_text(tr(lang, "new_option")).desired_width(210.0),
                    );
                    let typed = new_option.trim().to_string();
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !typed.is_empty() {
                        if p.option(&typed).is_none() {
                            let color = COLORS[(p.options.len() + 1) % COLORS.len()].to_string();
                            if let Some(pp) =
                                ws.dbs.get_mut(db).and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id))
                            {
                                pp.options.push(SelectOption {
                                    name: typed.clone(),
                                    color,
                                    group: (p.kind == PropKind::Status).then_some(StatusGroup::ToDo),
                                });
                            }
                            save_schema(ws, db, actions);
                        }
                        if let Some(p2) = ws.dbs[db].schema.prop(&p.id).cloned() {
                            let v = toggle_option(&p2, &selected, &typed);
                            set_row_value(ws, db, rid, &p2, v, actions);
                        }
                        new_option.clear();
                    }
                    ui.separator();
                    // one list in the order the options were made: a new one comes last
                    let groups: Vec<Option<StatusGroup>> = vec![None];
                    for g in groups {
                        if let Some(g) = g {
                            let key = match g {
                                StatusGroup::ToDo => "group_to_do",
                                StatusGroup::InProgress => "group_in_progress",
                                StatusGroup::Complete => "group_complete",
                            };
                            ui.label(small(tr(lang, key), pal));
                        }
                        for o in
                            p.options.iter().filter(|o| g.is_none() || Some(o.group.unwrap_or(StatusGroup::ToDo)) == g)
                        {
                            if !typed.is_empty() && !o.name.to_lowercase().contains(&typed.to_lowercase()) {
                                continue;
                            }
                            ui.horizontal(|ui| {
                                let r = if p.kind == PropKind::Status {
                                    status_chip(ui, pal, &o.name, &o.color)
                                } else {
                                    chip(ui, pal, &o.name, &o.color)
                                };
                                if selected.contains(&o.name) {
                                    ui.label(RichText::new("✓").color(pal.accent));
                                }
                                if r.clicked() {
                                    let v = toggle_option(p, &selected, &o.name);
                                    set_row_value(ws, db, rid, p, v, actions);
                                    if !p.kind.is_multi_choice() {
                                        ui.close();
                                    }
                                }
                                // a small button next to the tag: rename, colour, group, delete
                                ui.menu_button("...", |ui| option_menu(ui, ws, db, p, &o.name, pal, lang, actions));
                            });
                        }
                    }
                });
        }
        PropKind::Relation => {
            let target_db = p.relation_db.clone().unwrap_or_else(|| db.to_string());
            let ids: Vec<String> = match &value {
                PropValue::Multi(v) => v.clone(),
                _ => vec![],
            };
            let label = ws.display_value(db, &ws.dbs[db].rows[rid], p);
            let text = if label.is_empty() { format!("↗ {}", tr(lang, "empty")) } else { format!("↗ {label}") };
            let r = ui.add(egui::Button::new(RichText::new(text).size(13.0).color(if label.is_empty() {
                pal.text_faint
            } else {
                pal.text
            })));
            egui::Popup::menu(&r)
                .id(id.with("rel"))
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_min_width(240.0);
                    let rows: Vec<(String, String)> = ws
                        .dbs
                        .get(&target_db)
                        .map(|d| {
                            d.live_rows()
                                .filter(|r| !(target_db == db && r.meta.id == rid))
                                .map(|r| (r.meta.id.clone(), r.meta.title.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (rel_id, title) in rows {
                            let mut on = ids.contains(&rel_id);
                            let label = if title.is_empty() { tr(lang, "untitled").to_string() } else { title };
                            if ui.checkbox(&mut on, label).changed() {
                                let mut v = ids.clone();
                                if on {
                                    v.push(rel_id.clone());
                                } else {
                                    v.retain(|x| x != &rel_id);
                                }
                                set_row_value(ws, db, rid, p, PropValue::Multi(v), actions);
                            }
                        }
                    });
                });
        }
    }
}

pub(super) fn toggle_option(p: &Property, selected: &[String], name: &str) -> PropValue {
    if p.kind.is_multi_choice() {
        let mut v = selected.to_vec();
        if let Some(i) = v.iter().position(|x| x == name) {
            v.remove(i);
        } else {
            v.push(name.to_string());
        }
        PropValue::Multi(v)
    } else if selected.first().map(|s| s.as_str()) == Some(name) {
        PropValue::Empty
    } else {
        PropValue::Select(name.to_string())
    }
}

/// Default values of a template (simple widgets; saved as the template file).
#[allow(clippy::too_many_arguments)]
pub(super) fn template_cell(
    ui: &mut Ui,
    ws: &mut Workspace,
    db: &str,
    tid: &str,
    p: &Property,
    pal: &Palette,
    lang: Lang,
    actions: &mut Vec<DbAction>,
) {
    let Some(t) = ws.dbs.get(db).and_then(|d| d.templates.get(tid)) else { return };
    let value = t.value(p);
    let mut new_value: Option<PropValue> = None;
    match p.kind {
        PropKind::Select | PropKind::Status | PropKind::MultiSelect | PropKind::Person => {
            let selected: Vec<String> = match &value {
                PropValue::Select(s) if !s.is_empty() => vec![s.clone()],
                PropValue::Multi(v) => v.clone(),
                _ => vec![],
            };
            ui.horizontal_wrapped(|ui| {
                for o in &p.options {
                    let on = selected.contains(&o.name);
                    let label = format!("{}{}", if on { "✓ " } else { "" }, o.name);
                    if chip(ui, pal, &label, if on { &o.color } else { "default" }).clicked() {
                        new_value = Some(toggle_option(p, &selected, &o.name));
                    }
                }
            });
        }
        PropKind::Checkbox => {
            let mut b = matches!(value, PropValue::Checkbox(true));
            if ui.checkbox(&mut b, "").changed() {
                new_value = Some(PropValue::Checkbox(b));
            }
        }
        PropKind::CreatedTime
        | PropKind::EditedTime
        | PropKind::Relation
        | PropKind::Title
        | PropKind::Formula
        | PropKind::Rollup => {
            ui.label(RichText::new("—").color(pal.text_faint));
        }
        _ => {
            let mut text = value.display();
            let hint = if p.kind == PropKind::Date { "YYYY-MM-DD" } else { tr(lang, "empty") };
            if ui
                .add(
                    egui::TextEdit::singleline(&mut text)
                        .frame(egui::Frame::NONE)
                        .hint_text(RichText::new(hint).color(pal.text_faint)),
                )
                .changed()
            {
                new_value = Some(match p.kind {
                    PropKind::Number => text.trim().parse::<f64>().map(PropValue::Number).unwrap_or(PropValue::Empty),
                    PropKind::Date => PropValue::Date(text),
                    _ => PropValue::Text(text),
                });
            }
        }
    }
    if let Some(v) = new_value {
        if let Some(t) = ws.dbs.get_mut(db).and_then(|d| d.templates.get_mut(tid)) {
            t.set_value(p, v);
        }
        if let Err(e) = ws.save_template(db, tid) {
            actions.push(DbAction::Error(e.to_string()));
        }
    }
}

/// Recolour, regroup and delete options of a select/status property.
pub(super) fn options_editor(
    ui: &mut Ui,
    ws: &mut Workspace,
    db: &str,
    pid: &str,
    pal: &Palette,
    lang: Lang,
    actions: &mut Vec<DbAction>,
) {
    let Some(p) = ws.dbs[db].schema.prop(pid).cloned() else { return };
    let mut opts = p.options.clone();
    let mut changed = false;
    let mut remove = None;
    let group_label = |g: StatusGroup| match g {
        StatusGroup::ToDo => tr(lang, "group_to_do"),
        StatusGroup::InProgress => tr(lang, "group_in_progress"),
        StatusGroup::Complete => tr(lang, "group_complete"),
    };
    for (k, o) in opts.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            chip(ui, pal, &o.name, &o.color);
            crate::widgets::name_combo(
                egui::ComboBox::from_id_salt(("opt-color", pid, k)).selected_text(o.color.clone()).width(90.0).show_ui(
                    ui,
                    |ui| {
                        for c in COLORS {
                            if ui.selectable_label(o.color == c, c).clicked() {
                                o.color = c.to_string();
                                changed = true;
                            }
                        }
                    },
                ),
                tr(lang, "combo_color"),
            );
            if p.kind == PropKind::Status {
                let g = o.group.unwrap_or(StatusGroup::ToDo);
                crate::widgets::name_combo(
                    egui::ComboBox::from_id_salt(("opt-group", pid, k))
                        .selected_text(group_label(g))
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            for gg in [StatusGroup::ToDo, StatusGroup::InProgress, StatusGroup::Complete] {
                                if ui.selectable_label(g == gg, group_label(gg)).clicked() {
                                    o.group = Some(gg);
                                    changed = true;
                                }
                            }
                        }),
                    tr(lang, "combo_status_group"),
                );
            }
            if crate::widgets::icon_button(ui, "×", tr(lang, "remove")).clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = remove {
        opts.remove(k);
        changed = true;
    }
    if changed {
        if let Some(pp) = ws.dbs.get_mut(db).and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == pid)) {
            pp.options = opts;
        }
        save_schema(ws, db, actions);
    }
}

/// Menus to pick what a rollup summarises: the relation, the property of the related rows, the calculation.
pub(super) fn rollup_menus(
    ui: &mut egui::Ui,
    ws: &mut Workspace,
    db: &str,
    p: &notecraft_core::database::Property,
    pal: &Palette,
    lang: Lang,
    actions: &mut Vec<DbAction>,
) {
    use notecraft_core::formula::{Rollup, RollupFunc};
    let cur = p.rollup.clone();
    let none = tr(lang, "rollup_none");
    let relations: Vec<(String, String, String)> = ws.dbs[db]
        .schema
        .properties
        .iter()
        .filter(|x| x.kind == PropKind::Relation)
        .map(|x| (x.id.clone(), x.name.clone(), x.relation_db.clone().unwrap_or_else(|| db.to_string())))
        .collect();
    let rel_name = cur
        .as_ref()
        .and_then(|c| relations.iter().find(|r| r.0 == c.relation))
        .map(|r| r.1.clone())
        .unwrap_or_else(|| none.to_string());
    let set = |ws: &mut Workspace, f: &dyn Fn(&mut Rollup), actions: &mut Vec<DbAction>| {
        if let Some(pp) = ws.dbs.get_mut(db).and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id)) {
            let r = pp.rollup.get_or_insert(Rollup {
                relation: String::new(),
                target: String::new(),
                func: RollupFunc::Count,
            });
            f(r);
        }
        save_schema(ws, db, actions);
    };
    ui.menu_button(format!("{}: {rel_name}", tr(lang, "rollup_relation")), |ui| {
        if relations.is_empty() {
            ui.label(RichText::new(tr(lang, "rollup_need_relation")).color(pal.text_faint));
        }
        for (id, name, _) in &relations {
            let on = cur.as_ref().is_some_and(|c| &c.relation == id);
            if ui.selectable_label(on, name).clicked() {
                let id = id.clone();
                set(
                    ws,
                    &|r| {
                        if r.relation != id {
                            r.relation = id.clone();
                            r.target.clear();
                        }
                    },
                    actions,
                );
                ui.close();
            }
        }
    });
    let target_db = cur.as_ref().and_then(|c| relations.iter().find(|r| r.0 == c.relation)).map(|r| r.2.clone());
    if let Some(tdb) = target_db {
        let props: Vec<(String, String)> = ws
            .dbs
            .get(&tdb)
            .map(|d| d.schema.properties.iter().map(|x| (x.id.clone(), x.name.clone())).collect())
            .unwrap_or_default();
        let shown = cur
            .as_ref()
            .and_then(|c| props.iter().find(|x| x.0 == c.target))
            .map(|x| x.1.clone())
            .unwrap_or_else(|| none.to_string());
        ui.menu_button(format!("{}: {shown}", tr(lang, "rollup_property")), |ui| {
            for (id, name) in &props {
                let on = cur.as_ref().is_some_and(|c| &c.target == id);
                if ui.selectable_label(on, name).clicked() {
                    let id = id.clone();
                    set(ws, &|r| r.target = id.clone(), actions);
                    ui.close();
                }
            }
        });
    }
    let func = cur.as_ref().map(|c| c.func).unwrap_or(RollupFunc::Count);
    ui.menu_button(format!("{}: {}", tr(lang, "rollup_calc"), tr(lang, &format!("rollup_{}", func.key()))), |ui| {
        for f in RollupFunc::ALL {
            if ui.selectable_label(f == func, tr(lang, &format!("rollup_{}", f.key()))).clicked() {
                set(ws, &|r| r.func = f, actions);
                ui.close();
            }
        }
    });
}

/// The small menu of one option: rename, colour, group (status) and delete. A rename or a delete is carried
/// to every row that holds the option.
#[allow(clippy::too_many_arguments)]
fn option_menu(
    ui: &mut Ui,
    ws: &mut Workspace,
    db: &str,
    p: &Property,
    name: &str,
    pal: &Palette,
    lang: Lang,
    actions: &mut Vec<DbAction>,
) {
    let Some(o) = p.option(name).cloned() else { return };
    ui.set_min_width(170.0);
    let buf_id = Id::unique(("opt-rename", &p.id, name));
    let mut text: String = ui.data_mut(|d| d.get_temp(buf_id)).unwrap_or_else(|| name.to_string());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(150.0));
    ui.data_mut(|d| d.insert_temp(buf_id, text.clone()));
    let new = text.trim().to_string();
    let renamed = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !new.is_empty() && new != name;
    if renamed && p.option(&new).is_none() {
        {
            if let Some(op) = ws
                .dbs
                .get_mut(db)
                .and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id))
                .and_then(|pp| pp.options.iter_mut().find(|x| x.name == name))
            {
                op.name = new.clone();
            }
            save_schema(ws, db, actions);
            carry_option(ws, db, p, name, Some(&new), actions);
            ui.data_mut(|d| d.remove_temp::<String>(buf_id));
            ui.close();
            return;
        }
    }
    ui.separator();
    ui.label(small(tr(lang, "combo_color"), pal));
    for c in COLORS {
        if ui.selectable_label(o.color == c, c).clicked() {
            if let Some(op) = ws
                .dbs
                .get_mut(db)
                .and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id))
                .and_then(|pp| pp.options.iter_mut().find(|x| x.name == name))
            {
                op.color = c.to_string();
            }
            save_schema(ws, db, actions);
            ui.close();
        }
    }
    if p.kind == PropKind::Status {
        ui.separator();
        ui.label(small(tr(lang, "combo_status_group"), pal));
        let cur = o.group.unwrap_or(StatusGroup::ToDo);
        for (g, key) in [
            (StatusGroup::ToDo, "group_to_do"),
            (StatusGroup::InProgress, "group_in_progress"),
            (StatusGroup::Complete, "group_complete"),
        ] {
            if ui.selectable_label(cur == g, tr(lang, key)).clicked() {
                if let Some(op) = ws
                    .dbs
                    .get_mut(db)
                    .and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id))
                    .and_then(|pp| pp.options.iter_mut().find(|x| x.name == name))
                {
                    op.group = Some(g);
                }
                save_schema(ws, db, actions);
                ui.close();
            }
        }
    }
    ui.separator();
    if ui.button(tr(lang, "remove")).clicked() {
        if let Some(pp) = ws.dbs.get_mut(db).and_then(|d| d.schema.properties.iter_mut().find(|x| x.id == p.id)) {
            pp.options.retain(|x| x.name != name);
        }
        save_schema(ws, db, actions);
        carry_option(ws, db, p, name, None, actions);
        ui.close();
    }
}

/// Rows holding the option `old` now hold `new`, or lose it when `new` is None.
fn carry_option(ws: &mut Workspace, db: &str, p: &Property, old: &str, new: Option<&str>, actions: &mut Vec<DbAction>) {
    let ids: Vec<String> = ws.dbs.get(db).map(|d| d.rows.keys().cloned().collect()).unwrap_or_default();
    for rid in ids {
        let v = ws.dbs[db].rows[&rid].value(p);
        let nv = match v {
            PropValue::Select(s) if s == old => {
                Some(new.map_or(PropValue::Empty, |n| PropValue::Select(n.to_string())))
            }
            PropValue::Multi(m) if m.iter().any(|x| x == old) => {
                let list: Vec<String> =
                    m.iter().filter_map(|x| if x == old { new.map(str::to_string) } else { Some(x.clone()) }).collect();
                Some(if list.is_empty() { PropValue::Empty } else { PropValue::Multi(list) })
            }
            _ => None,
        };
        if let Some(nv) = nv {
            set_row_value(ws, db, &rid, p, nv, actions);
        }
    }
}
