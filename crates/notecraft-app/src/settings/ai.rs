//! Settings, AI and assistants: engines, models, keys and the MCP connection.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OllamaState {
    Checking,
    Ready,
    NoModel,
    Stopped,
    Missing,
}

/// What an assistant connected through MCP may do (see `docs/mcp.md`).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum McpLevel {
    Readonly,
    /// The server default: read and write, deletion tools stay off unless ticked.
    #[default]
    Standard,
}

#[derive(Clone, Default)]
pub(super) struct McpOptions {
    level: McpLevel,
    delete: bool,
    scope: String,
}

impl NcApp {
    pub(super) fn ollama_state(&mut self) -> OllamaState {
        let active_local = self.prefs.ai.engine == EngineChoice::Ollama;
        let local_url = notecraft_ai::is_local_url(&self.prefs.ai.ollama_url);
        if (active_local || local_url)
            && self.ai.models.is_empty()
            && self.ai.models_loading.is_none()
            && self.ai.models_error.is_none()
        {
            self.refresh_models();
        }
        if self.ai.models_error.is_some() {
            if notecraft_ai::ollama_installed() {
                OllamaState::Stopped
            } else {
                OllamaState::Missing
            }
        } else if self.ai.models_loading.is_some() && self.ai.models.is_empty() {
            OllamaState::Checking
        } else if self.ai.models.is_empty() {
            OllamaState::NoModel
        } else {
            OllamaState::Ready
        }
    }

    pub(super) fn key_status_slot(&mut self, p: keychain::Provider) -> &mut Option<String> {
        match p {
            keychain::Provider::Claude => &mut self.ai.key_status,
            keychain::Provider::OpenAi => &mut self.ai.openai_key_status,
            keychain::Provider::Grok => &mut self.ai.grok_key_status,
        }
    }

    /// What is stored for a cloud service: a masked key, "env" (taken from the environment) or nothing.
    pub(super) fn key_status(&mut self, provider: keychain::Provider) -> String {
        if let Some(s) = self.key_status_slot(provider).clone() {
            return s;
        }
        let s = if keychain::has_key(provider) {
            "stored".to_string()
        } else if keychain::is_available(provider) {
            "env".to_string()
        } else {
            String::new()
        };
        *self.key_status_slot(provider) = Some(s.clone());
        s
    }

    /// The models a cloud service offers: its own list once it answers, a short known one until then.
    pub(super) fn provider_models(&self, p: keychain::Provider) -> Vec<String> {
        let (list, fallback): (&Vec<String>, &[&str]) = match p {
            keychain::Provider::Claude => (&self.ai.claude_models, notecraft_ai::CLAUDE_FALLBACK_MODELS),
            keychain::Provider::OpenAi => (&self.ai.openai_models, notecraft_ai::OPENAI_FALLBACK_MODELS),
            keychain::Provider::Grok => (&self.ai.grok_models, notecraft_ai::GROK_FALLBACK_MODELS),
        };
        if list.is_empty() {
            fallback.iter().map(|m| m.to_string()).collect()
        } else {
            list.clone()
        }
    }

    /// The AI lines. First: where the text goes, as icons (this Mac, Claude, OpenAI, Grok), with the
    /// key of a cloud service. Second: the model, in a small menu.
    pub(super) fn ai_row(&mut self, ui: &mut Ui) {
        use crate::widgets::{icon_action, icon_choice, srow, text_link, Glyph};
        let pal = self.pal;
        let lang = self.prefs.lang;
        let engine = self.prefs.ai.engine;
        let provider = engine.provider();
        let ostate = if provider.is_none() { self.ollama_state() } else { OllamaState::Ready };
        let key = provider.map(|p| self.key_status(p)).unwrap_or_default();
        let (sub, bad) = match provider {
            None => match ostate {
                OllamaState::Ready | OllamaState::Checking => (tr(lang, "ai_local_note"), false),
                OllamaState::NoModel => (tr(lang, "ollama_no_model"), true),
                OllamaState::Stopped => (tr(lang, "ollama_stopped"), true),
                OllamaState::Missing => (tr(lang, "ollama_missing"), true),
            },
            Some(_) if key.is_empty() => (tr(lang, "ai_key_missing"), true),
            Some(_) => (tr(lang, crate::ai_panel::cloud_note_key(engine)), false),
        };
        let mut models: Vec<String> = match provider {
            None => self.ai.models.clone(),
            Some(p) => self.provider_models(p),
        };
        let current = self.prefs.ai.model().to_string();
        if !current.is_empty() && !models.contains(&current) && !models.is_empty() {
            models.insert(0, current.clone());
        }

        let mut pick: Option<EngineChoice> = None;
        let mut key_resp: Option<egui::Response> = None;
        let mut open_key = false;
        srow(ui, &pal, tr(lang, "ai_title"), sub, bad.then_some(pal.danger), |ui| {
            // right to left: the key, then the services from the last to the first
            if provider.is_some() {
                let tint = key.is_empty().then_some(pal.danger);
                let r = icon_action(ui, &pal, Glyph::Key, tr(lang, "ai_key"), tint);
                open_key = r.clicked();
                key_resp = Some(r);
            }
            for (e, g, k) in [
                (EngineChoice::Grok, Glyph::Grok, "ai_grok_short"),
                (EngineChoice::OpenAi, Glyph::OpenAi, "ai_openai_short"),
                (EngineChoice::Claude, Glyph::Sparkle, "ai_claude_short"),
                (EngineChoice::Ollama, Glyph::Laptop, "ai_ollama_short"),
            ] {
                if icon_choice(ui, &pal, g, engine == e, tr(lang, k)).clicked() {
                    pick = Some(e);
                }
            }
        });

        let ready = provider.is_some() || matches!(ostate, OllamaState::Ready | OllamaState::Checking);
        let mut model: Option<String> = None;
        let (mut launch, mut download) = (false, false);
        let local = provider.is_none();
        let can_pick = ready || (local && matches!(ostate, OllamaState::NoModel));
        let mut pull_start: Option<String> = None;
        let mut delete_model: Option<String> = None;
        srow(ui, &pal, tr(lang, "ai_model"), "", None, |ui| {
            if can_pick {
                if !models.is_empty() || local {
                    model = crate::widgets::model_menu(ui, &pal, &current, &models, |ui| {
                        if !local {
                            return;
                        }
                        use std::sync::atomic::Ordering;
                        ui.separator();
                        let busy = self.ai.pull.is_some();
                        for (tag, size) in [
                            ("qwen2.5:7b", "4.7 GB"),
                            ("llama3.2", "2 GB"),
                            ("qwen2.5:3b", "1.9 GB"),
                            ("qwen2.5:1.5b", "1 GB"),
                        ] {
                            if models.iter().any(|m| m == tag || m.starts_with(&format!("{tag}:"))) {
                                continue;
                            }
                            match self.ai.pull.as_ref().filter(|j| j.model == tag) {
                                Some(job) => {
                                    let (d, t) = (job.done.load(Ordering::Relaxed), job.total.load(Ordering::Relaxed));
                                    let pct = (d * 100).checked_div(t).map_or("…".to_string(), |v| format!("{v}%"));
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("{tag} {pct}")).size(13.0).color(pal.text_faint),
                                        );
                                        if text_link(ui, &pal, tr(lang, "cancel")).clicked() {
                                            job.cancel.store(true, Ordering::Relaxed);
                                        }
                                    });
                                }
                                None => {
                                    let label = format!("\u{2193} {tag}  ({size})");
                                    if ui.add_enabled(!busy, egui::Button::new(label).frame(false)).clicked() {
                                        pull_start = Some(tag.to_string());
                                        ui.close();
                                    }
                                }
                            }
                        }
                        if !models.is_empty() {
                            ui.menu_button(tr(lang, "remove"), |ui| {
                                // two clicks: the first asks, the second removes
                                let cid = Id::unique("confirm-delete-model");
                                let pending: Option<String> = ui.data(|d| d.get_temp(cid));
                                for m in &models {
                                    let asked = pending.as_deref() == Some(m.as_str());
                                    let label = if asked {
                                        format!("{} {m}", tr(lang, "model_delete_confirm"))
                                    } else {
                                        m.clone()
                                    };
                                    if ui.button(label).clicked() {
                                        if asked {
                                            delete_model = Some(m.clone());
                                            ui.data_mut(|d| d.remove::<String>(cid));
                                            ui.close();
                                        } else {
                                            ui.data_mut(|d| d.insert_temp(cid, m.clone()));
                                        }
                                    }
                                }
                            });
                        }
                    });
                }
            } else {
                match ostate {
                    OllamaState::Missing => download = text_link(ui, &pal, tr(lang, "ollama_download")).clicked(),
                    OllamaState::Stopped => launch = text_link(ui, &pal, tr(lang, "ollama_launch")).clicked(),
                    _ => {}
                }
            }
        });
        if local {
            // network lock: closed = the local engine only talks to this Mac; open (red) = any address is allowed
            let open = self.prefs.ai.allow_remote_ollama;
            let (title, sub, color) = if open {
                (tr(lang, "ollama_lock_open"), tr(lang, "ollama_lock_open_sub"), Some(pal.danger))
            } else {
                (tr(lang, "ollama_lock_closed"), tr(lang, "ollama_lock_closed_sub"), None)
            };
            let mut toggled = false;
            srow(ui, &pal, title, sub, color, |ui| {
                let (glyph, col) = if open { ("\u{1F513}", pal.danger) } else { ("\u{1F512}", pal.text_secondary) };
                let r = ui.add(egui::Button::new(RichText::new(glyph).size(16.0).color(col)).frame(false));
                toggled = r.on_hover_text(tr(lang, "ollama_lock_tip")).clicked();
            });
            if toggled {
                self.prefs.ai.allow_remote_ollama = !open;
                if open && !notecraft_ai::is_local_url(&self.prefs.ai.ollama_url) {
                    // closing the lock on a remote address: back to this Mac
                    self.prefs.ai.ollama_url = notecraft_ai::DEFAULT_OLLAMA_URL.into();
                }
                self.ai.models_error = None;
                self.refresh_models();
            }
        }
        if local {
            // the model server itself must not be reachable from the network either
            let stale = self.ai.ollama_exposed.is_none_or(|(at, _)| at.elapsed().as_secs() > 60);
            if stale {
                let v = notecraft_ai::ollama_listens_on_network().unwrap_or(false);
                self.ai.ollama_exposed = Some((std::time::Instant::now(), v));
            }
            if self.ai.ollama_exposed.is_some_and(|(_, v)| v) {
                crate::widgets::hint(ui, &pal, tr(lang, "ollama_exposed"));
            }
        }
        {
            // lockdown: nothing leaves this Mac at all, and what the app tried to reach
            let mut lock = self.prefs.ai.network_lockdown;
            srow(ui, &pal, tr(lang, "net_lockdown"), tr(lang, "net_lockdown_sub"), None, |ui| {
                crate::widgets::toggle(ui, &pal, &mut lock);
            });
            if lock != self.prefs.ai.network_lockdown {
                self.prefs.ai.network_lockdown = lock;
                notecraft_net::set_lockdown(lock);
            }
            ui.collapsing(RichText::new(tr(lang, "net_activity")).size(13.0).color(pal.text_secondary), |ui| {
                let events = notecraft_net::recent();
                if events.is_empty() {
                    ui.label(RichText::new(tr(lang, "net_none")).size(12.0).color(pal.text_faint));
                }
                for e in events.iter().take(12) {
                    let (mark, col) = if e.allowed { ("\u{2713}", pal.text_faint) } else { ("\u{2715}", pal.danger) };
                    ui.label(
                        RichText::new(format!("{mark}  {}  ·  {}", e.purpose.label(), e.host)).size(12.0).color(col),
                    );
                }
            });
        }
        if let Some(m) = pull_start {
            self.start_pull(&m);
        }
        if let Some(m) = delete_model {
            let url = self.prefs.ai.ollama_url.clone();
            let _ = notecraft_ai::ollama_delete(&url, &m);
            self.ai.models_error = None;
            self.refresh_models();
        }

        if let Some(e) = pick {
            self.prefs.ai.engine = e;
            match e.provider() {
                Some(p) => {
                    // a service without a key cannot answer: ask for the key right away
                    if self.key_status(p).is_empty() {
                        self.ai.key_open = true;
                    }
                }
                None => {
                    self.ai.models_error = None;
                    self.refresh_models();
                }
            }
        }
        if let Some(m) = model {
            self.prefs.ai.set_model(m);
        }
        if open_key {
            self.ai.key_open = !self.ai.key_open;
        }
        if download {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://ollama.com/download"));
        }
        if launch {
            let _ = std::process::Command::new("open").args(["-a", "Ollama"]).spawn();
            self.ai.models_error = None;
        }

        // the key, in a small box under its icon
        let Some(p) = self.prefs.ai.engine.provider() else {
            self.ai.key_open = false;
            return;
        };
        let Some(anchor) = key_resp else { return };
        if !self.ai.key_open {
            return;
        }
        let (placeholder, env_key, get_url) = match p {
            keychain::Provider::Claude => ("sk-ant-…", "ai_key_env", "https://console.anthropic.com/settings/keys"),
            keychain::Provider::OpenAi => ("sk-…", "ai_key_env_openai", "https://platform.openai.com/api-keys"),
            keychain::Provider::Grok => ("xai-…", "ai_key_env_grok", "https://console.x.ai"),
        };
        let status = self.key_status(p);
        let mut open = true;
        let (mut save, mut remove) = (false, false);
        egui::Popup::from_response(&anchor)
            .id(Id::unique("ai-key"))
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(260.0);
                crate::widgets::menu_label(ui, &pal, tr(lang, "ai_key"));
                ui.add_space(2.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.ai.key_input)
                        .password(true)
                        .desired_width(f32::INFINITY)
                        .hint_text(placeholder),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.ai.key_input.trim().is_empty(), egui::Button::new(tr(lang, "save_keychain")))
                        .clicked()
                    {
                        save = true;
                    }
                    if !status.is_empty() && status != "env" && text_link(ui, &pal, tr(lang, "remove_key")).clicked() {
                        remove = true;
                    }
                });
                let line = if status.is_empty() {
                    tr(lang, "ai_key_missing").to_string()
                } else if status == "env" {
                    tr(lang, env_key).to_string()
                } else {
                    tr(lang, "ai_key_stored").to_string()
                };
                ui.add_space(4.0);
                ui.label(RichText::new(line).size(12.0).color(pal.text_faint));
                ui.hyperlink_to(RichText::new(tr(lang, "ai_key_get")).size(12.0), get_url);
            });
        self.ai.key_open = open;
        if save {
            match keychain::save_key(p, self.ai.key_input.trim()) {
                Ok(()) => {
                    self.ai.key_input.clear();
                    self.ai.key_open = false;
                    *self.key_status_slot(p) = None;
                    self.refresh_models_for(p);
                    self.toast = Some((tr(lang, "ai_key_saved").into(), Instant::now()));
                }
                Err(e) => self.toast = Some((e, Instant::now())),
            }
        }
        if remove {
            let _ = keychain::delete_key(p);
            self.forget_models_for(p);
        }
    }

    // -------------------------------------------------------------- assistants

    /// The MCP server: what it is in one line, two switches, and the two apps it can be connected to.
    pub(super) fn tab_assistants(&mut self, ui: &mut Ui) {
        use crate::widgets::{srow, text_link};
        let pal = self.pal;
        let lang = self.prefs.lang;
        page_title(ui, &pal, tr(lang, "set_assistants"), tr(lang, "set_assistants_sub"));
        let Some(ws) = &self.ws else { return };
        let bin = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("notecraft-mcp")))
            .unwrap_or_else(|| "notecraft-mcp".into());
        let root = ws.root.display().to_string();
        let bin_s = bin.display().to_string();
        let bin_exists = bin.exists();
        let opts_id = egui::Id::unique("mcp-options");
        let mut opts: McpOptions = ui.data_mut(|d| d.get_temp(opts_id)).unwrap_or_default();

        // two switches: may the apps edit, and may they delete
        let mut write = opts.level == McpLevel::Standard;
        srow(ui, &pal, tr(lang, "mcp_write_title"), tr(lang, "mcp_write_hint"), None, |ui| {
            toggle(ui, &pal, &mut write);
        });
        opts.level = if write { McpLevel::Standard } else { McpLevel::Readonly };
        if !write {
            opts.delete = false;
        }
        srow(ui, &pal, tr(lang, "mcp_allow_delete"), tr(lang, "mcp_allow_delete_hint"), None, |ui| {
            ui.add_enabled_ui(write, |ui| {
                toggle(ui, &pal, &mut opts.delete);
            });
        });
        ui.data_mut(|d| d.insert_temp(opts_id, opts.clone()));

        let mut extra: Vec<String> = Vec::new();
        match opts.level {
            McpLevel::Readonly => extra.push("--read-only".into()),
            McpLevel::Standard if opts.delete => extra.push("--allow-delete".into()),
            McpLevel::Standard => {}
        }
        if !opts.scope.trim().is_empty() {
            extra.push("--scope".into());
            extra.push(opts.scope.trim().to_string());
        }
        let flags: String =
            extra.iter().map(|a| if a.starts_with("--") { format!(" {a}") } else { format!(" \"{a}\"") }).collect();
        let code_cmd = format!("claude mcp add --scope user notecraft -- \"{bin_s}\" --workspace \"{root}\"{flags}");
        let mut args = vec!["--workspace".to_string(), root];
        args.extend(extra);

        // Claude Desktop: read the configuration at most once a second
        let now = ui.input(|i| i.time);
        let cache_id = egui::Id::unique("mcp-desktop-state");
        let key = format!("{bin_s}|{}", args.join("|"));
        let cached: Option<(String, DesktopState, f64)> = ui.data(|d| d.get_temp(cache_id));
        let state = match cached {
            Some((k, s, t)) if k == key && now - t < 1.0 => s,
            _ => {
                let s = mcp_setup::state(&bin_s, &args);
                ui.data_mut(|d| d.insert_temp(cache_id, (key.clone(), s, now)));
                s
            }
        };

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(tr(lang, "mcp_apps").to_uppercase()).size(11.0).color(pal.text_faint));
        });
        ui.add_space(2.0);
        let (mut add, mut remove) = (false, false);
        let desktop_sub = match state {
            DesktopState::Connected | DesktopState::Different => tr(lang, "mcp_desktop_connected"),
            DesktopState::NotAdded => tr(lang, "mcp_desktop_notadded"),
            DesktopState::Missing => tr(lang, "mcp_desktop_missing"),
        };
        let can = bin_exists && state != DesktopState::Missing;
        srow(ui, &pal, tr(lang, "mcp_desktop_title"), desktop_sub, None, |ui| match state {
            DesktopState::Connected => remove = text_link(ui, &pal, tr(lang, "mcp_desktop_remove")).clicked(),
            DesktopState::Different => {
                remove = text_link(ui, &pal, tr(lang, "mcp_desktop_remove")).clicked();
                add = text_link(ui, &pal, tr(lang, "mcp_desktop_update")).clicked();
            }
            _ => {
                ui.add_enabled_ui(can, |ui| {
                    add = text_link(ui, &pal, tr(lang, "mcp_connect")).clicked();
                });
            }
        });
        let (mut code_add, mut code_copy) = (false, false);
        let cli = mcp_setup::claude_cli().is_some();
        srow(ui, &pal, tr(lang, "mcp_code_title"), if cli { "" } else { tr(lang, "mcp_code_missing") }, None, |ui| {
            ui.add_enabled_ui(bin_exists && cli, |ui| {
                code_add = text_link(ui, &pal, tr(lang, "mcp_connect")).clicked();
            });
            code_copy = text_link(ui, &pal, tr(lang, "mcp_code_copy")).clicked();
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(10.0);
            let note = if bin_exists {
                tr(lang, "mcp_footer").to_string()
            } else {
                format!("{} cargo build --release -p notecraft-mcp", tr(lang, "mcp_build_first"))
            };
            ui.label(RichText::new(note).size(12.0).color(pal.text_faint));
        });

        if add {
            match mcp_setup::add(&bin_s, &args) {
                Ok(()) => self.notify(tr(lang, "mcp_desktop_done")),
                Err(e) => self.notify(e),
            }
            ui.data_mut(|d| d.remove::<(String, DesktopState, f64)>(cache_id));
        }
        if remove {
            match mcp_setup::remove() {
                Ok(()) => self.notify(tr(lang, "mcp_desktop_removed")),
                Err(e) => self.notify(e),
            }
            ui.data_mut(|d| d.remove::<(String, DesktopState, f64)>(cache_id));
        }
        if code_copy {
            ui.ctx().copy_text(code_cmd.clone());
            self.toast = Some((tr(lang, "copied").into(), Instant::now()));
        }
        if code_add {
            match mcp_setup::add_to_claude_code(&bin_s, &args) {
                Ok(()) => self.notify(tr(lang, "mcp_code_done")),
                Err(e) => self.notify(e),
            }
        }
    }

    // --------------------------------------------------------------- shortcuts

    /// Model lists change (a model is installed, a new one is released): ask again when the page
    /// opens and then at most once a minute while it stays open.
    pub(super) fn ai_poll(&mut self, ui: &mut Ui) {
        // model lists change (a model is installed, a new one is released): ask again when the page
        // opens and then at most once a minute while it stays open
        if self.ai.checked.is_none_or(|t| t.elapsed().as_secs() >= 60) {
            self.ai.checked = Some(Instant::now());
            if self.ai.models_loading.is_none() {
                self.ai.models_error = None;
                self.refresh_models();
            }
            // the cloud lists need the keys: ask once per session, not every minute, so that
            // macOS does not ask for the password again and again
            if !self.ai.cloud_checked {
                self.ai.cloud_checked = true;
                if self.ai.claude_loading.is_none() && keychain::is_available(keychain::Provider::Claude) {
                    self.refresh_claude_models();
                }
                if self.ai.openai_loading.is_none() && keychain::is_available(keychain::Provider::OpenAi) {
                    self.refresh_openai_models();
                }
                if self.ai.grok_loading.is_none() && keychain::is_available(keychain::Provider::Grok) {
                    self.refresh_grok_models();
                }
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
    }
}
