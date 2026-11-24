//! Short, sortable, collision-resistant identifiers (no network, no extra crates).
//!
//! An id is 12 lowercase base-36 characters: 8 from the current time in
//! milliseconds and 4 from a per-process counter mixed with the process id.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);
static SEED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

fn base36(mut n: u64, width: usize) -> String {
    let mut out = vec![b'0'; width];
    for slot in out.iter_mut().rev() {
        *slot = ALPHABET[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8(out).expect("ascii")
}

/// A fresh identifier such as `mf3k2a9z01b7`.
pub fn new_id() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    // A per-process seed keeps two processes apart; the counter keeps ids
    // made in the same millisecond apart.
    let seed = *SEED.get_or_init(|| {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0);
        nanos ^ (std::process::id() as u64).wrapping_mul(2_654_435_761)
    });
    let tail = seed.wrapping_add(c) % 36u64.pow(4);
    format!("{}{}", base36(ms % 36u64.pow(8), 8), base36(tail, 4))
}

/// True when `s` looks like an id produced by [`new_id`] (or any short token).
pub fn is_valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_valid() {
        let mut seen = HashSet::new();
        for _ in 0..5000 {
            let id = new_id();
            assert_eq!(id.len(), 12);
            assert!(is_valid_id(&id));
            assert!(seen.insert(id));
        }
    }

    #[test]
    fn base36_pads() {
        assert_eq!(base36(0, 3), "000");
        assert_eq!(base36(35, 2), "0z");
    }
}
