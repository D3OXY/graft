# Secrets cryptographic contracts

Research date: 2026-08-19. Sources are specifications, maintained implementation
documentation/source, and first-party cryptographic-library guidance.

## Recommendation

Freeze one small suite for v1; no algorithm agility:

- A user has **one portable Graft credential containing two independently generated
  secrets**: a standard age X25519 identity and a 32-byte Ed25519 signing seed. The public
  member record contains the corresponding `age1...` recipient and 32-byte Ed25519
  verifying key. “One X25519 key per user” remains true for encryption; it was never a
  signing key.
- Represent each environment generation by a fresh native age X25519 identity. Store a
  complete binary **age v1 file** that encrypts that identity to the active users, encrypt
  the whole SecretBundle to the environment recipient, and encrypt the previous
  generation identity to the current environment recipient. This uses age end-to-end and
  makes current-to-history traversal one-way.
- Sign a canonical unsigned Keyring with pure Ed25519. Every non-genesis Keyring names its
  predecessor artifact ID; its signer must be an active member in that predecessor. The
  genesis Keyring is self-signed.
- Put the repo secrets domain, environment, generation, and object purpose inside every
  age-encrypted plaintext and require an exact match with outer routing metadata after
  decryption. The age format then owns the AEAD, KDF, nonce, and envelope contract.
- Reject non-canonical encodings, duplicate records, bad signatures, broken predecessor
  chains, wrong routing metadata, and AEAD failures before releasing plaintext.

This is the smallest v1 that has ordinary, reviewable pieces. It adds a signing key, but
does not add a new public-key construction, key conversion, custom age stanza, or custom
symmetric-encryption protocol. The one semantic adjustment is precise but real: an
“environment key” becomes a shared native age X25519 identity rather than a long-lived
symmetric master. Age still encrypts every payload with a fresh symmetric file key. If
the earlier noun **must** remain literal, the fallback contract later in this note retains
a 32-byte symmetric environment master at the cost of a Graft-owned AEAD composition.

“Auditable” is the defensible claim, not “audited”: age v1 is a public specification with
independent Go and Rust implementations, and the primitives are standard, but no primary
source found establishes that this Graft composition—or the current Rust age release—has
received a dedicated third-party audit.

## Why the age identity cannot sign

An age X25519 identity is 32 random bytes; its recipient is `X25519(identity, basepoint)`.
Its stanza performs ephemeral-static X25519, HKDF-SHA-256, and ChaCha20-Poly1305. The age
spec defines no signing operation or sender signature
([C2SP age, X25519 recipient](https://age-encryption.org/v1#the-x25519-recipient-type)).
RFC 7748 defines X25519 for scalar multiplication and Diffie-Hellman key agreement, not as
a signature scheme ([RFC 7748, Sections 5 and 6](https://www.rfc-editor.org/rfc/rfc7748.html#section-5)).
Anyone who knows a public age recipient can create a valid age ciphertext to it; age's
AEAD authenticates knowledge of the file key, not a named sender. The age author describes
this distinction and the multi-recipient forgery issue explicitly
([age and Authenticated Encryption](https://words.filippo.io/age-authentication/)).

Therefore a Keyring signature cannot be inferred from, or verified against, the native
age recipient. Graft needs a signing public key in each member record.

## Identity shapes

| Shape | Safe enough? | Cost / consequence |
|---|---|---|
| Two independent random keys in one Graft credential | **Recommended** | One file to move; two cryptographic keys; direct age *recipient and ciphertext* interop. Compromise of the file still compromises both purposes, but there is no mathematical key reuse. |
| Two independent files: standard age identity + signing seed | Yes | Best direct private-file interop with `age`/`rage`; worse “one thing to move” UX. |
| One uniform master seed, HKDF-derived age and Ed25519 seeds | Plausible, not needed in v1 | HKDF supports application-specific `info` for domain separation ([RFC 5869](https://www.rfc-editor.org/rfc/rfc5869.html#section-3.2)), but derivation labels, seed encoding, recovery, and rotation become a permanent Graft contract. The Rust `age` public API generates or parses its own identity, so deriving raw age bytes also adds adapter code. |
| Convert Ed25519 to X25519 | Do not choose | libsodium implements the conversion but still “highly recommends” distinct encryption and signing keys, partly because their lifetimes differ ([libsodium Ed25519-to-Curve25519 guidance](https://doc.libsodium.org/advanced/ed25519-curve25519)). It also makes encryption identity rotation inseparable from signing rotation. |
| Reuse the same raw 32 bytes as both inputs | No | Neither age nor RFC 8032 specifies this cross-protocol construction. It has no purpose separation and creates a format Graft alone must defend. |
| Reuse an SSH Ed25519 key through age's SSH recipient | Not this contract | It abandons the already-selected native age X25519 recipient. The Rust age docs recommend native X25519 over SSH compatibility recipients and note that SSH recipients are not anonymous ([`age::ssh`](https://docs.rs/age/latest/age/ssh/)). |

The recommended private container is Graft-specific, e.g. a versioned file carrying the
standard `AGE-SECRET-KEY-...` string plus an independently generated Ed25519 seed. Do not
hide the signing seed in an age identity-file comment: comments are an interoperability
convenience, not a secret-container contract. If direct `age -i` use matters, keep two
files or provide an explicit `graft identity export-age` command. At rest, create the file
atomically with owner-only permissions; optional passphrase/OS-keychain protection is a UX
choice, not part of repo cryptography.

Define the displayed member fingerprint from both public keys, not the hub account:

```text
member_id = BLAKE3-256(
  "graft/member/v1\0" || u32be(len(age_recipient)) || age_recipient_ascii || ed25519_public
)
```

This makes the out-of-band fingerprint cover the actual encryption-and-signing tuple.

## Exact recommended v1 contract

### Common encoding rules

All cryptographic transcripts use bytes, not JSON objects or reserialized maps:

- integers are fixed-width unsigned big-endian;
- variable fields are `u32be(length) || bytes` (ciphertext may use `u64be` if artifacts can
  exceed 4 GiB);
- text is valid UTF-8 but sorted and compared by its exact UTF-8 bytes; do not apply Unicode
  normalization;
- maps are encoded as a count followed by entries sorted by canonical key bytes;
- duplicates, out-of-order entries, unknown required fields, non-minimal integers, and
  invalid UTF-8 are errors;
- the artifact parser must re-encode and compare byte-for-byte before verifying. There is
  exactly one canonical byte string for a semantic Keyring or SecretBundle.

These rules can sit behind the already-selected canonical text-card artifact envelope.
The crucial contract is that `canonical_unsigned_keyring` is the exact normative encoding
with the signature record absent—not “whatever serde emitted.” Golden byte fixtures are
part of the format.

### Member keys and signatures

Use pure Ed25519 from RFC 8032: 32-byte seed, 32-byte public key, 64-byte signature. RFC
8032 specifies the 64-byte `R || S` encoding and recommends Ed25519 where its roughly
128-bit classical security level is sufficient
([RFC 8032, Sections 5.1.6 and 8.5](https://www.rfc-editor.org/rfc/rfc8032.html#section-5.1.6)).
Do not use Ed25519ph, Ed25519ctx, a BLAKE3 prehash, or a generic “sign hash” helper.

The signed message is exactly:

```text
keyring_signature_input =
  "graft/keyring-signature/v1\0" ||
  u64be(len(canonical_unsigned_keyring)) ||
  canonical_unsigned_keyring
```

The unsigned payload includes `secrets_domain_id`, `previous_keyring_id` (absent only at
genesis), the signer `member_id`, every member's two public keys, and every environment,
generation, age wrap, and history-link ciphertext. Thus the signature cannot be moved to
another object type, repo secrets domain, signer, membership set, wrap set, or predecessor.
The signature is a final 64-byte field outside the unsigned payload.

Validation is stateful:

1. Parse canonically, reconstruct the signature input, and strictly verify the signature.
2. Genesis must have no predecessor, contain its signer as a member, and be self-signed.
3. An update must name the exact BLAKE3-256 artifact ID of its predecessor; the signing key
   is taken from that predecessor's active member record, not trusted from the new object.
4. Validate all generation/membership transition invariants, then accept the new tip.

Use `ed25519-dalek::VerifyingKey::verify_strict`, reject weak public keys when they enter a
Keyring, and never enable `legacy_compatibility`, batch verification, or `hazmat` signing.
The maintained crate documents weak-key forgeries and notes that ordinary `verify` and
batch verification do not provide the same weak-key rejection
([ed25519-dalek validation criteria](https://docs.rs/crate/ed25519-dalek/latest#validation-criteria)).

### Environment generations and recipient wraps

Each environment generation is a fresh standard age X25519 identity:

```text
env_identity[g] = age::x25519::Identity::generate()
env_recipient[g] = env_identity[g].to_public()
```

The signed Keyring stores one complete, unarmored binary age v1 file per environment's
current generation:

```text
member_wrap[g] = age_v1_encrypt(
  recipients = every active member.age_recipient,
  plaintext = encode("graft/env-identity/v1", secrets_domain_id, env, g,
                     env_identity[g])
)
```

This file naturally contains one native X25519 stanza per member; it is the concrete form
of the earlier “wrapped per recipient” decision. Add-member re-encrypts the same current
environment identity to the new complete recipient set and signs a new Keyring; it does
not touch bundles. Remove-member generates `g + 1`, as already required.

Standard age files use a fresh 128-bit symmetric file key, authenticate the header, derive
the payload key with HKDF-SHA-256, and encrypt the payload with ChaCha20-Poly1305
([C2SP age encrypted-file format](https://age-encryption.org/v1#encrypted-file-format)).
The Rust age crate's high-level API produces files compatible with `rage` and the reference
Go implementation ([age crate documentation](https://docs.rs/age/latest/age/)). Full files
are therefore preferable to copied bare stanzas: Graft does not own age's file key, header
MAC, nonce schedule, parser, or private types.

The Keyring signature remains essential. Public member recipients let anyone construct an
age ciphertext; the signature says which exact member-wrap file and recipient set an
existing member authorized.

### Generation-history link

For each `g > 0`, the signed Keyring stores:

```text
previous_key_link[g] = age_v1_encrypt(
  recipients = [env_recipient[g]],
  plaintext = encode("graft/env-history-link/v1", secrets_domain_id, env,
                     current_generation = g, previous_generation = g - 1,
                     env_identity[g - 1])
)
```

A newly granted member decrypts `member_wrap[g]`, obtains the current environment identity,
then walks the standard age links backward. A removed member has only an older identity
and cannot decrypt a link addressed to the new recipient. The encrypted context is checked
at each hop, preventing a valid age file from being accepted for the wrong domain,
environment, or generation.

### Whole-environment SecretBundle

Canonical secret-map bytes are one whole snapshot:

```text
u32be(entry_count) ||
  repeated in ascending raw name-byte order {
    u32be(name_len) || name_utf8 ||
    u64be(value_len) || value_bytes
  }
```

Reject duplicate names. Whether v1 restricts names to the portable environment-variable
grammar and values to NUL-free UTF-8 is a CLI/platform decision; the encrypted format can
preserve bytes without weakening the cryptographic contract.

The bundle's plaintext and artifact envelope are exact:

```text
bundle_plaintext = encode(
  "graft/secret-bundle/v1",
  secrets_domain_id,
  env,
  generation,
  canonical_secret_map
)

age_file = age_v1_encrypt(
  recipients = [env_recipient[generation]],  // exactly one
  plaintext = bundle_plaintext
)

SecretBundle artifact = encode(
  secrets_domain_id, env, generation, age_file
)
```

On decrypt, compare every encrypted routing field to the outer canonical artifact and the
accepted Keyring before releasing or parsing secret values. Exactly one environment
recipient is used. Secret names and values stay inside the age payload. Age specifies the
whole AEAD/KDF/nonce envelope: a new CSPRNG file key and 16-byte payload nonce per file,
HKDF-SHA-256 payload-key derivation, and ChaCha20-Poly1305 STREAM chunks with counter/final
nonces ([C2SP payload](https://age-encryption.org/v1#payload)). No Graft AEAD mode or nonce
rule remains to invent.

### Fallback if “symmetric environment key” must remain literal

Use a fresh 32-byte symmetric master per generation and wrap it with the same complete
age file used for `member_wrap`. Do **not** manipulate raw age stanzas. For each bundle,
generate a fresh 32-byte salt, derive a single-use key with
`HKDF-SHA-256(master, salt, "graft/secret-bundle-key/v1" || routing_context)`, then use
RFC 8439 ChaCha20-Poly1305 with the fixed all-zero 12-byte nonce and the exact canonical
outer routing bytes as AAD. Use a distinct HKDF `info` label for history-link boxes.

The zero nonce is safe only because every salt produces a new single-use key; duplicate
salt must fail closed. This mirrors age's use of a derived one-time key and fixed nonce,
but it is still a new Graft protocol requiring vectors and external review. HKDF binds
output to application `info` ([RFC 5869](https://www.rfc-editor.org/rfc/rfc5869.html));
RFC 8439 requires ChaCha20-Poly1305 nonces to be unique per key and recommends deterministic
nonce construction ([RFC 8439, Section 2.8](https://www.rfc-editor.org/rfc/rfc8439.html#section-2.8)).
This fallback is smaller and more standards-conservative than XChaCha while meeting nonce
uniqueness by construction.

## Rust implementation constraints

- **`age` 0.12.x:** use only the public high-level X25519 identity/recipient and full-file
  encryption APIs. The crate says it implements age v1 and interoperates with `rage` and
  the reference Go implementation, but also cautions that all pre-1.0 versions are beta
  ([age crate docs](https://docs.rs/age/latest/age/)). Pin the selected minor/patch, disable
  unused SSH/plugin/armor features, and keep independent Go-age/rage golden vectors.
- **`ed25519-dalek` 2.2.x or 3.x:** pure Rust, forbids unsafe code at the crate level,
  zeroizes signing keys by default, and provides strict verification. Version 3 requires
  Rust 1.85 and was released in July 2026; version 2 supports Rust 1.60. Prefer 2.2 unless
  3.x satisfies the repo's minimum-release-age policy when implementation begins. Its
  compatibility table and safety notes are the source of truth
  ([ed25519-dalek crate docs](https://docs.rs/crate/ed25519-dalek/latest)). Do not use
  versions before 2.0; their keypair API was affected by a signing-oracle flaw fixed in 2.0
  ([RustSec RUSTSEC-2022-0093](https://rustsec.org/advisories/RUSTSEC-2022-0093.html)).
- The recommended design needs no direct AEAD or HKDF dependency. If the literal-symmetric
  fallback is selected, RustCrypto `chacha20poly1305` reports a historical NCC Group audit
  with no significant findings, and `hkdf` implements RFC 5869; that audit is not a blanket
  audit of later releases or of Graft's composition
  ([ChaCha20Poly1305 security notes](https://docs.rs/crate/chacha20poly1305/latest#security-notes),
  [`hkdf` docs](https://docs.rs/crate/hkdf/latest)).
- Do not depend directly on `x25519-dalek` for v1. Let `age` own age parsing, contributory
  checks, HKDF labels, and stanza rules. The age spec requires rejecting all-zero shared
  secrets and malformed/non-canonical X25519 stanzas
  ([C2SP X25519 stanza rules](https://age-encryption.org/v1#x25519-recipient-stanza)).
- Keep secrets in zeroizing wrappers, inject deterministic RNG only in tests, and make
  production generation use the OS CSPRNG. No secret-bearing type should implement
  `Debug`, `Display`, serde, logging, or generic error conversion.

Version pinning is not a substitute for review. The composition, canonical codecs,
transition validator, file permissions, and plaintext lifetime are Graft code and need
their own focused audit.

## User tradeoffs the freeze ticket still owns

1. **One Graft file vs direct private-key interoperability.** One container gives the
   promised portable credential; two files let `age -i` consume the private identity
   directly. Public `age1...` recipients and age-wrapped generation keys interoperate in
   either case.
2. **Full age files vs literal bare stanzas.** Full files are the recommendation: more
   bytes, much smaller protocol surface. Bare stanzas preserve the earlier wording but
   require Graft to own more age framing/API coupling.
3. **Environment-key primitive.** A native age identity makes the complete system age v1
   and removes custom AEAD/KDF/nonce code; a literal symmetric master preserves the earlier
   noun but creates the fallback composition above. This is the main decision still owed
   by the freeze ticket.
4. **Rollback expectations.** A signed predecessor chain lets a client with a remembered
   tip reject rollback and splicing. It cannot stop a malicious hub serving a valid old
   prefix to a fresh clone or a client that lost its checkpoint; genesis remains TOFU.
   State this beside the existing out-of-band fingerprint warning.
5. **Bundle freshness inside a generation.** AEAD proves the writer knew the environment
   key, not that the hub served the newest snapshot. If same-generation edits need signed
   freshness, either put the current bundle artifact ID in each signed Keyring (signing on
   every edit) or add a signed predecessor/sequence to bundles. The smallest v1 can rely
   on remembered ref state and accept fresh-clone TOFU, but the spec must say so.
6. **Private-container protection.** Owner-only plaintext file, OS keychain, or
   passphrase-encrypted portable file changes recovery and automation UX. It does not
   change repo-object cryptography; do not improvise a password KDF inside this format.

## Required golden/adversarial vectors

- independent Go `age` or `rage` decrypts a Graft-generated generation wrap, and Graft
  decrypts one generated independently;
- exact initial/update signature bytes, with failures for field reordering, alternate
  encodings, signer substitution, predecessor substitution, cross-domain replay, weak
  Ed25519 keys, and a non-canonical signature;
- generation `g` unlocks every prior key but no prior key unlocks `g`;
- bundle vectors cover empty and non-ASCII data, deterministic canonical ordering, wrong
  encrypted env/generation/domain context, age-file corruption, duplicate names,
  truncation, and no plaintext release on failure;
- remembered-tip verification rejects a valid signed rollback, while the fresh-clone test
  documents the unavoidable TOFU behavior.
