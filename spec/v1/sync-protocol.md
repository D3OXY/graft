# Sync protocol v1

Sync uses `POST /repo/<name>/sync` with a zstd-compressed body. The uncompressed body is a
sequence of frames, each `u8 type || u32be payload_length || payload`. Version 1 frame
types are handshake, `igot`, `gimme`, `file`, phantom, view, ref-batch, and typed error.

The handshake carries protocol version, direction, sorted capability names, and the
optional advertised `max_artifact_bytes`. Unknown capabilities are ignored. `file` frames
carry a complete canonical artifact; receivers verify both its BLAKE3-256 ID and envelope
before insertion. Oversize preflight happens before a sender emits any file frame.

Reconciliation enumerates each full artifact set. Missing IDs become phantoms and are
requested with `gimme`; rounds continue until both sets converge. Ref views are separate
from the artifact bag. Ref batches use expected-old compare-and-swap and are applied
atomically; a stale update changes no refs.
