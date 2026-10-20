//! What the kind's registry answers about the entity an edit changes: where it is, its fields, and the refusals
//! for a field it does not declare.

use super::super::EditField;
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};
use specforge_common::find_close_match;
use specforge_formatter::FormatConfig;
use specforge_graph::field_form::Reading;
use specforge_registry::FieldRegistryEntry;

/// The edit names a field its kind does not declare.
const UNKNOWN_FIELD: &str = "unknown_field";
/// The edit names the other side of a relation another kind owns (ADR 0150, ADR 0157).
const OTHER_SIDE: &str = "other_side";

/// The entity an edit changes, and where it is.
pub struct Site<'a> {
    pub view: &'a ProjectView<'a>,
    /// The file, relative to the spec root.
    pub key: &'a str,
    pub id: &'a str,
    pub kind: &'a str,
    /// The format configuration of the entity's file, looked up once per plan (ADR 0169).
    pub config: FormatConfig,
}

impl Site<'_> {
    /// How `field` of the entity's kind reads; refused when the kind does not declare it. A value naming `target`
    /// (a link's or unlink's): an undeclared `field` whose target's
    /// kind owns a relation into this kind is told as that relation's other side, as E079 tells it by its values.
    pub fn reading_toward(
        &self,
        field: &str,
        target: Option<&str>,
    ) -> Result<Reading<'_>, Refusal<EditField>> {
        match self
            .view
            .registries()
            .field_forms()
            .reading(self.kind, field)
        {
            Reading::Undeclared => Err(self.unknown_field(field, target)),
            reading => Ok(reading),
        }
    }

    /// `other_side` when `owning_fields` names an owner, else `unknown_field` with its near miss.
    fn unknown_field(&self, field: &str, target: Option<&str>) -> Refusal<EditField> {
        let target = target.and_then(|id| {
            let node = self.view.graph().node(id)?;
            Some((id, node.kind.raw.as_str()))
        });
        let owners = self.view.registries().fields().owning_fields(
            self.kind,
            field,
            target.map(|(_, kind)| kind),
        );
        if !owners.is_empty() {
            return self.other_side(field, &owners, target);
        }
        let declared = order(self.view, self.kind);
        let error = OpError::new(
            OpErrorKind::InvalidInput,
            UNKNOWN_FIELD,
            format!("kind '{}' has no field '{field}'", self.kind),
        );
        match find_close_match(field, declared.iter().map(String::as_str)) {
            Some(near) => error.with_suggestion(format!("did you mean '{near}'?")),
            None => error,
        }
        .about(EditField::Field)
    }

    /// The `other_side` refusal: the owners as E079 names them, and where to change it instead. `target` is
    /// `(id, kind)` when the graph knows the link's target.
    fn other_side(
        &self,
        field: &str,
        owners: &[&FieldRegistryEntry],
        target: Option<(&str, &str)>,
    ) -> Refusal<EditField> {
        let names = |owners: &mut dyn Iterator<Item = &&FieldRegistryEntry>| {
            owners
                .map(|o| format!("'{}'", o.name()))
                .collect::<Vec<_>>()
                .join(" or ")
        };
        let of_target = target.map(|(id, kind)| {
            let names = names(&mut owners.iter().filter(|o| o.kind_name() == kind));
            (names, kind, id)
        });
        let suggestion = match of_target {
            Some((names, kind, id)) if !names.is_empty() => {
                format!("change it on the owning side: {names} of {kind} '{id}'")
            }
            _ => {
                let each: Vec<String> = owners
                    .iter()
                    .map(|o| format!("'{}' of each {} it names", o.name(), o.kind_name()))
                    .collect();
                format!("change it on the owning side: {}", each.join(" or "))
            }
        };
        let owned: Vec<String> = owners
            .iter()
            .map(|o| format!("{}.{}", o.kind_name(), o.name()))
            .collect();
        OpError::new(
            OpErrorKind::InvalidInput,
            OTHER_SIDE,
            format!(
                "field '{field}' on {} '{}' is the other side of {}: a relation is written on its owning side only",
                self.kind,
                self.id,
                owned.join(" or ")
            ),
        )
        .with_entity(self.id)
        .with_suggestion(suggestion)
        .about(EditField::Field)
    }
}

/// The fields `kind` declares, in the order a block writes them: the kind's own in declaration order, then every
/// other registered field (shared, enhancements') by name. Empty for a kind no loaded extension declares.
pub fn order(view: &ProjectView<'_>, kind: &str) -> Vec<String> {
    let registries = view.registries();
    let mut order: Vec<String> = registries
        .kinds()
        .get(kind)
        .map(|k| k.declared.fields.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();
    let mut more: Vec<String> = registries
        .fields()
        .fields_for_kind(kind)
        .iter()
        .map(|f| f.name().to_string())
        .filter(|name| !order.contains(name))
        .collect();
    more.sort();
    order.extend(more);
    order
}
