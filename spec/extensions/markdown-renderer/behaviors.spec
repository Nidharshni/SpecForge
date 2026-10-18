// @specforge/markdown-renderer extension behaviors
// These behaviors are contributed by the markdown-renderer extension,
// not part of the core compiler.

use "invariants/validation"
use "ports/outbound"
use "types/errors"
use "types/graph"
use "types/output"

defaults behavior {
  invariants [diagnostic_determinism]
  types      [Graph, OutputFile, EmitterError]
  ports      [GraphSerializer, FileSystem]
  features   [markdown_documentation_generation]
}

behavior render_markdown_documentation "Render Markdown Documentation" {
  category query
  contract """
    When specforge render markdown is invoked, the system MUST traverse
    the graph and produce one .md file per entity group. Rendered
    markdown MUST include entity titles, descriptions, cross-reference
    links, and traceability chains. Output MUST be written to the
    specified output directory.
  """
  verify unit "each entity group produces a markdown file"
  verify unit "cross-references render as links"
  verify integration "output directory is created if missing"
}

behavior render_index_files "Render Index Files" {
  category query
  contract """
    The render command MUST auto-generate index files listing all entities
    grouped by kind. Index files MUST NOT be hand-written. They MUST be
    regenerated on every render invocation.
  """
  verify unit "index file lists all entities by kind"
  verify unit "index is regenerated on recompilation"
}

behavior selective_render_by_entity_type "Selective Render by Entity Kind" {
  category query
  contract """
    The render command SHOULD support filtering by entity kind
    (e.g., specforge render markdown --only behaviors). When a filter
    is specified, only matching entities MUST appear in the output.
  """
  verify unit "filter renders only matching entity kinds"
  verify unit "no filter renders all entity kinds"
}
