// Error types — typed errors for compiler operations

use "types/core"

type ParseError "Parse Error" {
  _tag    "ParseError" @literal
  message string
  span    SourceSpan   @readonly
  verify unit "ParseError schema is valid"
}

type ResolutionError "Resolution Error" {
  _tag          "ResolutionError" @literal
  message       string
  span          SourceSpan        @readonly
  unresolved_id string
  suggestions   string[]          @optional
  verify unit "ResolutionError schema is valid"
}

type CycleError "Cycle Error" {
  _tag         "CycleError" @literal
  message      string
  span         SourceSpan   @readonly
  participants string[]
  // String representation of EdgeType.label for diagnostic display
  edge_type    string
  verify unit "CycleError schema is valid"
}

type DuplicateIdError "Duplicate Id Error" {
  _tag        "DuplicateIdError" @literal
  entity_id   string
  first_span  SourceSpan         @readonly
  second_span SourceSpan         @readonly
  verify unit "DuplicateIdError schema is valid"
}

type ValidationError "Validation Error" {
  _tag    "ValidationError" @literal
  code    string
  message string
  span    SourceSpan        @readonly
  verify unit "ValidationError schema is valid"
}

type EmitterError "Emitter Error" {
  _tag        "EmitterError" @literal
  message     string
  output_path string         @optional
  verify unit "EmitterError schema is valid"
}

type ExtensionError "Extension Error" {
  _tag            "ExtensionError" @literal
  extension_name  string
  message         string
  wasm_trap       string           @optional
  lifecycle_state string           @optional
  peer_dependency string           @optional
  verify unit "ExtensionError schema is valid"
}

type UnknownKindError "Unknown Kind Error" {
  _tag        "UnknownKindError" @literal
  code        string             @readonly
  message     string
  span        SourceSpan         @readonly
  keyword     string
  suggestions string[]           @optional
  verify unit "UnknownKindError schema is valid"
}

type RegistryError "Registry Error" {
  _tag         "RegistryError" @literal
  message      string
  registry_url string          @optional
  status_code  integer         @optional
  verify unit "RegistryError schema is valid"
}
