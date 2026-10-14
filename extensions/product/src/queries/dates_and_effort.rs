use super::{of_kind, out, share, status, status_is, text};
use crate::vocabulary::*;
use serde::Serialize;
use specforge_extension_sdk::prelude::CommandGraph;

/// The days from 1970-01-01 to a `YYYY-MM-DD` date (negative before it);
/// `None` for anything else: another shape, or a day its month does not
/// have.
pub fn parse_ymd(date: &str) -> Option<i64> {
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |at: std::ops::Range<usize>| -> Option<i64> {
        let part = &b[at];
        part.iter().all(u8::is_ascii_digit).then(|| {
            part.iter()
                .fold(0, |n, &digit| n * 10 + i64::from(digit - b'0'))
        })
    };
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days = [
        31,
        28 + i64::from(leap),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month) || day < 1 || day > month_days[(month - 1) as usize] {
        return None;
    }
    // Days from civil (H. Hinnant): years start in March, so a leap day
    // ends one.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// `MilestoneTimelineEntry`: the milestone's fields as written, and
/// whether it is overdue.
#[derive(Debug, Serialize)]
pub struct MilestoneTimelineEntry {
    pub milestone_id: String,
    pub target_date: Option<String>,
    pub status: Option<String>,
    pub is_overdue: bool,
    pub priority: Option<String>,
}

/// `MilestoneTimelinePayload`.
#[derive(Debug, Serialize)]
pub struct MilestoneTimeline {
    pub milestones: Vec<MilestoneTimelineEntry>,
    pub overdue_count: usize,
}

/// Every milestone, by target date, earliest first, then by id; one
/// without a target date that is a date comes after every dated one, by
/// id. A milestone is overdue when its target date is before `as_of`
/// (days, as [`parse_ymd`] counts them) and it is not `completed` (one
/// without a status is `planned`).
pub fn milestone_timeline(graph: &CommandGraph, as_of: i64) -> MilestoneTimeline {
    let mut milestones: Vec<(Option<i64>, MilestoneTimelineEntry)> = graph
        .nodes_of_kind("milestone")
        .map(|m| {
            let date = m.text("target_date").and_then(parse_ymd);
            let entry = MilestoneTimelineEntry {
                milestone_id: m.id.clone(),
                target_date: text(m, "target_date"),
                status: text(m, "status"),
                is_overdue: date.is_some_and(|d| d < as_of)
                    && status(m) != Some(MILESTONE_COMPLETED),
                priority: text(m, "priority"),
            };
            (date, entry)
        })
        .collect();
    milestones.sort_by(|(a, x), (b, y)| {
        (a.is_none(), a, &x.milestone_id).cmp(&(b.is_none(), b, &y.milestone_id))
    });
    let milestones: Vec<MilestoneTimelineEntry> = milestones.into_iter().map(|(_, e)| e).collect();
    MilestoneTimeline {
        overdue_count: milestones.iter().filter(|m| m.is_overdue).count(),
        milestones,
    }
}

/// `MilestoneVelocityPayload`.
#[derive(Debug, Serialize)]
pub struct MilestoneVelocity {
    pub milestone_id: String,
    pub total_features: usize,
    pub done_features: usize,
    pub in_progress_features: usize,
    pub remaining_features: usize,
    pub completion_ratio: Option<f64>,
    pub days_elapsed: Option<i64>,
    pub days_remaining: Option<i64>,
    pub features_per_day: Option<f64>,
}

/// The milestone's features by status (`done`, `in_progress`, the rest
/// remaining; one without a status is `proposed`), the share done (`null`
/// without features), the days from its `start_date` (else its
/// `target_date`) to `as_of`, 0 before it starts (`null` without either
/// date), the features done per elapsed day (`null` while none is done or
/// no day has elapsed) and the days that pace needs for the features not
/// done, rounded up (0 when none is left; `null` without a date or a
/// pace). `None` when `milestone_id` is not a milestone.
pub fn milestone_velocity(
    graph: &CommandGraph,
    milestone_id: &str,
    as_of: i64,
) -> Option<MilestoneVelocity> {
    let node = of_kind(graph, milestone_id, "milestone")?;
    let features = out(graph, milestone_id, "features", "feature");
    let count = |wanted: &str| {
        features
            .iter()
            .filter(|f| status_is(graph, f, wanted))
            .count()
    };
    let (total, done, in_progress) = (
        features.len(),
        count(FEATURE_DONE),
        count(FEATURE_IN_PROGRESS),
    );
    let start = node
        .text("start_date")
        .and_then(parse_ymd)
        .or_else(|| node.text("target_date").and_then(parse_ymd));
    let days_elapsed = start.map(|start| (as_of - start).max(0));
    let features_per_day = days_elapsed
        .filter(|&days| days > 0 && done > 0)
        .map(|days| done as f64 / days as f64);
    let left = total - done;
    // left / (done / days), rounded up, in integers: a float pace can land
    // a whole quotient just above itself and round it up a day.
    let days_remaining = days_elapsed.and_then(|days| {
        if left == 0 {
            Some(0)
        } else {
            features_per_day.map(|_| {
                let days = u64::try_from(days).unwrap_or(0);
                i64::try_from((left as u64 * days).div_ceil(done as u64)).unwrap_or(i64::MAX)
            })
        }
    });
    Some(MilestoneVelocity {
        milestone_id: milestone_id.to_string(),
        total_features: total,
        done_features: done,
        in_progress_features: in_progress,
        remaining_features: total - done - in_progress,
        completion_ratio: share(done, total),
        days_elapsed,
        days_remaining,
        features_per_day,
    })
}

/// Each `Effort` level's weight, in [`EFFORT`]'s order (xs=1, s=2, m=3,
/// l=5, xl=8): the scale's definition, not a setting (ADR 0011).
pub const EFFORT_WEIGHTS: [u64; EFFORT.values().len()] = [1, 2, 3, 5, 8];

/// The level (`m`, in [`EFFORT`]) a feature without an effort on the
/// scale weighs as.
const DEFAULT_EFFORT: usize = EFFORT.index("m");

/// `EffortBreakdownEntry`: how many of the milestone's features have the
/// effort level, and how many of those are done.
#[derive(Debug, Serialize)]
pub struct EffortBreakdownEntry {
    pub effort_level: String,
    pub total: usize,
    pub done: usize,
}

/// `WeightedMilestoneCompletionPayload`.
#[derive(Debug, Serialize)]
pub struct WeightedMilestoneCompletion {
    pub milestone_id: String,
    pub total_effort: u64,
    pub done_effort: u64,
    pub completion_ratio: Option<f64>,
    pub effort_breakdown: Vec<EffortBreakdownEntry>,
}

/// The milestone's features weighted by effort (xs=1, s=2, m=3, l=5,
/// xl=8; a feature without an effort on the scale weighs as m): the
/// weights of all and of the `done` ones, their ratio (`null` without
/// features) and the features per effort level they have, smallest first.
/// `None` when `milestone_id` is not a milestone.
pub fn weighted_milestone_completion(
    graph: &CommandGraph,
    milestone_id: &str,
) -> Option<WeightedMilestoneCompletion> {
    of_kind(graph, milestone_id, "milestone")?;
    let mut levels = [(0, 0); EFFORT.values().len()];
    for f in out(graph, milestone_id, "features", "feature") {
        let level = graph
            .node(&f)
            .and_then(|n| n.text("effort"))
            .and_then(|e| EFFORT.position(e))
            .unwrap_or(DEFAULT_EFFORT);
        levels[level].0 += 1;
        if status_is(graph, &f, FEATURE_DONE) {
            levels[level].1 += 1;
        }
    }
    let weigh = |pick: fn(&(usize, usize)) -> usize| -> u64 {
        levels
            .iter()
            .zip(EFFORT_WEIGHTS)
            .map(|(counts, weight)| pick(counts) as u64 * weight)
            .sum()
    };
    let (total_effort, done_effort) = (weigh(|c| c.0), weigh(|c| c.1));
    Some(WeightedMilestoneCompletion {
        milestone_id: milestone_id.to_string(),
        total_effort,
        done_effort,
        completion_ratio: (total_effort > 0).then(|| done_effort as f64 / total_effort as f64),
        effort_breakdown: levels
            .iter()
            .zip(EFFORT.values())
            .filter(|((total, _), _)| *total > 0)
            .map(|(&(total, done), level)| EffortBreakdownEntry {
                effort_level: (*level).to_string(),
                total,
                done,
            })
            .collect(),
    })
}

/// The weight of the effort level `level` names; an unknown one weighs
/// as m.
pub fn effort_weight(level: &str) -> u64 {
    EFFORT_WEIGHTS[EFFORT.position(level).unwrap_or(DEFAULT_EFFORT)]
}
