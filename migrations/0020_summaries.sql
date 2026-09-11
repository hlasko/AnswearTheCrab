-- AI readings of a section, cached.
--
-- Each is one OpenRouter call over facts assembled from these same tables,
-- so it costs money and says the same thing until the data changes. Cached
-- per (search, section); asking again replaces it.
--
-- The facts are stored next to the answer. Without them a summary is an
-- assertion; with them anyone can see exactly what the model was given,
-- which is the only way to tell a reading from an invention.

create table if not exists summaries (
    -- Null for sections that belong to no single search, like the page
    -- tracker. Postgres treats nulls in a unique index as distinct, so the
    -- key is on coalesce() rather than on the column.
    search_id uuid references searches(id) on delete cascade,
    -- Section key: priorities | phrases | answers | youtube | pages
    kind text not null,
    facts text not null,
    summary text not null,
    model text not null default '',
    created_at timestamptz not null default now()
);

create unique index if not exists summaries_key_idx
    on summaries (coalesce(search_id, '00000000-0000-0000-0000-000000000000'::uuid), kind);
