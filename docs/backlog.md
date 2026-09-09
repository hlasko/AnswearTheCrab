# Backlog

Ideas that are worth doing but are not being built yet. Each entry says what it
is, what it would take, and what has already been checked, so picking one up
does not start from zero.

## NeuronWriter-style term editor

An editor with a live list of terms taken from the pages that already rank, and
a counter showing how much of that list the text covers. NeuronWriter's core is
not the editor: it is the weighted term list plus a coverage score.

**Why it is cheap for us.** Measured, not assumed: the `content_parsing` call we
already make for each competitor returns the page's full body text, not just its
headings. One real competitor URL from our own database returned 19 sections and
9,697 characters of text for $0.0015, which is a call we are already paying for.
We keep the headings and throw the text away. The corpus needed for term
extraction is therefore already within reach at no extra API cost.

**Shape of the work.**

1. ~~Store the body text per competitor.~~ Done: `brief_competitors.content`,
   added for the fact checker. The corpus this editor needs now exists.
2. Extract weighted terms locally: 1-3 word n-grams, weighted by how many of the
   ranking pages use each one, with the median number of uses. Polish needs a
   stopword list and light lemmatisation, otherwise "kawa" and "kawy" count as
   two separate terms.
3. Editor with a live coverage counter as the writer types.
4. Feed the term list into the writing prompt so a draft starts with coverage
   rather than being graded for it afterwards.

**Open question, deliberately unresolved.** Step 4 pulls against a decision
already made in `brief_prompt`: our instructions are about *what to cover*, not
which strings to repeat, because keyword-density prose reads badly. A term
counter can be a diagnosis ("competitors all discuss grind size, you have not")
or a target to game. It is worth building as the former and not the latter.
