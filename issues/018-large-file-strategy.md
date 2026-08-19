---
id: 18
title: "Decide the large-file strategy for v1"
type: grilling
status: closed
assignee: d3oxy
blocked-by: [9]
---

## Question

Graduated from map fog by the object-model resolution (ticket 9). Blobs are raw bytes;
the frozen-format law means a chunked-blob artifact type can be added later without a
break, and set-reconciliation phantoms make "choose not to fetch" (partial clone) latent
in the model. So: does v1 ship raw blobs only (delta-encoded in storage) and defer
chunking/partial fetch entirely, or does the v1 spec commit to a chunked-blob artifact
type and/or a size threshold now? What's the honest answer for repos with big binaries at
v1, and what exactly is deferred?

## Resolution

**v1 product boundary.** Graft v1 targets source-code repositories, including those with
occasional large binaries. It does not claim efficient operation for media, ML-model,
artifact, or other asset-heavy repositories. The honest cost is that large files consume
their full bandwidth and working-copy disk footprint.

**Canonical representation.** v1 has only the existing raw-byte `blob` artifact. There
is no canonical size threshold, automatic chunking, or format-level size limit. A later
chunked representation, if evidence warrants one, must be a new artifact type; existing
raw blobs remain valid forever under the format-evolution law.

**Operational contract.** Implementations stream blob hashing, storage, transfer, and
materialization with bounded memory: a whole-file operation must not imply that the file
fits in RAM. Compression and delta encoding at rest remain transparent storage
optimizations, not user-visible large-file guarantees. Sync still sends whole artifacts;
after a successful clone or pull, every artifact in the reconciled set is present
locally. Phantoms may represent in-progress or interrupted sync, but v1 has no lazy blobs,
placeholder files, or on-demand checkout.

**Deployment policy.** Local repositories are unrestricted. A hub may configure an
inbound `max_artifact_bytes` policy, advertised during the sync handshake. The client
preflights its outgoing artifacts against that value and reports a typed error before
sending payloads; an oversized artifact cannot cause a partial ref update.

**Explicitly deferred, without choosing a design.** Chunk manifests and chunk-level
deduplication, within-artifact resumability, partial or sparse clone, wire deltas,
external/LFS-style stores, and asset-heavy performance guarantees. The already-decided
extension points remain sufficient: new artifact types for representations, phantoms for
selective possession, and protocol capabilities for new transfer behavior.
