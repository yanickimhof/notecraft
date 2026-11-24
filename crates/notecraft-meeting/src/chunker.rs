//! Decides where to cut a growing recording so each piece can be transcribed on its own,
//! preferably in a pause so no word is split.

use crate::SAMPLE_RATE;

/// Shortest piece worth transcribing.
pub const MIN_SECS: usize = 6;
/// Longest piece before a cut is forced.
pub const MAX_SECS: usize = 25;
/// Length of a window that counts as a pause.
const WINDOW: usize = SAMPLE_RATE / 5;

/// Root mean square level of `s` (0 for empty).
pub fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

/// Where to cut `pending` now, if it is time: the quietest 0.2 s window after `MIN_SECS`,
/// or the end once `MAX_SECS` is reached. Returns the number of samples to take.
pub fn next_cut(pending: &[f32]) -> Option<usize> {
    let min = MIN_SECS * SAMPLE_RATE;
    let max = MAX_SECS * SAMPLE_RATE;
    if pending.len() < min + WINDOW {
        return None;
    }
    let search_end = pending.len().min(max);
    // Look for a pause in the last part of the allowed range.
    let mut best: Option<(f32, usize)> = None;
    let mut i = min;
    while i + WINDOW <= search_end {
        let level = rms(&pending[i..i + WINDOW]);
        if best.is_none_or(|(b, _)| level < b) {
            best = Some((level, i + WINDOW / 2));
        }
        i += WINDOW / 2;
    }
    let (level, at) = best?;
    if level < 0.01 {
        return Some(at);
    }
    (pending.len() >= max).then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(secs: f32, amp: f32) -> Vec<f32> {
        (0..(secs * SAMPLE_RATE as f32) as usize).map(|i| amp * ((i as f32) * 0.05).sin()).collect()
    }

    #[test]
    fn waits_until_enough_audio() {
        assert_eq!(next_cut(&tone(3.0, 0.3)), None);
    }

    #[test]
    fn cuts_in_a_pause() {
        let mut a = tone(8.0, 0.3);
        a.extend(vec![0.0; SAMPLE_RATE]);
        a.extend(tone(2.0, 0.3));
        let cut = next_cut(&a).expect("a pause is available");
        let secs = cut as f32 / SAMPLE_RATE as f32;
        assert!((8.0..9.2).contains(&secs), "cut at {secs}");
    }

    #[test]
    fn forces_a_cut_when_nobody_stops_talking() {
        let a = tone(26.0, 0.3);
        assert!(next_cut(&a).is_some());
        assert_eq!(next_cut(&tone(12.0, 0.3)), None);
    }
}
