//! Look and feel: colours for light and dark mode, fonts, sizes and covers.
//! A quiet, Apple-like style: system font, soft greys, one blue accent.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Every colour the UI uses.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub sidebar: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_faint: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub code_bg: Color32,
    pub callout_bg: Color32,
    pub popup_bg: Color32,
    pub danger: Color32,
    pub mention: Color32,
    /// Drop-indicator line while dragging.
    pub drop: Color32,
    /// Dimming behind modal panels.
    pub overlay: Color32,
}

/// rgba() helper for the colour tokens below.
const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, (a * 255.0) as u8)
}

impl Palette {
    /// Light palette: neutral greys with one blue accent.
    pub fn light() -> Self {
        Palette {
            dark: false,
            bg: rgba(255, 255, 255, 1.0),             // background.light.primary
            sidebar: rgba(249, 248, 247, 1.0),        // background.light.secondary (tab bar / sidebar)
            text: rgba(44, 44, 43, 1.0),              // text.light.primary
            text_secondary: rgba(125, 122, 117, 1.0), // text.light.secondary
            text_faint: rgba(161, 158, 153, 1.0),     // text.light.tertiary
            hover: rgba(33, 27, 23, 0.05),            // background.light.interactiveTranslucent
            active: rgba(42, 28, 0, 0.07),            // background.light.tertiaryTranslucent
            border: rgba(230, 229, 227, 1.0),         // border.light.primary
            accent: rgba(39, 131, 222, 1.0),          // blue.light.background.accentPrimary
            accent_soft: rgba(206, 227, 247, 1.0),    // blue.light.background.tertiary
            code_bg: rgba(244, 243, 243, 1.0),        // background.light.interactive
            callout_bg: rgba(240, 239, 237, 1.0),     // gray.light.background.secondary
            popup_bg: rgba(255, 255, 255, 1.0),       // background.light.elevated
            danger: rgba(229, 100, 88, 1.0),          // red.light.background.accentPrimary
            mention: rgba(95, 94, 89, 1.0),           // text.light.accentPrimary
            drop: rgba(35, 131, 226, 0.43),           // electronColors.dropIndicator
            overlay: rgba(15, 15, 15, 0.6),           // electronColors.overlayBackground.light
        }
    }

    /// Dark tokens from the same table.
    pub fn dark() -> Self {
        Palette {
            dark: true,
            bg: rgba(25, 25, 25, 1.0),                // background.dark.primary
            sidebar: rgba(32, 32, 32, 1.0),           // background.dark.secondary
            text: rgba(240, 239, 237, 1.0),           // text.dark.primary
            text_secondary: rgba(173, 169, 163, 1.0), // text.dark.secondary
            text_faint: rgba(125, 122, 117, 1.0),     // text.dark.tertiary
            hover: rgba(255, 255, 255, 0.056),        // background.dark.interactiveTranslucent
            active: rgba(255, 255, 243, 0.082),       // background.dark.tertiaryTranslucent
            border: rgba(56, 56, 54, 1.0),            // border.dark.primary
            accent: rgba(39, 131, 222, 1.0),          // blue.dark.background.accentPrimary
            accent_soft: rgba(53, 95, 139, 1.0),      // blue.dark.background.tertiary
            code_bg: rgba(38, 38, 38, 1.0),           // background.dark.interactive
            callout_bg: rgba(56, 56, 54, 1.0),        // gray.dark.background.secondary
            popup_bg: rgba(32, 32, 32, 1.0),          // background.dark.elevated
            danger: rgba(229, 100, 88, 1.0),          // red.dark.background.accentPrimary
            mention: rgba(188, 186, 182, 1.0),        // text.dark.accentPrimary
            drop: rgba(35, 131, 226, 0.43),           // electronColors.dropIndicator
            overlay: rgba(15, 15, 15, 0.3),           // electronColors.overlayBackground.dark
        }
    }

    /// (chip background, chip text, status dot) of a select option colour:
    /// the soft background, the text and the accent of each named colour.
    pub fn option_tokens(&self, key: &str) -> (Color32, Color32, Color32) {
        // (light bg, light text, dark bg, dark text, accent)
        let t: [(u8, u8, u8); 5] = match key {
            "gray" => [(230, 229, 227), (73, 72, 70), (95, 94, 89), (240, 239, 237), (142, 139, 134)],
            "brown" => [(235, 223, 215), (88, 68, 55), (109, 83, 64), (245, 237, 233), (182, 137, 101)],
            "orange" => [(243, 221, 203), (106, 66, 34), (136, 82, 47), (251, 235, 222), (213, 128, 59)],
            "yellow" => [(242, 227, 183), (101, 81, 33), (131, 100, 38), (249, 243, 220), (216, 163, 47)],
            "green" => [(215, 230, 221), (42, 83, 60), (55, 103, 76), (232, 241, 236), (70, 161, 113)],
            "blue" => [(206, 227, 247), (38, 74, 114), (53, 95, 139), (229, 242, 252), (39, 131, 222)],
            "purple" => [(232, 219, 242), (85, 59, 105), (103, 77, 123), (243, 235, 249), (181, 119, 214)],
            "pink" => [(244, 216, 228), (104, 53, 78), (123, 71, 96), (250, 233, 241), (219, 105, 153)],
            "red" => [(247, 217, 213), (109, 53, 49), (146, 73, 67), (252, 233, 231), (229, 100, 88)],
            // "default": neutral background.tertiary / text.primary
            _ => [(240, 239, 237), (44, 44, 43), (56, 56, 54), (240, 239, 237), (142, 139, 134)],
        };
        let c = |(r, g, b): (u8, u8, u8)| Color32::from_rgb(r, g, b);
        if self.dark {
            (c(t[2]), c(t[3]), c(t[4]))
        } else {
            (c(t[0]), c(t[1]), c(t[4]))
        }
    }

    /// Background and text colour of a select option chip.
    pub fn option_colors(&self, key: &str) -> (Color32, Color32) {
        let (bg, fg, _) = self.option_tokens(key);
        (bg, fg)
    }
}

/// Window size and zoom limits.
pub const DEFAULT_WINDOW_SIZE: [f32; 2] = [1320.0, 860.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [640.0, 480.0];
pub const ZOOM_DEFAULT: f32 = 1.0;
pub const ZOOM_STEP: f32 = 0.1;
pub const ZOOM_MIN: f32 = 0.5;
pub const ZOOM_MAX: f32 = 4.9;
pub const QUICK_FIND_SIZE: [f32; 2] = [755.0, 700.0];

/// Clamp and round a zoom factor (round to 0.1, clamp 0.5–4.9).
pub fn clamp_zoom(z: f32) -> f32 {
    ((z * 10.0).round() / 10.0).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// Left padding (points) that keeps content clear of the macOS window
/// buttons, by zoom factor.
pub fn window_controls_padding_mac(zoom: f32) -> f32 {
    let z = clamp_zoom(zoom);
    if (z - 0.5).abs() < 0.01 {
        155.0
    } else if (z - 0.6).abs() < 0.01 {
        128.0
    } else if (z - 0.7).abs() < 0.01 {
        108.0
    } else if (z - 0.8).abs() < 0.01 {
        94.0
    } else if (z - 0.9).abs() < 0.01 {
        83.0
    } else {
        75.0
    }
}

pub const PAGE_MAX_WIDTH: f32 = 720.0;
pub const BODY_SIZE: f32 = 16.0;
pub const SIDEBAR_WIDTH: f32 = 248.0;

pub fn heading_size(level: u8) -> f32 {
    match level {
        1 => 28.0,
        2 => 22.0,
        _ => 18.5,
    }
}

/// Apply the palette to egui's visuals.
pub fn apply(ctx: &egui::Context, p: &Palette) {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.popup_bg;
    v.extreme_bg_color = if p.dark { Color32::from_rgb(0x18, 0x18, 0x1A) } else { Color32::from_rgb(0xFA, 0xFA, 0xFA) };
    v.faint_bg_color = p.code_bg;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 110 } else { 38 }),
    };
    v.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 100 } else { 30 }),
    };
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(6);
        w.fg_stroke.color = p.text;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill =
        if p.dark { Color32::from_rgb(0x33, 0x33, 0x36) } else { Color32::from_rgb(0xEC, 0xEC, 0xEE) };
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.weak_bg_fill = p.active;
    v.widgets.active.bg_fill = p.active;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.open.weak_bg_fill = p.active;
    v.text_cursor.stroke = Stroke::new(1.6, p.accent);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.menu_margin = egui::Margin::symmetric(10, 8);
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(13.5));
        s.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(20.0));
    });
}

/// True when the UI font has a `wght` axis (so bold can be drawn).
static PAGE_FONT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static PAGE_SMALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Typeface and size of the page being drawn (`""`, `"serif"` or `"mono"`).
static TEXT_PERCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(100);

/// Global body text size, in percent (Settings).
pub fn set_text_percent(p: u32) {
    TEXT_PERCENT.store(p.clamp(50, 200), std::sync::atomic::Ordering::Relaxed);
}

pub fn set_page_style(font: &str, small: bool) {
    let f = match font {
        "serif" => 1,
        "mono" => 2,
        _ => 0,
    };
    PAGE_FONT.store(f, std::sync::atomic::Ordering::Relaxed);
    PAGE_SMALL.store(small, std::sync::atomic::Ordering::Relaxed);
}

pub fn page_family() -> FontFamily {
    match PAGE_FONT.load(std::sync::atomic::Ordering::Relaxed) {
        1 => FontFamily::Name("serif".into()),
        2 => FontFamily::Monospace,
        _ => FontFamily::Proportional,
    }
}

/// Factor on the size of the body text.
pub fn body_scale() -> f32 {
    let page = if PAGE_SMALL.load(std::sync::atomic::Ordering::Relaxed) { 0.875 } else { 1.0 };
    page * TEXT_PERCENT.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0
}

fn serif_candidates() -> Vec<std::path::PathBuf> {
    if cfg!(target_os = "macos") {
        [
            "/System/Library/Fonts/NewYork.ttf",
            "/System/Library/Fonts/Supplemental/Georgia.ttf",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .collect()
    } else if cfg!(target_os = "windows") {
        std::env::var_os("WINDIR")
            .map(|w| vec![std::path::Path::new(&w).join("Fonts").join("georgia.ttf")])
            .unwrap_or_default()
    } else {
        vec![]
    }
}

static VARIABLE_WEIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn has_weight_axis() -> bool {
    VARIABLE_WEIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Candidate system fonts (the platform's own UI font), first found wins.
fn system_font_candidates() -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    if cfg!(target_os = "macos") {
        {
            let dir = "/System/Library/Fonts";
            v.push(std::path::Path::new(dir).join("SFNS.ttf"));
        }
    } else if cfg!(target_os = "windows") {
        if let Some(windir) = std::env::var_os("WINDIR") {
            v.push(std::path::Path::new(&windir).join("Fonts").join("segoeui.ttf"));
        }
    }
    v
}

fn mono_candidates() -> Vec<std::path::PathBuf> {
    if cfg!(target_os = "macos") {
        vec![
            std::path::PathBuf::from("/System/Library/Fonts/SFNSMono.ttf"),
            std::path::PathBuf::from("/System/Library/Fonts/Menlo.ttc"),
        ]
    } else {
        vec![]
    }
}

/// Install fonts: the system UI font when available, egui's defaults as fallback
/// (they also provide emoji glyphs).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some((path, bytes)) =
        system_font_candidates().into_iter().find_map(|p| std::fs::read(&p).ok().map(|b| (p, b)))
    {
        let _ = path;
        // a crude check for a variable font: the `fvar` table tag
        let variable = bytes.windows(4).take(4096).any(|w| w == b"fvar");
        VARIABLE_WEIGHT.store(variable, std::sync::atomic::Ordering::Relaxed);
        fonts.font_data.insert("system-ui".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "system-ui".into());
    }
    if let Some(bytes) = mono_candidates().into_iter().find_map(|p| std::fs::read(&p).ok()) {
        fonts.font_data.insert("system-mono".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "system-mono".into());
    }
    // the serif family falls back on the regular fonts (emoji, missing glyphs)
    let mut serif: Vec<String> = Vec::new();
    if let Some(bytes) = serif_candidates().into_iter().find_map(|p| std::fs::read(&p).ok()) {
        fonts.font_data.insert("system-serif".into(), Arc::new(FontData::from_owned(bytes)));
        serif.push("system-serif".into());
    }
    serif.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("serif".into()), serif);
    ctx.set_fonts(fonts);
}

/// Variation coordinates for a font weight (400 regular, 600 semibold, 700 bold).
pub fn weight(w: f32) -> egui::epaint::text::VariationCoords {
    egui::epaint::text::VariationCoords::new([("wght", w)])
}

/// Gradient covers: (top-left, bottom-right) colours.
pub const COVERS: [(Color32, Color32); 8] = [
    (Color32::from_rgb(0xF6, 0xD3, 0x65), Color32::from_rgb(0xFD, 0xA0, 0x85)),
    (Color32::from_rgb(0xA1, 0xC4, 0xFD), Color32::from_rgb(0xC2, 0xE9, 0xFB)),
    (Color32::from_rgb(0xD4, 0xFC, 0x79), Color32::from_rgb(0x96, 0xE6, 0xA1)),
    (Color32::from_rgb(0xFB, 0xC2, 0xEB), Color32::from_rgb(0xA6, 0xC1, 0xEE)),
    (Color32::from_rgb(0x2B, 0x58, 0x76), Color32::from_rgb(0x4E, 0x43, 0x76)),
    (Color32::from_rgb(0xE0, 0xC3, 0xFC), Color32::from_rgb(0x8E, 0xC5, 0xFC)),
    (Color32::from_rgb(0xFF, 0xEC, 0xD2), Color32::from_rgb(0xFC, 0xB6, 0x9F)),
    (Color32::from_rgb(0x43, 0x4E, 0x5E), Color32::from_rgb(0x8E, 0x9E, 0xAB)),
];

/// Paint a diagonal two-colour gradient into `rect`.
pub fn paint_gradient(painter: &egui::Painter, rect: egui::Rect, a: Color32, b: Color32) {
    let mut mesh = egui::Mesh::default();
    let mid = a.lerp_to_gamma(b, 0.5);
    mesh.colored_vertex(rect.left_top(), a);
    mesh.colored_vertex(rect.right_top(), mid);
    mesh.colored_vertex(rect.right_bottom(), b);
    mesh.colored_vertex(rect.left_bottom(), mid);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// A short curated set of emoji for page icons (all drawn by the bundled emoji font).
pub const EMOJI: &[&str] = &[
    "📄", "📝", "📘", "📗", "📕", "📙", "📚", "📓", "📒", "📋", "📌", "📎", "🗂", "🗃", "🗒", "📅", "📆", "📊", "📈",
    "💡", "⭐", "✨", "🔥", "✅", "☑", "❗", "❓", "⚠", "🎯", "🚀", "🏠", "🏢", "💼", "🧭", "🗺", "🌍", "🌱", "🌿",
    "🌸", "🌻", "🍀", "☀", "🌙", "⛅", "❄", "🌈", "☕", "🍎", "🍕", "🎨", "🎵", "🎬", "📷", "🎮", "⚽", "🏃", "🧘",
    "💪", "❤", "💬", "📣", "🔔", "🔒", "🔑", "🛠", "⚙", "💻", "🖥", "📱", "🧪", "🔬", "🧠", "🎓", "✏", "🖊", "📐", "💰",
    "🛒", "✈", "🚲", "👋", "😀", "🙂", "😎", "🤔", "🎉", "🐱", "🐶",
];

/// Colours of a block colour name (`red` for the text, `bg-red` for the background):
/// `(text, background)`; `None` for an unknown name.
pub fn block_color(name: &str, dark: bool) -> Option<(Option<Color32>, Option<Color32>)> {
    let (is_bg, base) = match name.strip_prefix("bg-") {
        Some(b) => (true, b),
        None => (false, name),
    };
    // (name, light text, dark text, light background, dark background)
    const TABLE: [(&str, u32, u32, u32, u32); 9] = [
        ("gray", 0x9B9A97, 0x979A9B, 0xEBECED, 0x454B4E),
        ("brown", 0x64473A, 0x937264, 0xE9E5E3, 0x434040),
        ("orange", 0xD9730D, 0xFFA344, 0xFAEBDD, 0x594A3A),
        ("yellow", 0xDFAB01, 0xFFDC49, 0xFBF3DB, 0x59563B),
        ("green", 0x0F7B6C, 0x4DAB9A, 0xDDEDEA, 0x354C4B),
        ("blue", 0x0B6E99, 0x529CCA, 0xDDEBF1, 0x364954),
        ("purple", 0x6940A5, 0x9A6DD7, 0xEAE4F2, 0x443F57),
        ("pink", 0xAD1A72, 0xE255A1, 0xF4DFEB, 0x533B4C),
        ("red", 0xE03E3E, 0xFF7369, 0xFBE4E4, 0x594141),
    ];
    let rgb = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    let (_, lt, dt, lb, db) = *TABLE.iter().find(|(n, ..)| *n == base)?;
    Some(if is_bg {
        (None, Some(rgb(if dark { db } else { lb })))
    } else {
        (Some(rgb(if dark { dt } else { lt })), None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_rules_match_the_original() {
        assert_eq!(clamp_zoom(0.3), 0.5);
        assert_eq!(clamp_zoom(9.0), 4.9);
        assert!((clamp_zoom(1.04) - 1.0).abs() < 1e-6);
        assert_eq!(window_controls_padding_mac(1.0), 75.0);
        assert_eq!(window_controls_padding_mac(0.5), 155.0);
        assert_eq!(window_controls_padding_mac(0.8), 94.0);
        assert_eq!(window_controls_padding_mac(2.0), 75.0);
    }

    #[test]
    fn option_tokens_by_theme() {
        let (bg, fg, _) = Palette::light().option_tokens("blue");
        assert_eq!(bg, Color32::from_rgb(206, 227, 247));
        assert_eq!(fg, Color32::from_rgb(38, 74, 114));
        let (bg, _, dot) = Palette::dark().option_tokens("green");
        assert_eq!(bg, Color32::from_rgb(55, 103, 76));
        assert_eq!(dot, Color32::from_rgb(70, 161, 113));
    }
}
