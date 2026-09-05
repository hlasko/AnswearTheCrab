//! Validates the DataForSEO parser against response bodies copied verbatim from
//! the official documentation, plus the failure shapes the API can return.
//!
//! The point is that these fixtures are NOT hand-written by us: they are the
//! exact JSON published by DataForSEO, so they catch wrong field paths that a
//! self-authored mock would happily confirm.

use atp::providers::dataforseo::{DataForSeo, Mode};
use atp::providers::SuggestionProvider;
use axum::{http::StatusCode, routing::post, Json, Router};
use serde_json::{json, Value};

const SUGGESTIONS_SEED_ONLY: &str =
    include_str!("fixtures/labs_keyword_suggestions_seed_only.json");
const RELATED_KEYWORDS: &str = include_str!("fixtures/labs_related_keywords.json");
/// Captured from the LIVE api.dataforseo.com endpoint with invalid credentials.
const LIVE_AUTH_ERROR: &str = include_str!("fixtures/live_auth_error_40100.json");

/// Serves a fixed JSON body on the Labs endpoint.
async fn serve(body: Value, status: StatusCode) -> (String, ()) {
    let app = Router::new().route(
        "/v3/dataforseo_labs/google/keyword_suggestions/live",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), ())
}

fn provider(base: String) -> DataForSeo {
    DataForSeo::new("login".into(), "password".into(), base).with_mode(Mode::Labs)
}

#[tokio::test]
async fn parses_documented_response_with_populated_items() {
    // Related Keywords shares the items[].keyword_data.keyword_info shape with
    // Keyword Suggestions, and the docs publish a populated example for it.
    let body: Value = serde_json::from_str(RELATED_KEYWORDS).unwrap();
    let (base, _) = serve(body, StatusCode::OK).await;

    let items = provider(base).harvest("phone", "en", "us").await.unwrap();

    // The documented body holds 3 items; "phone" is the seed and gets dropped.
    assert_eq!(items.len(), 2, "got: {items:?}");

    let call = items.iter().find(|s| s.text == "phone call").unwrap();
    // Values straight from the published fixture.
    assert_eq!(call.search_volume, Some(27100));
    assert_eq!(call.cpc, Some(4.23));
    // competition 0.07 on the 0..1 Labs scale becomes 7 on the 0..100 index.
    assert_eq!(call.competition, Some(7));

    let app = items.iter().find(|s| s.text == "phone app").unwrap();
    assert_eq!(app.search_volume, Some(22200));
    assert_eq!(app.competition, Some(15));

    // Both are plain phrases, so they bucket alphabetically by the word that
    // follows the seed.
    assert_eq!(call.category, "alphabetical");
    assert_eq!(call.modifier, "c");
    assert_eq!(app.modifier, "a");
}

#[tokio::test]
async fn parses_keyword_suggestions_flat_item_shape() {
    // Keyword Suggestions differs from Related Keywords in two ways that a
    // self-written mock would never reveal:
    //  * result[0] carries only seed data and has "items": null,
    //  * result[1] holds the real hits, and each item exposes `keyword` and
    //    `keyword_info` FLAT, without the `keyword_data` wrapper.
    let body: Value = serde_json::from_str(SUGGESTIONS_SEED_ONLY).unwrap();
    assert!(
        body["tasks"][0]["result"][0]["items"].is_null(),
        "fixture should exercise the null-items path"
    );
    assert!(
        body["tasks"][0]["result"][1]["items"][0]["keyword_data"].is_null(),
        "fixture should exercise the flat item shape"
    );
    let (base, _) = serve(body, StatusCode::OK).await;

    let items = provider(base).harvest("phone", "en", "us").await.unwrap();

    // The null-items result is skipped and the flat one is read correctly.
    assert_eq!(items.len(), 1, "got: {items:?}");
    let s = &items[0];
    assert_eq!(s.text, "boost cell phone");
    assert_eq!(s.search_volume, Some(1_830_000));
    assert_eq!(s.cpc, Some(1.47));
    // 0.96 on the Labs scale -> 96 on the 0..100 index.
    assert_eq!(s.competition, Some(96));
}

#[tokio::test]
async fn empty_result_is_an_answer_not_an_error() {
    // An obscure keyword genuinely has no suggestions. Treating that as a
    // failure marked working searches as broken, so it now returns an empty
    // list and the UI explains it.
    let body = json!({
        "status_code": 20000,
        "tasks": [{ "status_code": 20000, "result": [{ "seed_keyword": "phone", "items": null }] }]
    });
    let (base, _) = serve(body, StatusCode::OK).await;

    let items = provider(base).harvest("phone", "en", "us").await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn surfaces_task_level_errors() {
    // DataForSEO reports per-task problems with a 200 OK envelope, e.g. 40501
    // "Invalid Field" or 40203 "Not Enough Money".
    let body = json!({
        "status_code": 20000,
        "status_message": "Ok.",
        "tasks": [{
            "status_code": 40203,
            "status_message": "Not Enough Money.",
            "result": null
        }]
    });
    let (base, _) = serve(body, StatusCode::OK).await;

    let err = provider(base)
        .harvest("phone", "en", "us")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("40203"), "should keep the code: {msg}");
    assert!(
        msg.contains("Not Enough Money"),
        "should keep the message: {msg}"
    );
}

#[tokio::test]
async fn surfaces_live_auth_error_verbatim() {
    // This body was captured from the real api.dataforseo.com by sending invalid
    // credentials, so it reflects production exactly: HTTP 401, status_code
    // 40100 (not 40101 as one might guess) and, notably, "tasks": null. A parser
    // that assumed `tasks` is always an array would panic here.
    let body: Value = serde_json::from_str(LIVE_AUTH_ERROR).unwrap();
    assert_eq!(body["status_code"], 40100);
    assert!(body["tasks"].is_null(), "live error carries tasks: null");

    let (base, _) = serve(body, StatusCode::UNAUTHORIZED).await;

    let err = provider(base)
        .harvest("phone", "en", "us")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("401") || msg.contains("40100"),
        "auth failure must be visible to the operator: {msg}"
    );
    assert!(
        msg.contains("not authorized"),
        "the API explanation should be preserved: {msg}"
    );
}

#[tokio::test]
async fn tolerates_items_with_missing_metrics() {
    // Not every keyword has Google Ads data; keyword_info fields can be null.
    let body = json!({
        "status_code": 20000,
        "tasks": [{
            "status_code": 20000,
            "result": [{
                "seed_keyword": "phone",
                "items": [
                    { "keyword_data": { "keyword": "phone holder",
                        "keyword_info": { "search_volume": null, "cpc": null, "competition": null } } },
                    // whole keyword_info absent
                    { "keyword_data": { "keyword": "phone stand" } },
                    // malformed entry without a keyword must be skipped, not panic
                    { "keyword_data": { "keyword_info": { "search_volume": 10 } } }
                ]
            }]
        }]
    });
    let (base, _) = serve(body, StatusCode::OK).await;

    let items = provider(base).harvest("phone", "en", "us").await.unwrap();

    assert_eq!(
        items.len(),
        2,
        "malformed entry should be skipped: {items:?}"
    );
    assert!(items.iter().all(|s| s.search_volume.is_none()));
    assert!(items.iter().all(|s| s.cpc.is_none()));
    assert!(items.iter().all(|s| s.competition.is_none()));
}
