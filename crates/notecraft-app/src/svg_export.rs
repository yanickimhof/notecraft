//! Mermaid diagrams and equations as SVG, for the published page and the preview.
//!
//! The app already knows how to draw both with egui. Here the same drawing is made on an
//! off-screen context and the shapes it produces are written out as SVG, so the web page shows
//! what the editor shows without shipping any script. Colours follow the page theme through CSS
//! variables.

use eframe::egui::{self, epaint::ColorMode, Color32, Pos2, Shape};

const INK: Color32 = Color32::from_rgb(10, 10, 10);
const LINE: Color32 = Color32::from_rgb(20, 20, 20);
const FILL: Color32 = Color32::from_rgb(30, 30, 30);
const BG: Color32 = Color32::from_rgb(40, 40, 40);

/// Run `f` on a painter of an off-screen context and return what it painted.
fn capture<R>(f: impl FnOnce(&egui::Painter) -> R) -> (R, Vec<egui::epaint::ClippedShape>) {
    let ctx = egui::Context::default();
    ctx.begin_pass(egui::RawInput::default());
    let painter = ctx.layer_painter(egui::LayerId::background());
    let r = f(&painter);
    let mut out = ctx.end_pass();
    out.textures_delta.clear();
    (r, std::mem::take(&mut out.shapes))
}

fn color(c: Color32) -> String {
    if c == Color32::TRANSPARENT {
        return "none".into();
    }
    match (c.r(), c.g(), c.b()) {
        (10, 10, 10) => "var(--text)".into(),
        (20, 20, 20) => "var(--text2)".into(),
        (30, 30, 30) => "var(--bg)".into(),
        (40, 40, 40) => "var(--code)".into(),
        (r, g, b) if c.a() == 255 => format!("rgb({r},{g},{b})"),
        (r, g, b) => format!("rgba({r},{g},{b},{:.2})", f32::from(c.a()) / 255.0),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn pts(p: &[Pos2], dx: f32, dy: f32) -> String {
    p.iter().map(|p| format!("{:.1},{:.1}", p.x - dx, p.y - dy)).collect::<Vec<_>>().join(" ")
}

fn emit(shape: &Shape, dx: f32, dy: f32, out: &mut String) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| emit(s, dx, dy, out)),
        Shape::Rect(r) => {
            let rx = f32::from(r.corner_radius.nw);
            let sw = r.stroke.width;
            out.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"{rx}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{sw}\"/>",
                r.rect.min.x - dx,
                r.rect.min.y - dy,
                r.rect.width(),
                r.rect.height(),
                color(r.fill),
                if sw > 0.0 { color(r.stroke.color) } else { "none".into() },
            ));
        }
        Shape::LineSegment { points, stroke } => {
            out.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                points[0].x - dx,
                points[0].y - dy,
                points[1].x - dx,
                points[1].y - dy,
                color(stroke.color),
                stroke.width
            ));
        }
        Shape::Path(p) => {
            let stroke = match &p.stroke.color {
                ColorMode::Solid(c) if p.stroke.width > 0.0 => color(*c),
                _ => "none".into(),
            };
            let tag = if p.closed { "polygon" } else { "polyline" };
            out.push_str(&format!(
                "<{tag} points=\"{}\" fill=\"{}\" stroke=\"{stroke}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
                pts(&p.points, dx, dy),
                if p.closed { color(p.fill) } else { "none".into() },
                p.stroke.width
            ));
        }
        Shape::Circle(c) => out.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
            c.center.x - dx,
            c.center.y - dy,
            c.radius,
            color(c.fill),
            if c.stroke.width > 0.0 { color(c.stroke.color) } else { "none".into() },
            c.stroke.width
        )),
        Shape::Ellipse(e) => out.push_str(&format!(
            "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"{:.1}\" ry=\"{:.1}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
            e.center.x - dx,
            e.center.y - dy,
            e.radius.x,
            e.radius.y,
            color(e.fill),
            if e.stroke.width > 0.0 { color(e.stroke.color) } else { "none".into() },
            e.stroke.width
        )),
        Shape::Text(t) => {
            let size = t.galley.job.sections.first().map_or(14.0, |s| s.format.font_id.size);
            let c = t.override_text_color.unwrap_or_else(|| {
                let c = t.galley.job.sections.first().map_or(Color32::PLACEHOLDER, |s| s.format.color);
                if c == Color32::PLACEHOLDER {
                    t.fallback_color
                } else {
                    c
                }
            });
            for row in &t.galley.rows {
                let text: String = row.row.glyphs.iter().map(|g| g.chr).collect();
                if text.trim().is_empty() {
                    continue;
                }
                let width: f32 = row.row.glyphs.iter().map(|g| g.advance_width).sum();
                out.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{size:.1}\" fill=\"{}\" textLength=\"{width:.1}\" lengthAdjust=\"spacingAndGlyphs\" xml:space=\"preserve\">{}</text>",
                    t.pos.x + row.pos.x - dx,
                    t.pos.y + row.pos.y - dy + row.row.size.y * 0.78,
                    color(c),
                    esc(&text)
                ));
            }
        }
        _ => {}
    }
}

fn wrap(shapes: &[egui::epaint::ClippedShape], min: Pos2, w: f32, h: f32, class: &str) -> String {
    let mut body = String::new();
    for s in shapes {
        emit(&s.shape, min.x, min.y, &mut body);
    }
    format!(
        "<svg class=\"{class}\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" width=\"{w:.0}\" height=\"{h:.0}\" font-family=\"ui-sans-serif,-apple-system,'Segoe UI',Helvetica,Arial,sans-serif\" role=\"img\">{body}</svg>"
    )
}

/// A Mermaid diagram as an SVG picture, if the source is one the app can draw.
pub fn diagram(src: &str) -> Option<String> {
    let (size, shapes) = capture(|p| {
        let drawn = crate::mermaid_more::prepare(p, src)?;
        let pad = 8.0;
        let origin = Pos2::new(pad, pad);
        let colors = crate::mermaid::Colors { ink: INK, fill: FILL, line: LINE, soft: LINE, bg: BG };
        drawn.draw(p, origin, &colors);
        Some(drawn.size() + egui::vec2(2.0 * pad, 2.0 * pad))
    });
    let size = size?;
    Some(wrap(&shapes, Pos2::ZERO, size.x.ceil(), size.y.ceil(), "nc-diagram"))
}

/// A LaTeX equation as an SVG picture.
pub fn equation(src: &str) -> Option<String> {
    let (dims, shapes) = capture(|p| {
        let lay = crate::math::layout(p, &crate::math::parse(src), 26.0);
        let (w, h) = (lay.w + 16.0, lay.height().max(26.0) + 16.0);
        crate::math::draw(p, &lay, Pos2::new(8.0, 8.0 + crate::math::ascent(&lay)), INK);
        (w, h)
    });
    Some(wrap(&shapes, Pos2::ZERO, dims.0.ceil(), dims.1.ceil(), "nc-equation"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_flowchart_and_an_equation_become_svg() {
        let d = super::diagram("graph LR\n  A[Idée] --> B[Note]").expect("diagram");
        assert!(d.starts_with("<svg") && d.contains("<text") && d.contains("Idée"), "{d}");
        let e = super::equation("E = mc^2").expect("equation");
        assert!(e.contains("<text"), "{e}");
        if let Ok(dir) = std::env::var("NC_SVG_OUT") {
            let d2 =
                super::diagram("graph LR\n  A[Idée] --> B[Note] --> C[Page structurée] --> D[Publication]").unwrap();
            let e2 = super::equation("\\frac{a}{b} + \\sqrt{x^2}").unwrap();
            let page = format!("<body style=\"background:#fff;color:#2c2c2b;--text:#2c2c2b;--text2:#7d7a75;--bg:#fff;--code:#f4f3f3\">{d2}<br>{e2}<br>{e}</body>");
            std::fs::write(format!("{dir}/svg.html"), page).unwrap();
        }
    }
}
