---
id: 5
title: "Rust core and CLI"
type: grilling
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

TypeScript end-to-end for v1 speed, or Rust for the core + CLI from day one?

## Resolution

Rust. The substrate ambition (ticket 2) eventually needs mmap'd object stores, hashing at
scale, a watcher/auto-snapshot daemon, possibly FUSE — native territory TS can't credibly
reach. Also: single-binary CLI distribution and credibility in the VCS space (git, jj,
Sapling are native). Accepted cost: slower v1. Open remainder: whether the hub server is
also Rust (axum sharing the core crate — strong default since the hub diffs/merges
server-side) is decided in the hub-architecture ticket. Web frontend stays React/TS.
