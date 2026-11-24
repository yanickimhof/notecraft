//! Safe unpacking of a `.zip` export (Evernote, Bear, Joplin, Word files are zips too).
//!
//! A zip from a stranger is untrusted input: names are confined to the target folder, links are
//! never created, and the number of files, their sizes and nesting are capped.

use crate::{Error, Result};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// More entries than any real export holds.
pub const MAX_ENTRIES: usize = 300_000;
/// Total unpacked size we accept (16 GiB).
pub const MAX_TOTAL: u64 = 16 * 1024 * 1024 * 1024;
/// Largest single file we accept (4 GiB).
pub const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
/// Big exports may be split into zips inside the zip.
const MAX_NESTING: usize = 3;

/// True when `path` starts like a zip file.
pub fn looks_like_zip(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && &magic == b"PK\x03\x04"
}

fn skip_entry(name: &Path) -> bool {
    name.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "__MACOSX" || s == ".DS_Store" || s.starts_with("._")
    })
}

/// Unpack `zip_path` into the existing folder `dest`. Zips found inside are unpacked next to themselves.
/// Returns the number of files written.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> Result<usize> {
    let mut count = extract_one(zip_path, dest)?;
    for _ in 0..MAX_NESTING {
        let inner = find_zips(dest);
        if inner.is_empty() {
            break;
        }
        for z in inner {
            let target = z.with_extension("");
            let target = if target.exists() { z.with_extension("unzipped") } else { target };
            fs::create_dir_all(&target)?;
            match extract_one(&z, &target) {
                Ok(n) => count += n,
                Err(_) => {
                    // not a zip after all (an attachment that happens to end in .zip): keep it as is
                    let _ = fs::remove_dir_all(&target);
                    continue;
                }
            }
            let _ = fs::remove_file(&z);
        }
    }
    Ok(count)
}

/// Zip files at the top levels of an export (attachments deeper down are left alone).
fn find_zips(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 2 {
                stack.push((p, depth + 1));
            } else if p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")) && looks_like_zip(&p)
            {
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                // parts are called `...-Part-1.zip`; any other zip near the top of an export is a part too
                if depth == 0 || name.contains("part") {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

fn extract_one(zip_path: &Path, dest: &Path) -> Result<usize> {
    let file = fs::File::open(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| Error::Invalid(format!("not a readable zip file: {e}")))?;
    if archive.len() > MAX_ENTRIES {
        return Err(Error::Invalid(format!(
            "the zip holds {} entries, more than the {MAX_ENTRIES} accepted",
            archive.len()
        )));
    }
    let dest_canon = dest.canonicalize()?;
    let mut total: u64 = 0;
    let mut written = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| Error::Invalid(format!("damaged zip entry: {e}")))?;
        let Some(name) = entry.enclosed_name() else { continue };
        if skip_entry(&name) {
            continue;
        }
        // never create links, devices or anything but plain files and folders
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            continue;
        }
        let out = dest_canon.join(&name);
        if !out.starts_with(&dest_canon) {
            continue;
        }
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if entry.size() > MAX_FILE {
            return Err(Error::Invalid(format!("{} is larger than {} GiB", name.display(), MAX_FILE >> 30)));
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::File::create(&out)?;
        // trust what we actually read, not the size the zip announces
        let n = io::copy(&mut (&mut entry).take(MAX_FILE + 1), &mut f)?;
        if n > MAX_FILE {
            return Err(Error::Invalid("a file in the zip is too large".into()));
        }
        total += n;
        if total > MAX_TOTAL {
            return Err(Error::Invalid("the zip unpacks to more than 16 GiB".into()));
        }
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// Build a zip in memory-free fashion: write `files` (name, bytes) to `path`.
    pub fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let f = fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in files {
            z.start_file(*name, opts).unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn unpacks_files_and_skips_macos_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("a.zip");
        make_zip(
            &zp,
            &[("Root/Page abc.md", b"# Page"), ("__MACOSX/Root/._Page abc.md", b"x"), ("Root/.DS_Store", b"x")],
        );
        let out = tmp.path().join("out");
        fs::create_dir(&out).unwrap();
        assert_eq!(extract_zip(&zp, &out).unwrap(), 1);
        assert!(out.join("Root/Page abc.md").is_file());
        assert!(!out.join("__MACOSX").exists());
    }

    #[test]
    fn names_that_climb_out_of_the_folder_are_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("evil.zip");
        make_zip(&zp, &[("../outside.txt", b"x"), ("/abs.txt", b"x"), ("ok.txt", b"y")]);
        let out = tmp.path().join("out");
        fs::create_dir(&out).unwrap();
        extract_zip(&zp, &out).unwrap();
        assert!(!tmp.path().join("outside.txt").exists());
        assert!(out.join("ok.txt").is_file());
    }

    #[test]
    fn a_zip_inside_a_zip_is_opened_too() {
        let tmp = tempfile::tempdir().unwrap();
        let inner = tmp.path().join("Export-Part-1.zip");
        make_zip(&inner, &[("Page 0123456789abcdef0123456789abcdef.md", b"# Hi")]);
        let outer = tmp.path().join("outer.zip");
        make_zip(&outer, &[("Export-Part-1.zip", &fs::read(&inner).unwrap())]);
        let out = tmp.path().join("out");
        fs::create_dir(&out).unwrap();
        extract_zip(&outer, &out).unwrap();
        assert!(out.join("Export-Part-1/Page 0123456789abcdef0123456789abcdef.md").is_file());
    }

    #[test]
    fn garbage_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("bad.zip");
        fs::write(&zp, b"not a zip at all").unwrap();
        let out = tmp.path().join("out");
        fs::create_dir(&out).unwrap();
        assert!(extract_zip(&zp, &out).is_err());
        assert!(!looks_like_zip(&zp));
    }
}
