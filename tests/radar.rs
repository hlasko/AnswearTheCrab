// Compile the owned module without requiring shared lib/route registration.
pub use atp::app;
#[path = "../src/radar.rs"]
mod radar;
use radar::*;

fn record(answer: &str, urls: &[&str], domains: &[&str]) -> RadarRecord {
    RadarRecord {
        prompt: "Same prompt".into(),
        answer: answer.into(),
        recorded_at: "2026-11-06T06:00:00.000000Z".into(),
        timestamp_basis: "test".into(),
        engine: "perplexity".into(),
        language: "en".into(),
        country: "us".into(),
        internal_link: None,
        urls: urls.iter().map(|s| s.to_string()).collect(),
        domains: domains.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn strict_normalization_and_safe_links() {
    for s in [
        "EXAMPLE.com",
        "www.example.com/a",
        "https://WWW.Example.Com:443/a?b=c",
        "example.com.",
    ] {
        assert_eq!(normalize_host(s).unwrap(), "example.com");
    }
    for s in [
        "",
        "https:///example.com",
        "https://user@example.com",
        "javascript:example.com",
        "ftp://example.com",
        "data:text/plain,example.com",
        "//example.com",
        "https://example.com\\@evil.com",
        "a..com",
        "-a.com",
        "a-.com",
        "http://a.com:bad",
        "a.com:65536",
        "a.com:0",
        "https://%65xample.com",
        "https://a.com\n",
        "127.0.0.1",
    ] {
        // Input may have surrounding whitespace, unlike links.
        if s != "https://a.com\n" {
            assert!(normalize_host(s).is_err(), "{s}");
        }
        assert!(safe_source_url(s).is_none(), "{s}");
    }
    assert!(safe_source_url("https://example.com/a?q=1&x=2").is_some());
    assert!(safe_source_url("example.com/a").is_none());
}

#[test]
fn domain_boundaries_and_mentions() {
    assert!(host_matches("a.example.com", "example.com", true));
    assert!(!host_matches("a.example.com", "example.com", false));
    for s in [
        "notexample.com",
        "example.com.evil.org",
        "example.comevil.org",
    ] {
        assert!(!host_matches(s, "example.com", true));
    }
    assert!(text_mentions(
        "Visit HTTPS://WWW.EXAMPLE.COM/a. (example.com).",
        "example.com",
        false
    ));
    for s in [
        "notexample.com",
        "example.com.evil",
        "a.example.com",
        "example.comé",
        "user@example.com",
        "fake_example.com",
    ] {
        assert!(!text_mentions(s, "example.com", false), "{s}");
    }
    assert!(text_mentions("a.example.com", "example.com", true));
}

#[test]
fn evidence_types_counts_and_display_cap() {
    let records = vec![
        record("example.com", &[], &[]),
        record(
            "No mention",
            &[
                "https://example.com/a",
                "https://example.com/a",
                "javascript:example.com",
            ],
            &[],
        ),
        record("example.com", &[], &["example.com"]),
        record("notexample.com", &[], &[]),
    ];
    let r = match_corpus(records, 10, "example.com".into(), false);
    assert_eq!(
        (
            r.total_available,
            r.examined,
            r.matching_records,
            r.distinct_prompts
        ),
        (10, 4, 3, 1)
    );
    assert!(r.matches[0].mentioned && !r.matches[0].recorded_source);
    assert!(!r.matches[1].mentioned && r.matches[1].recorded_source);
    assert_eq!(r.matches[1].matching_urls.len(), 1);
    assert!(r.matches[2].mentioned && r.matches[2].recorded_source);
    let many = match_corpus(
        vec![record("example.com", &[], &[]); 120],
        200,
        "example.com".into(),
        false,
    );
    assert_eq!(
        (
            many.matching_records,
            many.distinct_prompts,
            many.matches.len()
        ),
        (120, 1, 100)
    );
    let empty = match_corpus(vec![], 0, "example.com".into(), false);
    assert_eq!(empty.examined, 0);
    assert!(empty.oldest_examined.is_none());
}

#[cfg(feature = "ssr")]
#[tokio::test]
async fn postgres_isolated_schema_filters_snapshots_and_cap() {
    use futures::FutureExt;
    use sqlx::postgres::PgPoolOptions;
    let url = std::env::var("RADAR_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql:///postgres".into());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("local postgres required, or set RADAR_TEST_DATABASE_URL");
    let schema = format!("radar_test_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("create schema {schema}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("set search_path to {schema}"))
        .execute(&pool)
        .await
        .unwrap();
    let result = std::panic::AssertUnwindSafe(run_database_checks(&pool))
        .catch_unwind()
        .await;
    sqlx::query(&format!("drop schema {schema} cascade"))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    match result {
        Ok(result) => result.unwrap(),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(feature = "ssr")]
async fn run_database_checks(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::raw_sql(r#"
      create table ai_answer_runs (id uuid primary key, search_id uuid, created_at timestamptz, language text, country text);
      create table ai_answers (run_id uuid, question text, answer text, urls jsonb, domains jsonb);
      create table briefs (id uuid primary key, topic text, created_at timestamptz, language text, country text, ai_answers jsonb, ai_overview text, ai_sources jsonb);
      insert into ai_answer_runs values ('00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', '2026-11-06 06:00:00+00', 'en', 'us');
      insert into ai_answers values ('00000000-0000-0000-0000-000000000001', 'Same prompt', 'example.com', '["https://example.com/a"]', '[]');
      insert into briefs values ('00000000-0000-0000-0000-000000000003', 'Topic', '2026-11-05 06:00:00+00', 'pl', 'pl', '[{"question":"Same prompt","answer":"example.com","domains":["example.com"]}]', 'Overview example.com', '["example.com"]');
      insert into briefs values ('00000000-0000-0000-0000-000000000004', 'Malformed legacy', '2026-11-04 06:00:00+00', 'en', 'us', 'null', null, '[]');
    "#).execute(pool).await?;
    let (rows, total) = load_corpus(pool, "", "", "").await?;
    assert_eq!((rows.len(), total), (3, 3));
    assert_eq!(rows[0].recorded_at, "2026-11-06T06:00:00.000000Z");
    assert!(rows[0]
        .internal_link
        .as_ref()
        .unwrap()
        .starts_with("/search/"));
    let all = match_corpus(rows, total, "example.com".into(), false);
    assert_eq!((all.matching_records, all.distinct_prompts), (3, 2));
    let (aio, total) = load_corpus(pool, "google_aio", "pl", "pl").await?;
    assert_eq!(total, 1);
    assert!(aio[0].urls.is_empty());
    assert_eq!(aio[0].domains, ["example.com"]);
    assert_eq!(load_corpus(pool, "perplexity", "pl", "pl").await?.1, 1);
    assert_eq!(load_corpus(pool, "", "us", "en").await?.1, 1);
    assert_eq!(load_corpus(pool, "", "zz", "").await?.1, 0);
    sqlx::query("insert into ai_answers select '00000000-0000-0000-0000-000000000001'::uuid, 'Prompt ' || n, 'No domain', '[]'::jsonb, '[]'::jsonb from generate_series(1, 5001) n").execute(pool).await?;
    let (rows, total) = load_corpus(pool, "", "", "").await?;
    assert_eq!(total, 5004);
    assert_eq!(rows.len(), CORPUS_LIMIT);
    // Old brief snapshots are outside the cap, not silently counted as examined.
    assert!(rows
        .iter()
        .all(|r| r.engine == "perplexity" && r.country == "us"));
    assert_eq!(load_corpus(pool, "google_aio", "", "").await?.1, 1);
    Ok(())
}
