//! The slash menu's items, their search and the `/query` detection.

use super::SlashMode;
use crate::i18n::both;
use notecraft_core::blocks::BlockKind;
use notecraft_core::search::fuzzy_score;

pub struct SlashItem {
    pub key: &'static str,
    pub glyph: &'static str,
    pub aliases: &'static [&'static str],
}

/// Slash menu items, in the order the person asked for.
pub const SLASH_ITEMS: &[SlashItem] = &[
    SlashItem { key: "text", glyph: "Aa", aliases: &["paragraph", "plain", "texte"] },
    SlashItem { key: "page", glyph: "📄", aliases: &["subpage", "sous-page"] },
    SlashItem { key: "todo", glyph: "☑", aliases: &["todo", "checkbox", "task", "tache", "[]"] },
    SlashItem { key: "h1", glyph: "H1", aliases: &["h1", "#", "title", "titre"] },
    SlashItem { key: "h2", glyph: "H2", aliases: &["h2", "##", "subtitle"] },
    SlashItem { key: "h3", glyph: "H3", aliases: &["h3", "###"] },
    SlashItem { key: "bulleted", glyph: "•", aliases: &["bullet", "ul", "puce", "-"] },
    SlashItem { key: "numbered", glyph: "1.", aliases: &["number", "ol", "numero", "1."] },
    SlashItem { key: "toggle", glyph: "▸", aliases: &["toggle", "collapse", "details", "deplier"] },
    SlashItem { key: "quote", glyph: "❝", aliases: &["quote", "blockquote", "citation"] },
    SlashItem { key: "divider", glyph: "—", aliases: &["divider", "hr", "line", "separateur", "---"] },
    SlashItem {
        key: "toc",
        glyph: "≡",
        aliases: &["toc", "contents", "table of contents", "sommaire", "table des matieres"],
    },
    SlashItem { key: "columns2", glyph: "▥", aliases: &["columns", "2 columns", "colonnes", "two columns"] },
    SlashItem { key: "columns3", glyph: "▥", aliases: &["3 columns", "three columns", "trois colonnes"] },
    SlashItem { key: "group_toggle", glyph: "▾", aliases: &["group", "section", "grouper", "wrap"] },
    SlashItem { key: "group_callout", glyph: "▣", aliases: &["group", "frame", "encadrer", "wrap"] },
    SlashItem { key: "callout", glyph: "💡", aliases: &["callout", "note", "tip", "encadre"] },
    SlashItem { key: "code", glyph: "</>", aliases: &["code", "snippet", "```"] },
    SlashItem { key: "link", glyph: "↗", aliases: &["link", "mention", "lien", "[["] },
    SlashItem { key: "date_short", glyph: "📅", aliases: &["date", "today", "aujourd'hui", "jour"] },
    SlashItem { key: "date_iso", glyph: "📅", aliases: &["date", "today", "iso", "aujourd'hui"] },
    SlashItem { key: "date_long", glyph: "📅", aliases: &["date", "today", "long", "aujourd'hui"] },
    SlashItem { key: "date_full", glyph: "📅", aliases: &["date", "today", "weekday", "jour", "aujourd'hui"] },
    SlashItem { key: "date_time", glyph: "🕒", aliases: &["date", "time", "now", "heure", "maintenant"] },
    SlashItem {
        key: "template",
        glyph: "📋",
        aliases: &["template", "modele", "modèle", "minutes", "compte rendu", "meeting minutes", "cr"],
    },
    SlashItem { key: "image", glyph: "🖼", aliases: &["image", "picture", "photo", "img"] },
    SlashItem { key: "file", glyph: "📎", aliases: &["file", "attachment", "pdf", "fichier"] },
    SlashItem {
        key: "mermaid",
        glyph: "⌬",
        aliases: &["mermaid", "diagram", "flowchart", "graph", "diagramme", "schema", "schéma"],
    },
    SlashItem {
        key: "equation",
        glyph: "∑",
        aliases: &["equation", "math", "latex", "tex", "maths", "équation", "formule"],
    },
    SlashItem { key: "bookmark", glyph: "🔖", aliases: &["bookmark", "link", "web", "url", "signet", "lien"] },
    SlashItem { key: "simple_table", glyph: "⊟", aliases: &["table", "grid", "tableau", "simple", "array"] },
    SlashItem { key: "table", glyph: "▦", aliases: &["table", "database", "db", "tableau"] },
    SlashItem { key: "board", glyph: "▥", aliases: &["board", "kanban", "database"] },
    SlashItem { key: "list", glyph: "☰", aliases: &["list", "database", "liste"] },
    SlashItem { key: "dbview", glyph: "⊞", aliases: &["inline", "embed", "database", "view", "base", "vue"] },
    SlashItem {
        key: "meeting",
        glyph: "🎙",
        aliases: &[
            "meeting",
            "meet",
            "record",
            "transcribe",
            "transcript",
            "notes",
            "reunion",
            "enregistrer",
            "transcription",
        ],
    },
];

/// Items that can turn an existing block into another type.
const TURN_INTO: &[&str] = &[
    "text",
    "todo",
    "h1",
    "h2",
    "h3",
    "bulleted",
    "numbered",
    "toggle",
    "quote",
    "callout",
    "code",
    "group_toggle",
    "group_callout",
];

pub fn filter_items(query: &str, mode: &SlashMode) -> Vec<&'static SlashItem> {
    let mut scored: Vec<(i32, usize, &'static SlashItem)> = SLASH_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            if it.key.starts_with("group_") {
                *mode == SlashMode::TurnInto
            } else {
                *mode == SlashMode::Insert || TURN_INTO.contains(&it.key)
            }
        })
        .filter_map(|(i, it)| {
            if query.is_empty() {
                // the other date formats show up once "date" is typed
                return (!matches!(it.key, "date_iso" | "date_long" | "date_full" | "date_time")).then_some((0, i, it));
            }
            let (en, fr) = both(&format!("blk_{}", it.key));
            let best =
                [en, fr].into_iter().chain(it.aliases.iter().copied()).filter_map(|n| fuzzy_score(query, n)).max()?;
            (best >= 150).then_some((best, i, it))
        })
        .collect();
    if !query.is_empty() {
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    scored.into_iter().map(|(_, _, it)| it).collect()
}

pub(super) fn kind_for(key: &str) -> Option<BlockKind> {
    Some(match key {
        "text" => BlockKind::Paragraph,
        "todo" => BlockKind::Todo { checked: false },
        "h1" => BlockKind::Heading(1),
        "h2" => BlockKind::Heading(2),
        "h3" => BlockKind::Heading(3),
        "bulleted" => BlockKind::Bulleted,
        "numbered" => BlockKind::Numbered,
        "toggle" => BlockKind::Toggle { open: true },
        "quote" => BlockKind::Quote,
        "callout" => BlockKind::Callout { icon: "💡".into() },
        "code" => BlockKind::Code { lang: String::new() },
        "mermaid" => BlockKind::Code { lang: "mermaid".into() },
        "divider" => BlockKind::Divider,
        "toc" => BlockKind::Toc,
        "meeting" => BlockKind::Meeting,
        "dbview" => BlockKind::Database,
        "simple_table" => BlockKind::Table,
        "bookmark" => BlockKind::Bookmark { url: String::new() },
        "equation" => BlockKind::Equation,
        _ => return None,
    })
}

/// The `/query` the caret is right after, if any: (char index of '/', query).
pub fn slash_query(text: &str, caret: usize) -> Option<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    if caret > chars.len() {
        return None;
    }
    let mut i = caret;
    while i > 0 {
        i -= 1;
        let c = chars[i];
        if c == '/' {
            if i == 0 || chars[i - 1].is_whitespace() {
                let q: String = chars[i + 1..caret].iter().collect();
                if q.chars().count() <= 24 {
                    return Some((i, q));
                }
            }
            return None;
        }
        if c.is_whitespace() {
            return None;
        }
    }
    None
}
