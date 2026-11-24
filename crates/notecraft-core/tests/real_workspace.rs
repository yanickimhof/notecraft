//! Optional check against a real workspace: `NC_REAL_WS=<folder> cargo test -p notecraft-core --test real_workspace -- --nocapture`.
//! Every page body must survive parse, write, parse unchanged (same blocks), so that saving a
//! page, or rewriting it through the MCP server, never alters it. Skipped when the variable is unset.

use notecraft_core::blocks::{from_markdown, to_markdown, Block};

fn strip(b: &[Block]) -> Vec<(String, String, u8, String)> {
    b.iter().map(|x| (format!("{:?}", x.kind), x.text.clone(), x.depth, x.color.clone())).collect()
}

fn markdown_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                markdown_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
}

#[test]
fn real_pages_are_stable_through_a_markdown_round_trip() {
    let Ok(root) = std::env::var("NC_REAL_WS") else { return };
    let mut files = Vec::new();
    markdown_files(&std::path::Path::new(&root).join("pages"), &mut files);
    markdown_files(&std::path::Path::new(&root).join("databases"), &mut files);
    let mut bad = Vec::new();
    for f in &files {
        let t = std::fs::read_to_string(f).unwrap();
        let body = match t.strip_prefix("---\n").and_then(|r| r.split_once("\n---\n")) {
            Some((_, b)) => b.to_string(),
            None => continue,
        };
        let a = from_markdown(&body);
        let b = from_markdown(&to_markdown(&a));
        if strip(&a) != strip(&b) {
            let (sa, sb) = (strip(&a), strip(&b));
            let at = sa.iter().zip(sb.iter()).position(|(x, y)| x != y).unwrap_or(sa.len().min(sb.len()));
            bad.push(format!(
                "{} blocks {} -> {} first diff at {}: {:?} vs {:?}",
                f.display(),
                sa.len(),
                sb.len(),
                at,
                sa.get(at),
                sb.get(at)
            ));
        }
    }
    println!("{} files checked, {} unstable", files.len(), bad.len());
    for b in bad.iter().take(20) {
        println!("{}", b.chars().take(600).collect::<String>());
    }
    assert!(bad.is_empty());
}
