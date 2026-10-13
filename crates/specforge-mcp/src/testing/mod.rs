//! The MCP driver: a server over a `specforge_project::testing::TestProject`, as a client of `specforge mcp`
//! talks to it, and the request helpers every MCP test uses.

pub mod events;
pub mod rpc;
mod served;

pub use events::{EventLog, Recorded};
pub use served::Served;
