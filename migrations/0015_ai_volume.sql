-- How often a phrase's words appear in questions, per DataForSEO's
-- "AI search volume".
--
-- Not a count of questions put to ChatGPT or Perplexity; nobody has that.
-- DataForSEO models it from Google's People Also Ask boxes (their help
-- centre says so), joins grammatical forms, and for a multi-word phrase
-- counts questions that contain all its words in any order. So it is a
-- relative measure of how "askable" a phrase is, useful next to Google's
-- query volume: "lokata" 284 against Google 14.8K, "obligacje skarbowe" 105
-- against 165K, means people ask about deposits and google bonds.
-- $0.01 per 1000 phrases, works for every market tried.

alter table suggestions add column if not exists ai_volume bigint;
-- Twelve months, oldest first, so the shape can be drawn like `monthly`.
alter table suggestions add column if not exists ai_monthly jsonb;
