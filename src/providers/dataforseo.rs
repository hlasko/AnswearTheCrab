//! DataForSEO provider: Google Autocomplete (live/advanced) plus optional
//! Google Ads search volume enrichment.
//!
//! Env:
//! * `DATAFORSEO_LOGIN`, `DATAFORSEO_PASSWORD` - API credentials.
//! * `DATAFORSEO_BASE_URL` - override for sandbox, default `https://api.dataforseo.com`.
//! * `DATAFORSEO_CLIENT` - autocomplete client, default `gws-wiz-serp`.
//! * `DATAFORSEO_SEARCH_VOLUME` - `1`/`true` to enrich with Google Ads metrics.
//! * `DATAFORSEO_CONCURRENCY` - parallel autocomplete calls, default 8.
//! * `DATAFORSEO_MODE` - `labs` (default) or `autocomplete`.
//! * `DATAFORSEO_LIMIT` - Labs result cap, default 700, max 1000.
//!
//! Cost note: `labs` issues ONE billable request per search and already includes
//! search volume/CPC. `autocomplete` issues ~58 requests and needs a separate
//! Google Ads call for metrics, so it is roughly an order of magnitude pricier.

use super::{dedupe, probes, SuggestionProvider};
use crate::domain::{classify_with, detect_vocabulary, Suggestion};
use futures::stream::{self, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Google Ads / SERP location codes for the countries exposed in the UI.
pub fn location_code(country: &str) -> i64 {
    match country.to_lowercase().as_str() {
        "pl" => 2616,
        "gb" | "uk" => 2826,
        "de" => 2276,
        "fr" => 2250,
        "es" => 2724,
        "it" => 2380,
        "nl" => 2528,
        "ca" => 2124,
        "au" => 2036,
        _ => 2840, // United States
    }
}

/// How suggestions are sourced from DataForSEO.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// One `dataforseo_labs/google/keyword_suggestions` call returning up to
    /// 1000 long-tail phrases with metrics included. Cheapest and default.
    #[default]
    Labs,
    /// The ~58-probe autocomplete matrix, mirroring what the site's search box
    /// actually suggests. Closer to Google Autocomplete, much more expensive.
    Autocomplete,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s.trim().to_lowercase().as_str() {
            "autocomplete" | "suggest" => Mode::Autocomplete,
            _ => Mode::Labs,
        }
    }
}

#[derive(Clone)]
pub struct DataForSeo {
    client: reqwest::Client,
    login: String,
    password: String,
    base_url: String,
    autocomplete_client: String,
    with_search_volume: bool,
    concurrency: usize,
    mode: Mode,
    limit: usize,
}

impl DataForSeo {
    /// Builds a provider using only explicit arguments. Prefer [`Self::from_env`]
    /// in production; tests use this to stay independent of process env.
    pub fn new(login: String, password: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("http client");

        Self {
            client,
            login,
            password,
            base_url: base_url.trim_end_matches('/').to_string(),
            autocomplete_client: "gws-wiz-serp".to_string(),
            with_search_volume: false,
            concurrency: 8,
            mode: Mode::default(),
            limit: 700,
        }
    }

    /// Applies the optional `DATAFORSEO_*` tuning knobs.
    pub fn from_env(login: String, password: String, base_url: String) -> Self {
        let mut this = Self::new(login, password, base_url);
        if let Ok(v) = std::env::var("DATAFORSEO_CLIENT") {
            if !v.trim().is_empty() {
                this.autocomplete_client = v;
            }
        }
        this.with_search_volume = std::env::var("DATAFORSEO_SEARCH_VOLUME")
            .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
            .unwrap_or(false);
        if let Some(c) = std::env::var("DATAFORSEO_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            this.concurrency = c.clamp(1, 20);
        }
        if let Ok(m) = std::env::var("DATAFORSEO_MODE") {
            this.mode = Mode::parse(&m);
        }
        if let Some(l) = std::env::var("DATAFORSEO_LIMIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            this.limit = l.clamp(1, 1000);
        }
        this
    }

    /// Selects the sourcing strategy.
    pub fn with_mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Enables Google Ads volume/CPC enrichment.
    pub fn with_search_volume(mut self, enabled: bool) -> Self {
        self.with_search_volume = enabled;
        self
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

        // DataForSEO reports the real reason in status_code/status_message even
        // on non-2xx responses, so prefer those over dumping the raw body: the
        // message ends up in the UI and a wall of JSON helps nobody.
        let code = value.get("status_code").and_then(Value::as_i64);
        let msg = value
            .get("status_message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error")
            .trim()
            .trim_end_matches('.');

        if !status.is_success() {
            match code {
                Some(c) => anyhow::bail!(
                    "dataforseo {path}: {msg} (http {}, code {c})",
                    status.as_u16()
                ),
                None => anyhow::bail!("dataforseo {path}: http {status}"),
            }
        }
        if code != Some(20000) {
            anyhow::bail!(
                "dataforseo {path}: {msg} (code {})",
                code.unwrap_or_default()
            );
        }
        Ok(value)
    }

    /// One autocomplete call. Returns the suggestion strings.
    async fn autocomplete(
        &self,
        keyword: &str,
        language: &str,
        location: i64,
    ) -> anyhow::Result<Vec<String>> {
        let body = json!([{
            "keyword": keyword,
            "language_code": language,
            "location_code": location,
            "client": self.autocomplete_client,
        }]);

        let value = self
            .post("/v3/serp/google/autocomplete/live/advanced", body)
            .await?;

        let mut out = Vec::new();
        if let Some(tasks) = value.get("tasks").and_then(Value::as_array) {
            for task in tasks {
                let task_code = task.get("status_code").and_then(Value::as_i64).unwrap_or(0);
                if task_code != 20000 {
                    let msg = task
                        .get("status_message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    tracing::warn!(
                        "dataforseo autocomplete task `{keyword}` status {task_code}: {msg}"
                    );
                    continue;
                }
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
                        if let Some(s) = item.get("suggestion").and_then(Value::as_str) {
                            out.push(s.to_string());
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Google Ads metrics for up to 1000 keywords per call.
    async fn search_volume(
        &self,
        keywords: &[String],
        language: &str,
        location: i64,
    ) -> anyhow::Result<HashMap<String, (Option<i64>, Option<f64>, Option<i32>)>> {
        let mut map = HashMap::new();
        for chunk in keywords.chunks(700) {
            let body = json!([{
                "keywords": chunk,
                "language_code": language,
                "location_code": location,
            }]);
            let value = self
                .post("/v3/keywords_data/google_ads/search_volume/live", body)
                .await?;

            for task in value
                .get("tasks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                for r in task
                    .get("result")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(kw) = r.get("keyword").and_then(Value::as_str) else {
                        continue;
                    };
                    map.insert(
                        kw.to_lowercase(),
                        (
                            r.get("search_volume").and_then(Value::as_i64),
                            r.get("cpc").and_then(Value::as_f64),
                            r.get("competition_index")
                                .and_then(Value::as_i64)
                                .map(|v| v as i32),
                        ),
                    );
                }
            }
        }
        Ok(map)
    }

    /// One Labs call returning long-tail phrases that contain the seed keyword,
    /// with search volume, CPC and competition already attached.
    async fn keyword_suggestions(
        &self,
        keyword: &str,
        language: &str,
        location: i64,
    ) -> anyhow::Result<Vec<Suggestion>> {
        let body = json!([{
            "keyword": keyword,
            "language_code": language,
            "location_code": location,
            "limit": self.limit,
            "order_by": ["keyword_info.search_volume,desc"],
        }]);

        let value = self
            .post("/v3/dataforseo_labs/google/keyword_suggestions/live", body)
            .await?;

        let mut out = Vec::new();
        // Collect the raw phrases too: the vocabulary is detected from the whole
        // batch, so a mis-set language dropdown cannot silently dump every result
        // into the alphabetical bucket.
        let mut raw: Vec<String> = Vec::new();
        for task in value
            .get("tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let code = task.get("status_code").and_then(Value::as_i64).unwrap_or(0);
            if code != 20000 {
                let msg = task
                    .get("status_message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                anyhow::bail!("dataforseo labs task status {code}: {msg}");
            }
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
                    let kd = item.get("keyword_data").unwrap_or(item);
                    let Some(text) = kd.get("keyword").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let info = kd.get("keyword_info");
                    raw.push(text.to_string());
                    out.push(Suggestion {
                        text: text.to_string(),
                        // Filled in below, once the batch language is known.
                        category: String::new(),
                        modifier: String::new(),
                        search_volume: info
                            .and_then(|i| i.get("search_volume"))
                            .and_then(Value::as_i64),
                        cpc: info.and_then(|i| i.get("cpc")).and_then(Value::as_f64),
                        // Labs reports competition as 0..1; scale to the 0..100
                        // index used by the Google Ads endpoint so the UI and CSV
                        // stay consistent across modes.
                        competition: info
                            .and_then(|i| i.get("competition"))
                            .and_then(Value::as_f64)
                            .map(|c| (c * 100.0).round() as i32),
                    });
                }
            }
        }
        let vocab = detect_vocabulary(&raw, language);
        for s in &mut out {
            let (cat, modifier) = classify_with(&s.text, keyword, vocab);
            s.category = cat.as_str().to_string();
            s.modifier = modifier;
        }

        Ok(dedupe(keyword, out))
    }
}

#[async_trait::async_trait]
impl SuggestionProvider for DataForSeo {
    fn name(&self) -> &'static str {
        "dataforseo"
    }

    async fn harvest(
        &self,
        keyword: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Vec<Suggestion>> {
        let location = location_code(country);

        if self.mode == Mode::Labs {
            let items = self
                .keyword_suggestions(keyword, language, location)
                .await?;
            if items.is_empty() {
                anyhow::bail!("dataforseo labs returned no suggestions for `{keyword}`");
            }
            return Ok(items);
        }

        let results: Vec<Vec<Suggestion>> = stream::iter(probes(keyword))
            .map(|p| {
                let this = self.clone();
                let language = language.to_string();
                async move {
                    match this.autocomplete(&p.query, &language, location).await {
                        Ok(items) => items
                            .into_iter()
                            .map(|text| Suggestion::new(text, p.category, p.modifier.clone()))
                            .collect(),
                        Err(e) => {
                            tracing::warn!("dataforseo autocomplete failed for `{}`: {e}", p.query);
                            Vec::new()
                        }
                    }
                }
            })
            .buffer_unordered(self.concurrency)
            .collect()
            .await;

        let mut suggestions = dedupe(keyword, results.into_iter().flatten().collect());

        if suggestions.is_empty() {
            anyhow::bail!("dataforseo returned no suggestions for `{keyword}`");
        }

        if self.with_search_volume {
            let keywords: Vec<String> = suggestions.iter().map(|s| s.text.clone()).collect();
            match self.search_volume(&keywords, language, location).await {
                Ok(metrics) => {
                    for s in &mut suggestions {
                        if let Some((vol, cpc, comp)) = metrics.get(&s.text.to_lowercase()) {
                            s.search_volume = *vol;
                            s.cpc = *cpc;
                            s.competition = *comp;
                        }
                    }
                    // Most interesting keywords first inside each modifier group.
                    suggestions.sort_by(|a, b| {
                        b.search_volume
                            .unwrap_or(-1)
                            .cmp(&a.search_volume.unwrap_or(-1))
                    });
                }
                Err(e) => tracing::warn!("dataforseo search volume failed: {e}"),
            }
        }

        Ok(suggestions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_offered_market_has_a_real_location_code() {
        // A market in the dropdown that silently fell back to the US would send
        // searches to the wrong country without any visible error.
        for m in crate::domain::MARKETS.iter() {
            let code = location_code(m.country);
            if m.country != "us" {
                assert_ne!(
                    code, 2840,
                    "market {} falls back to the US location code",
                    m.label
                );
            }
        }
    }

    #[test]
    fn maps_countries_to_location_codes() {
        assert_eq!(location_code("pl"), 2616);
        assert_eq!(location_code("GB"), 2826);
        assert_eq!(location_code("zz"), 2840);
    }
}
