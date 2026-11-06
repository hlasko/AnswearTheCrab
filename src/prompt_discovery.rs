//! Conservative, deterministic prompt discovery. No model calls or invented prompts.
use crate::domain::{SearchResult, Suggestion};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PROMPTS: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub search_id: String,
    pub provider: String,
    pub source: String,
    pub collected_at: String,
    pub original: String,
    pub category: String,
    pub modifier: String,
    pub google_volume: Option<i64>,
    #[serde(default)]
    pub bing_volume: Option<i64>,
    pub asked_signal: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    pub text: String,
    pub intent: String,
    pub provenance: Vec<Provenance>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Discovery {
    pub candidates: Vec<Candidate>,
    pub approved: Vec<String>,
}

/// Preserve word order, negation, numbers, accents and internal punctuation.
/// Only case, whitespace, terminal question marks, explicit versus aliases,
/// and two exact English cost-question templates are equivalent. No stemming,
/// token sorting or fuzzy similarity thresholds. Other languages stay literal.
pub fn semantic_key(text: &str) -> String {
    let normalized = text
        .trim()
        .trim_end_matches('?')
        .split_whitespace()
        .map(|word| match word.to_lowercase().as_str() {
            "versus" | "vs." => "vs".to_string(),
            _ => word.to_lowercase(),
        })
        .collect::<Vec<_>>()
        .join(" ");
    // The subject must match verbatim, including qualifiers and negation.
    // Do not treat 'cost per month', 'worth', or 'can cost' as the same question.
    let cost_subject = normalized
        .strip_prefix("how much does ")
        .and_then(|s| s.strip_suffix(" cost"))
        .or_else(|| normalized.strip_prefix("what is the price of "));
    match cost_subject.filter(|s| !s.is_empty()) {
        Some(subject) => format!("cost-question::{subject}"),
        None => normalized,
    }
}

/// UI edits only invalidate approval when the actual set changes, not order.
pub fn matches_approved(selected: &[String], approved: &[String]) -> bool {
    if approved.is_empty() {
        return false;
    }
    let mut selected = selected.to_vec();
    let mut approved = approved.to_vec();
    selected.sort();
    approved.sort();
    selected == approved
}

fn intent(s: &Suggestion) -> Option<&'static str> {
    let key = semantic_key(&s.text);
    let words: Vec<_> = key.split_whitespace().collect();
    if words.iter().any(|w| {
        matches!(
            *w,
            "alternative"
                | "alternatives"
                | "alternatywa"
                | "alternatywy"
                | "alternativen"
                | "alternativa"
                | "alternativas"
        )
    }) {
        return Some("Alternatives");
    }
    if s.category == "comparisons" {
        return Some("Comparison");
    }
    if s.category == "recommendations"
        || words.iter().any(|w| {
            matches!(
                *w,
                "best"
                    | "recommend"
                    | "recommended"
                    | "recommendation"
                    | "recommendations"
                    | "najlepszy"
                    | "najlepsza"
                    | "najlepsze"
                    | "najlepszych"
                    | "polecany"
                    | "polecana"
                    | "jaki"
                    | "jaka"
                    | "jakie"
                    | "który"
                    | "która"
                    | "które"
            )
        })
    {
        return Some("Recommendation");
    }
    if s.category != "questions" {
        return None;
    }
    if key.starts_with("cost-question::") {
        return Some("Cost");
    }
    if words.iter().any(|w| {
        matches!(
            *w,
            "cost" | "price" | "prices" | "koszt" | "cena" | "ceny" | "preis" | "prix" | "precio"
        )
    }) {
        Some("Cost")
    } else if words
        .iter()
        .any(|w| matches!(*w, "how" | "jak" | "wie" | "comment" | "cómo"))
    {
        Some("How-to")
    } else {
        Some("Understanding")
    }
}

pub fn discover(result: &SearchResult) -> Vec<Candidate> {
    let mut items = result.suggestions.iter().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.text
            .cmp(&b.text)
            .then(a.category.cmp(&b.category))
            .then(a.modifier.cmp(&b.modifier))
    });
    let mut groups: BTreeMap<String, Candidate> = BTreeMap::new();
    for s in items {
        let Some(intent) = intent(s) else { continue };
        let key = semantic_key(&s.text);
        if key.is_empty() {
            continue;
        }
        let provenance = Provenance {
            search_id: result.search.id.clone(),
            provider: result.search.provider.clone(),
            source: result.search.source.clone(),
            collected_at: result.search.created_at.clone(),
            original: s.text.clone(),
            category: s.category.clone(),
            modifier: s.modifier.clone(),
            google_volume: if result.search.source == "bing" {
                None
            } else {
                s.search_volume
            },
            asked_signal: s.ai_volume,
            bing_volume: s.bing_volume.or_else(|| {
                (result.search.source == "bing")
                    .then_some(s.search_volume)
                    .flatten()
            }),
        };
        let candidate = groups.entry(key).or_insert_with(|| Candidate {
            text: s.text.clone(),
            intent: intent.into(),
            provenance: vec![],
        });
        if !candidate.provenance.contains(&provenance) {
            candidate.provenance.push(provenance);
        }
    }
    groups.into_values().collect()
}

/// Separate observed maxima, not totals across semantic variants. Metrics have
/// different units and must never be summed into a manufactured volume score.
pub fn metrics(candidate: &Candidate) -> (Option<i64>, Option<i64>, Option<i64>) {
    (
        candidate
            .provenance
            .iter()
            .filter_map(|p| p.asked_signal)
            .max(),
        candidate
            .provenance
            .iter()
            .filter_map(|p| p.google_volume)
            .max(),
        candidate
            .provenance
            .iter()
            .filter_map(|p| p.bing_volume)
            .max(),
    )
}

/// Filtering is presentation-only and cannot alter a selection or approval.
pub fn matches_filter(candidate: &Candidate, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || candidate.text.to_lowercase().contains(&query)
        || candidate.intent.to_lowercase().contains(&query)
        || candidate
            .provenance
            .iter()
            .any(|p| p.original.to_lowercase().contains(&query))
}

/// Round-robin over intent buckets. Within each bucket: Asked descending,
/// then Google descending, then Bing descending, then text. Known zero ranks
/// above unknown. These are separate tie-breakers, not an additive score.
pub fn recommended(candidates: &[Candidate]) -> Vec<String> {
    let mut buckets: BTreeMap<&str, Vec<&Candidate>> = BTreeMap::new();
    for c in candidates {
        buckets.entry(&c.intent).or_default().push(c);
    }
    for bucket in buckets.values_mut() {
        bucket.sort_by(|a, b| metrics(b).cmp(&metrics(a)).then(a.text.cmp(&b.text)));
    }
    let mut result = vec![];
    for index in 0..MAX_PROMPTS {
        for bucket in buckets.values() {
            if let Some(c) = bucket.get(index) {
                result.push(c.text.clone());
                if result.len() == MAX_PROMPTS {
                    return result;
                }
            }
        }
    }
    result
}

pub fn validate_selection(
    candidates: &[Candidate],
    selected: &[String],
) -> Result<Vec<Candidate>, String> {
    if selected.is_empty() || selected.len() > MAX_PROMPTS {
        return Err(format!("Approve between 1 and {MAX_PROMPTS} prompts."));
    }
    let mut seen = BTreeSet::new();
    selected
        .iter()
        .map(|text| {
            if !seen.insert(semantic_key(text)) {
                return Err("Duplicate prompts are not allowed.".into());
            }
            candidates
                .iter()
                .find(|c| &c.text == text)
                .cloned()
                .ok_or_else(|| {
                    "A selected prompt is not an eligible candidate. Reload discovery.".into()
                })
        })
        .collect()
}

#[cfg(feature = "ssr")]
pub async fn approved(
    pool: &sqlx::PgPool,
    search_id: uuid::Uuid,
) -> anyhow::Result<Vec<Candidate>> {
    let value: Option<serde_json::Value> = sqlx::query_scalar(
        "select p.prompts from prompt_selections p join searches s
         on p.topic = lower(s.keyword) and p.language = s.language
         and p.country = s.country and p.source = s.source where s.id = $1",
    )
    .bind(search_id)
    .fetch_optional(pool)
    .await?;
    value
        .map(serde_json::from_value)
        .transpose()
        .map(|v| v.unwrap_or_default())
        .map_err(Into::into)
}

#[cfg(feature = "ssr")]
pub async fn save(
    pool: &sqlx::PgPool,
    search_id: uuid::Uuid,
    candidates: &[Candidate],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !candidates.is_empty() && candidates.len() <= MAX_PROMPTS,
        "Approve 1 to 20 prompts"
    );
    anyhow::ensure!(
        sqlx::query_scalar::<_, bool>("select exists(select 1 from searches where id = $1)")
            .bind(search_id)
            .fetch_one(pool)
            .await?,
        "search not found"
    );
    sqlx::query(
        "insert into prompt_selections (topic, language, country, source, prompts)
         select lower(keyword), language, country, source, $2 from searches where id = $1
         on conflict (topic, language, country, source) do update
         set prompts = excluded.prompts, approved_at = now()
         where prompt_selections.prompts is distinct from excluded.prompts",
    )
    .bind(search_id)
    .bind(serde_json::to_value(candidates)?)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn result(items: Vec<Suggestion>) -> SearchResult {
        SearchResult {
            search: crate::domain::SearchSummary {
                id: "search-1".into(),
                provider: "google-suggest".into(),
                source: "google".into(),
                keyword: "coffee".into(),
                language: "en".into(),
                country: "us".into(),
                status: "done".into(),
                error: None,
                suggestion_count: items.len() as i32,
                created_at: "2026-11-06".into(),
                age: "just now".into(),
            },
            suggestions: items,
        }
    }
    #[test]
    fn merges_only_supported_variants_and_preserves_all_real_provenance() {
        let mut a = Suggestion::new("Coffee versus tea?", "comparisons", "versus");
        a.search_volume = Some(70);
        a.ai_volume = Some(3);
        let b = Suggestion::new("coffee vs tea", "comparisons", "vs");
        let data = result(vec![
            a,
            b,
            Suggestion::new("coffee alternatives", "related", "a"),
            Suggestion::new("how to brew coffee", "questions", "how"),
            Suggestion::new("coffee mugs", "related", "m"),
            Suggestion::new("tea vs coffee", "comparisons", "vs"),
        ]);
        let candidates = discover(&data);
        assert_eq!(candidates.len(), 4);
        let merged = candidates.iter().find(|c| c.provenance.len() == 2).unwrap();
        assert_eq!(merged.provenance[0].google_volume, Some(70));
        assert_eq!(merged.provenance[0].asked_signal, Some(3));
        assert_eq!(merged.provenance[1].google_volume, None);
        assert!(merged
            .provenance
            .iter()
            .all(|p| p.search_id == "search-1" && p.provider == "google-suggest"));
        let mut reversed = data.clone();
        reversed.suggestions.reverse();
        assert_eq!(candidates, discover(&reversed));
        let mut bing = data;
        bing.search.source = "bing".into();
        assert!(discover(&bing)
            .iter()
            .flat_map(|c| &c.provenance)
            .all(|p| p.google_volume.is_none()));
    }
    #[test]
    fn conservative_equivalence() {
        assert_eq!(semantic_key("  A versus B? "), semantic_key("a vs. b"));
        assert_eq!(
            semantic_key("How much does coffee cost?"),
            semantic_key("What is the price of coffee?")
        );
        assert_ne!(
            semantic_key("How much does coffee cost per month?"),
            semantic_key("What is the price of coffee?")
        );
        for (a, b) in [
            ("a vs b", "b vs a"),
            ("is x safe", "is x not safe"),
            ("x 10", "x 20"),
            ("résumé", "resume"),
            ("x-y", "x y"),
        ] {
            assert_ne!(semantic_key(a), semantic_key(b));
        }
    }
    #[test]
    fn approval_changes_only_with_the_set() {
        let approved = vec!["a".into(), "b".into()];
        assert!(matches_approved(&approved, &approved));
        assert!(matches_approved(&["b".into(), "a".into()], &approved));
        assert!(!matches_approved(&["a".into()], &approved));
        assert!(!matches_approved(&[], &[]));
    }
    fn candidate(text: &str, intent: &str) -> Candidate {
        Candidate {
            text: text.into(),
            intent: intent.into(),
            provenance: vec![],
        }
    }
    #[test]
    fn recommendation_intent_includes_polish_and_related_phrases() {
        let phrases = [
            "best coffee",
            "najlepszy ekspres",
            "najlepsza kawa",
            "jaki ekspres kupić",
            "który ekspres wybrać",
        ];
        let mut items = phrases
            .iter()
            .map(|s| Suggestion::new(*s, "related", ""))
            .collect::<Vec<_>>();
        items.push(Suggestion::new("editor's picks", "recommendations", ""));
        let candidates = discover(&result(items));
        assert_eq!(candidates.len(), 6);
        assert!(candidates.iter().all(|c| c.intent == "Recommendation"));
        assert_eq!(
            intent(&Suggestion::new("best coffee vs tea", "comparisons", "vs")),
            Some("Comparison")
        );
    }
    #[test]
    fn ranks_by_separate_observed_metrics_not_alphabet_or_combined_volume() {
        let measured = |text, asked, google, bing| {
            let mut s = Suggestion::new(text, "questions", "what");
            s.ai_volume = asked;
            s.search_volume = google;
            s.bing_volume = bing;
            s
        };
        let mut candidates = discover(&result(vec![
            measured("z asked", Some(10), Some(1), None),
            measured("a google", Some(9), Some(999999), Some(999999)),
            measured("b google", Some(9), Some(99), Some(999999)),
            measured("c bing", Some(9), Some(99), Some(99)),
            measured("d zero", Some(0), None, None),
            measured("e unknown", None, Some(999999), Some(999999)),
        ]));
        assert_eq!(
            recommended(&candidates),
            [
                "z asked",
                "a google",
                "b google",
                "c bing",
                "d zero",
                "e unknown"
            ]
        );
        candidates.reverse();
        assert_eq!(recommended(&candidates)[0], "z asked");
        let c = candidates.iter_mut().find(|c| c.text == "z asked").unwrap();
        let mut other = c.provenance[0].clone();
        other.asked_signal = Some(3);
        other.google_volume = Some(7);
        c.provenance.push(other);
        assert_eq!(metrics(c), (Some(10), Some(7), None));
    }
    #[test]
    fn candidate_filter_and_old_snapshot_are_selection_safe() {
        let candidates = discover(&result(vec![Suggestion::new(
            "najlepsza kawa",
            "related",
            "",
        )]));
        let c = &candidates[0];
        let before = c.clone();
        assert!(matches_filter(c, "  KAWA "));
        assert!(matches_filter(c, "recommendation"));
        assert!(matches_filter(c, ""));
        assert!(!matches_filter(c, "not present"));
        assert_eq!(*c, before);
        let mut old = serde_json::to_value(c).unwrap();
        old["provenance"][0]
            .as_object_mut()
            .unwrap()
            .remove("bing_volume");
        let restored: Candidate = serde_json::from_value(old).unwrap();
        assert_eq!(restored.text, c.text);
        assert_eq!(restored.provenance[0].bing_volume, None);
    }
    #[test]
    fn balanced_and_bounded_without_metric_conflation() {
        let mut candidates = (0..40)
            .map(|i| candidate(&format!("q{i:02}"), "Understanding"))
            .collect::<Vec<_>>();
        candidates.push(candidate("alternative", "Alternatives"));
        candidates.push(candidate("comparison", "Comparison"));
        let set = recommended(&candidates);
        assert_eq!(set.len(), 20);
        assert!(set.contains(&"alternative".into()) && set.contains(&"comparison".into()));
        candidates.reverse();
        assert_eq!(set, recommended(&candidates));
    }
    #[test]
    fn rejects_empty_unknown_duplicate_and_over_limit() {
        let c = vec![
            candidate("a vs b", "Comparison"),
            candidate("a versus b", "Comparison"),
        ];
        assert!(validate_selection(&c, &[]).is_err());
        assert!(validate_selection(&c, &["arbitrary".into()]).is_err());
        assert!(validate_selection(&c, &["a vs b".into(), "a versus b".into()]).is_err());
        assert!(validate_selection(&c, &vec!["a vs b".into(); 21]).is_err());
        assert_eq!(validate_selection(&c, &["a vs b".into()]).unwrap().len(), 1);
    }
}
