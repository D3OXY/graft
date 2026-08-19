---
id: 13
title: "Decide proposal semantics"
type: grilling
status: open
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
