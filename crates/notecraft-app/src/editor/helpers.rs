//! Small stateless widgets and formatters used by the block editor.

use crate::theme::Palette;
use eframe::egui::{self, Color32, CornerRadius, RichText};

/// Flat segmented control (one rounded track, the chosen segment raised). Returns the id picked.
pub(super) fn segmented(ui: &mut egui::Ui, pal: &Palette, options: &[(&str, String)], current: &str) -> Option<String> {
    let mut picked = None;
    egui::Frame::new().fill(pal.hover).corner_radius(CornerRadius::same(8)).inner_margin(2.0).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (id, label) in options {
                let on = *id == current;
                let text = RichText::new(label).size(12.5).color(if on { pal.text } else { pal.text_secondary });
                let mut b = egui::Button::new(text)
                    .stroke(if on { egui::Stroke::new(0.5, pal.border) } else { egui::Stroke::NONE })
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 22.0));
                if on {
                    b = b.fill(pal.bg);
                }
                if ui.add(b).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                    picked = Some((*id).to_string());
                }
            }
        });
    });
    picked
}

/// A menu that looks like plain secondary text with a thin chevron, no box.
pub(super) fn flat_combo(
    ui: &mut egui::Ui,
    pal: &Palette,
    salt: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    name: &str,
    width: f32,
    add: impl FnOnce(&mut egui::Ui),
) {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::NONE;
        w.hovered.weak_bg_fill = pal.hover;
        w.hovered.bg_stroke = egui::Stroke::NONE;
        w.active.weak_bg_fill = pal.active;
        w.active.bg_stroke = egui::Stroke::NONE;
        let tint = pal.text_secondary;
        crate::widgets::name_combo(
            egui::ComboBox::from_id_salt(salt)
                .width(width)
                .selected_text(RichText::new(text).size(12.5).color(tint))
                .icon(move |ui, rect, _vis, _open| {
                    let c = rect.center();
                    let st = egui::Stroke::new(1.2, tint);
                    ui.painter().line_segment([c + egui::vec2(-3.0, -1.5), c + egui::vec2(0.0, 1.5)], st);
                    ui.painter().line_segment([c + egui::vec2(0.0, 1.5), c + egui::vec2(3.0, -1.5)], st);
                })
                .show_ui(ui, add),
            name,
        );
    });
}

pub(super) fn number_label(n: usize, depth: u8) -> String {
    match depth % 3 {
        1 => format!("{}.", (b'a' + ((n - 1) % 26) as u8) as char),
        2 => format!("{}.", roman(n)),
        _ => format!("{n}."),
    }
}

fn roman(mut n: usize) -> String {
    let mut s = String::new();
    for (v, r) in [(10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
