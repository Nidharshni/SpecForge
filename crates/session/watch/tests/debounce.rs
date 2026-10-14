use specforge_test_macros::test as spec;
use specforge_watch::Debouncer;
use std::sync::mpsc;
use std::time::Duration;

#[spec(
    behavior = "debounce_file_changes",
    verify = "rapid successive changes coalesced into single batch"
)]
fn rapid_successive_changes_coalesced_into_single_batch() {
    let (tx, rx) = mpsc::channel();
    let debouncer = Debouncer::new(Duration::from_millis(50));

    // Send 3 rapid changes
    tx.send("a.spec".to_string()).unwrap();
    tx.send("b.spec".to_string()).unwrap();
    tx.send("a.spec".to_string()).unwrap(); // duplicate

    let batch = debouncer.coalesce(&rx).unwrap();

    // Should be deduplicated and sorted
    assert_eq!(batch, vec!["a.spec", "b.spec"]);
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "single isolated change triggers after debounce window"
)]
fn single_isolated_change_triggers_after_debounce_window() {
    let window = Duration::from_millis(40);
    let (tx, rx) = mpsc::channel();
    let debouncer = Debouncer::new(window);

    tx.send("only.spec".to_string()).unwrap();

    // `tx` stays alive, so only the window of silence can end the batch.
    let start = std::time::Instant::now();
    let batch = debouncer.coalesce(&rx).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(batch, vec!["only.spec"]);
    assert!(
        elapsed >= window,
        "batch emitted after {elapsed:?}, before the {window:?} debounce window elapsed"
    );
    // ...and it does fire once the window is over (bounded; generous CI margin).
    assert!(
        elapsed < window + Duration::from_millis(400),
        "batch took {elapsed:?}, far past the {window:?} window"
    );
    drop(tx);
}

#[spec(behavior = "debounce_file_changes")]
fn closed_channel_returns_none() {
    let (tx, rx) = mpsc::channel::<String>();
    let debouncer = Debouncer::new(Duration::from_millis(20));

    drop(tx);

    let result = debouncer.coalesce(&rx);
    assert!(result.is_none());
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "debounce window prevents redundant recompilation"
)]
fn debounce_window_prevents_redundant_recompilation() {
    let (tx, rx) = mpsc::channel();
    let debouncer = Debouncer::new(Duration::from_millis(50));

    // Send the same file 5 times in rapid succession
    for _ in 0..5 {
        tx.send("same.spec".to_string()).unwrap();
    }

    let batch = debouncer.coalesce(&rx).unwrap();

    // Should produce exactly one entry — meaning one recompilation, not five
    assert_eq!(
        batch.len(),
        1,
        "5 changes to same file should produce 1 batch entry, not {}",
        batch.len()
    );
    assert_eq!(batch[0], "same.spec");
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "coalesced batch includes union of all changed files"
)]
fn coalesced_batch_includes_union_of_all_changed_files() {
    let (tx, rx) = mpsc::channel();
    let debouncer = Debouncer::new(Duration::from_millis(50));

    tx.send("x.spec".to_string()).unwrap();
    tx.send("y.spec".to_string()).unwrap();
    tx.send("z.spec".to_string()).unwrap();

    let batch = debouncer.coalesce(&rx).unwrap();
    assert_eq!(batch.len(), 3);
    assert!(batch.contains(&"x.spec".to_string()));
    assert!(batch.contains(&"y.spec".to_string()));
    assert!(batch.contains(&"z.spec".to_string()));
}

/// The std adapter (watch's) and the tokio adapter (the LSP's reparse
/// worker) are one rule: the same changes make the same batch, and a closed
/// channel ends both.
#[cfg(feature = "tokio")]
#[spec(
    behavior = "shared_incremental_pipeline",
    verify = "CLI and LSP share identical debounce window"
)]
#[tokio::test]
async fn both_adapters_batch_alike() {
    let window = specforge_watch::DEFAULT_DEBOUNCE_WINDOW;
    let debouncer = Debouncer::new(window);

    let (tx, rx) = mpsc::channel();
    let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel();
    for change in ["b", "a", "b"] {
        tx.send(change).unwrap();
        async_tx.send(change).unwrap();
    }
    assert_eq!(debouncer.coalesce(&rx), Some(vec!["a", "b"]));
    assert_eq!(
        debouncer.coalesce_async(&mut async_rx).await,
        Some(vec!["a", "b"])
    );

    drop(tx);
    drop(async_tx);
    assert_eq!(debouncer.coalesce(&rx), None);
    assert_eq!(debouncer.coalesce_async(&mut async_rx).await, None);
}

/// A change sent while the window runs restarts it, on a tokio channel too.
#[cfg(feature = "tokio")]
#[spec(
    behavior = "debounce_file_changes",
    verify = "each change restarts the quiet window"
)]
#[tokio::test]
async fn the_tokio_adapter_restarts_the_window_on_every_change() {
    let window = Duration::from_millis(120);
    let debouncer = Debouncer::new(window);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        for change in ["a", "b", "c"] {
            tx.send(change).unwrap();
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        // The sender stays alive past the batch.
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let started = std::time::Instant::now();
    let batch = debouncer.coalesce_async(&mut rx).await;
    assert_eq!(batch, Some(vec!["a", "b", "c"]));
    // 120 ms of sends plus a quiet window after the last one.
    assert!(started.elapsed() >= Duration::from_millis(120 + 120));
}
