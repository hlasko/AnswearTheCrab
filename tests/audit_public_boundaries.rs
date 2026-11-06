#![cfg(feature = "ssr")]

// Call the actual exported server functions, not copied validators. No app
// context is installed: reaching persistence would return "app state missing".
// These rejection paths must finish before accessing a DB, queue or provider.
#[tokio::test]
async fn public_search_rejects_bad_inputs_before_queue_access() {
    for (keyword, market, source, expected) in [
        (" ".to_string(), "pl-pl", "google", "keyword is empty"),
        ("a".repeat(101), "pl-pl", "google", "keyword too long"),
        (
            "coffee".to_string(),
            "invalid",
            "google",
            "unsupported market",
        ),
        ("coffee".to_string(), "pl-pl", "not-a-source", "source"),
    ] {
        let result = atp::app::create_search(keyword, market.into(), source.into()).await;
        assert!(
            matches!(result, Err(error) if error.to_string().to_lowercase().contains(expected) && !error.to_string().contains("app state missing"))
        );
    }
}

#[tokio::test]
async fn public_approval_rejects_malformed_and_oversized_payloads() {
    for (payload, expected) in [
        ("not json".to_string(), "Invalid prompt selection"),
        ("{}".to_string(), "Invalid prompt selection"),
        ("x".repeat(256 * 1024 + 1), "Prompt selection is too large"),
    ] {
        let result = atp::app::approve_prompts("invalid-id".into(), payload).await;
        assert!(matches!(result, Err(error) if error.to_string().contains(expected)));
    }
}

#[tokio::test]
async fn public_watch_rejects_invalid_intervals_before_id_or_database() {
    for days in [i32::MIN, -1, 0, 3651, i32::MAX] {
        let result = atp::app::toggle_watch("invalid-id".into(), Some(days)).await;
        assert!(
            matches!(result, Err(error) if error.to_string().contains("Schedule interval must be between 1 and 3650 days"))
        );
    }
    for days in [None, Some(1), Some(3650)] {
        let result = atp::app::toggle_watch("invalid-id".into(), days).await;
        assert!(matches!(result, Err(error) if error.to_string().contains("bad id")));
    }
}
