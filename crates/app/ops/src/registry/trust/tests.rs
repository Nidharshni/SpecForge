use super::*;
use crate::asking::testing::{Asked, Scripted};
use crate::asking::{Asking, ExpectedFrom};
use crate::registry::LockedPin;
use specforge_test_macros::test as specforge_test;

fn tool() -> PackageName {
    PackageName::parse("@acme/tool").unwrap()
}

fn signed(key_id: &str, first_use: bool) -> Publisher {
    Publisher::Signed {
        key_id: key_id.into(),
        first_use,
    }
}

fn repinned(key_id: &str, previous: &str) -> Publisher {
    Publisher::Repinned {
        key_id: key_id.into(),
        previous: previous.into(),
    }
}

/// One row of the table: the policy's choices, the signer, what the user's store says, and the answer.
struct Row {
    allow_unsigned: bool,
    asking: Asking<'static>,
    locked: Option<&'static str>,
    signer: Signer<'static>,
    pin: Option<&'static str>,
    trusted: bool,
    denied: bool,
    expect: Result<Publisher, &'static str>,
}

const ROW: Row = Row {
    allow_unsigned: false,
    asking: Asking::Nobody,
    locked: None,
    signer: Signer::Key("k2"),
    pin: None,
    trusted: false,
    denied: false,
    expect: Ok(Publisher::Unsigned),
};

fn decide(row: &Row) -> Result<Publisher, OpError> {
    let policy =
        TrustPolicy::new(row.allow_unsigned, row.asking).locked(row.locked.map(LockedPin::key));
    policy.decide(
        &tool(),
        row.signer,
        UserKeys {
            pin: row.pin,
            trusted: row.trusted,
            denied: row.denied,
        },
    )
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a publisher is decided once, by the trust policy, from the signer and the user's keys"
)]
fn a_publisher_is_decided_from_the_signer_and_the_users_keys() {
    let rows = [
        // Unsigned: refused, or allowed.
        (
            "unsigned refused",
            Row {
                signer: Signer::Unsigned,
                expect: Err("R-TRUST-001"),
                ..ROW
            },
        ),
        (
            "unsigned allowed",
            Row {
                signer: Signer::Unsigned,
                allow_unsigned: true,
                expect: Ok(Publisher::Unsigned),
                ..ROW
            },
        ),
        // First use, then a match.
        (
            "first use",
            Row {
                expect: Ok(signed("k2", true)),
                ..ROW
            },
        ),
        (
            "user pin matches",
            Row {
                pin: Some("k2"),
                expect: Ok(signed("k2", false)),
                ..ROW
            },
        ),
        (
            "lock key matches",
            Row {
                locked: Some("k2"),
                expect: Ok(signed("k2", true)),
                ..ROW
            },
        ),
        (
            "the user's pin wins over the lock",
            Row {
                pin: Some("k2"),
                locked: Some("k1"),
                expect: Ok(signed("k2", false)),
                ..ROW
            },
        ),
        // A key change: refused by Nobody, consented by Yes, accepted unasked when trusted.
        (
            "key change refused",
            Row {
                pin: Some("k1"),
                expect: Err("R-TRUST-003"),
                ..ROW
            },
        ),
        (
            "lock key change refused",
            Row {
                locked: Some("k1"),
                expect: Err("R-TRUST-003"),
                ..ROW
            },
        ),
        (
            "key change consented",
            Row {
                pin: Some("k1"),
                asking: Asking::Yes,
                expect: Ok(repinned("k2", "k1")),
                ..ROW
            },
        ),
        (
            "lock key change consented",
            Row {
                locked: Some("k1"),
                asking: Asking::Yes,
                expect: Ok(repinned("k2", "k1")),
                ..ROW
            },
        ),
        // A denied key: nothing accepts it.
        (
            "denied",
            Row {
                denied: true,
                expect: Err("R-TRUST-005"),
                ..ROW
            },
        ),
        (
            "denied beats consent and allow_unsigned",
            Row {
                denied: true,
                asking: Asking::Yes,
                allow_unsigned: true,
                pin: Some("k2"),
                trusted: true,
                expect: Err("R-TRUST-005"),
                ..ROW
            },
        ),
    ];
    for (what, row) in rows {
        let answer = decide(&row);
        match (&row.expect, answer) {
            (Ok(want), Ok(got)) => assert_eq!(&got, want, "{what}"),
            (Err(code), Err(error)) => {
                assert!(error.is(error_code(code)), "{what}: {error:?}");
                assert_eq!(error.kind, OpErrorKind::PermissionDenied, "{what}");
            }
            (want, got) => panic!("{what}: wanted {want:?}, got {got:?}"),
        }
    }
}

fn error_code(code: &str) -> specforge_common::Code {
    match code {
        "R-TRUST-001" => codes::R_TRUST_001,
        "R-TRUST-003" => codes::R_TRUST_003,
        "R-TRUST-005" => codes::R_TRUST_005,
        other => panic!("{other} is not a trust decision"),
    }
}

#[test]
fn a_trusted_key_is_accepted_without_asking_and_a_change_asks_once() {
    let asker = Scripted::answering(false);
    let trusted = TrustPolicy::new(false, Asking::Ask(&asker)).decide(
        &tool(),
        Signer::Key("k2"),
        UserKeys {
            pin: Some("k1"),
            trusted: true,
            denied: false,
        },
    );
    assert_eq!(trusted.unwrap(), repinned("k2", "k1"));
    assert!(asker.asked().is_empty(), "a trusted key is not asked about");

    let lock = LockedPin::key("k1");
    let refused = TrustPolicy::new(false, Asking::Ask(&asker))
        .locked(Some(lock))
        .decide(&tool(), Signer::Key("k2"), UserKeys::default());
    assert!(refused.unwrap_err().is(codes::R_TRUST_003));
    assert_eq!(
        asker.asked(),
        [Asked::KeyChange {
            package: "@acme/tool".into(),
            expected: "k1".into(),
            offered: "k2".into(),
            from: ExpectedFrom::Lock,
        }]
    );
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a refused key change names the commands that can consent"
)]
fn a_refused_key_change_names_the_commands_that_can_consent() {
    let error = decide(&Row {
        pin: Some("k1"),
        ..ROW
    })
    .unwrap_err();
    assert_eq!(
        error.suggestion.as_deref(),
        Some(
            "if you trust the new key, accept it with `specforge add` or `specforge update` in a terminal (`specforge add --yes` and `specforge update --yes` accept it without asking); or add 'k2' to trusted_keys"
        )
    );
}

#[test]
fn a_change_from_the_locked_key_names_specforge_lock() {
    let from_lock = decide(&Row {
        locked: Some("k1"),
        ..ROW
    })
    .unwrap_err();
    assert_eq!(
        from_lock.message,
        "key change rejected for '@acme/tool': specforge.lock records 'k1' but package is signed 'k2'"
    );

    // The user's pin is expected, and the offered key is the one the lock records.
    let from_pin = decide(&Row {
        pin: Some("k1"),
        locked: Some("k2"),
        ..ROW
    })
    .unwrap_err();
    assert_eq!(
        from_pin.message,
        "key change rejected for '@acme/tool': pinned 'k1' but package is signed 'k2' (the key specforge.lock records)"
    );
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "an unsigned package is refused for a name with an expected key, whatever --allow-unsigned and consent"
)]
fn an_unsigned_package_is_refused_for_a_name_with_an_expected_key() {
    let unsigned = |pin, locked, asking| Row {
        signer: Signer::Unsigned,
        allow_unsigned: true,
        pin,
        locked,
        asking,
        ..ROW
    };
    for (what, row) in [
        ("user pin", unsigned(Some("k1"), None, Asking::Nobody)),
        ("lock key", unsigned(None, Some("k1"), Asking::Nobody)),
        ("user pin, consent", unsigned(Some("k1"), None, Asking::Yes)),
        ("lock key, consent", unsigned(None, Some("k1"), Asking::Yes)),
        (
            "user pin, not allowed",
            Row {
                allow_unsigned: false,
                ..unsigned(Some("k1"), None, Asking::Nobody)
            },
        ),
    ] {
        let error = decide(&row).expect_err(what);
        assert_eq!(error.code, "R-TRUST-007", "{what}: {error:?}");
        assert_eq!(error.kind, OpErrorKind::PermissionDenied, "{what}");
    }
    assert_eq!(
        decide(&unsigned(Some("k1"), Some("k2"), Asking::Nobody))
            .unwrap_err()
            .message,
        "package '@acme/tool' is not signed, but its publisher key is pinned to 'k1'"
    );
    assert_eq!(
        decide(&unsigned(None, Some("k1"), Asking::Nobody))
            .unwrap_err()
            .message,
        "package '@acme/tool' is not signed, but specforge.lock records its publisher key 'k1'"
    );
    // Nothing expected: --allow-unsigned decides, as today.
    assert_eq!(
        decide(&unsigned(None, None, Asking::Nobody)).unwrap(),
        Publisher::Unsigned
    );
}
