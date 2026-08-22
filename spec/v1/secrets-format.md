# Secrets format v1

Graft secrets use native age v1 binary files for every encrypted payload. Graft does not
implement an AEAD, KDF, nonce schedule, custom age stanza, or key conversion.

## Credential and member identity

The portable credential is `graft-credential-v1\0`, a `u32be`-length-prefixed native
`AGE-SECRET-KEY-...` X25519 identity followed by a 32-byte Ed25519 seed. It is written
atomically with owner-only permissions. The public Member Identity contains the native
`age1...` recipient and Ed25519 verifying key. Its member ID is:

```text
 BLAKE3-256("graft/member/v1\0" || u32be(len(age_recipient)) ||
           age_recipient_ascii || ed25519_public)
```

## Keyring

`Keyring` payloads are canonical `keyring-v1\0` binary records: domain, optional
predecessor artifact ID, signer member ID, members sorted by member ID, environments sorted
by exact UTF-8 name, generation wraps, and history links. The final 64 bytes are the pure
Ed25519 signature over:

```text
"graft/keyring-signature/v1\0" || u64be(len(unsigned_keyring)) || unsigned_keyring
```

Genesis is self-signed. Updates name the exact predecessor and are verified against the
signer's public key in that predecessor. Parsing rejects duplicate/out-of-order records,
weak or malformed keys, non-canonical re-encodings, and invalid routing.

## Environment and bundles

Every environment generation has a fresh native age X25519 identity. Complete age files
wrap that identity to active member recipients. A rotation stores a complete age history
link encrypted to the new generation recipient. A SecretBundle stores only its routing
domain, environment, generation, and one complete age ciphertext in the outer artifact.

The encrypted bundle plaintext is `secret-bundle-v1\0`, routing fields, and a sorted map:

```text
u32be(count) || repeated (u32be(name_len) || name_utf8 || u64be(value_len) || value_bytes)
```

After decryption, inner and outer routing must match before secret values are released.
The CLI accepts portable ASCII environment names and UTF-8, NUL-free values up to 64 KiB;
the complete plaintext bundle is capped at 1 MiB. Superseded ciphertext remains in the
grow-only artifact bag.
