---
id: 2
title: "Core model: snapshot VCS, substrate-ready object model"
type: grilling
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

How radical is the core model? (a) snapshot VCS with jj-style UX (no staging area, stable
change IDs, auto-snapshot working copy); (b) history-native storage engine where VCS
semantics emerge from the substrate; (c) start (a) with the object model designed so (b)
can grow under it.

## Resolution

(c). (a) alone is "git but mine"; (b) alone is a research project. Ship the snapshot VCS,
but every object-model decision gets judged against "does this foreclose the substrate?"
The strategic sequencing (from the originating discussion): win the UX layer first, swap
the storage model underneath after.
