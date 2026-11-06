#![cfg(feature = "ssr")]
//! Opt-in PostgreSQL integration tests. All relations are session-local temporary
//! tables, and the answer provider is a loopback-only HTTP fixture. No paid calls.
use atp::{
    ai_answers,
    prompt_discovery::{self, Candidate, Provenance},
    providers::dataforseo::DataForSeo,
    watch,
};
use serde_json::{json, Value};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use std::{
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;

async fn search(
    pool: &PgPool,
    keyword: &str,
    language: &str,
    country: &str,
    source: &str,
) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar("insert into searches (keyword,language,country,source,status) values ($1,$2,$3,$4,'done') returning id")
        .bind(keyword).bind(language).bind(country).bind(source).fetch_one(pool).await?)
}

async fn database() -> anyhow::Result<PgPool> {
    let url = std::env::var("PROMPT_TEST_DATABASE_URL")?;
    let options = PgConnectOptions::from_str(&url)?;
    anyhow::ensure!(
        matches!(options.get_host(), "localhost" | "127.0.0.1" | "::1")
            || options.get_host().starts_with('/'),
        "explicit local test database required"
    );
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    for migration in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0008_watches.sql"),
        include_str!("../migrations/0009_settings.sql"),
        include_str!("../migrations/0017_ai_answers.sql"),
        include_str!("../migrations/0021_prompt_selections.sql"),
        // Startup migrations must be idempotent.
        include_str!("../migrations/0021_prompt_selections.sql"),
    ] {
        sqlx::raw_sql(&migration.replace(
            "create table if not exists",
            "create temporary table if not exists",
        ))
        .execute(&pool)
        .await?;
    }
    Ok(pool)
}

fn snapshot(search_id: Uuid, text: &str) -> Vec<Candidate> {
    vec![Candidate {
        text: text.into(),
        intent: "How-to".into(),
        provenance: vec![Provenance {
            search_id: search_id.to_string(),
            provider: "fixture".into(),
            source: "google".into(),
            collected_at: "2026-11-06".into(),
            original: text.into(),
            category: "questions".into(),
            modifier: "how".into(),
            google_volume: Some(12),
            bing_volume: None,
            asked_signal: Some(3),
        }],
    }]
}

struct Fixture {
    provider: DataForSeo,
    calls: Arc<Mutex<Vec<String>>>,
    pause: Arc<AtomicBool>,
    entered: Arc<Semaphore>,
    release: Arc<Semaphore>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn fixture() -> anyhow::Result<Fixture> {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let pause = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let (received, paused, started, resume) = (
        calls.clone(),
        pause.clone(),
        entered.clone(),
        release.clone(),
    );
    let app = axum::Router::new().route("/v3/ai_optimization/perplexity/llm_responses/live", axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
        let (received, paused, started, resume) = (received.clone(), paused.clone(), started.clone(), resume.clone());
        async move {
            let question = body[0]["user_prompt"].as_str().ok_or(axum::http::StatusCode::BAD_REQUEST)?.to_string();
            received.lock().await.push(question.clone());
            if paused.load(Ordering::SeqCst) {
                started.add_permits(1);
                resume.acquire().await.map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?.forget();
            }
            if question.starts_with("Fail") {
                return Ok::<_, axum::http::StatusCode>(axum::Json(json!({"status_code":20000,"tasks":[{"status_message":"Fixture failure"}]})));
            }
            Ok(axum::Json(json!({"status_code":20000,"tasks":[{"status_message":"Ok.","result":[{"items":[{"sections":[{"text":"Local test answer", "annotations":[{"url":"https://example.test/page"}]}]}]}]}]})))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let provider = DataForSeo::new("fixture".into(), "fixture".into(), base)?;
    Ok(Fixture {
        provider,
        calls,
        pause,
        entered,
        release,
        server,
    })
}

#[tokio::test]
#[ignore = "set PROMPT_TEST_DATABASE_URL to a local PostgreSQL database"]
async fn approval_snapshot_survives_reruns_and_gates_paid_paths() -> anyhow::Result<()> {
    let pool = database().await?;
    let mock = fixture().await?;
    let first = search(&pool, "Topic", "en", "us", "google").await?;
    sqlx::query("insert into watches (keyword,language,country,source,ask_ai) values ('Topic','en','us','google',true)").execute(&pool).await?;
    assert!(ai_answers::questions_for(&pool, first).await?.is_empty());
    assert!(
        matches!(ai_answers::ask_and_store(&pool, &mock.provider, first, "").await, Err(e) if e.to_string().contains("Approve"))
    );
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &mock.provider).await?,
        0
    );
    assert!(
        mock.calls.lock().await.is_empty(),
        "legacy watches and on-demand runs must not call the provider"
    );
    let snapshot = snapshot(first, "How does Topic work?");
    assert!(prompt_discovery::save(&pool, first, &[]).await.is_err());
    prompt_discovery::save(&pool, first, &snapshot).await?;
    let stamp: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("select approved_at from prompt_selections")
            .fetch_one(&pool)
            .await?;
    prompt_discovery::save(&pool, first, &snapshot).await?;
    let same_stamp: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("select approved_at from prompt_selections")
            .fetch_one(&pool)
            .await?;
    assert_eq!(stamp, same_stamp, "identical approval is a no-op");
    let newest = search(&pool, "topic", "en", "us", "google").await?;
    sqlx::query("insert into suggestions (search_id,text,category,modifier,search_volume) values ($1,'A new popular question','questions','what',999999)").bind(newest).execute(&pool).await?;
    assert_eq!(prompt_discovery::approved(&pool, newest).await?, snapshot);
    for (lang, country, source) in [
        ("de", "us", "google"),
        ("en", "gb", "google"),
        ("en", "us", "bing"),
    ] {
        let other = search(&pool, "topic", lang, country, source).await?;
        assert!(prompt_discovery::approved(&pool, other).await?.is_empty());
    }
    assert_eq!(
        ai_answers::ask_and_store(&pool, &mock.provider, newest, "")
            .await?
            .answers[0]
            .question,
        snapshot[0].text
    );
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &mock.provider).await?,
        1
    );
    assert_eq!(
        *mock.calls.lock().await,
        vec![snapshot[0].text.clone(), snapshot[0].text.clone()]
    );
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &mock.provider).await?,
        0,
        "schedule stamp prevents immediate repeats"
    );
    let saved_runs: i64 = sqlx::query_scalar("select count(*) from ai_answer_runs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(saved_runs, 2);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "set PROMPT_TEST_DATABASE_URL to a local PostgreSQL database"]
async fn concurrent_ticks_claim_before_calls_without_holding_connection() -> anyhow::Result<()> {
    let pool = database().await?;
    let mock = fixture().await?;
    let id = search(&pool, "Topic", "en", "us", "google").await?;
    prompt_discovery::save(&pool, id, &snapshot(id, "How does Topic work?")).await?;
    sqlx::query("insert into watches (keyword,language,country,source,ask_ai) values ('Topic','en','us','google',true)").execute(&pool).await?;
    mock.pause.store(true, Ordering::SeqCst);
    let (first_pool, provider) = (pool.clone(), mock.provider.clone());
    let mut first =
        tokio::spawn(async move { watch::ask_tick_with_provider(&first_pool, &provider).await });
    let (second_pool, provider) = (pool.clone(), mock.provider.clone());
    let mut second =
        tokio::spawn(async move { watch::ask_tick_with_provider(&second_pool, &provider).await });
    // Start both ticks before awaiting either so their due reads can interleave.
    // Hold the winner's paid request open while the loser finishes. A
    // one-connection pool also proves no connection or transaction is held.
    tokio::time::timeout(Duration::from_secs(5), mock.entered.acquire())
        .await??
        .forget();
    let winner = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            result = &mut first => {
                assert_eq!(result??, 0);
                Ok::<_, anyhow::Error>(second)
            }
            result = &mut second => {
                assert_eq!(result??, 0);
                Ok::<_, anyhow::Error>(first)
            }
        }
    })
    .await??;
    assert_eq!(mock.calls.lock().await.len(), 1);
    mock.release.add_permits(1);
    assert_eq!(winner.await??, 1);
    mock.pause.store(false, Ordering::SeqCst);
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &mock.provider).await?,
        0
    );
    // Broken legacy intervals must not defeat the claim stamp.
    sqlx::query("update watches set every_days = 0, ai_last_run_at = null")
        .execute(&pool)
        .await?;
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &mock.provider).await?,
        0
    );
    assert_eq!(mock.calls.lock().await.len(), 1);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "set PROMPT_TEST_DATABASE_URL to a local PostgreSQL database"]
async fn failed_answers_and_database_inserts_do_not_leave_partial_runs() -> anyhow::Result<()> {
    let pool = database().await?;
    let mock = fixture().await?;
    let id = search(&pool, "Topic", "en", "us", "google").await?;
    prompt_discovery::save(&pool, id, &snapshot(id, "Fail this answer")).await?;
    assert!(
        matches!(ai_answers::ask_and_store(&pool, &mock.provider, id, "").await, Err(e) if e.to_string().contains("every question failed"))
    );
    let counts: (i64, i64) = sqlx::query_as(
        "select (select count(*) from ai_answer_runs), (select count(*) from ai_answers)",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(counts, (0, 0));
    sqlx::query("insert into watches (keyword,language,country,source,ask_ai) values ('Topic','en','us','google',true)")
        .execute(&pool).await?;
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &mock.provider).await?,
        0
    );
    let calls_after_failure = mock.calls.lock().await.len();
    assert_eq!(calls_after_failure, 2);
    assert_eq!(
        watch::ask_tick_with_provider(&pool, &mock.provider).await?,
        0
    );
    assert_eq!(
        mock.calls.lock().await.len(),
        calls_after_failure,
        "failed paid attempts must retain their claim and not retry immediately"
    );
    let mut partial = snapshot(id, "How does Topic work?");
    partial.extend(snapshot(id, "Fail this answer"));
    prompt_discovery::save(&pool, id, &partial).await?;
    let run = ai_answers::ask_and_store(&pool, &mock.provider, id, "example.test").await?;
    assert_eq!(
        run.answers.len(),
        1,
        "only actual successful answers are returned"
    );
    assert!(run.answers[0].cited);
    let counts: (i64, i64) = sqlx::query_as(
        "select (select count(*) from ai_answer_runs), (select count(*) from ai_answers)",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(counts, (1, 1));
    // Fail on the second answer insert, after a header and an answer have been
    // inserted. The sequence is intentionally nontransactional for observation.
    sqlx::raw_sql("create temporary sequence answer_insert_attempts;
        alter table ai_answers add constraint fixture_failure check (nextval('answer_insert_attempts') < 2) not valid")
        .execute(&pool).await?;
    let mut both = snapshot(id, "How does another topic work?");
    both.extend(snapshot(id, "How does a third topic work?"));
    prompt_discovery::save(&pool, id, &both).await?;
    assert!(ai_answers::ask_and_store(&pool, &mock.provider, id, "")
        .await
        .is_err());
    let attempts: i64 = sqlx::query_scalar("select last_value from answer_insert_attempts")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        attempts, 2,
        "failure must occur after the first answer insert"
    );
    let counts: (i64, i64) = sqlx::query_as(
        "select (select count(*) from ai_answer_runs), (select count(*) from ai_answers)",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(counts, (1, 1), "all writes for a failed run must roll back");
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "set PROMPT_TEST_DATABASE_URL to a local PostgreSQL database"]
async fn corrupt_approved_snapshots_never_reach_paid_provider() -> anyhow::Result<()> {
    let pool = database().await?;
    let mock = fixture().await?;
    let id = search(&pool, "Topic", "en", "us", "google").await?;
    sqlx::query("insert into watches (keyword,language,country,source,ask_ai) values ('Topic','en','us','google',true)").execute(&pool).await?;
    prompt_discovery::save(&pool, id, &snapshot(id, "How does Topic work?")).await?;
    let mut blank = serde_json::to_value(snapshot(id, "How does Topic work?"))?;
    blank[0]["text"] = json!("   ");
    for corrupt in [json!([{"text":17}]), blank] {
        sqlx::query("update prompt_selections set prompts = $1")
            .bind(corrupt)
            .execute(&pool)
            .await?;
        assert!(ai_answers::ask_and_store(&pool, &mock.provider, id, "")
            .await
            .is_err());
        assert!(watch::ask_tick_with_provider(&pool, &mock.provider)
            .await
            .is_err());
        assert!(mock.calls.lock().await.is_empty());
    }
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "set PROMPT_TEST_DATABASE_URL to a local PostgreSQL database"]
async fn corrupt_intervals_never_claim_or_call_provider() -> anyhow::Result<()> {
    let pool = database().await?;
    let mock = fixture().await?;
    let search_id = search(&pool, "Topic", "en", "us", "google").await?;
    prompt_discovery::save(
        &pool,
        search_id,
        &snapshot(search_id, "How does Topic work?"),
    )
    .await?;
    let watch_id: Uuid = sqlx::query_scalar(
        "insert into watches (keyword,language,country,source,ask_ai)
         values ('Topic','en','us','google',true) returning id",
    )
    .fetch_one(&pool)
    .await?;
    for days in [0, -1, i32::MIN, i32::MAX, 3651] {
        for has_stamp in [false, true] {
            // A non-null timestamp forces the arithmetic path too. A guard
            // alone is not enough if the optimizer evaluates it after addition.
            sqlx::query(
                "update watches set every_days = $1,
                 last_run_at = case when $2 then now() - interval '4000 days' else null end,
                 ai_last_run_at = case when $2 then now() - interval '4000 days' else null end",
            )
            .bind(days)
            .bind(has_stamp)
            .execute(&pool)
            .await?;
            let before: (
                Option<chrono::DateTime<chrono::Utc>>,
                Option<chrono::DateTime<chrono::Utc>>,
            ) = sqlx::query_as("select last_run_at, ai_last_run_at from watches")
                .fetch_one(&pool)
                .await?;
            assert!(
                !watch::claim_due_watch(&pool, watch_id).await?,
                "invalid interval {days}"
            );
            assert_eq!(
                watch::ask_tick_with_provider(&pool, &mock.provider).await?,
                0,
                "invalid interval {days}"
            );
            let after: (
                Option<chrono::DateTime<chrono::Utc>>,
                Option<chrono::DateTime<chrono::Utc>>,
            ) = sqlx::query_as("select last_run_at, ai_last_run_at from watches")
                .fetch_one(&pool)
                .await?;
            assert_eq!(before, after, "invalid intervals must not consume attempts");
        }
    }
    assert!(mock.calls.lock().await.is_empty());
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "set PROMPT_TEST_DATABASE_URL to a local PostgreSQL database"]
async fn concurrent_keyword_claims_have_one_winner_without_enqueueing() -> anyhow::Result<()> {
    let pool = database().await?;
    let id: Uuid = sqlx::query_scalar(
        "insert into watches (keyword,language,country,source)
         values ('Topic','en','us','google') returning id",
    )
    .fetch_one(&pool)
    .await?;
    // Test both persisted boundaries with stale timestamps. Calling the exact
    // production preclaim directly never enqueues or executes a paid harvest.
    for days in [1, 3650] {
        sqlx::query(
            "update watches set every_days = $1, last_run_at = now() - interval '4000 days'",
        )
        .bind(days)
        .execute(&pool)
        .await?;
        let (left, right) = tokio::join!(
            watch::claim_due_watch(&pool, id),
            watch::claim_due_watch(&pool, id),
        );
        assert_ne!(left?, right?, "exactly one concurrent claim must win");
        assert!(
            !watch::claim_due_watch(&pool, id).await?,
            "claim is retained even without enqueueing"
        );
    }
    sqlx::query("update watches set enabled = false, last_run_at = null")
        .execute(&pool)
        .await?;
    assert!(
        !watch::claim_due_watch(&pool, id).await?,
        "disabled watches cannot be claimed"
    );
    let searches: i64 = sqlx::query_scalar("select count(*) from searches")
        .fetch_one(&pool)
        .await?;
    assert_eq!(searches, 0);
    pool.close().await;
    Ok(())
}
