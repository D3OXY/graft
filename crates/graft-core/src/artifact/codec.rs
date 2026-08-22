use std::fmt;

use super::id::ArtifactId;

const MAGIC: &[u8; 6] = b"GRAFT\0";
pub const FORMAT_VERSION: u8 = 1;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ArtifactKind {
    Blob,
    Tree,
    Revision,
    SecretBundle,
    Keyring,
    Unknown(u8),
}

impl ArtifactKind {
    pub const fn code(self) -> u8 {
        match self {
            Self::Blob => 1,
            Self::Tree => 2,
            Self::Revision => 3,
            Self::SecretBundle => 4,
            Self::Keyring => 5,
            Self::Unknown(code) => code,
        }
    }

    pub const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Blob,
            2 => Self::Tree,
            3 => Self::Revision,
            4 => Self::SecretBundle,
            5 => Self::Keyring,
            code => Self::Unknown(code),
        }
    }
}

impl fmt::Debug for ArtifactKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blob => formatter.write_str("Blob"),
            Self::Tree => formatter.write_str("Tree"),
            Self::Revision => formatter.write_str("Revision"),
            Self::SecretBundle => formatter.write_str("SecretBundle"),
            Self::Keyring => formatter.write_str("Keyring"),
            Self::Unknown(code) => formatter.debug_tuple("Unknown").field(code).finish(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("artifact is truncated")]
    Truncated,
    #[error("artifact has an invalid magic or format version")]
    InvalidHeader,
    #[error("artifact payload length does not match its bytes")]
    InvalidLength,
    #[error("artifact payload is not valid UTF-8 where text was required")]
    InvalidUtf8,
    #[error("artifact field is too large")]
    TooLarge,
    #[error("artifact contains a non-canonical field")]
    NonCanonical,
    #[error("artifact contains duplicate tree entry `{0}`")]
    DuplicateTreeEntry(String),
    #[error("artifact tree entries are not sorted")]
    UnsortedTreeEntries,
    #[error("artifact tree entry name is invalid")]
    InvalidTreeEntryName,
    #[error("artifact has an invalid tree entry kind")]
    InvalidTreeEntryKind,
    #[error("artifact contains an invalid revision field")]
    InvalidRevision,
    #[error("artifact contains trailing bytes")]
    TrailingBytes,
}

#[derive(Clone, Eq, PartialEq)]
pub struct Artifact {
    id: ArtifactId,
    kind: ArtifactKind,
    bytes: Vec<u8>,
}

impl fmt::Debug for Artifact {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Artifact")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("size", &self.bytes.len())
            .finish()
    }
}

impl Artifact {
    pub fn new(kind: ArtifactKind, payload: &[u8]) -> Result<Self, ArtifactError> {
        let bytes = encode_envelope(kind, payload)?;
        let id = ArtifactId::digest(&bytes);
        Ok(Self { id, kind, bytes })
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ArtifactError> {
        let (kind, payload) = decode_envelope(&bytes)?;
        let id = ArtifactId::digest(&bytes);
        if matches!(kind, ArtifactKind::Tree) {
            super::types::Tree::decode_payload(payload)?;
        } else if matches!(kind, ArtifactKind::Revision) {
            super::types::Revision::decode_payload(payload)?;
        }
        Ok(Self { id, kind, bytes })
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    pub fn kind(&self) -> ArtifactKind {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn payload(&self) -> &[u8] {
        &self.bytes[16..]
    }
}

pub(crate) fn encode_envelope(
    kind: ArtifactKind,
    payload: &[u8],
) -> Result<Vec<u8>, ArtifactError> {
    let length = u64::try_from(payload.len()).map_err(|_| ArtifactError::TooLarge)?;
    let mut bytes = Vec::with_capacity(16 + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.push(FORMAT_VERSION);
    bytes.push(kind.code());
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

pub(crate) fn decode_envelope(bytes: &[u8]) -> Result<(ArtifactKind, &[u8]), ArtifactError> {
    if bytes.len() < 16 {
        return Err(ArtifactError::Truncated);
    }
    if &bytes[..6] != MAGIC || bytes[6] != FORMAT_VERSION {
        return Err(ArtifactError::InvalidHeader);
    }
    let length = u64::from_be_bytes(bytes[8..16].try_into().expect("fixed width"));
    let payload_length = usize::try_from(length).map_err(|_| ArtifactError::TooLarge)?;
    if payload_length != bytes.len() - 16 {
        return Err(ArtifactError::InvalidLength);
    }
    Ok((ArtifactKind::from_code(bytes[7]), &bytes[16..]))
}

#[cfg(test)]
mod tests {
    use super::{Artifact, ArtifactError, ArtifactKind};

    #[test]
    fn unknown_kinds_are_opaque_and_byte_exact() {
        let mut bytes = b"GRAFT\0".to_vec();
        bytes.extend([1, 99]);
        bytes.extend_from_slice(&(3u64).to_be_bytes());
        bytes.extend_from_slice(b"xyz");
        let artifact = Artifact::from_bytes(bytes.clone()).expect("unknown artifact");
        assert_eq!(artifact.kind(), ArtifactKind::Unknown(99));
        assert_eq!(artifact.bytes(), bytes.as_slice());
        assert_eq!(
            artifact.id().as_bytes(),
            super::ArtifactId::digest(&bytes).as_bytes()
        );
    }

    #[test]
    fn malformed_envelope_is_rejected() {
        assert!(matches!(
            Artifact::from_bytes(b"not-an-artifact".to_vec()),
            Err(ArtifactError::Truncated)
        ));
    }
}
