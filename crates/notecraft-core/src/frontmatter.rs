//! YAML front matter: split a Markdown file into `(yaml, body)` and join back.

/// Split `---\n<yaml>\n---\n<body>`. Files without front matter return `None`
/// for the YAML part and the whole text as body.
pub fn split(text: &str) -> (Option<&str>, &str) {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = match t.strip_prefix("---\n").or_else(|| t.strip_prefix("---\r\n")) {
        Some(r) => r,
        None => return (None, t),
    };
    // Find a closing line that is exactly `---`.
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (Some(yaml), body);
        }
        offset += line.len();
    }
    // An unterminated block: a closing `---` at the very end without newline.
    if rest.trim_end().ends_with("\n---") {
        let end = rest.trim_end().len() - 3;
        return (Some(&rest[..end]), "");
    }
    (None, t)
}

/// The `format:` number declared in a YAML block; 1 when there is none (files written before it existed).
pub fn declared_format(yaml: &str) -> u32 {
    yaml.lines().find_map(|l| l.strip_prefix("format:")).and_then(|v| v.trim().parse().ok()).unwrap_or(1)
}

/// Join a YAML block and a body into a Markdown file.
pub fn join(yaml: &str, body: &str) -> String {
    let mut out = String::with_capacity(yaml.len() + body.len() + 16);
    out.push_str("---\n");
    out.push_str(yaml.trim_end());
    out.push_str("\n---\n");
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body.trim_start_matches('\n'));
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_join_round_trip() {
        let text = join("id: a\ntitle: Hello", "Body line\n");
        let (y, b) = split(&text);
        assert_eq!(y.unwrap().trim(), "id: a\ntitle: Hello");
        assert_eq!(b.trim(), "Body line");
    }

    #[test]
    fn declared_format_defaults_to_one() {
        assert_eq!(declared_format("id: a\ntitle: x"), 1);
        assert_eq!(declared_format("format: 3\nid: a"), 3);
        assert_eq!(declared_format("id: a\nformat:  2 \n"), 2);
        assert_eq!(declared_format("title: format: 9"), 1);
    }

    #[test]
    fn no_front_matter() {
        let (y, b) = split("# Title\ntext");
        assert!(y.is_none());
        assert_eq!(b, "# Title\ntext");
    }

    #[test]
    fn divider_in_body_is_kept() {
        let text = "---\nid: x\n---\n\nabove\n\n---\n\nbelow\n";
        let (y, b) = split(text);
        assert_eq!(y.unwrap().trim(), "id: x");
        assert!(b.contains("---"));
        assert!(b.contains("below"));
    }
}
