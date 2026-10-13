// Shared surface conventions — error handling, format conventions, and cross-cutting surface behaviors
//
// This file specifies cross-cutting contracts that apply to ALL product
// commands (each also the auto-promoted MCP tool specforge.product.<id>).
// See surfaces-cli.spec for the commands. The extension declares no MCP
// resources (ADR 0011).

use "extensions/product/behaviors-operations"
use "extensions/product/behaviors-queries"
use "extensions/product/behaviors-registration"
use "extensions/product/features"
use "extensions/product/types"

// ════════════════════════════════════════════════════════════════
// Surface Error Handling — common error contracts
// ════════════════════════════════════════════════════════════════

behavior surface_error_handling "Surface Error Handling" {
  category command
  types    [ProductSurfaceError, ProductQueryError]
  contract """
    All product commands MUST follow consistent error handling. A command
    that cannot answer writes one error to stderr and nothing to stdout:
    1. Entity-not-found: code="ENTITY_NOT_FOUND", a message naming the
       entity kind and ID, and a suggestion when an ID of the same kind is
       within Levenshtein distance 2. Exit code 1.
    2. Invalid-input: code="INVALID_INPUT", a message describing the
       validation failure (a value outside a closed filter's enum, a
       missing required entity id, a negative offset, an unknown sort
       field, a malformed date). Exit code 2, the code the host gives the
       usage errors it catches itself: a closed filter and --sort-order
       are enum args, so on the command line the host refuses a value
       outside them first, as it does a missing positional id.
    3. Under --format json the error is the ProductSurfaceError object
       {code, message, entity_id?, suggestion?}; under --format human it
       is the line "error: <message>", then "did you mean '<id>'?" when
       there is a suggestion.
    4. Over MCP (which always asks for json) a non-zero exit is an isError
       tool result carrying the same object.
    A command only ever runs over a built graph, so there is no
    graph-not-ready error.
  """
  ensures {
    entity_not_found_code "entity-not-found errors use code ENTITY_NOT_FOUND and exit 1"
    invalid_input_code    "invalid-input errors use code INVALID_INPUT and exit 2"
    suggestion_on_typo    "ENTITY_NOT_FOUND includes suggestion when Levenshtein distance <= 2 match exists"
    cli_stderr            "error messages are written to stderr, not stdout"
    json_error_object     "under --format json the error is a ProductSurfaceError JSON object"
    human_error_line      "under --format human the error is one error: line, with the suggestion when there is one"
    mcp_tool_is_error     "over MCP an error is an isError tool result carrying the same object"
    no_panic              "no command panics on any input"
  }
  features [pe_surface_contributions]
  verify unit "entity-not-found returns ENTITY_NOT_FOUND code"
  verify unit "invalid filter value returns INVALID_INPUT code"
  verify unit "CLI errors go to stderr"
  verify unit "MCP tool errors are isError results carrying the error object"
  verify unit "fuzzy-match suggestion present when close match exists"
  verify unit "no surface panics on null, empty, or malformed input"
}

// ════════════════════════════════════════════════════════════════
// Surface Format Conventions — shared output formatting
// ════════════════════════════════════════════════════════════════

behavior surface_format_conventions "Surface Format Conventions" {
  category command
  types    [ProductListFilter]
  contract """
    Every product command takes the host's --format flag, not one it
    declares: human (the CLI default) or json. The host passes the value
    in CommandInput.format; over MCP it always passes json and has no
    format argument. The extension renders both:
    - json: one root object on stdout, the payload type the command's
      behavior names.
    - human: the extension's layout for people; a tabular payload is a
      table with a header row and fixed-width columns.
    There are no other formats (ADR 0011).
  """
  ensures {
    human_default   "--format defaults to human on the CLI"
    json_valid      "json format produces one valid JSON object, the command's payload type"
    human_table     "human format of a tabular payload has a header row and aligned columns"
    mcp_always_json "MCP tools always run with format json"
    utf8_output     "all output is valid UTF-8"
  }
  features [pe_surface_contributions]
  verify unit "default format is human"
  verify unit "json output is valid JSON"
  verify unit "human table output has header and aligned columns"
  verify unit "an MCP tool call returns the json payload"
}

// ════════════════════════════════════════════════════════════════
// Surface List Command Contract — shared contract for all 9 list commands
// ════════════════════════════════════════════════════════════════

behavior surface_list_command_contract "Surface List Command Shared Contract" {
  category command
  types    [ProductListFilter, ProductListResult, PaginationMetadata, ProductSurfaceError]
  contract """
    All 9 product list commands (specforge product features, journeys,
    deliverables, milestones, modules, terms, personas, channels,
    releases) MUST follow a uniform contract:

    Input: ProductListFilter with optional --status, --priority, --tags
    (a comma-separated string arg), --limit (default 100, clamped to
    [1, 1000]), --offset (default 0), --sort-by (default "id") and
    --sort-order (default "asc"), plus the host's --format.

    Output: A typed *ListResult (e.g., FeatureListResult, JourneyListResult)
    containing the entries under the kind's plural, total, offset, limit,
    has_more.

    Behavior:
    1. Filter phase: apply --status, --priority, --tags filters (AND logic).
       Invalid enum values produce INVALID_INPUT error. Empty filters match all.
    2. Sort phase: sort by --sort-by field (must exist on entity kind, else
       INVALID_INPUT). Tie-break by entity ID ascending for determinism.
    3. Paginate phase: apply --offset and --limit. Clamp --limit to [1, 1000].
       --offset beyond total returns empty entries with correct total.
    4. Serialize phase: apply --format (human|json) per
       surface_format_conventions.

    The same offset/limit pagination (and only it: there are no cursors)
    applies to the project-wide matrix queries: coverage-matrix,
    channel-coverage-matrix, feature-overlap, owner-workload and
    module-coupling page their per-entity entries the same way.

    Each list command delegates to the same query pipeline — only the entity
    kind and result type differ. Wasm export: cmd__product_{kind}s (plural).
    MCP tool auto-promotion: specforge.product.{kind}s.
  """
  ensures {
    filter_and_logic            "multiple filters combine with AND logic"
    invalid_filter_error        "invalid enum filter value returns INVALID_INPUT"
    sort_deterministic          "tie-break sort by entity ID ascending"
    sort_field_validated        "invalid sort_by field returns INVALID_INPUT"
    limit_clamped               "limit clamped to [1, 1000] range"
    offset_beyond_total         "offset beyond total returns empty list with correct total"
    pagination_correct          "total reflects filtered count; has_more == (offset + entities.length < total)"
    empty_graph_ok              "empty graph returns no entries, total=0, has_more=false"
    delegates_to_query          "each list command delegates to a common query pipeline"
    filters_read_the_vocabulary "a closed filter takes exactly its field's declared vocabulary, and an entity without the field counts as the vocabulary's absent value"
  }
  features [pe_surface_contributions]
  verify unit "filter by status returns only matching entities"
  verify unit "filter by priority returns only matching entities"
  verify unit "combined status+priority filter uses AND logic"
  verify unit "invalid status filter returns INVALID_INPUT"
  verify unit "sort by priority with tie-break by ID is deterministic"
  verify unit "invalid sort_by field returns INVALID_INPUT"
  verify unit "limit=0 is clamped to 1"
  verify unit "limit=5000 is clamped to 1000"
  verify unit "offset beyond total returns empty list"
  verify unit "empty graph returns total=0 and has_more=false"
  verify property "for all list commands: entities.length <= limit"
  verify property "for all list commands: has_more == (offset + entities.length < total)"
}

// ════════════════════════════════════════════════════════════════
// Surface Query Command Contract — shared contract for all query commands
// ════════════════════════════════════════════════════════════════

behavior surface_query_command_contract "Surface Query Command Shared Contract" {
  category command
  types    [ProductSurfaceError]
  contract """
    All 31 product query commands (40 commands less the 9 lists) MUST
    follow a uniform contract:

    Entity-scoped queries (e.g., specforge product milestone-completion,
    feature-impact):
    1. Take the entity ID as a required positional arg named after its
       kind (milestone, journey, feature, ...); the MCP tool's argument
       has the same name.
    2. Validate entity exists and is the correct kind. Return ENTITY_NOT_FOUND
       with fuzzy-match suggestion if not found.
    3. Delegate to the corresponding ProductQueryPort method.
    4. Return the typed payload (e.g., MilestoneCompletionPayload).

    Project-wide queries (e.g., specforge product critical-path):
    1. Take no entity ID.
    2. Delegate to the corresponding ProductQueryPort method.
    3. Return the typed payload.

    All query commands:
    - Support the host's --format (human|json) per surface_format_conventions.
    - Are auto-promoted to MCP tools with the same input schema.
  """
  ensures {
    entity_validated  "entity-scoped queries validate entity existence and kind"
    entity_arg_named  "the entity arg is positional and named after its kind"
    fuzzy_suggestion  "ENTITY_NOT_FOUND includes Levenshtein distance <= 2 suggestion"
    delegates_to_port "each command delegates to a ProductQueryPort method"
    format_respected  "output respects --format flag"
  }
  features [pe_surface_contributions]
  verify unit "entity-scoped query with valid ID returns typed payload"
  verify unit "entity-scoped query with invalid ID returns ENTITY_NOT_FOUND"
  verify unit "entity-scoped query with close typo returns suggestion"
  verify unit "project-wide query returns typed payload"
  verify unit "query result respects --format=json"
}
