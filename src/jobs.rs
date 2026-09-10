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
    /// Which search box to harvest. Defaults to Google so jobs already queued
    /// before multi-source support still deserialise.
    #[serde(default)]
    pub source: String,
}

pub const QUEUE: &str = "atp::harvest";

pub async fn harvest(
    job: HarvestJob,
    pool: Data<PgPool>,
    providers: Data<Providers>,
) -> Result<(), Error> {
    let pool: &PgPool = &pool;
    let source = crate::domain::Source::parse(&job.source);
    tracing::info!(
        "harvesting `{}` from {} ({})",
        job.keyword,
        source.label(),
        job.search_id
    );

    sqlx::query("update searches set status = 'running', started_at = now() where id = $1")
        .bind(job.search_id)
        .execute(pool)
        .await
        .map_err(to_err)?;

    match providers
        .harvest(source, &job.keyword, &job.language, &job.country)
        .await
    {
        Ok(mut items) => {
            // Bing's count, where Bing publishes one. On a Google run it rides
            // along as the second number. On a Bing run it IS the number: the
            // first version fetched it for Google runs only, and a 926-phrase
            // Bing run came back with no volume at all, which was the one
            // place the Bing count mattered most. Failure must not fail the
            // harvest: the run is worth storing without it.
            let wants_bing = matches!(
                source,
                crate::domain::Source::Google | crate::domain::Source::Bing
            );
            if wants_bing && crate::domain::bing_volume_available(&job.language, &job.country) {
                if let Some(dfs) = crate::providers::dataforseo_from_env() {
                    let texts: Vec<String> = items.iter().map(|s| s.text.clone()).collect();
                    match dfs.bing_volume(&texts, &job.language, &job.country).await {
                        Ok(map) => {
                            for s in items.iter_mut() {
                                let b = map.get(&s.text.to_lowercase()).copied();
                                s.bing_volume = b;
                                // A Bing run's phrases have no Google number,
                                // so Bing's fills the volume slot the wheels,
                                // lists and topics are drawn from.
                                if source == crate::domain::Source::Bing {
                                    s.search_volume = b;
                                }
                            }
                            tracing::info!(
                                "bing volume for {} of {} phrases",
                                map.len(),
                                items.len()
                            );
                        }
                        Err(e) => tracing::warn!("bing volume skipped: {e}"),
                    }
                }
            }
            // Perplexity phrases have no volume of their own (nobody publishes
            // one), so Google's is attached where the phrase exists there:
            // enough to colour the wheels and rank the lists, and honest,
            // since it is labelled as Google's in the UI. Google Ads takes
            // ~30 s live and refuses a batch with a restricted term; failure
            // leaves the run grey rather than failing it.
            if source == crate::domain::Source::Perplexity {
                if let Some(dfs) = crate::providers::dataforseo_from_env() {
                    let texts: Vec<String> = items.iter().map(|s| s.text.clone()).collect();
                    match dfs.google_volume(&texts, &job.language, &job.country).await {
                        Ok(map) => {
                            let mut n = 0;
                            for s in items.iter_mut() {
                                if let Some(v) = map.get(&s.text.to_lowercase()) {
                                    s.search_volume = Some(*v);
                                    n += 1;
                                }
                            }
                            tracing::info!(
                                "google volume for {n} of {} perplexity phrases",
                                items.len()
                            );
                        }
                        Err(e) => tracing::warn!("google volume for perplexity skipped: {e}"),
                    }
                }
            }

            // The AI-question figure, for every market: $0.01 a run. Failure
            // must not fail the harvest.
            if let Some(dfs) = crate::providers::dataforseo_from_env() {
                let texts: Vec<String> = items.iter().map(|s| s.text.clone()).collect();
                match dfs.ai_volume(&texts, &job.language, &job.country).await {
                    Ok(map) => {
                        let mut n = 0;
                        for s in items.iter_mut() {
                            // A reordering of the seed gets the seed's whole
                            // figure (all words present, any order) and would
                            // top every list. Leave it unknown.
                            if s.is_permutation_of(&job.keyword) {
                                continue;
                            }
                            if let Some((cur, months)) = map.get(&s.text.to_lowercase()) {
                                s.ai_volume = Some(*cur);
                                s.ai_monthly = months.clone();
                                n += 1;
                            }
                        }
                        tracing::info!("ai volume for {n} of {} phrases", items.len());
                    }
                    Err(e) => tracing::warn!("ai volume skipped: {e}"),
                }
            }
            let mut tx = pool.begin().await.map_err(to_err)?;
            sqlx::query("delete from suggestions where search_id = $1")
                .bind(job.search_id)
                .execute(&mut *tx)
                .await
                .map_err(to_err)?;

            for s in &items {
                sqlx::query(
                    "insert into suggestions (search_id, text, category, modifier, search_volume, cpc,
                                              competition, monthly, trend_yearly, trend_quarterly,
                                              bing_volume, ai_volume, ai_monthly)
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                     on conflict (search_id, text) do nothing",
                )
                .bind(job.search_id)
                .bind(&s.text)
                .bind(&s.category)
                .bind(&s.modifier)
                .bind(s.search_volume)
                .bind(s.cpc)
                .bind(s.competition)
                // Null rather than an empty array when the provider gave no
                // history, so "unknown" and "twelve zeros" stay distinct.
                .bind(if s.monthly.is_empty() {
                    None
                } else {
                    Some(serde_json::to_value(&s.monthly).unwrap_or_default())
                })
                .bind(s.trend_yearly)
                .bind(s.trend_quarterly)
                .bind(s.bing_volume)
                .bind(s.ai_volume)
                .bind(if s.ai_monthly.is_empty() {
                    None
                } else {
                    Some(serde_json::to_value(&s.ai_monthly).unwrap_or_default())
                })
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
            .bind(providers.name(source))
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
