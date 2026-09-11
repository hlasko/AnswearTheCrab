//! Re-runs watched searches on their schedule.
//!
//! A plain loop rather than a cron crate: the schedule is "every N days per
//! watch", the check is one SQL query, and waking once an hour is more than
//! enough resolution for something measured in days. Adding a scheduler
//! dependency for that would be more code to trust than this.
//!
//! Each due watch becomes an ordinary harvest job, so the result is a normal
//! search with a normal comparison against its predecessor. Nothing here knows
//! how to search; it only decides when.

use apalis::prelude::Storage;
use apalis_sql::postgres::PostgresStorage;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::jobs::HarvestJob;

/// How often the loop looks for due watches.
const TICK: Duration = Duration::from_secs(60 * 60);

pub async fn run(pool: PgPool, mut storage: PostgresStorage<HarvestJob>) {
    // A short first delay so a freshly started app does not fire every watch
    // at once before anything else has settled.
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        if let Err(e) = tick(&pool, &mut storage).await {
            tracing::warn!("watch scheduler: {e}");
        }
        if let Err(e) = ask_tick(&pool).await {
            tracing::warn!("watch scheduler (answer engine): {e}");
        }
        if let Err(e) = crate::pages::tick(&pool).await {
            tracing::warn!("watch scheduler (tracked pages): {e}");
        }
        tokio::time::sleep(TICK).await;
    }
}

/// Queues a run for every enabled watch whose interval has elapsed.
pub async fn tick(
    pool: &PgPool,
    storage: &mut PostgresStorage<HarvestJob>,
) -> anyhow::Result<usize> {
    type Row = (Uuid, String, String, String, String);
    let due: Vec<Row> = sqlx::query_as(
        "select id, keyword, language, country, source
           from watches
          where enabled
            and (last_run_at is null
                 or last_run_at + make_interval(days => every_days) <= now())",
    )
    .fetch_all(pool)
    .await?;

    let mut queued = 0;
    for (id, keyword, language, country, source) in due {
        let search_id: Uuid = sqlx::query_scalar(
            "insert into searches (keyword, language, country, source)
             values ($1, $2, $3, $4) returning id",
        )
        .bind(&keyword)
        .bind(&language)
        .bind(&country)
        .bind(&source)
        .fetch_one(pool)
        .await?;

        storage
            .push(HarvestJob {
                search_id,
                keyword: keyword.clone(),
                language,
                country,
                source,
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Stamped on queueing, not on completion: a run that fails should
        // not be retried every hour until it succeeds, it should wait its
        // normal interval like any other.
        sqlx::query("update watches set last_run_at = now() where id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        tracing::info!("watch: queued `{keyword}` ({search_id})");
        queued += 1;
    }
    Ok(queued)
}

/// Re-asks the answer engine for watches that want it.
///
/// Separate from `tick` because it does not need a fresh harvest: the
/// questions come from the topic's last finished run, and the point is the
/// series, the same questions asked week after week. Runs on its own
/// interval so a topic whose suggestions are watched daily does not pay
/// $0.13 a day for answers.
pub async fn ask_tick(pool: &PgPool) -> anyhow::Result<usize> {
    let Some(provider) = crate::providers::dataforseo_from_env() else {
        return Ok(0);
    };
    let domain: String = sqlx::query_scalar("select value from settings where key = 'domain'")
        .fetch_optional(pool)
        .await?
        .unwrap_or_default();

    type Row = (Uuid, String, String, String, String);
    let due: Vec<Row> = sqlx::query_as(
        "select id, keyword, language, country, source
           from watches
          where enabled and ask_ai
            and (ai_last_run_at is null
                 or ai_last_run_at + make_interval(days => every_days) <= now())",
    )
    .fetch_all(pool)
    .await?;

    let mut done = 0;
    for (id, keyword, language, country, source) in due {
        // The newest finished run of the same topic: its questions are the
        // ones to ask, and the run the answers hang off.
        let search_id: Option<Uuid> = sqlx::query_scalar(
            "select id from searches
              where lower(keyword) = lower($1) and language = $2 and country = $3
                and source = $4 and status = 'done'
              order by created_at desc limit 1",
        )
        .bind(&keyword)
        .bind(&language)
        .bind(&country)
        .bind(&source)
        .fetch_optional(pool)
        .await?;
        let Some(search_id) = search_id else {
            continue;
        };

        let questions = crate::ai_answers::questions_for(pool, search_id).await?;
        if questions.is_empty() {
            continue;
        }
        match crate::ai_answers::ask_and_store(pool, &provider, search_id, questions, &domain).await
        {
            Ok(run) => {
                tracing::info!(
                    "watch: asked {} questions for `{keyword}`, cited in {}",
                    run.answers.len(),
                    run.hits()
                );
                done += 1;
            }
            // One topic failing must not stop the rest; the stamp is still
            // set so a persistently failing topic is not retried hourly.
            Err(e) => tracing::warn!("watch: answer engine for `{keyword}` failed: {e}"),
        }
        sqlx::query("update watches set ai_last_run_at = now() where id = $1")
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(done)
}

#[cfg(test)]
mod realdata {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a database; run with --ignored"]
    async fn tick_queues_due_watches() {
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".into());
        let pool = PgPool::connect(&url).await.unwrap();
        // The app's harvester listens on a named queue; a job pushed to the
        // default one sits Pending forever, which is how this test first went.
        let mut storage = PostgresStorage::new_with_config(
            pool.clone(),
            apalis_sql::Config::new(crate::jobs::QUEUE),
        );
        let n = tick(&pool, &mut storage).await.unwrap();
        println!("queued: {n}");
    }
}
