---
id: 10
title: "Decide the secrets model"
type: grilling
status: closed
assignee: d3oxy
blocked-by: [8, 9]
---

## Question

How do secrets live as E2E-encrypted repo objects (ticket 3)? Recipient/key model
(per-user keys? team key envelopes?), access granularity (per repo? per environment like
dev/staging/prod? per path?), what the hub stores and enforces vs what cryptography
enforces, rotation and member-removal semantics, and the CLI/hub UX for granting access.
Must answer honestly: what does revoking access mean for already-synced ciphertext?

## Resolution

Adopted the research model (ticket 8) wholesale, then locked the flows:

**Recipient model.** Per-user X25519 keypair (age-compatible), one key per user in v1
(user moves it between machines; keyring lists pubkeys-per-member so multi-device is a
later add, not a format break). Pubkeys live in the repo as auditable objects.

**Scope.** Environment is the only access boundary. Each env: symmetric key + generation
number; env key wrapped per-recipient as age stanzas. Generations chain so one wrap grants
history ("clone = get the env"). No group keys, no Shamir, no path ACLs, no roles — any
env member can grant, remove, and rotate that env (a member can already read everything;
restricting grant is ceremony, not a boundary).

**Bundle = env snapshot.** One Secret Bundle is the entire value set of one env at one
generation, atomically replaced on any change. Secret *names* are encrypted inside;
plaintext routing metadata is env name + generation + keyring reference only.

**Currency = refs, parallel track.** `secrets/<env>` ref points at the current bundle;
`secrets/keyring` at the keyring. Secrets do not participate in revisions, branches,
merges, or conflicts; old bundles are de-facto history with no v1 log/rollback UX.
(Code-coupled secret versioning stays fog.)

**Keyring.** Single artifact for the whole repo (members + per-env generations/wraps),
replaced whole so membership changes are atomic across envs. Every update is signed by an
existing member; the first keyring is self-signed by the repo creator (TOFU root). No
default envs — `secrets init <env>` creates them on demand.

**Rotation.** `rotate <env>` = new generation + re-wrap to current members + re-encrypt
current values, one atomic keyring+bundle replacement. Member removal runs rotation
non-optionally in the same operation. Lost/compromised key = remove + re-grant. No escrow:
all keys lost ⇒ env unrecoverable, re-enter from real sources.

**Hub role.** Crypto is the only secrets boundary. Hub enforces repo-level read/write
only, treats keyring/bundles as opaque bytes, no secrets-aware endpoints; it may display
plaintext keyring metadata (roster, generation bumps) as UI. Spec states the metadata
leak explicitly: hub sees env names, roster, recipient counts, change frequency — never
values or names.

**Honest limits (in spec verbatim).** Revocation is future-only — removed members keep
what they cloned; CLI nags that rotating keys doesn't un-know values, rotating the
underlying credential is the only true revocation. Superseded ciphertext persists forever
in the grow-only bag (accepted; purge = history rewrite, out of scope). Residual TOFU
edge: pubkeys fetched via hub profiles are convenience — grant prints the fingerprint and
tells the granter to verify out-of-band.

**Plaintext-never-in-tree boundary.** Secrets never materialize into the working copy or
any tree artifact (the snapshotter would sweep a `.env` into a public tree). Plaintext
exists only in process env (`run -- <cmd>`) or explicitly requested output. Command names
and ergonomics belong to ticket 11; the boundary is decided here.
