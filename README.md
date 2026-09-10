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

Three search boxes are supported, chosen per search in the UI:

| Source | Provider | Extras |
|---|---|---|
| Google | `dataforseo` when keys are set, else `google-suggest` | search volume, CPC, competition |
| YouTube | `youtube-suggest` (free, `ds=yt`) | suggestions only |
| Bing | `bing-suggest` (free, OpenSearch JSON) | suggestions only |

All three free endpoints return the same `["seed", [...]]` array, so
`providers::suggest` serves them with one parser.

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

Each wheel can be saved as a PNG straight from the page: the SVG is serialised
and rasterised at 2x in the browser, so nothing round-trips through the server.
Wheel styling is therefore set through SVG attributes rather than the stylesheet,
since a canvas rasterises the markup without the page's CSS.

Each modifier column shows its strongest few phrases with a "+N more" hint, and
a per-category "Show all" toggle reveals the rest. A single DataForSEO search can
return 700 phrases, which rendered in full made the page ~21 screens tall; the
collapsed view is ~6.6 screens and nothing is dropped, only hidden. The filter and
the toggle compose, so you can narrow first and then expand what is left.

"Run again" repeats a search with the same keyword, market and source, and the
result page reports what appeared and disappeared since the previous run. That
covers monitoring on demand; scheduled runs and alerting would need accounts and
a mail path, which a single-user app does not have.

The search page polls every 2s **only while the job is `pending`/`running`**, and
stops once it reaches `done` or `failed`. Polling past that point refetched the
same rows forever and rebuilt the view on every response, which threw the reader
back to the top of the page and wiped the filter and any expanded sections. Apalis uses Postgres `LISTEN/NOTIFY`, so
jobs start immediately after being pushed.

## Content briefs

The second section turns picked research topics into content briefs. Tick phrases
on a results page, press "Create briefs", and each topic is researched in the
background:

* **Google's AI answer** and the domains it cites, which is the concrete target
  list for AEO work: to be quoted alongside them you have to answer at least as
  directly. Measured on real topics, an AI overview exists for roughly half of
  queries; shopping and brand queries usually have none, and the brief says so
  rather than looking broken.
* **What to write**: the format the ranking pages imply, e.g. "article of about
  6 sections, closing with a FAQ". Derived from the median number of sections
  among readable competitors and how many of them close with a FAQ block, so the
  brief answers "article or FAQ?" instead of leaving it to be inferred.
* **People Also Ask** entries, which are gaps the piece must close rather than an
  instruction to build a FAQ: broad ones carry a section, narrow ones belong in a
  closing FAQ.
* **Competitor structure**: the headings of the pages that rank, with the ones
  several competitors agree on surfaced separately. Content parsing succeeds for
  about 60% of pages, so the brief reports how many were readable.

* **Who you are up against**: the domain authority of each competitor and a
  verdict (Open, Contested, Entrenched) from their median. Bands come from
  measured topics: coffee health at 326, mortgages at 405 to 548.
* **What the AI answer leaves out**: subjects several ranking pages give a
  section to that Google's summary never mentions. The ground where a reader
  gains something by clicking through.
* **Are you being cited?**: check a domain against the AI answer's sources and
  the organic top 10. Checks are saved, so a topic researched again shows
  whether anything moved.

Cost is about $0.039 per topic against the DataForSEO API: $0.0026 for the
SERP with People Also Ask expanded to 15 questions, $0.012 for parsing eight
competitors, $0.024 for their domain authority.

### On the results page

* **Topics**: phrases grouped into the subjects they are about. On "kredyt
  hipoteczny", 603 of 699 phrases fall into 90 topics; the calculator topic
  alone is 88 phrases and 81K monthly searches. A topic can be briefed as a
  whole.
* **What people want**: informational, commercial, transactional or
  navigational, as a filter over a sortable table. Median CPC per group is the
  check that the split is real: it runs $0.37, $0.48, $0.62, $2.13. An
  **Overlooked** filter shows phrases with real volume and few advertisers.
* **Across engines**: the same keyword on Google, YouTube and Bing side by
  side, once it has been run on more than one.
* **Track changes**: re-run a search every N days; the home page opens with
  what moved since the run before.
* **Trend**: twelve months of volume per phrase as a sparkline, and a
  **Rising** filter for phrases up 20% or more over the year. The data comes
  with the call we already make, so it costs nothing.

### Keyword gap

A separate page: enter a competitor's domain and get the phrases they rank in
the top 10 for that your site does not rank for at all, with their page
linked. One $0.013 call per comparison, stored so a later run shows whether
the gap shrank.

### Bing

Bing publishes a real monthly count through Microsoft Advertising, which
DataForSEO resells (`keywords_data/bing/search_volume`, about $0.01 per 100
phrases). It exists for six countries only: US, GB, CA, AU, DE, FR, in
en/de/fr. Checked against the API's own location list: no Polish location at
all, so PL runs get a line saying so rather than an empty column.

For markets that have it, every Google run also fetches the Bing count for
its phrases and shows it as a **Bing** column next to Google's, with a "Most
searched on Bing" sort. The two disagree in useful ways: for "cold brew
coffee" in the US, Bing is 0.4% of Google overall, but "nitro cold brew
coffee" is 17% and "starbucks cold brew coffee" is 1%, so a phrase's Bing
share says something about who searches it.

### YouTube

YouTube publishes no search volume, and no keyword API sells a measured one:
the tools that show a number for YouTube estimate it from clickstream panels.
The one first-party signal is Google Trends in YouTube mode, which this app
reads through DataForSEO (`keywords_data/google_trends/explore`, `type:
youtube`, about $0.011 a call).

On a results page, **Check YouTube** asks Trends about the seed on YouTube and
on Google search for the same twelve months: two charts, a verdict on whether
it is a video topic (from how many weeks had measurable searches), and the
related queries YouTube itself ranks under it, top and rising. That last list
is where the long tail lives on YouTube. Measured before building this:
individual long-tail phrases are flat zero on YouTube Trends (52 of 53 weeks
for "kredyt hipoteczny kalkulator"), so they are not asked about.

The **YouTube** page compares up to five topics on YouTube against each other,
next to the same ranking on Google search and Google's real monthly volume,
which is the one absolute number that exists. A topic loud on YouTube and
quiet on Google is one people would rather watch than read ("jak inwestować":
590 Google searches a month, top of four on YouTube). Nobody publishes
monthly YouTube searches for a phrase, so the scale-to-real-numbers option
is folded away: if you trust a figure for one topic from somewhere, the rest
scale to it and inherit its error.

Everything is exported from the brief page as markdown, or as a **prompt**
(`.txt`) written as instructions about coverage rather than keywords: the format
the ranking pages imply, the AI answer as ground already taken, the questions to
close, and a couple of real competitor outlines for depth.

### Writing

With `OPENROUTER_API_KEY` set, a brief can be turned into a draft in the app.
OpenRouter is used because one key reaches models from several vendors, so the
model is a setting (`OPENROUTER_MODEL`) rather than an architectural commitment;
the wire format is OpenAI's `/chat/completions`, so any compatible endpoint works
by pointing `OPENROUTER_BASE_URL` elsewhere.

Drafting is optional. Without a key the section is hidden rather than shown as a
button that can only fail, and the prompt export covers the same ground for
pasting into a chat window. Drafts are kept rather than replaced, so different
models stay comparable.

Two kinds: the full article, or just the FAQ answers to the People Also Ask
questions, which is much cheaper and exports as FAQPage JSON-LD for the page.
Every draft shows how many sections open with their answer, how many sentences
carry a figure, and how many claims are sourced; **Check facts** looks each
figure and attribution up in the competitor pages and the AI answer, and lists
the ones found nowhere for a human to verify before publishing.

A draft can be **edited** in place, with those numbers following the
keystrokes, or **revised** by the model from an instruction ("shorten the
section on instalments") with everything else left intact. Both save a new
version rather than overwriting. **Copy for Docs** puts the text on the
clipboard as rich text, so it pastes into Google Docs or Notion with headings
and lists.

Set **your site** once in the header, and every brief opens with whether it is
cited, ranking but not cited, or absent; the briefs list becomes a scoreboard.
Each People Also Ask question can be briefed and each related search
researched from inside the brief, so one topic leads to the next.

### Adding a research source

`research::ResearchSource` is the seam. A source returns [`Findings`] and the job
merges contributions from every configured source, so AEO/GEO research becomes
one more implementation rather than a rework. This mirrors
`providers::SuggestionProvider`, where YouTube and Bing were each a single file.

## Docs

* [`docs/comparison.md`](docs/comparison.md) - how this stands against AnswerThePublic.
* [`docs/adding-sources.md`](docs/adding-sources.md) - plan for YouTube, Amazon, Bing, TikTok.
* [`docs/aeo-geo.md`](docs/aeo-geo.md) - what gets content cited by answer engines, measured.
* [`docs/backlog.md`](docs/backlog.md) - ideas worth building, not started yet.

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
