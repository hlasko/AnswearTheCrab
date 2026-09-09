-- A draft can be a revision of an earlier one.
--
-- "Write again" starts from nothing and loses the parts that were fine. A
-- revision carries the previous text and the reader's instruction ("shorten
-- the section on instalments, add a source for the 37%"), so the model edits
-- rather than rewrites. The chain is kept: each revision points at its parent.

alter table drafts add column if not exists parent_id uuid references drafts (id) on delete set null;
alter table drafts add column if not exists instruction text;
