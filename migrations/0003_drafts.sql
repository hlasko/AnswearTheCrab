-- Drafts written from a brief.
--
-- A brief can have several drafts: different models, or a second attempt after
-- editing the brief. Keeping them all makes comparison possible and means a
-- regeneration never destroys work.

create table if not exists drafts (
    id uuid primary key default gen_random_uuid(),
    brief_id uuid not null references briefs (id) on delete cascade,
    status text not null default 'pending', -- pending | running | done | failed
    error text,
    -- Which model produced it, so drafts stay comparable over time.
    model text not null default '',
    content text,
    created_at timestamptz not null default now(),
    finished_at timestamptz
);

create index if not exists drafts_brief_idx on drafts (brief_id, created_at desc);
