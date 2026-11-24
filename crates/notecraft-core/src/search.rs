//! Quick-finder ranking: fuzzy title matching merged with full-text hits.

/// Fold case and common accents for matching.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ÿ' | 'ý' => 'y',
            c => c,
        })
        .collect()
}

/// Score how well `query` matches `title` (higher is better), or `None`.
///
/// Rules: an exact match beats a prefix, a prefix beats a word start, a word
/// start beats a substring, and a substring beats a scattered subsequence.
pub fn fuzzy_score(query: &str, title: &str) -> Option<i32> {
    let q = fold(query.trim());
    if q.is_empty() {
        return Some(0);
    }
    fuzzy_score_folded(&q, title)
}

/// Same as [`fuzzy_score`] for a query that is already folded and non-empty
/// (lets a caller scoring thousands of titles fold the query once).
pub fn fuzzy_score_folded(q: &str, title: &str) -> Option<i32> {
    let t = fold(title);
    if t == q {
        return Some(1000);
    }
    if t.starts_with(q) {
        return Some(800 - t.len().min(200) as i32);
    }
    if let Some(pos) = t.find(q) {
        let word_start = pos == 0 || !t[..pos].chars().last().unwrap().is_alphanumeric();
        let base = if word_start { 600 } else { 400 };
        return Some(base - pos.min(100) as i32);
    }
    // subsequence
    let mut score = 200;
    let mut ti = t.chars().peekable();
    let mut gap = 0;
    for qc in q.chars() {
        if qc == ' ' {
            continue;
        }
        loop {
            match ti.next() {
                Some(tc) if tc == qc => break,
                Some(_) => gap += 1,
                None => return None,
            }
        }
    }
    score -= gap.min(150);
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_order() {
        let exact = fuzzy_score("notes", "Notes").unwrap();
        let prefix = fuzzy_score("not", "Notes").unwrap();
        let word = fuzzy_score("not", "My notes").unwrap();
        let sub = fuzzy_score("ote", "Notes").unwrap();
        let seq = fuzzy_score("nts", "Notes").unwrap();
        assert!(exact > prefix && prefix > word && word > sub && sub > seq);
        assert!(fuzzy_score("xyz", "Notes").is_none());
    }

    #[test]
    fn accents_fold() {
        assert!(fuzzy_score("ete", "Été").is_some());
    }
}
