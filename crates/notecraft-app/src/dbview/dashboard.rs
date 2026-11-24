//! Dashboard view: totals, sums and distributions of the rows that pass the view's filters.

use super::*;

pub(super) fn dashboard(ui: &mut Ui, ws: &Workspace, db: &str, view: &View, pal: &Palette, lang: Lang) {
    let data = database::dashboard_data(&ws.dbs[db], view);
    let fmt = database::format_number;
    let width = ui.available_width().min(960.0);
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(width);
        let card = |ui: &mut Ui, title: &str, big: String, lines: Vec<String>| {
            egui::Frame::new()
                .stroke(Stroke::new(1.0, pal.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(190.0);
                        ui.label(RichText::new(title).size(12.0).color(pal.text_faint));
                        ui.label(RichText::new(big).size(26.0).strong().color(pal.text));
                        for l in lines {
                            ui.label(RichText::new(l).size(12.0).color(pal.text_secondary));
                        }
                    });
                });
        };
        card(ui, tr(lang, "dash_rows"), data.total.to_string(), vec![]);
        for n in &data.numbers {
            card(
                ui,
                &format!("{} · {}", n.prop, tr(lang, "dash_total")),
                fmt(n.sum),
                vec![
                    format!("{} {}", tr(lang, "dash_average"), fmt((n.average() * 100.0).round() / 100.0)),
                    format!("{} {}   {} {}", tr(lang, "dash_min"), fmt(n.min), tr(lang, "dash_max"), fmt(n.max)),
                ],
            );
        }
        for (name, ticked) in &data.checks {
            card(
                ui,
                name,
                format!("{ticked} / {}", data.total),
                vec![
                    format!("{} {}", tr(lang, "dash_ticked"), (ticked * 100).checked_div(data.total).unwrap_or(0))
                        + " %",
                ],
            );
        }
    });
    for d in &data.distributions {
        ui.add_space(18.0);
        ui.label(RichText::new(&d.prop).size(13.0).strong().color(pal.text));
        ui.add_space(4.0);
        let max = d.counts.iter().map(|c| c.1).max().unwrap_or(1).max(1) as f32;
        let schema_prop = ws.dbs[db].schema.prop_by_name(&d.prop);
        for (i, (label, n)) in d.counts.iter().enumerate() {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 20.0), Sense::hover());
                ui.painter().text(
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(13.0),
                    pal.text,
                );
                let color = schema_prop
                    .and_then(|p| p.option(label))
                    .map(|o| pal.option_colors(&o.color).1)
                    .unwrap_or_else(|| chart_color(pal, i));
                let bar_w = (width - 230.0).max(80.0) * (*n as f32 / max);
                let (bar, _) = ui.allocate_exact_size(Vec2::new(bar_w.max(3.0), 14.0), Sense::hover());
                ui.painter().rect_filled(bar, CornerRadius::same(3), color);
                ui.label(RichText::new(n.to_string()).size(12.5).color(pal.text_secondary));
            });
        }
    }
}
