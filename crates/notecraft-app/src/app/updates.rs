//! The update check: once per launch, after the first launch assistant, when it is switched on.

use super::NcApp;

impl NcApp {
    /// Starts the check when it is due and picks up its answer. Silent when anything fails.
    pub fn poll_update(&mut self, ctx: &eframe::egui::Context) {
        if self.update_rx.is_none() && !self.update_done {
            if !(self.prefs.check_updates && self.prefs.onboarded) || notecraft_net::lockdown() {
                return;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(notecraft_ai::update::latest_release());
            });
            self.update_rx = Some(rx);
        }
        let Some(rx) = &self.update_rx else { return };
        match rx.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(500))
            }
            Ok(found) => {
                self.update_rx = None;
                self.update_done = true;
                self.update_found =
                    found.filter(|r| notecraft_ai::update::is_newer(notecraft_core::version(), &r.version));
            }
            Err(_) => {
                self.update_rx = None;
                self.update_done = true;
            }
        }
    }

    /// Asks again (the "Check now" button).
    pub fn recheck_update(&mut self) {
        self.update_done = false;
        self.update_found = None;
    }
}
