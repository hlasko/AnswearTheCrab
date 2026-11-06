# Prompt discovery and local Radar

These are two different workflows: discovery chooses which prompts to ask next;
Radar searches answers the application has already collected. Neither exposes
private conversations from ChatGPT or any other assistant.

## Phase 1: approve a reproducible prompt set

Open a finished search and find **Ask the answer engine**. Review the candidate
prompts, inspect their provenance, and choose up to 20. Approval stores a
snapshot of the wording and provenance. It does not call an external API.

The candidates come from harvested suggestions, not from an LLM inventing
customer questions. Question, comparison, alternative and recommendation
intents are detected with explicit language heuristics. Equivalent variants are
merged conservatively. This is not a general embedding-based semantic matcher:
different wording can still need human review, and genuinely different questions
must not disappear merely because they share a topic.

The recommended set balances intents. Metrics are evidence for prioritisation,
not interchangeable measures:

- Google search volume measures Google demand, not assistant prompt frequency.
- Bing volume must not be presented as Google volume.
- Asked is the provider's modelled question-demand signal, not observed counts
  of private AI conversations.
- Unknown values stay unknown. They are not zeros.

Click **Approve this prompt set**, then explicitly run the approved prompts.
Running sends billable requests to the configured provider. The application
loads the stored approval server-side instead of trusting a client-supplied
list of prompts.

Approval is scoped to topic, language, country and search source. A new harvest
of that same scope reuses the exact approved wording. This makes repeat checks
comparable rather than silently changing the questions being measured.
Changing a checkbox is an unsaved edit, not a change to the stored schedule.
Restore the approved set or approve the replacement before running it.

Scheduled AI checks also require approval. Existing watches without an approved
set skip the AI step until the user approves one. The normal keyword-harvest
schedule remains independent and can still incur its usual provider charges.

## Phase 2: inspect the local answer corpus

The **Radar** page accepts your domain or a competitor's domain. It searches
recorded monitoring answers, Perplexity answers kept in briefs, and Google AI
Overviews kept in briefs. Searching this corpus does not call a provider or
queue work.

Radar separates two kinds of evidence:

- **Recorded source**: the domain appears in the stored source list.
- **Text mention**: the domain appears in the answer text.

These are not interchangeable with an endorsement, recommendation position or
verified citation. The existing Perplexity integration combines annotation URLs
with web-search-result URLs. Older stored rows cannot establish which kind each
URL was. Radar must not manufacture that distinction or call source-list order
a recommendation rank. AI Overviews currently retain source domains, not the
exact cited page URL.

Coverage is only what this installation has saved. The interface reports the
available corpus, the bounded sample it inspected, matching records, distinct
prompt texts, and the display limit. Repeated observations count as separate
records, not new unique prompts. No counts are extrapolated into global reach,
AI query volume or market share. An empty result means no match in the inspected
sample, not that an assistant never mentions the site.

The app is still single-user. Radar searches this installation's local corpus;
adding multi-user accounts requires tenant scoping before shared deployment.

## Validation and operating boundaries

Normal Rust tests use local fixtures or mocks rather than live paid providers.
Database tests must use isolated tables/schemas in a test database. For a UI
acceptance run, use a separate database and explicitly unset provider access by
setting `DATAFORSEO_LOGIN=`, `DATAFORSEO_PASSWORD=` and `OPENROUTER_API_KEY=` in
the server environment. Do not run the user's production watch queue as part of
a UI test.

Standard checks:

```bash
cargo test --offline --no-default-features --features ssr
cargo leptos build
PROMPT_TEST_DATABASE_URL=postgresql:///atp_feature_test \
  cargo test --test prompt_discovery -- --ignored
RADAR_TEST_DATABASE_URL=postgresql:///atp_feature_test \
  cargo test --test radar -- --ignored
```

For the browser smoke test, first create a **new, empty** `atp_feature_test`
database. Apply `migrations/*.sql` in order, then
`tests/fixtures/discovery_radar.sql`. Start the built server against that database
with the empty provider variables above, `LEPTOS_SITE_ADDR=127.0.0.1:3017`,
`LEPTOS_OUTPUT_NAME=atp` and `LEPTOS_SITE_ROOT=target/site`. With the existing
Playwright dependency in `scripts/` installed, run:

```bash
ATP_SMOKE_ALLOW_FIXTURES=1 node scripts/feature-smoke.mjs
```

The smoke test approves only the synthetic fixture's prompts. It never clicks
the paid run button. It checks hydration, approval/reload/restore behavior,
provenance, domain and subdomain matching, engine and country filters, empty and
error states, and mobile overflow. `ATP_SMOKE_OUTPUT` optionally selects a
directory for screenshots. Stop the test server when finished.
