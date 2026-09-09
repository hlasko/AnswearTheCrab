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

    pub fn from_str(s: &str) -> Self {
        match s {
            "faq" => Self::Faq,
            _ => Self::Article,
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
}

/// The configured writer, if any.
///
/// Writing is optional: without a key the app still exports the prompt, which
/// is the same instructions rendered for a human to paste elsewhere.
#[derive(Clone)]
pub struct Writers(pub Option<std::sync::Arc<dyn Writer>>);

impl Writers {
    pub fn from_env() -> Self {
        match openrouter::OpenRouter::from_env() {
            Some(w) => {
                tracing::info!("writer: openrouter ({})", w.model());
                Self(Some(std::sync::Arc::new(w)))
            }
            None => {
                tracing::info!(
                    "no OPENROUTER_API_KEY, drafting is disabled (prompt export still works)"
                );
                Self(None)
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }
}
