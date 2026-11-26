//! Publishing pages on the web through the user's own Cloudflare account.
//!
//! One Worker (`notecraft-pub`) serves pages kept in one KV namespace (`notecraft-pages`).
//! Publishing writes keys `<secret>/index.html`, `<secret>/<slug>/index.html` and
//! `<secret>/a/<file>`; unpublishing deletes them. The secret is random, so an address cannot be
//! guessed, and the Worker answers 404 for anything else. The API token comes from the Keychain.

use super::{agent, Timeouts};
use serde_json::{json, Value};
use std::fmt;

const API: &str = "https://api.cloudflare.com/client/v4";
const WORKER_NAME: &str = "notecraft-pub";
const NAMESPACE: &str = "notecraft-pages";
const WORKER_JS: &str = include_str!("publish_worker.js");

#[derive(Debug, Clone, PartialEq)]
pub enum PubError {
    /// The token is refused or lacks a permission.
    BadToken(String),
    /// The account has no workers.dev name yet: the user has to pick one (`set_subdomain`).
    NeedSubdomain,
    Network(String),
    Api(String),
}

impl fmt::Display for PubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PubError::BadToken(m) => write!(f, "Cloudflare refused the token: {m}"),
            PubError::NeedSubdomain => write!(f, "Choose a name for your web address first"),
            PubError::Network(m) => write!(f, "Network problem: {m}"),
            PubError::Api(m) => write!(f, "Cloudflare: {m}"),
        }
    }
}

impl std::error::Error for PubError {}

/// What setup found or created; keep it to skip setup next time.
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    pub account: String,
    pub namespace: String,
    /// `https://notecraft-pub.<name>.workers.dev`
    pub host: String,
}

pub struct Cloudflare {
    token: String,
    t: Timeouts,
}

fn guard(url: &str) -> Result<(), PubError> {
    notecraft_net::check(notecraft_net::Purpose::Publish, url).map_err(|b| PubError::Network(b.to_string()))
}

fn net(e: ureq::Error) -> PubError {
    PubError::Network(e.to_string())
}

/// Percent-encode a KV key (slashes included).
pub fn enc(key: &str) -> String {
    let mut o = String::new();
    for b in key.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// A new secret for a published page or section: 128 random bits, URL safe (22 characters).
pub fn new_secret() -> String {
    let mut bytes = [0u8; 16];
    let mut filled = false;
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            filled = f.read_exact(&mut bytes).is_ok();
        }
    }
    if !filled {
        use std::hash::{BuildHasher, Hasher};
        for (i, chunk) in bytes.chunks_mut(8).enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i as u64);
            h.write_u128(
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0),
            );
            chunk.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut o = String::new();
    for c in bytes.chunks(3) {
        let n =
            (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        if c.len() > 1 {
            o.push(T[(n >> 6) as usize & 63] as char);
        }
        if c.len() > 2 {
            o.push(T[n as usize & 63] as char);
        }
    }
    o
}

/// A workers.dev name: 3 to 30 lowercase letters, digits or hyphens, not starting or ending with one.
pub fn valid_subdomain(s: &str) -> bool {
    (3..=30).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

impl Cloudflare {
    pub fn new(token: &str, t: Timeouts) -> Self {
        Cloudflare { token: token.trim().to_string(), t }
    }

    fn read(mut resp: ureq::http::Response<ureq::Body>) -> Result<(u16, Value), PubError> {
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string().map_err(net)?;
        let v: Value = serde_json::from_str(&body)
            .unwrap_or_else(|_| json!({ "raw": body.chars().take(200).collect::<String>() }));
        Ok((status, v))
    }

    fn message(v: &Value) -> String {
        v["errors"]
            .as_array()
            .map(|a| a.iter().filter_map(|e| e["message"].as_str()).collect::<Vec<_>>().join("; "))
            .filter(|m| !m.is_empty())
            .or_else(|| v["raw"].as_str().map(String::from))
            .unwrap_or_else(|| "unknown error".into())
    }

    fn check(status: u16, v: &Value) -> Result<(), PubError> {
        match status {
            200..=299 if v["success"].as_bool().unwrap_or(true) => Ok(()),
            401 | 403 => Err(PubError::BadToken(Self::message(v))),
            _ => Err(PubError::Api(format!("{} ({status})", Self::message(v)))),
        }
    }

    fn get(&self, path: &str) -> Result<Value, PubError> {
        guard(API)?;
        let resp = agent(self.t)
            .get(format!("{API}{path}"))
            .header("Authorization", format!("Bearer {}", self.token))
            .call()
            .map_err(net)?;
        let (s, v) = Self::read(resp)?;
        Self::check(s, &v)?;
        Ok(v)
    }

    fn send(&self, put: bool, path: &str, ctype: &str, body: &[u8]) -> Result<Value, PubError> {
        guard(API)?;
        let a = agent(self.t);
        let url = format!("{API}{path}");
        let resp = if put { a.put(url) } else { a.post(url) }
            .header("Authorization", format!("Bearer {}", self.token))
            .content_type(ctype)
            .send(body)
            .map_err(net)?;
        let (s, v) = Self::read(resp)?;
        Self::check(s, &v)?;
        Ok(v)
    }

    fn delete(&self, path: &str) -> Result<Value, PubError> {
        guard(API)?;
        let resp = agent(self.t)
            .delete(format!("{API}{path}"))
            .header("Authorization", format!("Bearer {}", self.token))
            .call()
            .map_err(net)?;
        let (s, v) = Self::read(resp)?;
        // deleting something that is already gone is fine
        if s == 404 {
            return Ok(v);
        }
        Self::check(s, &v)?;
        Ok(v)
    }

    /// Find or create everything the pages need, from the token alone. Safe to repeat.
    pub fn setup(&self) -> Result<Setup, PubError> {
        let accounts = self.get("/accounts")?;
        let account = accounts["result"][0]["id"]
            .as_str()
            .ok_or_else(|| PubError::BadToken("no Cloudflare account found for this token".into()))?
            .to_string();
        let acc = format!("/accounts/{account}");

        let list = self.get(&format!("{acc}/storage/kv/namespaces?per_page=100"))?;
        let found = list["result"]
            .as_array()
            .and_then(|a| a.iter().find(|n| n["title"] == NAMESPACE))
            .and_then(|n| n["id"].as_str())
            .map(String::from);
        let namespace = match found {
            Some(id) => id,
            None => {
                let v = self.send(
                    false,
                    &format!("{acc}/storage/kv/namespaces"),
                    "application/json",
                    json!({ "title": NAMESPACE }).to_string().as_bytes(),
                )?;
                v["result"]["id"].as_str().ok_or_else(|| PubError::Api("no namespace id".into()))?.to_string()
            }
        };

        let meta = json!({
            "main_module": "worker.js",
            "compatibility_date": "2026-11-01",
            "bindings": [{ "type": "kv_namespace", "name": "PAGES", "namespace_id": namespace }],
        });
        let b = "----ncpub7f3a91c2";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!("--{b}\r\nContent-Disposition: form-data; name=\"metadata\"\r\nContent-Type: application/json\r\n\r\n{meta}\r\n").as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "--{b}\r\nContent-Disposition: form-data; name=\"worker.js\"; filename=\"worker.js\"\r\nContent-Type: application/javascript+module\r\n\r\n{WORKER_JS}\r\n--{b}--\r\n"
            )
            .as_bytes(),
        );
        self.send(
            true,
            &format!("{acc}/workers/scripts/{WORKER_NAME}"),
            &format!("multipart/form-data; boundary={b}"),
            &body,
        )?;

        let sub = self
            .get(&format!("{acc}/workers/subdomain"))
            .ok()
            .and_then(|v| v["result"]["subdomain"].as_str().map(String::from));
        let Some(sub) = sub.filter(|s| !s.is_empty()) else { return Err(PubError::NeedSubdomain) };
        self.send(
            false,
            &format!("{acc}/workers/scripts/{WORKER_NAME}/subdomain"),
            "application/json",
            json!({ "enabled": true, "previews_enabled": false }).to_string().as_bytes(),
        )?;
        Ok(Setup { account, namespace, host: format!("https://{WORKER_NAME}.{sub}.workers.dev") })
    }

    /// Register the workers.dev name of the account (first time only). Names are global.
    pub fn set_subdomain(&self, account: &str, name: &str) -> Result<(), PubError> {
        if !valid_subdomain(name) {
            return Err(PubError::Api("use 3 to 30 lowercase letters, digits or hyphens".into()));
        }
        self.send(
            true,
            &format!("/accounts/{account}/workers/subdomain"),
            "application/json",
            json!({ "subdomain": name }).to_string().as_bytes(),
        )
        .map(|_| ())
    }

    /// The account to use before `setup` has succeeded (for `set_subdomain`).
    pub fn account_id(&self) -> Result<String, PubError> {
        let v = self.get("/accounts")?;
        v["result"][0]["id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| PubError::BadToken("no Cloudflare account found".into()))
    }

    pub fn put(&self, s: &Setup, key: &str, bytes: &[u8]) -> Result<(), PubError> {
        self.send(
            true,
            &format!("/accounts/{}/storage/kv/namespaces/{}/values/{}", s.account, s.namespace, enc(key)),
            "application/octet-stream",
            bytes,
        )
        .map(|_| ())
    }

    pub fn remove(&self, s: &Setup, key: &str) -> Result<(), PubError> {
        self.delete(&format!("/accounts/{}/storage/kv/namespaces/{}/values/{}", s.account, s.namespace, enc(key)))
            .map(|_| ())
    }

    /// Keys starting with `prefix` (all pages of a publication).
    pub fn keys(&self, s: &Setup, prefix: &str) -> Result<Vec<String>, PubError> {
        let v = self.get(&format!(
            "/accounts/{}/storage/kv/namespaces/{}/keys?prefix={}&limit=1000",
            s.account,
            s.namespace,
            enc(prefix)
        ))?;
        Ok(v["result"]
            .as_array()
            .map(|a| a.iter().filter_map(|k| k["name"].as_str().map(String::from)).collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_long_and_url_safe() {
        let a = new_secret();
        assert_eq!(a.len(), 22);
        assert!(a.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert_ne!(a, new_secret());
    }

    #[test]
    fn keys_are_encoded() {
        assert_eq!(enc("tok/a/x.png"), "tok%2Fa%2Fx.png");
    }

    #[test]
    fn subdomain_rules() {
        assert!(valid_subdomain("nc-yim"));
        assert!(!valid_subdomain("-ab"));
        assert!(!valid_subdomain("AB"));
        assert!(!valid_subdomain("ab"));
    }
}
