//! Fetching the title of a web page, for bookmark blocks. Only runs when the person pastes a link.

use std::time::Duration;

/// The text of the `<title>` of the page at `url` (http or https only), or `None` when it cannot be read.
pub fn page_title(url: &str) -> Option<String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .new_agent();
    let mut resp = agent.get(url).header("User-Agent", "Mozilla/5.0 NoteCraft").call().ok()?;
    let html = resp.body_mut().with_config().limit(300_000).read_to_string().ok()?;
    title_of(&html)
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
    fn finds_and_cleans_the_title() {
        let html = "<html><head><TITLE>\n  Tom &amp; Jerry's\n  page </TITLE></head></html>";
        assert_eq!(title_of(html).as_deref(), Some("Tom & Jerry's page"));
        assert_eq!(title_of("<html></html>"), None);
        assert_eq!(title_of("<title> </title>"), None);
    }

    #[test]
    fn only_web_addresses_are_fetched() {
        assert_eq!(page_title("file:///etc/passwd"), None);
    }

    /// Needs the network: `cargo test -p notecraft-ai -- --ignored`.
    #[test]
    #[ignore]
    fn reads_a_real_page() {
        let t = page_title("https://example.com").expect("a title");
        assert!(t.contains("Example"), "{t}");
    }
}
