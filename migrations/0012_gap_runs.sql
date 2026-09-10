-- Keyword gaps between a competitor and the user's site.
--
-- One row per comparison, phrases as JSON: the gap is read as a whole and
-- re-run rarely, so a child table would be more schema for no query it needs
-- to answer. Kept rather than fetched live because each run costs money and
-- the interesting question later is "did the gap shrink".

create table if not exists gap_runs (
    id uuid primary key default gen_random_uuid(),
    competitor text not null,
    mine text not null,
    language text not null,
    country text not null,
    -- Phrases the competitor ranks top 10 for and the user's site does not:
    -- [{keyword, volume, cpc, competitor_rank, competitor_url}, ...]
    phrases jsonb not null default '[]',
    total int not null default 0,
    created_at timestamptz not null default now()
);

create index if not exists gap_runs_pair_idx on gap_runs (competitor, mine, created_at desc);
