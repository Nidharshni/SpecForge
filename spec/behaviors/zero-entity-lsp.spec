// Zero-entity core — extension-driven LSP features

use "invariants/lsp"
use "invariants/zero-entity-core"
use "ports/inbound"
use "types/lsp"
use "types/zero-entity-core"

defaults behavior {
  features   [extension_driven_lsp, hover_and_autocomplete]
  invariants [zero_domain_knowledge_core, lsp_response_latency]
  types      [KindRegistryEntry]
  ports      [LspProtocol]
}

// -- Extension-Driven LSP ----------------------------------------------------

behavior complete_extension_defined_keywords "Complete Extension-Defined Keywords" {
  category command
  contract """
    The LSP autocomplete MUST query the KindRegistry for all registered
    entity kinds when completing keywords at the top level of a .spec file.
    Each completion item MUST include the keyword, a snippet template
    for the block body based on the kind's fields, and a detail string
    showing the source extension name.
  """
  requires {
    kind_registry_populated "KindRegistry is populated (registries_populated event has fired)"
  }
  ensures {
    all_registered_keywords_included "Every keyword from the KindRegistry is included in the completion list"
    snippet_templates_provided       "Each completion item includes a snippet template for block scaffolding"
  }
  verify unit "completion includes all registered keywords"
  verify unit "completion items include snippet templates"
  verify unit "completion detail shows source extension"
  verify contract "Complete Extension-Defined Keywords: extension keyword completion holds — kind_registry_populated, all_registered_keywords_included, snippet_templates_provided"
}

behavior provide_extension_entity_semantic_tokens "Provide Extension Entity Semantic Tokens" {
  features [extension_driven_lsp, semantic_tokens]
  category query
  contract """
    The LSP semantic token provider MUST classify an extension entity's
    ID at its declaration site with the semantic_token field of its kind's
    KindRegistry entry. The legend is sent at initialize, before
    extensions load, so it is static: the full list of standard LSP
    semantic token types. When the kind declares no semantic_token, or
    declares one the legend does not contain, the declaration MUST fall
    back to "function". Entity kind keywords are classified as "type".
    Enhanced fields from entity enhancements MUST be classified as
    property. provide_semantic_tokens (behaviors/lsp.spec) states the
    same rule for the whole document.
  """
  requires {
    kind_registry_populated "KindRegistry is populated (registries_populated event has fired)"
  }
  ensures {
    token_type_resolved_from_registry "Token type for each entity ID declaration is resolved from KindRegistryEntry.semantic_token"
    function_fallback                 "A missing or non-legend semantic_token falls back to function"
  }
  verify unit "extension kind's semantic_token classifies its entity ID declaration"
  verify unit "entity ID declaration falls back to 'function' when semantic_token is not specified"
  verify unit "semantic_token outside the static legend falls back to 'function'"
  verify contract "Provide Extension Entity Semantic Tokens: extension semantic tokens holds — kind_registry_populated, token_type_resolved_from_registry, function_fallback"
}

behavior provide_extension_entity_hover "Provide Extension Entity Hover" {
  category query
  types    [KindRegistryEntry, HoverContent]
  contract """
    When hovering over an entity ID, the LSP MUST render the entity's facts
    from the inspect read view, the one specforge.inspect renders: the
    entity kind name, the source extension that defines it, the kind's
    description, the entity's title, its testability (for a testable kind,
    from the same standing inspect reports), the statement its extension
    declares headline and normative as a summary (whole, every line
    quoted), its coverage (status, proven obligations and recorded tests;
    or that it is exempt and why; or that the recorded report cannot be
    read; while the project rebuilds, the last complete compile's coverage),
    the references to it grouped
    by referencing kind and field and the references it makes grouped by
    field, each with its count, every other field value (a long string cut
    at a character boundary), its obligations, and the diagnostics about the entity that the
    diagnostics under the cursor do not already show, each code linked to
    its catalogue entry when it has one. The hover content MUST be
    formatted as markdown and MUST NOT carry editor-specific markup.
    This is the authoritative behavior for extension-aware hover logic;
    hover_information (behaviors/lsp.spec) delegates here.
  """
  requires {
    kind_registry_populated "KindRegistry is populated (registries_populated event has fired)"
  }
  ensures {
    hover_content_from_registry "Hover content is generated from KindRegistryEntry metadata"
    source_extension_shown      "Source extension name is displayed in hover content"
  }
  verify unit "hover shows entity kind and source extension"
  verify unit "hover shows testability for testable kinds"
  verify unit "hover content formatted as markdown"
  verify unit "hover shows the headline statement as its summary"
  verify unit "hover shows reference count from graph"
  verify unit "hover shows the entity's coverage as specforge.inspect reports it"
  verify unit "hover lists the entity's obligations apart from its fields"
  verify unit "hover lists the entity's diagnostics the cursor's do not already show"
  verify unit "a long field value is cut at a character boundary"
  verify unit "hover shows the last complete compile's coverage while the project rebuilds"
  verify contract "Provide Extension Entity Hover: extension entity hover holds — kind_registry_populated, hover_content_from_registry, source_extension_shown"
}

behavior provide_extension_defined_lsp_icons "Provide Extension-Defined LSP Icons" {
  features [extension_driven_lsp, outline_and_symbol_search]
  category query
  contract """
    The LSP document symbols and workspace symbols MUST use the lsp_icon
    field from the KindRegistry entry to determine the SymbolKind. If no
    lsp_icon is specified, the default MUST be SymbolKind::Object.
    Extension-defined icons MUST appear in the outline view and symbol search.
  """
  requires {
    kind_registry_populated "KindRegistry is populated (registries_populated event has fired)"
  }
  ensures {
    symbol_kind_from_registry       "SymbolKind is resolved from KindRegistryEntry.lsp_icon"
    default_object_for_unregistered "Unspecified lsp_icon defaults to SymbolKind::Object"
  }
  verify unit "custom SymbolKind used from manifest lsp_icon"
  verify unit "default SymbolKind::Object when lsp_icon not specified"
  verify unit "extension icons appear in outline view"
  verify contract "Provide Extension-Defined LSP Icons: extension LSP icons holds — kind_registry_populated, symbol_kind_from_registry, default_object_for_unregistered"
}
