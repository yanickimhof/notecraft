//! File helpers: atomic writes, slugs, safe file names.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Write `bytes` to `path` atomically: write a temp file in the same folder,
/// flush it to disk, then rename it over the target. A crash never leaves a
/// half-written file behind.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", crate::ids::new_id()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Chunk size for copying attachments: 1 MiB at a time.
pub const COPY_CHUNK: usize = 1_048_576;

/// Copy an attachment the careful way: refuse symbolic
/// links (it opens with O_NOFOLLOW) and anything that is not a regular file,
/// copy in 1 MiB chunks, and refuse the result if the file changed while it
/// was being read ("file changed after inspection"). Atomic: temp file, then rename.
pub fn copy_file_checked(src: &Path, dest: &Path) -> std::io::Result<u64> {
    use std::io::{Error, ErrorKind, Read};
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(Error::new(ErrorKind::InvalidInput, "path is not a regular file"));
    }
    let size = meta.len();
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let name = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", crate::ids::new_id()));
    let result = (|| -> std::io::Result<u64> {
        let mut input = fs::File::open(src)?;
        let mut out = fs::File::create(&tmp)?;
        let mut buf = vec![0u8; COPY_CHUNK];
        let mut total = 0u64;
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            total += n as u64;
        }
        out.sync_all()?;
        let after = fs::metadata(src)?.len();
        if total != size || after != size {
            return Err(Error::other("file changed after inspection"));
        }
        Ok(total)
    })();
    match result {
        Ok(n) => {
            fs::rename(&tmp, dest)?;
            Ok(n)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// MIME type by extension, a short list; anything else is binary.
pub fn mime_type(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("csv") => "text/csv",
        Some("html") => "text/html",
        Some("json") => "application/json",
        Some("md") => "text/markdown",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Lowercase ASCII slug for file names: `Ma Page à moi!` → `ma-page-a-moi`.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars() {
        let c = fold_accent(c);
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "untitled".into()
    } else {
        out
    }
}

fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ÿ' | 'ý' | 'Ý' => 'y',
        other => other,
    }
}

/// A file name that is safe on macOS, Windows and Linux, keeping readable
/// characters (used for exports, where titles become file names).
pub fn safe_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let t = cleaned.trim().trim_matches('.').trim();
    if t.is_empty() {
        "Untitled".into()
    } else {
        t.chars().take(100).collect()
    }
}

/// Pick a path in `dir` named `stem.ext`, adding ` 2`, ` 3`… if it exists.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let make = |n: usize| {
        let name = if n == 1 { format!("{stem}{ext}") } else { format!("{stem} {n}{ext}") };
        dir.join(name)
    };
    let mut n = 1;
    loop {
        let p = make(n);
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

/// Relative path from the workspace root using forward slashes (git-friendly).
pub fn rel_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_folds_accents_and_symbols() {
        assert_eq!(slugify("Ma Page à moi!"), "ma-page-a-moi");
        assert_eq!(slugify("   "), "untitled");
        assert_eq!(slugify("Été 2026 — notes"), "ete-2026-notes");
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a/b.txt");
        atomic_write(&p, b"one").unwrap();
        atomic_write(&p, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        // no temp files left behind
        let n = std::fs::read_dir(p.parent().unwrap()).unwrap().count();
        assert_eq!(n, 1);
    }

    #[test]
    fn checked_copy_refuses_links_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.bin");
        std::fs::write(&src, vec![7u8; COPY_CHUNK + 10]).unwrap();
        let n = copy_file_checked(&src, &dir.path().join("out/a.bin")).unwrap();
        assert_eq!(n, (COPY_CHUNK + 10) as u64);
        assert!(copy_file_checked(dir.path(), &dir.path().join("x")).is_err(), "a folder is not a regular file");
        #[cfg(unix)]
        {
            let link = dir.path().join("link.bin");
            std::os::unix::fs::symlink(&src, &link).unwrap();
            let e = copy_file_checked(&link, &dir.path().join("y")).unwrap_err();
            assert!(e.to_string().contains("not a regular file"));
        }
        assert_eq!(mime_type(Path::new("x/notes.MD")), "text/markdown");
        assert_eq!(mime_type(Path::new("photo.jpg")), "application/octet-stream");
    }

    #[test]
    fn safe_names() {
        assert_eq!(safe_file_name("a/b:c"), "a-b-c");
        assert_eq!(safe_file_name(""), "Untitled");
    }

    #[test]
    fn unique_paths_count_up() {
        let dir = tempfile::tempdir().unwrap();
        let a = unique_path(dir.path(), "x", ".md");
        std::fs::write(&a, "").unwrap();
        let b = unique_path(dir.path(), "x", ".md");
        assert!(b.ends_with("x 2.md"));
    }
}
