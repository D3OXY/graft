# Research: E2E team-secrets prior art

Resolves issue #8. Context: issue #3 decided secrets are E2E-encrypted repo objects — they
travel with the repo, the hub is dumb sync and never sees plaintext. This doc surveys how
existing systems handle the three problems that decision leaves open: **key distribution**
(who can decrypt), **access granularity** (what scopes exist), and **rotation** (what
happens when someone leaves). All claims cite primary sources: specs, official docs, or the
vendor's security white paper.

---

## 1. age — the recipient/stanza model

Source: [C2SP age spec](https://github.com/C2SP/C2SP/blob/main/age.md) (canonical at
[age-encryption.org/v1](https://age-encryption.org/v1)); implementation
[FiloSottile/age](https://github.com/FiloSottile/age).

- **One file key per file.** "A 128-bit symmetric *file key*" that "MUST be generated as 16
  bytes of CSPRNG output" and "MUST NOT be reused across multiple files."
- **Multi-recipient = wrap the same key N times.** "Each recipient stanza wraps the same
  *file key* independently. Identity implementations are provided the full set of stanzas
  and recognize those addressed to them from their arguments." Any single recipient can
  decrypt; adding a recipient is one more stanza, not another ciphertext.
- **X25519 stanza mechanics.** Per stanza: fresh ephemeral X25519 secret ("A new ephemeral
  secret MUST be generated for each stanza and each file"), shared secret via DH with the
  recipient key, then
  `wrap key = HKDF-SHA-256(ikm = shared secret, salt = ephemeral share || recipient, info = "age-encryption.org/v1/X25519")`,
  and the file key is wrapped with ChaCha20-Poly1305 (fixed all-zero nonce, safe because
  the wrap key is single-use).
- **Header MAC.** The header (including all stanzas) is authenticated with
  `HMAC-SHA-256` keyed by `HKDF-SHA-256(ikm = file key, info = "header")`. Note the
  limit: anyone who holds the file key can recompute the MAC, so it protects against
  outsiders tampering with the recipient list, not against a legitimate recipient.
- **Recipient anonymity is a deliberate default.** Stanzas for X25519 do not identify
  their recipient — you trial-decrypt. The spec warns that recipient-identifying tags
  (as in the `p256tag`/`mlkem768p256tag` types) "sacrifice any chance of ciphertext
  anonymity and unlinkability." What always leaks: the *number* of stanzas, i.e. the
  recipient count.

**Takeaway for Graft:** the recipient model is the right primitive for "wrap one symmetric
key to N public keys." It is deliberately minimal: no groups, no revocation, no rotation —
those are the caller's problem, which is exactly the gap sops/Keybase/1Password fill.

## 2. sops — key groups, updatekeys, rotate

Sources: [key management](https://getsops.io/docs/usage/key-management/),
[key groups](https://getsops.io/docs/usage/identities/key-groups/),
[common operations](https://getsops.io/docs/usage/common-operations/).

- **One data key per file**, encrypted to every configured master key (KMS, age, PGP…);
  the wrapped copies live in the file's `sops` metadata block. Saving after adding keys
  means "SOPS will update its metadata and encrypt the data key with the freshly added
  master keys."
- **Key groups = Shamir over the data key.** "Key groups allow to require multiple
  identities together to edit or decrypt a file": the data key is split with Shamir's
  Secret Sharing so each group holds a fragment (any key in the group recovers that
  group's fragment), and `shamir_threshold` fragments reconstruct the data key. Default
  threshold = number of groups.
- **Two distinct maintenance operations — this distinction matters:**
  - `sops updatekeys` re-wraps the *existing* data key to match the recipients declared
    in `.sops.yaml`. It adds/removes wrapped copies; it does **not** change the data key.
  - `sops rotate` (or `-r`) "causes sops to reencrypt the file with a new data key, which
    is then encrypted with the various … master keys defined in the file."
- **Removal requires both.** The docs are explicit that removed key owners may have had
  access to the data key in the past — so the sequence after removing someone is
  `updatekeys` (drop their wrap) *then* `rotate` (new data key + re-encrypt values), or
  the removed party can still decrypt anything encrypted under the old data key.

**Takeaway for Graft:** sops names the two operations every E2E secrets system needs:
*re-wrap* (membership change, cheap, same key) and *rotate* (new key + re-encrypt, the only
thing that actually cuts off a removed member going forward). Graft should collapse the
footgun: removal must imply rotate automatically.

## 3. Doppler — the UX bar for structure

Sources: [workplace structure](https://docs.doppler.com/docs/workplace-structure),
[configs](https://docs.doppler.com/docs/root-configs),
[branch configs](https://docs.doppler.com/docs/branch-configs).

- **Hierarchy: Workplace → Projects → Environments → Configs.** A project is "a
  one-to-one mapping to a specific application or service that needs secrets"; projects
  default to three environments (`dev`, `stg`, `prd`), each with a **root config**
  holding that environment's master secret list.
- **Branch configs** inherit from their root config and override per-deployment values;
  "when a root config secret is updated, it will automatically update the secret in any
  Branch Configs stemming from that root config."
- **Access control is environment-scoped:** "Access control for Environments impacts
  access to all Branch Configs under it. So, if you give a user access to the `dev`
  Environment, they'll have access to all Branch Configs under that Root Config."
  Service tokens are scoped to a single config
  ([service tokens](https://docs.doppler.com/docs/service-tokens)).
- Quality-of-life features users expect: cross-environment diff / "missing secret"
  detection warning when a key exists in one environment but not another.
- Doppler is a **server-side** secrets manager — access control is enforced by their
  service, not by cryptography. That is precisely the category issue #3 wants to kill for
  Graft users, but its *shape* (project/environment, inheritance, per-environment access)
  is the usability bar.

## 4. Infisical — paths, environments, granular RBAC

Sources: [projects](https://infisical.com/docs/documentation/platform/project),
[folders](https://infisical.com/docs/documentation/platform/folder),
[access control overview](https://infisical.com/docs/documentation/platform/access-controls/overview),
[RBAC](https://infisical.com/docs/documentation/platform/access-controls/role-based-access-controls).

- **Hierarchy: Organization → Projects → Environments → folders (paths) → secrets.**
  "Infisical Folders can be infinitely nested to mirror your application architecture";
  a Secrets Overview page compares the same folder path across environments.
- **Access control:** "Project access is strictly scoped: only members of a project can
  view or manage its resources." Built-in project roles (Admin / Member / Viewer /
  No Access) plus Enterprise custom roles that restrict access to "specific secrets,
  folders, and environments"; "permissions are additive, not intersected." On top:
  additional privileges, temporary (timed) access, ABAC, approval workflows, audit logs.
- Also server-enforced, not E2E-cryptographic, in its standard deployment.

**Takeaway for Graft (3+4):** the market-standard mental model is
**project → environment → key/value secrets**, with per-environment access as the coarse
knob and path/folder scoping as the enterprise long tail. Graft's repo *is* the project;
environment is the one scope users will actually reach for. Folder-level ACLs are
enterprise gravy — not needed to clear the bar.

## 5. Keybase teams — per-team keys and rotate-on-remove

Source: [Keybase book: teams crypto](https://book.keybase.io/docs/teams/crypto).

- **Per-team key (PTK):** a 32-byte random seed; HMAC-SHA512 with fixed labels derives an
  EdDSA signing key, a Curve25519 DH key, and a symmetric SecretBox key
  (`"Keybase-Derived-Team-NaCl-EdDSA-1"` / `…-DH-1` / `…-SecretBox-1`).
- **Distribution:** "Whenever a new user is added, *sᵢ* is encrypted for the user's
  public PUK DH key" (PUK = per-user key, itself synced across the user's devices).
- **Mandatory rotation on membership change:** when "a team member revokes a device, or a
  team member is reset, or a user leaves or is removed from a team, the PTK must rotate."
  "The new PTK keys are encrypted for all remaining members, and the new public key
  halves are written into the team's sigchain."
- **Generational chaining:** "the previous seed *sᵢ* is encrypted with *cᵢ₊₁* via NaCl's
  SecretBox" — each generation wraps the previous one, so holding the newest seed
  unlocks all history. New members can read old data; removed members lose *future*
  generations only.
- **Lazy re-encryption:** "When the key is rotated, new chats are encrypted with the new
  key, but old chats aren't reencrypted." (KBFS likewise.) Rotation protects future
  writes; it does not claw back the past.

## 6. 1Password — vault keys and the honest appendix

Source: [1Password Security Design white paper](https://1passwordstatic.com/files/security/1password-white-paper.pdf)
(section numbers from the current edition).

- **Vault key model (§5):** each vault has a client-generated 256-bit AES-GCM vault key
  that encrypts every item in the vault. "If you have access to a vault, a copy of the
  vault key is encrypted with your public key." The private key is wrapped by keys
  derived from account password + Secret Key (2SKD), so "AgileBits has no ability to
  decrypt your or your team's data" (§3.3).
- **Sharing (§6):** "Once a vault has been created, it can be securely shared by
  encrypting the vault key with the recipient's public key." "The 1Password server never
  has a copy of the decrypted vault key … an attack on our server could not result in
  unwanted sharing."
- **Groups & recovery (§12):** groups have key sets too; "a vault is held by a group if
  the vault key is encrypted with the group's public key." The Recovery Group holds a
  wrapped copy of every vault key — powerful, and flagged as such (§A.2).
- **Revocation — the honest part (§A.4 "Limited re-encryption secrecy"):**
  > "Removing someone from a vault, group, or team isn't cryptographically enforced.
  > Cryptographic keys are not changed."
  Removal deletes the member's wrapped vault-key copy from the server and the server
  stops serving them vault data. But: "If prior to being removed from a vault the person
  makes a copy of the vault key which they store locally, they will be able to decrypt
  all future data if they find a way to obtain the encrypted vault data" (Story 10).
  The documented mitigation (§A.4.2): "create a new vault (which will have a new key),
  and move items from the old vault to the new one."
- **Related caveats:** password changes don't change keysets, only the wrapping key
  (§A.5). There is "no practical method for a user to verify the public key they're
  encrypting data to belongs to their intended recipient" — a malicious server could MITM
  public keys (§A.3). Server stores cleartext metadata: team names, user names and
  emails, group names, device info, IPs (§15.2).

## 7. The hard truth: what "revoke" can and cannot mean

Once ciphertext has been cloned, no protocol can un-know it. Every system above lands on
the same three-layer answer:

1. **Policy revocation (immediate, weakest):** the server stops serving new ciphertext to
   the removed member (1Password §A.4.1; Doppler/Infisical are *only* this layer). In
   Graft's model the hub can and should do this, but it is advisory: the repo may already
   be cloned everywhere.
2. **Lazy rotation (the real mechanism):** mint a new key on removal, wrap it to the
   remaining members only, encrypt all *future* writes under it (Keybase PTK rotation;
   sops `updatekeys` + `rotate`; 1Password's "make a new vault" mitigation). The removed
   member keeps everything up to the rotation point — that is unavoidable, since they
   could legitimately read it while they were a member.
3. **Re-encrypt-on-rotate (bounded benefit):** additionally re-encrypt *current* values
   under the new key (sops `rotate` does this; Keybase deliberately doesn't for chat
   history). This only protects copies fetched *after* rotation — it does nothing about
   ciphertext cloned before. Its real value: after rotation + re-encryption, the ex-member's
   stale key opens nothing that is still being distributed, and secrets that were rotated
   *at the value level* (new DB password, new API token) are safe even if the old
   ciphertext leaks.

Forward secrecy in the messaging sense (ephemeral keys, Signal-style ratchets) is
**incompatible with the product**: "clone the repo, get the env" requires that current
members can decrypt state they didn't witness being written, so keys must be long-lived and
history must stay decryptable. The correct posture is Keybase's: generational keys, rotate
on membership change, don't pretend the past is protectable.

The corollary users must be told: **removing a member protects future secrets only.
Rotating the actual credential (the database password itself) is the only true
revocation.** A good system makes key rotation automatic and *nags* about value rotation.

---

## Recommended model for Graft

Secrets are E2E-encrypted repo objects; the hub syncs opaque bytes. The model below is the
smallest one that clears the Doppler/Infisical UX bar while being honest about crypto.

### Identity

- Each user has an **X25519 keypair** (age-compatible identity; can literally be an age
  key). Public keys are published to the repo as objects; private keys never leave the
  user's machines. Age-compatibility means `age` itself is the escape hatch for
  inspection/recovery tooling.
- Trust in public keys is anchored in the repo history (whoever holds admin/write rights
  added them), not in the hub. This inherits 1Password's §A.3 caveat — a compromised hub
  could serve a tampered repo with a swapped key — unless key additions are signed by
  existing members. Signed membership changes (Keybase sigchain-lite) are the upgrade
  path; start by at least making the recipient list a visible, diffable object so a bad
  key addition is *auditable*.

### Keys and scopes

- **Scope = environment, keyed per environment.** A repo declares environments
  (`dev`, `staging`, `prod`, …). Each environment has its own symmetric **env key**
  (à la vault key / PTK), with a **generation number**.
- **Recipient model = age stanzas.** Each env key generation is wrapped once per
  authorized recipient (X25519 stanza, exactly the age construction) into a keyring
  object stored in the repo. Secrets for that environment are encrypted under the env
  key (AEAD, e.g. ChaCha20-Poly1305/AES-GCM) as repo objects.
- Granularity ladder: repo-wide access falls out for free (member is recipient of all
  envs); per-environment is the product feature ("everyone gets dev, three people get
  prod") and matches where Doppler puts its access boundary; per-path/folder ACLs are
  explicitly **out of scope** (Infisical-enterprise territory, yagni).
- **Group keys are deliberately omitted** for v1. 1Password/Keybase need group keypairs
  because teams are huge; Graft teams-on-a-repo are small, and N wraps per env per
  generation is cheap. Revisit only if recipient lists get painful.

### Operations (sops' two verbs, made safe)

- **Add member** → re-wrap only: wrap current env key(s) to the new key. No rotation
  needed. New members can read current secrets; whether they can read *historical*
  generations is a policy choice — wrapping old generations to new members (Keybase-style
  chaining: each generation wraps its predecessor, so one wrap suffices) keeps
  "clone = get the env" true across history and is the recommended default.
- **Remove member** → **rotation is automatic and non-optional**: bump generation, new
  env key, wrap to remaining recipients, and re-encrypt the *current* secret values under
  the new generation in the same commit (secrets are tiny; re-encrypt-on-rotate costs
  nothing, unlike Keybase's chat corpus). History stays under old generations — the
  ex-member could read it anyway.
- **Device loss / key compromise** → same operation as removal (rotate everything the key
  could read), which is exactly Keybase's trigger list.
- Tooling nags: after a removal-rotation, prompt "rotate the underlying credentials in
  `prod` — key rotation does not un-know values" (the sops lesson, surfaced in UX).

### Honest limits (ship these in the docs, not the fine print)

- **Revocation = future-only.** A removed member keeps everything already cloned, and if
  they saved keys, everything until the rotation commit. Cryptographic keys not being
  retroactive is the industry norm — 1Password says so in its own white paper; we should
  say it in ours.
- **The hub sees metadata even though it never sees plaintext:** environment names,
  recipient public keys and counts (per age, stanzas leak count; identifying which key is
  which comes from the repo's own membership objects), generation bumps (someone was
  removed), change frequency, and ciphertext sizes. Mitigations if it matters later:
  padded values, opaque env ids. Compare 1Password §15.2, which candidly lists its
  cleartext columns.
- **A malicious hub can attempt key-substitution** (1Password §A.3) until membership
  changes are member-signed. Auditable recipient objects first, signatures next.
- **Client compromise is out of scope:** malware on a member's machine gets the keys, as
  every surveyed system concedes (1Password §A.10).

### Why this and not alternatives

- Server-side encryption (GitHub-environments style) was rejected in issue #3; everything
  above is what "own the object model" buys: the keyring, recipients, and rotation history
  are just repo objects, versioned and syncable like everything else.
- Shamir/key-groups (sops) add threshold ceremony Graft doesn't need for env sharing;
  noted as prior art for a future "break-glass prod" feature, nothing more.
