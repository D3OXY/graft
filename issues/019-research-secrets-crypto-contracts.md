---
id: 19
title: "Research an auditable secrets cryptographic contract"
type: research
status: closed
assignee: d3oxy
blocked-by: []
---

## Question

Using primary specifications and maintained Rust implementations, determine the smallest
audited construction that satisfies Graft's already-locked secrets semantics: an
age-compatible X25519 recipient identity, member-authenticated Keyring updates, per-env
key generations, and whole-environment SecretBundle encryption. Establish whether one
portable user secret can safely support encryption and signing; compare the viable key
shapes; specify suitable AEAD/KDF/nonce/signature primitives and canonical signed bytes;
and identify library/interoperability constraints the final v1 format decision must own.

## Resolution

Research captured in
[`docs/research/secrets-crypto-contracts.md`](../docs/research/secrets-crypto-contracts.md).

Native age X25519 identities cannot sign. The smallest auditable v1 shape is one portable
Graft credential containing two independently generated keys: native age X25519 for
recipient encryption and Ed25519 for member signatures. Pure Ed25519 signs canonical,
domain-separated, predecessor-linked Keyring bytes, verified against the previous
Keyring's member record.

The strongest simplification is to use complete standard age v1 files end-to-end: wrap a
fresh per-generation age identity to members, encrypt SecretBundles to that environment
recipient, and encrypt history links backward. This leaves AEAD/KDF/nonces to age rather
than a Graft-owned composition. The decision ticket must choose this small semantic shift
from a literal symmetric env master, plus credential-file interoperability, rollback and
freshness expectations, and private-key storage policy.
