-- What already gets watched under a topic on YouTube.
--
-- YouTube publishes no search volume. What it does publish is the view
-- count of every video, and its search ranking for a phrase. Summing the
-- views of the top ten videos for a phrase gives a measure of how much is
-- watched on that topic: supply and audience, not demand, and most of
-- those views came from recommendations rather than the search box. So it
-- is called appetite, never volume. Measured before building: it separates
-- topics ("jak inwestować" 18.7M, "lokata" 0.7M) and phrases ("kredyt
-- hipoteczny kalkulator" 1.65M, "bez wkładu własnego" 130K). $0.002 a phrase.
--
-- One row per (search, phrase); a re-check replaces the phrase's row.

create table if not exists youtube_appetite (
    search_id uuid not null references searches(id) on delete cascade,
    phrase text not null,
    -- Videos YouTube ranks for the phrase, how many it returned.
    videos int not null default 0,
    -- Summed views of the top ten.
    views_top10 bigint not null default 0,
    -- Median views of the top ten: one viral video should not carry a topic.
    views_median bigint not null default 0,
    -- Clips under a minute dropped before taking the top ten. Bank adverts
    -- rank for their own brand with millions of bought views.
    ads_dropped int not null default 0,
    -- Videos published within the last year among the top ten: a topic
    -- where nothing new ranks is settled; one where everything is new is
    -- being fought over.
    fresh int not null default 0,
    -- Top videos: [{"title", "channel", "views", "age", "url"}, ...]
    top jsonb not null default '[]',
    checked_at timestamptz not null default now(),
    primary key (search_id, phrase)
);
