// Surface contribution invariants — uniqueness and sandbox guarantees

invariant surface_contribution_uniqueness "Surface Contribution Uniqueness" {
  guarantee """
    No two extensions MAY contribute the same name within a surface type.
    No two commands MAY route under one name (`specforge <short> <cli
    name>`); a later one is E039 at load. MCP tool names
    MUST be unique across all extensions. MCP resource URI templates MUST
    be unique across all extensions. Duplicate contributions MUST produce
    E039 at extension load time.
  """
  risk      medium
  verify property "no two commands route under one CLI name"
  verify property "no two extensions can register the same MCP tool name"
  verify unit "duplicate surface contribution produces E039"
}

invariant surface_sandbox_ceiling "Surface Sandbox Ceiling" {
  guarantee """
    Every surface export (a cmd__ command, an mcp__ tool or resource)
    MUST run with no capability: no preopened directory, environment,
    arguments, inherited stdio or network. That is the ceiling: no
    declaration grants an export more. MCP resources MUST NOT write
    files. A surface's sandbox key, which a guest built before ADR 0037
    may still declare, grants nothing and is W153 at load.
  """
  risk      high
  verify unit "a surface export whose sandbox override asks for every capability is granted none"
  verify unit "a surface's sandbox override grants nothing and is W153 at load"
}
