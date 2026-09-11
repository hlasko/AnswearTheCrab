-- Who the AI answers cite, question by question.
--
-- Everything else here measures Google. This measures the answer engine
-- itself: put a real question to Perplexity through DataForSEO's LLM
-- Responses endpoint ($0.0065 a question, Polish works) and record which
-- domains its answer cites. Repeat for the questions a topic actually
-- raises and you get the one number GEO is about: of the questions your
-- customers ask an assistant, in how many does the assistant mention you.
--
-- One row per (run, question). The run groups them so a repeat is
-- comparable: the same questions, a week later, is the whole point.

create table if not exists ai_answer_runs (
    id uuid primary key default gen_random_uuid(),
    search_id uuid references searches(id) on delete set null,
    -- The topic these questions belong to, for display.
    topic text not null,
    language text not null,
    country text not null,
    -- The site being watched, normalised. Empty when none is set.
    domain text not null default '',
    model text not null default 'sonar',
    created_at timestamptz not null default now()
);

create table if not exists ai_answers (
    run_id uuid not null references ai_answer_runs(id) on delete cascade,
    question text not null,
    -- The answer text, kept so a later brief can quote what the assistant
    -- already says rather than guessing at it.
    answer text not null default '',
    -- Cited domains in the order the answer cites them: ["bankier.pl", ...]
    domains jsonb not null default '[]',
    -- Full source URLs, for reading what got cited.
    urls jsonb not null default '[]',
    -- Whether the watched domain is among them, and where.
    cited bool not null default false,
    cited_rank int,
    primary key (run_id, question)
);

create index if not exists ai_answer_runs_topic_idx on ai_answer_runs (topic, created_at desc);

-- Watches can carry the answer-engine check too, on the same schedule.
alter table watches add column if not exists ask_ai bool not null default false;
alter table watches add column if not exists ai_last_run_at timestamptz;
