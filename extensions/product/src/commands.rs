//! The `specforge product <command>` commands, each declared ([`declare`])
//! with the handler that answers its `cmd__product_<id>` export: the SDK
//! derives the `surfaces` payload and the export routing from the one
//! declaration. A handler reads its args through the declaration
//! ([`CommandCall`]), runs a query over the graph the host passes and
//! renders it in the format the host asked for (`human`, the CLI default,
//! or `json`, always over MCP; ADR 0011). The same exports serve the MCP
//! tools `specforge.product.<id>`.
//!
//! Under `json` a command prints one object, the payload type its surface
//! behavior names; under `human` its layout here, a table with a header row
//! where the payload is tabular. A command that cannot answer prints one
//! `ProductSurfaceError` to stderr and nothing to stdout: `ENTITY_NOT_FOUND`
//! (exit 1) with the nearest id of the kind, or `INVALID_INPUT` (exit 2).

use crate::queries;
use serde::Serialize;
use specforge_extension_sdk::prelude::{
    CommandBuilder, CommandCall, CommandGraph, CommandOutput, ContributionsBuilder,
};
use std::fmt::Write as _;

mod coverage_matrices;
mod dates_and_effort;
mod dependency_graphs;
mod features;
mod lists;
mod milestones;
mod project_wide;
mod rollups;
mod term_analytics;
mod traceability;

/// Declare every command, each with the handler that answers it.
pub fn declare(c: &mut ContributionsBuilder) {
    c.command_prefix("product");
    lists::declare(c);
    milestones::declare(c);
    features::declare(c);
    traceability::declare(c);
    rollups::declare(c);
    dependency_graphs::declare(c);
    coverage_matrices::declare(c);
    term_analytics::declare(c);
    dates_and_effort::declare(c);
    project_wide::declare(c);
}

/// Declare the positional arg naming the entity of `kind` a query is about.
fn entity_arg(cmd: &mut CommandBuilder, kind: &str) {
    cmd.arg(kind, |a| {
        a.required().description(&format!("The {kind} id"));
    });
}

/// Declare the `--limit` and `--offset` counts of a paged command.
fn page_args(cmd: &mut CommandBuilder) {
    cmd.arg("limit", |a| {
        a.count()
            .description("Return at most this many (default 100, clamped to 1-1000)");
    })
    .arg("offset", |a| {
        a.count().description("Skip this many first (default 0)");
    });
}

/// The `--offset` and `--limit` `call` sets ([`page_args`]).
fn page(call: &CommandCall<'_>) -> (Option<usize>, Option<usize>) {
    (call.count("offset"), call.count("limit"))
}

/// A paged project-wide command: one page of `items` under `key` beside
/// the fields `extra` adds.
fn paged<T: Serialize>(
    call: &CommandCall<'_>,
    key: &str,
    items: Vec<T>,
    extra: serde_json::Value,
    human: impl FnOnce(&queries::Page<T>, &mut String),
) -> CommandOutput {
    let (offset, limit) = page(call);
    let page = queries::paginate(items, offset, limit);
    let mut payload = page.payload(key);
    if let (Some(payload), serde_json::Value::Object(extra)) = (payload.as_object_mut(), extra) {
        payload.extend(extra);
    }
    call.render(&payload, |out| {
        human(&page, out);
        if page.has_more {
            let _ = writeln!(
                out,
                "{} of {} {key}; --offset {} for more",
                page.items.len(),
                page.total,
                page.offset + page.items.len()
            );
        }
    })
}

/// A query about the entity the positional arg `kind` names
/// ([`entity_arg`]): its payload rendered, or `ENTITY_NOT_FOUND` (exit 1)
/// with the nearest id of the kind.
fn lookup<T: Serialize>(
    call: &CommandCall<'_>,
    kind: &str,
    query: impl FnOnce(&CommandGraph, &str) -> Option<T>,
    human: impl FnOnce(&T, &mut String),
) -> CommandOutput {
    let id = call.str(kind).unwrap_or_default();
    match query(call.graph(), id) {
        Some(result) => call.render(&result, |out| human(&result, out)),
        None => call.fail(
            &queries::not_found(call.graph(), kind, id),
            queries::NOT_FOUND_EXIT,
        ),
    }
}

/// Entity ids, one per line under `heading`.
fn ids(out: &mut String, heading: &str, ids: &[String]) {
    let _ = writeln!(out, "{heading}:");
    for i in ids {
        let _ = writeln!(out, "  {i}");
    }
    if ids.is_empty() {
        let _ = writeln!(out, "  (none)");
    }
}

/// `rows` under `headers`, each column as wide as its widest cell, two
/// spaces apart; no trailing spaces.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let header: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    for row in std::iter::once(&header).chain(rows) {
        let mut line = String::new();
        for (i, (cell, width)) in row.iter().zip(&widths).enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            let _ = write!(line, "{cell:<width$}");
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
