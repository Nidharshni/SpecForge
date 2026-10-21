use serde::Serialize;
use specforge_common::shape::Shape;

use specforge_graph::{Graph, Node};

use crate::error::EmitterError;

/// The `token_budget` block of an export that did not fit its budget whole:
/// what was dropped to make it fit.
#[derive(Serialize, Shape)]
#[shape(define)]
pub(crate) struct TokenBudget {
    strategy: &'static str,
    budget_tokens: usize,
    estimated_tokens: usize,
    /// The IDs of the entities dropped, least central first.
    truncated_entities: Vec<String>,
}

/// The token cost a `--max-tokens` budget measures: each word-like run
/// counts one token, each JSON structural character (`{ } [ ] : , "`) half.
pub fn estimate_tokens(s: &str) -> usize {
    // Count word-like sequences (alphanumeric runs) as ~1 token each,
    // plus JSON structural characters ({, }, [, ], :, ,) as ~0.5 tokens each.
    let mut words = 0usize;
    let mut structural = 0usize;
    let mut in_word = false;

    for ch in s.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            if !in_word {
                words += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if matches!(ch, '{' | '}' | '[' | ']' | ':' | ',' | '"') {
                structural += 1;
            }
        }
    }

    // Each word ≈ 1 token, structural chars ≈ 0.5 tokens each (round up)
    words + structural.div_ceil(2)
}

/// The graph's nodes in truncation order: least connected first (`Graph::degree`), ties by ID.
fn nodes_by_priority(graph: &Graph) -> Vec<&Node> {
    let mut nodes = graph.nodes();
    nodes.sort_by_cached_key(|node| (graph.degree(node.id.raw.as_str()).total(), node.id.raw));
    nodes
}

/// `render`'s export of `graph` fitted to `max_tokens` by the `prioritize`
/// strategy.
///
/// The whole export when it fits. Otherwise the fewest least connected
/// entities are dropped, along with their edges, for it to fit, and the export
/// names them in its `token_budget` block (`render`'s second argument). The
/// envelope always counts and is never cut: an embedded schema costing
/// `schema_cost` tokens that is over the budget fails with E062 rather than
/// shipping part of it, and so does a budget that cannot hold the export with
/// no entities.
pub(crate) fn fit(
    graph: &Graph,
    max_tokens: usize,
    schema_cost: Option<usize>,
    render: impl Fn(&Graph, Option<&TokenBudget>) -> Result<String, EmitterError>,
) -> Result<String, EmitterError> {
    // Everything, in graph order, when it fits.
    let full = render(graph, None)?;
    if estimate_tokens(&full) <= max_tokens {
        return Ok(full);
    }

    if let Some(schema_tokens) = schema_cost
        && schema_tokens > max_tokens
    {
        return Err(EmitterError::BudgetTooSmall {
            reason: format!(
                "the embedded schema alone costs {schema_tokens} tokens, over \
                 the token budget of {max_tokens}; raise the budget or export without the schema"
            ),
        });
    }

    // The export with the `cut` least central entities dropped, and its cost.
    let priority = nodes_by_priority(graph);
    let with_cut = |cut: usize| -> Result<(String, usize), EmitterError> {
        let kept = graph.induced(priority[cut..].iter().map(|n| n.id.raw));
        let marker = |estimated_tokens| TokenBudget {
            strategy: "prioritize",
            budget_tokens: max_tokens,
            estimated_tokens,
            truncated_entities: priority[..cut]
                .iter()
                .map(|n| n.id.raw.to_string())
                .collect(),
        };
        // The estimate is one number, one token whatever its value, so the
        // placeholder costs what the real figure does.
        let estimated = estimate_tokens(&render(&kept, Some(&marker(0)))?);
        Ok((render(&kept, Some(&marker(estimated)))?, estimated))
    };

    // Cutting an entity always lowers the cost (its ID is cheaper in the
    // truncated list than its whole entry), so the fewest cuts that fit are
    // found by bisection. (An empty graph never gets here: its full export
    // is the cheapest there is, so it fits or the empty export fails below.)
    let n = priority.len();
    let (_, empty_cost) = with_cut(n)?;
    if empty_cost > max_tokens {
        return Err(EmitterError::BudgetTooSmall {
            reason: format!(
                "the token budget of {max_tokens} cannot hold even an export \
                 with no entities ({empty_cost} tokens: the envelope and the truncated entity IDs); \
                 raise the budget"
            ),
        });
    }
    let (mut lo, mut hi) = (1, n);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if with_cut(mid)?.1 <= max_tokens {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Ok(with_cut(lo)?.0)
}
