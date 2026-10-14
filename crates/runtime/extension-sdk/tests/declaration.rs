//! The SDK builds one `ExtensionDeclaration` and serves exactly it: the
//! handshake carries the meta's short name, description and keywords; a
//! raw category is checked when the extension is built; and the generated
//! guest's routing is one value, `Served`, which an in-process host reuses
//! and which reads the declaration once.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::testing::MockHost;
use specforge_extension_sdk::{ExtensionDeclaration, HandshakeResponse, Served, no_other_exports};
use specforge_protocol_types::{DeclaredCategory, DescribeResponse};

fn reports() -> ContributionsBuilder {
    let mut meta = ExtensionMeta::new("@acme/reports", "0.1.0");
    meta.short = Some("rep".to_string());
    meta.description = Some("Reports over the graph".to_string());
    meta.keywords = vec!["reports".to_string(), "graph".to_string()];
    let mut b = ContributionsBuilder::new(meta);
    b.kind("report", |k| {
        k.description("A report").field("title", |f| {
            f.field_type(FieldType::String).required();
        });
    });
    b.command("list", |c| {
        c.title("List")
            .description("List reports")
            .handler(|_| CommandOutput {
                exit_code: 0,
                stdout: "hi".to_string(),
                stderr: String::new(),
            });
    });
    b.pass("audit", |p| {
        p.phase("check")
            .run(|_: &PassInput| Vec::<PassDiagnostic>::new());
    });
    b
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "the SDK's short name reaches the handshake as ext_short"
)]
fn the_short_name_reaches_the_handshake() {
    let host = MockHost::new(reports());
    let handshake: serde_json::Value = serde_json::from_str(&host.handshake_json()).unwrap();
    assert_eq!(handshake["ext_short"], "rep");
    assert_eq!(handshake["description"], "Reports over the graph");
    assert_eq!(
        handshake["keywords"],
        serde_json::json!(["reports", "graph"])
    );
    let declaration = host.declaration();
    assert_eq!(declaration.short(), "rep");

    // Without them, the handshake carries none of the three keys.
    let plain = ContributionsBuilder::new(ExtensionMeta::new("@acme/plain", "1.0.0"));
    let handshake: serde_json::Value =
        serde_json::from_str(&plain.declaration().handshake_json()).unwrap();
    for key in ["ext_short", "description", "keywords"] {
        assert!(handshake.get(key).is_none(), "{key}: {handshake}");
    }
    assert_eq!(plain.declaration().short(), "plain");
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "the SDK's peer and optional_peer calls reach the handshake as peer_dependencies, in call order"
)]
fn peers_reach_the_handshake_in_call_order() {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/peers", "1.0.0"));
    b.peer("@a/x", "^1.0").optional_peer("@a/y", "^2");

    let handshake: serde_json::Value =
        serde_json::from_str(&b.declaration().handshake_json()).unwrap();

    assert_eq!(
        handshake["peer_dependencies"],
        serde_json::json!([
            {"name": "@a/x", "version": "^1.0", "optional": false},
            {"name": "@a/y", "version": "^2", "optional": true},
        ])
    );
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "a raw category that does not parse panics when the extension is built"
)]
fn a_raw_category_that_does_not_parse_panics() {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/raw", "1.0.0"));
    b.raw_category(
        DeclaredCategory::Passes,
        serde_json::json!([{ "nam": "x" }]),
    );
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b.declaration()))
        .expect_err("a malformed raw category panics");
    let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("raw category 'passes'"), "{message}");
    assert!(message.contains("missing field `name`"), "{message}");
    // Serving any category builds the declaration, so the guest fails too.
    let served = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        MockHost::new(b).describe_json("entities")
    }));
    assert!(served.is_err());
}

/// What the guest serves loads back as the declaration the builder built:
/// every category, the derived `fields` and flags included.
#[test]
fn the_guest_serves_its_declaration() {
    let host = MockHost::new(reports());
    let declared = host.declaration();
    let handshake: HandshakeResponse = serde_json::from_str(&host.handshake_json()).unwrap();
    let loaded = ExtensionDeclaration::from_wire(
        handshake,
        |category| {
            Ok(serde_json::from_str::<DescribeResponse>(
                &host.describe_json(category.name()).unwrap(),
            )
            .unwrap())
        },
        |key| panic!("unexpected key {key:?}"),
    )
    .unwrap();
    assert_eq!(loaded, declared);
    assert_eq!(declared.surfaces.commands[0].id, "list");
    assert_eq!(declared.passes[0].name, "audit");
    assert!(declared.declares(DeclaredCategory::Entities));
    for category in DeclaredCategory::ALL {
        assert_eq!(
            host.describe_json(category.name()),
            Some(declared.describe_json(category)),
            "{category:?}"
        );
    }
    // `fields` is no category: nothing serves it.
    assert_eq!(host.describe_json("fields"), None);
}

fn dispatch(export: &str, _input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    (export == "scan__x").then(|| Ok(b"scanned".to_vec()))
}

/// A served guest routes as the component guest does: the protocol exports,
/// then the declared surfaces and operations, then the handler, else an
/// unknown export.
#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a guest answers the handshake and every describe from one declaration it builds once"
)]
fn a_served_guest_routes_every_export() {
    let host = MockHost::new(reports());
    let handshake_json = host.handshake_json().into_bytes();
    let describe_entities = host.describe_json("entities").unwrap().into_bytes();
    let served = Served::new(host.0);
    let handshake = served.call(dispatch, "__handshake", b"{}").unwrap();
    assert_eq!(handshake, handshake_json);
    let describe = served
        .call(dispatch, "__describe", br#"{"category":"entities"}"#)
        .unwrap();
    assert_eq!(describe, describe_entities);
    let unsupported = served.call(dispatch, "__describe", br#"{"category":"nope"}"#);
    assert_eq!(unsupported, Err("unsupported category: nope".to_string()));
    let input = serde_json::to_vec(&CommandInput::default()).unwrap();
    let command = served.call(dispatch, "cmd__list", &input).unwrap();
    let output: serde_json::Value = serde_json::from_slice(&command).unwrap();
    assert_eq!(output["stdout"], "hi");
    let pass = served
        .call(dispatch, "__pass_audit", br#"{"entities":[]}"#)
        .unwrap();
    assert_eq!(pass, b"[]");
    assert_eq!(
        served.call(dispatch, "scan__x", b""),
        Ok(b"scanned".to_vec())
    );
    assert_eq!(
        served.call(dispatch, "nope", b""),
        Err("unknown export 'nope'".to_string())
    );
}

/// What a served guest answers is what its builder declares, for every
/// category the protocol defines: the answers it reads from one declaration
/// are the ones the builder gives.
#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a guest answers the handshake and every describe from one declaration it builds once"
)]
fn served_answers_what_the_builder_declares() {
    let host = MockHost::new(reports());
    let expected: Vec<(&str, Option<String>)> = DeclaredCategory::ALL
        .iter()
        .map(|category| (category.name(), host.describe_json(category.name())))
        .collect();
    let handshake = host.handshake_json();
    let served = Served::new(host.0);
    for (category, answer) in expected {
        let request = serde_json::to_vec(&serde_json::json!({ "category": category })).unwrap();
        assert_eq!(
            served.call(no_other_exports, "__describe", &request),
            Ok(answer.expect("a supported category").into_bytes()),
            "{category}"
        );
    }
    assert_eq!(
        served.call(no_other_exports, "__handshake", b""),
        Ok(handshake.into_bytes())
    );
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "the SDK's raw category and the host's load parse a category's items through one function"
)]
fn a_raw_category_and_a_loaded_one_parse_alike() {
    let mut built = reports();
    built
        .edge("charts", |e| {
            e.source_kind("report").target_kind("chart");
        })
        .shared_field("tags", |f| {
            f.field_type(FieldType::String);
        })
        .enhance("behavior", "@acme/base", |e| {
            e.field("report", |f| {
                f.field_type(FieldType::String);
            });
        })
        .rule("R001", |r| {
            r.check(CheckKind::NoIncomingEdges).message_template("x");
        })
        .feature_flag("beta", false, "a flag");
    let declared = built.declaration();
    let built = MockHost::new(built);

    for category in DeclaredCategory::ALL {
        let items = declared.describe_items(category);
        // The category is served raw, in place of the builder's own.
        let mut raw = ContributionsBuilder::new(ExtensionMeta::new("@acme/reports", "0.1.0"));
        raw.raw_category(category, items.clone());
        assert_eq!(
            raw.declaration().describe_items(category),
            items,
            "{category:?}"
        );
    }

    // The host's load of what the builder serves is the builder's declaration.
    let handshake: HandshakeResponse = serde_json::from_str(&built.handshake_json()).unwrap();
    let loaded = ExtensionDeclaration::from_wire(
        handshake,
        |category| {
            Ok(serde_json::from_str::<DescribeResponse>(
                &built.describe_json(category.name()).unwrap(),
            )
            .unwrap())
        },
        |key| panic!("unexpected key {key:?}"),
    )
    .unwrap();
    assert_eq!(loaded, declared);
}
