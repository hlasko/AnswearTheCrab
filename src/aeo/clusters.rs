//! Groups phrases into the topics they are really about.
//!
//! Seven hundred phrases about a keyword are not seven hundred topics. "kredyt
//! hipoteczny kalkulator", "kalkulator kredyt hipoteczny", "ing kredyt
//! hipoteczny kalkulator" and 85 others are one topic, and someone writing
//! about it wants one brief for the cluster, not one per variant.
//!
//! Measured on "kredyt hipoteczny" (699 phrases): grouping by the dominant
//! content stem puts 603 phrases into 92 clusters and leaves 96 alone. The
//! "kalkulator" cluster alone carries 88 phrases and 81K monthly searches,
//! which is the kind of thing a flat list hides.
//!
//! Local and free: stems and counts, no API. The stem length and stopword
//! list are shared with the gap detector, which needed the same tools.

use crate::domain::Suggestion;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// A topic and the phrases that belong to it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cluster {
    /// The most searched phrase, standing in for the cluster's name.
    pub label: String,
    /// The stem the cluster was built on, for debugging and stable ids.
    pub stem: String,
    pub phrases: Vec<Suggestion>,
    /// Summed monthly volume across the phrases, when known.
    pub volume: i64,
    /// Median CPC across phrases that have one.
    pub cpc: Option<f64>,
}

impl Cluster {
    pub fn len(&self) -> usize {
        self.phrases.len()
    }
    pub fn is_empty(&self) -> bool {
        self.phrases.is_empty()
    }
}

/// Function words that carry no topic, in the languages we handle.
const STOPWORDS: &[&str] = &[
    "jak", "co", "czy", "to", "jest", "sie", "się", "nie", "dla", "przy", "oraz", "lub", "ale",
    "tez", "też", "byc", "być", "ma", "na", "do", "od", "za", "po", "we", "ze", "że", "i", "a",
    "o", "u", "w", "z", "ktory", "który", "która", "które", "jego", "jej", "ich", "tym", "tej",
    "ten", "ta", "te", "tego", "sa", "są", "bez", "pod", "nad", "przed", "przez", "ile", "kiedy",
    "gdzie", "czym", "kto", "jaki", "jaka", "jakie", "jakim", "the", "and", "for", "with", "what",
    "how", "why", "when", "does", "can", "you", "your", "are", "is", "of", "to", "in", "on", "at",
    "it", "its", "this", "that", "an", "or", "but", "from", "about", "into", "than", "then",
    "they", "their", "where", "who", "which", "best", "vs",
];

/// Truncation length. Five characters: enough to merge Polish inflections
/// ("kalkulator"/"kalkulacja" both give "kalku") without merging unrelated
/// words as four would ("mielenie"/"mieszanie").
const STEM: usize = 5;

fn stems(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 2 && !STOPWORDS.contains(w))
        // A year is a qualifier, not a topic: "ranking 2025" and "ranking
        // 2026" are the ranking topic, not two year topics.
        .filter(|w| !(w.len() == 4 && w.starts_with("20") && w.chars().all(|c| c.is_ascii_digit())))
        .map(|w| w.chars().take(STEM).collect::<String>())
        .filter(|s| !STOPWORDS.contains(&s.as_str()))
        .collect()
}

/// Clusters phrases around the keyword they were harvested for.
///
/// The seed's own stems are removed first, since every phrase contains them
/// and they would put everything in one cluster. Each phrase then joins the
/// cluster of its most frequent remaining stem: the word most other phrases
/// share is the one most likely to name the topic. Phrases with nothing left
/// after the seed ("kredyt hipoteczny", "kredyt hipoteczny 2025") form the
/// seed's own cluster.
pub fn cluster(seed: &str, phrases: &[Suggestion]) -> Vec<Cluster> {
    let seed_stems: Vec<String> = stems(seed);

    let stemmed: Vec<(usize, Vec<String>)> = phrases
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut st = stems(&s.text);
            st.retain(|x| !seed_stems.contains(x));
            st.sort();
            st.dedup();
            (i, st)
        })
        .collect();

    // How many phrases each stem appears in: the signal for "what is this
    // group of phrases about".
    let mut freq: HashMap<&str, usize> = HashMap::new();
    for (_, st) in &stemmed {
        for s in st {
            *freq.entry(s.as_str()).or_default() += 1;
        }
    }

    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, st) in &stemmed {
        let key = st
            .iter()
            .max_by_key(|s| (freq[s.as_str()], std::cmp::Reverse(s.len())))
            .cloned()
            .unwrap_or_else(|| seed.to_lowercase());
        groups.entry(key).or_default().push(*i);
    }

    let mut out: Vec<Cluster> = groups
        .into_iter()
        .map(|(stem, idx)| {
            let mut members: Vec<Suggestion> = idx.iter().map(|&i| phrases[i].clone()).collect();
            members.sort_by(|a, b| b.search_volume.cmp(&a.search_volume));
            let volume = members.iter().filter_map(|s| s.search_volume).sum();
            let mut cpcs: Vec<f64> = members
                .iter()
                .filter_map(|s| s.cpc)
                .filter(|c| *c > 0.0)
                .collect();
            cpcs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let cpc = cpcs.get(cpcs.len() / 2).copied();
            Cluster {
                label: members[0].text.clone(),
                stem,
                phrases: members,
                volume,
                cpc,
            }
        })
        .collect();

    // Biggest topics first: by volume where known, then by size, which is
    // what stands in for volume when a source does not carry it.
    out.sort_by(|a, b| b.volume.cmp(&a.volume).then(b.len().cmp(&a.len())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str, vol: i64) -> Suggestion {
        Suggestion {
            text: text.into(),
            category: String::new(),
            modifier: String::new(),
            search_volume: Some(vol),
            cpc: None,
            competition: None,
            monthly: Vec::new(),
            trend_yearly: None,
            trend_quarterly: None,
        }
    }

    #[test]
    fn word_order_and_inflection_land_in_one_cluster() {
        // Real phrases: five spellings of the same topic.
        let phrases = vec![
            s("kredyt hipoteczny kalkulator", 40500),
            s("kalkulator kredyt hipoteczny", 12100),
            s("kalkulacja kredyt hipoteczny", 12100),
            s("kredyt kalkulator hipoteczny", 90),
            s("ing kredyt hipoteczny kalkulator", 1900),
            s("kredyt hipoteczny oprocentowanie", 1900),
        ];
        let c = cluster("kredyt hipoteczny", &phrases);
        let kalk = c
            .iter()
            .find(|c| c.stem == "kalku")
            .expect("a kalkulator cluster");
        assert_eq!(
            kalk.len(),
            5,
            "{:?}",
            kalk.phrases.iter().map(|p| &p.text).collect::<Vec<_>>()
        );
        assert_eq!(
            kalk.label, "kredyt hipoteczny kalkulator",
            "labelled by its most searched phrase"
        );
        assert_eq!(kalk.volume, 40500 + 12100 + 12100 + 90 + 1900);
        // The unrelated phrase stays out.
        assert!(!kalk
            .phrases
            .iter()
            .any(|p| p.text.contains("oprocentowanie")));
    }

    #[test]
    fn the_seed_alone_forms_its_own_cluster() {
        let phrases = vec![
            s("kredyt hipoteczny", 33100),
            s("kredyt hipoteczny 2025", 500),
            s("kredyt hipoteczny ing", 8100),
        ];
        let c = cluster("kredyt hipoteczny", &phrases);
        let seed = c
            .iter()
            .find(|c| c.stem == "kredyt hipoteczny")
            .expect("seed cluster");
        assert_eq!(seed.len(), 2, "bare seed and seed+year, not the ing one");
    }

    #[test]
    fn a_brand_that_shares_a_word_with_the_topic_joins_the_more_common_stem() {
        // "ing kredyt hipoteczny kalkulator": both "ing" and "kalku" are
        // candidates. It should follow whichever is the bigger topic.
        let phrases = vec![
            s("kredyt hipoteczny kalkulator", 40500),
            s("kalkulator kredyt hipoteczny", 12100),
            s("pko kredyt hipoteczny kalkulator", 1900),
            s("ing kredyt hipoteczny kalkulator", 1900),
            s("ing kredyt hipoteczny", 8100),
        ];
        let c = cluster("kredyt hipoteczny", &phrases);
        let kalk = c.iter().find(|c| c.stem == "kalku").unwrap();
        assert_eq!(
            kalk.len(),
            4,
            "kalkulator appears in 4 phrases, ing in 2, so the bank variants follow kalkulator"
        );
    }

    #[test]
    fn biggest_topic_comes_first() {
        let phrases = vec![
            s("kredyt hipoteczny ranking", 4400),
            s("kredyt hipoteczny kalkulator", 40500),
            s("kalkulator kredyt hipoteczny", 12100),
        ];
        let c = cluster("kredyt hipoteczny", &phrases);
        assert_eq!(c[0].stem, "kalku");
    }
}
