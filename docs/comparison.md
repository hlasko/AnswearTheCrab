# Answer the Crab vs AnswerThePublic

Snapshot taken 2026-09-05. Competitor facts come from answerthepublic.com and its
pricing page; our facts come from the code and from live runs against the
DataForSEO API, not from memory.

## Core product

| Capability | AnswerThePublic | This clone |
|---|---|---|
| Categories | Questions, Prepositions, Comparisons, Alphabeticals, Related | Same five |
| Wheel visualisation | Yes | Yes, SVG, spoke length scales with group size |
| Column/list view | Yes | Yes, with per-modifier counts |
| Search volume + CPC | Yes, on every paid plan and every source | **Google only**, via DataForSEO Labs; YouTube and Bing return phrases without metrics |
| Competition index (0-100) | Not surfaced | Yes, in CSV and stored (Google only) |
| CSV export | Yes | Yes, including metrics |
| Wheel image export | Yes | Yes, PNG at 2x per category |
| Result filtering | Yes | Yes, client-side, live |
| Search history | Yes, paid plans | Yes, unlimited |

## Reach and scale

| | AnswerThePublic | This clone |
|---|---|---|
| Phrases per search | ~100-400 typical | capped at 700 (`DATAFORSEO_LIMIT`, max 1000), but the cap is rarely the binding limit. Measured medians across our own searches: Google 637, YouTube 388, Bing 286; the floor was 40 for a narrow keyword |
| Markets | 20+ countries | 8 (US, UK, PL, DE, ES, FR, CA, AU) |
| Search quota | 100-300/month by plan | none; you pay per API call |
| Data sources | Google, YouTube, Amazon, Bing, TikTok | Google, YouTube, Bing |

## Cost

| | AnswerThePublic | This clone |
|---|---|---|
| Model | Subscription | Pay per use |
| Price | 48-470 PLN/month (billed yearly) | $0.0132 per Google search, measured on a real call; YouTube and Bing are free |
| At 100 Google searches/month | 48 PLN (Starter) | ~4.75 PLN |
| YouTube/Bing searches | Counted against the plan quota | Free and unlimited |
| Without an API key | Not possible, account required | Google falls back to free Suggest; YouTube and Bing are unaffected |

## What we are missing

| Feature | Status |
|---|---|
| Amazon, TikTok sources | Missing, see `adding-sources.md` |
| Alerts and trend monitoring over time | **Partly.** "Run again" plus the comparison view covers monitoring on demand; scheduled runs and notifications are absent because they need accounts and a mail path |
| AI content generation (Content Studio) | Missing, and out of scope: a separate product |
| Period-over-period comparison | **Done.** Re-running a keyword shows what appeared and disappeared since the previous run |
| Accounts and team collaboration | Missing, single-user app |
| PNG export of the wheel | **Done.** Each wheel has a PNG button; rendered at 2x (1120x1120) client-side. PDF still missing |
| Search by domain URL | Missing |

## Where we are ahead

- **Cost at volume:** no search quota, you pay only for Google API calls;
  YouTube and Bing cost nothing at all.
- **Depth:** up to 700 phrases per query where the original typically shows a
  few hundred.
- **Data ownership:** your own Postgres, full SQL access, no lock-in.
- **Robust to misconfiguration:** the language is detected from the returned
  phrases, so a wrong market selection degrades gracefully.
- **Free fallback:** with no API keys it still works off public Google Suggest.
- **Noise filtering:** suggestions unrelated to the seed are dropped. Autocomplete
  completes prefixes, so probing "kawa co" also returns "co awareness week";
  17% of one Bing run was this kind of debris.

## Verdict

The visualisation and categorisation core is fully reproduced, paid metrics
included. The gap is breadth: extra data sources, alerting, and team features,
which in the original are the SaaS layer built around the same idea.
