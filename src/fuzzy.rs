//! Fuzzy ("lazy") matching for the task searches: type the letters you
//! remember, in order, and the task surfaces — `wsp` finds "Website
//! Proposal", `loginbtn` finds "Fix the login button".
//!
//! Scoring is fzf-shaped: the same characters are worth more when they land
//! consecutively, at the start of a word, and early in the text. Absolute
//! values mean nothing — they only order candidates for one query.

/// Per matched character.
const MATCH: i32 = 16;
/// Extra when it directly follows the previous match (`logi` beats `l-o-g-i`).
const CONSECUTIVE: i32 = 12;
/// Extra when it starts a word (`wp` for "Website Proposal").
const BOUNDARY: i32 = 10;
/// Per character skipped between two matches, and the cap per gap.
const GAP: i32 = 2;
const GAP_CAP: i32 = 12;
/// Per character skipped before the first match, and its cap — an early hit
/// beats a late one, but a long text isn't buried for it.
const LEAD: i32 = 1;
const LEAD_CAP: i32 = 12;

/// Lowercase, one char out per char in, so indices stay aligned with the
/// original (`char::to_lowercase` can yield several — we keep the first).
fn lower(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// Start of a word: the first character, one after a separator, or the
/// upper half of a camelCase hump.
fn is_boundary(orig: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let (prev, cur) = (orig[i - 1], orig[i]);
    !prev.is_alphanumeric() || (prev.is_lowercase() && cur.is_uppercase())
}

/// Score one term against `text`. None when the term's characters don't all
/// appear in `text`, in order.
pub fn score(term: &str, text: &str) -> Option<i32> {
    let q = lower(term);
    if q.is_empty() {
        return Some(0);
    }
    let orig: Vec<char> = text.chars().collect();
    let t = lower(text);

    // Leftmost match, to find where the term can end at the earliest…
    let mut qi = 0;
    let mut end = None;
    for (i, c) in t.iter().enumerate() {
        if *c == q[qi] {
            qi += 1;
            if qi == q.len() {
                end = Some(i);
                break;
            }
        }
    }
    let end = end?;
    // …then backwards from there for the tightest window ending at `end`:
    // greedy-from-the-left alone would score "ab" in "a-x-ab" on the far
    // apart pair instead of the adjacent one.
    let mut qi = q.len();
    let mut start = end;
    for i in (0..=end).rev() {
        if t[i] == q[qi - 1] {
            qi -= 1;
            if qi == 0 {
                start = i;
                break;
            }
        }
    }

    // Forward through that window, scoring the match it contains.
    let mut score = -((start as i32 * LEAD).min(LEAD_CAP));
    let mut qi = 0;
    let mut prev: Option<usize> = None;
    for (i, c) in t.iter().enumerate().take(end + 1).skip(start) {
        if qi == q.len() || *c != q[qi] {
            continue;
        }
        score += MATCH;
        match prev {
            Some(p) if p + 1 == i => score += CONSECUTIVE,
            Some(p) => score -= (((i - p - 1) as i32) * GAP).min(GAP_CAP),
            None => {}
        }
        if is_boundary(&orig, i) {
            score += BOUNDARY;
        }
        prev = Some(i);
        qi += 1;
    }
    Some(score)
}

/// Every whitespace-separated term of `query` must match `text`; the score
/// is their sum. An empty query matches everything, at score 0.
pub fn score_all(query: &str, text: &str) -> Option<i32> {
    let mut total = 0;
    for term in query.split_whitespace() {
        total += score(term, text)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked<'a>(query: &str, texts: &[&'a str]) -> Vec<&'a str> {
        let mut hits: Vec<(&str, i32)> = texts
            .iter()
            .filter_map(|t| score_all(query, t).map(|s| (*t, s)))
            .collect();
        hits.sort_by_key(|h| std::cmp::Reverse(h.1));
        hits.into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn matches_scattered_characters_in_order() {
        assert!(score("wsp", "Website Proposal").is_some());
        assert!(score("loginbtn", "Fix the login button").is_some());
        assert!(score("", "anything").is_some());
        // out of order, or a character that isn't there at all
        assert!(score("psw", "Website Proposal").is_none());
        assert!(score("wspz", "Website Proposal").is_none());
    }

    #[test]
    fn ignores_case_both_ways() {
        assert!(score("WEB", "website redesign").is_some());
        assert!(score("web", "WEBSITE REDESIGN").is_some());
    }

    #[test]
    fn every_term_must_match() {
        assert!(score_all("web prop", "Website Proposal").is_some());
        // order between terms doesn't matter
        assert!(score_all("prop web", "Website Proposal").is_some());
        assert!(score_all("web invoice", "Website Proposal").is_none());
    }

    #[test]
    fn tighter_earlier_matches_rank_higher() {
        // whole word beats initials beats scattered
        assert_eq!(
            ranked("login", &["Rework the analog inbox", "Fix the login button"]),
            ["Fix the login button", "Rework the analog inbox"]
        );
        // word starts beat mid-word letters
        assert_eq!(
            ranked("wp", &["Sewing patterns", "Website Proposal"]),
            ["Website Proposal", "Sewing patterns"]
        );
        // an early hit beats a late one, all else equal
        assert_eq!(
            ranked("api", &["Rewrite the docs for the api", "Api rate limits"]),
            ["Api rate limits", "Rewrite the docs for the api"]
        );
    }

    #[test]
    fn picks_the_tightest_window_not_the_leftmost() {
        // the adjacent "ab" must win over a-…-b spanning the whole string
        let tight = score("ab", "ab").unwrap();
        let spread = score("ab", "a-x-x-x-b").unwrap();
        assert!(tight > spread);
        // …even when the loose pair comes first in the text
        assert!(score("ab", "a-x-ab").unwrap() > spread);
    }

    #[test]
    fn survives_multibyte_text() {
        assert!(score("cf", "café frappé").is_some());
        assert!(score("é", "café").is_some());
        assert!(score("zz", "café").is_none());
    }
}
