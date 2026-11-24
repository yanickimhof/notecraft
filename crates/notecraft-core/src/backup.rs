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

/// Every automatic backup of the last two days is kept, then one per day for a month.
const KEEP_ALL_HOURS: i64 = 48;
const KEEP_DAILY_DAYS: i64 = 30;

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

/// Thin out old automatic backups: all of the last 48 hours, then the newest of each day.
pub fn prune_auto(base: &Path) {
    let now = jiff::Timestamp::now().as_second();
    let mut seen_days: Vec<i64> = Vec::new();
    let mut autos: Vec<(jiff::Timestamp, SnapshotInfo)> = list_snapshots_in(base)
        .into_iter()
        .filter(|i| i.reason == AUTO_REASON)
        .filter_map(|i| Some((i.time.parse::<jiff::Timestamp>().ok()?, i)))
        .collect();
    // newest first, so the first one met on a day is the one kept
    autos.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    for (t, info) in autos {
        let age_hours = (now - t.as_second()) / 3600;
        if age_hours < KEEP_ALL_HOURS {
            continue;
        }
        let day = t.as_second().div_euclid(86_400);
        let too_old = age_hours / 24 > KEEP_DAILY_DAYS;
        if too_old || seen_days.contains(&day) {
            let _ = fs::remove_dir_all(base.join(&info.id));
        } else {
            seen_days.push(day);
        }
    }
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
        // both versions of a.md can be read back
        let versions = versions_of(&base, "a.md");
        assert_eq!(versions.len(), 2);
        assert_eq!(version_text(&base, &versions[0].id, "a.md").unwrap(), "one more line");
        assert_eq!(version_text(&base, &versions[1].id, "a.md").unwrap(), "one");
        assert!(version_text(&base, "../x", "a.md").is_err());
    }

    #[test]
    fn old_automatic_backups_are_thinned_but_manual_ones_stay() {
        let (_d, _root, base) = setup();
        let now = jiff::Timestamp::now().as_second();
        let mk = |id: &str, age_hours: i64, reason: &str| {
            let dir = base.join(id);
            fs::create_dir_all(&dir).unwrap();
            let t = jiff::Timestamp::from_second(now - age_hours * 3600).unwrap().to_string();
            let info = SnapshotInfo { id: id.into(), time: t, reason: reason.into(), items: vec![], files: vec![] };
            fs::write(dir.join("manifest.json"), serde_json::to_vec(&info).unwrap()).unwrap();
        };
        mk("s1", 1, AUTO_REASON); // recent: kept
        mk("s2", 30, AUTO_REASON); // within 48 h: kept
        mk("s3", 100, AUTO_REASON); // a day with two backups: the newest is kept
        mk("s4", 101, AUTO_REASON); // ... and the older one goes (same UTC day as s3 unless the day rolls over)
        mk("s5", 24 * 40, AUTO_REASON); // older than a month: removed
        mk("m1", 24 * 40, "before deleting"); // manual: untouched
        prune_auto(&base);
        let left: Vec<String> = list_snapshots_in(&base).into_iter().map(|i| i.id).collect();
        assert!(left.contains(&"s1".to_string()) && left.contains(&"s2".to_string()));
        assert!(left.contains(&"s3".to_string()));
        assert!(!left.contains(&"s5".to_string()));
        assert!(left.contains(&"m1".to_string()));
    }
}
