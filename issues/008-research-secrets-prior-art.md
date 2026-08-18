---
id: 8
title: "Research: E2E team secrets prior art (age, sops, Doppler, Infisical)"
type: research
status: open
assignee: d3oxy (research agent running)
blocked-by: []
---

## Question

How do existing systems share encrypted secrets across a team, and what must Graft's model
adopt? Specifically: age recipient model and sops' key-group re-encryption workflow;
how Doppler/Infisical model environments, scoping, and access control (what UX bar Graft
must clear); Keybase/1Password-style team key hierarchies; and the hard problem — key
rotation when a member is removed (what "remove access" can and cannot mean for previously
cloned history). Output: findings doc with a recommended recipient/rotation model for
secrets-as-repo-objects (ticket 3), including its honest limits.
