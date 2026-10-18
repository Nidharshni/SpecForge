// Diagnostic types — compiler messages and validation codes

use "types/core"

type Diagnostic "Diagnostic" {
  code       ValidationCode @readonly
  severity   Severity
  message    string
  span       SourceSpan     @readonly
  context    string         @optional
  suggestion string         @optional
  // The values the message names, typed, for consumers that act on the
  // diagnostic (an editor's quick fix) instead of parsing its message.
  data       DiagnosticData @optional
  verify unit "Diagnostic schema is valid"
}

type DiagnosticData "A diagnostic's structured payload, tagged by kind. A kind exists only for a diagnostic some consumer acts on." = UnresolvedReferenceData
  | UnresolvedImportData
  | ShadowedKeywordData
  | ReferenceCycleData
  | SubjectData

type UnresolvedReferenceData "E003: entity's reference field names target, which no entity declares." {
  kind         "unresolved_reference" @literal
  target       string
  entity       string
  field        string
  did_you_mean string                 @optional
  verify unit "UnresolvedReferenceData schema is valid"
}

type UnresolvedImportData "E025: a use import names path, which resolves to no .spec file." {
  kind         "unresolved_import" @literal
  path         string
  did_you_mean string              @optional
  verify unit "UnresolvedImportData schema is valid"
}

type ShadowedKeywordData "E013, E026: keyword (an entity id or an extension's kind keyword) collides with a keyword the grammar or an earlier extension owns." {
  kind    "shadowed_keyword" @literal
  keyword string
  verify unit "ShadowedKeywordData schema is valid"
}

type ReferenceCycleData "W061: the reference cycle's entities, in path order." {
  kind "reference_cycle" @literal
  path string[]
  verify unit "ReferenceCycleData schema is valid"
}

type SubjectData "A diagnostic an extension pass raised about entity." {
  kind   "subject" @literal
  entity string
  verify unit "SubjectData schema is valid"
}

type ValidationCode "ValidationCode is a structured type with a display format: the prefix letter concatenated with the zero-padded number (e.g., E001, W012, I004). The canonical string form is used in diagnostics, documentation, and cross-references throughout the spec." {
  prefix CodePrefix
  number integer
  verify unit "ValidationCode schema is valid"
}

type CodePrefix "Code Prefix" = E | W | I

type Severity "Severity" = error | warning | info

type DiagnosticBag "Counts MUST equal the filtered length of the diagnostics array by severity: error_count == diagnostics.filter(d => d.severity == error).length, etc." {
  diagnostics Diagnostic[]
  error_count integer
  warn_count  integer
  info_count  integer
  verify unit "DiagnosticBag schema is valid"
}
