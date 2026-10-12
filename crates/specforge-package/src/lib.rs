//! What an extension package is called, which of its versions is asked for and what an update
//! moves a locked package to, and what the peers an extension declares require (ADR 0036, 0041,
//! 0050).
//!
//! The guest wire (`specforge_protocol_types`) carries these as text: a declaration's `name`, a
//! declared peer's `version` range. Every builtin blob is built from the wire, so the host's
//! reading of that text lives here, where no guest compiles it.
//!
//! Pure: no I/O and no diagnostics. Callers map its errors to their codes
//! (`specforge_common::package`: E072; `specforge_common::peers`: E073, E027).

mod name;
pub mod peers;
pub mod search;
pub mod upgrade;

pub use name::{
    MAX_NAME_LEN, PackageName, PackageNameError, PackageRef, SpecifierError, VersionRequirement,
    VersionRequirementError, highest_matching, split_requirement,
};
pub use semver::{Version, VersionReq};
pub use upgrade::{LockedVersion, Reach, upgrade_target};
