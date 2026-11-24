//! `notecraft`: command line companion of the desktop app.
//!
//! ```text
//! notecraft version
//! notecraft verify  <workspace>            look for damage left by an import (exit code 2 if any)
//! notecraft reindex <workspace>            rebuild .notecraft/index.db from the files
//! notecraft search  <workspace> <words…>   full-text search
//! notecraft import  <folder> <workspace>   import a folder of Markdown files
//! notecraft export  <workspace> <dest>     export the workspace to plain Markdown
//! notecraft rebody  <workspace> <page> <file.md> [files dir]  replace the body of a page
//! notecraft bench  <workspace> [pages] [rows]  fill an EMPTY workspace with test data and time it
//! notecraft snapshots <workspace>          list the automatic snapshots, newest first
//! notecraft restore <workspace> <id>       put a snapshot's files back
//! ```

use notecraft_core::{import_export, Workspace};
mod bench;
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  notecraft version\n  notecraft verify <workspace>
  notecraft reindex <workspace>\n  notecraft search <workspace> <words…>\n  notecraft import <folder> <workspace>\n  notecraft export <workspace> <dest>\n  notecraft rebody <workspace> <page title or id> <file.md> [files folder]\n  notecraft sample <workspace>\n  notecraft bench <empty workspace> [pages] [rows]\n  notecraft snapshots <workspace>\n  notecraft restore <workspace> <snapshot id>"
    );
    ExitCode::from(2)
}

/// Print what `import_check` finds in the workspace; returns how many problems there are.
fn report_findings(ws: &Workspace) -> usize {
    let found = notecraft_core::import_check::check_workspace(ws);
    for f in &found {
        eprintln!("check: {f}");
    }
    println!("check: {}", notecraft_core::import_check::summary(&found));
    found.len()
}

fn run(args: &[String]) -> Result<(), String> {
    let open = |p: &str| Workspace::open(PathBuf::from(p)).map_err(|e| e.to_string());
    match args.first().map(String::as_str) {
        Some("version") | None => println!("NøteCraft {}", notecraft_core::version()),
        Some("reindex") if args.len() == 2 => {
            let ws = open(&args[1])?;
            println!("indexed {} pages and {} databases", ws.pages.len(), ws.dbs.len());
            for w in &ws.warnings {
                eprintln!("warning: {w}");
            }
        }
        Some("search") if args.len() >= 3 => {
            let ws = open(&args[1])?;
            for r in ws.search(&args[2..].join(" "), 30) {
                let snippet = r.snippet.replace(['\u{1}', '\u{2}'], "").replace('\n', " ");
                println!("{} {}  [{}]  {}", r.icon, r.title, r.path, snippet);
            }
        }
        Some("import") if args.len() == 3 => {
            let mut ws = open(&args[2])?;
            let (source, rep) = notecraft_core::import_any::import_any(&mut ws, &PathBuf::from(&args[1]), None)
                .map_err(|e| e.to_string())?;
            println!("imported {}", notecraft_core::import_any::summary(source, &rep));
            for s in rep.skipped {
                eprintln!("skipped: {s}");
            }
            for n in rep.notes {
                eprintln!("note: {n}");
            }
            report_findings(&ws);
        }
        Some("verify") if args.len() == 2 => {
            let ws = open(&args[1])?;
            if report_findings(&ws) > 0 {
                std::process::exit(2);
            }
        }
        Some("export") if args.len() == 3 => {
            let ws = open(&args[1])?;
            let n = import_export::export_workspace(&ws, &PathBuf::from(&args[2])).map_err(|e| e.to_string())?;
            println!("exported {n} files");
        }
        Some("site") if args.len() == 3 => {
            let ws = open(&args[1])?;
            let name = PathBuf::from(&args[1]).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let r =
                notecraft_core::site::export_site(&ws, &PathBuf::from(&args[2]), &name).map_err(|e| e.to_string())?;
            println!("wrote {} pages ({} locked pages left out)", r.pages, r.skipped);
        }
        Some("rebody") if (4..=5).contains(&args.len()) => {
            let mut ws = open(&args[1])?;
            let row_id =
                ws.dbs.iter().find_map(|(db, d)| d.rows.contains_key(&args[2]).then(|| (db.clone(), args[2].clone())));
            let id = match ws.resolve_title(&args[2]) {
                _ if row_id.is_some() => String::new(),
                Some(notecraft_core::DocId::Page(id)) => id,
                _ if ws.pages.contains_key(&args[2]) => args[2].clone(),
                _ => return Err(format!("no page called {}", args[2])),
            };
            let file = PathBuf::from(&args[3]);
            let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
            let dir = args.get(4).map(PathBuf::from).or_else(|| file.parent().map(PathBuf::from)).unwrap_or_default();
            let rep = match &row_id {
                Some((db, row)) => import_export::replace_row_body(&mut ws, db, row, &text, &dir),
                None => import_export::replace_page_body(&mut ws, &id, &text, &dir),
            }
            .map_err(|e| e.to_string())?;
            println!("body replaced, {} file(s) copied", rep.assets);
            for n in rep.notes {
                eprintln!("note: {n}");
            }
            report_findings(&ws);
        }
        Some("bench") if (2..=4).contains(&args.len()) => {
            let pages = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(10_000);
            let rows = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(50_000);
            bench::run(&PathBuf::from(&args[1]), pages, rows)?;
        }
        Some("snapshots") if args.len() == 2 => {
            let ws = open(&args[1])?;
            let list = ws.list_snapshots();
            if list.is_empty() {
                println!("no snapshots");
            }
            for s in list {
                println!("{}  {}  {}  ({} file(s))", s.id, s.time, s.reason, s.files.len());
                for t in s.items {
                    println!("    {t}");
                }
            }
        }
        Some("restore") if args.len() == 3 => {
            let mut ws = open(&args[1])?;
            let n = ws.restore_snapshot(&args[2]).map_err(|e| e.to_string())?;
            println!("restored {n} file(s)");
        }
        _ => return Err(String::new()),
    }
    Ok(())
}

fn main() -> ExitCode {
    // Piping the output into `head` closes stdout early: stop quietly instead of a panic report.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = info.to_string();
        if text.contains("Broken pipe") {
            std::process::exit(0);
        }
        default_hook(info);
    }));
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => usage(),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
