---
id: 14
title: "Scope git interop for v1"
type: grilling
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

Does v1 ship any git bridge? Options: none (clean break); one-way import (`graft import`
from a git repo — cheap adoption on-ramp); full jj-style git-backend colocation (huge).
The originating thesis says compatibility is how substrate replacements win adoption —
weigh that against v1 scope. Decision only; if in, it lands in the handoff plan.

## Resolution

**v1 ships one-way `graft import` (git → Graft). Export deferred; colocation out.**

- **Import, full history.** Every git commit becomes a revision. The cheapest slice of
  the compatibility thesis: first contact with Graft is "try it on my existing repo".
- **Deterministic.** Import is a pure function of the git repo. Change IDs are *derived*
  from the git commit hash (truncated BLAKE3 of it; exact recipe in the spec), not minted
  randomly — so independent imports of the same repo produce identical artifacts and
  converge on sync. Team migration needs zero coordination.
- **Mapping table.** Modes: `100755` → executable, `120000` → symlink, else file.
  Octopus merges import as-is (revisions are n-ary). Commit signatures dropped.
  Submodules: entry skipped with a prominent per-path warning (hard refusal would block
  most real repos). Git branches → Graft branches. Git tags: not imported (Graft has no
  tag noun in v1), warned.
- **One-shot only**, into a fresh Graft repo. Incremental re-import against a grown
  upstream is out — that's the creep line where import becomes a bridge. Determinism
  keeps the door technically open for free; reopening it is a new decision, not a flag.
- **Implementation: shell out to `git fast-export`** and parse the stream. The user
  definitionally has git (the repo being imported is one), the stream format is stable,
  and the parser is tiny. No gitoxide/libgit2 dependency.
- **Export (Graft → git) deferred post-v1**, but the spec states the lock-in guarantee
  explicitly: the grow-only artifact bag and frozen formats make faithful export always
  possible. If the trust story proves load-bearing at launch, export is the piece to
  let back in first.
- **Colocation / bidirectional bridge: out of v1, not built toward.** jj gets colocation
  nearly free because git *is* its store; for Graft it means maintaining a mapping
  between two disagreeing object models forever. A bridge is derived-layer tooling, so
  deferring forecloses nothing — but the working assumption is better import/export
  beats it.
