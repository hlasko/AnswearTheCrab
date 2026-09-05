-- What a draft is: a full article, or just the FAQ block.
--
-- The People Also Ask questions are already usable as FAQ headings, so writing
-- only the answers is a much smaller and cheaper job than a whole article, and
-- it is often all someone needs to add to a page that already exists.
--
-- Existing rows are articles, which is what the single button used to produce.

alter table drafts add column if not exists kind text not null default 'article';
