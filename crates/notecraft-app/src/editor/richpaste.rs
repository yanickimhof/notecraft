//! Rich paste: when the clipboard holds HTML (Outlook, Word, a web page), it is pasted as blocks and
//! inline formatting instead of one block of plain text.

/// The clipboard HTML converted to Markdown, or `None` when the clipboard has no HTML (or nothing in it).
pub fn clipboard_markdown() -> Option<String> {
    let html = arboard::Clipboard::new().ok()?.get().html().ok()?;
    let md = notecraft_core::import_html::clipboard_html_to_markdown(&html);
    (!md.trim().is_empty()).then_some(md)
}

/// Puts the blocks on the clipboard as HTML with a Markdown fallback: a mail or a Word document keeps the
/// headings, lists and bold, a text editor gets Markdown. Falls back to plain Markdown text.
pub fn copy_blocks(ctx: &eframe::egui::Context, blocks: &[notecraft_core::blocks::Block]) {
    let md = notecraft_core::blocks::to_markdown(blocks);
    let html = notecraft_core::render::html_body(blocks, &|_| None);
    let doc = format!("<html><head><meta charset=\"utf-8\"></head><body>{html}</body></html>");
    let done = arboard::Clipboard::new().ok().is_some_and(|mut c| c.set_html(doc, Some(md.clone())).is_ok());
    if !done {
        ctx.copy_text(md);
    }
}
