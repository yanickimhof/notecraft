//! Database pages: view tabs, toolbar (filter, sort, group, properties, new),
//! table / board / list views, cell editors and the property list shown on
//! top of a row page.

use crate::i18n::{tr, Lang};
use crate::theme::Palette;
use crate::widgets::{chip, status_chip};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, RichText, Sense, Stroke, Ui, Vec2};
use notecraft_core::database::{
    self, FilterMode, FilterOp, PropKind, PropValue, Property, SelectOption, Sort, StatusGroup, View, ViewKind, COLORS,
};
use notecraft_core::{DocId, Workspace};
use std::collections::HashMap;

mod cells;
mod dashboard;
#[cfg(test)]
mod ui_tests;
mod views;
pub use cells::*;
mod chart;
use chart::*;

#[derive(Default)]
pub struct DbUi {
    /// Active view per database.
    pub active: HashMap<String, String>,
    new_option: String,
    rename_buf: HashMap<String, String>,
    /// Month shown by each calendar view: (year, month).
    cal: HashMap<String, (i16, i8)>,
    /// Pixels per day of each timeline view.
    tl_zoom: HashMap<String, f32>,
    /// Row just added in a table: its title cell takes the caret.
    focus_row: Option<String>,
    /// Width of the column being dragged, until the mouse is released.
    col_drag: Option<(String, f32)>,
}

/// What the database UI asks the app to do.
pub enum DbAction {
    Open(DocId),
    EditTemplate(String, String),
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
struct CardDrag {
    row: String,
}

pub fn kind_glyph(k: PropKind) -> &'static str {
    match k {
        PropKind::Title => "Aa",
        PropKind::Text => "≡",
        PropKind::Number => "#",
        PropKind::Select => "◉",
        PropKind::MultiSelect => "☰",
        PropKind::Status => "◐",
        PropKind::Date => "📅",
        PropKind::Checkbox => "☑",
        PropKind::Url => "🔗",
        PropKind::Email => "@",
        PropKind::Phone => "☎",
        PropKind::CreatedTime => "🕒",
        PropKind::EditedTime => "🕘",
        PropKind::Relation => "↗",
        PropKind::Formula => "ƒ",
        PropKind::Rollup => "Σ",
        PropKind::Person => "👤",
        PropKind::Files => "📎",
        PropKind::UniqueId => "№",
    }
}

pub fn kind_name(lang: Lang, k: PropKind) -> &'static str {
    tr(lang, &format!("kind_{}", k.key()))
}

fn small(text: &str, pal: &Palette) -> RichText {
    RichText::new(text).size(13.0).color(pal.text_secondary)
}

fn save_schema(ws: &mut Workspace, db: &str, actions: &mut Vec<DbAction>) {
    if let Some(d) = ws.dbs.get_mut(db) {
        d.schema.updated = notecraft_core::now_string();
    }
    if let Err(e) = ws.save_schema(db) {
        actions.push(DbAction::Error(e.to_string()));
    }
}

/// Mutate one view of a database and save the schema.
fn edit_view(ws: &mut Workspace, db: &str, view: &str, actions: &mut Vec<DbAction>, f: impl FnOnce(&mut View)) {
    if let Some(v) = ws.dbs.get_mut(db).and_then(|d| d.schema.views.iter_mut().find(|v| v.id == view)) {
        f(v);
    }
    save_schema(ws, db, actions);
}

fn view_glyph(k: ViewKind) -> &'static str {
    match k {
        ViewKind::Table => "▦",
        ViewKind::Board => "▥",
        ViewKind::List => "☰",
        ViewKind::Gallery => "▤",
        ViewKind::Calendar => "▣",
        ViewKind::Chart => "◔",
        ViewKind::Timeline => "▤",
        ViewKind::Dashboard => "▧",
    }
}

const VIEW_KINDS: [(ViewKind, &str); 8] = [
    (ViewKind::Table, "view_table"),
    (ViewKind::Board, "view_board"),
    (ViewKind::List, "view_list"),
    (ViewKind::Gallery, "view_gallery"),
    (ViewKind::Calendar, "view_calendar"),
    (ViewKind::Chart, "view_chart"),
    (ViewKind::Timeline, "view_timeline"),
    (ViewKind::Dashboard, "view_dashboard"),
];

/// One group of filters: the AND/OR switch, its rules and its nested groups (up to three levels).
fn filter_group_ui(
    ui: &mut Ui,
    group: &mut database::FilterGroup,
    props: &[Property],
    lang: Lang,
    salt: &str,
    depth: usize,
    changed: &mut bool,
) {
    if group.rules.len() + group.groups.len() > 1 {
        ui.horizontal(|ui| {
            for (m, key) in [(FilterMode::And, "and"), (FilterMode::Or, "or")] {
                if ui.selectable_label(group.mode == m, tr(lang, key)).clicked() {
                    group.mode = m;
                    *changed = true;
                }
            }
        });
    }
    let mut remove = None;
    for (k, f) in group.rules.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let pname = props.iter().find(|p| p.id == f.prop).map(|p| p.name.clone()).unwrap_or_default();
            crate::widgets::name_combo(
                egui::ComboBox::from_id_salt((salt, "prop", k)).selected_text(pname).width(120.0).show_ui(ui, |ui| {
                    for p in props {
                        if ui.selectable_label(p.id == f.prop, &p.name).clicked() {
                            f.prop = p.id.clone();
                            f.op = FilterOp::for_kind(p.kind)[0];
                            f.value.clear();
                            *changed = true;
                        }
                    }
                }),
                tr(lang, "combo_property"),
            );
            let prop = props.iter().find(|p| p.id == f.prop);
            let kind = prop.map(|p| p.kind).unwrap_or(PropKind::Text);
            crate::widgets::name_combo(
                egui::ComboBox::from_id_salt((salt, "op", k))
                    .selected_text(tr(lang, &format!("op_{}", f.op.key())))
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for op in FilterOp::for_kind(kind) {
                            if ui.selectable_label(op == f.op, tr(lang, &format!("op_{}", op.key()))).clicked() {
                                f.op = op;
                                *changed = true;
                            }
                        }
                    }),
                tr(lang, "combo_operator"),
            );
            if f.op.needs_value() {
                let opts = prop.map(|p| p.options.clone()).unwrap_or_default();
                if !opts.is_empty() {
                    crate::widgets::name_combo(
                        egui::ComboBox::from_id_salt((salt, "val", k))
                            .selected_text(f.value.clone())
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for o in &opts {
                                    if ui.selectable_label(o.name == f.value, &o.name).clicked() {
                                        f.value = o.name.clone();
                                        *changed = true;
                                    }
                                }
                            }),
                        tr(lang, "combo_value"),
                    );
                } else if ui
                    .add(egui::TextEdit::singleline(&mut f.value).desired_width(110.0).hint_text(tr(lang, "value")))
                    .changed()
                {
                    *changed = true;
                }
            }
            if crate::widgets::icon_button(ui, "×", tr(lang, "remove")).clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = remove {
        group.rules.remove(k);
        *changed = true;
    }
    let mut remove_group = None;
    for (g, sub) in group.groups.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.vertical(|ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr(lang, "filter_group")).size(12.0));
                        if crate::widgets::icon_button(ui, "×", tr(lang, "remove")).clicked() {
                            remove_group = Some(g);
                        }
                    });
                    filter_group_ui(ui, sub, props, lang, &format!("{salt}.{g}"), depth + 1, changed);
                });
            });
        });
    }
    if let Some(g) = remove_group {
        group.groups.remove(g);
        *changed = true;
    }
    ui.horizontal(|ui| {
        if ui.button(format!("+  {}", tr(lang, "add_filter"))).clicked() {
            if let Some(p) = props.first() {
                group.rules.push(database::Filter {
                    prop: p.id.clone(),
                    op: FilterOp::for_kind(p.kind)[0],
                    value: String::new(),
                });
                *changed = true;
            }
        }
        if depth < 2 && ui.button(format!("+  {}", tr(lang, "add_filter_group"))).clicked() {
            let mut sub = database::FilterGroup::default();
            if let Some(p) = props.first() {
                sub.rules.push(database::Filter {
                    prop: p.id.clone(),
                    op: FilterOp::for_kind(p.kind)[0],
                    value: String::new(),
                });
            }
            group.groups.push(sub);
            *changed = true;
        }
    });
}

impl DbUi {
    fn active_view(&mut self, ws: &Workspace, db: &str) -> String {
        let d = &ws.dbs[db];
        match self.active.get(db).cloned().filter(|id| d.view(id).is_some()) {
            Some(id) => id,
            None => {
                let id = d.schema.views.first().map(|v| v.id.clone()).unwrap_or_default();
                self.active.insert(db.to_string(), id.clone());
                id
            }
        }
    }

    /// The body of a database page (below its title).
    pub fn show(&mut self, ui: &mut Ui, ws: &mut Workspace, db: &str, pal: &Palette, lang: Lang) -> Vec<DbAction> {
        let mut actions = Vec::new();
        if ws.dbs[db].schema.views.is_empty() {
            ws.dbs.get_mut(db).unwrap().schema.views.push(View::new(tr(lang, "view_table"), ViewKind::Table));
            save_schema(ws, db, &mut actions);
        }
        let view_id = self.active_view(ws, db);
        self.tabs_and_toolbar(ui, ws, db, &view_id, pal, lang, &mut actions);
        let r = ui.cursor();
        ui.painter().hline(
            egui::Rangef::new(r.min.x, r.min.x + ui.available_width()),
            r.min.y + 2.0,
            Stroke::new(1.0, pal.border),
        );
        ui.add_space(8.0);
        let Some(view) = ws.dbs[db].view(&view_id).cloned() else { return actions };
        match view.kind {
            ViewKind::Table => self.table(ui, ws, db, &view, pal, lang, &mut actions),
            ViewKind::Board => self.board(ui, ws, db, &view, pal, lang, &mut actions),
            ViewKind::List => self.list(ui, ws, db, &view, pal, lang, &mut actions),
            ViewKind::Gallery => self.gallery(ui, ws, db, &view, pal, lang, &mut actions),
            ViewKind::Calendar => self.calendar(ui, ws, db, &view, pal, lang, &mut actions),
            ViewKind::Chart => chart(ui, ws, db, &view, pal, lang),
            ViewKind::Timeline => self.timeline(ui, ws, db, &view, pal, lang, &mut actions),
            ViewKind::Dashboard => dashboard::dashboard(ui, ws, db, &view, pal, lang),
        }
        actions
    }

    #[allow(clippy::too_many_arguments)]
    fn tabs_and_toolbar(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view_id: &str,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let views = ws.dbs[db].schema.views.clone();
        ui.horizontal(|ui| {
            for v in &views {
                let active = v.id == view_id;
                let text = RichText::new(format!("{}  {}", view_glyph(v.kind), v.name)).size(14.0).color(if active {
                    pal.text
                } else {
                    pal.text_secondary
                });
                let r = ui.add(egui::Button::new(text));
                if active {
                    ui.painter().hline(r.rect.x_range(), r.rect.max.y + 4.0, Stroke::new(2.0, pal.text));
                }
                if r.clicked() {
                    self.active.insert(db.to_string(), v.id.clone());
                }
                r.context_menu(|ui| {
                    ui.label(small(tr(lang, "rename_view"), pal));
                    let buf = self.rename_buf.entry(format!("view:{}", v.id)).or_insert_with(|| v.name.clone());
                    let resp = ui.text_edit_singleline(buf);
                    if resp.changed() {
                        let name = buf.trim().to_string();
                        if !name.is_empty() {
                            edit_view(ws, db, &v.id, actions, |vv| vv.name = name);
                        }
                    }
                    ui.separator();
                    for (k, label) in VIEW_KINDS {
                        if ui.selectable_label(v.kind == k, format!("{}  {}", view_glyph(k), tr(lang, label))).clicked()
                        {
                            edit_view(ws, db, &v.id, actions, |vv| vv.kind = k);
                            ui.close();
                        }
                    }
                    if views.len() > 1 {
                        ui.separator();
                        if ui.button(RichText::new(tr(lang, "delete_view")).color(pal.danger)).clicked() {
                            if let Some(d) = ws.dbs.get_mut(db) {
                                d.schema.views.retain(|x| x.id != v.id);
                            }
                            save_schema(ws, db, actions);
                            ui.close();
                        }
                    }
                });
            }
            ui.menu_button(RichText::new("+").size(16.0).color(pal.text_secondary), |ui| {
                ui.label(small(tr(lang, "new_view"), pal));
                for (k, label) in VIEW_KINDS {
                    if ui.button(format!("{}  {}", view_glyph(k), tr(lang, label))).clicked() {
                        let v = View::new(tr(lang, label), k);
                        let id = v.id.clone();
                        ws.dbs.get_mut(db).unwrap().schema.views.push(v);
                        save_schema(ws, db, actions);
                        self.active.insert(db.to_string(), id);
                        ui.close();
                    }
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.new_button(ui, ws, db, view_id, pal, lang, actions);
                let Some(view) = ws.dbs[db].view(view_id).cloned() else { return };
                self.props_menu(ui, ws, db, &view, pal, lang, actions);
                self.group_menu(ui, ws, db, &view, pal, lang, actions);
                self.sort_menu(ui, ws, db, &view, pal, lang, actions);
                self.filter_menu(ui, ws, db, &view, pal, lang, actions);
            });
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn new_button(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view_id: &str,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let templates: Vec<(String, String)> =
            ws.dbs[db].templates.iter().map(|(id, t)| (id.clone(), t.meta.title.clone())).collect();
        let more = ui.add(
            egui::Button::new(RichText::new("▾").color(Color32::WHITE).size(13.0))
                .fill(pal.accent)
                .corner_radius(CornerRadius::same(6)),
        );
        crate::widgets::a11y(&more, tr(lang, "templates"));
        egui::Popup::menu(&more)
            .id(Id::unique(("new-menu", db)))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(240.0);
                ui.label(small(tr(lang, "templates"), pal));
                if ui.button(tr(lang, "empty_row")).clicked() {
                    self.add_row(ws, db, view_id, None, None, actions);
                    ui.close();
                }
                for (id, name) in &templates {
                    ui.horizontal(|ui| {
                        let label = if name.is_empty() { tr(lang, "untitled").to_string() } else { name.clone() };
                        if ui.button(format!("📋  {label}")).clicked() {
                            self.add_row(ws, db, view_id, Some(id.as_str()), None, actions);
                            ui.close();
                        }
                        if ui.small_button(tr(lang, "edit_template")).clicked() {
                            actions.push(DbAction::EditTemplate(db.to_string(), id.clone()));
                            ui.close();
                        }
                        if crate::widgets::icon_button(ui, "×", tr(lang, "remove")).clicked() {
                            let _ = ws.delete_template(db, id);
                        }
                    });
                }
                ui.separator();
                if ui.button(format!("+  {}", tr(lang, "new_template"))).clicked() {
                    match ws.add_template(db, tr(lang, "new_template")) {
                        Ok(id) => actions.push(DbAction::EditTemplate(db.to_string(), id)),
                        Err(e) => actions.push(DbAction::Error(e.to_string())),
                    }
                    ui.close();
                }
            });
        let b = egui::Button::new(RichText::new(tr(lang, "new_row")).color(Color32::WHITE).size(13.5))
            .fill(pal.accent)
            .corner_radius(CornerRadius::same(6));
        if ui.add(b).clicked() {
            self.add_row(ws, db, view_id, None, None, actions);
        }
    }

    /// Add a row; values from the clicked group and simple filters are pre-filled.
    fn add_row(
        &mut self,
        ws: &mut Workspace,
        db: &str,
        view_id: &str,
        template: Option<&str>,
        group: Option<(String, Option<String>)>,
        actions: &mut Vec<DbAction>,
    ) {
        let rid = match ws.add_row(db, template, "") {
            Ok(r) => r,
            Err(e) => {
                actions.push(DbAction::Error(e.to_string()));
                return;
            }
        };
        let view = ws.dbs[db].view(view_id).cloned();
        let props = ws.dbs[db].schema.properties.clone();
        if let Some(row) = ws.dbs.get_mut(db).and_then(|d| d.rows.get_mut(&rid)) {
            if let Some(v) = &view {
                for f in &v.filter.rules {
                    if let Some(p) = props.iter().find(|p| p.id == f.prop) {
                        let val = match (f.op, p.kind) {
                            (FilterOp::Is, PropKind::Select | PropKind::Status) => PropValue::Select(f.value.clone()),
                            (FilterOp::Contains, PropKind::MultiSelect | PropKind::Person) => {
                                PropValue::Multi(vec![f.value.clone()])
                            }
                            (FilterOp::Checked, PropKind::Checkbox) => PropValue::Checkbox(true),
                            _ => continue,
                        };
                        row.set_value(p, val);
                    }
                }
            }
            if let Some((pid, key)) = group {
                if let Some(p) = props.iter().find(|p| p.id == pid) {
                    row.set_value(p, group_value(p, key));
                }
            }
        }
        let _ = ws.save_row(db, &rid);
        if view.as_ref().is_some_and(|v| v.kind == ViewKind::Table) {
            // in a table the new row appears in place, with the caret in its first cell
            self.focus_row = Some(rid);
        } else {
            actions.push(DbAction::Open(DocId::Row(db.to_string(), rid)));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn filter_menu(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let n = view.filter.count();
        let label = if n > 0 { format!("{} · {n}", tr(lang, "filter")) } else { tr(lang, "filter").to_string() };
        let color = if n > 0 { pal.accent } else { pal.text_secondary };
        let btn = ui.add(egui::Button::new(RichText::new(label).size(13.5).color(color)));
        egui::Popup::menu(&btn)
            .id(Id::unique(("filter-menu", &view.id)))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(440.0);
                let props = ws.dbs[db].schema.properties.clone();
                let mut v = view.clone();
                let mut changed = false;
                filter_group_ui(ui, &mut v.filter, &props, lang, "f", 0, &mut changed);
                if changed {
                    let filter = v.filter.clone();
                    edit_view(ws, db, &view.id, actions, |vv| vv.filter = filter);
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn sort_menu(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let n = view.sorts.len();
        let label = if n > 0 { format!("{} · {n}", tr(lang, "sort")) } else { tr(lang, "sort").to_string() };
        let color = if n > 0 { pal.accent } else { pal.text_secondary };
        let btn = ui.add(egui::Button::new(RichText::new(label).size(13.5).color(color)));
        egui::Popup::menu(&btn)
            .id(Id::unique(("sort-menu", &view.id)))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(320.0);
                let props = ws.dbs[db].schema.properties.clone();
                let mut sorts = view.sorts.clone();
                let mut changed = false;
                let mut remove = None;
                for (k, s) in sorts.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let pname = props.iter().find(|p| p.id == s.prop).map(|p| p.name.clone()).unwrap_or_default();
                        crate::widgets::name_combo(
                            egui::ComboBox::from_id_salt(("sort-prop", k)).selected_text(pname).width(140.0).show_ui(
                                ui,
                                |ui| {
                                    for p in &props {
                                        if ui.selectable_label(p.id == s.prop, &p.name).clicked() {
                                            s.prop = p.id.clone();
                                            changed = true;
                                        }
                                    }
                                },
                            ),
                            tr(lang, "combo_property"),
                        );
                        let dir = if s.descending { tr(lang, "descending") } else { tr(lang, "ascending") };
                        crate::widgets::name_combo(
                            egui::ComboBox::from_id_salt(("sort-dir", k)).selected_text(dir).width(110.0).show_ui(
                                ui,
                                |ui| {
                                    if ui.selectable_label(!s.descending, tr(lang, "ascending")).clicked() {
                                        s.descending = false;
                                        changed = true;
                                    }
                                    if ui.selectable_label(s.descending, tr(lang, "descending")).clicked() {
                                        s.descending = true;
                                        changed = true;
                                    }
                                },
                            ),
                            tr(lang, "combo_sort_dir"),
                        );
                        if crate::widgets::icon_button(ui, "×", tr(lang, "remove")).clicked() {
                            remove = Some(k);
                        }
                    });
                }
                if let Some(k) = remove {
                    sorts.remove(k);
                    changed = true;
                }
                if ui.button(format!("+  {}", tr(lang, "add_sort"))).clicked() {
                    if let Some(p) = props.iter().find(|p| !sorts.iter().any(|s| s.prop == p.id)) {
                        sorts.push(Sort { prop: p.id.clone(), descending: false });
                        changed = true;
                    }
                }
                if changed {
                    edit_view(ws, db, &view.id, actions, |vv| vv.sorts = sorts.clone());
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn group_menu(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let props = ws.dbs[db].schema.properties.clone();
        let current = database::group_prop(&ws.dbs[db].schema, view).map(|p| p.id.clone());
        let color = if view.group_by.is_some() { pal.accent } else { pal.text_secondary };
        if matches!(view.kind, ViewKind::Gallery | ViewKind::Dashboard) {
            return;
        }
        if view.kind == ViewKind::Chart {
            chart_menu(ui, ws, db, view, pal, lang, actions);
            return;
        }
        if view.kind == ViewKind::Timeline {
            timeline_menu(ui, ws, db, view, pal, lang, actions);
            return;
        }
        if view.kind == ViewKind::Calendar {
            let current = database::date_prop(&ws.dbs[db].schema, view).map(|p| p.id.clone());
            let btn = ui
                .add(egui::Button::new(RichText::new(tr(lang, "date_property")).size(13.5).color(pal.text_secondary)));
            egui::Popup::menu(&btn).id(Id::unique(("date-menu", &view.id))).show(|ui| {
                ui.set_min_width(200.0);
                for p in props.iter().filter(|p| p.kind == PropKind::Date) {
                    if ui
                        .selectable_label(
                            current.as_deref() == Some(p.id.as_str()),
                            format!("{}  {}", kind_glyph(p.kind), p.name),
                        )
                        .clicked()
                    {
                        let id = p.id.clone();
                        edit_view(ws, db, &view.id, actions, |vv| vv.group_by = Some(id));
                    }
                }
            });
            return;
        }
        let btn = ui.add(egui::Button::new(RichText::new(tr(lang, "group")).size(13.5).color(color)));
        egui::Popup::menu(&btn).id(Id::unique(("group-menu", &view.id))).show(|ui| {
            ui.set_min_width(200.0);
            if view.kind != ViewKind::Board && ui.selectable_label(current.is_none(), tr(lang, "no_grouping")).clicked()
            {
                edit_view(ws, db, &view.id, actions, |vv| vv.group_by = None);
            }
            for p in props.iter().filter(|p| p.kind.groupable()) {
                if ui
                    .selectable_label(
                        current.as_deref() == Some(p.id.as_str()),
                        format!("{}  {}", kind_glyph(p.kind), p.name),
                    )
                    .clicked()
                {
                    let id = p.id.clone();
                    edit_view(ws, db, &view.id, actions, |vv| vv.group_by = Some(id));
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn props_menu(
        &mut self,
        ui: &mut Ui,
        ws: &mut Workspace,
        db: &str,
        view: &View,
        pal: &Palette,
        lang: Lang,
        actions: &mut Vec<DbAction>,
    ) {
        let btn = ui.add(egui::Button::new(RichText::new(tr(lang, "properties")).size(13.5).color(pal.text_secondary)));
        egui::Popup::menu(&btn)
            .id(Id::unique(("props-menu", &view.id)))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(260.0);
                let props = ws.dbs[db].schema.properties.clone();
                let mut visible: Vec<String> = if view.visible.is_empty() {
                    props.iter().map(|p| p.id.clone()).collect()
                } else {
                    view.visible.clone()
                };
                let mut changed = false;
                let mut order: Vec<Property> =
                    visible.iter().filter_map(|id| props.iter().find(|p| &p.id == id).cloned()).collect();
                order.extend(props.iter().filter(|p| !visible.contains(&p.id)).cloned());
                for p in &order {
                    ui.horizontal(|ui| {
                        let mut on = visible.contains(&p.id);
                        let enabled = p.kind != PropKind::Title;
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Checkbox::new(&mut on, format!("{}  {}", kind_glyph(p.kind), p.name)),
                            )
                            .changed()
                        {
                            if on {
                                visible.push(p.id.clone());
                            } else {
                                visible.retain(|x| x != &p.id);
                            }
                            changed = true;
                        }
                        if let Some(pos) = visible.iter().position(|x| x == &p.id) {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if pos + 1 < visible.len()
                                    && crate::widgets::icon_button(ui, "↓", tr(lang, "move_down")).clicked()
                                {
                                    visible.swap(pos, pos + 1);
                                    changed = true;
                                }
                                if pos > 0 && crate::widgets::icon_button(ui, "↑", tr(lang, "move_up")).clicked() {
                                    visible.swap(pos, pos - 1);
                                    changed = true;
                                }
                            });
                        }
                    });
                }
                if ui.button(tr(lang, "show_all")).clicked() {
                    visible = props.iter().map(|p| p.id.clone()).collect();
                    changed = true;
                }
                if changed {
                    edit_view(ws, db, &view.id, actions, |vv| vv.visible = visible.clone());
                }
            });
    }
}

const CHART_COLORS: [&str; 8] = ["blue", "green", "orange", "purple", "pink", "yellow", "red", "brown"];
