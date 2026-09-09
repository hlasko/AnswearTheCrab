-- Topics the user wants re-researched on a schedule.
--
-- Autosuggest drifts: the comparison view shows +N/-N between two runs, but
-- only if someone remembers to run again. A watch runs the search itself
-- every N days, and the home page surfaces what changed since the previous
-- run, so movement is noticed rather than looked for.

create table if not exists watches (
    id uuid primary key default gen_random_uuid(),
    keyword text not null,
    language text not null,
    country text not null,
    source text not null,
    every_days int not null default 7,
    enabled bool not null default true,
    -- When the scheduler last queued a run, so the interval is measured from
    -- the last attempt rather than from the last success.
    last_run_at timestamptz,
    created_at timestamptz not null default now()
);

-- An expression cannot be a table constraint, only a unique index, and the
-- upsert in the app targets it through the same expression.
create unique index if not exists watches_key_idx
    on watches (lower(keyword), language, country, source);
