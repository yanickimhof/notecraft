# NøteCraft technical guide

For developers and anyone who wants to know how it works. The everyday description is in the [README](../README.md).

## Build and run

Requirements: Rust (stable) and the Xcode command line tools on macOS.

```sh
cargo build --release            # core, CLI, app, MCP server, AI client
cargo test --workspace           # unit and integration tests
./target/release/notecraft-app                 # last workspace, or a welcome screen
./target/release/notecraft-app ~/Notes         # open (or create) a workspace folder
scripts/bundle-macos.sh                          # NoteCraft.app in target/NoteCraft.noindex
scripts/install-macos.sh [--dmg]                 # install in /Applications, optionally build the DMG
scripts/screenshots.sh                           # regenerate docs/screenshots (needs a demo workspace)
```

App options: `--workspace <folder>`, `--dark`, `--open <title>`, `--view table|board|list|gallery|calendar`, `--panel ai|settings|tabs|remind|demo:<page>|present:<slide>`, `--screenshot <file.png>` (saves the window as PNG), `--export-icns <file>`.

Checks run by CI (`.github/workflows/ci.yml`): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, the bundle script, `cargo audit` and `cargo deny`.

## Releases

1. Update `CHANGELOG.md` and the version in `Cargo.toml`.
2. `scripts/install-macos.sh --dmg` writes `target/release/NoteCraft.dmg` (not notarised; signed ad hoc).
3. Tag `vX.Y.Z`, push the tag, and attach the DMG and its `.sha256` file to a GitHub release (`gh release create`). To republish a release for the same version, delete it with `gh release delete --cleanup-tag` and create it again on `main`; never force-push.

Installing an update is manual (DMG or `scripts/install-macos.sh`). Notes are plain folders, so an update never touches them. `scripts/bundle-macos.sh` signs the app with a self-signed `NoteCraft Local` code-signing certificate when one is in the login keychain, and falls back to ad hoc otherwise. A stable identity keeps the Keychain and microphone permissions across rebuilds; an ad hoc build changes identity on every install, so macOS asks again.

## Vendored winit

`vendor/winit` is winit 0.30.13 with one change on macOS (see `vendor/winit/NOTECRAFT-PATCH.md`): the emoji and symbol picker (Control Command Space) and dictation send text outside any key press, which upstream drops; the patch passes it on as an IME commit so egui inserts it. The root `Cargo.toml` points to it with `[patch.crates-io]`. Remove both once a winit release fixes it.

## Crates

| Crate | Role |
|---|---|
| `notecraft-core` | Storage and logic, no UI: blocks and Markdown, inline formatting, pages, databases and queries, workspace, search index, import and export (HTML, Word, PDF), encryption, backups, sync helpers, file watching, conflicts |
| `notecraft-app` | The desktop app (eframe and egui) |
| `notecraft-cli` | The `notecraft` command: `reindex`, `search`, `import`, `export`, `bench`, `snapshots`, `restore` |
| `notecraft-mcp` | MCP server over stdio |
| `notecraft-ai` | Ollama, Claude, OpenAI and Grok clients, model lists, prompts, Keychain access |
| `notecraft-meeting` | Meeting notes: audio capture, chunking, Whisper transcription, model downloads, microphone permission |

`app`, `cli` and `mcp` depend on `core`; `app` also uses `ai` and `meeting`. The largest modules are folders (`app/`, `editor/`, `dbview/`, `workspace/`) with helpers and tests in their own files. The core never depends on them. More in [architecture.md](architecture.md).

## Data layout

```text
<workspace>/                    only notes live here
  pages/<slug>-<id>.md          one Markdown file per page
  databases/<slug>-<id>/        one folder per database
    schema.json                 properties and views
    rows/<slug>-<id>.md         one Markdown file per row
    templates/<id>.md           row templates
  assets/                       images, PDFs, any attachment (relative paths)

~/Library/Application Support/NoteCraft/
  indexes/<hash of the workspace path>/index.db     SQLite + FTS5 search index (a rebuildable cache)
  indexes/<hash>/snapshots/                         automatic backups and restore points
  audit/<hash>.jsonl                                journal of assistant (MCP) calls
  logs/                                             local log file
  whisper/                                          speech models downloaded on request
  parakeet/                                         Parakeet engine files (ONNX), downloaded on request
```

Page front matter: `format, id, title, icon, cover, created, updated, parent, order, favorite, trashed, locked, encrypted`. Database rows add `properties:` keyed by property name. A block colour is a trailing `<!--c:NAME-->`: a text colour (`red`), a background (`bg-blue`) or both (`red bg-blue`). Bodies are readable Markdown: headings, lists, `- [ ]` to-dos, `<details>` toggles, `>` quotes, `> [!NOTE]` callouts, fenced code, `---`, images, files, `[[Page title]]` mentions, and inline `**bold**`, `*italic*`, `<u>underline</u>`, `~~strike~~` and `` `code` ``.

The index is never the source of truth: delete it at any time and it is rebuilt when the workspace opens (or with Settings, General, Rebuild). Set `NOTECRAFT_INDEX_DIR` to move it.

A file written by a newer version than the app knows makes the workspace read-only until the app is updated.

## Sync: no sync engine

NøteCraft never syncs by itself. Keep the workspace in a synced folder (iCloud Drive, kDrive, Dropbox, OneDrive) or a local one.

- **Live changes**: a file watcher notices files changed by another Mac, the MCP server, git or a text editor, and the open window updates.
- **Merging**: if the page you are editing changed elsewhere, both edits are merged block by block. If both sides changed the same block, your version is saved as `… (conflicted copy <date>).md` and a notice appears.
- **Conflict copies** made by sync tools are recognised and never indexed as a second page. The page offers to keep one version, the other, or both.
- **Safe writes**: every write goes to a temporary file in the same folder, is flushed, then renamed. If the file changed underneath, the write becomes a conflict instead of an overwrite.
- Settings shows which sync service the folder seems to be in, and warns when a synced folder is also a git repository.

## Private pages

A page with `encrypted: true` stores its body as an armored ChaCha20-Poly1305 block, with the key derived from one workspace passphrase by Argon2id. Titles stay readable, bodies are never indexed, earlier plain copies are erased from the backups when a page is encrypted, and there is no recovery if the passphrase is lost. Details in [architecture.md](architecture.md#encrypted-pages).

## Assistants (MCP)

`notecraft-mcp --workspace <folder> [--read-only] [--allow-delete] [--scope <page>]` speaks the Model Context Protocol over stdio and edits the same files as the app, so changes appear live.

- Block edits (update, move, delete) accept an `expect` argument: the current text of the block as last read. If the block changed since, the call is refused and the assistant must read again.
- Permissions are chosen at start: read only, read and write (default), deletion tools only with `--allow-delete`, and `--scope` to limit an assistant to one page tree.
- Locked pages stay read-only for assistants, and every call is written to an audit journal outside the notes folder.
- Settings, MCP server shows the choices and can add the server to Claude Desktop (it edits `claude_desktop_config.json`, keeping a backup) or to Claude Code (`claude mcp add --scope user`).

Tools cover pages (list, get, search, create, update, append, rename, move, duplicate, trash, restore, lock), databases (create, properties, rows, queries, templates) and files (attachments, Import from Evernote, Obsidian, Bear, Joplin, Word, HTML and CSV; Markdown export). The full reference is in [mcp.md](mcp.md).

## AI help

Engines, chosen in Settings (icons: this Mac, Claude, OpenAI, Grok), each with a model menu fed by the provider's own model list, grouped by family and showing only the newest versions (the model in use is always kept):

- **Ollama** (default) at `http://localhost:11434`, using models already installed. NøteCraft never downloads one. Everything stays on the Mac.
- **Claude API** with your own key, stored in the macOS Keychain (service NøteCraft) or taken from `ANTHROPIC_API_KEY`. The panel and the top bar show that text leaves the Mac.
- **OpenAI API** (ChatGPT models) with your own key, stored the same way (account `openai-api-key`) or taken from `OPENAI_API_KEY`; chat completions, model list read from the account.

- **Grok (xAI)** with your own key, stored the same way (account `grok-api-key`) or taken from `XAI_API_KEY`; xAI's OpenAI compatible API at `https://api.x.ai`, default model `grok-4`.

Review actions (Summarize, Improve, Proofread, Translate) are applied to the page as soon as the answer arrives, as one undoable edit (⌘Z); Ask stays in the panel as a draft, and so does any answer whose figures or dates differ from the original. Before sending, `ai_review::prepare` replaces tables, databases, images, code and transcripts with `@@KEEPn@@` markers and `restore` puts them back, so they never reach the engine and cannot be altered.

Local requests (Ollama) are streamed (`stream: true`, one JSON line per chunk). The panel shows seconds, part and characters received, and the timeout applies to silence between chunks rather than to the whole answer. They bypass any system or corporate proxy (`local_agent`), which would otherwise intercept `localhost`. Models that echo the request (`Page title:`, `<page>` tags) are cleaned by `clean_answer`. Cloud engines still answer in one piece.

## Update check

`notecraft-ai::update::latest_release` reads `releases/latest` of the project on `api.github.com` (purpose `UpdateCheck`, refused by the network guard for any other host and in lockdown). It runs once per launch on a thread, after the first launch assistant, when `check_updates` is on (default); the answer is accepted only if its page address is on the project's GitHub page. `is_newer` compares version numbers. A newer version shows as a faint line above the trash in the sidebar and in Settings, About. Failures are silent.

## Meeting notes

- Crate `notecraft-meeting`: `audio` (cpal capture on the UI thread, linear resampler to 16 kHz mono; sources: microphone, system audio through a Core Audio tap on the default output device, or both mixed), `chunker` (cuts at pauses, 6 to 25 s), `transcribe` (whisper-rs with Metal and, for English, NVIDIA Parakeet through ONNX Runtime with `parakeet-rs`; worker thread, `choose_engine`, vocabulary hint, noise filter; `parakeet-rs` 0.3 needs `ort` 2.0.0-rc.13, kept by Cargo.lock), `models` (ggml files and Parakeet ONNX files downloaded from Hugging Face into the app support folder), `permission` (AVCaptureDevice authorization on macOS).
- The block is stored as `<!--meeting:Title-->`. The transcript is a `<details><summary>Transcript` toggle with one indented paragraph per chunk; the notes are ordinary blocks between the two.
- `notecraft-ai::build_notes_prompt` turns the transcript and a template into the prompt; only text leaves the machine, and only with a cloud engine (Claude, OpenAI or Grok).
- Two families of templates, both ordinary pages under the top level **Templates** page (`app/templates.rs`): page templates (the sub-pages pasted by `/template`, offered by the finder) and **transcription templates** (the sub-pages of the group page "Transcription templates", whose text is the instruction handed to `build_notes_prompt`; the selection is stored as `page:<id>`). The group is found by its title, or, for a workspace made before 0.6.0, by the built-in templates it holds, and renamed when the workspace opens. The first run writes one example of each kind, only into a section it creates; the choice shown in the block is always one of the existing pages, and the built-in general summary stands in only when none is left.
- System audio uses cpal's loopback (macOS 14.6+), pinned to a main-branch revision in `crates/notecraft-meeting/Cargo.toml` because released versions deliver silence; `NSAudioCaptureUsageDescription` in the bundle script is what lets macOS ask for permission. Mixing takes the part both tracks have reached, and pads one track only if the other stalls for more than a second.
- macOS grants the microphone per code identity. Ad hoc signed builds change identity on every install, so the permission is asked again after a reinstall; the `NoteCraft Local` signature avoids that.

## Keyboard shortcuts

The list lives in `SHORTCUTS` (`crates/notecraft-app/src/app/mod.rs`), is shown in Settings, Shortcuts, and is reproduced in the [user guide](user-guide.md#keyboard-shortcuts).

## Tabs, presentation, reminders and workspaces

- **Tabs** are per window and restored at launch (stored with the app preferences); back and forward history is shared.
- **Presentation** (`present.rs`) opens the local preview of the page (the same HTML as the published page, with a full screen button) in the browser. Mermaid diagrams and equations are turned into SVG by `svg_export.rs`, which paints them on an off-screen egui context and writes the shapes as SVG. The built-in slide view (`--panel present:<n>`) remains for screenshots.
- **First launch and demo**: `app/onboarding.rs` is the four step assistant (`Prefs.onboarded`), and `notecraft-core/src/tour.rs` writes the French or English tour page into a new workspace. `demo.rs` holds the demo pages and its pictures (`crates/notecraft-app/assets/demo`, embedded with `include_bytes!`) and draws them in an `egui::Modal`: every page has the same size so Next stays under the pointer, and `Prefs.demo_seen` stores "Do not show again". It opens by itself once per launch until that box is ticked, and again on request from the welcome screen, the assistant or Settings, About.
- **Published marker**: the sidebar draws a green dot (and the lock, side by side) for pages found in the publish list (`pubs`), and the page menu shows "Publish... ● live".
- **Reminders** are stored in the page front matter and checked by the app while it runs (and once at launch for missed ones). The notification goes through `osascript`; the native API needs a signed app.
- **Workspaces**: recent ones are kept in the app preferences. `Remove from the list` only edits the list. `Move to the Trash` moves the folder to `~/.Trash` after a confirmation, only if it contains `pages/`, and never for the home folder, Desktop, Documents or Downloads (`is_too_broad`).

## Import

`import_any.rs` is the single entry point. It detects the source (a zip is unpacked first, to a scratch folder that is removed afterwards) and converts it to NøteCraft Markdown, then reuses the Markdown importer for pages, links and files.

| Source | Module | Notes |
|---|---|---|
| Markdown folder or CSV export | `import_export.rs`, `import_csv.rs` | Markdown and CSV exports; ids are stripped from names, databases and rows are rebuilt with typed properties, links between pages become mentions. A text column whose cells name the rows of another imported database (at least 80 %) becomes a real relation |
| HTML export | `import_html.rs` | HTML is converted to Markdown with html5ever; `data:` images are decoded, local images stay inside the page's folder |
| Evernote | `import_enex.rs` | Streams the file note by note (a 10 GB export does not load whole); attachments are matched by their MD5 |
| Word | `import_docx.rs` | Reads `word/document.xml` (DTDs refused): headings, lists, tables, links, images |
| Obsidian | `import_any.rs` | Wikilinks, embeds, callouts and comments are rewritten before the Markdown pass |
| Bear, Joplin | `import_export.rs`, `import_any.rs` | `.textbundle`, Markdown folder, `.jex` (tar read in memory) |
| CSV | `import_csv.rs` | Column types are guessed; one row per record |

Zip extraction (`import_zip.rs`) refuses paths that leave the target folder and caps entries and sizes. Anything skipped or imported with a caveat is listed on an "Import report" page next to the result.

## Hostile input and safety

- Parsers for formulas (`math.rs`), the equation solver (`solve.rs`) and diagrams (`mermaid.rs`) cap input length, nesting depth, nodes and edges. Fuzz style tests (`tests/robustness.rs`) feed them random text.
- Links open only for allowlisted targets; a file opens only if its canonical path stays inside the workspace. Export never copies files from outside the workspace.
- Equations are stored as `$$` lines, diagrams as ```` ```mermaid ```` code blocks and partial colour as `<span style="color:NAME">`, so plain Markdown stays readable anywhere.
- Importers treat every file as hostile: zip paths are confined to the target folder, XML refuses DTDs, HTML depth is capped, local files are only read from inside the imported folder.
- `scripts/e2e-cli.sh` checks the command line tool end to end on a scratch workspace.

## Known limits

- Mouse text selection works inside one block; block selection (Esc, then Shift and arrows) copies, cuts and pastes whole blocks as Markdown.
- Merging is per block: two people editing the same block make a conflict. A database's `schema.json` edited in two places keeps one version.
- One window with tabs. No comments or sharing yet. Formulas and rollups are computed in memory (`formula.rs`) and never written to the Markdown files.
- Reminders only fire while the app is running. Memory grows by roughly 6 MB per full reload at 25 000 documents (cause not established).
- Local AI answers are streamed; cloud answers arrive whole. A request cannot be cancelled once sent.
- Images are previewed; PDFs and other files open in their default app.
