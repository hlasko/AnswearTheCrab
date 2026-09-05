//! Free autocomplete endpoints: Google, YouTube and Bing.
//!
//! All three answer with the same OpenSearch-style array,
//! `["seed", ["suggestion", ...]]`, so one parser and one probe loop serve them
//! all; only the URL and the market parameters differ.
//!
//! These need no API key, which is why Google here is also the fallback when no
//! DataForSEO credentials are configured.

use super::{dedupe, probes, SuggestionProvider};
use crate::domain::{Source, Suggestion};
use futures::stream::{self, StreamExt};
use std::time::Duration;

const CONCURRENCY: usize = 6;

#[derive(Clone)]
pub struct SuggestApi {
    client: reqwest::Client,
    source: Source,
}

impl Default for SuggestApi {
    fn default() -> Self {
        Self::new(Source::Google)
    }
}

impl SuggestApi {
    pub fn new(source: Source) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            )
            .build()
            .expect("http client");
        Self { client, source }
    }

    pub fn source(&self) -> Source {
        self.source
    }

    /// One autocomplete call.
    async fn suggest(&self, query: &str, lang: &str, country: &str) -> anyhow::Result<Vec<String>> {
        let request = match self.source {
            // YouTube is the same endpoint as Google plus `ds=yt`, and honours
            // the same hl/gl market parameters.
            Source::Google | Source::YouTube => {
                let mut params = vec![
                    ("client", "firefox".to_string()),
                    ("hl", lang.to_string()),
                    ("gl", country.to_string()),
                    ("q", query.to_string()),
                ];
                if self.source == Source::YouTube {
                    params.push(("ds", "yt".to_string()));
                }
                self.client
                    .get("https://suggestqueries.google.com/complete/search")
                    .query(&params)
            }
            // Bing takes a single combined market code, e.g. pl-PL.
            Source::Bing => self.client.get("https://api.bing.com/osjson.aspx").query(&[
                ("query", query.to_string()),
                ("mkt", format!("{}-{}", lang, country.to_uppercase())),
            ]),
        };

        let resp = request.send().await?.error_for_status()?.text().await?;
        Ok(parse_suggest_array(&resp))
    }
}

/// Reads `["seed", ["a", "b", ...]]`, the shape all three endpoints return.
fn parse_suggest_array(body: &str) -> Vec<String> {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    parsed
        .get(1)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl SuggestionProvider for SuggestApi {
    fn name(&self) -> &'static str {
        match self.source {
            Source::Google => "google-suggest",
            Source::YouTube => "youtube-suggest",
            Source::Bing => "bing-suggest",
        }
    }

    async fn harvest(
        &self,
        keyword: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Vec<Suggestion>> {
        let results: Vec<Vec<Suggestion>> = stream::iter(probes(keyword, language))
            .map(|p| {
                let this = self.clone();
                async move {
                    match this.suggest(&p.query, language, country).await {
                        Ok(items) => items
                            .into_iter()
                            .map(|text| Suggestion::new(text, p.category, p.modifier.clone()))
                            .collect(),
                        Err(e) => {
                            tracing::warn!("{} failed for `{}`: {e}", this.name(), p.query);
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

/// Kept for the fallback path, which is always Google.
pub type GoogleSuggest = SuggestApi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shared_opensearch_shape() {
        // Bodies as actually returned by the three endpoints.
        let google = r#"["kredyt",["kredyt hipoteczny","kredyt ok"],[],{}]"#;
        assert_eq!(
            parse_suggest_array(google),
            ["kredyt hipoteczny", "kredyt ok"]
        );

        let bing = r#"["coffee",["coffeedesk","coffee and sons"]]"#;
        assert_eq!(parse_suggest_array(bing), ["coffeedesk", "coffee and sons"]);

        // Malformed or empty bodies yield nothing rather than panicking.
        assert!(parse_suggest_array("not json").is_empty());
        assert!(parse_suggest_array(r#"["seed"]"#).is_empty());
        assert!(parse_suggest_array(r#"["seed",[]]"#).is_empty());
    }

    #[test]
    fn each_source_reports_a_distinct_name() {
        let names: Vec<&str> = Source::all()
            .iter()
            .map(|s| SuggestApi::new(*s).name())
            .collect();
        assert_eq!(names, ["google-suggest", "youtube-suggest", "bing-suggest"]);
    }
}
