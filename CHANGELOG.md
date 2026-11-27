# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com). Bug fixes are not itemised.

## 0.6.1 (2026-11-27)

### Added
- AI review of a page: in the AI panel, Proofread (spelling and grammar only), Proofread and clarify, and Reorganise the notes (headings, lists, summary, action items, tables kept). The AI works on the whole page, or on the selection when there is one, and shows a draft with a Show original switch. Nothing changes until you press Replace the page, and Command Z undoes it. Transcripts, databases, images, code, equations and the meeting block are never sent to the AI and come back untouched; a warning appears when figures or dates differ, and a review that loses an element is refused.
- Flags of 255 countries and territories in the page icon picker, with a search box and the country name (English and French) on hover. Flag icons are drawn as pictures in the sidebar and the page header.

### Fixed
- Shift and the arrow keys extend the text selection again.
- Apple colour emoji now show in colour in pages, titles and icons (typed with Control Command Space or pasted).

## 0.6.0 (2026-11-27)

Merges the 0.5.x releases.

### Added
- Network guard: every connection of the app (local model, cloud AI, model downloads, publishing, link previews) goes through one gatekeeper. Settings, AI: a network lockdown switch (nothing leaves this Mac), a list of the latest connection attempts, a lock that keeps the local model on this Mac (red when open) and a warning when Ollama answers on the whole network.
- A one minute demo built into the app, in French or English, and a four step assistant at first launch (language, folder, AI and meetings, optional speech model).
- A tour page, Discover NøteCraft, in every new workspace.
- Publish a page or a section on the web at a private link, with Preview, Update and Unpublish; a green dot in the sidebar shows what is live. Mermaid diagrams, equations and image sizes carry over.
- Grok as a fourth AI engine (with Claude, ChatGPT and the local model).
- Meeting notes: one transcription template per kind of meeting (summary, minutes, one to one, interview, sales call, lecture, or your own), written as editable pages in a Transcription templates section. The meeting block lists only these templates, apart from the page templates, and has menus for what is recorded, the language, the microphone and the template, plus a reset of the recording settings.
- Templates: a Templates section in the sidebar holds page templates that `/` then Template pastes into any page; a new workspace starts with one example of each kind, and nothing is ever added to a section you already have.
- Local models: suggested models with download, progress, choose and remove, from the same menu as the cloud engines; an offer to download one when Ollama has none.
- Sidebar: folding sections that remember their state.
- Today's date (/ then date, or Command Shift D), and 24 cover gradients.
- Command A selects a line, then the whole page; copying gives rich text for a mail or Word, and Markdown for a text editor.
- Actions on several selected lines at once, and columns by dragging a line beside another.
- Dropping a file on a page shows where the link will land.
- Images can be resized and aligned.
- Database tables: resizable columns, wrapped text, keyboard moves between cells, renamable and recolourable status and tag options.
- Pasting from Outlook, Word or a web page keeps headings, lists, bold, links and tables, and colours.
- Command click on a link opens it.
- Automatic backups thinned like Time Machine.
- Send a page by email: Page menu, Export, Send by email opens a draft in the Mac's mail application (Mail, Outlook...) with the formatting kept (headings, lists, bold, colours, links, images).
- Simple tables: select several cells (drag from one cell to another, shift click, the grip at the left of a row, the arrow above a column, or Escape in a cell), then format them together with a small bar: bold, italic, underline, strike, code, text colour and cell background (also shown on published pages). The page shortcuts work in a cell (Command B, I, U, E, Shift S). Delete empties the selected cells and Command C copies them.

### Changed
- Long pages stay smooth: blocks far outside the window are no longer redrawn.
- Presentation opens the page in the browser with the published rendering.
- Simpler page menu, "Turn into" ticks the current type, simpler bullets, and /page adds a sub-page without a link in the body.
- Reminders and Graph shortcuts left the sidebar.
- Pasted links are never fetched when they point at this Mac or a private network.

## 0.4.0 (2026-11-24)

### Added
- Dashboard view for databases and a graph of the links between pages.
- Meetings: speakers told apart and renamed, custom vocabulary.
- ChatGPT as a third AI engine.
- Import check to spot what an import broke.

### Changed
- Faster speech engine for meetings.

### Fixed
- Various bug fixes.

## 0.3.0 (2026-11-23)

### Added
- Import from Evernote, Obsidian, Bear, Joplin, Word, HTML and CSV.
- Tabs, presentation mode and reminders.
- Formula and rollup properties, chart and timeline views.
- Meetings can record what the Mac plays (Teams, Zoom, Meet).

### Fixed
- Various bug fixes.

## 0.2.0 (2026-11-23)

### Added
- Meeting notes with local transcription, note templates and microphone choice.

### Fixed
- Various bug fixes.

## 0.1.0 (2026-11-22)

First release: pages inside pages with a block editor, links and backlinks, databases with five views, search, attachments, import and export (Markdown, HTML, PDF, Word), private pages, automatic backups, page history, local AI (Ollama) or Claude, an assistant connection (MCP), light and dark appearance, English and French.
