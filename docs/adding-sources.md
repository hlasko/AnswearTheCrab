# Adding YouTube, Amazon, Bing and TikTok

The clone currently harvests Google only. This is the plan for the remaining four
sources AnswerThePublic offers.

Every endpoint below was probed live on 2026-09-05 with this project's
credentials; the notes record what actually happened, not what the docs promise.

## Summary

| Source | Free endpoint | Paid endpoint (DataForSEO) | Status |
|---|---|---|---|
| YouTube | **Works**, `ds=yt` on Google Suggest | `serp/youtube/organic/live/advanced` (OK) | **Shipped** |
| Bing | **Works**, `api.bing.com/osjson.aspx` | `keywords_data/bing/keywords_for_keywords/live` (OK) | **Shipped** |
| Amazon | **Works**, needs full parameter set | `dataforseo_labs/amazon/related_keywords/live` (OK, $0.0132) | ~2h, next |
| TikTok | Responds but returns nothing without a browser session | No first-party endpoint | ~1-2 days, or skip |

## What shipping YouTube and Bing taught us

Both were added in `src/providers/suggest.rs`, which now serves all three free
endpoints: they return the identical `["seed", [...]]` array, so one parser and
one probe loop cover them.

Building them surfaced a bug that had been in the Google path all along. The
probe matrix was hardcoded to English modifiers, so searching the Polish "kawa"
asked for "are kawa" and the engines answered "are kawasaki engines good" -
genuine suggestions, useless for a Polish search. `probes()` now takes the
language and reuses the same `Vocabulary` the classifier uses.

Measured on the live endpoints for "kawa" in Polish:

| | before | after |
|---|---|---|
| Bing questions | (English noise) | 274 |
| YouTube questions | (English noise) | 101 |
| phrases containing "kawasaki" | 10 (YouTube) | 3 |

## Verified probes

```bash
# YouTube: Google Suggest with the YouTube dataset. Free, no key.
curl "https://suggestqueries.google.com/complete/search?client=firefox&ds=yt&q=kawa"
# -> ["kawa",["kawa wjechała","kawa z diabłem","kawaleria powietrzna", ...]]

# Bing: public OpenSearch JSON. Free, no key.
curl "https://api.bing.com/osjson.aspx?query=kredyt"
# -> ["kredyt",["kredyt hipoteczny","kredyt ok","kredyt gotówkowy", ...]]

# Amazon: the short form returns an empty HTML page; the full parameter set works.
curl -H 'User-Agent: Mozilla/5.0' \
  "https://completion.amazon.com/api/2017/suggestions?session-id=1&page-type=Gateway\
&lop=en_US&site-variant=desktop&client-info=amazon-search-ui&mid=ATVPDKIKX0DER\
&alias=aps&prefix=coffee&limit=11&suggestion-type=KEYWORD"
# -> coffee maker, coffee table, coffee pods, keurig coffee machine, ...

# Amazon marketplaces are selected by DOMAIN. Measured, because the obvious
# guess is wrong: changing mid or lop on completion.amazon.com does nothing,
# and a German mid on the .com domain still returns English results.
curl -H 'User-Agent: Mozilla/5.0' "https://completion.amazon.de/api/2017/suggestions?\
session-id=1&page-type=Gateway&lop=de_DE&site-variant=desktop\
&client-info=amazon-search-ui&mid=A1PA6795UKMFR9&alias=aps&prefix=kaffee\
&limit=4&suggestion-type=KEYWORD"
# -> kaffeevollautomat, kaffeemaschine, kaffeebohnen, senseo kaffeemaschine

# TikTok: HTTP 200 but sug_list is empty without a real browser session.
curl "https://www.tiktok.com/api/search/general/sug/?keyword=coffee"
# -> {"sug_list":[], "status_code":0, ...}
```

DataForSEO endpoints confirmed reachable on this account:

```
keywords_data/bing/keywords_for_keywords/live       20000 Ok.
dataforseo_labs/amazon/related_keywords/live        20000 Ok.   cost $0.0132
dataforseo_labs/amazon/bulk_search_volume/live      20000 Ok.
serp/youtube/organic/live/advanced                  20000 Ok.
```

## The shape of the work

The provider trait already exists and is the right seam:

```rust
// src/providers/mod.rs
#[async_trait::async_trait]
pub trait SuggestionProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn harvest(&self, keyword: &str, language: &str, country: &str)
        -> anyhow::Result<Vec<Suggestion>>;
}
```

`GoogleSuggest` in `src/providers/google.rs` is a ~90 line template: build the
probe matrix with `probes()`, fire the requests with `buffer_unordered`, map the
strings into `Suggestion::new`, then `dedupe()`. A new source is mostly that file
with a different URL and response shape.

### 1. Schema

Searches need to record their source, and a keyword can now be searched once per
source:

```sql
alter table searches add column if not exists source text not null default 'google';
```

Note `suggestions` already has `unique (search_id, text)`, which stays correct:
one search belongs to one source.

### 2. Domain

`Source` enum next to `Category`, plus per-source category rules. This matters:
alphabetical buckets and question words are a *web search* idea. Amazon queries
are overwhelmingly product nouns ("coffee maker", "coffee pods"), so the
Questions wheel will be nearly empty and Alphabeticals will hold everything. Two
options:

- keep the five categories everywhere and accept a lopsided distribution, or
- give commerce sources their own grouping (by brand, by attribute, by price
  intent).

Recommend starting with the first, measuring the split on real data, and only
then deciding. That is exactly how the Polish classification bug was found.

### 3. Provider per source

| Source | Notes |
|---|---|
| YouTube | Identical to `google.rs`, add `ds=yt`. The existing `hl`/`gl` market parameters work unchanged (verified: `gl=pl` returns different results than `gl=us`), so `MARKETS` maps over directly. Cheapest possible win. |
| Bing | `api.bing.com/osjson.aspx?query=`, same `["seed",[...]]` array shape as Google, so the parser is reusable. Market via `&mkt=<lang>-<COUNTRY>`, verified working: `mkt=pl-PL` returns "coffeedesk", `mkt=de-DE` returns "coffeefair", `mkt=en-US` returns "coffee shops near me". Maps cleanly onto our existing `Market` struct. |
| Amazon | Needs the full parameter set, response is `{"suggestions":[{"value":...}]}`. The **domain** selects the marketplace, not `mid` or `lop`: `completion.amazon.de` returns "kaffeevollautomat", while `completion.amazon.com` returns "coffee maker" no matter what `mid`/`lop` you pass. Rate-limits harder than Google; keep concurrency at 2-3. |
| TikTok | No usable free endpoint. Options: (a) drive a headless browser via the existing Playwright setup, slow and brittle; (b) third-party API; (c) skip and say so. Recommend (c) until there is a real need. |

### 4. UI

The market dropdown gains a sibling source selector, and `MARKETS` needs a
per-source view since Amazon marketplaces are not the same set as Google
locations. The wheels themselves need no change.

### 5. Tests

Follow `tests/dataforseo_fixtures.rs`: capture one real response body per source
into `tests/fixtures/` and assert against it. Self-written mocks confirmed their
own assumptions and hid the flat-vs-nested Labs difference; do not repeat that.

## Suggested order

1. **YouTube** — one file, free, immediately doubles perceived coverage.
2. **Bing** — same response shape as Google, so the parser is shared.
3. **Amazon** — different shape and marketplace ids; also the point where the
   category model needs measuring against real commerce data.
4. **TikTok** — only if the browser-driven cost is acceptable.

Steps 1-3 are roughly a day of work and bring the source list to four of the
original's five.
