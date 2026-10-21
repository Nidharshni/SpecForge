//! How the obligation rule sees one entity (ADR 0019).

use crate::compilation::RegistryBuild;
use crate::entity::{EntityRecord, Exemption};

/// How the obligation rule sees one entity (ADR 0019): whether its kind is
/// testable, which rule requires its kind to declare obligations, what
/// exempts it whatever its kind, and how many it declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// Its kind is testable (its kind's registry entry says so). Nothing is
    /// testable by default, and accepting `verify` statements does not make
    /// a kind testable.
    pub testable: bool,
    /// The code of the `no_verify_statements` rule that applies to its kind
    /// (the first in code order), if any: its kind must declare obligations.
    pub rule: Option<String>,
    /// What exempts it from obligations of its own, if anything.
    pub exemption: Option<Exemption>,
    /// How many obligations (`verify` statements) it declares.
    pub declared: usize,
}

impl Standing {
    /// The one obligation rule: read from the record and the registry build.
    pub fn of(record: &EntityRecord, registries: &RegistryBuild) -> Standing {
        Standing {
            testable: registries
                .kinds()
                .get(&record.kind)
                .is_some_and(|kind| kind.testable),
            rule: registries
                .rules()
                .verify_rule_for(&record.kind)
                .map(|rule| rule.code().to_string()),
            exemption: record.exemption.clone(),
            declared: record.obligations.len(),
        }
    }

    /// Its kind must declare obligations (a rule applies to its kind).
    pub fn obligated(&self) -> bool {
        self.rule.is_some()
    }

    /// It owes obligations of its own: obligated, and nothing exempts it.
    /// `PassEntity::owes_none` and `specforge_coverage::Entity::owes_none` are its
    /// negation.
    pub fn owes_obligations(&self) -> bool {
        self.rule.is_some() && self.exemption.is_none()
    }

    /// It declares at least one obligation (`verify` statement) of its own,
    /// whatever its kind owes: inspect's and the hover's "declared".
    pub fn declares_obligations(&self) -> bool {
        self.declared > 0
    }

    /// The facts the coverage rule counts it by.
    pub fn counting(&self) -> specforge_coverage::Counting {
        specforge_coverage::Counting {
            testable: self.testable,
            owes_obligations: self.owes_obligations(),
            declared: self.declared,
        }
    }

    /// It counts toward coverage: the coverage rule's
    /// [`Counting::counts`](specforge_coverage::Counting::counts) over its
    /// facts. Every reader (inspect, the coverage view, stats, the plan check) asks this.
    pub fn counts(&self) -> bool {
        self.counting().counts()
    }

    /// Testable, but it owes none and declares none, so it does not count
    /// (the coverage view's and inspect's `exempt`).
    pub fn exempt(&self) -> bool {
        self.counting().exempt()
    }

    /// The rule that reports it now: it owes obligations and declares none.
    pub fn reported_by(&self) -> Option<&str> {
        if self.owes_obligations() && self.declared == 0 {
            self.rule.as_deref()
        } else {
            None
        }
    }

    /// A verify stub fixes something here: it declares none, and either
    /// owes obligations (its stub fixes [`Self::reported_by`]) or is of a
    /// testable kind that nothing exempts. An exempt entity (a union, an
    /// `abstract` one, one whose kind accepts no `verify`) is offered none.
    pub fn wants_obligations(&self) -> bool {
        self.declared == 0 && self.exemption.is_none() && (self.rule.is_some() || self.testable)
    }
}
