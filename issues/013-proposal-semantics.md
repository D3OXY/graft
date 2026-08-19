---
id: 13
title: "Decide proposal semantics"
type: grilling
status: closed
assignee: d3oxy
blocked-by: [9]
---

## Question

Graft's PR-equivalent: branch-based (GitHub model) or change-based (Gerrit/jj model, where
a stable change ID is the unit of review and stacks come free)? What's it called? Merge
mechanics on the hub (who computes the merge, fast-forward vs merge-change), review
primitives for v1 (comments only? approvals?), and how a proposal interacts with rewritten
changes (amend after review — the thing GitHub is worst at, and stable change IDs should
make trivial).

## Resolution

Ratified in two rounds; all twelve bets landed as recommended.

**Unit of review: stack-based hybrid.** A **Proposal** is the ordered segment of changes
between target and tip — what `graft propose` grabs from your branch. Opened, discussed,
and landed as one thing (GitHub familiarity), but reviewed **change-by-change**: each
change is the anchor for comments and interdiffs (Gerrit precision). Stacks come free —
the segment *is* the stack. Pure branch-based rejected (throws away change IDs); pure
Gerrit rejected (one-change-one-ceremony fights the auto-advancing-branch CLI).

**Name: Proposal**, final. `propose` is already the ratified verb; "a graft on Graft"
collides with the tool name.

**Storage: hub-side rows for v1**, not artifacts. Discipline: proposals reference changes
only by change ID + revision hash, never by copying content — a later
proposals-as-artifacts migration is a re-homing, not a re-model (sync ticket already
holds the door: new artifact type, zero protocol change).

**Merge mechanics: hub computes; one strategy — rebase-and-land.** The hub rebases the
stack onto the target and fast-forwards. Conflicts are first-class so the rebase always
*succeeds*; the invariant is the hub **refuses to land a conflicted result** (author
pulls, resolves, amends; proposal updates automatically). Rebase isn't falsification when
change IDs survive it; main stays linear. No squash (snap discipline = changes are already
curated units), no merge-revision option in v1. **Consequence for ticket 16: the hub
embeds the Rust core.**

**Review primitives (the v1 floor):** top-level comments, inline comments (anchored
file+line on a specific revision), and a per-reviewer **approve** flag. Inline comments
form **threads with a resolve toggle** (anyone can resolve). No request-changes state, no
required-approvals policy engine, no suggested edits — anyone with write can land;
enforcement is social. Policy knobs are post-v1.

**Amend-after-review: fully automatic** — the headline feature. The proposal tracks change
IDs; pushing a rewrite advances it, no `propose --update` ceremony. Comments stay anchored
to the revision they were written on (Gerrit-style per-patchset); the hub offers
**interdiff** — diff between two revisions of the same change — so reviewers see exactly
what moved since their last pass.

**GC/reachability:** hub-managed internal ref per proposal (`proposals/<id>` → stack tip),
advanced on amend, dropped on close; invisible to normal branch listing. This is what
makes proposed revisions survive GC and arrive via plain sync.

**Access: write-access-only in v1.** Fork-based contribution deferred to map fog; nothing
forecloses it — change IDs are globally unique, so a forked change is the *same change*.

**Landing granularity: whole-proposal atomic.** One button, one rebase, one CAS ref batch.
Prefix-landing (land first n changes) is v2; changes are individually addressed already,
nothing forecloses it.

**Stale-proposal status:** trial rebase computed lazily, cached per (target tip, stack
tip) pair — same computation as landing, always terminates. "Would conflict" on the
proposal page is a hub *may*; refusal at land time is the invariant.

**Divergence meets proposal: the proposal ref is the arbiter.** A proposal tracks whatever
revision `proposals/<id>` points through; pushing is choosing. Divergence stays local and
displayed; the hub never judges it, no divergence UI in v1.

**Lifecycle:** open → landed | closed; reopen allowed on closed (not landed). Who-landed
is already captured by the committer field on the rebased revisions.

Glossary updated in CONTEXT.md (Proposal finalized; Land, Interdiff added).
