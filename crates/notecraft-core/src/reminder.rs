//! Reminders on pages: a local date and time kept in the page's front matter.

use jiff::{civil::DateTime, Zoned};

const FMT: &str = "%Y-%m-%d %H:%M";

/// Now, as local `YYYY-MM-DD HH:MM`.
pub fn now_local() -> String {
    Zoned::now().strftime(FMT).to_string()
}

/// Accept `YYYY-MM-DD HH:MM`, `YYYY-MM-DDTHH:MM` or a bare date (taken as 09:00). `None` when it is not a real date.
pub fn normalize(input: &str) -> Option<String> {
    let s = input.trim().replace('T', " ");
    let s = if s.len() == 10 { format!("{s} 09:00") } else { s };
    let dt = DateTime::strptime(FMT, &s).ok()?;
    Some(dt.strftime(FMT).to_string())
}

/// `in 1 hour`, `tomorrow 09:00` or `next Monday 09:00`, counted from `now`.
pub enum Preset {
    InOneHour,
    Tomorrow,
    NextMonday,
}

pub fn preset(p: Preset, now: &Zoned) -> String {
    let at = |z: Zoned| z.strftime(FMT).to_string();
    match p {
        Preset::InOneHour => at(now.checked_add(jiff::Span::new().hours(1)).unwrap_or_else(|_| now.clone())),
        Preset::Tomorrow => {
            let d = now.date().tomorrow().unwrap_or(now.date());
            d.at(9, 0, 0, 0).strftime(FMT).to_string()
        }
        Preset::NextMonday => {
            let d = now.date().nth_weekday(1, jiff::civil::Weekday::Monday).unwrap_or(now.date());
            d.at(9, 0, 0, 0).strftime(FMT).to_string()
        }
    }
}

/// A reminder is due once its time is not after `now` (both `YYYY-MM-DD HH:MM`, so text order is time order).
pub fn is_due(when: &str, now: &str) -> bool {
    !when.is_empty() && when <= now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_normalised_or_refused() {
        assert_eq!(normalize("2026-12-01 14:30").as_deref(), Some("2026-12-01 14:30"));
        assert_eq!(normalize("2026-12-01T08:05").as_deref(), Some("2026-12-01 08:05"));
        assert_eq!(normalize("2026-12-01").as_deref(), Some("2026-12-01 09:00"));
        assert_eq!(normalize("2026-13-45 99:99"), None);
        assert_eq!(normalize("tomorrow"), None);
    }

    #[test]
    fn due_compares_times() {
        assert!(is_due("2026-12-01 09:00", "2026-12-01 09:00"));
        assert!(is_due("2026-11-30 23:59", "2026-12-01 00:00"));
        assert!(!is_due("2026-12-02 00:00", "2026-12-01 23:59"));
        assert!(!is_due("", "2026-12-01 09:00"));
    }

    #[test]
    fn presets_are_in_the_future_and_well_formed() {
        let now: Zoned = "2026-11-23T14:31:00+01:00[Europe/Zurich]".parse().unwrap();
        assert_eq!(preset(Preset::InOneHour, &now), "2026-11-23 15:31");
        assert_eq!(preset(Preset::Tomorrow, &now), "2026-11-24 09:00");
        assert_eq!(preset(Preset::NextMonday, &now), "2026-11-30 09:00");
    }
}
