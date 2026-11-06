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

    let list = questions_for(pool, search_id).await?;
    anyhow::ensure!(
        !list.is_empty(),
        "Approve a prompt set before asking the answer engine"
    );
    anyhow::ensure!(
        list.len() <= MAX_QUESTIONS,
        "Approved prompt set exceeds the limit"
    );

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

    anyhow::ensure!(
        answers.iter().any(|(_, result)| result.is_ok()),
        "every question failed; check the DataForSEO balance"
    );
    // All paid work is complete before acquiring a transaction. A failed
    // insert must not leave a run header or a misleading subset of answers.
    let mut transaction = pool.begin().await?;
    let run_id: Uuid = sqlx::query_scalar(
        "insert into ai_answer_runs (search_id, topic, language, country, domain)
         values ($1, $2, $3, $4, $5) returning id",
    )
    .bind(search_id)
    .bind(&topic)
    .bind(&language)
    .bind(&country)
    .bind(domain)
    .fetch_one(&mut *transaction)
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
             values ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(run_id)
        .bind(&question)
        .bind(&answer)
        .bind(serde_json::to_value(&domains)?)
        .bind(serde_json::to_value(&urls)?)
        .bind(cited_rank.is_some())
        .bind(cited_rank)
        .execute(&mut *transaction)
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
    transaction.commit().await?;
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

/// Exact approved snapshot, never reselected from changing search results.
pub async fn questions_for(pool: &PgPool, search_id: Uuid) -> anyhow::Result<Vec<String>> {
    Ok(crate::prompt_discovery::approved(pool, search_id)
        .await?
        .into_iter()
        .map(|c| c.text)
        .collect())
}
