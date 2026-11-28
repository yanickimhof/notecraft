# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com). Bug fixes are not itemised.

## 0.6.0 (2026-11-27)

### Added
- AI proofreading of a page, with a short AI menu (Summarize, Improve, Proofread, Translate, Ask). The result is applied at once and Command Z undoes it; answers to questions stay in the panel. Transcripts, databases, images and code are never sent to the AI.
- A discreet notice in the sidebar when a newer version is available (can be switched off in Settings, About).
- Local AI shows its progress (seconds, part, characters) and no longer times out while the model is still writing.
- Network guard: one gatekeeper for every connection of the app, a network lockdown switch and an activity log. The local model stays on this Mac.
- Publish a page or a section on the web at a private link, with Preview, Update and Unpublish.
- Send a page by email: the default mail app opens with the title, and the formatted page is ready to paste.
- Flags of every country in the icon picker, with search. Apple colour emoji display in colour.
- Grok as a fourth AI engine, next to Claude, ChatGPT and the local model.
- Meeting notes with one transcription template per kind of meeting; speakers told apart, renamed and merged.
- Page templates, a Templates section, and a one minute demo with a first launch assistant.
- Simple tables: select several cells and format them together (bold, colours, cell background).
- Database tables: resizable columns, wrapped text, keyboard moves between cells, editable tag colours.
- Pasting from Outlook, Word or a web page keeps headings, lists, bold, links and tables.
- Actions on several selected lines, columns by dragging a line beside another, resizable images, today's date shortcut, cover gradients.
- Command click on a link opens it. Backups thinned like Time Machine. Foldable sidebar sections.

### Changed
- Smoother keyboard navigation: arrows, Shift and block selection behave the same way as in a text editor.
- Long pages stay smooth. Simpler page menu.

## 0.5.0 (2026-11-27)

### Added
- Publish a page or a section on the web at a private link, with Preview, Update and Unpublish. A green dot marks published pages, which cannot be trashed or deleted until unpublished.
- A one minute demo, a tour page and a first launch assistant (language, folder, AI and meetings); nothing is downloaded without a click.
- Grok as a fourth AI engine. Local models: suggested downloads with progress, and removal.
- Network guard: one gatekeeper for every connection, a lockdown switch, an activity log, and a lock that keeps the local model on this Mac.
- Templates section in the sidebar: page templates and meeting note templates (editable pages, chosen in the meeting block). Today's date with / date or Command Shift D.
- Actions on several selected lines, columns by dragging a line beside another, resizable images, cover gradients.
- Command A selects a line then the whole page; copying keeps bold, colours and links for a mail or Word.
- Database tables: resizable columns, wrapped text, keyboard moves between cells, editable status and tag options.
- Simple tables: typing, arrows, Tab and Shift Enter work as in a text editor.
- Pasting from Outlook, Word or a web page keeps headings, lists, bold, links and tables. Command click opens a link.
- Automatic backups thinned like Time Machine. Foldable sidebar sections.
- A database created by Claude (MCP) inside a page shows in that page.

### Changed
- Long pages stay smooth (blocks outside the window are not redrawn).
- Simpler page menu and sidebar. Presentation opens the page in the browser.
- Pasted links are never fetched when they point at this Mac or a private network.

### Security
- HTML export no longer writes links that run code. Colour changes and unusual pasted HTML can no longer damage text.

### Fixed
- Various bug fixes.

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
