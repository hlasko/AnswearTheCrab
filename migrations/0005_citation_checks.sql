-- Tracking whether a site gets cited by the AI answer.
--
-- The point of AEO work is a citation, so it has to be measurable, otherwise
-- every technique is a matter of belief. A brief already records which domains
-- Google cites; this records how one particular site fared, once per brief run,
-- so the same topic researched again produces a comparable data point.
--
-- Stored per brief rather than computed on the fly because ai_sources changes
-- with every run: without a snapshot, history is lost the moment a topic is
-- researched again.

create table if not exists citation_checks (
    id bigserial primary key,
    brief_id uuid not null references briefs (id) on delete cascade,
    -- The site being tracked, normalised (no scheme, no "www.").
    domain text not null,
    -- Whether the AI answer cited it at all.
    cited bool not null,
    -- Position within the citation list, 1-based. Null when not cited.
    citation_rank int,
    -- Position in the organic results, if present. Null when absent.
    organic_rank int,
    created_at timestamptz not null default now(),
    unique (brief_id, domain)
);

create index if not exists citation_checks_domain_idx on citation_checks (domain, created_at desc);
