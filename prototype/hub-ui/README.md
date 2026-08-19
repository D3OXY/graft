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

## The variants

**A won the first reaction round** and got a polish pass: IBM Plex Sans / JetBrains
Mono / Bricolage Grotesque, a graft-green accent, icons, a mini Rust highlighter,
collapsible tree, a stack strip on the proposal page, and a working
conversation → interdiff jump. B and C stay rough, kept for stealing bits.

- **A — Familiar (GitHub-shaped, polished).** Top nav, repo tabs, PR-style
  proposal page with a Conversation/Changes split and a land sidebar. Graft's
  model appears as vocabulary inside a frame every GitHub user already knows.
- **B — Change-first (the log is home).** The repo's landing surface is the
  log; files are reached by drilling into a change's tree. Review is
  Gerrit-shaped: stack rail → one change at a time → threads column.
  Environments read as a signed keyring audit (membership matrix + timeline).
- **C — Console (dense operator).** Dark, monospace, multi-pane, statusline,
  keyboard hints. Bets that graft users live in terminals and the hub should
  extend that, not translate it.

## What to react to (round 2, on polished A)

1. **Stack strip**: the proposal page leads with the numbered stack next to
   the target branch. Does that make "a proposal is a stack" land, or is it
   noise above the tabs?
2. **Interdiff affordance**: "view interdiff →" on amend events jumps to the
   Changes tab with the interdiff on; the pill reads "what changed since r1?".
   Right verb, right place?
3. **Land panel**: checklist tone ("rebases cleanly… main stays linear" /
   "hub refuses to land a conflicted result"). Honest enough? Too chatty?
4. **Environments**: lock-icon banner + "████ opaque" contents line + signed
   keyring timeline. Does ciphertext-only read as a feature here?
5. **Identity**: graft-green + violet change chips + letter-vs-hex distinction.
   Distinct enough from GitHub, or still a skin?
6. **Vocabulary check**: "land", "keyring", "generation", "revisions" chips —
   anything to rename before the spec freezes it?
