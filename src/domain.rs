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

/// Modifier vocabulary for one language.
///
/// The probe matrix and the phrase classifier are only as good as these lists.
/// Running an English-only vocabulary over Polish results dumped every phrase
/// into "alphabetical", which is useless, so each supported language brings its
/// own words. Unknown languages fall back to English.
pub struct Vocabulary {
    pub questions: &'static [&'static str],
    pub prepositions: &'static [&'static str],
    pub comparisons: &'static [&'static str],
}

const EN: Vocabulary = Vocabulary {
    questions: &[
        "how", "what", "why", "when", "where", "who", "which", "are", "can", "is", "does", "do",
        "should", "will",
    ],
    prepositions: &[
        "for", "with", "without", "to", "near", "on", "in", "at", "from", "by",
    ],
    comparisons: &[
        "vs",
        "versus",
        "and",
        "or",
        "like",
        "compared to",
        "better than",
        "instead of",
    ],
};

const PL: Vocabulary = Vocabulary {
    questions: &[
        "jak", "jaki", "jaka", "jakie", "jakim", "co", "czy", "czym", "dlaczego", "kiedy", "gdzie",
        "kto", "ile", "który", "która", "które", "po co", "za ile",
    ],
    prepositions: &[
        "dla", "do", "z", "ze", "bez", "na", "w", "we", "od", "pod", "przy", "o",
    ],
    // "czy" mid-phrase is the Polish "X or Y" comparison ("kawa czy herbata"),
    // while a leading "czy" is a yes/no question and is caught earlier.
    comparisons: &[
        "vs",
        "czy",
        "albo",
        "lub",
        "zamiast",
        "porównanie",
        "lepszy niż",
    ],
};

const DE: Vocabulary = Vocabulary {
    questions: &[
        "wie", "was", "warum", "wann", "wo", "wer", "welche", "welcher", "ist", "sind", "kann",
    ],
    prepositions: &[
        "für", "mit", "ohne", "zu", "bei", "auf", "in", "von", "nach",
    ],
    comparisons: &[
        "vs",
        "oder",
        "und",
        "wie",
        "besser als",
        "statt",
        "vergleich",
    ],
};

const ES: Vocabulary = Vocabulary {
    questions: &[
        "como", "cómo", "que", "qué", "por que", "por qué", "cuando", "cuándo", "donde", "dónde",
        "quien", "quién", "cual", "cuál", "cuanto", "cuánto", "es",
    ],
    prepositions: &["para", "con", "sin", "de", "en", "por", "a", "desde"],
    comparisons: &[
        "vs",
        "o",
        "y",
        "como",
        "mejor que",
        "en lugar de",
        "comparacion",
    ],
};

const FR: Vocabulary = Vocabulary {
    questions: &[
        "comment", "quoi", "que", "pourquoi", "quand", "ou", "où", "qui", "quel", "quelle",
        "combien", "est",
    ],
    prepositions: &[
        "pour", "avec", "sans", "de", "en", "sur", "dans", "chez", "par",
    ],
    comparisons: &[
        "vs",
        "ou",
        "et",
        "comme",
        "mieux que",
        "au lieu de",
        "comparaison",
    ],
};

/// Vocabulary for a language code, falling back to English.
pub fn vocabulary(language: &str) -> &'static Vocabulary {
    match language.trim().to_lowercase().as_str() {
        "pl" => &PL,
        "de" => &DE,
        "es" => &ES,
        "fr" => &FR,
        _ => &EN,
    }
}

/// Classifies a ready-made phrase into an ATP category.
///
/// Bulk endpoints (e.g. DataForSEO Labs) return finished long-tail phrases
/// rather than one response per modifier, so the category has to be derived
/// from the phrase itself. Checks run most-specific first: comparisons beat
/// prepositions, which beat questions, because "coffee vs tea for sleep"
/// is more usefully a comparison than a preposition.
pub fn classify(phrase: &str, seed: &str, language: &str) -> (Category, String) {
    let vocab = vocabulary(language);
    let lower = phrase.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    // Modifier words that are part of the seed itself carry no signal: the seed
    // "ekspres do kawy" contains the preposition "do", so without this every
    // phrase would look like a preposition hit.
    let seed_lower = seed.trim().to_lowercase();
    let seed_words: Vec<&str> = seed_lower.split_whitespace().collect();
    let in_seed = |w: &str| {
        let parts: Vec<&str> = w.split(' ').collect();
        seed_words
            .windows(parts.len())
            .any(|win| win == parts.as_slice())
    };

    let has = |w: &str| {
        if in_seed(w) {
            return false;
        }
        let parts: Vec<&str> = w.split(' ').collect();
        words
            .windows(parts.len())
            .any(|win| win == parts.as_slice())
    };

    // A leading question word decides the phrase outright: "how to brew coffee"
    // is a question even though it also contains the preposition "to", and
    // "jak odkamienić ekspres" is a question despite containing "do".
    if let Some(first) = words.first() {
        if let Some(q) = vocab.questions.iter().find(|q| q == &first && !in_seed(q)) {
            return (Category::Questions, q.to_string());
        }
    }
    for c in vocab.comparisons {
        if has(c) {
            return (Category::Comparisons, c.to_string());
        }
    }
    for q in vocab.questions {
        if has(q) {
            return (Category::Questions, q.to_string());
        }
    }
    for p in vocab.prepositions {
        if has(p) {
            return (Category::Prepositions, p.to_string());
        }
    }

    // Alphabetical: bucket by the first character after the seed, mirroring the
    // "keyword + letter" probes. A phrase that is just the seed has no bucket.
    //
    // The prefix only counts when the seed ends on a word boundary: for the seed
    // "cold brew coffee", the phrase "cold brew coffeedesk" is the brand
    // Coffeedesk, not the seed followed by "desk", so it belongs under "c".
    let trimmed = lower.trim();
    let after_seed = trimmed.strip_prefix(&seed_lower).filter(|r| {
        r.is_empty() || r.starts_with(|c: char| c.is_whitespace() || c == '-' || c == ',')
    });
    let rest = match after_seed {
        Some(r) if r.trim().is_empty() => return (Category::Related, "related".to_string()),
        Some(r) => r.trim(),
        None if trimmed == seed_lower => return (Category::Related, "related".to_string()),
        None => trimmed,
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

    fn cls(p: &str, seed: &str) -> (Category, String) {
        classify(p, seed, "en")
    }

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
        assert_eq!(cls("coffee vs tea", "coffee").0, Category::Comparisons);
        // comparison wins over the preposition "for"
        let (cat, m) = cls("coffee vs tea for sleep", "coffee");
        assert_eq!(cat, Category::Comparisons);
        assert_eq!(m, "vs");
        assert_eq!(cls("coffee for sleep", "coffee").0, Category::Prepositions);
        assert_eq!(cls("how to brew coffee", "coffee").0, Category::Questions);
    }

    #[test]
    fn classify_buckets_plain_phrases_alphabetically_after_the_seed() {
        let (cat, m) = cls("coffee grinder", "coffee");
        assert_eq!(cat, Category::Alphabetical);
        assert_eq!(m, "g");
        // seed not at the start still yields a stable bucket
        let (cat, m) = cls("best coffee beans", "coffee");
        assert_eq!(cat, Category::Alphabetical);
        assert_eq!(m, "b");
    }

    #[test]
    fn classify_matches_whole_words_not_substrings() {
        // "organic" contains "or", but must not count as a comparison
        assert_ne!(cls("organic coffee", "coffee").0, Category::Comparisons);
        // multi-word modifiers still match
        assert_eq!(cls("coffee compared to tea", "coffee").1, "compared to");
    }

    #[test]
    fn classify_requires_a_word_boundary_after_the_seed() {
        // Real result from Google for "cold brew coffee": Coffeedesk is a shop,
        // so the phrase is not the seed plus "desk" and must bucket under "c".
        let (cat, m) = cls("cold brew coffeedesk", "cold brew coffee");
        assert_eq!(cat, Category::Alphabetical);
        assert_eq!(m, "c");

        // A genuine suffix after a space still buckets on that word.
        assert_eq!(cls("cold brew coffee maker", "cold brew coffee").1, "m");
        // Hyphens count as a boundary too.
        assert_eq!(cls("cold brew coffee-maker", "cold brew coffee").1, "m");
    }

    #[test]
    fn classify_falls_back_to_related_for_the_bare_seed() {
        assert_eq!(cls("coffee", "coffee").0, Category::Related);
    }

    #[test]
    fn classify_understands_polish() {
        let seed = "ekspres do kawy";
        // Real phrases returned by DataForSEO for this seed. With an
        // English-only vocabulary all of these fell into "alphabetical".
        let q = classify("jak odkamienić ekspres do kawy", seed, "pl");
        assert_eq!(q.0, Category::Questions);
        assert_eq!(q.1, "jak");
        assert_eq!(
            classify("jaki ekspres do kawy do domu", seed, "pl").0,
            Category::Questions
        );
        assert_eq!(
            classify("ile kosztuje ekspres do kawy", seed, "pl").1,
            "ile"
        );

        // "dla" is a preposition, and must not be shadowed by the "do" in the seed.
        assert_eq!(
            classify("ekspres do kawy dla dzieci", seed, "pl").0,
            Category::Prepositions
        );

        // Mid-phrase "czy" is a comparison; leading "czy" is a question.
        assert_eq!(
            classify("kawa czy herbata", "kawa", "pl").0,
            Category::Comparisons
        );
        assert_eq!(
            classify("czy ekspres do kawy sie oplaca", seed, "pl").0,
            Category::Questions
        );

        // Plain product phrases still bucket alphabetically.
        assert_eq!(
            classify("ekspres do kawy philips", seed, "pl").0,
            Category::Alphabetical
        );
    }

    #[test]
    fn classify_falls_back_to_english_for_unknown_languages() {
        assert_eq!(
            classify("how to brew coffee", "coffee", "xx").0,
            Category::Questions
        );
    }

    #[test]
    fn category_roundtrip() {
        for c in Category::all() {
            assert_eq!(Category::parse(c.as_str()), c);
        }
    }
}
