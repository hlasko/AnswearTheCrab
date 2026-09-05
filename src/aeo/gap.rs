//! What the AI answer leaves out that the ranking pages cover.
//!
//! Google's summary is shown above the results, so a page that repeats it adds
//! nothing a reader cannot already see. The useful ground is what competitors
//! treat as worth a section and the summary does not carry: named methods,
//! specific numbers, edge cases.
//!
//! This turns "write something better" into a concrete instruction, and it is
//! free: we already store both the AI answer text and the competitors' outlines.

use crate::domain::Brief;

/// A subject competitors cover that the AI answer does not mention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapTopic {
    /// The heading as a competitor wrote it, so the writer sees real wording.
    pub title: String,
    /// How many competitors give it a section. Higher means safer ground.
    pub competitors: usize,
}

/// Words too common to signal what a section is about.
///
/// Without these, "jak" and "kawa" match everything and every heading looks
/// covered. The lists are deliberately small: only words that carry no topic.
const STOPWORDS: &[&str] = &[
    // Polish
    "jak", "czy", "co", "to", "jest", "sie", "się", "nie", "dla", "przy", "oraz", "lub", "ale",
    "tez", "też", "byc", "być", "ma", "na", "do", "od", "za", "po", "we", "ze", "że", "i", "a",
    "o", "u", "w", "z", "ktory", "który", "która", "które", "jego", "jej", "ich", "tym", "tej",
    "ten", "ta", "te", "tego", "sa", "są", "bez", "pod", "nad", "przed", "przez",
    // English
    "the", "and", "for", "with", "what", "how", "why", "when", "does", "can", "you", "your", "are",
    "is", "of", "to", "in", "on", "at", "it", "its", "this", "that", "a", "an", "or", "but",
    "from", "about", "into", "than", "then", "they", "their",
    // German, Spanish, French: only the highest-frequency function words
    "der", "die", "das", "und", "ist", "wie", "was", "fur", "für", "mit", "el", "la", "los", "las",
    "que", "como", "por", "para", "con", "les", "des", "une", "est", "pour", "avec", "dans",
];

/// Length a word is truncated to before comparison.
///
/// A crude stand-in for lemmatisation, which Polish badly needs: without it
/// "filtra" and "filtrem" look like different subjects and the summary appears
/// to skip ground it plainly covers.
///
/// Four was chosen by measuring both directions on real pairs from our briefs
/// rather than by taste. It errs towards treating words as the same
/// ("smak"/"smakowy" collide), which drops a borderline gap. The opposite error
/// invents a gap that the summary already covers, sending the writer to
/// duplicate Google, and that is the worse failure.
const STEM: usize = 4;

/// Splits text into meaningful lowercase word stems.
///
/// Stopwords are removed *after* truncation as well as before: "przygotowanie"
/// shortens to "przy", which is itself a Polish preposition, and letting that
/// through made a covered heading look like a gap because "przy" appears in no
/// summary as a subject.
fn stems(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 2)
        .filter(|w| !STOPWORDS.contains(w))
        .map(|w| -> String { w.chars().take(STEM).collect() })
        .filter(|w: &String| !STOPWORDS.contains(&w.as_str()))
        .collect()
}

/// Headings whose subject the AI answer never touches.
///
/// A heading counts as covered when *every* meaningful word in it appears in
/// the summary. That direction is deliberate: claiming a gap that is really
/// covered wastes the writer's effort and makes the feature untrustworthy, so
/// borderline cases are dropped rather than reported.
///
/// Returns nothing when there is no AI answer, because then the whole page is
/// open ground and there is no gap to speak of.
pub fn topic_gap(brief: &Brief) -> Vec<GapTopic> {
    let Some(ai) = &brief.ai_overview else {
        return Vec::new();
    };
    let answer: Vec<String> = stems(ai);
    if answer.is_empty() {
        return Vec::new();
    }

    // Every heading from every competitor, not just the ones repeated word for
    // word. Measured on real briefs: of 45 distinct headings for "jak parzyć
    // kawę w dripie", exactly one is shared verbatim by two competitors, so
    // filtering on repetition throws away nearly all the evidence. Pages phrase
    // the same subject differently, so headings are grouped by their stems.
    let mut groups: Vec<(Vec<String>, String, Vec<usize>)> = Vec::new();

    for c in &brief.competitors {
        for h in &c.headings {
            let title = h.title.trim();
            // Furniture rather than subject matter. These say nothing about
            // what a page covers, and a writer told to "add Podsumowanie" has
            // learned nothing.
            if title.chars().count() < 8 || is_furniture(title) {
                continue;
            }
            let words = stems(title);
            if words.is_empty() {
                continue;
            }
            // Skip anything the summary already covers. Erring towards
            // "covered" is deliberate: inventing a gap sends the writer to
            // duplicate Google, which is the worse mistake.
            let hits = words.iter().filter(|w| answer.contains(w)).count();
            if hits * 2 > words.len() {
                continue;
            }

            match groups.iter_mut().find(|(ws, _, _)| overlap(ws, &words)) {
                Some((_, _, seen)) => {
                    if !seen.contains(&(c.rank as usize)) {
                        seen.push(c.rank as usize);
                    }
                }
                None => groups.push((words, title.to_string(), vec![c.rank as usize])),
            }
        }
    }

    let mut out: Vec<GapTopic> = groups
        .into_iter()
        // Two independent pages, not one. Measured on real briefs, single-page
        // headings are dominated by another article's sidebar and by page
        // furniture that survived the name filter: "Dbamy o Twoją prywatność",
        // "COPYRIGHT", "Mammografia a USG piersi" under an article about coffee.
        // Agreement between competitors is what separates a subject from noise.
        .filter(|(_, _, seen)| seen.len() > 1)
        .map(|(_, title, seen)| GapTopic {
            title,
            competitors: seen.len(),
        })
        .collect();

    // Most useful first: ground several independent pages thought worth a
    // section is safer than one page's idiosyncrasy.
    out.sort_by(|a, b| {
        b.competitors
            .cmp(&a.competitors)
            .then(a.title.cmp(&b.title))
    });
    out.truncate(12);
    out
}

/// Structural headings that carry no subject.
fn is_furniture(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    const MARKERS: &[&str] = &[
        "podsumowanie",
        "faq",
        "pytania i odpowiedzi",
        "najczestsze pytania",
        "najczęstsze pytania",
        "spis tresci",
        "spis treści",
        "summary",
        "conclusion",
        "table of contents",
        "zobacz tez",
        "zobacz też",
        "powiazane",
        "powiązane",
        "komentarze",
        "bibliografia",
        "zrodla",
        "źródła",
        "references",
    ];
    if MARKERS.iter().any(|m| t == *m || t.starts_with(m)) {
        return true;
    }
    // Numbered steps: "2. Jak powstaje kamień?" is a step in someone's
    // walkthrough, not a subject to write about.
    t.chars().next().is_some_and(|c| c.is_ascii_digit())
        && t.chars()
            .take_while(|c| !c.is_whitespace())
            .any(|c| c == '.' || c == ')')
}

/// Whether two headings are about the same subject.
///
/// Half the shorter heading's words in common: enough to group "Metoda Jamesa
/// Hoffmana" with "Przepis Hoffmana" without merging unrelated sections.
fn overlap(a: &[String], b: &[String]) -> bool {
    let shared = a.iter().filter(|w| b.contains(w)).count();
    let shorter = a.len().min(b.len());
    shorter > 0 && shared * 2 >= shorter
}

#[cfg(test)]
mod realdata {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a database; run with --ignored to inspect real briefs"]
    async fn gap_on_every_real_brief() {
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".into());
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let ids: Vec<(uuid::Uuid, String)> =
            sqlx::query_as("select id, topic from briefs where ai_overview is not null")
                .fetch_all(&pool)
                .await
                .unwrap();
        for (id, topic) in ids {
            let b = crate::draft_job::load_brief(&pool, id).await.unwrap();
            let gap = topic_gap(&b);
            println!("\n=== {topic} ({} luk)", gap.len());
            for g in &gap {
                println!("   [{}x] {}", g.competitors, g.title);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Competitor, Heading};

    fn brief(ai: Option<&str>, headings: &[&[&str]]) -> Brief {
        Brief {
            id: "x".into(),
            topic: "jak parzyć kawę w dripie".into(),
            language: "pl".into(),
            country: "pl".into(),
            status: "done".into(),
            error: None,
            ai_overview: ai.map(str::to_string),
            ai_sources: vec![],
            questions: vec![],
            related: vec![],
            created_at: String::new(),
            competitors: headings
                .iter()
                .enumerate()
                .map(|(i, hs)| Competitor {
                    rank: i as i32 + 1,
                    url: format!("https://e{i}.example"),
                    domain: format!("e{i}.example"),
                    title: None,
                    description: None,
                    parsed: true,
                    headings: hs
                        .iter()
                        .map(|t| Heading {
                            level: 2,
                            title: (*t).to_string(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn reports_what_the_summary_never_mentions() {
        // Shortened from the real AI Overview for "jak parzyć kawę w dripie".
        let ai = "Parzenie kawy w dripie to prosta metoda przelewowa. Dripper Hario V60 \
                  z papierowym filtrem, świeżo palona kawa, młynek żarnowy, waga i woda \
                  o temperaturze 92-95°C. Użyj 6 g kawy na każde 100 ml wody.";
        // Real competitor headings from the same brief.
        let b = brief(
            Some(ai),
            &[
                &[
                    "Metoda Jamesa Hoffmana",
                    "Przygotowanie filtra",
                    "Podsumowanie",
                ],
                &["Metoda Jamesa Hoffmana", "Przygotowanie filtra", "Krok 1"],
            ],
        );

        let gap = topic_gap(&b);
        let titles: Vec<String> = gap.iter().map(|g| g.title.to_lowercase()).collect();

        // A named technique the summary never mentions: real, actionable ground.
        assert!(
            titles.iter().any(|t| t.contains("hoffman")),
            "expected Hoffman method as a gap, got: {titles:?}"
        );
        // The summary already covers filter preparation, so it is not a gap.
        assert!(
            !titles.iter().any(|t| t.contains("filtra")),
            "filter prep is covered by the summary: {titles:?}"
        );
        // "Krok 1" and "Podsumowanie" are furniture, not subjects.
        assert!(
            !titles
                .iter()
                .any(|t| t.contains("krok") || t.contains("podsumowanie")),
            "page furniture must not be reported: {titles:?}"
        );
    }

    #[test]
    fn one_page_saying_it_is_not_enough() {
        // Real noise from the database: a coffee article's sidebar offering
        // "Mammografia a USG piersi", and a cookie banner that survives the
        // furniture name filter. Only competitor agreement separates these
        // from a genuine subject.
        let b = brief(
            Some("Kawa zawiera kofeinę i polifenole."),
            &[
                &["Mammografia a USG piersi", "Dbamy o Twoją prywatność"],
                &["Kawa a układ sercowo-naczyniowy"],
                &["Kawa a układ sercowo-naczyniowy"],
            ],
        );
        let titles: Vec<String> = topic_gap(&b).iter().map(|g| g.title.clone()).collect();
        assert_eq!(
            titles,
            vec!["Kawa a układ sercowo-naczyniowy"],
            "only ground two competitors agree on should survive"
        );
    }

    #[test]
    fn numbered_steps_are_not_subjects() {
        let b = brief(
            Some("Kawa zawiera kofeinę."),
            &[
                &["2. Jak powstaje kamień?", "FAQ - pytania i odpowiedzi"],
                &["2. Jak powstaje kamień?", "FAQ - pytania i odpowiedzi"],
            ],
        );
        assert!(
            topic_gap(&b).is_empty(),
            "steps and FAQ blocks are structure, not gaps"
        );
    }

    #[test]
    fn no_ai_answer_means_no_gap() {
        // Without a summary there is nothing to be missing from.
        let b = brief(
            None,
            &[&["Metoda Jamesa Hoffmana"], &["Metoda Jamesa Hoffmana"]],
        );
        assert!(topic_gap(&b).is_empty());
    }

    #[test]
    fn inflection_does_not_invent_a_gap() {
        // "parzenia" vs "parzyć": Polish inflection must not read as a new subject.
        let b = brief(
            Some("Temperatura parzenia kawy powinna wynosić 92-95 stopni."),
            &[
                &["Temperatura parzenia kawy"],
                &["Temperatura parzenia kawy"],
            ],
        );
        assert!(
            topic_gap(&b).is_empty(),
            "same subject in a different form is not a gap"
        );
    }
}
