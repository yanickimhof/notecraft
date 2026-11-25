//! Publishing pages on the web: what is sent, how it is prepared and where the record is kept.
//!
//! The record of what is published lives in the app data folder, never inside the notes, so a
//! shared or exported workspace carries no trace of it and no secret address.

use notecraft_ai::publish::{Cloudflare, PubError, Setup};
use notecraft_core::publish_html::{render_page, Cover, Hooks, NavLink, Options};
use notecraft_core::{DocId, Workspace};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const MAX_PAGES: usize = 200;
const MAX_IMAGE_WIDTH: u32 = 1600;
/// Replaced by the real address when the page is sent (link previews need an absolute address).
const HOST_MARK: &str = "https://__NC_HOST__";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetupRec {
    pub account: String,
    pub namespace: String,
    pub host: String,
}

impl SetupRec {
    fn to_setup(&self) -> Setup {
        Setup { account: self.account.clone(), namespace: self.namespace.clone(), host: self.host.clone() }
    }
    fn from_setup(s: &Setup) -> Self {
        SetupRec { account: s.account.clone(), namespace: s.namespace.clone(), host: s.host.clone() }
    }
}

/// One published page or section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub secret: String,
    pub with_children: bool,
    pub url: String,
    /// (page id, `updated` stamp of the page when it was sent)
    pub pages: Vec<(String, String)>,
    pub at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub setup: Option<SetupRec>,
    /// workspace folder -> root page id -> entry
    #[serde(default)]
    pub sites: BTreeMap<String, BTreeMap<String, Entry>>,
}

impl Store {
    fn path() -> PathBuf {
        let base = notecraft_core::sync::index_base_dir();
        base.parent().map(|p| p.to_path_buf()).unwrap_or(base).join("publications.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    pub fn get(&self, ws_root: &str, id: &str) -> Option<&Entry> {
        self.sites.get(ws_root).and_then(|m| m.get(id))
    }

    pub fn set(&mut self, ws_root: &str, id: &str, e: Entry) {
        self.sites.entry(ws_root.to_string()).or_default().insert(id.to_string(), e);
    }

    pub fn remove(&mut self, ws_root: &str, id: &str) {
        if let Some(m) = self.sites.get_mut(ws_root) {
            m.remove(id);
        }
    }
}

/// True when a page of the publication changed (or went away) since it was sent.
pub fn is_stale(ws: &Workspace, e: &Entry) -> bool {
    e.pages.iter().any(|(id, updated)| ws.pages.get(id).map(|p| &p.meta.updated != updated).unwrap_or(true))
}

/// Everything a publication is made of, ready to send or to preview.
pub struct Build {
    /// (storage key, bytes), sub pages and images first, the first page last.
    pub files: Vec<(String, Vec<u8>)>,
    pub pages: Vec<(String, String)>,
    /// Titles of pages left out (encrypted).
    pub skipped: Vec<String>,
}

fn slugify(title: &str, id: &str) -> String {
    let mut s = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s: String = s.trim_matches('-').chars().take(40).collect();
    let s = if s.is_empty() { "page".to_string() } else { s };
    let tail: String = id.chars().filter(char::is_ascii_alphanumeric).take(4).collect();
    format!("{}-{}", s.trim_matches('-'), tail)
}

fn fnv(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Shrink an image for the web: at most 1600 px wide, JPEG for photos and PNG when it has
/// transparency. GIF and SVG are kept as they are. Returns the bytes and the file extension.
pub fn prepare_image(path: &str, bytes: Vec<u8>) -> Option<(Vec<u8>, &'static str)> {
    let ext = path.rsplit('.').next().map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "gif" => return Some((bytes, "gif")),
        "svg" => return Some((bytes, "svg")),
        _ => {}
    }
    let img = image::load_from_memory(&bytes).ok()?;
    let keep_ext: &'static str = match ext.as_str() {
        "png" => "png",
        "webp" => "webp",
        _ => "jpg",
    };
    if img.width() <= MAX_IMAGE_WIDTH && bytes.len() < 700_000 {
        return Some((bytes, keep_ext));
    }
    let img = if img.width() > MAX_IMAGE_WIDTH {
        let h = (u64::from(img.height()) * u64::from(MAX_IMAGE_WIDTH) / u64::from(img.width())).max(1) as u32;
        img.resize_exact(MAX_IMAGE_WIDTH, h, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Vec::new();
    if img.color().has_alpha() {
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
        Some((out, "png"))
    } else {
        let rgb = img.to_rgb8();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82);
        rgb.write_with_encoder(enc).ok()?;
        Some((out, "jpg"))
    }
}

struct Node {
    id: String,
    parent: Option<String>,
    slug: String,
}

fn collect(ws: &Workspace, id: &str, parent: Option<&str>, depth: usize, out: &mut Vec<Node>, skipped: &mut Vec<String>) {
    if out.len() >= MAX_PAGES || depth > 8 {
        return;
    }
    let Some(page) = ws.pages.get(id) else { return };
    if ws.is_encrypted(id) {
        skipped.push(page.meta.title.clone());
        return;
    }
    out.push(Node {
        id: id.to_string(),
        parent: parent.map(String::from),
        slug: if parent.is_none() { String::new() } else { slugify(&page.meta.title, id) },
    });
    for item in ws.children(Some(id)) {
        if let DocId::Page(cid) = item.doc {
            if !ws.pages.get(&cid).map(|p| p.meta.archived).unwrap_or(true) {
                collect(ws, &cid, Some(id), depth + 1, out, skipped);
            }
        }
    }
}

/// Build the files of a publication. With `preview`, images are embedded in the page and links
/// to other pages are left as text, so the result opens from a plain file.
pub fn build(
    ws: &Workspace,
    root: &str,
    with_children: bool,
    secret: &str,
    lang: &str,
    preview: bool,
) -> Result<Build, String> {
    let page = ws.pages.get(root).ok_or_else(|| "page not found".to_string())?;
    if ws.is_encrypted(root) {
        return Err("encrypted".into());
    }
    let mut nodes = Vec::new();
    let mut skipped = Vec::new();
    if with_children {
        collect(ws, root, None, 0, &mut nodes, &mut skipped);
    } else {
        nodes.push(Node { id: root.to_string(), parent: None, slug: String::new() });
    }
    let _ = page;
    let url_of = |n: &Node| if n.slug.is_empty() { format!("/{secret}/") } else { format!("/{secret}/{}/", n.slug) };
    let mut titles: BTreeMap<String, String> = BTreeMap::new();
    for n in &nodes {
        if let Some(p) = ws.pages.get(&n.id) {
            titles.entry(p.meta.title.clone()).or_insert_with(|| url_of(n));
        }
    }
    let nav = |n: &Node| {
        let p = ws.pages.get(&n.id);
        NavLink {
            title: p.map(|p| p.meta.title.clone()).unwrap_or_default(),
            icon: p.map(|p| p.meta.icon.clone()).unwrap_or_default(),
            url: url_of(n),
        }
    };

    let mut assets: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut memo: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut pages = Vec::new();
    let mut first: Option<(String, Vec<u8>)> = None;

    for n in &nodes {
        let Some(p) = ws.pages.get(&n.id) else { continue };
        let mut image = |path: &str| -> Option<String> {
            if let Some(v) = memo.get(path) {
                return v.clone();
            }
            let res = std::fs::read(ws.asset_path(path)).ok().and_then(|b| prepare_image(path, b)).map(|(bytes, ext)| {
                if preview {
                    let mime = match ext {
                        "png" => "image/png",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        "svg" => "image/svg+xml",
                        _ => "image/jpeg",
                    };
                    format!("data:{mime};base64,{}", notecraft_core::render::base64(&bytes))
                } else {
                    let name = format!("{}.{ext}", fnv(&bytes));
                    assets.entry(name.clone()).or_insert(bytes);
                    format!("/{secret}/a/{name}")
                }
            });
            memo.insert(path.to_string(), res.clone());
            res
        };
        let cover = match p.meta.cover.as_str() {
            "" => Cover::None,
            c => match c.strip_prefix("gradient:").and_then(|n| n.parse::<usize>().ok()) {
                Some(n) => Cover::Gradient(n),
                None => image(c).map(Cover::Image).unwrap_or(Cover::None),
            },
        };
        let og_image = match (&cover, preview) {
            (Cover::Image(u), false) => Some(format!("{HOST_MARK}{u}")),
            _ => None,
        };
        let mut crumbs = Vec::new();
        let mut up = n.parent.clone();
        while let Some(pid) = up {
            let Some(pn) = nodes.iter().find(|x| x.id == pid) else { break };
            crumbs.push(nav(pn));
            up = pn.parent.clone();
        }
        crumbs.reverse();
        let children: Vec<NavLink> =
            nodes.iter().filter(|c| c.parent.as_deref() == Some(n.id.as_str())).map(&nav).collect();
        let opts = Options {
            title: &p.meta.title,
            icon: &p.meta.icon,
            cover,
            font: &p.meta.font,
            small: p.meta.small_text,
            wide: p.meta.full_width,
            lang,
            crumbs: if preview { vec![] } else { crumbs },
            children: if preview { vec![] } else { children },
            og_image,
        };
        let link = |t: &str| if preview { None } else { titles.get(t).cloned() };
        let db = |_: &str| None;
        let diagram = |kind: &str, src: &str| -> Option<String> {
            if kind == "equation" {
                crate::svg_export::equation(src)
            } else {
                crate::svg_export::diagram(src)
            }
        };
        let mut hooks = Hooks { image: &mut image, link: &link, database: &db, diagram: &diagram };
        let html = render_page(&opts, &p.blocks, &mut hooks).into_bytes();
        pages.push((n.id.clone(), p.meta.updated.clone()));
        let key = if n.slug.is_empty() { format!("{secret}/index.html") } else { format!("{secret}/{}/index.html", n.slug) };
        if n.slug.is_empty() {
            first = Some((key, html));
        } else {
            files.push((key, html));
        }
    }
    for (name, bytes) in assets {
        files.insert(0, (format!("{secret}/a/{name}"), bytes));
    }
    if let Some(f) = first {
        files.push(f);
    }
    Ok(Build { files, pages, skipped })
}

// ----------------------------------------------------------------------------- the worker side

/// What a background job reports to the window.
pub enum Msg {
    Published { root: String, entry: Entry, setup: SetupRec },
    Unpublished { root: String },
    NeedSubdomain { account: String },
    SubdomainSet,
    Failed(String),
}

fn get_setup(cf: &Cloudflare, cached: &Option<SetupRec>) -> Result<(Setup, bool), PubError> {
    match cached {
        Some(c) => Ok((c.to_setup(), true)),
        None => cf.setup().map(|s| (s, false)),
    }
}

fn fail(e: PubError, cf: &Cloudflare) -> Msg {
    match e {
        PubError::NeedSubdomain => match cf.account_id() {
            Ok(account) => Msg::NeedSubdomain { account },
            Err(e) => Msg::Failed(e.to_string()),
        },
        e => Msg::Failed(e.to_string()),
    }
}

fn content_ok(files: &[(String, Vec<u8>)], cf: &Cloudflare, s: &Setup) -> Result<(), PubError> {
    // assets and sub pages in parallel, the first page last so a half sent page is never shown
    let (last, rest) = match files.split_last() {
        Some((l, r)) => (l, r),
        None => return Ok(()),
    };
    let n = 4usize.min(rest.len().max(1));
    let chunk = rest.len().div_ceil(n).max(1);
    let mut first_err: Option<PubError> = None;
    std::thread::scope(|sc| {
        let handles: Vec<_> = rest
            .chunks(chunk)
            .map(|part| {
                sc.spawn(move || -> Result<(), PubError> {
                    for (k, b) in part {
                        cf.put(s, k, b)?;
                    }
                    Ok(())
                })
            })
            .collect();
        for h in handles {
            if let Ok(Err(e)) = h.join() {
                first_err.get_or_insert(e);
            }
        }
    });
    if let Some(e) = first_err {
        return Err(e);
    }
    cf.put(s, &last.0, &last.1)
}

/// Send a publication (new or an update of the same address).
pub fn run_publish(token: &str, cached: Option<SetupRec>, root: &str, with_children: bool, secret: &str, mut b: Build) -> Msg {
    let cf = Cloudflare::new(token, notecraft_ai::Timeouts::default());
    let (mut setup, from_cache) = match get_setup(&cf, &cached) {
        Ok(v) => v,
        Err(e) => return fail(e, &cf),
    };
    let fill = |files: &mut Vec<(String, Vec<u8>)>, host: &str| {
        for (k, bytes) in files.iter_mut() {
            if k.ends_with(".html") {
                let text = String::from_utf8_lossy(bytes).replace(HOST_MARK, host);
                *bytes = text.into_bytes();
            }
        }
    };
    fill(&mut b.files, &setup.host);
    let mut res = content_ok(&b.files, &cf, &setup);
    if res.is_err() && from_cache {
        // the stored setup may be out of date (namespace removed): set up again once
        match cf.setup() {
            Ok(s) => {
                fill(&mut b.files, &s.host);
                setup = s;
                res = content_ok(&b.files, &cf, &setup);
            }
            Err(e) => return fail(e, &cf),
        }
    }
    if let Err(e) = res {
        return fail(e, &cf);
    }
    // an update: remove what the new version no longer has
    if let Ok(old) = cf.keys(&setup, &format!("{secret}/")) {
        let keep: std::collections::HashSet<&str> = b.files.iter().map(|(k, _)| k.as_str()).collect();
        for k in old.iter().filter(|k| !keep.contains(k.as_str())) {
            let _ = cf.remove(&setup, k);
        }
    }
    let entry = Entry {
        secret: secret.to_string(),
        with_children,
        url: format!("{}/{secret}/", setup.host),
        pages: b.pages,
        at: notecraft_core::now_string(),
    };
    Msg::Published { root: root.to_string(), entry, setup: SetupRec::from_setup(&setup) }
}

/// Take a publication down: every key under its secret goes.
pub fn run_unpublish(token: &str, cached: Option<SetupRec>, root: &str, secret: &str) -> Msg {
    let cf = Cloudflare::new(token, notecraft_ai::Timeouts::default());
    let (setup, _) = match get_setup(&cf, &cached) {
        Ok(v) => v,
        Err(e) => return fail(e, &cf),
    };
    for _ in 0..5 {
        let keys = match cf.keys(&setup, &format!("{secret}/")) {
            Ok(k) => k,
            Err(e) => return Msg::Failed(e.to_string()),
        };
        if keys.is_empty() {
            break;
        }
        for k in keys {
            if let Err(e) = cf.remove(&setup, &k) {
                return Msg::Failed(e.to_string());
            }
        }
    }
    Msg::Unpublished { root: root.to_string() }
}

pub fn run_set_subdomain(token: &str, account: &str, name: &str) -> Msg {
    let cf = Cloudflare::new(token, notecraft_ai::Timeouts::default());
    match cf.set_subdomain(account, name) {
        Ok(()) => Msg::SubdomainSet,
        Err(e) => Msg::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_plain_and_unique_by_id() {
        assert_eq!(slugify("Été à Paris !", "ab12cd34"), "t-paris-ab12");
        assert_eq!(slugify("???", "x"), "page-x");
    }

    #[test]
    fn small_images_are_kept_and_big_ones_shrunk() {
        let mut img = image::RgbImage::new(2400, 100);
        for p in img.pixels_mut() {
            *p = image::Rgb([200, 30, 30]);
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let (out, ext) = prepare_image("a.png", png).unwrap();
        assert_eq!(ext, "jpg");
        let back = image::load_from_memory(&out).unwrap();
        assert_eq!(back.width(), 1600);
        let tiny = image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10));
        let mut small = Vec::new();
        tiny.write_to(&mut std::io::Cursor::new(&mut small), image::ImageFormat::Png).unwrap();
        let (o2, e2) = prepare_image("b.png", small.clone()).unwrap();
        assert_eq!((o2, e2), (small, "png"));
        assert!(prepare_image("c.png", b"not an image".to_vec()).is_none());
    }

    /// Real round trip against Cloudflare with a demo workspace (needs the token in the Keychain):
    /// `cargo test -p notecraft-app publish::tests::live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live() {
        use notecraft_core::blocks::{Block, BlockKind};
        let dir = tempfile::tempdir().unwrap();
        let idx = dir.path().join("idx");
        let mut ws = Workspace::open_at(dir.path().join("w"), idx.join("i.db")).unwrap();
        let root = ws.create_page(None, "Démo publication").unwrap();
        let child = ws.create_page(Some(&root), "Sous-page démo").unwrap();
        {
            let b = ws.blocks_mut(&DocId::Page(root.clone())).unwrap();
            b.clear();
            b.push(Block::new(BlockKind::Heading(1), "Titre de section"));
            b.push(Block::new(BlockKind::Paragraph, "Un **texte** de démonstration avec [[Sous-page démo]]."));
            b.push(Block::new(BlockKind::Callout { icon: "💡".into() }, "Une note"));
        }
        {
            let b = ws.blocks_mut(&DocId::Page(child.clone())).unwrap();
            b.clear();
            b.push(Block::new(BlockKind::Paragraph, "Contenu de la sous-page."));
        }
        let token = notecraft_ai::keychain::load_item(notecraft_ai::keychain::PUBLISH_TOKEN_ACCOUNT).expect("token");
        let secret = notecraft_ai::publish::new_secret();
        let built = build(&ws, &root, true, &secret, "fr", false).unwrap();
        assert_eq!(built.pages.len(), 2);
        let (url, setup) = match run_publish(&token, Store::load().setup, &root, true, &secret, built) {
            Msg::Published { entry, setup, .. } => (entry.url, setup),
            Msg::Failed(e) => panic!("{e}"),
            _ => panic!("unexpected"),
        };
        let code = |u: &str| {
            let o = std::process::Command::new("curl").args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "-m", "10", u]).output().unwrap();
            String::from_utf8_lossy(&o.stdout).to_string()
        };
        println!("URL {url}");
        std::thread::sleep(std::time::Duration::from_secs(3));
        println!("published: {}", code(&url));
        let body = std::process::Command::new("curl").args(["-s", "-m", "10", &url]).output().unwrap();
        let html = String::from_utf8_lossy(&body.stdout);
        assert!(html.contains("Titre de section") && html.contains("Sous-page démo"));
        let t0 = std::time::Instant::now();
        match run_unpublish(&token, Some(setup), &root, &secret) {
            Msg::Unpublished { .. } => {}
            Msg::Failed(e) => panic!("{e}"),
            _ => panic!("unexpected"),
        }
        loop {
            let c = code(&url);
            println!("{:>3}s after unpublish: {c}", t0.elapsed().as_secs());
            if c == "404" || t0.elapsed().as_secs() > 120 {
                assert_eq!(c, "404");
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }
}
