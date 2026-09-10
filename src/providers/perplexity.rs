//! Perplexity autocomplete, through a headless browser.
//!
//! Perplexity publishes no suggest API. Its REST route answers 403 behind
//! Cloudflare to anything that is not a browser, and the suggestions arrive
//! over a WebSocket the page opens after the Cloudflare check. So this
//! provider shells out to `scripts/perplexity-suggest.mjs`, a Playwright
//! script that types every probe into the real search box and reads the
//! frames. Measured: 56 probes, 187 unique phrases, 63 seconds.
//!
//! What comes back is different from Google's list, which is the point:
//! "czy kredyt hipoteczny można odliczyć od podatku", "jaki kredyt
//! hipoteczny przy zarobkach 7000 netto", "kredyt hipoteczny bez karty
//! pobytu". People ask an assistant in full sentences with their own
//! situation in them. No volume exists for these; Google's is attached
//! afterwards in the job where a phrase overlaps.

use super::{dedupe, probes, SuggestionProvider};
use crate::domain::{classify_with, vocabulary, Suggestion};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Perplexity {
    script: PathBuf,
}

impl Perplexity {
    /// Finds the script relative to the working directory, which is the
    /// repo root under `cargo leptos` and `./run.sh`. Overridable with
    /// `PERPLEXITY_SCRIPT` for deployments that put it elsewhere.
    pub fn from_env() -> Self {
        let script = std::env::var("PERPLEXITY_SCRIPT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("scripts/perplexity-suggest.mjs"));
        Self { script }
    }

    pub fn available(&self) -> bool {
        self.script.exists()
            && self
                .script
                .parent()
                .map(|d| d.join("node_modules/playwright").exists())
                .unwrap_or(false)
    }

    async fn run(
        &self,
        queries: &[String],
        language: &str,
    ) -> anyhow::Result<HashMap<String, Vec<String>>> {
        anyhow::ensure!(
            self.available(),
            "perplexity needs {} and `npm install` in scripts/",
            self.script.display()
        );
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("probes.json");
        std::fs::write(&path, serde_json::to_vec(queries)?)?;

        // The page's locale steers which market's suggestions come back.
        let locale = match language {
            "pl" => "pl-PL",
            "de" => "de-DE",
            "fr" => "fr-FR",
            "es" => "es-ES",
            _ => "en-US",
        };
        let out = tokio::process::Command::new("node")
            .arg(&self.script)
            .arg(&path)
            .env("PPLX_LOCALE", locale)
            .output()
            .await?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        for line in stderr.lines() {
            tracing::info!("{line}");
        }
        anyhow::ensure!(
            out.status.success(),
            "perplexity script failed: {}",
            stderr.lines().last().unwrap_or("no output")
        );
        Ok(serde_json::from_slice(&out.stdout)?)
    }
}

#[async_trait::async_trait]
impl SuggestionProvider for Perplexity {
    fn name(&self) -> &'static str {
        "perplexity-suggest"
    }

    async fn harvest(
        &self,
        keyword: &str,
        language: &str,
        _country: &str,
    ) -> anyhow::Result<Vec<Suggestion>> {
        let probes = probes(keyword, language);
        let queries: Vec<String> = probes.iter().map(|p| p.query.clone()).collect();
        let by_query = self.run(&queries, language).await?;

        let mut items = Vec::new();
        for p in &probes {
            for text in by_query.get(&p.query).into_iter().flatten() {
                items.push(Suggestion::new(
                    text.clone(),
                    p.category,
                    p.modifier.clone(),
                ));
            }
        }
        let items = dedupe(keyword, items);

        // Same as the other suggest sources: the probe is a hint, the phrase
        // decides the category.
        let vocab = vocabulary(language);
        Ok(items
            .into_iter()
            .map(|mut s| {
                let (cat, modifier) = classify_with(&s.text, keyword, vocab);
                s.category = cat.as_str().to_string();
                s.modifier = modifier;
                s
            })
            .collect())
    }
}
