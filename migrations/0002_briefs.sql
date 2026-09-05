-- Content Writer: briefs built from topics picked in the research section.
--
-- Deliberately source-agnostic. A brief collects findings from one or more
-- research sources (today Google SERP, later AEO/GEO), so nothing here names
-- DataForSEO or any other vendor.

create table if not exists briefs (
    id uuid primary key default gen_random_uuid(),
    -- The phrase the brief is built around, copied rather than referenced so a
    -- brief survives the deletion of the search it came from.
    topic text not null,
    language text not null default 'en',
    country text not null default 'us',
    -- Where the topic was picked from, for traceability. Null when the brief was
    -- created directly.
    search_id uuid references searches (id) on delete set null,
    status text not null default 'pending', -- pending | running | done | failed
    error text,
    -- Google's AI answer. Absent for roughly half of topics (measured), which is
    -- a normal outcome rather than a failure.
    ai_overview text,
    -- Domains Google cites in that answer: the practical target list for AEO.
    ai_sources jsonb not null default '[]',
    -- "People also ask", ready-made FAQ headings.
    questions jsonb not null default '[]',
    -- Related searches surfaced on the SERP.
    related jsonb not null default '[]',
    created_at timestamptz not null default now(),
    started_at timestamptz,
    finished_at timestamptz
);

create index if not exists briefs_created_at_idx on briefs (created_at desc);
create index if not exists briefs_topic_idx on briefs (lower(topic));

-- One row per competing page found for the brief's topic.
create table if not exists brief_competitors (
    id bigserial primary key,
    brief_id uuid not null references briefs (id) on delete cascade,
    rank int not null,
    url text not null,
    domain text not null,
    title text,
    description text,
    -- Headings extracted from the page: [{level, title}, ...].
    -- Content parsing succeeds for about 61% of pages (measured), so an empty
    -- array is expected and the brief stays useful without it.
    headings jsonb not null default '[]',
    parsed bool not null default false,
    parse_error text,
    created_at timestamptz not null default now(),
    unique (brief_id, url)
);

create index if not exists brief_competitors_brief_idx on brief_competitors (brief_id, rank);
