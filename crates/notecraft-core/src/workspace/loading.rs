//! Free helpers that read pages, rows and databases from disk.

use super::*;

pub(super) fn mentions_of(blocks: &[Block]) -> Vec<String> {
    let mut v = Vec::new();
    for b in blocks {
        if b.kind.has_rich_text() {
            for m in inline::mentions(&b.text) {
                if !v.contains(&m) {
                    v.push(m);
                }
            }
        }
    }
    v
}

pub(super) fn rename_in_blocks(blocks: &mut [Block], old: &str, new: &str) -> bool {
    let mut changed = false;
    for b in blocks.iter_mut() {
        if b.kind.has_rich_text() {
            if let Some(t) = inline::rename_mentions(&b.text, old, new) {
                b.text = t;
                changed = true;
            }
        }
    }
    changed
}

pub(super) fn md_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "md") && !is_hidden(p))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub(super) fn is_hidden(p: &Path) -> bool {
    p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

pub(super) fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Pick the original among files that claim the same id; the others become conflicts.
///
/// Files are grouped by id, and only the ids claimed by several files are ranked (which needs a
/// look at the files on disk): a workspace without conflicts costs one hash lookup per file.
pub(super) fn split_duplicates<T>(
    found: Vec<(PathBuf, T)>,
    id_of: impl Fn(&T) -> String,
    doc_of: impl Fn(&str) -> DocId,
    conflicts: &mut Vec<Conflict>,
    warnings: &mut Vec<String>,
) -> Vec<(PathBuf, T)> {
    let mut groups: BTreeMap<String, Vec<(PathBuf, T)>> = BTreeMap::new();
    for (path, t) in found {
        groups.entry(id_of(&t)).or_default().push((path, t));
    }
    let mut kept: Vec<(PathBuf, T)> = Vec::with_capacity(groups.len());
    for (id, mut group) in groups {
        if group.len() > 1 {
            group.sort_by_cached_key(|(path, _)| sync::original_rank(path, &id));
        }
        let mut it = group.into_iter();
        let Some(first) = it.next() else { continue };
        for (path, _) in it {
            warnings.push(format!("{}: another version of {} (conflict)", path.display(), first.0.display()));
            conflicts.push(Conflict { doc: doc_of(&id), original: first.0.clone(), copy: path });
        }
        kept.push(first);
    }
    kept
}

/// Run `f` over the items on several threads, keeping the order of the results.
/// Reading and parsing thousands of small files is the cost of opening a large workspace.
pub(super) fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(8);
    if items.len() < 256 || threads < 2 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let handles: Vec<_> =
            items.chunks(chunk).map(|part| scope.spawn(|| part.iter().map(&f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

pub(super) fn newer_format_warning(path: &Path, format: u32) -> String {
    format!(
        "{} {}: written by a newer NøteCraft (format {format}, this version reads up to {})",
        crate::NEWER_FORMAT,
        path.display(),
        crate::FORMAT_VERSION
    )
}

/// Record a warning when a file declares a format this version does not know.
pub(super) fn note_newer_format(text: &str, path: &Path, warnings: &mut Vec<String>) {
    if let (Some(yaml), _) = crate::frontmatter::split(text) {
        let format = crate::frontmatter::declared_format(yaml);
        if format > crate::FORMAT_VERSION {
            warnings.push(newer_format_warning(path, format));
        }
    }
}

pub(super) fn load_pages(
    dir: &Path,
    warnings: &mut Vec<String>,
    conflicts: &mut Vec<Conflict>,
) -> BTreeMap<String, Page> {
    let files = md_files(dir);
    let parsed = par_map(&files, |path| {
        let mut notes = Vec::new();
        let page = fs::read_to_string(path).map_err(Error::from).and_then(|t| {
            note_newer_format(&t, path, &mut notes);
            Page::parse(&t, &stem(path))
        });
        (page, notes)
    });
    let mut found = Vec::with_capacity(files.len());
    for (path, (page, notes)) in files.into_iter().zip(parsed) {
        warnings.extend(notes);
        match page {
            Ok(p) => found.push((path, p)),
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
    let kept =
        split_duplicates(found, |p: &Page| p.meta.id.clone(), |id| DocId::Page(id.to_string()), conflicts, warnings);
    kept.into_iter()
        .map(|(path, mut p)| {
            p.path = Some(path);
            (p.meta.id.clone(), p)
        })
        .collect()
}

pub(super) fn load_rows(
    dir: &Path,
    db: &str,
    warnings: &mut Vec<String>,
    conflicts: &mut Vec<Conflict>,
) -> BTreeMap<String, Row> {
    let files = md_files(dir);
    let parsed = par_map(&files, |path| {
        let mut notes = Vec::new();
        let row = fs::read_to_string(path).map_err(Error::from).and_then(|t| {
            note_newer_format(&t, path, &mut notes);
            Row::parse(&t)
        });
        (row, notes)
    });
    let mut found = Vec::with_capacity(files.len());
    for (path, (row, notes)) in files.into_iter().zip(parsed) {
        warnings.extend(notes);
        match row {
            Ok(mut r) => {
                if r.meta.title.is_empty() && r.meta.created.is_empty() {
                    r.meta.created = crate::now_string();
                }
                found.push((path, r));
            }
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
    let db = db.to_string();
    let kept = split_duplicates(
        found,
        |r: &Row| r.meta.id.clone(),
        |id| DocId::Row(db.clone(), id.to_string()),
        conflicts,
        warnings,
    );
    kept.into_iter()
        .map(|(path, mut r)| {
            r.path = Some(path);
            (r.meta.id.clone(), r)
        })
        .collect()
}

pub(super) fn load_databases(
    dir: &Path,
    warnings: &mut Vec<String>,
    conflicts: &mut Vec<Conflict>,
) -> BTreeMap<String, Database> {
    let mut dbs = BTreeMap::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir() && !is_hidden(p)).collect())
        .unwrap_or_default();
    dirs.sort();
    for d in dirs {
        let schema_path = d.join("schema.json");
        let schema: Schema = match fs::read_to_string(&schema_path)
            .map_err(Error::from)
            .and_then(|t| serde_json::from_str(&t).map_err(Error::from))
        {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("{}: {e}", schema_path.display()));
                continue;
            }
        };
        if schema.format > crate::FORMAT_VERSION {
            warnings.push(newer_format_warning(&schema_path, schema.format));
        }
        if dbs.contains_key(&schema.id) {
            warnings.push(format!("{}: same database id as another folder; skipped", d.display()));
            continue;
        }
        // other versions of schema.json made by a sync tool
        for other in sync::schema_files(&d).into_iter().filter(|p| p != &schema_path) {
            conflicts.push(Conflict {
                doc: DocId::Database(schema.id.clone()),
                original: schema_path.clone(),
                copy: other,
            });
        }
        let rows = load_rows(&d.join("rows"), &schema.id, warnings, conflicts);
        let mut tpl_conflicts = Vec::new();
        let templates = load_rows(&d.join("templates"), &schema.id, warnings, &mut tpl_conflicts);
        dbs.insert(schema.id.clone(), Database { schema, rows, templates, dir: d });
    }
    dbs
}
