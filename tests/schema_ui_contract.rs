//! Contracts against the actual startup schema and rendered Radar form.
#![cfg(feature = "ssr")]

#[test]
fn radar_form_uses_labelled_native_controls_and_live_status() {
    use leptos::prelude::*;
    let owner = Owner::new();
    let html = owner.with(|| view! { <atp::radar::RadarPage/> }.to_html());
    for name in ["domain", "subdomains", "engine", "country", "language"] {
        assert!(html.contains(&format!("name=\"{name}\"")), "missing {name}");
    }
    assert_eq!(html.matches("<label>").count(), 5);
    assert!(html.contains("aria-live=\"polite\""));
    assert!(html.contains("aria-busy=\"false\""));
    assert!(html.contains("type=\"submit\""));
    assert!(html.contains("type=\"checkbox\""));
    assert!(html.contains("Enter any domain to search the local corpus."));
    assert!(html.contains("No paid API calls, no queued jobs."));
}

#[tokio::test]
#[ignore = "requires PostgreSQL; set RADAR_TEST_DATABASE_URL and run --ignored"]
async fn real_migrations_replay_and_preserve_radar_after_search_deletion(
) -> Result<(), Box<dyn std::error::Error>> {
    use futures::FutureExt;
    use sqlx::postgres::PgPoolOptions;
    let url = std::env::var("RADAR_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql:///postgres".into());
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let schema = format!("radar_contract_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("create schema {schema}"))
        .execute(&admin)
        .await?;
    let search_path = format!("set search_path to {schema}");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _| {
            let search_path = search_path.clone();
            Box::pin(async move {
                sqlx::query(&search_path).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await;
    let pool = match pool {
        Ok(pool) => pool,
        Err(error) => {
            let cleanup = sqlx::query(&format!("drop schema {schema} cascade"))
                .execute(&admin)
                .await;
            admin.close().await;
            if let Err(cleanup_error) = cleanup {
                eprintln!("failed to remove test schema {schema}: {cleanup_error}");
            }
            return Err(error.into());
        }
    };
    let result = std::panic::AssertUnwindSafe(async {
        let mut migrations = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()?;
        migrations.retain(|path| path.extension().is_some_and(|e| e == "sql"));
        migrations.sort();
        // Replay startup migrations with data already present, not just empty tables.
        for pass in 0..2 {
            for migration in &migrations {
                sqlx::raw_sql(&std::fs::read_to_string(migration)?)
                    .execute(&pool)
                    .await?;
            }
            if pass == 0 {
                sqlx::raw_sql(include_str!("fixtures/discovery_radar.sql"))
                    .execute(&pool)
                    .await?;
            }
        }
        let (records, total) = atp::radar::load_corpus(&pool, "", "", "").await?;
        assert_eq!((records.len(), total), (6, 6));
        let matches = atp::radar::match_corpus(records, total, "example.com".into(), false);
        assert_eq!(matches.matching_records, 4);
        // Existing ON DELETE SET NULL contracts must preserve historical answers.
        sqlx::query("delete from searches where id = '11111111-1111-4111-8111-111111111111'")
            .execute(&pool)
            .await?;
        let (records, total) = atp::radar::load_corpus(&pool, "", "", "").await?;
        assert_eq!((records.len(), total), (6, 6));
        assert_eq!(
            records.iter().filter(|r| r.internal_link.is_none()).count(),
            4
        );
        assert_eq!(
            records
                .iter()
                .filter(|r| r
                    .internal_link
                    .as_deref()
                    .is_some_and(|l| l.starts_with("/brief/")))
                .count(),
            2
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    pool.close().await;
    let cleanup = sqlx::query(&format!("drop schema {schema} cascade"))
        .execute(&admin)
        .await;
    admin.close().await;
    if let Err(error) = &cleanup {
        eprintln!("failed to remove test schema {schema}: {error}");
    }
    match result {
        Ok(result) => {
            result?;
            cleanup?;
            Ok(())
        }
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
