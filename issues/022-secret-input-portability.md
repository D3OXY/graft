---
id: 22
title: "Freeze secret input and process-environment portability"
type: grilling
status: open
assignee:
blocked-by: []
---

## Question

What exact plaintext contract does v1 expose for `graft env set` and `graft env run`?
Freeze how values enter without leaking through argv or files, the accepted secret-name
grammar and value byte constraints, overwrite/delete behavior, cross-platform process
environment semantics, and the failure behavior for values the target OS cannot inject.
Keep the encrypted SecretBundle format capable of preserving canonical bytes without
silently promising portability the CLI cannot provide.
