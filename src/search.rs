//! Full-vault text search and fuzzy quick-switcher scoring.

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Note path relative to vault root ('/' separators).
    pub rel: String,
    /// 0-based line number of the match.
    pub line_no: usize,
    /// The matching line, trimmed.
    pub line_text: String,
}

/// Search all notes line-by-line (case-insensitive). Lines are supplied by the
/// caller as `(rel_path, content)` pairs so this stays filesystem-free.
pub fn search_notes(notes: &[(String, String)], query: &str) -> Vec<SearchHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (rel, content) in notes {
        for (i, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&q) {
                hits.push(SearchHit {
                    rel: rel.clone(),
                    line_no: i,
                    line_text: line.trim().to_string(),
                });
            }
        }
    }
    hits
}

/// A note candidate for the quick switcher.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitcherCandidate {
    pub rel: String,
    pub score: i64,
}

/// Score one filename stem against a typed query. Higher is better;
/// returns None when the query cannot match as an in-order subsequence.
pub fn score_stem(stem: &str, query: &str) -> Option<i64> {
    let stem_lower = stem.to_lowercase();
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }

    let mut score: i64 = 0;
    let mut stem_bytes = stem_lower.char_indices();

    // First query char must match at a word boundary to earn bonuses.
    let mut first = true;

    for qc in q.chars() {
        let mut matched = false;
        // Search remaining part of stem; gap penalty grows with skipped chars.
        let mut gap: i64 = 0;
        for (idx, sc) in stem_bytes.by_ref() {
            if sc == qc {
                if first {
                    let at_word_start =
                        idx == 0 || stem[..idx].ends_with([' ', '-', '_', '/', '.']);
                    if at_word_start {
                        score += 20;
                    } else {
                        score += 5;
                    }
                    first = false;
                } else {
                    score += 10 - gap.min(9);
                }
                matched = true;
                break;
            }
            gap += 1;
        }
        if !matched {
            return None;
        }
    }

    // Prefer shorter stems (closer match), slight bonus for full prefix.
    score -= (stem_lower.len() as i64) / 8;
    if stem_lower.starts_with(&q) {
        score += 15;
    }
    Some(score)
}

/// Rank notes for the quick switcher; empty query = all notes.
pub fn quick_switcher<'a>(
    stems: impl IntoIterator<Item = &'a str>,
    query: &str,
) -> Vec<SwitcherCandidate> {
    let mut candidates: Vec<SwitcherCandidate> = Vec::new();
    for stem in stems {
        if let Some(score) = score_stem(stem, query) {
            candidates.push(SwitcherCandidate { rel: stem.to_string(), score });
        }
    }
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.rel.cmp(&b.rel)));
    candidates
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<(String, String)> {
        vec![
            (
                "a.md".into(),
                "# Rust\n\nfast editor\nsecond fast line\n".into(),
            ),
            ("b.md".into(), "nothing here\n".into()),
            ("sub/c.md".into(), "RUST uppercase\n".into()),
        ]
    }

    #[test]
    fn search_is_case_insensitive_and_reports_lines() {
        let hits = search_notes(&sample(), "rust");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].rel, "a.md");
        assert_eq!(hits[0].line_no, 0);
        assert_eq!(hits[1].rel, "sub/c.md");
        assert_eq!(hits[1].line_no, 0);
    }

    #[test]
    fn search_finds_multiple_lines_in_one_note() {
        let hits = search_notes(&sample(), "fast");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].line_no, 2);
        assert_eq!(hits[1].line_no, 3);
        assert_eq!(hits[1].line_text, "second fast line");
    }

    #[test]
    fn search_empty_query_returns_nothing() {
        assert!(search_notes(&sample(), "").is_empty());
        assert!(search_notes(&sample(), "   ").is_empty());
    }

    #[test]
    fn scoring_prefers_word_start_and_prefix_matches() {
        // prefix of the stem beats a later subsequence
        let prefix = score_stem("meeting", "mee").unwrap();
        let scattered = score_stem("some meeting", "mee").unwrap();
        assert!(prefix > scattered);

        // word-start inside beats mid-word
        let word_start = score_stem("daily meeting", "meet").unwrap();
        let mid_word = score_stem("unmeetable", "meet").unwrap();
        assert!(word_start > mid_word);
    }

    #[test]
    fn scoring_is_case_insensitive_and_rejects_non_matches() {
        assert_eq!(score_stem("Meeting Notes", "MEET"), score_stem("Meeting Notes", "meet"));
        assert!(score_stem("rust", "xyz").is_none());
    }

    #[test]
    fn switcher_ranks_best_match_first() {
        // Both match at a word start; the shorter prefix-complete stem wins
        // (fzf/VS Code convention).
        let stems = ["unrelated", "meeting", "Meeting Notes"];
        let ranked = quick_switcher(stems.iter().copied(), "mee");
        assert_eq!(ranked[0].rel, "meeting");
        assert!(ranked.len() >= 2);
        assert!(ranked.windows(2).all(|w| w[0].score >= w[1].score));
    }

    #[test]
    fn switcher_empty_query_lists_everything() {
        let stems = ["b", "a"];
        let ranked = quick_switcher(stems.iter().copied(), "");
        assert_eq!(ranked.len(), 2);
    }
}
