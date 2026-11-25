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
    let _ = pal;
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(2, 2)).show(ui, |ui| {
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

/// A word that acts as a button: accent colour, soft background under the pointer.
pub fn link(ui: &mut Ui, pal: &Palette, text: &str) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let w = ui.painter().layout_no_wrap(text.to_string(), font.clone(), pal.accent).size().x + 16.0;
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), pal.hover);
    }
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font, pal.accent);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An on/off switch, black on grey (white on grey in the dark theme).
pub fn toggle(ui: &mut Ui, pal: &Palette, on: &mut bool) -> egui::Response {
    let (rect, mut r) = ui.allocate_exact_size(Vec2::new(36.0, 22.0), Sense::click());
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(r.id, *on);
    let off = pal.border.lerp_to_gamma(pal.text_faint, 0.5);
    ui.painter().rect_filled(rect, CornerRadius::same(11), off.lerp_to_gamma(pal.text, t));
    let knob = if pal.dark { Color32::WHITE.lerp_to_gamma(pal.bg, t) } else { Color32::WHITE };
    let cx = rect.min.x + 11.0 + t * 14.0;
    ui.painter().circle_filled(egui::pos2(cx, rect.center().y), 9.0, knob);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, true, *on, ""));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
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

/// An icon followed by an underlined title: the underline stops at the text, so icons of
/// different widths do not break the alignment of a list of links.
pub fn icon_title(icon: &str, title: &str, size: f32, color: egui::Color32) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let font = egui::FontId::proportional(size);
    let mut job = LayoutJob::default();
    job.append(&format!("{icon}  "), 0.0, TextFormat { font_id: font.clone(), color, ..Default::default() });
    job.append(
        title,
        0.0,
        TextFormat { font_id: font, color, underline: egui::Stroke::new(1.0, color), ..Default::default() },
    );
    job
}

// ------------------------------------------------------------------ settings page

/// A small drawn glyph for the cycling controls.
#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Sun,
    Moon,
    Half,
    /// The letter A at the given size (text size choices).
    Letter(f32),
    /// Two letters in a thin frame (language codes).
    Code(&'static str),
    Laptop,
    Sparkle,
    /// Three crossing loops, after the OpenAI mark.
    OpenAi,
    /// A circle cut by a slash, after the Grok mark.
    Grok,
    Key,
    /// A cloud outline (publishing).
    Cloud,
    Mic,
    Speaker,
    Both,
    Trash,
    Download,
    Check,
    None,
}

fn paint_glyph(p: &egui::Painter, c: egui::Pos2, g: Glyph, color: Color32) {
    use std::f32::consts::{PI, TAU};
    let stroke = Stroke::new(1.4, color);
    match g {
        Glyph::Sun => {
            p.circle_stroke(c, 3.6, stroke);
            for i in 0..8 {
                let (s, co) = (i as f32 * PI / 4.0).sin_cos();
                p.line_segment([c + Vec2::new(co, s) * 6.0, c + Vec2::new(co, s) * 8.0], stroke);
            }
        }
        Glyph::Half => {
            p.circle_stroke(c, 7.0, stroke);
            let pts: Vec<egui::Pos2> =
                (0..=16).map(|i| c + Vec2::angled(PI / 2.0 + PI * i as f32 / 16.0) * 7.0).collect();
            p.add(egui::epaint::PathShape::convex_polygon(pts, color, Stroke::NONE));
        }
        Glyph::Moon => {
            let (r_out, r_in) = (7.0_f32, 6.0_f32);
            let d = Vec2::angled(-PI / 4.0) * 3.6;
            let c_in = c + d;
            let phi = d.angle();
            let outer: Vec<egui::Pos2> = (0..=48)
                .map(|i| c + Vec2::angled(phi + TAU * i as f32 / 48.0) * r_out)
                .filter(|q| (*q - c_in).length() > r_in)
                .collect();
            let mut inner: Vec<egui::Pos2> = (0..=48)
                .map(|i| c_in + Vec2::angled(phi + PI - PI + TAU * i as f32 / 48.0) * r_in)
                .filter(|q| (*q - c).length() < r_out)
                .collect();
            if let (Some(last), Some(a), Some(b)) = (outer.last(), inner.first(), inner.last()) {
                if (*b - *last).length() < (*a - *last).length() {
                    inner.reverse();
                }
            }
            let mut pts = outer;
            pts.extend(inner);
            p.add(egui::epaint::PathShape::closed_line(pts, stroke));
        }
        Glyph::Letter(size) => {
            p.text(c, egui::Align2::CENTER_CENTER, "A", egui::FontId::proportional(size), color);
        }
        Glyph::Code(t) => {
            let r = egui::Rect::from_center_size(c, Vec2::new(8.0 + 6.5 * t.chars().count() as f32, 15.0));
            p.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.2, color), egui::StrokeKind::Inside);
            p.text(c, egui::Align2::CENTER_CENTER, t, egui::FontId::proportional(10.5), color);
        }
        Glyph::Laptop => {
            let r = egui::Rect::from_center_size(c + Vec2::new(0.0, -1.0), Vec2::new(14.0, 9.5));
            p.rect_stroke(r, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
            p.line_segment([c + Vec2::new(-8.0, 6.0), c + Vec2::new(8.0, 6.0)], stroke);
        }
        Glyph::Sparkle => {
            let pts: Vec<egui::Pos2> = (0..8)
                .map(|i| {
                    let r = if i % 2 == 0 { 8.0 } else { 2.4 };
                    c + Vec2::angled(-PI / 2.0 + i as f32 * PI / 4.0) * r
                })
                .collect();
            p.add(egui::epaint::PathShape::closed_line(pts, stroke));
        }
        Glyph::OpenAi => {
            for k in 0..3 {
                let a = k as f32 * PI / 3.0;
                let (sa, ca) = a.sin_cos();
                let pts: Vec<egui::Pos2> = (0..36)
                    .map(|i| {
                        let t = TAU * i as f32 / 36.0;
                        let (x, y) = (7.6 * t.cos(), 3.2 * t.sin());
                        c + Vec2::new(x * ca - y * sa, x * sa + y * ca)
                    })
                    .collect();
                p.add(egui::epaint::PathShape::closed_line(pts, stroke));
            }
        }
        Glyph::Grok => {
            p.circle_stroke(c + Vec2::new(-1.0, 1.0), 5.2, stroke);
            p.line_segment([c + Vec2::new(-5.5, 5.5), c + Vec2::new(7.0, -7.0)], stroke);
        }
        Glyph::Key => {
            p.circle_stroke(c + Vec2::new(-4.0, 0.0), 3.4, stroke);
            p.line_segment([c + Vec2::new(-0.6, 0.0), c + Vec2::new(7.0, 0.0)], stroke);
            p.line_segment([c + Vec2::new(4.0, 0.0), c + Vec2::new(4.0, 3.0)], stroke);
            p.line_segment([c + Vec2::new(7.0, 0.0), c + Vec2::new(7.0, 2.6)], stroke);
        }
        Glyph::Cloud => {
            let mut pts: Vec<egui::Pos2> = Vec::new();
            let arc = |pts: &mut Vec<egui::Pos2>, ctr: Vec2, r: f32, a0: f32, a1: f32| {
                for i in 0..=10 {
                    let a = a0 + (a1 - a0) * i as f32 / 10.0;
                    pts.push(c + ctr + Vec2::angled(a) * r);
                }
            };
            arc(&mut pts, Vec2::new(-5.0, 1.8), 3.2, PI / 2.0, PI * 1.5);
            arc(&mut pts, Vec2::new(-0.6, -0.8), 4.6, PI * 1.12, PI * 1.9);
            arc(&mut pts, Vec2::new(5.0, 1.8), 3.2, -PI / 2.0, PI / 2.0);
            p.add(egui::epaint::PathShape::closed_line(pts, stroke));
        }
        Glyph::Mic => paint_mic(p, c, 1.0, color),
        Glyph::Speaker => paint_speaker(p, c, 1.0, color),
        Glyph::Both => {
            paint_mic(p, c + Vec2::new(-5.0, 0.0), 0.75, color);
            paint_speaker(p, c + Vec2::new(5.0, 0.0), 0.75, color);
        }
        Glyph::Trash => {
            p.line_segment([c + Vec2::new(-6.0, -3.5), c + Vec2::new(6.0, -3.5)], stroke);
            p.line_segment([c + Vec2::new(-2.0, -3.5), c + Vec2::new(-2.0, -6.0)], stroke);
            p.line_segment([c + Vec2::new(-2.0, -6.0), c + Vec2::new(2.0, -6.0)], stroke);
            p.line_segment([c + Vec2::new(2.0, -6.0), c + Vec2::new(2.0, -3.5)], stroke);
            let body = egui::Rect::from_min_max(c + Vec2::new(-4.5, -3.5), c + Vec2::new(4.5, 6.5));
            p.rect_stroke(body, CornerRadius::same(1), stroke, egui::StrokeKind::Inside);
        }
        Glyph::Download => {
            p.line_segment([c + Vec2::new(0.0, -6.0), c + Vec2::new(0.0, 3.0)], stroke);
            p.line_segment([c + Vec2::new(-3.5, -0.5), c + Vec2::new(0.0, 3.0)], stroke);
            p.line_segment([c + Vec2::new(0.0, 3.0), c + Vec2::new(3.5, -0.5)], stroke);
            p.line_segment([c + Vec2::new(-6.0, 7.0), c + Vec2::new(6.0, 7.0)], stroke);
        }
        Glyph::Check => {
            let s2 = Stroke::new(1.8, color);
            p.line_segment([c + Vec2::new(-5.0, 0.5), c + Vec2::new(-1.5, 4.0)], s2);
            p.line_segment([c + Vec2::new(-1.5, 4.0), c + Vec2::new(5.5, -4.0)], s2);
        }
        Glyph::None => {}
    }
}

fn paint_mic(p: &egui::Painter, c: egui::Pos2, k: f32, color: Color32) {
    use std::f32::consts::PI;
    let s = Stroke::new(1.4, color);
    let body = egui::Rect::from_center_size(c + Vec2::new(0.0, -2.0 * k), Vec2::new(5.0 * k, 9.0 * k));
    p.rect_stroke(body, CornerRadius::same((2.5 * k) as u8), s, egui::StrokeKind::Inside);
    let pts: Vec<egui::Pos2> =
        (0..=12).map(|i| c + Vec2::new(0.0, 0.5 * k) + Vec2::angled(PI * i as f32 / 12.0) * 4.8 * k).collect();
    p.add(egui::epaint::PathShape::line(pts, s));
    p.line_segment([c + Vec2::new(0.0, 5.3 * k), c + Vec2::new(0.0, 7.5 * k)], s);
}

fn paint_speaker(p: &egui::Painter, c: egui::Pos2, k: f32, color: Color32) {
    let s = Stroke::new(1.4, color);
    let pts: Vec<egui::Pos2> = [(-5.0, -2.5), (-2.0, -2.5), (2.0, -5.5), (2.0, 5.5), (-2.0, 2.5), (-5.0, 2.5)]
        .iter()
        .map(|(x, y)| c + Vec2::new(*x * k, *y * k))
        .collect();
    p.add(egui::epaint::PathShape::closed_line(pts, s));
    for r in [3.6_f32, 6.4] {
        let arc: Vec<egui::Pos2> =
            (0..=8).map(|i| c + Vec2::new(2.0 * k, 0.0) + Vec2::angled(-0.9 + 1.8 * i as f32 / 8.0) * r * k).collect();
        p.add(egui::epaint::PathShape::line(arc, s));
    }
}

fn chevron_right(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(1.3, color);
    p.line_segment([c + Vec2::new(-1.5, -3.5), c + Vec2::new(2.0, 0.0)], s);
    p.line_segment([c + Vec2::new(2.0, 0.0), c + Vec2::new(-1.5, 3.5)], s);
}

/// One line of the settings page: title (and a small grey line under it) on the left, a control on
/// the right, a soft background under the pointer.
pub fn srow(
    ui: &mut Ui,
    pal: &Palette,
    title: &str,
    sub: &str,
    sub_color: Option<Color32>,
    right: impl FnOnce(&mut Ui),
) {
    srow_tip(ui, pal, title, sub, sub_color, "", right);
}

/// Like [`srow`], with a longer explanation shown when the pointer rests on the line.
pub fn srow_tip(
    ui: &mut Ui,
    pal: &Palette,
    title: &str,
    sub: &str,
    sub_color: Option<Color32>,
    tip: &str,
    right: impl FnOnce(&mut Ui),
) {
    let h = if sub.is_empty() { 40.0 } else { 50.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
    if !tip.is_empty() {
        resp.on_hover_text(tip);
    }
    if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.hover);
    }
    let inner = rect.shrink2(Vec2::new(10.0, 0.0));
    let left = egui::Rect::from_min_size(inner.min, Vec2::new(inner.width() * 0.62, inner.height()));
    let p = ui.painter().with_clip_rect(left);
    let (ty, sy) = (rect.center().y - if sub.is_empty() { 0.0 } else { 8.0 }, rect.center().y + 9.0);
    p.text(egui::pos2(inner.min.x, ty), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(13.0), pal.text);
    if !sub.is_empty() {
        p.text(
            egui::pos2(inner.min.x, sy),
            egui::Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(12.0),
            sub_color.unwrap_or(pal.text_faint),
        );
    }
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
    child.set_clip_rect(rect);
    right(&mut child);
}

/// A line that opens a page of its own: title, a grey value, a small arrow.
pub fn nav_row(ui: &mut Ui, pal: &Palette, title: &str, value: &str) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.hover);
    }
    let font = egui::FontId::proportional(13.0);
    ui.painter().text(
        rect.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        font.clone(),
        pal.text,
    );
    ui.painter().text(
        rect.right_center() - Vec2::new(28.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        value,
        font,
        pal.text_secondary,
    );
    chevron_right(ui.painter(), rect.right_center() - Vec2::new(16.0, 0.0), pal.text_faint);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A control that steps to the next choice at every click, showing a small drawn icon and the
/// current choice in grey. Returns true when the choice changed.
pub fn cycle<T: PartialEq + Clone>(
    ui: &mut Ui,
    pal: &Palette,
    value: &mut T,
    options: &[(T, Glyph, String)],
    show_label: bool,
) -> bool {
    let idx = options.iter().position(|o| o.0 == *value).unwrap_or(0);
    let (_, glyph, label) = &options[idx];
    let font = egui::FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(label.clone(), font, pal.text_secondary);
    let gw = if *glyph == Glyph::None { 0.0 } else { 28.0 };
    let tw = if show_label { galley.size().x } else { 0.0 };
    let w = 10.0 + gw + tw + 10.0 + if glyph_wide(glyph) { 6.0 } else { 0.0 };
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.active);
    }
    let color = if r.hovered() { pal.text } else { pal.text_secondary };
    if *glyph != Glyph::None {
        paint_glyph(ui.painter(), egui::pos2(rect.min.x + 10.0 + 12.0, rect.center().y), *glyph, color);
    }
    if show_label {
        let pos = egui::pos2(rect.min.x + 10.0 + gw, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, color);
    }
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label.as_str()));
    let mut r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    if !show_label {
        r = r.on_hover_text(label.as_str());
    }
    if r.clicked() {
        *value = options[(idx + 1) % options.len()].0.clone();
        return true;
    }
    false
}

fn glyph_wide(g: &Glyph) -> bool {
    matches!(g, Glyph::Code(t) if t.chars().count() > 2)
}

/// A small icon button that is either chosen or not (one of several, side by side).
pub fn icon_choice(ui: &mut Ui, pal: &Palette, glyph: Glyph, selected: bool, label: &str) -> egui::Response {
    let w = if glyph_wide(&glyph) { 44.0 } else { 34.0 };
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.active);
    } else if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.hover);
    }
    let color = if selected || r.hovered() { pal.text } else { pal.text_faint };
    paint_glyph(ui.painter(), rect.center(), glyph, color);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, label));
    r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(label)
}

/// A small icon button that does something (delete, download, key).
pub fn icon_action(ui: &mut Ui, pal: &Palette, glyph: Glyph, label: &str, tint: Option<Color32>) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(34.0, 30.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.active);
    }
    let color = tint.unwrap_or(if r.hovered() { pal.text } else { pal.text_secondary });
    paint_glyph(ui.painter(), rect.center(), glyph, color);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(label)
}

/// One keyboard shortcut: what it does on the left, the keys in small frames on the right.
pub fn shortcut_row(ui: &mut Ui, pal: &Palette, keys: &str, desc: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
    if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.hover);
    }
    let font = egui::FontId::proportional(12.0);
    // tokens: chips, "/" separators, and a trailing note in brackets
    let mut parts: Vec<(String, bool)> = Vec::new();
    let mut note = String::new();
    for t in keys.split_whitespace() {
        if !note.is_empty() || t.starts_with('(') {
            if !note.is_empty() {
                note.push(' ');
            }
            note.push_str(t);
        } else if t == "/" {
            parts.push(("/".into(), false));
        } else {
            parts.push((t.to_string(), true));
        }
    }
    if !note.is_empty() {
        parts.push((note, false));
    }
    let items: Vec<(std::sync::Arc<egui::Galley>, bool)> =
        parts.iter().map(|(t, chip)| (ui.painter().layout_no_wrap(t.clone(), font.clone(), pal.text), *chip)).collect();
    let mut x = rect.max.x - 10.0;
    for (g, chip) in items.iter().rev() {
        let w = g.size().x + if *chip { 12.0 } else { 4.0 };
        let r = egui::Rect::from_min_size(egui::pos2(x - w, rect.center().y - 11.0), Vec2::new(w, 22.0));
        if *chip {
            ui.painter().rect_filled(r, CornerRadius::same(5), pal.active);
        }
        let c = if *chip { pal.text } else { pal.text_faint };
        ui.painter().galley(egui::pos2(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g.clone(), c);
        x -= w + 4.0;
    }
    let width = (x - rect.min.x - 20.0).max(60.0);
    let mut job = egui::text::LayoutJob::simple(desc.to_string(), egui::FontId::proportional(13.0), pal.text, width);
    job.wrap.max_rows = 1;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, pal.text);
}

/// A grey word that acts as a button (no accent colour).
pub fn text_link(ui: &mut Ui, pal: &Palette, text: &str) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, pal.text_secondary);
    let (rect, r) = ui.allocate_exact_size(Vec2::new(galley.size().x + 20.0, 30.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.active);
    }
    let color = if r.hovered() { pal.text } else { pal.text_secondary };
    ui.painter().galley(egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, color);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One entry of a small menu: text, a check mark on the current one, grey background under the pointer.
pub fn menu_item(ui: &mut Ui, pal: &Palette, text: &str, checked: bool) -> egui::Response {
    let w = ui.available_width().max(200.0);
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), pal.active);
    }
    ui.painter().text(
        rect.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(13.0),
        pal.text,
    );
    if checked {
        let c = rect.right_center() - Vec2::new(16.0, 0.0);
        let s = Stroke::new(1.5, pal.text);
        ui.painter().line_segment([c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.5, 3.0)], s);
        ui.painter().line_segment([c + Vec2::new(-1.5, 3.0), c + Vec2::new(4.0, -3.5)], s);
    }
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, checked, text));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The small grey heading inside a menu or above a group of lines.
pub fn menu_label(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).size(11.0).color(pal.text_faint));
}

/// The model of the AI line: its name on a quiet pill; a click opens the list, the current one ticked.
/// Returns the model picked.
pub fn model_menu(ui: &mut Ui, pal: &Palette, current: &str, models: &[String]) -> Option<String> {
    let shown = if current.chars().count() > 26 {
        format!("{}…", current.chars().take(25).collect::<String>())
    } else {
        current.to_string()
    };
    let galley = ui.painter().layout_no_wrap(shown, egui::FontId::proportional(13.0), pal.text_secondary);
    let (rect, r) = ui.allocate_exact_size(Vec2::new(galley.size().x + 10.0 + 8.0 + 22.0, 30.0), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.active);
    }
    let color = if r.hovered() { pal.text } else { pal.text_secondary };
    ui.painter().galley(egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley.clone(), color);
    let c = egui::pos2(rect.min.x + 10.0 + galley.size().x + 12.0, rect.center().y + 0.5);
    let s = Stroke::new(1.3, pal.text_faint);
    ui.painter().line_segment([c + Vec2::new(-3.5, -1.5), c + Vec2::new(0.0, 2.0)], s);
    ui.painter().line_segment([c + Vec2::new(0.0, 2.0), c + Vec2::new(3.5, -1.5)], s);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, current));
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut picked = None;
    egui::Popup::menu(&r).id(egui::Id::unique("ai-model-menu")).show(|ui| {
        ui.set_min_width(250.0);
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            let mut last = "";
            // Never hide the model in use, even if it is an older one.
            let mut list = notecraft_ai::current_models(models);
            if !current.is_empty() && !list.iter().any(|m| m == current) {
                list.push(current.to_string());
                list = notecraft_ai::sort_models(&list);
            }
            for m in &list {
                let label = notecraft_ai::model_tier(m).1;
                if label != last {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(label.to_uppercase()).size(10.5).color(pal.text_faint));
                    last = label;
                }
                if menu_item(ui, pal, m, m == current).clicked() {
                    picked = Some(m.clone());
                    ui.close();
                }
            }
        });
    });
    picked
}
