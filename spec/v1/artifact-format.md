# Artifact format v1

This document freezes the bytes written by `graft-core`. An artifact ID is the BLAKE3-256
digest of the complete envelope, including its header and payload.

## Envelope

The envelope is the ASCII bytes `GRAFT`, a NUL byte, a one-byte version (`1`), a one-byte
kind code, a big-endian `u64` payload length, and exactly that many payload bytes:

```text
6 bytes  GRAFT\0
1 byte   version = 1
1 byte   kind
8 bytes  payload length, big endian
N bytes  payload
```

Kinds `1` through `5` are Blob, Tree, Revision, SecretBundle, and Keyring. Readers preserve
unknown kind codes byte-for-byte.

Blob payloads are raw bytes. Tree and Revision payloads use the canonical length-prefixed
binary records implemented by `graft-core`; all lengths are unsigned big-endian integers.
Trees sort entries by exact UTF-8 name bytes and encode each name, one-byte entry kind, and
32-byte target ID. Revision lists preserve order and carry the ordered tree list, ordered
parents, stable 16-byte Change ID, author, committer, and message.

Malformed lengths, duplicate tree names, invalid names, invalid UTF-8, unknown entry kinds,
and non-canonical re-encodings are rejected before an artifact is accepted.
