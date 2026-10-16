// Resolution behaviors — import resolution and symbol linking

use "events/compilation"
use "invariants/core"
use "invariants/validation"
use "invariants/zero-entity-core"
use "ports/outbound"
use "types/core"
use "types/errors"
use "types/graph"

behavior resolve_use_imports "Resolve Use Imports" {
  features   [reference_resolution]
  invariants [import_dag, reference_resolution_completeness, compilation_pipeline_ordering]
  category   query
  types      [SpecFile, FileEntry]
  consumes   [registries_populated]
  requires {
    registries_populated_fired "registries_populated event has fired, confirming KindRegistry and FieldRegistry are ready"
    sources_held               "The compile holds its sources: the path relative to the spec root of every .spec file discovery kept, parsed or unreadable, and of every editor buffer of one"
  }
  ensures {
    imports_resolved        "All use directives are resolved to the project sources they name, relative to the spec root"
    missing_files_diagnosed "An import that names no project source produces E025"
    dependency_graph_built  "File dependency graph is constructed from resolved imports"
  }
  contract   """
    The resolver MUST resolve a use directive against the project's
    sources, the .spec files the compile holds, by their paths relative
    to the spec root, and MUST NOT read, list or stat any file. A
    relative path (./ or ../) is read from the importing file's
    directory, any other path from the spec root. The .spec extension
    MUST be appended implicitly, and a dotted name keeps its dots
    ("api.v2" names api.v2.spec); a path that already ends in .spec
    names that file; when no source has that path, the path's
    index.spec names the directory. Paths compare exactly, whatever the
    case rules of the file system. An import that names no source MUST
    produce E025: no file by that path, a file an exclude entry leaves
    out, one under a skipped directory, one inside a nested project, one
    inside a symlinked
    directory, and one above the spec root (whichever step of the
    cascade names it) are all no source. An @scope/name import names an
    extension (I004 when it is not installed). A .spec file discovery
    finds that cannot be read (not UTF-8, no permission) MUST produce an
    E025 naming its path, in a compile and after every update, and is
    left out of the graph; it is still a source, so an import of it
    reports nothing more and a re-export from it no W027. An editor
    buffer the session holds for a file discovery would find is a
    source, saved or not.
    The resolver MUST build the file dependency graph from imports.
    A .spec file that an exclude entry of specforge.json leaves out MUST
    NOT be compiled, on every surface (check, watch, the LSP and MCP),
    and no import names it. Each entry is a pattern over a file's path
    relative to the spec root, written with /: * and ? match within one
    name, ** any number of directories, [...] one character of a set.
    An entry with no / except a trailing one matches a name at any depth
    ("drafts" leaves out drafts/a.spec and sub/drafts/b.spec, not
    mydrafts/c.spec); an entry with a / at its start or in its middle is
    anchored at the spec root ("/drafts", "sub/drafts"). A trailing /
    matches directories only. A file is left out when an entry matches
    its path or the path of one of its directories. An entry that can
    match no source (empty, starting with !, with a . or .. name, or not
    a glob pattern) is ignored, and specforge.json reports it (E069).
    A file under a skipped directory (target, node_modules, .git, .hg,
    dist, build) inside the spec root is not compiled either, on every
    surface; the spec root itself is read whatever its own name and
    whatever it holds. A directory below the spec root that holds
    specforge.json is a nested project: no file inside it is a source of
    this project, on every surface (check, watch, the LSP, MCP, format,
    migrate, rename), an editor buffer of one included, and no import
    names it; its own project compiles it. Only files are sources: a directory named like a .spec file is none, and
    a symlink is a source only when it resolves to a file, read through
    the link wherever it points and keyed by its own path; a symlinked
    directory is never entered.
  """
  verify unit "a use path resolves to the project source it names"
  verify unit "missing import file produces E025"
  verify unit "an import path may spell out the .spec extension"
  verify unit "relative import traversing above spec_root is rejected"
  verify unit "an import reaching above spec_root by any cascade step produces E025"
  verify integration "imports across nested directories resolve correctly"
  verify contract "Resolve Use Imports: use import resolution holds — registries_populated_fired, sources_held, imports_resolved, missing_files_diagnosed, dependency_graph_built"
  verify unit "resolve extension import path"
  verify unit "symlink pointing outside spec_root is rejected"
  verify unit "files matching an exclude entry are not compiled"
  verify unit "a source that cannot be read produces E025 naming it"
  verify unit "a spec root named like a skipped directory is compiled"
  verify unit "a directory named like a .spec file is no source"
  verify unit "a .spec file inside a nested project below the spec root is no source"
  verify unit "a nested project's files are not compiled, an editor buffer of one included"
  verify unit "an editor buffer of a file inside a symlinked directory is no source"
  verify unit "a symlink is a source only when it resolves to a file"
  verify unit "discovery at a path finds what the whole walk finds there"
  verify unit "an import of a file that is no project source produces E025"
  verify unit "an import of an unreadable source reports only that source's E025"
  verify unit "an import names an editor buffer's source, saved or not"
  verify unit "a dotted import path names the file with .spec appended"
  verify unit "import paths match source paths exactly, whatever the file system's case rules"
  verify unit "an exclude entry without a slash matches a name at any depth"
  verify unit "an exclude entry with a slash is anchored at the spec root"
  verify unit "exclude entries are glob patterns matched against a file and its directories"
  verify unit "an exclude entry that can match no source is E069 and ignored"
}

// No consumes — called inline during use import resolution
behavior detect_import_cycles "Detect Import Cycles" {
  features   [reference_resolution]
  invariants [import_dag]
  category   validation
  types      [CycleError, FileEntry]
  requires {
    import_graph_available "File dependency graph from use imports has been constructed"
  }
  ensures {
    cycles_detected          "All circular import chains are identified via topological sort"
    cycle_diagnostic_emitted "Each detected cycle produces a W113 warning naming all participating files"
    non_cyclic_unaffected    "Files not involved in any cycle continue processing normally"
  }
  contract   """
    The resolver MUST detect circular use imports using topological sort.
    When a cycle is detected, the compiler MUST emit a W113 warning that
    names all files participating in the cycle, starting from the
    lexicographically smallest path, with the cycles in sorted order.
    Files involved in a cycle MUST NOT prevent processing of non-cyclic
    files, and are compiled themselves.
  """
  verify unit "detect direct cycle between two files"
  verify unit "detect transitive cycle across three files"
  verify unit "non-cyclic files still process when a cycle exists"
  verify contract "Detect Import Cycles: import cycle detection holds — import_graph_available, cycles_detected, cycle_diagnostic_emitted, non_cyclic_unaffected"
  verify unit "W113 carries actionable suggestion"
}

behavior link_entity_references "Link Entity References" {
  features   [reference_resolution]
  invariants [string_interning_consistency, entity_id_uniqueness, reference_resolution_completeness]
  category   command
  types      [EntityId, Graph, Edge, ResolutionError]
  produces   [resolution_complete]
  requires {
    registries_populated "KindRegistry and FieldRegistry are populated from extension manifests"
    all_files_parsed     "All .spec files have been parsed"
  }
  ensures {
    all_references_resolved "Every reference a field's declared type reads, a list item or a single reference, resolves to a declared entity or emits E003 (I004 when its target kind's extension is not enabled); one that names an entity of another kind than its field's target kind is E022"
    no_silent_ignoring      "No reference is silently ignored"
  }
  contract   """
    The resolver MUST link every entity ID in a reference list to the
    declaration site of that entity. Each resolved reference MUST be
    recorded as a pending edge (source, target, edge type) for the
    graph builder to materialize. Unresolvable references MUST produce
    an E003 diagnostic with a "did you mean?" suggestion when a close
    match exists. A reference whose field declares a target kind and that
    names an existing entity of another kind MUST be E022 at the
    reference, whether or not a loaded extension declares that kind; it
    still links, so navigation and rename reach it.
  """
  verify unit "reference list IDs create graph edges"
  verify unit "unresolvable reference produces E003"
  verify unit "close match triggers did-you-mean suggestion"
  verify unit "a single reference that resolves to nothing is E003"
  verify unit "a quoted single reference is a reference"
  verify unit "a string list's items reference nothing but scheme ref IDs"
  verify unit "a quoted id on a reference list field is a reference"
  verify unit "an open-fields kind's undeclared field references nothing"
  verify unit "an unresolved reference in a field whose target kind no extension declares is I004 naming the extension"
  verify unit "an unresolved reference to a loaded or undeclared target kind is E003 alone"
  verify unit "a reference naming an existing entity of the absent target kind links without a diagnostic"
  verify unit "a reference to an existing entity of another kind than its field's target kind is E022 at the reference"
  verify unit "a single reference to an existing entity of another kind than its field's target kind is E022 at the reference"
  verify contract "Link Entity References: entity reference linking holds — registries_populated, all_files_parsed, all_references_resolved, no_silent_ignoring"
  verify unit "E003 span covers exactly the unresolved identifier token"
  verify unit "cross-file duplicate entity ID produces E002 naming the first declaration"
  verify unit "same ID different kind across files warns W060 with both kinds named"
}

behavior resolve_soft_cross_extension_references "Resolve Soft Cross-Extension References" {
  features   [reference_resolution]
  invariants [reference_resolution_completeness]
  category   query
  types      [EntityId]
  consumes   [registries_populated]
  requires {
    registries_populated_fired         "registries_populated event has fired, confirming KindRegistry is available for keyword lookup"
    known_extensions_catalog_available "Known-extensions catalog (keyword-to-extension mapping) is loaded"
  }
  ensures {
    suggestion_emitted            "Unknown keywords matching a known extension get an E024 whose suggestion names that extension"
    installed_extensions_resolved "Keywords from installed extensions follow normal E003 resolution"
  }
  contract   """
    When an entity block uses a keyword not present in the KindRegistry,
    the semantic validation phase MUST emit E024 via
    detect_unknown_entity_kinds, once per entity and with no second
    diagnostic for the same keyword. Its suggestion consults the
    known-extensions catalog (a curated mapping of keywords to extensions,
    suggest_missing_extensions): if an installable extension provides the
    keyword, the suggestion MUST name it ("install it with: specforge add
    X"). The keyword stays an error: a project that uses a kind without
    enabling its extension does not pass the check.
    This is about entity declarations with unknown keywords. An unresolvable
    ID in a field whose declared target kind no loaded extension declares is
    I004 (link_entity_references), whose suggestion consults the same
    catalog.
    When the extension IS installed and the target entity does not exist,
    normal E003 validation via link_entity_references MUST apply.
  """
  verify unit "unknown keyword matching known extension gets E024 naming the extension"
  verify unit "installed extension with missing entity emits E003"
  verify unit "installed extension with imported file but missing entity emits E003"
  verify contract "Resolve Soft Cross-Extension References: soft cross-extension resolution holds — registries_populated_fired, known_extensions_catalog_available, suggestion_emitted, installed_extensions_resolved"
}

// No consumes — called inline during reference resolution
behavior resolve_external_ref_declarations "Resolve External Ref Declarations" {
  features   [reference_resolution]
  invariants [reference_resolution_completeness]
  category   query
  types      [EntityId, Graph]
  ports      [FileSystem]
  requires {
    ref_blocks_parsed    "All ref blocks have been parsed with scheme, kind, and identifier extracted"
    filesystem_available "FileSystem port is available for provider validation"
  }
  ensures {
    refs_registered          "Each ref declaration is registered as a node in the graph with scheme metadata"
    known_schemes_marked     "Refs with schemes matching installed providers are marked for provider validation"
    unknown_schemes_deferred "Refs with unrecognized schemes emit I005 and are deferred"
  }
  contract   """
    The resolver MUST process ref entity declarations, registering each
    ref as a node in the in-memory graph with its scheme and identifier.
    Refs with schemes that match an installed provider MUST be marked for
    provider validation. Refs with unrecognized schemes MUST be deferred
    and emit I005 if no provider is installed for the scheme.
  """
  verify unit "ref with known scheme is registered and marked for provider validation"
  verify unit "ref with unknown scheme emits I005"
  verify unit "ref node is added to graph with scheme metadata"
  verify contract "Resolve External Ref Declarations: external ref resolution holds — ref_blocks_parsed, filesystem_available, refs_registered, known_schemes_marked, unknown_schemes_deferred"
}

behavior resolve_reexports "Resolve pub use Re-exports" {
  features   [reference_resolution]
  invariants [import_dag]
  category   query
  types      [FileEntry]
  requires {
    imports_resolved "use imports have been resolved to files"
  }
  ensures {
    all_reexported       "pub use adds every entity the target exports to the re-exporting file's exports"
    selective_reexported "pub use { A, B } adds only the named entities to the file's exports"
    chains_transitive    "exports through several pub use files chain transitively"
    plain_use_private    "a plain use adds nothing to the file's exports"
    unknown_name_warned  "a selective re-export of a name the target doesn't export is W027"
  }
  contract   """
    References resolve across the project without use (ADR 0004 D1-a),
    so a file's exports decide no reference; they decide W027. A file
    exports the entities it declares. `pub use "target"` MUST add every
    entity the target exports, and `pub use { A, B } from "target"` MUST
    add only the named ones (`A as B` adds B). Exports MUST chain
    transitively, so a barrel index file can re-export its sub-files. A
    plain `use` MUST NOT add anything. A selective re-export of a name the
    target doesn't export MUST produce W027. When files import each other
    in a cycle, a re-export through a cycle participant MUST use only that
    file's declared set, never a transitive closure through the cycle.
  """
  verify unit "pub use re-exports all entities from target"
  verify unit "pub use selective re-exports only named entities"
  verify unit "pub use chains resolve transitively"
  verify unit "regular use does not re-export"
  verify unit "barrel index with pub use re-exports from sub-files"
  verify unit "selective re-export of unknown entity produces W027"
  verify unit "pub use through cycle participant uses only declared set"
}
