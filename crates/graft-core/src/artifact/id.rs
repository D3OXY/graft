use std::{fmt, str::FromStr};

use blake3::Hash;

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactId(pub(crate) [u8; 32]);

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum IdError {
    #[error("artifact id must contain exactly 64 hexadecimal characters")]
    WrongLength,
    #[error("artifact id contains a non-hexadecimal character")]
    InvalidHex,
}

impl ArtifactId {
    pub const LEN: usize = 32;

    pub fn digest(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn as_slice(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        self.to_string()
    }
}

impl From<Hash> for ArtifactId {
    fn from(value: Hash) -> Self {
        Self(*value.as_bytes())
    }
}

impl fmt::Debug for ArtifactId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ArtifactId")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for ArtifactId {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64 {
            return Err(IdError::WrongLength);
        }
        let mut bytes = [0u8; 32];
        for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            bytes[index] = hex_nibble(pair[0])? << 4 | hex_nibble(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

fn hex_nibble(byte: u8) -> Result<u8, IdError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(IdError::InvalidHex),
    }
}

#[cfg(test)]
mod tests {
    use super::ArtifactId;
    use std::str::FromStr;

    #[test]
    fn blake3_empty_vector_is_stable() {
        assert_eq!(
            ArtifactId::digest(b"").to_string(),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn ids_round_trip_hex() {
        let id = ArtifactId::from_str(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        )
        .expect("valid id");
        assert_eq!(
            id.to_string(),
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
        );
    }
}
