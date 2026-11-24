//! Living in a synced folder (kDrive, Dropbox, iCloud Drive, OneDrive…) or a
//! plain local folder. NoteCraft never syncs by itself; this module only
//! helps it behave well there:
//!
//! - the SQLite index lives in the app's data folder, never in the notes folder;
//! - file stamps notice files changed by someone else (another Mac, the MCP
//!   server, git, a text editor);
//! - conflict copies made by sync tools ("Page 2.md", "(conflicted copy)",
//!   "(Kopie)", "(conflit)") are recognised;
//! - hints: is this folder synced, is there a `.git` in it, iCloud placeholders.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Size and modification time of a file, to notice changes made elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

impl FileStamp {
    pub fn of(path: &Path) -> Option<FileStamp> {
        let m = fs::metadata(path).ok()?;
        Some(FileStamp { modified: m.modified().ok(), len: m.len() })
    }
}

fn visible(p: &Path) -> bool {
    !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

fn push_md(dir: &Path, out: &mut HashMap<PathBuf, FileStamp>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if visible(&p) && p.extension().is_some_and(|x| x == "md") {
                if let Some(s) = FileStamp::of(&p) {
                    out.insert(p, s);
                }
            }
        }
    }
}

/// Stamps of every file the workspace is made of: pages, schemas (and their
/// conflict copies), rows, templates.
pub fn scan_stamps(root: &Path) -> HashMap<PathBuf, FileStamp> {
    let mut out = HashMap::new();
    push_md(&root.join(crate::workspace::PAGES_DIR), &mut out);
    if let Ok(rd) = fs::read_dir(root.join(crate::workspace::DBS_DIR)) {
        for e in rd.filter_map(|e| e.ok()) {
            let d = e.path();
            if !d.is_dir() || !visible(&d) {
                continue;
            }
            for f in schema_files(&d) {
                if let Some(s) = FileStamp::of(&f) {
                    out.insert(f, s);
                }
            }
            push_md(&d.join("rows"), &mut out);
            push_md(&d.join("templates"), &mut out);
        }
    }
    out
}

/// `schema.json` and any sync conflict copies of it (`schema 2.json`…).
pub fn schema_files(db_dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(db_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    n.starts_with("schema") && n.ends_with(".json") && visible(p)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|p| (p.file_name().is_some_and(|n| n != "schema.json"), p.clone()));
    v
}

/// Paths added, changed or removed between two scans.
pub fn diff_stamps(old: &HashMap<PathBuf, FileStamp>, new: &HashMap<PathBuf, FileStamp>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = new
        .iter()
        .filter(|(p, s)| old.get(*p) != Some(*s))
        .map(|(p, _)| p.clone())
        .chain(old.keys().filter(|p| !new.contains_key(*p)).cloned())
        .collect();
    v.sort();
    v
}

// ------------------------------------------------------------------ the index

/// Folder that holds the indexes of all workspaces:
/// - `NOTECRAFT_INDEX_DIR` when set (tests, portable setups);
/// - macOS: `~/Library/Application Support/NoteCraft/indexes`;
/// - Windows: `%APPDATA%\NoteCraft\indexes`;
/// - elsewhere: `$XDG_DATA_HOME/notecraft/indexes` or `~/.local/share/notecraft/indexes`.
pub fn index_base_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("NOTECRAFT_INDEX_DIR") {
        return PathBuf::from(d);
    }
    if cfg!(test) {
        return std::env::temp_dir().join("notecraft-test-indexes");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support").join("NoteCraft").join("indexes")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("NoteCraft").join("indexes")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("notecraft")
            .join("indexes")
    }
}

/// FNV-1a 64-bit hash, written as 16 hex digits (stable across runs and builds).
pub fn path_hash(path: &Path) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `<index base>/<hash of the workspace path>/index.db`.
pub fn index_file_for(root: &Path) -> PathBuf {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    index_base_dir().join(path_hash(&canonical)).join("index.db")
}

// --------------------------------------------------------------- conflicts

/// Words sync tools put in the names of conflict copies.
const CONFLICT_WORDS: &[&str] =
    &["conflicted copy", "conflict", "conflit", "kopie", "copy", "copie", "konflikt", "conflicto", "conflitto"];

/// True when a file name looks like a copy made by a sync tool:
/// `Page 2`, `Page (conflicted copy 2026-11-22)`, `Page (Kopie)`, `Page (conflit)`…
pub fn looks_like_conflict_copy(stem: &str) -> bool {
    let lower = stem.to_lowercase();
    if let Some(open) = lower.rfind('(') {
        let inside = &lower[open + 1..];
        if CONFLICT_WORDS.iter().any(|w| inside.contains(w)) {
            return true;
        }
    }
    // "Name 2", "Name 3" (iCloud, kDrive)
    if let Some((_, tail)) = lower.rsplit_once(' ') {
        if !tail.is_empty() && tail.len() <= 2 && tail.chars().all(|c| c.is_ascii_digit()) && tail != "0" && tail != "1"
        {
            return true;
        }
    }
    false
}

/// Order files that claim the same id: lowest first is the original.
/// The name NoteCraft gave it (`<slug>-<id>`) wins, then names that don't
/// look like conflict copies, then the oldest.
pub fn original_rank(path: &Path, id: &str) -> (u8, u8, Option<SystemTime>, PathBuf) {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let canonical = stem.ends_with(&format!("-{id}")) || stem == id || stem == "schema";
    let copyish = looks_like_conflict_copy(&stem);
    let modified = fs::metadata(path).and_then(|m| m.modified()).ok();
    (u8::from(!canonical), u8::from(copyish), modified, path.to_path_buf())
}

/// Where to put "my" version when a file changed underneath us:
/// `<stem> (conflicted copy <date time>).<ext>` in the same folder.
pub fn conflict_copy_path(original: &Path) -> PathBuf {
    let stem = original.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = original.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let when = jiff::Zoned::now().strftime("%Y-%m-%d %H%M%S");
    let dir = original.parent().unwrap_or(Path::new("."));
    crate::fsutil::unique_path(dir, &format!("{stem} (conflicted copy {when})"), &ext)
}

// ------------------------------------------------------------------- hints

/// Name of the sync service a folder seems to be in (a hint for the UI):
/// iCloud Drive, kDrive, Dropbox, OneDrive, Google Drive, pCloud, Nextcloud,
/// Syncthing, or another File Provider folder in `~/Library/CloudStorage`.
pub fn synced_folder_kind(path: &Path) -> Option<&'static str> {
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for c in p.components() {
        let n = c.as_os_str().to_string_lossy().to_lowercase();
        let kind = if n == "mobile documents" {
            "iCloud Drive"
        } else if n.starts_with("kdrive") {
            "kDrive"
        } else if n.starts_with("dropbox") {
            "Dropbox"
        } else if n.starts_with("onedrive") {
            "OneDrive"
        } else if n.starts_with("googledrive") || n.starts_with("google drive") {
            "Google Drive"
        } else if n.starts_with("pcloud") {
            "pCloud"
        } else if n.starts_with("nextcloud") {
            "Nextcloud"
        } else {
            continue;
        };
        return Some(kind);
    }
    if p.components().any(|c| c.as_os_str() == "CloudStorage") {
        return Some("a cloud folder");
    }
    let mut cur = Some(p.as_path());
    while let Some(d) = cur {
        if d.join(".stfolder").exists() {
            return Some("Syncthing");
        }
        cur = d.parent();
    }
    None
}

/// True when the workspace folder itself is a git repository.
pub fn has_git(root: &Path) -> bool {
    root.join(".git").exists()
}

/// Copy a whole folder tree (files and folders; symbolic links are skipped).
pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Optional iCloud hint: the folder is inside iCloud Drive.
pub fn is_in_icloud(path: &Path) -> bool {
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    p.components().any(|c| c.as_os_str() == "Mobile Documents")
}

/// Optional iCloud hint: files not downloaded yet (placeholders named `.Name.md.icloud`).
pub fn pending_downloads(root: &Path) -> usize {
    fn walk(dir: &Path, depth: usize, n: &mut usize) {
        if depth > 4 {
            return;
        }
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') && name.ends_with(".icloud") {
                    *n += 1;
                } else if p.is_dir() && !name.starts_with('.') {
                    walk(&p, depth + 1, n);
                }
            }
        }
    }
    let mut n = 0;
    walk(root, 0, &mut n);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_tree_copies_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("pages/sub")).unwrap();
        fs::write(src.join("pages/sub/a.md"), "a").unwrap();
        fs::write(src.join("b.txt"), "b").unwrap();
        let dst = dir.path().join("dst");
        copy_tree(&src, &dst).unwrap();
        assert_eq!(fs::read_to_string(dst.join("pages/sub/a.md")).unwrap(), "a");
        assert_eq!(fs::read_to_string(dst.join("b.txt")).unwrap(), "b");
        assert!(src.join("b.txt").exists());
    }

    #[test]
    fn stamps_notice_changes_additions_and_removals() {
        let dir = tempfile::tempdir().unwrap();
        let pages = dir.path().join("pages");
        fs::create_dir_all(&pages).unwrap();
        fs::write(pages.join("a.md"), "a").unwrap();
        fs::write(pages.join(".hidden.md"), "x").unwrap();
        let before = scan_stamps(dir.path());
        assert_eq!(before.len(), 1);
        fs::write(pages.join("a.md"), "aa").unwrap();
        fs::write(pages.join("b.md"), "b").unwrap();
        let after = scan_stamps(dir.path());
        assert_eq!(diff_stamps(&before, &after).len(), 2);
        fs::remove_file(pages.join("b.md")).unwrap();
        assert_eq!(diff_stamps(&after, &scan_stamps(dir.path())), vec![pages.join("b.md")]);
    }

    #[test]
    fn index_lives_outside_the_notes_folder() {
        let dir = tempfile::tempdir().unwrap();
        let f = index_file_for(dir.path());
        assert!(!f.starts_with(dir.path()));
        assert!(f.ends_with("index.db"));
        // stable for the same folder, different for another
        assert_eq!(f, index_file_for(dir.path()));
        let other = tempfile::tempdir().unwrap();
        assert_ne!(f, index_file_for(other.path()));
        assert_eq!(path_hash(Path::new("/a")).len(), 16);
    }

    #[test]
    fn conflict_copy_names_are_recognised() {
        for n in [
            "notes 2",
            "notes (conflicted copy 2026-11-22)",
            "Notes (Alex's conflicted copy)",
            "notes (Kopie)",
            "notes (conflit)",
            "notes (Copie 1)",
            "notes-abc123 (conflicted copy 2026-11-22 101010)",
        ] {
            assert!(looks_like_conflict_copy(n), "{n}");
        }
        for n in ["notes", "chapter 1", "2026 plans", "notes-abc123", "Project (draft)"] {
            assert!(!looks_like_conflict_copy(n), "{n}");
        }
    }

    #[test]
    fn the_canonical_file_ranks_first() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("notes-abc.md");
        let b = dir.path().join("notes 2.md");
        let c = dir.path().join("other name.md");
        for p in [&a, &b, &c] {
            fs::write(p, "x").unwrap();
        }
        let mut v = [b.clone(), c.clone(), a.clone()];
        v.sort_by_key(|p| original_rank(p, "abc"));
        assert_eq!(v[0], a);
        assert_eq!(v[2], b);
        let cp = conflict_copy_path(&a);
        assert!(cp.file_name().unwrap().to_string_lossy().starts_with("notes-abc (conflicted copy "));
        assert!(looks_like_conflict_copy(&cp.file_stem().unwrap().to_string_lossy()));
    }

    #[test]
    fn synced_folder_hints() {
        assert_eq!(synced_folder_kind(Path::new("/Users/x/kDrive/Notes")), Some("kDrive"));
        assert_eq!(synced_folder_kind(Path::new("/Users/x/Dropbox (Personal)/Notes")), Some("Dropbox"));
        assert_eq!(
            synced_folder_kind(Path::new("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/N")),
            Some("iCloud Drive")
        );
        assert_eq!(synced_folder_kind(Path::new("/Users/x/Library/CloudStorage/Box-Box/N")), Some("a cloud folder"));
        assert!(is_in_icloud(Path::new("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/N")));
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_git(dir.path()));
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        assert!(has_git(dir.path()));
    }

    #[test]
    fn placeholders_are_counted() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pages")).unwrap();
        fs::write(dir.path().join("pages/.Note-abc.md.icloud"), "").unwrap();
        assert_eq!(pending_downloads(dir.path()), 1);
    }
}
