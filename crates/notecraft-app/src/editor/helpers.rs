//! Small stateless widgets and formatters used by the block editor.

pub(super) fn number_label(n: usize, depth: u8) -> String {
    match depth % 3 {
        1 => format!("{}.", (b'a' + ((n - 1) % 26) as u8) as char),
        2 => format!("{}.", roman(n)),
        _ => format!("{n}."),
    }
}

fn roman(mut n: usize) -> String {
    let mut s = String::new();
    for (v, r) in [(10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
