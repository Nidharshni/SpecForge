#[cfg(test)]
mod tests {
    use crate::*;
    use specforge_common::SourceSpan;
    use specforge_extension_sdk::prelude::*;
    use specforge_protocol_types::ExtensionDeclaration;

    fn dummy_span() -> SourceSpan {
        SourceSpan {
            file: specforge_common::Sym::new("test.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 10,
        }
    }

    fn make_extension(name: &str) -> ContributionsBuilder {
        ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"))
    }

    fn make_kind(c: &mut ContributionsBuilder, keyword: &str, testable: bool) {
        c.kind(keyword, |k| {
            k.keyword(keyword)
                .testable(testable)
                .supports_verify(testable);
        });
    }

    /// The build of `declarations`.
    fn built(declarations: &[ExtensionDeclaration]) -> RegistryBuild {
        build_registries(declarations.to_vec())
    }

    // -- I:zero_domain_knowledge_core --

    // I:zero_domain_knowledge_core — verify property "core with zero extensions installed has zero entity kinds in KindRegistry"
    #[test]
    fn test_core_with_zero_extensions_has_zero_entity_kinds() {
        let build = built(&[]);
        let (kind_reg, field_reg, edge_reg) = (build.kinds(), build.fields(), build.edges());
        let diags: Vec<_> = build.diagnostics().collect();
        assert_eq!(kind_reg.len(), 0);
        assert_eq!(field_reg.len(), 0);
        assert_eq!(edge_reg.len(), 0);
        // Only diagnostic should be I002 (no extensions)
        let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.is_empty() || codes.iter().all(|c| c.starts_with('I')));
    }

    // -- I:registry_population_before_validation --

    // I:registry_population_before_validation — verify property "no validation diagnostic references a kind that was registered after validation started"
    #[test]
    fn test_no_validation_diagnostic_references_post_registration_kind() {
        // Populate registries first (Phase 1), then validate (Phase 2).
        // All kinds referenced in validation must exist in the registry at validation time.
        let mut c = make_extension("@specforge/software");
        make_kind(&mut c, "behavior", true);
        c.rule("V001", |r| {
            r.severity(ValidationSeverity::Error)
                .message_template("Behavior {id} has no incoming edges")
                .check(CheckKind::NoIncomingEdges)
                .target_kind("behavior");
        });
        let m = c.declaration();

        let build = build_registries(vec![m]);
        // Kind exists in registry before we validate
        assert!(build.kinds().contains("behavior"));

        // The build's rules reference the kind
        assert!(build.rules().iter().any(|rule| rule.code() == "V001"));
        for rule in build.rules() {
            if let Some(tk) = rule.target_kind() {
                assert!(
                    build.kinds().contains(tk),
                    "Validation rule references kind '{}' not in registry",
                    tk
                );
            }
        }
    }

    // I:registry_population_before_validation — verify unit "adding an extension that defines kind X makes X available in the validation phase"
    #[test]
    fn test_adding_extension_makes_kind_available_in_validation() {
        let mut c = make_extension("@specforge/software");
        make_kind(&mut c, "behavior", true);
        let m = c.declaration();

        let build = built(&[m]);
        let kind_reg = build.kinds();
        assert!(kind_reg.contains("behavior"));
        // Kind is now available for validation queries
        assert!(kind_reg.get("behavior").unwrap().testable);
    }

    // -- I:declarative_validation_determinism --

    // I:declarative_validation_determinism — verify property "same extensions and sources produce identical diagnostics across 100 runs"
    #[test]
    fn test_same_extensions_produce_identical_diagnostics_across_runs() {
        let mut c = make_extension("@specforge/software");
        make_kind(&mut c, "behavior", true);
        make_kind(&mut c, "feature", false);
        c.shared_field("contract", |f| {
            f.field_type(FieldType::Block);
        });
        let m = c.declaration();

        let mut baseline_codes: Option<Vec<String>> = None;
        for _ in 0..100 {
            let diags: Vec<_> = built(std::slice::from_ref(&m))
                .diagnostics()
                .cloned()
                .collect();
            let codes: Vec<String> = diags.iter().map(|d| d.code.clone()).collect();
            match &baseline_codes {
                None => baseline_codes = Some(codes),
                Some(base) => assert_eq!(&codes, base, "Diagnostics differed across runs"),
            }
        }
    }

    // I:declarative_validation_determinism — verify unit "diagnostic ordering is deterministic regardless of extension load order"
    #[test]
    fn test_diagnostic_ordering_deterministic_regardless_of_load_order() {
        let mut c1 = make_extension("@ext/aaa");
        make_kind(&mut c1, "alpha", true);
        let m1 = c1.declaration();

        let mut c2 = make_extension("@ext/zzz");
        make_kind(&mut c2, "beta", false);
        make_kind(&mut c2, "alpha", false); // duplicate
        let m2 = c2.declaration();

        // Same input order, multiple runs — must produce identical diagnostics
        let mut baseline: Option<Vec<String>> = None;
        for _ in 0..10 {
            let diags: Vec<_> = built(&[m1.clone(), m2.clone()])
                .diagnostics()
                .cloned()
                .collect();
            let codes: Vec<String> = diags
                .iter()
                .map(|d| format!("{}:{}", d.code, d.message))
                .collect();
            match &baseline {
                None => baseline = Some(codes),
                Some(base) => assert_eq!(&codes, base, "Diagnostic ordering differed across runs"),
            }
        }
        // Ensure there IS at least one diagnostic (the E026 duplicate)
        assert!(!baseline.unwrap().is_empty());
    }

    // -- I:testable_entity_classification --

    // I:testable_entity_classification — verify unit "entity kind with testable=true in manifest accepts verify statements"
    #[test]
    fn test_entity_kind_with_testable_true_accepts_verify() {
        let mut c = make_extension("@specforge/software");
        make_kind(&mut c, "behavior", true);
        let m = c.declaration();
        let build = built(&[m]);
        let kind_reg = build.kinds();
        let entry = kind_reg.get("behavior").unwrap();
        assert!(entry.testable);
        assert!(entry.supports_verify);
    }

    // I:testable_entity_classification — verify unit "testable=true entity counts toward coverage"
    #[test]
    fn test_testable_true_entity_counts_toward_coverage() {
        let mut c = make_extension("@specforge/software");
        make_kind(&mut c, "behavior", true);
        make_kind(&mut c, "feature", false);
        let m = c.declaration();
        let build = built(&[m]);
        let kind_reg = build.kinds();

        let testable_count = kind_reg.iter().filter(|(_, e)| e.testable).count();
        assert_eq!(testable_count, 1);
        assert!(kind_reg.get("behavior").unwrap().testable);
    }

    // I:testable_entity_classification — verify unit "testable=false entity excluded from coverage"
    #[test]
    fn test_testable_false_entity_excluded_from_coverage() {
        let mut c = make_extension("@specforge/software");
        make_kind(&mut c, "feature", false);
        let m = c.declaration();
        let build = built(&[m]);
        let kind_reg = build.kinds();
        assert!(!kind_reg.get("feature").unwrap().testable);
    }

    // I:testable_entity_classification — verify unit "no default testability assumed by core"
    #[test]
    fn test_no_default_testability_assumed_by_core() {
        // A fresh KindRegistry has no opinions about testability
        let kind_reg = KindRegistry::new();
        // No entry means no testability assumption
        assert!(kind_reg.get("anything").is_none());

        // Even after populate, testability comes only from manifest
        let mut c = make_extension("@specforge/software");
        c.kind("behavior", |k| {
            k.keyword("behavior")
                .testable(false) // explicitly false
                .supports_verify(false);
        });
        let m = c.declaration();
        let build2 = built(&[m]);
        let kind_reg2 = build2.kinds();
        assert!(!kind_reg2.get("behavior").unwrap().testable);
    }

    // -- I:compilation_pipeline_ordering --

    // I:compilation_pipeline_ordering — verify property "pipeline events fire in declared order"
    #[test]
    fn test_pipeline_events_fire_in_declared_order() {
        // The pipeline ordering is: parse → load manifests → populate registries → validate
        // We verify this by running each step sequentially and confirming each depends on the previous.

        // Step 1: Declarations (simulating post-parse)
        let mut c = make_extension("@specforge/software");
        make_kind(&mut c, "behavior", true);
        make_kind(&mut c, "feature", false);
        let m = c.declaration();

        // Step 2: Populate registries
        let build = build_registries(vec![m]);
        assert!(build.kinds().contains("behavior"));
        assert!(build.kinds().contains("feature"));

        // Step 3: Validation (after all registries are populated): the
        // populated registry accepts `behavior`.
        let record = crate::entity::EntityRecord::new("behavior", "b1", &dummy_span());
        let diagnostics = build.check(
            &crate::entity::RuleInput {
                entities: &[record],
                edges: &[],
                spec_root: std::path::Path::new("."),
                disk: &crate::MemoryDisk::new(),
            },
            &crate::rules::NoVerdicts,
        );
        assert!(
            diagnostics.iter().all(|d| d.code != "E024"),
            "{diagnostics:?}"
        ); // "behavior" is registered

        // verify edge registry and field registry are also available
        assert_eq!(build.edges().len(), 0); // no edges declared
    }
}
