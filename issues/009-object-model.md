---
id: 9
title: "Decide the object model"
type: grilling
status: closed
assignee: d3oxy
blocked-by: [7]
---

## Question

Graft's object kinds and encodings: what are the objects (blob/tree/change/…), what is the
content-address (hash choice, canonical encoding), how does stable change identity relate
to content hash, how are conflicts represented (first-class in the model à la jj, or
marker-files à la git), and — the governing constraint — which choices keep the door open
for a history-native substrate to grow underneath (ticket 2)? Also: where do
secrets-bearing objects hook in (interface only; detail in ticket 10)?

## Resolution

Adopted the research synthesis (ticket 7) wholesale, with parameters locked:

**Canonical layer.** A repo is a grow-only set of immutable, content-addressed
**artifacts**; everything else is derived. Hash: **BLAKE3-256**, 32 bytes, single
algorithm, no agility. v1 artifact types: **blob** (raw bytes), **tree**, **revision**,
**secret bundle**. Structural artifacts use Fossil-style UTF-8 text cards, except tree
*entries*, which use a compact binary encoding inside the tree artifact (trees dominate
byte volume, never need eyeballing).

**Identity (two nouns).** A **Change** is the evolving unit of work — not an artifact,
just a stable random 16-byte **change ID** minted at creation. A **Revision** is the
immutable artifact recording one version of a Change; its content hash is the revision ID.
Rewriting = new revision, same change ID. Divergence (multiple visible revisions of one
change) is a displayed state, never an error.

**Revision fields** (the frozen format): ordered tree list (one = clean, >1 = conflict),
parent revision IDs (0..n, n-ary merges allowed), change ID, author, committer
(name/email/timestamp each; committer is the only *synced* trace of who rewrote), message.
Nothing else.

**Change ID placement**: in the revision artifact (survives sync, no sidecar).
**Predecessors**: NOT in the artifact — they live in the local op-log layer, ignorable
once successors are invisible (Sapling's lesson; keeps abandoned drafts out of the bag and
GC sane). Accepted consequence: predecessor history does not sync in v1.

**Op log**: local-only per-workspace state (jj-style views; powers undo), but encoded in
the same artifact format so syncing it later is a graduation, not a migration.

**Conflicts**: jj's model wholesale — conflicted revision holds an ordered tree list
interpreted algebraically (`A+(C-B)+…`), simplified on rebase, materialized to markers
only at the working-copy boundary; same-change-on-all-sides counts as resolved.

**Tree entries**: `file`, `executable file`, `symlink`, `subtree`. No numeric modes, no
submodules. Empty dirs not tracked by the snapshotter, but the format permits them (entry
→ empty tree), so tracking could be enabled later without a format break.

**Secrets hook (interface only)**: secret bundle = opaque ciphertext + minimal plaintext
routing metadata; content-addressed, immutable, syncs like any artifact, plaintext never
in the bag. Free-floating (not referenced from trees); GC treats bundles as roots-by-type.
Flags for ticket 10: (1) how a client knows which bundle is *current* (probably a ref, not
an artifact property); (2) rotation leaves old ciphertext in the bag forever — ticket 10
owns whether that's acceptable.

**Storage layer**: single SQLite file per repo — artifacts table (compressed,
delta-encoded) + derived relational tables, with `graft rebuild` regenerating all derived
state from artifacts as a **day-one invariant** (the enforcement mechanism keeping the
canonical layer honest).

**Format evolution law** (in the spec verbatim): (1) a shipped artifact type's encoding is
frozen forever; (2) evolution = new artifact types or new optional cards old readers can
skip safely; (3) unknown artifact types must sync and store fine as opaque bytes —
tolerate-and-preserve from day one.

Glossary updated in CONTEXT.md (Artifact, Change, Revision, Divergence, Op log, Secret
Bundle).
