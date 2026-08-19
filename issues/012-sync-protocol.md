---
id: 12
title: "Decide the sync protocol"
type: grilling
status: closed
assignee: d3oxy
blocked-by: [9]
---

## Question

Wire protocol between CLI and hub: transport (HTTP? gRPC? plain JSON+binary?), object
negotiation (want/have walk? bloom filters? full-list diff for v1?), ref update semantics
(fast-forward rules, force semantics), auth (token shape), and how ciphertext objects sync
without the hub learning anything. Bias: dumbest protocol that survives v2.

## Resolution

Ratified in one round; all eight bets landed as recommended.

**Transport.** Plain HTTPS, one opaque sync endpoint per repo (`POST /repo/<name>/sync`),
batched request/response rounds — resumable, proxy-friendly, no SSH/gRPC/WebSocket. A
separate `GET /artifact/<hash>` can be added later for tooling without touching sync.

**Encoding.** Fossil card *semantics* (`igot`/`gimme`/`file`), binary *encoding*:
length-prefixed frames `(type, len, bytes)`, zstd-compressed body. Wire is volume-dominated
(hashes + payloads); debuggability is a 50-line dump tool.

**Negotiation.** Set reconciliation over the grow-only artifact bag: `igot`/`gimme`/`file`
plus **phantoms** in v1; **no clusters** — v1 enumerates the full hash set (fine at v1 repo
sizes), and clusters are Fossil-style artifacts, so adding them later is a new artifact
type the format-evolution law already makes non-breaking. Phantoms are load-bearing: they
drive multi-round completion and are the latent door to shallow clone and the large-file
ticket. Ref-driven DAG walk rejected — it reopens the per-artifact-type sidecar problem.

**Ref/view exchange.** A separate, versioned view layer carries `branches/<name>`,
`secrets/<env>`, `secrets/keyring`. Update rule: **compare-and-swap** — every update states
the expected old target; the hub applies the batch atomically or rejects, with no ancestry
or succession judgment (it can't judge rewrites: predecessors don't sync in v1). The CLI
owns safety/`--force` UX via its op log. No special secrets-ref rules: CAS makes stale
writers lose, and keyring signatures are verified client-side — the hub stays crypto-only
per the secrets model.

**Auth.** Opaque hub-issued bearer tokens over TLS (`Authorization: Bearer`); per-user,
repo-scoped r/w decided hub-side. Transport auth deliberately decoupled from the secrets
identity (CI machines must push without joining an environment); key-based auth noted as a
possible later addition, not v1.

**Push atomicity.** Artifacts land unconditionally — grow-only set, worst case is
unreferenced garbage. The ref batch is all-or-nothing, preserving the secrets model's
atomic keyring+bundle replacement (revoke/rotate move two refs together or not at all).

**Versioning & deltas.** Round 1 carries client protocol version + capability list; server
replies in kind; unknown capabilities ignored — tolerate-and-preserve applied to the wire.
No wire deltas in v1: zstd whole-artifact only; delta transfer is a later capability flag.
(Delta encoding remains a storage-layer concern.)

**Mechanical consequences** (spec details, not open decisions): clone = sync against an
empty local set + view fetch + keyring verification (per the CLI surface); push/pull/sync
are one conversation with direction flags; protocol errors are typed frames in the body,
HTTP status codes only for transport/auth failures.

**Scope edges.** Proposals stay off this wire pending proposal semantics — if they become
artifacts, reconciliation carries them with zero protocol change. The large-file ticket is
owed exactly one thing from here: phantoms as the door.
