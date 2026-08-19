---
id: 18
title: "Decide the large-file strategy for v1"
type: grilling
status: open
assignee: d3oxy
blocked-by: [9]
---

## Question

Graduated from map fog by the object-model resolution (ticket 9). Blobs are raw bytes;
the frozen-format law means a chunked-blob artifact type can be added later without a
break, and set-reconciliation phantoms make "choose not to fetch" (partial clone) latent
in the model. So: does v1 ship raw blobs only (delta-encoded in storage) and defer
chunking/partial fetch entirely, or does the v1 spec commit to a chunked-blob artifact
type and/or a size threshold now? What's the honest answer for repos with big binaries at
v1, and what exactly is deferred?
