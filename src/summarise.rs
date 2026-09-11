//! Facts for the AI summaries, assembled from the database.
//!
//! Every section's summary is produced the same way: this module reads the
//! same rows the section renders, writes them out as plain lines of
//! figures, and hands that text to the model. The model gets no page, no
//! query access and no tools, so the only numbers it can cite are the ones
//! measured here. That is the whole design: a summary that cannot drift
//! from what the section shows, because it never saw anything else.
//!
//! Each function returns `(title, facts)`. The title names the section so
//! the answer is about the right thing; the facts are deliberately terse,
//! because the model is being asked to read a table, not prose.

use crate::domain::Suggestion;
use sqlx::PgPool;
use uuid::Uuid;

/// Which section a summary is for. Values are stored, so they are stable
/// strings rather than a positional enum.
pub fn parse(kind: &str) -> Option<Kind> {
    match kind {
        "priorities" => Some(Kind::Priorities),
        "answers" => Some(Kind::Answers),
        "youtube" => Some(Kind::Youtube),
        "pages" => Some(Kind::Pages),
        "phrases" => Some(Kind::Phrases),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Priorities,
    Answers,
    Youtube,
    Pages,
    Phrases,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Priorities => "priorities",
            Kind::Answers => "answers",
            Kind::Youtube => "youtube",
            Kind::Pages => "pages",
            Kind::Phrases => "phrases",
        }
    }
}

/// Facts for one section. `search_id` is absent for sections that belong to
/// no single search, like the page tracker.
pub async fn facts(
    pool: &PgPool,
    kind: Kind,
    search_id: Option<Uuid>,
) -> anyhow::Result<(String, String)> {
    if kind == Kind::Pages {
        return pages(pool).await;
    }
    let Some(search_id) = search_id else {
        anyhow::bail!("this summary needs a search");
    };
    let head: Option<(String, String, String)> =
        sqlx::query_as("select keyword, language, country from searches where id = $1")
            .bind(search_id)
            .fetch_optional(pool)
            .await?;
    let Some((keyword, language, country)) = head else {
        anyhow::bail!("search not found");
    };
    let market = format!("{}/{}", language.to_uppercase(), country.to_uppercase());

    let (title, facts) = match kind {
        Kind::Priorities => priorities(pool, search_id, &keyword, &market).await,
        Kind::Phrases => phrases(pool, search_id, &keyword, &market).await,
        Kind::Answers => answers(pool, search_id, &keyword).await,
        Kind::Youtube => youtube(pool, search_id, &keyword).await,
        Kind::Pages => unreachable!("handled above"),
    }?;
    // The first live summary came back in English over Polish phrases,
    // because "the language of the data" is ambiguous when the data is a
    // table of numbers with Polish labels. The market says it outright.
    Ok((
        format!(
            "{title}\n\nWrite the answer in {}.",
            crate::domain::language_name(&language)
        ),
        facts,
    ))
}

/// The suggestions of a run, as the page sees them.
async fn suggestions(pool: &PgPool, search_id: Uuid) -> anyhow::Result<Vec<Suggestion>> {
    type Row = (
        String,
        String,
        String,
        Option<i64>,
        Option<f64>,
        Option<i32>,
        Option<i32>,
        Option<i64>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "select text, category, modifier, search_volume, cpc, competition,
                trend_yearly, ai_volume
           from suggestions where search_id = $1",
    )
    .bind(search_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let mut s = Suggestion::new(r.0, r.1, r.2);
            s.search_volume = r.3;
            s.cpc = r.4;
            s.competition = r.5;
            s.trend_yearly = r.6;
            s.ai_volume = r.7;
            s
        })
        .collect())
}

async fn priorities(
    pool: &PgPool,
    search_id: Uuid,
    keyword: &str,
    market: &str,
) -> anyhow::Result<(String, String)> {
    let items = suggestions(pool, search_id).await?;
    let clusters = crate::aeo::cluster(keyword, &items);
    let ranked = crate::aeo::rank(&clusters);
    anyhow::ensure!(!ranked.is_empty(), "no topics to interpret");

    let mut f = String::new();
    f.push_str(&format!(
        "Topic: \"{keyword}\", market {market}, {} phrases in {} topics.\n\
         Columns: topic | phrases | monthly searches | paid competition 0-100 \
         (lower is less contested) | asked-as-question figure | year-on-year \
         trend of its main phrase | suggested format.\n\n",
        items.len(),
        ranked.len()
    ));
    for p in ranked.iter().take(15) {
        f.push_str(&format!(
            "- {} | {} phrases | {} | comp {} | asked {} | trend {} | {}\n",
            p.topic,
            p.phrases,
            p.volume,
            p.competition.map(|c| c.to_string()).unwrap_or("?".into()),
            p.asked,
            p.trend.map(|t| format!("{t}%")).unwrap_or("?".into()),
            p.format.label(),
        ));
    }
    Ok((
        format!("Which of these topics to write first, for \"{keyword}\"."),
        f,
    ))
}

async fn phrases(
    pool: &PgPool,
    search_id: Uuid,
    keyword: &str,
    market: &str,
) -> anyhow::Result<(String, String)> {
    let items = suggestions(pool, search_id).await?;
    anyhow::ensure!(!items.is_empty(), "no phrases to interpret");

    let mut by_volume: Vec<&Suggestion> = items.iter().collect();
    by_volume.sort_by(|a, b| b.search_volume.cmp(&a.search_volume));
    let overlooked: Vec<&Suggestion> = items
        .iter()
        .filter(|s| s.search_volume.unwrap_or(0) >= 100 && s.competition.unwrap_or(100) < 30)
        .take(10)
        .collect();
    let mut rising: Vec<&Suggestion> = items
        .iter()
        .filter(|s| s.trend_yearly.unwrap_or(0) >= 20 && s.search_volume.unwrap_or(0) >= 50)
        .collect();
    rising.sort_by(|a, b| b.search_volume.cmp(&a.search_volume));
    let asked: Vec<&Suggestion> = items.iter().filter(|s| s.is_asked()).take(10).collect();

    let line = |s: &Suggestion| {
        format!(
            "- {} | {} searches | comp {} | cpc {} | trend {} | asked {}\n",
            s.text,
            s.search_volume.map(|v| v.to_string()).unwrap_or("?".into()),
            s.competition.map(|c| c.to_string()).unwrap_or("?".into()),
            s.cpc.map(|c| format!("${c:.2}")).unwrap_or("?".into()),
            s.trend_yearly
                .map(|t| format!("{t}%"))
                .unwrap_or("?".into()),
            s.ai_volume.map(|a| a.to_string()).unwrap_or("?".into()),
        )
    };

    let mut f = format!(
        "Topic: \"{keyword}\", market {market}, {} phrases.\n\n## Most searched\n",
        items.len()
    );
    for s in by_volume.iter().take(12) {
        f.push_str(&line(s));
    }
    if !overlooked.is_empty() {
        f.push_str("\n## Wanted but barely bid on (volume >=100, competition <30)\n");
        for s in &overlooked {
            f.push_str(&line(s));
        }
    }
    if !rising.is_empty() {
        f.push_str("\n## Growing at least 20% year on year\n");
        for s in rising.iter().take(10) {
            f.push_str(&line(s));
        }
    }
    if !asked.is_empty() {
        f.push_str(
            "\n## Asked as questions more than googled (the asked figure is at least 5% of searches)\n",
        );
        for s in &asked {
            f.push_str(&line(s));
        }
    }
    Ok((
        format!("What these phrases say about demand for \"{keyword}\"."),
        f,
    ))
}

async fn answers(
    pool: &PgPool,
    search_id: Uuid,
    keyword: &str,
) -> anyhow::Result<(String, String)> {
    type RunRow = (Uuid, String, chrono::DateTime<chrono::Utc>);
    let runs: Vec<RunRow> = sqlx::query_as(
        "select id, domain, created_at from ai_answer_runs
          where search_id = $1 order by created_at desc limit 5",
    )
    .bind(search_id)
    .fetch_all(pool)
    .await?;
    anyhow::ensure!(!runs.is_empty(), "nothing asked yet");

    let (run_id, domain, _) = runs[0].clone();
    type AnsRow = (String, serde_json::Value, bool);
    let rows: Vec<AnsRow> = sqlx::query_as(
        "select question, domains, cited from ai_answers where run_id = $1 order by question",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    let total = rows.len();
    let hits = rows.iter().filter(|r| r.2).count();
    let mut counts: std::collections::HashMap<String, usize> = Default::default();
    for r in &rows {
        let ds: Vec<String> = serde_json::from_value(r.1.clone()).unwrap_or_default();
        let mut seen: Vec<String> = Vec::new();
        for d in ds {
            if !seen.contains(&d) {
                *counts.entry(d.clone()).or_default() += 1;
                seen.push(d);
            }
        }
    }
    let mut board: Vec<(String, usize)> = counts.into_iter().collect();
    board.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut f = format!(
        "Topic: \"{keyword}\". Questions put to Perplexity: {total}.\n\
         Site being watched: {}.\n\
         Answers citing that site: {hits} of {total}.\n\n## Sites cited, and in how many answers\n",
        if domain.is_empty() {
            "none set"
        } else {
            &domain
        }
    );
    for (d, n) in board.iter().take(12) {
        f.push_str(&format!("- {d}: {n}/{total}\n"));
    }
    f.push_str("\n## Each question and the sites its answer cited\n");
    for r in rows.iter().take(20) {
        let ds: Vec<String> = serde_json::from_value(r.1.clone()).unwrap_or_default();
        f.push_str(&format!(
            "- {} -> {}\n",
            r.0,
            ds.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if runs.len() > 1 {
        f.push_str("\n## Earlier runs of the same questions\n");
        for (id, _, at) in runs.iter().skip(1) {
            let (n, c): (i64, i64) = sqlx::query_as(
                "select count(*), count(*) filter (where cited) from ai_answers where run_id = $1",
            )
            .bind(id)
            .fetch_one(pool)
            .await?;
            f.push_str(&format!(
                "- {}: cited in {c} of {n}\n",
                at.format("%Y-%m-%d")
            ));
        }
    }
    Ok((
        format!("Where \"{keyword}\" stands in AI answers, and what to do about it."),
        f,
    ))
}

async fn youtube(
    pool: &PgPool,
    search_id: Uuid,
    keyword: &str,
) -> anyhow::Result<(String, String)> {
    type Row = (String, i32, i64, i64, i32, i32);
    let rows: Vec<Row> = sqlx::query_as(
        "select phrase, videos, views_top10, views_median, fresh, ads_dropped
           from youtube_appetite where search_id = $1 order by views_top10 desc",
    )
    .bind(search_id)
    .fetch_all(pool)
    .await?;

    let check: Option<(serde_json::Value, serde_json::Value)> =
        sqlx::query_as("select weekly, weekly_web from youtube_checks where search_id = $1")
            .bind(search_id)
            .fetch_optional(pool)
            .await?;
    anyhow::ensure!(
        !rows.is_empty() || check.is_some(),
        "nothing checked on YouTube yet"
    );

    let mut f = format!("Topic: \"{keyword}\".\n");
    if let Some((weekly, weekly_web)) = check {
        let w: Vec<crate::domain::TrendPoint> = serde_json::from_value(weekly).unwrap_or_default();
        let wb: Vec<crate::domain::TrendPoint> =
            serde_json::from_value(weekly_web).unwrap_or_default();
        let zeros = w.iter().filter(|p| p.v == 0).count();
        let mean = |v: &[crate::domain::TrendPoint]| {
            if v.is_empty() {
                0.0
            } else {
                v.iter().map(|p| p.v as f64).sum::<f64>() / v.len() as f64
            }
        };
        f.push_str(&format!(
            "Google Trends, YouTube mode: mean index {:.0} over {} weeks, {zeros} of them \
             with no measurable searches. Same weeks on web search: mean index {:.0}. \
             (Each property is scaled to its own 100, so the two means are not sizes to \
             compare; the silent weeks are what say how steady it is.)\n",
            mean(&w),
            w.len(),
            mean(&wb)
        ));
    }
    if !rows.is_empty() {
        f.push_str(
            "\n## Views of the top ten videos per phrase (audience, not searches; clips \
             under a minute are dropped as adverts)\n\
             Columns: phrase | summed views | median views | how many of the top ten are \
             under a year old | clips dropped\n",
        );
        for r in &rows {
            f.push_str(&format!(
                "- {} | {} | median {} | {}/10 fresh | {} clips dropped\n",
                r.0, r.2, r.3, r.4, r.5
            ));
        }
    }
    Ok((
        format!("Whether \"{keyword}\" is worth a video, and which angle."),
        f,
    ))
}

async fn pages(pool: &PgPool) -> anyhow::Result<(String, String)> {
    type Row = (Uuid, String, String, chrono::DateTime<chrono::Utc>);
    let rows: Vec<Row> = sqlx::query_as(
        "select id, url, topic, created_at from tracked_pages where enabled order by created_at",
    )
    .fetch_all(pool)
    .await?;
    anyhow::ensure!(!rows.is_empty(), "no pages tracked yet");

    let mut f = String::from(
        "Published pages being checked. For each: its topic, its best organic \
         position now, the position at the previous check, and whether Perplexity \
         cited the site when asked the topic.\n\n",
    );
    for (id, url, topic, _) in &rows {
        type CheckRow = (Option<i32>, Option<bool>, serde_json::Value);
        let checks: Vec<CheckRow> = sqlx::query_as(
            "select best_rank, cited, ranks from page_checks
              where page_id = $1 order by checked_at desc limit 2",
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        let now = checks.first();
        let before = checks.get(1);
        f.push_str(&format!(
            "- {topic} ({url}): best position {}, previously {}, cited: {}\n",
            now.and_then(|c| c.0)
                .map(|r| format!("#{r}"))
                .unwrap_or("not in top 100".into()),
            before
                .and_then(|c| c.0)
                .map(|r| format!("#{r}"))
                .unwrap_or("no earlier check".into()),
            match now.and_then(|c| c.1) {
                Some(true) => "yes",
                Some(false) => "no",
                None => "not checked",
            }
        ));
        if let Some(c) = now {
            let ranks: Vec<crate::domain::PhraseRank> =
                serde_json::from_value(c.2.clone()).unwrap_or_default();
            for r in ranks {
                f.push_str(&format!(
                    "    - {}: {}\n",
                    r.phrase,
                    r.rank.map(|v| format!("#{v}")).unwrap_or("absent".into())
                ));
            }
        }
    }
    // The page tracker spans markets, so it has no one language to follow.
    // The topics are the user's own words, so that is the cue.
    Ok((
        "How the published pages are doing and what to do next. Write the answer in \
         the language the page topics are written in."
            .to_string(),
        f,
    ))
}
