# answer the crab

An AnswerThePublic clone in Rust: **Axum + Leptos (SSR/hydrate) + Apalis + Postgres**.

Type a keyword, a background worker harvests autocomplete suggestions across a
probe matrix (questions / prepositions / comparisons / alphabeticals / related),
and the results are rendered as spoke wheels plus grouped lists, with CSV export.

## Data providers

Suggestions are classified into ATP categories using a per-language vocabulary
(`domain::vocabulary`), currently EN, PL, DE, ES, FR, falling back to English.
Without this a Polish search put all 699 phrases into "alphabetical".

The form offers **markets** (`domain::MARKETS`), not free language x country
pairs, because search engines do not serve every language in every country:
Poland only offers `pl`, and asking DataForSEO for `en` there fails the whole job
with an opaque `Invalid Field: 'language_code'`.

Classification also detects the language from the returned phrases
(`domain::detect_vocabulary`) and overrides the requested one when the evidence
is clear, so a market left on the wrong value degrades gracefully instead of
dumping everything into "alphabetical".

| Provider | When used | Extras |
|---|---|---|
| `dataforseo` | `DATAFORSEO_LOGIN` + `DATAFORSEO_PASSWORD` set | search volume, CPC, competition |
| `google-suggest` | fallback, no credentials needed | suggestions only |

### DataForSEO modes

Set with `DATAFORSEO_MODE`.

| Mode | Requests per search | Metrics | Endpoint |
|---|---|---|---|
| `labs` (default) | **1** | included | `dataforseo_labs/google/keyword_suggestions/live` |
| `autocomplete` | ~58 (+1 if metrics) | separate call | `serp/google/autocomplete/live/advanced` |

`labs` returns up to 1000 long-tail phrases containing the seed keyword, with search
volume, CPC and competition already attached, for a single billable request. Phrases
are classified into ATP categories locally (`domain::classify`). This is roughly an
order of magnitude cheaper than the probe matrix and is the sensible default.

Use `autocomplete` when you specifically want what Google's search box suggests
right now: it fires one request per modifier (`how X`, `X for`, `X vs`, `X a..z`),
which mirrors AnswerThePublic more literally but bills ~58 requests per keyword.
Add `DATAFORSEO_SEARCH_VOLUME=true` for metrics in that mode; keep
`DATAFORSEO_CONCURRENCY` low, since Google Ads live endpoints allow 12 requests/minute.

Tune the Labs result cap with `DATAFORSEO_LIMIT` (default 700, max 1000).

## Setup

```bash
cp .env.example .env      # then fill in DataForSEO credentials if you have them
./run.sh                  # creates the DB, builds, serves on the first free port
```

`run.sh` builds with `cargo leptos` (required for hydration), creates the database
from `DATABASE_URL` if missing, and if the port is busy it moves to the *nearest*
free one instead of failing. Nearest means either direction: with 3000 taken and
3001 also taken, it will use 2999 rather than 3002. It never goes below 1024, and
re-checks the port right before binding in case something grabbed it while the
build was running.

```bash
./run.sh              # prefer 3000, fall back to 3001, 3002, ...
./run.sh 8080         # prefer 8080
./run.sh --kill       # take the preferred port back instead of moving
./run.sh --release    # release build
./run.sh --no-build   # serve what is already in target/
```

For hot reload during development use `cargo leptos watch` instead.

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

Each modifier column shows its strongest few phrases with a "+N more" hint, and
a per-category "Show all" toggle reveals the rest. A single DataForSEO search can
return 700 phrases, which rendered in full made the page ~21 screens tall; the
collapsed view is ~6.6 screens and nothing is dropped, only hidden. The filter and
the toggle compose, so you can narrow first and then expand what is left.

The search page polls every 2s **only while the job is `pending`/`running`**, and
stops once it reaches `done` or `failed`. Polling past that point refetched the
same rows forever and rebuilt the view on every response, which threw the reader
back to the top of the page and wiped the filter and any expanded sections. Apalis uses Postgres `LISTEN/NOTIFY`, so
jobs start immediately after being pushed.

## Docs

* [`docs/comparison.md`](docs/comparison.md) - how this stands against AnswerThePublic.
* [`docs/adding-sources.md`](docs/adding-sources.md) - plan for YouTube, Amazon, Bing, TikTok.

## Tests

```bash
cargo test --no-default-features --features ssr
```

`tests/dataforseo.rs` runs the provider against a local mock, covering mode
selection, dedupe, location mapping and enrichment.

`tests/dataforseo_fixtures.rs` is the stricter one: it replays response bodies
taken **verbatim** from DataForSEO rather than written by us, so it catches wrong
field paths that a self-authored mock would happily confirm. It already did:

* Keyword Suggestions returns `keyword`/`keyword_info` **flat** inside `items[]`,
  while Related Keywords nests them under `keyword_data`. Both shapes are parsed.
* A Keyword Suggestions response can contain several `result` entries, the first
  holding only seed data with `"items": null`.
* `tests/fixtures/live_auth_error_40100.json` was captured from the live API with
  invalid credentials: HTTP 401, code `40100` (not `40101`) and `"tasks": null`.

All fixtures cost nothing to run.
