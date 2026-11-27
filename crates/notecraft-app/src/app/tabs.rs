//! Navigation, history and tabs of the application window (split out of mod.rs, unchanged).

use super::*;

impl NcApp {
    // ------------------------------------------------------------ navigation

    pub fn navigate(&mut self, target: Target) {
        if self.current.as_ref() == Some(&target) {
            return;
        }
        self.flush_save();
        if let Some(cur) = self.current.take() {
            self.back.push(cur);
            if self.back.len() > 100 {
                self.back.remove(0);
            }
        }
        self.forward.clear();
        self.current = Some(target);
        self.editor.reset_for_doc();
        self.show_other_version = false;
        self.snapshot_base();
        self.show_backlinks = false;
        self.icon_picker = false;
        // expand the sidebar down to the page
        let from_fav = matches!((&self.fav_nav_doc, &self.current), (Some(f), Some(Target::Doc(d))) if f == d);
        if !from_fav {
            self.fav_nav_doc = None;
            if let (Some(ws), Some(Target::Doc(d))) = (&self.ws, &self.current) {
                for a in ws.breadcrumbs(d).iter().rev().skip(1) {
                    self.expanded.insert(a.key());
                }
            }
        }
    }

    pub fn go_back(&mut self) {
        if let Some(t) = self.back.pop() {
            self.flush_save();
            if let Some(c) = self.current.take() {
                self.forward.push(c);
            }
            self.current = Some(t);
            self.editor.reset_for_doc();
            self.snapshot_base();
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(t) = self.forward.pop() {
            self.flush_save();
            if let Some(c) = self.current.take() {
                self.back.push(c);
            }
            self.current = Some(t);
            self.editor.reset_for_doc();
            self.snapshot_base();
        }
    }

    // ------------------------------------------------------------------ tabs

    pub(super) fn target_exists(&self, t: &Target) -> bool {
        self.ws.as_ref().is_some_and(|ws| target_is_live(ws, t))
    }

    /// Reopen the tabs of the last session, when they belong to this workspace (several tabs only).
    pub fn restore_tabs(&mut self) {
        let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) else { return };
        if self.prefs.tabs_workspace.as_deref() != Some(root.as_path()) || self.prefs.tabs.len() < 2 {
            return;
        }
        let saved = std::mem::take(&mut self.prefs.tabs);
        let wanted = self.prefs.active_tab.min(saved.len() - 1);
        let mut tabs = Vec::new();
        let mut active = 0;
        for (i, key) in saved.iter().enumerate() {
            if let Some(t) = Target::from_key(key).filter(|t| self.target_exists(t)) {
                if i == wanted {
                    active = tabs.len();
                }
                tabs.push(t);
            }
        }
        if tabs.len() >= 2 {
            self.current = Some(tabs[active].clone());
            self.tabs = tabs;
            self.active_tab = active;
        }
    }

    /// Remember the open tabs for the next launch.
    pub(super) fn remember_tabs(&mut self) {
        self.sync_tabs();
        let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) else { return };
        self.prefs.tabs = if self.tabs.len() > 1 { self.tabs.iter().map(Target::key).collect() } else { Vec::new() };
        self.prefs.active_tab = self.active_tab;
        self.prefs.tabs_workspace = Some(root);
    }

    /// Keep the active tab on the current page and drop tabs whose page is gone.
    pub fn sync_tabs(&mut self) {
        let Some(cur) = self.current.clone() else { return };
        if self.tabs.is_empty() {
            self.tabs.push(cur);
            self.active_tab = 0;
            return;
        }
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        self.tabs[self.active_tab] = cur;
        if self.tabs.len() > 1 {
            let active = self.active_tab;
            let mut new_active = active;
            let mut kept = Vec::new();
            for (i, t) in self.tabs.iter().enumerate() {
                if i == active || self.target_exists(t) {
                    kept.push(t.clone());
                } else if i < active {
                    new_active -= 1;
                }
            }
            self.tabs = kept;
            self.active_tab = new_active;
        }
    }

    /// Open `target` in a new tab next to the active one and switch to it.
    pub fn open_in_new_tab(&mut self, target: Target) {
        self.sync_tabs();
        if self.tabs.len() >= 30 {
            return;
        }
        self.flush_save();
        let at = (self.active_tab + 1).min(self.tabs.len());
        self.tabs.insert(at, target.clone());
        self.active_tab = at;
        // the new tab starts from the page it shows; its own back history is shared
        if self.current.as_ref() != Some(&target) {
            self.navigate(target);
        }
    }

    pub fn switch_tab(&mut self, i: usize) {
        self.sync_tabs();
        if i >= self.tabs.len() || i == self.active_tab {
            return;
        }
        let target = self.tabs[i].clone();
        self.active_tab = i;
        if self.target_exists(&target) {
            self.navigate(target);
        } else {
            self.tabs.remove(i);
            self.active_tab = self.active_tab.min(self.tabs.len().saturating_sub(1));
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        self.sync_tabs();
        if self.tabs.len() < 2 || i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        let new_active = if i < self.active_tab {
            self.active_tab - 1
        } else if i == self.active_tab {
            i.min(self.tabs.len() - 1)
        } else {
            self.active_tab
        };
        let was_active = i == self.active_tab;
        self.active_tab = new_active;
        if was_active {
            let target = self.tabs[new_active].clone();
            self.navigate(target);
        }
    }

    pub fn next_tab(&mut self, step: isize) {
        self.sync_tabs();
        let n = self.tabs.len() as isize;
        if n > 1 {
            self.switch_tab((self.active_tab as isize + step).rem_euclid(n) as usize);
        }
    }

    pub(super) fn tab_label(&self, t: &Target) -> String {
        let Some(ws) = &self.ws else { return String::new() };
        let (icon, title) = match t {
            Target::Doc(d) => (ws.icon(d), ws.title(d)),
            Target::Template(db, id) => (
                "📋".to_string(),
                ws.dbs.get(db).and_then(|d| d.templates.get(id)).map(|r| r.meta.title.clone()).unwrap_or_default(),
            ),
        };
        let title = if title.is_empty() { self.t("untitled").to_string() } else { title };
        let text = if icon.is_empty() { title } else { format!("{icon} {title}") };
        if text.chars().count() > 22 {
            text.chars().take(20).collect::<String>() + "…"
        } else {
            text
        }
    }

    pub(super) fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let labels: Vec<String> = self.tabs.iter().map(|t| self.tab_label(t)).collect();
        let (mut switch, mut close, mut add) = (None, None, false);
        egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                for (i, label) in labels.iter().enumerate() {
                    let active = i == self.active_tab;
                    let fill = if active { pal.active } else { egui::Color32::TRANSPARENT };
                    let color = if active { pal.text } else { pal.text_secondary };
                    let (shown, flag) = crate::flags::reserve(label);
                    let r = ui.add(
                        egui::Button::new(RichText::new(shown).size(13.0).color(color))
                            .fill(fill)
                            .stroke(egui::Stroke::NONE),
                    );
                    if let Some(code) = flag {
                        crate::flags::paint_in_button(ui, &r, code, 13.0);
                    }
                    widgets::a11y(&r, label);
                    if r.clicked() {
                        switch = Some(i);
                    }
                    if r.middle_clicked() {
                        close = Some(i);
                    }
                    let x = widgets::subtle_button(ui, RichText::new("×").size(13.0).color(pal.text_faint), &pal)
                        .on_hover_text(format!("{}  ⌘W", self.t("close_tab")));
                    if x.clicked() {
                        close = Some(i);
                    }
                    ui.add_space(4.0);
                }
                let plus = widgets::subtle_button(ui, RichText::new("+").size(15.0).color(pal.text_secondary), &pal)
                    .on_hover_text(format!("{}  ⌘T", self.t("new_tab")));
                widgets::a11y(&plus, self.t("new_tab"));
                if plus.clicked() {
                    add = true;
                }
            });
        });
        if let Some(i) = close {
            self.close_tab(i);
        } else if let Some(i) = switch {
            self.switch_tab(i);
        }
        if add {
            self.new_tab();
        }
    }

    /// A new tab on the current page, with the page finder open to pick another.
    pub fn new_tab(&mut self) {
        if let Some(t) = self.current.clone() {
            self.open_in_new_tab(t);
            self.finder = Some(Finder::new(FinderMode::Jump));
        }
    }
}
