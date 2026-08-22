> ⚠️ Warning: It's AI Generated Slop.

# Graft

Graft is a local-first, content-addressed workspace with encrypted environments,
GitHub-shaped review, and a small Hub server.

The implementation handoff is in [`docs/build-handoff.md`](docs/build-handoff.md).
The authoritative end-to-end journey is
[`crates/graft-e2e/tests/v1_journey.rs`](crates/graft-e2e/tests/v1_journey.rs).

## Fastest full test

From the repository root:

```bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm build

cargo build --release -p graft-cli -p graft-hub

GRAFT_E2E_GRAFT_BIN="$PWD/target/release/graft" \
GRAFT_E2E_HUB_BIN="$PWD/target/release/graft-hub" \
cargo test -p graft-e2e --test v1_journey -- --nocapture
```

The test starts a real Hub, registers Alice and Bob, creates PATs, exercises
encrypted environments, clone/pull/push, proposals, review threads, amend,
interdiff, landing, fresh clones, secret-leak scanning, and Hub restart from a
copied data directory.

## Manual local demo

This v1 build uses loopback HTTP. HTTPS remotes are not supported yet.

### Start the Hub

```bash
cd /path/to/graft

DEMO="$(mktemp -d -t graft-demo)"
printf 'Demo directory: %s\n' "$DEMO"

./target/release/graft-hub \
  --data-dir "$DEMO/hub-data" \
  --bind 127.0.0.1:7676 \
  --open-registration
```

Keep that terminal running and open <http://127.0.0.1:7676>.

In the browser Account screen:

1. Register `alice` and create an `rw` PAT named `alice-cli`.
2. Open an incognito window, register `bob`, and create `bob-cli`.
3. Copy both PATs immediately; they are only shown once.

### Alice: create and publish a workspace

Run this in a second terminal. Replace `DEMO` with the path printed above.

```bash
cd /path/to/graft

export DEMO="/path/printed/above"
export HUB="http://127.0.0.1:7676"
export GRAFT_BIN="$PWD/target/release/graft"
export HOME="$DEMO/alice-home"
export GRAFT_CREDENTIAL_FILE="$HOME/.graft/credential"
export ALICE_PAT="paste-alice-pat-here"

printf '%s\n' "$ALICE_PAT" | "$GRAFT_BIN" auth login "$HUB"
"$GRAFT_BIN" auth whoami

"$GRAFT_BIN" init "$DEMO/alice-workspace"
cd "$DEMO/alice-workspace"

printf 'hello from Alice\n' > README.md
"$GRAFT_BIN" snap -m "seed"

"$GRAFT_BIN" env init dev
export MARKER="graft-secret-$(date +%s)"
printf '%s' "$MARKER" | "$GRAFT_BIN" env set dev MARKER
"$GRAFT_BIN" env run dev -- sh -c 'printf "child sees MARKER=%s\n" "$MARKER"'

"$GRAFT_BIN" push --create secret
```

In the Alice browser, set the repository field to `secret`, open Account, and
add Bob as a `read / write` collaborator.

### Bob: clone and receive the environment key

```bash
cd /path/to/graft

export DEMO="/path/printed/above"
export HUB="http://127.0.0.1:7676"
export GRAFT_BIN="$PWD/target/release/graft"
export HOME="$DEMO/bob-home"
export GRAFT_CREDENTIAL_FILE="$HOME/.graft/credential"
export BOB_PAT="paste-bob-pat-here"

printf '%s\n' "$BOB_PAT" | "$GRAFT_BIN" auth login "$HUB"
"$GRAFT_BIN" auth whoami

"$GRAFT_BIN" clone "$HUB/secret" "$DEMO/bob-workspace"
cd "$DEMO/bob-workspace"

# Expected to fail: Bob has the code, but not the environment key.
"$GRAFT_BIN" env run dev -- sh -c 'test -n "$MARKER"'
"$GRAFT_BIN" auth whoami
```

Copy Bob's `age recipient` and `signing public` values into Alice's terminal:

```bash
export BOB_AGE="paste-bobs-age-recipient"
export BOB_SIGNING="paste-bobs-signing-public"

cd "$DEMO/alice-workspace"
"$GRAFT_BIN" env grant dev "$BOB_AGE" "$BOB_SIGNING"
"$GRAFT_BIN" push
```

Back in Bob's terminal:

```bash
cd "$DEMO/bob-workspace"
"$GRAFT_BIN" pull
"$GRAFT_BIN" env run dev -- sh -c 'printf "Bob sees MARKER=%s\n" "$MARKER"'
"$GRAFT_BIN" env ls dev

if rg -a --hidden --glob '!.graft/**' "$MARKER" .; then
  echo "unexpected plaintext leak"
else
  echo "marker absent from working tree"
fi
```

### Bob: open a review

```bash
cd "$DEMO/bob-workspace"
"$GRAFT_BIN" branch dev

printf 'review-one\n' >> README.md
"$GRAFT_BIN" snap -m "first review change"

printf 'review notes\n' > notes.txt
"$GRAFT_BIN" snap -m "second review change"
"$GRAFT_BIN" push
"$GRAFT_BIN" propose --title "Review the dev stack"
```

Note the proposal number printed by `graft propose`, then use Alice's terminal
for the review actions below:

```bash
export ALICE_PAT="paste-alice-pat-here"
export PROPOSAL_ID="1"

DETAIL=$(curl -fsS \
  -H "Authorization: Bearer $ALICE_PAT" \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID")

FIRST_CHANGE=$(printf '%s' "$DETAIL" | python3 -c \
  'import json,sys; print(json.load(sys.stdin)["changes"][0]["change_id"])')
FIRST_REVISION=$(printf '%s' "$DETAIL" | python3 -c \
  'import json,sys; print(json.load(sys.stdin)["changes"][0]["revision"])')

curl -fsS -X POST \
  -H "Authorization: Bearer $ALICE_PAT" \
  -H 'Content-Type: application/json' \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID/comments" \
  -d '{"body":"Please keep the stack focused."}'

curl -fsS -X POST \
  -H "Authorization: Bearer $ALICE_PAT" \
  -H 'Content-Type: application/json' \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID/threads" \
  -d "{\"revision\":\"$FIRST_REVISION\",\"path\":\"README.md\",\"line\":1,\"body\":\"Please explain this change.\"}"

curl -fsS -X POST \
  -H "Authorization: Bearer $ALICE_PAT" \
  -H 'Content-Type: application/json' \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID/approvals" \
  -d '{}'
```

In Bob's terminal, amend the first review Change. Use the `FIRST_CHANGE`
value from above:

```bash
cd "$DEMO/bob-workspace"
export FIRST_CHANGE="paste-the-change-id-from-above"
"$GRAFT_BIN" edit "$FIRST_CHANGE"
printf 'review-amended\n' >> README.md
"$GRAFT_BIN" amend
"$GRAFT_BIN" push
```

Back in Alice's terminal, verify the outdated thread and interdiff, resolve the
thread, and land:

```bash
DETAIL=$(curl -fsS \
  -H "Authorization: Bearer $ALICE_PAT" \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID")
THREAD_ID=$(printf '%s' "$DETAIL" | python3 -c \
  'import json,sys; print(json.load(sys.stdin)["threads"][0]["id"])')

curl -fsS \
  -H "Authorization: Bearer $ALICE_PAT" \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID/interdiff/$FIRST_CHANGE"

curl -fsS -X POST \
  -H "Authorization: Bearer $ALICE_PAT" \
  -H 'Content-Type: application/json' \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID/threads/$THREAD_ID/resolve" \
  -d '{"resolved":true}'

curl -fsS -X POST \
  -H "Authorization: Bearer $ALICE_PAT" \
  "$HUB/api/repos/secret/proposals/$PROPOSAL_ID/land"
```

Open the Proposals tab at <http://127.0.0.1:7676/?repo=secret> to inspect the
ordered stack, comments, outdated thread, interdiff, approval, and landed state.

### Verify landing and restart

After Alice reviews and lands the proposal, clone into a fresh directory:

```bash
cd "$DEMO"
"$GRAFT_BIN" clone "$HUB/secret" "$DEMO/fresh-workspace"
cd "$DEMO/fresh-workspace"

cat README.md
cat notes.txt
"$GRAFT_BIN" log
```

Stop the Hub with `Ctrl-C`, copy its stopped data directory, and restart it:

```bash
cp -R "$DEMO/hub-data" "$DEMO/hub-data-restored"

./target/release/graft-hub \
  --data-dir "$DEMO/hub-data-restored" \
  --bind 127.0.0.1:7676 \
  --open-registration
```

Clone again and confirm the landed files and log survive. Finally scan the demo
directory for the marker:

```bash
if rg -a --hidden "$MARKER" "$DEMO"; then
  echo "secret persisted unexpectedly"
else
  echo "no persisted plaintext secret found"
fi
```

## CLI overview

```text
graft init [path]
graft status | log | undo | rebuild
graft desc -m MESSAGE
graft snap -m MESSAGE
graft amend | edit CHANGE
graft branch [NAME] | switch NAME | merge NAME
graft clone HUB/REPO PATH | pull | push [--create NAME]
graft remote [set HUB/REPO]
graft propose [SOURCE] [--target REF] [--title TITLE]
graft env init|ls|set|unset|grant|revoke|rotate|run
graft auth whoami|login|logout|export|import
```
