// @specforge/software features — capability groupings

use "behaviors/validation"

feature se_gherkin_bridge "Gherkin Bridge" {
  status   done
  problem  """
    Behavior entities need a way to reference Gherkin .feature files for
    BDD traceability. This is a domain-specific concern — not all spec
    domains use Cucumber/BDD — so it must be an extension responsibility,
    not a core grammar construct.
  """
  solution """
    The @specforge/software extension declares a gherkin field with type
    string_list and file_reference=true on behavior entities via the
    FieldRegistry. The field is parsed as a standard StringList value —
    no dedicated grammar rule or AST type is needed. File existence
    validation is handled by the generic validate_file_reference_paths
    behavior which operates on any field with file_reference=true (E016).
  """
}

feature se_core_entity_kinds "Core Entity Kind Registration" {
  status   done
  problem  """
    The @specforge/software extension must register 6 entity kinds with
    full metadata, 9 edge types, field definitions, validation rules,
    verify kinds, and LSP metadata. Without this registration, the
    compiler has zero knowledge of software engineering domain concepts.
  """
  solution """
    A comprehensive manifest declaration provides all entity kinds with
    testability flags, verify kind allowlists, LSP metadata (semantic
    tokens, icons), DOT shapes, typed field definitions with edge
    mappings, and declarative validation rules. Registration follows
    the zero-entity core protocol defined in ExtensionDeclaration.
  """
}

feature se_validation_suite "Software Validation Suite" {
  status   done
  problem  """
    Without domain-specific validation rules, the compiler can only
    perform structural checks. Unreferenced entities, unverified testables,
    invalid trigger references, and type annotation errors go undetected.
    Users receive no warnings about specification quality issues.
  """
  solution """
    Declarative validation rules (W001-W005, W007-W010, E004)
    detect common specification quality issues: unreferenced entities without
    incoming edges, testable entities without verify statements,
    features with empty behavior lists, unknown field
    annotations, unenforced invariants, and invalid verify kinds for entity
    types. Each rule uses the declarative pattern engine.
  """
}
