//! Background job: research one topic and store the resulting brief.

use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::research::{Findings, ResearchSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefJob {
    pub brief_id: Uuid,
    pub topic: String,
    pub language: String,
    pub country: String,
}

pub const QUEUE: &str = "atp::brief";

/// The sources a brief is built from.
///
/// A vector rather than a single source: this is where AEO/GEO will be added,
/// and [`Findings::merge`] already defines how contributions combine.
#[derive(Clone)]
pub struct Sources(pub std::sync::Arc<Vec<std::sync::Arc<dyn ResearchSource>>>);

impl Sources {
    pub fn from_env() -> Self {
        let mut sources: Vec<std::sync::Arc<dyn ResearchSource>> = Vec::new();
        match crate::research::serp::SerpSource::from_env() {
            Some(s) => {
                tracing::info!("research source: google-serp");
                sources.push(std::sync::Arc::new(s));
            }
            None => tracing::warn!(
                "no DataForSEO credentials, content briefs will fail until they are set"
            ),
        }
        Self(std::sync::Arc::new(sources))
    }
}

pub async fn build_brief(
    job: BriefJob,
    pool: Data<PgPool>,
    sources: Data<Sources>,
) -> Result<(), Error> {
    let pool: &PgPool = &pool;
    tracing::info!("researching `{}` ({})", job.topic, job.brief_id);

    sqlx::query("update briefs set status = 'running', started_at = now() where id = $1")
        .bind(job.brief_id)
        .execute(pool)
        .await
        .map_err(to_err)?;

    if sources.0.is_empty() {
        let msg = "no research sources configured (set DATAFORSEO_LOGIN and DATAFORSEO_PASSWORD)";
        fail(pool, job.brief_id, msg).await?;
        return Err(err(msg));
    }

    // Run every source and merge. One failing source must not lose the findings
    // of the others, so errors are collected rather than propagated.
    let mut findings = Findings::default();
    let mut errors: Vec<String> = Vec::new();
    for source in sources.0.iter() {
        match source
            .research(&job.topic, &job.language, &job.country)
            .await
        {
            Ok(f) => findings.merge(f),
            Err(e) => {
                tracing::warn!("source {} failed: {e}", source.name());
                errors.push(format!("{}: {e}", source.name()));
            }
        }
    }

    // Only a brief with nothing at all in it counts as a failure.
    if findings.competitors.is_empty() && findings.ai_overview.is_none() {
        let msg = if errors.is_empty() {
            "no research results for this topic".to_string()
        } else {
            errors.join("; ")
        };
        fail(pool, job.brief_id, &msg).await?;
        return Err(err(&msg));
    }

    let mut tx = pool.begin().await.map_err(to_err)?;

    sqlx::query("delete from brief_competitors where brief_id = $1")
        .bind(job.brief_id)
        .execute(&mut *tx)
        .await
        .map_err(to_err)?;

    for c in &findings.competitors {
        sqlx::query(
            "insert into brief_competitors
                 (brief_id, rank, url, domain, title, description, headings, parsed, content)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             on conflict (brief_id, url) do nothing",
        )
        .bind(job.brief_id)
        .bind(c.rank)
        .bind(&c.url)
        .bind(&c.domain)
        .bind(&c.title)
        .bind(&c.description)
        .bind(serde_json::to_value(&c.headings).unwrap_or_default())
        .bind(c.parsed)
        .bind(&c.content)
        .execute(&mut *tx)
        .await
        .map_err(to_err)?;
    }

    sqlx::query(
        "update briefs
            set status = 'done',
                finished_at = now(),
                error = $2,
                ai_overview = $3,
                ai_sources = $4,
                questions = $5,
                related = $6
          where id = $1",
    )
    .bind(job.brief_id)
    // Partial failures are recorded without failing the brief, so the reader
    // knows a source was missing rather than silently getting less.
    .bind(if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    })
    .bind(&findings.ai_overview)
    .bind(serde_json::to_value(&findings.ai_sources).unwrap_or_default())
    .bind(serde_json::to_value(&findings.questions).unwrap_or_default())
    .bind(serde_json::to_value(&findings.related).unwrap_or_default())
    .execute(&mut *tx)
    .await
    .map_err(to_err)?;

    tx.commit().await.map_err(to_err)?;

    tracing::info!(
        "brief {} done: {} competitors ({} parsed), ai_overview: {}",
        job.brief_id,
        findings.competitors.len(),
        findings.competitors.iter().filter(|c| c.parsed).count(),
        findings.ai_overview.is_some()
    );
    Ok(())
}

async fn fail(pool: &PgPool, id: Uuid, msg: &str) -> Result<(), Error> {
    sqlx::query(
        "update briefs set status = 'failed', error = $2, finished_at = now() where id = $1",
    )
    .bind(id)
    .bind(msg)
    .execute(pool)
    .await
    .map_err(to_err)?;
    Ok(())
}

fn err(msg: &str) -> Error {
    Error::Failed(std::sync::Arc::new(Box::new(std::io::Error::other(
        msg.to_string(),
    ))))
}

fn to_err<E: std::fmt::Display>(e: E) -> Error {
    err(&e.to_string())
}
