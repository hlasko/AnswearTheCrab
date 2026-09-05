-- searches: one user request for a keyword
create table if not exists searches (
    id uuid primary key default gen_random_uuid(),
    keyword text not null,
    language text not null default 'en',
    country text not null default 'us',
    status text not null default 'pending', -- pending | running | done | failed
    error text,
    suggestion_count int not null default 0,
    created_at timestamptz not null default now(),
    started_at timestamptz,
    finished_at timestamptz
);

create index if not exists searches_created_at_idx on searches (created_at desc);
create index if not exists searches_keyword_idx on searches (lower(keyword));

-- suggestions harvested from the autocomplete endpoints
create table if not exists suggestions (
    id bigserial primary key,
    search_id uuid not null references searches (id) on delete cascade,
    text text not null,
    category text not null,   -- questions | prepositions | comparisons | alphabetical | related
    modifier text not null,   -- e.g. "how", "for", "vs", "a"
    created_at timestamptz not null default now(),
    unique (search_id, text)
);

create index if not exists suggestions_search_idx on suggestions (search_id, category);
