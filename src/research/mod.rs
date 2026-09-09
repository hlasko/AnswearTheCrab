//! Research sources for the Content Writer.
//!
//! A source contributes findings about one topic into a shared [`Findings`]
//! struct. The brief job runs every configured source and merges the results, so
//! adding a new kind of research (AEO/GEO, a competitor's sitemap, an internal
//! knowledge base) means adding one implementation here and nothing else.
//!
//! This mirrors `providers::SuggestionProvider`, which already proved the shape:
//! YouTube and Bing were each a single file once the trait existed.

pub mod serp;

use crate::domain::Competitor;

/// What one source learned about a topic.
///
/// Every field is optional in practice: sources fill in what they can and leave
/// the rest empty, and the merge below is additive. A missing AI overview or an
/// unparseable competitor is a normal outcome, not an error.
#[derive(Debug, Default, Clone)]
pub struct Findings {
    pub ai_overview: Option<String>,
    pub ai_sources: Vec<String>,
    pub questions: Vec<String>,
    pub related: Vec<String>,
    pub competitors: Vec<Competitor>,
}

impl Findings {
    /// Folds another source's findings into this one.
    ///
    /// First non-empty value wins for the singular fields, so source order
    /// expresses priority. Lists concatenate while skipping duplicates.
    pub fn merge(&mut self, other: Findings) {
        if self.ai_overview.is_none() {
            self.ai_overview = other.ai_overview;
        }
        for s in other.ai_sources {
            if !self.ai_sources.contains(&s) {
                self.ai_sources.push(s);
            }
        }
        for q in other.questions {
            if !self.questions.contains(&q) {
                self.questions.push(q);
            }
        }
        for r in other.related {
            if !self.related.contains(&r) {
                self.related.push(r);
            }
        }
        for c in other.competitors {
            if !self.competitors.iter().any(|x| x.url == c.url) {
                self.competitors.push(c);
            }
        }
    }
}

/// A place to learn about a topic.
#[async_trait::async_trait]
pub trait ResearchSource: Send + Sync {
    fn name(&self) -> &'static str;

    async fn research(
        &self,
        topic: &str,
        language: &str,
        country: &str,
    ) -> anyhow::Result<Findings>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn competitor(url: &str) -> Competitor {
        Competitor {
            rank: 1,
            url: url.into(),
            domain: "example.com".into(),
            title: None,
            description: None,
            headings: vec![],
            parsed: false,
            content: None,
        }
    }

    #[test]
    fn merge_is_additive_and_deduplicates() {
        let mut a = Findings {
            questions: vec!["how to descale".into()],
            competitors: vec![competitor("https://a.example")],
            ..Default::default()
        };
        a.merge(Findings {
            ai_overview: Some("answer".into()),
            ai_sources: vec!["a.example".into(), "a.example".into()],
            questions: vec!["how to descale".into(), "how often".into()],
            competitors: vec![
                competitor("https://a.example"),
                competitor("https://b.example"),
            ],
            ..Default::default()
        });

        assert_eq!(a.ai_overview.as_deref(), Some("answer"));
        assert_eq!(a.ai_sources, ["a.example"]);
        assert_eq!(a.questions, ["how to descale", "how often"]);
        assert_eq!(a.competitors.len(), 2);
    }

    #[test]
    fn first_source_wins_for_singular_fields() {
        // Source order expresses priority, so a later source must not overwrite
        // an answer an earlier one already provided.
        let mut a = Findings {
            ai_overview: Some("first".into()),
            ..Default::default()
        };
        a.merge(Findings {
            ai_overview: Some("second".into()),
            ..Default::default()
        });
        assert_eq!(a.ai_overview.as_deref(), Some("first"));
    }
}
