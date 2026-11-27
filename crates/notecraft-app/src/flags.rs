//! Flags as page icons. The icon is stored as the flag emoji (two regional indicator letters), which keeps
//! Markdown and other apps happy; the app draws it as a picture because its fonts have no flag glyphs.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

pub use crate::flags_data::FLAGS;

/// The flag emoji of a two letter country code (`ch` gives the Swiss flag).
pub fn emoji_of(code: &str) -> String {
    code.to_ascii_uppercase()
        .chars()
        .filter(char::is_ascii_uppercase)
        .map(|c| char::from_u32(0x1F1E6 + (c as u32 - 'A' as u32)).unwrap_or(c))
        .collect()
}

/// The country code (lower case) when `icon` is exactly one flag emoji we have a picture for.
pub fn code_of(icon: &str) -> Option<&'static str> {
    let mut it = icon.trim().chars();
    let (a, b) = (it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let letter = |c: char| {
        (0x1F1E6..=0x1F1FF)
            .contains(&(c as u32))
            .then(|| char::from_u32('a' as u32 + (c as u32 - 0x1F1E6)).unwrap_or('a'))
    };
    let code: String = [letter(a)?, letter(b)?].iter().collect();
    FLAGS.iter().find(|f| f.0 == code).map(|f| f.0)
}

/// The picture of a flag, drawn once and kept for the session.
pub fn texture(ctx: &egui::Context, code: &str) -> Option<TextureHandle> {
    let id = egui::Id::unique(("flag-texture", code));
    if let Some(t) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(t);
    }
    let svg = FLAGS.iter().find(|f| f.0 == code)?.3;
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let (w, h) = (128u32, 96u32);
    let mut pix = resvg::tiny_skia::Pixmap::new(w, h)?;
    let size = tree.size();
    let tf = resvg::tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, tf, &mut pix.as_mut());
    let img = ColorImage::from_rgba_premultiplied([w as usize, h as usize], pix.data());
    let tex = ctx.load_texture(format!("flag-{code}"), img, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Draw a flag in `rect` (4:3 picture centred in it).
pub fn paint(ui: &egui::Ui, rect: egui::Rect, code: &str) -> bool {
    let Some(tex) = texture(ui.ctx(), code) else { return false };
    let w = rect.width().min(rect.height() * 4.0 / 3.0);
    let r = egui::Rect::from_center_size(rect.center(), egui::vec2(w, w * 0.75));
    ui.painter().image(
        tex.id(),
        r,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    ui.painter().rect_stroke(
        r,
        2.0,
        egui::Stroke::new(0.5, egui::Color32::from_black_alpha(40)),
        egui::StrokeKind::Outside,
    );
    true
}

/// A title that starts with a flag (`🇨🇭 Title`): the text with blank space where the flag goes (so a button
/// can reserve the room), and the flag's code. Other text comes back unchanged.
pub fn reserve(text: &str) -> (String, Option<&'static str>) {
    let head: String = text.chars().take(2).collect();
    match code_of(&head) {
        Some(code) => {
            let rest: String = text.chars().skip(2).collect();
            (format!("\u{2003}\u{2002} {}", rest.trim_start()), Some(code))
        }
        None => (text.to_string(), None),
    }
}

/// Draw the flag reserved by [`reserve`] at the left of a button that has the text.
pub fn paint_in_button(ui: &egui::Ui, button: &egui::Response, code: &str, text_size: f32) {
    let h = text_size * 0.95;
    let rect = egui::Rect::from_min_size(
        egui::pos2(button.rect.min.x + ui.spacing().button_padding.x, button.rect.center().y - h * 0.5),
        egui::vec2(h * 1.34, h),
    );
    paint(ui, rect, code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_flag_is_swapped_for_room() {
        let (text, code) = reserve("🇨🇭 Suisse");
        assert_eq!(code, Some("ch"));
        assert!(text.ends_with("Suisse") && !text.contains('🇨'));
        assert_eq!(reserve("📄 Page"), ("📄 Page".to_string(), None));
    }

    #[test]
    fn flag_emoji_round_trip() {
        assert_eq!(emoji_of("ch"), "🇨🇭");
        assert_eq!(code_of("🇨🇭"), Some("ch"));
        assert_eq!(code_of("📄"), None);
        assert_eq!(code_of("🇨🇭 x"), None);
        assert!(FLAGS.len() > 200);
    }

    #[test]
    fn every_flag_draws() {
        let ctx = egui::Context::default();
        for f in FLAGS {
            assert!(texture(&ctx, f.0).is_some(), "{} does not draw", f.0);
        }
    }
}
