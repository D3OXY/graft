---
id: 20
title: "Freeze the secrets cryptographic contracts"
type: grilling
status: closed
assignee: d3oxy
blocked-by: [19]
---

## Question

Choose and freeze the v1 SecretBundle and Keyring cryptographic contracts from the
research: the user's encryption/signing identity shape; public profile and private-key
storage shape; keyring signature scheme, signer identity, canonical signed bytes, and
domain separation; bundle AEAD/KDF/nonce/envelope; and the exact relationship to native
age recipients. Reconcile the earlier "one X25519 key per user" promise with the later
requirement that every Keyring update is signed, without inventing unaudited crypto.

## Resolution

Freeze the research recommendation in
[`docs/research/secrets-crypto-contracts.md`](../docs/research/secrets-crypto-contracts.md)
as Graft's sole v1 suite. There is no algorithm agility in v1.

**User identity.** One portable, versioned Graft credential contains two independently
generated secrets: a standard age X25519 identity and a 32-byte Ed25519 signing seed.
The public Member Identity contains the `age1...` recipient and 32-byte Ed25519 verifying
key. Its displayed identity is:

```text
member_id = BLAKE3-256(
  "graft/member/v1\0" || u32be(len(age_recipient)) ||
  age_recipient_ascii || ed25519_public
)
```

This preserves the earlier promise: each user still has exactly one X25519 encryption
key; signatures use a separate, purpose-specific key. Never derive, convert, or reuse raw
key material between the two.

The private credential is one Graft-specific plaintext file, created atomically with
owner-only permissions. Passphrase wrapping and OS-keychain integration are deferred.
Secret export must be explicit. A Hub profile may publish the Member Identity for
discovery, but is not a trust anchor; grants use the Keyring copy and retain the existing
out-of-band joint-fingerprint warning.

**Keyring authorization.** Sign canonical unsigned Keyring bytes using pure Ed25519. The
exact signature input is:

```text
"graft/keyring-signature/v1\0" ||
u64be(len(canonical_unsigned_keyring)) ||
canonical_unsigned_keyring
```

The unsigned payload contains the secrets domain ID, predecessor Keyring artifact ID
(absent only at genesis), signer Member ID, every Member Identity, and every Environment
Generation wrap and history-link ciphertext. The signature is the final 64-byte field
outside that payload. Genesis contains its signer and is self-signed. Every update is
verified strictly against the signer's active Member Identity in the exact predecessor,
not a key supplied by the update. Use strict Ed25519 verification and reject weak keys.

Cryptographic transcripts use the research document's canonical binary rules: fixed-width
big-endian integers, length-prefixed variable bytes, maps sorted by exact canonical key
bytes, no Unicode normalization, and rejection of duplicates, unknown required fields,
non-minimal encodings, invalid UTF-8, or bytewise re-encoding differences. Golden bytes
are part of the frozen format.

**Environment Generations.** Each Generation owns a freshly generated native age X25519
Environment Identity and its derived recipient. The signed Keyring stores a complete,
unarmored binary age v1 file encrypting that identity to every active member recipient.
Adding a member re-encrypts the current identity to the new complete recipient set without
changing the Generation. Removing a member creates a fresh Generation, as previously
decided.

Each Generation after genesis also stores a complete age v1 history-link file encrypted
to the current Environment recipient and carrying the preceding Environment Identity.
Thus current access walks backward through history, while an old identity cannot unlock a
new Generation.

**SecretBundle.** Encode the entire secret-name/value map canonically, bind the secrets
domain ID, Environment, Generation, and object purpose inside the plaintext, and encrypt
it as one complete age v1 file to exactly the corresponding Environment recipient. The
outer artifact repeats only the routing domain, Environment, and Generation. Decryption
must authenticate and compare all inner routing fields with the outer artifact and the
accepted Keyring before parsing or releasing any secret bytes.

Complete age files—not copied stanzas—own the file key, X25519 wrapping, header MAC,
HKDF-SHA-256, ChaCha20-Poly1305 STREAM nonce schedule, and framing. Graft defines no AEAD,
KDF, or nonce construction. Use only the maintained Rust `age` crate's public high-level
APIs and retain independent Go-age/rage interoperability vectors.

**Freshness boundary.** Clients persist accepted Keyring and secrets-ref tips and reject
older states or broken predecessor chains. A fresh clone, or a client that loses its
checkpoint, remains TOFU and can be served a valid old prefix by a malicious Hub. Bundles
are not separately signed or sequenced in v1; remembered ref state supplies the same
rollback boundary. State this limitation beside the existing future-only revocation and
fingerprint warnings.

The suite is **auditable**, not claimed to be audited: standard primitives and age v1
replace a Graft-owned composition, while Graft's canonical codec, transition validator,
credential handling, and plaintext lifetime still require focused review and the
adversarial vectors enumerated by the research.
