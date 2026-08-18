---
id: 10
title: "Decide the secrets model"
type: grilling
status: open
assignee:
blocked-by: [8, 9]
---

## Question

How do secrets live as E2E-encrypted repo objects (ticket 3)? Recipient/key model
(per-user keys? team key envelopes?), access granularity (per repo? per environment like
dev/staging/prod? per path?), what the hub stores and enforces vs what cryptography
enforces, rotation and member-removal semantics, and the CLI/hub UX for granting access.
Must answer honestly: what does revoking access mean for already-synced ciphertext?
