//! The AI panel (✨, ⌘J): summarize, continue, improve, translate, ask —
//! on the selection or the whole page. The answer is a draft the person
//! accepts or discards; nothing is applied silently. A clear note says
//! whether text stays on this Mac (Ollama) or leaves it (Claude or OpenAI API).

use crate::app::{NcApp, Target};
use crate::i18n::tr;
use crate::widgets;
use eframe::egui::{self, Color32, CornerRadius, Id, RichText, Vec2};
use notecraft_ai::{build_prompt, keychain, AiAction, AiError, Engine, Timeouts};
use notecraft_core::blocks::{self, Block};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EngineChoice {
    #[default]
    Ollama,
    Claude,
    OpenAi,
    Grok,
}

/// The i18n key of the "your text is sent to …" note for this engine.
pub fn cloud_note_key(e: EngineChoice) -> &'static str {
    match e {
        EngineChoice::OpenAi => "ai_cloud_note_openai",
        EngineChoice::Grok => "ai_cloud_note_grok",
        _ => "ai_cloud_note",
    }
}

impl EngineChoice {
    /// The service whose key this engine needs (none for Ollama).
    pub fn provider(self) -> Option<keychain::Provider> {
        match self {
            EngineChoice::Ollama => None,
            EngineChoice::Claude => Some(keychain::Provider::Claude),
            EngineChoice::OpenAi => Some(keychain::Provider::OpenAi),
            EngineChoice::Grok => Some(keychain::Provider::Grok),
        }
    }

    /// Short name shown next to the cloud note.
    pub fn name(self) -> &'static str {
        match self {
            EngineChoice::Ollama => "Ollama",
            EngineChoice::Claude => "Claude",
            EngineChoice::OpenAi => "ChatGPT",
            EngineChoice::Grok => "Grok",
        }
    }
}

/// Saved with the app's preferences. The API keys are NOT here (Keychain only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiPrefs {
    pub engine: EngineChoice,
    pub ollama_url: String,
    pub ollama_model: String,
    pub claude_model: String,
    pub openai_model: String,
    pub grok_model: String,
    /// Allows an Ollama that is not on this Mac (only by editing the preferences file).
    pub allow_remote_ollama: bool,
    /// Lockdown: no connection leaves this Mac (cloud AI, downloads and publishing are refused).
    pub network_lockdown: bool,
}

impl Default for AiPrefs {
    fn default() -> Self {
        AiPrefs {
            engine: EngineChoice::Ollama,
            ollama_url: notecraft_ai::DEFAULT_OLLAMA_URL.into(),
            ollama_model: String::new(),
            claude_model: notecraft_ai::DEFAULT_CLAUDE_MODEL.into(),
            openai_model: notecraft_ai::DEFAULT_OPENAI_MODEL.into(),
            grok_model: notecraft_ai::DEFAULT_GROK_MODEL.into(),
            allow_remote_ollama: false,
            network_lockdown: false,
        }
    }
}

impl AiPrefs {
    /// The engine, with the key read from the Keychain for the cloud ones.
    pub fn engine(&self) -> Engine {
        match self.engine {
            EngineChoice::Ollama => Engine::Ollama { url: self.ollama_url.clone(), model: self.ollama_model.clone() },
            EngineChoice::Claude => Engine::Claude {
                url: notecraft_ai::DEFAULT_CLAUDE_URL.into(),
                model: self.claude_model.clone(),
                api_key: keychain::load_api_key().unwrap_or_default(),
            },
            EngineChoice::OpenAi => Engine::OpenAi {
                url: notecraft_ai::DEFAULT_OPENAI_URL.into(),
                model: self.openai_model.clone(),
                api_key: keychain::load_key(keychain::Provider::OpenAi).unwrap_or_default(),
            },
            EngineChoice::Grok => Engine::Grok {
                url: notecraft_ai::DEFAULT_GROK_URL.into(),
                model: self.grok_model.clone(),
                api_key: keychain::load_key(keychain::Provider::Grok).unwrap_or_default(),
            },
        }
    }

    /// Choose the model of the active engine.
    pub fn set_model(&mut self, m: String) {
        match self.engine {
            EngineChoice::Ollama => self.ollama_model = m,
            EngineChoice::Claude => self.claude_model = m,
            EngineChoice::OpenAi => self.openai_model = m,
            EngineChoice::Grok => self.grok_model = m,
        }
    }

    /// The model chosen for the active engine.
    pub fn model(&self) -> &str {
        match self.engine {
            EngineChoice::Ollama => &self.ollama_model,
            EngineChoice::Claude => &self.claude_model,
            EngineChoice::OpenAi => &self.openai_model,
            EngineChoice::Grok => &self.grok_model,
        }
    }

    pub fn leaves_this_mac(&self) -> bool {
        match self.engine {
            EngineChoice::Claude | EngineChoice::OpenAi | EngineChoice::Grok => true,
            EngineChoice::Ollama => !notecraft_ai::is_local_url(&self.ollama_url),
        }
    }
}

/// What the action works on.
#[derive(Debug, Clone)]
pub struct AiContext {
    pub target: Target,
    /// (block, start char, end char, selected text)
    pub selection: Option<(u64, usize, usize, String)>,
}

pub struct Draft {
    pub action: AiAction,
    pub text: String,
    pub ctx: AiContext,
    /// Whole-page review: the page as it will be after accepting, and the Markdown it was made from.
    pub review: Option<(Vec<Block>, String)>,
    /// The page Markdown as sent (kept elements shown as a note), for the "original" view.
    pub original: String,
    pub warning: Option<String>,
}

/// A request in flight: what was asked, where the answer will arrive, and when it started.
pub type RunningAi = (AiAction, Receiver<Result<String, AiError>>, Instant);

/// A model download: name, bytes done and total (shared with the thread), and the final result.
pub struct PullJob {
    pub model: String,
    pub done: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub total: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub rx: Receiver<Result<(), AiError>>,
}

#[derive(Default)]
pub struct AiState {
    /// The key box under the AI line of the settings is open.
    pub key_open: bool,
    pub open: bool,
    pub question: String,
    pub ctx: Option<AiContext>,
    pub running: Option<RunningAi>,
    pub draft: Option<Draft>,
    /// Review in flight: the elements left out of the request, and the Markdown sent.
    pub review_kept: Vec<Vec<Block>>,
    pub review_md: String,
    pub show_original: bool,
    pub error: Option<String>,
    /// Installed Ollama models (listed, never downloaded).
    pub models: Vec<String>,
    pub models_loading: Option<Receiver<Result<Vec<String>, AiError>>>,
    pub models_error: Option<String>,
    /// Ollama answers on every interface, so other machines could reach it (checked now and then).
    pub ollama_exposed: Option<(std::time::Instant, bool)>,
    /// A local model being downloaded through Ollama.
    pub pull: Option<PullJob>,
    pub key_input: String,
    pub key_status: Option<String>,
    pub openai_key_status: Option<String>,
    /// Models the Claude API offers to the saved key.
    pub claude_models: Vec<String>,
    pub claude_loading: Option<Receiver<Result<Vec<String>, AiError>>>,
    pub claude_error: Option<String>,
    /// Same for the OpenAI API.
    pub openai_models: Vec<String>,
    pub openai_loading: Option<Receiver<Result<Vec<String>, AiError>>>,
    pub openai_error: Option<String>,
    /// Same for the xAI API (Grok).
    pub grok_key_status: Option<String>,
    pub grok_models: Vec<String>,
    pub grok_loading: Option<Receiver<Result<Vec<String>, AiError>>>,
    pub grok_error: Option<String>,
    /// When both lists were last asked for (they are asked again when the AI page of Settings opens).
    pub checked: Option<Instant>,
    /// The cloud model lists were asked for once in this session (each ask reads a Keychain item).
    pub cloud_checked: bool,
}

impl NcApp {
    /// ✨ in the top bar, with a cloud note when the Claude API is active.
    pub fn ai_button(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let lang = self.prefs.lang;
        let r = widgets::subtle_button(ui, RichText::new("✨").size(15.0), &pal)
            .on_hover_text(format!("{}  ⌘J", tr(lang, "a11y_ai")));
        widgets::a11y(&r, tr(lang, "a11y_ai"));
        if r.clicked() {
            if self.ai.open {
                self.ai.open = false;
            } else {
                self.open_ai();
            }
        }
        if self.prefs.ai.leaves_this_mac() {
            ui.label(
                RichText::new(format!("☁ {}", self.prefs.ai.engine.name()))
                    .size(12.0)
                    .color(pal.option_tokens("orange").2),
            )
            .on_hover_text(tr(lang, cloud_note_key(self.prefs.ai.engine)));
        }
    }

    /// Open the panel, capturing the current selection (if any).
    pub fn open_ai(&mut self) {
        let Some(target) = self.current.clone() else { return };
        if matches!(target, Target::Doc(notecraft_core::DocId::Database(_))) {
            return;
        }
        let mut selection = None;
        if let (Some((bid, a, b)), Some(ws)) = (self.editor.last_selection, self.ws.as_mut()) {
            if let Some(blocks) = NcApp::blocks_mut(ws, &target) {
                if let Some(bl) = blocks.iter().find(|x| x.id == bid) {
                    let text: String = bl.text.chars().skip(a).take(b - a).collect();
                    let plain = notecraft_core::inline::strip(&text);
                    if !plain.trim().is_empty() {
                        selection = Some((bid, a, b, plain));
                    }
                }
            }
        }
        self.ai.ctx = Some(AiContext { target, selection });
        self.ai.open = true;
        self.ai.error = None;
        if self.ai.models.is_empty() && self.prefs.ai.engine == EngineChoice::Ollama {
            self.refresh_models();
        }
    }

    /// Download `model` into Ollama; the list is asked again when it is done.
    pub fn start_pull(&mut self, model: &str) {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;
        if self.ai.pull.is_some() {
            return;
        }
        let url = self.prefs.ai.ollama_url.clone();
        let (done, total, cancel) =
            (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
        let (d2, t2, c2) = (done.clone(), total.clone(), cancel.clone());
        let name = model.to_string();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = notecraft_ai::ollama_pull(&url, &name, |d, t| {
                d2.store(d, Ordering::Relaxed);
                t2.store(t, Ordering::Relaxed);
                !c2.load(Ordering::Relaxed)
            });
            let _ = tx.send(r);
        });
        self.ai.models_error = None;
        self.ai.pull = Some(PullJob { model: model.to_string(), done, total, cancel, rx });
    }

    pub fn refresh_models(&mut self) {
        let url = self.prefs.ai.ollama_url.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let t = Timeouts { connect: std::time::Duration::from_secs(2), total: std::time::Duration::from_secs(5) };
            let _ = tx.send(notecraft_ai::ollama_models(&url, t));
        });
        self.ai.models_loading = Some(rx);
    }

    /// Ask the Claude API which models this key can use.
    pub fn refresh_claude_models(&mut self) {
        let Some(key) = notecraft_ai::keychain::load_api_key() else {
            self.ai.claude_models.clear();
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let t = Timeouts { connect: std::time::Duration::from_secs(4), total: std::time::Duration::from_secs(10) };
            let _ = tx.send(notecraft_ai::claude_models(notecraft_ai::DEFAULT_CLAUDE_URL, &key, t));
        });
        self.ai.claude_loading = Some(rx);
    }

    /// Ask the OpenAI API which models this key can use.
    pub fn refresh_openai_models(&mut self) {
        let Some(key) = keychain::load_key(keychain::Provider::OpenAi) else {
            self.ai.openai_models.clear();
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let t = Timeouts { connect: std::time::Duration::from_secs(4), total: std::time::Duration::from_secs(10) };
            let _ = tx.send(notecraft_ai::openai_models(notecraft_ai::DEFAULT_OPENAI_URL, &key, t));
        });
        self.ai.openai_loading = Some(rx);
    }

    /// Ask the xAI API which models this key can use.
    pub fn refresh_grok_models(&mut self) {
        let Some(key) = keychain::load_key(keychain::Provider::Grok) else {
            self.ai.grok_models.clear();
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let t = Timeouts { connect: std::time::Duration::from_secs(4), total: std::time::Duration::from_secs(10) };
            let _ = tx.send(notecraft_ai::grok_models(notecraft_ai::DEFAULT_GROK_URL, &key, t));
        });
        self.ai.grok_loading = Some(rx);
    }

    /// Ask a cloud service again which models it offers.
    pub fn refresh_models_for(&mut self, p: keychain::Provider) {
        match p {
            keychain::Provider::Claude => self.refresh_claude_models(),
            keychain::Provider::OpenAi => self.refresh_openai_models(),
            keychain::Provider::Grok => self.refresh_grok_models(),
        }
    }

    /// Forget what is known of a service (its key was removed).
    pub fn forget_models_for(&mut self, p: keychain::Provider) {
        match p {
            keychain::Provider::Claude => {
                self.ai.key_status = None;
                self.ai.claude_models.clear();
                self.ai.claude_error = None;
            }
            keychain::Provider::OpenAi => {
                self.ai.openai_key_status = None;
                self.ai.openai_models.clear();
                self.ai.openai_error = None;
            }
            keychain::Provider::Grok => {
                self.ai.grok_key_status = None;
                self.ai.grok_models.clear();
                self.ai.grok_error = None;
            }
        }
    }

    /// Solve the equation in block `bid`: on this Mac when the local solver can, otherwise with the AI.
    pub fn solve_equation(&mut self, bid: u64) {
        let Some(target) = self.current.clone() else { return };
        let fr = self.prefs.lang == crate::i18n::Lang::Fr;
        let title = self.t("solve_steps").to_string();
        let result = {
            let Some(ws) = self.ws.as_mut() else { return };
            let Some(blocks) = NcApp::blocks_mut(ws, &target) else { return };
            let Some(i) = blocks.iter().position(|b| b.id == bid) else { return };
            let src = blocks[i].text.clone();
            match notecraft_core::solve::solve(&src, fr) {
                Ok(steps) => {
                    let before = blocks.clone();
                    let d = blocks[i].depth;
                    let mut new =
                        vec![Block::new(notecraft_core::BlockKind::Toggle { open: true }, title).with_depth(d)];
                    for (n, s) in steps.iter().skip(1).enumerate() {
                        new.push(
                            Block::new(notecraft_core::BlockKind::Paragraph, format!("{}. {}", n + 1, s.label))
                                .with_depth(d + 1),
                        );
                        new.push(Block::new(notecraft_core::BlockKind::Equation, s.latex.clone()).with_depth(d + 1));
                    }
                    let at = blocks::subtree_end(blocks, i);
                    for (k, b) in new.into_iter().enumerate() {
                        blocks.insert(at + k, b);
                    }
                    Ok((before, src))
                }
                Err(e) => Err((e, src)),
            }
        };
        match result {
            Ok((before, _)) => {
                self.editor.record(&target.key(), &before, None);
                self.mark_dirty();
            }
            Err((notecraft_core::solve::SolveError::NotAnEquation, _)) => {
                let m = self.t("solve_need_equation");
                self.notify(m);
            }
            Err((_, src)) => {
                let m = self.t("solve_ask_ai");
                self.notify(m);
                let len = src.chars().count();
                self.ai.ctx = Some(AiContext { target, selection: Some((bid, 0, len, src)) });
                self.ai.open = true;
                self.start_ai(AiAction::Solve(if fr { "French" } else { "English" }.into()));
            }
        }
    }

    fn start_ai(&mut self, action: AiAction) {
        let Some(ctx) = self.ai.ctx.clone() else { return };
        let Some(ws) = self.ws.as_mut() else { return };
        let title = match &ctx.target {
            Target::Doc(d) => ws.title(d),
            Target::Template(db, t) => {
                ws.dbs.get(db).and_then(|d| d.templates.get(t)).map(|r| r.meta.title.clone()).unwrap_or_default()
            }
        };
        let page = NcApp::blocks_mut(ws, &ctx.target).map(|b| blocks::to_markdown(b)).unwrap_or_default();
        let whole_review = action.is_review() && ctx.selection.is_none();
        let mut prompts: Vec<notecraft_ai::Prompt> = Vec::new();
        if whole_review {
            let prepared = NcApp::blocks_mut(ws, &ctx.target).map(|b| crate::ai_review::prepare(b));
            let Some(prep) = prepared else { return };
            if prep.markdown.chars().count() > notecraft_ai::MAX_CHARS {
                self.ai.error = Some(tr(self.prefs.lang, "ai_review_too_long").to_string());
                return;
            }
            // proofreading goes piece by piece (small models lose text on long inputs); restructuring needs the whole page
            let pieces = if action == AiAction::Restructure {
                vec![prep.markdown.clone()]
            } else {
                crate::ai_review::chunks(&prep.markdown, 5000)
            };
            for piece in &pieces {
                prompts.push(build_prompt(&action, &title, piece, None));
            }
            self.ai.review_md = prep.markdown;
            self.ai.review_kept = prep.kept;
        } else {
            prompts.push(build_prompt(&action, &title, &page, ctx.selection.as_ref().map(|s| s.3.as_str())));
            self.ai.review_md.clear();
            self.ai.review_kept.clear();
        }
        let mut engine = self.prefs.ai.engine();
        let installed = self.ai.models.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            // no model chosen yet: use the first one already installed (never download)
            if let Engine::Ollama { url, model } = &mut engine {
                if model.trim().is_empty() {
                    let list = if installed.is_empty() {
                        notecraft_ai::ollama_models(url, Timeouts::default()).unwrap_or_default()
                    } else {
                        installed
                    };
                    match notecraft_ai::pick_chat_model(&list) {
                        Some(m) => *model = m,
                        None => {
                            let _ = tx.send(Err(AiError::ModelMissing { model: "—".into(), installed: vec![] }));
                            return;
                        }
                    }
                }
            }
            let mut parts: Vec<String> = Vec::new();
            for prompt in &prompts {
                match notecraft_ai::complete_with_retry(&engine, prompt, Timeouts::default()) {
                    Ok(t) => parts.push(t.trim().to_string()),
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
            }
            let _ = tx.send(Ok(parts.join("\n\n")));
        });
        self.ai.error = None;
        self.ai.draft = None;
        self.ai.running = Some((action, rx, Instant::now()));
    }

    /// Collect answers from the background threads.
    pub fn poll_ai(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.ai.pull {
            if let Ok(r) = job.rx.try_recv() {
                let name = job.model.clone();
                self.ai.pull = None;
                match r {
                    Ok(()) => {
                        self.prefs.ai.ollama_model = name;
                        self.ai.models_error = None;
                        self.refresh_models();
                    }
                    Err(e) => self.ai.models_error = Some(e.to_string()),
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
        if let Some(rx) = &self.ai.models_loading {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(list) => {
                        if self.prefs.ai.ollama_model.is_empty() {
                            if let Some(m) = notecraft_ai::pick_chat_model(&list) {
                                self.prefs.ai.ollama_model = m;
                            }
                        }
                        self.ai.models_error = None;
                        self.ai.models = list;
                    }
                    Err(e) => self.ai.models_error = Some(e.to_string()),
                }
                self.ai.models_loading = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        if let Some(rx) = &self.ai.claude_loading {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(list) => {
                        self.ai.claude_error = None;
                        self.ai.claude_models = list;
                    }
                    Err(e) => self.ai.claude_error = Some(e.to_string()),
                }
                self.ai.claude_loading = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        if let Some(rx) = &self.ai.openai_loading {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(list) => {
                        self.ai.openai_error = None;
                        self.ai.openai_models = list;
                    }
                    Err(e) => self.ai.openai_error = Some(e.to_string()),
                }
                self.ai.openai_loading = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        if let Some(rx) = &self.ai.grok_loading {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(list) => {
                        self.ai.grok_error = None;
                        self.ai.grok_models = list;
                    }
                    Err(e) => self.ai.grok_error = Some(e.to_string()),
                }
                self.ai.grok_loading = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        let Some((action, rx, _)) = &self.ai.running else { return };
        match rx.try_recv() {
            Ok(Ok(text)) => {
                let action = action.clone();
                let c = self.ai.ctx.clone();
                self.ai.running = None;
                if let Some(c) = c {
                    let mut draft = Draft {
                        action: action.clone(),
                        text,
                        ctx: c,
                        review: None,
                        original: String::new(),
                        warning: None,
                    };
                    if action.is_review() && !self.ai.review_md.is_empty() {
                        let lang = self.prefs.lang;
                        let kept = std::mem::take(&mut self.ai.review_kept);
                        let md = self.ai.review_md.clone();
                        let old =
                            self.ws.as_mut().and_then(|ws| NcApp::blocks_mut(ws, &draft.ctx.target).map(|b| b.clone()));
                        match old.and_then(|o| crate::ai_review::restore(&o, &kept, &draft.text)) {
                            Some(new) => {
                                if crate::ai_review::numbers(&md) != crate::ai_review::numbers(&draft.text) {
                                    draft.warning = Some(tr(lang, "ai_numbers_differ").to_string());
                                }
                                draft.original = crate::ai_review::readable(&md, kept.len());
                                draft.text = crate::ai_review::readable(&draft.text, kept.len());
                                draft.review = Some((new, md));
                            }
                            None => {
                                self.ai.error = Some(tr(lang, "ai_review_lost").to_string());
                                return;
                            }
                        }
                        self.ai.show_original = false;
                    }
                    self.ai.draft = Some(draft);
                }
            }
            Ok(Err(e)) => {
                self.ai.running = None;
                self.ai.error = Some(e.to_string());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(120))
            }
            Err(_) => {
                self.ai.running = None;
                self.ai.error = Some("the AI request stopped unexpectedly".into());
            }
        }
    }

    /// Apply the accepted draft: insert it after the selection (or at the end),
    /// or replace the selected text.
    fn accept_draft(&mut self, replace: bool) {
        let Some(d) = self.ai.draft.take() else { return };
        let key = d.ctx.target.key();
        let lang = self.prefs.lang;
        let Some(ws) = self.ws.as_mut() else { return };
        let Some(blocks) = NcApp::blocks_mut(ws, &d.ctx.target) else { return };
        let before = blocks.clone();
        if let Some((new, sent)) = d.review {
            // the page must be the one the AI worked on
            if crate::ai_review::prepare(blocks).markdown != sent {
                self.ai.error = Some(tr(lang, "ai_review_changed").to_string());
                return;
            }
            *blocks = new;
            self.editor.record(&key, &before, None);
            if self.current.as_ref() == Some(&d.ctx.target) {
                self.mark_dirty();
            } else if let Target::Doc(doc) = &d.ctx.target {
                let _ = ws.save(doc);
            }
            self.ai.open = false;
            return;
        }
        let mut new = blocks::from_markdown(&d.text);
        if new.is_empty() {
            return;
        }
        match (&d.ctx.selection, replace) {
            (Some((bid, a, b, _)), true) => {
                if let Some(i) = blocks.iter().position(|x| x.id == *bid) {
                    let first = new.remove(0);
                    let text = &blocks[i].text;
                    let start = notecraft_core::inline::char_offset_to_byte(text, *a);
                    let end = notecraft_core::inline::char_offset_to_byte(text, *b);
                    let replaced = format!("{}{}{}", &text[..start], first.text, &text[end..]);
                    blocks[i].text = replaced;
                    let at = blocks::subtree_end(blocks, i);
                    let depth = blocks[i].depth;
                    for (k, b) in new.into_iter().enumerate() {
                        blocks.insert(at + k, Block { depth: depth + b.depth, ..b });
                    }
                }
            }
            (Some((bid, ..)), false) => {
                let at = blocks
                    .iter()
                    .position(|x| x.id == *bid)
                    .map(|i| blocks::subtree_end(blocks, i))
                    .unwrap_or(blocks.len());
                for (k, b) in new.into_iter().enumerate() {
                    blocks.insert(at + k, b);
                }
            }
            (None, _) => {
                // drop a trailing empty paragraph so the answer follows the text
                if blocks.last().is_some_and(|b| b.kind == notecraft_core::BlockKind::Paragraph && b.text.is_empty()) {
                    blocks.pop();
                }
                blocks.extend(new);
            }
        }
        self.editor.record(&key, &before, None);
        if self.current.as_ref() == Some(&d.ctx.target) {
            self.mark_dirty();
        } else if let Target::Doc(doc) = &d.ctx.target {
            let _ = ws.save(doc);
        }
        self.ai.open = false;
    }

    pub fn ai_panel_ui(&mut self, ctx: &egui::Context) {
        if !self.ai.open {
            return;
        }
        let pal = self.pal;
        let lang = self.prefs.lang;
        let screen = ctx.content_rect();
        let width = 440.0f32.min(screen.width() - 40.0);
        let mut close = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let mut action: Option<AiAction> = None;
        let mut accept: Option<bool> = None;
        let mut discard = false;
        let engine_label = match self.prefs.ai.engine {
            EngineChoice::Ollama => format!(
                "Ollama · {}",
                if self.prefs.ai.ollama_model.is_empty() { "…" } else { &self.prefs.ai.ollama_model }
            ),
            EngineChoice::Claude | EngineChoice::OpenAi | EngineChoice::Grok => {
                format!("{} · {}", self.prefs.ai.engine.name(), self.prefs.ai.model())
            }
        };
        let cloud = self.prefs.ai.leaves_this_mac();
        egui::Area::new(Id::unique("ai-panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.max.x - width - 16.0, screen.min.y + 50.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("✨  {}", tr(lang, "a11y_ai"))).size(15.0).color(pal.text));
                            ui.label(RichText::new(engine_label).size(12.0).color(pal.text_faint));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if widgets::icon_button(ui, "×", tr(lang, "close")).clicked() {
                                    close = true;
                                }
                            });
                        });
                        // where the text goes
                        let (note, color) = if cloud {
                            (tr(lang, cloud_note_key(self.prefs.ai.engine)), pal.option_tokens("orange").1)
                        } else {
                            (tr(lang, "ai_local_note"), pal.option_tokens("green").1)
                        };
                        egui::Frame::NONE
                            .fill(if cloud { pal.option_colors("orange").0 } else { pal.option_colors("green").0 })
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                ui.set_width(width - 16.0);
                                ui.label(RichText::new(note).size(12.5).color(color));
                            });
                        ui.add_space(6.0);
                        if let Some(d) = &self.ai.draft {
                            ui.label(RichText::new(tr(lang, "ai_draft")).size(12.0).color(pal.text_secondary));
                            egui::Frame::NONE
                                .fill(pal.code_bg)
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(width - 20.0);
                                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                        let shown = if self.ai.show_original && !d.original.is_empty() {
                                            &d.original
                                        } else {
                                            &d.text
                                        };
                                        draft_view(ui, &pal, shown, width - 40.0);
                                    });
                                });
                            if let Some(w) = &d.warning {
                                ui.add_space(4.0);
                                ui.label(RichText::new(w).size(12.5).color(pal.danger));
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let primary = |t: &str| {
                                    egui::Button::new(RichText::new(t).color(Color32::WHITE)).fill(pal.accent)
                                };
                                if d.review.is_some() {
                                    if ui.add(primary(tr(lang, "ai_replace_page"))).clicked() {
                                        accept = Some(true);
                                    }
                                    let label =
                                        if self.ai.show_original { "ai_show_draft" } else { "ai_show_original" };
                                    if ui.button(tr(lang, label)).clicked() {
                                        self.ai.show_original = !self.ai.show_original;
                                    }
                                } else if d.ctx.selection.is_some() && d.action.replaces() {
                                    if ui.add(primary(tr(lang, "ai_replace"))).clicked() {
                                        accept = Some(true);
                                    }
                                    if ui.button(tr(lang, "ai_insert")).clicked() {
                                        accept = Some(false);
                                    }
                                } else if ui.add(primary(tr(lang, "ai_insert"))).clicked() {
                                    accept = Some(false);
                                }
                                if ui.button(tr(lang, "copy")).clicked() {
                                    ui.ctx().copy_text(d.text.clone());
                                }
                                if ui.button(tr(lang, "ai_retry")).clicked() {
                                    action = Some(d.action.clone());
                                }
                                if ui.button(tr(lang, "ai_discard")).clicked() {
                                    discard = true;
                                }
                            });
                            return;
                        }
                        if let Some((a, _, started)) = &self.ai.running {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new(format!(
                                        "{} — {} ({}s)",
                                        tr(lang, a.key()),
                                        tr(lang, "ai_thinking"),
                                        started.elapsed().as_secs()
                                    ))
                                    .color(pal.text_secondary),
                                );
                            });
                            return;
                        }
                        let scope = match self.ai.ctx.as_ref().and_then(|c| c.selection.as_ref()) {
                            Some((.., text)) => {
                                let short: String = text.chars().take(60).collect();
                                format!(
                                    "{} « {short}{} »",
                                    tr(lang, "ai_on_selection"),
                                    if text.chars().count() > 60 { "…" } else { "" }
                                )
                            }
                            None => tr(lang, "ai_on_page").to_string(),
                        };
                        ui.label(RichText::new(scope).size(12.5).color(pal.text_secondary));
                        ui.add_space(4.0);
                        let item = |ui: &mut egui::Ui, a: AiAction| -> Option<AiAction> {
                            let b = egui::Button::new(RichText::new(tr(lang, a.key())).size(14.0))
                                .min_size(Vec2::new(width, 28.0));
                            ui.add(b).clicked().then_some(a)
                        };
                        for a in [
                            AiAction::Summarize,
                            AiAction::Continue,
                            AiAction::Improve,
                            AiAction::Proofread,
                            AiAction::Clarify,
                            AiAction::Restructure,
                            AiAction::Translate("French".into()),
                            AiAction::Translate("English".into()),
                        ] {
                            if let Some(a) = item(ui, a) {
                                action = Some(a);
                            }
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.ai.question)
                                    .desired_width(width - 70.0)
                                    .hint_text(tr(lang, "ai_question_hint")),
                            );
                            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let ask = ui.button(tr(lang, "ai_ask_short"));
                            widgets::a11y(&ask, tr(lang, "ai_ask"));
                            if (enter || ask.clicked()) && !self.ai.question.trim().is_empty() {
                                action = Some(AiAction::Ask(self.ai.question.trim().to_string()));
                            }
                        });
                        if let Some(e) = &self.ai.error {
                            ui.add_space(6.0);
                            ui.label(RichText::new(e).size(12.5).color(pal.danger));
                        }
                    });
            });
        if let Some(a) = action {
            self.start_ai(a);
        }
        if let Some(replace) = accept {
            self.accept_draft(replace);
        }
        if discard {
            self.ai.draft = None;
        }
        if close {
            self.ai.open = false;
            self.ai.draft = None;
        }
    }
}

/// The draft as rich text (titles, bullets, bold, italics, code) instead of raw Markdown.
fn draft_view(ui: &mut egui::Ui, pal: &crate::theme::Palette, text: &str, width: f32) {
    use eframe::egui::text::{LayoutJob, TextFormat};
    use eframe::egui::FontId;
    use notecraft_core::blocks::BlockKind;
    let mut number = 0usize;
    for b in blocks::from_markdown(text) {
        if b.kind != BlockKind::Numbered {
            number = 0;
        }
        let indent = f32::from(b.depth) * 18.0;
        let (size, prefix, heading) = match &b.kind {
            BlockKind::Heading(1) => (18.0, String::new(), true),
            BlockKind::Heading(2) => (16.0, String::new(), true),
            BlockKind::Heading(_) => (15.0, String::new(), true),
            BlockKind::Bulleted => (14.0, "\u{2022}  ".to_string(), false),
            BlockKind::Numbered => {
                number += 1;
                (14.0, format!("{number}.  "), false)
            }
            BlockKind::Todo { checked } => {
                (14.0, if *checked { "\u{2611}  " } else { "\u{2610}  " }.to_string(), false)
            }
            BlockKind::Quote => (14.0, "\u{258f} ".to_string(), false),
            _ => (14.0, String::new(), false),
        };
        let mut job = LayoutJob::default();
        job.wrap.max_width = (width - indent).max(120.0);
        let base = TextFormat { font_id: FontId::proportional(size), color: pal.text, ..Default::default() };
        if !prefix.is_empty() {
            job.append(&prefix, 0.0, TextFormat { color: pal.text_secondary, ..base.clone() });
        }
        for seg in notecraft_core::inline::parse(&b.text) {
            if seg.marker {
                continue;
            }
            let st = &seg.style;
            let mut f = base.clone();
            if st.bold || heading {
                if crate::theme::has_weight_axis() {
                    f.coords = crate::theme::weight(700.0);
                } else {
                    f.color = pal.text;
                }
            }
            f.italics = st.italic;
            if st.strike {
                f.strikethrough = egui::Stroke::new(1.0, f.color);
            }
            if st.code {
                f.font_id = FontId::monospace(size * 0.88);
                f.background = pal.code_bg;
            }
            job.append(&b.text[seg.range.clone()], 0.0, f);
        }
        ui.add_space(if heading { 6.0 } else { 2.0 });
        ui.horizontal_top(|ui| {
            ui.add_space(indent);
            ui.add(egui::Label::new(job).wrap());
        });
    }
}
