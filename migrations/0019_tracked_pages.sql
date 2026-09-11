-- Pages the user published, and whether the work paid off.
--
-- The app researches a topic, briefs it and writes it, and then stops. The
-- question it could never answer is the only one that matters a month
-- later: did the page rank, and does the assistant cite it. This closes
-- that loop. One row per page; its checks accumulate so the answer is a
-- line rather than a snapshot.

create table if not exists tracked_pages (
    id uuid primary key default gen_random_uuid(),
    url text not null unique,
    -- The topic it was written for, and the market to check in.
    topic text not null,
    language text not null,
    country text not null,
    -- Phrases to check the ranking for, usually from the brief.
    phrases jsonb not null default '[]',
    -- Where it came from, for the link back. Null when entered by hand.
    brief_id uuid references briefs(id) on delete set null,
    enabled bool not null default true,
    last_check_at timestamptz,
    created_at timestamptz not null default now()
);

create table if not exists page_checks (
    id bigserial primary key,
    page_id uuid not null references tracked_pages(id) on delete cascade,
    -- Best organic position across the tracked phrases, null when absent
    -- from every one of them.
    best_rank int,
    -- Per phrase: [{"phrase": "...", "rank": 7}, ...]; rank null when absent.
    ranks jsonb not null default '[]',
    -- Whether Perplexity cited this page's domain when asked the topic.
    cited bool,
    checked_at timestamptz not null default now()
);

create index if not exists page_checks_page_idx on page_checks (page_id, checked_at desc);
