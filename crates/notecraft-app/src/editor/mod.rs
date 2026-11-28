//! The block editor: renders a page's blocks, handles typing, the slash
//! menu, Markdown shortcuts, keyboard shortcuts, block selection, drag and
//! drop reordering and undo/redo. Anything that needs the workspace
//! (creating pages, picking files, navigating) is returned as an [`Action`].

use crate::i18n::{tr, Lang};
use crate::theme::{self, Palette};
use crate::widgets;
use eframe::egui::{
    self, text::CCursor, text::CCursorRange, text::LayoutJob, Align2, Color32, CornerRadius, FontId, Id, Key,
    KeyboardShortcut, Modifiers, Rect, RichText, Sense, Stroke, TextFormat, Ui, Vec2,
};
use notecraft_core::blocks::{self, Block, BlockKind, MAX_DEPTH};
use notecraft_core::database::ViewKind;
use notecraft_core::history::History;
use notecraft_core::inline;
use notecraft_core::DocId;
use slash_items::kind_for;
pub use slash_items::{filter_items, slash_query, SLASH_ITEMS};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use text_paint::{base_for, inline_job, paint_underlines, placeholder, Base};

/// Where to put the cursor when focusing a block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Caret {
    Start,
    End,
    At(usize),
    Select(usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlashMode {
    Insert,
    TurnInto,
}

#[derive(Debug, Clone)]
pub struct Slash {
    pub block: u64,
    /// Char index of the `/` (Insert mode).
    pub start: usize,
    pub query: String,
    pub selected: usize,
    pub mode: SlashMode,
}

/// Something the app must do with the workspace.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Navigate(DocId),
    /// A `[[mention]]` whose page does not exist yet.
    CreateMentioned(String),
    OpenUrl(String),
    OpenFile(String),
    /// Create a sub-page and put a link to it at `index` (replacing an empty block when `replace`).
    CreateSubpage {
        index: usize,
        replace: bool,
    },
    /// Choose a page to link; insert `[[title]]` into `block` at char `at`.
    LinkToPage {
        block: u64,
        at: usize,
    },
    /// Choose a file; insert an image or file block at `index`.
    PickFile {
        index: usize,
        replace: bool,
        image: bool,
    },
    CreateDatabase {
        index: usize,
        replace: bool,
        kind: ViewKind,
    },
    /// Something asked on the card of a meeting block.
    Meeting(crate::meeting::MeetingAction),
    /// Solve the equation in this block, step by step.
    SolveEquation(u64),
    /// Pick a template page and insert its blocks here.
    InsertTemplate {
        index: usize,
        replace: bool,
    },
}

/// Today's date (or date and time) in the format of a `date_*` slash key, in the language of the app.
pub fn date_text(key: &str, fr: bool) -> String {
    const MONTHS_FR: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    const MONTHS_EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const DAYS_FR: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
    const DAYS_EN: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    let now = jiff::Zoned::now();
    let (y, m, d) = (now.year(), now.month(), now.day());
    let mi = (m.clamp(1, 12) - 1) as usize;
    let wi = now.weekday().to_monday_zero_offset() as usize % 7;
    match key {
        "date_iso" => format!("{y}-{m:02}-{d:02}"),
        "date_long" if fr => format!("{d} {} {y}", MONTHS_FR[mi]),
        "date_long" => format!("{} {d}, {y}", MONTHS_EN[mi]),
        "date_full" if fr => format!("{} {d} {} {y}", DAYS_FR[wi], MONTHS_FR[mi]),
        "date_full" => format!("{}, {} {d}, {y}", DAYS_EN[wi], MONTHS_EN[mi]),
        "date_time" => format!("{d:02}.{m:02}.{y} {:02}:{:02}", now.hour(), now.minute()),
        _ => format!("{d:02}.{m:02}.{y}"),
    }
}

/// The second line of a slash menu entry: a date shows today's date in its format.
pub fn slash_desc(lang: Lang, key: &str) -> String {
    if key.starts_with("date_") {
        date_text(key, lang == Lang::Fr)
    } else {
        tr(lang, &format!("desc_{key}")).to_string()
    }
}

#[derive(Default)]
pub struct Output {
    pub changed: bool,
    pub actions: Vec<Action>,
}

/// What the editor needs to know about the outside world.
pub struct Env<'a> {
    pub pal: &'a Palette,
    pub lang: Lang,
    pub root: &'a Path,
    /// lowercased title → (doc, icon)
    pub titles: &'a HashMap<String, (DocId, String)>,
    /// Key of the document (for history and widget ids).
    pub key: &'a str,
    /// State of meeting recording and notes, for the card of a `/meeting` block.
    pub meeting: &'a crate::meeting::View,
    /// Titles of the databases a `/database` block can show.
    pub db_titles: &'a [String],
    /// Draws the database with this title inside the page (database id returned by the title lookup).
    pub embed: &'a dyn Fn(&mut Ui, u64, &str),
}

#[derive(Clone, PartialEq, Debug)]
struct DragBlock(u64);

#[derive(Debug, Clone)]
struct LinkCard {
    bid: u64,
    pos: egui::Pos2,
    url: String,
    caret: usize,
}

/// Byte offsets (label start, address start, address end) of the `[label](address)` link under a caret.
fn link_target_at(text: &str, char_pos: usize) -> Option<(usize, usize, usize)> {
    let at = inline::char_offset_to_byte(text, char_pos);
    let seg = inline::parse(text)
        .into_iter()
        .find(|s| !s.marker && s.style.link.is_some() && (s.range.contains(&at) || s.range.end == at))?;
    let rest = text.get(seg.range.end..)?.strip_prefix("](")?;
    let close = rest.find(')')?;
    Some((seg.range.start, seg.range.end + 2, seg.range.end + 2 + close))
}

/// Make a typed address usable: `example.com` gets `https://`, `me@example.com` gets `mailto:`.
fn normalize_url(s: &str) -> String {
    let s = s.trim();
    let has_scheme = s.contains("://") || s.starts_with("mailto:") || s.starts_with("tel:");
    if s.is_empty() || has_scheme || s.starts_with(['/', '.', '#']) || s.contains(' ') {
        s.to_string()
    } else if s.contains('@') && !s.contains('/') {
        format!("mailto:{s}")
    } else if s.split('/').next().is_some_and(|host| host.contains('.')) {
        format!("https://{s}")
    } else {
        s.to_string()
    }
}

pub struct EditorState {
    /// The `date_*` slash key last used, for Cmd+Shift+D.
    pub date_key: &'static str,
    histories: HashMap<String, History<Vec<Block>>>,
    pub focus: Option<(u64, Caret)>,
    pub slash: Option<Slash>,
    /// Block chosen with Esc (block selection).
    pub selected: Option<u64>,
    /// Other end of a multi-block selection (Shift+Up/Down); `selected` is the moving end.
    sel_anchor: Option<u64>,
    focused_block: Option<u64>,
    /// Page titles found for bookmarks: (block, address, title).
    title_tx: std::sync::mpsc::Sender<(u64, String, String)>,
    title_rx: std::sync::mpsc::Receiver<(u64, String, String)>,
    /// Address being typed in each empty bookmark block.
    bookmark_input: HashMap<u64, String>,
    at_first_row: bool,
    at_last_row: bool,
    /// Index of the block under the pointer (dropped files go after it).
    pub insert_hint: Option<usize>,
    /// Index of the block that holds (or last held) the text caret.
    caret_hint: Option<usize>,
    /// Image whose caption field is open (empty captions are hidden otherwise).
    caption_open: Option<u64>,
    /// Image bar of the last frame: (image block, rectangle), so it stays while the pointer is on it.
    img_bar: Option<(u64, Rect)>,
    /// First pointer position seen while files hover over the window, and whether it moved since:
    /// macOS sends no pointer moves during a drag from the Finder, so a stale position must not decide.
    hover_start: Option<Option<egui::Pos2>>,
    drop_by_pointer: bool,
    /// Heading to scroll to (clicked in a table of contents).
    toc_target: Option<u64>,
    /// Last text selection in a block: (block, start char, end char). Used by AI actions.
    pub last_selection: Option<(u64, usize, usize)>,
    /// Link being added with ⌘K: (block, start, end, URL typed so far).
    link_edit: Option<(u64, usize, usize, String)>,
    /// When editing an existing link: byte offsets in the block text of its label start, and of its
    /// address (start, end).
    link_replace: Option<(usize, usize, usize)>,
    /// Column to keep when an arrow key moves the caret into the neighbouring block:
    /// (block, x in the text, wants the last row, frame of the request).
    goal: Option<(u64, f32, bool, u64)>,
    caret_x: f32,
    frame: u64,
    /// Block where a mouse drag started (dragging into another block selects whole blocks).
    drag_origin: Option<u64>,
    /// A focus request held back a few frames: (block, caret, frame to apply it at).
    focus_later: Option<(u64, Caret, u64)>,
    /// Height of every row drawn so far (block id → points), so that rows far outside the window are skipped.
    heights: HashMap<u64, f32>,
    /// The page and width the heights were measured for.
    heights_for: (String, u32),
    /// Card shown under a link while the caret is inside it.
    link_card: Option<LinkCard>,
    card_rect: Option<Rect>,
    /// Formatting bar over a text selection: (block, start, end, screen position).
    fmt_bar: Option<(u64, usize, usize, egui::Pos2)>,
    /// The colour row of the formatting bar is open.
    fmt_colors: bool,
    bar_rect: Option<Rect>,
    block_menu: Option<u64>,
    /// Sub-menu open next to the block menu: 1 = turn into, 2 = colour.
    menu_sub: u8,
    menu_query: String,
    /// Frame in which the block menu was opened (a click that opens it must not also close it).
    block_menu_frame: u64,
    icon_menu: Option<u64>,
    dismissed_slash: Option<(u64, usize)>,
    /// A table cell (block, row, column) that must take the keyboard focus.
    table_focus: Option<(u64, usize, usize)>,
    /// Selected cells of a simple table: block, anchor (row, column), head (row, column).
    table_sel: Option<TableSel>,
    /// Cell where the mouse went down (a drag to another cell selects the cells in between).
    table_press: Option<(u64, usize, usize)>,
    /// Last cell that had the keyboard focus (the anchor of a shift-click).
    table_last: Option<(u64, usize, usize)>,
    /// Text selected inside a cell (block, row, column, from, to), kept while the format bar is used.
    table_range: Option<(u64, usize, usize, usize, usize)>,
    /// Selection to put back in the cell that takes the focus next.
    table_range_set: Option<(usize, usize)>,
    /// Open palette of the table bar: 1 = text colour, 2 = cell fill.
    table_menu: u8,
    /// The table whose format bar is showing (the bar and its menu are shared by all tables of a page).
    table_bar_owner: Option<u64>,
    table_bar_rect: Option<Rect>,
    pending_shortcut: Option<(u64, BlockKind, usize)>,
    start: Instant,
}

impl Default for EditorState {
    fn default() -> Self {
        let (title_tx, title_rx) = std::sync::mpsc::channel();
        EditorState {
            date_key: "date_short",
            title_tx,
            title_rx,
            bookmark_input: HashMap::new(),
            histories: HashMap::new(),
            focus: None,
            slash: None,
            selected: None,
            sel_anchor: None,
            focused_block: None,
            at_first_row: true,
            at_last_row: true,
            insert_hint: None,
            caret_hint: None,
            caption_open: None,
            img_bar: None,
            hover_start: None,
            drop_by_pointer: false,
            toc_target: None,
            last_selection: None,
            link_edit: None,
            link_replace: None,
            goal: None,
            caret_x: 0.0,
            frame: 0,
            drag_origin: None,
            focus_later: None,
            heights: HashMap::new(),
            heights_for: (String::new(), 0),
            link_card: None,
            card_rect: None,
            fmt_bar: None,
            fmt_colors: false,
            bar_rect: None,
            block_menu: None,
            menu_sub: 0,
            menu_query: String::new(),
            block_menu_frame: 0,
            icon_menu: None,
            dismissed_slash: None,
            table_focus: None,
            table_sel: None,
            table_press: None,
            table_last: None,
            table_range: None,
            table_range_set: None,
            table_menu: 0,
            table_bar_owner: None,
            table_bar_rect: None,
            pending_shortcut: None,
            start: Instant::now(),
        }
    }
}

const MERMAID_SAMPLE: &str =
    "graph TD\n  A[Start] --> B{Choice}\n  B -->|yes| C[Done]\n  B -->|no| D[Retry]\n  D --> B";

/// A laid out formula or diagram, kept between frames and rebuilt only when its text
/// (or the screen scale) changes, so a page with many of them stays smooth.
fn cached_layout<T: Send + Sync + 'static>(
    ctx: &egui::Context,
    id: Id,
    text: &str,
    build: impl FnOnce() -> T,
) -> Arc<T> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    ctx.pixels_per_point().to_bits().hash(&mut h);
    let key = h.finish();
    if let Some((k, v)) = ctx.data(|d| d.get_temp::<(u64, Arc<T>)>(id)) {
        if k == key {
            return v;
        }
    }
    let v = Arc::new(build());
    ctx.data_mut(|d| d.insert_temp(id, (key, v.clone())));
    v
}

/// The drawn diagram above a Mermaid code block (flowcharts only; other types stay plain code).
fn mermaid_view(ui: &mut Ui, bid: u64, src: &str, pal: &Palette, lang: Lang) {
    let drawn = cached_layout(ui.ctx(), Id::unique(("nc-mermaid-layout", bid)), src, || {
        crate::mermaid_more::prepare(ui.painter(), src)
    });
    let Some(drawn) = drawn.as_ref().as_ref() else {
        ui.label(RichText::new(tr(lang, "mermaid_hint")).size(11.5).color(pal.text_faint));
        return;
    };
    let avail = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, drawn.size().y + 4.0), Sense::hover());
    // centred when it fits, clipped to the block when it is wider
    let origin = egui::pos2((rect.center().x - drawn.size().x / 2.0).max(rect.min.x), rect.min.y);
    let painter = ui.painter().with_clip_rect(rect);
    let colors = crate::mermaid::Colors {
        ink: pal.text,
        fill: pal.bg,
        line: pal.text_secondary,
        soft: pal.text_secondary,
        bg: pal.code_bg,
    };
    drawn.draw(&painter, origin, &colors);
}

// ------------------------------------------------------------ text helpers

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn split_at_char(s: &str, ci: usize) -> (String, String) {
    let b = inline::char_offset_to_byte(s, ci);
    (s[..b].to_string(), s[b..].to_string())
}

fn remove_chars(s: &str, from: usize, to: usize) -> String {
    let a = inline::char_offset_to_byte(s, from);
    let b = inline::char_offset_to_byte(s, to);
    format!("{}{}", &s[..a], &s[b..])
}

pub fn insert_at_char(s: &str, at: usize, ins: &str) -> String {
    let a = inline::char_offset_to_byte(s, at);
    format!("{}{}{}", &s[..a], ins, &s[a..])
}

/// A small text menu without frame: the current choice and a caret, the other choices when opened.
fn quiet_menu(
    ui: &mut Ui,
    pal: &theme::Palette,
    label: &str,
    items: &[(String, String)],
    current: &str,
    tip: &str,
) -> Option<String> {
    let mut pick = None;
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        let r = ui.menu_button(RichText::new(format!("{label} \u{25be}")).size(12.5).color(pal.text_secondary), |ui| {
            for (id, l) in items {
                if ui.selectable_label(current == id, l).clicked() {
                    pick = Some(id.clone());
                    ui.close();
                }
            }
        });
        r.response.on_hover_text(tip);
    });
    pick
}

fn text_id(block: u64) -> Id {
    Id::unique(("nc-block", block))
}

fn cursor_range(ctx: &egui::Context, id: Id) -> Option<(usize, usize)> {
    let st = egui::text_edit::TextEditState::load(ctx, id)?;
    let r = st.cursor.char_range()?;
    let a = r.primary.index.0;
    let b = r.secondary.index.0;
    Some((a.min(b), a.max(b)))
}

/// Where the moving end of the selection (the caret itself when nothing is selected) sits.
fn cursor_head(ctx: &egui::Context, id: Id) -> Option<usize> {
    let st = egui::text_edit::TextEditState::load(ctx, id)?;
    Some(st.cursor.char_range()?.primary.index.0)
}

/// Where the fixed end of the selection sits.
fn cursor_anchor(ctx: &egui::Context, id: Id) -> Option<usize> {
    let st = egui::text_edit::TextEditState::load(ctx, id)?;
    Some(st.cursor.char_range()?.secondary.index.0)
}

fn set_cursor(ctx: &egui::Context, id: Id, caret: Caret, len: usize) {
    let mut st = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let range = match caret {
        Caret::Start => CCursorRange::one(CCursor::new(0)),
        Caret::End => CCursorRange::one(CCursor::new(len)),
        Caret::At(i) => CCursorRange::one(CCursor::new(i.min(len))),
        Caret::Select(a, b) => CCursorRange::two(CCursor::new(a.min(len)), CCursor::new(b.min(len))),
    };
    st.cursor.set_char_range(Some(range));
    st.store(ctx, id);
}

/// Selected cells of a table: block, anchor and head as (row, column).
type TableSel = (u64, (usize, usize), (usize, usize));

/// The paragraph is exactly one `[[mention]]` → shown as a page link block.
fn sole_mention(text: &str) -> Option<&str> {
    let t = text.trim();
    let inner = t.strip_prefix("[[")?.strip_suffix("]]")?;
    (!inner.contains("]]") && !inner.contains("[[") && !inner.is_empty()).then_some(inner)
}

/// Where a block is drawn: its index in the page, the width of its text and the environment.
struct BlockCx<'a> {
    i: usize,
    width: f32,
    env: &'a Env<'a>,
}

/// Where a block drawing function reports what happened (deferred edits and the output of the editor).
struct Sink<'a> {
    deferred: &'a mut Vec<Deferred>,
    out: &'a mut Output,
}

#[derive(Debug)]
struct RowCtx {
    selected_range: Option<(usize, usize)>,
    dragging: bool,
    gutter: f32,
    hidden: Vec<bool>,
}

enum Deferred {
    AddColumn(usize),
    UnwrapColumns(usize),
    ToggleCheck(usize),
    ToggleOpen(usize),
    Shortcut(usize, BlockKind, usize),
    PlusAfter(usize),
    AppendParagraph,
    SetCalloutIcon(usize, String),
    Slash(usize, &'static str),
    TurnInto(usize, &'static str),
    Delete(usize),
    /// Block, colour name (empty to clear), and whether it is the background.
    SetColor(usize, String, bool),
    Duplicate(usize),
    Move(usize, bool),
    Open(String),
}

// ------------------------------------------------------------------- editor

/// When block `i` belongs to a callout (as a child or grandchild): the callout's index, and whether `i` ends the group.
fn callout_group(blocks: &[Block], i: usize) -> Option<(usize, bool)> {
    let mut cur = i;
    let callout = loop {
        let d = blocks[cur].depth;
        if d == 0 {
            return None;
        }
        let parent = (0..cur).rev().find(|&j| blocks[j].depth < d)?;
        if matches!(blocks[parent].kind, BlockKind::Callout { .. }) {
            break parent;
        }
        cur = parent;
    };
    let cd = blocks[callout].depth;
    let last = blocks.get(i + 1).is_none_or(|b| b.depth <= cd);
    Some((callout, last))
}

/// Number of characters a cell shows: inline markup (`**`, `<span ...>`, link targets) takes no room.
fn visible_len(t: &str) -> usize {
    let mut n = 0;
    let mut in_tag = false;
    let mut in_url = false;
    let mut prev = ' ';
    for ch in t.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            '(' if prev == ']' => in_url = true,
            ')' if in_url => in_url = false,
            _ if in_url => {}
            '*' | '`' | '~' => {}
            _ => n += 1,
        }
        prev = ch;
    }
    n
}

impl EditorState {
    fn now_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    fn history(&mut self, key: &str) -> &mut History<Vec<Block>> {
        self.histories.entry(key.to_string()).or_default()
    }

    /// Record the state before a change (structural changes get their own step).
    pub fn record(&mut self, key: &str, before: &[Block], group: Option<&str>) {
        let now = self.now_ms();
        self.history(key).record(&before.to_vec(), group, now);
    }

    pub fn undo(&mut self, key: &str, blocks: &mut Vec<Block>) -> bool {
        let cur = blocks.clone();
        if let Some(prev) = self.history(key).undo(&cur) {
            *blocks = prev;
            self.slash = None;
            self.focus_after_history(&cur, blocks);
            return true;
        }
        false
    }

    pub fn redo(&mut self, key: &str, blocks: &mut Vec<Block>) -> bool {
        let cur = blocks.clone();
        if let Some(next) = self.history(key).redo(&cur) {
            *blocks = next;
            self.slash = None;
            self.focus_after_history(&cur, blocks);
            return true;
        }
        false
    }

    /// After undo/redo, put the caret in the block that changed.
    fn focus_after_history(&mut self, before: &[Block], after: &[Block]) {
        let changed = after.iter().find(|b| !before.iter().any(|o| o == *b)).or_else(|| after.last());
        if let Some(b) = changed {
            if b.kind.has_rich_text() || matches!(b.kind, BlockKind::Code { .. }) {
                self.focus = Some((b.id, Caret::End));
            }
        }
    }

    /// Insert text into a block at a char position (used after "Link to page").
    pub fn insert_text(&mut self, key: &str, blocks: &mut [Block], block: u64, at: usize, text: &str) -> bool {
        let before = blocks.to_vec();
        if let Some(b) = blocks.iter_mut().find(|b| b.id == block) {
            let at = at.min(char_len(&b.text));
            b.text = insert_at_char(&b.text, at, text);
            self.focus = Some((block, Caret::At(at + char_len(text))));
            self.record(key, &before, None);
            return true;
        }
        false
    }

    pub fn focus_first(&mut self, blocks: &[Block]) {
        if let Some(b) = blocks.iter().find(|b| b.kind.has_rich_text()) {
            self.focus = Some((b.id, Caret::Start));
        }
    }

    pub fn reset_for_doc(&mut self) {
        self.slash = None;
        self.selected = None;
        self.sel_anchor = None;
        self.focused_block = None;
        self.block_menu = None;
        self.icon_menu = None;
    }

    /// Show the editor. `blocks` is edited in place.
    pub fn show(&mut self, ui: &mut Ui, blocks: &mut Vec<Block>, env: &Env) -> Output {
        let mut out = Output::default();
        if blocks.is_empty() {
            blocks.push(Block::paragraph(""));
        }
        let ctx = ui.ctx().clone();
        self.frame = ctx.cumulative_frame_nr();
        while let Ok((bid, url, title)) = self.title_rx.try_recv() {
            if let Some(b) = blocks.iter_mut().find(|b| b.id == bid) {
                if matches!(&b.kind, BlockKind::Bookmark { url: u } if *u == url) {
                    b.text = title;
                    out.changed = true;
                }
            }
        }
        // Esc in the slash menu: close it, keep the text and keep the caret in
        // the block (even if egui already released the focus for Escape).
        if let Some(slash) = self.slash.clone() {
            if slash.mode == SlashMode::Insert && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.dismissed_slash = Some((slash.block, slash.start));
                self.slash = None;
                if let Some(b) = blocks.iter().find(|b| b.id == slash.block) {
                    let caret = cursor_range(&ctx, text_id(b.id)).map(|c| c.1).unwrap_or(char_len(&b.text));
                    self.focus = Some((b.id, Caret::At(caret)));
                }
            }
        }
        // egui moves the focus on its own when an arrow key arrives while no field has it, which would
        // undo a caret placed in the same frame; so the caret is placed a few frames later
        if let Some((b, c, at)) = self.focus_later {
            if self.frame >= at {
                self.focus = Some((b, c));
                self.focus_later = None;
            }
            ctx.request_repaint();
        }
        let focused_idx = self
            .focused_block
            .filter(|b| ctx.memory(|m| m.has_focus(text_id(*b))))
            .and_then(|b| blocks.iter().position(|x| x.id == b));
        if focused_idx.is_none() {
            self.focused_block = None;
            if self.slash.as_ref().is_some_and(|s| s.mode == SlashMode::Insert) {
                self.slash = None;
            }
        }
        if focused_idx.is_some() {
            self.caret_hint = focused_idx;
        }
        if let Some(i) = focused_idx {
            self.handle_keys(&ctx, blocks, i, env, &mut out);
        } else if self.selected.is_some() {
            self.handle_selection_keys(&ctx, blocks, env, &mut out);
        }

        let selected_range = self.selected_range(blocks);
        let hidden = blocks::hidden_by_toggles(blocks);
        let mut rects: Vec<(usize, Rect)> = Vec::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let full_width = ui.available_width();
        let marker = (env.key.to_string(), full_width.round() as u32);
        if self.heights_for != marker {
            self.heights.clear();
            self.heights_for = marker;
        }
        let gutter = 46.0;
        let dragging = egui::DragAndDrop::has_payload_of_type::<DragBlock>(&ctx);

        let rc = RowCtx { selected_range, dragging, gutter, hidden };
        let mut i = 0;
        while i < blocks.len() {
            if rc.hidden[i] {
                i += 1;
                continue;
            }
            if let Some((end, firsts)) = blocks::column_layout(blocks, i) {
                self.show_columns(
                    ui,
                    blocks,
                    i,
                    end,
                    &firsts,
                    full_width,
                    &rc,
                    env,
                    &mut rects,
                    &mut deferred,
                    &mut out,
                );
                i = end + 1;
                continue;
            }
            // rows far outside the window are not drawn again: a long page keeps a short frame time
            let view = ui.clip_rect().expand2(Vec2::new(0.0, 900.0));
            let before = ui.cursor().min.y;
            let known = self.heights.get(&blocks[i].id).copied();
            match known {
                Some(h) if self.can_skip_row(blocks, i, &rc) && (before + h < view.min.y || before > view.max.y) => {
                    ui.add_space(h.max(0.0));
                    let r =
                        Rect::from_min_size(egui::pos2(ui.min_rect().min.x, before), Vec2::new(full_width, h.max(1.0)));
                    rects.push((i, r));
                }
                _ => {
                    self.show_row(ui, blocks, i, full_width, &rc, env, &mut rects, &mut deferred, &mut out);
                    self.heights.insert(blocks[i].id, ui.cursor().min.y - before);
                }
            }
            i += 1;
        }

        // click below the last block: focus it or add a paragraph
        let rest = ui.available_rect_before_wrap();
        let tail = Rect::from_min_size(rest.min, Vec2::new(full_width, 160.0));
        let tail_resp = ui.allocate_rect(tail, Sense::click());
        if tail_resp.clicked() {
            match blocks.last() {
                Some(b) if b.kind == BlockKind::Paragraph && b.text.is_empty() => self.focus = Some((b.id, Caret::End)),
                _ => deferred.push(Deferred::AppendParagraph),
            }
        }

        // drag and drop of blocks: between two blocks, or on the right edge of one to make columns
        if let Some(payload) = egui::DragAndDrop::payload::<DragBlock>(&ctx) {
            if let Some(pointer) = ctx.pointer_interact_pos() {
                let from_idx = blocks.iter().position(|b| b.id == payload.0);
                let moving =
                    from_idx.map(|f| self.multi_range(blocks, f).unwrap_or((f, blocks::subtree_end(blocks, f))));
                // the right edge of a block (a column's edge in columns) is a side drop
                let side = rects
                    .iter()
                    .find(|(_, r)| {
                        pointer.y >= r.min.y
                            && pointer.y <= r.max.y
                            && pointer.x >= r.max.x - 80.0
                            && pointer.x <= r.max.x + 10.0
                    })
                    .map(|(i, r)| (*i, *r))
                    .filter(|(i, _)| {
                        moving.is_some_and(|(a, b)| !(a..b).contains(i))
                            && !matches!(
                                blocks[*i].kind,
                                BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd
                            )
                            && !moving.is_some_and(|(a, b)| b - a > 1)
                    });
                let target =
                    rects.iter().find(|(_, r)| pointer.y < r.center().y).map(|(i, _)| *i).unwrap_or(blocks.len());
                if let Some((_, r)) = side {
                    ui.painter().line_segment(
                        [egui::pos2(r.max.x - 2.0, r.min.y), egui::pos2(r.max.x - 2.0, r.max.y)],
                        Stroke::new(3.0, env.pal.drop),
                    );
                } else {
                    let y = rects
                        .iter()
                        .find(|(i, _)| *i == target)
                        .map(|(_, r)| r.min.y - 1.0)
                        .or_else(|| rects.last().map(|(_, r)| r.max.y + 1.0))
                        .unwrap_or(pointer.y);
                    let x0 = rects.first().map(|(_, r)| r.min.x + gutter).unwrap_or(pointer.x);
                    ui.painter().line_segment(
                        [egui::pos2(x0, y), egui::pos2(x0 + full_width - gutter, y)],
                        Stroke::new(3.0, env.pal.drop),
                    );
                }
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                if ctx.input(|i| i.pointer.any_released()) {
                    if let Some(from) = from_idx {
                        let before = blocks.clone();
                        let multi = self.multi_range(blocks, from);
                        let mut moved = None;
                        if let Some((onto, _)) = side {
                            moved = blocks::drop_beside(blocks, from, onto);
                        } else if let Some((lo, hi)) = multi {
                            blocks::move_range(blocks, lo, hi, target);
                            blocks::tidy_columns(blocks);
                        } else {
                            let at = blocks::move_block(blocks, from, target);
                            blocks::tidy_columns(blocks);
                            moved = blocks.get(at).map(|b| b.id);
                        }
                        if *blocks != before {
                            self.record(env.key, &before, None);
                            out.changed = true;
                            if multi.is_none() {
                                self.sel_anchor = None;
                                self.selected = moved;
                            }
                        }
                    }
                    egui::DragAndDrop::clear_payload(&ctx);
                }
            }
        }

        if let Some((bid, kind, strip)) = self.pending_shortcut.take() {
            if let Some(i) = blocks.iter().position(|b| b.id == bid) {
                deferred.push(Deferred::Shortcut(i, kind, strip));
            }
        }
        // files being dragged over the window: a line shows where they will land (after the block under the
        // pointer once it moves, otherwise after the block with the caret)
        let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering_files {
            let pos = ctx.input(|i| i.pointer.hover_pos());
            match self.hover_start {
                None => {
                    self.hover_start = Some(pos);
                    self.drop_by_pointer = false;
                }
                Some(p0) => {
                    if p0 != pos {
                        self.drop_by_pointer = true;
                    }
                }
            }
        } else if ctx.input(|i| i.raw.dropped_files.is_empty()) {
            self.hover_start = None;
        }
        if hovering_files {
            let last = blocks.len().saturating_sub(1);
            let at = self.drop_hint().unwrap_or(last).min(last);
            if let Some((_, r)) = rects.iter().find(|(j, _)| *j == at).or(rects.last()) {
                let y = r.max.y + 2.0;
                let x0 = r.min.x + gutter;
                let stroke = Stroke::new(2.5, env.pal.accent);
                ui.painter().hline(x0..=r.max.x, y, stroke);
                ui.painter().circle_filled(egui::pos2(x0, y), 4.0, env.pal.accent);
                ui.painter().text(
                    egui::pos2(x0 + 10.0, y + 4.0),
                    Align2::LEFT_TOP,
                    tr(env.lang, "drop_here"),
                    FontId::proportional(12.0),
                    env.pal.accent,
                );
            }
        }
        if self.slash.is_some() {
            self.slash_menu_ui(&ctx, blocks, &rects, env, &mut deferred);
        }
        self.drag_across_blocks(&ctx, blocks, &rects);
        if self.link_edit.is_some() {
            self.link_popup(&ctx, blocks, &rects, env, &mut out);
        } else {
            let over = |r: Option<Rect>| {
                r.is_some_and(|r| ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| r.expand(6.0).contains(p)))
            };
            if self.focused_block.is_none() {
                if !over(self.bar_rect) {
                    self.fmt_bar = None;
                }
                if !over(self.card_rect) {
                    self.link_card = None;
                }
            }
            if self.slash.is_none() {
                self.fmt_bar_ui(&ctx, blocks, env);
            }
            self.link_card_ui(&ctx, blocks, env, &mut out);
        }

        for d in deferred {
            self.apply_deferred(blocks, d, env, &mut out);
        }
        // a focus request made in this frame (the new line after Enter) is applied when its block is drawn,
        // which can be next frame: without a repaint, the next key would still go to the old block
        if self.focus.is_some_and(|(b, _)| blocks.iter().any(|x| x.id == b)) {
            ctx.request_repaint();
        }
        out
    }

    /// A row may be left out when it is far off screen and nothing asks for it: no focus or selection,
    /// no menu, no drag, and a plain kind of block (text, list, quote, code, divider).
    fn can_skip_row(&self, blocks: &[Block], i: usize, rc: &RowCtx) -> bool {
        if rc.dragging
            || rc.selected_range.is_some()
            || self.focus.is_some()
            || self.focus_later.is_some()
            || self.toc_target.is_some()
            || self.slash.is_some()
            || self.selected.is_some()
            || self.sel_anchor.is_some()
            || self.link_edit.is_some()
            || self.focused_block == Some(blocks[i].id)
            || self.table_focus.is_some()
            || self.block_menu.is_some()
        {
            return false;
        }
        matches!(
            blocks[i].kind,
            BlockKind::Paragraph
                | BlockKind::Heading(_)
                | BlockKind::Bulleted
                | BlockKind::Numbered
                | BlockKind::Todo { .. }
                | BlockKind::Quote
                | BlockKind::Divider
        ) && callout_group(blocks, i).is_none()
    }

    /// One block row: the block, its colour and selection backgrounds, and its "+" and handle.
    fn show_row(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        i: usize,
        full_width: f32,
        rc: &RowCtx,
        env: &Env,
        rects: &mut Vec<(usize, Rect)>,
        deferred: &mut Vec<Deferred>,
        out: &mut Output,
    ) {
        let ctx = ui.ctx().clone();
        let gutter = rc.gutter;
        if rc.hidden[i] {
            return;
        }
        let extra_top = match blocks[i].kind {
            BlockKind::Heading(1) => 14.0,
            BlockKind::Heading(2) => 10.0,
            BlockKind::Heading(3) => 6.0,
            _ => 1.0,
        };
        ui.add_space(extra_top);
        let top = ui.cursor().min;
        let grp_slot = ui.painter().add(egui::Shape::Noop);
        let bg_slot = ui.painter().add(egui::Shape::Noop);
        let sel_slot = ui.painter().add(egui::Shape::Noop);
        let indent = blocks[i].depth as f32 * 26.0;
        let in_callout = callout_group(blocks, i).is_some();
        let content_width = (full_width - gutter - indent - if in_callout { 18.0 } else { 0.0 }).max(60.0);
        let row = ui.horizontal_top(|ui| {
            ui.add_space(gutter + indent);
            self.block_ui(ui, blocks, i, content_width, env, deferred, out)
        });
        let row_rect =
            Rect::from_min_max(top, egui::pos2(top.x + full_width, row.response.rect.max.y.max(top.y + 20.0)));
        // children of a callout sit on its background
        if let Some((c, last)) = callout_group(blocks, i) {
            let gap = ui.spacing().item_spacing.y;
            let x0 = row_rect.min.x + gutter + blocks[c].depth as f32 * 26.0;
            let rect = Rect::from_min_max(
                egui::pos2(x0, row_rect.min.y - extra_top - gap),
                egui::pos2(row_rect.min.x + full_width, row_rect.max.y + if last { 8.0 } else { gap }),
            );
            let rounding = if last { CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 } } else { CornerRadius::ZERO };
            let mut shapes = Vec::new();
            if let Some((_, Some(bg))) = theme::block_color(&blocks[c].color, env.pal.dark) {
                let wide = Rect::from_min_max(egui::pos2(rect.min.x - 6.0, rect.min.y), rect.max);
                shapes.push(egui::epaint::RectShape::filled(wide, CornerRadius::ZERO, bg).into());
            }
            shapes.push(egui::epaint::RectShape::filled(rect, rounding, env.pal.callout_bg).into());
            ui.painter().set(grp_slot, egui::Shape::Vec(shapes));
            if last {
                ui.add_space(8.0);
            }
        }
        rects.push((i, row_rect));
        if self.toc_target == Some(blocks[i].id) {
            ui.scroll_to_rect(row_rect, Some(egui::Align::Center));
            self.toc_target = None;
        }
        if let Some((_, Some(bg))) = theme::block_color(&blocks[i].color, env.pal.dark) {
            let rect = Rect::from_min_max(
                egui::pos2(row_rect.min.x + gutter + indent - 6.0, row_rect.min.y - 1.0),
                egui::pos2(row_rect.max.x, row_rect.max.y + 1.0),
            );
            ui.painter().set(bg_slot, egui::epaint::RectShape::filled(rect, CornerRadius::same(4), bg));
        }
        if rc.selected_range.is_some_and(|(a, b)| (a..b).contains(&i)) {
            // painted into the slot reserved before the block, so the text stays on top
            let rect = Rect::from_min_max(
                egui::pos2(row_rect.min.x + gutter + indent - 4.0, row_rect.min.y - 1.0),
                egui::pos2(row_rect.max.x, row_rect.max.y + 1.0),
            );
            ui.painter()
                .set(sel_slot, egui::epaint::RectShape::filled(rect, CornerRadius::same(4), env.pal.accent_soft));
        }
        let hovered = ui.rect_contains_pointer(row_rect) && !rc.dragging;
        if hovered {
            self.insert_hint = Some(i);
        }
        if hovered || self.block_menu == Some(blocks[i].id) {
            let first_line_h = match blocks[i].kind {
                BlockKind::Heading(l) => theme::heading_size(l) * 1.5,
                BlockKind::Divider => row_rect.height(),
                BlockKind::Toc => 30.0,
                BlockKind::Meeting
                | BlockKind::Database
                | BlockKind::Table
                | BlockKind::Bookmark { .. }
                | BlockKind::Equation => 40.0,
                BlockKind::Image { .. } | BlockKind::File { .. } | BlockKind::Code { .. } => 30.0,
                BlockKind::Callout { .. } => 48.0,
                _ => theme::BODY_SIZE * 1.5,
            };
            let cy = row_rect.min.y + first_line_h.min(row_rect.height()) / 2.0;
            let x0 = row_rect.min.x + indent;
            let plus = Rect::from_center_size(egui::pos2(x0 + 12.0, cy), Vec2::new(20.0, 22.0));
            let handle = Rect::from_center_size(egui::pos2(x0 + 32.0, cy), Vec2::new(16.0, 22.0));
            let pr = ui.interact(plus, Id::unique(("nc-plus", blocks[i].id)), Sense::click());
            let hr = ui.interact(handle, Id::unique(("nc-handle", blocks[i].id)), Sense::click_and_drag());
            widgets::a11y(&pr, tr(env.lang, "a11y_add_block"));
            widgets::a11y(&hr, tr(env.lang, "a11y_block_menu"));
            let p = ui.painter();
            if pr.hovered() {
                p.rect_filled(plus, CornerRadius::same(4), env.pal.hover);
            }
            if hr.hovered() {
                p.rect_filled(handle, CornerRadius::same(4), env.pal.hover);
            }
            p.text(plus.center(), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), env.pal.text_faint);
            for dy in [-5.0f32, 0.0, 5.0] {
                for dx in [-2.5f32, 2.5] {
                    p.circle_filled(handle.center() + Vec2::new(dx, dy), 1.3, env.pal.text_faint);
                }
            }
            if pr.clicked() {
                deferred.push(Deferred::PlusAfter(i));
            }
            if hr.drag_started() {
                egui::DragAndDrop::set_payload(&ctx, DragBlock(blocks[i].id));
            }
            if hr.clicked() {
                self.block_menu = Some(blocks[i].id);
                self.block_menu_frame = ctx.cumulative_frame_nr();
                // the menu acts on the whole selection when this block is part of one
                if self.multi_range(blocks, i).is_none() {
                    self.sel_anchor = None;
                    self.selected = Some(blocks[i].id);
                }
                ctx.memory_mut(|m| m.surrender_focus(text_id(blocks[i].id)));
            }
        }
        if self.block_menu == Some(blocks[i].id) {
            let pos = egui::pos2(row_rect.min.x + indent + 24.0, row_rect.min.y + 24.0);
            self.block_menu_ui(&ctx, blocks, i, pos, env, deferred);
        }
    }

    /// A side-by-side layout: each column is laid out in its own child area.
    #[allow(clippy::too_many_arguments)]
    fn show_columns(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        start: usize,
        end: usize,
        firsts: &[usize],
        full_width: f32,
        rc: &RowCtx,
        env: &Env,
        rects: &mut Vec<(usize, Rect)>,
        deferred: &mut Vec<Deferred>,
        out: &mut Output,
    ) {
        let pal = env.pal;
        let n = firsts.len();
        // the controls only show while the pointer is over the block of columns; the strip keeps its
        // height so nothing jumps when they appear
        let hover_id = Id::unique(("columns-hover", blocks[start].id));
        let show_bar = ui.ctx().data(|d| d.get_temp::<bool>(hover_id)).unwrap_or(false);
        let region_min = ui.cursor().min;
        ui.horizontal(|ui| {
            ui.set_min_height(22.0);
            if show_bar {
                ui.add_space(rc.gutter);
                ui.label(RichText::new(format!("▥ {}", tr(env.lang, "columns"))).size(11.5).color(pal.text_faint));
                if n < 5 && widgets::icon_button(ui, "+", tr(env.lang, "add_column")).clicked() {
                    deferred.push(Deferred::AddColumn(start));
                }
                if widgets::icon_button(ui, "×", tr(env.lang, "unwrap_columns")).clicked() {
                    deferred.push(Deferred::UnwrapColumns(start));
                }
            }
        });
        let gap = 14.0;
        let col_w = ((full_width - gap * (n as f32 - 1.0)) / n as f32).max(120.0);
        let top = ui.cursor().min;
        let mut bottom = top.y;
        for (c, &first) in firsts.iter().enumerate() {
            let last = firsts.get(c + 1).map(|f| f - 1).unwrap_or(end);
            let x = top.x + c as f32 * (col_w + gap);
            let rect = Rect::from_min_size(egui::pos2(x, top.y), Vec2::new(col_w, 100_000.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
            for i in first..last {
                if !rc.hidden[i] {
                    self.show_row(&mut child, blocks, i, col_w, rc, env, rects, deferred, out);
                }
            }
            bottom = bottom.max(child.min_rect().max.y);
            if c > 0 {
                let sx = x - gap / 2.0;
                ui.painter().vline(sx, top.y..=bottom.max(top.y + 24.0), Stroke::new(1.0, pal.border));
            }
        }
        let region = Rect::from_min_max(region_min, egui::pos2(top.x + full_width, bottom + 6.0));
        let hovered = ui.rect_contains_pointer(region);
        if hovered != show_bar {
            ui.ctx().data_mut(|d| d.insert_temp(hover_id, hovered));
            ui.ctx().request_repaint();
        }
        ui.allocate_rect(Rect::from_min_max(top, egui::pos2(top.x + full_width, bottom + 6.0)), Sense::hover());
    }

    /// Dragging the mouse from one block into another selects whole blocks.
    fn drag_across_blocks(&mut self, ctx: &egui::Context, blocks: &[Block], rects: &[(usize, Rect)]) {
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.drag_origin = None;
            return;
        }
        let (Some(origin), Some(p)) = (self.drag_origin, ctx.input(|i| i.pointer.hover_pos())) else { return };
        let (Some(first), Some(last)) = (rects.first(), rects.last()) else { return };
        let Some(oi) = blocks.iter().position(|b| b.id == origin) else { return };
        let target = if p.y < first.1.min.y {
            first.0
        } else if p.y > last.1.max.y {
            last.0
        } else {
            match rects.iter().find(|(_, r)| p.y >= r.min.y && p.y <= r.max.y) {
                Some((j, _)) => *j,
                None => return,
            }
        };
        if target != oi {
            self.sel_anchor = Some(origin);
            self.selected = Some(blocks[target].id);
            self.last_selection = None;
            self.fmt_bar = None;
            self.link_card = None;
            ctx.memory_mut(|m| m.surrender_focus(text_id(origin)));
            self.focused_block = None;
            ctx.request_repaint();
        } else if self.selected.is_some() && self.focused_block.is_none() {
            self.selected = Some(origin);
            self.sel_anchor = Some(origin);
        }
    }

    /// Floating formatting bar over a text selection.
    fn fmt_bar_ui(&mut self, ctx: &egui::Context, blocks: &mut [Block], env: &Env) {
        let Some((bid, a, b, pos)) = self.fmt_bar else {
            self.bar_rect = None;
            self.fmt_colors = false;
            return;
        };
        let Some(i) = blocks.iter().position(|x| x.id == bid) else {
            self.fmt_bar = None;
            return;
        };
        let lang = env.lang;
        let mut style: Option<inline::StyleKind> = None;
        let mut link = false;
        let mut turn = false;
        let mut color_pick: Option<Option<&'static str>> = None;
        let mut toggle_colors = false;
        let colors_open = self.fmt_colors;
        let top = (pos.y - 40.0 - if colors_open { 32.0 } else { 0.0 }).max(4.0);
        let area = egui::Area::new(Id::unique("nc-fmt-bar"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2((pos.x - 8.0).max(4.0), top))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    if colors_open {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let r = ui.button("×").on_hover_text(tr(lang, "color_text_default"));
                            if r.clicked() {
                                color_pick = Some(None);
                            }
                            for name in blocks::BLOCK_COLORS {
                                let dot = env.pal.option_tokens(name).2;
                                let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                                let r = r.on_hover_text(tr(lang, &format!("color_{name}")));
                                let radius = if r.hovered() { 9.0 } else { 7.5 };
                                ui.painter().circle_filled(rect.center(), radius, dot);
                                if r.clicked() {
                                    color_pick = Some(Some(name));
                                }
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if ui.button("Aa ▾").on_hover_text(tr(lang, "fmt_turn_into")).clicked() {
                            turn = true;
                        }
                        ui.separator();
                        let items: [(RichText, inline::StyleKind, &str); 5] = [
                            (RichText::new("B").strong(), inline::StyleKind::Bold, "fmt_bold"),
                            (RichText::new("I").italics(), inline::StyleKind::Italic, "fmt_italic"),
                            (RichText::new("U").underline(), inline::StyleKind::Underline, "fmt_underline"),
                            (RichText::new("S").strikethrough(), inline::StyleKind::Strike, "fmt_strike"),
                            (RichText::new("</>").monospace(), inline::StyleKind::Code, "fmt_code"),
                        ];
                        for (label, kind, tip) in items {
                            let r = ui.button(label).on_hover_text(tr(lang, tip));
                            crate::widgets::a11y(&r, tr(lang, tip));
                            if r.clicked() {
                                style = Some(kind);
                            }
                        }
                        let r = ui
                            .button(RichText::new("A").color(env.pal.accent))
                            .on_hover_text(tr(lang, "color_text_title"));
                        crate::widgets::a11y(&r, tr(lang, "color_text_title"));
                        if r.clicked() {
                            toggle_colors = true;
                        }
                        ui.separator();
                        let r = ui.button("🔗").on_hover_text(tr(lang, "fmt_link"));
                        crate::widgets::a11y(&r, tr(lang, "fmt_link"));
                        if r.clicked() {
                            link = true;
                        }
                    });
                });
            });
        self.bar_rect = Some(area.response.rect);
        if toggle_colors {
            self.fmt_colors = !self.fmt_colors;
            ctx.request_repaint();
        }
        if let Some(pick) = color_pick {
            let before = blocks.to_vec();
            let (t, sel) = inline::set_color(&blocks[i].text, (a, b), pick);
            blocks[i].text = t;
            self.focus = Some((bid, Caret::Select(sel.0, sel.1)));
            self.record(env.key, &before, None);
            self.fmt_colors = false;
            ctx.request_repaint();
        } else if let Some(kind) = style {
            let before = blocks.to_vec();
            let (t, sel) = inline::toggle_style(&blocks[i].text, (a, b), kind);
            blocks[i].text = t;
            self.focus = Some((bid, Caret::Select(sel.0, sel.1)));
            self.record(env.key, &before, None);
            ctx.request_repaint();
        } else if link {
            self.link_edit = Some((bid, a, b, "https://".into()));
            self.link_replace = None;
            self.fmt_bar = None;
        } else if turn {
            self.slash =
                Some(Slash { block: bid, start: 0, query: String::new(), selected: 0, mode: SlashMode::TurnInto });
            self.focus = Some((bid, Caret::Select(a, b)));
        }
    }

    /// Small card under a link while the caret is in it: address, open, edit.
    fn link_card_ui(&mut self, ctx: &egui::Context, blocks: &[Block], env: &Env, out: &mut Output) {
        let Some(card) = self.link_card.clone() else {
            self.card_rect = None;
            return;
        };
        let lang = env.lang;
        let pal = env.pal;
        let (mut open, mut edit) = (false, false);
        let area = egui::Area::new(Id::unique("nc-link-card"))
            .order(egui::Order::Foreground)
            .fixed_pos(card.pos + egui::vec2(0.0, 6.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // the address gets a fixed share and is cut with "…"; the buttons keep their room
                        ui.allocate_ui_with_layout(
                            Vec2::new(260.0, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::Label::new(RichText::new(&card.url).color(pal.text_secondary)).truncate())
                                    .on_hover_text(&card.url);
                            },
                        );
                        ui.add_space(6.0);
                        if ui.button(tr(lang, "open")).clicked() {
                            open = true;
                        }
                        if ui.button(tr(lang, "link_edit")).clicked() {
                            edit = true;
                        }
                    });
                });
            });
        self.card_rect = Some(area.response.rect);
        if open {
            let url = normalize_url(&card.url);
            if url.contains("://") || url.starts_with("mailto:") {
                out.actions.push(Action::OpenUrl(url));
            } else {
                out.actions.push(Action::OpenFile(url));
            }
        } else if edit {
            if let Some(b) = blocks.iter().find(|b| b.id == card.bid) {
                let text = b.text.clone();
                if self.begin_link_edit(card.bid, &text, card.caret) {
                    self.link_card = None;
                }
            }
        }
    }

    // -------------------------------------------------------------- one block

    fn block_ui(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        i: usize,
        width: f32,
        env: &Env,
        deferred: &mut Vec<Deferred>,
        out: &mut Output,
    ) -> egui::Response {
        let pal = env.pal;
        let marker_w = 24.0;
        let kind = blocks[i].kind.clone();
        let bid = blocks[i].id;
        match &kind {
            BlockKind::Divider => {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::click());
                ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, pal.border));
                if resp.clicked() {
                    self.sel_anchor = None;
                    self.selected = Some(bid);
                }
                resp
            }
            BlockKind::ColumnsStart | BlockKind::ColumnBreak | BlockKind::ColumnsEnd => {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click());
                ui.painter().text(
                    rect.left_center(),
                    Align2::LEFT_CENTER,
                    format!("▥ {}", tr(env.lang, "columns")),
                    FontId::proportional(11.5),
                    pal.text_faint,
                );
                if resp.clicked() {
                    self.sel_anchor = None;
                    self.selected = Some(bid);
                }
                resp
            }
            BlockKind::Equation => self.equation_ui(
                ui,
                blocks,
                &BlockCx { i, width, env },
                &mut Sink { deferred: &mut *deferred, out: &mut *out },
            ),
            BlockKind::Bookmark { url } => self.bookmark_ui(
                ui,
                blocks,
                &BlockCx { i, width, env },
                &mut Sink { deferred: &mut *deferred, out: &mut *out },
                url,
            ),
            BlockKind::Table => self.table_ui(
                ui,
                blocks,
                &BlockCx { i, width, env },
                &mut Sink { deferred: &mut *deferred, out: &mut *out },
            ),
            BlockKind::Database => self.database_ui(
                ui,
                blocks,
                &BlockCx { i, width, env },
                &mut Sink { deferred: &mut *deferred, out: &mut *out },
            ),
            BlockKind::Meeting => meeting_card::meeting_card(ui, blocks, i, width, env, out),
            BlockKind::Toc => self.toc_ui(ui, blocks, &BlockCx { i, width, env }),
            BlockKind::Image { path } => self.image_ui(
                ui,
                blocks,
                &BlockCx { i, width, env },
                &mut Sink { deferred: &mut *deferred, out: &mut *out },
                path,
            ),
            BlockKind::File { path } => {
                let name = if blocks[i].text.is_empty() { path.clone() } else { blocks[i].text.clone() };
                let size = std::fs::metadata(env.root.join(path)).map(|m| m.len()).unwrap_or(0);
                let resp = ui
                    .add(
                        egui::Button::new(RichText::new(format!("📎  {name}")).size(15.0).color(pal.text))
                            .right_text(RichText::new(human_size(size)).size(12.0).color(pal.text_faint))
                            .fill(pal.code_bg)
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(width.min(440.0), 34.0)),
                    )
                    .on_hover_text(path.as_str());
                if resp.clicked() {
                    deferred.push(Deferred::Open(path.clone()));
                }
                if resp.secondary_clicked() {
                    self.sel_anchor = None;
                    self.selected = Some(bid);
                }
                resp
            }
            BlockKind::Code { lang } => self.code_ui(
                ui,
                blocks,
                &BlockCx { i, width, env },
                &mut Sink { deferred: &mut *deferred, out: &mut *out },
                lang,
            ),
            BlockKind::Callout { icon } => self.callout_ui(
                ui,
                blocks,
                &BlockCx { i, width, env },
                &mut Sink { deferred: &mut *deferred, out: &mut *out },
                icon,
            ),
            _ => {
                let r = ui.horizontal_top(|ui| {
                    let line_h = base_for(&kind, &blocks[i].color, pal).size * 1.5;
                    let mut text_width = width;
                    let gap = ui.spacing().item_spacing.x;
                    match &kind {
                        BlockKind::Bulleted => {
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(marker_w, line_h), Sense::hover());
                            // one simple dot at every level, a little lighter when nested
                            let colour = if blocks[i].depth == 0 { pal.text } else { pal.text_faint };
                            ui.painter().circle_filled(rect.center() + Vec2::new(0.0, 1.0), 2.6, colour);
                            text_width -= marker_w + gap;
                        }
                        BlockKind::Numbered => {
                            let n = blocks::numbered_index(blocks, i);
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(marker_w, line_h), Sense::hover());
                            ui.painter().text(
                                rect.right_center() - Vec2::new(3.0, 0.0),
                                Align2::RIGHT_CENTER,
                                number_label(n, blocks[i].depth),
                                FontId::proportional(theme::BODY_SIZE),
                                pal.text,
                            );
                            text_width -= marker_w + gap;
                        }
                        BlockKind::Todo { checked } => {
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(marker_w, line_h), Sense::click());
                            widgets::paint_checkbox(ui, rect, *checked, resp.hovered(), pal);
                            let checked_now = *checked;
                            resp.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::accesskit::Role::CheckBox,
                                    true,
                                    checked_now,
                                    tr(env.lang, "todo_placeholder"),
                                )
                            });
                            if resp.clicked() {
                                deferred.push(Deferred::ToggleCheck(i));
                            }
                            text_width -= marker_w + gap;
                        }
                        BlockKind::Toggle { open } => {
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(marker_w, line_h), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(rect.shrink(2.0), CornerRadius::same(4), pal.hover);
                            }
                            let tri = if *open { "▾" } else { "▸" };
                            widgets::a11y(&resp, tr(env.lang, if *open { "a11y_collapse" } else { "a11y_expand" }));
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                tri,
                                FontId::proportional(16.0),
                                pal.text,
                            );
                            if resp.clicked() {
                                deferred.push(Deferred::ToggleOpen(i));
                            }
                            text_width -= marker_w + gap;
                        }
                        BlockKind::Quote => {
                            ui.add_space(16.0);
                            text_width -= 16.0 + gap;
                        }
                        _ => {}
                    }
                    self.rich_text(ui, blocks, i, text_width, env, out)
                });
                if kind == BlockKind::Quote {
                    let rr = r.response.rect;
                    let x = rr.min.x + 1.5;
                    ui.painter().line_segment(
                        [egui::pos2(x, rr.min.y + 3.0), egui::pos2(x, rr.max.y - 3.0)],
                        Stroke::new(3.0, pal.text),
                    );
                }
                r.response
            }
        }
    }

    /// Block after which dropped files go: the one under the pointer if it moved during the drag,
    /// otherwise the one with the caret.
    pub fn drop_hint(&self) -> Option<usize> {
        if self.drop_by_pointer {
            self.insert_hint.or(self.caret_hint)
        } else {
            self.caret_hint.or(self.insert_hint)
        }
    }

    fn apply_focus_request(&mut self, ctx: &egui::Context, b: &Block, id: Id) {
        if let Some((fb, caret)) = self.focus {
            if fb == b.id {
                ctx.memory_mut(|m| m.request_focus(id));
                set_cursor(ctx, id, caret, char_len(&b.text));
                self.focus = None;
                self.focused_block = Some(b.id);
                self.selected = None;
                self.sel_anchor = None;
            }
        }
    }

    fn track_rows(&mut self, o: &egui::text_edit::TextEditOutput) {
        if let Some(r) = o.cursor_range {
            let lc = o.galley.layout_from_cursor(r.primary);
            self.at_first_row = lc.row == 0;
            self.at_last_row = lc.row + 1 >= o.galley.rows.len().max(1);
        }
    }

    fn rich_text(
        &mut self,
        ui: &mut Ui,
        blocks: &mut [Block],
        i: usize,
        width: f32,
        env: &Env,
        out: &mut Output,
    ) -> egui::Response {
        let pal = env.pal;
        let bid = blocks[i].id;
        let id = text_id(bid);
        let ctx = ui.ctx().clone();
        let was_focused = ctx.memory(|m| m.has_focus(id));
        // a paragraph that is only a page mention renders as a page link block
        if !was_focused && blocks[i].kind == BlockKind::Paragraph && self.focus.map(|f| f.0) != Some(bid) {
            if let Some(title) = sole_mention(&blocks[i].text) {
                let target = env.titles.get(&title.to_lowercase()).cloned();
                let (icon, color) = match &target {
                    Some((DocId::Database(_), icon)) => {
                        (if icon.is_empty() { "🗂".to_string() } else { icon.clone() }, pal.text)
                    }
                    Some((_, icon)) => (if icon.is_empty() { "📄".to_string() } else { icon.clone() }, pal.text),
                    None => ("📄".to_string(), pal.text_faint),
                };
                let resp = ui.add(
                    egui::Button::new(crate::widgets::icon_title(&icon, title, theme::BODY_SIZE, color))
                        .min_size(Vec2::new(0.0, theme::BODY_SIZE * 1.5)),
                );
                let resp = resp.on_hover_text(if target.is_some() {
                    tr(env.lang, "click_open")
                } else {
                    tr(env.lang, "missing_page")
                });
                if resp.clicked() {
                    match target {
                        Some((doc, _)) => out.actions.push(Action::Navigate(doc)),
                        None => out.actions.push(Action::CreateMentioned(title.to_string())),
                    }
                }
                if resp.secondary_clicked() {
                    self.focus = Some((bid, Caret::End));
                }
                return resp;
            }
        }
        self.apply_focus_request(&ctx, &blocks[i], id);
        let focused_now = ctx.memory(|m| m.has_focus(id)) || was_focused;
        let base = base_for(&blocks[i].kind, &blocks[i].color, pal);
        let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| -> Arc<egui::Galley> {
            let job = inline_job(buf.as_str(), &base, pal, wrap);
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let before = blocks[i].text.clone();
        let mut te = egui::TextEdit::multiline(&mut blocks[i].text)
            .id(id)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .desired_rows(1)
            .desired_width(width)
            .lock_focus(true)
            .return_key(Some(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)))
            .layouter(&mut layouter);
        if before.is_empty() {
            if let Some(ph) = placeholder(&blocks[i].kind, focused_now, env.lang) {
                te = te.hint_text(RichText::new(ph).size(base.size).color(pal.text_faint));
            }
        }
        let o = te.show(ui);
        paint_underlines(ui.painter(), &o, &blocks[i].text, pal, base.color);
        let resp = o.response.response.clone();
        if resp.has_focus() {
            self.focused_block = Some(bid);
            self.selected = None;
            self.sel_anchor = None;
            self.track_rows(&o);
            if let Some(r) = o.cursor_range {
                let (a, b) = (r.primary.index.0, r.secondary.index.0);
                let here = o.galley.pos_from_cursor(r.primary);
                self.caret_x = here.center().x;
                let low = if a <= b { r.primary } else { r.secondary };
                let at = o.galley_pos + o.galley.pos_from_cursor(low).left_top().to_vec2();
                if a != b {
                    self.fmt_bar = Some((bid, a.min(b), a.max(b), at));
                    self.link_card = None;
                } else {
                    self.fmt_bar = None;
                    self.link_card = link_target_at(&blocks[i].text, a).map(|(_, d0, d1)| LinkCard {
                        bid,
                        pos: o.galley_pos + o.galley.pos_from_cursor(r.primary).left_bottom().to_vec2(),
                        url: blocks[i].text[d0..d1].replace("%20", " "),
                        caret: a,
                    });
                }
                if a == b {
                    // a caret must never rest between hidden markers
                    let snapped = inline::snap_caret(&blocks[i].text, a);
                    if snapped != a {
                        set_cursor(&ctx, id, Caret::At(snapped), char_len(&blocks[i].text));
                    }
                    self.last_selection = None;
                } else {
                    self.last_selection = Some((bid, a.min(b), a.max(b)));
                }
            }
        }
        // ⌘ click on a link opens it
        if resp.clicked() && ctx.input(|i| i.modifiers.command) {
            if let Some(r) = o.cursor_range {
                if let Some((_, d0, d1)) = link_target_at(&blocks[i].text, r.primary.index.0) {
                    let url = normalize_url(&blocks[i].text[d0..d1].replace("%20", " "));
                    if url.contains("://") || url.starts_with("mailto:") {
                        out.actions.push(Action::OpenUrl(url));
                    } else {
                        out.actions.push(Action::OpenFile(url));
                    }
                }
            }
        }
        if resp.drag_started() {
            self.drag_origin = Some(bid);
        }
        if let Some((gb, x, bottom, fr)) = self.goal {
            if gb == bid {
                if resp.has_focus() {
                    let y = if bottom { o.galley.rect.height() - 2.0 } else { 2.0 };
                    let c = o.galley.cursor_from_pos(egui::vec2(x, y));
                    set_cursor(&ctx, id, Caret::At(c.index.0), char_len(&blocks[i].text));
                    ctx.request_repaint();
                    self.goal = None;
                } else if self.frame > fr + 3 {
                    self.goal = None;
                }
            }
        }
        // clicking a link or mention: a page link opens, a web link opens with Cmd (a card shows it otherwise)
        if resp.clicked() {
            if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                let c = o.galley.cursor_from_pos(pos - o.galley_pos);
                if let Some(style) = inline::link_at(&blocks[i].text, c.index.0) {
                    if let Some(m) = style.mention {
                        match env.titles.get(&m.to_lowercase()) {
                            Some((doc, _)) => out.actions.push(Action::Navigate(doc.clone())),
                            None => out.actions.push(Action::CreateMentioned(m)),
                        }
                    } else if let (Some(url), true) = (style.link, ui.input(|i| i.modifiers.command)) {
                        let url = normalize_url(&url);
                        if url.contains("://") || url.starts_with("mailto:") {
                            out.actions.push(Action::OpenUrl(url));
                        } else {
                            out.actions.push(Action::OpenFile(url));
                        }
                    }
                }
            }
        }
        if blocks[i].text != before {
            let mut snap = blocks.to_vec();
            snap[i].text = before.clone();
            self.record(env.key, &snap, Some(&format!("t{bid}")));
            out.changed = true;
            let caret = o.cursor_range.map(|r| r.primary.index.0).unwrap_or(0);
            // Markdown shortcuts at the start of a paragraph
            if blocks[i].kind == BlockKind::Paragraph {
                if let Some((kind, strip)) = blocks::markdown_shortcut(&blocks[i].text) {
                    if caret == strip && blocks::markdown_shortcut(&before).is_none() {
                        self.pending_shortcut = Some((bid, kind, strip));
                    }
                }
            }
            match slash_query(&blocks[i].text, caret) {
                Some((start, q)) if self.dismissed_slash != Some((bid, start)) => {
                    let keep = self
                        .slash
                        .as_ref()
                        .filter(|s| s.block == bid && s.start == start)
                        .map(|s| s.selected)
                        .unwrap_or(0);
                    self.slash = Some(Slash { block: bid, start, query: q, selected: keep, mode: SlashMode::Insert });
                }
                found => {
                    if found.is_none() && self.dismissed_slash.is_some_and(|(b, _)| b == bid) {
                        // the "/" is gone: typing it again must open the menu again
                        self.dismissed_slash = None;
                    }
                    if self.slash.as_ref().is_some_and(|s| s.block == bid && s.mode == SlashMode::Insert) {
                        self.slash = None;
                    }
                }
            }
        }
        resp
    }
}

mod callout;
mod helpers;
mod image_block;
mod keys;
mod meeting_card;
mod menus;
mod richpaste;
mod selection;
mod simple;
mod slash_items;
mod table;
mod text_paint;
pub use helpers::human_size;
use helpers::number_label;

#[cfg(test)]
mod tests;

/// Pixel size of an image file, read from its header only and cached.
pub(crate) fn image_dims(path: &std::path::Path) -> Option<(u32, u32)> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static DIMS: RefCell<HashMap<std::path::PathBuf, Option<(u32, u32)>>> = RefCell::new(HashMap::new());
    }
    DIMS.with(|c| {
        *c.borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| image::image_dimensions(path).ok().filter(|(w, h)| *w > 0 && *h > 0))
    })
}
