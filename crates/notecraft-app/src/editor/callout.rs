//! The callout block: icon, colour and its children.

use super::*;

impl EditorState {
    pub(super) fn callout_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        cx: &BlockCx,
        sink: &mut Sink,
        icon: &str,
    ) -> egui::Response {
        let (i, width, env) = (cx.i, cx.width, cx.env);
        let deferred = &mut *sink.deferred;
        let out = &mut *sink.out;
        let pal = env.pal;
        let bid = blocks[i].id;
        let has_kids = blocks.get(i + 1).is_some_and(|b| b.depth > blocks[i].depth);
        let fr = egui::Frame::NONE
            .fill(pal.callout_bg)
            .corner_radius(if has_kids { CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 } } else { CornerRadius::same(6) })
            .inner_margin(egui::Margin { left: 12, right: 16, top: 12, bottom: if has_kids { 4 } else { 12 } })
            .show(ui, |ui| {
                ui.set_width(width - 28.0);
                ui.horizontal_top(|ui| {
                    let ir = ui.add(egui::Button::new(RichText::new(icon).size(19.0)).min_size(Vec2::new(28.0, 24.0)));
                    if ir.clicked() {
                        self.icon_menu = Some(bid);
                    }
                    if self.icon_menu == Some(bid) {
                        let mut chosen = None;
                        let a = egui::Area::new(Id::unique(("nc-callout-icon", bid)))
                            .order(egui::Order::Foreground)
                            .fixed_pos(ir.rect.left_bottom())
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    chosen = widgets::emoji_grid(ui, pal, "", false);
                                });
                            });
                        if let Some(e) = chosen {
                            deferred.push(Deferred::SetCalloutIcon(i, e));
                            self.icon_menu = None;
                        } else if a.response.clicked_elsewhere() && !ir.clicked() {
                            self.icon_menu = None;
                        }
                    }
                    let w = ui.available_width();
                    self.rich_text(ui, blocks, i, w, env, out);
                });
            });
        fr.response
    }
}
