# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com). Bug fixes are not itemised.

## 0.5.5 (2026-11-26)

### Added
- A Templates section in the sidebar, under Pages: page templates, then the templates for meeting notes. The + creates a page template.
- The meeting block chooses the kind of meeting (note template) before recording, with small frameless menus for what is recorded, the language, the microphone and the template. A small icon in the corner puts the settings back to their defaults.

### Changed
- The New database entry is gone from the sidebar (the / menu still creates databases).

## 0.5.4 (2026-11-26)

### Added
- Note templates for recorded meetings: a "Meeting notes" page inside Templates holds them (each sub-page is one, written as instructions for the AI). The meeting block has a template menu with the built-in ones and yours, and a Reset link that puts the meeting settings back to their defaults.
- Local models: Settings, AI lists suggested models with a download button, the progress and a choose button.
- More covers: 24 gradients, picked from a gallery.
- Simple tables: Shift Enter adds a line break inside a cell.
- Database tags can be renamed in the options editor; renaming or deleting a tag updates every row.

### Changed
- Copying part of a text keeps its formatting (bold, colours, links) when pasted in a mail or a document, and colours from a mail or web page are pasted as colours.
- Bullets use one simple dot at every level.

### Fixed
- Colouring a whole line now also recolours the words that had a colour of their own.
- The slash menu keeps its height when the page is short.

## 0.5.3 (2026-11-26)

### Added
- Today's date: type / then date to pick a format (numeric, ISO, long, with the weekday, with the time), or press Command Shift D for the format used last.
- Templates: / then Template inserts the blocks of a template page. The Templates page holds your own templates (each sub-page is one) and a standard Meeting minutes; {{date}} becomes today's date.

### Fixed
- Simple tables: spaces can be typed, and the arrows, Enter, Tab and Shift Tab move between cells.
- Various bug fixes.

## 0.5.2 (2026-11-26)

### Added
- Command A selects the text of a line, then every block of the page; copying gives rich text for a mail or Word, and Markdown for a text editor.
- Status and tag options: a new one is added at the end, and each can be renamed, recoloured, regrouped or deleted from its cell menu.
- Local AI: when Ollama has no model, NøteCraft offers to download one and shows the progress.

### Fixed
- Speech model loading falls back to the processor, and a damaged model file is detected.
- Various bug fixes.

## 0.5.1 (2026-11-26)

### Added
- A database created by Claude (MCP) inside a page now shows in that page, not only as a sub-page.

### Fixed
- Model downloads trust the macOS certificate store (company proxies) and explain network errors.
- Various bug fixes.

## 0.5.0 (2026-11-24)

### Added
- A one minute demo built into the app, in French or English following the app language: on the welcome screen, in the first launch assistant and in Settings, About.
- A green dot in the sidebar and "live" in the page menu show which pages are published. It stacks with the lock icon.
- Grok as a fourth AI engine.
- Actions on several selected lines at once: turn into, colour, group, move, delete.
- Columns by dragging a line beside another.
- Publish a page or a section on the web at a private link, with Preview, Update and Unpublish.
- A published page cannot be trashed, archived or deleted until it is unpublished: NøteCraft asks first and takes the link down, and Claude (MCP) is refused while a page is live.
- A tour page, Discover NøteCraft, opens in every new workspace and shows all the features.
- A four step assistant at first launch: welcome and language (taken from macOS), folder, AI and meetings, with an optional speech model download. Nothing is downloaded without a click.
- Images can be resized (drag a corner handle) and aligned left, centre or right; size and place are kept on the published page.
- Mermaid diagrams and equations appear as pictures on published pages and in the preview.
- Pasting from Outlook, Word or a web page keeps headings, lists, bold, links and tables.
- Database tables: drag a column edge to resize it, long text wraps, the keyboard moves between cells (arrows, Tab, Enter), and a tag's colour can be changed from its cell menu.
- Command click on a link opens it.
- Automatic backups are thinned like Time Machine: hourly for a day, daily for a month, weekly for a year, then yearly.

### Changed
- Interface refinements.
- Simpler page menu: Export, Protection, Appearance and Arrange are submenus.
- Presentation opens the page in the browser with the same rendering as the published page, with scrolling, zoom and a full screen button.

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
