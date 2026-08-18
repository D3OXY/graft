---
id: 7
title: "Research: object models & sync in jj, Pijul, Fossil, Sapling"
type: research
status: open
assignee: d3oxy (research agent running)
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
