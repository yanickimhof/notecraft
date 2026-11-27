#!/usr/bin/env python3
"""Generate the feature poster (docs/brand/features*.html). Render to PNG with:
   ./scripts/gen-features.py && ./scripts/render-features.sh
Edit the CONTENT table below; every string exists in English and French."""
import html, pathlib

ICONS = {
 "write": '<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/>',
 "math": '<path d="M18 7V5a1 1 0 0 0-1-1H6.5a.5.5 0 0 0-.4.8l4.5 5.7a.5.5 0 0 1 0 .6l-4.5 5.7a.5.5 0 0 0 .4.8H17a1 1 0 0 0 1-1v-2"/>',
 "db": '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18"/><path d="M9 21V9"/>',
 "find": '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
 "lock": '<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
 "ai": '<path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9Z"/><path d="M19 15l.7 1.8 1.8.7-1.8.7L19 20l-.7-1.8-1.8-.7 1.8-.7Z"/>',
 "mic": '<rect x="9" y="2" width="6" height="12" rx="3"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><path d="M12 19v3"/>',
 "link": '<path d="M8 3 4 7l4 4"/><path d="M4 7h16"/><path d="m16 21 4-4-4-4"/><path d="M20 17H4"/>',
 "home": '<path d="M3 11 12 3l9 8"/><path d="M5 10v10h14V10"/>',
 "md": '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M6 15V9l3 3 3-3v6"/><path d="M17 9v6m-2-2 2 2 2-2"/>',
 "user": '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
 "plane": '<path d="M2 8.8a15 15 0 0 1 4-2.5"/><path d="M5 12.9a10 10 0 0 1 5.2-2.7"/><path d="M8.5 16.4a5 5 0 0 1 7 0"/><path d="M12 20h.01"/><path d="m2 2 20 20"/>',
 "sun": '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.9 4.9l1.4 1.4m11.4 11.4 1.4 1.4M2 12h2m16 0h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
}

CONTENT = {
 "en": {
  "file": "features.html", "tagline": "Your notes, tables and ideas in one fast Mac app.",
  "cards": [
   ("write", "Write", ["Pages inside pages, type / to add anything", "Headings, lists, to-dos, callouts, code", "Page templates, tabs, columns, [[links]] and backlinks"]),
   ("math", "Study and visualise", ["Equations with a step by step solver", "Flowcharts drawn from text", "Present any page as full screen slides"]),
   ("db", "Databases", ["Formula, rollup, relation, status and more", "7 views: table, board, list, calendar, gallery, chart, timeline", "Filters, sorting, grouping, templates"]),
   ("find", "Find", ["Quick finder with ⌘P", "Full text search across everything", "Reminders with a macOS notification"]),
   ("lock", "Protect", ["Private pages locked by passphrase", "Automatic backups and version history", "Safe saving, conflict copies, read-only lock"]),
   ("ai", "AI, your way", ["Summarise, improve, translate, ask a page", "On your Mac with Ollama, or your Claude key", "You see the result before anything changes"]),
   ("mic", "Meeting notes", ["Live transcript by Whisper, on your Mac", "Microphone, system audio, or both", "Your own transcription templates: minutes, 1:1, lecture"]),
   ("link", "Connect", ["Claude Desktop and Claude Code via MCP", "Import Evernote, Obsidian, Bear, Word, CSV", "Command line for scripts"]),
  ],
  "pills": [("home", "Local first"), ("md", "Plain Markdown files"), ("user", "No account, no tracking"), ("plane", "Works offline"), ("sun", "Light and dark, English and French")],
  "foot": "macOS 11+, Apple silicon. Your notes stay in a folder you own.",
 },
}

def svg(name, size, color, sw=2):
    return (f'<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="{color}" '
            f'stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round">{ICONS[name]}</svg>')

CSS = """
*{box-sizing:border-box;margin:0;padding:0}
body{width:1600px;height:900px;background:#F4EEE5;font-family:-apple-system,"SF Pro Text","Helvetica Neue",Helvetica,Arial,sans-serif;color:#262421;position:relative;overflow:hidden}
.bar{position:absolute;left:0;top:0;width:1600px;height:8px;background:#E2672C}
header{position:absolute;left:56px;top:38px;display:flex;align-items:center;gap:22px}
header img{width:84px;height:84px;border-radius:19px;box-shadow:0 4px 14px rgba(38,36,33,.25)}
h1{font-size:46px;font-weight:800;letter-spacing:-1px;line-height:1}
.tag{font-size:21px;color:#B8501F;margin-top:9px}
.grid{position:absolute;left:56px;top:158px;width:1488px;display:grid;grid-template-columns:repeat(4,1fr);gap:20px}
.card{background:#fff;border:1px solid #E4DACB;border-radius:20px;height:262px;padding:22px 22px 0;box-shadow:0 1px 2px rgba(38,36,33,.05)}
.top{display:flex;align-items:center;gap:13px;margin-bottom:15px}
.tile{width:46px;height:46px;border-radius:13px;background:#FBE6DA;display:flex;align-items:center;justify-content:center;flex:none}
h2{font-size:22px;font-weight:700;letter-spacing:-.3px}
ul{list-style:none}
li{position:relative;padding-left:17px;font-size:17px;line-height:1.38;color:#4A453F;margin-bottom:10px}
li:before{content:"";position:absolute;left:0;top:9px;width:7px;height:7px;border-radius:50%;background:#E2672C}
.pills{position:absolute;left:56px;top:748px;width:1488px;height:76px;background:#262421;border-radius:20px;display:flex;align-items:center;justify-content:space-around;padding:0 18px}
.pill{display:flex;align-items:center;gap:11px;color:#F4EEE5;font-size:17px;font-weight:600}
.foot{position:absolute;left:0;top:849px;width:1600px;text-align:center;font-size:15px;color:#8A8277}
"""

def page(lang):
    c = CONTENT[lang]
    cards = ""
    for ic, title, bullets in c["cards"]:
        li = "".join(f"<li>{html.escape(b)}</li>" for b in bullets)
        cards += (f'<div class="card"><div class="top"><div class="tile">{svg(ic, 26, "#C2490F")}</div>'
                  f'<h2>{html.escape(title)}</h2></div><ul>{li}</ul></div>')
    pills = "".join(f'<div class="pill">{svg(i, 24, "#F2B33D")}<span>{html.escape(t)}</span></div>' for i, t in c["pills"])
    return (f'<!doctype html><meta charset="utf-8"><title>NøteCraft</title><style>{CSS}</style>'
            f'<div class="bar"></div><header><img src="../../assets/icon.svg" alt=""><div><h1>NøteCraft</h1>'
            f'<div class="tag">{html.escape(c["tagline"])}</div></div></header>'
            f'<div class="grid">{cards}</div><div class="pills">{pills}</div>'
            f'<div class="foot">{html.escape(c["foot"])}</div>')

out = pathlib.Path(__file__).resolve().parent.parent / "docs" / "brand"
out.mkdir(parents=True, exist_ok=True)
for lang, c in CONTENT.items():
    (out / c["file"]).write_text(page(lang), encoding="utf-8")
    print("wrote", out / c["file"])
