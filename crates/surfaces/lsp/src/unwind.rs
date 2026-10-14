//! A panic inside the server is a fault of the server, not the end of the session (the LSP's form of ADR 0004
//! D4-a, `lsp_survives_its_faults`).

/// `f`'s result, or the message it panicked with.
pub(crate) fn unwinding<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|panic| {
        panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a handler panicked".into())
    })
}

#[cfg(test)]
mod tests {
    use super::unwinding;
    use specforge_test_macros::test as spec;

    #[spec(
        invariant = "lsp_survives_its_faults",
        verify = "a panic is turned into its message"
    )]
    fn a_panic_is_turned_into_its_message() {
        assert_eq!(unwinding(|| 7), Ok(7));
        assert_eq!(unwinding(|| panic!("boom")), Err::<(), _>("boom".into()));
        let formatted = String::from("attempt to add with overflow");
        assert_eq!(
            unwinding(|| panic!("{formatted}")),
            Err::<(), _>("attempt to add with overflow".into()),
            "a String payload"
        );
    }
}
