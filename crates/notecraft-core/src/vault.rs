//! Encrypted pages: a passphrase protects the body of a page file.
//!
//! The key comes from the passphrase with Argon2id and a random salt kept in the
//! file; the body is sealed with ChaCha20-Poly1305 (authenticated: a wrong
//! passphrase or a damaged file is detected, never silently misread). The
//! result is stored as text between two marker lines, so the file stays a
//! normal Markdown file whose front matter (title, icon, order…) is readable.
//! There is no recovery: without the passphrase the body cannot be read.

use crate::{Error, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use zeroize::Zeroize;

pub const BEGIN: &str = "-----BEGIN NOTECRAFT ENCRYPTED PAGE-----";
pub const END: &str = "-----END NOTECRAFT ENCRYPTED PAGE-----";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[cfg(not(test))]
const COST: (u32, u32, u32) = (65_536, 3, 1);
#[cfg(test)]
const COST: (u32, u32, u32) = (1_024, 1, 1);

fn bad() -> Error {
    Error::Invalid("wrong passphrase or damaged encrypted page".into())
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| Error::Invalid(format!("no secure random source: {e}")))?;
    Ok(b)
}

/// A key derived for one page (salt included). Never printed, wiped when dropped.
#[derive(Clone, PartialEq)]
pub struct VaultKey {
    salt: [u8; SALT_LEN],
    cost: (u32, u32, u32),
    key: [u8; 32],
}

impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultKey(<hidden>)")
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

impl VaultKey {
    fn derive(pass: &str, salt: [u8; SALT_LEN], cost: (u32, u32, u32)) -> Result<Self> {
        let params = Params::new(cost.0, cost.1, cost.2, Some(32)).map_err(|_| bad())?;
        let mut key = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(pass.as_bytes(), &salt, &mut key)
            .map_err(|_| bad())?;
        Ok(VaultKey { salt, cost, key })
    }

    /// A new key for `pass` with a fresh random salt.
    pub fn new(pass: &str) -> Result<Self> {
        Self::derive(pass, random::<SALT_LEN>()?, COST)
    }

    /// Encrypt `plain` into the armored text stored in the page file.
    pub fn seal(&self, plain: &str) -> Result<String> {
        let nonce = random::<NONCE_LEN>()?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key));
        let ct = cipher.encrypt(Nonce::from_slice(&nonce), plain.as_bytes()).map_err(|_| bad())?;
        let mut raw = Vec::with_capacity(SALT_LEN + NONCE_LEN + ct.len());
        raw.extend_from_slice(&self.salt);
        raw.extend_from_slice(&nonce);
        raw.extend_from_slice(&ct);
        let b64 = crate::render::base64(&raw);
        let mut out = format!("{BEGIN}\nv1 argon2id m={} t={} p={}\n", self.cost.0, self.cost.1, self.cost.2);
        for chunk in b64.as_bytes().chunks(76) {
            out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
            out.push('\n');
        }
        out.push_str(END);
        out.push('\n');
        Ok(out)
    }
}

pub(crate) fn decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

struct Parsed {
    cost: (u32, u32, u32),
    raw: Vec<u8>,
}

fn parse(armored: &str) -> Result<Parsed> {
    let mut lines = armored.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next() != Some(BEGIN) {
        return Err(bad());
    }
    let header = lines.next().ok_or_else(bad)?;
    let mut cost = COST;
    let mut parts = header.split_whitespace();
    if parts.next() != Some("v1") || parts.next() != Some("argon2id") {
        return Err(bad());
    }
    for p in parts {
        let (k, v) = p.split_once('=').ok_or_else(bad)?;
        let v: u32 = v.parse().map_err(|_| bad())?;
        match k {
            "m" => cost.0 = v,
            "t" => cost.1 = v,
            "p" => cost.2 = v,
            _ => {}
        }
    }
    let body: String = lines.take_while(|l| *l != END).collect();
    let raw = decode(&body).ok_or_else(bad)?;
    if raw.len() < SALT_LEN + NONCE_LEN + 16 {
        return Err(bad());
    }
    Ok(Parsed { cost, raw })
}

fn decrypt(key: &VaultKey, raw: &[u8]) -> Result<String> {
    let nonce = &raw[SALT_LEN..SALT_LEN + NONCE_LEN];
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key.key));
    let plain = cipher.decrypt(Nonce::from_slice(nonce), &raw[SALT_LEN + NONCE_LEN..]).map_err(|_| bad())?;
    String::from_utf8(plain).map_err(|_| bad())
}

/// Decrypt an armored body with the passphrase. Returns the text and the key (reuse it to save).
pub fn open(pass: &str, armored: &str) -> Result<(String, VaultKey)> {
    let p = parse(armored)?;
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&p.raw[..SALT_LEN]);
    let key = VaultKey::derive(pass, salt, p.cost)?;
    let plain = decrypt(&key, &p.raw)?;
    Ok((plain, key))
}

/// Decrypt with a key already derived for this page (no new key derivation).
pub fn open_with(key: &VaultKey, armored: &str) -> Result<String> {
    decrypt(key, &parse(armored)?.raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_wrong_passphrase() {
        let key = VaultKey::new("correct horse").unwrap();
        let sealed = key.seal("Secret é€ text\n- item").unwrap();
        assert!(sealed.starts_with(BEGIN) && sealed.trim_end().ends_with(END));
        assert!(!sealed.contains("Secret"));
        let (plain, _) = open("correct horse", &sealed).unwrap();
        assert_eq!(plain, "Secret é€ text\n- item");
        assert!(open("wrong", &sealed).is_err());
        assert_eq!(open_with(&key, &sealed).unwrap(), plain);
    }

    #[test]
    fn two_seals_differ_and_tampering_is_detected() {
        let key = VaultKey::new("pw").unwrap();
        let a = key.seal("same").unwrap();
        let b = key.seal("same").unwrap();
        assert_ne!(a, b, "fresh nonce each time");
        let mut lines: Vec<String> = a.lines().map(String::from).collect();
        let n = lines.len();
        let mid = &mut lines[n - 2];
        let flipped = if mid.starts_with('A') { "B" } else { "A" };
        mid.replace_range(0..1, flipped);
        assert!(open("pw", &lines.join("\n")).is_err());
    }

    #[test]
    fn debug_never_shows_the_key() {
        let key = VaultKey::new("pw").unwrap();
        assert_eq!(format!("{key:?}"), "VaultKey(<hidden>)");
    }
}
