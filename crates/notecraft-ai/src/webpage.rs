//! Fetching the title of a web page, for bookmark blocks. Only runs when the person pastes a link.

use std::time::Duration;

/// The text of the `<title>` of the page at `url` (http or https only), or `None` when it cannot be read.
pub fn page_title(url: &str) -> Option<String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    // a pasted link must never probe this Mac or the local network, nor leave in lockdown; every hop of a
    // redirect is checked again, and a name that resolves to a private address is refused
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(8)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut current = url.to_string();
    for _ in 0..4 {
        notecraft_net::check(notecraft_net::Purpose::LinkPreview, &current).ok()?;
        if notecraft_net::resolves_privately(&current) {
            return None;
        }
        let mut resp = agent.get(&current).header("User-Agent", "Mozilla/5.0 NoteCraft").call().ok()?;
        if resp.status().is_redirection() {
            let next = resp.headers().get("location")?.to_str().ok()?.to_string();
            current = resolve_location(&current, &next)?;
            continue;
        }
        let html = resp.body_mut().with_config().limit(300_000).read_to_string().ok()?;
        return title_of(&html);
    }
    None
}

/// The absolute address a `Location` header points to (http or https only).
fn resolve_location(base: &str, location: &str) -> Option<String> {
    let next = if location.starts_with("http://") || location.starts_with("https://") {
        location.to_string()
    } else {
        let path = location.strip_prefix('/')?;
        let (scheme, rest) = base.split_once("://")?;
        let host = rest.split(['/', '?', '#']).next()?;
        format!("{scheme}://{host}/{path}")
    };
    (next.starts_with("http://") || next.starts_with("https://")).then_some(next)
}

/// The decoded text of the first `<title>` element.
pub fn title_of(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</title")? + open_end;
    let raw = html.get(open_end..close)?;
    let title = decode_entities(raw).split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

fn decode_entities(s: &str) -> String {
    let mut out = s.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
    out = out.replace("&#39;", "'").replace("&apos;", "'").replace("&#x27;", "'");
    out.replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_stay_on_web_addresses() {
        assert_eq!(resolve_location("https://a.com/x?y", "/z").as_deref(), Some("https://a.com/z"));
        assert_eq!(resolve_location("https://a.com/", "https://b.com/q").as_deref(), Some("https://b.com/q"));
        assert_eq!(resolve_location("https://a.com/", "file:///etc/passwd"), None);
        assert_eq!(resolve_location("https://a.com/", "relative"), None);
    }

    #[test]
    fn finds_and_cleans_the_title() {
        let html = "<html><head><TITLE>\n  Tom &amp; Jerry's\n  page </TITLE></head></html>";
        assert_eq!(title_of(html).as_deref(), Some("Tom & Jerry's page"));
        assert_eq!(title_of("<html></html>"), None);
        assert_eq!(title_of("<title> </title>"), None);
    }

    #[test]
    fn only_web_addresses_are_fetched() {
        assert_eq!(page_title("file:///etc/passwd"), None);
        assert_eq!(page_title("http://127.0.0.1:1/"), None);
        assert_eq!(page_title("http://192.168.1.1/admin"), None);
        assert_eq!(page_title("http://[::ffff:127.0.0.1]:1/"), None);
        assert_eq!(page_title("http://0x7f.1/"), None);
    }

    /// Needs the network: `cargo test -p notecraft-ai -- --ignored`.
    #[test]
    #[ignore]
    fn reads_a_real_page() {
        let t = page_title("https://example.com").expect("a title");
        assert!(t.contains("Example"), "{t}");
    }
}
