//! Harvests autocomplete suggestions from Google's public suggest endpoint.

use crate::domain::{Suggestion, COMPARISONS, PREPOSITIONS, QUESTION_WORDS};
use futures::stream::{self, StreamExt};
use std::collections::HashSet;
use std::time::Duration;

const CONCURRENCY: usize = 6;

#[derive(Clone)]
pub struct Scraper {
    client: reqwest::Client,
}

/// One (category, modifier) probe.
struct Probe {
    category: &'static str,
    modifier: String,
    query: String,
}

impl Default for Scraper {
    fn default() -> Self {
        Self::new()
    }
}

impl Scraper {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            )
            .build()
            .expect("http client");
        Self { client }
    }

    fn probes(keyword: &str) -> Vec<Probe> {
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

    async fn suggest(&self, query: &str, lang: &str, country: &str) -> anyhow::Result<Vec<String>> {
        let url = "https://suggestqueries.google.com/complete/search";
        let resp = self
            .client
            .get(url)
            .query(&[
                ("client", "firefox"),
                ("hl", lang),
                ("gl", country),
                ("q", query),
            ])
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let parsed: serde_json::Value = serde_json::from_str(&resp)?;
        Ok(parsed
            .get(1)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Runs all probes for a keyword and returns deduplicated suggestions.
    pub async fn harvest(
        &self,
        keyword: &str,
        lang: &str,
        country: &str,
    ) -> anyhow::Result<Vec<Suggestion>> {
        let probes = Self::probes(keyword);
        let results: Vec<Vec<Suggestion>> = stream::iter(probes)
            .map(|p| {
                let this = self.clone();
                async move {
                    match this.suggest(&p.query, lang, country).await {
                        Ok(items) => items
                            .into_iter()
                            .map(|text| Suggestion {
                                text,
                                category: p.category.to_string(),
                                modifier: p.modifier.clone(),
                            })
                            .collect(),
                        Err(e) => {
                            tracing::warn!("suggest failed for {}: {e}", p.query);
                            Vec::new()
                        }
                    }
                }
            })
            .buffer_unordered(CONCURRENCY)
            .collect()
            .await;

        let kw_lower = keyword.trim().to_lowercase();
        let mut seen: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for s in results.into_iter().flatten() {
            let norm = s.text.trim().to_lowercase();
            if norm.is_empty() || norm == kw_lower {
                continue;
            }
            if seen.insert(norm) {
                out.push(s);
            }
        }
        Ok(out)
    }
}
