# NøteCraft user guide

Everything you can do in NøteCraft, in plain words. For installing, see the [README](../README.md). For how it is built, see the [technical guide](technical.md).

## Your notes, your folder

A workspace is a folder you choose. Each page is a Markdown file inside it, so you can open it with any app, back it up like any folder, and keep it in iCloud Drive, Dropbox or kDrive if you want it on several Macs. NøteCraft never syncs by itself. If a page changes elsewhere while you edit it, both edits are merged; if the same block changed on both sides, your version is kept as a "conflicted copy" and the app tells you.

**Workspaces**
- The workspace menu (top of the sidebar) switches between recent workspaces, closes the open one, or moves it to the Trash after a confirmation.
- In the list of recent workspaces, a red trash can moves that folder to the Trash; right click offers "Remove from the list", which keeps the folder.
- Only a folder that holds a workspace can be trashed, never your home folder, Desktop, Documents or Downloads.
- Choose a dedicated folder for a workspace, not your whole Documents folder.

## Write

- Pages inside pages, shown as a tree you can drag around.
- Headings, lists, to-dos, toggles, quotes, callouts, code, dividers, images and files. Type `/` to add anything, or use the usual Markdown shortcuts.
- Bold, italic, underline, strikethrough, code, links and colours (also on part of a text).
- Columns, a table of contents, simple tables, web bookmarks and whole databases shown inside a page.
- Link pages with `[[Page name]]`; the other page shows a backlink.
- Icons, covers, favourites, trash with restore.
- **Tabs**: ⌘T opens a tab, ⌘-click on a page opens it in a new tab, ⌘W closes it. Tabs come back when you restart.
- **Presentation**: ⌥⌘P shows the page as full screen slides, cut at dividers or headings. Equations and diagrams are drawn.
- **Reminders**: use the page menu (•••) to set a reminder on a page. The Reminders entry in the sidebar lists them all. A reminder fires while the app is open, or the next time you open it.

## Study and visualise

- **Equations** (`/equation`) with a step by step solver: exact solving of first and second degree equations on your Mac, AI help for the rest.
- **Diagrams** (`/mermaid`): flowcharts, sequence diagrams, pie charts and state diagrams drawn from text.
- **Graph** (sidebar): the links between your pages, draggable and zoomable.
- **Dashboard** (database view): totals, averages and distributions.
- **Export as a website** (workspace menu): a folder of HTML pages you can host anywhere.

## Databases

- Properties: text, number, select, multi-select, status, date, checkbox, link, email, phone, relation, person, files, unique ID, created and edited time, formula and rollup.
- Views: Table, Board, List, Calendar, Gallery, Chart (bars, line, donut) and Timeline.
- Filters, sorting, grouping and templates. Every row is also a full page.
- In a table, the Open button and the clear control of a date appear when the pointer is over the row.

## Find

Quick finder with ⌘P and full text search across everything.

## Keep it safe

- **Private pages**: lock a page with a passphrase (strong encryption). There is no way to recover a lost passphrase.
- **Read-only lock** for pages you do not want to change by accident.
- **Automatic backups** with restore points, and a **version history** for each page.
- Safe saving and conflict copies: a sync mishap never silently overwrites your work.

## Get things in and out

- **Import** from Evernote, Obsidian, Bear, Joplin, Word, HTML, CSV and Markdown (a zip, a file or a folder, recognised automatically). The import runs in the background; skipped items are listed on an "Import report" page.
- **Export** a page or the whole workspace as Markdown, and a page as PDF, Word or HTML, with its images.

## Work with AI (optional)

- Summarise, continue, improve or translate text, or ask a question about a page (⌘J).
- Runs on your Mac with Ollama (nothing leaves your computer) or with your own Claude or ChatGPT (OpenAI) API key. You always see the result first: nothing changes until you click.
- Connect Claude Desktop or Claude Code (Settings, Connected assistants) so they can read and edit your notes through MCP, with permissions you choose: read only, read and write, or deletion allowed, and optionally limited to one page tree.

## Meeting notes (optional)

1. Type `/meeting` and press **Start recording**. The speech model downloads once (60 MB to 574 MB), then your voice is transcribed on your Mac by Whisper.
2. Pick the spoken language (French, English or automatic) and what to record: your **microphone**, the **system audio** (the other people in a video call, macOS 14.6 or later), or both. macOS asks permission the first time.
3. The transcript appears live in a **Transcript** toggle you can read and edit. When the system audio is recorded (a call), the other people are told apart by their voices: **Speaker 1 :**, **Speaker 2 :**... and your own words are marked **Me :** if you record the microphone too. It works on the sound of the voice, on your Mac, with nothing to download; it separates clearly different voices well, and can mix up similar ones or very short replies. Once the meeting is over, the speakers are listed under the card: click a name, type the real one and press Enter, and every line of that speaker is renamed. Giving a speaker the name of another merges the two (useful when one voice was split in two). Esc cancels.
4. When you stop, the AI engine you chose writes the notes from the transcript, with a template: general summary, meeting minutes, one to one, interview, sales call, lecture, or your own instructions.

In Settings, Meetings, you can add a **custom vocabulary** (names, projects, jargon, separated by commas) so the transcript spells them right, pick the speech model (Base, Small or Large v3 Turbo) and the **engine**. **Parakeet** (NVIDIA, about 670 MB) is downloaded on request and removed with one click; it is much faster and handles English and 24 other languages, but ignores the custom vocabulary. In *Automatic* mode NøteCraft uses it only for English, when it is downloaded and no vocabulary is set; Whisper does everything else.

Only the transcript text goes to Claude, and only if you picked Claude or ChatGPT. For a call, wear headphones when you record the microphone and the system audio together, otherwise the speakers are heard twice.

## Privacy

- Your notes live in the folder you pick, as plain files. There is no hidden database to lose.
- No account and no telemetry. NøteCraft never connects to the internet on its own. The only network uses are the AI engine you choose (Ollama on your Mac stays local; Claude sends the text you submit to Anthropic and ChatGPT to OpenAI, and the app says so) and, if you ask for it, the one-time download of a speech model (Whisper or Parakeet).
- Your API keys (Claude, OpenAI) are kept in the macOS Keychain, never in your notes.
- Backups and the search index are stored outside your notes folder, so syncing never carries them around.

## Keyboard shortcuts

The full list is also in **Settings, Shortcuts**.

| Keys | Action |
|---|---|
| ⌘P, ⌘K | Quick find (⌘K with selected text adds a link) |
| ⌘N | New page |
| ⌘\ | Show or hide the sidebar |
| ⌘[ ⌘] | Back, forward |
| ⌘T ⌘W | New tab, close tab |
| ⌃Tab ⌃⇧Tab | Next tab, previous tab |
| ⌥⌘P | Present the current page |
| ⌘J | AI assistant |
| ⌘Z ⌘⇧Z | Undo, redo |
| ⌘B ⌘I ⌘U ⌘⇧S ⌘E | Bold, italic, underline, strikethrough, inline code |
| ⌘D | Duplicate block |
| ⌘⇧↑ ⌘⇧↓ | Move block |
| ⌘/ | Turn into |
| ⌘↵ | Check a to-do, open a toggle |
| Esc | Close a menu (keeps your text), or select the block |
| ⇧↓ in a selection | Select several blocks; ← or → goes back to the text |
| ⌥← ⌥→ | Move by word (links included) |
| Tab ⇧Tab | Indent, outdent |
| ⇧↵ | Line break inside a block |
| ⌘= ⌘- ⌘0 | Zoom in, out, actual size |
| ⌘⇧L | Light or dark |
| ⌘, | Settings |

## Settings

**Settings** (⌘,) has sections for General (language, appearance and a "Where your files are" overview with Show in Finder, Switch and Move), Data (backups and restore points), Security (private pages), AI, Connected assistants (add NøteCraft to Claude Desktop or Claude Code), Shortcuts and About.

## Everyday comfort

Light and dark mode, English and French, zoom and VoiceOver labels. The menu bar mark opens the window, creates a page, jumps to the finder or quits. A `notecraft` command line tool (index, search, import, export, backups) is described in the [technical guide](technical.md).
