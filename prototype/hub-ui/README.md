# PROTOTYPE — graft hub UI (throwaway)

Asset for ticket 015 "Prototype the hub UI". Fake data, real React. Three
structurally different takes on the hub's three key screens, sharing one fake
repo (`acme/api` — the same day as the CLI prototype, ticket 011).

```sh
pnpm install
pnpm dev        # then open the printed URL
```

Flip variants with the floating bar or ←/→. Deep-link with
`?variant=a|b|c&screen=repo|proposals|envs`.

## The variants (react to structure, not colors)

- **A — Familiar (GitHub-shaped).** Top nav, repo tabs, PR-style proposal page
  with a Conversation/Changes split and a land sidebar. Graft's model appears
  as vocabulary inside a frame every GitHub user already knows.
- **B — Change-first (the log is home).** The repo's landing surface is the
  log; files are reached by drilling into a change's tree. Review is
  Gerrit-shaped: stack rail → one change at a time → threads column.
  Environments read as a signed keyring audit (membership matrix + timeline).
- **C — Console (dense operator).** Dark, monospace, multi-pane, statusline,
  keyboard hints. Bets that graft users live in terminals and the hub should
  extend that, not translate it.

## What to react to

1. **Home surface**: files-first (A/C) or log-first (B)? What is a repo *page*
   when changes, not commits, are the unit?
2. **Proposal review shape**: PR-with-tabs (A), stack-rail change-at-a-time
   (B), or panes (C)? All three implement change-by-change review with
   revision selectors and interdiff — which makes amend-tracking legible?
3. **Interdiff affordance**: A's "what changed since r1?" pill vs B/C's
   revision chips. Is interdiff a mode or a diff-target choice?
4. **Land panel honesty**: trial-rebase status ("rebases cleanly" / "would
   conflict — hub refuses") wording and placement.
5. **Environments screen**: it can only show keyring metadata (never names,
   values, or counts — ciphertext). Cards+roster (A), audit-first matrix (B),
   or ops table (C)? Does the "opaque" framing read as honest or as broken?
6. **Vocabulary check**: change chips (letters) vs revision chips (hex),
   "land" vs "merge", "keyring", "generation" — anything that needs renaming
   before the spec freezes it?
