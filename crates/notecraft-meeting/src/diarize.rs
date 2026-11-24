//! Tells voices apart without any model: a short voice print (the average spectral shape of the voiced
//! parts, plus the pitch) is computed for each stretch of speech, and stretches whose prints are close
//! are given the same speaker number. It separates clearly different voices (a man and a woman, a deep
//! and a high voice) and is unsure about similar ones, so it is a help to read a transcript, not proof.

use crate::SAMPLE_RATE;
use std::f32::consts::PI;
use std::sync::OnceLock;

const FRAME: usize = 400; // 25 ms
const HOP: usize = 160; // 10 ms
const NFFT: usize = 512;
const MELS: usize = 26;
/// Cepstral coefficients kept (c1 to c12; c0 is loudness).
const COEFFS: usize = 12;
/// Fewest voiced frames a print needs (0.8 s of speech); shorter stretches are not told apart,
/// they go to whoever spoke before (a "yes" or a "hm" is not worth a new speaker).
const MIN_FRAMES: usize = 80;
/// Most speakers told apart in one meeting; beyond this a voice joins the nearest one.
pub const MAX_SPEAKERS: usize = 8;
/// Distance above which two prints are two people.
const SAME_VOICE: f32 = 2.5;
/// How much a pitch difference of one octave weighs, against the spectral shape.
const PITCH_WEIGHT: f32 = 3.0;
/// Spectral distance that counts as 1.0.
const SHAPE_SCALE: f32 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct VoicePrint {
    mfcc: [f32; COEFFS],
    /// Median pitch (natural log of Hz), when the voice was clear enough to measure.
    log_f0: Option<f32>,
    frames: usize,
}

fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

fn mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn unmel(m: f32) -> f32 {
    700.0 * (10f32.powf(m / 2595.0) - 1.0)
}

/// Triangular mel filters over the positive half of the spectrum, 100 Hz to 7 kHz.
fn filters() -> &'static Vec<Vec<f32>> {
    static F: OnceLock<Vec<Vec<f32>>> = OnceLock::new();
    F.get_or_init(|| {
        let bins = NFFT / 2 + 1;
        let (lo, hi) = (mel(100.0), mel(7000.0));
        let pts: Vec<f32> = (0..MELS + 2).map(|i| unmel(lo + (hi - lo) * i as f32 / (MELS + 1) as f32)).collect();
        let at = |hz: f32| hz * NFFT as f32 / SAMPLE_RATE as f32;
        (0..MELS)
            .map(|m| {
                let (a, b, c) = (at(pts[m]), at(pts[m + 1]), at(pts[m + 2]));
                (0..bins)
                    .map(|k| {
                        let k = k as f32;
                        if k <= a || k >= c {
                            0.0
                        } else if k <= b {
                            (k - a) / (b - a)
                        } else {
                            (c - k) / (c - b)
                        }
                    })
                    .collect()
            })
            .collect()
    })
}

/// Pitch of one frame by autocorrelation, 70 to 400 Hz; `None` when the frame is not clearly voiced.
fn pitch(frame: &[f32]) -> Option<f32> {
    let (min_lag, max_lag) = (SAMPLE_RATE / 400, SAMPLE_RATE / 70);
    if frame.len() <= max_lag + 20 {
        return None;
    }
    let energy: f32 = frame.iter().map(|x| x * x).sum();
    if energy <= 1e-9 {
        return None;
    }
    let mut best = (0.0f32, 0usize);
    for lag in min_lag..=max_lag {
        let n = frame.len() - lag;
        let r: f32 = (0..n).map(|i| frame[i] * frame[i + lag]).sum();
        let norm =
            (frame[..n].iter().map(|x| x * x).sum::<f32>() * frame[lag..].iter().map(|x| x * x).sum::<f32>()).sqrt();
        let r = if norm > 0.0 { r / norm } else { 0.0 };
        if r > best.0 {
            best = (r, lag);
        }
    }
    (best.0 > 0.5).then(|| SAMPLE_RATE as f32 / best.1 as f32)
}

/// The print of a stretch of speech (16 kHz mono); `None` when there is too little voiced sound.
pub fn voice_print(samples: &[f32]) -> Option<VoicePrint> {
    if samples.len() < FRAME + HOP {
        return None;
    }
    let starts: Vec<usize> = (0..=samples.len() - FRAME).step_by(HOP).collect();
    let rms: Vec<f32> = starts.iter().map(|&s| crate::chunker::rms(&samples[s..s + FRAME])).collect();
    let mut sorted = rms.clone();
    sorted.sort_by(f32::total_cmp);
    let loud = sorted[sorted.len() * 3 / 4];
    let gate = (0.5 * loud).max(0.003);
    let window: Vec<f32> = (0..FRAME).map(|i| 0.54 - 0.46 * (2.0 * PI * i as f32 / (FRAME - 1) as f32).cos()).collect();
    let fb = filters();
    let mut sum = [0.0f32; COEFFS];
    let mut frames = 0usize;
    let mut pitches: Vec<f32> = Vec::new();
    for (k, &s) in starts.iter().enumerate() {
        if rms[k] < gate {
            continue;
        }
        let mut re = vec![0.0f32; NFFT];
        let mut im = vec![0.0f32; NFFT];
        for i in 0..FRAME {
            // pre-emphasis lifts the high part of the spectrum, where the voice's colour is
            let prev = if s + i > 0 { samples[s + i - 1] } else { 0.0 };
            re[i] = (samples[s + i] - 0.97 * prev) * window[i];
        }
        fft(&mut re, &mut im);
        let power: Vec<f32> = (0..=NFFT / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
        let logmel: Vec<f32> =
            fb.iter().map(|f| (f.iter().zip(&power).map(|(w, p)| w * p).sum::<f32>() + 1e-6).ln()).collect();
        for (c, acc) in sum.iter_mut().enumerate() {
            let k = (c + 1) as f32;
            *acc += logmel
                .iter()
                .enumerate()
                .map(|(m, v)| v * (PI * k * (m as f32 + 0.5) / MELS as f32).cos())
                .sum::<f32>();
        }
        frames += 1;
        // the pitch is slow to measure and changes slowly: every fourth voiced frame is enough
        if frames % 4 == 1 {
            let end = (s + 640).min(samples.len());
            if let Some(f) = pitch(&samples[s..end]) {
                pitches.push(f);
            }
        }
    }
    if frames < MIN_FRAMES {
        return None;
    }
    let mut mfcc = [0.0f32; COEFFS];
    for (m, s) in mfcc.iter_mut().zip(sum) {
        *m = s / frames as f32;
    }
    pitches.sort_by(f32::total_cmp);
    let log_f0 = (pitches.len() >= 6).then(|| pitches[pitches.len() / 2].ln());
    Some(VoicePrint { mfcc, log_f0, frames })
}

/// How far apart two voices are: under 1.0 is the same person.
pub fn distance(a: &VoicePrint, b: &VoicePrint) -> f32 {
    let shape = a.mfcc.iter().zip(&b.mfcc).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt() / SHAPE_SCALE;
    let pitch = match (a.log_f0, b.log_f0) {
        (Some(x), Some(y)) => (x - y).abs() / std::f32::consts::LN_2 * PITCH_WEIGHT,
        _ => 0.0,
    };
    (shape * shape + pitch * pitch).sqrt() / SAME_VOICE
}

/// Numbers the voices of one meeting, in order of appearance (0 is the first voice heard).
#[derive(Default)]
pub struct Diarizer {
    speakers: Vec<VoicePrint>,
    last: usize,
}

impl Diarizer {
    /// The speaker of a stretch of speech. A stretch too short to tell goes to whoever spoke last.
    pub fn assign(&mut self, print: Option<VoicePrint>) -> usize {
        let Some(p) = print else { return self.last };
        let nearest =
            self.speakers.iter().enumerate().map(|(i, s)| (i, distance(s, &p))).min_by(|a, b| a.1.total_cmp(&b.1));
        let id = match nearest {
            Some((i, d)) if d < 1.0 || self.speakers.len() >= MAX_SPEAKERS => {
                let s = &mut self.speakers[i];
                // the centre of a speaker follows their voice, weighted by how much of it was heard
                let (wa, wb) = (s.frames as f32, p.frames as f32);
                for (a, b) in s.mfcc.iter_mut().zip(p.mfcc) {
                    *a = (*a * wa + b * wb) / (wa + wb);
                }
                s.log_f0 = match (s.log_f0, p.log_f0) {
                    (Some(a), Some(b)) => Some((a * wa + b * wb) / (wa + wb)),
                    (a, b) => a.or(b),
                };
                s.frames += p.frames;
                i
            }
            _ => {
                self.speakers.push(p);
                self.speakers.len() - 1
            }
        };
        self.last = id;
        id
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A synthetic voice: a pitch and a vocal tract length (the formants scale with it), saying a run of
    /// random vowels; `seed` changes the vowels, so two sentences of one person differ.
    pub(crate) fn voice(f0: f32, tract: f32, seed: usize, secs: f32) -> Vec<f32> {
        let n = (secs * SAMPLE_RATE as f32) as usize;
        let seg = SAMPLE_RATE / 8;
        let mut state = 0x9e3779b97f4a7c15u64 ^ (seed as u64).wrapping_mul(0xd1b54a32d192ed03);
        let mut rnd = move |lo: f32, hi: f32| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            lo + (hi - lo) * ((state >> 40) as f32 / (1u64 << 24) as f32)
        };
        let mut out = Vec::with_capacity(n);
        let mut phase_t = 0usize;
        while out.len() < n {
            let v = [rnd(400.0, 700.0), rnd(1100.0, 2000.0), rnd(2300.0, 2800.0)];
            let mut part: Vec<f32> = (0..seg)
                .map(|_| {
                    let t = phase_t as f32 / SAMPLE_RATE as f32;
                    phase_t += 1;
                    let f0 = f0 * (1.0 + 0.04 * (2.0 * PI * 0.7 * t + seed as f32).sin());
                    let mut x = 0.0;
                    let mut h = 1;
                    while (h as f32) * f0 < 5000.0 {
                        let hz = h as f32 * f0;
                        let env: f32 =
                            v.iter().map(|f| (-((hz - f * tract) / (120.0 + 0.1 * f * tract)).powi(2)).exp()).sum();
                        x += env / h as f32 * (2.0 * PI * hz * t).sin();
                        h += 1;
                    }
                    x
                })
                .collect();
            let level = crate::chunker::rms(&part).max(1e-6);
            part.iter_mut().for_each(|x| *x *= 0.1 / level);
            out.extend(part);
        }
        out.truncate(n);
        out
    }

    fn print(s: &[f32]) -> VoicePrint {
        voice_print(s).expect("enough speech")
    }

    #[test]
    fn a_deep_voice_and_a_high_voice_are_far_apart() {
        let man = print(&voice(115.0, 1.0, 1, 4.0));
        let woman = print(&voice(210.0, 1.17, 2, 4.0));
        let d = distance(&man, &woman);
        assert!(d > 1.5, "different voices, distance {d}");
    }

    #[test]
    fn a_deeper_voice_is_farther_from_a_voice_than_the_voice_is_from_itself() {
        let a = print(&voice(105.0, 1.0, 1, 4.0));
        let b = print(&voice(150.0, 1.08, 1, 4.0));
        let same = distance(&a, &print(&voice(105.0, 1.0, 2, 4.0)));
        let d = distance(&a, &b);
        assert!(d > same + 0.1, "two men, distance {d}, against {same} for one man");
    }

    #[test]
    fn speakers_are_numbered_in_order_of_appearance_and_recognised_when_they_come_back() {
        let mut d = Diarizer::default();
        let ids: Vec<usize> = [(115.0, 1.0, 1), (210.0, 1.17, 1), (115.0, 1.0, 2), (210.0, 1.17, 3), (115.0, 1.0, 4)]
            .iter()
            .map(|&(f, t, s)| d.assign(voice_print(&voice(f, t, s, 3.0))))
            .collect();
        assert_eq!(ids, vec![0, 1, 0, 1, 0]);
    }

    #[test]
    fn too_little_speech_or_silence_gives_no_print_and_goes_to_the_last_speaker() {
        assert!(voice_print(&voice(120.0, 1.0, 1, 0.2)).is_none(), "0.2 s is too short");
        assert!(voice_print(&vec![0.0; SAMPLE_RATE * 3]).is_none(), "silence");
        let mut d = Diarizer::default();
        assert_eq!(d.assign(None), 0);
        d.assign(voice_print(&voice(210.0, 1.17, 1, 3.0)));
        d.assign(voice_print(&voice(115.0, 1.0, 1, 3.0)));
        assert_eq!(d.assign(None), 1, "the short interjection is put with the previous speaker");
    }

    #[test]
    fn there_is_a_ceiling_on_the_number_of_speakers() {
        let mut d = Diarizer::default();
        for i in 0..20 {
            d.assign(voice_print(&voice(90.0 + i as f32 * 15.0, 0.9 + i as f32 * 0.02, i, 3.0)));
        }
        assert!(d.speakers.len() <= MAX_SPEAKERS);
    }

    #[test]
    fn the_fft_finds_a_pure_tone() {
        let n = 512;
        let mut re: Vec<f32> = (0..n).map(|i| (2.0 * PI * 32.0 * i as f32 / n as f32).sin()).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mags: Vec<f32> = (0..n / 2).map(|k| re[k].hypot(im[k])).collect();
        let peak = mags.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(peak, 32);
    }

    /// With the real voices of this Mac (text to speech): who is who must come out right, in sentences
    /// cut into pieces of three seconds and mixed up. Skipped where the voices are not installed.
    #[cfg(target_os = "macos")]
    #[test]
    fn real_voices_are_told_apart_in_mixed_up_pieces() {
        use std::process::Command;
        let dir = std::env::temp_dir().join(format!("nc-voices-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let text = "Bonjour à tous, merci d'être venus. Nous allons commencer par le point sur le budget du trimestre, puis nous parlerons du calendrier. Je pense que le projet avance bien, mais il faudra encore valider les tests avant la fin du mois.";
        let mut pieces: Vec<(usize, Vec<f32>)> = Vec::new();
        for (who, voice) in ["Thomas", "Amélie", "Flo (French (France))"].iter().enumerate() {
            let (aiff, wav) = (dir.join(format!("{who}.aiff")), dir.join(format!("{who}.wav")));
            let said = Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).output();
            let conv = Command::new("afconvert")
                .args(["-f", "WAVE", "-d", "LEF32@16000", "-c", "1"])
                .arg(&aiff)
                .arg(&wav)
                .output();
            if !said.is_ok_and(|o| o.status.success()) || !conv.is_ok_and(|o| o.status.success()) {
                eprintln!("voice {voice} not available: test skipped");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
            let b = std::fs::read(&wav).unwrap();
            let at = b.windows(4).position(|w| w == b"data").unwrap() + 8;
            let all: Vec<f32> = b[at..].as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
            pieces
                .extend(all.chunks(3 * SAMPLE_RATE).filter(|c| c.len() == 3 * SAMPLE_RATE).map(|c| (who, c.to_vec())));
        }
        let _ = std::fs::remove_dir_all(&dir);
        // a fixed shuffle, so the test is the same every time
        let mut order: Vec<usize> = (0..pieces.len()).collect();
        let mut x = 99u64;
        for i in (1..order.len()).rev() {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            order.swap(i, (x >> 33) as usize % (i + 1));
        }
        let mut d = Diarizer::default();
        let mut seen: std::collections::HashMap<usize, usize> = Default::default();
        for i in order {
            let id = d.assign(voice_print(&pieces[i].1));
            let who = *seen.entry(pieces[i].0).or_insert(id);
            assert_eq!(who, id, "a piece of voice {} was given to another speaker", pieces[i].0);
        }
        let mut ids: Vec<usize> = seen.values().copied().collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3, "three people, three speakers");
    }
}
