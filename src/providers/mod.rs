//! Suggestion providers.
//!
//! Two implementations:
//! * [`dataforseo::DataForSeo`] - paid API, richer data (search volume, CPC).
//! * [`google::GoogleSuggest`] - free public autocomplete endpoint, used as fallback.

pub mod dataforseo;
pub mod google;

use crate::domain::{Suggestion, COMPARISONS, PREPOSITIONS, QUESTION_WORDS};
use std::collections::HashSet;

/// One (category, modifier) autocomplete probe.
#[derive(Debug, Clone)]
pub struct Probe {
    pub category: &'static str,
    pub modifier: String,
    pub query: String,
}

/// Builds the standard ATP probe matrix for a keyword.
pub fn probes(keyword: &str) -> Vec<Probe> {
    let kw = keyword.trim();
    let mut probes = Vec::new();

    for w in QUESTION_WORDS {
        probes.push(Probe {
            category: "questions",
            modifier: w.to_string(),
            query: format!("{w} {kw}"),
        });
        probes.push(Probe {
            category: "questions",
            modifier: w.to_string(),
            query: format!("{kw} {w}"),
        });
    }
    for w in PREPOSITIONS {
        probes.push(Probe {
            category: "prepositions",
            modifier: w.to_string(),
            query: format!("{kw} {w}"),
        });
    }
    for w in COMPARISONS {
        probes.push(Probe {
            category: "comparisons",
            modifier: w.to_string(),
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

/// Provider chosen from the environment.
#[derive(Clone)]
pub struct Providers {
    inner: std::sync::Arc<dyn SuggestionProvider>,
}

impl Providers {
    /// Uses DataForSEO when `DATAFORSEO_LOGIN`/`DATAFORSEO_PASSWORD` are present,
    /// otherwise falls back to the free Google endpoint.
    pub fn from_env() -> Self {
        let login = std::env::var("DATAFORSEO_LOGIN")
            .ok()
            .filter(|s| !s.is_empty());
        let password = std::env::var("DATAFORSEO_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty());

        match (login, password) {
            (Some(l), Some(p)) => {
                let base = std::env::var("DATAFORSEO_BASE_URL")
                    .unwrap_or_else(|_| "https://api.dataforseo.com".into());
                tracing::info!("suggestion provider: dataforseo ({base})");
                Self {
                    inner: std::sync::Arc::new(dataforseo::DataForSeo::from_env(l, p, base)),
                }
            }
            _ => {
                tracing::warn!(
                    "DATAFORSEO_LOGIN/DATAFORSEO_PASSWORD not set, falling back to public Google suggest"
                );
                Self {
                    inner: std::sync::Arc::new(google::GoogleSuggest::new()),
                }
            }
        }
    }

    pub fn name(&self) -> &'static str {
        self.inner.name()
    }

    pub async fn harvest(
        &self,
        keyword: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Vec<Suggestion>> {
        self.inner.harvest(keyword, language, country).await
    }
}
