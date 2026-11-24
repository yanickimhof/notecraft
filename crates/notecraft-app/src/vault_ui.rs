//! Encrypted pages: the locked screen, the passphrase window and the menu actions.

use crate::app::NcApp;
use crate::i18n::tr;
use eframe::egui::{self, Id, RichText, Ui};

#[derive(Clone, PartialEq)]
pub enum VaultAsk {
    /// Choose a passphrase (or enter the existing one) to encrypt this page.
    Encrypt(String),
    /// Enter the passphrase to read this page.
    Unlock(String),
}

#[derive(Default)]
pub struct VaultUi {
    pub ask: Option<VaultAsk>,
    pass: String,
    confirm: String,
    error: Option<String>,
    focus: bool,
}

impl VaultUi {
    pub fn open(&mut self, ask: VaultAsk) {
        *self = VaultUi { ask: Some(ask), focus: true, ..Default::default() };
    }
    fn close(&mut self) {
        *self = VaultUi::default();
    }
}

impl NcApp {
    /// Ask to encrypt the page (the menu entry).
    pub fn ask_encrypt(&mut self, id: &str) {
        self.vault.open(VaultAsk::Encrypt(id.to_string()));
    }

    /// Forget the passphrase and close every encrypted page.
    pub fn lock_vault_now(&mut self) {
        self.flush_save();
        if let Some(ws) = self.ws.as_mut() {
            ws.lock_vault();
        }
        self.editor.reset_for_doc();
        self.notify(tr(self.prefs.lang, "vault_locked_now").to_string());
    }

    pub fn remove_encryption(&mut self, id: &str) {
        self.flush_save();
        let res = self.ws.as_mut().map(|w| w.decrypt_page(id));
        match res {
            Some(Err(e)) => self.notify(e.to_string()),
            Some(Ok(())) => self.notify(tr(self.prefs.lang, "vault_removed").to_string()),
            None => {}
        }
    }

    /// What replaces the editor while an encrypted page is locked.
    pub fn locked_page_ui(&mut self, ui: &mut Ui, id: &str) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        // the session passphrase may already be known
        if self.ws.as_mut().is_some_and(|w| w.unlock_page_if_known(id)) {
            return;
        }
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("🔐").size(34.0));
            ui.add_space(6.0);
            ui.label(RichText::new(tr(lang, "vault_page_locked")).size(17.0).strong());
            ui.label(RichText::new(tr(lang, "vault_page_locked_hint")).size(13.0).color(pal.text_secondary));
            ui.add_space(12.0);
            if ui.button(tr(lang, "vault_unlock")).clicked() {
                self.vault.open(VaultAsk::Unlock(id.to_string()));
            }
        });
    }

    pub fn vault_ui(&mut self, ctx: &egui::Context) {
        let Some(ask) = self.vault.ask.clone() else { return };
        let lang = self.prefs.lang;
        let pal = self.pal;
        let (encrypting, id) = match &ask {
            VaultAsk::Encrypt(i) => (true, i.clone()),
            VaultAsk::Unlock(i) => (false, i.clone()),
        };
        let first = encrypting && !self.ws.as_ref().is_some_and(|w| w.has_vault());
        let (mut submit, mut cancel) = (false, false);
        let modal = egui::Modal::new(Id::unique("vault")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new(tr(lang, if encrypting { "vault_encrypt_title" } else { "vault_unlock" }))
                    .size(17.0)
                    .strong(),
            );
            ui.add_space(4.0);
            if first {
                ui.label(RichText::new(tr(lang, "vault_warning")).size(12.5).color(pal.danger));
            } else if encrypting {
                ui.label(RichText::new(tr(lang, "vault_same_pass")).size(12.5).color(pal.text_secondary));
            }
            ui.add_space(8.0);
            let pass = egui::TextEdit::singleline(&mut self.vault.pass)
                .password(true)
                .hint_text(tr(lang, "vault_passphrase"))
                .desired_width(f32::INFINITY);
            let r = ui.add(pass);
            if self.vault.focus {
                r.request_focus();
                self.vault.focus = false;
            }
            let mut enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if first {
                let r2 = ui.add(
                    egui::TextEdit::singleline(&mut self.vault.confirm)
                        .password(true)
                        .hint_text(tr(lang, "vault_confirm"))
                        .desired_width(f32::INFINITY),
                );
                enter |= r2.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            }
            if let Some(e) = &self.vault.error {
                ui.add_space(4.0);
                ui.label(RichText::new(e).size(12.5).color(pal.danger));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let label = tr(lang, if encrypting { "vault_encrypt" } else { "vault_unlock" });
                if ui.button(label).clicked() || enter {
                    submit = true;
                }
                if ui.button(tr(lang, "cancel")).clicked() {
                    cancel = true;
                }
            });
        });
        if cancel || modal.should_close() {
            self.vault.close();
            return;
        }
        if !submit {
            return;
        }
        let pass = self.vault.pass.clone();
        if first && pass != self.vault.confirm {
            self.vault.error = Some(tr(lang, "vault_mismatch").to_string());
            return;
        }
        if encrypting {
            self.flush_save();
        }
        let res =
            self.ws.as_mut().map(|w| if encrypting { w.encrypt_page(&id, &pass) } else { w.unlock_page(&id, &pass) });
        match res {
            Some(Ok(())) => {
                self.vault.close();
                self.editor.reset_for_doc();
                if encrypting {
                    self.notify(tr(lang, "vault_encrypted").to_string());
                }
            }
            Some(Err(e)) => {
                self.vault.error = Some(if encrypting { e.to_string() } else { tr(lang, "vault_wrong").to_string() });
            }
            None => self.vault.close(),
        }
    }
}
