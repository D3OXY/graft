---
id: 16
title: "Decide hub architecture"
type: grilling
status: closed
assignee: d3oxy
blocked-by: [10, 12, 13]
---

## Question

The hub's shape: server language (proposal semantics settled that the hub embeds the Rust
core to compute rebase-and-land, which all but decides axum over a TS server — confirm or
find another way to embed), SQLite schema (users/repos/objects/refs/proposals/issues),
auth (tokens, sessions, registration flow for self-hosters), minimal org/team/role model
that the secrets model needs, static frontend serving, and deploy story (single binary?
Docker? Railway template?). Also names: does the hub product carry its own name?

## Resolution

Ratified in two rounds; all thirteen bets landed as recommended. One gap surfaced and
closed here: no prior ticket had decided what a v1 **issue** is.

**Server: axum, core as a crate.** One Rust binary; rebase-and-land calls the same code
the CLI runs. TS-server-plus-FFI and sidecar variants rejected — they reintroduce exactly
the packaging complexity the destination forbids.

**Storage: the hub reuses the core's repo store.** `repos/<owner>/<name>.db` per repo
(the core engine, unmodified — sync, GC, and trial rebase hit existing code) plus one
`hub.db` for everything hub-side. Repo delete/backup is a file operation. Single-big-DB
rejected: it forks the storage code and saves nothing.

**hub.db sketch** (spec detail, not open decisions): `users`, `sessions`, `tokens`,
`repos`, `collaborators`, `proposals` (+ threads, comments, approvals per ticket 13),
`issues`, `issue_comments`.

**Auth.** Email+password (argon2), cookie sessions for the web UI, hub-issued PATs for
CLI/CI. First registered user is admin; `--registration=open|invite` toggle. No email
verification, no SMTP, no OAuth/OIDC in v1 (password reset via admin). **PATs act as the
user** everywhere their account reaches, with a single `rw|ro` scope at creation — named,
hashed at rest, revocable. Fine-grained/per-repo token scoping is post-v1; the scope
column already holds the door open.

**Access model: personal repos + collaborators, no orgs.** Owner grants `read`/`write`;
`write` = push, propose, approve, land, edit issues; owner additionally manages
collaborators, tokens, deletion. A `public` flag makes a repo world-readable:
**anonymous users browse, hit the read API, and pull-sync (clone) without an account**;
all mutation requires one. Private repos 404 (never 403) to non-collaborators — existence
doesn't leak. Secrets need nothing more from the hub: env membership is crypto-enforced.

**Issues: the v1 floor, decided here.** Per-repo numbered issues — markdown title/body,
flat comment timeline, open/closed. No labels, assignees, milestones, or issue↔proposal
linking (mentions render as text). Hub-side rows, like proposals.

**Frontend.** The ratified Vite/React/Tailwind SPA (ticket 15, variant A), built to
static assets and **embedded in the hub binary** (`include_dir`), served next to a
plain-JSON REST API under `/api/`. No SSR, no GraphQL, no second process. The REST API is
**internal and explicitly unstable in v1** — the sync protocol is the hub's only stable
wire; third-party tooling integrates via clone/sync.

**Deploy.** Single static binary per platform; all state under one `--data-dir` (hub.db,
repos/, nothing else) so backup = copy the dir. Thin Dockerfile wrapping the same binary;
Railway template as a nicety. No external services — no Postgres, Redis, or job queue;
trial rebases run in-process via `spawn_blocking`, lazily, cached per (target tip, stack
tip) as ticket 13 fixed. Config is flags + `GRAFT_HUB_*` env twins (`--data-dir`,
`--listen`, `--base-url`, `--registration`); no config file until TLS/SMTP/OIDC arrive
(post-v1).

**Deletion.** Repo delete (owner/admin): removes the repo db file, cascades hub rows —
its proposals and issues go with it, stated plainly in the spec. User delete (admin;
repos deleted/transferred first): comments, proposals, and issues persist attributed to a
ghost "deleted user" — cascading them would punch holes in other people's history.

**Name: no separate product name.** It's **Graft Hub** — one brand to explain; a rebrand
later is cheap, retiring a name is not. (Orchard/Nursery/Rootstock noted as candidates if
it ever grows its own gravity.)

**Multi-tenant door (owed to the map's fog):** nothing here forecloses hosted
multi-tenant — per-repo db files make tenant isolation a directory-layout question, and
auth/roles are already hub-side rows.
