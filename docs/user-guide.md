# NøteCraft user guide

Everything you can do in NøteCraft, in plain words. For installing, see the [README](../README.md). For how it is built, see the [technical guide](technical.md).

## First launch

A one minute demo is built in. Open it from the welcome screen or from Settings, About, whenever you want to see it again. It opens in a window with Next, Back, Skip and "Do not show again", and follows the language of the app.

A short assistant in four steps opens the first time: **welcome** (how your notes are stored, and the language, taken from macOS), **folder** (a recommended `NoteCraft` folder in Documents, or any folder you pick), **AI** (what it is for and that nothing is set up now) and **meetings** (an optional button to download the speech model, with its size). "Skip the rest" is always there. A new, empty workspace opens on **Discover NøteCraft**, a tour page that shows every feature with a live example (columns, tables, images, equations, diagrams, a database with all its views, publishing, AI, meetings, shortcuts). Delete it whenever you like.

Nothing is downloaded automatically. When you first press record in a meeting block, NøteCraft says what it needs (the speech model, with its size, one time, then offline) and waits for your click. The AI engines run only when you choose them: Ollama on your Mac, or your own Claude, ChatGPT or Grok key.

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
- Images can be resized by dragging the round handle on a corner (double click it to go back to the natural size) and aligned left, centre or right from the right-click menu. The size and place are saved in the Markdown file as a small comment, and kept on the published page.
- Bold, italic, underline, strikethrough, code, links and colours (also on part of a text).
- Columns (drag a line to the right edge of another to make one), a table of contents, simple tables, web bookmarks and whole databases shown inside a page.
- Select several lines (⇧↓ or drag the handles) and act on all of them at once: turn into, colour, group in a section, copy, cut, duplicate, move, delete. A text colour and a background colour can be combined.
- Link pages with `[[Page name]]`; the other page shows a backlink.
- Icons, covers, favourites, trash with restore.
- **Tabs**: ⌘T opens a tab, ⌘-click on a page opens it in a new tab, ⌘W closes it. Tabs come back when you restart.
- **Presentation**: ⌥⌘P opens the page in your browser exactly as it looks when published, full width. Scroll, zoom with ⌘+ and ⌘−, and press F (or the button at the top right) for full screen. Nothing is sent anywhere.
- **Reminders**: use the page menu (•••) to set a reminder on a page. The Reminders entry in the sidebar lists them all. A reminder fires while the app is open, or the next time you open it.

## Study and visualise

- **Equations** (`/equation`) with a step by step solver: exact solving of first and second degree equations on your Mac, AI help for the rest.
- **Diagrams** (`/mermaid`): flowcharts, sequence diagrams, pie charts and state diagrams drawn from text.
- **Graph** (sidebar): the links between your pages, draggable and zoomable.
- **Dashboard** (database view): totals, averages and distributions.

## Databases

- Properties: text, number, select, multi-select, status, date, checkbox, link, email, phone, relation, person, files, unique ID, created and edited time, formula and rollup.
- Views: Table, Board, List, Calendar, Gallery, Chart (bars, line, donut) and Timeline.
- Filters, sorting, grouping and templates. Every row is also a full page.

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

## Publish on the web (optional)

- **Publish** a page, or a page with its sub pages, from the page menu (⋯, then Publish). It goes to a private link that cannot be guessed, on your own free Cloudflare account. Nothing is sent until you choose Publish, and **Preview** shows the page in your browser first.
- **Update** sends the new version to the same link (visitors see it within a minute), and **Unpublish** takes the page down (the link stops working within a minute). A green dot next to the page in the sidebar, and "live" after "Publish" in the page menu, tell you the page is published. The dot sits beside the lock when the page is also locked. A page that is live cannot be trashed, archived or deleted for good before it is unpublished: NøteCraft asks, then takes the link down and carries on. If the link cannot be taken down (no network, for instance), the page stays as it is. The MCP server refuses to trash a live page; unpublish it from the app first.
- The published page looks like the page on screen: light and dark mode, covers, columns, callouts, tables, images, Mermaid diagrams and equations (drawn as pictures, no script). Links between published pages work; links to pages you did not publish stay plain text. Encrypted pages are never published.
- Set it up once in Settings, under Publishing: paste a Cloudflare API token with Edit access to Workers Scripts and Workers KV Storage. The rest is created for you.

## Work with AI (optional)

- Summarise, continue, improve or translate text, or ask a question about a page (⌘J).
- Runs on your Mac with Ollama (nothing leaves your computer) or with your own Claude, ChatGPT (OpenAI) or Grok (xAI) API key. Pick the engine with its icon in Settings, then the model from a menu grouped by family. You always see the result first: nothing changes until you click.
- Connect Claude Desktop or Claude Code (Settings, MCP server) so they can read and edit your notes through MCP, with permissions you choose: read only, read and write, or deletion allowed, and optionally limited to one page tree.

## Meeting notes (optional)

1. Type `/meeting` and press **Start recording**. The speech model downloads once (60 MB to 574 MB), then your voice is transcribed on your Mac by Whisper.
2. Pick the spoken language (French, English or automatic) and what to record: your **microphone**, the **system audio** (the other people in a video call, macOS 14.6 or later), or both. macOS asks permission the first time.
3. The transcript appears live in a **Transcript** toggle you can read and edit. When the system audio is recorded (a call), the other people are told apart by their voices: **Speaker 1 :**, **Speaker 2 :**... and your own words are marked **Me :** if you record the microphone too. It works on the sound of the voice, on your Mac, with nothing to download; it separates clearly different voices well, and can mix up similar ones or very short replies. Once the meeting is over, the speakers are listed under the card: click a name, type the real one and press Enter, and every line of that speaker is renamed. Giving a speaker the name of another merges the two (useful when one voice was split in two). Esc cancels.
4. When you stop, the AI engine you chose writes the notes from the transcript, with a transcription template: general summary, meeting minutes, one to one, interview, sales call, lecture, or your own instructions. Transcription templates are pages in the **Transcription templates** section of the sidebar; each holds the structure and the instructions the AI follows, and you can edit them freely. They are kept apart from the page templates (meeting notes, action tracker, and so on) that `/template` pastes into a page, and only they appear in the meeting block's menu. A new workspace starts with one example of each kind (delete it when you have your own); NøteCraft never adds templates to a section you already have, and what you delete does not come back.

In Settings, Meetings, you can add a **custom vocabulary** (names, projects, jargon, separated by commas) so the transcript spells them right, pick the speech model (Base, Small or Large v3 Turbo) and the **engine**. **Parakeet** (NVIDIA, about 670 MB) is downloaded on request and removed with one click; it is much faster and handles English and 24 other languages, but ignores the custom vocabulary. In *Automatic* mode NøteCraft uses it only for English, when it is downloaded and no vocabulary is set; Whisper does everything else.

Only the transcript text goes to Claude, and only if you picked Claude, ChatGPT or Grok. For a call, wear headphones when you record the microphone and the system audio together, otherwise the speakers are heard twice.

## Privacy

- Your notes live in the folder you pick, as plain files. There is no hidden database to lose.
- No account and no telemetry. NøteCraft never connects to the internet on its own. The only network uses are publishing a page, when you ask for it, the AI engine you choose (Ollama on your Mac stays local; Claude sends the text you submit to Anthropic, ChatGPT to OpenAI and Grok to xAI, and the app says so) and, if you ask for it, the one-time download of a speech model (Whisper or Parakeet).
- Your API keys (Claude, OpenAI, Grok) and your Cloudflare token are kept in the macOS Keychain, never in your notes.
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

**Settings** (⌘,) opens on the main page: language, appearance, the AI engine and model, backups and meeting settings. Further pages cover Data (where your files are, with Show in Finder, Switch and Move), Security (private pages), MCP server (add NøteCraft to Claude Desktop or Claude Code), Shortcuts and About. Backups and restore points are under the Backups row.

## Everyday comfort

Light and dark mode, English and French, zoom and VoiceOver labels. The menu bar mark opens the window, creates a page, jumps to the finder or quits. A `notecraft` command line tool (index, search, import, export, backups) is described in the [technical guide](technical.md).
