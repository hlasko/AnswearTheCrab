//! Suggestion providers.
//!
//! Two implementations:
//! * [`dataforseo::DataForSeo`] - paid API, richer data (search volume, CPC).
//! * [`suggest::SuggestApi`] - free public autocomplete for Google, YouTube and
//!   Bing; the Google variant is also the fallback when no keys are configured.

pub mod dataforseo;
pub mod suggest;

use crate::domain::{Source, Suggestion};
use std::collections::HashSet;

/// One (category, modifier) autocomplete probe.
#[derive(Debug, Clone)]
pub struct Probe {
    pub category: &'static str,
    pub modifier: String,
    pub query: String,
}

/// Builds the ATP probe matrix for a keyword in a given language.
///
/// The modifiers must match the language of the keyword. Probing the Polish
/// "kawa" with English words produced queries like "are kawa", which the search
/// engines happily answered with "are kawasaki engines good" - real suggestions,
/// entirely useless for a Polish search.
pub fn probes(keyword: &str, language: &str) -> Vec<Probe> {
    let vocab = crate::domain::vocabulary(language);
    let kw = keyword.trim();
    let mut probes = Vec::new();

    for w in vocab.questions {
        probes.push(Probe {
            category: "questions",
            modifier: (*w).to_string(),
            query: format!("{w} {kw}"),
        });
        probes.push(Probe {
            category: "questions",
            modifier: (*w).to_string(),
            query: format!("{kw} {w}"),
        });
    }
    for w in vocab.prepositions {
        probes.push(Probe {
            category: "prepositions",
            modifier: (*w).to_string(),
            query: format!("{kw} {w}"),
        });
    }
    for w in vocab.comparisons {
        probes.push(Probe {
            category: "comparisons",
            modifier: (*w).to_string(),
            query: format!("{kw} {w}"),
        });
    }
    for c in b'a'..=b'z' {
        let letter = (c as char).to_string();
        probes.push(Probe {
            category: "alphabetical",
            modifier: letter.clone(),
            query: format!("{kw} {letter}"),
        });
    }
    probes.push(Probe {
        category: "related",
        modifier: "related".to_string(),
        query: kw.to_string(),
    });
    probes
}

/// Drops duplicates and echoes of the seed keyword, preserving first-seen order.
pub fn dedupe(keyword: &str, items: Vec<Suggestion>) -> Vec<Suggestion> {
    let kw_lower = keyword.trim().to_lowercase();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for s in items {
        let norm = s.text.trim().to_lowercase();
        if norm.is_empty() || norm == kw_lower {
            continue;
        }
        if seen.insert(norm) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_use_the_language_of_the_keyword() {
        let pl: Vec<String> = probes("kawa", "pl").into_iter().map(|p| p.query).collect();
        // Polish modifiers appear...
        assert!(pl.iter().any(|q| q == "jak kawa"), "missing Polish probe");
        assert!(
            pl.iter().any(|q| q == "kawa dla"),
            "missing Polish preposition"
        );
        // ...and English ones do not. "are kawa" returned "are kawasaki engines
        // good" from the live API, which is noise for a Polish search.
        assert!(
            !pl.iter().any(|q| q == "are kawa"),
            "English probe leaked into Polish"
        );
        assert!(
            !pl.iter().any(|q| q == "kawa with"),
            "English probe leaked into Polish"
        );

        let en: Vec<String> = probes("coffee", "en")
            .into_iter()
            .map(|p| p.query)
            .collect();
        assert!(en.iter().any(|q| q == "how coffee"));
        assert!(en.iter().any(|q| q == "coffee for"));
        assert!(!en.iter().any(|q| q == "jak coffee"));
    }

    #[test]
    fn probes_always_cover_every_category() {
        for lang in ["en", "pl", "de", "es", "fr", "unknown"] {
            let ps = probes("x", lang);
            for cat in [
                "questions",
                "prepositions",
                "comparisons",
                "alphabetical",
                "related",
            ] {
                assert!(
                    ps.iter().any(|p| p.category == cat),
                    "language {lang} has no {cat} probes"
                );
            }
            // The alphabet sweep is language independent.
            assert_eq!(
                ps.iter().filter(|p| p.category == "alphabetical").count(),
                26
            );
        }
    }
}

/// A source of keyword suggestions.
#[async_trait::async_trait]
pub trait SuggestionProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn harvest(
        &self,
        keyword: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Vec<Suggestion>>;
}

/// Routes a search to the right provider for its source.
///
/// DataForSEO covers Google with paid metrics; YouTube and Bing use their free
/// public autocomplete, which needs no key. Google also falls back to the free
/// endpoint when no credentials are configured.
#[derive(Clone)]
pub struct Providers {
    google: std::sync::Arc<dyn SuggestionProvider>,
    youtube: std::sync::Arc<dyn SuggestionProvider>,
    bing: std::sync::Arc<dyn SuggestionProvider>,
}

impl Default for Providers {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Providers {
    pub fn from_env() -> Self {
        let login = std::env::var("DATAFORSEO_LOGIN")
            .ok()
            .filter(|s| !s.is_empty());
        let password = std::env::var("DATAFORSEO_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty());

        let google: std::sync::Arc<dyn SuggestionProvider> = match (login, password) {
            (Some(l), Some(p)) => {
                let base = std::env::var("DATAFORSEO_BASE_URL")
                    .unwrap_or_else(|_| "https://api.dataforseo.com".into());
                tracing::info!("google source: dataforseo ({base})");
                std::sync::Arc::new(dataforseo::DataForSeo::from_env(l, p, base))
            }
            _ => {
                tracing::warn!(
                    "DATAFORSEO_LOGIN/DATAFORSEO_PASSWORD not set, google falls back to public suggest"
                );
                std::sync::Arc::new(suggest::SuggestApi::new(Source::Google))
            }
        };

        Self {
            google,
            youtube: std::sync::Arc::new(suggest::SuggestApi::new(Source::YouTube)),
            bing: std::sync::Arc::new(suggest::SuggestApi::new(Source::Bing)),
        }
    }

    fn for_source(&self, source: Source) -> &std::sync::Arc<dyn SuggestionProvider> {
        match source {
            Source::Google => &self.google,
            Source::YouTube => &self.youtube,
            Source::Bing => &self.bing,
        }
    }

    /// Provider name recorded against a finished search, e.g. `dataforseo`.
    pub fn name(&self, source: Source) -> &'static str {
        self.for_source(source).name()
    }

    pub async fn harvest(
        &self,
        source: Source,
        keyword: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Vec<Suggestion>> {
        self.for_source(source)
            .harvest(keyword, language, country)
            .await
    }
}
