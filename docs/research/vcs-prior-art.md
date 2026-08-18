# VCS Prior Art: jj, Pijul, Fossil, Sapling

Research for ticket 007. All claims sourced from primary material (official docs, design
docs, author statements); links inline per claim. Judged against ticket 002's resolution:
ship a snapshot VCS now, with an object model that does not foreclose a history-native
substrate growing underneath later.

Note: `jj-vcs.github.io/jj/latest/...` now 301-redirects to `docs.jj-vcs.dev`; links below
use the canonical target.

---

## Jujutsu (jj)

### Change ID vs commit ID

jj splits identity in two. A **commit** is "a snapshot of the files in the repository at a
given point in time (technically a tree object), together with some metadata"; its
**commit ID** is the content hash (20 bytes on the Git backend). A **change** is "a commit
as it evolves over time. Changes themselves don't exist as an object in the data model;
only the change ID does" — a ~16-byte, typically randomly generated identifier.
([glossary](https://docs.jj-vcs.dev/latest/glossary/))

Consequences of "the change is not an object":

- Rewriting (amend/rebase) produces a new commit carrying the same change ID; the old
  commit becomes **hidden** but stays "accessible by its commit ID or the combination of
  its change ID and a change offset". ([glossary](https://docs.jj-vcs.dev/latest/glossary/))
- Two visible commits with one change ID is a **divergent change** — a displayed state,
  not an error: "the command will succeed and e.g. `jj log` will indicate that the change
  has diverged". ([glossary](https://docs.jj-vcs.dev/latest/glossary/),
  [operation log](https://docs.jj-vcs.dev/latest/operation-log/))
- On the Git backend, change IDs and predecessor lists are extra metadata in a
  `StackedTable` under `.jj/repo/store/extra/`; foreign Git commits get "the bit-reversed
  commit ID as change ID" and empty predecessors — i.e. the scheme degrades gracefully
  over a store that knows nothing about it.
  ([architecture](https://docs.jj-vcs.dev/latest/technical/architecture/))

### Rewrite/evolution mechanics

"Whenever you modify a commit, any descendants of the old commit will be rebased onto the
new commit... Bookmarks pointing to rebased commits will be updated."
([README](https://github.com/jj-vcs/jj/blob/main/README.md)) This auto-rebase is what makes
stable change IDs pay off: mid-stack edits propagate without manual `rebase --onto`.

Repo state is versioned by an **operation log**: each operation stores "a snapshot of the
visible commits and bookmarks at a given point in time (technically a view object)" plus
metadata; operations form a DAG like commits do. This yields `jj undo` / `op restore`, and
"lock-free concurrency — you can run concurrent `jj` commands without corrupting the repo,
even if you run the commands on different machines"; concurrent edits surface as divergence
rather than corruption. ([glossary](https://docs.jj-vcs.dev/latest/glossary/),
[operation log](https://docs.jj-vcs.dev/latest/operation-log/))

### First-class conflicts

Conflicts live in commits, not in the working copy: "If an operation results in conflicts,
information about those conflicts will be recorded in the commit(s). The operation will
succeed. You can then resolve the conflicts later."
([README](https://github.com/jj-vcs/jj/blob/main/README.md))

Representation: a conflicted commit links "an ordered list of tree objects... (instead of
the usual single tree per commit)". The list is interpreted algebraically — trees
A, B, C, D, E resolve as `A+(C-B)+(E-D)`; a plain 3-way merge of sides A, C over base B is
`A+(C-B)`. The algebra simplifies: rebasing a conflicted commit `C+(B-A)` onto D yields
`D+((C+(B-A))-C)` which flattens to `D+(B-A)` — conflicts don't compound across rebases.
Materialization to conflict markers happens only at the working-copy boundary, and markers
are parsed back into the structured form on snapshot. Known tradeoff: when all sides make
the same change jj "automatically consider[s] it resolved to that value" (matching
Git/Mercurial, unlike Darcs), trading algebraic purity for practicality.
([conflicts design doc](https://docs.jj-vcs.dev/latest/technical/conflicts/),
[working copy](https://docs.jj-vcs.dev/latest/working-copy/))

### Working-copy-as-commit

"Jujutsu uses a real commit to represent the working copy."
([README](https://github.com/jj-vcs/jj/blob/main/README.md)) "Jujutsu will automatically
create commits from the working-copy contents when they have changed" — every command
snapshots first (recorded as an operation), mutates in memory, then updates the working
directory. No staging area; "added files are implicitly tracked by default". One
working-copy commit per workspace. ([working copy](https://docs.jj-vcs.dev/latest/working-copy/),
[glossary](https://docs.jj-vcs.dev/latest/glossary/))

---

## Pijul

### Patch theory

Repository state is a directed graph: "vertices are uniquely identified, by the hash of the
change that introduced them, along with a position in that change"; edges carry the change
that introduced them and a status label. Only two primitive actions exist — add vertices
with edges, or relabel existing edges (e.g. mark deleted) — so the store is append-only,
and dependencies are structural: "If a change c deletes a vertex, or maps an existing edge
introduced by a change d, then c must depend on d."
([manual: theory](https://pijul.org/manual/theory.html))

This makes the state a CRDT: "we're just adding vertices and edges to a graph, or mapping
edge labels which we know exist because of dependencies" — application order can't change
the result. ([manual: theory](https://pijul.org/manual/theory.html))

### Associative/commutative merges

Pijul's pitch: Git's 3-way merge is not associative — merging the same changes via
different intermediate states can "shuffle lines around", silently producing a different
result than reviewing the changes separately would suggest. Pijul merges "according to
formal axioms"; "for any two changes A and B, either A and B can be applied in any order,
or A depends on B, or B depends on A" — so cherry-picking is just applying a change, and
maintainers can "pull only the changes they are interested in" without rewriting them. It
also claims to fix Darcs's "exponential merge problem" (hours to merge a two-line change).
([manual: why pijul](https://pijul.org/manual/why_pijul.html))

Conflicts are fully modeled states of the graph (disconnected alive vertices, cycles,
"zombie" vertices with both alive and dead incoming edges), the "internal representation of
repositories fully models conflicts", and "a conflict resolution is always a change" —
resolution itself merges like anything else.
([manual: theory](https://pijul.org/manual/theory.html),
[manual: conflicts](https://pijul.org/manual/conflicts.html))

### Adoption friction

Primary/author-adjacent evidence, mostly from the author's own comments in the
["Towards 1.0" HN thread](https://news.ycombinator.com/item?id=25032956) and Pijul's posts:

- **Format churn**: the Nov 2020 alpha was a ground-up rewrite of formats and algorithms;
  format stability was only promised at the
  [1.0 beta (Jan 2022)](https://pijul.org/posts/2022-01-08-beta/). Years of "will break
  again" kept people out.
- **Hosting as bottleneck**: the Nest (only real Pijul host) was "still experimental",
  slow, and intermittently down by the author's own account; it needed major rewrites for
  each format change and later a
  [full new direction](https://pijul.org/posts/2023-05-23-nest-a-new-hope/). No
  network-effect host, no adoption.
- **Onboarding friction**: contemporaneous reports of broken installs
  (`cargo install` dependency failures), 404ing repo links, and a rename to "Anu" and back
  that confused documentation and hosting.
- **Single-maintainer surface**: theory, libpijul, its custom KV store (Sanakirja), and
  the Nest were all one person's stack; each rewrite stalled everything else.

Lesson: the theory was never the problem. The cost of making patches the *primary* store —
custom storage engine, custom host, unstable formats — is what burned a decade.

---

## Fossil

### Everything-in-SQLite (actually: everything-is-an-artifact)

The load-bearing design is not SQLite; it is the artifact set. "The global state of a
fossil repository is an unordered set of artifacts" — immutable blobs named by SHA1/SHA3-256
of their content. Structural artifacts are UTF-8 "cards" in strict lexicographic order, and
eight types cover the whole product surface: manifests (check-ins), clusters, control
artifacts (tags), wiki pages, ticket changes, attachments, technical notes, forum posts.
Tickets and wiki are just more content-addressed artifacts in the same bag.
([fileformat](https://fossil-scm.org/home/doc/trunk/www/fileformat.wiki))

SQLite is explicitly a cache: "Fossil is not based on SQLite. The current implementation
... uses SQLite as a local store for the content of the distributed database and as a
cache." "Everything in the relational tables can be recomputed from the artifacts, and in
fact that is exactly what happens when one runs the `fossil rebuild` command." The artifact
format is "intended to be enduring"; the storage layer is not. So the relational schema can
evolve freely while the canonical format stays frozen, and SQL gives cheap timelines,
ticket reports, etc. as derived indexes. Single-file DB + atomic transactions gives crash
durability for free. ([theory1](https://fossil-scm.org/home/doc/trunk/www/theory1.wiki),
[tech overview](https://fossil-scm.org/home/doc/trunk/www/tech_overview.wiki),
[fileformat](https://fossil-scm.org/home/doc/trunk/www/fileformat.wiki))

### Sync protocol

Because state is a grow-only set of hash-named artifacts, sync is pure set reconciliation —
Fossil's docs call it a CRDT (grow-only set). Wire protocol: line-oriented cards over HTTP
POST (zlib-compressed): `igot` ("I have this artifact", bidirectional), `gimme` (request
content), `file` (payload, optionally delta-encoded against another artifact), `login`
(nonce/signature auth). **Clusters** — artifacts that list other artifacts' hashes, minted
server-side when the "unclustered" set exceeds ~100 — compress the enumeration
hierarchically. **Phantoms** — hashes known via `igot`/references but whose content is
missing — drive the next round's `gimme`s. Rounds repeat (replies capped ~1MB) until
neither side holds phantoms the other can fill. Push, pull, and sync are the same
conversation with direction flags. ([sync](https://fossil-scm.org/home/doc/trunk/www/sync.wiki))

Notable: the protocol never mentions commits, branches, or DAG negotiation. Any new
artifact type (ticket, wiki, forum post) syncs with zero protocol changes.

---

## Sapling

### Scale posture

Built for "large monorepos that have tens of millions of files and commits" by going lazy
on every axis, in historical order: on-demand file fetching (remotefilelog, 2013),
on-demand trees (2017), server infra (Mononoke, 2017), segmented changelog (2020),
on-demand commit fetching (2021), plus EdenFS virtual filesystem and Watchman so status is
"O(files-you-have-changed)". It is deliberately client-server rather than fully
distributed: commit cloud auto-backs-up commits; the server holds full indexes.
([scale overview](https://sapling-scm.com/docs/scale/overview/),
[axes of scale](https://sapling-scm.com/docs/scale/axes/))

### Segmented changelog

History shape is separated from commit metadata: "When you first clone a repository, no
commit messages are downloaded." The segmented changelog encodes the whole commit graph's
*shape* "in just a few megabytes" and answers graph queries (ancestry, common ancestor)
"in O(number-of-merges) time" — milliseconds on a monorepo. Metadata and even hash↔id
resolution are filled in lazily from the server.
([axes of scale](https://sapling-scm.com/docs/scale/axes/),
[scale overview](https://sapling-scm.com/docs/scale/overview/))

### Mutation & visibility (vs Mercurial obsstore)

Sapling inverts Mercurial's visibility default: commits are invisible unless reachable from
explicit visible heads/bookmarks (like Git). Rewrites are **mutation records**, and "If all
successors of a mutation are invisible, then the mutation is ignored. This means mutation
can be implicitly tracked by visibility." Phases are inferred from remote bookmarks instead
of tracked explicitly. Storage is IndexedLog (append-only log + O(log N) indexes, no full
loads) with MetaLog for atomic multi-file state updates.
([internal differences from hg](https://sapling-scm.com/docs/dev/internals/internal-difference-hg/),
[indexedlog](https://sapling-scm.com/docs/dev/internals/indexedlog/))

### Push at scale

Server-side rebase kills push races: "the server actually takes your commit and rebases it
to be on the top of bookmark you are pushing to."
([axes of scale](https://sapling-scm.com/docs/scale/axes/))

---

## Recommendations for Graft

Yardstick per ticket 002: does the choice ship a clean snapshot VCS now, and does it leave
room for a history-native substrate to grow underneath without a format break?

### (a) Change identity — adopt jj's split, wholesale

- **Change ID**: random 16-byte ID minted at change creation; *not* an object, just an
  identity that commits carry (jj glossary model). **Commit ID**: content hash of the
  snapshot object. Evolution = new commit, same change ID, with an explicit predecessor
  list.
- **Divergence is a state, not an error** (jj): two visible commits sharing a change ID
  render as "divergent"; ops never fail because of it. This is what makes lock-free
  concurrent edits and later multi-device sync sane.
- Take Sapling's lesson for GC: visibility-first (only heads in the current view are
  visible; hidden commits linger until GC), and mutation/predecessor records that can be
  ignored once successors are invisible — this keeps evolution metadata from growing
  unbounded.
- **Substrate check**: change ID is exactly the stable entity key a history-native layer
  needs — provenance, per-change discussion, proposals all hang off change ID while
  content hashes stay free to churn. Random IDs (vs hash-derived) mean identity never
  depends on content, so the substrate can re-encode content without breaking identity.
  Also adopt jj's operation log (views as snapshots of refs+heads): it is itself a small
  history-native substrate and gives undo for free.

### (b) Tree/blob encoding — Fossil's two-layer discipline, git-shaped trees

- **Canonical layer**: content-addressed immutable artifacts with a frozen, minimal byte
  format (Fossil: "the artifact format is intended to be enduring; the storage is not").
  Snapshot trees git-style: blob = file bytes, tree = hash-addressed list of entries,
  change/commit = tree hash + parents + change ID + metadata. Hash-addressed subtrees mean
  lazy/partial tree fetch (Sapling's on-demand trees) needs no format change later — a
  client can hold a tree hash without holding its children.
- **Storage layer**: single SQLite file per repo, Fossil-style — artifacts table
  (compressed, delta-encoded) plus derived relational tables that are *rebuildable from
  artifacts by definition* (`rebuild` must exist from day one). This buys atomicity, crash
  safety, cheap queries for hub features (timelines, proposals, secrets index), and
  freedom to reshape the schema without a repo-format version bump.
- **Do not** make Pijul's graph-of-lines the canonical encoding. It welds the core format
  to text-line semantics and demands a bespoke storage engine — the exact stack whose
  rewrites stalled Pijul for years. Snapshots stay primary; patches are derivable views.
- **Substrate check**: this is the decision that most protects the substrate. A frozen
  artifact bag + derived-relational cache means the future history-native layer is "new
  artifact types + new derived tables", not a migration. Fossil proved the pattern holds
  for a decade-plus.

### (c) Conflict representation — first-class, jj's tree-list algebra

- Store conflicts **in the commit**: an ordered list of trees interpreted as
  `A+(C-B)+(E-D)` (jj's conflicts design). Merge/rebase always succeeds; resolution is
  deferred and itself recorded. Auto-simplify the algebra on rebase (`D+(B-A)`), so
  conflicts never compound.
- Markers are a **rendering** at the working-copy boundary only, parsed back to the
  structured form on snapshot (jj working-copy doc). The object model never contains
  marker text.
- Accept jj's same-change rule (identical change on all sides = resolved) — pragmatism
  over Darcs-style purity.
- Pijul shows the maximal version (conflicts as graph states, resolution-as-change,
  order-independent merges) but only by making patch theory the core model — rejected in
  (b). jj's encoding delivers the user-visible wins (never-blocked operations, conflicts
  travel with history, resolvable at leisure) at snapshot cost.
- **Substrate check**: a conflicted commit is ordinary data — multiple tree references in
  one artifact. A future substrate can reinterpret or enrich it (e.g. compute Pijul-style
  precision lazily) without format change. Marker-based conflicts (git) would foreclose
  this; first-class is strictly more substrate-friendly.

### (d) Sync negotiation — Fossil-style artifact-set reconciliation

- Model the repo as a **grow-only set of content-addressed artifacts** and make sync set
  reconciliation, not git-style haves/wants over the commit DAG. Wire sketch per Fossil:
  `igot`/`gimme`/`file` cards over HTTPS, batched rounds with a size cap, **clusters**
  (artifacts listing artifact hashes) to compress enumeration, **phantoms** (known hash,
  missing content) to drive subsequent rounds, deltas in `file` payloads.
- Why this over git's negotiation: it syncs *any* artifact type unchanged. Graft already
  plans non-commit repo objects (E2E-encrypted secrets, proposals, hub metadata); with
  set reconciliation they ride the same protocol with zero extensions — exactly how Fossil
  ships tickets+wiki sync for free. Git's protocol would need a sidecar per object type.
- Layer a small **view exchange** on top (bookmarks/heads/change-ID tips — jj's view
  object) since set membership alone doesn't convey refs; keep it a separate, versioned
  message so ref semantics can evolve independently of transfer.
- Phantoms are latent laziness: a client may simply *choose not to* `gimme` old blobs —
  shallow/partial clone falls out of the model (Sapling's lazy-everything, and Pijul's
  "postpone downloading deleted content", without new machinery). Adopt Sapling's
  client-server honesty for the hub: the hub may hold indexes (e.g. a segmented-changelog-
  style graph index) clients fill in lazily; that's a derived-table concern, not a wire-
  format concern.
- **Substrate check**: the substrate will add artifact types (fine-grained history, ops,
  discussion). Set reconciliation is the only one of the surveyed protocols that is
  already closed under "add a new artifact type" — the sync layer never needs to know what
  the substrate grew.

### One-line synthesis

jj's identity + conflict algebra on top of Fossil's artifact-bag + derived-SQLite storage
and set-reconciliation sync, with Sapling's laziness kept reachable and Pijul kept as a
lesson: steal the semantics, never the substrate-first bet.
