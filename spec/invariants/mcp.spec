// MCP-specific invariants — guarantees for MCP server protocol interactions

use "types/mcp"

invariant mcp_structured_error_responses "MCP Structured Error Responses" {
  guarantee """
    All MCP tools, resources and prompts MUST return structured error objects
    (not plain strings) with error code, message, and optional entity_id. This
    ensures agents can programmatically handle errors without parsing
    free-form text. A failed tool call is an isError result whose content is
    an McpError; a diagnostic code behind the failure is in its diagnostic,
    not only in the message. A failed prompts/get or resources/read, which
    have no isError result, is a JSON-RPC error whose data is its McpError:
    -32602 when the client can fix it (an argument it named, invalid input,
    an unknown entity or extension, a conflict), -32603 otherwise. A resource
    that does not exist (no entry serves the URI, or the entity it names is
    not in the graph) is -32002 in a handshake session and -32602 in a
    2026-07-28 request, and its data names the URI as uri. A refusal for a
    missing project names the path argument only where the entry takes one.
    An operation's failure kind decides the McpError code. An operation
    states the kind where it raises the failure; no table maps a code to a
    kind, and a package the registry or its trust check refuses is never
    internal_error. An operation states which field of its request a
    refusal is about; a refusal of the project or the environment is about
    none.
  """
  risk      medium
  verify unit "error response includes error code and message fields"
  verify unit "error response includes entity_id when applicable"
  verify unit "no MCP endpoint returns a plain string error"
  verify unit "a package the registry or the trust check refuses is never an internal_error"
  verify unit "success responses never have error field"
  verify unit "a failed tool call is an isError result whose content is an McpError with a code"
  verify unit "a diagnostic code behind a failed tool call is in its McpError diagnostic"
  verify unit "a missing entity is one E003 refusal naming the closest entity on every read view"
  verify unit "a failed prompts/get carries its McpError as the error's data"
  verify unit "a failed resources/read carries its McpError as the error's data"
  verify unit "a no-project refusal names path only for an entry that takes one"
  verify unit "a path that does not exist is a file_not_found error on path"
  verify unit "an operation's failure kind decides its McpError code, never its code text"
  verify unit "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
}

invariant mcp_subscription_cleanup "MCP Subscription Cleanup" {
  guarantee """
    When an MCP client disconnects, all its subscriptions and listen streams
    MUST be removed, each removal recorded. Subscriptions belong to the
    connection: no request parameter names another client. No orphan
    subscriptions may remain after client disconnect. This prevents resource
    leaks and ensures notification delivery targets only active clients.
  """
  risk      high
  verify unit "client disconnect removes all subscriptions for that client"
  verify unit "no orphan subscriptions remain after disconnect"
  verify integration "rapid connect/disconnect cycles leave zero subscriptions"
}

invariant mcp_tool_idempotency "MCP Tool Idempotency" {
  guarantee """
    Read-only MCP tools (core, navigation, and read-only project management:
    extensions, providers, doctor) and all MCP
    prompts MUST be idempotent: repeated calls with the same parameters MUST
    return the same result if the graph has not changed between calls. This
    guarantees agents can safely retry read operations and prompt invocations.
  """
  risk      medium
  verify property "repeated calls with same params return identical results when graph unchanged"
  verify unit "read-only tools return equivalent results for identical inputs"
}

invariant mcp_served_project_consistency "MCP Served Project Consistency" {
  guarantee """
    The project an MCP server serves is replaced whole or not at all, and
    every request that reads it (tools/call, resources/read, prompts/get and
    the list methods) first brings it up to date with the files on disk
    (bring_session_up_to_date):
    an update for changed sources, an environment reload with its extension
    tools and resources for a changed specforge.json, specforge.lock or
    extension module. Subscribed clients learn what changed. A call whose
    path names another project, one nested in the served project, acts on
    that project only, opened for the call and brought up to date after it
    writes as the served project is, and the server keeps serving its own
    without reloading it. A path an MCP request names resolves inside the
    served project (while none is served, inside the server's root): a
    relative path is read against that directory, never the process's, and
    a path that leaves it, by .., by an absolute name or by a symlink, is
    refused as invalid_input on its argument, whatever tool, prompt or
    resource takes it and in either revision. A file or directory a tool
    argument names (format's paths, render's out_dir, outline's file)
    resolves inside the root of the project the call acts on, and no
    mutation reads or writes through a symlink that leaves it. init creates
    its project inside the server's root and never inside the served
    project. The CLI is not bounded: its user is already at the file system.
    A call
    that names a tool, a resource or a prompt looks it up in the project as
    it is on disk: an extension enabled since the last request is found by
    the next one, whether the request is a call, a read, a listing or a
    subscription (one freshness decision, owned by the request pipeline). Only a
    project (a directory holding specforge.json) is ever
    served: the server serves the nearest one at or above its root (the
    client's projectRoot, else the path it was started with), and nothing
    when there is none; while none is served, every request that reads
    what is served looks there again first, so a project created there
    since is served by the next such request. In a handshake session, a call whose path names a project
    while none is served serves that project, and init serves the project
    it creates; a stateless request (MCP 2026-07-28) never changes what is
    served: its path is read for that call only and init serves nothing. A
    path to a directory that is no project is read for that call only and
    never served.
    With no project served and none named, what a call gets is declared by
    the entry's target: an entry that reads only the project view (the read
    views, export and render, the navigation reads of the graph, every core
    prompt and resource) answers over the empty session, and one of its
    reads that names a file or an entity is the no-project refusal
    (precondition_failed), never not-found; an entry that acts on the
    project on disk (a tool that takes a path, a management tool, a read of
    the inference or anchors manifest, every extension tool and resource)
    is refused as no project before its arguments are read.
  """
  risk      high
  verify unit "an environment change on disk updates the extension tools listed"
  verify unit "a tool call serves files written since the last call, without watch"
  verify unit "a resource read serves files written since the last call, without watch"
  verify unit "a prompt reads the project as it is on disk"
  verify unit "an extension tool enabled on disk since the last request is callable by name"
  verify unit "an extension enabled on disk since the last request is listed by the next listing of every kind"
  verify unit "a subscription to an extension resource enabled on disk since the last request is accepted"
  verify unit "a mutation on another project does not reload the served one"
  verify unit "a mutation on another project brings it up to date with what it wrote and reports what a fresh compile reports"
  verify unit "in a handshake session, a path while no project is served serves that project, for every tool that takes a path"
  verify unit "a path inside the served project names the served project"
  verify unit "a path an MCP request names resolves inside the served project, whatever its spelling"
  verify unit "a file or directory an MCP tool argument names resolves inside the call's project"
  verify unit "a mutation reads and writes no file through a symlink that leaves the project"
  verify unit "init creates its project inside the server's root and never inside the served project"
  verify unit "a server whose root lies in no project serves nothing, and a handshake init then serves the project it creates"
  verify unit "a server started inside a project serves that project"
  verify unit "while nothing is served, a project created at or above the server's root is served by the next request, in either revision"
  verify unit "a request that reads nothing of the project, such as server/discover, serves nothing; the next request that reads it serves the project created at the root"
  verify unit "a path to a directory that is no project is read for that call and never served"
  verify unit "rename with a path to another project edits that project only and keeps serving this one"
  verify unit "remove_extension with a path to another project checks that project's dependents and entities"
  verify unit "analyze notifies subscribers when the diagnostics it compiled changed"
  verify unit "validate with a path to another project leaves the served project in place"
  verify unit "a mutation tool that wrote files leaves the server serving what is on disk"
  verify unit "initialize naming another project replaces the project a stateless request served"
  verify unit "with no project served, a read naming a file or an entity is the no-project refusal, an aggregate read answers over the empty session"
  verify unit "with no project served, every core tool, prompt and resource answers or refuses as its target declares"
  verify unit "with no project served, an entry that acts on the project is refused before its arguments are read"
}

invariant mcp_event_record "MCP Event Record" {
  guarantee """
    What an MCP server records it did (the events of spec/events/mcp.spec and
    spec/events/surface-contributions.spec, and the extension_added and
    project_initialized a mutation produces) is one typed value per event,
    its payload the spec's field for field, recorded through the server's
    one event sink. Events never leave the process: a server keeps none of
    them, so what it holds does not grow with the requests it answered; a
    test gives the server a sink that keeps them, to observe it. A sink that
    keeps an event stamps it with the time it was recorded (timestamp).
  """
  risk      medium
  verify unit "a server keeps no event: what it holds does not grow with the requests it answers"
  verify unit "every event the server records is one an events spec declares, its payload exactly the declared fields"
  verify unit "every event a recording sink keeps carries the time it was recorded"
}
