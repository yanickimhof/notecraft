//! Scheduled backups and page versions.
//!
//! A backup is an ordinary snapshot (see `snapshot.rs`) holding the files that changed since the
//! previous backup, so the first one is complete and the next ones are small. Because every
//! backup keeps the versions of the files it contains, the same snapshots give a version history
//! for each page and a point to restore from.
//!
//! The work needs only paths, so it can run on a background thread without the workspace.

use crate::snapshot::{list_snapshots_in, write_snapshot, SnapshotInfo};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// `reason` of the snapshots made by the scheduler.
pub const AUTO_REASON: &str = "auto";

/// Files above this size (attachments) are not copied.
const MAX_FILE_BYTES: u64 = 5_000_000;

/// Retention, in the manner of Time Machine: one automatic backup per hour for the last day,
/// one per day for a month, one per week for a year, then one per year.
const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
const YEAR: i64 = 365 * DAY;

#[derive(Default, Serialize, Deserialize)]
struct State {
    /// Workspace-relative path → (modified seconds, size) as of the last backup.
    files: BTreeMap<String, (u64, u64)>,
}

fn state_path(base: &Path) -> PathBuf {
    base.join("auto-state.json")
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<(PathBuf, String, u64, u64)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let p = e.path();
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            walk(&p, root, out);
        } else if meta.is_file() && meta.len() <= MAX_FILE_BYTES {
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            out.push((p.clone(), rel.to_string_lossy().to_string(), mtime, meta.len()));
        }
    }
}

/// Back up the files that changed since the last backup. `None` when nothing changed.
pub fn run_backup(root: &Path, base: &Path) -> Result<Option<SnapshotInfo>> {
    let mut found = Vec::new();
    walk(root, root, &mut found);
    let mut state: State =
        fs::read(state_path(base)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let changed: Vec<&(PathBuf, String, u64, u64)> =
        found.iter().filter(|(_, rel, m, s)| state.files.get(rel) != Some(&(*m, *s))).collect();
    if changed.is_empty() {
        return Ok(None);
    }
    let files: Vec<PathBuf> = changed.iter().map(|c| c.0.clone()).collect();
    let mut items: Vec<String> = changed
        .iter()
        .take(8)
        .map(|c| Path::new(&c.1).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
        .collect();
    if changed.len() > 8 {
        items.push(format!("+{}", changed.len() - 8));
    }
    let made = write_snapshot(root, base, &files, items, AUTO_REASON)?;
    state.files = found.iter().map(|(_, rel, m, s)| (rel.clone(), (*m, *s))).collect();
    fs::create_dir_all(base)?;
    fs::write(state_path(base), serde_json::to_vec(&state)?)?;
    prune_auto(base);
    Ok(made)
}

/// The slot an automatic backup of this age belongs to: only the newest backup of a slot stays.
fn slot(time: i64, now: i64) -> (u8, i64) {
    let age = now - time;
    if age < DAY {
        (0, time.div_euclid(HOUR))
    } else if age < 30 * DAY {
        (1, time.div_euclid(DAY))
    } else if age < YEAR {
        (2, time.div_euclid(WEEK))
    } else {
        (3, time.div_euclid(YEAR))
    }
}

/// Thin out automatic backups: hourly for a day, daily for a month, weekly for a year, yearly after.
/// Before a backup goes, the files that no newer snapshot holds are handed to the next one, so
/// pruning never makes a page disappear from the backups. Returns the number of backups removed.
pub fn prune_auto(base: &Path) -> usize {
    prune_auto_at(base, jiff::Timestamp::now().as_second())
}

fn prune_auto_at(base: &Path, now: i64) -> usize {
    let mut all: Vec<(i64, SnapshotInfo)> = list_snapshots_in(base)
        .into_iter()
        .filter_map(|i| Some((i.time.parse::<jiff::Timestamp>().ok()?.as_second(), i)))
        .collect();
    // oldest first: the plan is made newest first, the removal goes from the oldest up
    all.sort_by_key(|(t, i)| (*t, i.id.clone()));
    let mut seen: Vec<(u8, i64)> = Vec::new();
    let mut doomed: Vec<String> = Vec::new();
    for (t, info) in all.iter().rev() {
        if info.reason != AUTO_REASON {
            continue;
        }
        let key = slot(*t, now);
        if seen.contains(&key) {
            doomed.push(info.id.clone());
        } else {
            seen.push(key);
        }
    }
    let mut removed = 0;
    for id in doomed.iter().rev() {
        let Some(pos) = all.iter().position(|(_, i)| &i.id == id) else { continue };
        let (_, info) = all.remove(pos);
        // what only this snapshot holds goes to the next newer one (the newest is never removed)
        let orphans: Vec<&String> =
            info.files.iter().filter(|f| !all[pos..].iter().any(|(_, n)| n.files.contains(f))).collect();
        if !orphans.is_empty() {
            if let Some((_, next)) = all.get_mut(pos) {
                let from = base.join(&info.id).join("files");
                let to = base.join(&next.id).join("files");
                for rel in orphans {
                    let dest = to.join(rel);
                    if let Some(parent) = dest.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if fs::copy(from.join(rel), &dest).is_ok() {
                        next.files.push(rel.clone());
                    }
                }
                if let Ok(bytes) = serde_json::to_vec_pretty(&*next) {
                    let _ = fs::write(base.join(&next.id).join("manifest.json"), bytes);
                }
            }
        }
        if fs::remove_dir_all(base.join(&info.id)).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Erase every backed up plain copy of the page with this id (used when a page becomes
/// encrypted, so no readable copy stays behind). `base` is the snapshots folder.
pub fn purge_page(base: &Path, page_id: &str) {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let needle = format!("id: {page_id}");
    let Ok(dirs) = std::fs::read_dir(base) else { return };
    for d in dirs.flatten() {
        let dir = d.path();
        let files_dir = dir.join("files");
        let mut all = Vec::new();
        walk(&files_dir, &mut all);
        let mut gone = Vec::new();
        for f in all.into_iter().filter(|f| f.extension().is_some_and(|e| e == "md")) {
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            let head = crate::frontmatter::split(&text).0.unwrap_or("");
            if head.lines().any(|l| l.trim() == needle) {
                let _ = std::fs::remove_file(&f);
                gone.push(crate::fsutil::rel_string(&files_dir, &f));
            }
        }
        if gone.is_empty() {
            continue;
        }
        let manifest = dir.join("manifest.json");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            if let Ok(mut info) = serde_json::from_str::<crate::snapshot::SnapshotInfo>(&text) {
                info.files.retain(|f| !gone.contains(f));
                if let Ok(json) = serde_json::to_string_pretty(&info) {
                    let _ = std::fs::write(&manifest, json);
                }
            }
        }
    }
}

/// Snapshots that hold a version of the file at `rel`, newest first.
pub fn versions_of(base: &Path, rel: &str) -> Vec<SnapshotInfo> {
    list_snapshots_in(base).into_iter().filter(|i| i.files.iter().any(|f| f == rel)).collect()
}

/// The text of `rel` as saved in snapshot `id`.
pub fn version_text(base: &Path, id: &str, rel: &str) -> Result<String> {
    if id.contains('/') || id.contains("..") || rel.contains("..") {
        return Err(Error::Invalid("bad path".into()));
    }
    fs::read_to_string(base.join(id).join("files").join(rel)).map_err(|_| Error::NotFound(format!("{id}/{rel}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("notes");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join("a.md"), "one").unwrap();
        fs::write(root.join("sub/b.md"), "two").unwrap();
        fs::write(root.join(".hidden/c.md"), "no").unwrap();
        let base = dir.path().join("snaps");
        (dir, root, base)
    }

    #[test]
    fn first_backup_is_complete_then_only_changes_are_copied() {
        let (_d, root, base) = setup();
        let first = run_backup(&root, &base).unwrap().expect("first backup");
        assert_eq!(first.reason, AUTO_REASON);
        assert_eq!(first.files.len(), 2, "hidden folders are skipped: {:?}", first.files);
        // nothing changed: nothing is written
        assert!(run_backup(&root, &base).unwrap().is_none());
        // a changed file (different size, so the check does not depend on the clock) and a new one
        fs::write(root.join("a.md"), "one more line").unwrap();
        fs::write(root.join("d.md"), "new").unwrap();
        let second = run_backup(&root, &base).unwrap().expect("second backup");
        let mut files = second.files.clone();
        files.sort();
        assert_eq!(files, vec!["a.md".to_string(), "d.md".to_string()]);
        // both backups fall in the same hour, so only the newest stays; it also took over the
        // file that only the first one held, and a.md is read back in its newest version
        let versions = versions_of(&base, "a.md");
        assert_eq!(versions.len(), 1);
        assert_eq!(version_text(&base, &versions[0].id, "a.md").unwrap(), "one more line");
        assert_eq!(version_text(&base, &versions[0].id, "sub/b.md").unwrap(), "two");
        assert!(version_text(&base, "../x", "a.md").is_err());
    }

    fn mk(base: &Path, now: i64, id: &str, age_hours: i64, reason: &str, files: &[&str]) {
        let dir = base.join(id);
        for f in files {
            let p = dir.join("files").join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, format!("{id}:{f}")).unwrap();
        }
        let t = jiff::Timestamp::from_second(now - age_hours * 3600).unwrap().to_string();
        let info = SnapshotInfo {
            id: id.into(),
            time: t,
            reason: reason.into(),
            items: vec![],
            files: files.iter().map(|f| f.to_string()).collect(),
        };
        fs::write(dir.join("manifest.json"), serde_json::to_vec(&info).unwrap()).unwrap();
    }

    fn left(base: &Path) -> Vec<String> {
        list_snapshots_in(base).into_iter().map(|i| i.id).collect()
    }

    #[test]
    fn retention_is_hourly_then_daily_weekly_yearly_and_manual_ones_stay() {
        let (_d, _root, base) = setup();
        // a round hour, so that "same hour" below is the same slot whatever the clock says
        let now = jiff::Timestamp::now().as_second().div_euclid(3600) * 3600 + 1800;
        // last day: one per hour (two in the same hour -> the newer stays)
        mk(&base, now, "h1", 0, AUTO_REASON, &["a.md"]);
        mk(&base, now - 600, "h1b", 0, AUTO_REASON, &["a.md"]);
        mk(&base, now, "h2", 5, AUTO_REASON, &["a.md"]);
        // last month: one per day
        mk(&base, now, "d1", 24 * 6 + 1, AUTO_REASON, &["a.md"]);
        mk(&base, now, "d1b", 24 * 6 + 2, AUTO_REASON, &["a.md"]);
        // last year: one per week
        mk(&base, now, "w1", 24 * 100, AUTO_REASON, &["a.md"]);
        mk(&base, now, "w1b", 24 * 100 + 1, AUTO_REASON, &["a.md"]);
        // older: one per year
        mk(&base, now, "y1", 24 * 800, AUTO_REASON, &["a.md"]);
        mk(&base, now, "y1b", 24 * 800 + 1, AUTO_REASON, &["a.md"]);
        mk(&base, now, "m1", 24 * 800, "before deleting", &["a.md"]);
        prune_auto_at(&base, now);
        let l = left(&base);
        for keep in ["h1", "h2", "m1"] {
            assert!(l.contains(&keep.to_string()), "{keep} must stay: {l:?}");
        }
        assert!(!l.contains(&"h1b".to_string()), "same hour: {l:?}");
        assert!(l.contains(&"d1".to_string()) ^ l.contains(&"d1b".to_string()), "one per day: {l:?}");
        assert!(l.contains(&"w1".to_string()) ^ l.contains(&"w1b".to_string()), "one per week: {l:?}");
        assert!(l.contains(&"y1".to_string()) ^ l.contains(&"y1b".to_string()), "one per year: {l:?}");
    }

    #[test]
    fn pruning_hands_unique_files_to_the_next_snapshot() {
        let (_d, _root, base) = setup();
        let now = jiff::Timestamp::now().as_second().div_euclid(3600) * 3600 + 1800;
        // the old snapshot holds `only.md`, which no newer one has; its slot is shared with `new`
        mk(&base, now, "old", 24 * 6 + 2, AUTO_REASON, &["only.md", "a.md"]);
        mk(&base, now, "new", 24 * 6 + 1, AUTO_REASON, &["a.md"]);
        assert_eq!(prune_auto_at(&base, now), 1);
        assert_eq!(left(&base), vec!["new".to_string()]);
        let info = list_snapshots_in(&base).remove(0);
        assert!(info.files.contains(&"only.md".to_string()));
        assert_eq!(fs::read_to_string(base.join("new/files/only.md")).unwrap(), "old:only.md");
        // and the version of a file that exists in the newer snapshot is the newer one
        assert_eq!(fs::read_to_string(base.join("new/files/a.md")).unwrap(), "new:a.md");
    }
}
