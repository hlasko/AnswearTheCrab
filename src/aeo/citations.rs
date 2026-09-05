//! Whether a given site is cited by the AI answer, and how that changes.
//!
//! Without this, every AEO technique is a matter of belief. The brief already
//! knows which domains Google cites and which pages rank, so checking one site
//! against both is free.
//!
//! The comparison that matters is cited-versus-ranking, not cited alone.
//! Measured across our briefs, 74% of citations are pages already in the top
//! 10, so "you rank 7th and are not cited" is a content problem, while "you do
//! not rank at all" is a different problem entirely, and the advice differs.

use crate::domain::Brief;

/// How one site fared for one topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationCheck {
    pub domain: String,
    pub cited: bool,
    /// Position among the cited sources, 1-based.
    pub citation_rank: Option<usize>,
    /// Position in the organic results.
    pub organic_rank: Option<i32>,
}

impl CitationCheck {
    /// A plain-language reading of the result, with the next step implied.
    ///
    /// The wording follows from our own measurements rather than from
    /// encouragement: ranking without being cited is the case worth acting on,
    /// and not ranking at all makes AEO work premature.
    pub fn verdict(&self) -> &'static str {
        match (self.cited, self.organic_rank) {
            (true, _) => "Cited by the AI answer.",
            (false, Some(_)) => {
                "Ranking but not cited. This is the gap AEO work addresses: the page is \
                 already found, its answers are not being quoted."
            }
            (false, None) => {
                "Not in the top results for this topic. Citation almost always follows \
                 ranking, so classic visibility comes first here."
            }
        }
    }
}

/// Strips scheme, path and a leading "www." so user input matches stored data.
///
/// People type "https://kawa.pl/blog" or "www.kawa.pl" when they mean the site,
/// and a check that fails on punctuation would look broken.
pub fn normalise_domain(input: &str) -> String {
    let d = input.trim().to_lowercase();
    let d = d
        .strip_prefix("https://")
        .or_else(|| d.strip_prefix("http://"))
        .unwrap_or(&d);
    let d = d.split('/').next().unwrap_or(d);
    let d = d.split('?').next().unwrap_or(d);
    d.strip_prefix("www.").unwrap_or(d).trim().to_string()
}

/// Checks one site against a brief's citations and rankings.
pub fn check(brief: &Brief, domain: &str) -> CitationCheck {
    let want = normalise_domain(domain);

    let citation_rank = brief
        .ai_sources
        .iter()
        .position(|s| normalise_domain(s) == want)
        .map(|i| i + 1);

    let organic_rank = brief
        .competitors
        .iter()
        .find(|c| normalise_domain(&c.domain) == want)
        .map(|c| c.rank);

    CitationCheck {
        domain: want,
        cited: citation_rank.is_some(),
        citation_rank,
        organic_rank,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Competitor;

    fn brief(sources: &[&str], comps: &[(i32, &str)]) -> Brief {
        Brief {
            id: "x".into(),
            topic: "czy kawa jest zdrowa".into(),
            language: "pl".into(),
            country: "pl".into(),
            status: "done".into(),
            error: None,
            ai_overview: Some("Kawa zawiera kofeinę.".into()),
            ai_sources: sources.iter().map(|s| s.to_string()).collect(),
            questions: vec![],
            related: vec![],
            created_at: String::new(),
            competitors: comps
                .iter()
                .map(|(rank, domain)| Competitor {
                    rank: *rank,
                    url: format!("https://{domain}/a"),
                    domain: (*domain).to_string(),
                    title: None,
                    description: None,
                    parsed: true,
                    headings: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn finds_the_site_however_it_was_typed() {
        // Real cited domains from the database carry the "www." prefix.
        let b = brief(&["www.dietetycy.org.pl", "kawa.pl"], &[(3, "kawa.pl")]);
        for typed in [
            "kawa.pl",
            "www.kawa.pl",
            "https://kawa.pl",
            "https://www.kawa.pl/blog/artykul",
            "  KAWA.PL  ",
        ] {
            let c = check(&b, typed);
            assert!(c.cited, "should match citation for input: {typed:?}");
            assert_eq!(c.citation_rank, Some(2));
            assert_eq!(c.organic_rank, Some(3));
        }
    }

    #[test]
    fn ranking_without_a_citation_is_the_actionable_case() {
        // The case our own data says is most common and most fixable.
        let b = brief(&["www.dietetycy.org.pl"], &[(7, "kawa.pl")]);
        let c = check(&b, "kawa.pl");
        assert!(!c.cited);
        assert_eq!(c.organic_rank, Some(7));
        assert!(c.verdict().contains("Ranking but not cited"));
    }

    #[test]
    fn absent_entirely_is_a_ranking_problem_not_an_aeo_one() {
        let b = brief(&["www.dietetycy.org.pl"], &[(1, "inny.pl")]);
        let c = check(&b, "kawa.pl");
        assert!(!c.cited);
        assert_eq!(c.organic_rank, None);
        assert!(c.verdict().contains("Not in the top results"));
    }
}
