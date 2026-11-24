//! The graph of `[[links]]` between pages: a window with draggable, zoomable nodes.

use crate::app::{NcApp, Target};
use crate::i18n::tr;
use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, Vec2};
use notecraft_core::graph::{self, LinkGraph};
use notecraft_core::DocId;

/// A graph being shown: what links to what, where each node sits, and how the view is moved.
pub struct GraphState {
    pub graph: LinkGraph,
    pub pos: Vec<(f32, f32)>,
    pub pan: Vec2,
    pub zoom: f32,
}

impl GraphState {
    pub fn new(graph: LinkGraph) -> Self {
        let pos = graph::layout(graph.nodes.len(), &graph.edges, 220);
        GraphState { graph, pos, pan: Vec2::ZERO, zoom: 1.0 }
    }
}

impl NcApp {
    pub fn graph_window(&mut self, ctx: &egui::Context) {
        if !self.graph_open {
            self.graph = None;
            return;
        }
        if self.graph.is_none() {
            let Some(ws) = self.ws.as_ref() else {
                self.graph_open = false;
                return;
            };
            self.graph = Some(GraphState::new(ws.link_graph()));
        }
        let lang = self.prefs.lang;
        let pal = self.pal;
        let mut open = true;
        let mut go: Option<DocId> = None;
        if let Some(st) = self.graph.as_mut() {
            egui::Window::new(tr(lang, "graph"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([860.0, 620.0])
                .show(ctx, |ui| {
                    go = graph_ui(ui, st, &pal, lang).0;
                });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        if let Some(doc) = go {
            self.navigate(Target::Doc(doc));
            open = false;
        }
        if !open {
            self.graph_open = false;
            self.graph = None;
        }
    }
}

/// Draws the graph in `ui`; returns the page that was clicked, if any, and the area the graph covers.
pub(crate) fn graph_ui(
    ui: &mut egui::Ui,
    st: &mut GraphState,
    pal: &crate::theme::Palette,
    lang: crate::i18n::Lang,
) -> (Option<DocId>, egui::Rect) {
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    let mut go: Option<DocId> = None;
    if st.graph.nodes.is_empty() {
        ui.add_space(20.0);
        ui.label(RichText::new(tr(lang, "graph_empty")).color(pal.text_faint));
        return (None, ui.min_rect());
    }
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, pal.bg);
    if resp.dragged() {
        st.pan += resp.drag_delta();
    }
    if resp.hovered() {
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            st.zoom = (st.zoom * (scroll * 0.004).exp()).clamp(0.3, 8.0);
        }
    }
    let size = rect.width().min(rect.height()) * 0.86;
    let at = |p: (f32, f32)| rect.center() + st.pan + Vec2::new((p.0 - 0.5) * size, (p.1 - 0.5) * size) * st.zoom;
    let screen: Vec<egui::Pos2> = st.pos.iter().map(|p| at(*p)).collect();
    let radius = |i: usize| 3.5 + (st.graph.degree[i] as f32).sqrt() * 2.0;
    let hovered = ctx.pointer_hover_pos().filter(|p| rect.contains(*p) && resp.hovered()).and_then(|p| {
        (0..screen.len())
            .map(|i| (i, screen[i].distance(p)))
            .filter(|(i, d)| *d <= radius(*i) + 5.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    });
    let near = |i: usize| {
        hovered.is_some_and(|h| h == i || st.graph.edges.iter().any(|&(a, b)| (a == h && b == i) || (b == h && a == i)))
    };
    for &(a, b) in &st.graph.edges {
        let lit = hovered.is_some_and(|h| h == a || h == b);
        let stroke = if lit { Stroke::new(1.6, pal.accent) } else { Stroke::new(1.0, pal.border) };
        painter.line_segment([screen[a], screen[b]], stroke);
    }
    for (i, p) in screen.iter().enumerate() {
        let dim = hovered.is_some() && !near(i);
        let color = match st.graph.nodes[i] {
            DocId::Page(_) => pal.accent,
            _ => pal.text_secondary,
        };
        let color = if dim { color.gamma_multiply(0.35) } else { color };
        painter.circle_filled(*p, radius(i), color);
        let show = hovered == Some(i)
            || near(i) && hovered.is_some()
            || (hovered.is_none() && (st.zoom >= 1.6 || st.graph.degree[i] >= 3));
        if show {
            let title = &st.graph.titles[i];
            let title = if title.is_empty() { "Untitled" } else { title.as_str() };
            painter.text(
                *p + Vec2::new(radius(i) + 4.0, 0.0),
                Align2::LEFT_CENTER,
                title,
                FontId::proportional(12.0),
                if dim { pal.text_faint } else { pal.text },
            );
        }
    }
    if hovered.is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        if let Some(h) = hovered {
            go = Some(st.graph.nodes[h].clone());
        }
    }
    (go, rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::theme::Palette;
    use egui_kittest::Harness;
    use notecraft_core::blocks::Block;
    use notecraft_core::Workspace;

    type St = (GraphState, Option<DocId>, egui::Rect);

    fn harness() -> (Harness<'static, St>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::open(dir.path().join("ws")).unwrap();
        let a = w.create_page(None, "Alpha").unwrap();
        let _b = w.create_page(None, "Beta").unwrap();
        let _c = w.create_page(None, "Gamma").unwrap();
        w.pages.get_mut(&a).unwrap().blocks.push(Block::paragraph("see [[Beta]] and [[Gamma]]"));
        let h = Harness::builder().with_accessibility_check(false).build_ui_state(
            |ui, (st, go, rect): &mut St| {
                let (clicked, r) = graph_ui(ui, st, &Palette::light(), Lang::En);
                *rect = r;
                if clicked.is_some() {
                    *go = clicked;
                }
            },
            (GraphState::new(w.link_graph()), None, egui::Rect::NOTHING),
        );
        (h, dir)
    }

    fn node_at(st: &GraphState, rect: egui::Rect, i: usize) -> egui::Pos2 {
        let size = rect.width().min(rect.height()) * 0.86;
        let p = st.pos[i];
        rect.center() + st.pan + Vec2::new((p.0 - 0.5) * size, (p.1 - 0.5) * size) * st.zoom
    }

    #[test]
    fn clicking_a_node_opens_that_page() {
        let (mut h, _d) = harness();
        h.run();
        let (nodes, rect) = (h.state().0.graph.nodes.len(), h.state().2);
        assert_eq!(nodes, 3);
        assert!(rect.width() > 100.0, "the graph fills the window: {rect:?}");
        for i in 0..nodes {
            let want = h.state().0.graph.nodes[i].clone();
            let at = node_at(&h.state().0, rect, i);
            h.state_mut().1 = None;
            h.hover_at(at);
            h.run();
            h.run();
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            h.run();
            assert_eq!(h.state().1, Some(want), "node {i}");
        }
    }

    #[test]
    fn clicking_empty_space_opens_nothing_and_dragging_pans_the_view() {
        let (mut h, _d) = harness();
        h.run();
        let rect = h.state().2;
        // a corner of the area, far from any node
        let corner = rect.left_top() + Vec2::new(3.0, 3.0);
        h.hover_at(corner);
        h.run();
        h.event(egui::Event::PointerButton {
            pos: corner,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.event(egui::Event::PointerButton {
            pos: corner,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        assert_eq!(h.state().1, None);
        // drag by 60 px
        let before = h.state().0.pan;
        h.event(egui::Event::PointerMoved(corner));
        h.event(egui::Event::PointerButton {
            pos: corner,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        for k in 1..=6 {
            h.event(egui::Event::PointerMoved(corner + Vec2::new(10.0 * k as f32, 0.0)));
            h.run();
        }
        h.event(egui::Event::PointerButton {
            pos: corner + Vec2::new(60.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        let moved = h.state().0.pan - before;
        assert!(moved.x > 40.0 && moved.y.abs() < 1.0, "the view followed the drag: {moved:?}");
    }

    #[test]
    fn scrolling_zooms_within_limits() {
        let (mut h, _d) = harness();
        h.run();
        let c = h.state().2.center();
        h.hover_at(c);
        h.run();
        for _ in 0..40 {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, 80.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: egui::Modifiers::NONE,
            });
            h.run();
        }
        assert!(h.state().0.zoom > 1.0 && h.state().0.zoom <= 8.0, "{}", h.state().0.zoom);
        for _ in 0..80 {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -80.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: egui::Modifiers::NONE,
            });
            h.run();
        }
        assert!(h.state().0.zoom >= 0.3 && h.state().0.zoom < 1.0, "{}", h.state().0.zoom);
    }
}
