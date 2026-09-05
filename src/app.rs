use crate::domain::{group, Category, ModifierGroup, SearchResult, SearchSummary, Suggestion};
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
    language: String,
    country: String,
) -> Result<String, ServerFnError> {
    use apalis::prelude::Storage;

    let keyword = keyword.trim().to_string();
    if keyword.is_empty() {
        return Err(ServerFnError::new("keyword is empty"));
    }
    if keyword.chars().count() > 100 {
        return Err(ServerFnError::new("keyword too long"));
    }
    let language = if language.trim().is_empty() {
        "en".into()
    } else {
        language
    };
    let country = if country.trim().is_empty() {
        "us".into()
    } else {
        country
    };

    let mut st = ssr::state()?;

    let id: uuid::Uuid = sqlx::query_scalar(
        "insert into searches (keyword, language, country) values ($1, $2, $3) returning id",
    )
    .bind(&keyword)
    .bind(&language)
    .bind(&country)
    .fetch_one(&st.pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    st.storage
        .push(crate::jobs::HarvestJob {
            search_id: id,
            keyword,
            language,
            country,
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
        "select id, keyword, language, country, status, error, suggestion_count, created_at, provider
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

#[server(RecentSearches, "/api")]
pub async fn recent_searches() -> Result<Vec<SearchSummary>, ServerFnError> {
    use ssr::{summary, SearchRow};
    let st = ssr::state()?;
    let rows: Vec<SearchRow> = sqlx::query_as(
        "select id, keyword, language, country, status, error, suggestion_count, created_at, provider
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
            </header>
            <main>
                <Routes fallback=|| view! { <p class="empty">"Page not found."</p> }.into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=(StaticSegment("search"), ParamSegment("id")) view=SearchPage/>
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
                    <select name="language">
                        <option value="en">"English"</option>
                        <option value="pl">"Polski"</option>
                        <option value="de">"Deutsch"</option>
                        <option value="es">"Español"</option>
                        <option value="fr">"Français"</option>
                    </select>
                    <select name="country">
                        <option value="us">"US"</option>
                        <option value="pl">"PL"</option>
                        <option value="gb">"UK"</option>
                        <option value="de">"DE"</option>
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

    #[cfg(feature = "hydrate")]
    {
        use leptos::leptos_dom::helpers::set_interval_with_handle;
        use std::time::Duration;
        if let Ok(handle) = set_interval_with_handle(
            move || tick.update(|t| *t += 1),
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
    let groups = group(&result.suggestions);
    let running = s.status == "pending" || s.status == "running";
    let csv_href = format!("/export/{}.csv", s.id);

    view! {
        <section class="result-head">
            <h1>{s.keyword.clone()}</h1>
            <div class="meta">
                <span class=format!("badge badge-{}", s.status)>{s.status.clone()}</span>
                <span>{format!("{} suggestions", s.suggestion_count)}</span>
                <span>{format!("{} / {}", s.language.to_uppercase(), s.country.to_uppercase())}</span>
                <span class="provider" title="data source">{s.provider.clone()}</span>
                <a class="csv" href=csv_href>"Download CSV"</a>
            </div>
            {s.error.clone().map(|e| view! { <p class="error">{e}</p> })}
            {running.then(|| view! {
                <p class="working">"Working on it, this page refreshes automatically..."</p>
            })}
        </section>

        {if groups.is_empty() && !running {
            view! { <p class="empty">"No suggestions found."</p> }.into_any()
        } else {
            view! {
                <div class="wheels">
                    {groups.into_iter().map(|(cat, gs)| view! {
                        <CategoryBlock cat gs/>
                    }).collect_view()}
                </div>
            }.into_any()
        }}
    }
}

#[component]
fn CategoryBlock(cat: Category, gs: Vec<ModifierGroup>) -> impl IntoView {
    let total: usize = gs.iter().map(|(_, v)| v.len()).sum();
    view! {
        <section class="wheel">
            <h2>{cat.label()} <span class="count">{format!("{total}")}</span></h2>
            <Wheel groups=gs.clone()/>
            <div class="columns">
                {gs.into_iter().map(|(modifier, items)| view! {
                    <div class="col">
                        <h3>{modifier}</h3>
                        <ul>
                            {items.into_iter().map(|s| {
                                let href = format!("https://www.google.com/search?q={}", urlencode(&s.text));
                                let volume = s.search_volume.map(|v| view! {
                                    <span class="vol" title="monthly searches">{format_volume(v)}</span>
                                });
                                view! {
                                    <li>
                                        <a href=href target="_blank" rel="noreferrer">{s.text.clone()}</a>
                                        {volume}
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    </div>
                }).collect_view()}
            </div>
        </section>
    }
}

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
                      fill=format!("hsl({hue} 45% 35%)")>
                    {format!("{} ({})", modifier, items.len())}
                </text>
            </g>
        });
    }

    view! {
        <svg class="wheel-svg" viewBox=format!("0 0 {size} {size}") role="img">
            <circle cx=cx cy=cy r="52" class="hub"/>
            {spokes}
        </svg>
    }
}
