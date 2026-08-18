---
id: 3
title: "Secrets are E2E-encrypted repo objects"
type: grilling
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

Are team-shared, access-controlled env vars/secrets (a) a hub feature (server-side
encrypted, like GitHub environments), or (b) part of the VCS model itself — secrets as
encrypted objects that travel with the repo, hub as dumb sync, server never sees plaintext?

## Resolution

(b). Strongest differentiator in the idea: "clone the repo, get the env — if you have the
key." Kills .env-sharing SaaS as a category for Graft users. Falls out naturally from
owning the object model. Key distribution, access granularity, and rotation are decided in
the secrets-model ticket.
