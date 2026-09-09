-- One-row settings. A single-user app needs a place for "my site" and not
-- much else; a key/value table is simpler than a column per setting and the
-- upsert is one statement.

create table if not exists settings (
    key text primary key,
    value text not null,
    updated_at timestamptz not null default now()
);
