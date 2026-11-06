//! Turning a brief into a draft.
//!
//! The trait keeps the model provider swappable; [`openrouter::OpenRouter`] is
//! the first implementation because one key there reaches models from several
//! vendors, which suits a tool where the right model differs per job.

pub mod openrouter;

use crate::domain::Brief;

/// What to write.
///
/// The FAQ is a separate kind rather than a flag on the article because the two
/// have different instructions, different lengths and very different cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Article,
    Faq,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Faq => "faq",
        }
    }
}

impl std::str::FromStr for Kind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "faq" => Ok(Self::Faq),
            "article" => Ok(Self::Article),
            _ => anyhow::bail!("invalid draft kind: {s}"),
        }
    }
}

/// A model that can write from a brief.
#[async_trait::async_trait]
pub trait Writer: Send + Sync {
    fn name(&self) -> &'static str;

    /// The model actually used, for display and for the record.
    fn model(&self) -> String;

    /// Writes a draft of the requested kind. Returns markdown.
    async fn write(&self, brief: &Brief, kind: Kind) -> anyhow::Result<String>;

    /// Revises an existing draft according to an instruction. Returns markdown.
    ///
    /// Separate from `write` because the prompt is different in kind: the
    /// model is handed a text and told what to change, and everything not
    /// mentioned must survive intact.
    async fn revise(
        &self,
        brief: &Brief,
        previous: &str,
        instruction: &str,
    ) -> anyhow::Result<String>;

    /// Reads a block of measured facts and says what to do about them.
    ///
    /// Deliberately not given the page, the database or a tool: the caller
    /// assembles every figure from its own tables and passes them in, so
    /// the model can only arrange what it was handed. Anything it cannot
    /// see, it cannot invent a number for.
    async fn interpret(&self, title: &str, facts: &str) -> anyhow::Result<String>;
}

/// The configured writer, if any.
///
/// Writing is optional: without a key the app still exports the prompt, which
/// is the same instructions rendered for a human to paste elsewhere.
#[derive(Clone)]
pub struct Writers(pub Option<std::sync::Arc<dyn Writer>>);

impl Writers {
    pub fn from_env() -> anyhow::Result<Self> {
        match openrouter::OpenRouter::from_env()? {
            Some(w) => {
                tracing::info!("writer: openrouter ({})", w.model());
                Ok(Self(Some(std::sync::Arc::new(w))))
            }
            None => {
                tracing::info!(
                    "no OPENROUTER_API_KEY, drafting is disabled (prompt export still works)"
                );
                Ok(Self(None))
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn kinds_round_trip() -> anyhow::Result<()> {
        for kind in [Kind::Article, Kind::Faq] {
            assert_eq!(kind.as_str().parse::<Kind>()?.as_str(), kind.as_str());
        }
        Ok(())
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        for value in ["", "blog", "FAQ", " article", "article "] {
            assert!(value.parse::<Kind>().is_err(), "accepted {value:?}");
        }
    }
}
