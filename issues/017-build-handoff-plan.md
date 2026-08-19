---
id: 17
title: "Write the build handoff plan"
type: task
status: closed
assignee: d3oxy
blocked-by: [10, 11, 14, 15, 16, 18, 20, 21, 22]
---

## Question

Decompose the locked spec into implementer sessions: ordered work packages with file/crate
ownership, verification criteria per package, and the integration test that proves v1
(clone → work → secret share → push → propose → review → merge, end to end). This ticket
closes the map.

## Resolution

The plan is final at [`docs/build-handoff.md`](../docs/build-handoff.md): twelve
session-sized packages across five ownership boundaries (`graft-core`, `graft-sync`,
`graft-cli`, `graft-hub`, `web`, plus black-box `graft-e2e`), each with explicit
crate/path ownership, dependency order, and verification gates, ending in the
`v1_journey` acceptance test that drives release binaries over loopback HTTP through
clone → work → secret share → push → propose → review → land, then byte-scans every
store for the marker secret.

The last two blocking decisions are now incorporated:

- [Decide credential, Hub, and remote bootstrap](021-credential-hub-remote-bootstrap.md)
  — credential lifecycle (`~/.graft/`, lazy auto-create, `auth whoami|export|import`,
  `GRAFT_CREDENTIAL_FILE`) landed in Package 06; paste-PAT `auth login|logout`, PAT
  storage, single-remote `graft remote`, `push --create`, and `GRAFT_TOKEN` landed in
  Package 07; the internal publication/repo-creation endpoints in Package 09; the
  `/settings/tokens/new` page and discovery-only profile display in Package 11.
- [Freeze secret input and process-environment portability](022-secret-input-portability.md)
  — non-argv `set` (prompt/stdin/strict dotenv), POSIX-strict names with case-collision
  rejection, UTF-8/no-NUL values with 64 KiB / 1 MiB caps, `unset` on the verb surface,
  inherit-and-overlay `run` with atomic pre-flight abort — all in Package 06 with
  matching verify gates.

Package 12's journey steps now name the frozen flows concretely (paste-PAT login with
lazy credential creation; marker secret entered over piped stdin). Package ordering
gained one edge: Package 07's `auth`/bootstrap commands consume Package 06's credential
API, so 06 lands before they are wired. No placeholder or "blocked by" language remains
in the handoff.

This closes the map: every in-scope question is decided, and the destination — a locked
v1 spec with a build handoff plan for implementer sessions — is reached. Implementation
begins from `docs/build-handoff.md`, Package 01.
