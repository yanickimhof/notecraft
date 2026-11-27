//! Network guard. Every outbound connection of the app asks [`check`] first, says why it connects
//! ([`Purpose`]) and where, and the answer is logged. Rules:
//!
//! * a loopback address (this Mac) is always fine, unless the lockdown is on for non-local purposes only;
//! * the lockdown refuses everything that is not this Mac;
//! * the local model refuses any other address unless the lock was opened by the user;
//! * model downloads only go to Hugging Face, publishing only to Cloudflare, cloud AI only when not locked;
//! * link previews never reach a private network address (a pasted link must not probe the local network).
//!
//! Redirects are followed by the callers themselves, which check each address again.

use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

/// Why the app connects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// The local model (Ollama).
    LocalAi,
    /// A cloud AI service chosen by the user.
    CloudAi,
    /// Downloading a speech or language model.
    ModelDownload,
    /// Publishing a page to the user's own Cloudflare account.
    Publish,
    /// Reading the title of a pasted link.
    LinkPreview,
}

impl Purpose {
    pub fn label(self) -> &'static str {
        match self {
            Purpose::LocalAi => "local AI",
            Purpose::CloudAi => "cloud AI",
            Purpose::ModelDownload => "model download",
            Purpose::Publish => "publish",
            Purpose::LinkPreview => "link preview",
        }
    }
}

/// A refused connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    pub host: String,
    pub reason: &'static str,
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not allowed: {}", self.host, self.reason)
    }
}

impl std::error::Error for Blocked {}

/// One decision, kept in memory for the "network activity" view.
#[derive(Debug, Clone)]
pub struct Event {
    pub at: SystemTime,
    pub purpose: Purpose,
    pub host: String,
    pub allowed: bool,
    pub reason: &'static str,
}

const LOG_CAP: usize = 200;

static LOCKDOWN: AtomicBool = AtomicBool::new(false);
static ALLOW_REMOTE_LOCAL_AI: AtomicBool = AtomicBool::new(false);
static LOG: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

/// Lockdown: nothing leaves this Mac.
pub fn set_lockdown(on: bool) {
    LOCKDOWN.store(on, Ordering::Relaxed);
}

pub fn lockdown() -> bool {
    LOCKDOWN.load(Ordering::Relaxed)
}

/// Lets the local model use an address that is not on this Mac.
pub fn set_allow_remote_local_ai(on: bool) {
    ALLOW_REMOTE_LOCAL_AI.store(on, Ordering::Relaxed);
}

/// The host part of an address (no scheme, user, port, path), lower case; IPv6 without brackets.
pub fn host_of(url: &str) -> String {
    let rest = url.trim().split("://").nth(1).unwrap_or(url.trim());
    let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
    // what comes before the last @ is credentials, never the host
    let auth = auth.rsplit('@').next().unwrap_or("");
    let host = if let Some(inner) = auth.strip_prefix('[') {
        inner.split(']').next().unwrap_or("")
    } else {
        auth.split(':').next().unwrap_or("")
    };
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// True for this Mac: `localhost` and real loopback addresses (not names that merely look like one).
pub fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// True for this Mac or a private network (RFC 1918, link-local, unique-local, `.local`, `.internal`).
pub fn is_private_host(host: &str) -> bool {
    if is_loopback_host(host) || host.ends_with(".local") || host.ends_with(".internal") || host.ends_with(".lan") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => is_private_ip(IpAddr::V4(ip)),
        Ok(IpAddr::V6(ip)) => is_private_ip(IpAddr::V6(ip)),
        Err(_) => !host.contains('.'),
    }
}

/// True for an address of this Mac or of a private network, including an IPv4 address written as IPv6.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            // 100.64.0.0/10 is left out on purpose: corporate proxies such as Zscaler answer from it
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast()
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_private_ip(IpAddr::V4(v4));
            }
            let s = ip.segments();
            ip.is_loopback() || ip.is_unspecified() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
        }
    }
}

/// True when the host of `url` resolves to this Mac or a private network, whatever way its address is
/// written. A name that cannot be resolved counts as private (nothing to fetch anyway).
pub fn resolves_privately(url: &str) -> bool {
    use std::net::ToSocketAddrs;
    let host = host_of(url);
    if host.is_empty() {
        return true;
    }
    match (host.as_str(), 443).to_socket_addrs() {
        Ok(mut addrs) => addrs.any(|a| is_private_ip(a.ip())),
        Err(_) => true,
    }
}

/// [`is_loopback_host`] for a whole address.
pub fn is_local_url(url: &str) -> bool {
    is_loopback_host(&host_of(url))
}

fn on_domain(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|p| p.ends_with('.'))
}

fn decide(purpose: Purpose, host: &str) -> Result<(), &'static str> {
    if host.is_empty() {
        return Err("no address");
    }
    let local = is_loopback_host(host);
    if lockdown() && !local {
        return Err("lockdown is on: nothing leaves this Mac");
    }
    match purpose {
        Purpose::LocalAi => {
            if local || ALLOW_REMOTE_LOCAL_AI.load(Ordering::Relaxed) {
                Ok(())
            } else {
                Err("the local model only talks to this Mac (the lock is closed)")
            }
        }
        Purpose::CloudAi => Ok(()),
        Purpose::ModelDownload => {
            if on_domain(host, "huggingface.co") || on_domain(host, "hf.co") {
                Ok(())
            } else {
                Err("models are only downloaded from Hugging Face")
            }
        }
        Purpose::Publish => {
            if on_domain(host, "cloudflare.com") || on_domain(host, "workers.dev") {
                Ok(())
            } else {
                Err("pages are only published to Cloudflare")
            }
        }
        Purpose::LinkPreview => {
            if is_private_host(host) {
                Err("a link preview never reaches this Mac or a private network")
            } else {
                Ok(())
            }
        }
    }
}

/// Asks permission to connect to `url` for `purpose`. Every answer is logged.
pub fn check(purpose: Purpose, url: &str) -> Result<(), Blocked> {
    let host = host_of(url);
    let res = decide(purpose, &host);
    record(Event {
        at: SystemTime::now(),
        purpose,
        host: host.clone(),
        allowed: res.is_ok(),
        reason: res.err().unwrap_or("allowed"),
    });
    res.map_err(|reason| Blocked { host, reason })
}

fn record(e: Event) {
    // a poisoned lock only means another thread panicked while logging: keep logging
    let mut log = LOG.lock().unwrap_or_else(|p| p.into_inner());
    if log.len() >= LOG_CAP {
        log.pop_front();
    }
    log.push_back(e);
}

/// The latest decisions, newest first.
pub fn recent() -> Vec<Event> {
    let log = LOG.lock().unwrap_or_else(|p| p.into_inner());
    log.iter().rev().cloned().collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn mapped_and_odd_addresses_are_private() {
        use std::net::IpAddr;
        for a in ["::ffff:127.0.0.1", "::ffff:192.168.1.5", "::1", "10.0.0.1", "169.254.1.1"] {
            assert!(is_private_ip(a.parse::<IpAddr>().unwrap()), "{a}");
        }
        for a in ["8.8.8.8", "100.64.1.1", "2606:4700::1111"] {
            assert!(!is_private_ip(a.parse::<IpAddr>().unwrap()), "{a}");
        }
    }

    use super::*;

    // the switches are process-wide: tests that touch them run one after the other
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn hosts_are_read_the_way_a_client_reads_them() {
        assert_eq!(host_of("http://localhost:11434/api"), "localhost");
        assert_eq!(host_of("HTTPS://Example.COM./x?y"), "example.com");
        assert_eq!(host_of("http://[::1]:11434"), "::1");
        // credentials before the @ are not the host
        assert_eq!(host_of("http://localhost@evil.example.com/"), "evil.example.com");
        assert_eq!(host_of("http://127.0.0.1#@evil.example.com"), "127.0.0.1");
    }

    #[test]
    fn look_alike_names_are_not_loopback() {
        assert!(is_local_url("http://127.0.0.1:1"));
        assert!(is_local_url("http://[::1]:1"));
        assert!(is_local_url("http://localhost"));
        assert!(!is_local_url("http://127.evil.example.com"));
        assert!(!is_local_url("http://localhost.evil.example.com"));
        assert!(!is_local_url("http://localhost@evil.example.com"));
        assert!(!is_local_url("http://192.168.1.20:11434"));
    }

    #[test]
    fn private_networks_are_recognised() {
        for h in [
            "192.168.0.1",
            "10.1.2.3",
            "172.16.0.9",
            "169.254.1.1",
            "fe80::1",
            "fd00::1",
            "nas",
            "printer.local",
            "0.0.0.0",
        ] {
            assert!(is_private_host(h), "{h}");
        }
        for h in ["example.com", "8.8.8.8", "2606:4700::1111"] {
            assert!(!is_private_host(h), "{h}");
        }
    }

    #[test]
    fn each_purpose_has_its_own_rule() {
        let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        set_lockdown(false);
        set_allow_remote_local_ai(false);
        assert!(check(Purpose::LocalAi, "http://localhost:11434").is_ok());
        assert!(check(Purpose::LocalAi, "http://10.0.0.2:11434").is_err());
        set_allow_remote_local_ai(true);
        assert!(check(Purpose::LocalAi, "http://10.0.0.2:11434").is_ok());
        set_allow_remote_local_ai(false);
        assert!(check(Purpose::ModelDownload, "https://huggingface.co/a/b").is_ok());
        assert!(check(Purpose::ModelDownload, "https://cdn-lfs.hf.co/x").is_ok());
        assert!(check(Purpose::ModelDownload, "https://huggingface.co.evil.example/x").is_err());
        assert!(check(Purpose::Publish, "https://api.cloudflare.com/client/v4/x").is_ok());
        assert!(check(Purpose::Publish, "https://notecraft-pub.me.workers.dev").is_ok());
        assert!(check(Purpose::Publish, "https://evil.example.com").is_err());
        assert!(check(Purpose::LinkPreview, "http://192.168.1.1/admin").is_err());
        assert!(check(Purpose::LinkPreview, "http://localhost:8080").is_err());
        assert!(check(Purpose::LinkPreview, "https://example.com").is_ok());
        assert!(check(Purpose::CloudAi, "https://api.anthropic.com").is_ok());
    }

    #[test]
    fn the_lockdown_stops_everything_but_this_mac_and_is_logged() {
        let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        set_lockdown(true);
        assert!(check(Purpose::CloudAi, "https://api.anthropic.com").is_err());
        assert!(check(Purpose::ModelDownload, "https://huggingface.co/x").is_err());
        assert!(check(Purpose::LocalAi, "http://127.0.0.1:11434").is_ok());
        let last = recent();
        assert!(last.iter().any(|e| !e.allowed && e.host == "api.anthropic.com"));
        set_lockdown(false);
    }

    #[test]
    fn the_log_keeps_the_latest_events_only() {
        let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        for _ in 0..(LOG_CAP + 20) {
            let _ = check(Purpose::LocalAi, "http://localhost");
        }
        assert!(recent().len() <= LOG_CAP);
    }
}
