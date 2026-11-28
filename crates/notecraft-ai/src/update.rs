//! Looking for a newer version of the app: one anonymous request to GitHub's public release list.
//! Nothing about the person or the Mac is sent.

use std::time::Duration;

const API: &str = "https://api.github.com/repos/yanickimhof/notecraft/releases/latest";
const PAGE_PREFIX: &str = "https://github.com/yanickimhof/notecraft/";

/// The latest published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Without the leading `v`.
    pub version: String,
    /// The release page on GitHub.
    pub url: String,
}

/// The latest release, or `None` when it cannot be read (offline, refused, lockdown).
pub fn latest_release() -> Option<Release> {
    notecraft_net::check(notecraft_net::Purpose::UpdateCheck, API).ok()?;
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(4)))
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut resp =
        agent.get(API).header("User-Agent", "NoteCraft").header("Accept", "application/vnd.github+json").call().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.body_mut().with_config().limit(200_000).read_to_string().ok()?;
    parse_release(&body)
}

/// Reads the answer of the releases API. The page address must be on the project's own GitHub page.
pub fn parse_release(json: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = v.get("tag_name")?.as_str()?;
    let url = v.get("html_url")?.as_str()?;
    if !url.starts_with(PAGE_PREFIX) || v.get("draft").and_then(|d| d.as_bool()) == Some(true) {
        return None;
    }
    let version = tag.trim_start_matches('v').to_string();
    parts(&version)?;
    Some(Release { version, url: url.to_string() })
}

/// True when `latest` is a higher version than `current` (both like `0.6.0`).
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parts(current), parts(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

fn parts(v: &str) -> Option<Vec<u32>> {
    let core = v.split(['-', '+']).next()?;
    let p: Option<Vec<u32>> = core.split('.').map(|n| n.parse().ok()).collect();
    p.filter(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_compared_by_number() {
        assert!(is_newer("0.6.0", "0.6.1"));
        assert!(is_newer("0.6.9", "0.10.0"));
        assert!(!is_newer("0.6.0", "0.6.0"));
        assert!(!is_newer("0.6.1", "0.6.0"));
        assert!(!is_newer("0.6.0", "garbage"));
    }

    #[test]
    fn release_answer_is_read_and_checked() {
        let ok = r#"{"tag_name":"v0.7.0","html_url":"https://github.com/yanickimhof/notecraft/releases/tag/v0.7.0"}"#;
        let r = parse_release(ok).unwrap();
        assert_eq!(r.version, "0.7.0");
        let elsewhere = r#"{"tag_name":"v0.7.0","html_url":"https://evil.example/x"}"#;
        assert!(parse_release(elsewhere).is_none());
        assert!(parse_release("not json").is_none());
        let draft =
            r#"{"tag_name":"v0.7.0","draft":true,"html_url":"https://github.com/yanickimhof/notecraft/releases/x"}"#;
        assert!(parse_release(draft).is_none());
    }
}
