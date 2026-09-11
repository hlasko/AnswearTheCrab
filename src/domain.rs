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

/// Where suggestions are harvested from.
///
/// Each source is a different search box with its own audience: Google is
/// general web intent, YouTube is what people want to watch, Bing skews older
/// and more desktop, Amazon is purchase intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Google,
    YouTube,
    Bing,
    /// Perplexity's search box. What people ask an assistant, in their own
    /// words; no volume exists for it.
    Perplexity,
}

impl Source {
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Google => "google",
            Source::YouTube => "youtube",
            Source::Bing => "bing",
            Source::Perplexity => "perplexity",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Source::Google => "Google",
            Source::YouTube => "YouTube",
            Source::Bing => "Bing",
            Source::Perplexity => "Perplexity",
        }
    }

    pub fn all() -> [Source; 4] {
        [
            Source::Google,
            Source::YouTube,
            Source::Bing,
            Source::Perplexity,
        ]
    }

    /// Unknown values fall back to Google, which is the default source.
    pub fn parse(s: &str) -> Source {
        match s.trim().to_lowercase().as_str() {
            "youtube" | "yt" => Source::YouTube,
            "bing" => Source::Bing,
            "perplexity" | "pplx" => Source::Perplexity,
            _ => Source::Google,
        }
    }
}

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
    /// Twelve months of volume, oldest first, when the provider gives it.
    #[serde(default)]
    pub monthly: Vec<i64>,
    /// Change over the past year, in percent, when the provider gives it.
    #[serde(default)]
    pub trend_yearly: Option<i32>,
    /// Change over the past quarter, in percent.
    #[serde(default)]
    pub trend_quarterly: Option<i32>,
    /// Monthly searches on Bing, from Microsoft Advertising. Only for the
    /// six countries Bing publishes for; None elsewhere.
    #[serde(default)]
    pub bing_volume: Option<i64>,
    /// How often the phrase's words appear in questions, per DataForSEO's
    /// PAA-derived "AI search volume". Relative, not a count of AI queries.
    #[serde(default)]
    pub ai_volume: Option<i64>,
    /// Twelve months of the above, oldest first.
    #[serde(default)]
    pub ai_monthly: Vec<i64>,
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
            monthly: Vec::new(),
            trend_yearly: None,
            trend_quarterly: None,
            bing_volume: None,
            ai_volume: None,
            ai_monthly: Vec::new(),
        }
    }

    /// Asked more than googled: the AI-question figure is at least 5% of
    /// Google's query volume.
    ///
    /// Calibrated twice. On seeds: "lokata" 284 vs 14.8K (1.9%) and "jak
    /// inwestować" 38 vs 590 (6.4%) ask, "obligacje skarbowe" 105 vs 165K
    /// (0.06%) googles. Then on a 699-phrase run, where a floor of 20 on the
    /// AI figure left one phrase: the long tail sits at 5-19, so the floor
    /// is 5, and the ratio rises to 5% because at 1.5% half the tail would
    /// qualify. What passes: "kredyt hipoteczny czy warto" 12 vs 50 (24%),
    /// "ile trzeba zarabiać żeby dostać kredyt hipoteczny" 13 vs 90 (14%),
    /// "w jakim banku kredyt hipoteczny" 10 vs 110 (9%). Questions, all.
    pub fn is_asked(&self) -> bool {
        match (self.ai_volume, self.search_volume) {
            (Some(a), Some(g)) if a >= 5 && g >= 50 => a * 100 >= g * 5,
            _ => false,
        }
    }

    /// Whether this phrase is the seed's words in another order. DataForSEO's
    /// AI figure counts questions containing all of a phrase's words in any
    /// order, so "hipoteczny kredyt" inherits every question about "kredyt
    /// hipoteczny" (231) while having 170 Google searches, and would top the
    /// asked list on a technicality.
    pub fn is_permutation_of(&self, seed: &str) -> bool {
        let mut a: Vec<String> = self
            .text
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        let mut b: Vec<String> = seed.split_whitespace().map(|w| w.to_lowercase()).collect();
        a.sort();
        b.sort();
        a == b
    }
}

/// Countries Bing publishes search volume for, as of a check on 2026-09-10
/// against DataForSEO's `keywords_data/bing/locations`: 20 537 locations,
/// six countries, no Poland. Anything else is rejected at the request, so
/// it is not worth sending.
pub fn bing_volume_available(language: &str, country: &str) -> bool {
    let c = country.to_lowercase();
    let l = language.to_lowercase();
    matches!(
        (l.as_str(), c.as_str()),
        ("en", "us" | "gb" | "uk" | "ca" | "au") | ("de", "de") | ("fr", "fr")
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSummary {
    pub id: String,
    pub provider: String,
    pub source: String,
    pub keyword: String,
    pub language: String,
    pub country: String,
    pub status: String,
    pub error: Option<String>,
    pub suggestion_count: i32,
    pub created_at: String,
    /// How old the run is, in words: "2 days ago".
    ///
    /// Computed on the server because the date library is a server-only
    /// dependency; shipping it to the browser to render one phrase would be a
    /// poor trade.
    pub age: String,
}

/// A phrase a competitor ranks for and the user's site does not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GapPhrase {
    pub keyword: String,
    pub volume: Option<i64>,
    pub cpc: Option<f64>,
    pub competitor_rank: i32,
    /// The competitor's page that ranks, so the reader can see what beat them.
    pub competitor_url: String,
}

/// One keyword-gap comparison, stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GapRun {
    pub id: String,
    pub competitor: String,
    pub mine: String,
    pub language: String,
    pub country: String,
    pub phrases: Vec<GapPhrase>,
    /// How many phrases matched in total; `phrases` holds the top slice.
    pub total: i32,
    pub created_at: String,
}

/// A watched topic and its latest movement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatchSummary {
    pub id: String,
    pub keyword: String,
    pub source: String,
    pub language: String,
    pub country: String,
    pub every_days: i32,
    pub enabled: bool,
    /// Newest finished search for this watch, for linking.
    pub latest_search: Option<String>,
    /// Added and removed since the run before, when both exist.
    pub added: i32,
    pub removed: i32,
    /// Age of the newest run, in words.
    pub age: String,
}

/// How hard a topic's current top 10 is to displace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    /// Blogs and small sites. A good page can rank.
    Open,
    /// Mixed. Rankable with real depth and some authority.
    Contested,
    /// Banks, national portals, brands. Ranking needs more than content.
    Entrenched,
}

impl Difficulty {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Contested => "Contested",
            Self::Entrenched => "Entrenched",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Contested => "contested",
            Self::Entrenched => "entrenched",
        }
    }
    pub fn advice(self) -> &'static str {
        match self {
            Self::Open => {
                "The pages that rank are blogs and small sites. A thorough page with real \
                 depth can take a place here."
            }
            Self::Contested => {
                "A mix of authorities. Rankable, but the page has to be clearly better than \
                 what is there, not just present."
            }
            Self::Entrenched => {
                "Banks, national portals or brands hold the top 10. Content alone rarely \
                 displaces them; aim for the AI answer and the long tail instead."
            }
        }
    }
}

/// A draft's claims checked against the pages that rank.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DraftCheck {
    pub total: usize,
    pub backed: usize,
    /// Claims found in no source, for a human to verify before publishing.
    pub unbacked: Vec<UncheckedClaim>,
    /// How many sources the check ran against; zero means it could not run.
    pub sources: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UncheckedClaim {
    pub text: String,
    /// "figure" or "attribution".
    pub kind: String,
}

/// The same keyword seen through different search engines.
///
/// Each engine's autosuggest reflects its own audience: a phrase all three
/// agree on has demand everywhere, while one only YouTube offers is a video
/// topic, and one only Bing offers tends to skew older and desktop-bound.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceOverlap {
    pub keyword: String,
    /// Sources that have a finished run for this keyword and market.
    pub sources: Vec<String>,
    /// Phrases every listed source suggests. The safest topics.
    pub shared: Vec<Suggestion>,
    /// Phrases only one source suggests, keyed by that source.
    pub only: Vec<(String, Vec<Suggestion>)>,
}

/// Difference between two runs of the same keyword and source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchDiff {
    pub previous: SearchSummary,
    pub current: SearchSummary,
    /// Phrases present now but not in the earlier run.
    pub added: Vec<Suggestion>,
    /// Phrases that were there before and are gone now.
    pub removed: Vec<Suggestion>,
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

    fn brief_with(pages: &[(usize, bool)], ai: bool) -> Brief {
        Brief {
            id: "x".into(),
            topic: "t".into(),
            language: "pl".into(),
            country: "pl".into(),
            status: "done".into(),
            error: None,
            ai_overview: ai.then(|| "answer".to_string()),
            ai_sources: vec![],
            questions: vec![],
            related: vec![],
            created_at: String::new(),
            competitors: pages
                .iter()
                .enumerate()
                .map(|(i, (n, faq))| Competitor {
                    rank: i as i32 + 1,
                    url: format!("https://e{i}.example"),
                    domain: format!("e{i}.example"),
                    title: None,
                    description: None,
                    parsed: true,
                    content: None,
                    domain_rank: None,
                    headings: (0..*n)
                        .map(|j| Heading {
                            level: 2,
                            title: if *faq && j + 1 == *n {
                                "Najczęstsze pytania".into()
                            } else {
                                format!("Sekcja {j}")
                            },
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn format_advice_reads_the_shape_of_ranking_pages() {
        // Numbers taken from a real brief: "jak odkamienić ekspres do kawy"
        // had 5 readable competitors, a median of 6 sections and 2 with a FAQ.
        let b = brief_with(
            &[(6, true), (18, true), (5, false), (10, false), (4, false)],
            true,
        );
        let a = b.format_advice().unwrap();
        assert_eq!(a.sample, 5);
        assert_eq!(
            a.sections, 6,
            "median, so the 18-heading outlier cannot skew it"
        );
        assert_eq!(a.with_faq, 2);
        assert!(
            a.headline.contains("FAQ"),
            "2 of 5 is enough to suggest one: {}",
            a.headline
        );
        assert!(
            a.detail.contains("Google already answers"),
            "AI overview must change the advice"
        );
        assert!(
            !a.detail.contains("  "),
            "no double spaces from line wrapping: {}",
            a.detail
        );
    }

    #[test]
    fn format_advice_omits_faq_when_competitors_do_not_use_one() {
        // "ile kalorii ma latte": 7 readable, median 9, none with a FAQ.
        let b = brief_with(
            &[
                (9, false),
                (8, false),
                (10, false),
                (9, false),
                (7, false),
                (12, false),
                (9, false),
            ],
            false,
        );
        let a = b.format_advice().unwrap();
        assert_eq!(a.sections, 9);
        assert_eq!(a.with_faq, 0);
        assert!(!a.headline.contains("FAQ"));
        assert!(a.detail.contains("none use a FAQ"));
        assert!(!a.detail.contains("  "), "no double spaces: {}", a.detail);
    }

    #[test]
    fn outline_filter_rejects_a_sidebar_of_unrelated_posts() {
        let h = |t: &str| Heading {
            level: 2,
            title: t.into(),
        };
        // Real extraction from zdrowie.pap.pl on the topic "czy kawa jest
        // zdrowa": one relevant heading, then the site's recent posts.
        let sidebar = vec![
            h("Czy picie kawy jest zdrowe? – Co jest faktem, a co mitem?"),
            h("Soja w chorobach tarczycy"),
            h("Skład i pora posiłków wpływają na późniejszy sen"),
            h("Grzyby chronią komórki?"),
            h("Mammografia a USG piersi"),
            h("zapisz się na newsletter"),
        ];
        assert!(!outline_looks_like_an_article(
            &sidebar,
            "czy kawa jest zdrowa"
        ));

        // Real extraction from dietetycy.org.pl on the same topic.
        let article = vec![
            h("Kawa źródłem polifenoli oraz kofeiny"),
            h("W której kawie znajdziemy najwięcej polifenoli?"),
            h("Ile kofeiny to za dużo?"),
            h("Kawa a choroby wątroby"),
            h("Kawa a układ sercowo-naczyniowy"),
            h("Podsumowanie"),
        ];
        assert!(outline_looks_like_an_article(
            &article,
            "czy kawa jest zdrowa"
        ));
    }

    #[test]
    fn prompt_asks_for_the_language_of_the_brief() {
        let mut b = brief_with(&[(6, false), (7, false), (8, false)], true);
        b.topic = "czy kawa jest zdrowa".into();
        b.language = "pl".into();
        let p = brief_prompt(&b);
        assert!(
            p.starts_with("Write an article in Polish"),
            "got: {}",
            &p[..60]
        );
        // The AI answer is context to go beyond, not text to restate.
        assert!(p.contains("Do not restate it"));
        // No keyword stuffing instructions.
        assert!(p.contains("No keyword repetition"));
    }

    #[test]
    fn faq_prompt_keeps_the_questions_and_asks_for_standalone_answers() {
        let mut b = brief_with(&[(6, false), (7, false)], true);
        b.topic = "ile kalorii ma latte".into();
        b.language = "pl".into();
        b.questions = vec![
            "Ile kalorii ma latte na mleku owsianym?".into(),
            "Czy latte tuczy?".into(),
        ];
        let p = faq_prompt(&b);

        assert!(
            p.starts_with("Write a FAQ section in Polish"),
            "got: {}",
            &p[..40]
        );
        // The questions are the headings; losing one loses a heading.
        for q in &b.questions {
            assert!(p.contains(q.as_str()), "missing question: {q}");
        }
        assert!(p.contains("wording intact"));
        // These answers get quoted away from the page they live on.
        assert!(p.contains("make sense on its own"));
        // A FAQ is answers, not an essay with an intro.
        assert!(p.contains("No introduction"));
        // The article instructions must not leak in.
        assert!(
            !p.contains("Ground the ranking pages cover"),
            "article section leaked in"
        );
    }

    #[test]
    fn faq_markers_cover_the_spellings_pages_actually_use() {
        let faq_heading = |title: &str| {
            let t = title.to_lowercase();
            FAQ_MARKERS.iter().any(|m| t.contains(m))
        };
        // Headings taken verbatim from competitors in the database.
        for h in [
            "FAQ – najczęściej zadawane pytania",
            "FAQ - pytania i odpowiedzi",
            "FAQ",
            "Najczęstsze pytania",
        ] {
            assert!(faq_heading(h), "should detect: {h}");
        }
        // Very common in Polish and shares no substring with "najczęstsze
        // pytania", so it needs its own marker rather than relying on "FAQ".
        assert!(faq_heading("Najczęściej zadawane pytania"));
        assert!(faq_heading("Häufig gestellte Fragen"));
        // Ordinary sections must not be mistaken for a FAQ.
        for h in ["Podsumowanie", "Bibliografia", "Ile kofeiny to za dużo?"] {
            assert!(!faq_heading(h), "should not detect: {h}");
        }
    }

    #[test]
    fn format_advice_needs_enough_readable_competitors() {
        // Parsing fails for about 40% of pages, so a brief can end up with too
        // little to say anything honest. Silence beats a guess from one page.
        assert!(brief_with(&[(6, true), (5, false)], true)
            .format_advice()
            .is_none());
        assert!(brief_with(&[(6, true), (5, false), (7, false)], true)
            .format_advice()
            .is_some());
    }

    #[test]
    fn very_short_competitors_mean_it_is_not_an_article() {
        // "zielona kawa gdzie kupić" had a median of 4; a listing page rather
        // than a guide. Two or fewer sections is not an article at all.
        let b = brief_with(&[(1, false), (2, false), (2, false), (0, false)], false);
        let a = b.format_advice().unwrap();
        assert!(a.headline.contains("Short page"), "got: {}", a.headline);
    }

    #[test]
    fn sources_round_trip_and_default_to_google() {
        for s in Source::all() {
            assert_eq!(Source::parse(s.as_str()), s);
        }
        // Rows written before multi-source support, and any unknown value,
        // must read back as Google rather than failing.
        assert_eq!(Source::parse(""), Source::Google);
        assert_eq!(Source::parse("nonsense"), Source::Google);
        assert_eq!(Source::parse("YouTube"), Source::YouTube);
        assert_eq!(Source::parse("yt"), Source::YouTube);
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

// ---------------------------------------------------------------------------
// Content Writer
// ---------------------------------------------------------------------------

/// A page competing for a topic, with whatever structure we managed to read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Competitor {
    pub rank: i32,
    pub url: String,
    pub domain: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// `(level, title)` pairs, e.g. `(2, "Why descaling matters")`.
    ///
    /// Empty when the page could not be parsed. Measured success rate is about
    /// 61%, so a brief has to stay useful without this.
    pub headings: Vec<Heading>,
    pub parsed: bool,
    /// Body text of the page, when parsing succeeded. Used to check a
    /// draft's figures and claims against what already ranks.
    #[serde(default)]
    pub content: Option<String>,
    /// Domain authority, 0-1000. How hard this page is to displace.
    #[serde(default)]
    pub domain_rank: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Heading {
    pub level: i32,
    pub title: String,
}

/// Everything gathered about one topic, independent of where it came from.
///
/// This is the seam that keeps the architecture open: research sources fill this
/// in, and the UI and exports only ever read it. Adding AEO/GEO later means
/// adding a source that contributes to these fields, not reshaping the type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Brief {
    pub id: String,
    pub topic: String,
    pub language: String,
    pub country: String,
    pub status: String,
    pub error: Option<String>,
    /// Google's AI answer, absent for roughly half of topics.
    pub ai_overview: Option<String>,
    /// Domains cited by that answer: the concrete target list for AEO work.
    pub ai_sources: Vec<String>,
    /// "People also ask" entries, usable as FAQ headings verbatim.
    pub questions: Vec<String>,
    pub related: Vec<String>,
    pub competitors: Vec<Competitor>,
    pub created_at: String,
}

/// One recorded citation check, for showing change over time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CitationRecord {
    pub domain: String,
    pub topic: String,
    pub cited: bool,
    pub citation_rank: Option<i32>,
    pub organic_rank: Option<i32>,
    pub created_at: String,
}

/// What the pages that already rank suggest you should write.
///
/// Derived from competitor structure rather than guessed: the brief already
/// knows how many sections ranking pages use and whether they close with a FAQ,
/// so it can say "article, ~7 sections, with a FAQ" instead of leaving the
/// reader to infer it from raw headings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormatAdvice {
    /// Typical number of sections among competitors that could be read.
    pub sections: usize,
    /// How many of them close with a FAQ block.
    pub with_faq: usize,
    /// How many could be read at all; the sample the advice rests on.
    pub sample: usize,
    pub headline: String,
    pub detail: String,
}

/// Heading titles that mark a FAQ block, in the languages we support.
///
/// Taken from headings actually seen on ranking pages. Polish in particular has
/// several spellings that share no common substring: "najczęstsze pytania" and
/// "najczęściej zadawane pytania" both occur, and the latter often appears
/// without the word FAQ anywhere in the heading.
const FAQ_MARKERS: [&str; 12] = [
    // Polish
    "najczęstsze pytania",
    "najczęściej zadawane",
    "często zadawane",
    "pytania i odpowiedzi",
    // Language independent
    "faq",
    // English
    "frequently asked",
    "common questions",
    // German
    "häufige fragen",
    "häufig gestellte",
    // Spanish
    "preguntas frecuentes",
    // French
    "questions fréquentes",
    "questions fréquemment",
];

impl Brief {
    /// Turns competitor structure into a concrete instruction about format.
    ///
    /// Returns `None` when too few competitors could be read to say anything
    /// honest; a recommendation from one page would be noise.
    pub fn format_advice(&self) -> Option<FormatAdvice> {
        let mut sizes: Vec<usize> = self
            .competitors
            .iter()
            .filter(|c| c.parsed)
            .map(|c| c.headings.len())
            .collect();
        if sizes.len() < 3 {
            return None;
        }
        sizes.sort_unstable();
        // Median rather than mean: one outlier with 40 headings should not
        // stretch the recommendation.
        let sections = sizes[sizes.len() / 2];
        let sample = sizes.len();

        let with_faq = self
            .competitors
            .iter()
            .filter(|c| c.parsed)
            .filter(|c| {
                c.headings.iter().any(|h| {
                    let t = h.title.to_lowercase();
                    FAQ_MARKERS.iter().any(|m| t.contains(m))
                })
            })
            .count();

        // A FAQ is worth recommending when a meaningful share of the pages that
        // rank actually use one.
        let faq_common = with_faq * 3 >= sample;

        let headline = match (sections, faq_common) {
            (0..=2, _) => "Short page or a list, not an article".to_string(),
            (s, true) => format!("Article of about {s} sections, closing with a FAQ"),
            (s, false) => format!("Article of about {s} sections"),
        };

        let mut detail =
            format!("Based on {sample} readable competitors: they use {sections} sections");
        if with_faq > 0 {
            detail.push_str(&format!(", and {with_faq} close with a FAQ"));
        } else {
            detail.push_str(", and none use a FAQ block");
        }
        detail.push('.');

        if self.ai_overview.is_some() {
            detail.push_str(
                " Google already answers this itself, so a general overview adds nothing:",
            );
            detail.push_str(" the value is in the specifics its summary cannot carry.");
        }

        Some(FormatAdvice {
            sections,
            with_faq,
            sample,
            headline,
            detail,
        })
    }

    /// Headings that several competitors share, strongest first.
    ///
    /// Agreement between independently ranking pages is the useful signal here:
    /// if four of five cover "why descaling matters", the section is expected.
    pub fn common_sections(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for c in &self.competitors {
            // Count each competitor once per distinct heading.
            let mut seen: Vec<String> = Vec::new();
            for h in &c.headings {
                let key = h.title.trim().to_lowercase();
                if key.is_empty() || seen.contains(&key) {
                    continue;
                }
                seen.push(key.clone());
                match counts.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((key, 1)),
                }
            }
        }
        counts.retain(|(_, n)| *n > 1);
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        counts
    }

    /// Median domain authority of the competitors, 0-1000, if known.
    ///
    /// The median rather than the mean: one nespresso.com at 484 among
    /// blogs at 260 should not make a coffee topic look like a bank topic.
    pub fn median_domain_rank(&self) -> Option<i32> {
        let mut v: Vec<i32> = self
            .competitors
            .iter()
            .filter_map(|c| c.domain_rank)
            .collect();
        if v.is_empty() {
            return None;
        }
        v.sort_unstable();
        Some(v[v.len() / 2])
    }

    /// What the competition's authority means for someone entering the topic.
    ///
    /// Bands come from measured topics: coffee health at a median of 317
    /// (blogs and a health portal), mortgages at 548 (banks and finance
    /// portals). The thresholds sit between those, not at round numbers.
    pub fn difficulty(&self) -> Option<Difficulty> {
        let m = self.median_domain_rank()?;
        Some(if m < 350 {
            Difficulty::Open
        } else if m < 480 {
            Difficulty::Contested
        } else {
            Difficulty::Entrenched
        })
    }

    /// How many competitors yielded structure, for honest reporting in the UI.
    pub fn parsed_count(&self) -> usize {
        self.competitors.iter().filter(|c| c.parsed).count()
    }
}

/// A generated draft.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Draft {
    pub id: String,
    pub brief_id: String,
    pub status: String,
    pub error: Option<String>,
    pub model: String,
    pub content: Option<String>,
    pub created_at: String,
    /// "article" or "faq".
    pub kind: String,
    /// What was asked for, when this draft is a revision of an earlier one.
    #[serde(default)]
    pub instruction: Option<String>,
}

/// Renders a brief as markdown, suitable for pasting into an LLM or a doc.
///
/// Written as instructions about coverage rather than a list of keywords: the
/// point is which questions a piece must answer and which sections readers
/// expect, not which strings to repeat.
pub fn brief_markdown(b: &Brief) -> String {
    let mut m = String::new();
    m.push_str(&format!("# Content brief: {}\n\n", b.topic));

    // The recommendation comes first: it is the question a writer asks before
    // reading anything else.
    if let Some(a) = b.format_advice() {
        m.push_str(&format!(
            "## What to write\n\n**{}**\n\n{}\n\n",
            a.headline, a.detail
        ));
    }

    if let Some(ai) = &b.ai_overview {
        m.push_str("## What Google's AI already answers\n\n");
        m.push_str(ai);
        m.push_str("\n\n");
        if !b.ai_sources.is_empty() {
            m.push_str(&format!(
                "Cited sources: {}\n\nTo be cited alongside these, the piece has to answer the \
                 question at least as directly.\n\n",
                b.ai_sources.join(", ")
            ));
        }
    } else {
        m.push_str(
            "## No AI answer\n\nGoogle shows no AI overview for this topic, so classic ranking \
             still decides visibility.\n\n",
        );
    }

    if !b.questions.is_empty() {
        m.push_str("## Questions the piece must answer\n\n");
        for q in &b.questions {
            m.push_str(&format!("- {q}\n"));
        }
        m.push('\n');
    }

    let sections = b.common_sections();
    if !sections.is_empty() {
        m.push_str("## Sections the ranking pages agree on\n\n");
        for (title, n) in sections {
            m.push_str(&format!("- {title} (used by {n} competitors)\n"));
        }
        m.push('\n');
    }

    if !b.competitors.is_empty() {
        m.push_str(&format!(
            "## Competitors ({} found, {} readable)\n\n",
            b.competitors.len(),
            b.parsed_count()
        ));
        for c in &b.competitors {
            m.push_str(&format!("### #{} {}\n", c.rank, c.domain));
            if let Some(t) = &c.title {
                m.push_str(&format!("{t}\n"));
            }
            m.push_str(&format!("{}\n", c.url));
            for h in &c.headings {
                m.push_str(&format!(
                    "{} {}\n",
                    "#".repeat((h.level as usize).clamp(1, 6)),
                    h.title
                ));
            }
            m.push('\n');
        }
    }

    if !b.related.is_empty() {
        m.push_str("## Related searches\n\n");
        for r in &b.related {
            m.push_str(&format!("- {r}\n"));
        }
    }
    m
}

/// Renders a brief as a prompt ready to paste into any chat model.
///
/// Deliberately written as instructions about *coverage* rather than keywords.
/// The competitors' headings say which ground readers expect a piece to cover,
/// and Google's own answer says which ground is already taken; repeating strings
/// would produce the keyword-stuffed prose that ranked in 2015 and reads badly
/// now.
pub fn brief_prompt(b: &Brief) -> String {
    let mut p = String::new();

    p.push_str(&format!(
        "Write an article in {} for the search query: \"{}\".\n\n",
        language_name(&b.language),
        b.topic
    ));

    if let Some(a) = b.format_advice() {
        p.push_str(&format!("## Format\n\n{}\n", a.headline));
        p.push_str(&format!("{}\n\n", a.detail));
    }

    if let Some(ai) = &b.ai_overview {
        p.push_str(
            "## What the search engine already answers\n\n\
             This summary is shown above the results, so readers see it without clicking. \
             Do not restate it. Go further: specifics, edge cases, numbers and situations \
             a summary cannot carry.\n\n",
        );
        p.push_str(&format!("```\n{}\n```\n\n", ai.trim()));

        // Naming the missing ground turns "go further" into something the model
        // can act on. These are subjects several ranking pages give a section to
        // and the summary never mentions.
        let gap = crate::aeo::topic_gap(b);
        if !gap.is_empty() {
            p.push_str(
                "### What that summary leaves out\n\n\
                 Pages that rank for this query cover these; the summary above does not. \
                 This is where a reader gains something by clicking, so give this ground \
                 real depth rather than a passing mention.\n\n",
            );
            for g in &gap {
                p.push_str(&format!(
                    "- {} (covered by {} of the ranking pages)\n",
                    g.title, g.competitors
                ));
            }
            p.push('\n');
        }
    }

    if !b.questions.is_empty() {
        p.push_str(
            "## Questions the article must answer\n\n\
             These are real questions people ask about this topic. Answer each one \
             clearly. Broad ones deserve their own section; narrow ones can go in a \
             closing FAQ.\n\n",
        );
        for q in &b.questions {
            p.push_str(&format!("- {q}\n"));
        }
        p.push('\n');
    }

    let sections = b.common_sections();
    if !sections.is_empty() {
        p.push_str(
            "## Ground the ranking pages cover\n\n\
             Independent pages that rank for this query all cover these. Treat them as \
             expected coverage, not as headings to copy.\n\n",
        );
        for (title, n) in sections.iter().take(12) {
            p.push_str(&format!("- {title} (on {n} of the pages)\n"));
        }
        p.push('\n');
    }

    // A couple of real outlines help the model match the depth readers expect,
    // but only where the extracted headings look like an article. Some pages
    // yield a sidebar of unrelated posts ("Soja w chorobach tarczycy" under an
    // article about coffee), and feeding that to a model is pure noise.
    let examples: Vec<&Competitor> = b
        .competitors
        .iter()
        .filter(|c| c.headings.len() >= 4 && outline_looks_like_an_article(&c.headings, &b.topic))
        .take(2)
        .collect();
    if !examples.is_empty() {
        p.push_str("## How competing articles are structured\n\n");
        for c in examples {
            p.push_str(&format!("{}:\n", c.domain));
            for h in c.headings.iter().take(12) {
                p.push_str(&format!(
                    "  {} {}\n",
                    "-".repeat(h.level.clamp(1, 4) as usize),
                    h.title
                ));
            }
            p.push('\n');
        }
    }

    // Instructions ordered by measured effect. In the GEO benchmark (KDD 2024)
    // quotations, statistics and named sources gained 27-41% visibility in
    // generative answers, while keyword stuffing scored below an untouched page.
    // The "answer first" rule comes from how these engines quote: they lift a
    // passage, and a passage that needs its neighbours to make sense is not
    // quotable.
    p.push_str(
        "## Rules\n\n\
         - Open every section with its answer in the first sentence, then explain. \
         A reader who stops after that sentence should still have been answered, \
         and an assistant quoting it should not need the rest of the page.\n\
         - Attribute what can be attributed: name the study, body, manufacturer or \
         standard behind a claim. An unsourced figure is worth less than a sourced one.\n\
         - Quote a real source directly where one exists, rather than paraphrasing \
         everything in your own voice.\n\
         - Be concrete: name amounts, times, materials, models, prices where relevant.\n\
         - Say plainly when something depends on the situation, and on what.\n\
         - Write for a reader, not for a search engine. No keyword repetition.\n\
         - No filler introduction. Start where the reader's problem starts.\n\
         - Output markdown with ## headings.\n",
    );

    p
}

/// Renders a prompt for the FAQ block alone.
///
/// The People Also Ask questions are already phrased the way people ask them,
/// so they are usable as headings unchanged; what is missing is the answers.
/// Writing only those is a fraction of the cost of a whole article and fits a
/// page that already exists.
///
/// The answers must stand on their own, because this block is what assistants
/// and rich results quote, and a quoted answer arrives without its page.
pub fn faq_prompt(b: &Brief) -> String {
    let mut p = String::new();

    p.push_str(&format!(
        "Write a FAQ section in {} for the search query: \"{}\".\n\n",
        language_name(&b.language),
        b.topic
    ));

    p.push_str(
        "## The questions\n\n\
         These are the questions people actually ask, taken from the search results. \
         Keep them as the headings, in this order, with their wording intact. Fix only \
         obvious capitalisation or punctuation.\n\n",
    );
    for q in &b.questions {
        p.push_str(&format!("- {q}\n"));
    }
    p.push('\n');

    if let Some(ai) = &b.ai_overview {
        p.push_str(
            "## What the search engine already says\n\n\
             Readers see this without clicking, so do not repeat it. Where an answer \
             overlaps, add the specifics this summary leaves out.\n\n",
        );
        p.push_str(&format!("```\n{}\n```\n\n", ai.trim()));
    }

    p.push_str(
        "## How to answer\n\n\
         - Answer in the first sentence, then explain. Never build up to the answer.\n\
         - Two to five sentences per question. These are answers, not sections.\n\
         - Each answer must make sense on its own, quoted away from the others.\n\
         - Be concrete: amounts, times, temperatures, prices where they apply.\n\
         - Name the source of a claim where there is one: the study, body or maker. \
         A quoted answer travels without your page, so it has to carry its own authority.\n\
         - Say plainly when the answer depends on the situation, and on what.\n\
         - No introduction and no closing summary. Start at the first question.\n\
         - Output markdown: `## question` followed by the answer.\n",
    );

    p
}

/// Whether an extracted outline reads like one article rather than a page of
/// links.
///
/// Signal used: an article's headings share vocabulary with its topic, while a
/// sidebar of recent posts does not. Requiring only a quarter of headings to
/// overlap keeps genuine outlines that use synonyms.
fn outline_looks_like_an_article(headings: &[Heading], topic: &str) -> bool {
    let topic_words: Vec<String> = topic
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.chars().count() > 3)
        .map(|w| w.chars().take(5).collect())
        .collect();
    if topic_words.is_empty() {
        return true;
    }
    let related = headings
        .iter()
        .filter(|h| {
            let t = h.title.to_lowercase();
            topic_words.iter().any(|w| t.contains(w.as_str()))
        })
        .count();
    related * 4 >= headings.len()
}

/// Human-readable language name for the prompt's opening line.
fn language_name(code: &str) -> &'static str {
    match code {
        "pl" => "Polish",
        "de" => "German",
        "es" => "Spanish",
        "fr" => "French",
        _ => "English",
    }
}

/// One week of a Google Trends index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrendPoint {
    pub date: String,
    pub v: i64,
}

/// A related query from Google Trends, with its relative index.
///
/// In the `top` list the value is 0-100 relative to the loudest. In the
/// `rising` list it is the percent growth, or a very large number ("breakout")
/// when the query was too small to measure a year ago.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrendQuery {
    pub query: String,
    pub value: i64,
}

/// How a search's topic looks on YouTube, from Google Trends.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YoutubeCheck {
    pub keyword: String,
    pub weekly: Vec<TrendPoint>,
    pub weekly_web: Vec<TrendPoint>,
    pub top: Vec<TrendQuery>,
    pub rising: Vec<TrendQuery>,
    pub checked_at: String,
}

impl YoutubeCheck {
    /// Average YouTube index over the period, 0-100.
    pub fn youtube_mean(&self) -> f64 {
        mean(&self.weekly)
    }

    pub fn web_mean(&self) -> f64 {
        mean(&self.weekly_web)
    }

    /// Weeks with no measurable YouTube interest at all.
    pub fn youtube_zero_weeks(&self) -> usize {
        self.weekly.iter().filter(|p| p.v == 0).count()
    }

    /// Change of the last quarter against the one before, in percent, on
    /// YouTube. None when the earlier quarter was silent.
    pub fn youtube_quarter_change(&self) -> Option<i64> {
        quarter_change(&self.weekly)
    }

    pub fn web_quarter_change(&self) -> Option<i64> {
        quarter_change(&self.weekly_web)
    }
}

fn mean(points: &[TrendPoint]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    points.iter().map(|p| p.v as f64).sum::<f64>() / points.len() as f64
}

fn quarter_change(points: &[TrendPoint]) -> Option<i64> {
    if points.len() < 26 {
        return None;
    }
    let recent: i64 = points.iter().rev().take(13).map(|p| p.v).sum();
    let before: i64 = points.iter().rev().skip(13).take(13).map(|p| p.v).sum();
    if before == 0 {
        return None;
    }
    Some((recent - before) * 100 / before)
}

/// A video YouTube ranks for a phrase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YoutubeVideo {
    pub title: String,
    pub channel: String,
    pub views: i64,
    /// YouTube's own wording, "9 months ago", in the market's language.
    pub age: String,
    pub url: String,
    #[serde(default)]
    pub seconds: i64,
}

/// What gets watched under a phrase on YouTube: the top ten videos' views.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YoutubeAppetite {
    pub phrase: String,
    pub videos: i32,
    /// Clips under a minute dropped from the top ten. On the first live
    /// run "ing kredyt hipoteczny" scored 11.9M views, and 10M of them were
    /// ING's 15-second adverts: bought views, not an audience.
    #[serde(default)]
    pub ads_dropped: i32,
    pub views_top10: i64,
    pub views_median: i64,
    pub fresh: i32,
    pub top: Vec<YoutubeVideo>,
    pub checked_at: String,
}

/// One topic in a YouTube comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YoutubeCompareRow {
    pub keyword: String,
    /// Mean YouTube index, relative to the loudest topic in the set.
    pub youtube: f64,
    /// Mean web-search index, same scale, same set.
    pub web: f64,
    /// Monthly Google searches, a real count, fetched alongside. YouTube has
    /// no equivalent; this is the one absolute number on the row.
    #[serde(default)]
    pub google_volume: Option<i64>,
    /// Estimated monthly YouTube searches, only when an anchor was given.
    pub estimate: Option<i64>,
}

/// Several topics on YouTube side by side, optionally scaled to real numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YoutubeCompare {
    pub id: String,
    pub language: String,
    pub country: String,
    pub rows: Vec<YoutubeCompareRow>,
    pub anchor: Option<String>,
    pub anchor_volume: Option<i64>,
    pub created_at: String,
}

#[cfg(test)]
mod asked_tests {
    use super::Suggestion;

    fn s(ai: i64, google: i64) -> Suggestion {
        let mut x = Suggestion::new("x", "", "");
        x.ai_volume = Some(ai);
        x.search_volume = Some(google);
        x
    }

    #[test]
    fn asked_follows_the_measured_pairs() {
        assert!(s(38, 590).is_asked());
        assert!(s(12, 50).is_asked());
        assert!(s(13, 90).is_asked());
        assert!(s(10, 110).is_asked());
        assert!(!s(231, 40_500).is_asked());
        assert!(!s(105, 165_000).is_asked());
        assert!(!s(5, 40_500).is_asked());
        // Below the floors nothing qualifies, however high the ratio.
        assert!(!s(4, 50).is_asked());
        assert!(!s(50, 40).is_asked());
    }

    #[test]
    fn a_reordered_seed_is_recognised() {
        let x = Suggestion::new("hipoteczny kredyt", "", "");
        assert!(x.is_permutation_of("kredyt hipoteczny"));
        assert!(x.is_permutation_of("Kredyt Hipoteczny"));
        let y = Suggestion::new("kredyt hipoteczny bank", "", "");
        assert!(!y.is_permutation_of("kredyt hipoteczny"));
    }
}

#[cfg(test)]
mod bing_tests {
    use super::bing_volume_available;

    #[test]
    fn bing_volume_exists_for_six_countries_only() {
        assert!(bing_volume_available("en", "us"));
        assert!(bing_volume_available("en", "GB"));
        assert!(bing_volume_available("de", "de"));
        assert!(bing_volume_available("fr", "fr"));
        // Measured: no Polish location in Bing's list at all.
        assert!(!bing_volume_available("pl", "pl"));
        // Country alone is not enough; the language must be one Bing prices.
        assert!(!bing_volume_available("es", "es"));
        assert!(!bing_volume_available("pl", "us"));
    }
}

#[cfg(test)]
mod youtube_tests {
    use super::*;

    fn pts(vs: &[i64]) -> Vec<TrendPoint> {
        vs.iter()
            .map(|v| TrendPoint {
                date: String::new(),
                v: *v,
            })
            .collect()
    }

    #[test]
    fn quarter_change_compares_last_13_weeks_to_the_13_before() {
        let mut v = vec![10; 13];
        v.extend(vec![20; 13]);
        assert_eq!(quarter_change(&pts(&v)), Some(100));
        let mut v = vec![20; 13];
        v.extend(vec![10; 13]);
        assert_eq!(quarter_change(&pts(&v)), Some(-50));
    }

    #[test]
    fn quarter_change_is_unknown_when_the_earlier_quarter_was_silent() {
        let mut v = vec![0; 13];
        v.extend(vec![5; 13]);
        assert_eq!(quarter_change(&pts(&v)), None);
        assert_eq!(quarter_change(&pts(&[1, 2, 3])), None);
    }
}
