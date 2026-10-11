//! The registry build's half of the peer requirements (ADR 0041): the declarations in load order,
//! and what the peer rule reports of them.

use specforge_common::Diagnostic;
use specforge_package::peers::{Member, Peers};
use specforge_protocol_types::ExtensionDeclaration;

/// `declarations` (entry order) in load order (`Peers::load_order`).
pub(crate) fn in_load_order(declarations: Vec<ExtensionDeclaration>) -> Vec<ExtensionDeclaration> {
    let order = Peers::of(declarations.iter().map(Member::from))
        .load_order()
        .to_vec();
    let mut slots: Vec<Option<ExtensionDeclaration>> = declarations.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|i| {
            slots[i]
                .take()
                .expect("a load order names each declaration once")
        })
        .collect()
}

/// The peer diagnostics of `declarations` (load order): each unsatisfied peer (E073, E027),
/// declaration by declaration, then one E027 per cycle among required peers.
pub(crate) fn check(declarations: &[ExtensionDeclaration]) -> Vec<Diagnostic> {
    let peers = Peers::of(declarations.iter().map(Member::from));
    let mut diagnostics: Vec<Diagnostic> = peers
        .unsatisfied()
        .iter()
        .filter_map(|u| specforge_common::peers::of(u.dependent, u.declared, &u.verdict))
        .collect();
    diagnostics.extend(
        peers
            .cycles()
            .map(|cycle| specforge_common::peers::cycle(&cycle)),
    );
    diagnostics
}
