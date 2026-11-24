//! Small reusable widgets: emoji picker, option chips, subtle buttons.

use crate::theme::{Palette, EMOJI};
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, Ui, Vec2};

/// A flat button that only shows a background when hovered.
pub fn subtle_button(ui: &mut Ui, text: impl Into<egui::WidgetText>, pal: &Palette) -> egui::Response {
    let b = egui::Button::new(text).stroke(Stroke::NONE);
    let r = ui.add(b);
    let _ = pal;
    r
}

/// A small rounded square with a glyph inside (slash-menu icons).
pub fn glyph_box(ui: &mut Ui, glyph: &str, pal: &Palette, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(6), pal.bg, Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(size * 0.42),
        pal.text_secondary,
    );
}

/// A coloured option chip (select, status, tags). Returns its response.
pub fn chip(ui: &mut Ui, pal: &Palette, name: &str, color: &str) -> egui::Response {
    let (bg, fg) = pal.option_colors(color);
    let text = RichText::new(name).size(13.0).color(fg);
    let b = egui::Button::new(text)
        .fill(bg)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(4))
        .min_size(Vec2::new(0.0, 20.0));
    ui.add(b)
}

/// Status chips show a dot (the colour's accent token) before the name.
pub fn status_chip(ui: &mut Ui, pal: &Palette, name: &str, color: &str) -> egui::Response {
    let (bg, fg, dot) = pal.option_tokens(color);
    let text = RichText::new(format!("     {name}")).size(13.0).color(fg);
    let b = egui::Button::new(text)
        .fill(bg)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(10))
        .min_size(Vec2::new(0.0, 20.0));
    let r = ui.add(b);
    ui.painter().circle_filled(r.rect.left_center() + egui::vec2(12.0, 0.0), 3.5, dot);
    r
}

/// A pill that is only drawn (no click of its own), so a click on it reaches
/// the cell around it and opens the cell's dropdown.
pub fn pill(ui: &mut Ui, pal: &Palette, name: &str, color: &str, status: bool) -> egui::Response {
    let (bg, fg, dot) = pal.option_tokens(color);
    let font = egui::FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(name.to_string(), font, fg);
    let pad_left = if status { 22.0 } else { 7.0 };
    let size = Vec2::new(galley.size().x + pad_left + 8.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let radius = if status { 10 } else { 4 };
    ui.painter().rect_filled(rect, CornerRadius::same(radius), bg);
    if status {
        ui.painter().circle_filled(rect.left_center() + egui::vec2(11.0, 0.0), 3.5, dot);
    }
    ui.painter().galley(egui::pos2(rect.min.x + pad_left, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Label, true, name));
    resp
}

/// A small glyph button with a tooltip and a spoken name.
pub fn icon_button(ui: &mut Ui, glyph: &str, label: &str) -> egui::Response {
    let r = ui.small_button(glyph).on_hover_text(label);
    a11y(&r, label);
    r
}

/// Give a drop-down a name for screen readers: its current choice is read as the value, this says what it is for.
pub fn name_combo<R>(r: egui::InnerResponse<Option<R>>, label: &str) -> egui::InnerResponse<Option<R>> {
    let enabled = r.response.enabled();
    r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, label));
    r
}

/// Give an icon-only control a name screen readers (VoiceOver) can say.
pub fn a11y(resp: &egui::Response, label: &str) {
    let enabled = resp.enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, label));
}

/// Grid of emoji; returns the chosen one. `allow_remove` adds a remove button.
pub fn emoji_grid(ui: &mut Ui, pal: &Palette, remove_label: &str, allow_remove: bool) -> Option<String> {
    let mut picked = None;
    ui.set_max_width(8.0 * 34.0);
    if allow_remove {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if subtle_button(ui, RichText::new(remove_label).color(pal.text_secondary), pal).clicked() {
                    picked = Some(String::new());
                }
            });
        });
    }
    egui::Grid::new("emoji-grid").spacing(Vec2::splat(2.0)).show(ui, |ui| {
        for (i, e) in EMOJI.iter().enumerate() {
            let b = egui::Button::new(RichText::new(*e).size(20.0)).min_size(Vec2::splat(32.0));
            if ui.add(b).clicked() {
                picked = Some(e.to_string());
            }
            if i % 8 == 7 {
                ui.end_row();
            }
        }
    });
    picked
}

/// Draw a checkbox the way the editor does (rounded, accent when checked).
pub fn paint_checkbox(ui: &Ui, rect: egui::Rect, checked: bool, hovered: bool, pal: &Palette) {
    let p = ui.painter();
    let r = egui::Rect::from_center_size(rect.center(), Vec2::splat(15.0));
    if checked {
        p.rect_filled(r, CornerRadius::same(3), pal.accent);
        let a = r.left_center() + egui::vec2(3.5, 0.5);
        let b = r.center_bottom() + egui::vec2(-1.0, -4.0);
        let c = r.right_top() + egui::vec2(-3.5, 4.0);
        p.line_segment([a, b], Stroke::new(1.8, Color32::WHITE));
        p.line_segment([b, c], Stroke::new(1.8, Color32::WHITE));
    } else {
        let fill = if hovered { pal.hover } else { Color32::TRANSPARENT };
        p.rect(r, CornerRadius::same(3), fill, Stroke::new(1.4, pal.text_secondary), egui::StrokeKind::Inside);
    }
}

/// Relative time such as "2 min ago" (en) / "il y a 2 min" (fr).
pub fn relative_time(rfc3339: &str, lang: crate::i18n::Lang) -> String {
    let Ok(t) = rfc3339.parse::<jiff::Timestamp>() else {
        return String::new();
    };
    let secs = (jiff::Timestamp::now().as_second() - t.as_second()).max(0);
    let fr = lang == crate::i18n::Lang::Fr;
    let (n, unit_en, unit_fr) = if secs < 60 {
        return if fr { "à l'instant".into() } else { "just now".into() };
    } else if secs < 3600 {
        (secs / 60, "min", "min")
    } else if secs < 86_400 {
        (secs / 3600, "h", "h")
    } else {
        return t.to_zoned(jiff::tz::TimeZone::UTC).strftime("%Y-%m-%d").to_string();
    };
    if fr {
        format!("il y a {n} {unit_fr}")
    } else {
        format!("{n} {unit_en} ago")
    }
}

/// A rounded panel that groups related settings.
pub fn card(ui: &mut Ui, pal: &Palette, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, pal.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// One line of a card: a title and an optional hint on the left, the control on the right.
/// Both sides are centred on the same line.
pub fn setting_row(ui: &mut Ui, pal: &Palette, title: &str, hint: &str, control: impl FnOnce(&mut Ui)) {
    let total = ui.available_width();
    ui.allocate_ui_with_layout(Vec2::new(total, 0.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(30.0);
        ui.scope(|ui| {
            ui.set_max_width(total * 0.56);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(title).size(13.0).color(pal.text));
                if !hint.is_empty() {
                    ui.add(egui::Label::new(RichText::new(hint).size(12.0).color(pal.text_faint)).wrap());
                }
            });
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

/// A thin separator between two rows of a card.
pub fn row_divider(ui: &mut Ui, pal: &Palette) {
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, pal.border);
    ui.add_space(6.0);
}

/// A segmented control: one choice among a few, all visible. Returns true when it changed.
pub fn segmented<T: PartialEq + Clone>(ui: &mut Ui, pal: &Palette, value: &mut T, options: &[(T, String)]) -> bool {
    let mut changed = false;
    egui::Frame::NONE.fill(pal.hover).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(2)).show(
        ui,
        |ui| {
            // inside a right-aligned row the buttons would come out reversed
            let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
            let ordered: Vec<&(T, String)> =
                if rtl { options.iter().rev().collect() } else { options.iter().collect() };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (v, label) in ordered {
                    let selected = value == v;
                    let color = if selected { pal.text } else { pal.text_secondary };
                    let mut b = egui::Button::new(RichText::new(label).size(13.0).color(color))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(0.0, 26.0));
                    b = if selected {
                        b.fill(pal.bg).stroke(Stroke::new(1.0, pal.border))
                    } else {
                        b.stroke(Stroke::NONE)
                    };
                    if ui.add(b).clicked() && !selected {
                        *value = v.clone();
                        changed = true;
                    }
                }
            });
        },
    );
    changed
}

/// A small status pill (ok, warning, neutral) with a coloured dot.
pub fn status_pill(ui: &mut Ui, pal: &Palette, text: &str, color: &str) {
    pill(ui, pal, text, color, true);
}

/// Small explanatory text under a control (one size, one colour everywhere in the settings).
pub fn hint(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(pal.text_faint)).wrap());
}

/// The small heading above a card.
pub fn section(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.0).strong().color(pal.text_secondary));
    ui.add_space(4.0);
}

/// A full width choice with a radio dot, a title and one line of explanation.
pub fn choice_row(ui: &mut Ui, pal: &Palette, selected: bool, title: &str, desc: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::click());
    let (fill, stroke) = if selected {
        (pal.accent_soft, Stroke::new(1.2, pal.accent))
    } else if resp.hovered() {
        (pal.hover, Stroke::new(1.0, pal.border))
    } else {
        (Color32::TRANSPARENT, Stroke::new(1.0, pal.border))
    };
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(8), fill, stroke, egui::StrokeKind::Inside);
    let dot = egui::pos2(rect.min.x + 20.0, rect.center().y);
    if selected {
        p.circle_filled(dot, 7.0, pal.accent);
        p.circle_filled(dot, 2.8, Color32::WHITE);
    } else {
        p.circle_stroke(dot, 6.5, Stroke::new(1.4, pal.text_faint));
    }
    p.text(
        egui::pos2(rect.min.x + 40.0, rect.min.y + 18.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(13.0),
        pal.text,
    );
    p.text(
        egui::pos2(rect.min.x + 40.0, rect.min.y + 36.0),
        egui::Align2::LEFT_CENTER,
        desc,
        egui::FontId::proportional(12.0),
        pal.text_faint,
    );
    resp.widget_info(|| egui::WidgetInfo::selected(egui::accesskit::Role::RadioButton, true, selected, title));
    resp
}
