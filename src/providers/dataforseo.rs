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

/// What one Google Trends call returned.
#[derive(Debug, Default, Clone)]
pub struct TrendsResult {
    /// One (phrase, weekly index) pair per requested phrase, in request order.
    pub series: Vec<(String, Vec<crate::domain::TrendPoint>)>,
    /// Related queries, only when a single phrase was asked for.
    pub top: Vec<crate::domain::TrendQuery>,
    pub rising: Vec<crate::domain::TrendQuery>,
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

    /// Phrases `competitor` ranks in the top 10 for that `mine` does not rank
    /// for at all, most searched first.
    ///
    /// One `domain_intersection` call with `intersections: false`, which is
    /// the set difference. Measured: $0.013 per call regardless of how many
    /// phrases come back, and dietetycy.org.pl against fitomed.pl yields
    /// 2670 such phrases with volume >= 100. The top-10 filter matters:
    /// without it the list is led by phrases the competitor sits at position
    /// 40 for, which is not a gap anyone can act on.
    pub async fn keyword_gap(
        &self,
        competitor: &str,
        mine: &str,
        language: &str,
        country: &str,
        limit: usize,
    ) -> anyhow::Result<(Vec<crate::domain::GapPhrase>, i64)> {
        let body = json!([{
            "target1": competitor,
            "target2": mine,
            "location_code": location_code(country),
            "language_code": language,
            "intersections": false,
            "limit": limit,
            "filters": [
                ["first_domain_serp_element.rank_absolute", "<=", 10],
                "and",
                ["keyword_data.keyword_info.search_volume", ">=", 50]
            ],
            "order_by": ["keyword_data.keyword_info.search_volume,desc"],
        }]);
        let value = self
            .post("/v3/dataforseo_labs/google/domain_intersection/live", body)
            .await?;

        let mut out = Vec::new();
        let mut total = 0i64;
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
                total = result
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                for item in result
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let kd = item.get("keyword_data");
                    let Some(keyword) = kd.and_then(|k| k.get("keyword")).and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let ki = kd.and_then(|k| k.get("keyword_info"));
                    let se = item.get("first_domain_serp_element");
                    out.push(crate::domain::GapPhrase {
                        keyword: keyword.to_string(),
                        volume: ki
                            .and_then(|i| i.get("search_volume"))
                            .and_then(Value::as_i64),
                        cpc: ki.and_then(|i| i.get("cpc")).and_then(Value::as_f64),
                        competitor_rank: se
                            .and_then(|s| s.get("rank_absolute"))
                            .and_then(Value::as_i64)
                            .unwrap_or(0) as i32,
                        competitor_url: se
                            .and_then(|s| s.get("url"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
            }
        }
        Ok((out, total))
    }

    /// Google Trends for up to five phrases, in one Trends property.
    ///
    /// `property` is Trends' own term: "web" for Google search, "youtube" for
    /// YouTube search. The graph comes back for every phrase, scaled so the
    /// loudest phrase's loudest week is 100. Related-query lists come back only
    /// for a single phrase (Trends does not compute them for a comparison),
    /// so callers wanting those pass one keyword.
    ///
    /// Costs about $0.011 a call regardless of phrase count.
    pub async fn trends(
        &self,
        keywords: &[String],
        language: &str,
        country: &str,
        property: &str,
    ) -> anyhow::Result<TrendsResult> {
        anyhow::ensure!(
            !keywords.is_empty() && keywords.len() <= 5,
            "google trends takes 1 to 5 phrases, got {}",
            keywords.len()
        );
        let mut task = json!({
            "keywords": keywords,
            "location_code": location_code(country),
            "language_code": language,
            "type": property,
            "time_range": "past_12_months",
        });
        // Related queries exist only for a single phrase. Asking for them in a
        // comparison is rejected as "Invalid Field: item_types" and the whole
        // call returns nothing, which happened on the first live test.
        if keywords.len() == 1 {
            task["item_types"] = json!(["google_trends_graph", "google_trends_queries_list"]);
        }
        let value = self
            .post(
                "/v3/keywords_data/google_trends/explore/live",
                json!([task]),
            )
            .await?;
        if let Some(msg) = value
            .pointer("/tasks/0/status_message")
            .and_then(Value::as_str)
            .filter(|m| !m.starts_with("Ok"))
        {
            anyhow::bail!("google trends: {}", msg.trim_end_matches('.'));
        }

        let mut out = TrendsResult::default();
        let items = value
            .pointer("/tasks/0/result/0/items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for item in items {
            match item.get("type").and_then(Value::as_str) {
                Some("google_trends_graph") => {
                    let names: Vec<String> = item
                        .get("keywords")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    out.series = names.iter().map(|n| (n.clone(), Vec::new())).collect();
                    for week in item
                        .get("data")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let date = week
                            .get("date_from")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let values = week
                            .get("values")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        for (i, series) in out.series.iter_mut().enumerate() {
                            // Trends leaves a week null when the phrase was too
                            // quiet to measure; that is a zero for our purposes.
                            let v = values.get(i).and_then(Value::as_i64).unwrap_or(0);
                            series.1.push(crate::domain::TrendPoint {
                                date: date.clone(),
                                v,
                            });
                        }
                    }
                }
                Some("google_trends_queries_list") => {
                    let read = |key: &str| -> Vec<crate::domain::TrendQuery> {
                        item.pointer(&format!("/data/{key}"))
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|q| {
                                Some(crate::domain::TrendQuery {
                                    query: q.get("query")?.as_str()?.to_string(),
                                    value: q.get("value").and_then(Value::as_i64).unwrap_or(0),
                                })
                            })
                            .collect()
                    };
                    out.top = read("top");
                    out.rising = read("rising");
                }
                _ => {}
            }
        }
        anyhow::ensure!(
            !out.series.is_empty(),
            "google trends returned no data for {}",
            keywords.join(", ")
        );
        Ok(out)
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
    /// Monthly Google search volume for a few phrases, keyed by lowercase phrase.
    pub async fn google_volume(
        &self,
        keywords: &[String],
        language: &str,
        country: &str,
    ) -> anyhow::Result<HashMap<String, i64>> {
        let map = self
            .search_volume(keywords, language, location_code(country))
            .await?;
        Ok(map
            .into_iter()
            .filter_map(|(k, (v, _, _))| v.map(|v| (k, v)))
            .collect())
    }

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
                        // The API lists months newest first; reverse so a
                        // sparkline reads left to right in time.
                        monthly: info
                            .and_then(|i| i.get("monthly_searches"))
                            .and_then(Value::as_array)
                            .map(|ms| {
                                let mut v: Vec<i64> = ms
                                    .iter()
                                    .filter_map(|m| m.get("search_volume").and_then(Value::as_i64))
                                    .collect();
                                v.reverse();
                                v
                            })
                            .unwrap_or_default(),
                        trend_yearly: info
                            .and_then(|i| i.get("search_volume_trend"))
                            .and_then(|t| t.get("yearly"))
                            .and_then(Value::as_i64)
                            .map(|t| t as i32),
                        trend_quarterly: info
                            .and_then(|i| i.get("search_volume_trend"))
                            .and_then(|t| t.get("quarterly"))
                            .and_then(Value::as_i64)
                            .map(|t| t as i32),
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
            // An empty result is a legitimate answer for an obscure keyword, not
            // a failure. Reporting it as an error made a working search look
            // broken, complete with a red "failed" badge.
            return self.keyword_suggestions(keyword, language, location).await;
        }

        let results: Vec<Vec<Suggestion>> = stream::iter(probes(keyword, language))
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
