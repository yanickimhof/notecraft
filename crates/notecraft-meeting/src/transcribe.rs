//! Whisper running on this Mac, fed one chunk of speech at a time from a background thread.

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};

/// What the worker reports back.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The model is loaded and ready.
    Ready,
    /// Text for one chunk, in order.
    Text(String),
    /// Every chunk sent before `finish` has been transcribed.
    Done,
    Failed(String),
}

/// Who a chunk of audio belongs to.
#[derive(Debug, Clone, PartialEq)]
enum Speaker {
    /// Not said: the text is kept as it is.
    Unknown,
    /// One known person ("Me").
    Named(String),
    /// The people on the other end: told apart by their voices and numbered, "Speaker 1", "Speaker 2"...
    Remote(String),
}

enum Job {
    Chunk(Vec<f32>, Speaker),
    Finish,
}

/// Handle to the worker thread. Drop it to stop the worker.
pub struct Transcriber {
    tx: Sender<Job>,
    pub events: Receiver<Event>,
}

impl Transcriber {
    /// Loads the model at `model` on a background thread. `language` is "auto" or a code such as "fr".
    pub fn start(model: &Path, language: &str) -> Transcriber {
        Transcriber::start_with(model, language, "")
    }

    /// Like `start`, with the person's own words (names, projects, jargon) given to the model as a hint,
    /// separated by commas or lines. See `vocabulary_prompt`.
    pub fn start_with(model: &Path, language: &str, vocabulary: &str) -> Transcriber {
        Transcriber::start_engine(model, language, vocabulary, "whisper")
    }

    /// Like `start_with`, and `engine` says which speech engine to use: "whisper", "parakeet" (when its files
    /// are installed) or "auto" (see `choose_engine`).
    pub fn start_engine(model: &Path, language: &str, vocabulary: &str, engine: &str) -> Transcriber {
        let (tx, rx) = channel::<Job>();
        let (etx, events) = channel::<Event>();
        let model = model.to_path_buf();
        let language = language.to_string();
        let vocabulary = vocabulary.to_string();
        let engine = engine.to_string();
        std::thread::spawn(move || worker(&model, &language, &vocabulary, &engine, &rx, &etx));
        Transcriber { tx, events }
    }

    pub fn push(&self, samples: Vec<f32>) {
        let _ = self.tx.send(Job::Chunk(samples, Speaker::Unknown));
    }

    /// Like `push`, but the text comes back as "label : text" (who is speaking, when the sources are kept apart).
    pub fn push_labeled(&self, samples: Vec<f32>, label: &str) {
        let _ = self.tx.send(Job::Chunk(samples, Speaker::Named(label.to_string())));
    }

    /// Like `push` for audio of other people: each stretch of speech is given a number by its voice and
    /// comes back as "prefix 2 : text". The numbers are kept for the whole meeting.
    pub fn push_remote(&self, samples: Vec<f32>, prefix: &str) {
        let _ = self.tx.send(Job::Chunk(samples, Speaker::Remote(prefix.to_string())));
    }

    /// Asks for `Event::Done` once the queue is empty.
    pub fn finish(&self) {
        let _ = self.tx.send(Job::Finish);
    }
}

/// "Me : text" when the speaker is known, the text alone otherwise.
pub fn labeled(label: Option<&str>, text: String) -> String {
    match label {
        Some(l) if !l.is_empty() => format!("{l} : {text}"),
        _ => text,
    }
}

/// Gives each stretch of speech a speaker by its voice and joins neighbours of the same speaker:
/// "Speaker 1 : ...", "Speaker 2 : ...". `segments` are (text, first sample, end sample).
fn remote_runs(
    segments: &[(String, usize, usize)],
    samples: &[f32],
    diarizer: &mut crate::diarize::Diarizer,
    prefix: &str,
) -> Vec<String> {
    let mut runs: Vec<(usize, String)> = Vec::new();
    for (text, from, to) in segments {
        let print = crate::diarize::voice_print(&samples[(*from).min(*to)..*to]);
        let id = diarizer.assign(print);
        match runs.last_mut() {
            Some((last, t)) if *last == id => {
                t.push(' ');
                t.push_str(text);
            }
            _ => runs.push((id, text.clone())),
        }
    }
    runs.into_iter().map(|(id, t)| format!("{prefix} {} : {t}", id + 1)).collect()
}

/// Which program turns speech into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Whisper,
    Parakeet,
}

/// Picks the engine for the next chunk.
/// - "whisper": always Whisper.
/// - "parakeet": Parakeet when its files are installed, Whisper otherwise.
/// - anything else ("auto"): Parakeet only when the language is known to be English, the files are installed and
///   no custom vocabulary is set (Parakeet cannot use it). Whisper otherwise, and while the language is unknown.
pub fn choose_engine(preference: &str, language: Option<&str>, vocabulary: &str, parakeet_installed: bool) -> Engine {
    match preference {
        "whisper" => Engine::Whisper,
        "parakeet" if parakeet_installed => Engine::Parakeet,
        "parakeet" => Engine::Whisper,
        _ if parakeet_installed && language == Some("en") && vocabulary_terms(vocabulary).is_empty() => {
            Engine::Parakeet
        }
        _ => Engine::Whisper,
    }
}

/// The longest hint given to the model, in characters: whisper keeps about 224 tokens of it.
const MAX_PROMPT_CHARS: usize = 500;

/// The words the person typed (separated by commas, semicolons or lines), cleaned and without duplicates.
pub fn vocabulary_terms(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in raw.split([',', ';', '\n']) {
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() && !out.iter().any(|o| o.to_lowercase() == t.to_lowercase()) {
            out.push(t);
        }
    }
    out
}

/// The hint handed to Whisper before the audio: the person's words, in order, cut at a whole word
/// before the limit. Empty when there is no vocabulary.
pub fn vocabulary_prompt(raw: &str) -> String {
    let mut prompt = String::new();
    for t in vocabulary_terms(raw) {
        let add = if prompt.is_empty() { t } else { format!(", {t}") };
        if prompt.chars().count() + add.chars().count() > MAX_PROMPT_CHARS {
            break;
        }
        prompt.push_str(&add);
    }
    prompt
}

fn words_of(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// On silence or noise, a model given a hint tends to repeat it: text made only of two or more of the
/// person's own words is that echo, not speech.
pub fn echoes_vocabulary(text: &str, terms: &[String]) -> bool {
    let words = words_of(text);
    if words.len() < 2 {
        return false;
    }
    let known: std::collections::HashSet<String> = terms.iter().flat_map(|t| words_of(t)).collect();
    words.iter().all(|w| known.contains(w))
}

/// Text Whisper invents on silence; never worth keeping.
pub fn is_noise(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let bracketed = (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'));
    let lower = t.to_lowercase();
    bracketed
        || is_looping(t)
        || ["thanks for watching", "thank you for watching", "sous-titrage", "sous-titres réalisés", "amara.org"]
            .iter()
            .any(|p| lower.contains(p))
}

/// Whisper sometimes gets stuck and repeats the same words; such text is never real speech.
pub fn is_looping(text: &str) -> bool {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    if words.len() < 8 {
        return false;
    }
    // One word making up most of the chunk.
    let mut counts = std::collections::HashMap::new();
    for w in &words {
        *counts.entry(w.as_str()).or_insert(0usize) += 1;
    }
    if counts.values().any(|&c| c * 10 >= words.len() * 6) {
        return true;
    }
    // The same three words in a row, over and over.
    let mut tri = std::collections::HashMap::new();
    for t in words.windows(3) {
        *tri.entry(t).or_insert(0usize) += 1;
    }
    tri.values().any(|&c| c >= 4)
}

/// Why a whisper.cpp model file cannot be right: too small or without the GGML signature.
fn damaged_model(path: &Path) -> Option<String> {
    use std::io::Read;
    let len = std::fs::metadata(path).ok()?.len();
    let mut head = [0u8; 4];
    let _ = std::fs::File::open(path).ok()?.read_exact(&mut head);
    // "ggml", "ggjt" and "ggla" appear in the file as little endian numbers: lmgg, tjgg, algg.
    let known = [b"lmgg", b"tjgg", b"algg", b"GGUF"].contains(&&head);
    if len < 20_000_000 {
        Some(format!("only {} MB", len / 1_000_000))
    } else if !known {
        Some("not a model file".to_string())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn worker(model: &Path, language: &str, vocabulary: &str, engine: &str, rx: &Receiver<Job>, etx: &Sender<Event>) {
    use parakeet_rs::Transcriber as _;
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
    // Keeps whisper.cpp from writing its progress and timing lines to the terminal.
    whisper_rs::install_logging_hooks();
    let fail = |m: String| {
        let _ = etx.send(Event::Failed(m));
    };
    let Some(path) = model.to_str() else { return fail("bad model path".into()) };
    // A damaged file is removed so the app offers to download it again.
    if let Some(problem) = damaged_model(model) {
        let _ = std::fs::remove_file(model);
        return fail(format!("the speech model file is damaged ({problem}): it was removed, download it again"));
    }
    // First on the GPU (Metal); if that fails on this Mac, again on the processor.
    let ctx = match WhisperContext::new_with_params(path, WhisperContextParameters::default()) {
        Ok(c) => c,
        Err(gpu_err) => {
            let mut cpu = WhisperContextParameters::default();
            cpu.use_gpu(false);
            match WhisperContext::new_with_params(path, cpu) {
                Ok(c) => c,
                Err(e) => {
                    return fail(format!(
                        "could not load the speech model ({gpu_err}; processor: {e}). Delete the model in Settings and download it again, or choose a smaller one"
                    ))
                }
            }
        }
    };
    let mut state = match ctx.create_state() {
        Ok(s) => s,
        Err(e) => return fail(e.to_string()),
    };
    let _ = etx.send(Event::Ready);
    // In auto mode the language is detected on the first clear speech, then kept for the whole meeting,
    // so a short or noisy chunk cannot flip a French meeting into English.
    let mut diarizer = crate::diarize::Diarizer::default();
    let mut locked: Option<String> = (language != "auto").then(|| language.to_string());
    // Parakeet is loaded only when it is first needed (loading takes a couple of seconds and a lot of memory).
    let mut parakeet: Option<parakeet_rs::ParakeetTDT> = None;
    let mut parakeet_failed = false;
    let prompt = vocabulary_prompt(vocabulary);
    let terms = vocabulary_terms(vocabulary);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
    while let Ok(job) = rx.recv() {
        match job {
            Job::Finish => {
                let _ = etx.send(Event::Done);
            }
            Job::Chunk(mut samples, who) => {
                // Whisper wants at least one second of audio.
                if samples.len() < crate::SAMPLE_RATE * 11 / 10 {
                    samples.resize(crate::SAMPLE_RATE * 11 / 10, 0.0);
                }
                if crate::chunker::rms(&samples) < 0.0015 {
                    continue; // silence: nothing to transcribe
                }
                let mut text = String::new();
                let mut segments: Vec<(String, usize, usize)> = Vec::new();
                let want_parakeet = choose_engine(
                    engine,
                    locked.as_deref(),
                    vocabulary,
                    crate::models::parakeet_installed() && !parakeet_failed,
                ) == Engine::Parakeet;
                if want_parakeet && parakeet.is_none() {
                    let dir = crate::models::parakeet_dir();
                    match parakeet_rs::ParakeetTDT::from_pretrained(dir.to_string_lossy().as_ref(), None) {
                        Ok(p) => parakeet = Some(p),
                        // The files are damaged or the engine will not start: go on with Whisper.
                        Err(_) => parakeet_failed = true,
                    }
                }
                if let (true, Some(pk)) = (want_parakeet, parakeet.as_mut()) {
                    let result = match pk.transcribe_samples(
                        samples.clone(),
                        crate::SAMPLE_RATE as u32,
                        1,
                        Some(parakeet_rs::TimestampMode::Sentences),
                    ) {
                        Ok(r) => r,
                        Err(e) => return fail(e.to_string()),
                    };
                    let at = |t: f32| ((t.max(0.0) * crate::SAMPLE_RATE as f32) as usize).min(samples.len());
                    for tok in result.tokens {
                        let s = tok.text.trim().to_string();
                        if is_noise(&s) {
                            continue;
                        }
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(&s);
                        segments.push((s, at(tok.start), at(tok.end)));
                    }
                } else {
                    let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 3, patience: -1.0 });
                    params.set_n_threads(threads);
                    params.set_translate(false);
                    params.set_print_special(false);
                    params.set_print_progress(false);
                    params.set_print_realtime(false);
                    params.set_print_timestamps(false);
                    params.set_suppress_blank(true);
                    params.set_no_speech_thold(0.6);
                    params.set_language(locked.as_deref());
                    if !prompt.is_empty() {
                        params.set_initial_prompt(&prompt);
                    }
                    if let Err(e) = state.full(params, &samples) {
                        return fail(e.to_string());
                    }
                    for seg in state.as_iter() {
                        let s = seg.to_string();
                        if !is_noise(&s) && !echoes_vocabulary(&s, &terms) {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(s.trim());
                            // whisper counts time in hundredths of a second
                            let at = |t: i64| (t.max(0) as usize * crate::SAMPLE_RATE / 100).min(samples.len());
                            segments.push((s.trim().to_string(), at(seg.start_timestamp()), at(seg.end_timestamp())));
                        }
                    }
                    if locked.is_none() && text.chars().count() >= 25 {
                        locked = whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_string);
                    }
                }
                if text.is_empty() {
                    continue;
                }
                match &who {
                    Speaker::Unknown => {
                        let _ = etx.send(Event::Text(text));
                    }
                    Speaker::Named(l) => {
                        let _ = etx.send(Event::Text(labeled(Some(l), text)));
                    }
                    Speaker::Remote(prefix) => {
                        for run in remote_runs(&segments, &samples, &mut diarizer, prefix) {
                            let _ = etx.send(Event::Text(run));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn worker(_model: &Path, _language: &str, _vocabulary: &str, _engine: &str, _rx: &Receiver<Job>, etx: &Sender<Event>) {
    let _ = etx.send(Event::Failed("transcription is only available on macOS".into()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_voice_gets_its_own_number_and_neighbours_of_one_voice_are_joined() {
        use crate::diarize::tests::voice;
        let sr = crate::SAMPLE_RATE;
        // man (3 s), the same man (3 s), a woman (3 s), the man again (3 s)
        let mut audio = Vec::new();
        for (f0, tract, seed) in [(115.0, 1.0, 1), (115.0, 1.0, 2), (210.0, 1.17, 3), (115.0, 1.0, 4)] {
            audio.extend(voice(f0, tract, seed, 3.0));
        }
        let segs: Vec<(String, usize, usize)> =
            (0..4).map(|i| (format!("part {i}"), i * 3 * sr, (i + 1) * 3 * sr)).collect();
        let mut d = crate::diarize::Diarizer::default();
        let out = remote_runs(&segs, &audio, &mut d, "Speaker");
        assert_eq!(out, vec!["Speaker 1 : part 0 part 1", "Speaker 2 : part 2", "Speaker 1 : part 3"]);
        // the numbers last for the whole meeting: the woman is still number 2 in the next chunk
        let next = vec![("again".to_string(), 0, 3 * sr)];
        let out = remote_runs(&next, &voice(210.0, 1.17, 5, 3.0), &mut d, "Speaker");
        assert_eq!(out, vec!["Speaker 2 : again"]);
    }

    /// The whole way, on this Mac: two real voices speak, Whisper transcribes, and each sentence comes
    /// back under the right speaker number. Skipped without the "base" speech model or the voices.
    #[cfg(target_os = "macos")]
    #[test]
    fn two_real_voices_in_one_recording_come_back_as_speaker_1_and_2() {
        use std::process::Command;
        let model = crate::models::WhisperModel::by_id("base").unwrap();
        if !model.installed() {
            eprintln!("speech model not installed: test skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("nc-e2e-voices-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let say = |n: usize, voice: &str, text: &str| -> Option<Vec<f32>> {
            let (aiff, wav) = (dir.join(format!("{n}.aiff")), dir.join(format!("{n}.wav")));
            let a = Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).output().ok()?;
            let b = Command::new("afconvert")
                .args(["-f", "WAVE", "-d", "LEF32@16000", "-c", "1"])
                .arg(&aiff)
                .arg(&wav)
                .output()
                .ok()?;
            if !a.status.success() || !b.status.success() {
                return None;
            }
            let bytes = std::fs::read(&wav).ok()?;
            let at = bytes.windows(4).position(|w| w == b"data")? + 8;
            Some(bytes[at..].as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
        };
        let lines = [
            ("Thomas", "Bonjour à tous, nous allons commencer la réunion par le point sur le budget."),
            ("Amélie", "Merci Thomas, je pense que le projet avance bien et que nous serons prêts à temps."),
            ("Thomas", "Parfait, alors nous validons le calendrier et nous passons à la suite de l'ordre du jour."),
        ];
        let mut audio: Vec<f32> = Vec::new();
        for (n, (voice, text)) in lines.iter().enumerate() {
            let Some(part) = say(n, voice, text) else {
                eprintln!("voice {voice} not available: test skipped");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            };
            audio.extend(vec![0.0; crate::SAMPLE_RATE / 2]);
            audio.extend(part);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let t = Transcriber::start(&model.path(), "fr");
        t.push_remote(audio, "Speaker");
        t.finish();
        let mut texts: Vec<String> = Vec::new();
        let start = std::time::Instant::now();
        loop {
            match t.events.recv_timeout(std::time::Duration::from_secs(120)) {
                Ok(Event::Text(x)) => texts.push(x),
                Ok(Event::Done) => break,
                Ok(Event::Failed(e)) => panic!("transcription failed: {e}"),
                Ok(Event::Ready) => {}
                Err(_) => panic!("no answer after {:?}", start.elapsed()),
            }
        }
        let who: Vec<&str> = texts.iter().map(|x| x.split(" : ").next().unwrap()).collect();
        assert_eq!(who, vec!["Speaker 1", "Speaker 2", "Speaker 1"], "{texts:?}");
        assert!(texts[0].to_lowercase().contains("budget") && texts[1].to_lowercase().contains("projet"), "{texts:?}");
    }

    /// Real voice, real model: the person's own words are spelled as typed. Skipped without the "base"
    /// speech model or the voice.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_custom_vocabulary_changes_how_rare_names_are_written() {
        use std::process::Command;
        let model = crate::models::WhisperModel::by_id("base").unwrap();
        if !model.installed() {
            eprintln!("speech model not installed: test skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("nc-e2e-vocab-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (aiff, wav) = (dir.join("v.aiff"), dir.join("v.wav"));
        let text = "Le dossier de non-conformité de la villa Orion à Lausanne est certifié Minergie, \
                    et l'entreprise Acme-Clima doit refaire la ventilation.";
        let ok =
            Command::new("say").args(["-v", "Thomas", "-o"]).arg(&aiff).arg(text).output().map(|o| o.status.success());
        let conv = Command::new("afconvert")
            .args(["-f", "WAVE", "-d", "LEF32@16000", "-c", "1"])
            .arg(&aiff)
            .arg(&wav)
            .output()
            .map(|o| o.status.success());
        let bytes = match (ok, conv, std::fs::read(&wav)) {
            (Ok(true), Ok(true), Ok(b)) => b,
            _ => {
                eprintln!("voice not available: test skipped");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        };
        let _ = std::fs::remove_dir_all(&dir);
        let at = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        let audio: Vec<f32> = bytes[at..].as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
        let run = |vocabulary: &str| -> String {
            let t = Transcriber::start_with(&model.path(), "fr", vocabulary);
            t.push(audio.clone());
            t.finish();
            let mut out = String::new();
            loop {
                match t.events.recv_timeout(std::time::Duration::from_secs(120)) {
                    Ok(Event::Text(x)) => {
                        out.push_str(&x);
                        out.push(' ');
                    }
                    Ok(Event::Done) => break,
                    Ok(Event::Failed(e)) => panic!("transcription failed: {e}"),
                    Ok(Event::Ready) => {}
                    Err(_) => panic!("no answer"),
                }
            }
            out
        };
        let without = run("");
        let with = run("Orion, Minergie, Acme-Clima, Lausanne");
        eprintln!("without vocabulary: {without}\nwith vocabulary:    {with}");
        let hits = |s: &str| ["Orion", "Minergie", "Acme-Clima", "Lausanne"].iter().filter(|w| s.contains(*w)).count();
        assert!(hits(&with) >= hits(&without), "the vocabulary made it worse: {without:?} / {with:?}");
        // the small test model still gets some accented names wrong; the vocabulary must at least fix some
        assert!(
            hits(&with) >= 2 && hits(&with) > hits(&without),
            "the vocabulary did not help: {without:?} / {with:?}"
        );
    }

    #[test]
    fn the_speaker_label_is_added_only_when_there_is_one() {
        assert_eq!(labeled(Some("Me"), "hello".into()), "Me : hello");
        assert_eq!(labeled(Some(""), "hello".into()), "hello");
        assert_eq!(labeled(None, "hello".into()), "hello");
    }

    #[test]
    fn the_vocabulary_is_cleaned_and_kept_in_order() {
        assert_eq!(
            vocabulary_terms("Orion, Minergie ;  Acme-Clima\n\n orion ,, Dupont  Menuiserie "),
            vec!["Orion", "Minergie", "Acme-Clima", "Dupont Menuiserie"]
        );
        assert_eq!(vocabulary_prompt("Orion, Minergie"), "Orion, Minergie");
        assert_eq!(vocabulary_prompt("  ,  "), "");
        assert_eq!(vocabulary_prompt(""), "");
    }

    #[test]
    fn the_hint_is_cut_at_a_whole_word_under_the_limit() {
        let raw: String = (0..200).map(|i| format!("terme{i}")).collect::<Vec<_>>().join(", ");
        let p = vocabulary_prompt(&raw);
        assert!(p.chars().count() <= MAX_PROMPT_CHARS);
        assert!(p.starts_with("terme0, terme1"));
        assert!(!p.ends_with(','));
        let last = p.rsplit(", ").next().unwrap();
        assert!(raw.split(", ").any(|t| t == last), "{last}");
    }

    #[test]
    fn a_repeated_hint_is_dropped_but_real_speech_is_kept() {
        let terms = vocabulary_terms("Orion, Minergie, Acme-Clima");
        assert!(echoes_vocabulary("Orion, Minergie, Acme-Clima.", &terms));
        assert!(echoes_vocabulary("Minergie Orion", &terms));
        // one word alone can be said for real
        assert!(!echoes_vocabulary("Orion.", &terms));
        assert!(!echoes_vocabulary("Le dossier Orion est prêt.", &terms));
        assert!(!echoes_vocabulary("Bonjour à tous", &[]));
    }

    #[test]
    fn the_engine_is_chosen_from_the_preference_the_language_and_the_vocabulary() {
        use Engine::*;
        // automatic: Parakeet only for known English, installed, and without a custom vocabulary
        assert_eq!(choose_engine("auto", Some("en"), "", true), Parakeet);
        assert_eq!(choose_engine("auto", Some("fr"), "", true), Whisper);
        assert_eq!(choose_engine("auto", None, "", true), Whisper, "language not known yet");
        assert_eq!(choose_engine("auto", Some("en"), "Orion, Minergie", true), Whisper);
        assert_eq!(choose_engine("auto", Some("en"), " , ;", true), Parakeet, "an empty vocabulary is none");
        assert_eq!(choose_engine("auto", Some("en"), "", false), Whisper, "not downloaded");
        // forced
        assert_eq!(choose_engine("whisper", Some("en"), "", true), Whisper);
        assert_eq!(choose_engine("parakeet", Some("fr"), "Orion", true), Parakeet);
        assert_eq!(choose_engine("parakeet", Some("en"), "", false), Whisper, "never without its files");
        // an unknown value behaves like automatic
        assert_eq!(choose_engine("", Some("en"), "", true), Parakeet);
    }

    /// Real voice, real engine: Parakeet transcribes English and keeps the speakers apart like Whisper does.
    /// Skipped without the Parakeet files, the "base" Whisper model or the voices.
    #[cfg(target_os = "macos")]
    #[test]
    fn parakeet_transcribes_two_english_voices_and_numbers_the_speakers() {
        use std::process::Command;
        let model = crate::models::WhisperModel::by_id("base").unwrap();
        if !model.installed() || !crate::models::parakeet_installed() {
            eprintln!("speech models not installed: test skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("nc-e2e-parakeet-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let say = |n: usize, voice: &str, text: &str| -> Option<Vec<f32>> {
            let (aiff, wav) = (dir.join(format!("{n}.aiff")), dir.join(format!("{n}.wav")));
            let a = Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).output().ok()?;
            let b = Command::new("afconvert")
                .args(["-f", "WAVE", "-d", "LEF32@16000", "-c", "1"])
                .arg(&aiff)
                .arg(&wav)
                .output()
                .ok()?;
            if !a.status.success() || !b.status.success() {
                return None;
            }
            let bytes = std::fs::read(&wav).ok()?;
            let at = bytes.windows(4).position(|w| w == b"data")? + 8;
            Some(bytes[at..].as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
        };
        let lines = [
            ("Daniel", "Good morning everyone, let us start the meeting with the budget review for the quarter."),
            ("Samantha", "Thank you Daniel, I think the project is going well and we will be ready on time."),
            ("Daniel", "Perfect, then we approve the schedule and move on to the next point on the agenda."),
        ];
        let mut audio: Vec<f32> = Vec::new();
        for (n, (voice, text)) in lines.iter().enumerate() {
            let Some(part) = say(n, voice, text) else {
                eprintln!("voice {voice} not available: test skipped");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            };
            audio.extend(vec![0.0; crate::SAMPLE_RATE / 2]);
            audio.extend(part);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let t = Transcriber::start_engine(&model.path(), "en", "", "parakeet");
        t.push_remote(audio, "Speaker");
        t.finish();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match t.events.recv_timeout(std::time::Duration::from_secs(180)) {
                Ok(Event::Text(x)) => texts.push(x),
                Ok(Event::Done) => break,
                Ok(Event::Failed(e)) => panic!("transcription failed: {e}"),
                Ok(Event::Ready) => {}
                Err(_) => panic!("no answer"),
            }
        }
        let who: Vec<&str> = texts.iter().map(|x| x.split(" : ").next().unwrap()).collect();
        assert_eq!(who, vec!["Speaker 1", "Speaker 2", "Speaker 1"], "{texts:?}");
        assert!(texts[0].to_lowercase().contains("budget") && texts[1].to_lowercase().contains("project"), "{texts:?}");
    }

    #[test]
    fn invented_text_is_dropped() {
        assert!(is_noise("[BLANK_AUDIO]"));
        assert!(is_noise("  "));
        assert!(is_noise("Thanks for watching!"));
        assert!(!is_noise("We agreed to ship on Friday."));
    }

    #[test]
    fn repeated_text_is_dropped() {
        assert!(is_noise("merci merci merci merci merci merci merci merci merci"));
        assert!(is_noise("on va voir on va voir on va voir on va voir on va voir"));
        assert!(!is_noise(
            "On a décidé de lancer la version en janvier, puis de préparer la démonstration pour le client."
        ));
    }
}
