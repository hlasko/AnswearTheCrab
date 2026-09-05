//! Background job: harvest suggestions for a search row.

use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::providers::Providers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestJob {
    pub search_id: Uuid,
    pub keyword: String,
    pub language: String,
    pub country: String,
}

pub const QUEUE: &str = "atp::harvest";

pub async fn harvest(
    job: HarvestJob,
    pool: Data<PgPool>,
    providers: Data<Providers>,
) -> Result<(), Error> {
    let pool: &PgPool = &pool;
    tracing::info!("harvesting `{}` ({})", job.keyword, job.search_id);

    sqlx::query("update searches set status = 'running', started_at = now() where id = $1")
        .bind(job.search_id)
        .execute(pool)
        .await
        .map_err(to_err)?;

    match providers
        .harvest(&job.keyword, &job.language, &job.country)
        .await
    {
        Ok(items) => {
            let mut tx = pool.begin().await.map_err(to_err)?;
            sqlx::query("delete from suggestions where search_id = $1")
                .bind(job.search_id)
                .execute(&mut *tx)
                .await
                .map_err(to_err)?;

            for s in &items {
                sqlx::query(
                    "insert into suggestions (search_id, text, category, modifier, search_volume, cpc, competition)
                     values ($1, $2, $3, $4, $5, $6, $7)
                     on conflict (search_id, text) do nothing",
                )
                .bind(job.search_id)
                .bind(&s.text)
                .bind(&s.category)
                .bind(&s.modifier)
                .bind(s.search_volume)
                .bind(s.cpc)
                .bind(s.competition)
                .execute(&mut *tx)
                .await
                .map_err(to_err)?;
            }

            sqlx::query(
                "update searches
                    set status = 'done',
                        provider = $2,
                        finished_at = now(),
                        error = null,
                        suggestion_count = (select count(*) from suggestions where search_id = $1)
                  where id = $1",
            )
            .bind(job.search_id)
            .bind(providers.name())
            .execute(&mut *tx)
            .await
            .map_err(to_err)?;

            tx.commit().await.map_err(to_err)?;
            tracing::info!("stored {} suggestions for {}", items.len(), job.search_id);
            Ok(())
        }
        Err(e) => {
            sqlx::query(
                "update searches set status = 'failed', error = $2, finished_at = now() where id = $1",
            )
            .bind(job.search_id)
            .bind(e.to_string())
            .execute(pool)
            .await
            .map_err(to_err)?;
            Err(Error::Failed(std::sync::Arc::new(Box::new(
                std::io::Error::other(e.to_string()),
            ))))
        }
    }
}

fn to_err<E: std::fmt::Display>(e: E) -> Error {
    Error::Failed(std::sync::Arc::new(Box::new(std::io::Error::other(
        e.to_string(),
    ))))
}
