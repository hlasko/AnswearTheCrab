# Backlog

Ideas that are worth doing but are not being built yet. Each entry says what it
is, what it would take, and what has already been checked, so picking one up
does not start from zero.

## NeuronWriter-style term editor

An editor with a live list of terms taken from the pages that already rank, and
a counter showing how much of that list the text covers. NeuronWriter's core is
not the editor: it is the weighted term list plus a coverage score.

**Why it is cheap for us.** Measured, not assumed: the `content_parsing` call we
already make for each competitor returns the page's full body text, not just its
headings. One real competitor URL from our own database returned 19 sections and
9,697 characters of text for $0.0015, which is a call we are already paying for.
We keep the headings and throw the text away. The corpus needed for term
extraction is therefore already within reach at no extra API cost.

**Shape of the work.**

1. ~~Store the body text per competitor.~~ Done: `brief_competitors.content`,
   added for the fact checker. The corpus this editor needs now exists.
2. Extract weighted terms locally: 1-3 word n-grams, weighted by how many of the
   ranking pages use each one, with the median number of uses. Polish needs a
   stopword list and light lemmatisation, otherwise "kawa" and "kawy" count as
   two separate terms.
3. Editor with a live coverage counter as the writer types.
4. Feed the term list into the writing prompt so a draft starts with coverage
   rather than being graded for it afterwards.

**Open question, deliberately unresolved.** Step 4 pulls against a decision
already made in `brief_prompt`: our instructions are about *what to cover*, not
which strings to repeat, because keyword-density prose reads badly. A term
counter can be a diagnosis ("competitors all discuss grind size, you have not")
or a target to game. It is worth building as the former and not the latter.

## YouTube per-phrase volume

Rejected after measurement (2026-09-10). Google Trends in YouTube mode has a
signal for the seed of a topic but is flat zero for nearly every long-tail
phrase: "kredyt hipoteczny kalkulator", Google volume ~6000, scored 52 silent
weeks of 53 on YouTube Trends, and so did "bez wkładu własnego" and "2 procent".
The Trends floor for PL YouTube is on the order of ten thousand searches a
month. No anchor trick recovers a number from a zero.

keywordtool.io sells a YouTube volume per phrase ($88/month with API). It is
modelled from clickstream panels, not measured; for PL long tail the panel
sees a phrase a handful of times a month. If a number per phrase becomes
worth paying for, it plugs into the "Across engines" YouTube column as an
optional provider. Until then the YouTube page's anchor estimate covers the
topic-level question.

## Per-platform volume beyond Google and Bing (checked 2026-09-10)

Nobody sells a measured search volume for YouTube, Perplexity, TikTok,
Instagram or Reddit. What keywordtool.io labels "estimated" is modelled from
clickstream panels. Checked each route before deciding:

- **DataForSEO Clickstream API**: one global figure per phrase with a country
  split. No platform split; their help centre describes it as "terms users
  enter in the search bar", web search only. Useless for the platforms above.
- **DataForSEO `include_clickstream_data`** (Labs, doubles the price): adds a
  clickstream volume plus gender and age per phrase. Measured on 300 PL and
  300 US phrases: 201 and 94 had distributions, and they are noise. Extremes
  are 100/0 ("wniosek o kredyt hipoteczny" 100% female, "oprocentowanie
  kredyt hipoteczny" 100% male), meaning one or two panel users typed the
  phrase. Large phrases contradict each other: "kredyt hipoteczny kalkulator"
  43% aged 18-24, "kredyt hipoteczny" 0% aged 18-24 and 50% aged 45-54.
  Showing this in a brief would steer tone off a coin flip. Rejected.
- **Reddit**: public `search.json` is 403 without login on every host tried
  (www, api, old with redirect to login). Only the OAuth API remains, which
  needs a Reddit app registered on the user's account. Thin for PL anyway.
- **TikTok Creative Center**: Keyword Insights now redirects to login; the
  hashtag page shows a top list only, no search; `creative_radar_api`
  answers "no permission". Needs a TikTok Ads account.
- **SocialFetch** (TikTok hashtags): returns videos under a hashtag, not
  demand. **SearchApi** (Perplexity): returns Perplexity's answer with cited
  sources, not autocomplete; useful as a Perplexity citation check if that
  becomes worth $40/month.
- **Perplexity autocomplete**: exists, over a WebSocket after a Cloudflare
  check, five suggestions per prefix, works for Polish ("kredyt hipoteczny po
  angielsku" appears there and not on Google). A fourth source via Playwright
  is feasible (~1 min per seed, fragile) and is the one item here still worth
  doing, as a list of questions people ask assistants, without volume.
  **Done 2026-09-10**: `src/providers/perplexity.rs` + `scripts/`.
