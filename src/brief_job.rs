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
                 (brief_id, rank, url, domain, title, description, headings, parsed, content,
                  domain_rank)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
        .bind(c.domain_rank)
        .execute(&mut *tx)
        .await
        .map_err(to_err)?;
    }

    // What an answer engine already says, for the writer to go past rather
    // than repeat. Three questions, not twenty: a brief needs the shape of
    // the existing answer, not a citation audit, and this runs on every
    // brief. Failure is not a failure of the brief.
    let answers = match crate::providers::dataforseo_from_env() {
        Some(p) => {
            let qs: Vec<String> = std::iter::once(job.topic.clone())
                .chain(findings.questions.iter().take(2).cloned())
                .collect();
            let mut got = Vec::new();
            for q in qs {
                match p.ask_perplexity(&q).await {
                    Ok((answer, urls)) => {
                        let mut domains: Vec<String> = Vec::new();
                        for u in &urls {
                            let d = crate::aeo::normalise_domain(u);
                            if !d.is_empty() && !domains.contains(&d) {
                                domains.push(d);
                            }
                        }
                        got.push(crate::domain::AiAnswer {
                            question: q,
                            answer,
                            domains,
                            urls,
                            cited: false,
                            cited_rank: None,
                        });
                    }
                    Err(e) => tracing::warn!("brief: answer engine for `{q}` failed: {e}"),
                }
            }
            got
        }
        None => Vec::new(),
    };

    sqlx::query(
        "update briefs
            set status = 'done',
                finished_at = now(),
                error = $2,
                ai_overview = $3,
                ai_sources = $4,
                questions = $5,
                related = $6,
                ai_answers = $7
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
    .bind(serde_json::to_value(&answers).unwrap_or_default())
    .execute(&mut *tx)
    .await
    .map_err(to_err)?;

    tx.commit().await.map_err(to_err)?;

    // Record where the user's site stands, once, at the moment the research
    // is fresh. This is what makes "researched again later" comparable, and
    // doing it here rather than on page view keeps page rendering read-only.
    if let Ok(Some(domain)) =
        sqlx::query_scalar::<_, String>("select value from settings where key = 'domain'")
            .fetch_optional(pool)
            .await
    {
        let brief = crate::domain::Brief {
            ai_answers: answers.clone(),
            id: job.brief_id.to_string(),
            topic: job.topic.clone(),
            language: job.language.clone(),
            country: job.country.clone(),
            status: "done".into(),
            error: None,
            ai_overview: findings.ai_overview.clone(),
            ai_sources: findings.ai_sources.clone(),
            questions: findings.questions.clone(),
            related: findings.related.clone(),
            created_at: String::new(),
            competitors: findings.competitors.clone(),
        };
        let c = crate::aeo::check(&brief, &domain);
        let _ = sqlx::query(
            "insert into citation_checks (brief_id, domain, cited, citation_rank, organic_rank)
             values ($1, $2, $3, $4, $5)
             on conflict (brief_id, domain) do update
                set cited = excluded.cited, citation_rank = excluded.citation_rank,
                    organic_rank = excluded.organic_rank, created_at = now()",
        )
        .bind(job.brief_id)
        .bind(&c.domain)
        .bind(c.cited)
        .bind(c.citation_rank.map(|r| r as i32))
        .bind(c.organic_rank)
        .execute(pool)
        .await;
    }

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
