//! The host's checks over the entity snapshot's records (ADR 0019, ADR 0031):
//! the structural checks, in one order behind one gate, then the rule set.
//! Only [`RegistryBuild::check`] and [`RegistryBuild::files`] reach them; no
//! check reads a graph node.

mod fields;
mod files;
mod identifiers;
mod index;
mod kinds;
mod references;
mod refs;
mod values;

use std::path::PathBuf;

use specforge_common::Diagnostic;

use crate::RegistryBuild;
use crate::entity::RuleInput;
use crate::rules::CustomVerdicts;

impl RegistryBuild {
    /// Every check over `input`, the entity snapshot's records, in this
    /// order:
    ///
    /// 1. W012: a `ref` nothing references;
    /// 2. E016: a path a `file_reference` field names that does not exist
    ///    under `input.spec_root` (with a close sibling as the suggestion);
    /// 3. unless [`Self::structural_only`]: E024 unknown kind, E013
    ///    reserved ID, E014 identifier length, W020 unknown field, E022
    ///    reference to the wrong kind, E061 value not of its declared type;
    ///    when structural-only with extensions loaded, one W151 instead;
    /// 4. the rule set ([`crate::rules::Rules::check`]), asking `verdicts`
    ///    for `custom` rules.
    ///
    /// Each check reports its entities in record (id) order. E016 and the
    /// rules' `file_exists` ask the file system whether a path exists;
    /// nothing else does I/O.
    pub fn check(&self, input: &RuleInput<'_>, verdicts: &dyn CustomVerdicts) -> Vec<Diagnostic> {
        let entities = input.entities;
        let mut diagnostics = refs::unreferenced(entities); // W012
        diagnostics.extend(files::missing(entities, &self.fields, input.spec_root)); // E016
        if self.structural_only() {
            // W151: extensions are loaded but none declares a kind.
            diagnostics.extend(kinds::unchecked(entities, !self.declarations().is_empty()));
        } else {
            diagnostics.extend(kinds::unknown(entities, &self.kinds)); // E024
            diagnostics.extend(identifiers::reserved(entities, &self.kinds)); // E013
            diagnostics.extend(identifiers::length(entities)); // E014
            diagnostics.extend(fields::unknown(entities, &self.kinds, &self.fields)); // W020
            diagnostics.extend(references::mistyped(entities, &self.kinds, &self.fields)); // E022
            diagnostics.extend(values::mistyped(entities, &self.kinds, &self.fields)); // E061
        }
        diagnostics.extend(self.rules.check(input, verdicts));
        diagnostics
    }

    /// The files the checks read on `input`, resolved against
    /// `input.spec_root`, sorted and unique: every path a `file_reference`
    /// field names (E016) and every file a `file_exists` rule reads. A
    /// project session's check inputs.
    pub fn files(&self, input: &RuleInput<'_>) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = input
            .entities
            .iter()
            .flat_map(|record| files::paths(record, &self.fields))
            .map(|path| input.spec_root.join(path))
            .collect();
        paths.extend(self.rules.files(input));
        paths.sort();
        paths.dedup();
        paths
    }

    /// No loaded extension declares an entity kind: the checks that read
    /// kinds, fields and identifiers do not run. With no extension loaded
    /// the environment says so (I002); with some loaded, `check` reports
    /// W151 for the entities it leaves unchecked.
    pub fn structural_only(&self) -> bool {
        self.kinds.is_empty()
    }
}
