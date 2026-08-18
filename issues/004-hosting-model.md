---
id: 4
title: "Hub is self-hostable: one process, SQLite"
type: grilling
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

Hosting model for the hub: (a) self-hostable OSS, single process + SQLite; (b) cloud
multi-tenant SaaS from day one; (c) both from the start?

## Resolution

(a), architected so a hosted flagship later is a deploy decision, not a rewrite. One
process, one database file, deploy anywhere.
