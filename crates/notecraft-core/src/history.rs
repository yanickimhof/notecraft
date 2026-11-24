//! Generic undo/redo history of snapshots.
//!
//! The editor records a snapshot before each change. Typing is coalesced:
//! snapshots recorded with the same `group` key within `merge_window_ms`
//! replace each other, so one undo removes a whole burst of typing.

#[derive(Debug, Clone)]
pub struct History<T: Clone + PartialEq> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
    last_group: Option<(String, u128)>,
    merge_window_ms: u128,
}

impl<T: Clone + PartialEq> Default for History<T> {
    fn default() -> Self {
        Self::new(200)
    }
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit, last_group: None, merge_window_ms: 1200 }
    }

    /// Record the state *before* a change. `group` merges bursts (e.g. typing
    /// in one block); `now_ms` is a monotonic timestamp in milliseconds.
    pub fn record(&mut self, before: &T, group: Option<&str>, now_ms: u128) {
        if let (Some(g), Some((lg, t))) = (group, &self.last_group) {
            if g == lg && now_ms.saturating_sub(*t) < self.merge_window_ms {
                self.last_group = Some((g.to_string(), now_ms));
                self.redo.clear();
                return;
            }
        }
        if self.undo.last() != Some(before) {
            self.undo.push(before.clone());
            if self.undo.len() > self.limit {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_group = group.map(|g| (g.to_string(), now_ms));
    }

    /// Undo: give the current state, get the previous one.
    pub fn undo(&mut self, current: &T) -> Option<T> {
        let prev = self.undo.pop()?;
        self.redo.push(current.clone());
        self.last_group = None;
        Some(prev)
    }

    /// Redo: give the current state, get the next one.
    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        self.last_group = None;
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// End the current typing burst so the next change gets its own step.
    pub fn break_group(&mut self) {
        self.last_group = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_cycle() {
        let mut h = History::new(10);
        let mut state = 1;
        h.record(&state, None, 0);
        state = 2;
        h.record(&state, None, 10_000);
        state = 3;
        assert_eq!(h.undo(&state), Some(2));
        assert_eq!(h.undo(&2), Some(1));
        assert_eq!(h.undo(&1), None);
        assert_eq!(h.redo(&1), Some(2));
        assert_eq!(h.redo(&2), Some(3));
    }

    #[test]
    fn typing_bursts_merge() {
        let mut h = History::new(10);
        h.record(&"".to_string(), Some("b1"), 0);
        h.record(&"a".to_string(), Some("b1"), 100);
        h.record(&"ab".to_string(), Some("b1"), 200);
        assert_eq!(h.undo(&"abc".to_string()), Some("".to_string()));
        assert!(!h.can_undo());
    }

    #[test]
    fn new_change_clears_redo() {
        let mut h = History::new(10);
        h.record(&1, None, 0);
        let _ = h.undo(&2);
        assert!(h.can_redo());
        h.record(&1, None, 5000);
        assert!(!h.can_redo());
    }

    #[test]
    fn limit_is_respected() {
        let mut h = History::new(3);
        for i in 0..10 {
            h.record(&i, None, i as u128 * 10_000);
        }
        let mut n = 0;
        let mut cur = 10;
        while let Some(p) = h.undo(&cur) {
            cur = p;
            n += 1;
        }
        assert_eq!(n, 3);
    }
}
