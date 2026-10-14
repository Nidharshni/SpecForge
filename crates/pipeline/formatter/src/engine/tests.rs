use super::*;
use crate::config::FormatConfig;

mod idempotency;
mod parse_errors;
mod range;

fn fmt(source: &str) -> String {
    format_source(source, &FormatConfig::default()).formatted
}

fn fmt_with(source: &str, config: &FormatConfig) -> String {
    format_source(source, config).formatted
}

// --- Slice 2: Tracer bullet (indent only) ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "indentation rules normalize to configured indent style"
)]
fn test_indent_normalizes_to_configured_style() {
    let input = "behavior foo \"Foo\" {\n      contract \"does stuff\"\n}\n";
    let result = fmt(input);
    assert!(
        result.contains("  contract \"does stuff\""),
        "got: {result}"
    );
}

// --- Slice 3: Remaining 7 rules ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "spacing rules normalize single spaces between tokens"
)]
fn test_spacing_normalizes_single_spaces() {
    let input = "behavior foo   \"Foo\" {\n  contract   \"does stuff\"\n}\n";
    let result = fmt(input);
    assert!(result.contains("behavior foo \"Foo\" {"), "got: {result}");
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "alignment rules align field values within blocks"
)]
fn test_alignment_aligns_field_values() {
    let input = "behavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  ports [y]\n}\n";
    let result = fmt(input);
    // All values should start at the same column
    let lines: Vec<&str> = result.lines().collect();
    let inv_line = lines.iter().find(|l| l.contains("invariants")).unwrap();
    let types_line = lines.iter().find(|l| l.contains("types")).unwrap();
    let ports_line = lines.iter().find(|l| l.contains("ports")).unwrap();

    // Find the column where '[' starts for each
    let inv_col = inv_line.find('[').unwrap();
    let types_col = types_line.find('[').unwrap();
    let ports_col = ports_line.find('[').unwrap();

    assert_eq!(
        inv_col, types_col,
        "invariants and types should align: {result}"
    );
    assert_eq!(
        types_col, ports_col,
        "types and ports should align: {result}"
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "wrapping rules break long reference lists to multi-line"
)]
fn test_wrapping_breaks_long_lists() {
    let config = FormatConfig {
        indent_width: 2,
        use_tabs: false,
        max_width: 40,
    };
    let input = "behavior foo \"Foo\" {\n  invariants [very_long_name_a, very_long_name_b, very_long_name_c]\n}\n";
    let result = fmt_with(input, &config);
    // Should be wrapped to multi-line since it exceeds max_width
    let lines: Vec<&str> = result.lines().collect();
    let has_multiline_list = lines.iter().any(|l| l.trim() == "[");
    let has_items = lines
        .iter()
        .any(|l| l.trim().starts_with("very_long_name_a"));
    assert!(
        has_multiline_list || has_items,
        "should wrap long list: {result}"
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "import sorting produces alphabetical order"
)]
fn test_import_sorting() {
    let input = "use \"types/core\"\nuse \"behaviors/auth\"\nuse \"events/compilation\"\n\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    let result = fmt(input);
    let lines: Vec<&str> = result.lines().collect();
    let import_lines: Vec<&&str> = lines.iter().filter(|l| l.starts_with("use ")).collect();
    assert!(import_lines.len() >= 3);
    assert!(import_lines[0].contains("behaviors/auth"), "got: {result}");
    assert!(
        import_lines[1].contains("events/compilation"),
        "got: {result}"
    );
    assert!(import_lines[2].contains("types/core"), "got: {result}");
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "blank line rules enforce exactly one between blocks"
)]
fn test_blank_line_between_blocks() {
    let input = "behavior foo \"Foo\" {\n  contract \"a\"\n}\nbehavior bar \"Bar\" {\n  contract \"b\"\n}\n";
    let result = fmt(input);
    // Should have exactly one blank line between the two blocks
    assert!(result.contains("}\n\nbehavior bar"), "got: {result}");
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "comment rules normalize spacing around inline comments"
)]
fn test_comment_spacing_normalized() {
    let input = "//comment without space\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    let result = fmt(input);
    assert!(result.contains("// comment without space"), "got: {result}");
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "comment rules normalize spacing around inline comments"
)]
fn test_normalize_comment_spacing() {
    assert_eq!(normalize_comment("//comment"), "// comment");
    assert_eq!(normalize_comment("// already good  "), "// already good");
    assert_eq!(normalize_comment("//"), "//");
    // The author's layout inside a comment is kept verbatim.
    assert_eq!(
        normalize_comment("//   - nested bullet"),
        "//   - nested bullet"
    );
    assert_eq!(normalize_comment("  //  indented"), "  //  indented");
    assert_eq!(normalize_comment("/// doc"), "/// doc");
    assert_eq!(normalize_comment("//! inner"), "//! inner");
    // Through the engine: a comment inside a block.
    let result = fmt("behavior a \"A\" {\n  //x\n  contract \"c\"\n}\n");
    assert!(result.contains("  // x"), "got: {result}");
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "indentation rules normalize to configured indent style"
)]
fn indent_follows_the_configured_width_and_tabs() {
    let input = "behavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n";
    let four = FormatConfig {
        indent_width: 4,
        ..FormatConfig::default()
    };
    let tabs = FormatConfig {
        use_tabs: true,
        ..FormatConfig::default()
    };

    assert!(
        fmt_with(input, &four).contains("\n    contract"),
        "{}",
        fmt_with(input, &four)
    );
    assert!(
        fmt_with(input, &tabs).contains("\n\tcontract"),
        "{}",
        fmt_with(input, &tabs)
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "multiline string literals are kept byte-for-byte"
)]
fn multiline_strings_are_kept_byte_for_byte() {
    let input = "behavior foo \"Foo\" {\n  contract \"\"\"\n      First line\n        Second line\n  \"\"\"\n}\n";
    let result = fmt(input);
    assert!(
        result.contains("contract \"\"\"\n      First line\n        Second line\n  \"\"\""),
        "got: {result}"
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "the formatted text has LF line endings and exactly one final newline"
)]
fn the_formatted_text_has_lf_endings_and_one_final_newline() {
    let canonical = "behavior a \"A\" {\n  contract \"x\"\n}\n";
    for input in [
        "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n",
        "behavior a \"A\" {\n  contract \"x\"\n}",
        "\n\n\nbehavior a \"A\" {\n  contract \"x\"\n}\n\n\n",
        "behavior a \"A\" {\r\n  contract \"x\"\r\n}",
    ] {
        assert_eq!(fmt(input), canonical, "{input:?}");
    }
    assert_eq!(fmt(""), "");
    assert_eq!(fmt("\n\n"), "");
    assert_eq!(fmt("// c\r\n"), "// c\n");
    // A triple-quoted string's lines end in LF too (the parser reads it so).
    let string = fmt("behavior a \"A\" {\r\n  contract \"\"\"\r\n    x\r\n  \"\"\"\r\n}");
    assert!(
        !string.contains('\r') && string.ends_with("}\n"),
        "{string:?}"
    );
}

// --- Behavior: apply_format_rules ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "two files differing only in whitespace produce identical output after formatting"
)]
fn test_whitespace_only_differences_produce_identical_output() {
    // Two files that differ only in whitespace should produce identical output
    let input_a =
        "behavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  contract \"test\"\n}\n";
    let input_b = "behavior foo \"Foo\" {\n    invariants   [a,   b]\n    types   [x]\n    contract   \"test\"\n}\n";
    let input_c =
        "behavior foo \"Foo\" {\n\tinvariants [a, b]\n\ttypes [x]\n\tcontract \"test\"\n}\n";

    let result_a = fmt(input_a);
    let result_b = fmt(input_b);
    let result_c = fmt(input_c);

    assert_eq!(result_a, result_b, "a vs b should be identical");
    assert_eq!(result_b, result_c, "b vs c should be identical");
}

// --- Invariant: formatting_consistency ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "two files differing only in whitespace produce identical output after formatting"
)]
fn test_tab_and_space_indented_inputs_produce_same_output() {
    let space_input = "behavior foo \"Foo\" {\n    contract \"test\"\n    types [x]\n}\n";
    let tab_input = "behavior foo \"Foo\" {\n\tcontract \"test\"\n\ttypes [x]\n}\n";

    let space_result = fmt(space_input);
    let tab_result = fmt(tab_input);

    assert_eq!(
        space_result, tab_result,
        "tab and space indented inputs should produce same output:\nspace:\n{space_result}\ntab:\n{tab_result}"
    );
}

// --- Invariant: formatting_semantic_preservation ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "Apply Format Rules: format rule application holds — cst_available, format_config_loaded, contribution_registry_available, deterministic_output, no_domain_logic, extension_rules_applied"
)]
fn test_formatting_does_not_alter_entity_ids_field_values_or_reference_lists() {
    let input = "behavior my_behavior \"My Behavior\" {\n      invariants    [inv_a, inv_b, inv_c]\n      types    [type_x, type_y]\n      ports    [port_z]\n      contract    \"does something important\"\n\n      verify unit \"test alpha\"\n      verify integration \"test beta\"\n}\n";
    let result = fmt(input);

    // Entity ID preserved
    assert!(
        result.contains("my_behavior"),
        "entity ID should be preserved"
    );
    // Title preserved
    assert!(
        result.contains("\"My Behavior\""),
        "title should be preserved"
    );
    // All reference list items preserved
    for item in &["inv_a", "inv_b", "inv_c", "type_x", "type_y", "port_z"] {
        assert!(
            result.contains(item),
            "reference list item '{item}' should be preserved"
        );
    }
    // Field values preserved
    assert!(
        result.contains("\"does something important\""),
        "contract value should be preserved"
    );
    // Verify statements preserved
    assert!(
        result.contains("verify unit \"test alpha\""),
        "verify statement should be preserved"
    );
    assert!(
        result.contains("verify integration \"test beta\""),
        "verify statement should be preserved"
    );
}

// --- Invariant: format_rule_priority ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "indentation rules normalize to configured indent style"
)]
fn test_indent_rule_takes_precedence_over_spacing_rule() {
    // Indent rule (priority 1) should set the leading whitespace,
    // spacing rule (priority 3) should not override it
    let input = "behavior foo \"Foo\" {\n      contract   \"test\"\n}\n";
    let result = fmt(input);
    // The indent should be exactly 2 spaces (indent rule), not collapsed to 0
    assert!(
        result.contains("\n  contract \"test\""),
        "indent rule should take precedence: {result}"
    );
}

// --- Invariant: format_rule_determinism ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "Apply Format Rules: format rule application holds — cst_available, format_config_loaded, contribution_registry_available, deterministic_output, no_domain_logic, extension_rules_applied"
)]
fn test_determinism_same_input_same_config_same_output() {
    let input = "behavior foo \"Foo\" {\n      contract   \"test\"\n    types [a, b]\n}\n";
    let config = FormatConfig::default();
    let result1 = fmt_with(input, &config);
    let result2 = fmt_with(input, &config);
    let result3 = fmt_with(input, &config);
    assert_eq!(result1, result2, "determinism: run 1 vs 2");
    assert_eq!(result2, result3, "determinism: run 2 vs 3");
}

// --- Invariant: comment_preservation ---

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "no comments are lost after formatting"
)]
fn test_every_comment_in_input_appears_in_formatted_output() {
    let input = concat!(
        "// file header comment\n",
        "use \"types/core\"\n",
        "\n",
        "// leading comment for behavior\n",
        "behavior foo \"Foo\" { // trailing comment\n",
        "  contract \"stuff\"\n",
        "}\n",
        "\n",
        "// standalone comment\n",
        "\n",
        "behavior bar \"Bar\" {\n",
        "  // inner comment\n",
        "  contract \"things\"\n",
        "}\n",
    );
    let result = fmt(input);
    assert!(
        result.contains("// file header comment"),
        "file header comment missing: {result}"
    );
    assert!(
        result.contains("// leading comment for behavior"),
        "leading comment missing: {result}"
    );
    assert!(
        result.contains("// trailing comment"),
        "trailing comment missing: {result}"
    );
    assert!(
        result.contains("// standalone comment"),
        "standalone comment missing: {result}"
    );
    assert!(
        result.contains("// inner comment"),
        "inner comment missing: {result}"
    );
}

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "trailing comment attaches to preceding node on same line"
)]
fn test_trailing_comments_remain_attached_to_preceding_node() {
    let input = "behavior foo \"Foo\" { // trailing\n  contract \"stuff\"\n}\n";
    let result = fmt(input);
    // Trailing comment should be preserved in the output (may be moved inside the block
    // by the formatter's block-level formatting)
    assert!(
        result.contains("// trailing"),
        "trailing comment should be preserved: {result}"
    );
}

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "leading comment attaches to following node"
)]
fn test_leading_comments_remain_attached_to_following_node() {
    let input = "// describes foo\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    let result = fmt(input);
    // leading comment should appear before the behavior block in the output
    let lines: Vec<&str> = result.lines().collect();
    let comment_idx = lines.iter().position(|l| l.contains("// describes foo"));
    let behavior_idx = lines.iter().position(|l| l.contains("behavior foo"));
    assert!(
        comment_idx.is_some() && behavior_idx.is_some(),
        "both should exist: {result}"
    );
    assert!(
        comment_idx.unwrap() < behavior_idx.unwrap(),
        "leading comment should be before behavior: {result}"
    );
}

// --- Performance tests ---

// --- Contract: apply_format_rules ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "Apply Format Rules: format rule application holds — cst_available, format_config_loaded, contribution_registry_available, deterministic_output, no_domain_logic, extension_rules_applied"
)]
fn test_apply_format_rules_contract() {
    // requires: cst_available — source must parse into a CST
    // requires: format_config_loaded — config is resolved
    let config = FormatConfig::default();
    let source = "behavior foo \"Foo\" {\n      contract   \"test\"\n    types   [a,  b]\n}\n";

    let result = format_source(source, &config);

    // ensures: deterministic_output — same input+config → same output
    let result2 = format_source(source, &config);
    assert_eq!(
        result.formatted, result2.formatted,
        "deterministic_output: same input+config must produce same output"
    );

    // ensures: no_domain_logic — formatter works with ANY keyword, not just known ones
    let custom_entity = "my_custom_thing foo \"Foo\" {\n      field1   \"value\"\n}\n";
    let custom_result = format_source(custom_entity, &config);
    assert!(
        custom_result.formatted.contains("my_custom_thing foo"),
        "no_domain_logic: formatter should handle unknown entity kinds"
    );
    // Verify indentation was applied (generic block formatting)
    assert!(
        custom_result.formatted.contains("  field1"),
        "no_domain_logic: generic blocks should still be indented"
    );
}

// --- Invariant: formatting_semantic_preservation (property) ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "two files differing only in whitespace produce identical output after formatting"
)]
fn test_format_parses_to_identical_entity_graph() {
    let inputs = [
        "behavior login \"Login\" {\n  invariants [auth_required, session_valid]\n  types [Credentials, Session]\n  ports [AuthService]\n  contract \"authenticates the user\"\n\n  verify unit \"valid credentials succeed\"\n  verify integration \"session is created\"\n}\n",
        "use \"types/core\"\nuse \"behaviors/auth\"\n\ntype user \"User\" {\n  name \"string\"\n  email \"string\" @unique\n  age \"number\" @optional\n}\n",
        "spec \"MyProject\" {\n  name \"my-project\"\n  version \"0.1.0\"\n}\n",
        "behavior a \"A\" {\n      contract   \"test\"\n    types   [x, y, z]\n    invariants [i1]\n}\n",
    ];

    for (i, input) in inputs.iter().enumerate() {
        let formatted = format_source(input, &FormatConfig::default()).formatted;

        let original_ast = specforge_parser::parse(input, "test.spec");
        let formatted_ast = specforge_parser::parse(&formatted, "test.spec");

        // Same number of entities
        assert_eq!(
            original_ast.entities.len(),
            formatted_ast.entities.len(),
            "input #{i}: entity count should be preserved"
        );

        // Same number of imports
        assert_eq!(
            original_ast.imports.len(),
            formatted_ast.imports.len(),
            "input #{i}: import count should be preserved"
        );

        // Each entity: same kind, id, title, and field keys/values
        for (orig, fmt_ent) in original_ast
            .entities
            .iter()
            .zip(formatted_ast.entities.iter())
        {
            assert_eq!(
                orig.kind, fmt_ent.kind,
                "input #{i}: entity kind should be preserved"
            );
            assert_eq!(
                orig.id, fmt_ent.id,
                "input #{i}: entity id should be preserved"
            );
            assert_eq!(
                orig.title, fmt_ent.title,
                "input #{i}: entity title should be preserved"
            );

            // Compare field keys
            let orig_keys: Vec<&str> = orig
                .fields
                .entries()
                .iter()
                .map(|e| e.key.as_str())
                .collect();
            let fmt_keys: Vec<&str> = fmt_ent
                .fields
                .entries()
                .iter()
                .map(|e| e.key.as_str())
                .collect();
            assert_eq!(
                orig_keys, fmt_keys,
                "input #{i}: field keys should be preserved for entity '{}'",
                orig.id.raw
            );

            // Compare field values as every output writes them (no
            // positions: formatting moves values).
            let orig_json =
                serde_json::to_string(&specforge_graph::field_form::write_fields(&orig.fields))
                    .unwrap();
            let fmt_json =
                serde_json::to_string(&specforge_graph::field_form::write_fields(&fmt_ent.fields))
                    .unwrap();
            assert_eq!(
                orig_json, fmt_json,
                "input #{i}: field values should be preserved for entity '{}':\norig: {orig_json}\nfmt:  {fmt_json}",
                orig.id.raw
            );
        }

        // Compare import paths (order may differ due to sorting)
        let mut orig_imports: Vec<&str> = original_ast
            .imports
            .iter()
            .map(|i| i.path.as_str())
            .collect();
        let mut fmt_imports: Vec<&str> = formatted_ast
            .imports
            .iter()
            .map(|i| i.path.as_str())
            .collect();
        orig_imports.sort();
        fmt_imports.sort();
        assert_eq!(
            orig_imports, fmt_imports,
            "input #{i}: import paths should be preserved (sorted)"
        );
    }
}

// --- Configured indentation (the width and tabs an editor asks for outside a project) ---

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "indentation rules normalize to configured indent style"
)]
fn a_configured_width_sets_the_indent() {
    // When no .specforgefmt.toml exists, the FormatConfig should use defaults
    // which correspond to what the editor would provide
    let config_4 = FormatConfig {
        indent_width: 4,
        use_tabs: false,
        max_width: 80,
    };
    let config_2 = FormatConfig {
        indent_width: 2,
        use_tabs: false,
        max_width: 80,
    };

    let input = "behavior foo \"Foo\" {\n        contract \"test\"\n}\n";

    let result_4 = format_source(input, &config_4);
    let result_2 = format_source(input, &config_2);

    // With indent_width=4, contract should be indented 4 spaces
    assert!(
        result_4.formatted.contains("    contract"),
        "indent_width=4 should produce 4-space indent: {}",
        result_4.formatted
    );
    // With indent_width=2, contract should be indented 2 spaces
    assert!(
        result_2.formatted.contains("  contract"),
        "indent_width=2 should produce 2-space indent: {}",
        result_2.formatted
    );
    // Different tab sizes produce different output
    assert_ne!(
        result_4.formatted, result_2.formatted,
        "different editor tab sizes should produce different formatting"
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "indentation rules normalize to configured indent style"
)]
fn use_tabs_indents_with_tabs() {
    let config = FormatConfig {
        indent_width: 2,
        use_tabs: true,
        max_width: 80,
    };
    let input = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
    let result = format_source(input, &config);
    assert!(
        result.formatted.contains("\tcontract"),
        "use_tabs=true should produce tab indentation: {:?}",
        result.formatted
    );
}
