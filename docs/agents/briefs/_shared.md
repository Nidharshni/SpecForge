# Rules every brief carries

Paste this block at the top of each brief you send.

- Return your report as the final reply. Write only inside your worktree and, for a plan agent, the planning directory.
- Scratch files go under `<scratchpad>/<your label>/`.
- End every commit message with the trailer from your own system context.
- Build with `CARGO_TARGET_DIR=<repo parent>/SpecForge-targets/<your label>`, `CARGO_PROFILE_DEV_DEBUG=line-tables-only`,
  `CARGO_INCREMENTAL=0`; never the repository's `target/`.
- First command: merge the integration tip into your branch. Last action before the final reply: merge it again and
  re-run clippy and nextest for the crates you touched.
- No backward compatibility: SpecForge is pre-release; delete the old name and update every caller.
- Untracked files in the checkout (`git status --short | grep '^??'`) are the user's: not yours to add, edit or delete.
- Disk low (`df -h .` under 10 GB, or "No space left on device" in a log): `cargo run -p xtask --bin clean-target`, then re-run.
  A failure with that text is the disk's, not the code's.
- macOS has BSD `sed` (`sed -i ''`, or the Edit tool) and zsh errors on an unquoted glob that matches nothing: quote
  `--include='*.rs'`.
- A Bash call that reads stdin never returns (the harness keeps stdin open), and the agent hangs for hours. Never leave
  a file argument empty (an unset variable as a file name), never run a bare `cat`, `sed`, `grep`, `head` or `python3 -`
  without a file or a heredoc that feeds it, append `</dev/null` to any command that might read stdin, wrap long
  commands in `timeout`, and edit files with the Edit/Write tools, not piped one-offs.
- A target directory built in one worktree bakes that worktree's path into the test binaries (`specforge-repo-root`).
  After the worktree is deleted or moved, use a fresh `CARGO_TARGET_DIR`: the stale one fails hundreds of tests.
