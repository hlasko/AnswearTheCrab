-- Monthly search history and trend per phrase.
--
-- The keyword_suggestions call we already pay for returns twelve months of
-- volume and a yearly/quarterly/monthly change; we kept only the average.
-- Keeping the history answers "when to publish" (seasonality) and "is this
-- growing", neither of which one number can.

alter table suggestions add column if not exists monthly jsonb;
alter table suggestions add column if not exists trend_yearly int;
alter table suggestions add column if not exists trend_quarterly int;
