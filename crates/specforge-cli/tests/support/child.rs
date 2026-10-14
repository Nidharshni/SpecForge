//! Long-running `specforge` children (`watch`) that never outlive the test.
//!
//! A `specforge watch` child only stops when it is killed. Killing it at the
//! end of the test body leaks it whenever the test panics first, and nothing
//! in-process runs at all when the test binary aborts or is killed. So:
//!
//! - [`ChildGuard`] stops and reaps the child on `Drop`, which covers panics.
//! - On Unix the child runs under a tiny `sh` supervisor whose stdin is a
//!   pipe (the "lifeline") held only by the test process. The supervisor
//!   blocks reading it; when the test process goes away for any reason
//!   (normal exit, panic, `abort`, `SIGKILL`) the kernel closes the write
//!   end, the read returns, and the supervisor kills the real child. No
//!   product behaviour is involved: the child never sees the lifeline.
//!
//! `specforge mcp` children need none of this: they are fed through a piped
//! stdin and exit on its EOF (`mcp_server_handles_eof_gracefully`), which
//! the kernel delivers when the test process dies.

use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// Run `"$0" "$@"` in the background on `/dev/null`, drop the
/// supervisor's own stdout/stderr (so a reader sees EOF as soon as the real
/// child exits), wait for the lifeline to close, then kill and reap.
#[cfg(unix)]
const SUPERVISOR: &str = r#""$0" "$@" </dev/null &
pid=$!
exec >/dev/null 2>&1
read -r _lifeline
kill -9 "$pid"
wait "$pid"
exit 0"#;

/// `program args...` wrapped so it dies with the test process. Set stdout
/// and stderr on the result, then [`ChildGuard::spawn`] it. Only the
/// program and its arguments carry over; set env and cwd on the wrapper
/// (the child inherits them).
pub fn guarded_command(program: &Command) -> Command {
    #[cfg(unix)]
    {
        let mut sh = Command::new("sh");
        sh.arg("-c")
            .arg(SUPERVISOR)
            .arg(program.get_program())
            .args(program.get_args());
        sh
    }
    #[cfg(not(unix))]
    {
        let mut direct = Command::new(program.get_program());
        direct.args(program.get_args());
        direct
    }
}

/// A spawned child that is stopped and reaped when dropped.
pub struct ChildGuard {
    child: Child,
    lifeline: Option<ChildStdin>,
}

impl ChildGuard {
    /// Spawn a [`guarded_command`].
    pub fn spawn(cmd: &mut Command) -> std::io::Result<Self> {
        let mut child = cmd.stdin(Stdio::piped()).spawn()?;
        let lifeline = child.stdin.take();
        Ok(Self { child, lifeline })
    }

    /// The child's stdout, if it was piped.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if cfg!(unix) {
            // Closing the lifeline makes the supervisor kill and reap the
            // real child; killing the supervisor would orphan it instead.
            drop(self.lifeline.take());
        } else {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
