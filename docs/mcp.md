# MCP server

`notecraft-mcp` exposes a NøteCraft workspace to any MCP client (Claude Desktop, Claude Code and others) over stdio. It reads and writes the same Markdown files as the app, offline. Changes made through it appear live in a running app.

## Run

```sh
notecraft-mcp --workspace <folder> [--read-only] [--allow-delete] [--scope <page>]... [--files-root <folder>]...
```

The workspace can also be given with the `NOTECRAFT_WORKSPACE` environment variable. The permission flags are described below. The app copies a ready-made `claude mcp add` command from Settings.

## Tools (47)

| Tool | Description |
|---|---|
| `list_pages` | The page tree (titles, ids, icons, nesting), databases included, 200 entries per page; pass next_cursor as cursor for more. |
| `get_page` | A page or database row: metadata (id, title, icon, parent, created, updated, favourite), row properties, backlinks and the Markdown body between `<content>` and `</content>`. Long pages can be read in slices with `offset` and `max_chars`. |
| `search` | Full-text search (titles and bodies, accent-insensitive, prefix matching) over pages, databases and rows, with snippets. |
| `backlinks` | Pages and rows that link to this page with [[Title]]. |
| `recent_pages` | Most recently edited pages, databases and rows. |
| `favorites` | Pages and databases marked as favourites. |
| `create_page` | Create a page with an optional Markdown body. |
| `update_page` | Replace or append to the Markdown body of a page or row. |
| `replace_text` | Replace one passage of the page (as `get_page` shows it) and leave the rest untouched; `old` must be unique unless `all` is true. |
| `append_blocks` | Append blocks to a page or row. Each block: {type: text|h1|h2|h3|bulleted|numbered|todo|toggle|quote|callout|code|divider, text, checked?, language?, depth?}. |
| `get_blocks` | The blocks of a page or row, numbered from 0 (type, text, depth, checked, language). Tables, columns, images and meetings appear as `other` and are left untouched. |
| `insert_blocks` | Insert blocks at a position. |
| `update_block` | Change one block in place (type, text, checked, language, icon, depth). A snapshot is taken first. Pass `expect` (the start of the block text as read) so the edit is refused if the block moved or changed. |
| `move_block` | Move one block to another position. Accepts `expect`, like `update_block`. |
| `delete_block` | Delete one block (snapshot first; needs `--allow-delete`). Accepts `expect`, like `update_block`. |
| `rename_page` | Rename a page, row or database; [[mentions]] of the old title follow. |
| `move_page` | Move a page or database under another page (or to the top level), at a position among its siblings. |
| `duplicate_page` | Duplicate a page (its blocks, icon and cover) next to it. |
| `set_icon` | Set the emoji icon of a page, row or database (empty string removes it). |
| `set_cover` | Set a cover: gradient:0 … gradient:7, the path of an existing image under assets/, or empty to remove. |
| `set_favorite` | Add a page or database to the favourites, or remove it. |
| `lock_page` | Lock a page or database read-only (inherited by its children). Only a person can unlock it, from the app. |
| `list_reminders` | Pages that have a reminder, soonest first (page, id, local date and time). |
| `set_reminder` | Set or clear the reminder of a page (`YYYY-MM-DD HH:MM`; a bare date means 09:00; empty clears). The app shows a notification when it is due. |
| `trash_page` | Move a page, row or database (with what is inside) to the trash. Restorable. |
| `restore_page` | Restore something from the trash (id or title). |
| `empty_trash` | Delete everything in the trash for good. |
| `list_databases` | Databases with properties (type, options), views and row counts. |
| `get_database` | A database: schema (properties, options, views), templates and its rows with their properties (200 by default, 1000 at most with `limit`; `total_rows` and `truncated` say whether more exist). |
| `create_database` | Create a database (title property 'Name', plus Status, Tags and Date by default). |
| `add_property` | Add a property. type: text, number, select, multi_select, status, date, checkbox, url, email, phone, created_time, edited_time, relation, formula, rollup, person (names), files (paths inside assets/), unique_id (numbered automatically, read-only) (a formula is an expression such as `prop("Price") * prop("Qty")`; a rollup needs a relation, a property of the related database and a function). |
| `create_view` | Add a view: table, board, list, gallery, calendar, chart or timeline, with filter, sort, group_by and visible properties. |
| `update_view` | Change a view (name, kind, filter, sort, group_by, visible properties, timeline end). |
| `delete_view` | Delete a view; a database keeps at least one (needs `--allow-delete`). |
| `create_template` | Add a row template with default properties and a Markdown body. |
| `delete_template` | Delete a row template (needs `--allow-delete`). |
| `rename_property` | Rename a property (row files follow). |
| `delete_property` | Delete a property and its values. |
| `add_row` | Add a row with properties and an optional Markdown body. |
| `update_row` | Change a row's title, properties, or append Markdown to its body. |
| `delete_row` | Move a row to the trash. |
| `query_rows` | Filter, sort and group rows. filter: {name: value} (equal/contains) or [{property, op, value}] with op is, is_not, contains, not_contains, is_empty, is_not_empty, greater, less, checked, unchecked; match: all|any; sort: [{property, descending}]; group_by: property; view: use a saved view. |
| `list_templates` | Row templates of a database, with their default properties and body. |
| `add_row_from_template` | Add a row from a template (by template id or name). |
| `add_attachment` | Copy a file into assets/ and optionally add it to a page as an image or file block. |
| `import_markdown` | Import notes from a file, folder or zip: Markdown, HTML, Word (.docx), CSV, Evernote (.enex), and exports from Obsidian, Bear, Joplin and other workspace apps (sub-folders become sub-pages; images and files are copied). |
| `export_markdown` | Export a page (with sub-pages), a database (+CSV) or the whole workspace to plain Markdown in a folder. |

## What an assistant can and cannot do

Can do, through the tools above: read, search and organise pages and databases; write page bodies, change single blocks and database rows; create views and row templates; create databases with every property type, including formulas, rollups and relations; query rows with filters, sorts, groups and saved views; use row templates; add attachments; import from other apps; export; set reminders and the favourite, icon and cover of a page; lock a page.

Cannot do (yet):

- **Edit every kind of block.** Text, headings, lists, to-dos, toggles, quotes, callouts, code, dividers and the table of contents can be changed one by one. Tables, columns, images, files, bookmarks, equations, diagrams, embedded databases and meetings can only be reached through the page as Markdown.
- **Set a view's chart options, column widths or board colours.** Views are created and filtered, sorted, grouped and trimmed to chosen properties; the finer display settings stay in the app.
- **Unlock a page, restore a snapshot or change permissions.** Only a person can, from the app or the command line (see Snapshots).
- **Read the body of an encrypted page.** The server has no passphrase; the content stays unreadable to it.
- **Drive the interface.** Tabs, presentation mode, the AI panel, meeting recording and transcription, settings, and choosing the workspace are not available. The workspace is fixed when the server starts.
- **Receive events.** The server answers requests; it does not notify the client when a reminder is due or when the person edits a page.
- **Read the content of attachments.** `add_attachment` copies a file in and links it; images, PDFs and audio are not analysed by the server.

## Permissions

The permissions are fixed when the server starts, in the client configuration. A client cannot change them: no tool widens them.

| Level | Tools | When allowed |
|---|---|---|
| Read | everything that only reads (`list_pages`, `get_page`, `search`, `query_rows`...) | always |
| Write | create, edit, rename, move, duplicate, import, attach, lock, restore, reminders | default, refused with `--read-only` |
| Delete | `trash_page`, `delete_row`, `delete_property`, `delete_block`, `delete_view`, `delete_template`, `empty_trash` | only with `--allow-delete` |

Tools a level does not allow are not even listed to the client. `--scope <page>` (repeatable, a title or an id) limits everything the client sees and changes to that page and what is inside it:

- `list_pages`, `search`, `recent_pages`, `favorites`, `backlinks`, `list_databases` and the resources only show documents inside the scope;
- reading, editing or moving something outside, or moving something out of the scope, is refused;
- creating or importing needs a parent inside the scope; `export_markdown` and `add_attachment` need a page inside it; `empty_trash` and attachments as resources are unavailable;
- a scope that does not exist makes the server refuse to start.

### Files on the computer

`add_attachment`, `import_markdown` and `export_markdown` take a path on the computer. So that an assistant, or text it was tricked by, cannot reach keys or passwords through them, the server refuses by default: hidden files and folders in the home folder (`.ssh`, `.aws`, `.config`...), the `Library` folder (keychains, cookies, application data), system folders such as `/etc`, relative paths and `..`. Ordinary folders of the home folder, the temporary folders, external volumes and the workspace stay open. `--files-root <folder>` (repeatable) replaces this rule: only those folders (and the workspace) are allowed.

Typical registrations:

```sh
# explore only
claude mcp add notecraft-ro -- notecraft-mcp --workspace ~/Notes --read-only
# let an assistant work in one project, without deleting anything
claude mcp add notecraft-work -- notecraft-mcp --workspace ~/Notes --scope "Projects"
```

Locks (`lock_page`) apply on top of these permissions, and only a person can lift them.

## Audit journal

Every tool call, allowed or refused, is appended to `~/Library/Application Support/NoteCraft/audit/<workspace hash>.jsonl`, outside the notes folder. One JSON object per line: `time`, `tool`, `level` (read, write, delete), `outcome` (ok, error, denied), `args` and `detail` for failures. Long texts are replaced by their length (`<500 chars>`), so the journal records what was asked without copying page content. The server prints the journal path on start.

## Safety notes

- Deleting goes through the trash (`trash_page`, `restore_page`); `empty_trash` requires `confirm: true` and is permanent.
- Before it replaces a page body (`update_page` with `mode: replace`) or changes a row, the server takes a snapshot (see below). Deleting for good, emptying the trash and deleting a property take one in the core, whoever asks.
- Keep deletion off, use a `--scope`, and connect an assistant to a copy of the notes until you trust the setup.

## Snapshots

A snapshot is a copy of the affected files, kept outside the notes folder in the app data folder, next to the search index (`.../indexes/<hash>/snapshots/<id>/`). The newest 200 are kept. A person lists and restores them from the command line; no MCP tool can restore, delete or list them:

```sh
notecraft snapshots ~/Notes
notecraft restore ~/Notes <snapshot id>
```

A restore takes a snapshot of what it overwrites first, so it can be undone too. In the app, the version history of a page restores earlier versions of that page.
