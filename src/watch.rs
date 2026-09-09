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
