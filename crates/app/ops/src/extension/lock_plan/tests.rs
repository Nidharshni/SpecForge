use super::super::fixtures::entry;
use super::judge::unify;
use super::*;
use std::path::Path;

use specforge_common::{Diagnostic, codes};
use specforge_installed::{Installed, Module};
use specforge_package::{PackageName, Version};
use specforge_protocol_types::PeerDependency;

use super::super::candidate::{Installable, LocalFile};
use super::super::peer_set::{Declarations, ProjectPeers};
use crate::OpError;
use crate::asking::Asking;
use crate::registry::METADATA_MISMATCH;
use crate::registry::testing::{Asked, MemoryRegistry, Published, declaration};
use crate::testing::declaring;
use specforge_installed::{LockFile, LockFileEntry, LockState};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;

const BASE: &[u8] = b"\0asm in-process @acme/base 2.0.0";
const APP: &[u8] = b"\0asm in-process @acme/app 1.0.0";
const APP_NEEDS_BASE_2: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs base 2";
const ONE_ISH: &[u8] = b"\0asm in-process @acme/app 1.0.0 one-ish";
const BASE_1: &[u8] = b"\0asm in-process @acme/base 1.0.0";
const APP_NEEDS_BASE_1: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs base 1";
const APP_NEEDS_BASE_3: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs base 3";

fn nowhere(entries: Vec<LockFileEntry>) -> Installed {
    Installed::with_lock(
        Path::new("/nowhere"),
        LockState::Read(LockFile {
            lockfile_version: 1,
            entries,
        }),
    )
}

fn peer(name: &str, range: &str) -> PeerDependency {
    PeerDependency {
        name: name.to_string(),
        version: range.to_string(),
        optional: false,
    }
}

/// `@acme/base` locked at 1.0.0 and required by `@acme/other` at `other_range`.
fn base_and_other(other_range: &str) -> Vec<LockFileEntry> {
    vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry(
            "@acme/other",
            "1.0.0",
            "registry",
            &[("@acme/base", other_range)],
        ),
    ]
}

/// The runtime serving every binary above.
fn runtime() -> InProcessRuntime {
    InProcessRuntime::new()
        .binary(BASE, declaring("@acme/base", "2.0.0", &[]))
        .binary(BASE_1, declaring("@acme/base", "1.0.0", &[]))
        .binary(APP, declaring("@acme/app", "1.0.0", &[]))
        .binary(
            APP_NEEDS_BASE_2,
            declaring("@acme/app", "1.0.0", &[("@acme/base", "^2.0", false)]),
        )
        .binary(
            APP_NEEDS_BASE_1,
            declaring("@acme/app", "1.0.0", &[("@acme/base", "^1.0", false)]),
        )
        .binary(
            APP_NEEDS_BASE_3,
            declaring("@acme/app", "1.0.0", &[("@acme/base", "^3.0", false)]),
        )
        .binary(
            ONE_ISH,
            declaring("@acme/app", "1.0.0", &[("@acme/base", "one-ish", false)]),
        )
}

/// `@acme/base` 1.0.0, 1.4.0 and 2.0.0 published.
fn base_registry() -> MemoryRegistry {
    let published = |version: &str, bytes: &[u8]| {
        Published::new(bytes.to_vec(), declaration("@acme/base", version, &[]))
    };
    MemoryRegistry::new()
        .serving(published("1.0.0", b"\0asm b1"))
        .serving(published("1.4.0", b"\0asm b14"))
        .serving(published("2.0.0", b"\0asm b2"))
}

/// The registry of a plan that fetches nothing.
fn no_registry() -> &'static MemoryRegistry {
    static NONE: std::sync::LazyLock<MemoryRegistry> =
        std::sync::LazyLock::new(MemoryRegistry::new);
    &NONE
}

fn fetching(registry: &MemoryRegistry) -> Fetching<'_> {
    Fetching {
        registry,
        allow_unsigned: true,
        asking: Asking::Nobody,
    }
}

/// `bytes` read in `runtime` as a local file at `/nowhere/<name>.wasm`.
fn local(runtime: &InProcessRuntime, bytes: &[u8], file: &str) -> Install<'static> {
    let installable = Installable::read(runtime, Module::new(bytes.to_vec())).unwrap();
    Install::local(
        Path::new("/nowhere"),
        LocalFile {
            path: Path::new("/nowhere").join(file),
            binary: installable,
        },
    )
    .unwrap()
}

/// A plan over `installed` whose `specforge.json` enables every locked extension, fetching
/// from `registry`.
fn plan_from<'a>(
    installed: &'a Installed,
    runtime: &'a InProcessRuntime,
    registry: &'a MemoryRegistry,
    verb: Verb,
) -> LockPlan<'a> {
    let entries: Vec<String> = installed
        .lock()
        .entries()
        .iter()
        .map(|e| e.name.to_string())
        .collect();
    match verb {
        Verb::Install => LockPlan::adding(
            installed,
            &entries,
            runtime,
            fetching(registry),
            Purpose::Commit,
            true,
        ),
        Verb::Update => LockPlan::updating(installed, &entries, runtime, fetching(registry)),
        Verb::Remove => unreachable!("a removal is planned over a compiled project"),
    }
    .unwrap()
}

/// [`plan_from`] a registry that serves nothing.
fn plan<'a>(installed: &'a Installed, runtime: &'a InProcessRuntime, verb: Verb) -> LockPlan<'a> {
    plan_from(installed, runtime, no_registry(), verb)
}

/// Stage `install` for the first ask; `add` enables it by its bare name.
fn stage<'a>(plan: &mut LockPlan<'a>, install: Install<'a>) {
    plan.install(Ask(0), install);
}

/// `name@version` fetched from the plan's registry and staged.
fn stage_fetched(plan: &mut LockPlan<'_>, name: &PackageName, version: Version) {
    let install = plan.fetch(name, &version).unwrap();
    stage(plan, install);
}

fn refused(plan: LockPlan<'_>) -> OpError {
    plan.check().map(drop).unwrap_err().failures().remove(0).1
}

#[test]
fn an_enabling_plan_accepts_an_unreadable_lock() {
    let installed = Installed::with_lock(
        Path::new("/nowhere"),
        LockState::Unreadable(specforge_common::Diagnostic::new(codes::E033, "unreadable")),
    );
    let runtime = crate::testing::probes();

    // A plan that only enables opens no change: the lock hides the installed extensions.
    let mut enabling = LockPlan::adding(
        &installed,
        &[],
        &runtime,
        fetching(no_registry()),
        Purpose::Commit,
        false,
    )
    .unwrap();
    enabling.enable(Ask(0), "@specforge/software");
    assert!(enabling.check().is_ok());

    // One that installs refuses: writing a lock would drop what it records.
    let refused = LockPlan::adding(
        &installed,
        &[],
        &runtime,
        fetching(no_registry()),
        Purpose::Commit,
        true,
    )
    .map(drop);
    assert_eq!(refused.unwrap_err().code, "E033");
}

mod close;
mod commit;
mod diamond;
mod judge;
