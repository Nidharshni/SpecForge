//! Peer requirements (ADR 0041): what the peers an extension declares mean, decided once.
//!
//! A declared peer (`PeerDependency`, the wire's text) is read as a [`PeerRequirement`]: the peer's
//! name, a SemVer range read as Cargo reads one, and whether the peer may be absent. [`verdict`] is
//! the one rule saying whether a peer is satisfied by the version its peer is installed at; a set
//! of extensions has one load order and its cycles among required peers ([`Peers`]).
//!
//! Pure: no I/O and no diagnostics. The registry build orders the declarations by it and reports
//! its verdicts (E073, E027, `specforge_common::peers`); `add` and `update` gate on [`verdict`].

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use semver::{Version, VersionReq};

use specforge_protocol_types::{ExtensionDeclaration, PeerDependency};

/// A declared peer, read: the peer's name, the versions of it the declaring extension accepts,
/// and whether the peer may be absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRequirement {
    pub name: String,
    pub range: VersionReq,
    pub optional: bool,
}

/// Why a declared peer's range can't be read: its text and semver's reason. `Display` is
/// "'one-ish' is not a SemVer requirement: unexpected character 'o' while parsing major version number".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableRange {
    pub range: String,
    pub reason: String,
}

impl fmt::Display for UnreadableRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' is not a SemVer requirement: {}",
            self.range, self.reason
        )
    }
}

impl std::error::Error for UnreadableRange {}

impl PeerRequirement {
    /// Read `declared`. Its range must be a SemVer requirement as Cargo reads one: `^1.2`, `~1`,
    /// `>=1, <2`, `1.x`, `*`, and `1.2.0`, which is `^1.2.0`. Anything else, the empty text
    /// included, is unreadable.
    pub fn read(declared: &PeerDependency) -> Result<Self, UnreadableRange> {
        let unreadable = |reason: String| UnreadableRange {
            range: declared.version.clone(),
            reason,
        };
        if declared.version.trim().is_empty() {
            return Err(unreadable(
                "the requirement is empty: write one such as ^1.0".to_string(),
            ));
        }
        let range = VersionReq::parse(&declared.version).map_err(|e| unreadable(e.to_string()))?;
        Ok(PeerRequirement {
            name: declared.name.clone(),
            range,
            optional: declared.optional,
        })
    }

    /// Whether this requirement accepts the installed `version` (text, as a declaration or a lock
    /// entry carries it): a SemVer version its range matches (a pre-release only when the range
    /// names a pre-release of the same MAJOR.MINOR.PATCH, semver's rule). `None` when the text is
    /// not a SemVer version, which no range accepts.
    pub fn accepts(&self, version: &str) -> Option<bool> {
        Version::parse(version)
            .ok()
            .map(|version| self.range.matches(&version))
    }
}

/// What the one rule says of one declared peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Installed at a version its range accepts, or optional and not installed.
    Satisfied,
    /// Required, and not installed.
    Missing,
    /// Installed at a version its range does not accept.
    OutOfRange { installed: String },
    /// Installed at a version that is not SemVer, which no range accepts.
    NotSemver { installed: String },
    /// Its range is not a SemVer requirement: no version satisfies it, installed or not.
    Unreadable(UnreadableRange),
}

impl Verdict {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Verdict::Satisfied)
    }
}

/// The one satisfaction rule: `declared` against `installed`, the version its peer is installed at
/// (`None`: no installed extension has its name). The range is read first, so a range that
/// can't be read is [`Verdict::Unreadable`] whether the peer is installed, missing or optional.
pub fn verdict(declared: &PeerDependency, installed: Option<&str>) -> Verdict {
    let requirement = match PeerRequirement::read(declared) {
        Ok(requirement) => requirement,
        Err(why) => return Verdict::Unreadable(why),
    };
    match installed {
        None if requirement.optional => Verdict::Satisfied,
        None => Verdict::Missing,
        Some(version) => match requirement.accepts(version) {
            Some(true) => Verdict::Satisfied,
            Some(false) => Verdict::OutOfRange {
                installed: version.to_string(),
            },
            None => Verdict::NotSemver {
                installed: version.to_string(),
            },
        },
    }
}

/// One extension as the peer rule reads it: its name, the version it is installed at, and the
/// peers it declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub peers: &'a [PeerDependency],
}

impl<'a> From<&'a ExtensionDeclaration> for Member<'a> {
    fn from(declaration: &'a ExtensionDeclaration) -> Self {
        Member {
            name: declaration.name(),
            version: declaration.version(),
            peers: declaration.peers(),
        }
    }
}

/// One declared peer a set of extensions leaves unsatisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsatisfied<'a> {
    /// The extension declaring the peer.
    pub dependent: &'a str,
    /// The peer as declared.
    pub declared: &'a PeerDependency,
    /// Never [`Verdict::Satisfied`].
    pub verdict: Verdict,
}

/// The peer requirements of a set of extensions, given in entry order (`specforge.json`'s): which
/// are unsatisfied, the order the set loads in and its cycles among required peers.
///
/// A peer is installed when a member has its name (the first such member). Peer edges are by
/// name; a peer on oneself orders nothing but is judged like any other.
#[derive(Debug, Clone)]
pub struct Peers<'a> {
    members: Vec<Member<'a>>,
    order: Vec<usize>,
    cycles: Vec<Vec<usize>>,
}

impl<'a> Peers<'a> {
    /// The peers of `members`, in entry order.
    pub fn of(members: impl IntoIterator<Item = Member<'a>>) -> Self {
        let members: Vec<Member<'a>> = members.into_iter().collect();
        let graph = Graph::of(&members);
        Peers {
            order: graph.order,
            cycles: graph.cycles,
            members,
        }
    }

    /// The members' indices (into the input) in load order: entry order, except that each member
    /// comes after the peers it declares. Every required peer counts. An optional one counts
    /// unless its edge would close a cycle; the optional edges are added after the required ones,
    /// in (dependent name, peer name) order, so which one is dropped does not depend on entry
    /// order. The members of a cycle among required peers come together, in entry order, after
    /// every other peer they declare. Given its own load order again, the result is the same
    /// order: the build of a built project is unchanged.
    pub fn load_order(&self) -> &[usize] {
        &self.order
    }

    /// Each cycle among required peers (a strongly connected set of two or more members): its
    /// members' names in entry order; the cycles in entry order of their first member.
    pub fn cycles(&self) -> impl Iterator<Item = Vec<&'a str>> + '_ {
        self.cycles
            .iter()
            .map(|cycle| cycle.iter().map(|&i| self.members[i].name).collect())
    }

    /// Every declared peer the set leaves unsatisfied: member by member in entry order, each
    /// member's peers in the order it declares them.
    pub fn unsatisfied(&self) -> Vec<Unsatisfied<'a>> {
        let index_of = index_by_name(&self.members);
        let mut unsatisfied = Vec::new();
        for member in &self.members {
            for declared in member.peers {
                let installed = index_of
                    .get(declared.name.as_str())
                    .map(|&i| self.members[i].version);
                let verdict = verdict(declared, installed);
                if !verdict.is_satisfied() {
                    unsatisfied.push(Unsatisfied {
                        dependent: member.name,
                        declared,
                        verdict,
                    });
                }
            }
        }
        unsatisfied
    }
}

/// Each name's first member.
fn index_by_name<'a>(members: &[Member<'a>]) -> HashMap<&'a str, usize> {
    let mut index_of = HashMap::new();
    for (i, member) in members.iter().enumerate() {
        index_of.entry(member.name).or_insert(i);
    }
    index_of
}

/// The peer edges of a set, an edge `(peer, dependent)` meaning the peer loads before the
/// dependent, and what is read off them.
struct Graph {
    order: Vec<usize>,
    cycles: Vec<Vec<usize>>,
}

impl Graph {
    fn of(members: &[Member<'_>]) -> Self {
        let n = members.len();
        let index_of = index_by_name(members);

        // The edges the members declare, by kind. A peer on oneself, or on a name no member has,
        // orders nothing.
        let mut required: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut optional: Vec<(usize, usize)> = Vec::new();
        for (dependent, member) in members.iter().enumerate() {
            for declared in member.peers {
                let Some(&peer) = index_of.get(declared.name.as_str()) else {
                    continue;
                };
                if peer == dependent {
                    continue;
                }
                if declared.optional {
                    optional.push((peer, dependent));
                } else {
                    required.insert((peer, dependent));
                }
            }
        }

        // Strongly connected sets of required peers: mutual reachability.
        let reach = Reach::of(n, required.iter().copied());
        let mut component: Vec<usize> = (0..n).collect();
        for (i, slot) in component.iter_mut().enumerate() {
            *slot = (0..=i)
                .find(|&j| reach.reaches(j, i) && reach.reaches(i, j))
                .unwrap_or(i);
        }
        let mut cycles: Vec<Vec<usize>> = Vec::new();
        for first in 0..n {
            if component[first] != first {
                continue;
            }
            let cycle: Vec<usize> = (first..n).filter(|&i| component[i] == first).collect();
            if cycle.len() > 1 {
                cycles.push(cycle);
            }
        }

        // Optional edges, after the required ones, in (dependent name, peer name) order; one
        // that would close a cycle is only a preference and is dropped.
        optional.sort_by(|a, b| {
            (members[a.1].name, members[a.0].name).cmp(&(members[b.1].name, members[b.0].name))
        });
        let mut edges = required;
        for (peer, dependent) in optional {
            if edges.contains(&(peer, dependent)) {
                continue;
            }
            if Reach::from(n, edges.iter().copied(), dependent).reaches(dependent, peer) {
                continue;
            }
            edges.insert((peer, dependent));
        }

        // Kahn's algorithm over the condensation, the smallest entry index first.
        let mut successors: HashMap<usize, BTreeSet<usize>> = HashMap::new();
        let mut waiting: HashMap<usize, usize> = HashMap::new();
        let mut members_of: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, &c) in component.iter().enumerate() {
            members_of.entry(c).or_default().push(i);
            waiting.entry(c).or_insert(0);
        }
        let between: BTreeSet<(usize, usize)> = edges
            .iter()
            .map(|&(peer, dependent)| (component[peer], component[dependent]))
            .filter(|(from, to)| from != to)
            .collect();
        for &(from, to) in &between {
            successors.entry(from).or_default().insert(to);
            *waiting.entry(to).or_insert(0) += 1;
        }
        // A component's first member is its smallest index, and is its name here.
        let mut ready: BTreeSet<usize> = waiting
            .iter()
            .filter(|&(_, &count)| count == 0)
            .map(|(&c, _)| c)
            .collect();
        let mut order = Vec::with_capacity(n);
        while let Some(c) = ready.pop_first() {
            order.extend(members_of[&c].iter().copied());
            for &next in successors.get(&c).into_iter().flatten() {
                let count = waiting.get_mut(&next).expect("every component is counted");
                *count -= 1;
                if *count == 0 {
                    ready.insert(next);
                }
            }
        }
        debug_assert_eq!(order.len(), n, "the condensation has no cycle");
        Graph { order, cycles }
    }
}

/// Which nodes reach which, over directed edges.
struct Reach {
    reached: Vec<Vec<bool>>,
}

impl Reach {
    /// Every node's reach.
    fn of(n: usize, edges: impl Iterator<Item = (usize, usize)>) -> Self {
        let out = adjacency(n, edges);
        let reached = (0..n).map(|from| walk(&out, from)).collect();
        Reach { reached }
    }

    /// Only `from`'s reach is filled in.
    fn from(n: usize, edges: impl Iterator<Item = (usize, usize)>, from: usize) -> Self {
        let out = adjacency(n, edges);
        let mut reached = vec![Vec::new(); n];
        reached[from] = walk(&out, from);
        Reach { reached }
    }

    fn reaches(&self, from: usize, to: usize) -> bool {
        from == to || self.reached[from].get(to).copied().unwrap_or(false)
    }
}

fn adjacency(n: usize, edges: impl Iterator<Item = (usize, usize)>) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); n];
    for (from, to) in edges {
        out[from].push(to);
    }
    out
}

/// The nodes reachable from `from` by one edge or more.
fn walk(out: &[Vec<usize>], from: usize) -> Vec<bool> {
    let mut seen = vec![false; out.len()];
    let mut stack: Vec<usize> = out[from].clone();
    while let Some(node) = stack.pop() {
        if !seen[node] {
            seen[node] = true;
            stack.extend(out[node].iter().copied());
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as spec;

    fn declared(name: &str, range: &str, optional: bool) -> PeerDependency {
        PeerDependency {
            name: name.to_string(),
            version: range.to_string(),
            optional,
        }
    }

    /// A member spec: `(name, [(peer, optional)])`, every range `^1.0`, every version 1.0.0.
    struct Spec {
        name: &'static str,
        peers: Vec<PeerDependency>,
    }

    fn ext(name: &'static str, peers: &[(&str, bool)]) -> Spec {
        Spec {
            name,
            peers: peers
                .iter()
                .map(|(peer, optional)| declared(peer, "^1.0", *optional))
                .collect(),
        }
    }

    fn peers_of(specs: &[Spec]) -> Peers<'_> {
        Peers::of(specs.iter().map(|s| Member {
            name: s.name,
            version: "1.0.0",
            peers: &s.peers,
        }))
    }

    fn members_of<'a>(order: &[&'a Spec]) -> Vec<Member<'a>> {
        order
            .iter()
            .map(|s| Member {
                name: s.name,
                version: "1.0.0",
                peers: &s.peers,
            })
            .collect()
    }

    fn names<'a>(specs: &'a [Spec], peers: &Peers<'_>) -> Vec<&'a str> {
        peers.load_order().iter().map(|&i| specs[i].name).collect()
    }

    #[spec(
        invariant = "peer_dependency_satisfaction",
        verify = "one rule judges a peer: its range read as SemVer, then the version its peer is installed at"
    )]
    fn the_peer_rule() {
        #[derive(Debug)]
        enum Want {
            Satisfied,
            Missing,
            OutOfRange,
            NotSemver,
            Unreadable,
        }
        use Want::*;
        let rows: &[(&str, bool, Option<&str>, Want)] = &[
            ("^1.0", false, Some("1.2.3"), Satisfied),
            ("^1.0", false, Some("2.0.0"), OutOfRange),
            ("~1.2.0", false, Some("1.2.5"), Satisfied),
            ("~1.2.0", false, Some("1.3.0"), OutOfRange),
            ("1.0.0", false, Some("1.0.0"), Satisfied),
            ("=1.0.0", false, Some("1.0.1"), OutOfRange),
            (">=1.0.0", false, Some("0.5.0"), OutOfRange),
            ("*", false, Some("9.9.9"), Satisfied),
            ("^1.0", false, None, Missing),
            ("^1.0", true, None, Satisfied),
            ("^1.0", true, Some("2.0.0"), OutOfRange),
            ("^1.0", false, Some("local"), NotSemver),
            ("^1.0", false, Some("1.0.0-rc.1"), OutOfRange),
            ("one-ish", false, Some("1.0.0"), Unreadable),
            ("one-ish", false, None, Unreadable),
            ("one-ish", true, None, Unreadable),
            ("", false, Some("1.0.0"), Unreadable),
        ];
        for (range, optional, installed, want) in rows {
            let got = verdict(&declared("@t/base", range, *optional), *installed);
            let ok = match want {
                Satisfied => got == Verdict::Satisfied,
                Missing => got == Verdict::Missing,
                OutOfRange => matches!(got, Verdict::OutOfRange { .. }),
                NotSemver => matches!(got, Verdict::NotSemver { .. }),
                Unreadable => matches!(got, Verdict::Unreadable(_)),
            };
            assert!(
                ok,
                "{range:?} optional={optional} installed={installed:?}: {got:?}, wanted {want:?}"
            );
        }
    }

    #[test]
    fn an_unreadable_range_names_its_text_and_the_reason() {
        let why = PeerRequirement::read(&declared("@t/base", "one-ish", false)).unwrap_err();
        assert_eq!(
            why.to_string(),
            "'one-ish' is not a SemVer requirement: unexpected character 'o' while parsing major version number"
        );
    }

    #[spec(
        behavior = "registry_build_load_order",
        verify = "a dependent listed before its peer loads after it"
    )]
    fn a_dependent_loads_after_its_peers() {
        let specs = [ext("B", &[("A", false)]), ext("A", &[])];
        assert_eq!(names(&specs, &peers_of(&specs)), ["A", "B"]);

        let specs = [
            ext("D", &[("B", false), ("C", false)]),
            ext("C", &[("A", false)]),
            ext("B", &[("A", false)]),
            ext("A", &[]),
        ];
        assert_eq!(names(&specs, &peers_of(&specs)), ["A", "C", "B", "D"]);

        let empty: [Spec; 0] = [];
        assert!(peers_of(&empty).load_order().is_empty());
    }

    #[spec(
        behavior = "registry_build_load_order",
        verify = "extensions with no peer between them keep the order they were given in"
    )]
    fn unrelated_members_keep_entry_order() {
        let specs = [ext("Z", &[]), ext("A", &[]), ext("M", &[])];
        assert_eq!(names(&specs, &peers_of(&specs)), ["Z", "A", "M"]);
    }

    #[spec(
        behavior = "registry_build_load_order",
        verify = "extensions naming each other as optional peers load without a cycle"
    )]
    fn a_mutual_optional_pair_loads_without_a_cycle() {
        let specs = [
            ext("B", &[("A", true)]),
            ext("A", &[("B", true)]),
            ext("C", &[]),
        ];
        let peers = peers_of(&specs);
        assert_eq!(names(&specs, &peers), ["B", "A", "C"]);
        assert_eq!(peers.cycles().count(), 0);

        let specs = [
            ext("C", &[]),
            ext("A", &[("B", true)]),
            ext("B", &[("A", true)]),
        ];
        assert_eq!(names(&specs, &peers_of(&specs)), ["C", "B", "A"]);
    }

    #[spec(
        behavior = "registry_build_load_order",
        verify = "a cycle among required peers is one E027 naming its extensions"
    )]
    fn a_required_cycle_is_one_strongly_connected_set() {
        let specs = [
            ext("A", &[("B", false)]),
            ext("B", &[("A", false)]),
            ext("C", &[("A", true)]),
        ];
        let peers = peers_of(&specs);
        assert_eq!(peers.cycles().collect::<Vec<_>>(), [["A", "B"]]);
        assert_eq!(names(&specs, &peers), ["A", "B", "C"]);

        let specs = [
            ext("X", &[("Y", false)]),
            ext("Y", &[("Z", false)]),
            ext("Z", &[("X", true)]),
        ];
        let peers = peers_of(&specs);
        assert_eq!(peers.cycles().count(), 0);
        assert_eq!(names(&specs, &peers), ["Z", "Y", "X"]);

        let specs = [
            ext("A", &[("B", false)]),
            ext("B", &[("A", false)]),
            ext("D", &[("A", false)]),
        ];
        let peers = peers_of(&specs);
        assert_eq!(peers.cycles().collect::<Vec<_>>(), [["A", "B"]]);
        assert_eq!(names(&specs, &peers), ["A", "B", "D"]);
    }

    #[spec(
        behavior = "registry_build_load_order",
        verify = "a load order given again comes back unchanged"
    )]
    fn a_load_order_given_again_comes_back_unchanged() {
        let sets: Vec<Vec<Spec>> = vec![
            vec![ext("B", &[("A", false)]), ext("A", &[])],
            vec![
                ext("D", &[("B", false), ("C", false)]),
                ext("C", &[("A", false)]),
                ext("B", &[("A", false)]),
                ext("A", &[]),
            ],
            vec![ext("Z", &[]), ext("A", &[]), ext("M", &[])],
            vec![
                ext("B", &[("A", true)]),
                ext("A", &[("B", true)]),
                ext("C", &[]),
            ],
            vec![
                ext("A", &[("B", false)]),
                ext("B", &[("A", false)]),
                ext("C", &[("A", true)]),
                ext("E", &[]),
                ext("D", &[("A", false)]),
            ],
            vec![
                ext("X", &[("Y", false)]),
                ext("Y", &[("Z", false)]),
                ext("Z", &[("X", true)]),
            ],
        ];
        for set in &sets {
            // Every rotation and reversal of the set as an entry order.
            let n = set.len();
            let mut entry_orders: Vec<Vec<usize>> = (0..n)
                .map(|r| (0..n).map(|i| (i + r) % n).collect())
                .collect();
            let reversed: Vec<Vec<usize>> = entry_orders
                .iter()
                .map(|o| o.iter().rev().copied().collect())
                .collect();
            entry_orders.extend(reversed);
            for entry in entry_orders {
                let entered: Vec<&Spec> = entry.iter().map(|&i| &set[i]).collect();
                let first = Peers::of(members_of(&entered));
                let ordered: Vec<&Spec> = first.load_order().iter().map(|&i| entered[i]).collect();
                let again = Peers::of(members_of(&ordered));
                let identity: Vec<usize> = (0..n).collect();
                assert_eq!(
                    again.load_order(),
                    identity.as_slice(),
                    "entry {:?} gave {:?}",
                    entered.iter().map(|s| s.name).collect::<Vec<_>>(),
                    ordered.iter().map(|s| s.name).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn unsatisfied_peers_come_in_entry_then_declared_order() {
        let specs = [
            ext("A", &[("A", false), ("Missing", false)]),
            Spec {
                name: "B",
                peers: vec![declared("A", "^2.0", false), declared("A", "nope", true)],
            },
        ];
        let peers = peers_of(&specs);
        let got: Vec<(&str, &str, Verdict)> = peers
            .unsatisfied()
            .into_iter()
            .map(|u| (u.dependent, u.declared.name.as_str(), u.verdict))
            .collect();
        assert_eq!(got.len(), 3, "{got:?}");
        // A's peer on itself is judged like any other: A is 1.0.0 and ^1.0 accepts it.
        assert_eq!(got[0], ("A", "Missing", Verdict::Missing));
        assert_eq!(
            got[1],
            (
                "B",
                "A",
                Verdict::OutOfRange {
                    installed: "1.0.0".into()
                }
            )
        );
        assert!(matches!(got[2].2, Verdict::Unreadable(_)));
    }
}
