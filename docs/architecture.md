# Architecture

NøteCraft is a Cargo workspace. The core has no UI dependency, so the app, the CLI and the MCP server all sit on the same `Workspace` type.

## Crates

| Crate | Role |
|---|---|
| `notecraft-core` | Storage and logic: blocks and Markdown, inline formatting, pages, databases and their query engine, workspace load and save, trash, search index, import and export, sync helpers, file watching, conflict handling |
| `notecraft-app` | Desktop app (eframe and egui): sidebar, page view, block editor, database views, quick finder, AI panel, settings, first launch assistant (`app/onboarding.rs`) and the built in demo window (`demo.rs`) |
| `notecraft-cli` | `notecraft` command: `reindex`, `search`, `import`, `export`, `bench`, `snapshots`, `restore`, `version` |
| `notecraft-mcp` | MCP server over stdio, optional `--read-only`, `--allow-delete`, `--scope` |
| `notecraft-ai` | AI engines (Ollama on the Mac, Claude, OpenAI and Grok APIs), API keys in the macOS Keychain, prompts, review markers that protect tables and code, streamed local answers |
| `notecraft-meeting` | Meeting notes: audio capture, chunking, Whisper transcription, model downloads, microphone permission |

Dependencies point one way: `app`, `cli` and `mcp` depend on `core`; `app` also depends on `ai` and `meeting`. The core never depends on them. Large modules are folders (`app/`, `editor/`, `dbview/`, `workspace/`) whose helpers and tests sit in their own files.

## Data layout

```text
<workspace>/
  pages/<slug>-<id>.md          one Markdown file per page
  databases/<slug>-<id>/        one folder per database
    schema.json                 properties and views
    rows/<slug>-<id>.md         one file per row
    templates/<id>.md           row templates
  assets/                       attachments, relative paths
```

The search index is a rebuildable cache stored outside the workspace, in `~/Library/Application Support/NoteCraft/indexes/<hash of the workspace path>/index.db`, so that a synced folder never carries it. Every write goes to a temporary file in the same folder, is flushed, then renamed over the target.

## File format version

Every file the app writes starts with `format: <n>` (front matter of pages, rows and templates) or has `"format": <n>` as the first key of `schema.json`. The current number is `FORMAT_VERSION` in `notecraft-core`. A file without it was written before versioning and counts as format 1.

- A workspace file with a **newer** format than the app knows is still read, but the whole workspace becomes read-only until the app is updated (one warning per file, error `newer format: ...`). An old app never rewrites what it does not fully understand.
- Changing the layout of the files in a way an older version could misread means raising `FORMAT_VERSION` and adding a migration that runs on open, after an automatic snapshot (see `docs/mcp.md`, Snapshots).

## Principles

1. Plain files are the source of truth; the index can be deleted at any time.
2. Sync is left to the folder provider (kDrive, Dropbox, iCloud Drive). The app watches files, merges outside edits with unsaved typing and keeps a "conflict copy" when both sides changed.
3. The app never runs git and never writes a `.git` folder.
4. Network access is limited to what the user enables: a local Ollama address, the Claude, OpenAI or Grok API with the user's own key, and the one-time download of a speech model on request. The only other connection is an update check: once per launch, one anonymous request to GitHub's public release list, which can be switched off in Settings, About. There is no telemetry.

## Build and release

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/bundle-macos.sh        # target/NoteCraft.noindex/NoteCraft.app with the MCP server and the CLI inside
```

Continuous integration runs the same checks (`.github/workflows/ci.yml`).

## Performance budget

Measured with `notecraft bench <empty folder> 5000 20000` (5 000 pages and 20 000 database rows) on 2026-11-23, release build, Apple silicon.

| Operation | Budget | Measured |
|---|---|---|
| Open (index up to date) | 1 s | 0.37 s |
| Memory after open | 200 MB | 150 MB |
| Quick search | 50 ms | 15 ms |
| Sidebar tree (5 001 items) | 16 ms | 1.7 ms |
| Query 20 000 rows | 200 ms | 0.8 ms |
| Save one page | 30 ms | 9.7 ms |
| Scan for outside changes | 200 ms | 60 ms |

Memory grows by about 6 MB per full reload or fresh open at this size (linear, cause not established, shown by the bench as information, not as a budget).

The search index remembers a fingerprint of the files when the workspace closes cleanly and forgets it while open, so a crash always leads to a rebuild.

## Logs and crashes

`notecraft_core::log` appends warnings, errors and panics to a local file next to the search indexes (`NOTECRAFT_LOG_DIR` overrides the folder). The app reports every error through one function (`notify_error`), which shows a message and writes the log line. Nothing is uploaded.

## Backups and version history

`notecraft_core::backup` copies only the files that changed since the last run into an automatic snapshot (`reason == "auto"`) stored with the other snapshots outside the notes folder. A background thread in the app starts it on the configured interval. The same snapshots feed the per-page version history (`versions_of`, `version_text`, `Workspace::restore_snapshot_file`). Automatic snapshots are thinned like Time Machine (`prune_auto`: hourly for a day, daily for a month, weekly for a year, yearly after; the files only a removed snapshot held are handed to the next one, so no page vanishes). Manual snapshots and those taken before a destructive action are never thinned, but the workspace keeps at most `snapshot::KEEP` (200), oldest first. Files over 5 MB are not copied. The history window (`backups::history_ui`) gives both lists a fixed height with their own scroll; a layout test (`layout_tests`) guards against the window growing every frame, which an `egui` separator inside a modal does.

## Encrypted pages

A page with `encrypted: true` in its front matter stores its body as an armored block
(`-----BEGIN NOTECRAFT ENCRYPTED PAGE-----`): salt, nonce and ChaCha20-Poly1305 ciphertext,
with the key derived by Argon2id (`vault.rs`). One passphrase per workspace is kept in memory only
for the session; each page has its own salt and a fresh nonce on every save. A locked page keeps
its stored ciphertext untouched when saved. Encrypted bodies are never indexed, two encrypted
versions of one page are never merged (the conflict copy is kept), and encrypting a page erases its
earlier plain copies from the automatic backups. There is no recovery mechanism.
