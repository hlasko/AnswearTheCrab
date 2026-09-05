# answer the crab

An AnswerThePublic clone in Rust: **Axum + Leptos (SSR/hydrate) + Apalis + Postgres**.

Type a keyword, a background worker harvests autocomplete suggestions across a
probe matrix (questions / prepositions / comparisons / alphabeticals / related),
and the results are rendered as spoke wheels plus grouped lists, with CSV export.

## Data providers

| Provider | When used | Extras |
|---|---|---|
| `dataforseo` | `DATAFORSEO_LOGIN` + `DATAFORSEO_PASSWORD` set | search volume, CPC, competition |
| `google-suggest` | fallback, no credentials needed | suggestions only |

DataForSEO endpoints used:
* `POST /v3/serp/google/autocomplete/live/advanced` - one call per probe (~58 calls per search).
* `POST /v3/keywords_data/google_ads/search_volume/live` - optional enrichment, enabled with `DATAFORSEO_SEARCH_VOLUME=true`.

Cost note: autocomplete is billed per request, so one keyword search issues ~58 billable
calls. Lower `DATAFORSEO_CONCURRENCY` to stay under rate limits (Google Ads live endpoints
allow 12 requests/minute).

## Setup

```bash
createdb atp
cp .env.example .env      # then fill in DataForSEO credentials if you have them
cargo leptos watch        # http://127.0.0.1:3000
```

Schema and the Apalis job tables are created automatically on boot.

### Building for deployment

Always build with `cargo leptos build --release`, not plain `cargo build`.
`cargo leptos` sets `LEPTOS_OUTPUT_NAME` at compile time, which is what makes the
server emit the correct client bundle URLs. A plain `cargo build` binary asks the
browser for `/pkg/<name>_bg.wasm` while wasm-bindgen writes `/pkg/<name>.wasm`,
so hydration fails with a 404 and the page silently stays static (server-rendered
HTML still looks correct, which makes this easy to miss).

Run the release binary with `target/server/release/atp` and keep `target/site`
next to it, or set `LEPTOS_SITE_ROOT`.

## Architecture

```
src/
  domain.rs             shared types, categories, grouping (client + server)
  app.rs                Leptos UI + server functions
  jobs.rs               Apalis job: harvest -> store -> mark done
  providers/
    mod.rs              probe matrix, dedupe, provider selection
    dataforseo.rs       DataForSEO autocomplete + Google Ads metrics
    google.rs           free public autocomplete fallback
  main.rs               Axum server, Apalis monitor, CSV export
```

The search page polls every 2s while the job is `pending`/`running`, so results
appear as soon as the worker finishes. Apalis uses Postgres `LISTEN/NOTIFY`, so
jobs start immediately after being pushed.

## Tests

```bash
cargo test --no-default-features --features ssr
```

`tests/dataforseo.rs` runs the provider against a local mock of the DataForSEO API,
so parsing, dedupe, location mapping and enrichment are verified without spending credits.
