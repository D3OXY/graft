---
id: 7
title: "Research: object models & sync in jj, Pijul, Fossil, Sapling"
type: research
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

What should Graft's object model steal or avoid from prior art? Specifically: how jj
separates change identity from commit identity (evolution/rewriting mechanics); how Pijul's
patch theory handles merges and why it didn't win adoption; how Fossil bundles
repo+tickets+wiki in SQLite; how Sapling handles scale (segmented changelog, lazy trees);
and what their sync/wire protocols look like. Output: a findings doc with concrete
recommendations for (a) change identity, (b) tree/blob encoding, (c) conflict
representation, (d) sync negotiation — each judged against the substrate-growth ambition
(ticket 2).

## Resolution

Findings: `docs/research/vcs-prior-art.md` on branch `research/vcs-prior-art` (7903f52).
Recommendations for the object-model ticket:

- **Change identity**: adopt jj wholesale — random change ID (not a stored object) +
  content-hash commit ID; rewriting = new commit, same change ID, predecessor list.
  Divergence is rendered state, never an error (lock-free concurrency).
- **Op log**: adopt jj's operation log (views as snapshots) — gives undo and is itself a
  proto-substrate. GC via Sapling's visibility-first model.
- **Tree/blob**: git-shaped content-addressed snapshots, but with Fossil's two-layer
  discipline — frozen canonical artifact format + SQLite as a rebuildable derived cache
  (`rebuild` from day one).
- **Reject** Pijul's graph-of-lines as canonical encoding: right theory, wrong substrate
  bet — format churn + bespoke storage/host killed adoption (author's own accounts).
- **Conflicts**: first-class, jj's tree-algebra `A+(C-B)` stored in commits; operations
  always succeed; markers are working-copy *rendering*, parsed back on snapshot.
- **Sync**: Fossil-style set reconciliation (igot/gimme, clusters, phantoms), not git
  haves/wants — closed under new artifact types, so secrets/proposals sync for free;
  phantoms give shallow/partial clone latently. Small versioned view-exchange layer on
  top for refs/heads.
- **Synthesis**: jj semantics on Fossil bones, Sapling laziness reachable, Pijul as
  cautionary tale.
