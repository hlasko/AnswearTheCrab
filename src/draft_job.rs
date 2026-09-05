//! Background job: write a draft from a finished brief.

use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::writer::{Kind, Writers};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftJob {
    pub draft_id: Uuid,
    pub brief_id: Uuid,
    /// "article" or "faq"; defaults to article for jobs queued before the FAQ
    /// button existed.
    #[serde(default)]
    pub kind: String,
}

pub const QUEUE: &str = "atp::draft";

pub async fn write_draft(
    job: DraftJob,
    pool: Data<PgPool>,
    writers: Data<Writers>,
) -> Result<(), Error> {
    let pool: &PgPool = &pool;

    let Some(writer) = writers.0.clone() else {
        let msg = "drafting is disabled: set OPENROUTER_API_KEY";
        fail(pool, job.draft_id, msg).await?;
        return Err(err(msg));
    };

    sqlx::query("update drafts set status = 'running', model = $2 where id = $1")
        .bind(job.draft_id)
        .bind(writer.model())
        .execute(pool)
        .await
        .map_err(to_err)?;

    // Load the brief through the same path the UI uses, so the model sees
    // exactly what the reader would have pasted.
    let brief = match load_brief(pool, job.brief_id).await {
        Ok(b) => b,
        Err(e) => {
            let msg = e.to_string();
            fail(pool, job.draft_id, &msg).await?;
            return Err(err(&msg));
        }
    };

    tracing::info!(
        "writing {} {} for `{}` with {}",
        job.kind,
        job.draft_id,
        brief.topic,
        writer.model()
    );

    let kind = Kind::from_str(&job.kind);

    match writer.write(&brief, kind).await {
        Ok(content) => {
            sqlx::query(
                "update drafts set status = 'done', content = $2, finished_at = now(),
                        error = null
                  where id = $1",
            )
            .bind(job.draft_id)
            .bind(&content)
            .execute(pool)
            .await
            .map_err(to_err)?;
            tracing::info!("draft {} done, {} characters", job.draft_id, content.len());
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            fail(pool, job.draft_id, &msg).await?;
            Err(err(&msg))
        }
    }
}

/// Test-only access to the brief loader, for measuring on real data.
#[cfg(test)]
pub async fn load_brief_for_test(pool: &PgPool, id: Uuid) -> anyhow::Result<crate::domain::Brief> {
    load_brief(pool, id).await
}

async fn load_brief(pool: &PgPool, id: Uuid) -> anyhow::Result<crate::domain::Brief> {
    use crate::domain::{Brief, Competitor, Heading};

    /// topic, language, country, ai_overview, ai_sources, questions, related
    type BriefRow = (
        String,
        String,
        String,
        Option<String>,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
    );

    let row: Option<BriefRow> = sqlx::query_as(
        "select topic, language, country, ai_overview, ai_sources, questions, related
           from briefs where id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some((topic, language, country, ai, sources, questions, related)) = row else {
        anyhow::bail!("brief not found");
    };

    let comps: Vec<(i32, String, String, Option<String>, serde_json::Value, bool)> =
        sqlx::query_as(
            "select rank, url, domain, title, headings, parsed
               from brief_competitors where brief_id = $1 order by rank",
        )
        .bind(id)
        .fetch_all(pool)
        .await?;

    let strings =
        |v: serde_json::Value| -> Vec<String> { serde_json::from_value(v).unwrap_or_default() };

    Ok(Brief {
        id: id.to_string(),
        topic,
        language,
        country,
        status: "done".into(),
        error: None,
        ai_overview: ai,
        ai_sources: strings(sources),
        questions: strings(questions),
        related: strings(related),
        created_at: String::new(),
        competitors: comps
            .into_iter()
            .map(|c| Competitor {
                rank: c.0,
                url: c.1,
                domain: c.2,
                title: c.3,
                description: None,
                headings: serde_json::from_value::<Vec<Heading>>(c.4).unwrap_or_default(),
                parsed: c.5,
            })
            .collect(),
    })
}

async fn fail(pool: &PgPool, id: Uuid, msg: &str) -> Result<(), Error> {
    sqlx::query(
        "update drafts set status = 'failed', error = $2, finished_at = now() where id = $1",
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
