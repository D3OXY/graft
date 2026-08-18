# Graft — Glossary

- **Graft**: the version control system itself (Rust core + CLI). Named for grafting —
  joining a living branch onto another stock so they grow as one.
- **Hub**: the self-hostable GitHub-equivalent for Graft repos (working name; may get its
  own product name — see hub-architecture ticket).
- **Artifact**: an immutable, content-addressed object in a repo's canonical store
  (BLAKE3-256). The repo is a grow-only set of artifacts; everything else is derived.
  v1 artifact types: blob, tree, revision, secret bundle.
- **Change**: the evolving unit of work — what you amend, rebase, and discuss. Not itself
  an artifact; it is the identity that revisions carry.
- **Change ID**: the stable random identity of a Change; minted at creation, survives
  rewriting. Distinct from any content hash (jj-style).
- **Revision**: the immutable artifact recording one version of a Change — tree, parents,
  change ID, message. Its content hash is the revision ID; rewriting a Change makes a new
  Revision with the same change ID.
- **Divergence**: the state of a Change with more than one visible Revision — displayed,
  never an error.
- **Op log**: per-workspace, local-only log of operations (jj-style views); powers undo.
  Not synced in v1.
- **Secret**: a team-shared value (env var etc.) stored E2E-encrypted in a Secret Bundle;
  the hub syncs ciphertext and never sees plaintext.
- **Secret Bundle**: the artifact type carrying secrets — the entire value set of one
  Environment at one Generation, names and values encrypted, plus minimal plaintext
  routing metadata. Replaced whole on any change.
- **Environment**: the access boundary for secrets (e.g. dev/staging/prod). Membership in
  an Environment is what "having access" means; created on demand, not from a fixed set.
- **Keyring**: the single per-repo artifact recording members' public keys and each
  Environment's wrapped keys; every update is signed by an existing member.
- **Generation**: one era of an Environment's key. Rotation starts a new Generation;
  removing a member always rotates. Generations chain, so joining grants history.
- **Proposal** *(provisional)*: the PR-equivalent; semantics and final name pending its
  ticket.
