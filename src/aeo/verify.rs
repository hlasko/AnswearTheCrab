//! Which of a draft's claims can be traced to a page that ranks.
//!
//! The writing prompt pushes for figures and named sources, because the GEO
//! benchmark found those raise visibility. The risk is obvious: a model asked
//! for statistics will produce statistics, and the ones it produces may be
//! invented. A draft full of confident, sourced, wrong numbers is worse than a
//! vague one, because it reads as authoritative.
//!
//! This checks each figure and each attribution against the text of the pages
//! that rank for the topic plus Google's own answer, all of which we hold. A
//! figure found there is not proven true, but it is at least a figure the
//! reader can find elsewhere; one found nowhere is flagged for a human to
//! check before publishing.

use std::collections::BTreeSet;

/// One claim and whether the corpus backs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The figure or the attributed sentence, as written in the draft.
    pub text: String,
    pub kind: ClaimKind,
    /// Where in the corpus it was found: a domain, "AI overview", or nothing.
    pub found_in: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// A number with context: "400 mg", "15-20%", "3-4 filiżanki".
    Figure,
    /// A sentence naming a source: "według badań", "WHO zaleca".
    Attribution,
}

/// Summary over a draft.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verification {
    pub claims: Vec<Claim>,
}

impl Verification {
    pub fn total(&self) -> usize {
        self.claims.len()
    }
    pub fn backed(&self) -> usize {
        self.claims.iter().filter(|c| c.found_in.is_some()).count()
    }
    pub fn unbacked(&self) -> Vec<&Claim> {
        self.claims
            .iter()
            .filter(|c| c.found_in.is_none())
            .collect()
    }
    /// Share of claims found somewhere, 0-100.
    pub fn share(&self) -> usize {
        if self.claims.is_empty() {
            0
        } else {
            self.backed() * 100 / self.claims.len()
        }
    }
}

/// A source the draft can be checked against.
pub struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

/// Words that mark a sentence as attributing a claim to someone.
const ATTRIBUTION: &[&str] = &[
    "według",
    "wedle",
    "zdaniem",
    "badani",
    "badań",
    "naukowc",
    "ekspert",
    "instytut",
    "raport",
    "wytyczn",
    "according",
    "study",
    "studies",
    "research",
    "report",
    "survey",
    "guideline",
    "who ",
    "efsa",
    "fda",
    "nhs",
    "usda",
    "gus ",
    "nbp",
    "knf",
    "bik",
];

/// Pulls figures out of a sentence: numbers with their unit or the word after.
///
/// A bare digit is not a claim ("krok 2"), so a figure keeps its neighbour:
/// "400 mg", "15-20%", "3 filiżanki". Years and list numbers are skipped.
fn figures(sentence: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Polish writes thousands with a space: "1 100 zł", "300 000 zł". Join a
    // digit-only token to a following three-digit token before scanning, or
    // "1 100 zł" reads as two figures, "1" and "100 zł".
    // Ranges come as "2 800–3 000" too, so the joining must see through a
    // dash: the token "800–3" ends in digits and "000" continues it.
    let raw: Vec<&str> = sentence.split_whitespace().collect();
    let mut joined: Vec<String> = Vec::new();
    let mut i = 0;
    let numeric = |t: &str| {
        !t.is_empty()
            && t.chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | '–' | ',' | '.'))
    };
    while i < raw.len() {
        let mut cur = raw[i].to_string();
        // The next token continues this number when it starts with exactly
        // three digits: "000", "000zł", "800–3" (a range whose second half
        // then continues on the following token).
        let continues = |t: &str| {
            let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.len() == 3 && numeric(t.trim_end_matches(|c: char| c.is_alphabetic()))
        };
        while i + 1 < raw.len()
            && numeric(&cur)
            && cur.ends_with(|c: char| c.is_ascii_digit())
            && continues(raw[i + 1])
        {
            cur.push_str(raw[i + 1]);
            i += 1;
        }
        joined.push(cur);
        i += 1;
    }
    let tokens: Vec<&str> = joined.iter().map(String::as_str).collect();
    for (i, tok) in tokens.iter().enumerate() {
        let core = tok.trim_matches(|c: char| {
            !c.is_ascii_digit() && c != '%' && c != ',' && c != '.' && c != '-' && c != '–'
        });
        if core.is_empty() || !core.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // Years read as figures but are never a statistic worth checking.
        if core.len() == 4 && core.starts_with("20") || core.starts_with("19") {
            continue;
        }
        // "1." at the start of a line is a list marker.
        if i == 0 && core.ends_with('.') {
            continue;
        }
        let unit = if tok.contains('%') {
            "%".to_string()
        } else {
            tokens
                .get(i + 1)
                .map(|u| {
                    u.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                // "450 do 550": the word after a number is not always its
                // unit. Connectives and articles are dropped so the figure
                // stands alone rather than as "450 do".
                .filter(|u| {
                    !matches!(
                        u.as_str(),
                        "do" | "i"
                            | "a"
                            | "lub"
                            | "oraz"
                            | "to"
                            | "and"
                            | "or"
                            | "of"
                            | "w"
                            | "na"
                            | "z"
                            | "od"
                    )
                })
                .unwrap_or_default()
        };
        let figure = if unit.is_empty() || unit == "%" {
            core.trim_end_matches('.').to_string()
        } else {
            format!("{} {}", core.trim_end_matches('.'), unit)
        };
        out.push(figure);
    }
    out
}

/// Whether a figure appears in a text, allowing the usual variants.
///
/// "15-20%" must match "15–20%" and "15 - 20 %"; "1 100 zł" must match "1100
/// zł" and "1.100 zł". Comparison is on the digits and unit with separators
/// removed, which is loose enough to find a real match and tight enough that
/// "400" does not match "1400".
fn normalise(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '–' | '-' => out.push('-'),
            ' ' | '\u{a0}' | '.' | ',' => {}
            c => out.push(c.to_ascii_lowercase()),
        }
    }
    out
}

fn figure_in(text_norm: &str, figure: &str) -> bool {
    let f = normalise(figure);
    if f.is_empty() {
        return false;
    }
    // Require a non-digit boundary before the figure so 400 does not hit 1400.
    let mut start = 0;
    while let Some(pos) = text_norm[start..].find(&f) {
        let at = start + pos;
        let before = text_norm[..at].chars().last();
        if !before.is_some_and(|c| c.is_ascii_digit()) {
            return true;
        }
        start = at + 1;
    }
    false
}

/// Content words of a sentence, for matching attributions loosely.
fn key_words(s: &str) -> BTreeSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 4)
        .map(|w| w.chars().take(6).collect())
        .collect()
}

/// Checks every figure and attribution in the draft against the sources.
pub fn verify(draft: &str, sources: &[Source<'_>]) -> Verification {
    let norms: Vec<(&str, String)> = sources
        .iter()
        .map(|s| (s.name, normalise(s.text)))
        .collect();
    let lowers: Vec<(&str, String)> = sources
        .iter()
        .map(|s| (s.name, s.text.to_lowercase()))
        .collect();

    let mut claims = Vec::new();
    let mut seen_figures: BTreeSet<String> = BTreeSet::new();

    for sentence in draft.split(|c| matches!(c, '.' | '!' | '?' | '\n')) {
        let sentence = sentence.trim();
        if sentence.chars().count() < 20 || sentence.starts_with('#') {
            continue;
        }

        for fig in figures(sentence) {
            if !seen_figures.insert(fig.clone()) {
                continue;
            }
            let found_in = norms
                .iter()
                .find(|(_, t)| figure_in(t, &fig))
                .map(|(n, _)| n.to_string());
            claims.push(Claim {
                text: fig,
                kind: ClaimKind::Figure,
                found_in,
            });
        }

        let lower = sentence.to_lowercase();
        if ATTRIBUTION.iter().any(|w| lower.contains(w)) {
            // An attribution is backed when a source shares most of its
            // content words: the study name, the body, the subject.
            let words = key_words(sentence);
            let needed = (words.len() * 6).div_ceil(10).max(2);
            let found_in = lowers
                .iter()
                .find(|(_, t)| words.iter().filter(|w| t.contains(w.as_str())).count() >= needed)
                .map(|(n, _)| n.to_string());
            claims.push(Claim {
                text: sentence.to_string(),
                kind: ClaimKind::Attribution,
                found_in,
            });
        }
    }

    Verification { claims }
}

#[cfg(test)]
mod realdata {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a database; run with --ignored"]
    async fn verify_the_newest_real_draft() {
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".into());
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let (bid, content): (uuid::Uuid, String) = sqlx::query_as(
            "select brief_id, content from drafts where status='done' and kind='article'
              order by created_at desc limit 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let comps: Vec<(String, String)> = sqlx::query_as(
            "select domain, content from brief_competitors where brief_id=$1 and content is not null",
        )
        .bind(bid)
        .fetch_all(&pool)
        .await
        .unwrap();
        let ai: Option<String> = sqlx::query_scalar("select ai_overview from briefs where id=$1")
            .bind(bid)
            .fetch_one(&pool)
            .await
            .unwrap();
        let mut sources: Vec<Source> = comps
            .iter()
            .map(|(d, t)| Source { name: d, text: t })
            .collect();
        if let Some(a) = &ai {
            sources.push(Source {
                name: "AI overview",
                text: a,
            });
        }
        let v = verify(&content, &sources);
        let figs = v
            .claims
            .iter()
            .filter(|c| c.kind == ClaimKind::Figure)
            .count();
        let atts = v
            .claims
            .iter()
            .filter(|c| c.kind == ClaimKind::Attribution)
            .count();
        println!(
            "\nclaims={} (figures={figs}, attributions={atts}) backed={} ({}%)",
            v.total(),
            v.backed(),
            v.share()
        );
        println!("BACKED sample:");
        for c in v.claims.iter().filter(|c| c.found_in.is_some()).take(6) {
            println!(
                "  [{}] {}",
                c.found_in.as_deref().unwrap(),
                c.text.chars().take(70).collect::<String>()
            );
        }
        println!("UNBACKED:");
        for c in v.unbacked().iter().take(12) {
            println!("  ?? {}", c.text.chars().take(90).collect::<String>());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_present_in_a_competitor_is_backed() {
        let draft = "Bezpieczna dawka kofeiny to 400 mg dziennie dla dorosłych.";
        let src = [Source {
            name: "efsa.europa.eu",
            text: "EFSA uznaje 400 mg kofeiny dziennie za bezpieczne.",
        }];
        let v = verify(draft, &src);
        assert_eq!(v.total(), 1);
        assert_eq!(v.claims[0].text, "400 mg");
        assert_eq!(v.claims[0].found_in.as_deref(), Some("efsa.europa.eu"));
    }

    #[test]
    fn a_figure_found_nowhere_is_flagged() {
        let draft = "Kawa obniża ryzyko cukrzycy o 37% według metaanalizy z 2014 roku.";
        let src = [Source {
            name: "x.pl",
            text: "Kawa może obniżać ryzyko cukrzycy typu 2.",
        }];
        let v = verify(draft, &src);
        let unbacked: Vec<&str> = v.unbacked().iter().map(|c| c.text.as_str()).collect();
        assert!(
            unbacked.contains(&"37%"),
            "invented percentage must be flagged: {unbacked:?}"
        );
        // The year is not a claim.
        assert!(!v.claims.iter().any(|c| c.text.starts_with("2014")));
    }

    #[test]
    fn separators_and_dashes_do_not_hide_a_match() {
        let draft = "Rata wynosi 1 100 zł, a zysk 15-20% rocznie.";
        let src = [Source {
            name: "bank.pl",
            text: "Miesięczna rata: 1100 zł. Zwrot na poziomie 15–20 % w skali roku.",
        }];
        let v = verify(draft, &src);
        assert_eq!(
            v.backed(),
            v.total(),
            "all should match: {:?}",
            v.unbacked()
        );
    }

    #[test]
    fn ranges_with_thousand_separators_stay_whole() {
        // Real output from a draft: "2 800–3 000 zł" was read as three
        // fragments, "2 800–3", "800–3 000" and "000 zł".
        let f = figures("Rata wynosi 2 800–3 000 zł miesięcznie, a wkład 450 do 550 tysięcy.");
        assert!(
            f.contains(&"2800-3000 zł".to_string()) || f.contains(&"2800–3000 zł".to_string()),
            "{f:?}"
        );
        assert!(
            !f.iter().any(|x| x.starts_with("000")),
            "no orphan fragments: {f:?}"
        );
        assert!(
            !f.iter().any(|x| x.ends_with(" do")),
            "connective is not a unit: {f:?}"
        );
    }

    #[test]
    fn four_hundred_does_not_match_fourteen_hundred() {
        let draft = "Limit to 400 mg dziennie.";
        let src = [Source {
            name: "x",
            text: "Niektórzy przekraczają 1400 mg tygodniowo.",
        }];
        assert_eq!(verify(draft, &src).backed(), 0);
    }

    #[test]
    fn an_attribution_is_backed_when_the_source_says_the_same_thing() {
        let draft = "Według Narodowego Centrum Edukacji Żywieniowej kawa wspiera pracę wątroby.";
        let backed = [Source {
            name: "ncez.pzh.gov.pl",
            text: "Narodowe Centrum Edukacji Żywieniowej: regularna kawa wspiera wątrobę.",
        }];
        let v = verify(draft, &backed);
        let att: Vec<&Claim> = v
            .claims
            .iter()
            .filter(|c| c.kind == ClaimKind::Attribution)
            .collect();
        assert_eq!(att.len(), 1);
        assert!(
            att[0].found_in.is_some(),
            "same body, same subject: {:?}",
            att[0]
        );

        let unrelated = [Source {
            name: "y",
            text: "Kawa pobudza dzięki kofeinie.",
        }];
        let v = verify(draft, &unrelated);
        assert!(v
            .claims
            .iter()
            .any(|c| c.kind == ClaimKind::Attribution && c.found_in.is_none()));
    }
}
