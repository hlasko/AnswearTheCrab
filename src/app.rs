use crate::domain::{
    group, Brief, Category, Draft, ModifierGroup, SearchDiff, SearchResult, SearchSummary, Source,
    Suggestion, MARKETS,
};
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

    pub fn summary(r: SearchRow) -> crate::domain::SearchSummary {
        crate::domain::SearchSummary {
            id: r.0.to_string(),
            keyword: r.1,
            language: r.2,
            country: r.3,
            status: r.4,
            error: r.5,
            suggestion_count: r.6,
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

    leptos_axum::redirect("/briefs");
    Ok(first)
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
        "select rank, url, domain, title, description, headings, parsed
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

#[server(RecentSearches, "/api")]
pub async fn recent_searches() -> Result<Vec<SearchSummary>, ServerFnError> {
    use ssr::{summary, SearchRow};
    let st = ssr::state()?;
    let rows: Vec<SearchRow> = sqlx::query_as(
        "select id, keyword, language, country, status, error, suggestion_count, created_at,
                provider, source
           from searches order by created_at desc limit 15",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(rows.into_iter().map(summary).collect())
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
    let recent = Resource::new(move || submit.version().get(), |_| recent_searches());

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

        <section class="recent">
            <h2>"Recent searches"</h2>
            <Suspense fallback=move || view! { <p class="empty">"Loading..."</p> }>
                {move || recent.get().map(|res| match res {
                    Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                    Ok(list) if list.is_empty() =>
                        view! { <p class="empty">"Nothing yet. Run your first search."</p> }.into_any(),
                    Ok(list) => view! {
                        <ul class="recent-list">
                            {list.into_iter().map(|s| view! {
                                <li>
                                    <A href=format!("/search/{}", s.id)>
                                        <span class="kw">{s.keyword.clone()}</span>
                                        <span class="source-badge">
                                            {Source::parse(&s.source).label()}
                                        </span>
                                        <span class=format!("badge badge-{}", s.status)>{s.status.clone()}</span>
                                        <span class="count">{format!("{} results", s.suggestion_count)}</span>
                                    </A>
                                </li>
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
                view! {
                    <div class="wheels">
                        {groups.into_iter().map(|(cat, gs)| view! {
                            <CategoryBlock cat gs/>
                        }).collect_view()}
                    </div>
                }.into_any()
            }
        }}
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
                                list.into_iter().map(|d| view! { <DraftView draft=d/> }).collect_view()
                            })}
                        </Transition>
                    </section>
                }
            })}
        </Transition>
    }
}

#[component]
fn DraftView(draft: Draft) -> impl IntoView {
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
            </div>
            {draft.error.clone().map(|e| view! { <p class="error">{e}</p> })}
            // A full article takes a minute or two on a real model (measured at
            // 98s for ~11k characters), so an empty box would read as a broken
            // page. Say what is happening and give an honest duration.
            {working.then(|| view! {
                <p class="draft-waiting">
                    <span class="spinner"></span>
                    {waiting}
                </p>
            })}
            {draft.content.clone().map(|c| view! { <pre class="draft-body">{c}</pre> })}
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

        <Drafts brief_id=brief.id.clone() ready=brief.status == "done"
                has_questions=!brief.questions.is_empty()/>

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

        {(!brief.questions.is_empty()).then(|| view! {
            <section class="brief-block">
                <h2>"Questions to answer" <span class="count">{brief.questions.len()}</span></h2>
                <p class="hint">"Taken from People Also Ask; usable as FAQ headings as they are."</p>
                <ul class="q-list">
                    {brief.questions.clone().into_iter().map(|q| view! { <li>{q}</li> }).collect_view()}
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
                            view! { <li>{s.text.clone()} {vol}</li> }
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
        <section class="wheel">
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
                                    let beyond_preview = i >= PREVIEW_PER_MODIFIER;
                                    let text = s.text.clone();
                                    view! {
                                        <li class:hidden-item=move || beyond_preview && !expanded.get()>
                                            <input type="checkbox" class="pick" value=text
                                                   aria-label="pick this phrase for a content brief"/>
                                            <a href=href target="_blank" rel="noreferrer">{s.text.clone()}</a>
                                            {volume}
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
    let size = 560.0_f64;
    let cx = size / 2.0;
    let cy = size / 2.0;
    let n = groups.len().max(1) as f64;

    let mut spokes = Vec::new();
    for (i, (modifier, items)) in groups.iter().enumerate() {
        let angle = (i as f64 / n) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
        let r_hub = 60.0;
        let r_out = 120.0 + (items.len().min(10) as f64) * 9.0;
        let x1 = cx + r_hub * angle.cos();
        let y1 = cy + r_hub * angle.sin();
        let x2 = cx + r_out * angle.cos();
        let y2 = cy + r_out * angle.sin();
        let hue = (i as f64 / n * 330.0) as i32;
        let dot_r = 4.0 + (items.len() as f64).sqrt();
        // Anchor the label on the side the spoke points to, so text grows away
        // from the wheel instead of back across it.
        let anchor = if angle.cos() < -0.1 { "end" } else { "start" };
        // A purely radial offset collapses to zero for near-vertical spokes and
        // the label would sit on top of its dot, so always keep a horizontal gap
        // and nudge vertically by the dot radius.
        let gap = dot_r + 6.0;
        let lx = x2 + if anchor == "end" { -gap } else { gap };
        let ly = y2 + if angle.sin() < 0.0 { -gap } else { gap + 4.0 };
        spokes.push(view! {
            <g>
                <line x1=x1 y1=y1 x2=x2 y2=y2 stroke=format!("hsl({hue} 70% 60%)") stroke-width="2"/>
                <circle cx=x2 cy=y2 r=dot_r fill=format!("hsl({hue} 70% 55%)")/>
                <text x=lx y=ly
                      text-anchor=anchor class="spoke-label"
                      font-size="11"
                      font-family="ui-sans-serif, -apple-system, Segoe UI, Inter, system-ui, sans-serif"
                      fill=format!("hsl({hue} 45% 35%)")>
                    {format!("{} ({})", modifier, items.len())}
                </text>
            </g>
        });
    }

    view! {
        // Presentation lives in attributes rather than the stylesheet: a PNG
        // export rasterises the serialised SVG on its own, with no access to the
        // page's CSS, so anything styled externally would come out invisible.
        <svg class="wheel-svg" viewBox=format!("0 0 {size} {size}") role="img"
             xmlns="http://www.w3.org/2000/svg">
            <circle cx=cx cy=cy r="52" class="hub"
                    fill="#fff1ed" stroke="#ff5a3c" stroke-width="2"/>
            {spokes}
        </svg>
    }
}
