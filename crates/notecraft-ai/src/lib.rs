//! AI writing help for NoteCraft, with three engines chosen in Settings:
//!
//! - **Ollama** (default): a model already installed on this Mac, reached at
//!   `http://localhost:11434`. Works offline; text never leaves the Mac.
//!   Models are only listed, never downloaded.
//! - **Claude API**: Anthropic's API with the person's own key (kept in the
//!   macOS Keychain, see [`keychain`]). Text leaves the Mac.
//! - **OpenAI API** (ChatGPT models): OpenAI's API with the person's own key,
//!   kept the same way. Text leaves the Mac.
//!
//! Results are always returned as drafts; the app asks before applying them.

pub mod keychain;
pub mod prompts;
pub mod publish;
pub mod webpage;

pub use prompts::{build_notes_prompt, build_prompt, note_structure_text, AiAction, Prompt, MAX_CHARS, NOTE_TEMPLATES};

use serde_json::{json, Value};
use std::time::Duration;

pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
pub const DEFAULT_CLAUDE_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-5-5";
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
/// Shown in the model menu until the account answers with its own list (no key yet, offline).
pub const DEFAULT_GROK_URL: &str = "https://api.x.ai";
pub const DEFAULT_GROK_MODEL: &str = "grok-4";
pub const GROK_FALLBACK_MODELS: &[&str] = &["grok-4", "grok-3", "grok-3-mini"];
pub const OPENAI_FALLBACK_MODELS: &[&str] = &["gpt-4o", "gpt-4o-mini"];
pub const CLAUDE_FALLBACK_MODELS: &[&str] = &["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"];

/// The family of a model, for grouping a model list: (rank, label). The lower the rank, the more
/// capable the family (Fable, then Opus, Sonnet, Haiku; GPT and Grok by generation).
pub fn model_tier(m: &str) -> (u8, &'static str) {
    let l = m.to_lowercase();
    let has = |k: &str| l.contains(k);
    if has("mythos") {
        (0, "Mythos")
    } else if has("fable") {
        (1, "Fable")
    } else if has("opus") {
        (2, "Opus")
    } else if has("sonnet") {
        (3, "Sonnet")
    } else if has("haiku") {
        (4, "Haiku")
    } else if l.starts_with("grok-4") {
        (10, "Grok 4")
    } else if l.starts_with("grok-3") {
        (11, "Grok 3")
    } else if l.starts_with("grok") {
        (12, "Grok")
    } else if l.starts_with("gpt-5") {
        (20, "GPT-5")
    } else if l.starts_with("gpt-4.1") || l.starts_with("gpt-4o") || l.starts_with("gpt-4") {
        (21, "GPT-4")
    } else if l.starts_with('o') && l.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
        (22, "Reasoning")
    } else {
        (30, "Other")
    }
}

/// The models worth offering: Claude keeps only the newest version of each family; GPT and Grok keep
/// only their newest generation. Older versions are not listed.
pub fn current_models(models: &[String]) -> Vec<String> {
    let sorted = sort_models(models);
    let mut seen_claude: Vec<u8> = Vec::new();
    let mut best_group: std::collections::HashMap<u8, u8> = std::collections::HashMap::new();
    for m in &sorted {
        let r = model_tier(m).0;
        if (10..30).contains(&r) {
            let e = best_group.entry(r / 10).or_insert(r);
            *e = (*e).min(r);
        }
    }
    sorted
        .into_iter()
        .filter(|m| {
            let r = model_tier(m).0;
            if r < 10 {
                if seen_claude.contains(&r) {
                    false
                } else {
                    seen_claude.push(r);
                    true
                }
            } else if r < 30 {
                best_group.get(&(r / 10)) == Some(&r)
            } else {
                true
            }
        })
        .collect()
}

/// Sorts a model list by family, then newest version first (dates are ignored in the version).
pub fn sort_models(models: &[String]) -> Vec<String> {
    let version = |m: &str| -> Vec<u32> {
        m.split(|c: char| !c.is_ascii_digit())
            .filter(|t| !t.is_empty() && t.len() < 5)
            .filter_map(|t| t.parse().ok())
            .collect()
    };
    let mut v: Vec<String> = models.to_vec();
    v.sort_by(|a, b| {
        model_tier(a).0.cmp(&model_tier(b).0).then_with(|| version(b).cmp(&version(a))).then_with(|| a.cmp(b))
    });
    v.dedup();
    v
}
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const MAX_TOKENS: u32 = 2048;

#[derive(Debug, Clone, PartialEq)]
pub enum Engine {
    Ollama {
        url: String,
        model: String,
    },
    Claude {
        url: String,
        model: String,
        api_key: String,
    },
    OpenAi {
        url: String,
        model: String,
        api_key: String,
    },
    /// xAI's Grok, which speaks the same protocol as the OpenAI API.
    Grok {
        url: String,
        model: String,
        api_key: String,
    },
}

impl Engine {
    /// True when using this engine sends text off this Mac.
    pub fn leaves_this_mac(&self) -> bool {
        match self {
            Engine::Claude { .. } | Engine::OpenAi { .. } | Engine::Grok { .. } => true,
            Engine::Ollama { url, .. } => !is_local_url(url),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Engine::Ollama { model, .. } => format!("Ollama · {model}"),
            Engine::Claude { model, .. } => format!("Claude · {model}"),
            Engine::OpenAi { model, .. } => format!("ChatGPT · {model}"),
            Engine::Grok { model, .. } => format!("Grok · {model}"),
        }
    }
}

pub use notecraft_net::is_local_url;

/// Lets the app talk to an Ollama that is not on this Mac (off by default).
pub fn set_allow_remote_ollama(on: bool) {
    notecraft_net::set_allow_remote_local_ai(on);
}

/// Network guard for the local engine.
fn guard_local(url: &str) -> Result<(), AiError> {
    notecraft_net::check(notecraft_net::Purpose::LocalAi, url)
        .map_err(|_| AiError::RemoteBlocked(url.trim().to_string()))
}

/// Network guard for a cloud service the user chose.
fn guard_cloud(url: &str) -> Result<(), AiError> {
    notecraft_net::check(notecraft_net::Purpose::CloudAi, url).map_err(|b| AiError::Network(b.to_string()))
}

/// How long to wait.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Duration,
    pub total: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts { connect: Duration::from_secs(4), total: Duration::from_secs(120) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AiError {
    /// Nothing answers at the Ollama address.
    OllamaNotRunning(String),
    /// The chosen Ollama model is not installed (installed ones listed).
    ModelMissing {
        model: String,
        installed: Vec<String>,
    },
    NoModel,
    /// The Ollama address is not on this Mac and remote use was not allowed.
    RemoteBlocked(String),
    NoApiKey,
    InvalidKey,
    RateLimited,
    Timeout,
    Network(String),
    Api(u16, String),
    BadResponse(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::RemoteBlocked(url) => write!(
                f,
                "{url} is not on this Mac. Local models only talk to this Mac: use http://localhost:11434, or allow remote Ollama in the preferences file."
            ),
            AiError::OllamaNotRunning(url) => {
                write!(f, "Ollama isn't running at {url}. Open the Ollama app (or run `ollama serve`) and try again.")
            }
            AiError::ModelMissing { model, installed } if installed.is_empty() => {
                write!(f, "The model \"{model}\" isn't installed and Ollama has no models yet. Install one yourself, e.g. `ollama pull llama3.2`.")
            }
            AiError::ModelMissing { model, installed } => {
                write!(f, "The model \"{model}\" isn't installed. Installed: {}.", installed.join(", "))
            }
            AiError::NoModel => write!(f, "Choose a model in Settings → AI."),
            AiError::NoApiKey => write!(f, "Add your API key in Settings → AI."),
            AiError::InvalidKey => write!(f, "The API key was refused. Check it in Settings → AI."),
            AiError::RateLimited => write!(f, "Too many requests right now. Wait a moment and try again."),
            AiError::Timeout => write!(f, "The AI took too long to answer (timeout)."),
            AiError::Network(e) => write!(f, "Network error: {e}"),
            AiError::Api(code, msg) => write!(f, "The AI service answered {code}: {msg}"),
            AiError::BadResponse(e) => write!(f, "Unexpected answer from the AI: {e}"),
        }
    }
}

impl std::error::Error for AiError {}

fn agent(t: Timeouts) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(t.connect))
        .timeout_global(Some(t.total))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn map_err(e: ureq::Error, ollama_url: Option<&str>) -> AiError {
    match &e {
        ureq::Error::Timeout(_) => AiError::Timeout,
        ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => match ollama_url {
            Some(u) => AiError::OllamaNotRunning(u.to_string()),
            None => AiError::Network(e.to_string()),
        },
        ureq::Error::Io(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::NotConnected
            ) =>
        {
            match ollama_url {
                Some(u) => AiError::OllamaNotRunning(u.to_string()),
                None => AiError::Network(e.to_string()),
            }
        }
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => AiError::Timeout,
        _ => AiError::Network(e.to_string()),
    }
}

fn trim_url(u: &str) -> String {
    u.trim().trim_end_matches('/').to_string()
}

/// Retry rule for transient errors: 3 attempts, 1 s base delay, exponential backoff with
/// "full jitter" (a random wait up to `min(base × 2^attempt, 30 s)`).
pub const RETRY_ATTEMPTS: u32 = 3;
pub const RETRY_BASE_MS: u64 = 1000;
pub const RETRY_MAX_MS: u64 = 30_000;

/// The longest wait before retry number `attempt + 1` (0-based attempt).
pub fn retry_cap_ms(attempt: u32) -> u64 {
    RETRY_BASE_MS.saturating_mul(1u64 << attempt.min(20)).min(RETRY_MAX_MS)
}

/// A random wait in `[0, retry_cap_ms(attempt))` (full jitter).
pub fn retry_delay_ms(attempt: u32) -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let r = (nanos.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 11) % 1_000_000;
    retry_cap_ms(attempt) * r / 1_000_000
}

/// Errors worth another try: busy service, network hiccup, server error.
pub fn is_transient(e: &AiError) -> bool {
    matches!(e, AiError::RateLimited | AiError::Network(_)) || matches!(e, AiError::Api(code, _) if *code >= 500)
}

/// [`complete`] with the retry rule for transient errors.
pub fn complete_with_retry(engine: &Engine, prompt: &Prompt, t: Timeouts) -> Result<String, AiError> {
    let mut last = AiError::Timeout;
    for attempt in 0..RETRY_ATTEMPTS {
        match complete(engine, prompt, t) {
            Ok(s) => return Ok(s),
            Err(e) if is_transient(&e) && attempt + 1 < RETRY_ATTEMPTS => {
                last = e;
                std::thread::sleep(Duration::from_millis(retry_delay_ms(attempt)));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// Whether something listens on the Ollama port on every network interface (not only on this Mac).
/// `None` when it cannot be told (no `lsof`, nothing listening).
pub fn ollama_listens_on_network() -> Option<bool> {
    let out =
        std::process::Command::new("/usr/sbin/lsof").args(["-nP", "-iTCP:11434", "-sTCP:LISTEN"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let names: Vec<&str> = text.lines().skip(1).filter_map(|l| l.split_whitespace().rev().nth(1)).collect();
    if names.is_empty() {
        return None;
    }
    Some(names.iter().any(|n| n.starts_with('*') || n.starts_with("0.0.0.0") || n.starts_with("[::]")))
}

/// Whether the Ollama app or command line tool is installed on this Mac (it may not be running).
pub fn ollama_installed() -> bool {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    [
        std::path::PathBuf::from("/Applications/Ollama.app"),
        home.join("Applications/Ollama.app"),
        std::path::PathBuf::from("/opt/homebrew/bin/ollama"),
        std::path::PathBuf::from("/usr/local/bin/ollama"),
    ]
    .iter()
    .any(|p| p.exists())
}

/// Models installed in Ollama (`GET /api/tags`). Never downloads anything.
pub fn ollama_models(url: &str, t: Timeouts) -> Result<Vec<String>, AiError> {
    guard_local(url)?;
    let base = trim_url(url);
    let mut resp = agent(t).get(format!("{base}/api/tags")).call().map_err(|e| map_err(e, Some(&base)))?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().map_err(|e| map_err(e, Some(&base)))?;
    if status != 200 {
        return Err(AiError::Api(status, body));
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| AiError::BadResponse(e.to_string()))?;
    let mut names: Vec<String> = v["models"]
        .as_array()
        .map(|a| {
            a.iter().filter_map(|m| m["name"].as_str().or_else(|| m["model"].as_str()).map(String::from)).collect()
        })
        .unwrap_or_default();
    names.sort();
    Ok(names)
}

/// Removes a model from Ollama (`DELETE /api/delete`).
pub fn ollama_delete(url: &str, model: &str) -> Result<(), AiError> {
    guard_local(url)?;
    let base = trim_url(url);
    let t = Timeouts { connect: Duration::from_secs(4), total: Duration::from_secs(30) };
    let resp = agent(t)
        .delete(format!("{base}/api/delete"))
        .force_send_body()
        .send_json(json!({"model": model}))
        .map_err(|e| map_err(e, Some(&base)))?;
    match resp.status().as_u16() {
        200 => Ok(()),
        s => Err(AiError::Api(s, String::new())),
    }
}

/// Downloads a model into Ollama (`POST /api/pull`), which streams its progress line by line.
/// `progress(done, total)` is called as bytes arrive and returns false to stop.
pub fn ollama_pull(url: &str, model: &str, mut progress: impl FnMut(u64, u64) -> bool) -> Result<(), AiError> {
    use std::io::BufRead;
    guard_local(url)?;
    let base = trim_url(url);
    let t = Timeouts { connect: Duration::from_secs(4), total: Duration::from_secs(4 * 3600) };
    let mut resp = agent(t)
        .post(format!("{base}/api/pull"))
        .send_json(json!({"model": model, "stream": true}))
        .map_err(|e| map_err(e, Some(&base)))?;
    let status = resp.status().as_u16();
    if status != 200 {
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        return Err(AiError::Api(status, body));
    }
    let reader = std::io::BufReader::new(resp.into_body().into_reader());
    for line in reader.lines() {
        let line = line.map_err(|e| AiError::Network(e.to_string()))?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(e) = v["error"].as_str() {
            return Err(AiError::BadResponse(e.to_string()));
        }
        let total = v["total"].as_u64().unwrap_or(0);
        if total > 0 && !progress(v["completed"].as_u64().unwrap_or(0), total) {
            return Err(AiError::Network("cancelled".into()));
        }
        if v["status"].as_str() == Some("success") {
            return Ok(());
        }
    }
    Ok(())
}

/// Models the Claude API offers to this key (`GET /v1/models`), newest first as the API sends them.
/// The list changes when Anthropic releases a model, so it is fetched, never hard-coded.
pub fn claude_models(url: &str, api_key: &str, t: Timeouts) -> Result<Vec<String>, AiError> {
    if api_key.trim().is_empty() {
        return Err(AiError::NoApiKey);
    }
    guard_cloud(url)?;
    let base = trim_url(url);
    let mut resp = agent(t)
        .get(format!("{base}/v1/models?limit=100"))
        .header("x-api-key", api_key.trim())
        .header("anthropic-version", ANTHROPIC_VERSION)
        .call()
        .map_err(|e| map_err(e, None))?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().map_err(|e| map_err(e, None))?;
    match status {
        200 => {
            let v: Value = serde_json::from_str(&body).map_err(|e| AiError::BadResponse(e.to_string()))?;
            Ok(v["data"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
                .unwrap_or_default())
        }
        401 | 403 => Err(AiError::InvalidKey),
        429 | 529 => Err(AiError::RateLimited),
        s => Err(AiError::Api(s, error_message(&body))),
    }
}

/// Models of the OpenAI API that can write text for this key (`GET /v1/models`), sorted by name.
/// Embeddings, speech, image, moderation and realtime models are left out.
pub fn openai_models(url: &str, api_key: &str, t: Timeouts) -> Result<Vec<String>, AiError> {
    compatible_models(url, api_key, t, is_openai_chat_model)
}

/// Models of the xAI API (same protocol as OpenAI's): text models only, image and video ones left out.
pub fn grok_models(url: &str, api_key: &str, t: Timeouts) -> Result<Vec<String>, AiError> {
    compatible_models(url, api_key, t, is_grok_chat_model)
}

/// A model id of the xAI API that answers chat requests.
pub fn is_grok_chat_model(id: &str) -> bool {
    let l = id.to_lowercase();
    l.starts_with("grok") && !["image", "imagine", "video"].iter().any(|w| l.contains(w))
}

fn compatible_models(url: &str, api_key: &str, t: Timeouts, keep: fn(&str) -> bool) -> Result<Vec<String>, AiError> {
    if api_key.trim().is_empty() {
        return Err(AiError::NoApiKey);
    }
    guard_cloud(url)?;
    let base = trim_url(url);
    let mut resp = agent(t)
        .get(format!("{base}/v1/models"))
        .header("authorization", format!("Bearer {}", api_key.trim()))
        .call()
        .map_err(|e| map_err(e, None))?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().map_err(|e| map_err(e, None))?;
    match status {
        200 => {
            let v: Value = serde_json::from_str(&body).map_err(|e| AiError::BadResponse(e.to_string()))?;
            let mut names: Vec<String> = v["data"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
                .unwrap_or_default();
            names.retain(|n| keep(n));
            names.sort();
            names.dedup();
            Ok(names)
        }
        401 | 403 => Err(AiError::InvalidKey),
        429 => Err(AiError::RateLimited),
        s => Err(AiError::Api(s, error_message(&body))),
    }
}

/// A model id that answers chat requests (`gpt-…`, `o1`, `o3-mini`, `chatgpt-…`).
pub fn is_openai_chat_model(id: &str) -> bool {
    let l = id.to_lowercase();
    let family = l.starts_with("gpt-")
        || l.starts_with("chatgpt-")
        || (l.starts_with('o') && l.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));
    let other = [
        "embedding",
        "whisper",
        "tts",
        "dall-e",
        "image",
        "audio",
        "transcribe",
        "moderation",
        "realtime",
        "search",
        "instruct",
        "codex",
        "computer-use",
    ];
    family && !other.iter().any(|w| l.contains(w))
}

/// The model to use when none is chosen: the first installed one that can
/// write (embedding-only models are skipped). Never downloads anything.
pub fn pick_chat_model(installed: &[String]) -> Option<String> {
    installed
        .iter()
        .find(|m| {
            let l = m.to_lowercase();
            !(l.contains("embed") || l.contains("bge-") || l.contains("minilm") || l.contains("rerank"))
        })
        .cloned()
}

/// The request body sent to Ollama's chat endpoint.
pub fn ollama_body(model: &str, p: &Prompt) -> Value {
    json!({
        "model": model,
        "stream": false,
        "messages": [
            {"role": "system", "content": p.system},
            {"role": "user", "content": p.user}
        ]
    })
}

/// The request body sent to the Claude Messages API.
pub fn claude_body(model: &str, p: &Prompt) -> Value {
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": p.system,
        "messages": [{"role": "user", "content": p.user}]
    })
}

/// The request body sent to the OpenAI Chat Completions API.
pub fn openai_body(model: &str, p: &Prompt) -> Value {
    json!({
        "model": model,
        "max_completion_tokens": MAX_TOKENS,
        "messages": [
            {"role": "system", "content": p.system},
            {"role": "user", "content": p.user}
        ]
    })
}

/// Text of an OpenAI chat answer.
pub fn parse_openai(body: &str) -> Result<String, AiError> {
    let v: Value = serde_json::from_str(body).map_err(|e| AiError::BadResponse(e.to_string()))?;
    let text = v["choices"][0]["message"]["content"].as_str().map(|s| s.trim().to_string());
    match text {
        Some(t) if !t.is_empty() => Ok(t),
        _ => Err(AiError::BadResponse("no text in the answer".into())),
    }
}

/// Text of an Ollama chat answer.
pub fn parse_ollama(body: &str) -> Result<String, AiError> {
    let v: Value = serde_json::from_str(body).map_err(|e| AiError::BadResponse(e.to_string()))?;
    v["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| AiError::BadResponse("no message content".into()))
}

/// Text of a Claude Messages answer (all text blocks joined).
pub fn parse_claude(body: &str) -> Result<String, AiError> {
    let v: Value = serde_json::from_str(body).map_err(|e| AiError::BadResponse(e.to_string()))?;
    let parts: Vec<&str> = v["content"]
        .as_array()
        .map(|a| a.iter().filter(|c| c["type"] == "text").filter_map(|c| c["text"].as_str()).collect())
        .unwrap_or_default();
    if parts.is_empty() {
        return Err(AiError::BadResponse("no text in the answer".into()));
    }
    Ok(parts.join("\n").trim().to_string())
}

fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().or_else(|| v["error"].as_str()).map(String::from))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

/// Ask the engine; returns the answer text (Markdown).
pub fn complete(engine: &Engine, prompt: &Prompt, t: Timeouts) -> Result<String, AiError> {
    match engine {
        Engine::Ollama { url, model } => {
            if model.trim().is_empty() {
                return Err(AiError::NoModel);
            }
            guard_local(url)?;
            let base = trim_url(url);
            let mut resp = agent(t)
                .post(format!("{base}/api/chat"))
                .send_json(ollama_body(model, prompt))
                .map_err(|e| map_err(e, Some(&base)))?;
            let status = resp.status().as_u16();
            let body = resp.body_mut().read_to_string().map_err(|e| map_err(e, Some(&base)))?;
            match status {
                200 => parse_ollama(&body),
                404 => Err(AiError::ModelMissing {
                    model: model.clone(),
                    installed: ollama_models(&base, t).unwrap_or_default(),
                }),
                s => Err(AiError::Api(s, error_message(&body))),
            }
        }
        Engine::Claude { url, model, api_key } => {
            if api_key.trim().is_empty() {
                return Err(AiError::NoApiKey);
            }
            guard_cloud(url)?;
            let base = trim_url(url);
            let mut resp = agent(t)
                .post(format!("{base}/v1/messages"))
                .header("x-api-key", api_key.trim())
                .header("anthropic-version", ANTHROPIC_VERSION)
                .send_json(claude_body(model, prompt))
                .map_err(|e| map_err(e, None))?;
            let status = resp.status().as_u16();
            let body = resp.body_mut().read_to_string().map_err(|e| map_err(e, None))?;
            match status {
                200 => parse_claude(&body),
                401 | 403 => Err(AiError::InvalidKey),
                429 | 529 => Err(AiError::RateLimited),
                s => Err(AiError::Api(s, error_message(&body))),
            }
        }
        Engine::OpenAi { url, model, api_key } | Engine::Grok { url, model, api_key } => {
            if api_key.trim().is_empty() {
                return Err(AiError::NoApiKey);
            }
            if model.trim().is_empty() {
                return Err(AiError::NoModel);
            }
            guard_cloud(url)?;
            let base = trim_url(url);
            let mut resp = agent(t)
                .post(format!("{base}/v1/chat/completions"))
                .header("authorization", format!("Bearer {}", api_key.trim()))
                .send_json(openai_body(model, prompt))
                .map_err(|e| map_err(e, None))?;
            let status = resp.status().as_u16();
            let body = resp.body_mut().read_to_string().map_err(|e| map_err(e, None))?;
            match status {
                200 => parse_openai(&body),
                401 | 403 => Err(AiError::InvalidKey),
                429 => Err(AiError::RateLimited),
                s => Err(AiError::Api(s, error_message(&body))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_local_guard_refuses_look_alike_hosts() {
        assert!(is_local_url("http://localhost:11434"));
        assert!(is_local_url("http://127.0.0.1:11434"));
        assert!(is_local_url("http://[::1]:11434"));
        assert!(!is_local_url("http://127.evil.example.com"));
        assert!(!is_local_url("http://localhost@evil.example.com"));
        assert!(!is_local_url("http://192.168.1.20:11434"));
    }

    #[test]
    fn models_are_grouped_by_family_newest_first() {
        let v: Vec<String> = [
            "claude-sonnet-4-5-20250929",
            "claude-opus-5",
            "claude-haiku-5-5",
            "claude-opus-5-5",
            "claude-fable-5-2",
            "claude-sonnet-5-5",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            sort_models(&v),
            [
                "claude-fable-5-2",
                "claude-opus-5-5",
                "claude-opus-5",
                "claude-sonnet-5-5",
                "claude-sonnet-4-5-20250929",
                "claude-haiku-5-5"
            ]
        );
    }

    #[test]
    fn only_current_models_are_listed() {
        let v: Vec<String> = [
            "claude-opus-4-8",
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-haiku-4-5-20251001",
            "claude-haiku-5-5",
            "grok-3",
            "grok-4",
            "gpt-4o",
            "gpt-5",
            "o3",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(current_models(&v), ["claude-opus-5-5", "claude-haiku-5-5", "grok-4", "gpt-5"]);
    }

    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// A one-shot HTTP server: answers `status` + `body` and reports the request it got.
    fn mock(status: u16, body: &'static str, delay_ms: u64) -> (String, mpsc::Receiver<String>) {
        mock_seq(vec![(status, body), (status, body)], delay_ms)
    }

    /// Answers each connection in turn from `answers`.
    fn mock_seq(answers: Vec<(u16, &'static str)>, delay_ms: u64) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (stream, (status, body)) in listener.incoming().zip(answers) {
                let Ok(mut s) = stream else { continue };
                let mut reader = BufReader::new(s.try_clone().unwrap());
                let mut head = String::new();
                let mut len = 0usize;
                let mut chunked = false;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
                        chunked = true;
                    }
                    head.push_str(&line);
                }
                let mut buf = Vec::new();
                if chunked {
                    loop {
                        let mut size = String::new();
                        if reader.read_line(&mut size).unwrap_or(0) == 0 {
                            break;
                        }
                        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
                        let mut chunk = vec![0; n + 2];
                        let _ = reader.read_exact(&mut chunk);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                    }
                } else {
                    buf = vec![0; len];
                    let _ = reader.read_exact(&mut buf);
                }
                let _ = tx.send(format!("{head}\r\n{}", String::from_utf8_lossy(&buf)));
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (url, rx)
    }

    fn prompt() -> Prompt {
        build_prompt(&AiAction::Summarize, "Trip", "- pack\n- book train", None)
    }

    #[test]
    fn ollama_chat_through_a_mock_server() {
        let (url, rx) = mock(200, r#"{"message":{"role":"assistant","content":" A short summary. "},"done":true}"#, 0);
        let e = Engine::Ollama { url: url.clone(), model: "llama3.2".into() };
        assert!(!e.leaves_this_mac());
        let out = complete(&e, &prompt(), Timeouts::default()).unwrap();
        assert_eq!(out, "A short summary.");
        let req = rx.recv().unwrap();
        assert!(req.starts_with("POST /api/chat"));
        let body: Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["model"], "llama3.2");
        assert_eq!(body["stream"], false);
        assert!(body["messages"][1]["content"].as_str().unwrap().contains("book train"));
    }

    #[test]
    fn ollama_lists_installed_models_only() {
        let (url, rx) = mock(200, r#"{"models":[{"name":"mistral:latest"},{"name":"llama3.2:latest"}]}"#, 0);
        assert_eq!(ollama_models(&url, Timeouts::default()).unwrap(), vec!["llama3.2:latest", "mistral:latest"]);
        assert!(rx.recv().unwrap().starts_with("GET /api/tags"));
    }

    #[test]
    fn claude_lists_the_models_of_the_key() {
        let (url, rx) = mock(200, r#"{"data":[{"id":"claude-b","type":"model"},{"id":"claude-a","type":"model"}]}"#, 0);
        assert_eq!(claude_models(&url, "sk-test", Timeouts::default()).unwrap(), vec!["claude-b", "claude-a"]);
        let req = rx.recv().unwrap().to_lowercase();
        assert!(req.starts_with("get /v1/models") && req.contains("x-api-key: sk-test"));
        assert_eq!(claude_models(&url, " ", Timeouts::default()), Err(AiError::NoApiKey));
        let (url, _rx) = mock(401, r#"{"error":{"message":"invalid x-api-key"}}"#, 0);
        assert_eq!(claude_models(&url, "bad", Timeouts::default()), Err(AiError::InvalidKey));
    }

    #[test]
    fn ollama_not_running_is_explained() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port(); // closed again
        let e = Engine::Ollama { url: format!("http://127.0.0.1:{port}"), model: "llama3.2".into() };
        let err = complete(&e, &prompt(), Timeouts::default()).unwrap_err();
        assert!(matches!(err, AiError::OllamaNotRunning(_)), "{err:?}");
        assert!(err.to_string().contains("Ollama isn't running"));
    }

    #[test]
    fn missing_model_lists_the_installed_ones() {
        let (url, _rx) = mock_seq(
            vec![(404, r#"{"error":"model 'nope' not found"}"#), (200, r#"{"models":[{"name":"llama3.2:latest"}]}"#)],
            0,
        );
        let e = Engine::Ollama { url, model: "nope".into() };
        match complete(&e, &prompt(), Timeouts::default()).unwrap_err() {
            AiError::ModelMissing { installed, .. } => assert_eq!(installed, vec!["llama3.2:latest"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn claude_messages_through_a_mock_server() {
        let (url, rx) = mock(200, r#"{"content":[{"type":"text","text":"Résumé."}],"stop_reason":"end_turn"}"#, 0);
        let e = Engine::Claude { url, model: DEFAULT_CLAUDE_MODEL.into(), api_key: "sk-test".into() };
        assert!(e.leaves_this_mac());
        assert_eq!(complete(&e, &prompt(), Timeouts::default()).unwrap(), "Résumé.");
        let req = rx.recv().unwrap();
        let lower = req.to_lowercase();
        assert!(lower.starts_with("post /v1/messages"));
        assert!(lower.contains("x-api-key: sk-test") && lower.contains("anthropic-version: 2023-06-01"));
        let body: Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["max_tokens"], 2048);
        assert_eq!(body["model"], DEFAULT_CLAUDE_MODEL);
    }

    #[test]
    fn claude_errors_are_clear() {
        let (url, _rx) =
            mock(401, r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#, 0);
        let e = Engine::Claude { url, model: "m".into(), api_key: "bad".into() };
        assert_eq!(complete(&e, &prompt(), Timeouts::default()).unwrap_err(), AiError::InvalidKey);
        let e = Engine::Claude { url: "http://127.0.0.1:9".into(), model: "m".into(), api_key: " ".into() };
        assert_eq!(complete(&e, &prompt(), Timeouts::default()).unwrap_err(), AiError::NoApiKey);
    }

    #[test]
    fn openai_chat_through_a_mock_server() {
        let (url, rx) = mock(200, r#"{"choices":[{"message":{"role":"assistant","content":" Résumé. "}}]}"#, 0);
        let e = Engine::OpenAi { url, model: DEFAULT_OPENAI_MODEL.into(), api_key: "sk-test".into() };
        assert!(e.leaves_this_mac());
        assert_eq!(e.label(), format!("ChatGPT · {DEFAULT_OPENAI_MODEL}"));
        assert_eq!(complete(&e, &prompt(), Timeouts::default()).unwrap(), "Résumé.");
        let req = rx.recv().unwrap();
        let lower = req.to_lowercase();
        assert!(lower.starts_with("post /v1/chat/completions"));
        assert!(lower.contains("authorization: bearer sk-test"));
        let body: Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["model"], DEFAULT_OPENAI_MODEL);
        assert_eq!(body["max_completion_tokens"], 2048);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
    }

    #[test]
    fn openai_errors_are_clear() {
        let (url, _rx) =
            mock(401, r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#, 0);
        let e = Engine::OpenAi { url, model: "m".into(), api_key: "bad".into() };
        assert_eq!(complete(&e, &prompt(), Timeouts::default()).unwrap_err(), AiError::InvalidKey);
        let (url, _rx) = mock(429, r#"{"error":{"message":"rate limit"}}"#, 0);
        let e = Engine::OpenAi { url, model: "m".into(), api_key: "k".into() };
        assert_eq!(complete(&e, &prompt(), Timeouts::default()).unwrap_err(), AiError::RateLimited);
        let e = Engine::OpenAi { url: "http://127.0.0.1:9".into(), model: "m".into(), api_key: " ".into() };
        assert_eq!(complete(&e, &prompt(), Timeouts::default()).unwrap_err(), AiError::NoApiKey);
        assert!(parse_openai(r#"{"choices":[]}"#).is_err());
    }

    #[test]
    fn grok_model_filter_keeps_text_models() {
        assert!(is_grok_chat_model("grok-4") && is_grok_chat_model("Grok-3-mini"));
        assert!(!is_grok_chat_model("grok-2-image") && !is_grok_chat_model("gpt-4o"));
    }

    #[test]
    fn openai_model_list_keeps_chat_models_only() {
        let (url, rx) = mock(
            200,
            r#"{"data":[{"id":"gpt-4o"},{"id":"text-embedding-3-small"},{"id":"whisper-1"},{"id":"o3-mini"},{"id":"dall-e-3"},{"id":"gpt-4o-mini-tts"},{"id":"gpt-4o"}]}"#,
            0,
        );
        let list = openai_models(&url, "sk-test", Timeouts::default()).unwrap();
        assert_eq!(list, vec!["gpt-4o".to_string(), "o3-mini".to_string()]);
        assert!(rx.recv().unwrap().to_lowercase().contains("authorization: bearer sk-test"));
        assert!(is_openai_chat_model("chatgpt-4o-latest") && !is_openai_chat_model("omni-moderation-latest"));
    }

    #[test]
    fn slow_answers_time_out() {
        let (url, _rx) = mock(200, r#"{"message":{"content":"late"}}"#, 1500);
        let e = Engine::Ollama { url, model: "m".into() };
        let t = Timeouts { connect: Duration::from_secs(1), total: Duration::from_millis(300) };
        assert_eq!(complete(&e, &prompt(), t).unwrap_err(), AiError::Timeout);
    }

    #[test]
    fn retry_rule_matches_the_original() {
        assert_eq!(retry_cap_ms(0), 1000);
        assert_eq!(retry_cap_ms(1), 2000);
        assert_eq!(retry_cap_ms(4), 16_000);
        assert_eq!(retry_cap_ms(5), 30_000, "capped at 30 s");
        for a in 0..6 {
            assert!(retry_delay_ms(a) < retry_cap_ms(a));
        }
        assert!(is_transient(&AiError::RateLimited) && is_transient(&AiError::Api(503, String::new())));
        assert!(!is_transient(&AiError::InvalidKey) && !is_transient(&AiError::OllamaNotRunning("x".into())));
    }

    #[test]
    fn a_busy_service_is_retried() {
        let (url, _rx) = mock_seq(
            vec![(529, r#"{"error":{"message":"overloaded"}}"#), (200, r#"{"content":[{"type":"text","text":"ok"}]}"#)],
            0,
        );
        let e = Engine::Claude { url, model: "m".into(), api_key: "k".into() };
        assert_eq!(complete_with_retry(&e, &prompt(), Timeouts::default()).unwrap(), "ok");
    }

    #[test]
    fn default_model_skips_embedding_models() {
        let list = vec!["qwen3-embedding:0.6b".to_string(), "nomic-embed-text".into(), "llama3.2:latest".into()];
        assert_eq!(pick_chat_model(&list).as_deref(), Some("llama3.2:latest"));
        assert_eq!(pick_chat_model(&["nomic-embed-text".to_string()]), None);
    }

    #[test]
    fn local_urls() {
        assert!(is_local_url("http://localhost:11434"));
        assert!(is_local_url("http://127.0.0.1:11434/"));
        assert!(is_local_url("http://[::1]:11434"));
        assert!(!is_local_url("http://192.168.1.5:11434"));
        assert!(!is_local_url("https://api.anthropic.com"));
    }
}
