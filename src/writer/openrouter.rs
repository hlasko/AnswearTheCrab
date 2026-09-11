//! OpenRouter as the writing backend.
//!
//! One key reaches models from several vendors, so the model becomes a setting
//! rather than an architectural commitment. The wire format is OpenAI's
//! `/chat/completions`, which means any compatible endpoint works by pointing
//! `OPENROUTER_BASE_URL` elsewhere.
//!
//! Env:
//! * `OPENROUTER_API_KEY` - required; without it drafting stays disabled.
//! * `OPENROUTER_MODEL` - default `anthropic/claude-sonnet-4.5`.
//! * `OPENROUTER_BASE_URL` - default `https://openrouter.ai/api/v1`.

use super::{Kind, Writer};
use crate::domain::{brief_prompt, faq_prompt, Brief};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone)]
pub struct OpenRouter {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenRouter {
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            // Long-form writing legitimately takes a while.
            .timeout(Duration::from_secs(300))
            .build()
            .expect("http client");
        Self {
            client,
            api_key,
            model,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let model = std::env::var("OPENROUTER_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "anthropic/claude-sonnet-4.5".to_string());
        let base = std::env::var("OPENROUTER_BASE_URL")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
        Some(Self::new(key, model, base))
    }
}

#[async_trait::async_trait]
impl Writer for OpenRouter {
    fn name(&self) -> &'static str {
        "openrouter"
    }

    fn model(&self) -> String {
        self.model.clone()
    }

    async fn write(&self, brief: &Brief, kind: Kind) -> anyhow::Result<String> {
        // Answers that get quoted out of context need a tighter brief than an
        // article does, so the two kinds carry different system prompts.
        let system = match kind {
            Kind::Article => {
                "You are a subject-matter writer. You write for readers, \
                 not for search engines: concrete, specific, and honest about \
                 what depends on circumstances. You never pad, never repeat a \
                 phrase to hit a keyword, and never open with a throat-clearing \
                 introduction. Output markdown."
            }
            Kind::Faq => {
                "You answer questions the way a knowledgeable person would when \
                 asked directly: the answer first, then the reason. Every answer \
                 must stand alone, because these get quoted away from the page. \
                 You never pad and never restate the question before answering. \
                 Output markdown."
            }
        };
        let prompt = match kind {
            Kind::Article => brief_prompt(brief),
            Kind::Faq => faq_prompt(brief),
        };
        self.complete(system, &prompt).await
    }

    async fn revise(
        &self,
        brief: &Brief,
        previous: &str,
        instruction: &str,
    ) -> anyhow::Result<String> {
        let system = "You are an editor. You are given a text and an instruction. \
                      Apply the instruction and nothing else: every sentence the \
                      instruction does not touch stays exactly as it was, including its \
                      wording, figures and sources. Return the complete revised text in \
                      markdown, not a summary of changes and not only the changed part.";
        // The brief is included so a revision that adds material has the same
        // ground truth the original was written from, not the model's memory.
        let prompt = format!(
            "## Instruction\n\n{}\n\n## The text to revise\n\n{}\n\n\
             ## Background the text was written from\n\n{}",
            instruction.trim(),
            previous.trim(),
            brief_prompt(brief)
        );
        self.complete(system, &prompt).await
    }

    async fn interpret(&self, title: &str, facts: &str) -> anyhow::Result<String> {
        // The model is handed figures and nothing else: no page, no
        // database, no tools. It can only arrange what the caller measured,
        // so there is nothing for it to invent a number about.
        let system = "You read measured data and say what it means and what to do \
                      next. Rules, in order of importance. Use only the figures \
                      given; never estimate, extrapolate, or introduce a number \
                      that is not in the input. If the data does not support a \
                      conclusion, say what is missing rather than guessing. Quote \
                      the figures that carry your point, with the names they were \
                      given. Be brief: two or three sentences of reading, then two \
                      to four concrete next steps, each naming the topic, phrase or \
                      page it applies to. No preamble, no restatement of what the \
                      data is, no encouragement. Output markdown: a short paragraph \
                      followed by a bulleted list. Answer in the language of the data.";
        let prompt = format!("{title}\n\n{facts}");
        self.complete(system, &prompt).await
    }
}

impl OpenRouter {
    /// One chat completion; shared by writing, revising and interpreting.
    async fn complete(&self, system: &str, user: &str) -> anyhow::Result<String> {
        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
        });

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            // OpenRouter uses these for attribution; harmless elsewhere.
            .header("HTTP-Referer", "https://github.com/answer-the-crab")
            .header("X-Title", "Answer the Crab")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let value: Value = resp.json().await?;

        if !status.is_success() {
            // The useful part is nested; falling back to the whole body would
            // put a wall of JSON in front of the user.
            let msg = value
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            anyhow::bail!("openrouter: {msg} (http {})", status.as_u16());
        }

        // A 200 can still carry an error, e.g. when a model is unavailable.
        if let Some(msg) = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
        {
            anyhow::bail!("openrouter: {msg}");
        }

        let text = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();

        if text.is_empty() {
            anyhow::bail!("openrouter returned an empty draft");
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Competitor, Heading};

    fn brief() -> Brief {
        Brief {
            ai_answers: vec![],
            id: "x".into(),
            topic: "jak odkamienić ekspres".into(),
            language: "pl".into(),
            country: "pl".into(),
            status: "done".into(),
            error: None,
            ai_overview: None,
            ai_sources: vec![],
            questions: vec!["Czy octem można?".into()],
            related: vec![],
            created_at: String::new(),
            competitors: vec![Competitor {
                rank: 1,
                url: "https://e.example".into(),
                domain: "e.example".into(),
                title: None,
                description: None,
                parsed: true,
                headings: vec![Heading {
                    level: 2,
                    title: "Odkamienianie ekspresu".into(),
                }],
                content: None,
                domain_rank: None,
            }],
        }
    }

    async fn serve(body: Value, status: axum::http::StatusCode) -> String {
        let app = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(move || {
                let body = body.clone();
                async move { (status, axum::Json(body)) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn extracts_the_draft() {
        let base = serve(
            json!({"choices": [{"message": {"content": "## Nagłówek\n\nTreść."}}]}),
            axum::http::StatusCode::OK,
        )
        .await;
        let w = OpenRouter::new("k".into(), "m".into(), base);
        assert_eq!(
            w.write(&brief(), Kind::Article).await.unwrap(),
            "## Nagłówek\n\nTreść."
        );
    }

    #[tokio::test]
    async fn reports_the_provider_message_not_raw_json() {
        let base = serve(
            json!({"error": {"message": "Insufficient credits", "code": 402}}),
            axum::http::StatusCode::PAYMENT_REQUIRED,
        )
        .await;
        let w = OpenRouter::new("k".into(), "m".into(), base);
        let e = w
            .write(&brief(), Kind::Article)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("Insufficient credits"), "got: {e}");
        assert!(!e.contains('{'), "must not dump raw JSON: {e}");
    }

    #[tokio::test]
    async fn an_error_inside_a_200_still_fails() {
        // Model routing problems arrive this way rather than as a 4xx.
        let base = serve(
            json!({"error": {"message": "No allowed providers"}, "choices": []}),
            axum::http::StatusCode::OK,
        )
        .await;
        let w = OpenRouter::new("k".into(), "m".into(), base);
        let e = w
            .write(&brief(), Kind::Article)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("No allowed providers"), "got: {e}");
    }

    #[tokio::test]
    async fn an_empty_draft_is_an_error() {
        let base = serve(
            json!({"choices": [{"message": {"content": "   "}}]}),
            axum::http::StatusCode::OK,
        )
        .await;
        let w = OpenRouter::new("k".into(), "m".into(), base);
        assert!(w.write(&brief(), Kind::Article).await.is_err());
    }
}
