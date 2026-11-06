//! Read-only discovery in the locally recorded answer corpus. No provider calls.
use leptos::form::ActionForm;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CORPUS_LIMIT: usize = 5_000;
pub const DISPLAY_LIMIT: usize = 100;

/// Deliberately strict ASCII DNS input. International domains must use punycode.
/// Paths are ignored, but ambiguous authorities, credentials and unsafe schemes are not.
pub fn normalize_host(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty()
        || input.len() > 4096
        || input.chars().any(|c| c.is_whitespace() || c.is_control())
        || input.contains('\\')
    {
        return Err("Enter a valid domain or HTTP(S) URL.".into());
    }
    let lower = input.to_ascii_lowercase();
    let rest = if let Some(s) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    {
        s
    } else {
        if lower.contains("://") || lower.starts_with('/') {
            return Err("Only domain names and HTTP(S) URLs are supported.".into());
        }
        lower.as_str()
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err("URLs with credentials are not allowed.".into());
    }
    let host = if let Some((h, p)) = authority.split_once(':') {
        if p.is_empty()
            || !p.bytes().all(|b| b.is_ascii_digit())
            || p.parse::<u16>().ok().filter(|p| *p > 0).is_none()
        {
            return Err("Invalid host or port.".into());
        }
        h
    } else {
        authority
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.len() > 253
        || !host.contains('.')
        || host.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        || !host
            .rsplit('.')
            .next()
            .unwrap_or("")
            .bytes()
            .any(|b| b.is_ascii_alphabetic())
    {
        return Err("Enter a valid DNS domain (use punycode for international domains).".into());
    }
    Ok(host.to_owned())
}

pub fn host_matches(host: &str, domain: &str, subdomains: bool) -> bool {
    host == domain
        || (subdomains
            && host
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.')))
}

pub fn safe_source_url(value: &str) -> Option<String> {
    // Do not trim links: reject hidden control/whitespace and browser parser ambiguity.
    if value != value.trim() {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return None;
    }
    normalize_host(value).ok().map(|_| value.to_owned())
}

pub fn text_mentions(answer: &str, domain: &str, subdomains: bool) -> bool {
    answer
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '@')))
        .filter_map(|token| normalize_host(token.trim_matches('.')).ok())
        .any(|host| host_matches(&host, domain, subdomains))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RadarRecord {
    pub prompt: String,
    pub answer: String,
    pub recorded_at: String,
    pub timestamp_basis: String,
    pub engine: String,
    pub language: String,
    pub country: String,
    pub internal_link: Option<String>,
    pub urls: Vec<String>,
    pub domains: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RadarMatch {
    pub record: RadarRecord,
    pub mentioned: bool,
    pub recorded_source: bool,
    pub matching_urls: Vec<String>,
    pub matching_domains: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RadarResults {
    pub domain: String,
    pub total_available: usize,
    pub examined: usize,
    pub matching_records: usize,
    pub distinct_prompts: usize,
    pub oldest_examined: Option<String>,
    pub newest_examined: Option<String>,
    pub matches: Vec<RadarMatch>,
}

pub fn match_corpus(
    records: Vec<RadarRecord>,
    total_available: usize,
    domain: String,
    subdomains: bool,
) -> RadarResults {
    let mut result = RadarResults {
        domain: domain.clone(),
        total_available,
        examined: records.len(),
        matching_records: 0,
        distinct_prompts: 0,
        oldest_examined: records.last().map(|r| r.recorded_at.clone()),
        newest_examined: records.first().map(|r| r.recorded_at.clone()),
        matches: Vec::new(),
    };
    let mut prompts = HashSet::new();
    for record in records {
        let mentioned = text_mentions(&record.answer, &domain, subdomains);
        let mut matching_urls: Vec<String> = record
            .urls
            .iter()
            .filter_map(|url| {
                let safe = safe_source_url(url)?;
                host_matches(&normalize_host(&safe).ok()?, &domain, subdomains).then_some(safe)
            })
            .collect();
        matching_urls.sort();
        matching_urls.dedup();
        let mut matching_domains: Vec<String> = record
            .domains
            .iter()
            .filter_map(|d| normalize_host(d).ok())
            .filter(|d| host_matches(d, &domain, subdomains))
            .collect();
        matching_domains.sort();
        matching_domains.dedup();
        let recorded_source = !matching_urls.is_empty() || !matching_domains.is_empty();
        if mentioned || recorded_source {
            result.matching_records += 1;
            prompts.insert(record.prompt.clone());
            if result.matches.len() < DISPLAY_LIMIT {
                result.matches.push(RadarMatch {
                    record,
                    mentioned,
                    recorded_source,
                    matching_urls,
                    matching_domains,
                });
            }
        }
    }
    result.distinct_prompts = prompts.len();
    result
}

/// Filters apply before the latest-record cap. Window count is before LIMIT.
#[cfg(feature = "ssr")]
const CORPUS_SQL: &str = r#"
with corpus as (
 select 'run:' || r.id::text || ':' || a.question as record_key,
        a.question as prompt, a.answer, r.created_at as recorded_at,
        'Run creation time (answer capture time not stored)'::text as timestamp_basis,
        'perplexity'::text as engine, r.language, r.country,
        case when r.search_id is not null then '/search/' || r.search_id::text end as internal_link,
        a.urls, a.domains
 from ai_answers a join ai_answer_runs r on r.id = a.run_id
 union all
 select 'brief-answer:' || b.id::text || ':' || e.n::text,
        coalesce(e.item->>'question', b.topic), e.item->>'answer', b.created_at,
        'Brief creation time (answer capture time not stored)', 'perplexity', b.language, b.country,
        '/brief/' || b.id::text,
        coalesce(e.item->'urls', '[]'::jsonb), coalesce(e.item->'domains', '[]'::jsonb)
 from briefs b cross join lateral jsonb_array_elements(
     case when jsonb_typeof(b.ai_answers) = 'array' then b.ai_answers else '[]'::jsonb end
 ) with ordinality e(item, n)
 where jsonb_typeof(e.item->'answer') = 'string'
 union all
 select 'brief-aio:' || b.id::text, b.topic, b.ai_overview, b.created_at,
        'Brief creation time (overview capture time not stored)', 'google_aio', b.language, b.country,
        '/brief/' || b.id::text, '[]'::jsonb, b.ai_sources
 from briefs b where b.ai_overview is not null
)
select prompt, answer, recorded_at, timestamp_basis, engine, language, country, internal_link,
       urls, domains, count(*) over() as total_available
from corpus
where answer ~ '[^[:space:]]'
  and ($1 = '' or engine = $1) and ($2 = '' or lower(country) = $2) and ($3 = '' or lower(language) = $3)
order by recorded_at desc, record_key asc limit $4
"#;

#[cfg(feature = "ssr")]
pub async fn load_corpus(
    pool: &sqlx::PgPool,
    engine: &str,
    country: &str,
    language: &str,
) -> Result<(Vec<RadarRecord>, usize), sqlx::Error> {
    use sqlx::Row;
    let rows = sqlx::query(CORPUS_SQL)
        .bind(engine)
        .bind(country)
        .bind(language)
        .bind(CORPUS_LIMIT as i64)
        .fetch_all(pool)
        .await?;
    let total = rows
        .first()
        .map(|r| r.get::<i64, _>("total_available") as usize)
        .unwrap_or(0);
    let strings = |v: serde_json::Value| {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let records = rows
        .into_iter()
        .map(|r| RadarRecord {
            prompt: r.get("prompt"),
            answer: r.get("answer"),
            recorded_at: r
                .get::<chrono::DateTime<chrono::Utc>, _>("recorded_at")
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            timestamp_basis: r.get("timestamp_basis"),
            engine: r.get("engine"),
            language: r.get("language"),
            country: r.get("country"),
            internal_link: r.get("internal_link"),
            urls: strings(r.get("urls")),
            domains: strings(r.get("domains")),
        })
        .collect();
    Ok((records, total))
}

#[server]
pub async fn search_radar(
    domain: String,
    engine: String,
    country: String,
    language: String,
    subdomains: Option<String>,
) -> Result<RadarResults, ServerFnError> {
    let domain = normalize_host(&domain).map_err(ServerFnError::new)?;
    if !matches!(engine.as_str(), "" | "perplexity" | "google_aio") {
        return Err(ServerFnError::new("Choose a supported engine."));
    }
    let country = country.trim().to_ascii_lowercase();
    let language = language.trim().to_ascii_lowercase();
    for value in [&country, &language] {
        if value.len() > 32 || !value.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-') {
            return Err(ServerFnError::new(
                "Use a country/language code, or leave the filter empty.",
            ));
        }
    }
    let state = crate::app::ssr::state()?;
    let (records, total) = load_corpus(&state.pool, &engine, &country, &language).await.map_err(|e| {
        tracing::warn!("Radar corpus query failed: {e}");
        ServerFnError::new("The local corpus could not be read. Please try again. No provider was called and no job was queued.")
    })?;
    Ok(match_corpus(records, total, domain, subdomains.is_some()))
}

#[component]
pub fn RadarPage() -> impl IntoView {
    let action = ServerAction::<SearchRadar>::new();
    view! {
        <section class="hero">
            <h1>"Local corpus Radar"</h1>
            <p class="sub">"Find your domain or a competitor in answers already recorded here."</p>
            <p class="hint">"Read-only search. No paid API calls, no queued jobs. This is not global coverage, reach, or search volume."</p>
        </section>
        <section class="yt-card radar-form">
            <ActionForm action=action>
                <label>"Domain or HTTP(S) URL"<input name="domain" type="text" required placeholder="example.com" maxlength="4096"/></label>
                <label><input name="subdomains" type="checkbox" value="1"/>" Include subdomains"</label>
                <label>"Engine"<select name="engine"><option value="">"All recorded engines"</option><option value="perplexity">"Perplexity"</option><option value="google_aio">"Google AI Overview"</option></select></label>
                <label>"Country code (optional)"<input name="country" placeholder="us" maxlength="32"/></label>
                <label>"Language code (optional)"<input name="language" placeholder="en" maxlength="32"/></label>
                <button type="submit" class="research" disabled=move || action.pending().get()>{move || if action.pending().get() { "Searching local records…" } else { "Search local corpus" }}</button>
            </ActionForm>
        </section>
        <p class="hint">"Evidence limitation: legacy Perplexity records merge citations, annotations and web_search_results. Recorded sources do not establish a verified citation, endorsement or rank. Google AI Overview stores source domains only, not cited URLs. Text matching finds literal domain names, not brand-name mentions."</p>
        <div aria-live="polite" aria-busy=move || if action.pending().get() { "true" } else { "false" }>
            {move || if action.pending().get() {
                view! { <p class="hint">"Examining the latest local records…"</p> }.into_any()
            } else { match action.value().get() {
                None => view! { <p class="empty">"Enter any domain to search the local corpus."</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                Some(Ok(r)) => view! { <RadarResultView results=r/> }.into_any(),
            } }}
        </div>
    }
}

#[component]
fn RadarResultView(results: RadarResults) -> impl IntoView {
    let summary = format!("{}: {} records available after engine/country/language filters; {} examined (latest-record cap {}). {} matching records across {} distinct exact prompts. Displaying {} of {} matches (display limit {}).", results.domain, results.total_available, results.examined, CORPUS_LIMIT, results.matching_records, results.distinct_prompts, results.matches.len(), results.matching_records, DISPLAY_LIMIT);
    let dates = match (&results.oldest_examined, &results.newest_examined) {
        (Some(a), Some(b)) => format!("Examined record dates: {a} through {b} (UTC)."),
        _ => "No recorded dates available.".into(),
    };
    view! {
        <section class="radar-results">
            <h2>"Recorded evidence"</h2><p>{summary}</p><p class="hint">{dates}</p>
            <p class="hint">"Repeated runs and brief snapshots count as separate records. Distinct prompts use exact text, not deduplicated intent. Dates below are record creation timestamps, not independently verified capture times."</p>
            {(results.total_available > results.examined).then(|| view! { <p class="hint">"Older records were not examined. Matching counts apply only to the examined subset."</p> })}
            {(results.total_available == 0).then(|| view! { <p class="empty">"No local answer records are available for these filters. Radar does not collect new answers."</p> })}
            {(results.total_available > 0 && results.matching_records == 0).then(|| view! { <p class="empty">"No matching domain was found in the examined records. This does not prove absence from other answers or older records."</p> })}
            {results.matches.into_iter().map(|m| {
                let label = match (m.mentioned, m.recorded_source) { (true, true) => "Mentioned in text + present in recorded sources", (true, false) => "Mentioned in text only", _ => "Present in recorded sources only" };
                view! {
                    <article class="yt-card radar-record">
                        <h3>{m.record.prompt}</h3><p class="badge">{label}</p>
                        <p class="hint">{format!("{} · language {} · country {} · {} UTC", if m.record.engine == "google_aio" { "Google AI Overview" } else { "Perplexity" }, m.record.language, m.record.country, m.record.recorded_at)}</p>
                        <p class="hint">{m.record.timestamp_basis}</p>
                        {m.record.internal_link.map(|href| view! { <a href=href>"Open original search / brief"</a> })}
                        <details class="radar-original">
                            <summary>"Original recorded answer"</summary>
                            <pre class="radar-answer" style="white-space: pre-wrap; overflow-wrap: anywhere; font: inherit;">{m.record.answer}</pre>
                        </details>
                        <h4>"Matching recorded sources"</h4>
                        {(!m.recorded_source).then(|| view! { <p class="hint">"No matching recorded source. This match is in the answer text only."</p> })}
                        <ul>{m.matching_urls.into_iter().map(|url| view! { <li><a href=url.clone() target="_blank" rel="noopener noreferrer">{url.clone()}</a></li> }).collect_view()}</ul>
                        <ul>{m.matching_domains.into_iter().map(|domain| view! { <li>{domain}" (recorded domain, not a verified URL)"</li> }).collect_view()}</ul>
                    </article>
                }
            }).collect_view()}
        </section>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod rendering_tests {
    use super::*;

    #[test]
    fn original_answer_is_escaped_and_unsafe_sources_are_not_linked() {
        let record = RadarRecord {
            prompt: "<script>prompt</script>".into(),
            answer: "<script>alert(1)</script> example.com".into(),
            recorded_at: "2026-11-06T06:00:00.000000Z".into(),
            timestamp_basis: "Run creation time".into(),
            engine: "perplexity".into(),
            language: "en".into(),
            country: "us".into(),
            internal_link: None,
            urls: vec!["javascript:alert(1)".into(), "https://example.com/a".into()],
            domains: vec![],
        };
        let results = match_corpus(vec![record], 1, "example.com".into(), false);
        let html = view! { <RadarResultView results=results/> }.to_html();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("href=\"javascript:"));
        assert!(html.contains("https://example.com/a"));
        assert!(html.contains("Mentioned in text + present in recorded sources"));
        assert!(html.contains("<details class=\"radar-original\">"));
        assert!(!html.contains("<details open"));
    }
}
