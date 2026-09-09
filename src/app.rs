use crate::domain::{
    group, Brief, Category, Draft, ModifierGroup, SearchDiff, SearchResult, SearchSummary, Source,
    Suggestion, MARKETS,
};
use crate::domain::{CitationRecord, DraftCheck, SourceOverlap, WatchSummary};
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_params_map;
use leptos_router::{ParamSegment, StaticSegment};

#[cfg(feature = "ssr")]
pub mod ssr {
    use apalis_sql::postgres::PostgresStorage;
    use leptos::prelude::ServerFnError;
    use sqlx::PgPool;

    #[derive(Clone)]
    pub struct AppState {
        pub pool: PgPool,
        pub storage: PostgresStorage<crate::jobs::HarvestJob>,
        pub brief_storage: PostgresStorage<crate::brief_job::BriefJob>,
        pub draft_storage: PostgresStorage<crate::draft_job::DraftJob>,
        /// Whether a model is configured; the UI hides drafting without one.
        pub writing_enabled: bool,
    }

    /// Row shape of the `searches` table as selected by the server fns.
    pub type SearchRow = (
        uuid::Uuid,
        String,
        String,
        String,
        String,
        Option<String>,
        i32,
        chrono::DateTime<chrono::Utc>,
        String,
        String,
    );

    /// Row shape of the `suggestions` table as selected by the server fns.
    pub type SuggestionRow = (
        String,
        String,
        String,
        Option<i64>,
        Option<f64>,
        Option<i32>,
    );

    /// Age of a run in words. Rounded, because nobody acts differently on 47
    /// versus 49 hours; the only question is whether it is stale enough to
    /// refresh.
    pub fn humanise_age(then: chrono::DateTime<chrono::Utc>) -> String {
        let mins = (chrono::Utc::now() - then).num_minutes();
        match mins {
            m if m < 2 => "just now".into(),
            m if m < 60 => format!("{m} min ago"),
            m if m < 60 * 24 => {
                let h = m / 60;
                if h == 1 {
                    "1 hour ago".into()
                } else {
                    format!("{h} hours ago")
                }
            }
            m if m < 60 * 24 * 2 => "1 day ago".into(),
            m if m < 60 * 24 * 60 => format!("{} days ago", m / (60 * 24)),
            m => format!("{} months ago", m / (60 * 24 * 30)),
        }
    }

    pub fn summary(r: SearchRow) -> crate::domain::SearchSummary {
        crate::domain::SearchSummary {
            id: r.0.to_string(),
            keyword: r.1,
            language: r.2,
            country: r.3,
            status: r.4,
            error: r.5,
            suggestion_count: r.6,
            age: humanise_age(r.7),
            created_at: r.7.to_rfc3339(),
            provider: r.8,
            source: r.9,
        }
    }

    /// Row shape of `briefs` as selected above.
    pub type BriefRow = (
        uuid::Uuid,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        chrono::DateTime<chrono::Utc>,
    );

    /// id, status, error, model, content, created_at, kind
    pub type DraftRow = (
        uuid::Uuid,
        String,
        Option<String>,
        String,
        Option<String>,
        chrono::DateTime<chrono::Utc>,
        String,
    );

    /// Row shape of `brief_competitors`.
    pub type CompetitorRow = (
        i32,
        String,
        String,
        Option<String>,
        Option<String>,
        serde_json::Value,
        bool,
        Option<i32>,
    );

    fn strings(v: serde_json::Value) -> Vec<String> {
        serde_json::from_value(v).unwrap_or_default()
    }

    pub fn brief(r: BriefRow, comps: Vec<CompetitorRow>) -> crate::domain::Brief {
        crate::domain::Brief {
            id: r.0.to_string(),
            topic: r.1,
            language: r.2,
            country: r.3,
            status: r.4,
            error: r.5,
            ai_overview: r.6,
            ai_sources: strings(r.7),
            questions: strings(r.8),
            related: strings(r.9),
            created_at: r.10.to_rfc3339(),
            competitors: comps
                .into_iter()
                .map(|c| crate::domain::Competitor {
                    rank: c.0,
                    url: c.1,
                    domain: c.2,
                    title: c.3,
                    description: c.4,
                    headings: serde_json::from_value(c.5).unwrap_or_default(),
                    parsed: c.6,
                    // The page view never needs the body text, and shipping
                    // ~10k characters per competitor to the browser for
                    // nothing would slow the brief page down.
                    content: None,
                    domain_rank: c.7,
                })
                .collect(),
        }
    }

    pub fn state() -> Result<AppState, ServerFnError> {
        leptos::prelude::use_context::<AppState>()
            .ok_or_else(|| ServerFnError::new("app state missing"))
    }
}

#[server(CreateSearch, "/api")]
pub async fn create_search(
    keyword: String,
    market: String,
    source: String,
) -> Result<String, ServerFnError> {
    use apalis::prelude::Storage;

    let keyword = keyword.trim().to_string();
    if keyword.is_empty() {
        return Err(ServerFnError::new("keyword is empty"));
    }
    if keyword.chars().count() > 100 {
        return Err(ServerFnError::new("keyword too long"));
    }

    // Only known markets are accepted. Search engines do not serve every
    // language in every country, and DataForSEO rejects invalid pairs with an
    // opaque "Invalid Field: 'language_code'", so the pairing is validated here
    // instead of being discovered by a failed job.
    let market = crate::domain::market(&market)
        .ok_or_else(|| ServerFnError::new(format!("unsupported market: {market}")))?;
    let language = market.language.to_string();
    let country = market.country.to_string();
    let source = crate::domain::Source::parse(&source);

    let mut st = ssr::state()?;

    let id: uuid::Uuid = sqlx::query_scalar(
        "insert into searches (keyword, language, country, source)
         values ($1, $2, $3, $4) returning id",
    )
    .bind(&keyword)
    .bind(&language)
    .bind(&country)
    .bind(source.as_str())
    .fetch_one(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    st.storage
        .push(crate::jobs::HarvestJob {
            search_id: id,
            keyword,
            language,
            country,
            source: source.as_str().to_string(),
        })
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    leptos_axum::redirect(&format!("/search/{id}"));
    Ok(id.to_string())
}

#[server(GetSearch, "/api")]
pub async fn get_search(id: String) -> Result<SearchResult, ServerFnError> {
    use ssr::{summary, SearchRow, SuggestionRow};
    let uid = uuid::Uuid::parse_str(&id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let row: Option<SearchRow> = sqlx::query_as(
        "select id, keyword, language, country, status, error, suggestion_count, created_at,
                provider, source
           from searches where id = $1",
    )
    .bind(uid)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let row = row.ok_or_else(|| ServerFnError::new("search not found"))?;

    let suggestions: Vec<SuggestionRow> = sqlx::query_as(
        "select text, category, modifier, search_volume, cpc, competition
           from suggestions where search_id = $1
          order by category, modifier, search_volume desc nulls last, text",
    )
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(SearchResult {
        search: summary(row),
        suggestions: suggestions
            .into_iter()
            .map(
                |(text, category, modifier, search_volume, cpc, competition)| Suggestion {
                    text,
                    category,
                    modifier,
                    search_volume,
                    cpc,
                    competition,
                },
            )
            .collect(),
    })
}

/// Compares a search against the previous run of the same keyword and source.
///
/// Every search is already its own row with a timestamp, so tracking how a topic
/// changes over time needs no new storage, only a set difference.
/// Queues content briefs for the picked topics.
///
/// Topics arrive as newline-separated text: the picker is a plain form, so this
/// works without client-side state and degrades gracefully.
#[server(CreateBriefs, "/api")]
pub async fn create_briefs(
    topics: String,
    market: String,
    search_id: String,
    stay: Option<String>,
) -> Result<String, ServerFnError> {
    use apalis::prelude::Storage;

    let market = crate::domain::market(&market)
        .ok_or_else(|| ServerFnError::new(format!("unsupported market: {market}")))?;

    let topics: Vec<String> = topics
        .lines()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && t.chars().count() <= 200)
        .collect();

    if topics.is_empty() {
        return Err(ServerFnError::new("pick at least one topic"));
    }
    // Each topic costs about a cent in API calls, so the batch is bounded.
    if topics.len() > 20 {
        return Err(ServerFnError::new("at most 20 topics at a time"));
    }

    let mut st = ssr::state()?;
    let search_uuid = uuid::Uuid::parse_str(&search_id).ok();
    let mut first = String::new();

    for topic in topics {
        let id: uuid::Uuid = sqlx::query_scalar(
            "insert into briefs (topic, language, country, search_id)
             values ($1, $2, $3, $4) returning id",
        )
        .bind(&topic)
        .bind(market.language)
        .bind(market.country)
        .bind(search_uuid)
        .fetch_one(&st.pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        st.brief_storage
            .push(crate::brief_job::BriefJob {
                brief_id: id,
                topic,
                language: market.language.to_string(),
                country: market.country.to_string(),
            })
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if first.is_empty() {
            first = id.to_string();
        }
    }

    // From the results page the reader wants the briefs list next. From inside
    // a brief they are queueing a follow-up and want to stay where they are.
    if stay.as_deref() != Some("1") {
        leptos_axum::redirect("/briefs");
    }
    Ok(first)
}

/// Records how one site fared for a brief, and returns the history for it.
///
/// Writing the result down matters: `ai_sources` is overwritten whenever a
/// topic is researched again, so a check that is not stored cannot be compared
/// against later.
#[server(CheckCitation, "/api")]
pub async fn check_citation(
    brief_id: String,
    domain: String,
) -> Result<Vec<CitationRecord>, ServerFnError> {
    let uid = uuid::Uuid::parse_str(&brief_id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let want = crate::aeo::normalise_domain(&domain);
    if want.is_empty() || !want.contains('.') {
        return Err(ServerFnError::new(
            "enter a domain, for example kawa.pl or www.kawa.pl",
        ));
    }

    let brief = crate::draft_job::load_brief(&st.pool, uid)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let c = crate::aeo::check(&brief, &want);

    sqlx::query(
        "insert into citation_checks (brief_id, domain, cited, citation_rank, organic_rank)
         values ($1, $2, $3, $4, $5)
         on conflict (brief_id, domain) do update
            set cited = excluded.cited,
                citation_rank = excluded.citation_rank,
                organic_rank = excluded.organic_rank,
                created_at = now()",
    )
    .bind(uid)
    .bind(&c.domain)
    .bind(c.cited)
    .bind(c.citation_rank.map(|r| r as i32))
    .bind(c.organic_rank)
    .execute(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    citation_history(want).await
}

/// Every recorded check for a domain, newest first.
#[server(CitationHistory, "/api")]
pub async fn citation_history(domain: String) -> Result<Vec<CitationRecord>, ServerFnError> {
    let st = ssr::state()?;
    let want = crate::aeo::normalise_domain(&domain);

    type Row = (
        String,
        bool,
        Option<i32>,
        Option<i32>,
        chrono::DateTime<chrono::Utc>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "select b.topic, c.cited, c.citation_rank, c.organic_rank, c.created_at
           from citation_checks c join briefs b on b.id = c.brief_id
          where c.domain = $1
          order by c.created_at desc
          limit 50",
    )
    .bind(&want)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| CitationRecord {
            domain: want.clone(),
            topic: r.0,
            cited: r.1,
            citation_rank: r.2,
            organic_rank: r.3,
            created_at: r.4.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect())
}

/// Queues a draft for a finished brief.
///
/// `kind` is "article" or "faq"; anything else is treated as an article so a
/// stale form cannot fail.
#[server(WriteDraft, "/api")]
pub async fn write_draft(brief_id: String, kind: Option<String>) -> Result<String, ServerFnError> {
    use apalis::prelude::Storage;

    let uid = uuid::Uuid::parse_str(&brief_id).map_err(|_| ServerFnError::new("bad id"))?;
    let mut st = ssr::state()?;

    if !st.writing_enabled {
        return Err(ServerFnError::new(
            "drafting is disabled: set OPENROUTER_API_KEY and restart",
        ));
    }

    let status: Option<String> = sqlx::query_scalar("select status from briefs where id = $1")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    match status.as_deref() {
        None => return Err(ServerFnError::new("brief not found")),
        // Writing from an unfinished brief would use half the research.
        Some("done") => {}
        Some(s) => return Err(ServerFnError::new(format!("brief is {s}, not done yet"))),
    }

    let kind = crate::writer::Kind::from_str(kind.as_deref().unwrap_or("article"));

    // A FAQ without questions would be an empty document; say so rather than
    // spending a request to find out.
    if kind == crate::writer::Kind::Faq {
        // jsonb_array_length returns int4, so this must not be i64.
        let n: Option<i32> =
            sqlx::query_scalar("select jsonb_array_length(questions) from briefs where id = $1")
                .bind(uid)
                .fetch_optional(&st.pool)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?
                .flatten();
        if n.unwrap_or(0) == 0 {
            return Err(ServerFnError::new(
                "this brief has no People Also Ask questions, so there is no FAQ to write",
            ));
        }
    }

    let id: uuid::Uuid =
        sqlx::query_scalar("insert into drafts (brief_id, kind) values ($1, $2) returning id")
            .bind(uid)
            .bind(kind.as_str())
            .fetch_one(&st.pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

    st.draft_storage
        .push(crate::draft_job::DraftJob {
            draft_id: id,
            brief_id: uid,
            kind: kind.as_str().to_string(),
        })
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(id.to_string())
}

#[server(ListDrafts, "/api")]
pub async fn list_drafts(brief_id: String) -> Result<Vec<Draft>, ServerFnError> {
    let uid = uuid::Uuid::parse_str(&brief_id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let rows: Vec<ssr::DraftRow> = sqlx::query_as(
        "select id, status, error, model, content, created_at, kind
           from drafts where brief_id = $1 order by created_at desc",
    )
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| Draft {
            id: r.0.to_string(),
            brief_id: brief_id.clone(),
            status: r.1,
            error: r.2,
            model: r.3,
            content: r.4,
            created_at: r.5.to_rfc3339(),
            kind: r.6,
        })
        .collect())
}

/// Whether a model is configured, so the UI can hide what cannot work.
#[server(WritingEnabled, "/api")]
pub async fn writing_enabled() -> Result<bool, ServerFnError> {
    Ok(ssr::state()?.writing_enabled)
}

#[server(ListBriefs, "/api")]
pub async fn list_briefs() -> Result<Vec<Brief>, ServerFnError> {
    let st = ssr::state()?;
    let rows: Vec<ssr::BriefRow> = sqlx::query_as(
        "select id, topic, language, country, status, error, ai_overview,
                ai_sources, questions, related, created_at
           from briefs order by created_at desc limit 40",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    // The list view only needs headline numbers, so competitors are not loaded.
    Ok(rows
        .into_iter()
        .map(|r| ssr::brief(r, Vec::new()))
        .collect())
}

#[server(GetBrief, "/api")]
pub async fn get_brief(id: String) -> Result<Brief, ServerFnError> {
    let uid = uuid::Uuid::parse_str(&id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let row: Option<ssr::BriefRow> = sqlx::query_as(
        "select id, topic, language, country, status, error, ai_overview,
                ai_sources, questions, related, created_at
           from briefs where id = $1",
    )
    .bind(uid)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let row = row.ok_or_else(|| ServerFnError::new("brief not found"))?;

    let comps: Vec<ssr::CompetitorRow> = sqlx::query_as(
        "select rank, url, domain, title, description, headings, parsed, domain_rank
           from brief_competitors where brief_id = $1 order by rank",
    )
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(ssr::brief(row, comps))
}

#[server(CompareSearch, "/api")]
pub async fn compare_search(id: String) -> Result<Option<SearchDiff>, ServerFnError> {
    use ssr::{summary, SearchRow, SuggestionRow};
    let uid = uuid::Uuid::parse_str(&id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let current: Option<SearchRow> = sqlx::query_as(
        "select id, keyword, language, country, status, error, suggestion_count, created_at,
                provider, source
           from searches where id = $1",
    )
    .bind(uid)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(current) = current else {
        return Err(ServerFnError::new("search not found"));
    };

    // The most recent finished run of the same keyword/source before this one.
    let previous: Option<SearchRow> = sqlx::query_as(
        "select id, keyword, language, country, status, error, suggestion_count, created_at,
                provider, source
           from searches
          where lower(keyword) = lower($1) and source = $2 and status = 'done'
            and created_at < $3
          order by created_at desc
          limit 1",
    )
    .bind(&current.1)
    .bind(&current.9)
    .bind(current.7)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(previous) = previous else {
        return Ok(None);
    };

    let fetch = |a: uuid::Uuid, b: uuid::Uuid| {
        let pool = st.pool.clone();
        async move {
            sqlx::query_as::<_, SuggestionRow>(
                "select text, category, modifier, search_volume, cpc, competition
                   from suggestions
                  where search_id = $1
                    and text not in (select text from suggestions where search_id = $2)
                  order by search_volume desc nulls last, text",
            )
            .bind(a)
            .bind(b)
            .fetch_all(&pool)
            .await
        }
    };

    let added = fetch(current.0, previous.0)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let removed = fetch(previous.0, current.0)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let to_suggestions = |rows: Vec<SuggestionRow>| {
        rows.into_iter()
            .map(
                |(text, category, modifier, search_volume, cpc, competition)| Suggestion {
                    text,
                    category,
                    modifier,
                    search_volume,
                    cpc,
                    competition,
                },
            )
            .collect()
    };

    Ok(Some(SearchDiff {
        previous: summary(previous),
        current: summary(current),
        added: to_suggestions(added),
        removed: to_suggestions(removed),
    }))
}

/// Re-runs an existing search with the same keyword, market and source.
///
/// Paired with the comparison view this covers the practical half of trend
/// monitoring: run it again, see what moved. Scheduling and notifications would
/// need accounts and a mail path, which this single-user app does not have.
#[server(RerunSearch, "/api")]
pub async fn rerun_search(id: String) -> Result<String, ServerFnError> {
    use apalis::prelude::Storage;

    let uid = uuid::Uuid::parse_str(&id).map_err(|_| ServerFnError::new("bad id"))?;
    let mut st = ssr::state()?;

    let original: Option<(String, String, String, String)> =
        sqlx::query_as("select keyword, language, country, source from searches where id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some((keyword, language, country, source)) = original else {
        return Err(ServerFnError::new("search not found"));
    };

    let new_id: uuid::Uuid = sqlx::query_scalar(
        "insert into searches (keyword, language, country, source)
         values ($1, $2, $3, $4) returning id",
    )
    .bind(&keyword)
    .bind(&language)
    .bind(&country)
    .bind(&source)
    .fetch_one(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    st.storage
        .push(crate::jobs::HarvestJob {
            search_id: new_id,
            keyword,
            language,
            country,
            source,
        })
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    leptos_axum::redirect(&format!("/search/{new_id}"));
    Ok(new_id.to_string())
}

/// Re-runs a search from the saved list, staying on the list.
///
/// Same work as [`rerun_search`] without the redirect: from a list of fifteen
/// searches, being thrown onto one result page is the wrong outcome. The
/// refreshed row appears in place once the job finishes.
#[server(UpdateSearch, "/api")]
pub async fn update_search(id: String) -> Result<String, ServerFnError> {
    use apalis::prelude::Storage;

    let uid = uuid::Uuid::parse_str(&id).map_err(|_| ServerFnError::new("bad id"))?;
    let mut st = ssr::state()?;

    let original: Option<(String, String, String, String)> =
        sqlx::query_as("select keyword, language, country, source from searches where id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some((keyword, language, country, source)) = original else {
        return Err(ServerFnError::new("search not found"));
    };

    let new_id: uuid::Uuid = sqlx::query_scalar(
        "insert into searches (keyword, language, country, source)
         values ($1, $2, $3, $4) returning id",
    )
    .bind(&keyword)
    .bind(&language)
    .bind(&country)
    .bind(&source)
    .fetch_one(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    st.storage
        .push(crate::jobs::HarvestJob {
            search_id: new_id,
            keyword,
            language,
            country,
            source,
        })
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(new_id.to_string())
}

/// How the same keyword looks across the engines it has been run on.
///
/// Returns nothing unless at least two sources have a finished run, since a
/// comparison of one thing with itself is noise. Uses the newest run per
/// source, so re-running one engine refreshes its column without disturbing
/// the others.
#[server(SourceComparison, "/api")]
pub async fn source_comparison(id: String) -> Result<Option<SourceOverlap>, ServerFnError> {
    let uid = uuid::Uuid::parse_str(&id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let head: Option<(String, String, String)> =
        sqlx::query_as("select keyword, language, country from searches where id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    let Some((keyword, language, country)) = head else {
        return Ok(None);
    };

    type Row = (String, String, Option<i64>, Option<f64>);
    let rows: Vec<Row> = sqlx::query_as(
        "with latest as (
             select distinct on (source) id, source
               from searches
              where lower(keyword) = lower($1) and language = $2 and country = $3
                and status = 'done'
              order by source, created_at desc
         )
         select l.source, g.text, g.search_volume, g.cpc
           from latest l join suggestions g on g.search_id = l.id",
    )
    .bind(&keyword)
    .bind(&language)
    .bind(&country)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let mut sources: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    sources.sort();
    sources.dedup();
    if sources.len() < 2 {
        return Ok(None);
    }

    // Phrase -> which sources offer it, with the richest data kept (Google is
    // the only source carrying volume, so its copy wins when present).
    use std::collections::BTreeMap;
    let mut by_text: BTreeMap<String, (Vec<String>, Suggestion)> = BTreeMap::new();
    for (source, text, vol, cpc) in rows {
        let key = text.trim().to_lowercase();
        let entry = by_text.entry(key).or_insert_with(|| {
            (
                Vec::new(),
                Suggestion {
                    text: text.clone(),
                    category: String::new(),
                    modifier: String::new(),
                    search_volume: None,
                    cpc: None,
                    competition: None,
                },
            )
        });
        if !entry.0.contains(&source) {
            entry.0.push(source);
        }
        if vol.is_some() {
            entry.1.search_volume = vol;
            entry.1.cpc = cpc;
        }
    }

    let n = sources.len();
    let mut shared = Vec::new();
    let mut only: BTreeMap<String, Vec<Suggestion>> = BTreeMap::new();
    for (_, (srcs, s)) in by_text {
        if srcs.len() == n {
            shared.push(s);
        } else if srcs.len() == 1 {
            only.entry(srcs[0].clone()).or_default().push(s);
        }
    }
    // Most searched first where volume exists. Bing and YouTube carry none, and
    // alphabetical order there put the oddest phrases on top ("acnh ile
    // kawaii"), so ties fall back to length: shorter phrases are the
    // established ones, long tails are the strays.
    let by_volume = |a: &Suggestion, b: &Suggestion| {
        b.search_volume
            .cmp(&a.search_volume)
            .then(a.text.len().cmp(&b.text.len()))
    };
    shared.sort_by(by_volume);
    for v in only.values_mut() {
        v.sort_by(by_volume);
    }

    Ok(Some(SourceOverlap {
        keyword,
        sources,
        shared,
        only: only.into_iter().collect(),
    }))
}

/// Checks a finished draft's figures and attributions against the competitor
/// pages and the AI answer stored for its brief.
///
/// Run on demand rather than on every page load: the corpus is ~130k
/// characters per brief and the scan is not free.
#[server(CheckDraft, "/api")]
pub async fn check_draft(draft_id: String) -> Result<DraftCheck, ServerFnError> {
    use crate::aeo::verify::{verify, ClaimKind, Source};
    use crate::domain::UncheckedClaim;

    let uid = uuid::Uuid::parse_str(&draft_id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let head: Option<(uuid::Uuid, Option<String>)> =
        sqlx::query_as("select brief_id, content from drafts where id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    let Some((brief_id, Some(content))) = head else {
        return Err(ServerFnError::new("draft not found or not written yet"));
    };

    let comps: Vec<(String, String)> = sqlx::query_as(
        "select domain, content from brief_competitors
          where brief_id = $1 and content is not null",
    )
    .bind(brief_id)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;
    let ai: Option<String> = sqlx::query_scalar("select ai_overview from briefs where id = $1")
        .bind(brief_id)
        .fetch_one(&st.pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let mut sources: Vec<Source> = comps
        .iter()
        .map(|(d, t)| Source { name: d, text: t })
        .collect();
    if let Some(a) = &ai {
        sources.push(Source {
            name: "AI overview",
            text: a,
        });
    }
    let n_sources = sources.len();
    if n_sources == 0 {
        // Briefs researched before competitor text was stored have nothing
        // to check against. Say so instead of reporting every claim as
        // unbacked, which would be the same number with the opposite meaning.
        return Ok(DraftCheck {
            total: 0,
            backed: 0,
            unbacked: vec![],
            sources: 0,
        });
    }

    let v = verify(&content, &sources);
    Ok(DraftCheck {
        total: v.total(),
        backed: v.backed(),
        unbacked: v
            .unbacked()
            .into_iter()
            .map(|c| UncheckedClaim {
                text: c.text.clone(),
                kind: match c.kind {
                    ClaimKind::Figure => "figure".into(),
                    ClaimKind::Attribution => "attribution".into(),
                },
            })
            .collect(),
        sources: n_sources,
    })
}

/// Starts or stops watching a search's keyword on a schedule.
#[server(ToggleWatch, "/api")]
pub async fn toggle_watch(
    search_id: String,
    every_days: Option<i32>,
) -> Result<bool, ServerFnError> {
    let uid = uuid::Uuid::parse_str(&search_id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;

    let head: Option<(String, String, String, String)> =
        sqlx::query_as("select keyword, language, country, source from searches where id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    let Some((keyword, language, country, source)) = head else {
        return Err(ServerFnError::new("search not found"));
    };

    // The first run already happened (that is the search being watched), so
    // the clock starts now rather than firing immediately.
    let now_enabled: bool = sqlx::query_scalar(
        "insert into watches (keyword, language, country, source, every_days, last_run_at)
         values ($1, $2, $3, $4, $5, now())
         on conflict (lower(keyword), language, country, source) do update
            set enabled = not watches.enabled,
                every_days = coalesce($5, watches.every_days)
         returning enabled",
    )
    .bind(&keyword)
    .bind(&language)
    .bind(&country)
    .bind(&source)
    .bind(every_days.unwrap_or(7))
    .fetch_one(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(now_enabled)
}

/// Whether this search's keyword is being watched.
#[server(WatchStatus, "/api")]
pub async fn watch_status(search_id: String) -> Result<Option<i32>, ServerFnError> {
    let uid = uuid::Uuid::parse_str(&search_id).map_err(|_| ServerFnError::new("bad id"))?;
    let st = ssr::state()?;
    let days: Option<i32> = sqlx::query_scalar(
        "select w.every_days from watches w join searches s
             on lower(s.keyword) = lower(w.keyword) and s.language = w.language
            and s.country = w.country and s.source = w.source
          where s.id = $1 and w.enabled",
    )
    .bind(uid)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(days)
}

/// Every watch with what moved on its last run.
#[server(ListWatches, "/api")]
pub async fn list_watches() -> Result<Vec<WatchSummary>, ServerFnError> {
    let st = ssr::state()?;

    type Row = (
        uuid::Uuid,
        String,
        String,
        String,
        String,
        i32,
        bool,
        Option<uuid::Uuid>,
        Option<uuid::Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    // The two newest finished runs per watch, so the diff is newest vs the
    // one before, which is what "what changed" means.
    let rows: Vec<Row> = sqlx::query_as(
        "select w.id, w.keyword, w.source, w.language, w.country, w.every_days, w.enabled,
                r.ids[1], r.ids[2], r.newest
           from watches w
           left join lateral (
                select array_agg(s.id order by s.created_at desc) ids,
                       max(s.created_at) newest
                  from (select id, created_at from searches s2
                         where lower(s2.keyword) = lower(w.keyword)
                           and s2.language = w.language and s2.country = w.country
                           and s2.source = w.source and s2.status = 'done'
                         order by created_at desc limit 2) s
           ) r on true
          order by w.enabled desc, r.newest desc nulls last",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let mut out = Vec::new();
    for (id, keyword, source, language, country, every_days, enabled, newest, prev, at) in rows {
        let (added, removed) = match (newest, prev) {
            (Some(n), Some(p)) => {
                let a: i64 = sqlx::query_scalar(
                    "select count(*) from suggestions n where n.search_id = $1
                       and not exists (select 1 from suggestions p
                                        where p.search_id = $2 and lower(p.text) = lower(n.text))",
                )
                .bind(n)
                .bind(p)
                .fetch_one(&st.pool)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
                let r: i64 = sqlx::query_scalar(
                    "select count(*) from suggestions p where p.search_id = $2
                       and not exists (select 1 from suggestions n
                                        where n.search_id = $1 and lower(n.text) = lower(p.text))",
                )
                .bind(n)
                .bind(p)
                .fetch_one(&st.pool)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
                (a as i32, r as i32)
            }
            _ => (0, 0),
        };
        out.push(WatchSummary {
            id: id.to_string(),
            keyword,
            source,
            language,
            country,
            every_days,
            enabled,
            latest_search: newest.map(|u| u.to_string()),
            added,
            removed,
            age: at.map(ssr::humanise_age).unwrap_or_else(|| "never".into()),
        });
    }
    Ok(out)
}

#[server(RecentSearches, "/api")]
pub async fn recent_searches() -> Result<Vec<SearchSummary>, ServerFnError> {
    use ssr::{summary, SearchRow};
    let st = ssr::state()?;
    // One row per keyword+source+locale, newest first. Running the same search
    // seven times is normal (that is how the comparison view gets something to
    // compare) but a list of seven identical rows is useless: what a reader
    // wants is the current state of each thing they have researched.
    let rows: Vec<SearchRow> = sqlx::query_as(
        "select distinct on (lower(keyword), source, language, country)
                id, keyword, language, country, status, error, suggestion_count, created_at,
                provider, source
           from searches
          order by lower(keyword), source, language, country, created_at desc",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    // `distinct on` dictates its own ordering, so sort for display here.
    let mut out: Vec<crate::domain::SearchSummary> = rows.into_iter().map(summary).collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out.truncate(15);
    Ok(out)
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/atp.css"/>
        <Title text="Answer the Crab - keyword question explorer"/>
        <Router>
            <header class="site-header">
                <A href="/"><span class="logo">"🦀 answer the crab"</span></A>
                <nav class="site-nav">
                    <A href="/">"Research"</A>
                    <A href="/briefs">"Content briefs"</A>
                </nav>
            </header>
            <main>
                <Routes fallback=|| view! { <p class="empty">"Page not found."</p> }.into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=(StaticSegment("search"), ParamSegment("id")) view=SearchPage/>
                    <Route path=StaticSegment("briefs") view=BriefsPage/>
                    <Route path=(StaticSegment("brief"), ParamSegment("id")) view=BriefPage/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let submit = ServerAction::<CreateSearch>::new();
    // A shared tick so an update started from any row refreshes the whole list:
    // the refreshed run is a new row, and the old one should stop being shown.
    let refreshed = RwSignal::new(0u32);
    let recent = Resource::new(
        move || (submit.version().get(), refreshed.get()),
        |_| recent_searches(),
    );

    // While anything is running, poll: harvesting takes seconds, and a list that
    // needs a manual reload to show the result looks broken.
    #[cfg(feature = "hydrate")]
    {
        use leptos::leptos_dom::helpers::set_interval_with_handle;
        use std::time::Duration;
        if let Ok(handle) = set_interval_with_handle(
            move || {
                let busy = matches!(recent.get_untracked(), Some(Ok(ref l))
                    if l.iter().any(|s| s.status == "pending" || s.status == "running"));
                if busy {
                    refreshed.update(|t| *t += 1);
                }
            },
            Duration::from_millis(2500),
        ) {
            on_cleanup(move || handle.clear());
        }
    }

    view! {
        <section class="hero">
            <h1>"Find every question people ask"</h1>
            <p class="sub">"Type a keyword. We harvest autocomplete data and organise it into questions, prepositions, comparisons and alphabeticals."</p>
            <ActionForm action=submit>
                <div class="search-box">
                    <input type="text" name="keyword" placeholder="e.g. cold brew coffee" required autofocus/>
                    <select name="source" aria-label="Source">
                        {Source::all().iter().map(|s| view! {
                            <option value=s.as_str()>{s.label()}</option>
                        }).collect_view()}
                    </select>
                    <select name="market" aria-label="Market">
                        {MARKETS.iter().map(|m| view! {
                            <option value=format!("{}-{}", m.language, m.country)>{m.label}</option>
                        }).collect_view()}
                    </select>
                    <button type="submit" disabled=move || submit.pending().get()>
                        {move || if submit.pending().get() { "Searching..." } else { "Search" }}
                    </button>
                </div>
            </ActionForm>
            {move || submit.value().get().and_then(|r| r.err()).map(|e| view! {
                <p class="error">{e.to_string()}</p>
            })}
        </section>

        <Watches/>

        <section class="recent">
            <h2>"Recent searches"</h2>
            <Suspense fallback=move || view! { <p class="empty">"Loading..."</p> }>
                {move || recent.get().map(|res| match res {
                    Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                    Ok(list) if list.is_empty() =>
                        view! { <p class="empty">"Nothing yet. Run your first search."</p> }.into_any(),
                    Ok(list) => view! {
                        <ul class="recent-list saved">
                            {list.into_iter().map(|s| {
                                let done = s.status == "done";
                                let id = s.id.clone();
                                view! {
                                    <li>
                                        <A href=format!("/search/{}", s.id)>
                                            <span class="kw">{s.keyword.clone()}</span>
                                            <span class="locale">
                                                {format!("{} | {}",
                                                         s.country.to_uppercase(),
                                                         s.language.to_uppercase())}
                                            </span>
                                            <span class="source-badge">
                                                {Source::parse(&s.source).label()}
                                            </span>
                                            {(!done).then(|| view! {
                                                <span class=format!("badge badge-{}", s.status)>
                                                    {s.status.clone()}
                                                </span>
                                            })}
                                            <span class="count">
                                                {format!("{} results", s.suggestion_count)}
                                            </span>
                                            // How stale the data is. Autosuggest
                                            // drifts, so a result from two months
                                            // ago is a different answer to the
                                            // same question.
                                            <span class="age">{s.age.clone()}</span>
                                        </A>
                                        {done.then(|| view! { <UpdateButton id=id.clone() refreshed/> })}
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                })}
            </Suspense>
        </section>
    }
}

#[component]
fn SearchPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();

    // Poll while the job is pending so the page fills in when the worker finishes.
    let tick = RwSignal::new(0u32);
    let data = Resource::new(move || (id(), tick.get()), |(id, _)| get_search(id));

    // Tracks whether the job is still running. Reading the resource directly in
    // a Memo would happen outside <Suspense/>, which breaks SSR rendering, so an
    // Effect mirrors the status into a plain signal instead.
    let still_working = RwSignal::new(true);
    Effect::new(move |_| {
        if let Some(Ok(r)) = data.get() {
            still_working.set(r.search.status != "done" && r.search.status != "failed");
        }
    });

    #[cfg(feature = "hydrate")]
    {
        use leptos::leptos_dom::helpers::set_interval_with_handle;
        use std::time::Duration;
        if let Ok(handle) = set_interval_with_handle(
            move || {
                if still_working.get_untracked() {
                    tick.update(|t| *t += 1);
                }
            },
            Duration::from_millis(2000),
        ) {
            on_cleanup(move || handle.clear());
        }
    }

    view! {
        <Suspense fallback=move || view! { <p class="empty">"Loading..."</p> }>
            {move || data.get().map(|res| match res {
                Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                Ok(result) => view! { <ResultView result/> }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
fn ResultView(result: SearchResult) -> impl IntoView {
    let s = result.search.clone();
    let running = s.status == "pending" || s.status == "running";
    let csv_href = format!("/export/{}.csv", s.id);

    // Client-side filter. A finished search can hold 500+ phrases, which is far
    // too many to scan by eye, so narrowing them is the difference between a
    // demo and something usable.
    let filter = RwSignal::new(String::new());
    let all = StoredValue::new(result.suggestions.clone());
    let filtered = Memo::new(move |_| {
        let q = filter.get().trim().to_lowercase();
        all.with_value(|items| {
            if q.is_empty() {
                items.clone()
            } else {
                items
                    .iter()
                    .filter(|s| s.text.to_lowercase().contains(&q))
                    .cloned()
                    .collect::<Vec<_>>()
            }
        })
    });
    let total = result.suggestions.len();

    view! {
        <section class="result-head">
            <h1>{s.keyword.clone()}</h1>
            <div class="meta">
                <span class=format!("badge badge-{}", s.status)>{s.status.clone()}</span>
                <span>{format!("{} suggestions", s.suggestion_count)}</span>
                <span>{format!("{} / {}", s.language.to_uppercase(), s.country.to_uppercase())}</span>
                <span class="source-badge" title="search engine">
                    {Source::parse(&s.source).label()}
                </span>
                <span class="provider" title="data source">{s.provider.clone()}</span>
                <a class="csv" href=csv_href download>"Download CSV"</a>
                <RerunButton id=s.id.clone()/>
                <WatchToggle id=s.id.clone()/>
            </div>
            {s.error.clone().map(|e| view! { <p class="error">{e}</p> })}
            {running.then(|| view! {
                <p class="working">"Working on it, this page refreshes automatically..."</p>
            })}
        </section>

        <ChangesSince id=s.id.clone()/>

        {(total > 0).then(|| view! {
            <div class="filter-bar">
                <input
                    type="search"
                    placeholder="Filter suggestions..."
                    aria-label="Filter suggestions"
                    on:input=move |ev| filter.set(event_target_value(&ev))
                    prop:value=move || filter.get()
                />
                <span class="filter-count">
                    {move || {
                        let n = filtered.get().len();
                        if n == total { format!("{total} suggestions") }
                        else { format!("{n} of {total} suggestions") }
                    }}
                </span>
            </div>
        })}

        <BriefBar id=s.id.clone() market=format!("{}-{}", s.language, s.country)/>

        {move || {
            let groups = group(&filtered.get());
            if groups.is_empty() {
                let msg = if running {
                    "Working on it..."
                } else if total > 0 {
                    "No suggestions match that filter."
                } else {
                    // A finished search with no results means the search engine
                    // has nothing for this keyword, which is an answer, not a fault.
                    "The search engine has no suggestions for this keyword. Try a broader one."
                };
                view! { <p class="empty">{msg}</p> }.into_any()
            } else {
                // Four wheels at ~900px each run to several screens, so a
                // jump bar with counts sits above them, like the category
                // tabs in AnswerThePublic's header.
                let tabs: Vec<(String, usize)> = groups
                    .iter()
                    .map(|(cat, gs)| (cat.label().to_string(), gs.iter().map(|(_, v)| v.len()).sum()))
                    .collect();
                view! {
                    <nav class="cat-nav">
                        {tabs.into_iter().map(|(label, n)| {
                            let href = format!("#cat-{}", label.to_lowercase());
                            view! {
                                <a href=href>
                                    {label}
                                    <span class="count">{format!("{n}")}</span>
                                </a>
                            }
                        }).collect_view()}
                        <a href="#topics">"Topics"</a>
                        <a href="#intent">"Intent"</a>
                    </nav>
                    <div class="wheels">
                        {groups.into_iter().map(|(cat, gs)| view! {
                            <CategoryBlock cat gs/>
                        }).collect_view()}
                    </div>
                }.into_any()
            }
        }}

        // Analysis below the wheels, not above. The wheels are what this page
        // is for; when topics and the intent table sat first, the first wheel
        // landed 2244px down and read as missing.
        {
            let seed = s.keyword.clone();
            move || {
                let list = filtered.get();
                (list.len() > 10).then(|| view! { <Clusters seed=seed.clone() items=list/> })
            }
        }
        {move || {
            let list = filtered.get();
            (!list.is_empty()).then(|| view! { <IntentBreakdown items=list/> })
        }}
        <SourceOverlapView id=s.id.clone()/>
    }
}

/// Phrases grouped into the topics they belong to.
///
/// Each cluster carries a `pick` checkbox with its label as the value, so it
/// feeds the same brief form as an individual phrase does. Picking a cluster
/// asks for one brief on the topic, which is what a writer wants; the member
/// phrases are the long tail that brief will cover.
#[component]
fn Clusters(seed: String, items: Vec<Suggestion>) -> impl IntoView {
    let all = crate::aeo::cluster(&seed, &items);
    let total = items.len();
    // Singletons are phrases, not topics; they stay in the lists below.
    let clusters: Vec<crate::aeo::Cluster> = all.into_iter().filter(|c| c.len() > 1).collect();
    let in_clusters: usize = clusters.iter().map(|c| c.len()).sum();
    let n = clusters.len();
    let has_volume = clusters.iter().any(|c| c.volume > 0);
    let open = RwSignal::new(None::<String>);
    let show_all = RwSignal::new(false);
    const PREVIEW: usize = 12;

    view! {
        <section class="clusters" id="topics">
            <h2>
                "Topics" <span class="count">{format!("{n}")}</span>
            </h2>
            <p class="hint">
                {format!(
                    "{in_clusters} of {total} phrases fall into {n} topics; the rest are one-offs. \
                     Tick a topic to brief it as a whole."
                )}
            </p>
            <div class="cluster-grid">
                {clusters.into_iter().enumerate().map(|(i, c)| {
                    let stem = c.stem.clone();
                    let stem_for_toggle = stem.clone();
                    let stem_for_open = stem.clone();
                    let label = c.label.clone();
                    let size = c.len();
                    let vol = c.volume;
                    let cpc = c.cpc;
                    let phrases = c.phrases.clone();
                    view! {
                        <div class="cluster" class:hidden-item=move || { i >= PREVIEW && !show_all.get() }>
                            <label class="cluster-head">
                                <input type="checkbox" class="pick" value=label.clone()
                                       aria-label="pick this topic for a content brief"/>
                                <span class="cluster-label">{label.clone()}</span>
                            </label>
                            <div class="cluster-meta">
                                <span class="count">{format!("{size} phrases")}</span>
                                {has_volume.then(|| view! {
                                    <span class="vol" title="summed monthly searches">
                                        {format_volume(vol)}
                                    </span>
                                })}
                                {cpc.map(|c| view! { <span class="cpc">{format!("${c:.2}")}</span> })}
                                <button class="cluster-toggle"
                                        on:click=move |_| open.update(|o| {
                                            *o = if o.as_deref() == Some(&stem_for_toggle) { None }
                                                 else { Some(stem_for_toggle.clone()) };
                                        })>
                                    {move || if open.get().as_deref() == Some(&stem_for_open) { "hide" } else { "show" }}
                                </button>
                            </div>
                            {
                                let stem_for_list = stem.clone();
                                move || (open.get().as_deref() == Some(&stem_for_list)).then(|| view! {
                                    <ul class="q-list cluster-list">
                                        {phrases.iter().map(|p| {
                                            let v = p.search_volume.map(|v| view! {
                                                <span class="vol">{format_volume(v)}</span>
                                            });
                                            view! { <li>{p.text.clone()} {v}</li> }
                                        }).collect_view()}
                                    </ul>
                                })
                            }
                        </div>
                    }
                }).collect_view()}
            </div>
            {(n > PREVIEW).then(|| view! {
                <button class="expand" on:click=move |_| show_all.update(|s| *s = !*s)>
                    {move || if show_all.get() { "Show fewer topics".to_string() }
                             else { format!("+{} more topics", n - PREVIEW) }}
                </button>
            })}
        </section>
    }
}

/// Real demand with few advertisers: worth writing for before someone else does.
///
/// Thresholds are deliberately plain. 100 searches a month is where a phrase
/// stops being noise, and a competition index under 30 means fewer than a
/// third of the possible bidders are there.
fn is_overlooked(s: &Suggestion) -> bool {
    s.search_volume.is_some_and(|v| v >= 100) && s.competition.is_some_and(|c| c < 30)
}

/// Phrases split by what the searcher wants.
///
/// Shows the median CPC per group rather than a made-up score: it is measured,
/// and it is the evidence that the split means anything. Across our database
/// the medians run $0.39 informational to $2.66 navigational.
#[component]
fn IntentBreakdown(items: Vec<Suggestion>) -> impl IntoView {
    use crate::aeo::Intent;

    // A sortable, filterable table rather than four stacked lists. The lists
    // were honest but unusable: four columns each hundreds of rows long, with
    // no way to ask "the expensive commercial phrases" without reading all of
    // them. Intent is a filter here, not a layout.
    let order = [
        Intent::Informational,
        Intent::Commercial,
        Intent::Transactional,
        Intent::Navigational,
    ];

    let classified: Vec<(Intent, Suggestion)> = items
        .iter()
        .map(|s| (crate::aeo::classify(&s.text), s.clone()))
        .collect();
    let total = classified.len();

    // Counts and median CPC per intent, computed once.
    let stats: Vec<(Intent, usize, Option<f64>)> = order
        .iter()
        .map(|want| {
            let mine: Vec<&(Intent, Suggestion)> =
                classified.iter().filter(|(i, _)| i == want).collect();
            let mut cpcs: Vec<f64> = mine
                .iter()
                .filter_map(|(_, s)| s.cpc)
                .filter(|c| *c > 0.0)
                .collect();
            cpcs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = cpcs.get(cpcs.len() / 2).copied();
            (*want, mine.len(), median)
        })
        .filter(|(_, n, _)| *n > 0)
        .collect();

    // None means "everything"; clicking the active chip clears it.
    let active = RwSignal::new(None::<&'static str>);
    let query = RwSignal::new(String::new());
    let sort = RwSignal::new("volume");
    let limit = RwSignal::new(25usize);
    // Wanted but not fought over: real volume and few advertisers. Measured
    // on the database: 1025 such phrases across 12 searches, led by "rozwód
    // a kredyt hipoteczny" at 1900 searches and a competition index of 4.
    let overlooked = RwSignal::new(false);
    let has_competition = classified.iter().any(|(_, s)| s.competition.is_some());
    let n_overlooked = classified.iter().filter(|(_, s)| is_overlooked(s)).count();

    let rows = {
        let classified = classified.clone();
        move || {
            let q = query.get().trim().to_lowercase();
            let want = active.get();
            let only_overlooked = overlooked.get();
            let mut v: Vec<(Intent, Suggestion)> = classified
                .iter()
                .filter(|(i, s)| {
                    want.is_none_or(|w| i.slug() == w)
                        && (q.is_empty() || s.text.to_lowercase().contains(&q))
                        && (!only_overlooked || is_overlooked(s))
                })
                .cloned()
                .collect();
            match sort.get() {
                "cpc" => v.sort_by(|a, b| {
                    b.1.cpc
                        .unwrap_or(0.0)
                        .partial_cmp(&a.1.cpc.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                }),
                // Least fought over first, ties by volume so the top of the
                // list is both quiet and wanted.
                "competition" => v.sort_by(|a, b| {
                    a.1.competition
                        .unwrap_or(101)
                        .cmp(&b.1.competition.unwrap_or(101))
                        .then(b.1.search_volume.cmp(&a.1.search_volume))
                }),
                "alpha" => v.sort_by(|a, b| a.1.text.cmp(&b.1.text)),
                _ => v.sort_by(|a, b| b.1.search_volume.cmp(&a.1.search_volume)),
            }
            v
        }
    };

    // Cloned per use: closures are not Copy, and this count is needed in three
    // separate reactive blocks.
    let matched = {
        let rows = rows.clone();
        move || rows().len()
    };
    let matched_bar = matched.clone();
    let matched_more = matched.clone();

    view! {
        <section class="intent" id="intent">
            <h2>"What people want" <span class="count">{format!("{total}")}</span></h2>
            <p class="hint">
                "Median CPC is shown because advertisers bid for intent: it is the check \
                 that these groups are real and not just word matching. Click a group to \
                 filter."
            </p>

            <div class="intent-chips">
                {stats.into_iter().map(|(intent, n, median)| {
                    let slug = intent.slug();
                    let share = n * 100 / total.max(1);
                    view! {
                        <button
                            class=format!("chip chip-{slug}")
                            class:on=move || active.get() == Some(slug)
                            title=intent.advice()
                            on:click=move |_| active.update(|a| {
                                *a = if *a == Some(slug) { None } else { Some(slug) };
                            })>
                            <span class="chip-label">{intent.label()}</span>
                            <span class="chip-n">{format!("{n}")}</span>
                            <span class="chip-meta">
                                {match median {
                                    Some(c) => format!("{share}% · ${c:.2}"),
                                    None => format!("{share}%"),
                                }}
                            </span>
                        </button>
                    }
                }).collect_view()}
            </div>

            // The advice for whichever group is selected, instead of repeating
            // all four paragraphs whether or not they are relevant.
            {move || active.get().and_then(|slug| {
                order.iter().find(|i| i.slug() == slug).map(|i| view! {
                    <p class="intent-advice">{i.advice()}</p>
                })
            })}

            <div class="table-bar">
                <input type="search" class="domain-input" placeholder="Filter phrases..."
                       aria-label="Filter phrases"
                       on:input=move |ev| { limit.set(25); query.set(event_target_value(&ev)) }
                       prop:value=move || query.get()/>
                <select class="sort-select" aria-label="Sort by"
                        on:change=move |ev| sort.set(match event_target_value(&ev).as_str() {
                            "cpc" => "cpc",
                            "competition" => "competition",
                            "alpha" => "alpha",
                            _ => "volume",
                        })>
                    <option value="volume">"Most searched"</option>
                    <option value="cpc">"Highest CPC"</option>
                    <option value="competition">"Least competition"</option>
                    <option value="alpha">"A-Z"</option>
                </select>
                {(has_competition && n_overlooked > 0).then(|| view! {
                    <button class="chip chip-overlooked" class:on=move || overlooked.get()
                            title="Searched at least 100 times a month with a paid competition index under 30"
                            on:click=move |_| { limit.set(25); overlooked.update(|o| *o = !*o) }>
                        <span class="chip-label">"Overlooked"</span>
                        <span class="chip-meta">{format!("{n_overlooked} phrases")}</span>
                    </button>
                })}
                <span class="filter-count">
                    {move || {
                        let n = matched_bar();
                        if n == total { format!("{total} phrases") }
                        else { format!("{n} of {total}") }
                    }}
                </span>
            </div>

            <table class="intent-table">
                <thead>
                    <tr>
                        <th>"Phrase"</th>
                        <th class="num">"Searches"</th>
                        <th class="num">"CPC"</th>
                        {has_competition.then(|| view! {
                            <th class="num" title="How many advertisers bid on this phrase, 0-100">
                                "Competition"
                            </th>
                        })}
                        <th>"Intent"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        let all = rows();
                        let shown = limit.get().min(all.len());
                        all.into_iter().take(shown).map(|(intent, s)| {
                            let href = format!(
                                "https://www.google.com/search?q={}",
                                urlencode(&s.text)
                            );
                            view! {
                                <tr>
                                    <td>
                                        <a href=href target="_blank" rel="noreferrer">
                                            {s.text.clone()}
                                        </a>
                                    </td>
                                    <td class="num">
                                        {s.search_volume.map(format_volume).unwrap_or_default()}
                                    </td>
                                    <td class="num cpc-cell">
                                        {s.cpc.filter(|c| *c > 0.0)
                                            .map(|c| format!("${c:.2}"))
                                            .unwrap_or_default()}
                                    </td>
                                    {has_competition.then(|| {
                                        let c = s.competition;
                                        let low = is_overlooked(&s);
                                        view! {
                                            <td class="num">
                                                <span class=if low { "comp comp-low" } else { "comp" }>
                                                    {c.map(|c| c.to_string()).unwrap_or_default()}
                                                </span>
                                            </td>
                                        }
                                    })}
                                    <td>
                                        <span class=format!("tag tag-{}", intent.slug())>
                                            {intent.label()}
                                        </span>
                                    </td>
                                </tr>
                            }
                        }).collect_view()
                    }}
                </tbody>
            </table>

            {move || {
                let n = matched_more();
                let shown = limit.get();
                (n > shown).then(|| view! {
                    <button class="expand" on:click=move |_| limit.update(|l| *l += 50)>
                        {format!("Show more ({} left)", n - shown)}
                    </button>
                })
            }}
            {move || (matched() == 0).then(|| view! {
                <p class="empty">"No phrases match that filter."</p>
            })}
        </section>
    }
}

/// Re-runs a saved search from the list, without leaving the page.
///
/// Separate from [`RerunButton`] only in wording and size: on the list the
/// point is freshness ("update this"), on a result page it is comparison.
#[component]
fn UpdateButton(id: String, refreshed: RwSignal<u32>) -> impl IntoView {
    let rerun = ServerAction::<UpdateSearch>::new();
    // Nudge the list as soon as the job is queued, so the new row appears at
    // once rather than after the next poll.
    Effect::new(move |_| {
        if rerun.value().get().is_some() {
            refreshed.update(|t| *t += 1);
        }
    });
    view! {
        <ActionForm action=rerun attr:class="update-form">
            <input type="hidden" name="id" value=id/>
            <button type="submit" class="update" disabled=move || rerun.pending().get()
                    title="Fetch today's suggestions for this keyword">
                {move || if rerun.pending().get() { "Updating..." } else { "Update results" }}
            </button>
        </ActionForm>
    }
}

/// Watch or stop watching this search's keyword.
///
/// Mirrors AnswerThePublic's "Updates in N days" switch: the point is that
/// autosuggest drifts, and a topic worth researching once is worth knowing
/// about when it moves.
#[component]
fn WatchToggle(id: String) -> impl IntoView {
    let toggle = ServerAction::<ToggleWatch>::new();
    let id_for_status = id.clone();
    let status = Resource::new(
        move || (id_for_status.clone(), toggle.version().get()),
        |(id, _)| watch_status(id),
    );

    view! {
        <ActionForm action=toggle attr:class="watch-form">
            <input type="hidden" name="search_id" value=id/>
            <input type="hidden" name="every_days" value="7"/>
            <Suspense fallback=|| ()>
                {move || {
                    let days = status.get().and_then(|r| r.ok()).flatten();
                    let on = days.is_some();
                    view! {
                        <button type="submit" class="watch" class:on=on
                                disabled=move || toggle.pending().get()
                                title=if on { "Stop re-running this search" }
                                      else { "Re-run this search every 7 days and show what changed" }>
                            <span class="watch-dot"></span>
                            {match days {
                                Some(d) => format!("Updates every {d} days"),
                                None => "Track changes".to_string(),
                            }}
                        </button>
                    }
                }}
            </Suspense>
        </ActionForm>
    }
}

/// Watched topics and what moved on their last run, on the home page.
///
/// This is the alert: not an email, but the first thing seen on opening the
/// app. Hidden entirely when nothing is watched.
#[component]
fn Watches() -> impl IntoView {
    let list = Resource::new(|| (), |_| list_watches());
    view! {
        <Suspense fallback=|| ()>
            {move || list.get().and_then(|r| r.ok()).filter(|l| !l.is_empty()).map(|l| view! {
                <section class="watches">
                    <h2>"Watched topics" <span class="count">{format!("{}", l.len())}</span></h2>
                    <ul class="recent-list saved">
                        {l.into_iter().map(|w| {
                            let moved = w.added + w.removed;
                            let href = w.latest_search.clone().map(|s| format!("/search/{s}"));
                            view! {
                                <li class:off=!w.enabled>
                                    {match href {
                                        Some(h) => view! {
                                            <A href=h>
                                                <span class="kw">{w.keyword.clone()}</span>
                                                <span class="locale">
                                                    {format!("{} | {}", w.country.to_uppercase(), w.language.to_uppercase())}
                                                </span>
                                                <span class="source-badge">{Source::parse(&w.source).label()}</span>
                                                {(moved > 0).then(|| view! {
                                                    <span class="delta">
                                                        <span class="plus">{format!("+{}", w.added)}</span>
                                                        <span class="minus">{format!("-{}", w.removed)}</span>
                                                    </span>
                                                })}
                                                {(moved == 0 && w.enabled).then(|| view! {
                                                    <span class="count">"no change yet"</span>
                                                })}
                                                <span class="age">{w.age.clone()}</span>
                                                <span class="count">
                                                    {if w.enabled { format!("every {} days", w.every_days) }
                                                     else { "paused".to_string() }}
                                                </span>
                                            </A>
                                        }.into_any(),
                                        None => view! {
                                            <span class="kw">{w.keyword.clone()}</span>
                                        }.into_any(),
                                    }}
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                </section>
            })}
        </Suspense>
    }
}

/// Runs the same search again so the comparison view has something to compare.
#[component]
fn RerunButton(id: String) -> impl IntoView {
    let rerun = ServerAction::<RerunSearch>::new();
    view! {
        <ActionForm action=rerun attr:class="rerun-form">
            <input type="hidden" name="id" value=id/>
            <button type="submit" class="rerun" disabled=move || rerun.pending().get()>
                {move || if rerun.pending().get() { "Running..." } else { "Run again" }}
            </button>
        </ActionForm>
    }
}

/// Turns picked topics into content briefs.
///
/// A plain form wrapping the checkboxes rendered next to each suggestion, so the
/// selection survives without any client-side bookkeeping.
#[component]
fn BriefBar(id: String, market: String) -> impl IntoView {
    let action = ServerAction::<CreateBriefs>::new();
    let picked = RwSignal::new(0usize);
    let topics = RwSignal::new(String::new());

    // Collect the ticked phrases on submit. The checkboxes live inside the
    // category blocks rather than this form, so they are gathered from the DOM.
    let collect = move |_| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::JsCast;
            let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            let Ok(nodes) = doc.query_selector_all("input.pick:checked") else {
                return;
            };
            let mut out: Vec<String> = Vec::new();
            for i in 0..nodes.length() {
                if let Some(el) = nodes
                    .item(i)
                    .and_then(|n| n.dyn_into::<web_sys::HtmlInputElement>().ok())
                {
                    let v = el.value();
                    if !v.trim().is_empty() && !out.contains(&v) {
                        out.push(v);
                    }
                }
            }
            picked.set(out.len());
            topics.set(out.join("\n"));
        }
    };

    view! {
        <div class="brief-bar">
            <ActionForm action=action attr:id="brief-form">
                <input type="hidden" name="search_id" value=id/>
                <input type="hidden" name="market" value=market/>
                <textarea name="topics" class="hidden-item" prop:value=move || topics.get()></textarea>
                <button type="submit" class="brief-submit" on:click=collect
                        disabled=move || action.pending().get()>
                    {move || if action.pending().get() {
                        "Creating...".to_string()
                    } else {
                        "Create briefs from picked topics".to_string()
                    }}
                </button>
                {move || (picked.get() > 0).then(|| view! {
                    <span class="count">{format!("{} picked", picked.get())}</span>
                })}
                <span class="hint">"Tick the phrases you want researched, then create briefs."</span>
            </ActionForm>
            {move || action.value().get().and_then(|r| r.err()).map(|e| view! {
                <p class="error">{e.to_string()}</p>
            })}
        </div>
    }
}

/// Drafts written from this brief.
///
/// Hidden entirely when no model is configured: the prompt export covers that
/// case, and a button that can only fail is worse than no button.
#[component]
fn Drafts(brief_id: String, ready: bool, has_questions: bool) -> impl IntoView {
    let enabled = Resource::new(|| (), |_| writing_enabled());
    let action = ServerAction::<WriteDraft>::new();
    let tick = RwSignal::new(0u32);
    let id_for_list = brief_id.clone();
    let drafts = Resource::new(
        move || (id_for_list.clone(), tick.get(), action.version().get()),
        |(id, _, _)| list_drafts(id),
    );

    let working = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(Ok(list)) = drafts.get() {
            working.set(
                list.iter()
                    .any(|d| d.status == "pending" || d.status == "running"),
            );
        }
    });

    #[cfg(feature = "hydrate")]
    {
        use leptos::leptos_dom::helpers::set_interval_with_handle;
        use std::time::Duration;
        if let Ok(handle) = set_interval_with_handle(
            move || {
                if working.get_untracked() {
                    tick.update(|t| *t += 1);
                }
            },
            Duration::from_millis(3000),
        ) {
            on_cleanup(move || handle.clear());
        }
    }

    view! {
        // Transition throughout: this subtree re-renders every 3s while a draft
        // is being written, and a Suspense would tear it down each time, which
        // sent the reader back to the top of the page mid-generation.
        <Transition fallback=|| ()>
            {move || enabled.get().and_then(|r| r.ok()).filter(|on| *on).map(|_| {
                let brief_id = brief_id.clone();
                view! {
                    <section class="brief-block drafts">
                        <h2>"Write"</h2>
                        {if ready {
                            let faq_id = brief_id.clone();
                            view! {
                                <div class="write-buttons">
                                    <ActionForm action=action>
                                        <input type="hidden" name="brief_id" value=brief_id/>
                                        <input type="hidden" name="kind" value="article"/>
                                        <button type="submit" class="brief-submit"
                                                disabled=move || action.pending().get() || working.get()>
                                            {move || if action.pending().get() || working.get() {
                                                "Writing..."
                                            } else {
                                                "Write the full article"
                                            }}
                                        </button>
                                    </ActionForm>
                                    // The FAQ is a separate, much cheaper job: the
                                    // questions are already usable as headings, so
                                    // only the answers are missing.
                                    {has_questions.then(|| view! {
                                        <ActionForm action=action>
                                            <input type="hidden" name="brief_id" value=faq_id/>
                                            <input type="hidden" name="kind" value="faq"/>
                                            <button type="submit" class="brief-submit secondary"
                                                    disabled=move || action.pending().get() || working.get()>
                                                {move || if action.pending().get() || working.get() {
                                                    "Writing..."
                                                } else {
                                                    "Write just the FAQ answers"
                                                }}
                                            </button>
                                        </ActionForm>
                                    })}
                                </div>
                            }.into_any()
                        } else {
                            view! { <p class="hint">"Available once the research finishes."</p> }.into_any()
                        }}
                        {move || action.value().get().and_then(|r| r.err()).map(|e| view! {
                            <p class="error">{e.to_string()}</p>
                        })}

                        // Transition, not Suspense: polling every 3s while a draft
                        // is being written must not tear the list down and rebuild
                        // it, which threw the reader back to the top of the page.
                        <Transition fallback=|| ()>
                            {move || drafts.get().and_then(|r| r.ok()).map(|list| {
                                // Only the newest draft is expanded. Several
                                // full articles at once made the page thousands
                                // of pixels long and buried everything under it.
                                list.into_iter().enumerate().map(|(i, d)| view! {
                                    <DraftView draft=d open=i == 0/>
                                }).collect_view()
                            })}
                        </Transition>
                    </section>
                }
            })}
        </Transition>
    }
}

/// One-click research from inside a brief.
///
/// `kind = "brief"` makes a content brief for the topic (a question is a piece
/// to write); `kind = "search"` runs a keyword search (a related phrase is a
/// topic to explore). Both reuse the existing server functions unchanged.
#[component]
fn ResearchButton(topic: String, market: String, kind: &'static str) -> impl IntoView {
    let brief = ServerAction::<CreateBriefs>::new();
    let search = ServerAction::<CreateSearch>::new();
    let topic = StoredValue::new(topic);
    let market = StoredValue::new(market);
    let pending = move || brief.pending().get() || search.pending().get();

    // Once queued the button turns into a link to what it made, so the click
    // has a visible result and cannot be repeated by accident. create_briefs
    // returns the first brief's id; create_search returns the search id.
    let made = move || -> Option<(String, &'static str)> {
        if let Some(Ok(id)) = brief.value().get() {
            let href = if id.is_empty() {
                "/briefs".to_string()
            } else {
                format!("/brief/{id}")
            };
            return Some((href, "Brief queued"));
        }
        if let Some(Ok(id)) = search.value().get() {
            return Some((format!("/search/{id}"), "Search queued"));
        }
        None
    };

    view! {
        {move || match made() {
            Some((href, label)) => view! {
                <A href=href attr:class="research-done">{label}</A>
            }.into_any(),
            None => if kind == "brief" {
                view! {
                    <ActionForm action=brief attr:class="research-form">
                        <input type="hidden" name="search_id" value=""/>
                        <input type="hidden" name="stay" value="1"/>
                        <input type="hidden" name="market" value=market.get_value()/>
                        <input type="hidden" name="topics" value=topic.get_value()/>
                        <button type="submit" class="research" disabled=pending>
                            {move || if pending() { "..." } else { "Brief this" }}
                        </button>
                    </ActionForm>
                }.into_any()
            } else {
                view! {
                    <ActionForm action=search attr:class="research-form">
                        <input type="hidden" name="keyword" value=topic.get_value()/>
                        <input type="hidden" name="market" value=market.get_value()/>
                        <input type="hidden" name="source" value="google"/>
                        <button type="submit" class="research" disabled=pending>
                            {move || if pending() { "..." } else { "Research this" }}
                        </button>
                    </ActionForm>
                }.into_any()
            },
        }}
    }
}

/// Whether a site is being cited for this topic, and how that has changed.
///
/// This is the only honest check on whether any AEO work is doing anything, so
/// it deliberately reports the unflattering cases plainly rather than showing a
/// score that always looks like progress.
#[component]
fn CitationTracker(brief_id: String) -> impl IntoView {
    let action = ServerAction::<CheckCitation>::new();

    view! {
        <section class="brief-block tracker">
            <h2>"Are you being cited?"</h2>
            <p class="hint">
                "Check a site against this topic's AI answer. Each check is saved, so \
                 running the research again later shows whether anything moved."
            </p>
            <ActionForm action=action>
                <input type="hidden" name="brief_id" value=brief_id/>
                <input type="text" name="domain" class="domain-input"
                       placeholder="kawa.pl" autocomplete="off"/>
                <button type="submit" class="brief-submit secondary"
                        disabled=move || action.pending().get()>
                    {move || if action.pending().get() { "Checking..." } else { "Check" }}
                </button>
            </ActionForm>

            {move || action.value().get().and_then(|r| r.err()).map(|e| view! {
                <p class="error">{e.to_string()}</p>
            })}

            {move || action.value().get().and_then(|r| r.ok()).map(|history| {
                let latest = history.first().cloned();
                view! {
                    {latest.map(|c| {
                        // The verdict is computed server-side from the same rule
                        // the check uses, so the wording cannot drift from it.
                        let verdict = if c.cited {
                            format!("Cited by the AI answer (source {}).",
                                    c.citation_rank.unwrap_or(0))
                        } else if c.organic_rank.is_some() {
                            format!("Ranking at {} but not cited. That gap is what AEO work \
                                     addresses: the page is found, its answers are not quoted.",
                                    c.organic_rank.unwrap_or(0))
                        } else {
                            "Not in the top results for this topic. Citation almost always \
                             follows ranking, so classic visibility comes first here.".to_string()
                        };
                        view! {
                            <p class=if c.cited { "verdict cited" } else { "verdict" }>{verdict}</p>
                        }
                    })}
                    {(history.len() > 1).then(|| view! {
                        <div class="history">
                            <h3>"Earlier checks"</h3>
                            <ul class="q-list">
                                {history.into_iter().map(|h| view! {
                                    <li>
                                        {h.topic}
                                        <span class="vol">
                                            {if h.cited { "cited".to_string() }
                                             else { "not cited".to_string() }}
                                        </span>
                                        <span class="count">{h.created_at}</span>
                                    </li>
                                }).collect_view()}
                            </ul>
                        </div>
                    })}
                }
            })}
        </section>
    }
}

/// On-demand check of a draft's figures against the pages that rank.
///
/// A button rather than automatic, because the scan reads ~130k characters
/// and most drafts are looked at more often than they are published.
#[component]
fn FactCheck(draft_id: String) -> impl IntoView {
    let action = ServerAction::<CheckDraft>::new();
    let show_all = RwSignal::new(false);

    view! {
        <div class="factcheck">
            <ActionForm action=action>
                <input type="hidden" name="draft_id" value=draft_id/>
                <button type="submit" class="check-btn" disabled=move || action.pending().get()
                        title="Look for every figure and attribution in the pages that rank">
                    {move || if action.pending().get() { "Checking..." } else { "Check facts" }}
                </button>
            </ActionForm>
            {move || action.value().get().and_then(|r| r.err()).map(|e| view! {
                <p class="error">{e.to_string()}</p>
            })}
            {move || action.value().get().and_then(|r| r.ok()).map(|c| {
                if c.sources == 0 {
                    return view! {
                        <p class="hint">
                            "No competitor text stored for this brief. Research it again \
                             and the check will have something to compare against."
                        </p>
                    }.into_any();
                }
                let share = if c.total == 0 { 0 } else { c.backed * 100 / c.total };
                let n_un = c.unbacked.len();
                view! {
                    <p class="fact-summary">
                        <b>{format!("{}/{}", c.backed, c.total)}</b>
                        {format!(" claims found in the {} sources ({share}%). ", c.sources)}
                        {(n_un > 0).then(|| format!(
                            "{n_un} to verify before publishing:"
                        ))}
                    </p>
                    {(n_un > 0).then(|| view! {
                        <ul class="unbacked">
                            {c.unbacked.iter().enumerate().map(|(i, u)| {
                                let cls = if u.kind == "attribution" { "claim attr" } else { "claim" };
                                view! {
                                    <li class=cls class:hidden-item=move || { i >= 10 && !show_all.get() }>
                                        {u.text.clone()}
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                        {(n_un > 10).then(|| view! {
                            <button class="expand" on:click=move |_| show_all.update(|s| *s = !*s)>
                                {move || if show_all.get() { "Show fewer".to_string() }
                                         else { format!("+{} more", n_un - 10) }}
                            </button>
                        })}
                    })}
                }.into_any()
            })}
        </div>
    }
}

#[component]
fn DraftView(draft: Draft, open: bool) -> impl IntoView {
    let href = format!("/export/draft/{}.md", draft.id);
    let working = draft.status == "pending" || draft.status == "running";
    let is_faq = draft.kind == "faq";
    let label = if is_faq { "FAQ" } else { "Article" };
    // A FAQ is a handful of short answers, an article is thousands of words,
    // so one shared estimate would be wrong for both.
    let waiting = if is_faq {
        "Answering the questions. Takes about half a minute."
    } else {
        "Writing the full article from this brief. Takes a minute or two."
    };
    // The model name is only known once the job picks the draft up.
    let model = if draft.model.is_empty() {
        "queued".to_string()
    } else {
        draft.model.clone()
    };

    view! {
        <article class="draft" class:draft-working=move || working>
            <div class="draft-head">
                <span class="kind">{label}</span>
                <span class=format!("badge badge-{}", draft.status)>{draft.status.clone()}</span>
                <span class="count">{model}</span>
                {(draft.status == "done").then(|| view! {
                    <a class="csv" href=href download>"Download"</a>
                })}
                // FAQPage markup is what turns a FAQ into an answer-box
                // candidate; only meaningful for the FAQ kind.
                {(draft.status == "done" && is_faq).then(|| view! {
                    <a class="csv" href=format!("/export/schema/{}.html", draft.id) download
                       title="FAQPage JSON-LD, ready to paste into the page">
                        "Schema"
                    </a>
                })}
            </div>
            {draft.error.clone().map(|e| view! { <p class="error">{e}</p> })}
            // What an answer engine can do with this text. Reported as counts
            // rather than a score: a score invites writing for the number, which
            // is the keyword-density mistake in a new costume.
            // Below a few hundred characters there is nothing to measure, and
            // "0% of sentences carry a figure" on a two-line test draft is noise
            // that makes the real numbers harder to trust.
            {draft.content.as_ref().filter(|c| c.chars().count() > 400).map(|c| {
                let e = crate::aeo::evidence(c);
                let q = crate::aeo::quotability(c);
                let weak: Vec<_> = q.weak().into_iter().cloned().collect();
                view! {
                    <div class="aeo-check">
                        <span class="metric">
                            <b>{q.standalone()}</b>"/"{q.total()}" sections open with the answer"
                        </span>
                        <span class="metric">
                            <b>{e.number_share()}"%"</b>" of sentences carry a figure"
                        </span>
                        <span class=if e.attributions + e.quotations == 0 { "metric warn" } else { "metric" }>
                            <b>{e.attributions + e.quotations}</b>" sourced or quoted claims"
                        </span>
                        {e.needs_attribution().then(|| view! {
                            <p class="hint">
                                "Nothing here is attributed. Named sources and direct quotes are \
                                 the two changes with the largest measured effect on being cited."
                            </p>
                        })}
                        <FactCheck draft_id=draft.id.clone()/>
                        {(!weak.is_empty()).then(|| view! {
                            <ul class="weak-list">
                                {weak.into_iter().take(5).map(|w| view! {
                                    <li>
                                        <b>{w.heading}</b>
                                        " - "{w.problem.unwrap_or("")}
                                    </li>
                                }).collect_view()}
                            </ul>
                        })}
                    </div>
                }
            })}
            // A full article takes a minute or two on a real model (measured at
            // 98s for ~11k characters), so an empty box would read as a broken
            // page. Say what is happening and give an honest duration.
            {working.then(|| view! {
                <p class="draft-waiting">
                    <span class="spinner"></span>
                    {waiting}
                </p>
            })}
            // Older drafts collapse to a summary line: they are kept for
            // comparison, not for re-reading, and expanding them all pushed the
            // research below them out of reach.
            {draft.content.clone().map(|c| {
                let chars = c.chars().count();
                if open {
                    view! { <pre class="draft-body">{c}</pre> }.into_any()
                } else {
                    view! {
                        <details class="draft-old">
                            <summary>{format!("Show this draft ({chars} characters)")}</summary>
                            <pre class="draft-body">{c}</pre>
                        </details>
                    }.into_any()
                }
            })}
        </article>
    }
}

/// List of content briefs.
#[component]
fn BriefsPage() -> impl IntoView {
    // Briefs take a while to research, so keep polling until none are running.
    let tick = RwSignal::new(0u32);
    let briefs = Resource::new(move || tick.get(), |_| list_briefs());
    let working = RwSignal::new(true);
    Effect::new(move |_| {
        if let Some(Ok(list)) = briefs.get() {
            working.set(
                list.iter()
                    .any(|b| b.status == "pending" || b.status == "running"),
            );
        }
    });

    #[cfg(feature = "hydrate")]
    {
        use leptos::leptos_dom::helpers::set_interval_with_handle;
        use std::time::Duration;
        if let Ok(handle) = set_interval_with_handle(
            move || {
                if working.get_untracked() {
                    tick.update(|t| *t += 1);
                }
            },
            Duration::from_millis(3000),
        ) {
            on_cleanup(move || handle.clear());
        }
    }

    view! {
        <section class="hero">
            <h1>"Content briefs"</h1>
            <p class="sub">
                "What Google already answers, who it cites, and how the pages that rank are structured. \
                 Pick topics on a research page to create one."
            </p>
        </section>
        // Transition so the list is not rebuilt on every poll.
        <Transition fallback=move || view! { <p class="empty">"Loading..."</p> }>
            {move || briefs.get().map(|res| match res {
                Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                Ok(list) if list.is_empty() => view! {
                    <p class="empty">"No briefs yet. Open a finished search and pick some topics."</p>
                }.into_any(),
                Ok(list) => view! {
                    <ul class="recent-list">
                        {list.into_iter().map(|b| view! {
                            <li>
                                <A href=format!("/brief/{}", b.id)>
                                    <span class="kw">{b.topic.clone()}</span>
                                    <span class=format!("badge badge-{}", b.status)>{b.status.clone()}</span>
                                    <span class="count">
                                        {format!("{} / {}", b.language.to_uppercase(), b.country.to_uppercase())}
                                    </span>
                                </A>
                            </li>
                        }).collect_view()}
                    </ul>
                }.into_any(),
            })}
        </Transition>
    }
}

/// A single content brief.
#[component]
fn BriefPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let tick = RwSignal::new(0u32);
    let data = Resource::new(move || (id(), tick.get()), |(id, _)| get_brief(id));
    let working = RwSignal::new(true);
    Effect::new(move |_| {
        if let Some(Ok(b)) = data.get() {
            working.set(b.status != "done" && b.status != "failed");
        }
    });

    #[cfg(feature = "hydrate")]
    {
        use leptos::leptos_dom::helpers::set_interval_with_handle;
        use std::time::Duration;
        if let Ok(handle) = set_interval_with_handle(
            move || {
                if working.get_untracked() {
                    tick.update(|t| *t += 1);
                }
            },
            Duration::from_millis(3000),
        ) {
            on_cleanup(move || handle.clear());
        }
    }

    view! {
        <Transition fallback=move || view! { <p class="empty">"Loading..."</p> }>
            {move || data.get().map(|res| match res {
                Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                Ok(b) => view! { <BriefView brief=b/> }.into_any(),
            })}
        </Transition>
    }
}

#[component]
fn BriefView(brief: Brief) -> impl IntoView {
    let running = brief.status == "pending" || brief.status == "running";
    let sections = brief.common_sections();
    let parsed = brief.parsed_count();
    let total = brief.competitors.len();
    let md_href = format!("/export/brief/{}.md", brief.id);
    let prompt_href = format!("/export/brief/{}.txt", brief.id);

    view! {
        <section class="result-head">
            <h1>{brief.topic.clone()}</h1>
            <div class="meta">
                <span class=format!("badge badge-{}", brief.status)>{brief.status.clone()}</span>
                <span>{format!("{} / {}", brief.language.to_uppercase(), brief.country.to_uppercase())}</span>
                <span>{format!("{total} competitors, {parsed} readable")}</span>
                <a class="csv" href=prompt_href download>"Download prompt"</a>
                <a class="csv" href=md_href download>"Download markdown"</a>
            </div>
            {brief.error.clone().map(|e| view! { <p class="error">{e}</p> })}
            {running.then(|| view! {
                <p class="working">"Researching, this page refreshes automatically..."</p>
            })}
        </section>

        // Difficulty before format: whether to write this at all comes
        // before how to write it.
        {brief.difficulty().map(|d| {
            let median = brief.median_domain_rank().unwrap_or(0);
            let mut ranked: Vec<(String, i32)> = brief
                .competitors
                .iter()
                .filter_map(|c| c.domain_rank.map(|r| (c.domain.clone(), r)))
                .collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1));
            view! {
                <section class=format!("brief-block difficulty diff-{}", d.slug())>
                    <h2>
                        "Who you are up against"
                        <span class=format!("tag tag-diff-{}", d.slug())>{d.label()}</span>
                    </h2>
                    <p class="advice-headline">
                        {format!("Median domain authority {median} of 1000")}
                    </p>
                    <p class="hint">{d.advice()}</p>
                    <ul class="auth-list">
                        {ranked.into_iter().map(|(dom, r)| view! {
                            <li>
                                <span class="auth-bar" style=format!("width: {}%", r / 10)></span>
                                <span class="auth-dom">{dom}</span>
                                <span class="vol">{format!("{r}")}</span>
                            </li>
                        }).collect_view()}
                    </ul>
                </section>
            }
        })}

        {brief.format_advice().map(|a| view! {
            <section class="brief-block advice">
                <h2>"What to write"</h2>
                <p class="advice-headline">{a.headline}</p>
                <p class="hint">{a.detail}</p>
            </section>
        })}

        {match brief.ai_overview.clone() {
            Some(text) => view! {
                <section class="brief-block ai">
                    <h2>"Google's AI answer"</h2>
                    <p class="ai-text">{text}</p>
                    {(!brief.ai_sources.is_empty()).then(|| view! {
                        <div class="ai-sources">
                            <h3>"Cited sources"</h3>
                            <p class="hint">"To be quoted by the AI answer you have to compete with these."</p>
                            <ul>
                                {brief.ai_sources.clone().into_iter()
                                    .map(|d| view! { <li>{d}</li> }).collect_view()}
                            </ul>
                        </div>
                    })}
                </section>
            }.into_any(),
            None if !running => view! {
                <section class="brief-block muted">
                    <h2>"No AI answer"</h2>
                    <p class="hint">
                        "Google shows no AI overview for this topic. That is common for shopping \
                         and brand queries, and means classic ranking still decides visibility here."
                    </p>
                </section>
            }.into_any(),
            None => ().into_any(),
        }}

        // The gap is the actionable half of the AI answer: not "Google said
        // this" but "and here is what it left for you".
        {{
            let gap = crate::aeo::topic_gap(&brief);
            (!gap.is_empty()).then(|| view! {
                <section class="brief-block gap">
                    <h2>"What the AI answer leaves out"</h2>
                    <p class="hint">
                        "Subjects the ranking pages give a section to and Google's summary \
                         never mentions. This is the ground where a reader gains something \
                         by clicking through to you."
                    </p>
                    <ul class="q-list">
                        {gap.into_iter().map(|g| view! {
                            <li>{g.title} <span class="vol">{format!("{}x", g.competitors)}</span></li>
                        }).collect_view()}
                    </ul>
                </section>
            })
        }}

        {(brief.status == "done").then(|| view! {
            <CitationTracker brief_id=brief.id.clone()/>
        })}

        // Writing sits below the research it is based on. It used to come
        // first, but a page with several past drafts pushed the analysis 6700
        // pixels down, where nobody would find it.
        <Drafts brief_id=brief.id.clone() ready=brief.status == "done"
                has_questions=!brief.questions.is_empty()/>

        {(!brief.questions.is_empty()).then(|| view! {
            <section class="brief-block">
                <h2>"Questions to answer" <span class="count">{brief.questions.len()}</span></h2>
                <p class="hint">
                    "Taken from People Also Ask; usable as FAQ headings as they are. A question \
                     big enough for its own piece can be briefed on its own."
                </p>
                <ul class="q-list">
                    {brief.questions.clone().into_iter().map(|q| {
                        let market = format!("{}-{}", brief.language, brief.country);
                        view! {
                            <li class="with-action">
                                <span>{q.clone()}</span>
                                <ResearchButton topic=q market kind="brief"/>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </section>
        })}

        // Related searches were stored and never shown. They are the phrases
        // Google offers next, so a brief on one topic is where the next
        // search starts.
        {(!brief.related.is_empty()).then(|| view! {
            <section class="brief-block">
                <h2>"Related searches" <span class="count">{brief.related.len()}</span></h2>
                <p class="hint">
                    "What Google suggests next to this topic. Research one to get its own \
                     wheels, questions and topics."
                </p>
                <ul class="q-list">
                    {brief.related.clone().into_iter().map(|r| {
                        let market = format!("{}-{}", brief.language, brief.country);
                        view! {
                            <li class="with-action">
                                <span>{r.clone()}</span>
                                <ResearchButton topic=r market kind="search"/>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </section>
        })}

        {(!sections.is_empty()).then(|| view! {
            <section class="brief-block">
                <h2>"Sections the ranking pages agree on"</h2>
                <p class="hint">"Headings used by more than one competitor, most common first."</p>
                <ul class="q-list">
                    {sections.into_iter().map(|(title, n)| view! {
                        <li>{title} <span class="vol">{format!("{n}x")}</span></li>
                    }).collect_view()}
                </ul>
            </section>
        })}

        {(!brief.competitors.is_empty()).then(|| view! {
            <section class="brief-block">
                <h2>"Competitors"</h2>
                <div class="competitors">
                    {brief.competitors.clone().into_iter().map(|c| view! {
                        <div class="competitor">
                            <h3>
                                <span class="rank">{format!("#{}", c.rank)}</span>
                                <a href=c.url.clone() target="_blank" rel="noreferrer">{c.domain.clone()}</a>
                            </h3>
                            {c.title.clone().map(|t| view! { <p class="c-title">{t}</p> })}
                            {if c.headings.is_empty() {
                                view! { <p class="hint">"structure unavailable"</p> }.into_any()
                            } else {
                                view! {
                                    <ul class="headings">
                                        {c.headings.clone().into_iter().map(|h| view! {
                                            <li class=format!("h{}", h.level)>{h.title}</li>
                                        }).collect_view()}
                                    </ul>
                                }.into_any()
                            }}
                        </div>
                    }).collect_view()}
                </div>
            </section>
        })}
    }
}

/// Shows what changed since the previous run of the same keyword and source.
///
/// The keyword across engines: what every source agrees on, what only one has.
///
/// Hidden unless two or more sources have a finished run for this keyword.
#[component]
fn SourceOverlapView(id: String) -> impl IntoView {
    let data = Resource::new(move || id.clone(), source_comparison);

    view! {
        <Suspense fallback=|| ()>
            {move || data.get().and_then(|r| r.ok()).flatten().map(|o| {
                let n = o.sources.len();
                let labels: Vec<&'static str> =
                    o.sources.iter().map(|s| Source::parse(s).label()).collect();
                view! {
                    <section class="overlap">
                        <h2>
                            "Across engines"
                            <span class="count">{labels.join(" · ")}</span>
                        </h2>
                        <p class="hint">
                            "Each engine's suggestions reflect its own audience. A phrase all \
                             of them offer has demand everywhere; one only YouTube offers is a \
                             video topic."
                        </p>
                        <div class="overlap-grid">
                            <div class="overlap-col shared">
                                <h3>
                                    {format!("All {n} agree")}
                                    <span class="col-count">{o.shared.len()}</span>
                                </h3>
                                <OverlapList items=o.shared/>
                            </div>
                            {o.only.into_iter().map(|(source, items)| {
                                let label = Source::parse(&source).label();
                                view! {
                                    <div class=format!("overlap-col only-{source}")>
                                        <h3>
                                            {format!("Only {label}")}
                                            <span class="col-count">{items.len()}</span>
                                        </h3>
                                        <OverlapList items/>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    </section>
                }
            })}
        </Suspense>
    }
}

#[component]
fn OverlapList(items: Vec<Suggestion>) -> impl IntoView {
    const PREVIEW: usize = 12;
    let open = RwSignal::new(false);
    let total = items.len();
    view! {
        <ul class="q-list">
            {items.into_iter().enumerate().map(|(i, s)| {
                let vol = s.search_volume.map(|v| view! {
                    <span class="vol">{format_volume(v)}</span>
                });
                view! {
                    <li class:hidden-item=move || { i >= PREVIEW && !open.get() }>
                        {s.text.clone()} {vol}
                    </li>
                }
            }).collect_view()}
        </ul>
        {(total > PREVIEW).then(|| view! {
            <button class="expand" on:click=move |_| open.update(|o| *o = !*o)>
                {move || if open.get() { "Show less".to_string() }
                         else { format!("+{} more", total - PREVIEW) }}
            </button>
        })}
    }
}

/// Hidden entirely when there is no earlier run, so a first search stays clean.
#[component]
fn ChangesSince(id: String) -> impl IntoView {
    let diff = Resource::new(move || id.clone(), compare_search);

    view! {
        <Suspense fallback=|| ()>
            {move || diff.get().and_then(|r| r.ok()).flatten().map(|d| {
                let added = d.added.len();
                let removed = d.removed.len();
                let since = d.previous.created_at.get(..10).unwrap_or("").to_string();
                let show = RwSignal::new(false);

                view! {
                    <section class="changes">
                        <button class="changes-toggle" on:click=move |_| show.update(|v| *v = !*v)>
                            <span class="delta-up">{format!("+{added}")}</span>
                            <span class="delta-down">{format!("-{removed}")}</span>
                            <span class="changes-label">
                                {format!("since {since}")}
                            </span>
                            <span class="chev">{move || if show.get() { "▾" } else { "▸" }}</span>
                        </button>
                        <div class:hidden-item=move || !show.get()>
                            <div class="changes-cols">
                                <ChangeList title="New" items=d.added.clone() kind="added"/>
                                <ChangeList title="Gone" items=d.removed.clone() kind="removed"/>
                            </div>
                        </div>
                    </section>
                }
            })}
        </Suspense>
    }
}

#[component]
fn ChangeList(title: &'static str, items: Vec<Suggestion>, kind: &'static str) -> impl IntoView {
    const PREVIEW: usize = 12;
    let total = items.len();
    view! {
        <div class=format!("change-col change-{kind}")>
            <h3>{title} <span class="col-count">{total}</span></h3>
            {if total == 0 {
                view! { <p class="empty">"nothing"</p> }.into_any()
            } else {
                view! {
                    <ul>
                        {items.into_iter().take(PREVIEW).map(|s| {
                            let vol = s.search_volume.map(|v| view! {
                                <span class="vol">{format_volume(v)}</span>
                            });
                            let cpc = s.cpc.filter(|c| *c > 0.0).map(|c| view! {
                                <span class="cpc">{format!("${c:.2}")}</span>
                            });
                            view! { <li>{s.text.clone()} {vol} {cpc}</li> }
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
            {(total > PREVIEW).then(|| view! {
                <p class="col-more">{format!("+{} more", total - PREVIEW)}</p>
            })}
        </div>
    }
}

#[component]
fn CategoryBlock(cat: Category, gs: Vec<ModifierGroup>) -> impl IntoView {
    let total: usize = gs.iter().map(|(_, v)| v.len()).sum();

    // A single category can hold hundreds of phrases; rendering all of them made
    // one result page ~21 screens tall. Show the strongest few per modifier and
    // let the reader open the rest.
    let expanded = RwSignal::new(false);
    let hidden: usize = gs
        .iter()
        .map(|(_, v)| v.len().saturating_sub(PREVIEW_PER_MODIFIER))
        .sum();

    let wheel_ref = NodeRef::<leptos::html::Div>::new();
    // Only read on the client, where the PNG export runs.
    let _label = cat.label();

    let save_png = move |_| {
        #[cfg(feature = "hydrate")]
        if let Some(container) = wheel_ref.get() {
            use wasm_bindgen::JsCast;
            let el: &web_sys::Element = container.unchecked_ref();
            if let Some(svg) = el.query_selector("svg").ok().flatten() {
                download_wheel_png(svg, format!("{}.png", _label.to_lowercase()));
            }
        }
    };

    view! {
        <section class="wheel" id=format!("cat-{}", cat.label().to_lowercase())>
            <h2>
                {cat.label()} <span class="count">{format!("{total}")}</span>
                <button class="png" on:click=save_png title="Download this wheel as PNG">
                    "PNG"
                </button>
            </h2>
            <div node_ref=wheel_ref>
                <Wheel groups=gs.clone()/>
            </div>
            <div class="columns">
                {gs.into_iter().map(|(modifier, items)| {
                    let shown = items.len().min(PREVIEW_PER_MODIFIER);
                    let more = items.len() - shown;
                    view! {
                        <div class="col">
                            <h3>{modifier} <span class="col-count">{items.len()}</span></h3>
                            <ul>
                                {items.into_iter().enumerate().map(|(i, s)| {
                                    let href = format!("https://www.google.com/search?q={}", urlencode(&s.text));
                                    let volume = s.search_volume.map(|v| view! {
                                        <span class="vol" title="monthly searches">{format_volume(v)}</span>
                                    });
                                    // CPC says what advertisers pay for this
                                    // phrase, which is the plainest read on
                                    // whether it carries buying intent.
                                    let cpc = s.cpc.filter(|c| *c > 0.0).map(|c| view! {
                                        <span class="cpc" title="cost per click">{format!("${c:.2}")}</span>
                                    });
                                    let beyond_preview = i >= PREVIEW_PER_MODIFIER;
                                    let text = s.text.clone();
                                    view! {
                                        <li class:hidden-item=move || beyond_preview && !expanded.get()>
                                            <input type="checkbox" class="pick" value=text
                                                   aria-label="pick this phrase for a content brief"/>
                                            <a href=href target="_blank" rel="noreferrer">{s.text.clone()}</a>
                                            {volume}
                                            {cpc}
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                            {(more > 0).then(|| view! {
                                <p class="col-more" class:hidden-item=move || expanded.get()>
                                    {format!("+{more} more")}
                                </p>
                            })}
                        </div>
                    }
                }).collect_view()}
            </div>
            {(hidden > 0).then(|| view! {
                <button class="expand" on:click=move |_| expanded.update(|e| *e = !*e)>
                    {move || if expanded.get() {
                        "Show less".to_string()
                    } else {
                        format!("Show all {total}")
                    }}
                </button>
            })}
        </section>
    }
}

/// Phrases shown per modifier before the category has to be expanded.
const PREVIEW_PER_MODIFIER: usize = 6;

fn format_volume(v: i64) -> String {
    if v >= 1_000_000 {
        format!("{:.1}M", v as f64 / 1_000_000.0)
    } else if v >= 1_000 {
        format!("{:.1}K", v as f64 / 1_000.0)
    } else {
        v.to_string()
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// SVG spoke chart, the visual signature of the original service.
/// Rasterises a wheel to PNG and downloads it, entirely in the browser.
///
/// The SVG is serialised, drawn onto a canvas at 2x for a usable resolution,
/// and handed to the user as a blob. Nothing round-trips through the server.
#[cfg(feature = "hydrate")]
fn download_wheel_png(svg: web_sys::Element, filename: String) {
    use wasm_bindgen::{closure::Closure, JsCast};

    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };

    let Ok(serialiser) = web_sys::XmlSerializer::new() else {
        return;
    };
    let Ok(markup) = serialiser.serialize_to_string(&svg) else {
        return;
    };

    // A data URL avoids the canvas tainting that a blob URL can trigger.
    let encoded = js_sys::encode_uri_component(&markup);
    let src = format!("data:image/svg+xml;charset=utf-8,{}", String::from(encoded));

    let Some(img) = doc
        .create_element("img")
        .ok()
        .and_then(|e| e.dyn_into::<web_sys::HtmlImageElement>().ok())
    else {
        return;
    };

    let img_for_load = img.clone();
    let onload = Closure::<dyn FnMut()>::new(move || {
        let scale = 2.0;
        let w = 560.0 * scale;
        let h = 560.0 * scale;

        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(canvas) = doc
            .create_element("canvas")
            .ok()
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            return;
        };
        canvas.set_width(w as u32);
        canvas.set_height(h as u32);

        let Ok(Some(ctx)) = canvas.get_context("2d") else {
            return;
        };
        let Ok(ctx) = ctx.dyn_into::<web_sys::CanvasRenderingContext2d>() else {
            return;
        };

        // Wheels are drawn for a light page; without this the PNG is transparent
        // and unreadable in most viewers.
        ctx.set_fill_style_str("#ffffff");
        ctx.fill_rect(0.0, 0.0, w, h);
        let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(&img_for_load, 0.0, 0.0, w, h);

        if let Ok(url) = canvas.to_data_url_with_type("image/png") {
            if let Some(a) = doc
                .create_element("a")
                .ok()
                .and_then(|e| e.dyn_into::<web_sys::HtmlAnchorElement>().ok())
            {
                a.set_href(&url);
                a.set_download(&filename);
                a.click();
            }
        }
    });
    img.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    img.set_src(&src);
}

#[component]
fn Wheel(groups: Vec<ModifierGroup>) -> impl IntoView {
    // The wheel exists to show the phrases themselves. It used to plot one dot
    // per modifier labelled "d (93)", which told a reader the alphabet exists
    // but nothing about what people search for. AnswerThePublic's own wheels
    // put every phrase on its own spoke, and that is the useful shape.
    // Derived from the geometry rather than picked. Labels sit at 9px type and
    // need about 11px of arc between them to stay apart. The innermost label
    // ring is at r=208, so the circumference there is 2*pi*208 = 1307px, which
    // fits 1307/11 = 118 phrases. At 136 the gap falls to 9.2px and the labels
    // collide, which is exactly what the prepositions wheel showed.
    const MAX_PER_GROUP: usize = 12;
    const MAX_TOTAL: usize = 118;

    // Keep the highest-volume phrases in each group, then flatten. Sorting by
    // volume matters more than completeness: a wheel with 700 spokes is a grey
    // disc, and the phrases worth writing about are the searched ones.
    let mut sectors: Vec<(String, Vec<Suggestion>)> = Vec::new();
    let mut budget = MAX_TOTAL;
    for (modifier, items) in groups {
        if budget == 0 {
            break;
        }
        let take = items.len().min(MAX_PER_GROUP).min(budget);
        if take == 0 {
            continue;
        }
        budget -= take;
        sectors.push((modifier, items.into_iter().take(take).collect()));
    }
    if sectors.is_empty() {
        return view! { <svg class="wheel-svg" viewBox="0 0 900 900"></svg> }.into_any();
    }

    // Volume drives dot brightness, as in the reference: darker means searched
    // more. The scale is by rank rather than by value, because volumes are
    // heavily skewed (90 to 33,100 in our data) and a linear scale would leave
    // everything but the top phrase invisible.
    let mut volumes: Vec<i64> = sectors
        .iter()
        .flat_map(|(_, items)| items.iter().filter_map(|s| s.search_volume))
        .collect();
    volumes.sort_unstable();
    let rank_of = move |v: i64, sorted: &[i64]| -> f64 {
        if sorted.is_empty() {
            return 0.5;
        }
        let below = sorted.partition_point(|x| *x < v) as f64;
        below / sorted.len() as f64
    };

    // Sized from the longest label, not by taste. Phrase labels start just
    // outside the dots and run outwards at roughly 4.6px per character at 9px
    // type, so a 45-character phrase like "kawa dla pracowników a odliczenie
    // vat" reaches ~410px from the centre. The group arcs used to sit at 214,
    // straight through the middle of that text.
    let phrase_count: usize = sectors.iter().map(|(_, v)| v.len()).sum();
    let longest = sectors
        .iter()
        .flat_map(|(_, items)| items.iter())
        .map(|s| s.text.chars().count())
        .max()
        .unwrap_or(20) as f64;
    let r_hub = 92.0;
    let r_dot_min = 128.0;
    let r_dot_max = 196.0;
    // Labels need roughly 11px of arc each at 9px type. Solving
    // 2*pi*r/count >= 11 for r gives the radius where they stop colliding,
    // floored so a short list does not push the ring outwards needlessly.
    let r_labels_start = (phrase_count as f64 * 11.0 / std::f64::consts::TAU).max(r_dot_max + 12.0);
    // Where the longest label ends, plus room for the arc and its own label.
    let r_labels_end = r_labels_start + longest * 4.6;
    let r_arc = r_labels_end + 26.0;
    let size = (r_arc + 96.0) * 2.0;
    let cx = size / 2.0;
    let cy = size / 2.0;

    let total: usize = sectors.iter().map(|(_, v)| v.len()).sum();
    let total_f = total as f64;
    let has_volume = !volumes.is_empty();
    // The median, not the mean: volumes span 90 to 33,100 in our data and one
    // popular phrase would otherwise speak for the whole wheel.
    let median_volume = if volumes.is_empty() {
        None
    } else {
        Some(volumes[volumes.len() / 2])
    };
    let mut cpcs: Vec<f64> = sectors
        .iter()
        .flat_map(|(_, items)| items.iter().filter_map(|s| s.cpc))
        .filter(|c| *c > 0.0)
        .collect();
    cpcs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_cpc = cpcs.get(cpcs.len() / 2).copied();

    let mut spokes = Vec::new();
    let mut arcs = Vec::new();
    let mut wedges = Vec::new();
    let mut index = 0usize;

    for (si, (modifier, items)) in sectors.iter().enumerate() {
        let hue = (si as f64 / sectors.len() as f64 * 330.0) as i32;
        let start_frac = index as f64 / total_f;
        let end_frac = (index + items.len()) as f64 / total_f;

        for s in items {
            // A small gap between sectors keeps groups visually separate.
            let frac = (index as f64 + 0.5) / total_f;
            let angle = frac * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
            let (sin, cos) = angle.sin_cos();

            let share = match s.search_volume {
                Some(v) => rank_of(v, &volumes),
                None => 0.0,
            };
            // Unsearched phrases stay faint but visible; searched ones darken.
            let light = 88.0 - share * 40.0;
            let r_dot = r_dot_min + share * (r_dot_max - r_dot_min);

            let dx = cx + r_dot * cos;
            let dy = cy + r_dot * sin;

            // Text runs along the spoke, flipped on the left half so it never
            // reads upside down.
            let deg = angle.to_degrees();
            let flip = cos < 0.0;
            // Labels start on a shared radius, not next to their own dot.
            // Anchoring them to the dot puts the low-volume ones at r=136,
            // where 118 labels have 7.2px of arc each and overlap; on the
            // common ring they all get the same, adequate spacing.
            let (label_r, rot, anchor) = if flip {
                (r_labels_start, deg + 180.0, "end")
            } else {
                (r_labels_start, deg, "start")
            };
            let lx = cx + label_r * cos;
            let ly = cy + label_r * sin;

            let text = s.text.clone();
            let title = match (s.search_volume, s.cpc) {
                (Some(v), Some(c)) => {
                    format!("{text} - {} searches/mo, ${c:.2} CPC", format_volume(v))
                }
                (Some(v), None) => format!("{text} - {} searches/mo", format_volume(v)),
                _ => text.clone(),
            };

            let href = format!("https://www.google.com/search?q={}", urlencode(&text));
            spokes.push(view! {
                // A link, not a bare group: these phrases are searches, and the
                // obvious thing to want on clicking one is to see the results.
                <a class="spoke" href=href target="_blank" rel="noreferrer"
                   data-group=format!("g{si}")>
                    <title>{title}</title>
                    <line x1=cx + r_hub * cos y1=cy + r_hub * sin x2=dx y2=dy
                          stroke="#e7e2de" stroke-width="1"/>
                    // A wide invisible line under the visible one, so the mouse
                    // has something to hit: a 1px stroke is nearly unhittable.
                    <line x1=cx + r_hub * cos y1=cy + r_hub * sin x2=dx y2=dy
                          stroke="transparent" stroke-width="9"/>
                    <circle cx=dx cy=dy r="4.5" class="dot"
                            fill=format!("hsl(12 90% {light}%)")/>
                    <text x=lx y=ly text-anchor=anchor class="lbl"
                          transform=format!("rotate({rot} {lx} {ly})")
                          font-size="9" dominant-baseline="middle"
                          font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                          fill="#3d3733">{text}</text>
                </a>
            });
            index += 1;
        }

        // An outer arc per modifier, labelled, so the groups stay readable.
        let (wedge, arc) = sector_arc(
            cx,
            cy,
            r_arc,
            start_frac,
            end_frac,
            hue,
            modifier.clone(),
            si,
            items.len(),
            r_hub,
        );
        wedges.push(wedge);
        arcs.push(arc);
    }

    view! {
        // Presentation lives in attributes rather than the stylesheet: a PNG
        // export rasterises the serialised SVG on its own, with no access to the
        // page's CSS, so anything styled externally would come out invisible.
        <svg class="wheel-svg" viewBox=format!("0 0 {size} {size}") role="img"
             xmlns="http://www.w3.org/2000/svg">
            // Interaction styles live inside the SVG, not in the stylesheet.
            // The PNG export serialises this element on its own and rasterises
            // it with no access to the page's CSS, so external rules would be
            // silently dropped from the image. Only hover states are here, and
            // hover never applies during export, so the PNG is unaffected.
            <style>
                {r#"
                .spoke { cursor: pointer; }
                .spoke .dot, .spoke .lbl { transition: all 120ms ease-out; }
                .spoke:hover .dot { r: 9.6; fill: #ff5a3c; }
                /* 14.4px against 9px at rest: large enough to read across the
                   wheel without the neighbours having to move. */
                .spoke:hover .lbl { font-size: 14.4px; font-weight: 700; fill: #1f1b18; }
                /* Fade everything else so one phrase can be read out of 140. */
                .wheel-svg:hover .spoke { opacity: 0.35; }
                .wheel-svg:hover .spoke:hover { opacity: 1; }
                /* Purely decorative, and it lies under every phrase: taking
                   pointer events here would block clicking the spokes. */
                .wedge { opacity: 0.055; pointer-events: none; transition: opacity 120ms ease-out; }
                .sector .arc, .sector .arc-label { transition: all 120ms ease-out; }
                .sector:hover .arc { stroke-width: 9; }
                .sector:hover .arc-label { font-size: 14px; }
                .wheel-svg:hover .sector { opacity: 0.45; }
                .wheel-svg:hover .sector:hover { opacity: 1; }
                /* Hovering a group lights up the phrases inside it. */
                .wheel-svg:has(.sector[data-group="g0"]:hover) .wedge[data-group="g0"],
                .wheel-svg:has(.sector[data-group="g1"]:hover) .wedge[data-group="g1"],
                .wheel-svg:has(.sector[data-group="g2"]:hover) .wedge[data-group="g2"],
                .wheel-svg:has(.sector[data-group="g3"]:hover) .wedge[data-group="g3"],
                .wheel-svg:has(.sector[data-group="g4"]:hover) .wedge[data-group="g4"],
                .wheel-svg:has(.sector[data-group="g5"]:hover) .wedge[data-group="g5"],
                .wheel-svg:has(.sector[data-group="g6"]:hover) .wedge[data-group="g6"],
                .wheel-svg:has(.sector[data-group="g7"]:hover) .wedge[data-group="g7"],
                .wheel-svg:has(.sector[data-group="g8"]:hover) .wedge[data-group="g8"],
                .wheel-svg:has(.sector[data-group="g9"]:hover) .wedge[data-group="g9"],
                .wheel-svg:has(.sector[data-group="g10"]:hover) .wedge[data-group="g10"],
                .wheel-svg:has(.sector[data-group="g11"]:hover) .wedge[data-group="g11"] {
                    opacity: 0.18;
                }
                .wheel-svg:has(.sector[data-group="g0"]:hover) .spoke[data-group="g0"],
                .wheel-svg:has(.sector[data-group="g1"]:hover) .spoke[data-group="g1"],
                .wheel-svg:has(.sector[data-group="g2"]:hover) .spoke[data-group="g2"],
                .wheel-svg:has(.sector[data-group="g3"]:hover) .spoke[data-group="g3"],
                .wheel-svg:has(.sector[data-group="g4"]:hover) .spoke[data-group="g4"],
                .wheel-svg:has(.sector[data-group="g5"]:hover) .spoke[data-group="g5"],
                .wheel-svg:has(.sector[data-group="g6"]:hover) .spoke[data-group="g6"],
                .wheel-svg:has(.sector[data-group="g7"]:hover) .spoke[data-group="g7"],
                .wheel-svg:has(.sector[data-group="g8"]:hover) .spoke[data-group="g8"],
                .wheel-svg:has(.sector[data-group="g9"]:hover) .spoke[data-group="g9"],
                .wheel-svg:has(.sector[data-group="g10"]:hover) .spoke[data-group="g10"],
                .wheel-svg:has(.sector[data-group="g11"]:hover) .spoke[data-group="g11"] {
                    opacity: 1;
                }
                @media (prefers-reduced-motion: reduce) {
                    .spoke .dot, .spoke .lbl, .sector .arc, .sector .arc-label {
                        transition: none;
                    }
                }
                "#}
            </style>
            <circle cx=cx cy=cy r=r_hub - 8.0 class="hub"
                    fill="#fff1ed" stroke="#ff5a3c" stroke-width="2"/>
            <text x=cx y=cy - 6.0 text-anchor="middle" dominant-baseline="middle"
                  font-size="17" font-weight="600"
                  font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                  fill="#1f1b18">{format!("{total}")}</text>
            <text x=cx y=cy + 14.0 text-anchor="middle" dominant-baseline="middle"
                  font-size="10"
                  font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                  fill="#8a8078">"phrases"</text>
            {median_volume.map(|v| view! {
                <text x=cx y=cy + 34.0 text-anchor="middle" dominant-baseline="middle"
                      font-size="10"
                      font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                      fill="#8a8078">
                    {match median_cpc {
                        Some(c) => format!("median {}/mo - ${c:.2} CPC", format_volume(v)),
                        None => format!("median {}/mo", format_volume(v)),
                    }}
                </text>
            })}
            // A legend, because a shade that means nothing to the reader is
            // just decoration. Only shown when volumes exist at all.
            {has_volume.then(|| view! {
                <g>
                    <circle cx="26" cy="20" r="5" fill="hsl(12 90% 52%)"/>
                    <text x="38" y="20" dominant-baseline="middle" font-size="11"
                          font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                          fill="#3d3733">"Most searched"</text>
                    <circle cx="26" cy="42" r="5" fill="hsl(12 90% 70%)"/>
                    <text x="38" y="42" dominant-baseline="middle" font-size="11"
                          font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                          fill="#3d3733">"Average"</text>
                    <circle cx="26" cy="64" r="5" fill="hsl(12 90% 86%)"/>
                    <text x="38" y="64" dominant-baseline="middle" font-size="11"
                          font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                          fill="#3d3733">"Least searched"</text>
                </g>
            })}
            // Painting order matters: tinted wedges underneath everything, then
            // the phrases, then the arcs. Arcs must come after the phrases
            // because the rotated labels reach past the arc radius and, drawn
            // last, they swallowed the pointer events meant for the group.
            {wedges}
            {spokes}
            {arcs}
        </svg>
    }
    .into_any()
}

/// One labelled arc around the outside of the wheel, marking a modifier's span.
#[allow(clippy::too_many_arguments)]
fn sector_arc(
    cx: f64,
    cy: f64,
    r: f64,
    start_frac: f64,
    end_frac: f64,
    hue: i32,
    label: String,
    index: usize,
    count: usize,
    r_inner: f64,
) -> (impl IntoView, impl IntoView) {
    let quarter = std::f64::consts::FRAC_PI_2;
    // Leave a hairline gap so neighbouring groups do not merge into one ring.
    let pad = 0.004_f64.min((end_frac - start_frac) / 4.0);
    let a0 = (start_frac + pad) * std::f64::consts::TAU - quarter;
    let a1 = (end_frac - pad) * std::f64::consts::TAU - quarter;
    let large = if a1 - a0 > std::f64::consts::PI { 1 } else { 0 };

    let d = format!(
        "M {} {} A {r} {r} 0 {large} 1 {} {}",
        cx + r * a0.cos(),
        cy + r * a0.sin(),
        cx + r * a1.cos(),
        cy + r * a1.sin()
    );

    // A faint wedge behind the phrases, so a group reads as one block rather
    // than as a run of unrelated spokes. Kept very light: it has to sit under
    // 9px text without competing with it.
    let wedge = format!(
        "M {} {} L {} {} A {r} {r} 0 {large} 1 {} {} L {} {} A {r_inner} {r_inner} 0 {large} 0 {} {} Z",
        cx + r_inner * a0.cos(),
        cy + r_inner * a0.sin(),
        cx + r * a0.cos(),
        cy + r * a0.sin(),
        cx + r * a1.cos(),
        cy + r * a1.sin(),
        cx + r_inner * a1.cos(),
        cy + r_inner * a1.sin(),
        cx + r_inner * a0.cos(),
        cy + r_inner * a0.sin(),
    );

    // The label sits outside the arc, upright, at the middle of the span.
    let mid = (a0 + a1) / 2.0;
    let lr = r + 14.0;
    let lx = cx + lr * mid.cos();
    let ly = cy + lr * mid.sin();
    let anchor = if mid.cos() < -0.2 {
        "end"
    } else if mid.cos() > 0.2 {
        "start"
    } else {
        "middle"
    };

    // The wedge is returned separately because it has to be painted beneath the
    // phrases while the arc has to sit above them.
    let wedge_view = view! {
        <path d=wedge class="wedge" data-group=format!("g{index}")
              fill=format!("hsl({hue} 70% 55%)") stroke="none"/>
    };

    let arc_view = view! {
        <g class="sector" data-group=format!("g{index}")>
            <title>{format!("{}: {count} phrases", label.to_uppercase())}</title>
            <path d=d.clone() fill="none" stroke=format!("hsl({hue} 70% 72%)") stroke-width="5"
                  stroke-linecap="round" class="arc"/>
            // Fat transparent copy: the visible arc is 5px and hard to hit.
            <path d=d fill="none" stroke="transparent" stroke-width="18"
                  stroke-linecap="round"/>
            <text x=lx y=ly text-anchor=anchor dominant-baseline="middle"
                  font-size="12" font-weight="600" letter-spacing="0.08em"
                  font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                  fill=format!("hsl({hue} 45% 38%)") class="arc-label">
                {label.to_uppercase()}
            </text>
        </g>
    };

    (wedge_view, arc_view)
}
