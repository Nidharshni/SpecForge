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
