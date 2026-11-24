//! The menu bar icon (macOS): open the window, start a new page, jump to the quick finder, quit.
//! The window itself is unchanged; this is only a shortcut that is always within reach.

/// What the person picked in the menu bar menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    NewPage,
    Search,
    Quit,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::TrayAction;
    use std::sync::mpsc::{channel, Receiver};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    pub struct Tray {
        _icon: TrayIcon,
        rx: Receiver<TrayAction>,
    }

    /// A line segment (from, to, half thickness).
    type Stroke = ((f32, f32), (f32, f32), f32);

    /// A 36 px monochrome glyph: the app mark (an N in a rounded square, its diagonal rising to the right like the bar of the ø;
    /// the same as the app icon). macOS tints it to match the menu bar.
    fn glyph() -> Icon {
        const S: i32 = 36;
        let mut px = vec![0u8; (S * S * 4) as usize];
        // distance from a point to a segment
        fn dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
            let (vx, vy) = (b.0 - a.0, b.1 - a.1);
            let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
            ((p.0 - (a.0 + t * vx)).powi(2) + (p.1 - (a.1 + t * vy)).powi(2)).sqrt()
        }
        // outline of a rounded square centred at (18, 18): half size 14, corner radius 6, half thickness 1.5
        fn frame(p: (f32, f32)) -> f32 {
            let (half, r) = (14.0_f32, 6.0_f32);
            let q = ((p.0 - 18.0).abs() - (half - r), (p.1 - 18.0).abs() - (half - r));
            let outside = (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt();
            let d = outside + q.0.max(q.1).min(0.0) - r;
            d.abs() - 1.5
        }
        let strokes: [Stroke; 3] =
            [((12.0, 24.0), (12.0, 12.0), 1.8), ((12.0, 24.0), (24.0, 12.0), 1.8), ((24.0, 24.0), (24.0, 12.0), 1.8)];
        for y in 0..S {
            for x in 0..S {
                let p = (x as f32 + 0.5, y as f32 + 0.5);
                let d = strokes.iter().map(|(a, b, w)| dist(p, *a, *b) - w).fold(frame(p), f32::min);
                let alpha = (0.5 - d).clamp(0.0, 1.0);
                if alpha > 0.0 {
                    px[((y * S + x) * 4 + 3) as usize] = (alpha * 255.0) as u8;
                }
            }
        }
        Icon::from_rgba(px, S as u32, S as u32).expect("valid icon")
    }

    impl Tray {
        /// Must be called on the main thread once the event loop runs (eframe's creation callback).
        pub fn new(ctx: &eframe::egui::Context, labels: [&str; 4]) -> Option<Tray> {
            let menu = Menu::new();
            let open = MenuItem::new(labels[0], true, None);
            let new_page = MenuItem::new(labels[1], true, None);
            let search = MenuItem::new(labels[2], true, None);
            let quit = MenuItem::new(labels[3], true, None);
            menu.append_items(&[&open, &new_page, &search, &PredefinedMenuItem::separator(), &quit]).ok()?;
            let ids = [
                (open.id().clone(), TrayAction::Open),
                (new_page.id().clone(), TrayAction::NewPage),
                (search.id().clone(), TrayAction::Search),
                (quit.id().clone(), TrayAction::Quit),
            ];
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if let Some((_, a)) = ids.iter().find(|(id, _)| *id == e.id) {
                    let _ = tx.send(*a);
                    ctx.request_repaint();
                }
            }));
            let icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_icon(glyph())
                .with_icon_as_template(true)
                .with_tooltip("NøteCraft")
                .build()
                .ok()?;
            Some(Tray { _icon: icon, rx })
        }

        pub fn poll(&self) -> Option<TrayAction> {
            self.rx.try_recv().ok()
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::TrayAction;
    pub struct Tray;
    impl Tray {
        pub fn new(_ctx: &eframe::egui::Context, _labels: [&str; 4]) -> Option<Tray> {
            None
        }
        pub fn poll(&self) -> Option<TrayAction> {
            None
        }
    }
}

pub use imp::Tray;
