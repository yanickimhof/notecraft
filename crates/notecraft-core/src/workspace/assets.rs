//! Attachments stored in the workspace (split out of mod.rs, unchanged).

use super::*;

impl Workspace {
    // ----------------------------------------------------------------- assets

    /// Copy a file into `assets/` and return its workspace-relative path.
    pub fn add_asset(&mut self, src: &Path) -> Result<String> {
        let dir = self.root.join(ASSETS_DIR);
        fs::create_dir_all(&dir)?;
        if src.starts_with(&dir) {
            return Ok(rel_string(&self.root, src));
        }
        // the same file imported twice is kept once
        if let Some(existing) = self.identical_asset(&dir, src) {
            return Ok(rel_string(&self.root, &existing));
        }
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
            _ => (name.clone(), String::new()),
        };
        let target = unique_path(&dir, &safe_file_name(&stem), &ext.to_lowercase());
        crate::fsutil::copy_file_checked(src, &target)?;
        Ok(rel_string(&self.root, &target))
    }

    /// A file already in `assets/` with the same name stem and the same content as `src`.
    pub(super) fn identical_asset(&self, dir: &Path, src: &Path) -> Option<PathBuf> {
        let size = fs::metadata(src).ok()?.len();
        let stem = src.file_stem()?.to_string_lossy().to_string();
        let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase());
        let wanted = safe_file_name(&stem);
        let mut mine: Option<Vec<u8>> = None;
        for e in fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            let name = p.file_stem()?.to_string_lossy().to_string();
            // `name`, `name 2`, `name 3`… as `unique_path` writes them
            let base = name.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end();
            if base != wanted && name != wanted {
                continue;
            }
            if p.extension().map(|x| x.to_string_lossy().to_lowercase()) != ext {
                continue;
            }
            if e.metadata().ok()?.len() != size {
                continue;
            }
            let mine = mine.get_or_insert_with(|| fs::read(src).unwrap_or_default());
            if fs::read(&p).ok().as_ref() == Some(mine) {
                return Some(p);
            }
        }
        None
    }

    /// Store `bytes` as an attachment named like `name`; returns its workspace-relative path.
    pub fn add_asset_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<String> {
        let dir = self.root.join(ASSETS_DIR);
        fs::create_dir_all(&dir)?;
        let name = Path::new(name)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "file".into());
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
            _ => (name.clone(), String::new()),
        };
        let target = unique_path(&dir, &safe_file_name(&stem), &ext.to_lowercase());
        atomic_write(&target, bytes)?;
        Ok(rel_string(&self.root, &target))
    }

    /// Absolute path of a workspace-relative asset path.
    pub fn asset_path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}
