//! Free fallback: Google's public autocomplete endpoint.

use super::{dedupe, probes, SuggestionProvider};
use crate::domain::Suggestion;
use futures::stream::{self, StreamExt};
use std::time::Duration;

const CONCURRENCY: usize = 6;

#[derive(Clone)]
pub struct GoogleSuggest {
    client: reqwest::Client,
}

impl Default for GoogleSuggest {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleSuggest {
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

    async fn suggest(&self, query: &str, lang: &str, country: &str) -> anyhow::Result<Vec<String>> {
        let resp = self
            .client
            .get("https://suggestqueries.google.com/complete/search")
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
}

#[async_trait::async_trait]
impl SuggestionProvider for GoogleSuggest {
    fn name(&self) -> &'static str {
        "google-suggest"
    }

    async fn harvest(
        &self,
        keyword: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Vec<Suggestion>> {
        let results: Vec<Vec<Suggestion>> = stream::iter(probes(keyword))
            .map(|p| {
                let this = self.clone();
                async move {
                    match this.suggest(&p.query, language, country).await {
                        Ok(items) => items
                            .into_iter()
                            .map(|text| Suggestion::new(text, p.category, p.modifier.clone()))
                            .collect(),
                        Err(e) => {
                            tracing::warn!("google suggest failed for `{}`: {e}", p.query);
                            Vec::new()
                        }
                    }
                }
            })
            .buffer_unordered(CONCURRENCY)
            .collect()
            .await;

        Ok(dedupe(keyword, results.into_iter().flatten().collect()))
    }
}
