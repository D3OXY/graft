---
id: 17
title: "Write the build handoff plan"
type: task
status: open
assignee:
blocked-by: [10, 11, 14, 15, 16, 18, 20, 21, 22]
---

## Question

Decompose the locked spec into implementer sessions: ordered work packages with file/crate
ownership, verification criteria per package, and the integration test that proves v1
(clone → work → secret share → push → propose → review → merge, end to end). This ticket
closes the map.

## Work in progress

Draft implementation route:
[`docs/build-handoff.md`](../docs/build-handoff.md): twelve session-sized packages with
explicit crate/path ownership, dependency order, verification gates, and one real-process
acceptance journey covering clone → work → secret share → push → propose → review → land.

The final audit incorporated
[Freeze the secrets cryptographic contracts](020-freeze-secrets-crypto-contracts.md) into
Package 06, then exposed two missing user-flow contracts. Do not hand this to implementers
until [Decide credential, Hub, and remote bootstrap](021-credential-hub-remote-bootstrap.md)
and
[Freeze secret input and process-environment portability](022-secret-input-portability.md)
are resolved and incorporated.
