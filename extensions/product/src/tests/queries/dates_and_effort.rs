use super::Rng;
use crate::queries::*;
use crate::tests::G;

#[test]
fn a_date_is_its_days_since_the_epoch_and_anything_else_is_none() {
    assert_eq!(parse_ymd("1970-01-01"), Some(0));
    assert_eq!(parse_ymd("1970-01-02"), Some(1));
    assert_eq!(parse_ymd("1969-12-31"), Some(-1));
    assert_eq!(parse_ymd("2000-03-01"), Some(11_017));
    assert_eq!(parse_ymd("2026-10-03"), Some(20_729));
    assert_eq!(
        parse_ymd("2024-02-29").map(|d| d + 1),
        parse_ymd("2024-03-01")
    );
    for bad in [
        "",
        "2026-1-03",
        "2026/10/03",
        "2026-13-01",
        "2026-00-10",
        "2026-02-29",
        "1900-02-29",
        "2026-04-31",
        "2026-10-00",
        "20261003xx",
        "2026-10-03T00",
        "２０２６-10-03",
        "+026-10-03",
    ] {
        assert_eq!(parse_ymd(bad), None, "{bad}");
    }
    assert_eq!(
        parse_ymd("2000-02-29").map(|d| d + 1),
        parse_ymd("2000-03-01")
    );
}

#[test]
fn every_day_from_1600_to_2400_follows_the_one_before() {
    // Walk the calendar by hand: each valid date is one day after the last.
    let mut expected = parse_ymd("1600-01-01").unwrap();
    for year in 1600..2400 {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        for (month, days) in [
            31,
            if leap { 29 } else { 28 },
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
        ]
        .into_iter()
        .enumerate()
        {
            for day in 1..=days {
                let date = format!("{year:04}-{:02}-{day:02}", month + 1);
                assert_eq!(parse_ymd(&date), Some(expected), "{date}");
                expected += 1;
            }
            assert_eq!(
                parse_ymd(&format!("{year:04}-{:02}-{:02}", month + 1, days + 1)),
                None
            );
        }
    }
}

#[test]
fn the_timeline_is_by_date_then_id_and_overdue_before_as_of() {
    let g = G::default()
        .node("late", "milestone", &[("target_date", "2026-01-10")])
        .node(
            "done",
            "milestone",
            &[("target_date", "2026-01-01"), ("status", "completed")],
        )
        .node("b", "milestone", &[("target_date", "2026-02-01")])
        .node("a", "milestone", &[("target_date", "2026-02-01")])
        .node("bad", "milestone", &[("target_date", "soon")])
        .n("undated", "milestone")
        .build();
    let as_of = parse_ymd("2026-02-01").unwrap();
    let t = milestone_timeline(&g, as_of);
    let order: Vec<(&str, bool)> = t
        .milestones
        .iter()
        .map(|m| (m.milestone_id.as_str(), m.is_overdue))
        .collect();
    // Due on the day is not overdue; a target date that is no date is
    // undated.
    assert_eq!(
        order,
        [
            ("done", false),
            ("late", true),
            ("a", false),
            ("b", false),
            ("bad", false),
            ("undated", false)
        ]
    );
    assert_eq!(t.overdue_count, 1);
    assert_eq!(t.milestones[4].target_date.as_deref(), Some("soon"));
}

#[test]
fn velocity_paces_the_features_left_by_those_done() {
    let g = G::default()
        .node(
            "ms",
            "milestone",
            &[("start_date", "2026-01-01"), ("target_date", "2026-12-31")],
        )
        .node("f1", "feature", &[("status", "done")])
        .node("f2", "feature", &[("status", "done")])
        .node("f3", "feature", &[("status", "in_progress")])
        .n("f4", "feature")
        .n("f5", "feature")
        .edge("ms", "f1", "features")
        .edge("ms", "f2", "features")
        .edge("ms", "f3", "features")
        .edge("ms", "f4", "features")
        .edge("ms", "f5", "features")
        .build();
    let day = |d: &str| parse_ymd(d).unwrap();
    let v = milestone_velocity(&g, "ms", day("2026-01-11")).unwrap();
    assert_eq!(
        (
            v.total_features,
            v.done_features,
            v.in_progress_features,
            v.remaining_features
        ),
        (5, 2, 1, 2)
    );
    assert_eq!(v.completion_ratio, Some(0.4));
    assert_eq!(v.days_elapsed, Some(10));
    assert_eq!(v.features_per_day, Some(0.2));
    // Three features left at 0.2 a day.
    assert_eq!(v.days_remaining, Some(15));
    // Before the start: no day elapsed, no pace.
    let v = milestone_velocity(&g, "ms", day("2025-12-01")).unwrap();
    assert_eq!(
        (v.days_elapsed, v.features_per_day, v.days_remaining),
        (Some(0), None, None)
    );
    assert!(milestone_velocity(&g, "f1", 0).is_none());
}

#[test]
fn weighted_completion_weighs_each_feature_by_its_effort() {
    let mut g = G::default().n("ms", "milestone").n("empty", "milestone");
    for (i, (effort, status)) in [
        ("xs", "done"),
        ("s", "done"),
        ("m", ""),
        ("l", "done"),
        ("xl", ""),
        ("", "done"),
        ("huge", ""),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("f{i}");
        let mut fields = Vec::new();
        if !effort.is_empty() {
            fields.push(("effort", effort));
        }
        if !status.is_empty() {
            fields.push(("status", status));
        }
        g = g.node(&id, "feature", &fields).edge("ms", &id, "features");
    }
    let g = g.build();
    let w = weighted_milestone_completion(&g, "ms").unwrap();
    // 1 + 2 + 3 + 5 + 8, and two that weigh as m.
    assert_eq!((w.total_effort, w.done_effort), (25, 11));
    assert_eq!(w.completion_ratio, Some(11.0 / 25.0));
    let breakdown: Vec<(&str, usize, usize)> = w
        .effort_breakdown
        .iter()
        .map(|e| (e.effort_level.as_str(), e.total, e.done))
        .collect();
    assert_eq!(
        breakdown,
        [
            ("xs", 1, 1),
            ("s", 1, 1),
            ("m", 3, 1),
            ("l", 1, 1),
            ("xl", 1, 0)
        ]
    );
    let empty = weighted_milestone_completion(&g, "empty").unwrap();
    assert_eq!((empty.total_effort, empty.completion_ratio), (0, None));
    assert!(empty.effort_breakdown.is_empty());
    assert_eq!(
        ["xs", "s", "m", "l", "xl", "huge"].map(effort_weight),
        [1, 2, 3, 5, 8, 3]
    );
}

#[test]
fn random_milestones_weigh_and_pace_as_brute_force_does() {
    let mut rng = Rng(0x94D0_49BB_1331_11EB);
    let efforts = ["xs", "s", "m", "l", "xl", "", "huge"];
    let statuses = ["done", "in_progress", "proposed", ""];
    for round in 0..300 {
        let n = rng.below(10) as usize;
        let mut g = G::default().node("ms", "milestone", &[("start_date", "2026-01-01")]);
        let (mut total, mut done_weight, mut done, mut in_progress) = (0, 0, 0, 0);
        for i in 0..n {
            let (effort, status) = (
                efforts[rng.below(7) as usize],
                statuses[rng.below(4) as usize],
            );
            let weight = match effort {
                "xs" => 1,
                "s" => 2,
                "l" => 5,
                "xl" => 8,
                _ => 3,
            };
            total += weight;
            if status == "done" {
                done_weight += weight;
                done += 1;
            }
            in_progress += usize::from(status == "in_progress");
            let fields: Vec<(&str, &str)> = [("effort", effort), ("status", status)]
                .into_iter()
                .filter(|(_, v)| !v.is_empty())
                .collect();
            let id = format!("f{i}");
            g = g.node(&id, "feature", &fields).edge("ms", &id, "features");
        }
        let g = g.build();
        let w = weighted_milestone_completion(&g, "ms").unwrap();
        assert_eq!(
            (w.total_effort, w.done_effort),
            (total, done_weight),
            "round {round}"
        );
        assert_eq!(
            w.effort_breakdown.iter().map(|e| e.total).sum::<usize>(),
            n,
            "round {round}"
        );
        assert_eq!(
            w.completion_ratio,
            (n > 0).then(|| done_weight as f64 / total as f64)
        );
        let elapsed = rng.below(400) as i64;
        let v = milestone_velocity(&g, "ms", parse_ymd("2026-01-01").unwrap() + elapsed).unwrap();
        assert_eq!(
            (
                v.done_features,
                v.in_progress_features,
                v.remaining_features
            ),
            (done, in_progress, n - done - in_progress),
            "round {round}"
        );
        assert_eq!(v.days_elapsed, Some(elapsed));
        let pace = (done > 0 && elapsed > 0).then(|| done as f64 / elapsed as f64);
        assert_eq!(v.features_per_day, pace, "round {round}");
        let left = n - done;
        let expected = if left == 0 {
            Some(0)
        } else {
            // The smallest whole number of days d with d * done >= left * elapsed.
            pace.map(|_| {
                (0..)
                    .find(|d| d * done as i64 >= left as i64 * elapsed)
                    .unwrap()
            })
        };
        assert_eq!(v.days_remaining, expected, "round {round}");
    }
}

#[test]
fn days_remaining_is_a_whole_quotient_when_the_pace_divides_evenly() {
    // 1 done in 49 days leaves 1 feature 49 days away; a float pace,
    // 1/49, puts 1 / (1/49) just above 49 and would round it to 50.
    let g = G::default()
        .node("ms", "milestone", &[("start_date", "2026-01-01")])
        .node("a", "feature", &[("status", "done")])
        .node("b", "feature", &[])
        .edge("ms", "a", "features")
        .edge("ms", "b", "features")
        .build();
    let as_of = parse_ymd("2026-01-01").unwrap() + 49;
    let v = milestone_velocity(&g, "ms", as_of).unwrap();
    assert_eq!((v.days_elapsed, v.days_remaining), (Some(49), Some(49)));
}
