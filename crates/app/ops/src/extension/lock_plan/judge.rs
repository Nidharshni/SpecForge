//! The peer rule over the project peer set as the change leaves it, why it refuses, and a version diamond
//! unified against the registry an install came from (ADR 0041, ADR 0001).

use specforge_common::{Diagnostic, codes};
use specforge_package::peers::{PeerRequirement, Verdict};
use specforge_package::{PackageName, Version, highest_matching};
use specforge_protocol_types::PeerDependency;

use super::super::peer_set::{Modules, ProjectPeers};
use super::{Ask, Changing, Checked, LockPlan, Staged, Verb};
use crate::registry::Registry;
use crate::{OpError, OpErrorKind};
use specforge_installed::Origin;

impl<'a> LockPlan<'a> {
    /// Judge the plan, then load what it fetched.
    ///
    /// 0. For a verb that enables, a touched name a `.wasm` file entry of the opened `specforge.json`
    ///    loads is refused (`extension_conflict`, ADR 0160).
    /// 1. [`Self::close`], then the peer rule over the project peer set as the change leaves it
    ///    ([`ProjectPeers::of`] over the staged lock and the entries plus the appended ones). It
    ///    judges every declared peer whose dependent or peer the change touches (a staged install,
    ///    an appended entry):
    ///    - E073 for a range that can't be read;
    ///    - E027 for a required peer no member has (check's message and suggestion);
    ///    - for a peer out of range or not SemVer, by where the peer comes from:
    ///      - installed, with the touching change fetched from a registry: unified against that
    ///        registry (R-RES-006 naming a version every requirer of the set accepts, R-RES-005
    ///        naming each once);
    ///      - otherwise E027: a local install's, or a builtin's or a `.wasm` file entry's, whose
    ///        versions no install changes.
    ///
    ///    A member the change breaks is named: "<installing|updating|enabling> '<name>' <version>
    ///    breaks '<dependent>': ...".
    /// 2. Each fetched install the gate did not refuse: its binary loaded in the plan's runtime
    ///    (E028, `extension_conflict`), declaring the name and version it was published as
    ///    (E028) and the declaration it was published with (R-TRUST-004).
    ///
    /// Each failure is charged to an ask: a staged install's or appended entry's own, a required
    /// builtin's to the ask whose extension required it. What passes carries the pins its registry
    /// fetches decided (`Publisher::pin`), in staging order, for [`Checked::finish`] to save. Lock
    /// order and entry order decide nothing.
    pub(crate) fn check(mut self) -> Result<Checked<'a>, Refused> {
        let changes = !self.changes_nothing();
        let changing = if changes {
            self.touched()?
        } else {
            Changing::none()
        };
        let mut failed: Vec<Option<(OpError, bool)>> =
            changing.names.iter().map(|_| None).collect();
        // A name a `.wasm` file entry loads is not installed or enabled beside it (ADR 0160): the
        // load would refuse one of the two.
        if self.verb.enables() && !changing.names.is_empty() {
            let before = ProjectPeers::of(
                self.installed,
                self.config.entries(),
                &mut self.declarations,
                Modules::OnDisk,
            )
            .map_err(|error| Refused::of(&changing, error))?;
            for (at, name) in changing.names.iter().enumerate() {
                if let Some(Origin::File { path }) = before.member(name).map(|m| &m.origin) {
                    failed[at] = Some((loaded_from_a_file(name, path), true));
                }
            }
        }
        if !changing.names.is_empty() && !(self.verb == Verb::Remove && self.force) {
            let view = self.view().map_err(|error| Refused::of(&changing, error))?;
            let after = self.entries_after();
            let set = match ProjectPeers::of(&view, &after, &mut self.declarations, Modules::Locked)
            {
                Ok(set) => set,
                Err(error) => return Err(Refused::of(&changing, error)),
            };
            let edges = unsatisfied(&set, &changing);
            for edge in &edges {
                let owner = edge.owner(&changing);
                if failed[owner].is_none() {
                    let error = refusal(
                        &set,
                        &self.staged,
                        &changing,
                        owner,
                        edge,
                        &edges,
                        self.verb,
                    );
                    failed[owner] = Some((error, true));
                }
            }
        }
        for (i, s) in self.staged.iter().enumerate() {
            if failed[i].is_none() {
                failed[i] = s.load().err().map(|error| (error, false));
            }
        }
        let (failures, judged): (Vec<(Ask, OpError)>, Vec<bool>) = changing
            .asks
            .iter()
            .zip(failed)
            .filter_map(|(ask, f)| f.map(|(e, judged)| ((*ask, e), judged)))
            .unzip();
        if !failures.is_empty() {
            return Err(Refused { failures, judged });
        }
        Ok(Checked {
            plan: self,
            changes,
            also: Vec::new(),
        })
    }

    /// What the change touches, once what it enables is closed over (E028 refuses the plan when a
    /// builtin it needs does not load): a staged install, an appended entry, and for a removal the
    /// members it takes out of the project's peer set or lets load. For a verb that enables, every
    /// member the change moves is among these (`tests/touched.rs`).
    pub(super) fn touched(&mut self) -> Result<Changing, Refused> {
        if self.verb == Verb::Remove {
            return self.differing();
        }
        if let Err(error) = self.close() {
            return Err(Refused::of(
                &Changing::of(&self.staged, &self.appended),
                error,
            ));
        }
        Ok(Changing::of(&self.staged, &self.appended))
    }

    /// The members that differ between the project peer set before the change and after it: for a
    /// removal, a member it removes or one it lets load (a `.wasm` file entry another entry
    /// shadowed), each charged to the removal's ask. What a removal touches; for a verb that
    /// enables, a subset of what it stages and appends.
    pub(super) fn differing(&mut self) -> Result<Changing, Refused> {
        let none = Changing::none;
        let before = ProjectPeers::of(
            self.installed,
            self.config.entries(),
            &mut self.declarations,
            Modules::Locked,
        )
        .map_err(|error| Refused::of(&none(), error))?;
        let view = self.view().map_err(|error| Refused::of(&none(), error))?;
        let after = ProjectPeers::of(
            &view,
            &self.entries_after(),
            &mut self.declarations,
            Modules::Locked,
        )
        .map_err(|error| Refused::of(&none(), error))?;
        let mut names: Vec<String> = Vec::new();
        for member in before.members().iter().chain(after.members()) {
            if before.member(&member.name) != after.member(&member.name)
                && !names.contains(&member.name)
            {
                names.push(member.name.clone());
            }
        }
        let ask = self
            .disabled
            .first()
            .map(|(ask, _)| *ask)
            .or_else(|| self.uninstalled.first().map(|(ask, _)| *ask))
            .unwrap_or(Ask(0));
        Ok(Changing {
            asks: names.iter().map(|_| ask).collect(),
            names,
            staged: 0,
        })
    }
}

/// Why a lock plan was refused: each ask that can't be done, with its first reason, in staging
/// order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Refused {
    failures: Vec<(Ask, OpError)>,
    /// For each failure, whether the peer rule refused it (else its binary did not load).
    judged: Vec<bool>,
}

impl Refused {
    /// `error`, refusing the plan as a whole: charged to the first ask it touches.
    fn of(changing: &Changing, error: OpError) -> Refused {
        Refused {
            failures: vec![(changing.asks.first().copied().unwrap_or(Ask(0)), error)],
            judged: vec![true],
        }
    }

    /// What an all-or-nothing operation reports (`add`, `init`): the first ask the peer rule
    /// refused, else the first whose binary did not load. A refusal the set earns is not hidden by
    /// a binary the plan would never have loaded.
    pub(crate) fn decisive(self) -> (Ask, OpError) {
        let at = self
            .judged
            .iter()
            .position(|judged| *judged)
            .unwrap_or_default();
        self.failures
            .into_iter()
            .nth(at)
            .expect("a refused plan has a failure")
    }

    /// Every refused ask, in staging order (`update` marks each row).
    pub(crate) fn failures(self) -> Vec<(Ask, OpError)> {
        self.failures
    }
}

/// A declared peer the project peer set leaves unsatisfied.
struct Edge<'s> {
    dependent: &'s str,
    declared: &'s PeerDependency,
    verdict: Verdict,
}

impl Edge<'_> {
    /// The touched name the refusal belongs to: the dependent when it is touched, else the peer.
    fn owner(&self, changing: &Changing) -> usize {
        changing
            .position(self.dependent)
            .or_else(|| changing.position(&self.declared.name))
            .expect("an edge touching the plan")
    }
}

/// The unsatisfied peers of `set` that touch `changing`, in this order: a touched extension's own
/// peers first (in touching order, each in its declared order), then the members a touched one
/// breaks (in entry order). A required peer no member has is among them (ADR 0041).
fn unsatisfied<'s>(set: &'s ProjectPeers, changing: &Changing) -> Vec<Edge<'s>> {
    let mut edges: Vec<Edge<'s>> = set
        .rule()
        .unsatisfied()
        .into_iter()
        .filter(|u| changing.contains(u.dependent) || changing.contains(&u.declared.name))
        .map(|u| Edge {
            dependent: u.dependent,
            declared: u.declared,
            verdict: u.verdict,
        })
        .collect();
    edges.sort_by_key(|e| changing.position(e.dependent).map_or((1, 0), |i| (0, i)));
    edges
}

/// The refusal of an install or enable of `name`, which the `.wasm` file entry `path` loads (ADR 0160).
fn loaded_from_a_file(name: &str, path: &str) -> OpError {
    OpError::new(
        OpErrorKind::Conflict,
        "extension_conflict",
        format!("'{name}' is loaded from the .wasm file entry '{path}' in specforge.json"),
    )
    .with_suggestion(format!("remove that entry first: specforge remove {path}"))
    .with_data(serde_json::json!({ "extension": name, "file": path }))
}

/// Why `edge` refuses the touched package at `owner`.
fn refusal(
    set: &ProjectPeers,
    staged: &[Staged],
    changing: &Changing,
    owner: usize,
    edge: &Edge,
    edges: &[Edge],
    verb: Verb,
) -> OpError {
    if verb == Verb::Remove {
        return removal_refusal(changing, owner, edge, edges);
    }
    let peer = edge.declared.name.as_str();
    let checks = || {
        OpError::of(
            OpErrorKind::SchemaMismatch,
            specforge_common::peers::of(edge.dependent, edge.declared, &edge.verdict)
                .expect("an unsatisfied peer is a diagnostic"),
        )
    };
    let error = match &edge.verdict {
        Verdict::Unreadable(why) => OpError::of(
            OpErrorKind::SchemaMismatch,
            specforge_common::peers::unreadable(edge.dependent, edge.declared, why),
        ),
        Verdict::Missing => checks(),
        Verdict::OutOfRange { installed } | Verdict::NotSemver { installed } => {
            match set.member(peer).map(|member| &member.origin) {
                Some(Origin::Builtin) => {
                    let dependent_is_builtin = set
                        .member(edge.dependent)
                        .is_some_and(|member| member.origin == Origin::Builtin);
                    checks().with_suggestion(if dependent_is_builtin {
                        "this specforge build's builtin extensions disagree: reinstall specforge"
                            .to_string()
                    } else {
                        format!(
                            "'{peer}' is the builtin this specforge embeds, at {installed}: install a version of '{}' that accepts it",
                            edge.dependent
                        )
                    })
                }
                Some(Origin::Installed { .. }) => {
                    match staged.get(owner).and_then(Staged::registry) {
                        None => checks(),
                        Some(registry) => {
                            // What the plan leaves the peer at: this operation's version, or the
                            // lock's.
                            let locked = if changing.contains(peer) {
                                format!("this {} locks {installed}", verb.noun())
                            } else {
                                format!("{installed} is locked")
                            };
                            diamond(registry, set, edge, &locked)
                        }
                    }
                }
                Some(Origin::File { path }) => checks().with_suggestion(format!(
                    "'{peer}' is loaded from the file entry '{path}' at {installed}: change that entry, or install a version of '{}' that accepts it",
                    edge.dependent
                )),
                Some(Origin::Unknown) | None => checks(),
            }
        }
        Verdict::Satisfied => unreachable!("filtered by `unsatisfied`"),
    };
    if changing.contains(edge.dependent) {
        error
    } else {
        let (participle, name, version) = if changing.is_staged(owner) {
            let install = &staged[owner];
            (
                verb.participle(),
                install.name.to_string(),
                install.version.clone(),
            )
        } else {
            let name = changing.names[owner].clone();
            let version = set
                .member(&name)
                .map_or_else(String::new, |member| member.version.clone());
            ("enabling", name, version)
        };
        error.prefixed(format!(
            "{participle} '{name}' {version} breaks '{}': ",
            edge.dependent
        ))
    }
}

/// Why a removal is refused for `edge`: what it takes out of the project's peer set (or lets load)
/// leaves a member's required peer unsatisfied. "removing '<name>' breaks '<dependent>': ...",
/// its suggestion naming every member the removal leaves that way.
fn removal_refusal(changing: &Changing, owner: usize, edge: &Edge, edges: &[Edge]) -> OpError {
    let diagnostic = match &edge.verdict {
        Verdict::Unreadable(why) => {
            specforge_common::peers::unreadable(edge.dependent, edge.declared, why)
        }
        verdict => specforge_common::peers::of(edge.dependent, edge.declared, verdict)
            .expect("an unsatisfied peer is a diagnostic"),
    };
    let error = OpError::of(OpErrorKind::Conflict, diagnostic);
    if changing.contains(edge.dependent) {
        return error;
    }
    let name = &changing.names[owner];
    let mut dependents: Vec<String> = edges
        .iter()
        .filter(|other| other.declared.name == *name && !changing.contains(other.dependent))
        .map(|other| format!("'{}'", other.dependent))
        .collect();
    dependents.sort();
    dependents.dedup();
    error
        .prefixed(format!("removing '{name}' breaks '{}': ", edge.dependent))
        .with_suggestion(format!(
            "remove {} first, or force the removal of '{name}' anyway",
            dependents.join(", ")
        ))
}

/// R-RES-006 naming the version of `edge`'s peer every requirer in `set` accepts, or R-RES-005
/// (E073) when none does (ADR 0001).
fn diamond(registry: &dyn Registry, set: &ProjectPeers, edge: &Edge, locked: &str) -> OpError {
    let peer = edge.declared.name.as_str();
    let versions = match published_versions(registry, peer) {
        Ok(versions) => versions,
        Err(error) => return error,
    };
    match unify(peer, &versions, &set.requirers(peer)) {
        Ok(unified) => OpError::coded(
            OpErrorKind::Conflict,
            codes::R_RES_006,
            format!(
                "version diamond: '{}' requires peer '{peer}' {} but {locked}; {peer} {unified} would satisfy every requirer",
                edge.dependent, edge.declared.version
            ),
        )
        .with_suggestion(format!(
            "install '{peer}' at {unified} (specforge add {peer}@{unified}), or at another version every requirer accepts, then retry; when a locked extension must move with it, specforge update --major moves them together"
        )),
        Err(diagnostic) => OpError::of(OpErrorKind::Conflict, diagnostic),
    }
}

/// The versions `registry` publishes of `peer`, in the registry's order. E072 when `peer` is no
/// package name: an extension's declaration is malformed (`schema_mismatch`), not the caller's
/// value.
fn published_versions(registry: &dyn Registry, peer: &str) -> Result<Vec<Version>, OpError> {
    let name = PackageName::parse(peer).map_err(|why| {
        OpError::of(
            OpErrorKind::SchemaMismatch,
            specforge_common::package::invalid(&why),
        )
    })?;
    registry.versions(&name)
}

/// Unify a version diamond (ADR 0001): the highest of `published` that every requirer's range
/// matches, each range read on its own (`specforge_package::highest_matching`; a pre-release only
/// when that range names its `MAJOR.MINOR.PATCH`). R-RES-005 naming each requirer when none does;
/// E073 for a range that is not SemVer.
pub(super) fn unify(
    name: &str,
    published: &[Version],
    requirers: &[(&str, &PeerDependency)],
) -> Result<Version, Diagnostic> {
    let mut reqs = Vec::with_capacity(requirers.len());
    for (requirer, declared) in requirers {
        let requirement = PeerRequirement::read(declared)
            .map_err(|why| specforge_common::peers::unreadable(requirer, declared, &why))?;
        reqs.push((*requirer, declared.version.as_str(), requirement.range));
    }
    let ranges: Vec<_> = reqs.iter().map(|(_, _, range)| range).collect();

    highest_matching(published, &ranges).cloned().ok_or_else(|| {
        let wanted = reqs
            .iter()
            .map(|(requirer, range, _)| format!("{requirer} wants {name} {range}"))
            .collect::<Vec<_>>()
            .join("; ");
        let available = published
            .iter()
            .map(Version::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        Diagnostic::new(
            codes::R_RES_005,
            format!(
                "version diamond for '{name}': no single version satisfies every requirer ({wanted}). Available: {available}"
            ),
        )
        .with_suggestion(
            "no version unifies these ranges; upgrade the requirer with the narrowest range or pin a compatible peer version manually"
                .to_string(),
        )
    })
}
