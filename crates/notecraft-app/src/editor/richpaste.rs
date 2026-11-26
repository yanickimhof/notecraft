//! Rich paste: when the clipboard holds HTML (Outlook, Word, a web page), it is pasted as blocks and
//! inline formatting instead of one block of plain text.

/// The clipboard HTML converted to Markdown, or `None` when the clipboard has no HTML (or nothing in it).
pub fn clipboard_markdown() -> Option<String> {
    let html = arboard::Clipboard::new().ok()?.get().html().ok()?;
    let md = notecraft_core::import_html::clipboard_html_to_markdown(&html);
    (!md.trim().is_empty()).then_some(md)
}
