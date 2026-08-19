---
id: 11
title: "Design the CLI surface"
type: prototype
status: closed
assignee: d3oxy
blocked-by: [9]
---

## Question

The command surface and UX of the `graft` CLI (binary name itself included): the
no-staging-area workflow, snapshot/amend verbs, branch/switch/merge flow, conflict
resolution UX, secrets verbs (`graft env`?), and sync verbs. Prototype: a fake-but-runnable
CLI transcript (script or stub binary) covering a full day's workflow to react to — clone,
work, snapshot, branch, merge with conflict, share a secret, push, propose.

## Resolution

Prototyped as a fake day-in-the-life transcript (`prototype/cli-day.sh` on branch
`prototype/cli-surface`); Ishan ratified all seven embedded bets as shown. The surface:

**Core loop (no staging).** The working copy is always a Change, auto-snapshotted before
every command. `status` / `log` / `desc -m`; `snap -m` seals the working change (new
revision) and starts a fresh empty one on top; `amend` folds working-copy edits into the
parent change (same change ID, new revision, descendants auto-rebase); `edit <change>`
rewrites an older change in place.

**Branches.** `branch <name>` creates and switches; `switch` moves the working copy.
Branches auto-advance when you snap on them (git instinct, deliberate departure from jj's
manual bookmarks).

**Merge & conflicts.** `merge` never fails: conflicts are recorded first-class in the
revision, everything stays usable, markers materialize only in the working copy, and
status flips the file to "resolved" once markers are gone — `snap` seals it.

**Undo.** `graft undo` is op-log-powered and covers every operation, merges and pulls
included.

**Secrets.** One noun: `graft env` — `init`/`ls`/`set`/`grant`/`revoke`/`rotate`/`run`
(`unset` added later by ticket 22, which also froze `set`/`run` input and portability
contracts).
`env run <env> -- <cmd>` is the only plaintext path (process env injection). `grant`
prints the fetched key's fingerprint with an out-of-band-verify nag; `revoke` forces
rotation and prints the honest "they keep what they synced — rotate the real credential"
nag. The snapshotter ignores `.env` with a pointer to `graft env`.

**Sync & hub.** `clone` (verifies keyring, reports env memberships, starts a working
change), `pull`, `push` (artifacts + refs in one go; ciphertext bundles and keyring ride
along), `propose` opens the hub's PR-equivalent (name/semantics provisional pending the
proposal-semantics ticket).

**Display language.** Change IDs render as letters (`mqkosuwt`), revision IDs as hex
(`c7e1b9`), `@` marks the working change. Binary is `graft`; no shipped short alias.
