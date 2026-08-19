---
id: 12
title: "Decide the sync protocol"
type: grilling
status: open
assignee: d3oxy
blocked-by: [9]
---

## Question

Wire protocol between CLI and hub: transport (HTTP? gRPC? plain JSON+binary?), object
negotiation (want/have walk? bloom filters? full-list diff for v1?), ref update semantics
(fast-forward rules, force semantics), auth (token shape), and how ciphertext objects sync
without the hub learning anything. Bias: dumbest protocol that survives v2.
