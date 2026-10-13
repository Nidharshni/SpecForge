use super::*;

fn full_steps() -> Vec<Step> {
    let names: Vec<&str> = CHECKS
        .iter()
        .filter(|c| c.name != "builtins-verify")
        .map(|c| c.name)
        .collect();
    CHECKS
        .iter()
        .filter(|c| names.contains(&c.name))
        .map(|c| Step {
            compiles: c.compiles,
            after: c
                .after
                .iter()
                .filter_map(|a| names.iter().position(|n| n == a))
                .collect(),
        })
        .collect()
}

fn index(name: &str) -> usize {
    CHECKS
        .iter()
        .filter(|c| c.name != "builtins-verify")
        .position(|c| c.name == name)
        .unwrap()
}

/// Drive `schedule` the way `run` does, finishing the running check that `finish` picks; returns the order checks
/// started in, asserting the rules at every step.
fn simulate(steps: &[Step], finish: impl Fn(&[usize]) -> usize) -> Vec<usize> {
    let mut states = vec![State::Pending; steps.len()];
    let mut order = Vec::new();
    loop {
        let wave = schedule(steps, &states);
        for &k in &wave.start {
            assert!(
                steps[k].after.iter().all(|&a| states[a] == State::Passed),
                "{k} started before its after passed"
            );
            states[k] = State::Running;
            order.push(k);
        }
        let running: Vec<usize> = (0..steps.len())
            .filter(|&k| states[k] == State::Running)
            .collect();
        let compiling = running.iter().filter(|&&k| steps[k].compiles).count();
        assert!(
            compiling <= 1,
            "two compiling checks run at once: {running:?}"
        );
        if running.is_empty() {
            break;
        }
        states[finish(&running)] = State::Passed;
    }
    assert!(states.iter().all(|s| *s == State::Passed), "{states:?}");
    order
}

#[test]
fn no_two_compiling_checks_overlap_and_after_is_respected() {
    let steps = full_steps();
    // Whichever running check finishes first, the rules hold.
    for pick in [0usize, 1, 2] {
        let order = simulate(&steps, |running| running[pick.min(running.len() - 1)]);
        let at = |name: &str| order.iter().position(|&k| k == index(name)).unwrap();
        assert!(at("build") < at("tests") && at("build") < at("doctests"));
        assert!(at("tests") < at("dogfood") && at("guests") < at("dogfood"));
    }
    // clippy never starts while build runs: the first wave holds build or clippy, not both.
    let first = schedule(&steps, &vec![State::Pending; steps.len()]);
    let compiling: Vec<usize> = first
        .start
        .iter()
        .copied()
        .filter(|&k| steps[k].compiles)
        .collect();
    assert_eq!(compiling.len(), 1);
}

#[test]
fn a_failed_after_blocks_its_dependents() {
    let steps = full_steps();
    let mut states = vec![State::Pending; steps.len()];
    states[index("build")] = State::Failed;
    let wave = schedule(&steps, &states);
    assert!(wave.blocked.contains(&index("tests")));
    assert!(wave.blocked.contains(&index("doctests")));
    assert!(!wave.blocked.contains(&index("clippy")));
    states[index("tests")] = State::Blocked;
    states[index("guests")] = State::Passed;
    let wave = schedule(&steps, &states);
    assert!(wave.blocked.contains(&index("dogfood")));
}

#[test]
fn the_report_names_failures_and_the_fix() {
    let report = Report {
        outcomes: vec![
            (
                "rustfmt",
                Outcome::Failed("`cargo fmt` exit status: 1".into()),
                Duration::from_secs(2),
            ),
            ("tests", Outcome::Blocked("build"), Duration::ZERO),
            ("links", Outcome::Skipped, Duration::ZERO),
            ("zero-entity", Outcome::Passed, Duration::from_secs(1)),
        ],
        log_dir: PathBuf::from("/nowhere"),
    };
    assert!(!report.passed());
    let mut table = Vec::new();
    report.print(Mode::Quick, &mut table).unwrap();
    let table = String::from_utf8(table).unwrap();
    assert!(
        table.contains("rustfmt")
            && table.contains("FAILED")
            && table.contains("fix: cargo fmt --all"),
        "{table}"
    );
    assert!(
        table.contains("blocked (by build)") && !table.contains("links"),
        "{table}"
    );
    let mut hook = Vec::new();
    report.print(Mode::Hook, &mut hook).unwrap();
    let hook = String::from_utf8(hook).unwrap();
    assert!(hook.starts_with("pre-commit: rustfmt: "), "{hook}");
}
