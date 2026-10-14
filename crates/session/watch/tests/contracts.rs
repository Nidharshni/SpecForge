use specforge_test_macros::test as specforge_test;

// B:debounce_file_changes — verify contract "requires/ensures consistency for file change debouncing"
#[specforge_test(
    behavior = "debounce_file_changes",
    verify = "Debounce File Changes: file change debouncing holds — file_changed_fired, coalesced_batch_produced, redundant_recompilation_prevented"
)]
fn debounce_file_changes_contract() {
    use specforge_watch::Debouncer;
    use std::sync::mpsc;
    use std::time::Duration;

    // Requires: rapid successive file change events (including duplicates)
    // Ensures: single coalesced batch with deduplicated and sorted files
    let (tx, rx) = mpsc::channel();
    let debouncer = Debouncer::new(Duration::from_millis(50));

    tx.send("b.spec".to_string()).unwrap();
    tx.send("a.spec".to_string()).unwrap();
    tx.send("b.spec".to_string()).unwrap(); // duplicate
    tx.send("a.spec".to_string()).unwrap(); // duplicate

    let batch = debouncer.coalesce(&rx).unwrap();

    assert_eq!(batch.len(), 2, "duplicates must be coalesced");
    assert_eq!(
        batch,
        vec!["a.spec", "b.spec"],
        "batch must be sorted and deduplicated"
    );
}
