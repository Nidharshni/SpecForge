//! Sample sources the surfaces' tests share, each using the kinds of the builtins it names.

/// The builtins whose kinds the samples use.
pub const BUILTINS: [&str; 3] = [
    "@specforge/software",
    "@specforge/product",
    "@specforge/governance",
];

/// 5 software entity kinds: behavior, invariant, event, type, port
pub const SOFTWARE: &str = r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    status done
}

behavior emit_output "Emit Output" {
    contract "The system MUST emit structured output"
}

invariant graph_acyclic "Graph Acyclicity" {
    guarantee "The dependency graph MUST be acyclic"
}

event input_parsed "Input Parsed" {
    trigger parse_input
}

type parse_result "Parse Result" {
    description "Result of a parse operation"
}

port file_reader "File Reader" {
    direction inbound
}
"#;

/// 6 product entity kinds: feature, journey, deliverable, milestone, module, term
pub const PRODUCT: &str = r#"
feature fast_parsing "Fast Parsing" {
    problem "Users need quick feedback"
    solution "Incremental parsing"
    behaviors [parse_input]
}

journey developer_workflow "Developer Workflow" {
    description "A developer writes and validates specs"
}

deliverable cli_tool "CLI Tool" {
    description "The specforge CLI binary"
}

milestone v1_release "V1 Release" {
    status planned
    description "First stable release"
}

module parser_module "Parser Module" {
    description "Handles .spec file parsing"
}

term spec_file "Spec File" {
    definition "A .spec file containing entity declarations"
}
"#;

/// 3 governance entity kinds: decision, constraint, failure_mode
pub const GOVERNANCE: &str = r#"
decision use_treesitter "Use Tree-Sitter" {
    status accepted
    context "Need a robust parser"
    decision_text "We will use tree-sitter for parsing"
    consequences "Fast parsing, good error recovery"
}

constraint sub_second_check "Sub-Second Check" {
    category performance
    priority high
    description "Check must complete in under 1 second for 100 files"
}

failure_mode parser_crash "Parser Crash" {
    severity high
    occurrence unlikely
    detection likely
    cause "Malformed input"
    effect "CLI exits with panic"
    mitigations [parse_input]
}
"#;

/// All 14 entity kinds combined with cross-references
pub const MULTI_EXTENSION: &str = r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    status done
}

behavior emit_output "Emit Output" {
    contract "The system MUST emit structured output"
}

invariant graph_acyclic "Graph Acyclicity" {
    guarantee "The dependency graph MUST be acyclic"
}

event input_parsed "Input Parsed" {
    trigger parse_input
}

type parse_result "Parse Result" {
    description "Result of a parse operation"
}

port file_reader "File Reader" {
    direction inbound
}

feature fast_parsing "Fast Parsing" {
    problem "Users need quick feedback"
    solution "Incremental parsing"
    behaviors [parse_input]
}

journey developer_workflow "Developer Workflow" {
    description "A developer writes and validates specs"
}

deliverable cli_tool "CLI Tool" {
    description "The specforge CLI binary"
}

milestone v1_release "V1 Release" {
    status planned
    description "First stable release"
}

module parser_module "Parser Module" {
    description "Handles .spec file parsing"
}

term spec_file "Spec File" {
    definition "A .spec file containing entity declarations"
}

decision use_treesitter "Use Tree-Sitter" {
    status accepted
    context "Need a robust parser"
    decision_text "We will use tree-sitter for parsing"
    consequences "Fast parsing, good error recovery"
}

constraint sub_second_check "Sub-Second Check" {
    category performance
    priority high
    description "Check must complete in under 1 second for 100 files"
}

failure_mode parser_crash "Parser Crash" {
    severity high
    occurrence unlikely
    detection likely
    cause "Malformed input"
    effect "CLI exits with panic"
    mitigations [parse_input]
}
"#;

/// 4-entity linear chain: feature → behavior → invariant → type (via ref lists)
pub const DEEP_CHAIN: &str = r#"
feature feat_root "Feature Root" {
    problem "needs parsing"
    solution "parse it"
    behaviors [beh_middle]
}

behavior beh_middle "Behavior Middle" {
    contract "The system MUST validate"
    features [feat_root]
}

invariant inv_deep "Invariant Deep" {
    guarantee "The graph MUST be acyclic"
    enforced_by [beh_middle]
}

type typ_leaf "Type Leaf" {
    description "A leaf type"
}
"#;

/// Graph with a cycle: a→b→c→a via reference-list fields
pub const CYCLE: &str = r#"
behavior cycle_a "Cycle A" {
    contract "first in cycle"
    features [cycle_c]
}

feature cycle_b "Cycle B" {
    problem "second"
    solution "cycle"
    behaviors [cycle_a]
}

feature cycle_c "Cycle C" {
    problem "third"
    solution "cycle"
    behaviors [cycle_a]
}
"#;

/// Single entity with no references (isolated node)
pub const ISOLATED: &str = r#"
behavior isolated_node "Isolated Node" {
    contract "The system MUST be isolated"
}
"#;

/// Entities with cross-kind references for testing resolution
pub const CROSS_REF: &str = r#"
behavior validate_graph "Validate Graph" {
    contract "The system MUST validate the graph"
    features [graph_validation]
}

behavior resolve_refs "Resolve References" {
    contract "The system MUST resolve cross-entity references"
    features [graph_validation]
}

feature graph_validation "Graph Validation" {
    problem "Graph must be valid before export"
    solution "Multi-pass validation"
    behaviors [validate_graph, resolve_refs]
}

invariant refs_resolved "References Resolved" {
    guarantee "All references MUST resolve to existing entities"
    enforced_by [validate_graph]
}

event validation_complete "Validation Complete" {
    trigger validate_graph
}

failure_mode unresolved_ref "Unresolved Reference" {
    severity medium
    occurrence occasional
    detection moderate
    cause "Typo in entity ID"
    effect "Broken graph edges"
    mitigations [resolve_refs]
}
"#;

/// software + product loaded: `pay` implements a feature and enforces an invariant; `refund` is linked to nothing
/// (what a trace reports as missing links).
pub const TRACE_GAPS: &str = r#"
feature checkout "Checkout" {
    problem "Buyers need to pay"
    solution "A payment step"
}

behavior pay "Pay" {
    contract "The system MUST take payment once"
    features [checkout]
    invariants [paid_once]
}

behavior refund "Refund" {
    contract "The system MUST return the payment"
}

invariant paid_once "Paid Once" {
    guarantee "An order MUST be paid at most once"
}
"#;
