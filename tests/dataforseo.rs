//! Verifies the DataForSEO provider against a local mock of the API.

use atp::providers::dataforseo::DataForSeo;
use atp::providers::SuggestionProvider;
use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Calls(Arc<Mutex<Vec<Value>>>);

async fn autocomplete(State(calls): State<Calls>, Json(body): Json<Value>) -> Json<Value> {
    calls.0.lock().unwrap().push(body.clone());
    let keyword = body[0]["keyword"].as_str().unwrap_or_default().to_string();
    Json(json!({
        "status_code": 20000,
        "status_message": "Ok.",
        "tasks": [{
            "status_code": 20000,
            "status_message": "Ok.",
            "result": [{
                "keyword": keyword,
                "items": [
                    {"type": "autocomplete", "suggestion": format!("{keyword} maker")},
                    {"type": "autocomplete", "suggestion": format!("{keyword} recipe")},
                    // duplicate across probes, must be deduplicated
                    {"type": "autocomplete", "suggestion": "shared suggestion"}
                ]
            }]
        }]
    }))
}

async fn search_volume(Json(body): Json<Value>) -> Json<Value> {
    let keywords = body[0]["keywords"].as_array().cloned().unwrap_or_default();
    let result: Vec<Value> = keywords
        .iter()
        .enumerate()
        .map(|(i, k)| {
            json!({
                "keyword": k.as_str().unwrap_or_default(),
                "search_volume": (i as i64 + 1) * 100,
                "cpc": 1.25,
                "competition_index": 42
            })
        })
        .collect();
    Json(json!({
        "status_code": 20000,
        "status_message": "Ok.",
        "tasks": [{"status_code": 20000, "result": result}]
    }))
}

async fn spawn_mock() -> (String, Calls) {
    let calls = Calls::default();
    let app = Router::new()
        .route(
            "/v3/serp/google/autocomplete/live/advanced",
            post(autocomplete),
        )
        .with_state(calls.clone())
        .route(
            "/v3/keywords_data/google_ads/search_volume/live",
            post(search_volume),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), calls)
}

#[tokio::test]
async fn harvests_and_categorises_dataforseo_suggestions() {
    let (base, calls) = spawn_mock().await;
    let provider = DataForSeo::new("login".into(), "password".into(), base);

    let items = provider.harvest("coffee", "en", "pl").await.unwrap();

    // Every probe fires exactly once: 9*2 questions + 7 prepositions + 6 comparisons
    // + 26 alphabetical + 1 related.
    assert_eq!(calls.0.lock().unwrap().len(), 18 + 7 + 6 + 26 + 1);

    // Location code for Poland is forwarded to the API.
    assert_eq!(calls.0.lock().unwrap()[0][0]["location_code"], 2616);
    assert_eq!(calls.0.lock().unwrap()[0][0]["language_code"], "en");

    // Duplicates collapse to a single entry.
    assert_eq!(
        items
            .iter()
            .filter(|s| s.text == "shared suggestion")
            .count(),
        1
    );

    let cats: std::collections::HashSet<_> = items.iter().map(|s| s.category.as_str()).collect();
    for expected in [
        "questions",
        "prepositions",
        "comparisons",
        "alphabetical",
        "related",
    ] {
        assert!(cats.contains(expected), "missing category {expected}");
    }

    // Without DATAFORSEO_SEARCH_VOLUME there are no paid metrics.
    assert!(items.iter().all(|s| s.search_volume.is_none()));
}

#[tokio::test]
async fn enriches_with_search_volume_when_enabled() {
    let (base, _calls) = spawn_mock().await;
    let provider =
        DataForSeo::new("login".into(), "password".into(), base).with_search_volume(true);

    let items = provider.harvest("tea", "en", "us").await.unwrap();

    assert!(items.iter().all(|s| s.search_volume.is_some()));
    assert!(items.iter().all(|s| s.cpc == Some(1.25)));
    assert!(items.iter().all(|s| s.competition == Some(42)));
    // Sorted by search volume, descending.
    let volumes: Vec<i64> = items.iter().filter_map(|s| s.search_volume).collect();
    let mut sorted = volumes.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(volumes, sorted);
}
