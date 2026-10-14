//! The read views: operations over the project view that answer what a project holds and how it
//! stands, each a typed outcome its surfaces render (ADR 0015); navigation, where the LSP and MCP
//! answer "where is it" and "what refers to it" (ADR 0016); inference's guide, plan, progress,
//! gaps, lint and sessions (ADR 0015 M12–M14); and explain, the catalog's word on one code. They
//! change no source: export records the schema (ADR 0015 D10) and an inference session its
//! manifest, nothing else. The group uses only the base (`lib.rs`, "Module groups").

pub mod coverage;
pub mod explain;
pub mod explore;
pub mod export;
pub mod infer;
pub mod inspect;
pub mod model;
pub mod navigate;
pub mod plan;
pub mod query;
pub mod review;
pub mod schema;
pub mod stats;
pub mod trace;
