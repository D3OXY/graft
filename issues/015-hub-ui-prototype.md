---
id: 15
title: "Prototype the hub UI"
type: prototype
status: open
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
bits. Round 2 agenda is the README's "What to react to": stack strip, interdiff
affordance, land-panel tone, ciphertext-only env framing, visual identity, vocabulary.
