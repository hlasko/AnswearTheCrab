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

    pub fn from_str(s: &str) -> Category {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Suggestion {
    pub text: String,
    pub category: String,
    pub modifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSummary {
    pub id: String,
    pub keyword: String,
    pub language: String,
    pub country: String,
    pub status: String,
    pub error: Option<String>,
    pub suggestion_count: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    pub search: SearchSummary,
    pub suggestions: Vec<Suggestion>,
}

/// Group suggestions by (category, modifier) preserving the canonical order.
pub fn group(suggestions: &[Suggestion]) -> Vec<(Category, Vec<(String, Vec<String>)>)> {
    let mut out = Vec::new();
    for cat in Category::all() {
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for s in suggestions.iter().filter(|s| s.category == cat.as_str()) {
            match groups.iter_mut().find(|(m, _)| *m == s.modifier) {
                Some((_, items)) => items.push(s.text.clone()),
                None => groups.push((s.modifier.clone(), vec![s.text.clone()])),
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
        Suggestion {
            text: text.into(),
            category: cat.as_str().into(),
            modifier: m.into(),
        }
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
    fn category_roundtrip() {
        for c in Category::all() {
            assert_eq!(Category::from_str(c.as_str()), c);
        }
    }
}
