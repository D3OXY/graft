---
id: 8
title: "Research: E2E team secrets prior art (age, sops, Doppler, Infisical)"
type: research
status: closed
assignee: d3oxy
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

## Resolution

Findings: `docs/research/secrets-prior-art.md` on branch `research/secrets-prior-art`
(269548c). Recommended model for the secrets-model ticket:

- **Identity**: each user holds an X25519 keypair (age-compatible); pubkeys live in the
  repo as auditable objects.
- **Scope**: environment is the access boundary (dev/staging/prod), each env with its own
  symmetric key + generation number. Repo-wide access falls out free; path/folder ACLs
  explicitly yagni.
- **Wrapping**: env keys wrapped per-recipient (age X25519 stanzas); keyring + ciphertext
  are plain repo objects — hub syncs opaque bytes. No group keys, no Shamir in v1.
- **Add member**: re-wrap only (sops-updatekeys-style); generations chain Keybase-style so
  one wrap grants history — "clone = get the env" holds.
- **Remove member / lost device**: automatic non-optional rotation — new generation,
  re-wrap to remaining members, re-encrypt current values in the same commit (secrets are
  tiny, so this is cheap).
- **Honest limits**: revocation is future-only (removed members keep what they cloned —
  1Password admits the same); hub still sees metadata (env names, recipient counts,
  generation bumps, change frequency); malicious-hub key substitution is possible until
  membership changes are member-signed. UX must nag: rotating keys doesn't un-know values —
  rotating the underlying credential is the only true revocation.

All claims cite primary sources (C2SP age spec, getsops docs, Doppler/Infisical docs,
Keybase book, 1Password Security Design white paper).
