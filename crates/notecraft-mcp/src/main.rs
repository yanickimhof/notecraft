//! `notecraft-mcp --workspace <folder> [--read-only] [--allow-delete] [--scope <page>]`: an MCP server over stdio.
//!
//! Register it with an MCP client (see the README for Claude Desktop and
//! Claude Code). Messages are newline-delimited JSON-RPC 2.0 on stdin/stdout;
//! logs go to stderr. No network is used.

use std::io::{BufRead, Write};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut folder: Option<PathBuf> = std::env::var_os("NOTECRAFT_WORKSPACE").map(PathBuf::from);
    let mut policy = notecraft_mcp::Policy::default();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--workspace" | "-w" => folder = args.next().map(PathBuf::from),
            "--read-only" => policy.read_only = true,
            "--files-root" => match args.next() {
                Some(p) => policy.files_roots.push(PathBuf::from(p)),
                None => {
                    eprintln!("--files-root needs a folder");
                    std::process::exit(2);
                }
            },
            "--allow-delete" => policy.allow_delete = true,
            "--scope" => match args.next() {
                Some(p) => policy.scope.push(p),
                None => {
                    eprintln!("--scope needs a page title or id");
                    std::process::exit(2);
                }
            },
            "--version" => {
                println!("notecraft-mcp {}", notecraft_core::version());
                return;
            }
            "-h" | "--help" => {
                println!(
                    "notecraft-mcp --workspace <folder> [--read-only] [--allow-delete] [--scope <page>]... [--files-root <folder>]...\n\
                     (or set NOTECRAFT_WORKSPACE)\n\n\
                     default      read and write tools; deletion tools are off\n\
                     --read-only  read tools only\n\
                     --allow-delete  also trash_page, delete_row, delete_property, empty_trash\n\
                     --files-root folder the file tools may read or write (default: home without hidden folders and Library)\n\
                     --scope      limit everything to that page and what is inside it (repeatable)\n\n\
                     Every call is recorded in the audit journal, outside the notes folder."
                );
                return;
            }
            other if !other.starts_with('-') => folder = Some(PathBuf::from(other)),
            other => {
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
    }
    let Some(folder) = folder else {
        eprintln!("notecraft-mcp: give --workspace <folder> or set NOTECRAFT_WORKSPACE");
        std::process::exit(2);
    };
    // "~/Notes" as written in a client config
    let folder = match folder.strip_prefix("~") {
        Ok(rest) => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(rest)).unwrap_or(folder.clone()),
        Err(_) => folder,
    };
    let mut server = match notecraft_mcp::Server::open(&folder, policy.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("notecraft-mcp: cannot open {}: {e}", folder.display());
            std::process::exit(1);
        }
    };
    eprintln!("notecraft-mcp: serving {} ({})", folder.display(), policy.describe());
    if let Some(a) = &server.audit {
        eprintln!("notecraft-mcp: audit journal {}", a.path().display());
    }
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_line(&line) {
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}
