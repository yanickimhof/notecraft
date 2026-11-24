//! Watching a workspace folder for changes made by someone else.
//!
//! Uses the system's file events (FSEvents on macOS) through `notify`, falls
//! back to `notify`'s polling watcher, and finally to plain timed polling.
//! Events are only a hint: the caller then compares file stamps
//! ([`crate::Workspace::external_changes`]) to know what really changed, so
//! our own writes never look like outside edits.

use notify::Watcher as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// How changes are noticed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchMode {
    /// System file events.
    Events,
    /// `notify`'s polling watcher.
    PollWatcher,
    /// Timed checks of file stamps by the caller.
    Timer,
}

/// Is this path part of the workspace content (pages, databases)?
/// Temporary files of atomic writes, hidden files and attachments are not.
pub fn is_relevant(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        // events may come with resolved paths (/private/var vs /var)
        let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        return path.strip_prefix(&canon).is_ok_and(relevant_rel);
    };
    relevant_rel(rel)
}

fn relevant_rel(rel: &Path) -> bool {
    let mut comps = rel.components();
    let Some(first) = comps.next() else { return false };
    let first = first.as_os_str().to_string_lossy();
    if first != crate::workspace::PAGES_DIR && first != crate::workspace::DBS_DIR {
        return false;
    }
    let name = rel.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if name.starts_with('.') || name.ends_with(".tmp") {
        return false;
    }
    // a folder event (database created or removed) or a content file
    name.ends_with(".md") || name.ends_with(".json") || rel.extension().is_none()
}

/// Waits for events to calm down before reporting, so a burst of writes
/// (a git checkout, a sync of many files) causes one reload.
#[derive(Debug, Clone)]
pub struct Debouncer {
    quiet: Duration,
    pending_since: Option<Instant>,
    last_event: Option<Instant>,
}

impl Debouncer {
    pub fn new(quiet: Duration) -> Self {
        Debouncer { quiet, pending_since: None, last_event: None }
    }

    /// Note an event at `now`.
    pub fn event(&mut self, now: Instant) {
        self.pending_since.get_or_insert(now);
        self.last_event = Some(now);
    }

    /// True once, when events happened and none came for `quiet` (or they
    /// kept coming for more than 4 × `quiet`).
    pub fn ready(&mut self, now: Instant) -> bool {
        let (Some(first), Some(last)) = (self.pending_since, self.last_event) else { return false };
        if now.duration_since(last) >= self.quiet || now.duration_since(first) >= self.quiet * 4 {
            self.pending_since = None;
            self.last_event = None;
            return true;
        }
        false
    }

    pub fn is_pending(&self) -> bool {
        self.pending_since.is_some()
    }
}

/// Interval of the polling fallbacks.
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Quiet time before a burst of events is reported.
pub const QUIET: Duration = Duration::from_millis(300);

pub struct Watcher {
    root: PathBuf,
    mode: WatchMode,
    _inner: Option<Box<dyn notify::Watcher + Send>>,
    rx: Option<Receiver<PathBuf>>,
    debounce: Debouncer,
    last_timer: Instant,
}

impl Watcher {
    /// Start watching `root` (pages/ and databases/ recursively).
    pub fn new(root: &Path) -> Watcher {
        let (tx, rx) = channel::<PathBuf>();
        let handler = move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                for p in ev.paths {
                    let _ = tx.send(p);
                }
            }
        };
        let mut mode = WatchMode::Timer;
        let mut inner: Option<Box<dyn notify::Watcher + Send>> = None;
        if let Ok(mut w) = notify::recommended_watcher(handler.clone()) {
            if w.watch(root, notify::RecursiveMode::Recursive).is_ok() {
                inner = Some(Box::new(w));
                mode = WatchMode::Events;
            }
        }
        if inner.is_none() {
            let cfg = notify::Config::default().with_poll_interval(POLL_INTERVAL);
            if let Ok(mut w) = notify::PollWatcher::new(handler, cfg) {
                if w.watch(root, notify::RecursiveMode::Recursive).is_ok() {
                    inner = Some(Box::new(w));
                    mode = WatchMode::PollWatcher;
                }
            }
        }
        Watcher {
            root: root.to_path_buf(),
            mode,
            rx: inner.as_ref().map(|_| rx),
            _inner: inner,
            debounce: Debouncer::new(QUIET),
            last_timer: Instant::now(),
        }
    }

    pub fn mode(&self) -> WatchMode {
        self.mode
    }

    /// Call often (every frame is fine). True when the caller should compare
    /// file stamps and reload what changed.
    pub fn poll(&mut self) -> bool {
        let now = Instant::now();
        if let Some(rx) = &self.rx {
            while let Ok(p) = rx.try_recv() {
                if is_relevant(&self.root, &p) {
                    self.debounce.event(now);
                }
            }
        }
        if self.mode == WatchMode::Timer && now.duration_since(self.last_timer) >= POLL_INTERVAL {
            self.last_timer = now;
            return true;
        }
        self.debounce.ready(now)
    }

    /// Events arrived and are waiting for the quiet time (keep repainting).
    pub fn is_pending(&self) -> bool {
        self.debounce.is_pending() || self.mode == WatchMode::Timer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance_filter() {
        let root = Path::new("/w");
        assert!(is_relevant(root, Path::new("/w/pages/a-1.md")));
        assert!(is_relevant(root, Path::new("/w/databases/tasks-1/rows/x-2.md")));
        assert!(is_relevant(root, Path::new("/w/databases/tasks-1/schema.json")));
        assert!(is_relevant(root, Path::new("/w/databases/tasks-1")));
        assert!(!is_relevant(root, Path::new("/w/pages/.a-1.md.abc.tmp")));
        assert!(!is_relevant(root, Path::new("/w/assets/pic.png")));
        assert!(!is_relevant(root, Path::new("/w/.git/index")));
        assert!(!is_relevant(root, Path::new("/elsewhere/pages/a.md")));
    }

    #[test]
    fn debouncer_waits_for_quiet() {
        let mut d = Debouncer::new(Duration::from_millis(100));
        let t0 = Instant::now();
        assert!(!d.ready(t0));
        d.event(t0);
        assert!(!d.ready(t0 + Duration::from_millis(50)));
        d.event(t0 + Duration::from_millis(80));
        assert!(!d.ready(t0 + Duration::from_millis(150)));
        assert!(d.ready(t0 + Duration::from_millis(200)));
        assert!(!d.ready(t0 + Duration::from_millis(300)), "reported once");
        // a never-ending burst is still reported after 4 × quiet
        let mut d = Debouncer::new(Duration::from_millis(100));
        for k in 0..10 {
            d.event(t0 + Duration::from_millis(k * 50));
        }
        assert!(d.ready(t0 + Duration::from_millis(450)));
    }

    #[test]
    fn real_changes_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("pages")).unwrap();
        let mut w = Watcher::new(dir.path());
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(dir.path().join("pages/new-page.md"), "hello").unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut seen = false;
        while Instant::now() < deadline {
            if w.poll() {
                seen = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(seen, "no change reported (mode {:?})", w.mode());
    }
}
