//! What the fetch policy reported, kept in the order it reported it.

use specforge_common::Diagnostic;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The fetch policy's diagnostics (W155 at a fetch, W156 at a pin). A panic while the lock was held leaves
/// whole diagnostics behind: a poisoned lock is still read.
#[derive(Default)]
pub(crate) struct PolicyReports(Mutex<Vec<Diagnostic>>);

impl PolicyReports {
    fn held(&self) -> MutexGuard<'_, Vec<Diagnostic>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record `more`, after what is already there.
    pub(crate) fn extend(&self, more: impl IntoIterator<Item = Diagnostic>) {
        self.held().extend(more);
    }

    /// Everything reported so far.
    pub(crate) fn all(&self) -> Vec<Diagnostic> {
        self.held().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::codes;

    #[test]
    fn reported_survives_a_poisoned_lock() {
        let reports = PolicyReports::default();
        reports.extend([Diagnostic::new(codes::W155, "kept")]);
        let died = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _held = reports.0.lock().unwrap();
                    panic!("a thread dies holding the lock");
                })
                .join()
        });
        assert!(died.is_err());
        assert!(reports.0.is_poisoned());
        let all = reports.all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].message, "kept");
    }
}
