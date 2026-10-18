// Host function ABI contracts for Wasm extensions
//
// Design note: Host functions are synchronous RPC-style ABI calls within a
// single compilation step, not asynchronous pipeline stages. They do NOT
// produce events themselves — traceability comes from the calling behavior's
// events, not from the host function invocation. This is an intentional architectural
// decision: host functions are leaf operations in the call chain, not
// pipeline stages that trigger downstream consumers.

use "invariants/wasm"
use "invariants/zero-entity-core"
use "ports/outbound"
use "types/wasm"
use "types/zero-entity-core"

defaults behavior {
  features   [wasm_host_function_api]
  invariants [wasm_sandbox_integrity, host_function_type_safety, extension_isolation]
  types      [HostFunctionBinding]
  ports      [WasmRuntime]
}

behavior provide_host_function_query_graph "Provide Host Function: query_graph" {
  invariants [wasm_sandbox_integrity, host_function_type_safety]
  category   query
  requires {
    graph_built "compiled graph is available for querying"
  }
  ensures {
    valid_json_returned "query_graph returns valid JSON graph string"
  }
  contract   """
    The specforge.query_graph host function MUST expose the compiled
    graph as a JSON string to the calling extension, with all its
    entities, edges and metadata.
  """
  verify unit "query_graph returns valid JSON graph"
  verify unit "graph includes entities and edges"
  verify contract "Provide Host Function: query_graph: query_graph host function holds — graph_built, valid_json_returned"
}

behavior provide_host_function_emit_diagnostic "Provide Host Function: emit_diagnostic" {
  invariants [host_function_type_safety]
  category   query
  requires {
    diagnostic_collection_available "compiler's diagnostic collection is available for appending"
  }
  ensures {
    diagnostic_added        "diagnostic is added to the compiler's diagnostic collection"
    rendered_like_core      "extension diagnostics are rendered identically to core diagnostics"
    malformed_input_trapped "malformed diagnostic input produces Wasm trap"
  }
  contract   """
    The specforge.emit_diagnostic host function MUST accept a diagnostic
    object with severity, code, message, and optional source span. The
    diagnostic MUST be added to the compiler's diagnostic collection and
    rendered like core diagnostics.
  """
  verify unit "emit_diagnostic adds to compiler diagnostic collection"
  verify unit "extension diagnostics rendered like core diagnostics"
  verify unit "malformed diagnostic input produces Wasm trap"
  verify unit "optional source span omitted without error"
  verify unit "diagnostic severity validated against allowed values"
  verify contract "Provide Host Function: emit_diagnostic: emit_diagnostic host function holds — diagnostic_collection_available, diagnostic_added, rendered_like_core, malformed_input_trapped"
}

// NOTE: Entity kinds and edge types are registered DECLARATIVELY from
// extension declarations by the registry build (see registry_build_kinds
// and registry_build_edges in behaviors/zero-entity-registries.spec).
// The specforge.add_graph_node and specforge.add_graph_edge host functions
// allow extensions to add graph node and edge INSTANCES at runtime during
// contribution exports. These do NOT register new entity kinds or edge types —
// they add instances of already-declared kinds to the graph.

behavior provide_host_function_add_graph_node "Provide Host Function: add_graph_node" {
  invariants [host_function_type_safety, zero_domain_knowledge_core]
  category   query
  types      [HostFunctionBinding, ExtensionDeclaration]
  requires {
    entity_kind_declared "entity kind for the node is declared in a loaded extension manifest"
    graph_available      "mutable graph is available for adding node instances"
  }
  ensures {
    node_added               "graph node instance is added for declared entity kind with validated field values"
    undeclared_kind_rejected "nodes with undeclared entity kinds are rejected"
    node_participates        "added nodes participate in resolution and validation like parser-produced nodes"
  }
  contract   """
    The specforge.add_graph_node host function MUST accept a graph node
    instance with an entity kind, ID, and field values. The entity kind
    MUST already be declared in the extension's manifest — this function
    adds node instances to the graph, NOT new entity kinds. The runtime
    MUST reject nodes whose entity kind is not declared by any loaded
    extension manifest. Field values MUST be validated against the kind's
    declared schema. Added nodes MUST participate in resolution and
    validation like parser-produced nodes.
  """
  verify unit "adds graph node instance for declared entity kind"
  verify unit "rejects node for undeclared entity kind"
  verify unit "validates field values against kind schema"
  verify contract "Provide Host Function: add_graph_node: add_graph_node host function holds — entity_kind_declared, graph_available, node_added, undeclared_kind_rejected, node_participates"
}

behavior provide_host_function_add_graph_edge "Provide Host Function: add_graph_edge" {
  invariants [host_function_type_safety, zero_domain_knowledge_core]
  category   query
  requires {
    edge_type_declared      "edge label corresponds to an edge type declared in a loaded extension manifest"
    source_and_target_exist "both source and target nodes exist in the graph"
  }
  ensures {
    edge_added                "graph edge instance is added for declared edge type"
    undeclared_label_rejected "edges with undeclared labels are rejected"
    missing_nodes_rejected    "edges with missing source or target nodes are rejected"
    edge_participates         "added edges participate in graph queries and validation like parser-produced edges"
  }
  contract   """
    The specforge.add_graph_edge host function MUST accept a graph edge
    instance with a label, source node ID, and target node ID. The edge
    label MUST correspond to an edge type already declared in a loaded
    extension manifest — this function adds edge instances to the graph,
    NOT new edge types. The runtime MUST reject edges whose label is not
    declared by any loaded extension manifest. Both source and target
    nodes MUST exist in the graph. Added edges MUST participate in
    graph queries and validation like parser-produced edges.
  """
  verify unit "adds graph edge instance for declared edge type"
  verify unit "rejects edge for undeclared edge label"
  verify unit "rejects edge when source or target node missing"
  verify contract "Provide Host Function: add_graph_edge: add_graph_edge host function holds — edge_type_declared, source_and_target_exist, edge_added, undeclared_label_rejected, missing_nodes_rejected, edge_participates"
}

behavior provide_host_function_read_file "Provide Host Function: read_file" {
  category query
  types    [HostFunctionBinding, SandboxPolicy]
  ports    [WasmRuntime, FileSystem]
  requires {
    parser_call_site "call originates from a parser contribution export (not validator, renderer, or provider)"
    spec_root_known  "project's spec root path is known for path scoping enforcement"
  }
  ensures {
    path_scoped         "resolved path is under the project's spec root; paths escaping via .. are rejected"
    pattern_restricted  "file path matches one of the calling extension's declared parser file patterns"
    size_limited        "files exceeding max_read_file_size (default 1MB) are rejected, not truncated"
    non_parser_rejected "calls from non-parser contribution exports return permission error"
  }
  contract """
    The specforge.read_file host function MUST accept a file path and return
    the file content as a string to the calling extension. The host MUST
    enforce all of the following constraints:

    1. Path scoping: the resolved path MUST be under the project's spec root.
       Paths containing ".." that escape the spec root MUST be rejected.
    2. Pattern restriction: the file path MUST match one of the calling
       extension's declared parser file patterns (e.g., *.feature, *.proto).
       Files not matching any declared pattern MUST be rejected.
    3. Read-only: no write, rename, or delete operations are exposed.
    4. Size limit: files exceeding SandboxPolicy.max_read_file_size
       (default 1MB) MUST be rejected with a diagnostic, not silently
       truncated.

    The specforge.read_file host function MUST only be callable from parser
    contribution exports. Calls from validators, renderers, providers, or
    entity contributions MUST return a permission error.
  """
  verify unit "reads file under spec root successfully"
  verify unit "rejects path escaping spec root via .."
  verify unit "rejects file not matching declared parser patterns"
  verify unit "rejects file exceeding max_read_file_size"
  verify unit "read_file from validator contribution returns permission error"
  verify unit "read_file from renderer contribution returns permission error"
  verify contract "Provide Host Function: read_file: read_file host function holds — parser_call_site, spec_root_known, path_scoped, pattern_restricted, size_limited, non_parser_rejected"
}

behavior provide_host_function_emit_file "Provide Host Function: emit_file" {
  category query
  types    [HostFunctionBinding, SandboxPolicy]
  ports    [WasmRuntime, FileSystem]
  requires {
    output_directory_known "project output directory is known for path validation"
    sandbox_policy_ready   "sandbox policy with allowed_output_extensions is computed for the extension"
  }
  ensures {
    path_scoped_to_output        "file path is validated to be within project output directory"
    extension_allowlist_enforced "only files with allowed extensions (.json, .html, .csv, .svg, .dot, .xml, .txt, .pdf) are written"
    no_code_generation           "code file extensions (.rs, .py, .js, .ts, .go, etc.) are always rejected"
  }
  contract """
    The specforge.emit_file host function MUST accept a file path and
    content from the extension. Valid files MUST be written to disk
    relative to the project output directory. The host MUST reject
    paths outside the output directory. Extensions MUST only use emit_file
    for non-code outputs (reports, traceability matrices, dashboards,
    graph visualizations).

    WHITELIST MODEL: The runtime MUST enforce a strict allowlist for
    output file extensions. The default allowed_output_extensions is:
    [.json, .html, .csv, .svg, .dot, .xml, .txt, .pdf]. All other
    extensions MUST be rejected unless explicitly added to
    SandboxPolicy.allowed_output_extensions in specforge.json or the
    extension manifest. This is a whitelist — unlisted extensions are
    denied by default. Note: .yaml/.yml are NOT in the defaults —
    extensions that need YAML output MUST explicitly opt in via their
    manifest or project config.

    Extensions MUST NOT use emit_file to generate source code,
    configuration files, or executable artifacts — SpecForge provides
    context, agents produce output.
  """
  verify unit "valid file path within output directory is written"
  verify unit "file path outside output directory is rejected"
  verify unit "file with non-allowed extension is rejected"
  verify unit "file with .json extension is accepted"
  verify unit "file with .html extension is accepted"
  verify unit "emit_file rejects .rs extension"
  verify unit "emit_file rejects .py extension"
  verify unit "emit_file rejects .go extension"
  verify unit "emit_file rejects .js extension"
  verify unit "emit_file rejects .ts extension"
  verify unit "emit_file rejects .sh extension"
  verify contract "Provide Host Function: emit_file: emit_file host function holds — output_directory_known, sandbox_policy_ready, path_scoped_to_output, extension_allowlist_enforced, no_code_generation"
}

behavior provide_host_function_http_get "Provide Host Function: http_get" {
  category query
  types    [HostFunctionBinding, SandboxPolicy]
  requires {
    provider_call_site   "call originates from a provider contribution export (not validator, renderer, or entity)"
    sandbox_policy_ready "sandbox policy with allowed_domains and timeout configuration is computed"
  }
  ensures {
    domain_allowlist_enforced "requests to disallowed domains are rejected"
    timeout_enforced          "per-request timeout (default 5s) and total budget (15s) are enforced"
    timeout_produces_warning  "timeout failures produce W-level warning, not hard error"
    non_provider_rejected     "calls from non-provider contribution exports return permission error"
  }
  contract """
    The specforge.http_get host function MUST fetch a URL and return
    the response body to the calling extension. The URL MUST be validated
    against the sandbox policy's allowed domains. Requests to disallowed
    domains MUST be rejected. Timeouts MUST be enforced. The default
    per-request timeout MUST be 5 seconds. The total time budget for all
    http_get calls within a single compilation MUST NOT exceed 15 seconds.
    Timeout failures MUST produce a W-level warning diagnostic, not a hard
    error, to preserve the seconds-to-value guarantee (P8). The timeout is
    configurable via SandboxPolicy. The specforge.http_get host function
    MUST only be callable from providers contribution exports. Calls from
    validators, renderers, or entities contributions MUST return a
    permission error.
  """
  verify unit "allowed domain returns response body"
  verify unit "disallowed domain is rejected"
  verify unit "timeout is enforced"
  verify unit "timeout defaults to 5 seconds when not configured"
  verify unit "timeout failure produces warning, not error"
  verify unit "http_get from validator contribution returns permission error"
  verify contract "Provide Host Function: http_get: http_get host function holds — provider_call_site, sandbox_policy_ready, domain_allowlist_enforced, timeout_enforced, timeout_produces_warning, non_provider_rejected"
}
