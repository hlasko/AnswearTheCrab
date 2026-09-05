//! Google SERP as a research source: AI Overview, People Also Ask, competitors.
//!
//! Field paths here were read off real responses from the live API rather than
//! from documentation, and two behaviours are load-bearing:
//!
//! * `ai_overview` is present for roughly half of topics. Questions like "how to
//!   descale X" have one; shopping and brand queries usually do not.
//! * Content parsing succeeds for about 61% of pages. The rest return no
//!   structure at all, so a competitor without headings is expected.

use super::{Findings, ResearchSource};
use crate::domain::{Competitor, Heading};
use futures::stream::{self, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;

/// How many organic results to consider.
///
/// Deliberately more than we need: only about three in five pages parse, so
/// asking for five leaves too little structure to compare.
const COMPETITORS: usize = 8;

/// Parsing a page is slow (seconds) and the endpoint is rate-sensitive.
const PARSE_CONCURRENCY: usize = 4;

#[derive(Clone)]
pub struct SerpSource {
    client: reqwest::Client,
    login: String,
    password: String,
    base_url: String,
}

impl SerpSource {
    pub fn new(login: String, password: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .expect("http client");
        Self {
            client,
            login,
            password,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Reads credentials the same way the suggestion provider does.
    pub fn from_env() -> Option<Self> {
        let login = std::env::var("DATAFORSEO_LOGIN")
            .ok()
            .filter(|s| !s.is_empty())?;
        let password = std::env::var("DATAFORSEO_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty())?;
        let base = std::env::var("DATAFORSEO_BASE_URL")
            .unwrap_or_else(|_| "https://api.dataforseo.com".into());
        Some(Self::new(login, password, base))
    }

    async fn post(&self, path: &str, body: Value) -> anyhow::Result<Value> {
        let resp = self
            .client
            .post(format!("{}{path}", self.base_url))
            .basic_auth(&self.login, Some(&self.password))
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let value: Value = resp.json().await?;
        let code = value.get("status_code").and_then(Value::as_i64);
        let msg = value
            .get("status_message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error")
            .trim()
            .trim_end_matches('.');

        if !status.is_success() {
            anyhow::bail!(
                "serp {path}: {msg} (http {}, code {:?})",
                status.as_u16(),
                code
            );
        }
        if code != Some(20000) {
            anyhow::bail!("serp {path}: {msg} (code {})", code.unwrap_or_default());
        }
        Ok(value)
    }

    /// Fetches the SERP and pulls out everything except page structure.
    async fn serp(&self, topic: &str, language: &str, country: &str) -> anyhow::Result<Findings> {
        let body = json!([{
            "keyword": topic,
            "language_code": language,
            "location_code": crate::providers::dataforseo::location_code(country),
            "depth": 10,
        }]);
        let value = self
            .post("/v3/serp/google/organic/live/advanced", body)
            .await?;

        let mut out = Findings::default();
        for task in value
            .get("tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let code = task.get("status_code").and_then(Value::as_i64).unwrap_or(0);
            if code != 20000 {
                let m = task
                    .get("status_message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                anyhow::bail!("serp task status {code}: {m}");
            }
            for result in task
                .get("result")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let items = match result.get("items").and_then(Value::as_array) {
                    Some(i) => i,
                    None => continue,
                };
                for item in items {
                    match item.get("type").and_then(Value::as_str) {
                        Some("ai_overview") => {
                            // The answer arrives as a list of blocks; join their text.
                            let text: String = item
                                .get("items")
                                .and_then(Value::as_array)
                                .map(|blocks| {
                                    blocks
                                        .iter()
                                        .filter_map(|b| b.get("text").and_then(Value::as_str))
                                        .collect::<Vec<_>>()
                                        .join("\n\n")
                                })
                                .unwrap_or_default();
                            if !text.trim().is_empty() {
                                out.ai_overview = Some(text);
                            }
                            for r in item
                                .get("references")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten()
                            {
                                if let Some(d) = r.get("domain").and_then(Value::as_str) {
                                    if !out.ai_sources.iter().any(|x| x == d) {
                                        out.ai_sources.push(d.to_string());
                                    }
                                }
                            }
                        }
                        Some("people_also_ask") => {
                            for q in item
                                .get("items")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten()
                            {
                                if let Some(t) = q.get("title").and_then(Value::as_str) {
                                    out.questions.push(t.to_string());
                                }
                            }
                        }
                        Some("related_searches") => {
                            for r in item
                                .get("items")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten()
                            {
                                if let Some(t) = r.as_str() {
                                    out.related.push(t.to_string());
                                }
                            }
                        }
                        Some("organic") => {
                            if out.competitors.len() >= COMPETITORS {
                                continue;
                            }
                            let Some(url) = item.get("url").and_then(Value::as_str) else {
                                continue;
                            };
                            out.competitors.push(Competitor {
                                rank: item
                                    .get("rank_absolute")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0) as i32,
                                url: url.to_string(),
                                domain: item
                                    .get("domain")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                title: item.get("title").and_then(Value::as_str).map(String::from),
                                description: item
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .map(String::from),
                                headings: Vec::new(),
                                parsed: false,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(out)
    }

    /// Reads a page's heading structure. Returns an empty list when the page
    /// yields nothing, which happens for about two in five pages.
    async fn headings(&self, url: &str) -> Vec<Heading> {
        let body = json!([{ "url": url, "enable_javascript": true }]);
        let value = match self.post("/v3/on_page/content_parsing/live", body).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("content parsing failed for {url}: {e}");
                return Vec::new();
            }
        };

        let mut out = Vec::new();
        for task in value
            .get("tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for result in task
                .get("result")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                for item in result
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let page = item.get("page_content");
                    // `main_topic` is the article proper; `secondary_topic` holds
                    // page furniture such as "Latest posts", which would only add
                    // noise to a content brief.
                    for section in page
                        .and_then(|p| p.get("main_topic"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(title) = section.get("h_title").and_then(Value::as_str) {
                            if title.trim().is_empty() {
                                continue;
                            }
                            out.push(Heading {
                                level: section.get("level").and_then(Value::as_i64).unwrap_or(2)
                                    as i32,
                                title: title.trim().to_string(),
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

#[async_trait::async_trait]
impl ResearchSource for SerpSource {
    fn name(&self) -> &'static str {
        "google-serp"
    }

    async fn research(
        &self,
        topic: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Findings> {
        let mut findings = self.serp(topic, language, country).await?;

        // Parse competitors in parallel. Failures are absorbed per page so one
        // unreadable site cannot cost us the whole brief.
        let parsed: Vec<(String, Vec<Heading>)> = stream::iter(
            findings
                .competitors
                .iter()
                .map(|c| c.url.clone())
                .collect::<Vec<_>>(),
        )
        .map(|url| {
            let this = self.clone();
            async move {
                let h = this.headings(&url).await;
                (url, h)
            }
        })
        .buffer_unordered(PARSE_CONCURRENCY)
        .collect()
        .await;

        for (url, headings) in parsed {
            if let Some(c) = findings.competitors.iter_mut().find(|c| c.url == url) {
                c.parsed = !headings.is_empty();
                c.headings = headings;
            }
        }

        Ok(findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes below are trimmed from real API responses.
    fn serp_body() -> Value {
        json!({
            "status_code": 20000,
            "tasks": [{
                "status_code": 20000,
                "result": [{
                    "items": [
                        {
                            "type": "ai_overview",
                            "items": [
                                {"text": "Aby odkamienić ekspres, użyj płynu."},
                                {"text": "Uruchom program odkamieniania."}
                            ],
                            "references": [
                                {"domain": "cleangang.pl"},
                                {"domain": "delonghi.com"},
                                {"domain": "cleangang.pl"}
                            ]
                        },
                        {
                            "type": "people_also_ask",
                            "items": [
                                {"title": "Czy octem można odkamieniać ekspres?"},
                                {"title": "Jak często odkamieniać?"}
                            ]
                        },
                        {"type": "related_searches", "items": ["odkamieniacz do ekspresu"]},
                        {
                            "type": "organic", "rank_absolute": 5,
                            "url": "https://cleangang.pl/a", "domain": "cleangang.pl",
                            "title": "Jak odkamienić", "description": "Poradnik"
                        },
                        {
                            "type": "organic", "rank_absolute": 6,
                            "url": "https://coffeedesk.pl/b", "domain": "coffeedesk.pl",
                            "title": "Odkamienianie", "description": null
                        },
                        {"type": "video", "url": "https://youtube.com/x"}
                    ]
                }]
            }]
        })
    }

    #[tokio::test]
    async fn extracts_ai_overview_questions_and_competitors() {
        let body = serp_body();
        let app = axum::Router::new().route(
            "/v3/serp/google/organic/live/advanced",
            axum::routing::post(move || {
                let body = body.clone();
                async move { axum::Json(body) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let src = SerpSource::new("l".into(), "p".into(), format!("http://{addr}"));
        let f = src
            .serp("jak odkamienić ekspres", "pl", "pl")
            .await
            .unwrap();

        // Blocks are joined into one answer.
        assert!(f.ai_overview.as_deref().unwrap().contains("płynu"));
        assert!(f.ai_overview.as_deref().unwrap().contains("program"));
        // Cited domains are deduplicated: this is the AEO target list.
        assert_eq!(f.ai_sources, ["cleangang.pl", "delonghi.com"]);
        assert_eq!(f.questions.len(), 2);
        assert_eq!(f.related, ["odkamieniacz do ekspresu"]);
        // Only organic results become competitors; videos are ignored.
        assert_eq!(f.competitors.len(), 2);
        assert_eq!(f.competitors[0].rank, 5);
        assert!(!f.competitors[0].parsed);
    }

    #[tokio::test]
    async fn a_serp_without_ai_overview_is_not_an_error() {
        // Measured: roughly half of topics have no AI answer. Shopping and brand
        // queries in particular. The brief must still be produced.
        let body = json!({
            "status_code": 20000,
            "tasks": [{"status_code": 20000, "result": [{"items": [
                {"type": "organic", "rank_absolute": 1,
                 "url": "https://x.example/a", "domain": "x.example", "title": "T"}
            ]}]}]
        });
        let app = axum::Router::new().route(
            "/v3/serp/google/organic/live/advanced",
            axum::routing::post(move || {
                let body = body.clone();
                async move { axum::Json(body) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let src = SerpSource::new("l".into(), "p".into(), format!("http://{addr}"));
        let f = src.serp("ekspres delonghi", "pl", "pl").await.unwrap();
        assert!(f.ai_overview.is_none());
        assert!(f.ai_sources.is_empty());
        assert_eq!(f.competitors.len(), 1);
    }

    #[tokio::test]
    async fn unparseable_page_yields_no_headings_rather_than_failing() {
        // About 39% of pages return no structure at all.
        let app = axum::Router::new().route(
            "/v3/on_page/content_parsing/live",
            axum::routing::post(|| async {
                axum::Json(json!({
                    "status_code": 20000,
                    "tasks": [{"status_code": 20000, "result": [{"items": null}]}]
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let src = SerpSource::new("l".into(), "p".into(), format!("http://{addr}"));
        assert!(src.headings("https://x.example").await.is_empty());
    }

    #[tokio::test]
    async fn keeps_only_the_article_headings() {
        // `secondary_topic` holds page furniture ("Latest posts", "Newsletter"),
        // which would pollute a content brief.
        let app = axum::Router::new().route(
            "/v3/on_page/content_parsing/live",
            axum::routing::post(|| async {
                axum::Json(json!({
                    "status_code": 20000,
                    "tasks": [{"status_code": 20000, "result": [{"items": [{
                        "page_content": {
                            "main_topic": [
                                {"level": 1, "h_title": "Jak odkamienić ekspres"},
                                {"level": 2, "h_title": "Dlaczego to ważne"},
                                {"level": 2, "h_title": "   "}
                            ],
                            "secondary_topic": [
                                {"level": 2, "h_title": "Latest Blog Posts"}
                            ]
                        }
                    }]}]}]
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let src = SerpSource::new("l".into(), "p".into(), format!("http://{addr}"));
        let h = src.headings("https://x.example").await;
        assert_eq!(
            h.len(),
            2,
            "blank titles and page furniture must be dropped"
        );
        assert_eq!(h[0].title, "Jak odkamienić ekspres");
    }
}
