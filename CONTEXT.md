# Graft — Glossary

- **Graft**: the version control system itself (Rust core + CLI). Named for grafting —
  joining a living branch onto another stock so they grow as one.
- **Hub**: the self-hostable GitHub-equivalent for Graft repos (working name; may get its
  own product name — see hub-architecture ticket).
- **Change**: Graft's commit-equivalent — a snapshot of the tree plus parents and message.
- **Change ID**: the stable identity of a Change that survives rewriting/amending;
  distinct from its content hash (jj-style).
- **Secret**: a team-shared value (env var etc.) stored as an E2E-encrypted repo object;
  the hub syncs ciphertext and never sees plaintext.
- **Proposal** *(provisional)*: the PR-equivalent; semantics and final name pending its
  ticket.
