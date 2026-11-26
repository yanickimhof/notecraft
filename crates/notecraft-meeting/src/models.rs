//! The speech models a person can download, and where they are kept.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// One downloadable Whisper model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhisperModel {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    /// Approximate download size in megabytes.
    pub size_mb: u32,
    /// One short line on what it is good for (English; translated in the app).
    pub note: &'static str,
}

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub const MODELS: &[WhisperModel] = &[
    WhisperModel { id: "base", label: "Base", file: "ggml-base-q5_1.bin", size_mb: 60, note: "fast_light" },
    WhisperModel { id: "small", label: "Small", file: "ggml-small-q5_1.bin", size_mb: 190, note: "balanced" },
    WhisperModel {
        id: "turbo",
        label: "Large v3 Turbo",
        file: "ggml-large-v3-turbo-q5_0.bin",
        size_mb: 574,
        note: "best",
    },
];

impl WhisperModel {
    pub fn by_id(id: &str) -> Option<&'static WhisperModel> {
        MODELS.iter().find(|m| m.id == id)
    }

    pub fn url(&self) -> String {
        format!("{BASE_URL}/{}", self.file)
    }

    pub fn path(&self) -> PathBuf {
        models_dir().join(self.file)
    }

    pub fn installed(&self) -> bool {
        self.path().is_file()
    }

    pub fn delete(&self) -> std::io::Result<()> {
        match fs::remove_file(self.path()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Models live next to the other app data, outside the notes folder.
pub fn models_dir() -> PathBuf {
    app_data().join("whisper")
}

fn app_data() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Library/Application Support/NoteCraft")
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        // This limit covers the whole body, not each read: it must outlast the largest model on a slow line.
        .timeout_recv_body(Some(std::time::Duration::from_secs(45 * 60)))
        // The macOS certificate store, so a company proxy or antivirus that re-signs HTTPS is trusted.
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
        .build()
        .into()
}

/// A network error in words a person can act on, with the technical detail kept at the end.
fn explain(url: &str, err: &str) -> String {
    let host = url.split('/').nth(2).unwrap_or(url);
    let low = err.to_lowercase();
    let why = if low.contains("status code 401") || low.contains("status code 403") {
        format!("{host} refused the request (access denied: a proxy or a filter may be blocking it)")
    } else if low.contains("status code 404") {
        format!("the file was not found on {host}")
    } else if low.contains("status code 429") || low.contains("status code 5") {
        format!("{host} is busy, try again in a few minutes")
    } else if low.contains("certificate") || low.contains("tls") || low.contains("handshake") {
        "the secure connection was refused: a company proxy or an antivirus may be inspecting HTTPS".to_string()
    } else if low.contains("dns") || low.contains("host") || low.contains("resolve") || low.contains("lookup") {
        format!("{host} cannot be reached: check the internet connection, VPN and DNS")
    } else if low.contains("timeout") || low.contains("timed out") {
        format!("{host} does not answer: check the internet connection, VPN or firewall")
    } else if low.contains("connection") {
        format!("the connection to {host} failed: check the internet connection, VPN or firewall")
    } else {
        format!("download from {host} failed")
    };
    format!("{why} ({err})")
}

/// Downloads `url` into `dest`, through a `.part` file: `dest` exists only when the file is complete.
/// `progress(done, total)` is about this file alone and returns false to cancel.
fn fetch(
    url: &str,
    dest: &Path,
    fallback_total: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), String> {
    notecraft_net::check(notecraft_net::Purpose::ModelDownload, url).map_err(|b| b.to_string())?;
    let resp = agent().get(url).call().map_err(|e| explain(url, &e.to_string()))?;
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(fallback_total);
    let part = dest.with_extension("part");
    let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut body = resp.into_body();
    let mut reader = body.as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| {
            let _ = fs::remove_file(&part);
            e.to_string()
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if !progress(done, total) {
            drop(file);
            let _ = fs::remove_file(&part);
            return Err("cancelled".into());
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    // A truncated download must never look like an installed model.
    if done < total.saturating_sub(total / 50) {
        let _ = fs::remove_file(&part);
        return Err("incomplete download".into());
    }
    fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Downloads `model` into the models folder. `progress(done, total)` returns false to cancel.
/// The file appears under its final name only when complete.
pub fn download(model: &WhisperModel, mut progress: impl FnMut(u64, u64) -> bool) -> Result<(), String> {
    fs::create_dir_all(models_dir()).map_err(|e| e.to_string())?;
    fetch(&model.url(), &model.path(), u64::from(model.size_mb) * 1_000_000, &mut progress)
}

// ---- Parakeet (NVIDIA), the second speech engine: fast, English and 24 other languages, no custom vocabulary.

const PARAKEET_URL: &str = "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main";

/// The files of the quantized Parakeet TDT 0.6B v3 and their exact sizes in bytes. They live together in one folder.
pub const PARAKEET_FILES: &[(&str, u64)] =
    &[("encoder-model.int8.onnx", 652_183_999), ("decoder_joint-model.int8.onnx", 18_202_004), ("vocab.txt", 93_939)];

/// Approximate download size in megabytes.
pub const PARAKEET_MB: u32 = 670;

/// Where the Parakeet files are kept (next to the Whisper ones, outside the notes folder).
pub fn parakeet_dir() -> PathBuf {
    app_data().join("parakeet")
}

/// True when every file is there: a file takes its final name only once complete.
pub fn parakeet_installed() -> bool {
    let dir = parakeet_dir();
    PARAKEET_FILES.iter().all(|(name, _)| dir.join(name).is_file())
}

/// Removes the Parakeet files; nothing to do when they are not there.
pub fn parakeet_delete() -> std::io::Result<()> {
    match fs::remove_dir_all(parakeet_dir()) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Downloads the Parakeet files one after the other. `progress(done, total)` covers all of them and returns
/// false to cancel. Files already complete are not downloaded again.
pub fn parakeet_download(mut progress: impl FnMut(u64, u64) -> bool) -> Result<(), String> {
    let dir = parakeet_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let total: u64 = PARAKEET_FILES.iter().map(|(_, n)| n).sum();
    let mut before = 0u64;
    for (name, size) in PARAKEET_FILES {
        let dest = dir.join(name);
        if !dest.is_file() {
            let base = before;
            fetch(&format!("{PARAKEET_URL}/{name}"), &dest, *size, &mut |d, _| progress(base + d, total))?;
        }
        before += size;
        if !progress(before, total) {
            return Err("cancelled".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_consistent() {
        assert!(WhisperModel::by_id("small").is_some());
        assert!(WhisperModel::by_id("nope").is_none());
        for m in MODELS {
            assert!(m.url().starts_with("https://huggingface.co/"));
            assert!(m.file.ends_with(".bin"));
        }
    }

    #[test]
    fn parakeet_files_are_listed_with_their_sizes() {
        assert!(PARAKEET_URL.starts_with("https://huggingface.co/"));
        let mb = PARAKEET_FILES.iter().map(|(_, n)| n).sum::<u64>() / 1_000_000;
        assert_eq!(mb, u64::from(PARAKEET_MB), "the size shown to the person must match the files");
        assert!(PARAKEET_FILES.iter().any(|(n, _)| n.starts_with("encoder")));
        assert!(PARAKEET_FILES.iter().any(|(n, _)| *n == "vocab.txt"));
        assert!(parakeet_dir().ends_with("NoteCraft/parakeet"));
    }
}
