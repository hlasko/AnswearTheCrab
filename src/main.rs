// The brief page nests enough view types that rustc's default query depth is
// not enough to compute their layout when the binary monomorphises them.
#![recursion_limit = "512"]

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use apalis::prelude::*;
    use apalis_sql::{
        postgres::{PgListen, PgPool, PostgresStorage},
        Config,
    };
    use atp::app::ssr::AppState;
    use atp::app::{shell, App};
    use atp::brief_job::{build_brief, BriefJob, Sources};
    use atp::draft_job::{write_draft, DraftJob};
    use atp::jobs::{harvest, HarvestJob, QUEUE};
    use atp::providers::Providers;
    use axum::{extract::Path, http::StatusCode, response::IntoResponse, routing::get, Router};
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};

    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".to_string());
    let pool = PgPool::connect(&database_url).await?;

    // apalis owns `_sqlx_migrations`, so run app schema separately and idempotently.
    PostgresStorage::setup(&pool).await?;
    for sql in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0002_briefs.sql"),
        include_str!("../migrations/0003_drafts.sql"),
        include_str!("../migrations/0004_draft_kind.sql"),
        include_str!("../migrations/0005_citation_checks.sql"),
        include_str!("../migrations/0006_competitor_content.sql"),
        include_str!("../migrations/0007_domain_rank.sql"),
        include_str!("../migrations/0008_watches.sql"),
        include_str!("../migrations/0009_settings.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await?;
    }

    let mut storage: PostgresStorage<HarvestJob> =
        PostgresStorage::new_with_config(pool.clone(), Config::new(QUEUE));
    let mut brief_storage: PostgresStorage<BriefJob> =
        PostgresStorage::new_with_config(pool.clone(), Config::new(atp::brief_job::QUEUE));
    let mut draft_storage: PostgresStorage<DraftJob> =
        PostgresStorage::new_with_config(pool.clone(), Config::new(atp::draft_job::QUEUE));

    // Push notifications so queued jobs start immediately.
    let mut listener = PgListen::new(pool.clone()).await?;
    listener.subscribe_with(&mut storage);
    listener.subscribe_with(&mut brief_storage);
    listener.subscribe_with(&mut draft_storage);
    tokio::spawn(async move {
        if let Err(e) = listener.listen().await {
            tracing::error!("pg listener stopped: {e}");
        }
    });

    // `cargo leptos` injects LEPTOS_* env vars. When the binary is run directly
    // (docker, systemd, `cargo run`) they are missing and the client bundle URLs
    // collapse to `/pkg/.js`, silently breaking hydration. Fall back to Cargo.toml.
    let conf = if std::env::var("LEPTOS_OUTPUT_NAME").is_ok() {
        get_configuration(None)?
    } else {
        get_configuration(Some("Cargo.toml"))?
    };
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let state = AppState {
        pool: pool.clone(),
        storage: storage.clone(),
        brief_storage: brief_storage.clone(),
        draft_storage: draft_storage.clone(),
        writing_enabled: atp::writer::Writers::from_env().is_enabled(),
    };

    // CSV export of a finished search.
    /// category, modifier, suggestion, search_volume, cpc
    type CsvRow = (String, String, String, Option<i64>, Option<f64>);

    async fn export_csv(
        Path(id): Path<String>,
        axum::extract::State(state): axum::extract::State<AppState>,
    ) -> impl IntoResponse {
        let Ok(uid) = uuid::Uuid::parse_str(id.trim_end_matches(".csv")) else {
            return (StatusCode::BAD_REQUEST, "bad id").into_response();
        };
        let rows: Result<Vec<CsvRow>, _> = sqlx::query_as(
            "select category, modifier, text, search_volume, cpc from suggestions where search_id = $1
              order by category, modifier, search_volume desc nulls last, text",
        )
        .bind(uid)
        .fetch_all(&state.pool)
        .await;
        match rows {
            Ok(rows) => {
                let mut body = String::from("category,modifier,suggestion,search_volume,cpc\n");
                for (c, m, t, vol, cpc) in rows {
                    body.push_str(&format!(
                        "{c},{m},\"{}\",{},{}\n",
                        t.replace('"', "\"\""),
                        vol.map(|v| v.to_string()).unwrap_or_default(),
                        cpc.map(|v| format!("{v:.2}")).unwrap_or_default(),
                    ));
                }
                (
                    StatusCode::OK,
                    [
                        ("content-type", "text/csv; charset=utf-8"),
                        (
                            "content-disposition",
                            "attachment; filename=\"answers.csv\"",
                        ),
                    ],
                    body,
                )
                    .into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    }

    /// topic, status, ai_overview, ai_sources, questions, related, language, country
    type BriefExportRow = (
        String,
        String,
        Option<String>,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        String,
        String,
    );

    /// Brief as markdown or as a prompt, depending on the extension.
    async fn export_brief_md(
        Path(id): Path<String>,
        axum::extract::State(state): axum::extract::State<AppState>,
    ) -> impl IntoResponse {
        use atp::domain::{Brief, Competitor, Heading};

        let Ok(uid) = uuid::Uuid::parse_str(id.trim_end_matches(".md").trim_end_matches(".txt"))
        else {
            return (StatusCode::BAD_REQUEST, "bad id").into_response();
        };

        let row: Option<BriefExportRow> = match sqlx::query_as(
            "select topic, status, ai_overview, ai_sources, questions, related,
                    language, country
               from briefs where id = $1",
        )
        .bind(uid)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(r) => r,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };
        let Some((topic, _status, ai, sources, questions, related, language, country)) = row else {
            return (StatusCode::NOT_FOUND, "brief not found").into_response();
        };

        let comps: Vec<(i32, String, String, Option<String>, serde_json::Value, bool)> =
            sqlx::query_as(
                "select rank, url, domain, title, headings, parsed
                   from brief_competitors where brief_id = $1 order by rank",
            )
            .bind(uid)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

        let strings =
            |v: serde_json::Value| -> Vec<String> { serde_json::from_value(v).unwrap_or_default() };
        let brief = Brief {
            id: uid.to_string(),
            topic: topic.clone(),
            // The prompt asks for an article in this language, so it has to be
            // the brief's own rather than a default.
            language,
            country,
            status: String::new(),
            error: None,
            ai_overview: ai,
            ai_sources: strings(sources),
            questions: strings(questions),
            related: strings(related),
            created_at: String::new(),
            competitors: comps
                .into_iter()
                .map(|c| Competitor {
                    rank: c.0,
                    url: c.1,
                    domain: c.2,
                    title: c.3,
                    description: None,
                    headings: serde_json::from_value::<Vec<Heading>>(c.4).unwrap_or_default(),
                    parsed: c.5,
                    content: None,
                    domain_rank: None,
                })
                .collect(),
        };

        // `.txt` yields a prompt ready to paste into a chat model; `.md` the
        // raw brief. Static header strings, since this runs per request.
        let want_prompt = id.ends_with(".txt");
        let body = if want_prompt {
            atp::domain::brief_prompt(&brief)
        } else {
            atp::domain::brief_markdown(&brief)
        };
        let (content_type, disposition) = if want_prompt {
            (
                "text/plain; charset=utf-8",
                "attachment; filename=\"prompt.txt\"",
            )
        } else {
            (
                "text/markdown; charset=utf-8",
                "attachment; filename=\"brief.md\"",
            )
        };
        (
            StatusCode::OK,
            [
                ("content-type", content_type),
                ("content-disposition", disposition),
            ],
            body,
        )
            .into_response()
    }

    /// A finished draft as markdown.
    async fn export_draft(
        Path(id): Path<String>,
        axum::extract::State(state): axum::extract::State<AppState>,
    ) -> impl IntoResponse {
        let Ok(uid) = uuid::Uuid::parse_str(id.trim_end_matches(".md")) else {
            return (StatusCode::BAD_REQUEST, "bad id").into_response();
        };
        let row: Option<(Option<String>, String)> =
            sqlx::query_as("select content, kind from drafts where id = $1")
                .bind(uid)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
        match row {
            Some((Some(content), kind)) => {
                // Someone exporting both kinds should not end up with two files
                // called draft.md, one silently overwriting the other.
                let disposition = if kind == "faq" {
                    "attachment; filename=\"faq.md\""
                } else {
                    "attachment; filename=\"draft.md\""
                };
                (
                    StatusCode::OK,
                    [
                        ("content-type", "text/markdown; charset=utf-8"),
                        ("content-disposition", disposition),
                    ],
                    content,
                )
                    .into_response()
            }
            _ => (StatusCode::NOT_FOUND, "draft not found").into_response(),
        }
    }

    /// FAQPage JSON-LD for a finished FAQ draft, as a script tag to paste.
    async fn export_schema(
        Path(id): Path<String>,
        axum::extract::State(state): axum::extract::State<AppState>,
    ) -> impl IntoResponse {
        let Ok(uid) = uuid::Uuid::parse_str(id.trim_end_matches(".html")) else {
            return (StatusCode::BAD_REQUEST, "bad id").into_response();
        };
        let row: Option<(Option<String>, String)> =
            sqlx::query_as("select content, kind from drafts where id = $1")
                .bind(uid)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
        match row {
            Some((Some(content), kind)) if kind == "faq" => {
                let faqs = atp::aeo::schema::parse(&content);
                if faqs.is_empty() {
                    return (
                        StatusCode::NOT_FOUND,
                        "no question/answer pairs in this draft",
                    )
                        .into_response();
                }
                (
                    StatusCode::OK,
                    [
                        ("content-type", "text/html; charset=utf-8"),
                        (
                            "content-disposition",
                            "attachment; filename=\"faq-schema.html\"",
                        ),
                    ],
                    atp::aeo::schema::script_tag(&faqs),
                )
                    .into_response()
            }
            Some(_) => (
                StatusCode::BAD_REQUEST,
                "schema is only built from FAQ drafts",
            )
                .into_response(),
            None => (StatusCode::NOT_FOUND, "draft not found").into_response(),
        }
    }

    let csv_router = Router::new()
        .route("/export/schema/{id}", get(export_schema))
        .route("/export/brief/{id}", get(export_brief_md))
        .route("/export/draft/{id}", get(export_draft))
        .route("/export/{id}", get(export_csv))
        .with_state(state.clone());

    let app = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let state = state.clone();
                move || provide_context(state.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .merge(csv_router)
        .fallback(leptos_axum::file_and_error_handler_with_context(
            {
                let state = state.clone();
                move || provide_context(state.clone())
            },
            shell,
        ))
        .with_state(leptos_options);

    // Watched topics re-run themselves; see `watch.rs`. Spawned before the
    // worker takes ownership of the storage handle.
    tokio::spawn(atp::watch::run(pool.clone(), storage.clone()));

    let providers = Providers::from_env();
    let worker = WorkerBuilder::new("harvester")
        .data(pool.clone())
        .data(providers)
        .enable_tracing()
        .backend(storage)
        .build_fn(harvest);

    // Briefs are slower and rate-sensitive, so they get their own worker rather
    // than competing with keyword harvesting for the same slots.
    let brief_worker = WorkerBuilder::new("brief-builder")
        .data(pool.clone())
        .data(Sources::from_env())
        .enable_tracing()
        .backend(brief_storage)
        .build_fn(build_brief);

    // Writing is slow and paid, so it gets its own worker.
    let draft_worker = WorkerBuilder::new("draft-writer")
        .data(pool.clone())
        .data(atp::writer::Writers::from_env())
        .enable_tracing()
        .backend(draft_storage)
        .build_fn(write_draft);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{addr}");

    let http = async {
        axum::serve(listener, app.into_make_service())
            .await
            .map_err(anyhow::Error::from)
    };
    let monitor = async {
        Monitor::new()
            .register(worker)
            .register(brief_worker)
            .register(draft_worker)
            .run_with_signal(async {
                tokio::signal::ctrl_c().await?;
                tracing::info!("shutting down");
                Ok(())
            })
            .await
            .map_err(anyhow::Error::from)
    };

    tokio::try_join!(http, monitor)?;
    Ok(())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
