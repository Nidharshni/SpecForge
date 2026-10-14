//! A runtime survives caught signals in its host process.
//!
//! On macOS wasmtime's default Mach-port trap handling runs a handler thread
//! that aborts the process when its `mach_msg` wait is interrupted, and any
//! signal with a handler can land on that thread. `assert_cmd`'s `.timeout()`
//! (via `wait-timeout`) installs exactly such a SIGCHLD handler, so the CLI
//! integration tests aborted once in a few parallel runs. This binary
//! installs the same handler, builds a runtime, and makes many children
//! exit: with Mach ports it aborts within a few hundred exits.
//!
//! `harness = false`: the handler must be process-wide, and the abort only
//! reproduced reliably with this binary's main thread owning the layout. The
//! binary still answers the libtest list protocol nextest asks first
//! (`--list --format terse`, then `--exact <name>`), so it lists its one test
//! instead of running it while nextest only wants the list.

/// The one test this binary runs, as libtest would name it.
#[cfg(target_os = "macos")]
const TEST: &str = "runtime_survives_sigchld_with_a_handler";

// Off macOS nothing follows the `--list` branch, so its `return` is needless.
#[cfg_attr(not(target_os = "macos"), allow(clippy::needless_return))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        // Ignored tests: none. Everything else: the one test, on macOS.
        #[cfg(target_os = "macos")]
        if !args.iter().any(|a| a == "--ignored") {
            println!("{TEST}: test");
        }
        return;
    }
    #[cfg(target_os = "macos")]
    if selected(&args) {
        runtime_survives_sigchld_with_a_handler();
    }
}

/// Whether the command line selects the test: no filter, or a filter that
/// matches it (exactly under `--exact`, as a substring otherwise).
#[cfg(target_os = "macos")]
fn selected(args: &[String]) -> bool {
    let exact = args.iter().any(|a| a == "--exact");
    let filters: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    filters.is_empty()
        || filters.iter().any(|f| {
            if exact {
                f.as_str() == TEST
            } else {
                TEST.contains(f.as_str())
            }
        })
}

#[cfg(target_os = "macos")]
fn runtime_survives_sigchld_with_a_handler() {
    /// macOS `struct sigaction` as `sigaction(2)` takes it.
    #[repr(C)]
    struct SigAction {
        handler: usize,
        mask: u32,
        flags: i32,
    }
    unsafe extern "C" {
        fn sigaction(sig: i32, act: *const SigAction, old: *mut SigAction) -> i32;
    }
    extern "C" fn on_sigchld(_: i32) {}
    const SIGCHLD: i32 = 20;
    const SA_RESTART: i32 = 0x0002;
    const SA_NOCLDSTOP: i32 = 0x0008;

    let act = SigAction {
        handler: on_sigchld as *const () as usize,
        mask: 0,
        flags: SA_RESTART | SA_NOCLDSTOP,
    };
    // SAFETY: installs a handler that does nothing; `act` outlives the call.
    assert_eq!(unsafe { sigaction(SIGCHLD, &act, std::ptr::null_mut()) }, 0);

    let _runtime = specforge_component::ComponentRuntime::new();

    let workers: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..400 {
                    std::process::Command::new("true").status().unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    println!("{TEST} ... ok");
}
