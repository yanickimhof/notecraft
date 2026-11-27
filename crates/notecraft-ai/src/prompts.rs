//! The instructions sent with each AI action.

/// What the person asked the AI to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiAction {
    Summarize,
    Continue,
    Improve,
    /// Target language, e.g. "French" or "English".
    Translate(String),
    Ask(String),
    /// Fix spelling, grammar and punctuation; change nothing else.
    Proofread,
    /// Proofread, and rewrite clumsy sentences.
    Clarify,
    /// Proofread, then regroup loose notes under headings, lists and a summary.
    Restructure,
    /// Solve the equation in the selection step by step; the language to write in, e.g. "French".
    Solve(String),
}

impl AiAction {
    /// Short key (used for translations of labels in the app).
    pub fn key(&self) -> &'static str {
        match self {
            AiAction::Summarize => "ai_summarize",
            AiAction::Continue => "ai_continue",
            AiAction::Improve => "ai_improve",
            AiAction::Translate(l) if l == "French" => "ai_to_french",
            AiAction::Translate(_) => "ai_to_english",
            AiAction::Ask(_) => "ai_ask",
            AiAction::Solve(_) => "ai_solve",
            AiAction::Proofread => "ai_proofread",
            AiAction::Clarify => "ai_clarify",
            AiAction::Restructure => "ai_restructure",
        }
    }

    /// The review actions: on a whole page they replace it (kept elements stay as they are).
    pub fn is_review(&self) -> bool {
        matches!(self, AiAction::Improve | AiAction::Proofread | AiAction::Clarify | AiAction::Restructure)
    }

    /// True when the answer replaces the selection rather than being added after it.
    pub fn replaces(&self) -> bool {
        matches!(self, AiAction::Improve | AiAction::Translate(_)) || self.is_review()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

/// Longest page text sent (characters); the rest is cut.
pub const MAX_CHARS: usize = 60_000;

const FORMAT: &str = "You are a writing assistant inside a personal notes app. Reply in Markdown using only paragraphs, # headings, - bullet lists, 1. numbered lists, - [ ] to-dos, > quotes and fenced code. Reply with the result only: no preamble, no closing remarks.";

fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_CHARS {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(MAX_CHARS).collect();
        t.push_str("\n\n[…the rest of the page was cut]");
        t
    }
}

const KEEP_RULE: &str = "Lines made only of a marker like @@KEEP3@@ stand for elements you cannot see: copy each one exactly, once, on its own line, in a place that fits.";

/// Build the prompt for an action on a page (or on a selection inside it).
pub fn build_prompt(action: &AiAction, title: &str, page_markdown: &str, selection: Option<&str>) -> Prompt {
    let page = clip(page_markdown);
    let target = selection.filter(|s| !s.trim().is_empty());
    let subject = match target {
        Some(sel) => format!("Page title: {title}\n\nFull page, for context:\n<page>\n{page}\n</page>\n\nSelected text:\n<selection>\n{sel}\n</selection>"),
        None => format!("Page title: {title}\n\n<page>\n{page}\n</page>"),
    };
    let what = if target.is_some() { "the selected text" } else { "the page" };
    let task = match action {
        AiAction::Summarize => format!("Summarize {what} in a few short bullet points, in the same language as the text."),
        AiAction::Continue => format!("Continue writing after the end of {what}, in the same language, tone and style. Write one to three paragraphs and return only the new text."),
        AiAction::Improve => format!("Rewrite {what} so it is clearer and reads better. Keep the meaning, the facts, the language, the order and the Markdown structure (tables, links, [[links]], tags). Never touch numbers, names, dates, amounts or URLs. {KEEP_RULE} Return the whole text."),
        AiAction::Proofread => format!("Correct the spelling, grammar, agreement, punctuation, capitalisation and typography of {what}. Change nothing else: keep every word choice, the tone, the order, the meaning, the language and the Markdown structure (headings, lists, tables, links, [[links]], bold, colours and other tags). Never touch numbers, names, dates, amounts or URLs. {KEEP_RULE} Return the whole text."),
        AiAction::Clarify => format!("Correct {what} and rewrite clumsy, heavy or unclear sentences so it reads well. Keep the meaning, the facts, the language, the tone, the order and the Markdown structure (tables, links, [[links]], tags). Never add or remove information; never touch numbers, names, dates, amounts or URLs. {KEEP_RULE} Return the whole text."),
        AiAction::Restructure => format!("Reorganise {what}, which is made of loose notes. Correct spelling and grammar, group related notes under clear headings (## and ###), turn runs of related short lines into bullet lists, keep chronological items in order, put a short ## Summary first when the text is longer than a few lines, and collect tasks and decisions in ## Action items (- [ ] Task, owner, date when stated) and ## Decisions. Keep every fact, number, name, date and link exactly; never invent anything; do not drop anything. Keep tables as Markdown tables (you may fix their cells). Keep the language of the text. {KEEP_RULE} Return the whole page."),
        AiAction::Translate(lang) => format!("Translate {what} into {lang}. Keep the Markdown structure, names and [[links]] as they are. Return only the translation."),
        AiAction::Solve(lang) => format!(
            "Solve the equation or exercise in {what} step by step, as a teacher helping a student. Write in {lang}. Put each step as one short sentence in its own paragraph, followed by the formula on its own lines between $$ and $$ (one formula per block). Use only this LaTeX: \\frac{{a}}{{b}}, \\sqrt{{x}}, ^, _, \\pm, \\cdot, \\times, \\leq, \\geq, \\neq, \\approx, \\infty, \\sum, \\prod, \\int, Greek letters, \\to, \\Rightarrow. Never use \\begin, \\text, \\left, \\right, align or matrices. End with the final answer and, when possible, a check by substitution. Be exact: if it cannot be solved, say why."
        ),
        AiAction::Ask(q) => format!("Answer this question using {what}: {q}\nIf the answer is not in the text, say so briefly. Answer in the language of the question."),
    };
    let system = if matches!(action, AiAction::Solve(_)) {
        format!("{FORMAT} Exception: formulas go in $$ blocks as described in the task.")
    } else {
        FORMAT.to_string()
    };
    Prompt { system, user: format!("{subject}\n\nTask: {task}") }
}

/// The built-in note templates, in the order they are offered. `custom` uses the person's own instructions.
pub const NOTE_TEMPLATES: &[&str] =
    &["summary", "minutes", "actions", "one_on_one", "interview", "lecture", "sales", "custom"];

/// The instructions of a built-in note template (used to write them as editable pages).
pub fn note_structure_text(key: &str) -> &'static str {
    note_structure(key)
}

/// What the notes should contain for a template.
fn note_structure(key: &str) -> &'static str {
    match key {
        "minutes" => "Write formal meeting minutes with these headings: ## Summary (3 to 5 lines), ## Participants (only people named in the transcript), ## Key points (bullets), ## Decisions (bullets), ## Action items (to-dos in the form - [ ] Task, owner, due date when stated), ## Open questions.",
        "actions" => "Write only what must happen next. Use two headings: ## Decisions (bullets) and ## Action items (to-dos in the form - [ ] Task, owner, due date when stated). Add a one-line ## Summary first.",
        "one_on_one" => "Write notes for a one-to-one conversation with these headings: ## Summary, ## Topics discussed (bullets), ## Feedback and concerns, ## Agreements, ## Action items (to-dos in the form - [ ] Task, owner, due date when stated), ## For next time.",
        "interview" => "Write interview notes with these headings: ## Summary, ## Background and experience, ## Strengths (bullets), ## Concerns (bullets), ## Notable answers (short quotes or paraphrases), ## Next steps. Stay factual; do not give a hire or no-hire verdict.",
        "lecture" => "Write study notes with these headings: ## Overview, ## Key concepts (each with a one-line explanation), ## Details and examples, ## Questions to review. Keep the order of the talk.",
        "sales" => "Write sales call notes with these headings: ## Summary, ## Needs and pain points, ## Budget, timing and decision makers, ## Objections, ## Next steps (to-dos in the form - [ ] Task, owner, due date when stated).",
        _ => "Write clear notes with these headings: ## Summary (3 to 5 lines), ## Key points (bullets), ## Decisions (bullets, only if any were made), ## Action items (to-dos in the form - [ ] Task, owner, due date when stated, only if any).",
    }
}

/// Keeps the beginning and the end of a very long transcript.
fn clip_transcript(s: &str) -> String {
    if s.chars().count() <= MAX_CHARS {
        return s.to_string();
    }
    let half = MAX_CHARS / 2;
    let head: String = s.chars().take(half).collect();
    let tail: String = s.chars().skip(s.chars().count() - half).collect();
    format!("{head}\n\n[…the middle of the transcript was cut because it is very long…]\n\n{tail}")
}

/// Build the prompt that turns a meeting transcript into notes.
/// `template` is one of [`NOTE_TEMPLATES`]; for `custom`, `custom_instructions` says what to write.
pub fn build_notes_prompt(
    template: &str,
    custom_instructions: &str,
    title: &str,
    date: &str,
    transcript: &str,
) -> Prompt {
    let structure = if template == "custom" && !custom_instructions.trim().is_empty() {
        custom_instructions.trim()
    } else {
        note_structure(template)
    };
    let system = format!(
        "{FORMAT} You turn the transcript of a recorded conversation into notes. The transcript is automatic speech recognition, so some words may be wrong: use the context to fix obvious errors, but never invent names, numbers, dates, decisions or tasks that were not said. If something is unclear, say it is unclear. Always write the notes in the language of the transcript."
    );
    let user = format!(
        "Meeting: {title}\nDate: {date}\n\n<transcript>\n{}\n</transcript>\n\nTask: {structure}\n\nLanguage: write every word of the notes, headings included, in the language the transcript is spoken in (a French transcript gives French notes with French headings). Never answer in English unless the transcript is in English.",
        clip_transcript(transcript)
    );
    Prompt { system, user }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_prompt_follows_the_template() {
        let p = build_notes_prompt("minutes", "", "Weekly", "2026-11-22", "We ship Friday.");
        assert!(p.user.contains("We ship Friday."));
        assert!(p.user.contains("## Decisions"));
        assert!(p.system.contains("never invent"));
        let c = build_notes_prompt("custom", "Write a haiku.", "T", "d", "x");
        assert!(c.user.contains("Write a haiku."));
        // an empty custom template falls back to the general one
        assert!(build_notes_prompt("custom", " ", "T", "d", "x").user.contains("## Key points"));
        for t in NOTE_TEMPLATES {
            assert!(!build_notes_prompt(t, "", "T", "d", "x").user.is_empty());
        }
    }

    #[test]
    fn very_long_transcripts_keep_both_ends() {
        let long = format!("START{}END", "a".repeat(MAX_CHARS * 2));
        let p = build_notes_prompt("summary", "", "T", "d", &long);
        assert!(p.user.contains("START") && p.user.contains("END") && p.user.contains("was cut"));
    }

    #[test]
    fn prompts_name_the_task_and_the_text() {
        let p = build_prompt(&AiAction::Translate("French".into()), "Plan", "Hello", None);
        assert!(p.user.contains("Translate the page into French"));
        assert!(p.user.contains("<page>\nHello\n</page>"));
        let p = build_prompt(&AiAction::Improve, "Plan", "Hello world", Some("world"));
        assert!(p.user.contains("<selection>\nworld\n</selection>") && p.user.contains("Rewrite the selected text"));
        let p = build_prompt(&AiAction::Ask("When do we leave?".into()), "Trip", "We leave Monday", None);
        assert!(p.user.contains("When do we leave?"));
        assert!(AiAction::Improve.replaces() && !AiAction::Summarize.replaces());
    }

    #[test]
    fn long_pages_are_cut() {
        let long = "x".repeat(MAX_CHARS + 10);
        let p = build_prompt(&AiAction::Summarize, "T", &long, None);
        assert!(p.user.contains("the rest of the page was cut"));
    }
}
