//! Whether a passage survives being quoted away from its page.
//!
//! Generative engines answer by lifting a passage and showing it without the
//! text around it. A section that opens with "To zależy od wielu czynników" or
//! "W tym artykule wyjaśnimy" gives them nothing to lift, however good the
//! paragraph that follows is.
//!
//! Measured against our own AI Overview data: Google's answers are 800-1400
//! characters, so the unit being competed for is a passage, not an article.
//! This module checks the opening sentence of each section, which is the part
//! an engine reaches first.

/// One section's opening, judged on whether it stands alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionOpening {
    pub heading: String,
    /// The first sentence under that heading.
    pub opening: String,
    /// Why it cannot be quoted alone. Empty when it can.
    pub problem: Option<&'static str>,
}

/// How quotable a draft's sections are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quotability {
    pub sections: Vec<SectionOpening>,
}

impl Quotability {
    pub fn total(&self) -> usize {
        self.sections.len()
    }

    pub fn standalone(&self) -> usize {
        self.sections.iter().filter(|s| s.problem.is_none()).count()
    }

    /// Share of sections that open with something quotable, 0-100.
    pub fn share(&self) -> usize {
        if self.sections.is_empty() {
            return 0;
        }
        self.standalone() * 100 / self.sections.len()
    }

    /// Only the sections worth fixing.
    pub fn weak(&self) -> Vec<&SectionOpening> {
        self.sections
            .iter()
            .filter(|s| s.problem.is_some())
            .collect()
    }
}

/// Openings that defer the answer instead of giving it.
///
/// Each entry is a phrase that, at the start of a section, means the reader has
/// been told nothing yet. Collected from the drafts and competitor pages in the
/// database rather than invented.
const HEDGES: &[(&str, &str)] = &[
    ("to zależy", "opens by deferring the answer"),
    ("zależy to", "opens by deferring the answer"),
    ("it depends", "opens by deferring the answer"),
    ("w tym artykule", "refers to the page instead of answering"),
    ("w tym wpisie", "refers to the page instead of answering"),
    ("w tym poradniku", "refers to the page instead of answering"),
    ("in this article", "refers to the page instead of answering"),
    ("in this guide", "refers to the page instead of answering"),
    ("poniżej", "points elsewhere on the page"),
    ("poniżej znajdziesz", "points elsewhere on the page"),
    ("below you", "points elsewhere on the page"),
    ("jak już wspomniano", "depends on earlier text"),
    ("jak wspomniano", "depends on earlier text"),
    ("as mentioned", "depends on earlier text"),
    ("as we saw", "depends on earlier text"),
    ("wiele osób zastanawia", "opens with throat-clearing"),
    ("many people wonder", "opens with throat-clearing"),
    ("zacznijmy od", "opens with throat-clearing"),
    ("let's start", "opens with throat-clearing"),
    ("na wstępie", "opens with throat-clearing"),
    ("warto wiedzieć", "opens with throat-clearing"),
];

/// Pronouns that, at the very start, point at something outside the passage.
const DANGLING: &[&str] = &[
    "to ", "ten ", "ta ", "te ", "tego ", "this ", "these ", "they ", "it ",
];

/// Splits markdown into (heading, first sentence) pairs.
fn openings(markdown: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut heading: Option<String> = None;
    let mut body = String::new();

    let flush = |heading: &Option<String>, body: &str, out: &mut Vec<(String, String)>| {
        let Some(h) = heading else { return };
        let first = body
            .split(|c| matches!(c, '.' | '!' | '?'))
            .map(str::trim)
            .find(|s| s.chars().count() > 15);
        if let Some(f) = first {
            out.push((h.clone(), f.to_string()));
        }
    };

    for line in markdown.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("## ") {
            flush(&heading, &body, &mut out);
            heading = Some(rest.trim().to_string());
            body.clear();
        } else if t.starts_with("# ") {
            // Title, not a section.
            continue;
        } else if heading.is_some() {
            // Bullets and tables are not prose openings.
            if t.starts_with('-') || t.starts_with('*') || t.starts_with('|') {
                continue;
            }
            body.push(' ');
            body.push_str(t);
        }
    }
    flush(&heading, &body, &mut out);
    out
}

/// Checks each section's opening sentence.
pub fn quotability(markdown: &str) -> Quotability {
    let sections = openings(markdown)
        .into_iter()
        .map(|(heading, opening)| {
            let l = opening.to_lowercase();
            let problem = HEDGES
                .iter()
                .find(|(phrase, _)| l.starts_with(phrase) || l.contains(phrase))
                .map(|(_, why)| *why)
                .or_else(|| {
                    DANGLING
                        .iter()
                        .find(|p| l.starts_with(*p))
                        .map(|_| "starts with a pronoun that needs earlier text")
                });
            SectionOpening {
                heading,
                opening,
                problem,
            }
        })
        .collect();

    Quotability { sections }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_first_opening_passes() {
        // Shaped like the FAQ answers we already produce, which is the standard
        // the article writer is being held to.
        let md = "## Ile kcal ma latte 250 ml?\n\n\
                  Kawa latte 250 ml ma około 90-150 kcal, w zależności od rodzaju mleka. \
                  Wersja z mlekiem chudym to około 90-110 kcal.";
        let q = quotability(md);
        assert_eq!(q.total(), 1);
        assert_eq!(q.standalone(), 1);
        assert_eq!(q.share(), 100);
    }

    #[test]
    fn deferred_answers_and_page_references_are_caught() {
        let md = "## Ile kalorii ma latte?\n\n\
                  To zależy od wielu czynników, które omówimy poniżej.\n\n\
                  ## Jak parzyć kawę?\n\n\
                  W tym artykule wyjaśnimy wszystkie metody parzenia kawy w domu.\n\n\
                  ## Jaka temperatura wody?\n\n\
                  Woda powinna mieć 92-95°C, ponieważ wyższa wypłukuje gorzkie związki.";
        let q = quotability(md);
        assert_eq!(q.total(), 3);
        assert_eq!(q.standalone(), 1, "only the temperature section answers");

        let weak = q.weak();
        assert_eq!(weak.len(), 2);
        assert!(weak[0].problem.unwrap().contains("deferring"));
        assert!(weak[1].problem.unwrap().contains("refers to the page"));
    }

    #[test]
    fn a_dangling_pronoun_needs_the_paragraph_before_it() {
        let md = "## Metoda Hoffmana\n\n\
                  Ta technika wymaga dwóch wlewów wody w równych proporcjach czasowych.";
        let q = quotability(md);
        assert_eq!(q.standalone(), 0);
        assert!(q.weak()[0].problem.unwrap().contains("pronoun"));
    }

    #[test]
    fn bullets_and_titles_are_not_openings() {
        let md = "# Tytuł artykułu\n\n\
                  Wprowadzenie do tematu, które nie jest sekcją.\n\n\
                  ## Czego potrzebujesz\n\n\
                  - dripper Hario V60 z papierowym filtrem\n\
                  - młynek żarnowy do kawy ziarnistej\n\n\
                  Potrzebujesz drippera, młynka żarnowego i wagi kuchennej z dokładnością do grama.";
        let q = quotability(md);
        assert_eq!(q.total(), 1, "the title is not a section");
        assert_eq!(
            q.sections[0].opening.chars().next(),
            Some('P'),
            "the bullet list must not be read as the opening: {:?}",
            q.sections[0].opening
        );
    }
}
