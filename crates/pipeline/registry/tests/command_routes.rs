//! An extension command's name is judged by its route, `specforge <short>
//! <cli name>`: E039 fires exactly when a command is shadowed (ADR 0097).

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{
    CommandDescriptor, ExtensionDeclaration, HandshakeResponse, SurfaceDescriptor,
};
use specforge_registry::{RegistryBuild, build_registries};
use specforge_test_macros::test as specforge_test;

/// Raw declarations of `(name, ext_short, command ids)`, in order. Unlike
/// the SDK it accepts `a-b` next to `a_b`.
fn declared(extensions: &[(&str, Option<&str>, &[&str])]) -> Vec<ExtensionDeclaration> {
    extensions
        .iter()
        .map(|(name, short, ids)| ExtensionDeclaration {
            handshake: HandshakeResponse {
                name: (*name).into(),
                version: "1.0.0".into(),
                ext_short: short.map(str::to_string),
                ..Default::default()
            },
            surfaces: SurfaceDescriptor {
                commands: ids
                    .iter()
                    .map(|id| CommandDescriptor {
                        id: (*id).into(),
                        title: (*id).into(),
                        description: String::new(),
                        category: None,
                        export: format!("cmd__{id}"),
                        args: vec![],
                    })
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        })
        .collect()
}

/// The messages of the build's E039s.
fn e039s(build: &RegistryBuild) -> Vec<String> {
    build
        .diagnostics()
        .filter(|d| d.code == "E039")
        .map(|d| d.message.clone())
        .collect()
}

/// One id under two short names is two routes, `specforge a list` and
/// `specforge b list`: both run.
#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "one command id under two short names is routed twice, with no E039"
)]
fn one_command_id_under_two_short_names_is_not_e039() {
    let build = build_registries(declared(&[
        ("@acme/a", None, &["list"]),
        ("@acme/b", None, &["list"]),
    ]));

    assert_eq!(e039s(&build), Vec::<String>::new());
}

/// The SDK's command prefix names the export (`cmd__a_list`), not the
/// command's id, so it does not avoid a collision under one name, and does
/// not make one under two.
#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "an SDK command prefix names the export, not the route"
)]
fn an_sdk_command_prefix_names_the_export_not_the_route() {
    let extension = |name: &str, prefix: &str| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"));
        c.command_prefix(prefix);
        c.command("list", |cmd| {
            cmd.title("List")
                .description("List")
                .handler(|_| CommandOutput::ok("list"));
        });
        c.declaration()
    };
    let declarations = vec![extension("@acme/a", "a"), extension("@acme/b", "b")];
    let exports: Vec<&str> = declarations
        .iter()
        .map(|d| d.surfaces.commands[0].export.as_str())
        .collect();
    assert_eq!(exports, ["cmd__a_list", "cmd__b_list"]);

    let build = build_registries(declarations);

    assert_eq!(e039s(&build), Vec::<String>::new());
}

/// `a-b` and `a_b` are one command line, `specforge t a-b`: only the first
/// runs.
#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "two command ids of one CLI name under one short name produce E039"
)]
fn two_ids_of_one_cli_name_under_one_short_name_are_e039() {
    let build = build_registries(declared(&[
        ("@acme/t", Some("t"), &["a-b"]),
        ("@other/t", Some("t"), &["a_b"]),
    ]));

    let reported = e039s(&build);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("'@other/t'"), "{}", reported[0]);
    assert!(reported[0].contains("'@acme/t'"), "{}", reported[0]);
}

/// A raw-JSON guest can declare one command line twice.
#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "one extension declaring one CLI name twice produces E039"
)]
fn one_extension_declaring_one_cli_name_twice_is_e039() {
    let build = build_registries(declared(&[("@acme/t", None, &["a-b", "a_b"])]));

    assert_eq!(e039s(&build).len(), 1, "{:?}", e039s(&build));
}

/// A real collision: one short name, one id. It is E039 before and after.
#[test]
fn one_cli_name_under_one_short_name_is_e039() {
    let build = build_registries(declared(&[
        ("@acme/w", Some("w"), &["list"]),
        ("@other/w", Some("w"), &["list"]),
    ]));

    let reported = e039s(&build);
    assert_eq!(reported.len(), 1, "{reported:?}");
    for part in ["'list'", "'@other/w'", "'@acme/w'"] {
        assert!(reported[0].contains(part), "{}", reported[0]);
    }
}
