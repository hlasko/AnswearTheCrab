//! What someone wants when they type a phrase.
//!
//! The four-way split (informational, navigational, commercial, transactional)
//! is standard, and the AnswerThePublic write-up leans on it: two people
//! searching the same topic can be at opposite ends of a buying decision, and
//! the piece you write for each is different.
//!
//! Classification is by wording, which is what we have. It is checked against
//! CPC rather than trusted blindly, because advertisers bid for intent: if a
//! bucket does not carry a higher price, the bucket is wrong. That check caught
//! the first version of this module, which sent the most expensive phrases in
//! the database ("kredyt hipoteczny doradca" at $15.46, "kredyt hipoteczny
//! łódź" at $8.92) to a nameless "other" pile, because it looked only for the
//! obvious "kup" and "cena".

use serde::{Deserialize, Serialize};

/// Why someone is searching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    /// Wants to know something. The bulk of question phrases.
    Informational,
    /// Wants a specific company, product line, branch or contact.
    Navigational,
    /// Comparing before deciding: reviews, rankings, "best", "vs".
    Commercial,
    /// Ready to act: buy, price, order, apply, book.
    Transactional,
}

impl Intent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Informational => "Informational",
            Self::Navigational => "Navigational",
            Self::Commercial => "Commercial",
            Self::Transactional => "Transactional",
        }
    }

    /// What to do with phrases in this bucket.
    pub fn advice(self) -> &'static str {
        match self {
            Self::Informational => {
                "People wanting to understand something. This is where answer engines quote \
                 you, and where a FAQ earns its place."
            }
            Self::Navigational => {
                "People looking for a named company, branch or contact. Only worth writing \
                 for if the name is yours."
            }
            Self::Commercial => {
                "People comparing before they decide. Comparisons, rankings and honest \
                 verdicts, with the criteria stated."
            }
            Self::Transactional => {
                "People ready to act. Prices, availability and the next step, not another \
                 introduction to the topic."
            }
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Informational => "informational",
            Self::Navigational => "navigational",
            Self::Commercial => "commercial",
            Self::Transactional => "transactional",
        }
    }
}

/// Ready to act: money changes hands, or a form gets submitted.
const TRANSACTIONAL: &[&str] = &[
    // Polish
    "kup",
    "kupić",
    "kupno",
    "cena",
    "ceny",
    "cennik",
    "kosztuje",
    "koszt",
    "sklep",
    "promocja",
    "promocje",
    "wyprzedaż",
    "tanio",
    "najtaniej",
    "zamów",
    "zamówienie",
    "dostawa",
    "raty",
    "wniosek",
    "wynajem",
    "rezerwacja",
    "zapisy",
    "kalkulator",
    "oferta",
    "oferty",
    // English
    "buy",
    "price",
    "prices",
    "pricing",
    "cost",
    "cheap",
    "cheapest",
    "discount",
    "deal",
    "deals",
    "sale",
    "order",
    "shipping",
    "shop",
    "coupon",
    "booking",
    "subscription",
];

/// Comparing options before deciding.
const COMMERCIAL: &[&str] = &[
    "najlepszy",
    "najlepsza",
    "najlepsze",
    "najlepszych",
    "ranking",
    "rankingu",
    "opinie",
    "opinia",
    "recenzja",
    "recenzje",
    "test",
    "testy",
    "porównanie",
    "porównać",
    "vs",
    "czy warto",
    "alternatywa",
    "alternatywy",
    "zamiennik",
    "polecane",
    "best",
    "top",
    "review",
    "reviews",
    "comparison",
    "compare",
    "versus",
    "alternative",
    "alternatives",
    "worth",
];

/// Looking for a named thing: a company, a branch, a contact channel.
///
/// These matter more than they look. In our data they carry the highest CPC of
/// any group, because a searcher naming a brand is deep in a decision.
const NAVIGATIONAL: &[&str] = &[
    "infolinia",
    "kontakt",
    "logowanie",
    "zaloguj",
    "oddział",
    "oddziały",
    "placówka",
    "placówki",
    "doradca",
    "doradcy",
    "wikipedia",
    "wiki",
    "adres",
    "godziny otwarcia",
    "login",
    "sign in",
    "contact",
    "customer service",
    "helpline",
    "branch",
    "near me",
    "w pobliżu",
];

/// Question openers, in the languages we classify.
const QUESTION_STARTS: &[&str] = &[
    "jak", "co", "czy", "dlaczego", "kiedy", "ile", "gdzie", "czym", "kto", "jaki", "jaka",
    "jakie", "który", "która", "które", "po co", "za ile", "what", "how", "why", "when", "where",
    "who", "which", "is", "are", "can", "does", "do", "will",
];

/// Service providers whose name in a phrase means "I am choosing a supplier".
///
/// Restricted to financial brands on purpose, and that restriction was forced
/// by measurement rather than taste. A first version included consumer
/// electronics and coffee makers, which dropped the navigational median from
/// $2.66 to $0.40: "philips" and "siemens" appear in 364 phrases at a median of
/// $0.33, because naming an appliance is how people ask questions *about* it.
/// Financial brands behave the opposite way, 281 phrases at $1.75, because
/// naming a bank means picking one.
///
/// The lesson generalises: a brand signals navigational intent only in
/// categories where the brand is the thing being chosen. Extending this list
/// should be justified by the same CPC check, not by adding familiar names.
const BRANDS: &[&str] = &[
    "ing",
    "pko",
    "bgk",
    "mbank",
    "santander",
    "millennium",
    "pekao",
    "alior",
    "unicredit",
    "getin",
    "raiffeisen",
    "citi",
    "citibank",
    "bnp",
    "revolut",
    "velobank",
    "noble",
    "bph",
    "eurobank",
    "stefczyka",
    // "bank" itself, not only named ones. Measured: 65 phrases containing the
    // word have a median CPC of $2.34 against $0.40 for everything else, so
    // someone writing "bank kredyt hipoteczny" is shopping for a provider even
    // when they have not settled on which.
    "bank",
    "banku",
    "banki",
    "banków",
];

/// The largest Polish cities, as a stand-in for local intent.
///
/// A city name turns a general phrase into a local one, and local phrases are
/// bought: "kredyt hipoteczny łódź" costs $8.92 in our data against a median of
/// $0.40. This list is short on purpose; it is a signal, not a gazetteer.
const CITIES: &[&str] = &[
    "warszawa",
    "warszawie",
    "kraków",
    "krakowie",
    "łódź",
    "łodzi",
    "wrocław",
    "wrocławiu",
    "poznań",
    "poznaniu",
    "gdańsk",
    "gdańsku",
    "szczecin",
    "szczecinie",
    "bydgoszcz",
    "lublin",
    "lublinie",
    "białystok",
    "katowice",
    "katowicach",
    "gdynia",
    "częstochowa",
    "radom",
    "toruń",
    "toruniu",
    "rzeszów",
    "olsztyn",
    "olsztynie",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn has_any(text: &str, ws: &[String], needles: &[&str]) -> bool {
    needles.iter().any(|n| {
        if n.contains(' ') {
            text.contains(n)
        } else {
            ws.iter().any(|w| w == n)
        }
    })
}

/// Classifies a phrase.
///
/// Order is by commitment, strongest first: someone typing "cena" has moved
/// past comparing, and someone naming a branch has moved past both. A phrase
/// with no signal at all is informational, which is the safe default: it is
/// both the largest real group and the least costly to be wrong about.
pub fn classify(text: &str) -> Intent {
    let lower = text.to_lowercase();
    let ws = words(&lower);

    if has_any(&lower, &ws, TRANSACTIONAL) {
        return Intent::Transactional;
    }
    if has_any(&lower, &ws, NAVIGATIONAL) || has_any(&lower, &ws, CITIES) {
        return Intent::Navigational;
    }
    // A brand named alongside a comparison word ("najlepszy laptop hp") is
    // still comparison shopping, so brands are checked after commercial intent
    // only when no comparison word is present.
    if has_any(&lower, &ws, BRANDS) && !has_any(&lower, &ws, COMMERCIAL) {
        return Intent::Navigational;
    }
    if has_any(&lower, &ws, COMMERCIAL) {
        return Intent::Commercial;
    }
    let _ = QUESTION_STARTS;
    Intent::Informational
}

#[cfg(test)]
mod realdata {
    use super::*;

    /// The honest test: advertisers bid for intent, so if these buckets mean
    /// anything the price should climb with commitment. Prints CPC per bucket
    /// across every phrase in the database.
    #[tokio::test]
    #[ignore = "needs a database; run with --ignored"]
    async fn cpc_rises_with_commitment() {
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".into());
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let rows: Vec<(String, Option<f64>, Option<i64>)> = sqlx::query_as(
            "select text, cpc, search_volume from suggestions where cpc is not null and cpc > 0",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let mut buckets: std::collections::BTreeMap<&str, Vec<f64>> = Default::default();
        for (text, cpc, _) in &rows {
            buckets
                .entry(classify(text).label())
                .or_default()
                .push(cpc.unwrap_or(0.0));
        }
        for (label, mut v) in buckets {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            println!(
                "{label}: n={} mean=${mean:.2} median=${:.2}",
                v.len(),
                v[v.len() / 2]
            );
        }
    }

    /// Guards the finding above: informational phrases must stay the cheapest.
    /// If a future edit to the word lists breaks that, the buckets have stopped
    /// tracking real intent and the feature is lying to the reader.
    #[tokio::test]
    #[ignore = "needs a database; run with --ignored"]
    async fn informational_is_the_cheapest_bucket() {
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/atp".into());
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let rows: Vec<(String, Option<f64>)> =
            sqlx::query_as("select text, cpc from suggestions where cpc is not null and cpc > 0")
                .fetch_all(&pool)
                .await
                .unwrap();

        let median = |mut v: Vec<f64>| -> f64 {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if v.is_empty() {
                0.0
            } else {
                v[v.len() / 2]
            }
        };
        let of = |want: Intent| -> f64 {
            median(
                rows.iter()
                    .filter(|(t, _)| classify(t) == want)
                    .filter_map(|(_, c)| *c)
                    .collect(),
            )
        };

        let info = of(Intent::Informational);
        for other in [
            Intent::Commercial,
            Intent::Transactional,
            Intent::Navigational,
        ] {
            assert!(
                of(other) > info,
                "{} median ${:.2} should exceed informational ${info:.2}",
                other.label(),
                of(other)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buying_words_win_over_question_words() {
        // "ile kosztuje" opens like a question but the person wants a price.
        assert_eq!(
            classify("ile kosztuje ekspres do kawy"),
            Intent::Transactional
        );
        assert_eq!(
            classify("gdzie kupić młynek do kawy"),
            Intent::Transactional
        );
        assert_eq!(classify("laptopy gamingowe na raty"), Intent::Transactional);
    }

    #[test]
    fn brands_branches_and_cities_are_navigational() {
        // Real phrases from the database, and the most expensive ones in it:
        // "kredyt hipoteczny doradca" at $15.46, "łódź" at $8.92.
        assert_eq!(classify("kredyt hipoteczny doradca"), Intent::Navigational);
        assert_eq!(
            classify("ing kredyt hipoteczny infolinia"),
            Intent::Navigational
        );
        assert_eq!(classify("kredyt hipoteczny łódź"), Intent::Navigational);
        assert_eq!(classify("unicredit wikipedia"), Intent::Navigational);
    }

    #[test]
    fn a_named_brand_is_someone_choosing_a_provider() {
        // Real phrases: "ing kredyt hipoteczny" landed in informational until
        // brands were recognised, despite costing far above the median.
        assert_eq!(classify("ing kredyt hipoteczny"), Intent::Navigational);
        assert_eq!(
            classify("kredyt hipoteczny z gwarancją bgk"),
            Intent::Navigational
        );
        assert_eq!(classify("mbank kredyt hipoteczny"), Intent::Navigational);
        // The generic word counts too: measured at a $2.34 median.
        assert_eq!(classify("bank kredyt hipoteczny"), Intent::Navigational);
        assert_eq!(classify("citibank kredyt hipoteczny"), Intent::Navigational);

        // But a comparison is a comparison, brand or not.
        assert_eq!(
            classify("najlepszy kredyt hipoteczny ing"),
            Intent::Commercial
        );
        // And a price question is still a purchase.
        assert_eq!(
            classify("ing kredyt hipoteczny cena"),
            Intent::Transactional
        );

        // Appliance brands are excluded deliberately: naming a coffee machine
        // is how people ask questions about it, and those phrases are cheap.
        assert_eq!(
            classify("ekspres philips jak odkamienić"),
            Intent::Informational
        );
    }

    #[test]
    fn comparing_is_not_the_same_as_buying() {
        assert_eq!(classify("najlepszy ekspres do kawy"), Intent::Commercial);
        assert_eq!(classify("ranking młynków do kawy"), Intent::Commercial);
        assert_eq!(classify("kawa vs herbata"), Intent::Commercial);
        assert_eq!(classify("best cold brew coffee"), Intent::Commercial);
    }

    #[test]
    fn plain_questions_are_informational() {
        assert_eq!(classify("czy kawa jest zdrowa"), Intent::Informational);
        assert_eq!(classify("jak parzyć kawę w dripie"), Intent::Informational);
        assert_eq!(classify("how bbq sauce is made"), Intent::Informational);
    }

    #[test]
    fn a_word_inside_another_word_does_not_count() {
        // "test" must not fire on "protestować", "top" not on "stop".
        assert_eq!(
            classify("jak protestować skutecznie"),
            Intent::Informational
        );
        assert_ne!(classify("stop klatka w filmie"), Intent::Commercial);
    }
}
