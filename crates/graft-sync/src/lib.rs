//! Versioned, transport-neutral synchronization frames and reconciliation.

use std::collections::BTreeSet;
use std::io::Cursor;

use graft_core::{Artifact, ArtifactId, RefUpdate, RepoStore};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Push,
    Pull,
    Both,
}

impl Direction {
    fn code(self) -> u8 {
        match self {
            Self::Push => 1,
            Self::Pull => 2,
            Self::Both => 3,
        }
    }
    fn from_code(code: u8) -> Result<Self, SyncError> {
        match code {
            1 => Ok(Self::Push),
            2 => Ok(Self::Pull),
            3 => Ok(Self::Both),
            _ => Err(SyncError::MalformedFrame),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Handshake {
    pub version: u16,
    pub direction: Direction,
    pub capabilities: BTreeSet<String>,
    pub max_artifact_bytes: Option<u64>,
}
impl Handshake {
    pub fn v1(direction: Direction) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            direction,
            capabilities: BTreeSet::new(),
            max_artifact_bytes: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewRef {
    pub name: String,
    pub target: ArtifactId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireRefUpdate {
    pub name: String,
    pub expected: Option<ArtifactId>,
    pub new: Option<ArtifactId>,
}
impl From<WireRefUpdate> for RefUpdate {
    fn from(value: WireRefUpdate) -> Self {
        Self {
            name: value.name,
            expected: value.expected,
            new: value.new,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Frame {
    Handshake(Handshake),
    Igot(Vec<ArtifactId>),
    Gimme(Vec<ArtifactId>),
    File { id: ArtifactId, bytes: Vec<u8> },
    Phantom(ArtifactId),
    View(Vec<ViewRef>),
    RefBatch(Vec<WireRefUpdate>),
    Error { code: u16, message: String },
}
impl Frame {
    fn code(&self) -> u8 {
        match self {
            Self::Handshake(_) => 1,
            Self::Igot(_) => 2,
            Self::Gimme(_) => 3,
            Self::File { .. } => 4,
            Self::Phantom(_) => 5,
            Self::View(_) => 6,
            Self::RefBatch(_) => 7,
            Self::Error { .. } => 8,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("sync frame is malformed")]
    MalformedFrame,
    #[error("sync frame uses an unsupported protocol version")]
    UnsupportedVersion,
    #[error("sync frame exceeds the configured artifact limit")]
    ArtifactTooLarge,
    #[error("sync frame contains an artifact whose hash does not match")]
    HashMismatch,
    #[error("sync frame contains an invalid artifact")]
    InvalidArtifact,
    #[error("sync frame contains duplicate or unsorted records")]
    NonCanonical,
    #[error("sync store error: {0}")]
    Store(#[from] graft_core::store::StoreError),
    #[error("compression error: {0}")]
    Compression(#[from] std::io::Error),
}

pub fn encode_frames(frames: &[Frame]) -> Result<Vec<u8>, SyncError> {
    let mut body = Vec::new();
    for frame in frames {
        let payload = encode_frame_payload(frame)?;
        body.push(frame.code());
        body.extend_from_slice(
            &(u32::try_from(payload.len()).map_err(|_| SyncError::MalformedFrame)?).to_be_bytes(),
        );
        body.extend_from_slice(&payload);
    }
    Ok(zstd::stream::encode_all(Cursor::new(body), 3)?)
}

pub fn decode_frames(bytes: &[u8]) -> Result<Vec<Frame>, SyncError> {
    let body = zstd::stream::decode_all(Cursor::new(bytes))?;
    let mut reader = Reader::new(&body);
    let mut frames = Vec::new();
    while reader.remaining() > 0 {
        let code = reader.u8()?;
        let length = reader.u32()? as usize;
        let payload = reader.take(length)?;
        frames.push(decode_frame(code, payload)?);
    }
    Ok(frames)
}

pub fn inventory(store: &RepoStore) -> Result<Vec<ArtifactId>, SyncError> {
    Ok(store.ids()?)
}

pub fn missing_from_inventory(
    store: &RepoStore,
    peer: &[ArtifactId],
) -> Result<Vec<ArtifactId>, SyncError> {
    let local = store.ids()?.into_iter().collect::<BTreeSet<_>>();
    let mut missing = Vec::new();
    for id in peer {
        if !local.contains(id) {
            store.add_phantom(*id)?;
            missing.push(*id);
        }
    }
    missing.sort();
    missing.dedup();
    Ok(missing)
}

pub fn plan_files(
    store: &RepoStore,
    requested: &[ArtifactId],
    max_artifact_bytes: Option<u64>,
) -> Result<Vec<Frame>, SyncError> {
    let mut frames = Vec::with_capacity(requested.len());
    for id in requested {
        let Some(artifact) = store.get(*id)? else {
            frames.push(Frame::Phantom(*id));
            continue;
        };
        if max_artifact_bytes.is_some_and(|limit| artifact.bytes().len() as u64 > limit) {
            return Err(SyncError::ArtifactTooLarge);
        }
        frames.push(Frame::File {
            id: *id,
            bytes: artifact.bytes().to_vec(),
        });
    }
    Ok(frames)
}

pub fn apply_file_frames(
    store: &RepoStore,
    frames: &[Frame],
    max_artifact_bytes: Option<u64>,
) -> Result<(), SyncError> {
    let mut artifacts = Vec::new();
    for frame in frames {
        let Frame::File { id, bytes } = frame else {
            continue;
        };
        if max_artifact_bytes.is_some_and(|limit| bytes.len() as u64 > limit) {
            return Err(SyncError::ArtifactTooLarge);
        }
        if ArtifactId::digest(bytes) != *id {
            return Err(SyncError::HashMismatch);
        }
        artifacts
            .push(Artifact::from_bytes(bytes.clone()).map_err(|_| SyncError::InvalidArtifact)?);
    }
    for artifact in artifacts {
        store.insert(&artifact)?;
    }
    Ok(())
}

pub fn sync_stores(
    left: &RepoStore,
    right: &RepoStore,
    max_artifact_bytes: Option<u64>,
) -> Result<(), SyncError> {
    for _round in 0..128 {
        let left_inventory = left.ids()?;
        let right_inventory = right.ids()?;
        let left_needs = missing_from_inventory(left, &right_inventory)?;
        let right_needs = missing_from_inventory(right, &left_inventory)?;
        if left_needs.is_empty() && right_needs.is_empty() {
            return Ok(());
        }
        let left_frames = plan_files(right, &left_needs, max_artifact_bytes)?;
        let right_frames = plan_files(left, &right_needs, max_artifact_bytes)?;
        apply_file_frames(left, &left_frames, max_artifact_bytes)?;
        apply_file_frames(right, &right_frames, max_artifact_bytes)?;
    }
    Err(SyncError::MalformedFrame)
}

pub fn apply_ref_batch(store: &RepoStore, updates: Vec<WireRefUpdate>) -> Result<(), SyncError> {
    store.cas_refs(&updates.into_iter().map(Into::into).collect::<Vec<_>>())?;
    Ok(())
}

fn encode_frame_payload(frame: &Frame) -> Result<Vec<u8>, SyncError> {
    let mut output = Vec::new();
    match frame {
        Frame::Handshake(handshake) => {
            output.extend_from_slice(&handshake.version.to_be_bytes());
            output.push(handshake.direction.code());
            put_u32(&mut output, handshake.capabilities.len())?;
            for capability in &handshake.capabilities {
                put_string(&mut output, capability)?;
            }
            match handshake.max_artifact_bytes {
                Some(value) => {
                    output.push(1);
                    output.extend_from_slice(&value.to_be_bytes());
                }
                None => output.push(0),
            }
        }
        Frame::Igot(ids) | Frame::Gimme(ids) => put_ids(&mut output, ids)?,
        Frame::File { id, bytes } => {
            output.extend_from_slice(id.as_slice());
            put_bytes(&mut output, bytes)?;
        }
        Frame::Phantom(id) => output.extend_from_slice(id.as_slice()),
        Frame::View(refs) => {
            put_u32(&mut output, refs.len())?;
            for reference in refs {
                put_string(&mut output, &reference.name)?;
                output.extend_from_slice(reference.target.as_slice());
            }
        }
        Frame::RefBatch(updates) => {
            put_u32(&mut output, updates.len())?;
            for update in updates {
                put_string(&mut output, &update.name)?;
                put_optional_id(&mut output, update.expected);
                put_optional_id(&mut output, update.new);
            }
        }
        Frame::Error { code, message } => {
            output.extend_from_slice(&code.to_be_bytes());
            put_string(&mut output, message)?;
        }
    }
    Ok(output)
}

fn decode_frame(code: u8, payload: &[u8]) -> Result<Frame, SyncError> {
    let mut reader = Reader::new(payload);
    let frame = match code {
        1 => {
            let version = reader.u16()?;
            if version != PROTOCOL_VERSION {
                return Err(SyncError::UnsupportedVersion);
            }
            let direction = Direction::from_code(reader.u8()?)?;
            let count = reader.u32()? as usize;
            let mut capabilities = BTreeSet::new();
            for _ in 0..count {
                if !capabilities.insert(reader.string()?) {
                    return Err(SyncError::NonCanonical);
                }
            }
            let max_artifact_bytes = match reader.u8()? {
                0 => None,
                1 => Some(reader.u64()?),
                _ => return Err(SyncError::MalformedFrame),
            };
            Frame::Handshake(Handshake {
                version,
                direction,
                capabilities,
                max_artifact_bytes,
            })
        }
        2 => Frame::Igot(reader.ids()?),
        3 => Frame::Gimme(reader.ids()?),
        4 => Frame::File {
            id: reader.id()?,
            bytes: reader.bytes()?.to_vec(),
        },
        5 => Frame::Phantom(reader.id()?),
        6 => {
            let count = reader.u32()? as usize;
            let mut refs = Vec::with_capacity(count);
            let mut previous = None;
            for _ in 0..count {
                let name = reader.string()?;
                if previous
                    .as_deref()
                    .is_some_and(|previous| previous >= name.as_str())
                {
                    return Err(SyncError::NonCanonical);
                }
                previous = Some(name.clone());
                refs.push(ViewRef {
                    name,
                    target: reader.id()?,
                });
            }
            Frame::View(refs)
        }
        7 => {
            let count = reader.u32()? as usize;
            let mut updates = Vec::with_capacity(count);
            for _ in 0..count {
                updates.push(WireRefUpdate {
                    name: reader.string()?,
                    expected: reader.optional_id()?,
                    new: reader.optional_id()?,
                });
            }
            Frame::RefBatch(updates)
        }
        8 => Frame::Error {
            code: reader.u16()?,
            message: reader.string()?,
        },
        _ => return Err(SyncError::MalformedFrame),
    };
    reader.finish()?;
    Ok(frame)
}

fn put_u32(output: &mut Vec<u8>, value: usize) -> Result<(), SyncError> {
    output.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| SyncError::MalformedFrame)?
            .to_be_bytes(),
    );
    Ok(())
}
fn put_string(output: &mut Vec<u8>, value: &str) -> Result<(), SyncError> {
    put_u32(output, value.len())?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}
fn put_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<(), SyncError> {
    output.extend_from_slice(
        &u64::try_from(value.len())
            .map_err(|_| SyncError::MalformedFrame)?
            .to_be_bytes(),
    );
    output.extend_from_slice(value);
    Ok(())
}
fn put_ids(output: &mut Vec<u8>, ids: &[ArtifactId]) -> Result<(), SyncError> {
    put_u32(output, ids.len())?;
    for id in ids {
        output.extend_from_slice(id.as_slice());
    }
    Ok(())
}
fn put_optional_id(output: &mut Vec<u8>, id: Option<ArtifactId>) {
    match id {
        Some(id) => {
            output.push(1);
            output.extend_from_slice(id.as_slice());
        }
        None => output.push(0),
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], SyncError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(SyncError::MalformedFrame)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(SyncError::MalformedFrame)?;
        self.position = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, SyncError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, SyncError> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| SyncError::MalformedFrame)?,
        ))
    }
    fn u32(&mut self) -> Result<u32, SyncError> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| SyncError::MalformedFrame)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, SyncError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| SyncError::MalformedFrame)?,
        ))
    }
    fn bytes(&mut self) -> Result<&'a [u8], SyncError> {
        let len = usize::try_from(self.u64()?).map_err(|_| SyncError::MalformedFrame)?;
        self.take(len)
    }
    fn string(&mut self) -> Result<String, SyncError> {
        let len = self.u32()? as usize;
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| SyncError::MalformedFrame)
    }
    fn id(&mut self) -> Result<ArtifactId, SyncError> {
        Ok(ArtifactId::from_bytes(
            self.take(32)?
                .try_into()
                .map_err(|_| SyncError::MalformedFrame)?,
        ))
    }
    fn ids(&mut self) -> Result<Vec<ArtifactId>, SyncError> {
        let count = self.u32()? as usize;
        let mut ids = Vec::with_capacity(count);
        let mut previous = None;
        for _ in 0..count {
            let id = self.id()?;
            if previous.is_some_and(|previous| previous >= id) {
                return Err(SyncError::NonCanonical);
            }
            previous = Some(id);
            ids.push(id);
        }
        Ok(ids)
    }
    fn optional_id(&mut self) -> Result<Option<ArtifactId>, SyncError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.id()?)),
            _ => Err(SyncError::MalformedFrame),
        }
    }
    fn finish(&self) -> Result<(), SyncError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(SyncError::MalformedFrame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graft_core::Blob;

    #[test]
    fn frames_are_deterministic_and_round_trip_through_zstd() {
        let mut handshake = Handshake::v1(Direction::Both);
        handshake.capabilities.insert("max_artifact_bytes".into());
        let id = ArtifactId::from_bytes([1; 32]);
        let frames = vec![
            Frame::Handshake(handshake),
            Frame::Igot(vec![id]),
            Frame::Phantom(id),
        ];
        let encoded = encode_frames(&frames).expect("encode");
        assert_eq!(decode_frames(&encoded).expect("decode"), frames);
        assert_eq!(encode_frames(&frames).expect("encode"), encoded);
    }

    #[test]
    fn disjoint_stores_converge_and_invalid_file_changes_nothing() {
        let left = RepoStore::memory().expect("left");
        let right = RepoStore::memory().expect("right");
        let blob = Blob::from_bytes(b"shared").expect("blob");
        left.insert(blob.artifact()).expect("insert");
        sync_stores(&left, &right, None).expect("sync");
        assert_eq!(left.ids().expect("ids"), right.ids().expect("ids"));
        let before = right.ids().expect("ids").len();
        let bad = Frame::File {
            id: ArtifactId::from_bytes([9; 32]),
            bytes: b"bad".to_vec(),
        };
        assert!(matches!(
            apply_file_frames(&right, &[bad], None),
            Err(SyncError::HashMismatch)
        ));
        assert_eq!(right.ids().expect("ids").len(), before);
    }

    #[test]
    fn ref_batch_is_cas_atomic() {
        let store = RepoStore::memory().expect("store");
        let blob = Blob::from_bytes(b"x").expect("blob");
        let id = store.insert(blob.artifact()).expect("insert");
        apply_ref_batch(
            &store,
            vec![WireRefUpdate {
                name: "branches/main".into(),
                expected: None,
                new: Some(id),
            }],
        )
        .expect("refs");
        assert!(apply_ref_batch(
            &store,
            vec![
                WireRefUpdate {
                    name: "branches/main".into(),
                    expected: None,
                    new: Some(id)
                },
                WireRefUpdate {
                    name: "branches/dev".into(),
                    expected: Some(id),
                    new: Some(id)
                }
            ]
        )
        .is_err());
        assert_eq!(
            store.refs().expect("refs"),
            vec![("branches/main".into(), id)]
        );
    }
}
