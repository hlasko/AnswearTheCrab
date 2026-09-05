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
| Search volume + CPC | Yes, on every paid plan | Yes, via DataForSEO Labs |
| Competition index (0-100) | Not surfaced | Yes, in CSV and stored |
| CSV export | Yes | Yes, including metrics |
| Result filtering | Yes | Yes, client-side, live |
| Search history | Yes, paid plans | Yes, unlimited |

## Reach and scale

| | AnswerThePublic | This clone |
|---|---|---|
| Phrases per search | ~100-400 typical | up to 700 (`DATAFORSEO_LIMIT`, max 1000) |
| Markets | 20+ countries | 8 (US, UK, PL, DE, ES, FR, CA, AU) |
| Search quota | 100-300/month by plan | none; you pay per API call |
| Data sources | Google, YouTube, Amazon, Bing, TikTok | Google, YouTube, Bing |

## Cost

| | AnswerThePublic | This clone |
|---|---|---|
| Model | Subscription | Pay per use |
| Price | 48-470 PLN/month (billed yearly) | ~$0.01 per search (Labs mode) |
| At 100 searches/month | 48 PLN (Starter) | ~4 PLN |
| Without an API key | Not possible, account required | Works on free Google Suggest |

## What we are missing

| Feature | Status |
|---|---|
| Amazon, TikTok sources | Missing, see `adding-sources.md` |
| Alerts and trend monitoring over time | Missing |
| AI content generation (Content Studio) | Missing |
| Period-over-period comparison | Missing |
| Accounts and team collaboration | Missing, single-user app |
| PNG/PDF export of the wheel | Missing, CSV only |
| Search by domain URL | Missing |

## Where we are ahead

- **Cost at volume:** no search quota, you pay only for API calls.
- **Depth:** 700 phrases per query against a typical few hundred.
- **Data ownership:** your own Postgres, full SQL access, no lock-in.
- **Robust to misconfiguration:** the language is detected from the returned
  phrases, so a wrong market selection degrades gracefully.
- **Free fallback:** with no API keys it still works off public Google Suggest.

## Verdict

The visualisation and categorisation core is fully reproduced, paid metrics
included. The gap is breadth: extra data sources, alerting, and team features,
which in the original are the SaaS layer built around the same idea.
