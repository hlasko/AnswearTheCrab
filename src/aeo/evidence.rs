//! How much verifiable evidence a draft carries.
//!
//! The GEO paper (Aggarwal et al., KDD 2024) measured nine content strategies
//! against a no-optimisation baseline of 19.3. Quotation Addition scored 27.2,
//! Statistics Addition 25.2 and Cite Sources 24.6, while Keyword Stuffing came
//! in at 17.7, below leaving the page alone. So this module counts evidence and
//! never counts keywords.
//!
//! What to enforce was decided by measuring our own output rather than by
//! adopting all three findings blindly. Across the drafts in the database, 64%
//! of sentences already carry a number and 34% carry a number with a unit,
//! about twice the density of Google's own AI answers. Named sources appeared
//! seven times in total, and direct quotations exactly zero times. Statistics
//! were therefore already handled; attribution and quotation were the real gap,
//! and that is where the prompt now pushes.

/// Evidence found in a piece of writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Evidence {
    /// Sentences long enough to make a claim.
    pub sentences: usize,
    /// Sentences containing a figure.
    pub with_number: usize,
    /// Sentences with a figure and a unit: "92°C", "400 mg", "3-4 filiżanki".
    /// A bare number is weaker evidence than a measured one.
    pub with_unit: usize,
    /// Passages in quotation marks.
    pub quotations: usize,
    /// Claims attributed to someone: "według", "badanie", "WHO".
    pub attributions: usize,
}

impl Evidence {
    /// Share of sentences carrying a figure, 0-100.
    pub fn number_share(&self) -> usize {
        if self.sentences == 0 {
            return 0;
        }
        self.with_number * 100 / self.sentences
    }

    /// Whether the draft is thin on the two things our drafts actually lacked.
    ///
    /// Deliberately not a score out of 100: a number invites gaming, and the
    /// GEO result that matters here is qualitative, namely that attributed and
    /// quoted material gets picked up while keyword density does not.
    pub fn needs_attribution(&self) -> bool {
        self.sentences > 20 && self.attributions == 0 && self.quotations == 0
    }
}

/// Words that mark a claim as coming from somewhere checkable.
const ATTRIBUTION: &[&str] = &[
    // Polish
    "według",
    "wedle",
    "zdaniem",
    "badanie",
    "badania",
    "badaniach",
    "naukowcy",
    "ekspert",
    "eksperci",
    "instytut",
    "raport",
    "analiza",
    "metaanaliza",
    "przegląd",
    "wytyczne",
    "norma",
    // English
    "according",
    "study",
    "studies",
    "research",
    "researchers",
    "report",
    "survey",
    "analysis",
    "guidelines",
    "trial",
    // Bodies that appear by name
    "who",
    "efsa",
    "fda",
    "nhs",
    "usda",
];

/// Units that turn a bare figure into a measurement.
const UNITS: &[&str] = &[
    "kcal", "kj", "mg", "kg", "ml", "cl", "dl", "°c", "°f", "min", "godz", "sek", "zł", "eur",
    "usd", "%", "cm", "mm", "km", "hz", "rpm", "bar",
];

/// Splits into sentences long enough to carry a claim.
fn sentences(text: &str) -> Vec<&str> {
    text.split(|c| matches!(c, '.' | '!' | '?' | '\n'))
        .map(str::trim)
        // Headings and list labels are not claims; 25 characters is where real
        // sentences start in the drafts we have.
        .filter(|s| s.chars().count() > 25)
        .collect()
}

/// Measures the evidence in a draft.
pub fn evidence(text: &str) -> Evidence {
    let sents = sentences(text);
    let lower = text.to_lowercase();

    let mut e = Evidence {
        sentences: sents.len(),
        ..Default::default()
    };

    for s in &sents {
        let l = s.to_lowercase();
        let has_digit = l.chars().any(|c| c.is_ascii_digit());
        if has_digit {
            e.with_number += 1;
            if UNITS.iter().any(|u| l.contains(u)) {
                e.with_unit += 1;
            }
        }
    }

    // Quotation marks vary by language: Polish uses „ ", English " ".
    e.quotations = count_quotations(text);

    e.attributions = ATTRIBUTION
        .iter()
        .map(|w| {
            lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|t| t == w)
                .count()
        })
        .sum();

    e
}

/// Counts quoted passages, accepting the marks used in the languages we write.
fn count_quotations(text: &str) -> usize {
    let mut n = 0;
    let mut open: Option<char> = None;
    let mut len = 0usize;

    for c in text.chars() {
        match open {
            None => {
                if matches!(c, '„' | '“' | '«' | '"') {
                    open = Some(c);
                    len = 0;
                }
            }
            Some(o) => {
                let closes = match o {
                    '„' => matches!(c, '”' | '“' | '"'),
                    '“' => matches!(c, '”' | '"'),
                    '«' => c == '»',
                    _ => c == '"',
                };
                if closes {
                    // Short marks are usually scare quotes around a term, not a
                    // quotation of a source.
                    if len > 20 {
                        n += 1;
                    }
                    open = None;
                } else {
                    len += 1;
                }
            }
        }
    }
    n
}

#[cfg(test)]
mod realdata {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a database; run with --ignored to measure real drafts"]
    async fn evidence_in_every_real_draft() {
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".into());
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "select b.topic, d.kind, d.content from drafts d join briefs b on b.id = d.brief_id
              where d.status = 'done'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        for (topic, kind, content) in rows {
            let Some(c) = content else { continue };
            let e = evidence(&c);
            if e.sentences < 20 {
                continue;
            }
            let q = crate::aeo::quotability(&c);
            println!(
                "{topic} [{kind}]: zdan={} liczby={}% cytaty={} zrodla={} | sekcje={} samodzielne={} ({}%)",
                e.sentences,
                e.number_share(),
                e.quotations,
                e.attributions,
                q.total(),
                q.standalone(),
                q.share()
            );
            for w in q.weak().iter().take(3) {
                println!(
                    "     SLABE: [{}] {} -> {}",
                    w.heading,
                    &w.opening.chars().take(50).collect::<String>(),
                    w.problem.unwrap()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_measurements_not_bare_digits() {
        let text = "Woda powinna mieć temperaturę 92-95°C przy parzeniu przelewowym. \
                    Ten sposób jest opisany w wielu poradnikach dla początkujących.";
        let e = evidence(text);
        assert_eq!(e.sentences, 2);
        assert_eq!(e.with_number, 1);
        assert_eq!(e.with_unit, 1, "°C makes it a measurement");
    }

    #[test]
    fn finds_attribution_in_both_languages() {
        let pl = evidence("Według badania opublikowanego przez EFSA dawka jest bezpieczna.");
        assert!(pl.attributions >= 2, "według + badania + EFSA: {pl:?}");

        let en = evidence("According to a study by the WHO the dose is considered safe.");
        assert!(en.attributions >= 2, "according + study + WHO: {en:?}");
    }

    #[test]
    fn a_quoted_passage_counts_but_a_scare_quote_does_not() {
        let quoted = evidence(
            "Autor wyjaśnia: „Kofeina działa na receptory adenozynowe w mózgu i to \
             tłumaczy jej efekt pobudzający”.",
        );
        assert_eq!(quoted.quotations, 1);

        let scare = evidence("Tak zwana „mocna kawa” to pojęcie bardzo względne w praktyce.");
        assert_eq!(scare.quotations, 0, "a single term is not a quotation");
    }

    #[test]
    fn flags_a_draft_with_no_verifiable_source() {
        // Shaped like our real drafts: full of numbers, empty of attribution.
        // Measured across the drafts in the database, that was the actual state:
        // 64% of sentences carried a number, quotations were zero.
        let mut text = String::new();
        for i in 0..25 {
            text.push_str(&format!(
                "Ta porcja zawiera około {} kcal w zależności od rodzaju mleka użytego w napoju. ",
                90 + i
            ));
        }
        let e = evidence(&text);
        assert!(e.number_share() > 90);
        assert!(
            e.needs_attribution(),
            "numbers without a source are still unverifiable: {e:?}"
        );

        let sourced = format!("{text} Według wytycznych EFSA dawka jest bezpieczna.");
        assert!(!evidence(&sourced).needs_attribution());
    }
}
