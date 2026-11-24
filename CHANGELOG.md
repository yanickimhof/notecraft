# Changelog

All notable changes are listed here. The format follows Keep a Changelog and the project follows semantic versioning.

## [0.5.0] - 2026-11-24

### Added
- **AI**: Grok as a fourth engine; engines are chosen with icons, and the model menu is grouped by family, newest first.
- **Editor**: actions on several selected lines (turn into, colour, group in a section, copy, cut, duplicate, move, delete); a text colour and a background colour can be combined; dropping a line beside another makes columns.
- **Assistant (MCP)**: a simpler settings page.

### Changed
- New name, **NøteCraft**, and a new logo. The command is `notecraft`; the data and settings folders carry the new name.
- Rich Markdown written by assistants (callouts, columns, colours, tables, toggles) is converted by a generic converter.
- Backup settings are reached from the Backups row.

### Fixed
- Several small issues with selections, colours and drag and drop in the editor.
- The Keychain permission is kept across updates of the app.

### Removed
- The sample workspace (welcome screen, workspace menu and `notecraft sample`).
- The import of one specific application's exports and the `notecraft restore-html` command. The other imports are unchanged.

## [0.4.0] - 2026-11-24

### Added
- **Assistant (MCP)**: `replace_text` corrects one passage of a page without rewriting it (refused when the passage is missing or ambiguous, snapshot first). `get_page` can be read in slices (`offset`, `max_chars`) and delimits the body with `<content>` and `</content>`.

- **Properties**: Person (names), Files (attachments) and Unique ID (automatic number); filters can be nested in groups (AND/OR inside AND/OR).
- **Dashboard view** for databases: totals, averages and distributions at a glance.
- **Graph view** of the links between pages (sidebar), with pan and zoom.
- **Diagrams**: Mermaid now also draws sequence diagrams, pie charts and state diagrams.
- **Export as a website**: one HTML file per page, mentions turned into links, breadcrumb and index; locked pages are left out (app menu, CLI `notecraft site <workspace> <folder>`).
- **Meetings**: the other people are told apart by their voices and numbered (Speaker 1, Speaker 2, ...); with microphone and system audio both recorded, your own words are labelled Me. No model to download: the voice is compared from its spectrum and pitch. Clearly different voices separate well; similar voices, crosstalk and very short interjections can be wrong.
- **Meetings**: rename a speaker in one click (name under the card, Enter to confirm); an existing name merges two speakers.
- **Meetings, custom vocabulary**: a list of names, projects and jargon (Settings, Meetings) is given to the speech model so the transcript spells them right; text that only repeats the list on silence is dropped.
- **Meetings, second speech engine**: NVIDIA Parakeet (quantized TDT 0.6B v3, about 670 MB) can be downloaded and removed from Settings, like the Whisper models. It is about sixteen times faster than real time on this Mac and handles English and 24 other languages, but cannot use the custom vocabulary. The engine setting is Automatic (Parakeet only for English, when downloaded and with no vocabulary), Whisper or Parakeet. Attribution in `THIRD_PARTY_MODELS.md`.
- **Import check**: `notecraft verify <workspace>` looks for what an import can break (table rows left as text, uneven tables, rich markup left in the text, missing files or images, mentions of pages that do not exist, unbalanced columns); `import` and `rebody` run it at the end and say what they found. It also finds callouts whose icon was left inside the text, citations (`[[1]](...)`) left as links and links to `.md` files that lead nowhere.

- **AI**: ChatGPT as a third engine (OpenAI API, own key in the Keychain, model list fetched from the account). Settings → AI shows the card of the chosen engine only.
- **CLI**: `notecraft rebody` also replaces the body of a database row (given by its id), so rows keep the formatting of an import.
- Drop-down menus have names for screen readers (VoiceOver).

### Changed

- Column controls only appear when the pointer is over the columns.
- Cover banners are cropped around the centre instead of stretched; images are centred in the page.

### Security
- Assistant (MCP): the file tools refuse keys, passwords, cookies and system files by default (hidden folders, `Library`, `/etc`, `..`), and `--files-root` limits them to chosen folders.

### Fixed
- Images that follow a very large image were drawn tiny or not at all.
- A colour on the last item of a callout was read as the callout colour and lost on save; a coloured callout without a title lost its colour.
- Coloured quotes showed their colour marker as text.
- A Yes/No first column no longer becomes the title of imported rows, and row pages are no longer duplicated by the import.
- `set_cover` refuses a file that is missing or outside `assets/`.
- Memory: the growth seen when reloading a very large workspace is the system allocator keeping freed pages (`leaks`: 0 leaks), not a leak.

## [0.3.0] - 2026-11-23

### Added
- **Import** from Evernote, Obsidian, Bear, Joplin, Word, HTML (Apple Notes exports included), CSV and Markdown. The source is recognised automatically, the import runs in the background, and an "Import report" page lists anything that could not come across. The sidebar Import button, the `notecraft import` command and the assistant use the same importer.
- **Tabs** (⌘T, ⌘W, ⌃Tab), restored at the next launch.
- **Presentation mode** (⌥⌘P): a page shown as full screen slides, with equations and diagrams drawn.
- **Reminders** on a page (••• then Remind me), with a Reminders list in the sidebar and a macOS notification, also for a reminder that came due while the app was closed.
- **Databases**: Formula and Rollup properties, and Chart (bars, line, donut) and Timeline views.
- **Blocks**: simple table (`/table`), web bookmark (`/bookmark`), database view inside a page (`/database`), equation (`/equation`) with step by step solving, and Mermaid flowcharts (`/diagram`). Text colour can be applied to part of a text.
- **Meeting notes**: record what the Mac plays (Teams, Zoom, Meet) as well as the microphone, or both mixed (macOS 14.6 or later); better transcription; one click to start; choice of spoken language in the block.
- **Workspaces**: a red trash can in the recent list moves a workspace folder to the Trash after confirmation, and "Remove from the list" keeps the folder. Only folders that hold a workspace can be trashed, never the home folder, Desktop, Documents or Downloads.
- **Assistant (MCP)**: 46 tools (blocks, views, row templates, reminders). `update_block`, `move_block` and `delete_block` accept an `expect` guard and refuse the edit if the block changed; `get_database` is capped and reports `total_rows` and `truncated`.

### Changed
- A calmer app icon (an N in a rounded square, its diagonal being the slash of the slash menu), also used in the menu bar.
- One set of drawn icons in the sidebar; database tables are quieter (the Open button and the date clear control show on hover).
- The slash menu uses the side of the window with the most room.
- Formulas and diagrams are laid out once and reused until their text changes.

### Fixed
- ⌥← and ⌥→ move by word, also across links; ← and → leave a block selection and put the caret back in the text.
- The large speech model (574 MB) could not be downloaded because of a one minute limit.

### Security
- Links never open programs or installers, and files open only from inside the workspace; export never copies files from outside it.
- Parsers for formulas, equations and diagrams cap input size and nesting.
- A bug inside one assistant tool returns an error instead of stopping the server; the reminders list respects the scope an assistant is limited to.

### Under the hood
- The largest source files are split into submodules, with no change in behaviour.
- `cpal` is pinned to a revision of its main branch, because released versions record silence from system audio.

## [0.2.0] - 2026-11-23

### Meeting notes
- New `/meeting` block: record the microphone, transcribe locally with Whisper (downloadable models: base, small, large turbo), show the transcript live in a "Transcript" toggle, then write notes with the selected AI engine.
- Note templates: general summary, meeting minutes, one to one, interview, sales call, lecture, or custom instructions. Notes can be rewritten with another template.
- Microphone selection in the block and in Settings, level indicator, explicit macOS permission request, and a clear message when a microphone only sends silence.
- Notes are written in the language of the meeting.

### App
- The standard "About NøteCraft" panel now shows the copyright, a short description and the project link.

## [0.1.0] - 2026-11-22

First release.

### Writing
- Pages inside pages, shown as a tree with drag and drop, favourites, trash with restore, breadcrumbs, back and forward, icons and covers.
- Block editor: text, headings, lists, to-dos, toggles, quotes, callouts, code, dividers, images, files, columns and a table of contents. A `/` menu, Markdown shortcuts, a block menu, and a floating bar over selected text.
- Inline formatting (bold, italic, underline, strikethrough, code, links) that combines, and text and background colours.
- Links between pages with `[[Page name]]`, backlinks, and automatic update of links when a page is renamed.
- Selection of several blocks, copied and pasted as Markdown. Undo and redo per page.

### Databases
- Properties: title, text, number, select, multi-select, status, date, checkbox, link, email, phone, relation, created time, last edited time.
- Five views: Table, Board, List, Calendar and Gallery, each with its own filters, sorting, grouping and visible properties.
- Row templates; every row opens as a full page.

### Search and files
- Quick finder (⌘P) and full-text search.
- Attachments by drag and drop, image previews.
- Import of Markdown files and folders; export as Markdown.
- Export of a page as HTML, PDF or Word, with its images. The PDF uses embedded fonts, links, checkboxes, quotes, callouts, code blocks and page numbers.

### Safety
- Private pages: a page can be locked with a passphrase (Argon2id and ChaCha20-Poly1305). Titles stay readable, text is never indexed, older plain copies are erased from backups, and there is no recovery if the passphrase is lost.
- Read-only lock for pages, databases and rows, respected by the app, the command line and assistants.
- Automatic backups on a schedule, restore points, and a version history window for each page. Backups live outside the notes folder.
- Safe saves (temporary file, then rename) and conflict copies instead of silent overwrites.
- The file format is versioned; a workspace written by a newer version opens read-only.
- A home, Desktop, Documents or Downloads folder cannot be opened as a workspace.
- Local log file, nothing is sent anywhere.

### Sync
- No sync engine: keep the folder in iCloud Drive, kDrive, Dropbox or any folder. Changes made elsewhere appear live, and edits are merged block by block.

### AI and assistants
- AI help on a selection or a page (summarise, continue, improve, translate, ask a question). Answers are drafts you accept or discard.
- Two engines: Ollama on the Mac, or the Claude API with your own key kept in the macOS Keychain. Both model lists are fetched live and refresh when new models appear.
- MCP server so Claude Desktop, Claude Code and other assistants can read and edit your notes. Choose read only or read and write, optionally allow deleting, limit to one page tree, and review an audit journal. One click adds it to Claude Desktop or Claude Code.

### App
- Light, dark and automatic appearance, English and French, zoom, VoiceOver labels.
- Menu bar icon: open, new page, search, quit.
- Settings page with sections General, Data, Security, AI, Connected assistants, Shortcuts and About, and a "Where your files are" overview with Show in Finder, Switch and Move for the workspace.
- DMG installer, and a command line tool (`notecraft`) for indexing, search, import, export, backups and benchmarks.

### Performance
- A workspace of 10 000 pages and 50 000 database rows opens in about 0.9 s; search answers in about 30 ms.
