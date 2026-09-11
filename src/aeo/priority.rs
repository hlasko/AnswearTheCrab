//! What to write first, from the signals already paid for.
//!
//! By this point a search carries six or seven numbers per phrase: Google
//! volume, Bing volume, CPC, paid competition, year-on-year trend, the
//! PAA-derived "Asked" figure, and for a handful of topics the YouTube
//! appetite. Every one of them was justified on its own, and together they
//! were a wall: nothing said what to do on Monday.
//!
//! This ranks topics by a score whose parts are all measured, and says in
//! words why each one is where it is. The score is deliberately crude,
//! because a precise-looking number built on Google Ads volume buckets and
//! a modelled question count would be false precision. What it is for is
//! ordering, not measurement.

use crate::domain::Suggestion;

/// What kind of page the signals point to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Questions dominate: an answer-first page or an FAQ.
    Answers,
    /// Commercial phrases with real CPC: a comparison or a buyer's guide.
    Comparison,
    /// Plain volume with no question or buying signal: a reference article.
    Article,
}

impl Format {
    pub fn label(&self) -> &'static str {
        match self {
            Format::Answers => "Answers / FAQ",
            Format::Comparison => "Comparison",
            Format::Article => "Article",
        }
    }
}

/// One topic, scored, with the reason in words.
#[derive(Debug, Clone)]
pub struct Priority {
    pub topic: String,
    pub phrases: usize,
    pub volume: i64,
    /// Median paid competition across the topic's phrases, 0-100.
    pub competition: Option<i32>,
    /// Summed "Asked" figure: how question-shaped the topic is.
    pub asked: i64,
    /// Best year-on-year trend among its phrases.
    pub trend: Option<i32>,
    pub cpc: Option<f64>,
    pub format: Format,
    pub score: i64,
    /// Why it sits where it does, in one sentence.
    pub why: String,
}

/// Ranks a run's topics. `clusters` comes from `aeo::cluster`.
pub fn rank(clusters: &[crate::aeo::Cluster]) -> Vec<Priority> {
    // The scale is set by the run itself: a topic with 40% of the biggest
    // topic's volume scores 40 on demand, whatever the absolute numbers are.
    // Comparing against a fixed scale would make every niche look worthless.
    let max_vol = clusters.iter().map(|c| c.volume).max().unwrap_or(0).max(1);
    let max_asked: i64 = clusters
        .iter()
        .map(|c| c.phrases.iter().filter_map(|s| s.ai_volume).sum::<i64>())
        .max()
        .unwrap_or(0)
        .max(1);

    let mut out: Vec<Priority> = clusters
        .iter()
        .filter(|c| c.len() > 1)
        .map(|c| {
            let asked: i64 = c.phrases.iter().filter_map(|s| s.ai_volume).sum();
            let competition = median_competition(&c.phrases);
            // The trend of the topic's biggest phrase, not the best trend in
            // it. The first live run said "growing 100% year on year" on
            // every row, because in a cluster of forty phrases one tiny one
            // always doubled.
            let trend = c
                .phrases
                .iter()
                .filter(|s| s.trend_yearly.is_some())
                .max_by_key(|s| s.search_volume.unwrap_or(0))
                .and_then(|s| s.trend_yearly);
            let questions = c
                .phrases
                .iter()
                .filter(|s| crate::aeo::classify(&s.text) == crate::aeo::Intent::Informational)
                .count();

            let demand = c.volume * 100 / max_vol;
            let asked_share = asked * 100 / max_asked;
            // Low paid competition means the phrase is cheap to rank for as
            // well as to buy; 50 when unknown, so a run without the metric
            // still orders by demand.
            let openness = competition.map(|v| 100 - v as i64).unwrap_or(50);
            let momentum = trend.map(|t| t.clamp(-50, 100) as i64).unwrap_or(0);

            // Demand dominates, openness is the multiplier everyone forgets,
            // and the question and trend terms are tie-breakers rather than
            // drivers.
            let score = demand * 4 + openness * 2 + asked_share + momentum;

            let commercial = c.cpc.unwrap_or(0.0) >= 1.0;
            let format = if asked_share >= 40 || questions * 2 >= c.len() {
                Format::Answers
            } else if commercial {
                Format::Comparison
            } else {
                Format::Article
            };

            let why = reason(demand, openness, asked_share, momentum, competition);

            Priority {
                topic: c.label.clone(),
                phrases: c.len(),
                volume: c.volume,
                competition,
                asked,
                trend,
                cpc: c.cpc,
                format,
                score,
                why,
            }
        })
        .collect();

    out.sort_by(|a, b| b.score.cmp(&a.score).then(b.volume.cmp(&a.volume)));
    out
}

fn median_competition(phrases: &[Suggestion]) -> Option<i32> {
    let mut v: Vec<i32> = phrases.iter().filter_map(|s| s.competition).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// The one sentence that makes the row actionable.
///
/// Named after whichever part of the score is doing the work, so two topics
/// with the same number do not get the same explanation.
fn reason(
    demand: i64,
    openness: i64,
    asked: i64,
    momentum: i64,
    competition: Option<i32>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if demand >= 60 {
        parts.push("most of this run's demand".into());
    } else if demand <= 15 {
        parts.push("a small slice of the demand".into());
    }
    match competition {
        Some(c) if c <= 30 => parts.push(format!("almost nobody bidding ({c}/100)")),
        Some(c) if c >= 80 => parts.push(format!("heavily bid on ({c}/100)")),
        _ => {}
    }
    if asked >= 50 {
        parts.push("asked as questions more than most".into());
    }
    if momentum >= 25 {
        parts.push(format!("its main phrase up {momentum}% year on year"));
    } else if momentum <= -30 {
        parts.push(format!(
            "its main phrase down {}% year on year",
            momentum.abs()
        ));
    }
    if parts.is_empty() {
        let _ = openness;
        return "middling on every signal: worth writing once the clear wins are done".into();
    }
    let mut s = parts.join(", ");
    if let Some(first) = s.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    s.push('.');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aeo::Cluster;

    fn phrase(text: &str, vol: i64, comp: Option<i32>, asked: Option<i64>) -> Suggestion {
        let mut s = Suggestion::new(text, "alphabetical", "");
        s.search_volume = Some(vol);
        s.competition = comp;
        s.ai_volume = asked;
        s
    }

    fn cluster(label: &str, phrases: Vec<Suggestion>, cpc: Option<f64>) -> Cluster {
        Cluster {
            label: label.into(),
            stem: label.into(),
            volume: phrases.iter().filter_map(|s| s.search_volume).sum(),
            cpc,
            bing_volume: None,
            phrases,
        }
    }

    #[test]
    fn an_open_topic_outranks_a_bigger_contested_one() {
        let big = cluster(
            "big",
            vec![
                phrase("big a", 10_000, Some(95), None),
                phrase("big b", 10_000, Some(95), None),
            ],
            None,
        );
        let open = cluster(
            "open",
            vec![
                phrase("open a", 8_000, Some(5), None),
                phrase("open b", 8_000, Some(5), None),
            ],
            None,
        );
        let r = rank(&[big, open]);
        assert_eq!(r[0].topic, "open", "openness must be able to beat raw size");
        assert!(r[0].why.contains("almost nobody bidding"));
    }

    #[test]
    fn question_shaped_topics_are_marked_as_answers() {
        let c = cluster(
            "asked",
            vec![
                phrase("jak zrobić x", 500, Some(20), Some(80)),
                phrase("czy warto x", 500, Some(20), Some(80)),
            ],
            None,
        );
        let plain = cluster(
            "plain",
            vec![
                phrase("x cena", 5_000, Some(20), Some(1)),
                phrase("x sklep", 5_000, Some(20), Some(1)),
            ],
            Some(2.50),
        );
        let r = rank(&[c, plain]);
        let asked = r.iter().find(|p| p.topic == "asked").unwrap();
        let plain = r.iter().find(|p| p.topic == "plain").unwrap();
        assert_eq!(asked.format, Format::Answers);
        assert_eq!(plain.format, Format::Comparison);
    }

    #[test]
    fn the_trend_comes_from_the_biggest_phrase_not_the_luckiest() {
        let mut big = phrase("main", 10_000, Some(20), None);
        big.trend_yearly = Some(-40);
        let mut tiny = phrase("tail", 10, Some(20), None);
        tiny.trend_yearly = Some(900);
        let c = cluster("t", vec![big, tiny], None);
        let r = rank(&[c]);
        assert_eq!(r[0].trend, Some(-40));
        assert!(r[0].why.contains("down 40%"), "{}", r[0].why);
    }

    #[test]
    fn singletons_are_not_topics() {
        let one = cluster("lonely", vec![phrase("lonely", 100, None, None)], None);
        assert!(rank(&[one]).is_empty());
    }
}
