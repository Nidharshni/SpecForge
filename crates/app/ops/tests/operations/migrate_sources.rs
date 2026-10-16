//! What a migration reads: the project's sources, and a config that hides them.

use std::sync::Arc;

use specforge_ops::authoring::migrate::{Request, rollback};
use specforge_test_macros::test as specforge_test;

use crate::migrate::{OLD, files_under, hooked, project, request, run};

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a specforge.json that does not say which files are the project's sources refuses the migration, dry run included, and nothing is written"
)]
fn a_config_that_hides_the_sources_refuses_the_migration() {
    let configs = [
        r#"{"spec_root": "spec", "extensions": ["#,
        "[1,2]",
        r#"{"exclude": "vendor"}"#,
        r#"{"spec_root": ["spec"]}"#,
        r#"{"spec_root": "spec", "extensions": "@specforge/product"}"#,
    ];
    for config in configs {
        for dry_run in [true, false] {
            let dir = project();
            std::fs::create_dir_all(dir.path().join("vendor")).unwrap();
            std::fs::write(dir.path().join("vendor/v.spec"), OLD).unwrap();
            std::fs::write(dir.path().join("specforge.json"), config).unwrap();
            let before = files_under(dir.path());
            let runtime = Arc::new(hooked(|_| Ok(())));

            let refused = specforge_ops::authoring::migrate::run(
                &Request {
                    dry_run,
                    ..request(&specforge_common::ProjectRoot::of(dir.path()).unwrap())
                },
                Some(runtime.clone()),
            )
            .unwrap_err();

            assert_eq!(refused.code, "config_invalid", "{config}");
            assert_eq!(
                files_under(dir.path()),
                before,
                "{config}: no .bak, no record"
            );
            assert!(runtime.calls().is_empty(), "{config}: no compile, no hook");
        }
    }
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "only the project's sources are migrated: under spec_root, without excluded files"
)]
fn migrate_reads_the_project_sources() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"spec_root": "specs", "exclude": ["drafts"]}"#,
    )
    .unwrap();
    for file in [
        "specs/a.spec",
        "specs/drafts/d.spec",
        "spec/old.spec",
        "fixtures/fx.spec",
    ] {
        std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        std::fs::write(root.join(file), OLD).unwrap();
    }

    let outcome = run(
        &Request {
            dry_run: true,
            no_backup: true,
            ..request(&specforge_common::ProjectRoot::of(root).unwrap())
        },
        None,
    );

    let visited: Vec<&str> = outcome
        .summary
        .results
        .iter()
        .map(|r| r.file_path.as_str())
        .collect();
    assert_eq!(visited.len(), 1, "{visited:?}");
    assert_eq!(visited[0], "specs/a.spec", "{visited:?}");
    // A dry run records nothing, so there is nothing to roll back.
    let rollback = rollback(&specforge_common::ProjectRoot::of(root).unwrap()).summary;
    assert!(rollback.results.is_empty(), "{rollback:?}");
    assert_eq!(rollback.warnings.len(), 1, "{rollback:?}");
    assert!(
        rollback.warnings[0].contains("nothing to roll back"),
        "{rollback:?}"
    );
}
