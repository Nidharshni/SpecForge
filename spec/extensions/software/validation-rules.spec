// @specforge/software validation rules — declarative validation patterns

use "extensions/software/invariants"
use "extensions/software/types"
use "types/zero-entity-core"

defaults behavior {
  features [se_validation_suite]
  types    [ValidationRulePattern]
}

behavior se_validate_unreferenced_behaviors "W001: Unreferenced Behaviors" {
  category   query
  invariants [se_edge_consistency]
  contract   """
    Detect behavior entities with no incoming Implements edges.
  """
  requires {
    graph_built "entity graph is fully constructed with all edges"
  }
  ensures {
    unreferenced_detected "behavior with no incoming Implements edge produces W001 warning"
    referenced_passes     "behavior with incoming Implements edge produces no diagnostic"
    correct_severity      "W001 severity is warning"
    correct_template      "message template is: behavior '{id}' is not referenced by any feature"
  }
  verify unit "behavior with no incoming Implements edge produces W001"
  verify unit "behavior with incoming Implements edge passes"
  verify unit "W001 severity is warning"
}

behavior se_validate_unreferenced_types "W002: Unreferenced Types" {
  category query
  contract """
    Detect type entities nothing references. A type is referenced when a
    behavior lists it in `types`, an event carries it as `payload`, another
    type names it in a field type (`members TsClassMember[]`, through the
    derived `composed_types` field) or a port names it in a method
    parameter or return type (`detect(config: TsExtensionConfig)`, through
    the derived `types` field). A primitive or generic wrapper name such as
    `string` or `Result` references nothing, and a type naming itself does
    not count.
  """
  requires {
    graph_built "entity graph is fully constructed with all edges"
  }
  ensures {
    unreferenced_detected "type with no incoming edge produces W002 warning"
    correct_template      "message template is: type '{id}' is not referenced by any behavior, port, or type"
  }
  verify unit "type with no incoming UsesType edge produces W002"
  verify unit "type with incoming UsesType edge passes"
  verify integration "a type named in another type's field type is referenced"
  verify integration "a type named in a port method signature is referenced"
  verify integration "a primitive or generic wrapper name references no type"
}

behavior se_validate_unused_invariants "W003: Unused Invariants" {
  category query
  contract """
    Detect invariant entities that nothing references: no behavior lists
    them in `invariants`, `requires`, `ensures` or `maintains`. This is the
    only unreferenced-invariant finding; `analyze coverage` counts them without
    reporting them again.
  """
  requires {
    graph_built "entity graph is fully constructed with all edges"
  }
  ensures {
    unused_detected   "invariant nothing references produces W003"
    referenced_passes "invariant with incoming reference edge produces no diagnostic"
  }
  verify unit "invariant nothing references produces W003"
  verify unit "invariant with incoming reference edge passes"
}

behavior se_validate_unreferenced_ports "W005: Unreferenced Ports" {
  category query
  contract """
    Detect port entities with no incoming UsesPort edges.
  """
  ensures {
    unreferenced_detected "port with no incoming UsesPort edge produces W005"
    correct_template      "message template is: port '{id}' is not referenced by any behavior"
  }
  verify unit "port with no incoming UsesPort edge produces W005"
  verify unit "port with incoming UsesPort edge passes"
}

// W006 is allocated to @specforge/product (Unreferenced Capabilities → W042)

behavior se_validate_unreferenced_events "W007: Unreferenced Events" {
  category query
  contract """
    Detect event entities with no incoming Produces edges.
  """
  ensures {
    unreferenced_detected "event with no incoming Produces edge produces W007"
    correct_template      "message template is: event '{id}' is not produced by any behavior"
  }
  verify unit "event with no incoming Produces edge produces W007"
  verify unit "event with incoming Produces edge passes"
}

behavior se_validate_features_with_empty_behaviors "W008: Features with Empty Behaviors" {
  category query
  contract """
    Detect feature entities with an empty behaviors list.
  """
  ensures {
    empty_detected   "feature with empty behaviors list produces W008"
    non_empty_passes "feature with at least one behavior produces no diagnostic"
    correct_template "message template is: feature '{id}' has no behaviors -- specification may be incomplete"
  }
  verify unit "feature with empty behaviors list produces W008"
  verify unit "feature with at least one behavior suppresses W008"
}

behavior se_validate_port_methods "E004: Invalid Port Methods" {
  category query
  types    [ValidationRulePattern, PortOperation]
  contract """
    Detect port entities whose methods block contains operations with
    invalid type signatures.
  """
  requires {
    type_registry_available "all declared type entities are registered"
  }
  ensures {
    valid_types_pass    "port method with valid type references produces no diagnostic"
    invalid_types_error "port method with unknown type reference produces E004 error"
    correct_template    "message template is: port '{id}' method '{field}' references unknown type '{value}'"
  }
  verify unit "port method with valid type references passes"
  verify unit "port method with unknown type reference produces E004"
}

behavior se_validate_type_field_annotations "W010: Unknown Field Annotations" {
  category query
  types    [ValidationRulePattern, FieldAnnotation]
  contract """
    Detect type entities whose field definitions contain unknown
    annotations. Valid: @readonly, @unique, @optional, @literal.
  """
  ensures {
    known_passes     "field with @readonly annotation produces no diagnostic"
    unknown_warns    "field with unknown annotation produces W010 warning"
    correct_template "message template is: type '{id}' field '{field}' has unknown annotation '{value}'"
  }
  verify unit "field with @readonly annotation passes"
  verify unit "field with @unknown annotation produces W010"
}

behavior se_validate_milestone_behavior_ranges "E010: Invalid Milestone Behavior Range" {
  category validation
  contract """
    The @specforge/software extension MUST declare a custom validation
    rule, run by its Wasm validate__milestone_behavior_ranges function,
    that checks a milestone's behaviors range: the range must be well
    formed, its start must not come after its end, and every behavior
    it expands to must exist. An invalid range MUST produce an E010 error
    naming the reason.
  """
  verify unit "a valid milestone behavior range passes"
  verify unit "a range whose start comes after its end produces E010"
  verify unit "a range naming a behavior that does not exist produces E010"
}
