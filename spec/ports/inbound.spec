// Inbound ports — interfaces the system offers to the outside world

use "types/config"
use "types/core"
use "types/diagnostics"
use "types/errors"
use "types/formatting"
use "types/graph"
use "types/lsp"
use "types/migration"
use "types/output"
use "types/wasm"

port CompilerApi {
  direction inbound
  category  "api/compiler"
  method check(config: CompilerConfig) -> Result<DiagnosticBag, ParseError>
  method parse(path: string) -> Result<SpecFile, ParseError>
  method resolve(files: SpecFile[]) -> Result<Graph, ResolutionError>
  method validate(graph: Graph) -> Result<DiagnosticBag, ValidationError>
  method trace(graph: Graph, entityId: EntityId @optional) -> Result<TraceChain, ResolutionError>
  method render(graph: Graph, format: string, outDir: string) -> Result<OutputFile[], EmitterError>
  method migrate(targetVersion: string @optional, dryRun: boolean @optional) -> Result<MigrationSummary, EmitterError>
  method export(graph: Graph, format: string, scope: EntityId @optional, maxTokens: integer @optional) -> Result<OutputFile, EmitterError>
  method query(graph: Graph, entityId: EntityId, depth: integer, kindFilter: string[] @optional) -> Result<Graph, ResolutionError>
  method format(paths: string[], config: FormatConfig) -> Result<FormatDiff[], EmitterError>
  method watch(config: CompilerConfig) -> Result<void, EmitterError>
  method init(config: InitConfig) -> Result<ProjectConfig, InitError>
  method add(specifiers: ExtensionSpecifier[]) -> Result<ExtensionDeclaration[], RegistryError>
  requires {
    config_valid        "CompilerConfig has a non-empty spec_root and at least one extension"
    export_format_known "export format argument is one of context, graph, brief, json, dot"
  }
  ensures {
    diagnostics_sorted       "DiagnosticBag entries are sorted by file path then line number"
    diagnostics_deduplicated "DiagnosticBag contains no duplicate diagnostic codes for the same span"
    export_schema_conformant "export output conforms to the GraphExport JSON schema for the requested format"
  }
  verify integration "CompilerApi contract is satisfied"
}

port LspProtocol {
  direction inbound
  category  "api/lsp"
  method initialize(config: CompilerConfig) -> Result<void, ExtensionError>
  method didOpen(path: string, content: string) -> Result<DiagnosticBag, ParseError>
  method didChange(path: string, changes: ContentChangeEvent[]) -> Result<DiagnosticBag, ParseError>
  method didClose(path: string) -> Result<void, never>
  method gotoDefinition(path: string, line: integer, col: integer) -> Result<SourceSpan, ResolutionError>
  method findReferences(entityId: EntityId) -> Result<SourceSpan[], never>
  method hover(path: string, line: integer, col: integer) -> Result<HoverContent, never>
  method completion(path: string, line: integer, col: integer) -> Result<CompletionItem[], never>
  method rename(entityId: EntityId, newName: string) -> Result<WorkspaceEditResult, ResolutionError>
  method prepareRename(path: string, line: integer, col: integer) -> Result<SourceSpan, never>
  method codeAction(path: string, line: integer, col: integer) -> Result<CodeAction[], never>
  method documentSymbol(path: string) -> Result<DocumentSymbolEntry[], never>
  method workspaceSymbol(query: string) -> Result<WorkspaceSymbolEntry[], never>
  method semanticTokens(path: string) -> Result<SemanticToken[], never>
  method formatting(path: string, content: string) -> Result<TextEdit[], never>
  method rangeFormatting(path: string, content: string, startLine: integer, endLine: integer) -> Result<TextEdit[], never>
  requires {
    root_uri_valid "initialize rootUri points to a directory containing specforge.json or specforge.spec"
    path_non_empty "didOpen and didChange path arguments are non-empty and resolve within the project root"
  }
  ensures {
    capabilities_published  "after initialize, server publishes supported capabilities to the client"
    diagnostics_on_open     "didOpen triggers publishDiagnostics for the opened document"
    incremental_consistency "didChange diagnostics reflect the cumulative effect of all applied changes"
  }
  verify integration "LspProtocol contract is satisfied"
}

port McpProtocol {
  direction inbound
  category  "api/mcp"
  method initialize(config: CompilerConfig) -> Result<void, ExtensionError>
  method listResources() -> Result<string[], never>
  method readResource(uri: string) -> Result<string, ExtensionError>
  method listTools() -> Result<string[], never>
  method callTool(name: string, arguments: JsonObject) -> Result<string, ExtensionError>
  method subscribe(uri: string) -> Result<void, never>
  method unsubscribe(uri: string) -> Result<void, never>
  method notify(uri: string, data: JsonObject) -> Result<void, never>
  method listPrompts() -> Result<string[], never>
  method getPrompt(name: string, arguments: JsonObject) -> Result<string, ExtensionError>
  method shutdown() -> Result<void, never>
  requires {
    params_valid  "initialize config contains a valid spec_root and protocol version"
    tool_exists   "callTool name matches a tool registered by an installed extension"
    prompt_exists "getPrompt name matches a prompt registered by an installed extension"
  }
  ensures {
    server_info_returned "initialize returns server name, version, and supported protocol capabilities"
    tool_schema_match    "callTool result conforms to the JSON Schema declared by the tool"
    resource_uri_stable  "readResource returns consistent content for the same URI within a compilation"
  }
  verify integration "McpProtocol contract is satisfied"
}
