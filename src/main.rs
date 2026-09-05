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

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/atp".to_string());
    let pool = PgPool::connect(&database_url).await?;

    // apalis owns `_sqlx_migrations`, so run app schema separately and idempotently.
    PostgresStorage::setup(&pool).await?;
    sqlx::raw_sql(include_str!("../migrations/0001_init.sql"))
        .execute(&pool)
        .await?;

    let mut storage: PostgresStorage<HarvestJob> =
        PostgresStorage::new_with_config(pool.clone(), Config::new(QUEUE));

    // Push notifications so queued jobs start immediately.
    let mut listener = PgListen::new(pool.clone()).await?;
    listener.subscribe_with(&mut storage);
    tokio::spawn(async move {
        if let Err(e) = listener.listen().await {
            tracing::error!("pg listener stopped: {e}");
        }
    });

    let conf = get_configuration(None)?;
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let state = AppState {
        pool: pool.clone(),
        storage: storage.clone(),
    };

    // CSV export of a finished search.
    async fn export_csv(
        Path(id): Path<String>,
        axum::extract::State(state): axum::extract::State<AppState>,
    ) -> impl IntoResponse {
        let Ok(uid) = uuid::Uuid::parse_str(id.trim_end_matches(".csv")) else {
            return (StatusCode::BAD_REQUEST, "bad id").into_response();
        };
        let rows: Result<Vec<(String, String, String, Option<i64>, Option<f64>)>, _> = sqlx::query_as(
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
                        ("content-disposition", "attachment; filename=\"answers.csv\""),
                    ],
                    body,
                )
                    .into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    }

    let csv_router = Router::new()
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

    let providers = Providers::from_env();
    let worker = WorkerBuilder::new("harvester")
        .data(pool.clone())
        .data(providers)
        .enable_tracing()
        .backend(storage)
        .build_fn(harvest);

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
