---
id: 21
title: "Decide credential, Hub, and remote bootstrap"
type: grilling
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

What is the complete v1 bootstrap flow that makes the already-ratified CLI, Hub auth,
and secrets identity operable: creating/importing/exporting the portable User Credential,
displaying and publishing its joint Member Identity, associating it with a Hub account,
obtaining and storing a PAT, and recording the default Hub/repo remote used by `clone`,
`pull`, `push`, and `propose`? Freeze the public commands, config locations, confirmation
and fingerprint boundaries, and non-interactive/CI behavior without making the Hub a
secrets trust root or adding a second product noun.

## Resolution

Ratified in two rounds; all twelve bets landed as recommended. No new product noun and no
new domain noun: everything hangs off one `graft auth` command namespace.

**Commands.** `graft auth login|logout|whoami|export|import` — hub auth and credential
verbs share the one namespace; `graft env` stays purely about secrets data. No `keygen`
verb: an explicit generator would invite accidental second identities.

**Credential lifecycle.** Lazy auto-create the first time anything needs it (`env init`,
accepting a grant, first `login`), with a loud one-time notice printing the joint
fingerprint and file location. `import` covers restore/second machines; `export` requires
explicit confirmation (`--yes` in scripts) per 020's "secret export must be explicit".
Exactly **one global credential per OS user** — the same Member Identity across all repos
and all hubs; no per-hub or per-repo identities in v1.

**Storage.** Everything global lives in `~/.graft/` (git/ssh-style, one obvious dir; no
XDG split): `credential` (owner-only, atomic create per 020), `hubs` (PATs keyed by hub
host), `config` (defaults, incl. `default_hub`). Per-repo state (the remote URL) lives in
the repo's `.graft/`.

**Login.** `graft auth login <hub-url>` prints `<hub>/settings/tokens/new`; the user
pastes a PAT. No password prompt in the CLI, no device flow — zero new hub surface.
First login sets `default_hub` (overridable). `logout` deletes the local PAT and prints a
"revoke it in the UI" hint; no server call.

**Publication.** `login` auto-publishes the Member Identity to the hub profile
(discovery-only per 020, never a trust anchor), printing exactly what was published;
`--no-publish` opts out.

**Remotes.** Exactly one remote per repo — "the hub" — recorded by `clone`, managed by
`graft remote [set <url>]`. No named multi-remotes, no `origin` vocabulary; `push`,
`pull`, `propose` need zero routing decisions.

**Hub repo creation.** `graft push --create <name>` creates the repo via the hub's
internal REST API, targeting `default_hub` unless a full URL is given; owner is the
logged-in user. The internal-API instability warning (016) covers third parties, not our
own CLI — CLI and hub ship together. The sync wire stays free of hub concepts.

**Non-interactive/CI.** `GRAFT_TOKEN` overrides the stored PAT; `GRAFT_CREDENTIAL_FILE`
(a path) overrides the credential location. No inline credential-blob env var — process
tables and CI logs leak. When stdin is not a TTY, every prompt hard-fails immediately
with the env-var hint instead of hanging.

**Fingerprint boundaries.** The joint fingerprint prints at credential creation, in
`auth whoami`, and at `login`/publish; `grant` keeps its out-of-band-verify nag and
fresh-clone TOFU warning (011/020). Confirmation is demanded only where secret material
leaves the machine (`export`). Nothing else prompts.
