//! A small sample workspace: a few pages and a task database, used for the
//! screenshots and for tests.

use crate::blocks::{Block, BlockKind};
use crate::database::{PropKind, PropValue, SelectOption, View, ViewKind};
use crate::workspace::{DocId, Workspace};
use crate::Result;

/// Fill an (empty) workspace with sample content. Does nothing if it already has pages.
pub fn populate(ws: &mut Workspace) -> Result<()> {
    if !ws.pages.is_empty() || !ws.dbs.is_empty() {
        return Ok(());
    }
    let home = ws.create_page(None, "Getting started")?;
    {
        let p = ws.pages.get_mut(&home).unwrap();
        p.meta.icon = "👋".into();
        p.meta.cover = "gradient:1".into();
        p.blocks = vec![
            Block::paragraph(
                "Welcome to your workspace. Everything here is a plain Markdown file in a folder you own.",
            ),
            Block::new(BlockKind::Heading(2), "The basics"),
            Block::new(BlockKind::Todo { checked: true }, "Click anywhere and start typing"),
            Block::new(BlockKind::Todo { checked: false }, "Type **/** to insert headings, lists, code and more"),
            Block::new(
                BlockKind::Todo { checked: false },
                "Select text and press **⌘B**, *⌘I*, <u>⌘U</u>, ~~⌘⇧S~~ or `⌘E` — they ***combine***",
            ),
            Block::new(BlockKind::Todo { checked: false }, "Press **⌘P** to find any page"),
            Block::new(
                BlockKind::Callout { icon: "💡".into() },
                "Link pages with [[Reading list]] — the other page shows a backlink.",
            ),
            Block::new(BlockKind::Toggle { open: false }, "Where are my files?"),
            Block::paragraph("Pages live in pages/, databases in databases/, attachments in assets/.").with_depth(1),
            Block::new(BlockKind::Divider, ""),
            Block::new(BlockKind::Quote, "Simple tools, used well."),
        ];
    }
    ws.save_page(&home)?;

    let reading = ws.create_page(None, "Reading list")?;
    {
        let p = ws.pages.get_mut(&reading).unwrap();
        p.meta.icon = "📚".into();
        p.blocks = vec![
            Block::new(BlockKind::Bulleted, "A book about gardens"),
            Block::new(BlockKind::Bulleted, "Essays on design"),
            Block::new(BlockKind::Bulleted, "Short stories").with_depth(1),
            Block::new(BlockKind::Numbered, "Read one chapter a day"),
            Block::new(BlockKind::Numbered, "Write a short note after each"),
            Block::new(BlockKind::Code { lang: "text".into() }, "notes/2026-11-22.md"),
        ];
    }
    ws.save_page(&reading)?;
    ws.set_favorite(&DocId::Page(reading.clone()), true)?;

    let journal = ws.create_page(Some(&home), "Journal")?;
    ws.pages.get_mut(&journal).unwrap().meta.icon = "📓".into();
    ws.pages.get_mut(&journal).unwrap().blocks = vec![Block::paragraph("Today I set up my workspace.")];
    ws.save_page(&journal)?;

    let sync = ws.create_page(None, "Weekly sync")?;
    {
        let p = ws.pages.get_mut(&sync).unwrap();
        p.meta.icon = "🎙".into();
        p.blocks = vec![
            Block::new(BlockKind::Meeting, "Weekly sync"),
            Block::new(BlockKind::Heading(2), "Summary"),
            Block::paragraph("The team confirmed the release for Friday and agreed to move the design review to Thursday."),
            Block::new(BlockKind::Heading(2), "Decisions"),
            Block::new(BlockKind::Bulleted, "Ship the new onboarding on Friday"),
            Block::new(BlockKind::Bulleted, "Design review moves to Thursday afternoon"),
            Block::new(BlockKind::Heading(2), "Action items"),
            Block::new(BlockKind::Todo { checked: false }, "Anna: send the release notes before Thursday"),
            Block::new(BlockKind::Todo { checked: false }, "Marc: book the room for the design review"),
            Block::new(BlockKind::Toggle { open: false }, "Transcript"),
            Block::paragraph("So, are we still good to ship on Friday? Yes, the tests are green. Then let's move the design review to Thursday.")
                .with_depth(1),
        ];
    }
    ws.save_page(&sync)?;

    let db = ws.create_database(None, "Projects")?;
    {
        let d = ws.dbs.get_mut(&db).unwrap();
        d.schema.icon = "🗂".into();
        d.schema.views.push(View::new("Board", ViewKind::Board));
        d.schema.views.push(View::new("List", ViewKind::List));
        d.schema.views.push(View::new("Calendar", ViewKind::Calendar));
        d.schema.views.push(View::new("Gallery", ViewKind::Gallery));
        let mut prio = crate::database::Property::new("Priority", PropKind::Select);
        prio.options = vec![
            SelectOption { name: "High".into(), color: "red".into(), group: None },
            SelectOption { name: "Medium".into(), color: "orange".into(), group: None },
            SelectOption { name: "Low".into(), color: "gray".into(), group: None },
        ];
        d.schema.properties.push(prio);
        d.schema.properties.push(crate::database::Property::new("Done", PropKind::Checkbox));
    }
    ws.save_schema(&db)?;
    let props = ws.dbs[&db].schema.properties.clone();
    let p = |n: &str| props.iter().find(|p| p.name == n).unwrap().clone();
    // dates are relative to today, so the calendar view always has something to show
    let day = |offset: i64| -> String {
        let today = jiff::Zoned::now().date();
        today.checked_add(jiff::Span::new().days(offset)).unwrap_or(today).strftime("%Y-%m-%d").to_string()
    };
    let rows = [
        ("Redesign the garden", "In progress", "High", vec!["Personal"], day(3)),
        ("Quarterly report", "Not started", "Medium", vec!["Work"], day(9)),
        ("Photo album", "Done", "Low", vec!["Personal", "Idea"], day(-4)),
        ("Plan the summer trip", "In progress", "Medium", vec!["Personal", "Idea"], day(6)),
        ("Team offsite", "Not started", "High", vec!["Work"], day(1)),
    ];
    for (title, status, prio, tags, date) in rows {
        let id = ws.add_row(&db, None, title)?;
        let r = ws.dbs.get_mut(&db).unwrap().rows.get_mut(&id).unwrap();
        r.set_value(&p("Status"), PropValue::Select(status.into()));
        r.set_value(&p("Priority"), PropValue::Select(prio.into()));
        r.set_value(&p("Tags"), PropValue::Multi(tags.into_iter().map(String::from).collect()));
        r.set_value(&p("Date"), PropValue::Date(date));
        r.set_value(&p("Done"), PropValue::Checkbox(status == "Done"));
        r.blocks = vec![Block::paragraph("Notes for this project.")];
        ws.save_row(&db, &id)?;
    }
    let tpl = ws.add_template(&db, "New project")?;
    {
        let t = ws.dbs.get_mut(&db).unwrap().templates.get_mut(&tpl).unwrap();
        t.set_value(&p("Status"), PropValue::Select("Not started".into()));
        t.blocks = vec![
            Block::new(BlockKind::Heading(2), "Goal"),
            Block::paragraph(""),
            Block::new(BlockKind::Heading(2), "Next steps"),
            Block::new(BlockKind::Todo { checked: false }, ""),
        ];
    }
    ws.save_template(&db, &tpl)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_complete_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let mut ws = Workspace::open(dir.path()).unwrap();
        populate(&mut ws).unwrap();
        assert_eq!(ws.pages.len(), 4);
        assert_eq!(ws.dbs.len(), 1);
        populate(&mut ws).unwrap();
        assert_eq!(ws.pages.len(), 4);
        let reading = ws.resolve_title("Reading list").unwrap();
        assert_eq!(ws.backlinks(&reading).len(), 1);
        assert!(!ws.search("garden", 10).is_empty());
    }
}
