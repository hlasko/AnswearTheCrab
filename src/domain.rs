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

/// A searchable market: a country paired with a language the search engine
/// actually serves there.
///
/// DataForSEO rejects unsupported pairs outright ("Invalid Field:
/// 'language_code'"), and Poland for instance only offers `pl`. Offering the two
/// dropdowns independently let users build combinations that always fail, so the
/// UI presents valid markets instead.
pub struct Market {
    pub label: &'static str,
    pub language: &'static str,
    pub country: &'static str,
}

pub const MARKETS: [Market; 8] = [
    Market {
        label: "United States (English)",
        language: "en",
        country: "us",
    },
    Market {
        label: "United Kingdom (English)",
        language: "en",
        country: "gb",
    },
    Market {
        label: "Polska (polski)",
        language: "pl",
        country: "pl",
    },
    Market {
        label: "Deutschland (Deutsch)",
        language: "de",
        country: "de",
    },
    Market {
        label: "España (español)",
        language: "es",
        country: "es",
    },
    Market {
        label: "France (français)",
        language: "fr",
        country: "fr",
    },
    Market {
        label: "Canada (English)",
        language: "en",
        country: "ca",
    },
    Market {
        label: "Australia (English)",
        language: "en",
        country: "au",
    },
];

/// Looks up a market by its `"<language>-<country>"` key, defaulting to the
/// first entry when the key is empty.
pub fn market(key: &str) -> Option<&'static Market> {
    let key = key.trim().to_lowercase();
    if key.is_empty() {
        return MARKETS.first();
    }
    MARKETS
        .iter()
        .find(|m| format!("{}-{}", m.language, m.country) == key)
}

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

/// All vocabularies, for language detection.
const ALL_VOCABULARIES: [(&str, &Vocabulary); 5] = [
    ("en", &EN),
    ("pl", &PL),
    ("de", &DE),
    ("es", &ES),
    ("fr", &FR),
];

/// Counts how many phrases start with a question word from `vocab`.
fn leading_question_hits(phrases: &[String], vocab: &Vocabulary) -> usize {
    phrases
        .iter()
        .filter(|p| {
            let lower = p.to_lowercase();
            match lower.split_whitespace().next() {
                Some(first) => vocab.questions.contains(&first),
                None => false,
            }
        })
        .count()
}

/// Picks the vocabulary that best explains a batch of phrases.
///
/// The language dropdown is easy to leave on the wrong value: searching the
/// Polish phrase "kredyt hipoteczny" with the default `en` put 626 of 637
/// results into "alphabetical", because no Polish question word was recognised.
/// Rather than trusting the form, look at the phrases themselves and keep the
/// requested language only when nothing else explains them better.
pub fn detect_vocabulary(phrases: &[String], requested: &str) -> &'static Vocabulary {
    let requested_vocab = vocabulary(requested);
    let baseline = leading_question_hits(phrases, requested_vocab);

    let mut best = requested_vocab;
    let mut best_hits = baseline;
    for (_, vocab) in ALL_VOCABULARIES {
        let hits = leading_question_hits(phrases, vocab);
        // Require a clear margin so a couple of coincidental matches cannot
        // override an explicitly chosen language.
        if hits > best_hits * 2 && hits > 2 {
            best = vocab;
            best_hits = hits;
        }
    }
    best
}

/// Classifies a ready-made phrase into an ATP category.
///
/// Bulk endpoints (e.g. DataForSEO Labs) return finished long-tail phrases
/// rather than one response per modifier, so the category has to be derived
/// from the phrase itself. Checks run most-specific first: comparisons beat
/// prepositions, which beat questions, because "coffee vs tea for sleep"
/// is more usefully a comparison than a preposition.
pub fn classify(phrase: &str, seed: &str, language: &str) -> (Category, String) {
    classify_with(phrase, seed, vocabulary(language))
}

/// [`classify`] against an explicit vocabulary, so a batch can detect the
/// language once instead of per phrase.
pub fn classify_with(phrase: &str, seed: &str, vocab: &Vocabulary) -> (Category, String) {
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
    fn detects_polish_phrases_despite_an_english_language_setting() {
        // Reproduces a real search: the keyword "kredyt hipoteczny" was sent with
        // the form still on "en", and 626 of 637 phrases fell into alphabetical.
        let phrases: Vec<String> = [
            "ile wkładu własnego na kredyt hipoteczny",
            "czy warto nadpłacać kredyt hipoteczny",
            "jaki kredyt hipoteczny",
            "jak się rozwieść mając kredyt hipoteczny",
            "ile trzeba zarabiać na kredyt hipoteczny",
            "kredyt hipoteczny kalkulator",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let vocab = detect_vocabulary(&phrases, "en");
        let cats: Vec<Category> = phrases
            .iter()
            .map(|p| classify_with(p, "kredyt hipoteczny", vocab).0)
            .collect();

        assert_eq!(
            cats.iter().filter(|c| **c == Category::Questions).count(),
            5,
            "Polish question words should be recognised: {cats:?}"
        );
        assert_eq!(
            cats[5],
            Category::Alphabetical,
            "plain phrase stays alphabetical"
        );
    }

    #[test]
    fn detection_keeps_the_requested_language_for_matching_content() {
        let english: Vec<String> = [
            "how to brew coffee",
            "what is cold brew",
            "why coffee is bitter",
            "when to drink coffee",
            "coffee grinder",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let vocab = detect_vocabulary(&english, "en");
        assert_eq!(
            classify_with("how to brew coffee", "coffee", vocab).0,
            Category::Questions
        );

        // A handful of ambiguous words must not hijack an explicit choice.
        let ambiguous: Vec<String> = ["a coffee", "o coffee", "coffee"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let vocab = detect_vocabulary(&ambiguous, "en");
        assert_eq!(
            classify_with("how to brew coffee", "coffee", vocab).0,
            Category::Questions,
            "English vocabulary should survive a few stray matches"
        );
    }

    #[test]
    fn markets_are_valid_and_looked_up_by_key() {
        assert_eq!(market("pl-pl").unwrap().language, "pl");
        assert_eq!(market("EN-GB").unwrap().country, "gb");
        // Empty falls back to the first market rather than erroring.
        assert!(market("").is_some());
        // A pair the search engine does not serve is not offered at all.
        assert!(market("en-pl").is_none());
        assert!(market("nonsense").is_none());

        // Every market must map to a vocabulary we actually have, otherwise
        // classification silently degrades to English.
        for m in MARKETS.iter() {
            let v = vocabulary(m.language);
            assert!(
                !v.questions.is_empty(),
                "no vocabulary for market {}",
                m.label
            );
        }
    }

    #[test]
    fn category_roundtrip() {
        for c in Category::all() {
            assert_eq!(Category::parse(c.as_str()), c);
        }
    }
}
