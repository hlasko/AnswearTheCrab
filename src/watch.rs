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
          where enabled and every_days between 1 and 3650
            and (last_run_at is null
                 or last_run_at + make_interval(days =>
                     case when every_days between 1 and 3650 then every_days else 1 end) <= now())",
    )
    .fetch_all(pool)
    .await?;

    let mut queued = 0;
    for (id, keyword, language, country, source) in due {
        if !claim_due_watch(pool, id).await? {
            continue;
        }
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

        tracing::info!("watch: queued `{keyword}` ({search_id})");
        queued += 1;
    }
    Ok(queued)
}

/// Atomically claims a keyword-harvest attempt before creating or enqueueing it.
/// A crash or enqueue failure consumes this interval. This deliberately trades
/// retries for at-most-once scheduling attempts, not exactly-once job delivery.
/// No connection or transaction is retained after the claim.
pub async fn claim_due_watch(pool: &PgPool, id: Uuid) -> anyhow::Result<bool> {
    // SQL may reorder AND predicates: the CASE also bounds the arithmetic
    // itself, even when corrupt persisted intervals are outside the guard.
    Ok(sqlx::query(
        "update watches set last_run_at = now()
          where id = $1 and enabled and every_days between 1 and 3650
            and (last_run_at is null
                 or last_run_at + make_interval(days =>
                     case when every_days between 1 and 3650 then every_days else 1 end) <= now())",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

/// Re-asks the answer engine for watches that want it.
///
/// Separate from `tick` because it does not need a fresh harvest: the
/// questions come from the topic's last finished run, and the point is the
/// series, the same questions asked week after week. Runs on its own
/// interval so a topic whose suggestions are watched daily does not pay
/// $0.13 a day for answers.
pub async fn ask_tick(pool: &PgPool) -> anyhow::Result<usize> {
    let Some(provider) = crate::providers::dataforseo_from_env()? else {
        return Ok(0);
    };
    ask_tick_with_provider(pool, &provider).await
}

/// Dependency-injected entry point, also used by local-only integration tests.
pub async fn ask_tick_with_provider(
    pool: &PgPool,
    provider: &crate::providers::dataforseo::DataForSeo,
) -> anyhow::Result<usize> {
    let domain: String = sqlx::query_scalar("select value from settings where key = 'domain'")
        .fetch_optional(pool)
        .await?
        .unwrap_or_default();

    type Row = (Uuid, String, String, String, String);
    let due: Vec<Row> = sqlx::query_as(
        "select id, keyword, language, country, source
           from watches
          where enabled and ask_ai and every_days between 1 and 3650
            and (ai_last_run_at is null
                 or ai_last_run_at + make_interval(days =>
                     case when every_days between 1 and 3650 then every_days else 1 end) <= now())",
    )
    .fetch_all(pool)
    .await?;

    let mut done = 0;
    for (id, keyword, language, country, source) in due {
        // Attach answers to the latest search, but only ask the stored approved
        // snapshot. Legacy schedules without approval cannot incur calls.
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
        // Claim the attempt, not its eventual success. PostgreSQL rechecks this
        // predicate after a concurrent updater releases the row lock. No pool
        // connection or transaction is kept while the provider is running.
        // Guard persisted intervals and CASE-bound the arithmetic regardless
        // of the SQL optimizer's predicate evaluation order.
        let claimed = sqlx::query(
            "update watches w set ai_last_run_at = now()
              where w.id = $1 and w.enabled and w.ask_ai and w.every_days between 1 and 3650
                and (w.ai_last_run_at is null
                     or w.ai_last_run_at + make_interval(days =>
                         case when w.every_days between 1 and 3650 then w.every_days else 1 end) <= now())
                and exists (
                    select 1 from prompt_selections p
                    join searches s on lower(s.keyword) = p.topic
                        and s.language = p.language and s.country = p.country
                        and s.source = p.source
                    where s.id = $2 and s.status = 'done'
                        and lower(w.keyword) = p.topic and w.language = p.language
                        and w.country = p.country and w.source = p.source
                        and jsonb_typeof(p.prompts) = 'array'
                        and p.prompts <> '[]'::jsonb
                )",
        )
        .bind(id)
        .bind(search_id)
        .execute(pool)
        .await?
        .rows_affected();
        if claimed == 0 {
            continue;
        }
        // ask_and_store validates approval again. Revocation after that read
        // cannot cancel an already-started request. A crash consumes this
        // interval, deliberately preferring a missed run to duplicate billing.
        match crate::ai_answers::ask_and_store(pool, provider, search_id, &domain).await {
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
    }
    Ok(done)
}

#[cfg(test)]
mod realdata {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a database; run with --ignored"]
    async fn tick_queues_due_watches() -> anyhow::Result<()> {
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".into());
        let pool = PgPool::connect(&url).await?;
        // The app's harvester listens on a named queue; a job pushed to the
        // default one sits Pending forever, which is how this test first went.
        let mut storage = PostgresStorage::new_with_config(
            pool.clone(),
            apalis_sql::Config::new(crate::jobs::QUEUE),
        );
        let n = tick(&pool, &mut storage).await?;
        println!("queued: {n}");
        Ok(())
    }
}
