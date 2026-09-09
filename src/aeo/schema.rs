//! FAQPage structured data from a written FAQ.
//!
//! Google's answer box favours pages that mark their questions up as
//! `FAQPage`; the AnswerThePublic write-up says so plainly, and it costs a page
//! nothing. We already hold both halves, the People Also Ask question and the
//! answer the writer produced, so the markup is a transformation, not new work.
//!
//! Parsed from the FAQ draft's markdown rather than from the brief, because the
//! draft is what the reader will publish and the two can drift once edited.

use serde_json::{json, Value};

/// One question with its answer, as the page will show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Faq {
    pub question: String,
    pub answer: String,
}

/// Reads `## question` / answer pairs out of FAQ markdown.
///
/// Anything before the first heading is ignored, and a heading with no prose
/// beneath it is dropped: an empty answer in structured data is worse than no
/// entry, because Google treats it as spam.
pub fn parse(markdown: &str) -> Vec<Faq> {
    let mut out = Vec::new();
    let mut question: Option<String> = None;
    let mut answer: Vec<String> = Vec::new();

    let flush = |q: &mut Option<String>, a: &mut Vec<String>, out: &mut Vec<Faq>| {
        if let Some(question) = q.take() {
            let answer = a.join("\n\n").trim().to_string();
            if !answer.is_empty() {
                out.push(Faq { question, answer });
            }
        }
        a.clear();
    };

    for line in markdown.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("## ") {
            flush(&mut question, &mut answer, &mut out);
            question = Some(h.trim().to_string());
        } else if t.starts_with('#') {
            // A title or a deeper heading; not a question.
            continue;
        } else if question.is_some() && !t.is_empty() {
            answer.push(strip_markdown(t));
        }
    }
    flush(&mut question, &mut answer, &mut out);
    out
}

/// Plain text for the `text` field: structured data carries no formatting.
fn strip_markdown(s: &str) -> String {
    let mut t = s.to_string();
    for mark in ["**", "__", "`"] {
        t = t.replace(mark, "");
    }
    // "- item" bullets become plain sentences.
    t.trim_start_matches(['-', '*']).trim().to_string()
}

/// Renders the FAQPage JSON-LD, ready to paste into a `<script>` tag.
pub fn json_ld(faqs: &[Faq]) -> String {
    let entities: Vec<Value> = faqs
        .iter()
        .map(|f| {
            json!({
                "@type": "Question",
                "name": f.question,
                "acceptedAnswer": {
                    "@type": "Answer",
                    "text": f.answer,
                }
            })
        })
        .collect();

    let doc = json!({
        "@context": "https://schema.org",
        "@type": "FAQPage",
        "mainEntity": entities,
    });

    // Pretty-printed: this is pasted by a person into a page, and a single
    // 4k-character line is impossible to review.
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

/// The whole thing wrapped in the tag, which is what actually goes into HTML.
pub fn script_tag(faqs: &[Faq]) -> String {
    format!(
        "<script type=\"application/ld+json\">\n{}\n</script>",
        json_ld(faqs)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_headings_with_their_answers() {
        // Shaped like our real FAQ drafts.
        let md = "## Ile kcal ma latte 250 ml?\n\n\
                  Kawa latte 250 ml ma około 90-150 kcal.\n\n\
                  Wersja z mlekiem chudym to około 90-110 kcal.\n\n\
                  ## Czy latte tuczy?\n\n\
                  Nie, jeśli pijesz bez cukru.";
        let f = parse(md);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].question, "Ile kcal ma latte 250 ml?");
        assert!(f[0].answer.starts_with("Kawa latte 250 ml"));
        assert!(f[0].answer.contains("90-110 kcal"), "both paragraphs kept");
        assert_eq!(f[1].answer, "Nie, jeśli pijesz bez cukru.");
    }

    #[test]
    fn a_question_without_an_answer_is_dropped() {
        // Empty answers in FAQPage markup read as spam to Google.
        let md = "## Pytanie bez odpowiedzi\n\n## Pytanie z odpowiedzią\n\nTak.";
        let f = parse(md);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].question, "Pytanie z odpowiedzią");
    }

    #[test]
    fn produces_valid_faqpage_markup() {
        let f = vec![Faq {
            question: "Czy \"latte\" tuczy?".into(),
            answer: "Nie, bez cukru.".into(),
        }];
        let ld = json_ld(&f);
        let v: Value = serde_json::from_str(&ld).expect("valid JSON");
        assert_eq!(v["@type"], "FAQPage");
        assert_eq!(v["mainEntity"][0]["@type"], "Question");
        // Quotes inside a question must survive as JSON, not break it.
        assert_eq!(v["mainEntity"][0]["name"], "Czy \"latte\" tuczy?");
        assert_eq!(
            v["mainEntity"][0]["acceptedAnswer"]["text"],
            "Nie, bez cukru."
        );
        assert!(script_tag(&f).starts_with("<script type=\"application/ld+json\">"));
    }

    #[test]
    fn markdown_emphasis_does_not_leak_into_the_answer() {
        let md = "## Q?\n\nTo jest **ważne** i `techniczne`.";
        assert_eq!(parse(md)[0].answer, "To jest ważne i techniczne.");
    }
}
