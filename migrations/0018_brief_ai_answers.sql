-- What the answer engine already says about a brief's topic.
--
-- A brief already carries Google's AI Overview and the ranking pages'
-- outlines. It did not carry the other answer: what Perplexity replies when
-- someone asks this topic's questions, and which sites it leans on. That is
-- the text a new page has to be better than, so the writer should see it.
--
-- Copied into the brief rather than joined at read time, for the same reason
-- as ai_overview: answers change, and a brief is a record of a moment.

alter table briefs add column if not exists ai_answers jsonb not null default '[]';
