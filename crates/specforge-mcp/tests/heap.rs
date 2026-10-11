//! A production server keeps no event (ADR 0057): what it holds does not
//! grow with the requests it answers. Its own test binary: the global
//! allocator below counts the bytes the test's thread holds.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use serde_json::json;
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// Counts the bytes each thread allocated and has not freed.
struct Counting;

thread_local! {
    static HELD: Cell<isize> = const { Cell::new(0) };
}

fn held_by(delta: isize) {
    // During thread teardown the counter may be gone: nothing to count then.
    let _ = HELD.try_with(|held| held.set(held.get() + delta));
}

// SAFETY: every method forwards to `System` unchanged; the counter only
// reads the sizes it is given.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            held_by(layout.size() as isize);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            held_by(layout.size() as isize);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        held_by(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            held_by(new_size as isize - layout.size() as isize);
        }
        new
    }
}

#[global_allocator]
static HEAP: Counting = Counting;

/// The bytes this thread holds.
fn held() -> isize {
    HELD.with(Cell::get)
}

/// One round of what an agent sends: three tool calls, a read, a listing, a
/// prompt, a protocol error, a cancel and a ping.
fn round(server: &mut McpServer) {
    for request in [
        json!({"method": "tools/call", "params": {"name": "specforge.explain", "arguments": {"code": "E001"}}}),
        json!({"method": "tools/call", "params": {"name": "specforge.stats", "arguments": {}}}),
        json!({"method": "tools/call", "params": {"name": "specforge.inspect", "arguments": {"entity_id": "a"}}}),
        json!({"method": "resources/read", "params": {"uri": "specforge://graph"}}),
        json!({"method": "tools/list", "params": {}}),
        json!({"method": "prompts/get", "params": {"name": "specforge://prompts/context", "arguments": {"entity_id": "a"}}}),
        json!({"method": "no/such/method", "params": {}}),
        json!({"method": "$/cancelRequest", "params": {"id": 7}}),
        json!({"method": "ping"}),
    ] {
        let mut request = request;
        request["jsonrpc"] = json!("2.0");
        request["id"] = json!(1);
        assert!(server.handle_message(&request.to_string()).is_some());
    }
}

#[specforge_test(
    behavior = "mcp_event_record",
    verify = "a server keeps no event: what it holds does not grow with the requests it answers"
)]
#[ignore = "fixed by T1"]
fn a_server_keeps_no_event() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("a.spec"), "behavior a \"A\" {\n}\n").unwrap();
    // What `specforge mcp <root>` runs: no sink given.
    let mut server = McpServer::with_project_root(dir.path().to_path_buf());
    let initialize = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}}});
    server.handle_message(&initialize.to_string()).unwrap();
    // Caches (output schemas, the catalog, the session's stamps) fill once.
    for _ in 0..50 {
        round(&mut server);
    }
    let before = held();
    for _ in 0..500 {
        round(&mut server);
    }
    let grown = held() - before;
    assert!(
        grown < 256 * 1024,
        "500 rounds of requests left {grown} more bytes held: the server keeps something per request"
    );
}
