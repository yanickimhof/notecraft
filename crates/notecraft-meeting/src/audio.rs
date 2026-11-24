//! Microphone capture. Audio is converted on the fly to what Whisper needs: 16 kHz mono.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::SAMPLE_RATE;

/// Turns audio at any sample rate into 16 kHz mono, one callback buffer at a time.
pub struct Resampler {
    ratio: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    pub fn new(from_rate: u32) -> Self {
        Resampler { ratio: f64::from(from_rate) / SAMPLE_RATE as f64, pos: 0.0, last: 0.0 }
    }

    /// `input` is interleaved with `channels` channels; they are averaged into one.
    pub fn push(&mut self, input: &[f32], channels: usize, out: &mut Vec<f32>) {
        let channels = channels.max(1);
        let mono: Vec<f32> = input.chunks(channels).map(|f| f.iter().sum::<f32>() / f.len() as f32).collect();
        // `pos` is a position in the stream; it starts at 0 and carries the fraction between calls.
        while self.pos < mono.len() as f64 {
            let i = self.pos.floor();
            let frac = (self.pos - i) as f32;
            let a = if i < 0.0 { self.last } else { mono[i as usize] };
            let b = mono.get(i as usize + 1).copied().unwrap_or(a);
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        self.pos -= mono.len() as f64;
        if let Some(l) = mono.last() {
            self.last = *l;
        }
    }
}

/// What to record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    /// The microphone only.
    #[default]
    Microphone,
    /// Everything the Mac plays (the other people in a video call).
    System,
    /// Both, mixed into one transcript.
    Both,
}

impl Source {
    pub fn id(self) -> &'static str {
        match self {
            Source::Microphone => "mic",
            Source::System => "system",
            Source::Both => "both",
        }
    }

    /// Unknown ids mean the microphone.
    pub fn from_id(id: &str) -> Source {
        match id {
            "system" => Source::System,
            "both" => Source::Both,
            _ => Source::Microphone,
        }
    }

    pub fn uses_mic(self) -> bool {
        self != Source::System
    }

    pub fn uses_system(self) -> bool {
        self != Source::Microphone
    }
}

/// Names of the microphones this Mac offers, and the name of the default one.
#[cfg(target_os = "macos")]
pub fn input_devices() -> (Vec<String>, Option<String>) {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let list = host.input_devices().map(|it| it.filter_map(|d| device_name(&d)).collect()).unwrap_or_default();
    (list, host.default_input_device().and_then(|d| device_name(&d)))
}

#[cfg(target_os = "macos")]
fn device_name(d: &cpal::Device) -> Option<String> {
    use cpal::traits::DeviceTrait;
    d.description().ok().map(|x| x.name().to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn input_devices() -> (Vec<String>, Option<String>) {
    (Vec::new(), None)
}

/// One captured source: its samples (16 kHz mono) and what the meter and diagnostics need.
#[derive(Default)]
struct Track {
    samples: Mutex<Vec<f32>>,
    /// Latest loudness as `f32` bits, for the level meter.
    level: AtomicU32,
    /// Largest raw input value seen and number of audio callbacks (to tell a silent device from a dead one).
    peak: AtomicU32,
    callbacks: std::sync::atomic::AtomicU64,
}

impl Track {
    fn take(&self, n: Option<usize>) -> Vec<f32> {
        let Ok(mut s) = self.samples.lock() else { return Vec::new() };
        match n {
            Some(n) if n < s.len() => s.drain(..n).collect(),
            _ => std::mem::take(&mut *s),
        }
    }

    fn len(&self) -> usize {
        self.samples.lock().map(|s| s.len()).unwrap_or(0)
    }

    fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// A running recording.
pub struct Recorder {
    mic: Arc<Track>,
    system: Arc<Track>,
    source: Source,
    #[cfg(target_os = "macos")]
    _streams: Vec<cpal::Stream>,
    started: std::time::Instant,
    device: String,
}

#[cfg(target_os = "macos")]
fn open_stream(
    device: &cpal::Device,
    cfg: cpal::SupportedStreamConfig,
    track: Arc<Track>,
) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    let channels = cfg.channels() as usize;
    let mut rs = Resampler::new(cfg.sample_rate());
    let mut handle = move |data: &[f32]| {
        track.callbacks.fetch_add(1, Ordering::Relaxed);
        let p = data.iter().fold(0f32, |m, x| m.max(x.abs()));
        if p > f32::from_bits(track.peak.load(Ordering::Relaxed)) {
            track.peak.store(p.to_bits(), Ordering::Relaxed);
        }
        let mut out = Vec::with_capacity(data.len() / channels.max(1) + 8);
        rs.push(data, channels, &mut out);
        track.level.store(crate::chunker::rms(&out).to_bits(), Ordering::Relaxed);
        if let Ok(mut s) = track.samples.lock() {
            s.extend_from_slice(&out);
        }
    };
    let err_fn = |e| eprintln!("audio stream error: {e}");
    let stream_cfg: cpal::StreamConfig = cfg.into();
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(stream_cfg, move |d: &[f32], _| handle(d), err_fn, None),
        cpal::SampleFormat::I16 => device.build_input_stream(
            stream_cfg,
            move |d: &[i16], _| handle(&d.iter().map(|x| f32::from(*x) / 32768.0).collect::<Vec<_>>()),
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            stream_cfg,
            move |d: &[u16], _| handle(&d.iter().map(|x| (f32::from(*x) - 32768.0) / 32768.0).collect::<Vec<_>>()),
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported audio format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.start().map_err(|e| e.to_string())?;
    Ok(stream)
}

impl Recorder {
    /// Records the default microphone.
    pub fn start() -> Result<Recorder, String> {
        Recorder::start_with(None, Source::Microphone)
    }

    /// Records `source`. `name` is the microphone called so by [`input_devices`]; `None`, empty or unknown
    /// means the system default. System audio is what the default output device plays (macOS 14.6 or later).
    #[cfg(target_os = "macos")]
    pub fn start_with(name: Option<&str>, source: Source) -> Result<Recorder, String> {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let mic = Arc::new(Track::default());
        let system = Arc::new(Track::default());
        let mut streams = Vec::new();
        let mut device = String::new();
        if source.uses_mic() {
            let chosen = name
                .filter(|n| !n.is_empty())
                .and_then(|n| host.input_devices().ok()?.find(|d| device_name(d).is_some_and(|dn| dn == n)));
            let dev = chosen.or_else(|| host.default_input_device()).ok_or("no microphone found")?;
            let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
            streams.push(open_stream(&dev, cfg, mic.clone())?);
            device = device_name(&dev).unwrap_or_default();
        }
        if source.uses_system() {
            let dev = host.default_output_device().ok_or("no audio output found")?;
            let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
            streams.push(open_stream(&dev, cfg, system.clone())?);
            if device.is_empty() {
                device = device_name(&dev).unwrap_or_default();
            }
        }
        Ok(Recorder { mic, system, source, _streams: streams, started: std::time::Instant::now(), device })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn start_with(_name: Option<&str>, _source: Source) -> Result<Recorder, String> {
        Err("recording is only available on macOS".into())
    }

    /// Takes what both sources have captured so far (16 kHz mono), mixed. The two run on their own clocks,
    /// so only the part both have reached is taken, unless one of them stalls for more than a second.
    pub fn take(&self) -> Vec<f32> {
        match self.source {
            Source::Microphone => self.mic.take(None),
            Source::System => self.system.take(None),
            Source::Both => {
                let (a, b) = (self.mic.len(), self.system.len());
                let n = if a.abs_diff(b) > crate::SAMPLE_RATE { a.max(b) } else { a.min(b) };
                mix(self.mic.take(Some(n)), self.system.take(Some(n)))
            }
        }
    }

    /// Like `take`, but the microphone (the person) and the system audio (the others in the call) stay
    /// apart, so each can be transcribed under its own name. Only meaningful for `Source::Both`.
    pub fn take_split(&self) -> (Vec<f32>, Vec<f32>) {
        take_aligned(&self.mic, &self.system)
    }

    /// Everything left of each source, kept apart (used when the recording stops).
    pub fn take_all_split(&self) -> (Vec<f32>, Vec<f32>) {
        (self.mic.take(None), self.system.take(None))
    }

    /// Takes everything left, whichever source is ahead (used when the recording stops).
    pub fn take_all(&self) -> Vec<f32> {
        mix(self.mic.take(None), self.system.take(None))
    }

    /// Current loudness of the loudest source, roughly 0.0 to 0.5 for speech.
    pub fn level(&self) -> f32 {
        match self.source {
            Source::Microphone => self.mic.level(),
            Source::System => self.system.level(),
            Source::Both => self.mic.level().max(self.system.level()),
        }
    }

    /// (audio callbacks so far, largest raw sample seen) of the microphone, or of the system audio when
    /// that is all that is recorded: both 0 means the device delivers nothing at all.
    pub fn stats(&self) -> (u64, f32) {
        let t = if self.source.uses_mic() { &self.mic } else { &self.system };
        (t.callbacks.load(Ordering::Relaxed), f32::from_bits(t.peak.load(Ordering::Relaxed)))
    }

    /// Audio callbacks of the system audio so far: 0 after a few seconds means macOS delivers nothing
    /// (usually a permission that has not been granted).
    pub fn system_callbacks(&self) -> u64 {
        self.system.callbacks.load(Ordering::Relaxed)
    }

    pub fn source(&self) -> Source {
        self.source
    }

    /// Name of the microphone in use (or of the output device for system audio only).
    pub fn device_name(&self) -> &str {
        &self.device
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.started.elapsed()
    }
}

/// Sums two tracks, padding the shorter one with silence, and keeps the result within -1 and 1.
/// Takes from both tracks the part they have both reached, unless one stalls for over a second.
fn take_aligned(mic: &Track, system: &Track) -> (Vec<f32>, Vec<f32>) {
    let (a, b) = (mic.len(), system.len());
    let n = if a.abs_diff(b) > crate::SAMPLE_RATE { a.max(b) } else { a.min(b) };
    (mic.take(Some(n)), system.take(Some(n)))
}

fn mix(mut a: Vec<f32>, b: Vec<f32>) -> Vec<f32> {
    if b.is_empty() {
        return a;
    }
    if a.len() < b.len() {
        a.resize(b.len(), 0.0);
    }
    for (x, y) in a.iter_mut().zip(b) {
        *x = (*x + y).clamp(-1.0, 1.0);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_48k_stereo_to_16k_mono() {
        let mut rs = Resampler::new(48_000);
        let frames = 4800; // 0.1 s
        let input: Vec<f32> = (0..frames * 2).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }).collect();
        let mut out = Vec::new();
        rs.push(&input, 2, &mut out);
        assert!((out.len() as i32 - 1600).abs() <= 2, "got {}", out.len());
        assert!(out.iter().all(|x| x.abs() < 1e-6), "channels average to silence");
    }

    fn track(samples: Vec<f32>) -> Track {
        let t = Track::default();
        *t.samples.lock().unwrap() = samples;
        t
    }

    #[test]
    fn split_tracks_stay_apart_and_wait_for_each_other() {
        // close together: only what both have reached is taken, the rest waits
        let (mic, sys) = (track(vec![0.1; 16_000 * 3]), track(vec![0.9; 16_000 * 3 + 500]));
        let (m, s) = take_aligned(&mic, &sys);
        assert_eq!((m.len(), s.len()), (48_000, 48_000));
        assert!(m.iter().all(|x| *x == 0.1) && s.iter().all(|x| *x == 0.9), "never mixed");
        assert_eq!((mic.len(), sys.len()), (0, 500));
        // one stalled for more than a second: the other one is not held back
        let (mic, sys) = (track(vec![0.1; 16_000 * 5]), track(vec![0.9; 16_000]));
        let (m, s) = take_aligned(&mic, &sys);
        assert_eq!((m.len(), s.len()), (80_000, 16_000));
    }

    #[test]
    fn mixes_two_tracks_and_pads_the_shorter() {
        let m = mix(vec![0.5, 0.5, 0.5], vec![0.25, 1.0]);
        assert_eq!(m, vec![0.75, 1.0, 0.5]);
        assert_eq!(mix(vec![0.1], Vec::new()), vec![0.1]);
        assert_eq!(Source::from_id("both"), Source::Both);
        assert_eq!(Source::from_id("anything"), Source::Microphone);
        assert!(Source::System.uses_system() && !Source::System.uses_mic());
    }

    #[test]
    fn keeps_the_fraction_between_calls() {
        let mut rs = Resampler::new(44_100);
        let mut total = 0;
        for _ in 0..100 {
            let mut out = Vec::new();
            rs.push(&vec![0.1; 441], 1, &mut out);
            total += out.len();
        }
        assert!((total as i32 - 16_000).abs() <= 3, "got {total}");
    }
}
