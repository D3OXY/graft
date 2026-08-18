---
id: 11
title: "Design the CLI surface"
type: prototype
status: open
assignee: d3oxy
blocked-by: [9]
---

## Question

The command surface and UX of the `graft` CLI (binary name itself included): the
no-staging-area workflow, snapshot/amend verbs, branch/switch/merge flow, conflict
resolution UX, secrets verbs (`graft env`?), and sync verbs. Prototype: a fake-but-runnable
CLI transcript (script or stub binary) covering a full day's workflow to react to — clone,
work, snapshot, branch, merge with conflict, share a secret, push, propose.

## Prototype (awaiting reaction)

Asset: `prototype/cli-day.sh` on branch `prototype/cli-surface` (an untracked runnable
copy sits in the worktree). Run `./prototype/cli-day.sh` — Enter steps through a fake
day; `--fast` plays it straight through. Every output is hand-written; the script's
closing screen lists the seven embedded bets to react to: verb set (snap/amend/edit),
branch auto-advance on snap, merge-never-fails + first-class conflict display, universal
`graft undo`, `graft env` ergonomics (run/grant/revoke nags), push/propose split, and the
change-ID/revision display language. HITL: resolves only on Ishan's reaction.
