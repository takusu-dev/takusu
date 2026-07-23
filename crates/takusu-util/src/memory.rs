use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

pub const MAX_KEY_SCALARS: usize = 256;
pub const MAX_CONTENT_SCALARS: usize = 4096;
pub const MAX_QUERY_SCALARS: usize = 256;

/// Worst-case cap on candidate rows fetched from SQL for similar-task search.
/// The bigram pre-filter already narrows candidates sharply; this only guards
/// against a very common bigram transferring an unbounded row set. Far above
/// any personal-scale completed-task count, so it never drops a relevant match
/// in practice (#942).
pub const SIMILAR_TASK_CANDIDATE_CAP: usize = 10000;

/// Shared Unicode normalization for memory keys, content, and search queries.
///
/// 1. NFKC
/// 2. ASCII uppercase → lowercase (per scalar value)
/// 3. Unicode whitespace → ASCII space
/// 4. collapse consecutive spaces and trim ends
/// 5. reject empty / control-only values and values exceeding `max_scalars`
pub fn normalize_text(input: &str, max_scalars: Option<usize>) -> Result<String, String> {
    let mut buf = Vec::with_capacity(input.chars().count().max(1));
    for c in input.nfkc() {
        let c = if c.is_ascii_uppercase() {
            c.to_ascii_lowercase()
        } else {
            c
        };
        let c = if c.is_whitespace() && c != ' ' {
            ' '
        } else {
            c
        };
        buf.push(c);
    }

    let mut out = String::with_capacity(buf.len());
    let mut prev_space = true;
    for c in buf {
        if c == ' ' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }

    if out.is_empty() {
        return Err("empty value".to_string());
    }
    if out.chars().all(|c| c.is_control()) {
        return Err("control characters only".to_string());
    }

    if let Some(max) = max_scalars {
        let count = out.chars().count();
        if count > max {
            return Err(format!("value exceeds {max} unicode scalar values"));
        }
    }

    Ok(out)
}

pub fn normalize_key(input: &str) -> Result<String, String> {
    normalize_text(input, Some(MAX_KEY_SCALARS))
}

pub fn normalize_content(input: &str) -> Result<String, String> {
    normalize_text(input, Some(MAX_CONTENT_SCALARS))
}

pub fn normalize_query(input: &str) -> Result<String, String> {
    normalize_text(input, Some(MAX_QUERY_SCALARS))
}

/// Build a set of distinct bigrams from a normalized string of scalar values.
pub fn bigrams(s: &str) -> HashSet<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return HashSet::new();
    }
    chars
        .windows(2)
        .filter_map(|w| {
            let [a, b] = w else { return None };
            Some((*a, *b))
        })
        .collect()
}

/// Sørensen–Dice coefficient of two sets of bigrams, plus a substring bonus.
pub fn dice_coefficient(a: &HashSet<(char, char)>, b: &HashSet<(char, char)>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection: f64 = a.intersection(b).count() as f64;
    (2.0 * intersection) / (a.len() + b.len()) as f64
}

fn similar_task_score_inner(q: &str, t: &str) -> Option<f64> {
    if t.is_empty() {
        return None;
    }

    let t_chars: Vec<char> = t.chars().collect();
    if t_chars.len() == 1 {
        return if t == q { Some(1.0) } else { None };
    }

    let q_bigrams = bigrams(q);
    let t_bigrams = bigrams(t);
    let mut score = dice_coefficient(&q_bigrams, &t_bigrams);
    if t.contains(q) {
        score += 0.25;
    }
    if score > 1.0 {
        score = 1.0;
    }
    if score <= 0.0 {
        return None;
    }
    Some(score)
}

/// Score a completed-task title against a query title.
/// Returns `None` when the title should be excluded from results.
pub fn similar_task_score(query: &str, title: &str) -> Option<f64> {
    let q = normalize_text(query, Some(MAX_QUERY_SCALARS)).ok()?;
    let t = normalize_text(title, Some(MAX_CONTENT_SCALARS)).ok()?;
    similar_task_score_inner(&q, &t)
}

/// Like [`similar_task_score`], but the `query` is already normalized so it
/// is not re-normalized for every title in a loop.
pub fn similar_task_score_pre_normalized(query: &str, title: &str) -> Option<f64> {
    let t = normalize_text(title, Some(MAX_CONTENT_SCALARS)).ok()?;
    similar_task_score_inner(query, &t)
}

/// Trait for items that can be ranked by `rank_memories`.
pub trait MemoryRankable {
    fn id(&self) -> &str;
    fn normalized_key(&self) -> &str;
    fn normalized_content(&self) -> &str;
    fn updated_at(&self) -> &str;
}

/// Rank key match quality; lower is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchRank {
    Exact = 0,
    Prefix = 1,
    KeySubstring = 2,
    ContentSubstring = 3,
    NoMatch = 4,
}

fn match_rank(query: &str, item: &dyn MemoryRankable) -> MatchRank {
    if item.normalized_key() == query {
        MatchRank::Exact
    } else if item.normalized_key().starts_with(query) {
        MatchRank::Prefix
    } else if item.normalized_key().contains(query) {
        MatchRank::KeySubstring
    } else if item.normalized_content().contains(query) {
        MatchRank::ContentSubstring
    } else {
        MatchRank::NoMatch
    }
}

/// Sort `items` by the deterministic memory search ranking in place.
/// After sorting, the caller should truncate to the desired `limit`.
pub fn sort_memories<T: MemoryRankable>(query: &str, items: &mut [T]) {
    if let Ok(q) = normalize_query(query) {
        items.sort_by(|a, b| {
            let ra = match_rank(&q, a);
            let rb = match_rank(&q, b);
            ra.cmp(&rb)
                .then_with(|| b.updated_at().cmp(a.updated_at()))
                .then_with(|| a.id().cmp(b.id()))
        });
    } else {
        items.sort_by(|a, b| {
            b.updated_at()
                .cmp(a.updated_at())
                .then_with(|| a.id().cmp(b.id()))
        });
    }
}

/// Compare two optional strings in descending order (`None` is "largest").
pub fn compare_optional_desc(a: &Option<String>, b: &Option<String>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.cmp(x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Escape a normalized query so it can be used as a literal inside a SQL
/// `LIKE` pattern. The escape character is `\`.
pub fn escape_like_pattern(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            other => vec![other],
        })
        .collect()
}

/// Build SQL `LIKE` patterns (wrapped in `%…%`, escaped for the `\` escape
/// character) that pre-filter candidate titles for similar-task search.
///
/// The patterns form a strict superset of the titles that would score non-zero
/// under [`similar_task_score_pre_normalized`], so using them as an SQL
/// pre-filter never drops a true match:
/// - a non-zero Dice score requires at least one shared bigram, i.e. the title
///   contains that bigram as a substring;
/// - the substring bonus for a query of length ≥ 2 likewise implies the title
///   contains a query bigram;
/// - a single-scalar query has no bigrams, so we fall back to a single-character
///   containment pattern (the substring bonus is its only scoring path).
///
/// Callers join the patterns with `OR` and bind each one as a SQL parameter
/// (never interpolate them into the SQL text). The result is sorted and
/// deduplicated for deterministic SQL.
pub fn similar_task_filter_patterns(normalized_query: &str) -> Vec<String> {
    let bg = bigrams(normalized_query);
    let mut out: Vec<String> = if bg.is_empty() {
        if normalized_query.is_empty() {
            return Vec::new();
        }
        vec![format!("%{}%", escape_like_pattern(normalized_query))]
    } else {
        bg.into_iter()
            .map(|(a, b)| {
                format!(
                    "%{}%",
                    escape_like_pattern(&[a, b].iter().collect::<String>())
                )
            })
            .collect()
    };
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_nfkc_case_and_whitespace() {
        let s = "  ＡＢＣ　  研究室  \t\n  ";
        let out = normalize_text(s, Some(100)).unwrap();
        assert_eq!(out, "abc 研究室");
    }

    #[test]
    fn normalize_rejects_empty() {
        assert!(normalize_text("   ", None).is_err());
        assert!(normalize_text("\t\n", None).is_err());
    }

    #[test]
    fn normalize_enforces_max() {
        let s = "a".repeat(300);
        assert!(normalize_text(&s, Some(256)).is_err());
        assert!(normalize_text(&s, Some(300)).is_ok());
    }

    #[test]
    fn bigram_basic() {
        let s = "abc";
        let b = bigrams(s);
        assert!(b.contains(&('a', 'b')));
        assert!(b.contains(&('b', 'c')));
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn dice_identical() {
        let a = bigrams("abc");
        let b = bigrams("abc");
        assert!((dice_coefficient(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similar_task_one_scalar_exact() {
        assert_eq!(similar_task_score("a", "a"), Some(1.0));
        assert_eq!(similar_task_score("a", "b"), None);
    }

    #[test]
    fn similar_task_substring_bonus() {
        let score = similar_task_score("数学の演習", "数学の演習問題を解く").unwrap();
        assert!(score > 0.25);
    }

    #[test]
    fn similar_task_excludes_empty_title() {
        assert_eq!(similar_task_score("x", "!!!"), None);
    }

    #[test]
    fn filter_patterns_are_superset_of_dice_matches() {
        // A reordered title shares bigrams but no substring with the query; the
        // patterns must still match it (recall safety).
        let q = normalize_query("数学演習").unwrap();
        let title = normalize_text("演習数学", Some(MAX_CONTENT_SCALARS)).unwrap();
        assert!(similar_task_score_pre_normalized(&q, &title).is_some());
        let pats = similar_task_filter_patterns(&q);
        assert!(
            pats.iter()
                .any(|p| title.contains(p.trim_start_matches('%').trim_end_matches('%')))
        );
    }

    #[test]
    fn filter_patterns_single_scalar_fallback() {
        let pats = similar_task_filter_patterns("x");
        assert_eq!(pats, vec!["%x%".to_string()]);
    }

    #[test]
    fn filter_patterns_empty_query() {
        assert!(similar_task_filter_patterns("").is_empty());
    }

    #[test]
    fn filter_patterns_escape_like_metachars() {
        // A bigram containing '%' must be escaped so it is matched literally.
        let pats = similar_task_filter_patterns("a%b");
        assert!(pats.contains(&"%a\\%%".to_string()));
        assert!(pats.contains(&"%\\%b%".to_string()));
    }

    #[test]
    fn filter_patterns_sorted_and_deduped() {
        let pats = similar_task_filter_patterns("aaaa");
        // Only one distinct bigram ("aa").
        assert_eq!(pats, vec!["%aa%".to_string()]);
    }

    #[test]
    fn memory_ranking() {
        struct M {
            id: String,
            key: String,
            content: String,
            updated: String,
        }
        impl MemoryRankable for M {
            fn id(&self) -> &str {
                &self.id
            }
            fn normalized_key(&self) -> &str {
                &self.key
            }
            fn normalized_content(&self) -> &str {
                &self.content
            }
            fn updated_at(&self) -> &str {
                &self.updated
            }
        }
        let mut items = vec![
            M {
                id: "1".into(),
                key: "研究室".into(),
                content: "...".into(),
                updated: "2025-01-02T00:00:00Z".into(),
            },
            M {
                id: "2".into(),
                key: "研究室".into(),
                content: "...".into(),
                updated: "2025-01-01T00:00:00Z".into(),
            },
            M {
                id: "3".into(),
                key: "研究室棟".into(),
                content: "...".into(),
                updated: "2025-01-03T00:00:00Z".into(),
            },
            M {
                id: "4".into(),
                key: "foo".into(),
                content: "研究室について".into(),
                updated: "2025-01-04T00:00:00Z".into(),
            },
        ];
        sort_memories("研究室", &mut items);
        assert_eq!(items[0].id, "1"); // exact, newer tie
        assert_eq!(items[1].id, "2"); // exact, older
        assert_eq!(items[2].id, "3"); // prefix
        assert_eq!(items[3].id, "4"); // content substring
    }
}
