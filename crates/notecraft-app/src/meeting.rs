//! Meeting notes in the app. A `/meeting` block records the microphone, shows the transcript
//! as it is written (Whisper runs on this Mac), then turns it into notes with the chosen AI engine
//! and note template. The transcript and the notes are ordinary blocks in the page.

use crate::app::{NcApp, Target};
use crate::i18n::{both, tr};
use eframe::egui;
use notecraft_ai::{AiError, Engine, Timeouts};
use notecraft_core::blocks::{self, Block};
use notecraft_core::BlockKind;
use notecraft_meeting::permission::{self, MicAccess};
use notecraft_meeting::{chunker, models, transcribe::Event, Recorder, Transcriber, WhisperModel, MODELS};
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Saved with the preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MeetingPrefs {
    /// Speech model id (see `notecraft_meeting::MODELS`).
    pub model: String,
    /// "auto", "en" or "fr".
    pub language: String,
    /// Note template key (see `notecraft_ai::NOTE_TEMPLATES`).
    pub template: String,
    /// Name of the microphone to use ("" = the system default).
    pub microphone: String,
    /// What to record: "mic", "system" (what the Mac plays, for video calls) or "both".
    pub source: String,
    /// Instructions used by the "custom" template.
    pub custom: String,
    /// Write the notes as soon as the recording stops.
    pub auto_notes: bool,
    /// Names, projects and jargon the speech model should spell right (commas or lines).
    pub vocabulary: String,
    /// The built-in note templates have been written as pages (so they can be edited).
    pub notes_seeded: bool,
    /// Writing the templates failed this session: do not try again at every frame.
    #[serde(skip)]
    pub seed_failed: bool,
    /// Speech engine: "auto" (Parakeet for English when it is downloaded), "whisper" or "parakeet".
    pub engine: String,
}

impl Default for MeetingPrefs {
    fn default() -> Self {
        MeetingPrefs {
            model: "small".into(),
            language: "auto".into(),
            template: "summary".into(),
            microphone: String::new(),
            source: "mic".into(),
            custom: String::new(),
            auto_notes: true,
            vocabulary: String::new(),
            engine: "auto".into(),
            notes_seeded: false,
            seed_failed: false,
        }
    }
}

/// What the card of a meeting block can ask for.
#[derive(Debug, Clone, PartialEq)]
pub enum MeetingAction {
    Start(u64),
    Stop,
    DownloadModel,
    /// Download the Parakeet engine (several files, about 670 MB).
    DownloadParakeet,
    CancelDownload,
    /// Write (or rewrite) the notes of the meeting block from its transcript.
    Notes(u64),
    SetTemplate(String),
    /// Open the page that holds the note templates (creating it when needed).
    ManageTemplates,
    /// Put the meeting settings back to their defaults.
    ResetSettings,
    OpenMicSettings,
    OpenAudioSettings,
    SetMicrophone(String),
    SetSource(String),
    SetLanguage(String),
    /// Switch to this microphone and start again (offered when the first one only sent silence).
    UseMicrophone(String, u64),
    /// Give a speaker of the transcript another name: (meeting block, old name, new name).
    RenameSpeaker(u64, String, String),
}

/// What the editor shows on the card, copied each frame.
#[derive(Debug, Clone, Default)]
pub struct View {
    /// Block being recorded, its loudness, time so far and whether the speech model is loaded.
    pub recording: Option<(u64, f32, Duration, bool)>,
    pub finishing: Option<u64>,
    pub writing: Option<(u64, String)>,
    /// Download progress 0..1 with the model name.
    pub download: Option<(String, f32)>,
    pub model_ready: bool,
    pub model_label: String,
    pub model_mb: u32,
    pub template: String,
    /// Note templates written by the user (key, title).
    pub custom_templates: Vec<(String, String)>,
    pub error: Option<String>,
    /// The error is about the microphone permission: offer a shortcut to the macOS settings.
    pub mic_help: bool,
    /// Same for the system audio permission.
    pub audio_help: bool,
    /// The microphone that only sent silence, so the card can offer the others.
    pub silent_device: Option<String>,
    /// Microphones this Mac offers, the system default among them, and the one chosen ("" = default).
    pub mics: Vec<String>,
    pub default_mic: Option<String>,
    pub microphone: String,
    pub source: String,
    /// Spoken language of the transcript: "auto", "en" or "fr".
    pub language: String,
}

struct Session {
    target: Target,
    block: u64,
    recorder: Option<Recorder>,
    transcriber: Transcriber,
    pending: Vec<f32>,
    /// Audio of the other people (system), kept apart from `pending` (the microphone) when both are recorded.
    pending_sys: Vec<f32>,
    /// Both sources are recorded: each piece of transcript says who spoke.
    split: bool,
    /// Only the other people are recorded (system audio): their voices are told apart and numbered.
    remote_only: bool,
    me: String,
    others: String,
    parts: Vec<String>,
    ready: bool,
    finishing: bool,
    started: Instant,
    loudest: f32,
    device: String,
}

struct NotesJob {
    target: Target,
    block: u64,
    engine_label: String,
    rx: Receiver<Result<String, AiError>>,
}

struct DownloadJob {
    label: String,
    done: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Result<(), String>>,
}

/// When the note templates were last read, and what was read.
type TemplateCache = (Instant, Vec<(String, String)>);

#[derive(Default)]
pub struct MeetingState {
    session: Option<Session>,
    notes: Option<NotesJob>,
    download: Option<DownloadJob>,
    pub error: Option<String>,
    pub mic_help: bool,
    pub audio_help: bool,
    silent_device: Option<String>,
    /// Recording was asked for and waits for a model download or a macOS permission answer.
    start_after: Option<(u64, Instant)>,
    mics: Vec<String>,
    default_mic: Option<String>,
    mics_checked: Option<Instant>,
    /// The note templates, re-read from the workspace at most once a second.
    tpl_cache: std::cell::RefCell<Option<TemplateCache>>,
}

// ------------------------------------------------------------ pure helpers

/// Where the transcript and the notes of the meeting block at `i` are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// Index of the "Transcript" toggle that belongs to this meeting.
    pub transcript: Option<usize>,
    /// Blocks between the meeting block and its transcript: the notes (empty when there are none).
    pub notes: Range<usize>,
}

fn is_transcript_title(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    let (en, fr) = both("meeting_transcript");
    t == en.to_lowercase() || t == fr.to_lowercase()
}

/// Finds the transcript toggle after the meeting block at `i` (before the next meeting block).
pub fn locate(blocks: &[Block], i: usize) -> Located {
    let depth = blocks[i].depth;
    for (j, b) in blocks.iter().enumerate().skip(i + 1) {
        if b.kind == BlockKind::Meeting {
            break;
        }
        if matches!(b.kind, BlockKind::Toggle { .. }) && b.depth == depth && is_transcript_title(&b.text) {
            return Located { transcript: Some(j), notes: i + 1..j };
        }
    }
    Located { transcript: None, notes: i + 1..i + 1 }
}

/// The transcript text under the toggle at `j`: one paragraph per piece.
fn transcript_text(blocks: &[Block], j: usize) -> String {
    let end = blocks::subtree_end(blocks, j);
    blocks[j + 1..end].iter().map(|b| b.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n")
}

/// A speaker name must stay short and plain, so that a line "Name : text" is never mistaken for
/// ordinary prose ("Attention : the budget...").
fn valid_speaker(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n.chars().count() <= 30
        && n.split_whitespace().count() <= 4
        && !n.contains('\n')
        && !n.contains(" : ")
}

/// The name in front of a transcript paragraph: "Speaker 2 : hello" gives "Speaker 2".
fn speaker_of(text: &str) -> Option<&str> {
    let (label, rest) = text.split_once(" : ")?;
    let label = label.trim();
    (valid_speaker(label) && !rest.trim().is_empty()).then_some(label)
}

/// The names the app gives by itself: "Me" / "Moi" and "Speaker N" / "Interlocuteur N".
fn is_default_speaker(label: &str) -> bool {
    let (me_en, me_fr) = both("speaker_me");
    if [me_en, me_fr].iter().any(|m| m.eq_ignore_ascii_case(label)) {
        return true;
    }
    let (o_en, o_fr) = both("speaker_others");
    [o_en, o_fr].iter().any(|o| {
        label
            .strip_prefix(*o)
            .and_then(|r| r.strip_prefix(' '))
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    })
}

/// The speakers of the transcript under the toggle at `j`, in order of first appearance. A name counts
/// when the app gave it, or when it opens at least two paragraphs (a name you typed yourself).
pub fn speakers(blocks: &[Block], j: usize) -> Vec<String> {
    let end = blocks::subtree_end(blocks, j);
    let labels: Vec<&str> = blocks[j + 1..end].iter().filter_map(|b| speaker_of(&b.text)).collect();
    let mut out: Vec<String> = Vec::new();
    for l in &labels {
        if out.iter().any(|o| o == l) {
            continue;
        }
        if is_default_speaker(l) || labels.iter().filter(|x| *x == l).count() >= 2 {
            out.push((*l).to_string());
        }
    }
    out
}

/// Gives a speaker another name in every paragraph of the transcript under `j`; returns how many
/// paragraphs changed. A name already in use merges the two speakers. Invalid names change nothing.
pub fn rename_speaker(blocks: &mut [Block], j: usize, old: &str, new: &str) -> usize {
    let new = new.trim();
    if new == old || !valid_speaker(new) {
        return 0;
    }
    let end = blocks::subtree_end(blocks, j);
    let mut n = 0;
    for b in &mut blocks[j + 1..end] {
        if speaker_of(&b.text) == Some(old) {
            let rest = b.text.split_once(" : ").map(|(_, r)| r.to_string()).unwrap_or_default();
            b.text = format!("{new} : {rest}");
            n += 1;
        }
    }
    n
}

/// "22 Nov 2026 23:40", local time.
fn now_label() -> String {
    let z = jiff::Zoned::now();
    z.strftime("%d %b %Y %H:%M").to_string()
}

fn mmss(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl NcApp {
    pub(crate) fn meeting_model(&self) -> &'static WhisperModel {
        WhisperModel::by_id(&self.prefs.meeting.model).unwrap_or(&MODELS[1])
    }

    pub fn meeting_view(&self) -> View {
        let m = &self.meeting;
        let model = self.meeting_model();
        View {
            recording: m
                .session
                .as_ref()
                .filter(|s| !s.finishing)
                .map(|s| (s.block, s.recorder.as_ref().map_or(0.0, Recorder::level), s.started.elapsed(), s.ready)),
            finishing: m.session.as_ref().filter(|s| s.finishing).map(|s| s.block),
            writing: m.notes.as_ref().map(|n| (n.block, n.engine_label.clone())),
            download: m.download.as_ref().map(|d| {
                let total = d.total.load(Ordering::Relaxed).max(1);
                (d.label.clone(), d.done.load(Ordering::Relaxed) as f32 / total as f32)
            }),
            model_ready: model.installed(),
            model_label: model.label.to_string(),
            model_mb: model.size_mb,
            template: self.effective_template(),
            custom_templates: self.cached_note_templates(),
            error: m.error.clone(),
            mic_help: m.mic_help,
            audio_help: m.audio_help,
            silent_device: m.silent_device.clone(),
            mics: m.mics.clone(),
            default_mic: m.default_mic.clone(),
            microphone: self.prefs.meeting.microphone.clone(),
            source: self.prefs.meeting.source.clone(),
            language: self.prefs.meeting.language.clone(),
        }
    }

    /// The template to use: a built-in one that has been written as a page (and maybe edited) is that page.
    pub fn effective_template(&self) -> String {
        let t = &self.prefs.meeting.template;
        // once the built-in templates are pages, the stored key is final: no lookup by name
        if t.starts_with("page:") || self.prefs.meeting.notes_seeded {
            return t.clone();
        }
        let label = template_label(self.prefs.lang, t);
        self.cached_note_templates().into_iter().find(|(_, n)| n == label).map(|(k, _)| k).unwrap_or_else(|| t.clone())
    }

    /// The note templates of the workspace, cached for a second (the meeting block asks at every frame).
    pub fn cached_note_templates(&self) -> Vec<(String, String)> {
        let mut c = self.meeting.tpl_cache.borrow_mut();
        if let Some((at, v)) = c.as_ref() {
            if at.elapsed() < Duration::from_secs(1) {
                return v.clone();
            }
        }
        let v = self.ws.as_ref().map(crate::app::note_templates).unwrap_or_default();
        *c = Some((Instant::now(), v.clone()));
        v
    }

    /// Re-reads the list of microphones now and then (headphones come and go).
    pub fn refresh_mics(&mut self) {
        if self.meeting.mics_checked.is_some_and(|t| t.elapsed() < Duration::from_secs(4)) {
            return;
        }
        let (list, default) = notecraft_meeting::input_devices();
        self.meeting.mics = list;
        self.meeting.default_mic = default;
        self.meeting.mics_checked = Some(Instant::now());
    }

    pub fn meeting_busy(&self) -> bool {
        self.meeting.session.is_some()
            || self.meeting.notes.is_some()
            || self.meeting.download.is_some()
            || self.meeting.start_after.is_some()
    }

    /// Change the blocks of a page, open or not, and make sure the change is saved.
    fn edit_page(&mut self, target: &Target, f: impl FnOnce(&mut Vec<Block>)) -> bool {
        let ok = {
            let Some(ws) = self.ws.as_mut() else { return false };
            let Some(b) = NcApp::blocks_mut(ws, target) else { return false };
            f(b);
            true
        };
        if self.current.as_ref() == Some(target) {
            self.mark_dirty();
        } else if let (Some(ws), Target::Doc(d)) = (self.ws.as_mut(), target) {
            let _ = ws.save(d);
        }
        ok
    }

    pub fn meeting_action(&mut self, a: MeetingAction) {
        self.meeting.error = None;
        self.meeting.mic_help = false;
        self.meeting.audio_help = false;
        self.meeting.silent_device = None;
        match a {
            MeetingAction::OpenMicSettings => {
                crate::app::open_external("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            }
            MeetingAction::OpenAudioSettings => crate::app::open_external(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
            ),
            MeetingAction::RenameSpeaker(block, old, new) => {
                let Some(target) = self.current.clone() else { return };
                self.edit_page(&target, |b| {
                    let Some(i) = b.iter().position(|x| x.id == block) else { return };
                    if let Some(j) = locate(b, i).transcript {
                        rename_speaker(b, j, &old, &new);
                    }
                });
            }
            MeetingAction::SetTemplate(t) => self.prefs.meeting.template = t,
            MeetingAction::ManageTemplates => {
                self.ensure_templates();
                if let Some(id) = self.ws.as_ref().and_then(crate::app::notes_group) {
                    self.navigate(Target::Doc(notecraft_core::DocId::Page(id)));
                }
            }
            MeetingAction::ResetSettings => {
                self.prefs.meeting = MeetingPrefs::default();
                self.meeting.error = None;
                self.meeting.silent_device = None;
                self.meeting.mic_help = false;
                self.meeting.audio_help = false;
                // models installed in the meantime, and microphones plugged in, are found again
                self.ai.models_error = None;
                self.refresh_models();
                self.meeting.mics_checked = None;
            }
            MeetingAction::SetMicrophone(m) => self.prefs.meeting.microphone = m,
            MeetingAction::SetSource(s) => self.prefs.meeting.source = s,
            MeetingAction::SetLanguage(l) => self.prefs.meeting.language = l,
            MeetingAction::UseMicrophone(m, block) => {
                self.prefs.meeting.microphone = m;
                self.start_meeting(block);
            }
            MeetingAction::DownloadModel => self.start_model_download(),
            MeetingAction::DownloadParakeet => self.start_parakeet_download(),
            MeetingAction::CancelDownload => {
                self.meeting.start_after = None;
                if let Some(d) = &self.meeting.download {
                    d.cancel.store(true, Ordering::Relaxed);
                }
            }
            MeetingAction::Start(block) => self.start_meeting(block),
            MeetingAction::Stop => self.stop_meeting(),
            MeetingAction::Notes(block) => {
                let Some(target) = self.current.clone() else { return };
                let text = {
                    let Some(ws) = self.ws.as_mut() else { return };
                    let Some(blocks) = NcApp::blocks_mut(ws, &target) else { return };
                    let Some(i) = blocks.iter().position(|b| b.id == block) else { return };
                    locate(blocks, i).transcript.map(|j| transcript_text(blocks, j))
                };
                match text {
                    Some(t) if !t.trim().is_empty() => self.start_notes(target, block, t),
                    _ => self.meeting.error = Some(self.t("meeting_no_transcript").into()),
                }
            }
        }
    }

    pub(crate) fn start_model_download(&mut self) {
        if self.meeting.download.is_some() {
            return;
        }
        let model = self.meeting_model();
        let (done, total, cancel) =
            (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
        let (d2, t2, c2) = (done.clone(), total.clone(), cancel.clone());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = models::download(model, |d, t| {
                d2.store(d, Ordering::Relaxed);
                t2.store(t, Ordering::Relaxed);
                !c2.load(Ordering::Relaxed)
            });
            let _ = tx.send(r);
        });
        self.meeting.download = Some(DownloadJob { label: model.label.to_string(), done, total, cancel, rx });
    }

    fn start_parakeet_download(&mut self) {
        if self.meeting.download.is_some() {
            return;
        }
        let (done, total, cancel) =
            (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
        let (d2, t2, c2) = (done.clone(), total.clone(), cancel.clone());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = models::parakeet_download(|d, t| {
                d2.store(d, Ordering::Relaxed);
                t2.store(t, Ordering::Relaxed);
                !c2.load(Ordering::Relaxed)
            });
            let _ = tx.send(r);
        });
        self.meeting.download = Some(DownloadJob { label: "Parakeet".to_string(), done, total, cancel, rx });
    }

    fn start_meeting(&mut self, block: u64) {
        if self.meeting.session.is_some() {
            self.meeting.error = Some(self.t("meeting_busy").into());
            return;
        }
        let model = self.meeting_model();
        if !model.installed() {
            // One click: download the model, then start by itself.
            self.meeting.start_after = Some((block, Instant::now()));
            self.start_model_download();
            return;
        }
        let Some(target) = self.current.clone() else { return };
        let source = notecraft_meeting::Source::from_id(&self.prefs.meeting.source);
        // System audio has its own permission, which macOS asks for by itself; only the microphone needs this.
        match if source.uses_mic() { permission::status() } else { MicAccess::Authorized } {
            MicAccess::Authorized => {}
            MicAccess::NotDetermined => {
                // macOS shows its own window; the person clicks Start again once they have answered.
                // macOS shows its own window; recording starts by itself once it is answered.
                permission::request();
                self.meeting.start_after = Some((block, Instant::now()));
                self.meeting.error = Some(self.t("meeting_allow_mic").into());
                return;
            }
            MicAccess::Denied => {
                self.meeting.error = Some(self.t("meeting_silent").into());
                self.meeting.mic_help = true;
                return;
            }
        }
        let mic = self.prefs.meeting.microphone.clone();
        let recorder = match Recorder::start_with(Some(&mic), source) {
            Ok(r) => r,
            Err(e) => {
                self.meeting.error = Some(format!("{}: {e}", self.t("meeting_mic_error")));
                return;
            }
        };
        let device = recorder.device_name().to_string();
        let transcriber = Transcriber::start_engine(
            &model.path(),
            &self.prefs.meeting.language,
            &self.prefs.meeting.vocabulary,
            &self.prefs.meeting.engine,
        );
        let title = format!("{} {}", self.t("meeting_default_title"), now_label());
        self.edit_page(&target, |b| {
            if let Some(m) = b.iter_mut().find(|x| x.id == block) {
                if m.text.trim().is_empty() {
                    m.text = title;
                }
            }
        });
        self.meeting.session = Some(Session {
            target,
            block,
            recorder: Some(recorder),
            transcriber,
            pending: Vec::new(),
            pending_sys: Vec::new(),
            split: source == notecraft_meeting::Source::Both,
            remote_only: source == notecraft_meeting::Source::System,
            me: self.t("speaker_me").to_string(),
            others: self.t("speaker_others").to_string(),
            parts: Vec::new(),
            ready: false,
            finishing: false,
            started: Instant::now(),
            loudest: 0.0,
            device,
        });
    }

    fn stop_meeting(&mut self) {
        let Some(s) = self.meeting.session.as_mut() else { return };
        if s.finishing {
            return;
        }
        if let Some(r) = s.recorder.take() {
            if s.split {
                let (mic, sys) = r.take_all_split();
                s.pending.extend(mic);
                s.pending_sys.extend(sys);
            } else {
                s.pending.extend(r.take_all());
            }
        }
        if !s.pending.is_empty() {
            let rest = std::mem::take(&mut s.pending);
            if s.split {
                s.transcriber.push_labeled(rest, &s.me);
            } else if s.remote_only {
                s.transcriber.push_remote(rest, &s.others);
            } else {
                s.transcriber.push(rest);
            }
        }
        if !s.pending_sys.is_empty() {
            let rest = std::mem::take(&mut s.pending_sys);
            s.transcriber.push_remote(rest, &s.others);
        }
        s.transcriber.finish();
        s.finishing = true;
    }

    /// Adds one piece of transcript to the page, creating the "Transcript" toggle on the first one.
    fn append_transcript(&mut self, target: &Target, block: u64, text: &str) {
        let title = self.t("meeting_transcript").to_string();
        let text = text.to_string();
        self.edit_page(target, |b| {
            let Some(i) = b.iter().position(|x| x.id == block) else { return };
            let depth = b[i].depth;
            let j = match locate(b, i).transcript {
                Some(j) => j,
                None => {
                    let at = i + 1;
                    b.insert(at, Block::new(BlockKind::Toggle { open: true }, title).with_depth(depth));
                    at
                }
            };
            let end = blocks::subtree_end(b, j);
            b.insert(end, Block::paragraph(text).with_depth(depth + 1));
        });
    }

    fn start_notes(&mut self, target: Target, block: u64, transcript: String) {
        let prefs = self.prefs.meeting.clone();
        let (title, date) = {
            let t = self
                .ws
                .as_mut()
                .and_then(|ws| {
                    NcApp::blocks_mut(ws, &target)
                        .and_then(|b| b.iter().find(|x| x.id == block).map(|m| m.text.clone()))
                })
                .unwrap_or_default();
            (t, now_label())
        };
        let effective = self.effective_template();
        let (tpl, custom) = if effective.starts_with("page:") {
            match self.ws.as_ref().and_then(|ws| crate::app::note_template_text(ws, &effective)) {
                Some(t) => (
                    "custom".to_string(),
                    format!(
                        "Write the notes following this template. Keep its headings and structure, fill each part from the transcript and leave out what was not said:\n\n{t}"
                    ),
                ),
                None => ("summary".to_string(), String::new()),
            }
        } else {
            (prefs.template.clone(), prefs.custom.clone())
        };
        let prompt = notecraft_ai::build_notes_prompt(&tpl, &custom, &title, &date, &transcript);
        let mut engine = self.prefs.ai.engine();
        if let Engine::Ollama { model, .. } = &mut engine {
            if model.trim().is_empty() {
                match notecraft_ai::pick_chat_model(&self.ai.models) {
                    Some(m) => *model = m,
                    None => {
                        self.meeting.error = Some(self.t("meeting_no_ai_model").into());
                        return;
                    }
                }
            }
        }
        let engine_label = engine.label();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let t = Timeouts { connect: Duration::from_secs(6), total: Duration::from_secs(300) };
            let _ = tx.send(notecraft_ai::complete_with_retry(&engine, &prompt, t));
        });
        self.meeting.notes = Some(NotesJob { target, block, engine_label, rx });
    }

    /// Put the notes after the meeting block, replacing the previous notes.
    fn insert_notes(&mut self, target: &Target, block: u64, markdown: &str) {
        let new = blocks::from_markdown(markdown);
        if new.is_empty() {
            return;
        }
        self.edit_page(target, |b| {
            let Some(i) = b.iter().position(|x| x.id == block) else { return };
            let depth = b[i].depth;
            let range = locate(b, i).notes;
            b.drain(range.clone());
            for (k, nb) in new.into_iter().enumerate() {
                let d = depth + nb.depth;
                b.insert(i + 1 + k, Block { depth: d, ..nb });
            }
        });
    }

    fn close_transcript(&mut self, target: &Target, block: u64) {
        self.edit_page(target, |b| {
            if let Some(i) = b.iter().position(|x| x.id == block) {
                if let Some(j) = locate(b, i).transcript {
                    b[j].kind = BlockKind::Toggle { open: false };
                }
            }
        });
    }

    /// Called every frame: moves audio to the transcriber, text to the page, and collects finished jobs.
    pub fn poll_meeting(&mut self, ctx: &egui::Context) {
        if !self.meeting_busy() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        self.poll_download();
        self.poll_pending_start();
        self.poll_session();
        self.poll_notes();
    }

    /// Starts the recording that was asked for as soon as what it waited for is there.
    fn poll_pending_start(&mut self) {
        let Some((block, since)) = self.meeting.start_after else { return };
        if self.meeting.download.is_some() || self.meeting.session.is_some() {
            return;
        }
        let uses_mic = notecraft_meeting::Source::from_id(&self.prefs.meeting.source).uses_mic();
        let access = if uses_mic { permission::status() } else { MicAccess::Authorized };
        match access {
            MicAccess::Authorized => {
                self.meeting.start_after = None;
                self.meeting.error = None;
                self.start_meeting(block);
            }
            MicAccess::Denied => {
                self.meeting.start_after = None;
                self.meeting.error = Some(self.t("meeting_silent").into());
                self.meeting.mic_help = true;
            }
            MicAccess::NotDetermined if since.elapsed() > Duration::from_secs(120) => self.meeting.start_after = None,
            MicAccess::NotDetermined => {}
        }
    }

    fn poll_download(&mut self) {
        let Some(d) = &self.meeting.download else { return };
        let Ok(r) = d.rx.try_recv() else { return };
        let label = d.label.clone();
        self.meeting.download = None;
        match r {
            Ok(()) => self.notify(format!("{label}: {}", self.t("meeting_model_ready"))),
            Err(e) if e == "cancelled" => self.meeting.start_after = None,
            Err(e) => {
                self.meeting.start_after = None;
                self.meeting.error = Some(format!("{}: {e}", self.t("meeting_download_failed")));
            }
        }
    }

    fn poll_session(&mut self) {
        let mut texts: Vec<String> = Vec::new();
        let mut done = false;
        let mut failed: Option<String> = None;
        let mut device = String::new();
        let (target, block) = {
            let Some(s) = self.meeting.session.as_mut() else { return };
            device.clone_from(&s.device);
            if let Some(r) = &s.recorder {
                if s.split {
                    let (mic, sys) = r.take_split();
                    s.pending.extend(mic);
                    s.pending_sys.extend(sys);
                } else {
                    s.pending.extend(r.take());
                }
                s.loudest = s.loudest.max(r.level());
            }
            if !s.finishing {
                for chunk in due_chunks(&mut s.pending) {
                    if s.split {
                        s.transcriber.push_labeled(chunk, &s.me);
                    } else if s.remote_only {
                        s.transcriber.push_remote(chunk, &s.others);
                    } else {
                        s.transcriber.push(chunk);
                    }
                }
                if s.split {
                    for chunk in due_chunks(&mut s.pending_sys) {
                        s.transcriber.push_remote(chunk, &s.others);
                    }
                }
            }
            while let Ok(ev) = s.transcriber.events.try_recv() {
                match ev {
                    Event::Ready => s.ready = true,
                    Event::Text(t) => {
                        s.parts.push(t.clone());
                        texts.push(t);
                    }
                    Event::Done => done = true,
                    Event::Failed(e) => failed = Some(e),
                }
            }
            if !s.finishing && s.started.elapsed() > Duration::from_secs(5) && s.loudest == 0.0 {
                if let Some(r) = &s.recorder {
                    let (cb, peak) = r.stats();
                    // Nobody may be speaking yet in a call: system audio alone is only a problem when it is dead.
                    if r.source() != notecraft_meeting::Source::System || cb == 0 {
                        notecraft_core::log::error(&format!(
                            "meeting: silent source {:?} {:?}: {cb} callbacks, peak {peak}, status {:?}",
                            r.source(),
                            s.device,
                            permission::status()
                        ));
                        let dead_system = r.source().uses_system() && r.system_callbacks() == 0;
                        failed = Some(String::from(if dead_system { "silent_system" } else { "silent" }));
                    }
                }
            }
            (s.target.clone(), s.block)
        };
        for t in texts {
            self.append_transcript(&target, block, &t);
        }
        if let Some(e) = failed {
            let dead_system = e == "silent_system";
            if dead_system {
                self.meeting.audio_help = true;
            }
            if e == "silent" {
                self.meeting.silent_device = Some(device.clone());
            }
            let msg = if dead_system {
                self.t("meeting_silent_system").to_string()
            } else if e == "silent" {
                format!("{} {device}. {}", self.t("meeting_silent_device"), self.t("meeting_silent_device_hint"))
            } else {
                format!("{}: {e}", self.t("meeting_failed"))
            };
            self.meeting.error = Some(msg);
            self.meeting.session = None;
            return;
        }
        if done {
            let Some(s) = self.meeting.session.take() else { return };
            if s.parts.is_empty() {
                self.meeting.error = Some(self.t("meeting_nothing_heard").into());
                return;
            }
            self.close_transcript(&s.target, s.block);
            if self.prefs.meeting.auto_notes {
                self.start_notes(s.target, s.block, s.parts.join("\n\n"));
            }
        }
    }

    fn poll_notes(&mut self) {
        let Some(n) = &self.meeting.notes else { return };
        let r = match n.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => Err(AiError::BadResponse("the AI request stopped unexpectedly".into())),
        };
        let (target, block) = (n.target.clone(), n.block);
        self.meeting.notes = None;
        match r {
            Ok(text) => self.insert_notes(&target, block, &text),
            Err(e) => self.meeting.error = Some(e.to_string()),
        }
    }
}

/// Label of a note template in the current language.
pub fn template_label(lang: crate::i18n::Lang, key: &str) -> &'static str {
    tr(lang, &format!("tpl_{key}"))
}

/// Short clock text for the card.
pub fn clock(d: Duration) -> String {
    mmss(d)
}

/// Cuts off every piece of `pending` that is ready to be transcribed (in a pause, or at the longest length).
fn due_chunks(pending: &mut Vec<f32>) -> Vec<Vec<f32>> {
    let mut out = Vec::new();
    while let Some(n) = chunker::next_cut(pending) {
        out.push(pending.drain(..n).collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(kind: BlockKind, text: &str, depth: u8) -> Block {
        Block::new(kind, text).with_depth(depth)
    }

    fn transcript(lines: &[&str]) -> Vec<Block> {
        let mut v = vec![b(BlockKind::Meeting, "M", 0), b(BlockKind::Toggle { open: true }, "Transcript", 0)];
        v.extend(lines.iter().map(|l| b(BlockKind::Paragraph, l, 1)));
        v
    }

    #[test]
    fn speakers_are_listed_in_order_and_ordinary_colons_are_not_speakers() {
        let blocks = transcript(&[
            "Speaker 1 : hello everyone",
            "Me : hi",
            "Speaker 2 : ok",
            "Speaker 1 : next point",
            "Attention : the budget changed",
            "no label at all",
        ]);
        assert_eq!(speakers(&blocks, 1), vec!["Speaker 1", "Me", "Speaker 2"]);
        // a transcript of the microphone alone, with French typography, has no speakers
        let blocks = transcript(&["Attention : le budget", "Rappel : la date", "Voici le point : suite"]);
        assert!(speakers(&blocks, 1).is_empty());
    }

    #[test]
    fn a_name_you_typed_stays_a_speaker_once_it_opens_two_paragraphs() {
        let mut blocks = transcript(&["Speaker 1 : a", "Speaker 2 : b", "Speaker 1 : c"]);
        assert_eq!(rename_speaker(&mut blocks, 1, "Speaker 1", "Thomas Martin"), 2);
        assert_eq!(speakers(&blocks, 1), vec!["Thomas Martin", "Speaker 2"]);
        assert_eq!(rename_speaker(&mut blocks, 1, "Thomas Martin", "Thomas"), 2, "it can be renamed again");
    }

    #[test]
    fn renaming_changes_only_that_speaker_and_keeps_the_text_untouched() {
        let mut blocks = transcript(&["Speaker 1 : a : b", "Speaker 2 : Speaker 1 said so", "Speaker 1 : c"]);
        assert_eq!(rename_speaker(&mut blocks, 1, "Speaker 1", "Anna"), 2);
        let texts: Vec<&str> = blocks[2..].iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["Anna : a : b", "Speaker 2 : Speaker 1 said so", "Anna : c"]);
    }

    #[test]
    fn renaming_to_an_existing_name_merges_two_speakers() {
        let mut blocks = transcript(&["Speaker 1 : a", "Speaker 2 : b", "Speaker 3 : c"]);
        assert_eq!(rename_speaker(&mut blocks, 1, "Speaker 3", "Speaker 1"), 1);
        assert_eq!(speakers(&blocks, 1), vec!["Speaker 1", "Speaker 2"]);
    }

    #[test]
    fn bad_names_and_other_transcripts_are_left_alone() {
        let mut blocks = transcript(&["Speaker 1 : a"]);
        for bad in ["", "   ", "a : b", "line\nbreak", "one two three four five", &"x".repeat(40), "Speaker 1"] {
            assert_eq!(rename_speaker(&mut blocks, 1, "Speaker 1", bad), 0, "{bad:?}");
        }
        assert_eq!(blocks[2].text, "Speaker 1 : a");
        // another meeting's transcript, after the next meeting block, is out of reach
        let mut two = transcript(&["Speaker 1 : a"]);
        two.push(b(BlockKind::Paragraph, "Speaker 1 : outside", 0));
        assert_eq!(rename_speaker(&mut two, 1, "Speaker 1", "Z"), 1);
        assert_eq!(two[3].text, "Speaker 1 : outside");
    }

    #[test]
    fn finds_the_transcript_and_the_notes() {
        let blocks = vec![
            b(BlockKind::Paragraph, "intro", 0),
            b(BlockKind::Meeting, "Weekly", 0),
            b(BlockKind::Heading(2), "Summary", 0),
            b(BlockKind::Paragraph, "We ship.", 0),
            b(BlockKind::Toggle { open: false }, "Transcript", 0),
            b(BlockKind::Paragraph, "hello there", 1),
            b(BlockKind::Paragraph, "after", 0),
        ];
        let l = locate(&blocks, 1);
        assert_eq!(l.transcript, Some(4));
        assert_eq!(l.notes, 2..4);
        assert_eq!(transcript_text(&blocks, 4), "hello there");
    }

    #[test]
    fn a_meeting_without_transcript_has_no_notes_range() {
        let blocks = vec![b(BlockKind::Meeting, "", 0), b(BlockKind::Paragraph, "x", 0)];
        let l = locate(&blocks, 0);
        assert_eq!(l.transcript, None);
        assert!(l.notes.is_empty());
    }

    #[test]
    fn does_not_borrow_the_transcript_of_the_next_meeting() {
        let blocks = vec![
            b(BlockKind::Meeting, "A", 0),
            b(BlockKind::Meeting, "B", 0),
            b(BlockKind::Toggle { open: false }, "Transcript", 0),
        ];
        assert_eq!(locate(&blocks, 0).transcript, None);
        assert_eq!(locate(&blocks, 1).transcript, Some(2));
    }

    #[test]
    fn transcript_title_in_both_languages() {
        assert!(is_transcript_title("Transcript"));
        assert!(is_transcript_title(" transcription "));
        assert!(!is_transcript_title("Notes"));
    }
}

#[cfg(test)]
mod chunk_tests {
    use super::*;

    #[test]
    fn pieces_are_cut_in_pauses_and_the_rest_waits() {
        let sr = notecraft_meeting::SAMPLE_RATE;
        // 8 s of speech, a 1 s pause, then 3 s more speech
        let mut pending: Vec<f32> = Vec::new();
        pending.extend(std::iter::repeat_n(0.2, 8 * sr));
        pending.extend(std::iter::repeat_n(0.0, sr));
        pending.extend(std::iter::repeat_n(0.2, 3 * sr));
        let total = pending.len();
        let chunks = due_chunks(&mut pending);
        assert_eq!(chunks.len(), 1, "one cut, inside the pause");
        let cut = chunks[0].len();
        assert!(cut > 8 * sr && cut < 9 * sr + sr / 2, "cut in the pause, not in a word: {cut}");
        assert_eq!(cut + pending.len(), total, "nothing lost");
        // too short to cut yet
        assert!(due_chunks(&mut pending).is_empty());
    }

    #[test]
    fn nonstop_speech_is_cut_at_the_longest_length() {
        let sr = notecraft_meeting::SAMPLE_RATE;
        let mut pending = vec![0.2f32; 60 * sr];
        let chunks = due_chunks(&mut pending);
        assert!(chunks.len() >= 2 && chunks.iter().all(|c| c.len() <= chunker::MAX_SECS * sr + sr));
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>() + pending.len(), 60 * sr);
    }
}
