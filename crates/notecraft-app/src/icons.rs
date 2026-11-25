//! Small vector icons drawn with the painter, so they stay sharp at any scale and do not depend
//! on which symbols the system font happens to have.
//!
//! Each icon is a single-colour glyph, so it follows the text colour of the theme.

use eframe::egui::{self, epaint::PathShape, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Icon {
    General,
    Data,
    #[allow(dead_code)]
    Security,
    Ai,
    Assistants,
    Shortcuts,
    About,
    // the sidebar
    Search,
    Clock,
    Graph,
    Import,
    Plus,
    Table,
    Trash,
    WebOff,
}

/// Draw `icon` in one colour inside `rect`; `hole` is the colour behind it (for cut-outs).
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, w: Color32, hole: Color32) {
    let s = rect.width();
    let at = |x: f32, y: f32| Pos2::new(rect.min.x + x * s, rect.min.y + y * s);
    let stroke = Stroke::new((s * 0.075).max(1.3), w);
    match icon {
        Icon::General => {
            // a gear: eight teeth around a ring
            let c = at(0.5, 0.5);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (sin, cos) = a.sin_cos();
                painter.line_segment(
                    [c + Vec2::new(cos, sin) * s * 0.27, c + Vec2::new(cos, sin) * s * 0.38],
                    Stroke::new(s * 0.12, w),
                );
            }
            painter.circle_stroke(c, s * 0.22, Stroke::new(s * 0.10, w));
        }
        Icon::Data => {
            // a cloud
            painter.circle_filled(at(0.36, 0.58), s * 0.15, w);
            painter.circle_filled(at(0.52, 0.47), s * 0.19, w);
            painter.circle_filled(at(0.70, 0.57), s * 0.14, w);
            painter.rect_filled(Rect::from_min_max(at(0.30, 0.58), at(0.76, 0.72)), CornerRadius::same(3), w);
        }
        Icon::Security => {
            // a padlock
            let shackle: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 16.0);
                    at(0.5, 0.45) + Vec2::new(a.cos(), a.sin()) * s * 0.15
                })
                .collect();
            painter.add(PathShape::line(shackle, Stroke::new(s * 0.09, w)));
            painter.rect_filled(Rect::from_min_max(at(0.30, 0.45), at(0.70, 0.76)), CornerRadius::same(2), w);
            painter.circle_filled(at(0.5, 0.60), s * 0.05, hole);
        }
        Icon::Ai => {
            // a four-pointed sparkle and a small one
            let star = |c: Pos2, r: f32, thin: f32| {
                painter.add(PathShape::convex_polygon(
                    vec![
                        c + Vec2::new(0.0, -r),
                        c + Vec2::new(thin, 0.0),
                        c + Vec2::new(0.0, r),
                        c + Vec2::new(-thin, 0.0),
                    ],
                    w,
                    Stroke::NONE,
                ));
                painter.add(PathShape::convex_polygon(
                    vec![
                        c + Vec2::new(-r, 0.0),
                        c + Vec2::new(0.0, thin),
                        c + Vec2::new(r, 0.0),
                        c + Vec2::new(0.0, -thin),
                    ],
                    w,
                    Stroke::NONE,
                ));
            };
            star(at(0.45, 0.55), s * 0.26, s * 0.08);
            star(at(0.72, 0.28), s * 0.12, s * 0.04);
        }
        Icon::Assistants => {
            // two arrows going opposite ways
            for (y, right) in [(0.37, true), (0.63, false)] {
                painter.line_segment([at(0.26, y), at(0.74, y)], stroke);
                let (tip, back) = if right { (0.74, 0.62) } else { (0.26, 0.38) };
                painter.line_segment([at(tip, y), at(back, y - 0.09)], stroke);
                painter.line_segment([at(tip, y), at(back, y + 0.09)], stroke);
            }
        }
        Icon::Shortcuts => {
            // the command key: a small square with a loop at each corner
            painter.rect_stroke(
                Rect::from_min_max(at(0.38, 0.38), at(0.62, 0.62)),
                CornerRadius::ZERO,
                stroke,
                egui::StrokeKind::Middle,
            );
            for (x, y) in [(0.31, 0.31), (0.69, 0.31), (0.31, 0.69), (0.69, 0.69)] {
                painter.circle_stroke(at(x, y), s * 0.085, Stroke::new(s * 0.06, w));
            }
        }
        Icon::Search => {
            painter.circle_stroke(at(0.44, 0.44), s * 0.25, stroke);
            painter.line_segment([at(0.63, 0.63), at(0.84, 0.84)], Stroke::new(stroke.width * 1.3, w));
        }
        Icon::Clock => {
            painter.circle_stroke(at(0.5, 0.5), s * 0.36, stroke);
            painter.line_segment([at(0.5, 0.5), at(0.5, 0.28)], stroke);
            painter.line_segment([at(0.5, 0.5), at(0.66, 0.58)], stroke);
        }
        Icon::Graph => {
            // three linked dots
            let (a, b, c) = (at(0.25, 0.7), at(0.5, 0.26), at(0.78, 0.66));
            painter.line_segment([a, b], stroke);
            painter.line_segment([b, c], stroke);
            painter.line_segment([a, c], stroke);
            for p in [a, b, c] {
                painter.circle_filled(p, s * 0.1, w);
            }
        }
        Icon::Import => {
            // an arrow going down into a tray
            painter.line_segment([at(0.5, 0.14), at(0.5, 0.62)], stroke);
            painter.line_segment([at(0.5, 0.64), at(0.33, 0.46)], stroke);
            painter.line_segment([at(0.5, 0.64), at(0.67, 0.46)], stroke);
            painter.add(PathShape::line(vec![at(0.2, 0.62), at(0.2, 0.84), at(0.8, 0.84), at(0.8, 0.62)], stroke));
        }
        Icon::Plus => {
            painter.line_segment([at(0.5, 0.2), at(0.5, 0.8)], stroke);
            painter.line_segment([at(0.2, 0.5), at(0.8, 0.5)], stroke);
        }
        Icon::Table => {
            painter.rect_stroke(
                Rect::from_min_max(at(0.18, 0.22), at(0.82, 0.78)),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([at(0.18, 0.42), at(0.82, 0.42)], stroke);
            painter.line_segment([at(0.42, 0.42), at(0.42, 0.78)], stroke);
        }
        Icon::Trash => {
            painter.line_segment([at(0.2, 0.28), at(0.8, 0.28)], stroke);
            painter.add(PathShape::line(vec![at(0.4, 0.28), at(0.4, 0.17), at(0.6, 0.17), at(0.6, 0.28)], stroke));
            painter.add(PathShape::line(vec![at(0.28, 0.34), at(0.33, 0.84), at(0.67, 0.84), at(0.72, 0.34)], stroke));
        }
        Icon::WebOff => {
            // a globe crossed out
            let c = at(0.5, 0.5);
            painter.circle_stroke(c, s * 0.34, stroke);
            painter.line_segment([at(0.16, 0.5), at(0.84, 0.5)], Stroke::new(stroke.width * 0.8, w));
            let meridian: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / 24.0;
                    c + Vec2::new(a.cos() * s * 0.15, a.sin() * s * 0.34)
                })
                .collect();
            painter.add(PathShape::line(meridian, Stroke::new(stroke.width * 0.8, w)));
            painter.line_segment([at(0.2, 0.84), at(0.82, 0.16)], Stroke::new(stroke.width * 1.5, hole));
            painter.line_segment([at(0.2, 0.84), at(0.82, 0.16)], stroke);
        }
        Icon::About => {
            // a letter i
            painter.circle_filled(at(0.5, 0.30), s * 0.065, w);
            painter.line_segment([at(0.5, 0.45), at(0.5, 0.72)], Stroke::new(s * 0.11, w));
        }
    }
}

/// A clickable row of the settings side list: icon, label, highlighted when selected.
#[allow(dead_code)]
pub fn nav_item(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    pal: &crate::theme::Palette,
) -> egui::Response {
    let size = Vec2::new(ui.available_width(), 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if selected {
        pal.accent_soft
    } else if resp.hovered() {
        pal.hover
    } else {
        pal.bg
    };
    if selected || resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    }
    let color = if selected { pal.accent } else { pal.text_secondary };
    let icon_rect = Rect::from_min_size(Pos2::new(rect.min.x + 8.0, rect.center().y - 11.0), Vec2::splat(22.0));
    paint(ui.painter(), icon_rect, icon, color, fill);
    ui.painter().text(
        Pos2::new(icon_rect.max.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        if selected { pal.accent } else { pal.text },
    );
    resp.widget_info(|| egui::WidgetInfo::selected(egui::accesskit::Role::Tab, true, selected, label));
    resp
}

/// Small drawn trash can: faint at rest, red under the pointer. Used to remove a workspace from lists.
pub fn trash_button(ui: &mut eframe::egui::Ui, pal: &crate::theme::Palette) -> eframe::egui::Response {
    use eframe::egui::{vec2, Sense};
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let color = if resp.hovered() { pal.danger } else { pal.text_faint };
    paint(
        ui.painter(),
        eframe::egui::Rect::from_center_size(rect.center(), vec2(15.0, 15.0)),
        Icon::Trash,
        color,
        pal.bg,
    );
    resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand)
}
