use super::*;

// --- Slice 5: Idempotency ---

#[specforge_test_macros::test(
    behavior = "maintain_format_idempotency",
    verify = "format(format(x)) == format(x) for random valid inputs"
)]
fn test_idempotency_simple() {
    let input = "use \"types/core\"\n\nbehavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n";
    let first = fmt(input);
    let second = fmt(&first);
    assert_eq!(first, second, "format(format(x)) != format(x)");
}

#[specforge_test_macros::test(
    behavior = "maintain_format_idempotency",
    verify = "format(format(x)) == format(x) for random valid inputs"
)]
fn test_idempotency_complex() {
    let input = concat!(
        "use \"types/core\"\n",
        "use \"behaviors/auth\"\n",
        "\n",
        "// Section header\n",
        "behavior foo \"Foo\" {\n",
        "  invariants [a, b, c]\n",
        "  types      [x, y]\n",
        "  ports      [z]\n",
        "\n",
        "  contract \"does stuff\"\n",
        "\n",
        "  verify unit \"test one\"\n",
        "  verify unit \"test two\"\n",
        "}\n",
    );
    let first = fmt(input);
    let second = fmt(&first);
    assert_eq!(
        first, second,
        "complex: format(format(x)) != format(x)\nfirst:\n{first}\nsecond:\n{second}"
    );
}

// --- Behavior: maintain_format_idempotency ---

#[specforge_test_macros::test(
    behavior = "maintain_format_idempotency",
    verify = "alignment rules do not oscillate between runs"
)]
fn test_alignment_rules_do_not_oscillate_between_runs() {
    let input = "behavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  ports [y, z]\n  contract \"stuff\"\n}\n";
    let first = fmt(input);
    let second = fmt(&first);
    let third = fmt(&second);
    assert_eq!(first, second, "alignment oscillated on 2nd pass");
    assert_eq!(second, third, "alignment oscillated on 3rd pass");
}

#[specforge_test_macros::test(
    behavior = "maintain_format_idempotency",
    verify = "wrapping decisions are stable across runs"
)]
fn test_wrapping_decisions_are_stable_across_runs() {
    let config = FormatConfig {
        indent_width: 2,
        use_tabs: false,
        max_width: 50,
    };
    let input = "behavior foo \"Foo\" {\n  invariants [very_long_name_a, very_long_name_b, very_long_name_c]\n}\n";
    let first = fmt_with(input, &config);
    let second = fmt_with(&first, &config);
    let third = fmt_with(&second, &config);
    assert_eq!(first, second, "wrapping oscillated on 2nd pass");
    assert_eq!(second, third, "wrapping oscillated on 3rd pass");
}

#[specforge_test_macros::test(
    behavior = "maintain_format_idempotency",
    verify = "format(format(x)) == format(x) for random valid inputs"
)]
fn test_idempotency_random_valid_inputs() {
    // Property-style test: multiple representative inputs
    let inputs = [
        "behavior a \"A\" {\n  contract \"x\"\n}\n",
        "use \"types/a\"\nuse \"behaviors/b\"\n\nbehavior x \"X\" {\n  invariants [i1, i2]\n  types [t1]\n  contract \"c\"\n  verify unit \"test\"\n}\n",
        "// header\nbehavior b \"B\" {\n  contract \"y\"\n}\n\n// standalone\n\nbehavior c \"C\" {\n  contract \"z\"\n}\n",
        "spec \"Test\" {\n  name \"test\"\n  version \"0.1.0\"\n}\n",
        "behavior d \"D\" {\n  contract \"\"\"\n    multi\n    line\n  \"\"\"\n}\n",
        "type my_type \"MyType\" {\n  field1 \"string\"\n  field2 \"number\" @optional\n}\n",
        "behavior e \"E\" {\n      invariants   [a,  b,   c]\n      types    [x, y]\n  contract \"test\"\n}\n",
        "use \"z/z\"\nuse \"a/a\"\nuse \"m/m\"\n\nbehavior f \"F\" {\n  contract \"sorted\"\n}\n",
    ];
    for (i, input) in inputs.iter().enumerate() {
        let first = fmt(input);
        let second = fmt(&first);
        assert_eq!(
            first, second,
            "idempotency failed for input #{i}:\nfirst:\n{first}\nsecond:\n{second}"
        );
    }
}

// --- Invariant: formatting_idempotency ---

#[test]
fn test_formatting_already_formatted_file_produces_identical_output() {
    // First format to get the canonical form, then verify idempotency
    let input = "use \"behaviors/auth\"\nuse \"types/core\"\n\nbehavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  contract \"does stuff\"\n\n  verify unit \"test one\"\n}\n";
    let canonical = fmt(input);
    let result = fmt(&canonical);
    assert_eq!(
        canonical, result,
        "already-formatted file should be unchanged:\nexpected:\n{canonical}\ngot:\n{result}"
    );
}

// --- Contract: maintain_format_idempotency ---

#[specforge_test_macros::test(
    behavior = "maintain_format_idempotency",
    verify = "Maintain Format Idempotency: format idempotency holds — format_rules_available, idempotency_holds, no_oscillation"
)]
fn test_maintain_format_idempotency_contract() {
    let config = FormatConfig::default();

    // requires: format_rules_available — all rules loaded (implicit in format_source)
    let inputs = [
        "behavior a \"A\" {\n      contract   \"x\"\n}\n",
        "behavior b \"B\" {\n  invariants [x, y]\n  types [z]\n  contract \"c\"\n  verify unit \"t\"\n}\n",
        "use \"z/z\"\nuse \"a/a\"\n\nbehavior c \"C\" {\n  contract \"d\"\n}\n",
    ];

    for (i, input) in inputs.iter().enumerate() {
        let first = format_source(input, &config);
        let second = format_source(&first.formatted, &config);

        // ensures: idempotency_holds
        assert_eq!(
            first.formatted, second.formatted,
            "idempotency_holds failed for input #{i}"
        );

        // ensures: no_oscillation — 3rd pass identical to 2nd
        let third = format_source(&second.formatted, &config);
        assert_eq!(
            second.formatted, third.formatted,
            "no_oscillation failed for input #{i}"
        );
    }
}
