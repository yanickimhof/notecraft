//! Chart view and timeline menu.

use super::*;

pub(super) fn chart_color(pal: &Palette, i: usize) -> egui::Color32 {
    pal.option_colors(CHART_COLORS[i % CHART_COLORS.len()]).1
}

/// Settings of a chart view: style, X axis, and what is measured.
pub(super) fn chart_menu(
    ui: &mut Ui,
    ws: &mut Workspace,
    db: &str,
    view: &View,
    pal: &Palette,
    lang: Lang,
    actions: &mut Vec<DbAction>,
) {
    use notecraft_core::database::{ChartFunc, ChartStyle};
    let schema = ws.dbs[db].schema.clone();
    let btn = ui.add(egui::Button::new(RichText::new(tr(lang, "view_chart")).size(13.5).color(pal.text_secondary)));
    egui::Popup::menu(&btn).id(Id::unique(("chart-menu", &view.id))).show(|ui| {
        ui.set_min_width(240.0);
        ui.label(RichText::new(tr(lang, "chart_style")).size(12.0).color(pal.text_faint));
        for (s, key) in
            [(ChartStyle::Bar, "chart_bar"), (ChartStyle::Line, "chart_line"), (ChartStyle::Donut, "chart_donut")]
        {
            if ui.selectable_label(view.chart.style == s, tr(lang, key)).clicked() {
                edit_view(ws, db, &view.id, actions, |vv| vv.chart.style = s);
            }
        }
        ui.separator();
        ui.label(RichText::new(tr(lang, "chart_xaxis")).size(12.0).color(pal.text_faint));
        let x = database::chart_x(&schema, view).map(|p| p.id.clone());
        for p in schema
            .properties
            .iter()
            .filter(|p| p.kind.groupable() || matches!(p.kind, PropKind::Date | PropKind::Text | PropKind::Title))
        {
            if ui
                .selectable_label(x.as_deref() == Some(p.id.as_str()), format!("{}  {}", kind_glyph(p.kind), p.name))
                .clicked()
            {
                let id = p.id.clone();
                edit_view(ws, db, &view.id, actions, |vv| vv.group_by = Some(id));
            }
        }
        ui.separator();
        ui.label(RichText::new(tr(lang, "chart_value")).size(12.0).color(pal.text_faint));
        if ui.selectable_label(view.chart.func == ChartFunc::Count, tr(lang, "chart_count")).clicked() {
            edit_view(ws, db, &view.id, actions, |vv| vv.chart.func = ChartFunc::Count);
        }
        for (f, key) in [(ChartFunc::Sum, "chart_sum"), (ChartFunc::Average, "chart_avg")] {
            for p in schema
                .properties
                .iter()
                .filter(|p| matches!(p.kind, PropKind::Number | PropKind::Formula | PropKind::Rollup))
            {
                let on = view.chart.func == f && view.chart.value.as_deref() == Some(p.id.as_str());
                if ui.selectable_label(on, format!("{} {}", tr(lang, key), p.name)).clicked() {
                    let id = p.id.clone();
                    edit_view(ws, db, &view.id, actions, |vv| {
                        vv.chart.func = f;
                        vv.chart.value = Some(id);
                    });
                }
            }
        }
    });
}

pub(super) fn short_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

/// A bar, line or donut chart of the rows of a database.
pub(super) fn chart(ui: &mut Ui, ws: &Workspace, db: &str, view: &View, pal: &Palette, lang: Lang) {
    use notecraft_core::database::ChartStyle;
    let data: Vec<(String, f64)> = database::chart_data(&ws.dbs[db], view)
        .into_iter()
        .map(|(k, v)| (if k.is_empty() { tr(lang, "chart_none").to_string() } else { k }, v))
        .collect();
    let max = data.iter().map(|d| d.1).fold(0.0_f64, f64::max);
    if data.is_empty() || max <= 0.0 && data.iter().all(|d| d.1 == 0.0) {
        ui.add_space(24.0);
        ui.label(RichText::new(tr(lang, "chart_empty")).color(pal.text_faint));
        return;
    }
    let width = ui.available_width().min(960.0);
    let fmt = database::format_number;
    let faint = FontId::proportional(11.5);
    match view.chart.style {
        ChartStyle::Donut => {
            let h = 260.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
            let total: f64 = data.iter().map(|d| d.1.max(0.0)).sum();
            if total <= 0.0 {
                return;
            }
            let r = (h * 0.42).min(width * 0.25);
            let c = egui::pos2(rect.min.x + r + 16.0, rect.center().y);
            let mut a0 = -std::f32::consts::FRAC_PI_2;
            let mut mesh = egui::epaint::Mesh::default();
            for (i, (_, v)) in data.iter().enumerate() {
                let span = (v.max(0.0) / total) as f32 * std::f32::consts::TAU;
                if span <= 0.0 {
                    continue;
                }
                let col = chart_color(pal, i);
                let steps = ((span / 0.05).ceil() as usize).max(1);
                for s in 0..steps {
                    let (x0, x1) = (a0 + span * s as f32 / steps as f32, a0 + span * (s + 1) as f32 / steps as f32);
                    let base = mesh.vertices.len() as u32;
                    for p in [c, c + Vec2::angled(x0) * r, c + Vec2::angled(x1) * r] {
                        mesh.vertices.push(egui::epaint::Vertex { pos: p, uv: egui::epaint::WHITE_UV, color: col });
                    }
                    mesh.indices.extend([base, base + 1, base + 2]);
                }
                a0 += span;
            }
            ui.painter().add(egui::Shape::mesh(mesh));
            ui.painter().circle_filled(c, r * 0.58, pal.bg);
            ui.painter().text(c, Align2::CENTER_CENTER, fmt(total), FontId::proportional(20.0), pal.text);
            let mut y = rect.min.y + 20.0;
            let x = c.x + r + 40.0;
            for (i, (label, v)) in data.iter().enumerate() {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), Vec2::splat(12.0)),
                    CornerRadius::same(3),
                    chart_color(pal, i),
                );
                ui.painter().text(
                    egui::pos2(x + 20.0, y + 6.0),
                    Align2::LEFT_CENTER,
                    format!("{}   {}  ({:.0}%)", short_label(label, 28), fmt(*v), v / total * 100.0),
                    FontId::proportional(13.0),
                    pal.text,
                );
                y += 24.0;
            }
        }
        style => {
            let h = 280.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
            let plot = egui::Rect::from_min_max(
                egui::pos2(rect.min.x + 44.0, rect.min.y + 18.0),
                egui::pos2(rect.max.x - 8.0, rect.max.y - 34.0),
            );
            let top = nice_ceiling(max);
            for k in 0..=4 {
                let y = plot.max.y - plot.height() * k as f32 / 4.0;
                ui.painter().hline(plot.x_range(), y, Stroke::new(1.0, pal.border));
                ui.painter().text(
                    egui::pos2(plot.min.x - 8.0, y),
                    Align2::RIGHT_CENTER,
                    fmt(top * k as f64 / 4.0),
                    faint.clone(),
                    pal.text_faint,
                );
            }
            let n = data.len();
            let slot = plot.width() / n as f32;
            let y_of = |v: f64| plot.max.y - plot.height() * (v / top).clamp(0.0, 1.0) as f32;
            let mut line: Vec<egui::Pos2> = Vec::new();
            for (i, (label, v)) in data.iter().enumerate() {
                let cx = plot.min.x + slot * (i as f32 + 0.5);
                let col = if style == ChartStyle::Line { chart_color(pal, 0) } else { chart_color(pal, i) };
                let hit = egui::Rect::from_min_max(
                    egui::pos2(cx - slot / 2.0, plot.min.y),
                    egui::pos2(cx + slot / 2.0, rect.max.y),
                );
                if style == ChartStyle::Bar {
                    let bw = (slot * 0.6).min(56.0);
                    let bar = egui::Rect::from_min_max(
                        egui::pos2(cx - bw / 2.0, y_of(*v)),
                        egui::pos2(cx + bw / 2.0, plot.max.y),
                    );
                    ui.painter().rect_filled(bar, CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, col);
                } else {
                    line.push(egui::pos2(cx, y_of(*v)));
                }
                ui.painter().text(
                    egui::pos2(cx, y_of(*v) - 6.0),
                    Align2::CENTER_BOTTOM,
                    fmt(*v),
                    FontId::proportional(12.0),
                    pal.text_secondary,
                );
                ui.painter().text(
                    egui::pos2(cx, plot.max.y + 8.0),
                    Align2::CENTER_TOP,
                    short_label(label, ((slot / 7.0) as usize).max(4)),
                    FontId::proportional(12.0),
                    pal.text_secondary,
                );
                ui.interact(hit, ui.scope_id().with(("chart-hit", i)), Sense::hover())
                    .on_hover_text(format!("{label}: {}", fmt(*v)));
            }
            if style == ChartStyle::Line {
                let col = chart_color(pal, 0);
                ui.painter().add(egui::Shape::line(line.clone(), Stroke::new(2.0, col)));
                for p in line {
                    ui.painter().circle_filled(p, 4.0, col);
                    ui.painter().circle_filled(p, 2.0, pal.bg);
                }
            }
        }
    }
}

/// A round upper bound for an axis (1, 2, 5, 10, 20, 50, ...).
pub(super) fn nice_ceiling(max: f64) -> f64 {
    if max <= 0.0 {
        return 1.0;
    }
    let p = 10f64.powf(max.log10().floor());
    for m in [1.0, 2.0, 4.0, 5.0, 8.0, 10.0] {
        if max <= m * p {
            return m * p;
        }
    }
    10.0 * p
}

/// Start and end date properties of a timeline view.
pub(super) fn timeline_menu(
    ui: &mut Ui,
    ws: &mut Workspace,
    db: &str,
    view: &View,
    pal: &Palette,
    lang: Lang,
    actions: &mut Vec<DbAction>,
) {
    let schema = ws.dbs[db].schema.clone();
    let start = database::date_prop(&schema, view).map(|p| p.id.clone());
    let end = database::end_prop(&schema, view).map(|p| p.id.clone());
    let btn = ui.add(egui::Button::new(RichText::new(tr(lang, "view_timeline")).size(13.5).color(pal.text_secondary)));
    egui::Popup::menu(&btn).id(Id::unique(("timeline-menu", &view.id))).show(|ui| {
        ui.set_min_width(220.0);
        ui.label(RichText::new(tr(lang, "timeline_start")).size(12.0).color(pal.text_faint));
        for p in schema.properties.iter().filter(|p| p.kind == PropKind::Date) {
            if ui
                .selectable_label(
                    start.as_deref() == Some(p.id.as_str()),
                    format!("{}  {}", kind_glyph(p.kind), p.name),
                )
                .clicked()
            {
                let id = p.id.clone();
                edit_view(ws, db, &view.id, actions, |vv| vv.group_by = Some(id));
            }
        }
        ui.separator();
        ui.label(RichText::new(tr(lang, "timeline_end")).size(12.0).color(pal.text_faint));
        if ui.selectable_label(end.is_none(), tr(lang, "timeline_no_end")).clicked() {
            edit_view(ws, db, &view.id, actions, |vv| vv.end_by = None);
        }
        for p in schema.properties.iter().filter(|p| p.kind == PropKind::Date && Some(&p.id) != start.as_ref()) {
            if ui
                .selectable_label(end.as_deref() == Some(p.id.as_str()), format!("{}  {}", kind_glyph(p.kind), p.name))
                .clicked()
            {
                let id = p.id.clone();
                edit_view(ws, db, &view.id, actions, |vv| vv.end_by = Some(id));
            }
        }
    });
}
