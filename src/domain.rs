use serde::{Deserialize, Serialize};

/// Categories mirroring AnswerThePublic's wheels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Questions,
    Prepositions,
    Comparisons,
    Alphabetical,
    Related,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Questions => "questions",
            Category::Prepositions => "prepositions",
            Category::Comparisons => "comparisons",
            Category::Alphabetical => "alphabetical",
            Category::Related => "related",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Category::Questions => "Questions",
            Category::Prepositions => "Prepositions",
            Category::Comparisons => "Comparisons",
            Category::Alphabetical => "Alphabeticals",
            Category::Related => "Related",
        }
    }

    pub fn all() -> [Category; 5] {
        [
            Category::Questions,
            Category::Prepositions,
            Category::Comparisons,
            Category::Alphabetical,
            Category::Related,
        ]
    }

    /// Inverse of [`Category::as_str`]; unknown values map to `Related`.
    pub fn parse(s: &str) -> Category {
        match s {
            "questions" => Category::Questions,
            "prepositions" => Category::Prepositions,
            "comparisons" => Category::Comparisons,
            "alphabetical" => Category::Alphabetical,
            _ => Category::Related,
        }
    }
}

pub const QUESTION_WORDS: [&str; 9] = [
    "how", "what", "why", "when", "where", "who", "which", "are", "can",
];

pub const PREPOSITIONS: [&str; 7] = ["for", "with", "without", "to", "near", "is", "can"];

pub const COMPARISONS: [&str; 6] = ["vs", "versus", "and", "or", "like", "compared to"];

/// Classifies a ready-made phrase into an ATP category.
///
/// Bulk endpoints (e.g. DataForSEO Labs) return finished long-tail phrases
/// rather than one response per modifier, so the category has to be derived
/// from the phrase itself. Checks run most-specific first: comparisons beat
/// prepositions, which beat questions, because "coffee vs tea for sleep"
/// is more usefully a comparison than a preposition.
pub fn classify(phrase: &str, seed: &str) -> (Category, String) {
    let lower = phrase.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let has = |w: &str| {
        let parts: Vec<&str> = w.split(' ').collect();
        words
            .windows(parts.len())
            .any(|win| win == parts.as_slice())
    };

    // A leading question word decides the phrase outright: "how to brew coffee"
    // is a question even though it also contains the preposition "to".
    if let Some(first) = words.first() {
        if let Some(q) = QUESTION_WORDS.iter().find(|q| *q == first) {
            return (Category::Questions, q.to_string());
        }
    }
    for c in COMPARISONS {
        if has(c) {
            return (Category::Comparisons, c.to_string());
        }
    }
    for p in PREPOSITIONS {
        if has(p) {
            return (Category::Prepositions, p.to_string());
        }
    }
    for q in QUESTION_WORDS {
        if has(q) {
            return (Category::Questions, q.to_string());
        }
    }

    // Alphabetical: bucket by the first character after the seed, mirroring the
    // "keyword + letter" probes. A phrase that is just the seed has no bucket.
    let seed_lower = seed.trim().to_lowercase();
    let rest = match lower.strip_prefix(&seed_lower) {
        Some(r) if r.trim().is_empty() => return (Category::Related, "related".to_string()),
        Some(r) => r.trim(),
        None if lower.trim() == seed_lower => return (Category::Related, "related".to_string()),
        None => lower.trim(),
    };
    match rest.chars().find(|c| c.is_alphanumeric()) {
        Some(c) => (Category::Alphabetical, c.to_lowercase().to_string()),
        None => (Category::Related, "related".to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Suggestion {
    pub text: String,
    pub category: String,
    pub modifier: String,
    /// Monthly search volume, only available from paid providers.
    pub search_volume: Option<i64>,
    /// Cost per click in USD, only available from paid providers.
    pub cpc: Option<f64>,
    /// Paid competition index 0-100, only available from paid providers.
    pub competition: Option<i32>,
}

impl Suggestion {
    pub fn new(
        text: impl Into<String>,
        category: impl Into<String>,
        modifier: impl Into<String>,
    ) -> Self {
        Self {
            text: text.into(),
            category: category.into(),
            modifier: modifier.into(),
            search_volume: None,
            cpc: None,
            competition: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSummary {
    pub id: String,
    pub provider: String,
    pub keyword: String,
    pub language: String,
    pub country: String,
    pub status: String,
    pub error: Option<String>,
    pub suggestion_count: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub search: SearchSummary,
    pub suggestions: Vec<Suggestion>,
}

/// Group suggestions by (category, modifier) preserving the canonical order.
/// A modifier (e.g. "how") together with its suggestions.
pub type ModifierGroup = (String, Vec<Suggestion>);
/// Suggestions grouped by category and then modifier.
pub type GroupedSuggestions = Vec<(Category, Vec<ModifierGroup>)>;

pub fn group(suggestions: &[Suggestion]) -> GroupedSuggestions {
    let mut out = Vec::new();
    for cat in Category::all() {
        let mut groups: Vec<ModifierGroup> = Vec::new();
        for s in suggestions.iter().filter(|s| s.category == cat.as_str()) {
            match groups.iter_mut().find(|(m, _)| *m == s.modifier) {
                Some((_, items)) => items.push(s.clone()),
                None => groups.push((s.modifier.clone(), vec![s.clone()])),
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        if !groups.is_empty() {
            out.push((cat, groups));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str, cat: Category, m: &str) -> Suggestion {
        Suggestion::new(text, cat.as_str(), m)
    }

    #[test]
    fn groups_by_category_then_modifier() {
        let items = vec![
            s("how to brew coffee", Category::Questions, "how"),
            s("why coffee bitter", Category::Questions, "why"),
            s("how long coffee lasts", Category::Questions, "how"),
            s("coffee for sleep", Category::Prepositions, "for"),
        ];
        let g = group(&items);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, Category::Questions);
        // modifiers sorted alphabetically: how, why
        assert_eq!(g[0].1[0].0, "how");
        assert_eq!(g[0].1[0].1.len(), 2);
        assert_eq!(g[0].1[1].0, "why");
        assert_eq!(g[1].0, Category::Prepositions);
    }

    #[test]
    fn empty_categories_are_skipped() {
        assert!(group(&[]).is_empty());
    }

    #[test]
    fn classify_prefers_comparisons_then_prepositions_then_questions() {
        assert_eq!(classify("coffee vs tea", "coffee").0, Category::Comparisons);
        // comparison wins over the preposition "for"
        let (cat, m) = classify("coffee vs tea for sleep", "coffee");
        assert_eq!(cat, Category::Comparisons);
        assert_eq!(m, "vs");
        assert_eq!(
            classify("coffee for sleep", "coffee").0,
            Category::Prepositions
        );
        assert_eq!(
            classify("how to brew coffee", "coffee").0,
            Category::Questions
        );
    }

    #[test]
    fn classify_buckets_plain_phrases_alphabetically_after_the_seed() {
        let (cat, m) = classify("coffee grinder", "coffee");
        assert_eq!(cat, Category::Alphabetical);
        assert_eq!(m, "g");
        // seed not at the start still yields a stable bucket
        let (cat, m) = classify("best coffee beans", "coffee");
        assert_eq!(cat, Category::Alphabetical);
        assert_eq!(m, "b");
    }

    #[test]
    fn classify_matches_whole_words_not_substrings() {
        // "organic" contains "or", but must not count as a comparison
        assert_ne!(
            classify("organic coffee", "coffee").0,
            Category::Comparisons
        );
        // multi-word modifiers still match
        assert_eq!(
            classify("coffee compared to tea", "coffee").1,
            "compared to"
        );
    }

    #[test]
    fn classify_falls_back_to_related_for_the_bare_seed() {
        assert_eq!(classify("coffee", "coffee").0, Category::Related);
    }

    #[test]
    fn category_roundtrip() {
        for c in Category::all() {
            assert_eq!(Category::parse(c.as_str()), c);
        }
    }
}
