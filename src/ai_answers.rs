//! Putting a topic's questions to an answer engine, and storing who it cites.
//!
//! Lives outside `app.rs` because two callers need it: the button on the
//! results page, and the watch loop, which repeats the same questions on a
//! schedule so the citation count can be compared over weeks.

use crate::domain::{AiAnswer, AiAnswerRun};
use crate::providers::dataforseo::DataForSeo;
use futures::stream::StreamExt;
use sqlx::PgPool;
use uuid::Uuid;

/// Six at a time: twenty sequential questions at ~8 s each is three minutes
/// of staring at a spinner, and DataForSEO allows far more than six.
const CONCURRENCY: usize = 6;

/// The most it will ask in one go, so a click costs about $0.13 rather than
/// an unbounded amount.
pub const MAX_QUESTIONS: usize = 20;

/// Asks every question and writes one run with its answers.
pub async fn ask_and_store(
    pool: &PgPool,
    provider: &DataForSeo,
    search_id: Uuid,
    questions: Vec<String>,
    domain: &str,
) -> anyhow::Result<AiAnswerRun> {
    let head: Option<(String, String, String)> =
        sqlx::query_as("select keyword, language, country from searches where id = $1")
            .bind(search_id)
            .fetch_optional(pool)
            .await?;
    let Some((topic, language, country)) = head else {
        anyhow::bail!("search not found");
    };

    let mut list = questions;
    list.dedup();
    list.truncate(MAX_QUESTIONS);
    anyhow::ensure!(!list.is_empty(), "no questions to ask");

    let answers: Vec<(String, anyhow::Result<(String, Vec<String>)>)> =
        futures::stream::iter(list.into_iter())
            .map(|q| {
                let provider = provider.clone();
                async move {
                    let r = provider.ask_perplexity(&q).await;
                    (q, r)
                }
            })
            .buffer_unordered(CONCURRENCY)
            .collect()
            .await;

    let run_id: Uuid = sqlx::query_scalar(
        "insert into ai_answer_runs (search_id, topic, language, country, domain)
         values ($1, $2, $3, $4, $5) returning id",
    )
    .bind(search_id)
    .bind(&topic)
    .bind(&language)
    .bind(&country)
    .bind(domain)
    .fetch_one(pool)
    .await?;

    let mut out = Vec::new();
    for (question, res) in answers {
        let (answer, urls) = match res {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("answer engine failed for `{question}`: {e}");
                continue;
            }
        };
        let mut domains: Vec<String> = Vec::new();
        for u in &urls {
            let d = crate::aeo::normalise_domain(u);
            if !d.is_empty() && !domains.contains(&d) {
                domains.push(d);
            }
        }
        let cited_rank = (!domain.is_empty())
            .then(|| domains.iter().position(|d| d == domain))
            .flatten()
            .map(|i| i as i32 + 1);

        sqlx::query(
            "insert into ai_answers (run_id, question, answer, domains, urls, cited, cited_rank)
             values ($1, $2, $3, $4, $5, $6, $7)
             on conflict (run_id, question) do nothing",
        )
        .bind(run_id)
        .bind(&question)
        .bind(&answer)
        .bind(serde_json::to_value(&domains).unwrap_or_default())
        .bind(serde_json::to_value(&urls).unwrap_or_default())
        .bind(cited_rank.is_some())
        .bind(cited_rank)
        .execute(pool)
        .await?;

        out.push(AiAnswer {
            question,
            answer,
            domains,
            urls,
            cited: cited_rank.is_some(),
            cited_rank,
        });
    }
    anyhow::ensure!(
        !out.is_empty(),
        "every question failed; check the DataForSEO balance"
    );
    out.sort_by(|a, b| b.cited.cmp(&a.cited).then(a.question.cmp(&b.question)));

    Ok(AiAnswerRun {
        id: run_id.to_string(),
        topic,
        language,
        country,
        domain: domain.to_string(),
        model: "sonar".into(),
        answers: out,
        created_at: "just now".into(),
    })
}

/// The questions a stored search suggests asking: its question phrases,
/// most searched first. Used by the watch loop, which has no UI to pick in.
pub async fn questions_for(pool: &PgPool, search_id: Uuid) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "select text from suggestions
          where search_id = $1 and category = 'questions'
          order by search_volume desc nulls last, text
          limit $2",
    )
    .bind(search_id)
    .bind(MAX_QUESTIONS as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}
