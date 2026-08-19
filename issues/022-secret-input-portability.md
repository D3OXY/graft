---
id: 22
title: "Freeze secret input and process-environment portability"
type: grilling
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

What exact plaintext contract does v1 expose for `graft env set` and `graft env run`?
Freeze how values enter without leaking through argv or files, the accepted secret-name
grammar and value byte constraints, overwrite/delete behavior, cross-platform process
environment semantics, and the failure behavior for values the target OS cannot inject.
Keep the encrypted SecretBundle format capable of preserving canonical bytes without
silently promising portability the CLI cannot provide.

## Resolution

All six contracts ratified as recommended:

**Input paths for `set`.** `graft env set <env> NAME` takes the value via hidden prompt on
a TTY or raw stdin when piped — never argv. `NAME=value` in argv is rejected outright with
a pointer to the safe forms (a warning users ignore is not a boundary). One bulk path:
`graft env set <env> --from-dotenv <file>` for the migrating-off-`.env` onboarding flow —
the plaintext file already exists, so the leak is the user's past, not one we create.
Dotenv parsing is a strict literal subset: `NAME=value` lines, `#` comments, blank lines;
anything else (quotes with semantics, `export` prefixes, interpolation, multiline) is
rejected loudly with line numbers rather than silently guessed at.

**Name grammar.** POSIX-strict `[A-Za-z_][A-Za-z0-9_]*`, max 256 chars. Case-sensitive as
stored, but two names differing only by case within one env are rejected at `set` — the
Windows process environment is case-insensitive, and a bundle that cannot round-trip there
is a silent portability lie. No uppercase enforcement. No denylist: `PATH`/`LD_PRELOAD`
are allowed because env members already hold code-execution trust; a denylist is ceremony
pretending to be a boundary.

**Value bytes.** Valid UTF-8, no NUL, enforced at `set` time — portable to every target
platform by construction. The SecretBundle codec still stores length-prefixed raw bytes,
so relaxing this later is a validation change, not a format break. Prompt mode strips the
terminating newline (it is the Enter key); piped stdin is exact bytes, nothing stripped.
Caps: 64 KiB per value, 1 MiB whole-bundle plaintext — anything bigger is a file, not an
env var.

**Overwrite, delete, `ls`.** `set` overwrites without confirmation, printing
`created`/`updated` (env-var mental model; undoable via op log, and old bundles persist
regardless). **`graft env unset <env> NAME`** joins the frozen verb surface (amends the
ticket-11 list); it writes a new bundle minus the name, no rotation — the spec states once
that superseded ciphertext persists, with no per-op nag. `env ls` lists envs plus your
membership; `env ls <env>` decrypts and lists names only.

**`run` composition.** Inherit the parent environment, overlay secrets, secrets win on
collision, silently — that is the point of injection. On Windows a case-insensitive match
counts as the collision. POSIX `exec`-replaces (no middleman, signals just work); Windows
spawns and forwards the exit code. Child's exit code is `run`'s. No `--clean` flag in v1.

**Uninjectable values.** Atomic pre-flight: validate every name and value against the
current platform before spawning; any failure aborts the whole `run` with a nonzero exit,
listing offending *names* (never values). No partial injection, no skip-with-warning — a
process running with half its secrets is the worst outcome. (Reachable only via bundles
written by foreign/future clients, since `set` prevents it locally.)
