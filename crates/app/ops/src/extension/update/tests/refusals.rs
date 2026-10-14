use super::*;

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "an update of a name the lock does not hold is refused before any registry is asked"
)]
fn an_update_of_a_name_the_lock_does_not_hold_is_refused() {
    let dir = enabled_project(vec![entry("@acme/a", "1.0.0", "registry", &[])]);
    let registry = MemoryRegistry::new().serving(old("@acme/a", "1.1.0"));
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let named = |name| UpdateRequest {
        name: Some(name),
        ..request(&root, Reach::Major)
    };

    let typo = update(&named("@nope/typo"), &registry, &runtime()).unwrap_err();
    assert_eq!(typo.code, "extension_not_found", "{typo:?}");
    assert_eq!(typo.message, "extension '@nope/typo' is not installed");

    let bad = update(&named("Bad Name"), &registry, &runtime()).unwrap_err();
    assert_eq!(bad.code, "E072", "{bad:?}");

    let builtin = update(&named("@specforge/product"), &registry, &runtime()).unwrap_err();
    assert_eq!(builtin.code, "extension_not_found", "{builtin:?}");
    assert!(
        builtin
            .suggestion
            .as_deref()
            .is_some_and(|s| s.contains("builtins")),
        "{builtin:?}"
    );
    assert!(registry.asked().is_empty(), "no registry is asked");
}

#[test]
fn no_lock_and_no_registry_fail_outright() {
    let empty = tempfile::tempdir().unwrap();
    let error = update(
        &request(
            &specforge_common::ProjectRoot::of(empty.path()).unwrap(),
            Reach::Compatible,
        ),
        &MemoryRegistry::new(),
        &runtime(),
    )
    .unwrap_err();
    assert!(error.is(NO_LOCK), "{error:?}");

    let dir = enabled_project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
    let unconfigured = crate::registry::Unconfigured("update");
    let error = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Compatible,
        ),
        &unconfigured,
        &runtime(),
    )
    .unwrap_err();
    assert!(error.is(NO_REGISTRY), "{error:?}");
}

#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
)]
fn an_unusable_config_is_refused_before_the_lock_is_read() {
    use crate::config::testing::{UNUSABLE, files_under};

    for config in UNUSABLE {
        // With a lock and without one: the config is refused first.
        for locked in [true, false] {
            let dir = if locked {
                enabled_project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])])
            } else {
                tempfile::tempdir().unwrap()
            };
            std::fs::write(dir.path().join("specforge.json"), config).unwrap();
            let before = files_under(dir.path());
            let read = specforge_common::read_project_config(dir.path());
            let refused = crate::config::refusal(&read.problems[0]);

            let unconfigured = crate::registry::Unconfigured("update");
            let error = update(
                &request(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    Reach::Compatible,
                ),
                &unconfigured,
                &runtime(),
            )
            .unwrap_err();

            assert_eq!(error, refused, "{config}, locked: {locked}");
            assert_eq!(files_under(dir.path()), before, "{config}");
        }
    }
}
