-- What a topic looks like on YouTube, from Google Trends in YouTube mode.
--
-- YouTube publishes no search volume. Google Trends is the only first-party
-- signal of what people search there: a 0-100 index per week, and for a
-- single phrase the related queries ranked by that index. Measured before
-- building this: the index exists for the seed of a topic ("kredyt
-- hipoteczny" 30 average) but is flat zero for nearly every long-tail phrase
-- (52 of 53 weeks for "kredyt hipoteczny kalkulator"), so per-phrase numbers
-- cannot be derived and are not attempted. The related-queries list is where
-- the long tail shows up, ranked rather than counted.
--
-- One row per search run; re-checking replaces the row.

create table if not exists youtube_checks (
    search_id uuid primary key references searches(id) on delete cascade,
    -- Weekly 0-100 index on YouTube, oldest first: [{"date": "2025-09-07", "v": 30}, ...]
    weekly jsonb not null default '[]',
    -- Same weeks on web search, so the two shapes can sit side by side.
    weekly_web jsonb not null default '[]',
    -- Related queries on YouTube: [{"query": "...", "value": 100}, ...]
    top jsonb not null default '[]',
    rising jsonb not null default '[]',
    checked_at timestamptz not null default now()
);

-- Optional comparison of several topics on YouTube against one anchor whose
-- monthly YouTube searches the user knows (their own YouTube Studio, say).
-- Trends returns every phrase relative to the loudest, so with one known
-- absolute the rest become estimates. Kept separately: it is a different
-- question ("which of these topics deserves a video") from the per-search
-- check above.
create table if not exists youtube_compares (
    id uuid primary key default gen_random_uuid(),
    language text not null,
    country text not null,
    -- [{"keyword": "...", "youtube": 28.7, "web": 84.5, "estimate": 1200}, ...]
    rows jsonb not null default '[]',
    anchor text,
    anchor_volume int,
    created_at timestamptz not null default now()
);
