---
id: 16
title: "Decide hub architecture"
type: grilling
status: open
assignee:
blocked-by: [10, 12, 13]
---

## Question

The hub's shape: server language (axum sharing the Rust core crate vs TS server — see
Rust decision's open remainder), SQLite schema (users/repos/objects/refs/proposals/issues),
auth (tokens, sessions, registration flow for self-hosters), minimal org/team/role model
that the secrets model needs, static frontend serving, and deploy story (single binary?
Docker? Railway template?). Also names: does the hub product carry its own name?
