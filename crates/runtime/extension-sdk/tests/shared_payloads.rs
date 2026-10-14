//! The SDK's operational payload names are the protocol's types, not
//! copies: what a guest decodes is what the host encodes, and the other way
//! round (ADR 0013).

use std::any::TypeId;

use specforge_extension_sdk as sdk;
use specforge_protocol_types as protocol;

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "every operational payload is one protocol type the host and the SDK share"
)]
fn the_sdk_payloads_are_the_protocol_types() {
    let same = [
        (
            TypeId::of::<sdk::CommandInput>(),
            TypeId::of::<protocol::CommandInput<sdk::CommandGraph>>(),
        ),
        (
            TypeId::of::<sdk::CommandOutput>(),
            TypeId::of::<protocol::CommandOutput>(),
        ),
        (
            TypeId::of::<sdk::CommandError>(),
            TypeId::of::<protocol::CommandError>(),
        ),
        (
            TypeId::of::<sdk::CommandFormat>(),
            TypeId::of::<protocol::CommandFormat>(),
        ),
        (
            TypeId::of::<sdk::GraphNode>(),
            TypeId::of::<protocol::GraphNode>(),
        ),
        (
            TypeId::of::<sdk::McpResourceContent>(),
            TypeId::of::<protocol::McpResourceContent>(),
        ),
        (
            TypeId::of::<sdk::PassInput>(),
            TypeId::of::<protocol::PassInput>(),
        ),
        (
            TypeId::of::<sdk::PassEntity>(),
            TypeId::of::<protocol::PassEntity>(),
        ),
        (
            TypeId::of::<sdk::PassDiagnostic>(),
            TypeId::of::<protocol::PassDiagnostic>(),
        ),
        (
            TypeId::of::<sdk::PassOutput>(),
            TypeId::of::<protocol::PassOutput>(),
        ),
        (
            TypeId::of::<sdk::PassAnswer>(),
            TypeId::of::<protocol::PassAnswer>(),
        ),
        (
            TypeId::of::<sdk::CollectInput>(),
            TypeId::of::<protocol::CollectInput>(),
        ),
        (
            TypeId::of::<sdk::CollectOutput>(),
            TypeId::of::<protocol::CollectOutput>(),
        ),
        (
            TypeId::of::<sdk::ValidatorContext>(),
            TypeId::of::<protocol::ValidatorContext>(),
        ),
        (
            TypeId::of::<sdk::ValidatorVerdict>(),
            TypeId::of::<protocol::ValidatorVerdict>(),
        ),
        (
            TypeId::of::<sdk::ScanRequest>(),
            TypeId::of::<protocol::ScanRequest>(),
        ),
        (
            TypeId::of::<sdk::MigrationInput>(),
            TypeId::of::<protocol::MigrationInput>(),
        ),
    ];
    for (i, (sdk_type, protocol_type)) in same.iter().enumerate() {
        assert_eq!(sdk_type, protocol_type, "pair {i}");
    }
}
