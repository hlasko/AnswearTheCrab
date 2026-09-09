-- Body text of each competing page.
--
-- The content_parsing call we already make returns the full article text, not
-- only the headings; measured at ~9.7k characters for one real competitor. We
-- kept the outline and threw the text away. Keeping it costs nothing extra and
-- is what lets a draft's claims be checked against pages that actually rank.

alter table brief_competitors add column if not exists content text;
