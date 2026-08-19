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

Built, awaiting reaction: `prototype/hub-ui/` (also on branch `prototype/hub-ui`) — a
Vite/React/Tailwind app with three structurally different variants of all three screens,
switchable via `?variant=a|b|c` (floating bar, ←/→): **A Familiar** (GitHub-shaped),
**B Change-first** (the log is home; Gerrit-shaped review), **C Console** (dense dark
operator panes). Same fake `acme/api` day as the CLI prototype. Run: `pnpm install &&
pnpm dev`. The README's "What to react to" list is the resolution agenda: home surface,
review shape, interdiff affordance, land-panel honesty, ciphertext-only env screen,
vocabulary.
