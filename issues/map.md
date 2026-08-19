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
- [Decide the sync protocol](012-sync-protocol.md) — HTTPS single-endpoint set reconciliation (igot/gimme/file + phantoms; clusters deferred), binary frames + zstd; versioned view layer with CAS all-or-nothing ref batches; bearer tokens decoupled from secrets identity; version+capability handshake; no wire deltas in v1.
- [Decide proposal semantics](013-proposal-semantics.md) — Proposal (name final) = stack of changes reviewed change-by-change, landed atomically by hub-computed rebase-and-land (conflicted results refused; hub embeds the Rust core); comments/threads/approve floor; automatic amend tracking with per-revision anchors + interdiffs; `proposals/<id>` refs as GC roots; write-access-only, hub-side rows.
- [Scope git interop for v1](014-git-interop-scope.md) — one-way `graft import`: full history, deterministic (change IDs derived from git hashes so independent imports converge), one-shot, via `git fast-export` shell-out; export deferred with a spec-stated lock-in guarantee; colocation out.
- [Prototype the hub UI](015-hub-ui-prototype.md) — variant A ratified: GitHub-shaped frame carrying graft vocabulary — stack strip, change-by-change review with interdiff jumps, land panel that refuses conflicts, metadata-only env screen. Asset in `prototype/hub-ui/` (branch `prototype/hub-ui`).

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
- Federation between hubs. CLI↔hub versioning is now decided (capability handshake, sync
  ticket); hub↔hub sync and identity across hubs stay foggy.
- Post-v1 review depth: fork-based contribution (change IDs are globally unique, so a
  forked change is the *same* change), prefix-landing of stacks, and proposals-as-artifacts
  offline review. Proposal semantics fixed the v1 floor; these deepen it later.

## Out of scope

- CI / actions and runners — explicitly deferred by the destination.
- Git feature parity — Graft competes on model, not checklist. (Git *interop* was scoped
  by [Scope git interop for v1](014-git-interop-scope.md).)
- Graft→git export and bidirectional colocation — deferred post-v1 by the interop
  decision; the spec carries the export guarantee, and nothing in v1 builds toward a
  bridge.
- Replacing the OS filesystem itself — the substrate bet stays inside the repo model for v1.
- SaaS operations (billing, multi-tenant isolation) — see Not yet specified; nothing in v1
  builds for it.
