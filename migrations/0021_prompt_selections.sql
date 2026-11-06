-- Immutable prompt/provenance snapshots until the user explicitly approves a replacement.
create table if not exists prompt_selections (
    topic text not null,
    language text not null,
    country text not null,
    source text not null,
    prompts jsonb not null check (jsonb_typeof(prompts) = 'array'
        and jsonb_array_length(prompts) between 1 and 20),
    approved_at timestamptz not null default now(),
    primary key (topic, language, country, source)
);
