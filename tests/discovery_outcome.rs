//! Outcome regression: equal provider-request budget, identical synthetic input.
//! This measures requirement coverage, not real-world GEO ranking or demand.
use atp::domain::{SearchResult, SearchSummary, Suggestion};
use atp::prompt_discovery::{discover, recommended, semantic_key, validate_selection, MAX_PROMPTS};
use std::collections::BTreeSet;

#[test]
fn same_budget_covers_more_intents_without_duplicate_requests(
) -> Result<(), Box<dyn std::error::Error>> {
    let measured = |text: &str, category: &str, volume: i64| {
        let mut suggestion = Suggestion::new(text, category, "");
        suggestion.search_volume = Some(volume);
        suggestion.ai_volume = Some(volume / 10);
        suggestion
    };
    let mut suggestions = (0..24)
        .map(|n| {
            measured(
                &format!("jak przygotować kawę metodą {n}"),
                "questions",
                1000 - n,
            )
        })
        .collect::<Vec<_>>();
    // Highest-volume near-duplicate used to consume two monitoring requests.
    suggestions.push(measured(
        "JAK PRZYGOTOWAĆ KAWĘ METODĄ 0?",
        "questions",
        1100,
    ));
    // Low-volume but distinct intents were excluded or crowded out previously.
    suggestions.extend([
        measured("czy kawa jest zdrowa", "questions", 4),
        measured("kawa vs herbata", "comparisons", 3),
        measured("alternatywa dla kawy", "related", 2),
        measured("najlepsza kawa", "alphabetical", 1),
    ]);
    let result = SearchResult {
        search: SearchSummary {
            id: "fixture".into(),
            provider: "fixture".into(),
            source: "google".into(),
            keyword: "kawa".into(),
            language: "pl".into(),
            country: "pl".into(),
            status: "done".into(),
            error: None,
            suggestion_count: suggestions.len() as i32,
            created_at: "2026-11-06T07:00:00Z".into(),
            age: "fixture".into(),
        },
        suggestions,
    };

    // The actual previous selection rule: question-category rows, volume DESC, limit 20.
    let mut previous = result
        .suggestions
        .iter()
        .filter(|s| s.category == "questions")
        .collect::<Vec<_>>();
    previous.sort_by(|a, b| b.search_volume.cmp(&a.search_volume));
    let previous = previous.into_iter().take(MAX_PROMPTS).collect::<Vec<_>>();
    let old_unique = previous
        .iter()
        .map(|s| semantic_key(&s.text))
        .collect::<BTreeSet<_>>();
    // Independent fixture labels: every old selected row is a how-to question.
    assert!(previous
        .iter()
        .all(|s| s.text.to_lowercase().starts_with("jak ")));

    let candidates = discover(&result);
    let selection = recommended(&candidates);
    let approved = validate_selection(&candidates, &selection)?;
    let new_unique = selection
        .iter()
        .map(|s| semantic_key(s))
        .collect::<BTreeSet<_>>();
    let intents = approved
        .iter()
        .map(|c| c.intent.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!((previous.len(), selection.len()), (20, 20));
    assert_eq!((old_unique.len(), new_unique.len()), (19, 20));
    assert_eq!(
        intents,
        BTreeSet::from([
            "Alternatives",
            "Comparison",
            "How-to",
            "Recommendation",
            "Understanding"
        ])
    );
    for expected in [
        "czy kawa jest zdrowa",
        "kawa vs herbata",
        "alternatywa dla kawy",
        "najlepsza kawa",
    ] {
        assert!(selection.iter().any(|s| s == expected));
    }
    assert!(approved.iter().all(|c| !c.provenance.is_empty()));
    let merged = approved
        .iter()
        .find(|c| semantic_key(&c.text) == "jak przygotować kawę metodą 0")
        .ok_or("expected merged how-to candidate")?;
    assert_eq!(
        merged.provenance.len(),
        2,
        "deduplication preserves both observations"
    );
    println!("OUTCOME same 20-request budget: unique prompts 19 -> 20; represented fixture intents 1 -> 5; duplicate requests 1 -> 0; provenance 20/20; merged observations 2/2 retained");
    Ok(())
}
