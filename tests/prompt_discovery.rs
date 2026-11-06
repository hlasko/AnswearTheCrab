#![cfg(feature = "ssr")]
//! Opt-in PostgreSQL integration test. All relations are session-local temporary
//! tables, and the answer provider is a loopback-only HTTP fixture. No paid calls.
use atp::{
    ai_answers,
    prompt_discovery::{self, Candidate, Provenance},
    providers::dataforseo::DataForSeo,
    watch,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

async fn search(pool: &PgPool, keyword: &str, language: &str, country: &str, source: &str) -> Uuid {
    sqlx::query_scalar("insert into searches (keyword,language,country,source,status) values ($1,$2,$3,$4,'done') returning id")
        .bind(keyword).bind(language).bind(country).bind(source).fetch_one(pool).await.unwrap()
}

#[tokio::test]
#[ignore = "set PROMPT_TEST_DATABASE_URL to a local PostgreSQL database"]
async fn approval_snapshot_survives_reruns_and_gates_paid_paths() {
    let url =
        std::env::var("PROMPT_TEST_DATABASE_URL").expect("explicit local test database required");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    for migration in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0008_watches.sql"),
        include_str!("../migrations/0009_settings.sql"),
        include_str!("../migrations/0017_ai_answers.sql"),
        include_str!("../migrations/0021_prompt_selections.sql"),
    ] {
        sqlx::raw_sql(&migration.replace(
            "create table if not exists",
            "create temporary table if not exists",
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    // The new migration is safe to rerun during every startup.
    sqlx::raw_sql(
        &include_str!("../migrations/0021_prompt_selections.sql").replace(
            "create table if not exists",
            "create temporary table if not exists",
        ),
    )
    .execute(&pool)
    .await
    .unwrap();
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let received = calls.clone();
    let app = axum::Router::new().route("/v3/ai_optimization/perplexity/llm_responses/live", axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
        let received = received.clone();
        async move {
            received.lock().unwrap().push(body[0]["user_prompt"].as_str().unwrap().to_string());
            axum::Json(json!({"status_code":20000,"tasks":[{"status_message":"Ok.","result":[{"items":[{"sections":[{"text":"Local test answer", "annotations":[{"url":"https://example.test/page"}]}]}]}]}]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = DataForSeo::from_env("fixture".into(), "fixture".into(), base);
    let first = search(&pool, "Topic", "en", "us", "google").await;
    sqlx::query("insert into watches (keyword,language,country,source,ask_ai) values ('Topic','en','us','google',true)").execute(&pool).await.unwrap();
    assert!(ai_answers::questions_for(&pool, first)
        .await
        .unwrap()
        .is_empty());
    assert!(ai_answers::ask_and_store(&pool, &provider, first, "")
        .await
        .unwrap_err()
        .to_string()
        .contains("Approve"));
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &provider)
            .await
            .unwrap(),
        0
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "legacy watches and on-demand runs must not call the provider"
    );

    let snapshot = vec![Candidate {
        text: "How does Topic work?".into(),
        intent: "How-to".into(),
        provenance: vec![Provenance {
            search_id: first.to_string(),
            provider: "fixture".into(),
            source: "google".into(),
            collected_at: "2026-11-06".into(),
            original: "How does Topic work?".into(),
            category: "questions".into(),
            modifier: "how".into(),
            google_volume: Some(12),
            asked_signal: Some(3),
        }],
    }];
    assert!(prompt_discovery::save(&pool, first, &[]).await.is_err());
    prompt_discovery::save(&pool, first, &snapshot)
        .await
        .unwrap();
    let stamp: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("select approved_at from prompt_selections")
            .fetch_one(&pool)
            .await
            .unwrap();
    prompt_discovery::save(&pool, first, &snapshot)
        .await
        .unwrap();
    let same_stamp: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("select approved_at from prompt_selections")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stamp, same_stamp, "identical approval is a no-op");
    let newest = search(&pool, "topic", "en", "us", "google").await;
    sqlx::query("insert into suggestions (search_id,text,category,modifier,search_volume) values ($1,'A new popular question','questions','what',999999)").bind(newest).execute(&pool).await.unwrap();
    assert_eq!(
        prompt_discovery::approved(&pool, newest).await.unwrap(),
        snapshot
    );
    for (lang, country, source) in [
        ("de", "us", "google"),
        ("en", "gb", "google"),
        ("en", "us", "bing"),
    ] {
        let other = search(&pool, "topic", lang, country, source).await;
        assert!(prompt_discovery::approved(&pool, other)
            .await
            .unwrap()
            .is_empty());
    }
    assert_eq!(
        ai_answers::ask_and_store(&pool, &provider, newest, "")
            .await
            .unwrap()
            .answers[0]
            .question,
        snapshot[0].text
    );
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &provider)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec![snapshot[0].text.clone(), snapshot[0].text.clone()]
    );
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &provider)
            .await
            .unwrap(),
        0,
        "schedule stamp prevents immediate repeats"
    );
    let saved_runs: i64 = sqlx::query_scalar("select count(*) from ai_answer_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(saved_runs, 2);
    server.abort();
    pool.close().await;
}
