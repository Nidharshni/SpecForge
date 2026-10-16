//! The host's checks over the entity snapshot's records (ADR 0019, ADR 0031):
//! the structural checks, in one order behind one gate, then the rule set.
//! Only [`RegistryBuild::check`] reaches them; no check reads a graph node,
//! and none reaches the file system but through `input.disk` (ADR 0107).

mod fields;
mod identifiers;
mod kinds;
mod obligations;
mod references;
mod refs;
mod values;

use specforge_common::Diagnostic;

use crate::RegistryBuild;
use crate::entity::RuleInput;
use crate::rules::CustomVerdicts;

impl RegistryBuild {
    /// Every check over `input`, the entity snapshot's records, in this
    /// order:
    ///
    /// 1. W012: a `ref` nothing references;
    /// 2. unless [`Self::structural_only`]: E024 unknown kind, E013
    ///    reserved ID, E014 identifier length, W020 unknown field, W158 verify
    ///    statements on a kind that accepts none, E022 reference to the wrong kind, E061 value not of its declared type;
    ///    when structural-only with extensions loaded, one W151 instead;
    /// 3. the rule set ([`crate::rules::Rules::check`]), asking `verdicts`
    ///    for `custom` rules.
    ///
    /// The suggestions of E024, W151 and W020 come from the build's candidate vocabulary
    /// ([`Self::with_candidates`], ADR 0122); it changes no other part of a diagnostic.
    ///
    /// Each check reports its entities in record (id) order. The rule set's
    /// `file_exists` rules, E016 among them (the host builds one per
    /// `file_reference` field), ask `input.disk` whether a path exists and
    /// list a missing one's directory for the suggestion; nothing else reads
    /// the file system.
    pub fn check(&self, input: &RuleInput<'_>, verdicts: &dyn CustomVerdicts) -> Vec<Diagnostic> {
        let entities = input.entities;
        let mut diagnostics = refs::unreferenced(entities); // W012
        if self.structural_only() {
            // W151: extensions are loaded but none declares a kind.
            diagnostics.extend(kinds::unchecked(
                entities,
                !self.declarations().is_empty(),
                &self.candidates,
            ));
        } else {
            diagnostics.extend(kinds::unknown(entities, &self.kinds, &self.candidates)); // E024
            diagnostics.extend(identifiers::reserved(entities, &self.kinds)); // E013
            diagnostics.extend(identifiers::length(entities)); // E014
            diagnostics.extend(fields::unknown(
                entities,
                &self.kinds,
                &self.fields,
                &self.candidates,
            )); // W020
            diagnostics.extend(obligations::refused(entities, &self.kinds)); // W158
            diagnostics.extend(references::mistyped(entities, &self.kinds, &self.fields)); // E022
            diagnostics.extend(values::mistyped(entities, &self.kinds, &self.fields)); // E061
        }
        diagnostics.extend(self.rules.check(input, verdicts));
        diagnostics
    }

    /// No loaded extension declares an entity kind: the checks that read
    /// kinds, fields and identifiers do not run. With no extension loaded
    /// the environment says so (I002); with some loaded, `check` reports
    /// W151 for the entities it leaves unchecked.
    pub fn structural_only(&self) -> bool {
        self.kinds.is_empty()
    }
}
