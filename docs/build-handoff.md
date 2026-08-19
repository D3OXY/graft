# Graft v1 build handoff

> **Draft:** Package 06 is blocked on
> [Freeze the secrets cryptographic contracts](../issues/020-freeze-secrets-crypto-contracts.md).
> Do not begin implementation until that resolution replaces its open crypto choices.

This is the implementation route for the decisions indexed by
[`issues/map.md`](../issues/map.md). It orders work; it does not reopen product choices.
Each package should fit one implementer session and land green before its dependants begin.

## Shipping shape

The workspace has four production boundaries:

```text
graft-cli ───────> graft-core <─────── graft-hub <──── web (embedded assets)
    └────────────> graft-sync <───────────┘

graft-e2e drives the real CLI and Hub binaries over loopback HTTP.
```

Arrows point from a consumer to its dependency.

- `graft-core`: canonical artifacts, repo store, revision graph, working copy, op log,
  secrets, diff/rebase/merge/land primitives. No HTTP or UI knowledge.
- `graft-sync`: HTTP-neutral sync frames, reconciliation, capabilities, and ref-batch
  contracts. The CLI and Hub supply transport adapters.
- `graft-cli`: command UX and local orchestration. It may depend on core and sync.
- `graft-hub`: axum binary, hub database, auth/access, sync transport, proposals, issues,
  and embedded SPA. It depends on core and sync; core never depends on it.
- `web`: internal React/Tailwind SPA. It consumes the Hub's internal `/api/` surface.
- `graft-e2e`: black-box v1 acceptance tests. It is never a production dependency.

The secrets decision is later and more specific than the object-model ticket: `Keyring`
is therefore a fifth canonical artifact kind beside Blob, Tree, Revision, and
SecretBundle. It is immutable, content-addressed, opaque to the Hub, and selected by
`secrets/keyring`; it is not a hub row or ref payload. Its signing contract remains blocked
on the decision linked above.

## Shared ownership rules

- Package owners may change only their listed paths plus their crate's manifest and
  focused tests. Do not refactor neighbouring packages during feature work.
- `Cargo.lock`, `pnpm-lock.yaml`, root manifests, CI, and release packaging are serialized
  integration files. The package that needs a change owns that change until it lands.
- A shipped canonical or wire encoding is frozen. Its package must land a normative
  `spec/v1/*.md` document and golden vectors with the codec, in the same change.
- SQLite tables outside the artifact bag are derived or hub-local. Core behaviour must be
  testable after deleting derived rows and running `graft rebuild`.
- All blob paths accept streaming readers/writers. Tests use chunk-limited I/O so an
  accidental whole-file buffer fails without relying on machine memory measurements.
- No package broadens v1: no daemon/FUSE, partial clone, wire deltas, orgs, CI service,
  OAuth/OIDC, SMTP, public API stability, fork flow, or Graft-to-git export.

## Package 01 — Workspace and contract harness

**Owns:** root `Cargo.toml`, toolchain/config files, `crates/*/Cargo.toml` skeletons,
`web/package.json`, `spec/v1/README.md`, shared test-support code, initial CI.

Create the workspace members `graft-core`, `graft-sync`, `graft-cli`, `graft-hub`, and
`graft-e2e`; keep production crates free of cyclic dependencies. Establish one error
style, temp-repo helpers, deterministic clocks/RNG injection for tests, and the commands
that every later package must keep green. Predeclare no architecture beyond the
boundaries above.

**Verify:** empty binaries start and print versions; `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and
the web typecheck/build all run in CI.

## Package 02 — Canonical artifacts and repository store

**Depends on:** 01.

**Owns:** `crates/graft-core/src/artifact/**`, `crates/graft-core/src/store/**`, core
artifact/store migrations and tests, `spec/v1/artifact-format.md`, artifact golden
fixtures.

Implement BLAKE3-256 IDs; frozen codecs for Blob, Tree, Revision, SecretBundle, and
Keyring; the compact binary tree-entry encoding; opaque preservation of unknown artifact
kinds; and the per-repo SQLite bag. Add derived indexes, refs, phantoms, atomic CAS ref
batches, root calculation, garbage collection, and `rebuild` as library operations.
Compression/delta choices remain invisible behind the store API.

Old secret bundles and keyrings remain reachable history under the grow-only/secrets
rules. Proposal refs are ordinary GC roots. Local repositories do not enforce an artifact
size cap.

**Verify:** golden bytes and hash vectors are stable; malformed structural artifacts are
rejected; unknown kinds round-trip byte-for-byte; duplicate insert is idempotent; stale
or partially-invalid CAS batches change no refs; deleting all derived rows then rebuilding
produces the same queries; streamed multi-chunk blobs ingest/materialize correctly.

## Package 03 — Revision graph, trees, conflicts, and rewrites

**Depends on:** 02.

**Owns:** `crates/graft-core/src/graph/**`, `tree/**`, `merge/**`, `rewrite/**`, `diff/**`
and their tests.

Implement Change/Revision lookup, n-ary ancestry, tree construction and traversal,
letter/hex display IDs, jj-style ordered-tree-list conflict algebra, rebase, descendant
rewrite, merge, file diff, and same-change interdiff. A conflict is data: merge and rebase
return revisions; only land policy may reject a conflicted result. Keep predecessor edges
out of synced revisions.

**Verify:** change IDs survive amend/rebase; divergent visible revisions remain
queryable; merge never errors merely because content conflicts; same-change-on-all-sides
simplifies; conflict materialization round-trips after resolution; n-ary imported parents
survive; interdiff compares two revisions of one change and rejects mismatched changes.

## Package 04 — Workspace, snapshotter, branches, and op log

**Depends on:** 03.

**Owns:** `crates/graft-core/src/workspace/**`, `snapshot/**`, `oplog/**`, `refs/**` and
their tests.

Implement init/open, the always-present working Change, pre-command auto-snapshot,
working-copy materialization, ignored metadata, `.env` exclusion with the `graft env`
pointer, branch create/switch/auto-advance, snap, amend, edit, merge, and transactional
undo. Store the op log per workspace, locally only, using the canonical artifact envelope
without placing it in the synced bag.

**Verify:** fresh init starts one empty working Change; every mutating operation has one
undo restoring refs/tree/workspace state; snap advances the active branch and creates a
new Change; amend preserves the parent Change ID and rebases descendants; `.env` never
enters a tree; conflicts appear as markers only in the working copy and disappear from
status when resolved.

## Package 05 — Local CLI surface

**Depends on:** 04.

**Owns:** `crates/graft-cli/src/main.rs`, CLI parsing/rendering, local command modules and
snapshot/black-box tests. Sync, env, import, and propose command modules remain owned by
their later packages.

Wire the ratified local verbs: `init`, `status`, `log`, `desc`, `snap`, `amend`, `edit`,
`branch`, `switch`, `merge`, `undo`, and `rebuild`. Auto-snapshot once at the command
boundary; do not reproduce domain logic in command handlers. Match the CLI transcript's
letter Change IDs, hex Revision IDs, `@` working marker, and conflict language.

**Verify:** focused black-box transcripts cover the day loop, including wrong merge then
undo and older-change edit; command failures are typed/actionable and leave repo state
unchanged; `--help` is snapshot-tested so nouns do not drift.

## Package 06 — End-to-end encrypted environments

**Depends on:** 02, 04, 05.

**Owns:** `crates/graft-core/src/secrets/**`, `crates/graft-cli/src/commands/env/**`,
`spec/v1/secrets-format.md`, secret/keyring golden fixtures and tests.

Implement per-user age-compatible X25519 identity, signed whole-repo Keyring, per-env
key generations and recipient wraps, whole-env SecretBundle encryption, and atomic
keyring+bundle ref moves. Implement `graft env init|ls|set|grant|revoke|rotate|run`.
Joining grants history; removal always rotates; `run` injects values directly into a
child process. No plaintext file/materialization helper is permitted.

The normative format must name the audited AEAD/KDF/signature constructions and byte
domains; there may be no home-grown cryptographic primitive. Keys use restrictive file
permissions and secrets must not enter errors, logs, argv, SQLite, temp files, or the
working tree.

**Verify:** golden vectors decrypt across independently-created readers; tampered bundle,
keyring, signature, or ref chain fails closed; grant unlocks current/history bundles;
revoke rotates and prevents the removed key decrypting the new generation; a marker
secret appears only in the child environment and a filesystem scan finds no plaintext;
loss of all keys produces the explicit unrecoverable error and no escrow path.

## Package 07 — Sync protocol and CLI transport

**Depends on:** 02, 05. Package 06 must land before the final acceptance test, but sync
must treat its artifacts generically.

**Owns:** all of `crates/graft-sync/**`, `crates/graft-cli/src/commands/{clone,pull,push}.rs`,
CLI HTTP adapter, `spec/v1/sync-protocol.md`, wire fixtures and tests.

Implement version/capability handshake; length-prefixed binary `igot`/`gimme`/`file`,
phantom, view, and typed-error frames in zstd bodies; multi-round full-set reconciliation;
direction flags; bearer auth; and all-or-nothing expected-old ref batches. The library is
transport-neutral. The CLI adapter uses `POST /repo/<name>/sync`, resumes at round
boundaries, verifies received hashes and keyrings, and records pull/push in the op log.

Support advertised `max_artifact_bytes`: preflight every outgoing artifact before sending
any payload or ref update. Do not add clusters, lazy blobs, deltas, or artifact-specific
sync logic.

**Verify:** golden frames are stable; two stores with disjoint artifacts converge over
multiple rounds; interruption leaves resumable phantoms and no corrupt object; unknown
capabilities are ignored; hash mismatch fails; stale ref CAS leaves all refs unchanged;
oversize preflight sends zero `file` frames; clone against an empty store yields all
artifacts/views and a fresh working Change.

## Package 08 — Deterministic Git import

**Depends on:** 03, 05.

**Owns:** `crates/graft-core/src/git_import/**`,
`crates/graft-cli/src/commands/import.rs`, import fixtures and tests.

Parse `git fast-export` from a child process into a fresh Graft repo. Define and freeze in
tests the exact Change ID derivation as a domain-separated BLAKE3 hash of the complete raw
Git object ID, truncated to 16 bytes. Import full history, branches, executable/symlink
modes, and n-ary merges; warn and skip submodules; warn and omit tags/signatures. Refuse a
non-fresh destination and expose no incremental mode.

**Verify:** two independent imports of the same SHA-1 fixture produce identical artifact
hash sets and refs; a SHA-256 fixture cannot collide by textual ambiguity; octopus merges
and modes survive; warnings name every omitted path/tag; missing `git` and malformed
streams leave no half-imported repository.

## Package 09 — Hub foundation, auth, access, and sync endpoint

**Depends on:** 02, 06, 07.

**Owns:** `crates/graft-hub/src/main.rs`,
`crates/graft-hub/src/{config,db,auth,access,repos,sync,users}/**`, Hub migrations/fixtures,
Docker/release wrapper owned only when this package lands.

Implement one axum process and data dir, `hub.db`, one core-store DB per repo, config
flags/env twins, first-user-admin registration, open/invite registration, argon2 passwords,
cookie sessions, named hashed PATs with `ro|rw`, user public-key profiles, personal repos,
collaborators, and public/private read rules. Serve sync by adapting the HTTP-neutral
engine; anonymous public pull works, all writes authenticate, and unauthorized private
repos return 404. Hub stores secret artifacts opaquely and never gets an env key.

**Verify:** restart preserves state; first user alone becomes admin under concurrency;
PAT/session revoke is immediate; read/write/owner matrix is exhaustive; private existence
does not leak; public anonymous clone succeeds; sync uses the same repo DB API as local
core; copying the stopped data dir restores a working Hub; configured size caps are
advertised and enforced before artifact insertion.

## Package 10 — Proposals, review, landing, and issues

**Depends on:** 03, 07, 09.

**Owns:** `crates/graft-core/src/land/**`,
`crates/graft-hub/src/{proposals,reviews,issues,api}/**`, related Hub migrations/tests,
`crates/graft-cli/src/commands/propose.rs`.

Implement stack discovery from target to tip, proposal rows and internal
`proposals/<id>` refs, automatic movement by Change ID after pushed rewrites, top-level
comments, revision-anchored inline threads, resolve toggle, per-reviewer approval, cached
interdiff/trial rebase, and whole-stack rebase-and-land. Landing holds the transaction,
refuses any conflicted result, and CAS-advances the target once. Close drops the proposal
ref; reopen restores it; landed proposals never reopen. Add minimal numbered issues with
markdown title/body, flat comments, and open/closed state.

**Verify:** a two-change stack preserves order; amend advances the proposal without an
update command; old inline threads remain anchored/outdated and interdiff is correct;
concurrent target advance makes the first land attempt stale rather than partial; a clean
retry lands one linear stack with stable Change IDs; conflicted trial/land cannot move
target; access rules match write/owner decisions; proposal refs keep revisions alive and
are removed on terminal close/land; issue numbering is per repo.

## Package 11 — Embedded Hub SPA

**Depends on:** 09, 10.

**Owns:** all of `web/**`, `crates/graft-hub/src/web/**`, embedded-asset build integration
and frontend tests. Start from `prototype/hub-ui/`; do not carry its variant switcher or
fake data into production.

Build the ratified GitHub-shaped frame: repo/files and log toggle, Proposal conversation
and change-by-change review with revision/interdiff controls, land verdict/panel, minimal
issues, and metadata-only Environments. Preserve Graft vocabulary and visual distinction:
letter/violet Changes, hex/neutral Revisions, land, keyring, generation. All environment
mutation remains CLI-only. Build static assets into the Rust binary; no SSR or second
server.

**Verify:** component/API tests cover loading, empty, error, unauthorized, stale, and
conflicted-land states; an amend event jumps to the correct interdiff; outdated threads
stay visible; public browse works logged out; environment screens never request or render
secret names/values; `cargo build -p graft-hub --release` produces a binary that serves
the SPA with no adjacent asset directory.

## Package 12 — v1 acceptance, packaging, and handoff closure

**Depends on:** all prior packages.

**Owns:** all of `crates/graft-e2e/**`, final root CI/release files, install/package smoke
tests. Feature fixes remain owned by the package that introduced the defect.

Add one deterministic black-box acceptance test, `v1_journey`, using release-equivalent
CLI and Hub binaries, loopback HTTP, real SQLite files, temp directories, and no mocked
core/sync/auth boundary:

1. Start an open-registration Hub. Alice registers first (admin), Bob registers with an
   X25519 public key; Alice creates a private repo, grants Bob write, and both create PATs.
2. Alice initializes, snaps seed code onto `main`, initializes `dev`, sets a unique marker
   secret, and pushes.
3. Bob clones. He has code but cannot decrypt `dev`. Alice grants Bob after verifying the
   displayed fingerprint and pushes the atomic keyring+bundle refs; Bob pulls and proves
   `graft env run dev -- <probe>` receives the marker while the working tree does not.
4. Bob branches, edits a file, snaps two Changes, pushes, and runs `graft propose` against
   `main`. The Hub reports the ordered two-change stack and holds its internal proposal ref.
5. Alice adds a top-level comment, an inline thread on the first Revision, and approval.
   Bob amends that Change and pushes. The Proposal follows the Change ID, keeps the old
   thread as outdated, and exposes a non-empty interdiff.
6. Alice resolves the thread and lands. The Hub trial-rebases the whole stack, refuses
   conflicts if present, then atomically advances `main`. A fresh clone sees a linear main,
   both stable Change IDs, final file contents, landed state, and no proposal GC ref.
7. Stop the Hub. Byte-scan every Hub/local repo DB, WAL, temp, log, and working-tree file
   for the marker secret; it must be absent. Restart from a copied data dir and repeat a
   clone plus log/read check.

Keep failure-mode coverage focused in owning packages; do not turn the acceptance suite
into a second copy of every unit test.

**Final gate:** `cargo fmt --check`; strict workspace clippy; all Rust tests including
`v1_journey`; web typecheck/tests/build; release builds for `graft` and `graft-hub`; run
the v1 journey against the release binaries. Ship only when the frozen-format and wire
goldens are committed and the repository contains no ignored failing tests.

## Handoff order

Use the numbered order as the default merge order. After Package 02, Packages 03 and 07
may proceed in parallel because their owned paths are disjoint. After Package 05,
Packages 06 and 08 may proceed in parallel. Packages 09–12 are intentionally serial: they
join the core, protocol, CLI, and product surfaces where parallel ownership would create
more integration work than it saves.
