//! Checking published pages: did they rank, and does the assistant cite them.
//!
//! The loop the app never closed. Research, brief and draft all happen
//! before publication; this is the only part that happens after, and it is
//! what tells you whether any of the rest worked.
//!
//! Shared by the button on the page list and the watch loop, so a page can
//! be checked by hand or left to itself.

use crate::domain::{PageCheck, PhraseRank};
use crate::providers::dataforseo::DataForSeo;
use sqlx::PgPool;
use uuid::Uuid;

/// Rank checks are one SERP call each, so a page with twenty phrases would
/// cost $0.04 a check and, on a weekly watch, add up quietly.
pub const MAX_PHRASES: usize = 8;

/// Checks one page and stores the result.
pub async fn check_page(
    pool: &PgPool,
    provider: &DataForSeo,
    page_id: Uuid,
) -> anyhow::Result<PageCheck> {
    type Row = (String, String, String, String, serde_json::Value);
    let row: Option<Row> = sqlx::query_as(
        "select url, topic, language, country, phrases from tracked_pages where id = $1",
    )
    .bind(page_id)
    .fetch_optional(pool)
    .await?;
    let Some((url, topic, language, country, phrases)) = row else {
        anyhow::bail!("page not found");
    };
    let mut phrases: Vec<String> = serde_json::from_value(phrases).unwrap_or_default();
    if phrases.is_empty() {
        phrases.push(topic.clone());
    }
    phrases.truncate(MAX_PHRASES);

    let mut ranks: Vec<PhraseRank> = Vec::new();
    for phrase in phrases {
        // Sequential: eight SERP calls in parallel is a burst DataForSEO
        // has no reason to enjoy, and a check is not time-critical.
        let rank = match provider
            .organic_rank(&phrase, &url, &language, &country)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("rank check for `{phrase}` failed: {e}");
                None
            }
        };
        ranks.push(PhraseRank { phrase, rank });
    }
    let best_rank = ranks.iter().filter_map(|r| r.rank).min();

    // Does the assistant cite this page's site when asked the topic. One
    // question, not twenty: the citation audit is its own feature, this is
    // a yes or no about one page.
    let domain = crate::aeo::normalise_domain(&url);
    let cited = match provider.ask_perplexity(&topic).await {
        Ok((_, urls)) => Some(
            urls.iter()
                .any(|u| crate::aeo::normalise_domain(u) == domain),
        ),
        Err(e) => {
            tracing::warn!("citation check for `{topic}` failed: {e}");
            None
        }
    };

    sqlx::query(
        "insert into page_checks (page_id, best_rank, ranks, cited) values ($1, $2, $3, $4)",
    )
    .bind(page_id)
    .bind(best_rank)
    .bind(serde_json::to_value(&ranks).unwrap_or_default())
    .bind(cited)
    .execute(pool)
    .await?;
    sqlx::query("update tracked_pages set last_check_at = now() where id = $1")
        .bind(page_id)
        .execute(pool)
        .await?;

    Ok(PageCheck {
        best_rank,
        ranks,
        cited,
        checked_at: "just now".into(),
    })
}

/// Checks every enabled page not checked in the last week.
pub async fn tick(pool: &PgPool) -> anyhow::Result<usize> {
    let Some(provider) = crate::providers::dataforseo_from_env() else {
        return Ok(0);
    };
    let due: Vec<(Uuid,)> = sqlx::query_as(
        "select id from tracked_pages
          where enabled and (last_check_at is null or last_check_at + interval '7 days' <= now())",
    )
    .fetch_all(pool)
    .await?;

    let mut n = 0;
    for (id,) in due {
        match check_page(pool, &provider, id).await {
            Ok(c) => {
                tracing::info!(
                    "page {id}: best rank {:?}, cited {:?}",
                    c.best_rank,
                    c.cited
                );
                n += 1;
            }
            Err(e) => tracing::warn!("page {id} check failed: {e}"),
        }
    }
    Ok(n)
}
