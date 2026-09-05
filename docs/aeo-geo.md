# AEO and GEO: what the data says

Notes on optimising for answer engines (AEO) and generative engines (GEO)
rather than for classic ranking. Everything here is either measured from our
own database or taken from a published experiment, with the source named. It
exists so that feature decisions are argued from evidence instead of from the
folklore that surrounds this topic.

## What our own briefs show

Measured over the briefs in the database: 12 finished briefs, 5 of them with an
AI Overview, 31 citations in total.

**74% of citations are pages that already rank.** 23 of the 31 cited domains
also appear in the same query's organic top 10. AEO is not a separate game from
ranking, it is a layer on top of it. Without being in the results there is
nothing to optimise.

**Cited positions run from 2 to 11, clustered in 6-10.** Being first is not a
condition of being quoted. This is the encouraging part: citability is won with
the content itself rather than with domain authority.

**The AI Overview appears for 5 of 12 queries, and not at random.** It shows up
for informational questions ("czy kawa jest zdrowa", "jak parzyć kawę w dripie")
and is absent for transactional and brand queries ("hp laptopy gamingowe",
"gdzie najlepiej kupować laptopy gamingowe"). The brief already reports this,
and it is a usable steering signal: on a query with no AI answer, classic
ranking still decides visibility.

**Answers are 800-1400 characters long.** That is the entire budget being
competed for. A paragraph fits in it, not an article.

## What the research shows

From *GEO: Generative Engine Optimization* (Aggarwal et al., KDD 2024,
arXiv:2311.09735), which tested nine content strategies over a 10k-query
benchmark. Scores are the Position-Adjusted Word Count impression metric,
against a no-optimisation baseline of 19.3:

| Strategy | Score | vs baseline |
| --- | --- | --- |
| Quotation Addition | 27.2 | +41% |
| Statistics Addition | 25.2 | +31% |
| Cite Sources | 24.6 | +27% |
| Fluency Optimization | 24.7 | +28% |
| Easy-to-Understand | 22.0 | +14% |
| **Keyword Stuffing** | **17.7** | **-8%** |

Two findings matter for us.

**Keyword stuffing scored below doing nothing at all.** The classic SEO move is
actively counterproductive in generative engines. This is the strongest
argument against copying a term-density editor without thinking, and the reason
the NeuronWriter-style feature sits in the backlog as a diagnosis rather than a
target.

**The gains are largest for pages that rank low.** From the paper's Table 2,
when every source is optimised, Cite Sources moved rank-5 pages by +115.1% while
the rank-1 page *lost* 30.3% of its visibility. Quotation Addition and
Statistics Addition behave the same way. Combined with our own finding that
cited pages cluster at positions 6-10, this says the leverage is precisely where
our users are: pages that rank respectably but are not first.

## What follows for this app

1. **Citability over term density.** A generative engine quotes a passage that
   survives being lifted out of its page. That is exactly what the FAQ writer
   already produces, which makes it stronger AEO than a term editor. The same
   idea extends to articles: open every section with a self-contained answer.
2. **Statistics, numbers and quotations.** The three techniques with measured
   effect. They are content requirements, not style settings, and belong in the
   brief and in the writing prompt.
3. **The gap against the AI Overview.** We hold Google's own answer text, so we
   can compute what it does not say that the ranking pages do cover. That turns
   "write something better" into a specific instruction: add what the summary
   cannot carry.
4. **Tracking whether you get cited.** Cited domains are already stored per run,
   so the change over time is computable. It is the only way to check whether any
   of the above works instead of believing it does.
