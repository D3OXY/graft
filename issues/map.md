---
id: map
title: "Graft: v1 spec for a substrate-ready VCS + hub"
labels: [wayfinder:map]
status: open
---

## Destination

A locked v1 spec + architecture for **Graft** — a Rust snapshot VCS with jj-style UX whose
object model can grow into a history-native substrate, secrets as E2E-encrypted repo
objects, and a self-hostable hub (repos, proposals, issues) — ending in a build handoff
plan for implementer sessions. Planning only: this map decides; it does not build.

## Notes

- Tracker: local-markdown. Tickets are `issues/NNN-<slug>.md`; frontmatter carries
  `id/title/type/status/assignee/blocked-by`. Closed tickets keep a `## Resolution` section.
- Skills every session should consult: `/grilling`, `/domain-modeling`; `/research` and
  `/prototype` per ticket type. Glossary lives in `CONTEXT.md`.
- Standing preferences (Ishan/DEOXY): smallest model that makes correct behavior
  unsurprising; fight scope creep; Rust core/CLI; hub self-hostable in one process +
  SQLite; web frontend React/TS/Tailwind.

## Decisions so far

- [Destination: locked v1 spec, execution handed off](001-destination-shape.md) — the map produces decisions + a handoff plan; building happens after, per implementer session.
- [Core model: snapshot VCS, substrate-ready object model](002-core-model-ambition.md) — jj-style UX (no staging, stable change IDs) on an object model deliberately shaped so a history-native substrate can grow under it.
- [Secrets are E2E-encrypted repo objects](003-secrets-positioning.md) — secrets travel with the repo; hub is just sync and never sees plaintext. Core differentiator.
- [Hub is self-hostable: one process, SQLite](004-hosting-model.md) — OSS-first; hosted multi-tenant later as a deploy decision, not a rewrite.
- [Rust core and CLI](005-rust-core.md) — the substrate bet needs native perf (mmap, hashing, daemon/FUSE later) and single-binary distribution. Hub server language decided in hub architecture.
- [The name is Graft](006-name-graft.md) — grafting: joining a living branch onto another stock so they grow as one.
- [Research: object models & sync in jj, Pijul, Fossil, Sapling](007-research-vcs-prior-art.md) — jj semantics (change IDs, first-class conflicts, op log) on Fossil bones (frozen artifact format + SQLite derived cache, set-reconciliation sync); Sapling laziness reachable; Pijul a cautionary tale. Full doc on `research/vcs-prior-art`.
- [Research: E2E team secrets prior art (age, sops, Doppler, Infisical)](008-research-secrets-prior-art.md) — per-user X25519 keys, per-environment env keys wrapped per-recipient (age stanzas) as plain repo objects; add = re-wrap, remove = forced rotation; revocation honestly future-only. Full doc on `research/secrets-prior-art`.
- [Decide the object model](009-object-model.md) — grow-only bag of BLAKE3-256 artifacts (blob/tree/revision/secret-bundle, text cards + binary tree entries); Change (stable random ID) vs Revision (immutable artifact) split; jj tree-list conflicts; local-only op log; SQLite storage with rebuildable derived tables; frozen-format evolution law.
- [Decide the secrets model](010-secrets-model.md) — per-user X25519 keys; env = only access boundary (env key + generations, age-stanza wraps); bundle = whole-env snapshot on a `secrets/<env>` ref, parallel to code history; single member-signed keyring, TOFU root; removal forces rotation, no escrow; crypto-only enforcement (hub sees opaque bytes + metadata); revocation honestly future-only; plaintext never touches the tree.
- [Design the CLI surface](011-cli-surface.md) — ratified via day-transcript prototype: no-staging working change with snap/amend/edit verbs; auto-advancing branches; merge-never-fails first-class conflicts; universal op-log undo; secrets under `graft env` (`run --` the only plaintext path); clone/pull/push + provisional propose; letter change IDs vs hex revisions. Asset on `prototype/cli-surface`.

## Not yet specified

- Substrate growth path: how auto-snapshotting evolves from CLI command → watcher daemon
  → filesystem-level capture. The object model's doors are now fixed (op-log graduation,
  new artifact types, frozen-format law); what remains foggy is the capture pipeline itself.
- Org/team structure and fine-grained roles beyond v1's minimal access control. The
  secrets model has now fixed what "access" means (env membership, crypto-enforced; any
  member can grant/rotate); what stays foggy is hub-side org/team/role structure layered
  on top — post-v1, revisit alongside hosted multi-tenant.
- Hosted multi-tenant path (isolation, billing) — post-v1, but the hub architecture ticket
  should note anything that would foreclose it.
- Protocol versioning / federation between hubs.

## Out of scope

- CI / actions and runners — explicitly deferred by the destination.
- Git feature parity — Graft competes on model, not checklist. (Git *interop* is in scope
  as a scoping ticket.)
- Replacing the OS filesystem itself — the substrate bet stays inside the repo model for v1.
- SaaS operations (billing, multi-tenant isolation) — see Not yet specified; nothing in v1
  builds for it.
