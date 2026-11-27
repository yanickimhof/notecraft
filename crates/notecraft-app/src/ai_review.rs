//! Whole-page AI review (proofread, clarify, restructure): the page goes to the AI as Markdown, with the
//! elements it must not touch (transcripts, databases, images, code, the meeting block...) replaced by
//! `@@KEEPn@@` markers, and the answer is put back together with those elements restored untouched.

use notecraft_core::blocks::{self, Block, BlockKind};
use std::collections::BTreeMap;

/// The page ready to send: Markdown with markers, and the elements the markers stand for.
pub struct Prepared {
    pub markdown: String,
    pub kept: Vec<Vec<Block>>,
}

fn is_transcript(b: &Block) -> bool {
    matches!(b.kind, BlockKind::Toggle { .. }) && {
        let t = b.text.trim().to_lowercase();
        t.contains("transcript")
    }
}

/// Text the AI may rewrite. Everything else (and every transcript) is kept as it is.
fn editable(b: &Block) -> bool {
    !is_transcript(b)
        && matches!(
            b.kind,
            BlockKind::Paragraph
                | BlockKind::Heading(_)
                | BlockKind::Bulleted
                | BlockKind::Numbered
                | BlockKind::Todo { .. }
                | BlockKind::Toggle { .. }
                | BlockKind::Quote
                | BlockKind::Callout { .. }
                | BlockKind::Table
        )
}

pub fn marker(n: usize) -> String {
    format!("@@KEEP{n}@@")
}

pub fn prepare(blocks: &[Block]) -> Prepared {
    let mut md = String::new();
    let mut kept: Vec<Vec<Block>> = Vec::new();
    let mut run: Vec<Block> = Vec::new();
    let flush = |run: &mut Vec<Block>, md: &mut String| {
        if !run.is_empty() {
            md.push_str(&blocks::to_markdown(run));
            md.push('\n');
            run.clear();
        }
    };
    let mut i = 0;
    while i < blocks.len() {
        if editable(&blocks[i]) {
            run.push(blocks[i].clone());
            i += 1;
        } else {
            flush(&mut run, &mut md);
            let end = blocks::subtree_end(blocks, i).max(i + 1);
            kept.push(blocks[i..end].to_vec());
            md.push_str(&marker(kept.len() - 1));
            md.push_str("\n\n");
            i = end;
        }
    }
    flush(&mut run, &mut md);
    Prepared { markdown: md.trim().to_string(), kept }
}

/// The Markdown with the markers shown as a plain note (for the "original" view).
pub fn readable(md: &str, kept: usize) -> String {
    let mut s = md.to_string();
    for n in 0..kept {
        s = s.replace(&marker(n), "*[element kept as it is]*");
    }
    s
}

/// Cut the Markdown into pieces of at most `max` characters, between paragraphs.
pub fn chunks(md: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for para in md.split("\n\n") {
        if !cur.is_empty() && cur.chars().count() + para.chars().count() + 2 > max {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Every number in a text (runs of digits), sorted, to spot a figure or a date the AI changed.
pub fn numbers(text: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            v.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        v.push(cur);
    }
    v.sort();
    v
}

/// True when the kept elements are not all present exactly once in the answer.
pub fn lost_marker(text: &str, kept: usize) -> bool {
    (0..kept).any(|n| text.matches(&marker(n)).count() != 1)
}

/// The page after the AI's answer: markers replaced by the kept elements. When the answer has the same
/// shape as the page (same kinds in the same order), only the text changes, so colours and ids stay.
pub fn restore(old: &[Block], kept: &[Vec<Block>], text: &str) -> Option<Vec<Block>> {
    if lost_marker(text, kept.len()) {
        return None;
    }
    let by_marker: BTreeMap<String, usize> = (0..kept.len()).map(|n| (marker(n), n)).collect();
    let mut out: Vec<Block> = Vec::new();
    for b in blocks::from_markdown(text) {
        match by_marker.get(b.text.trim()) {
            Some(&n) if b.kind == BlockKind::Paragraph => out.extend(kept[n].iter().cloned()),
            _ => out.push(b),
        }
    }
    if out.is_empty() {
        return None;
    }
    if out.len() == old.len()
        && out.iter().zip(old).all(|(n, o)| std::mem::discriminant(&n.kind) == std::mem::discriminant(&o.kind))
    {
        let merged: Vec<Block> = out
            .iter()
            .zip(old)
            .map(|(n, o)| {
                if n.id == o.id {
                    o.clone()
                } else if editable(o) {
                    let mut b = o.clone();
                    b.kind = n.kind.clone();
                    b.text = n.text.clone();
                    b.depth = n.depth;
                    b
                } else {
                    o.clone()
                }
            })
            .collect();
        return Some(merged);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Vec<Block> {
        let mut h = Block::new(BlockKind::Heading(1), "Plan");
        h.color = "red".into();
        vec![
            h,
            Block::new(BlockKind::Paragraph, "Voici le text du 12 mars."),
            Block::new(BlockKind::Database, "Tasks"),
            Block::new(BlockKind::Toggle { open: false }, "Transcript"),
            {
                let mut c = Block::new(BlockKind::Paragraph, "dit un");
                c.depth = 1;
                c
            },
            Block::new(BlockKind::Code { lang: "rust".into() }, "fn main() {}"),
            Block::new(BlockKind::Bulleted, "a"),
        ]
    }

    #[test]
    fn sensitive_elements_become_markers() {
        let p = prepare(&page());
        assert_eq!(p.kept.len(), 3);
        assert_eq!(p.kept[1].len(), 2, "the transcript keeps its content");
        assert!(p.markdown.contains("@@KEEP0@@") && p.markdown.contains("@@KEEP2@@"));
        assert!(!p.markdown.contains("dit un") && !p.markdown.contains("fn main"));
    }

    #[test]
    fn a_text_fix_keeps_everything_else_untouched() {
        let old = page();
        let p = prepare(&old);
        let fixed = p.markdown.replace("text du", "texte du");
        let new = restore(&old, &p.kept, &fixed).expect("restored");
        assert_eq!(new.len(), old.len());
        assert_eq!(new[1].text, "Voici le texte du 12 mars.");
        assert_eq!(new[0].color, "red", "colours survive a text fix");
        assert_eq!(new[2].kind, BlockKind::Database);
        assert_eq!(new[4].text, "dit un");
        assert_eq!(new[5].text, "fn main() {}");
    }

    #[test]
    fn a_dropped_marker_cancels_the_review() {
        let old = page();
        let p = prepare(&old);
        let bad = p.markdown.replace("@@KEEP1@@", "");
        assert!(restore(&old, &p.kept, &bad).is_none());
        let twice = format!("{}\n\n@@KEEP0@@", p.markdown);
        assert!(restore(&old, &p.kept, &twice).is_none());
    }

    #[test]
    fn a_restructured_answer_replaces_the_page_but_keeps_the_markers() {
        let old = page();
        let p = prepare(&old);
        let answer = "## Summary\n\nShort.\n\n@@KEEP1@@\n\n@@KEEP0@@\n\n@@KEEP2@@\n\n- a";
        let new = restore(&old, &p.kept, answer).expect("restored");
        assert_eq!(new[0].kind, BlockKind::Heading(2));
        assert!(new.iter().any(|b| b.kind == BlockKind::Database));
        assert!(new.iter().any(|b| b.text == "dit un"));
    }

    #[test]
    fn numbers_and_chunks() {
        assert_eq!(numbers("le 12 mars, 3 fois"), vec!["12".to_string(), "3".to_string()]);
        let md = (0..10).map(|i| format!("para {i} {}", "x".repeat(40))).collect::<Vec<_>>().join("\n\n");
        let c = chunks(&md, 150);
        assert!(c.len() > 2 && c.iter().all(|s| s.chars().count() <= 150));
        assert_eq!(c.join("\n\n"), md);
    }
}
