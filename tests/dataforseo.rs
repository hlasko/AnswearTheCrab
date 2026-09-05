//! Verifies the DataForSEO provider against a local mock of the API.

use atp::providers::dataforseo::{DataForSeo, Mode};
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
                    // returned by every probe, must be deduplicated
                    {"type": "autocomplete", "suggestion": "shared coffee suggestion"}
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
    let provider =
        DataForSeo::new("login".into(), "password".into(), base).with_mode(Mode::Autocomplete);

    let items = provider.harvest("coffee", "en", "pl").await.unwrap();

    // Every probe fires exactly once. Derived from the probe matrix rather than
    // hardcoded, so growing a vocabulary does not break this test for no reason.
    let expected = atp::providers::probes("coffee", "en").len();
    assert_eq!(calls.0.lock().unwrap().len(), expected);
    // The autocomplete mode is the expensive one; keep that visible.
    assert!(expected > 50, "probe matrix unexpectedly small: {expected}");

    // Location code for Poland is forwarded to the API.
    assert_eq!(calls.0.lock().unwrap()[0][0]["location_code"], 2616);
    assert_eq!(calls.0.lock().unwrap()[0][0]["language_code"], "en");

    // Duplicates collapse to a single entry.
    assert_eq!(
        items
            .iter()
            .filter(|s| s.text == "shared coffee suggestion")
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
    let provider = DataForSeo::new("login".into(), "password".into(), base)
        .with_mode(Mode::Autocomplete)
        .with_search_volume(true);

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

async fn keyword_suggestions(Json(body): Json<Value>) -> Json<Value> {
    let kw = body[0]["keyword"].as_str().unwrap_or_default().to_string();
    let limit = body[0]["limit"].as_i64().unwrap_or(100);
    // Shapes taken from the documented Labs response: items[].keyword_data.keyword_info
    let phrases = [
        format!("how to make {kw}"),
        format!("{kw} vs tea"),
        format!("{kw} for beginners"),
        format!("{kw} grinder"),
        kw.clone(),
        format!("{kw} grinder"), // duplicate, must collapse
    ];
    let items: Vec<Value> = phrases
        .iter()
        .map(|p| {
            json!({
                "se_type": "google",
                "keyword_data": {
                    "keyword": p,
                    "keyword_info": {
                        "search_volume": 2400,
                        "cpc": 3.5,
                        "competition": 0.42
                    }
                }
            })
        })
        .collect();
    Json(json!({
        "status_code": 20000,
        "tasks": [{
            "status_code": 20000,
            "result": [{ "seed_keyword": kw, "items_count": items.len(), "items": items,
                         "limit_echo": limit }]
        }]
    }))
}

#[tokio::test]
async fn labs_mode_uses_a_single_call_and_categorises_phrases() {
    let calls = Calls::default();
    let app = Router::new()
        .route(
            "/v3/dataforseo_labs/google/keyword_suggestions/live",
            post({
                let calls = calls.clone();
                move |Json(b): Json<Value>| {
                    calls.0.lock().unwrap().push(b.clone());
                    keyword_suggestions(Json(b))
                }
            }),
        )
        // Autocomplete must NOT be hit in labs mode.
        .route(
            "/v3/serp/google/autocomplete/live/advanced",
            post(|| async {
                panic!("autocomplete must not be called in labs mode");
                #[allow(unreachable_code)]
                Json(json!({}))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let provider =
        DataForSeo::new("l".into(), "p".into(), format!("http://{addr}")).with_mode(Mode::Labs);
    let items = provider.harvest("coffee", "en", "pl").await.unwrap();

    // The whole point: one billable request instead of ~58.
    assert_eq!(calls.0.lock().unwrap().len(), 1);
    assert_eq!(calls.0.lock().unwrap()[0][0]["location_code"], 2616);

    // Seed echo and duplicates are dropped.
    assert!(!items.iter().any(|s| s.text == "coffee"));
    assert_eq!(
        items.iter().filter(|s| s.text == "coffee grinder").count(),
        1
    );

    let by = |t: &str| items.iter().find(|s| s.text == t).unwrap().category.clone();
    assert_eq!(by("how to make coffee"), "questions");
    assert_eq!(by("coffee vs tea"), "comparisons");
    assert_eq!(by("coffee for beginners"), "prepositions");
    assert_eq!(by("coffee grinder"), "alphabetical");

    // Metrics arrive without a second endpoint; competition is scaled to 0..100.
    let g = items.iter().find(|s| s.text == "coffee grinder").unwrap();
    assert_eq!(g.search_volume, Some(2400));
    assert_eq!(g.cpc, Some(3.5));
    assert_eq!(g.competition, Some(42));
}
