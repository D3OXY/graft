---
id: 15
title: "Prototype the hub UI"
type: prototype
status: closed
assignee: d3oxy
blocked-by: [13]
---

## Question

What do the hub's key screens look like? Throwaway prototype (fake data, real React) of:
repo browse (tree + file + log), proposal review (diff + comments + merge), and the
secrets/environments manager. Reacting to these settles v1 UI scope and vocabulary before
the handoff plan freezes it.

## Asset

`prototype/hub-ui/` (also on branch `prototype/hub-ui`) — a Vite/React/Tailwind app with
three structurally different variants of all three screens, switchable via
`?variant=a|b|c` (floating bar, ←/→). Same fake `acme/api` day as the CLI prototype.
Run: `pnpm install && pnpm dev`.

**Round 1 reaction: variant A (Familiar, GitHub-shaped) won.** A then got a polish pass
(type system, graft-green accent, icons, Rust highlighting, stack strip, wired
conversation → interdiff jump); B (Change-first) and C (Console) stay rough for stealing
bits.

## Resolution

**Variant A ratified after the polish round** (two rounds: A picked over B Change-first
and C Console, then "a lot more polished" applied and approved). What this fixes for v1:

- **Frame: GitHub-shaped.** Top nav, repo tabs (Code / Proposals / Environments),
  files-first repo home with the log one toggle away. Graft's model rides inside a frame
  GitHub hands already know; differentiation comes from vocabulary and the review flow,
  not from novel navigation.
- **Repo browse:** branch picker, tip-change bar, collapsible tree + file view, log as a
  list of changes (letter chips) with current revision (hex chips), branch and
  proposal pills.
- **Proposal page:** stack strip up top (target branch → numbered changes with thread and
  revision counts), Conversation / Changes tabs. Conversation is a typed event timeline;
  amend events carry "view interdiff →" which jumps to the Changes tab with the interdiff
  on. Changes tab reviews change-by-change with a revision selector and a
  "what changed since r1?" interdiff pill. Inline threads anchor to the revision they
  were written on, with an "outdated — written on r1" badge.
- **Land panel:** review checklist (approvals with the revision they approved, open
  thread count) plus a trial-rebase verdict: "rebases cleanly… main stays linear" or
  "hub refuses to land a conflicted result — pull, resolve, amend". One land button.
- **Environments:** metadata-only, framed as a feature. Lock banner stating the hub holds
  ciphertext; env cards (generation badge, member avatars, "████ opaque" contents);
  fingerprint roster with the not-a-trust-root caveat; signed keyring timeline. All
  mutation happens in the CLI.
- **Vocabulary confirmed on-screen:** land (not merge), keyring, generation, proposal,
  change vs revision (letters vs hex, violet vs neutral).
- **Visual identity:** graft-green accent + warm neutrals, violet change chips, IBM Plex
  Sans / JetBrains Mono / Bricolage Grotesque.

The prototype (all three variants) is the primary source for ticket 16's frontend scope
and ticket 17's UI work packages: `prototype/hub-ui/` on main and branch
`prototype/hub-ui`.
