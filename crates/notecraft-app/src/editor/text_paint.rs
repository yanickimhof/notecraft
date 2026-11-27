//! How a block's text is drawn: base look per block type, inline formatting, underlines, placeholders.

use super::*;

/// Base look of a block's text.
#[derive(Clone, Copy)]
pub(super) struct Base {
    pub(super) size: f32,
    pub(super) weight: f32,
    pub(super) color: Color32,
    pub(super) strike: bool,
}

pub(super) fn base_for(kind: &BlockKind, color: &str, pal: &Palette) -> Base {
    let mut b = Base { size: theme::BODY_SIZE * theme::body_scale(), weight: 400.0, color: pal.text, strike: false };
    if let Some((Some(text), _)) = theme::block_color(color, pal.dark) {
        b.color = text;
    }
    match kind {
        BlockKind::Heading(l) => {
            b.size = theme::heading_size(*l) * theme::body_scale();
            b.weight = 680.0;
        }
        BlockKind::Todo { checked: true } => {
            b.color = pal.text_faint;
            b.strike = true;
        }
        BlockKind::Quote => b.size = 17.0 * theme::body_scale(),
        _ => {}
    }
    b
}

pub(super) fn format_for(base: &Base, pal: &Palette) -> TextFormat {
    let mut f = TextFormat {
        font_id: FontId::new(base.size, theme::page_family()),
        color: base.color,
        line_height: Some(base.size * 1.5),
        ..Default::default()
    };
    if theme::has_weight_axis() && base.weight != 400.0 {
        f.coords = theme::weight(base.weight);
    }
    if base.strike {
        f.strikethrough = Stroke::new(1.0, pal.text_faint);
    }
    f
}

/// Lay out rich text with inline styles; markers are dimmed when editing and
/// hidden otherwise.
pub(super) fn inline_job(text: &str, base: &Base, pal: &Palette, wrap: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    job.first_row_min_height = base.size * 1.5;
    for seg in inline::parse(text) {
        let s = &seg.style;
        let mut f = format_for(base, pal);
        let mut weight = base.weight;
        if s.bold {
            weight = 700.0;
        }
        if s.italic {
            f.italics = true;
        }
        if let Some(c) = &s.color {
            f.color = pal.option_tokens(c).2;
        }
        // underlines (also for links and mentions) are painted by `paint_underlines`
        if s.strike {
            f.strikethrough = Stroke::new(1.0, f.color);
        }
        if s.code {
            f.font_id = FontId::monospace(base.size * 0.86);
            f.background = pal.code_bg;
            f.color = if pal.dark { Color32::from_rgb(0xFF, 0x8A, 0x80) } else { Color32::from_rgb(0xC4, 0x3E, 0x3E) };
        }
        if s.mention.is_some() {
            weight = weight.max(560.0);
            f.color = pal.mention;
        }
        // a link reads as a link: blue (an explicit text colour still wins)
        if s.link.is_some() && s.color.is_none() && !s.code {
            f.color = pal.accent;
        }
        if theme::has_weight_axis() && weight != 400.0 {
            f.coords = theme::weight(weight);
        }
        if seg.marker {
            // WYSIWYG: markers stay in the stored text but are never shown
            // hidden markers must not take room: a long link address used to leave a wide gap
            f.font_id = FontId::proportional(0.01);
            f.color = Color32::TRANSPARENT;
            f.underline = Stroke::NONE;
            f.strikethrough = Stroke::NONE;
            f.background = Color32::TRANSPARENT;
            f.italics = false;
            f.line_height = None;
        }
        job.append(&text[seg.range.clone()], 0.0, f);
    }
    job
}

/// Draw underlines (underline style, links, mentions) just below the
/// baseline. egui's own underline sits at the bottom of the (taller) line
/// box, where the text field clips it, so it never showed.
pub(super) fn paint_underlines(
    painter: &egui::Painter,
    o: &egui::text_edit::TextEditOutput,
    text: &str,
    pal: &Palette,
    color: Color32,
) {
    let n = text.chars().count();
    let mut marks: Vec<Option<Color32>> = vec![None; n];
    let byte_to_char: std::collections::HashMap<usize, usize> =
        text.char_indices().enumerate().map(|(ci, (bi, _))| (bi, ci)).collect();
    let mut any = false;
    for seg in inline::parse(text) {
        if seg.marker {
            continue;
        }
        let s = &seg.style;
        let c = if s.link.is_some() && s.color.is_none() {
            Some(pal.accent)
        } else if s.link.is_some() || s.mention.is_some() {
            Some(pal.text_faint)
        } else if s.underline {
            Some(if s.code { pal.text_secondary } else { color })
        } else {
            None
        };
        if let Some(c) = c {
            for (bi, _) in text[seg.range.clone()].char_indices() {
                if let Some(ci) = byte_to_char.get(&(seg.range.start + bi)) {
                    marks[*ci] = Some(c);
                    any = true;
                }
            }
        }
    }
    if !any {
        return;
    }
    let mut ci = 0;
    for row in &o.galley.rows {
        for g in &row.row.glyphs {
            if let Some(Some(c)) = marks.get(ci) {
                let base = o.galley_pos + row.pos.to_vec2() + g.pos.to_vec2();
                let y = base.y + 2.5;
                painter.line_segment(
                    [egui::pos2(base.x, y), egui::pos2(base.x + g.advance_width, y)],
                    Stroke::new(1.0, *c),
                );
            }
            ci += 1;
        }
        if row.ends_with_newline {
            ci += 1;
        }
    }
}

pub(super) fn placeholder(kind: &BlockKind, focused: bool, lang: Lang) -> Option<&'static str> {
    match kind {
        BlockKind::Heading(1) => Some(tr(lang, "blk_h1")),
        BlockKind::Heading(2) => Some(tr(lang, "blk_h2")),
        BlockKind::Heading(_) => Some(tr(lang, "blk_h3")),
        _ if !focused => None,
        BlockKind::Paragraph => Some(tr(lang, "type_slash")),
        BlockKind::Bulleted | BlockKind::Numbered => Some(tr(lang, "list_placeholder")),
        BlockKind::Todo { .. } => Some(tr(lang, "todo_placeholder")),
        BlockKind::Toggle { .. } => Some(tr(lang, "toggle_placeholder")),
        BlockKind::Quote => Some(tr(lang, "quote_placeholder")),
        BlockKind::Callout { .. } => Some(tr(lang, "type_slash")),
        _ => None,
    }
}
