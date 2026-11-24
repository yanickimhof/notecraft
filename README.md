<p align="center"><img src="assets/icon-1024.png" width="110" alt="NøteCraft icon"></p>

# NøteCraft

**Notes, tables and ideas in one fast Mac app. Your files, your folder, your rules.**

No account. No tracking. No subscription. Works offline.

<p align="center"><a href="../../releases/latest"><b>Download for Mac</b></a> · Apple silicon, macOS 11 or later</p>

![A page in NøteCraft](docs/screenshots/page.png)

## At a glance

![NøteCraft at a glance](docs/brand/features.png)

## Why you will like it

| | |
|---|---|
| **Your notes stay yours** | Every page is a plain Markdown file in a folder you choose. Open it with any app, back it up like any folder, sync it with iCloud Drive, Dropbox or kDrive. If you stop using NøteCraft, nothing is locked in. |
| **Private by design** | No account, no telemetry. Private pages are encrypted with your passphrase. Your AI keys stay in the macOS Keychain. |
| **Instant** | A native app written in Rust. 10,000 pages open in under a second and search answers as you type. |
| **AI on your terms** | Run Ollama on your Mac, or bring your own Claude, ChatGPT or Grok key. You see every result before anything changes. Claude Desktop and Claude Code can work on your notes through MCP, with the permissions you set. |
| **More than notes** | Databases with seven views, meeting notes transcribed on your Mac, equations, diagrams, reminders, presentations. |

## See it in action

| Databases | Meeting notes |
|---|---|
| ![Board view](docs/screenshots/board.png) | ![Meeting notes](docs/screenshots/meeting.png) |
| **AI assistant** | **Dark mode** |
| ![AI assistant](docs/screenshots/ai.png) | ![Dark mode](docs/screenshots/page-dark.png) |

More views: [table](docs/screenshots/table.png), [calendar](docs/screenshots/calendar.png), [tabs](docs/screenshots/tabs.png), [presentation](docs/screenshots/present.png), [reminders](docs/screenshots/reminder.png).

## Get started

1. Download the DMG from the [latest release](../../releases/latest).
2. Drag **NøteCraft** into **Applications**.
3. First launch: the app is not signed with an Apple developer certificate yet, so macOS blocks it once. Right-click the app and choose **Open**; if that is not offered, open **System Settings, Privacy & Security** and click **Open Anyway**.
4. Pick a folder for your notes, or import them from Evernote, Obsidian, Bear, Joplin, Word, HTML or CSV.

## Learn more

- [User guide](docs/user-guide.md): everything the app does, shortcuts, how your data is handled.
- [Technical guide](docs/technical.md) and [architecture](docs/architecture.md): build instructions, file format, sync, the MCP server.
- [Changelog](CHANGELOG.md)

## License

Personal, non-commercial use. All rights reserved. See [LICENSE](LICENSE).
