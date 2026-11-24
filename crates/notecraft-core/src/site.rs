//! Export the workspace as a static website: one HTML file per page, a page of rows per database,
//! an index with the tree of pages. Nothing is loaded from the network and nothing is tracked, so
//! the folder can be put on any web server or opened from disk.

use crate::fsutil::slugify;
use crate::render::to_html;
use crate::workspace::{DocId, Workspace};
use crate::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SiteReport {
    pub pages: usize,
    /// Encrypted pages that are still locked: left out, never written in clear.
    pub skipped: usize,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

struct Entry {
    doc: DocId,
    title: String,
    file: String,
    parent: Option<usize>,
}

fn file_name(title: &str, doc: &DocId) -> String {
    let id = match doc {
        DocId::Page(i) | DocId::Database(i) => i.clone(),
        DocId::Row(_, r) => r.clone(),
    };
    let tail: String = id.chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
    let slug = slugify(title);
    format!("{}-{tail}.html", if slug.is_empty() { "untitled" } else { slug.as_str() })
}

fn collect(ws: &Workspace, parent: Option<usize>, doc_parent: Option<&str>, out: &mut Vec<Entry>) {
    for item in ws.children(doc_parent) {
        let idx = out.len();
        out.push(Entry {
            file: file_name(&item.title, &item.doc),
            title: item.title.clone(),
            doc: item.doc.clone(),
            parent,
        });
        match &item.doc {
            DocId::Page(id) => collect(ws, Some(idx), Some(id), out),
            DocId::Database(db) => {
                if let Some(d) = ws.dbs.get(db) {
                    let mut rows: Vec<_> = d.rows.iter().filter(|(_, r)| r.meta.trashed.is_none()).collect();
                    rows.sort_by_key(|(_, r)| r.meta.title.to_lowercase());
                    for (rid, r) in rows {
                        let doc = DocId::Row(db.clone(), rid.clone());
                        out.push(Entry {
                            file: file_name(&r.meta.title, &doc),
                            title: r.meta.title.clone(),
                            doc,
                            parent: Some(idx),
                        });
                    }
                }
            }
            DocId::Row(..) => {}
        }
    }
}

/// Turn `<span class="mention">Title</span>` into a link when the title is a page of the site.
fn link_mentions(html: &str, files: &HashMap<String, String>) -> String {
    const OPEN: &str = "<span class=\"mention\">";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(OPEN) {
        out.push_str(&rest[..i]);
        let after = &rest[i + OPEN.len()..];
        let Some(j) = after.find("</span>") else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = &after[..j];
        let key = unesc(inner).trim().to_lowercase();
        match files.get(&key) {
            Some(file) => out.push_str(&format!("<a class=\"mention\" href=\"{}\">{inner}</a>", esc(file))),
            None => out.push_str(&format!("{OPEN}{inner}</span>")),
        }
        rest = &after[j + 7..];
    }
    out.push_str(rest);
    out
}

const CSS: &str = "nav.crumbs{font:14px -apple-system,sans-serif;color:#787774;margin:0 0 1.2rem}nav.crumbs a{color:#787774;text-decoration:none}nav.crumbs a:hover{text-decoration:underline}a.mention{color:inherit;background:#f1f1ef;border-radius:3px;padding:0 .3em;text-decoration:none}table.rows{border-collapse:collapse;width:100%;font:15px -apple-system,sans-serif}table.rows td,table.rows th{border:1px solid #e9e9e7;padding:6px 10px;text-align:left}table.rows th{background:#f7f7f5;font-weight:600}ul.tree{list-style:none;padding-left:1.2rem}ul.tree li{margin:.3rem 0}body{max-width:46rem;margin:2.5rem auto;padding:0 1rem}";

/// Write the site into `dest` (created if needed). `name` titles the index page.
pub fn export_site(ws: &Workspace, dest: &Path, name: &str) -> Result<SiteReport> {
    let mut entries = Vec::new();
    collect(ws, None, None, &mut entries);
    std::fs::create_dir_all(dest)?;
    // which file a `[[Title]]` leads to (pages and databases only, as in the app)
    let mut files: HashMap<String, String> = HashMap::new();
    for e in &entries {
        if !matches!(e.doc, DocId::Row(..)) && !e.title.trim().is_empty() {
            files.entry(e.title.trim().to_lowercase()).or_insert_with(|| e.file.clone());
        }
    }
    let read = |rel: &str| std::fs::read(ws.asset_path(rel)).ok();
    let mut report = SiteReport::default();
    let mut written = vec![false; entries.len()];
    for (i, e) in entries.iter().enumerate() {
        let mut crumbs = vec!["<a href=\"index.html\">Home</a>".to_string()];
        let mut chain = Vec::new();
        let mut p = e.parent;
        while let Some(pi) = p {
            chain.push(pi);
            p = entries[pi].parent;
        }
        for pi in chain.into_iter().rev() {
            crumbs.push(format!("<a href=\"{}\">{}</a>", esc(&entries[pi].file), esc(&entries[pi].title)));
        }
        let nav = format!("<nav class=\"crumbs\">{}</nav>", crumbs.join(" › "));
        let title = if e.title.trim().is_empty() { "Untitled" } else { e.title.as_str() };
        let html = match &e.doc {
            DocId::Database(db) => {
                let Some(d) = ws.dbs.get(db) else { continue };
                let props: Vec<_> = d.schema.properties.iter().collect();
                let mut t = String::from("<table class=\"rows\"><tr>");
                for p in &props {
                    t.push_str(&format!("<th>{}</th>", esc(&p.name)));
                }
                t.push_str("</tr>");
                for r in entries.iter().filter(|r| r.parent == Some(i)) {
                    let DocId::Row(_, rid) = &r.doc else { continue };
                    let Some(row) = d.rows.get(rid) else { continue };
                    t.push_str("<tr>");
                    for (k, p) in props.iter().enumerate() {
                        if k == 0 {
                            let label = if r.title.trim().is_empty() { "Untitled" } else { r.title.as_str() };
                            t.push_str(&format!("<td><a href=\"{}\">{}</a></td>", esc(&r.file), esc(label)));
                        } else {
                            t.push_str(&format!("<td>{}</td>", esc(&ws.display_value(db, row, p))));
                        }
                    }
                    t.push_str("</tr>");
                }
                t.push_str("</table>");
                let page = to_html(title, &[], &read);
                page.replacen("<body>", &format!("<body>\n{nav}"), 1).replacen("</body>", &format!("{t}</body>"), 1)
            }
            doc => {
                if ws.is_page_locked(match doc {
                    DocId::Page(id) => id,
                    _ => "",
                }) {
                    report.skipped += 1;
                    continue;
                }
                let Some(blocks) = ws.blocks(doc) else { continue };
                let page = to_html(title, blocks, &read);
                let page = link_mentions(&page, &files);
                page.replacen("<body>", &format!("<body>\n{nav}"), 1)
            }
        };
        let html = html.replacen("</style>", &format!("{CSS}</style>"), 1);
        std::fs::write(dest.join(&e.file), html)?;
        written[i] = true;
        report.pages += 1;
    }
    // the index: the tree of what was written
    fn tree(entries: &[Entry], written: &[bool], parent: Option<usize>, out: &mut String) {
        let kids: Vec<usize> = (0..entries.len()).filter(|i| entries[*i].parent == parent && written[*i]).collect();
        if kids.is_empty() {
            return;
        }
        out.push_str("<ul class=\"tree\">");
        for i in kids {
            let label = if entries[i].title.trim().is_empty() { "Untitled" } else { entries[i].title.as_str() };
            out.push_str(&format!("<li><a href=\"{}\">{}</a>", esc(&entries[i].file), esc(label)));
            tree(entries, written, Some(i), out);
            out.push_str("</li>");
        }
        out.push_str("</ul>");
    }
    let mut list = String::new();
    tree(&entries, &written, None, &mut list);
    let index = to_html(name, &[], &read).replacen("</style>", &format!("{CSS}</style>"), 1).replacen(
        "</body>",
        &format!("{list}</body>"),
        1,
    );
    std::fs::write(dest.join("index.html"), index)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::Block;

    #[test]
    fn a_site_links_pages_to_each_other_and_leaves_out_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::open(dir.path().join("ws")).unwrap();
        let a = w.create_page(None, "Alpha & Co").unwrap();
        let b = w.create_page(Some(&a), "Beta").unwrap();
        w.pages.get_mut(&a).unwrap().blocks.push(Block::paragraph("see [[Beta]] and [[Nowhere]]"));
        w.pages.get_mut(&b).unwrap().blocks.push(Block::paragraph("back to [[alpha & co]]"));
        let db = w.create_database(None, "Tasks").unwrap();
        w.add_row(&db, None, "Write <it>").unwrap();
        let out = dir.path().join("site");
        let r = export_site(&w, &out, "My notes").unwrap();
        assert_eq!((r.pages, r.skipped), (4, 0));
        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(index.contains("My notes") && index.contains("Alpha &amp; Co") && index.contains("Beta"));
        let files: Vec<String> =
            std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        let alpha = files.iter().find(|f| f.starts_with("alpha-co-")).unwrap();
        let beta = files.iter().find(|f| f.starts_with("beta-")).unwrap();
        let alpha_html = std::fs::read_to_string(out.join(alpha)).unwrap();
        assert!(alpha_html.contains(&format!("<a class=\"mention\" href=\"{beta}\">Beta</a>")));
        // a link to nothing stays plain text
        assert!(alpha_html.contains("<span class=\"mention\">Nowhere</span>"));
        let beta_html = std::fs::read_to_string(out.join(beta)).unwrap();
        assert!(beta_html.contains(&format!("href=\"{alpha}\"")), "mentions ignore case and entities");
        assert!(beta_html.contains("<nav class=\"crumbs\"><a href=\"index.html\">Home</a> › <a href=\""));
        // the database page lists its rows with escaped titles
        let tasks = files.iter().find(|f| f.starts_with("tasks-")).unwrap();
        let tasks_html = std::fs::read_to_string(out.join(tasks)).unwrap();
        assert!(tasks_html.contains("Write &lt;it&gt;") && !tasks_html.contains("Write <it>"));
    }

    #[test]
    fn a_locked_page_is_left_out_and_its_text_never_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::open(dir.path().join("ws")).unwrap();
        let open_page = w.create_page(None, "Public").unwrap();
        w.pages.get_mut(&open_page).unwrap().blocks.push(Block::paragraph("visible words"));
        let secret = w.create_page(None, "Secret").unwrap();
        w.pages.get_mut(&secret).unwrap().blocks.push(Block::paragraph("hidden words"));
        w.encrypt_page(&secret, "a long enough phrase").unwrap();
        w.lock_vault();
        let out = dir.path().join("site");
        let r = export_site(&w, &out, "N").unwrap();
        assert_eq!((r.pages, r.skipped), (1, 1));
        let all: String = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap_or_default())
            .collect();
        assert!(all.contains("visible words"));
        assert!(!all.contains("hidden words") && !all.contains("Secret"), "neither its text nor its title leaks");
    }

    #[test]
    fn an_image_is_embedded_so_the_folder_stands_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::open(dir.path().join("ws")).unwrap();
        let png: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        let path = w.add_asset_bytes("pic.png", png).unwrap();
        let id = w.create_page(None, "With image").unwrap();
        w.pages.get_mut(&id).unwrap().blocks.push(Block::new(crate::blocks::BlockKind::Image { path }, "cap"));
        let out = dir.path().join("site");
        export_site(&w, &out, "N").unwrap();
        let page = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("with-image-"))
            .unwrap();
        let html = std::fs::read_to_string(page).unwrap();
        assert!(html.contains("data:image/png;base64,"), "the picture travels inside the page");
        assert!(!html.contains("assets/"), "no link back to the workspace");
    }

    #[test]
    fn running_it_twice_gives_the_same_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::open(dir.path().join("ws")).unwrap();
        let a = w.create_page(None, "Same").unwrap();
        w.pages.get_mut(&a).unwrap().blocks.push(Block::paragraph("x"));
        let (o1, o2) = (dir.path().join("s1"), dir.path().join("s2"));
        export_site(&w, &o1, "N").unwrap();
        export_site(&w, &o2, "N").unwrap();
        let list = |d: &Path| {
            let mut v: Vec<(String, String)> = std::fs::read_dir(d)
                .unwrap()
                .map(|e| {
                    let e = e.unwrap();
                    (e.file_name().to_string_lossy().to_string(), std::fs::read_to_string(e.path()).unwrap())
                })
                .collect();
            v.sort();
            v
        };
        assert_eq!(list(&o1), list(&o2));
    }
}
