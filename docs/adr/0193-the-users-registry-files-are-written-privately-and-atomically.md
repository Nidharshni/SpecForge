# The user's registry files are written privately and atomically

**Status:** accepted (2026-10-21, architecture round 21, plan 02). Amends [ADR 0178](0178-a-registry-credential-belongs-to-its-registry-root.md) (D3: `User` spells the signing key's and the known keys' names and hands doctor the key's path; D4 and the platform amendment: every file of the user's directory, not only a kept secret, is 0600 in a 0700 directory, written by `replace_private`). Follows [ADR 0144](0144-a-project-file-is-written-by-replacing-its-name.md) (the same temporary-name discipline; its D2, "a file keeps its permissions", does not hold for a private file) and keeps [ADR 0059](0059-trust-pins-are-written-with-the-lock-plan.md) D2 (the pins are replaced whole; now also synced).

## Context

- `~/.specforge` holds four kinds of file and three writers decided each file's mode, its directory's mode, its atomicity
  and its sync on their own. The credential store and the kept secrets had a private writer; the publisher signing key,
  the most valuable secret there, was written in place at the umask (0644) in a 0755 directory and chmodded after, the
  chmod's error dropped; the known keys wrote a temporary at 0644, chmodded, renamed, never synced.
- Reproduced: the whole 136-byte secret key readable as `0o644` during every first publish (3 of 3); the directory
  0700 or 0755 by which command ran first; a kept secret written through a link planted at the private writer's
  temporary name (it opened with create and truncate, so a link was followed and a stale file kept its mode).
- A signing key the process could not read was silently replaced by a new one (the read error fell through to
  "create"), contradicting the spec ("a signing key that can't be read ... MUST be refused with E074") and E074's own
  documentation; whoever pinned the old key then sees a changed key. A corrupt key's E074 quoted serde's error, which
  echoes a wrong-shaped value.
- `signing-key.json` was spelled twice: by `User` and by `credential_health`.

## Decision

- **D1.** One writer: `specforge_common::fresh::replace_private(path, contents)`, beside `replace_file`. Missing parent
  directories are created 0700; `<path>.tmp` is unlinked (a link, never its target) and created new (`O_EXCL`) at 0600
  before a byte is written, synced, renamed onto the name, removed on failure. A reader sees the old file or the new one.
  The directory entry is not synced (as for `replace_file`): after a power loss the old file may remain, never a torn one.
- **D2.** The credential store, the kept secrets, the signing key and the known keys all go through it. `write_private`
  and both `restrict_permissions` copies are deleted. A file's existing mode is replaced, not kept: 0600 is the rule
  (a pre-seeded 0444 `known-keys.json` is replaced on the first pin, as the rename always did).
- **D3.** Only `NotFound` creates a signing key. Any other read error is E074 and the file is left as it is; a file that
  is read but is not a key is E074 too, named and never quoted.
- **D4.** Each name is spelled once: `User` spells `signing-key.json` and `known-keys.json` and gives `credential_health`
  the key's path; the store spells `credentials.json` and `secrets/`.
- **D5.** Windows is unchanged: no mode is set; the files rely on the user profile directory's default ACL (ADR 0178's
  platform amendment, now for every user file). Open until a Windows user asks for more.
- **D6.** An existing directory is left as it is: a `~/.specforge` that a first publish or add created at 0755 stays
  0755. Every file in it is 0600 and `secrets/` is 0700, so another user sees only four fixed names; `chmod 700
  ~/.specforge` closes that. Existing files become 0600 on their next write. A temporary left by a crash under the old
  name (`credentials.tmp`, `secrets/<id>.tmp`) stays unused.

## Rejected

- Create-once by hard link for the first signing key (closes a concurrent first publish nobody reproduced; `hard_link`
  fails on some home file systems and a per-process temp leaves a secret behind after a crash).
- Making `write_private` public: it opened its temporary with create and truncate, which is the defect.
- A `UserDir` type that wraps four joins and a pass-through: a shallow module; `User` already owns the paths.
- Syncing the directory after the rename: the same guarantee as ADR 0144's writer.
- Making an existing directory 0700 on every write: it would chmod a directory the user may have opened on purpose.
- Setting the umask: process-wide and needs `unsafe`.

## Consequences

- Breaking: `credential_health` and `credential_health_now` take the signing key's path. A publish with an unreadable
  signing key refuses with E074 instead of replacing it.

## What would reopen it

A reproduced concurrent first publish (two processes both creating the key); a user on a file system where `rename`
is not atomic; a Windows user asking for an ACL.
