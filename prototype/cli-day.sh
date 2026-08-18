#!/usr/bin/env bash
# PROTOTYPE — throwaway. Asset for ticket 011 "Design the CLI surface".
# A fake day with the graft CLI. Nothing is real: every output line is hand-written
# to make the proposed command surface concrete enough to react to.
#
#   Run:  ./cli-day.sh          Enter advances one command at a time; q quits.
#         ./cli-day.sh --fast   plays the whole day with no pauses.

set -u

FAST=0
[[ "${1:-}" == "--fast" ]] && FAST=1

BOLD=$'\e[1m'; DIM=$'\e[2m'; CYN=$'\e[36m'; GRN=$'\e[32m'; YLW=$'\e[33m'; RST=$'\e[0m'
PROMPT="${CYN}~/dev${RST} ▸ "

pause() {
  (( FAST )) && return
  local key
  IFS= read -rs -n1 key
  [[ "$key" == "q" ]] && { printf '\n'; exit 0; }
}

type_cmd() { # prints the command with a typing effect; dims a trailing "  # comment"
  local s="$1" comment=""
  if [[ "$s" == *"  #"* ]]; then comment="  #${s#*  #}"; s="${s%%  #*}"; fi
  if (( FAST )); then
    printf '%s' "${BOLD}${s}${RST}"
  else
    local i
    for ((i = 0; i < ${#s}; i++)); do
      printf '%s' "${BOLD}${s:i:1}${RST}"
      sleep 0.012
    done
  fi
  [[ -n "$comment" ]] && printf '%s' "${DIM}${comment}${RST}"
  printf '\n'
}

run() { # run "command"  — fake output arrives on stdin (heredoc)
  pause
  printf '\n%s' "$PROMPT"
  type_cmd "$1"
  cat
}

scene() { # scene "title" "narration"
  printf '\n\n%s─── %s ───%s\n' "${BOLD}${YLW}" "$1" "$RST"
  [[ -n "${2:-}" ]] && printf '%s%s%s\n' "$DIM" "$2" "$RST"
}

printf '%s\n' "${BOLD}graft — a day in the life${RST}"
printf '%s\n' "${DIM}Fake transcript, real proposed UX. Enter advances, q quits.${RST}"

# ──────────────────────────────────────────────────────────────────────────────
scene "9:04 — clone and orient" \
"Clone pulls artifacts + refs, verifies the keyring, and tells you which
environments you can already read. A working change starts immediately."

run "graft clone https://hub.acme.dev/acme/api" <<'OUT'
fetching acme/api
  artifacts: 18,204 received (34.1 MiB, 2.1s)
  refs: main + 3 branches · secrets: dev staging prod
keyring verified (12 members, last update signed by @mia)
  you are a member of: dev, staging
workspace ready at ./api — working change started on main
OUT

PROMPT="${CYN}api${RST} ▸ "

run "graft status" <<'OUT'
working change  mqkosuwt  (empty, no message)  on main
parent          xtpolzn 8f3a2c  "rate limiter: clamp burst window"  @mia, 2h ago
nothing changed yet
OUT

run "graft log" <<'OUT'
@  mqkosuwt  ishan   now     (working change)
○  xtpolzn   mia     2h      rate limiter: clamp burst window        main
○  rwqnves   devon   1d      invoices: idempotency keys
○  kplomzu   mia     2d      invoices: schema for line items
OUT

# ──────────────────────────────────────────────────────────────────────────────
scene "9:20 — a quick fix, no staging area" \
"There is no add/stage step. The working change snapshots itself before every
command. 'snap' seals it with a message and starts a fresh one on top —
and moves the branch you're on."

run "vim src/limiter.rs  # fix the off-by-one" <<'OUT'
OUT

run "graft status" <<'OUT'
working change  mqkosuwt  (no message)  on main
  modified  src/limiter.rs
  ignored   .env   (plaintext secrets never enter trees — see `graft env`)
OUT

run "graft snap -m \"limiter: fix burst off-by-one\"" <<'OUT'
snapped mqkosuwt → revision 91d4e0
main moved to mqkosuwt
new working change tzunmyk (empty) on main
OUT

# ──────────────────────────────────────────────────────────────────────────────
scene "10:00 — feature branch" \
"'branch' creates and switches. 'amend' folds working-copy edits into the
parent change — same change ID, new revision. Descendants rebase themselves."

run "graft branch payments" <<'OUT'
branch payments created at tzunmyk (switched)
OUT

run "vim src/webhooks.rs" <<'OUT'
OUT

run "graft snap -m \"payments: add invoice webhook\"" <<'OUT'
snapped tzunmyk → revision b02f77
payments moved to tzunmyk
new working change znvatko (empty) on payments
OUT

run "vim src/webhooks.rs  # forgot the retry header" <<'OUT'
OUT

run "graft amend" <<'OUT'
amended tzunmyk "payments: add invoice webhook"  (b02f77 → c7e1b9)
descendants rebased: none
OUT

# ──────────────────────────────────────────────────────────────────────────────
scene "11:30 — merge, wrong branch, undo, conflict" \
"Merge never fails. A conflict is recorded inside the revision and everything
stays usable — you resolve it whenever. And every operation, merges included,
is one 'undo' away."

run "graft pull" <<'OUT'
fetched 142 artifacts from hub.acme.dev/acme/api
main: mqkosuwt → povwkil  (2 new changes from @mia)
OUT

run "graft merge release-2.4  # oops — wrong branch" <<'OUT'
merged release-2.4 into working change znvatko
  conflict recorded in src/limiter.rs — nothing is blocked
OUT

run "graft undo" <<'OUT'
undid operation 4821: merge release-2.4
working change restored: znvatko on payments (clean)
OUT

run "graft merge main" <<'OUT'
merged main into working change znvatko
  conflict recorded in src/limiter.rs — nothing is blocked
OUT

run "graft status" <<'OUT'
working change  znvatko  (conflict)  on payments
  merging  payments + main
  conflicted  src/limiter.rs
conflicts are saved state: you can snap, switch away, or resolve now
OUT

run "cat src/limiter.rs  # markers exist only in the working copy" <<'OUT'
    ...
<<<<<<< payments (yours)
    let burst = cfg.burst.min(WINDOW - 1);
||||||| base
    let burst = cfg.burst.min(WINDOW);
=======
    let burst = clamp_burst(cfg.burst, WINDOW);
>>>>>>> main
    ...
OUT

run "vim src/limiter.rs  # keep clamp_burst, apply the -1 inside it" <<'OUT'
OUT

run "graft status" <<'OUT'
working change  znvatko  on payments
  resolved  src/limiter.rs  (markers gone — conflict cleared)
OUT

run "graft snap -m \"merge main\"" <<'OUT'
snapped znvatko → revision 5d90ab
payments moved to znvatko
new working change wqzenrt (empty) on payments
OUT

# ──────────────────────────────────────────────────────────────────────────────
scene "14:00 — secrets live in the repo" \
"One noun: 'graft env'. Values are E2E-encrypted repo artifacts; the hub only
ever relays ciphertext. Plaintext exists in exactly one place: the process
env of a command you run."

run "graft env ls" <<'OUT'
env       secrets  generation  members
dev       12       gen 3       you + 11
staging   9        gen 2       you + 4
prod      14       gen 5       (not a member)
OUT

run "graft env set staging SENDGRID_KEY=SG.9fkq…" <<'OUT'
staging: 1 secret updated → new bundle (gen 2), encrypted for 5 members
the hub will only ever see ciphertext
OUT

run "graft env grant staging mia" <<'OUT'
fetched @mia's public key from hub.acme.dev
  fingerprint: age1qm7…7xk4
  verify this with Mia out-of-band — the hub is not a trust root
grant staging to @mia? [y/N] y
keyring updated (signed by you) — @mia can read staging, history included
OUT

run "graft env run dev -- pnpm test" <<'OUT'
[dev: 12 secrets injected into process env]
 ✓ 148 tests passed (3.2s)
OUT

run "graft env revoke staging devon" <<'OUT'
revoking rotates: staging → gen 3, re-encrypted for its 5 remaining members
note: @devon keeps every value they already synced.
      if this was a trust boundary, rotate the real credentials too.
OUT

# ──────────────────────────────────────────────────────────────────────────────
scene "17:45 — ship it" \
"'push' sends artifacts + refs (ciphertext bundles ride along like anything
else). 'propose' opens the hub's PR-equivalent — semantics still provisional,
its ticket is next on the map."

run "graft push" <<'OUT'
pushing to hub.acme.dev/acme/api
  artifacts: 37 sent (incl. staging bundle + keyring — ciphertext)
  payments → znvatko · main → mqkosuwt
OUT

run "graft propose" <<'OUT'
proposal opened: "payments: add invoice webhook"  (2 changes)
  https://hub.acme.dev/acme/api/proposals/218
OUT

run "graft log" <<'OUT'
@  wqzenrt   ishan   now     (working change)
○  znvatko   ishan   3h      merge main                              payments
├─○  povwkil mia     6h      rate limiter: burst metrics             main
○  tzunmyk   ishan   7h      payments: add invoice webhook
○  mqkosuwt  ishan   8h      limiter: fix burst off-by-one
OUT

# ──────────────────────────────────────────────────────────────────────────────
pause
printf '\n\n%s─── end of day — what to react to ───%s\n' "${BOLD}${GRN}" "$RST"
cat <<'REACT'

 1. Verbs: snap (seal + start fresh), amend (fold into parent), plus edit
    <change> for rewriting older changes. Alternatives: commit/save/squash.
 2. Branch auto-advance: snapping on a branch moves the branch (git instinct;
    jj makes you move bookmarks by hand). Right call?
 3. Merge never fails; a conflict is committed state, markers only appear in
    the working copy, and status flips to "resolved" when markers are gone.
 4. graft undo covers every operation — merges and pulls included.
 5. Secrets: one noun (graft env), `run --` as the only plaintext path, the
    out-of-band fingerprint nag on grant, forced rotation + honest nag on revoke.
 6. push = artifacts + refs in one go; propose opens the hub thing (name and
    semantics pending the proposal-semantics ticket).
 7. Display language: change IDs are letters (mqkosuwt), revisions are hex
    (c7e1b9), @ marks the working change. Binary is graft, no short alias.

REACT
