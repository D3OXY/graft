use std::{cmp::Ordering, fmt};

use super::{Artifact, ArtifactError, ArtifactId, ArtifactKind};

const TREE_MAGIC: &[u8] = b"tree-v1\0";
const REVISION_MAGIC: &[u8] = b"revision-v1\0";

type RevisionFields = (
    Vec<ArtifactId>,
    Vec<ArtifactId>,
    ChangeId,
    Person,
    Person,
    String,
);

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChangeId(pub(crate) [u8; 16]);

impl ChangeId {
    pub const LEN: usize = 16;

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(self) -> [u8; 16] {
        self.0
    }

    pub fn random_from_seed(seed: u128) -> Self {
        Self(seed.to_be_bytes())
    }
}

impl fmt::Debug for ChangeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ChangeId")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for ChangeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Blob {
    artifact: Artifact,
}

impl fmt::Debug for Blob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Blob")
            .field("id", &self.id())
            .field("size", &self.bytes().len())
            .finish()
    }
}

impl Blob {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ArtifactError> {
        Ok(Self {
            artifact: Artifact::new(ArtifactKind::Blob, bytes)?,
        })
    }

    pub fn from_artifact(artifact: Artifact) -> Result<Self, ArtifactError> {
        if artifact.kind() != ArtifactKind::Blob {
            return Err(ArtifactError::InvalidHeader);
        }
        Ok(Self { artifact })
    }

    pub fn id(&self) -> ArtifactId {
        self.artifact.id()
    }

    pub fn bytes(&self) -> &[u8] {
        self.artifact.payload()
    }

    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TreeEntryKind {
    File = 0,
    Executable = 1,
    Symlink = 2,
    Tree = 3,
}

#[derive(Clone, Eq, PartialEq)]
pub struct TreeEntry {
    pub name: String,
    pub kind: TreeEntryKind,
    pub target: ArtifactId,
}

impl fmt::Debug for TreeEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TreeEntry")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("target", &self.target)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Tree {
    artifact: Artifact,
    entries: Vec<TreeEntry>,
}

impl fmt::Debug for Tree {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Tree")
            .field("id", &self.id())
            .field("entries", &self.entries)
            .finish()
    }
}

impl Tree {
    pub fn new(mut entries: Vec<TreeEntry>) -> Result<Self, ArtifactError> {
        entries.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        validate_tree_entries(&entries)?;
        let payload = encode_tree_payload(&entries)?;
        let artifact = Artifact::new(ArtifactKind::Tree, &payload)?;
        Ok(Self { artifact, entries })
    }

    pub fn from_artifact(artifact: Artifact) -> Result<Self, ArtifactError> {
        if artifact.kind() != ArtifactKind::Tree {
            return Err(ArtifactError::InvalidHeader);
        }
        let entries = Self::decode_payload(artifact.payload())?;
        Ok(Self { artifact, entries })
    }

    pub fn id(&self) -> ArtifactId {
        self.artifact.id()
    }

    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }

    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    pub(crate) fn decode_payload(payload: &[u8]) -> Result<Vec<TreeEntry>, ArtifactError> {
        let mut reader = Reader::new(payload);
        if reader.take(TREE_MAGIC.len())? != TREE_MAGIC {
            return Err(ArtifactError::InvalidHeader);
        }
        let count = reader.u32()?;
        let count = usize::try_from(count).map_err(|_| ArtifactError::TooLarge)?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let name = String::from_utf8(reader.bytes_u32()?.to_vec())
                .map_err(|_| ArtifactError::InvalidUtf8)?;
            let kind = match reader.u8()? {
                0 => TreeEntryKind::File,
                1 => TreeEntryKind::Executable,
                2 => TreeEntryKind::Symlink,
                3 => TreeEntryKind::Tree,
                _ => return Err(ArtifactError::InvalidTreeEntryKind),
            };
            let target = ArtifactId::from_bytes(reader.array_32()?);
            entries.push(TreeEntry { name, kind, target });
        }
        reader.finish()?;
        validate_tree_entries(&entries)?;
        if entries
            .windows(2)
            .any(|pair| pair[0].name.as_bytes() >= pair[1].name.as_bytes())
        {
            return Err(ArtifactError::UnsortedTreeEntries);
        }
        Ok(entries)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub timestamp: i64,
}

impl fmt::Debug for Person {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Person")
            .field("name", &self.name)
            .field("email", &self.email)
            .field("timestamp", &self.timestamp)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Revision {
    artifact: Artifact,
    trees: Vec<ArtifactId>,
    parents: Vec<ArtifactId>,
    change_id: ChangeId,
    author: Person,
    committer: Person,
    message: String,
}

impl fmt::Debug for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Revision")
            .field("id", &self.id())
            .field("trees", &self.trees)
            .field("parents", &self.parents)
            .field("change_id", &self.change_id)
            .field("message", &self.message)
            .finish()
    }
}

impl Revision {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        trees: Vec<ArtifactId>,
        parents: Vec<ArtifactId>,
        change_id: ChangeId,
        author: Person,
        committer: Person,
        message: String,
    ) -> Result<Self, ArtifactError> {
        validate_revision_fields(&trees, &author, &committer, &message)?;
        let payload =
            encode_revision_payload(&trees, &parents, change_id, &author, &committer, &message)?;
        let artifact = Artifact::new(ArtifactKind::Revision, &payload)?;
        Ok(Self {
            artifact,
            trees,
            parents,
            change_id,
            author,
            committer,
            message,
        })
    }

    pub fn from_artifact(artifact: Artifact) -> Result<Self, ArtifactError> {
        if artifact.kind() != ArtifactKind::Revision {
            return Err(ArtifactError::InvalidHeader);
        }
        let fields = decode_revision_payload(artifact.payload())?;
        Ok(Self {
            artifact,
            trees: fields.0,
            parents: fields.1,
            change_id: fields.2,
            author: fields.3,
            committer: fields.4,
            message: fields.5,
        })
    }

    pub fn id(&self) -> ArtifactId {
        self.artifact.id()
    }

    pub fn trees(&self) -> &[ArtifactId] {
        &self.trees
    }

    pub fn parents(&self) -> &[ArtifactId] {
        &self.parents
    }

    pub fn change_id(&self) -> ChangeId {
        self.change_id
    }

    pub fn author(&self) -> &Person {
        &self.author
    }

    pub fn committer(&self) -> &Person {
        &self.committer
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    pub(crate) fn decode_payload(payload: &[u8]) -> Result<RevisionFields, ArtifactError> {
        decode_revision_payload(payload)
    }
}

fn validate_tree_entries(entries: &[TreeEntry]) -> Result<(), ArtifactError> {
    for entry in entries {
        if entry.name.is_empty()
            || entry.name == "."
            || entry.name == ".."
            || entry.name.contains('/')
            || entry.name.contains('\0')
        {
            return Err(ArtifactError::InvalidTreeEntryName);
        }
    }
    for pair in entries.windows(2) {
        if pair[0].name.as_bytes() == pair[1].name.as_bytes() {
            return Err(ArtifactError::DuplicateTreeEntry(pair[0].name.clone()));
        }
    }
    Ok(())
}

fn encode_tree_payload(entries: &[TreeEntry]) -> Result<Vec<u8>, ArtifactError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(TREE_MAGIC);
    put_u32(&mut bytes, entries.len())?;
    for entry in entries {
        put_bytes_u32(&mut bytes, entry.name.as_bytes())?;
        bytes.push(entry.kind as u8);
        bytes.extend_from_slice(entry.target.as_slice());
    }
    Ok(bytes)
}

fn validate_revision_fields(
    trees: &[ArtifactId],
    author: &Person,
    committer: &Person,
    message: &str,
) -> Result<(), ArtifactError> {
    if trees.is_empty()
        || !valid_text(&author.name)
        || !valid_text(&author.email)
        || !valid_text(&committer.name)
        || !valid_text(&committer.email)
        || !valid_text(message)
    {
        return Err(ArtifactError::InvalidRevision);
    }
    Ok(())
}

fn encode_revision_payload(
    trees: &[ArtifactId],
    parents: &[ArtifactId],
    change_id: ChangeId,
    author: &Person,
    committer: &Person,
    message: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(REVISION_MAGIC);
    put_ids(&mut bytes, trees)?;
    put_ids(&mut bytes, parents)?;
    bytes.extend_from_slice(&change_id.as_bytes());
    put_person(&mut bytes, author)?;
    put_person(&mut bytes, committer)?;
    put_bytes_u32(&mut bytes, message.as_bytes())?;
    Ok(bytes)
}

fn decode_revision_payload(payload: &[u8]) -> Result<RevisionFields, ArtifactError> {
    let mut reader = Reader::new(payload);
    if reader.take(REVISION_MAGIC.len())? != REVISION_MAGIC {
        return Err(ArtifactError::InvalidHeader);
    }
    let trees = reader.ids()?;
    let parents = reader.ids()?;
    let change_id = ChangeId(reader.array_16()?);
    let author = reader.person()?;
    let committer = reader.person()?;
    let message =
        String::from_utf8(reader.bytes_u32()?.to_vec()).map_err(|_| ArtifactError::InvalidUtf8)?;
    reader.finish()?;
    validate_revision_fields(&trees, &author, &committer, &message)?;
    let canonical =
        encode_revision_payload(&trees, &parents, change_id, &author, &committer, &message)?;
    if canonical != payload {
        return Err(ArtifactError::NonCanonical);
    }
    Ok((trees, parents, change_id, author, committer, message))
}

fn valid_text(value: &str) -> bool {
    !value.contains('\0')
}

fn put_u32(bytes: &mut Vec<u8>, value: usize) -> Result<(), ArtifactError> {
    let value = u32::try_from(value).map_err(|_| ArtifactError::TooLarge)?;
    bytes.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn put_bytes_u32(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(), ArtifactError> {
    put_u32(bytes, value.len())?;
    bytes.extend_from_slice(value);
    Ok(())
}

fn put_ids(bytes: &mut Vec<u8>, ids: &[ArtifactId]) -> Result<(), ArtifactError> {
    put_u32(bytes, ids.len())?;
    for id in ids {
        bytes.extend_from_slice(id.as_slice());
    }
    Ok(())
}

fn put_person(bytes: &mut Vec<u8>, person: &Person) -> Result<(), ArtifactError> {
    put_bytes_u32(bytes, person.name.as_bytes())?;
    put_bytes_u32(bytes, person.email.as_bytes())?;
    bytes.extend_from_slice(&person.timestamp.to_be_bytes());
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ArtifactError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ArtifactError::TooLarge)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ArtifactError::Truncated)?;
        self.position = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, ArtifactError> {
        Ok(*self.take(1)?.first().expect("one byte"))
    }

    fn u32(&mut self) -> Result<u32, ArtifactError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }

    fn bytes_u32(&mut self) -> Result<&'a [u8], ArtifactError> {
        let length = usize::try_from(self.u32()?).map_err(|_| ArtifactError::TooLarge)?;
        self.take(length)
    }

    fn array_16(&mut self) -> Result<[u8; 16], ArtifactError> {
        Ok(self.take(16)?.try_into().expect("sixteen bytes"))
    }

    fn array_32(&mut self) -> Result<[u8; 32], ArtifactError> {
        Ok(self.take(32)?.try_into().expect("thirty-two bytes"))
    }

    fn ids(&mut self) -> Result<Vec<ArtifactId>, ArtifactError> {
        let count = usize::try_from(self.u32()?).map_err(|_| ArtifactError::TooLarge)?;
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(ArtifactId::from_bytes(self.array_32()?));
        }
        Ok(ids)
    }

    fn person(&mut self) -> Result<Person, ArtifactError> {
        let name = String::from_utf8(self.bytes_u32()?.to_vec())
            .map_err(|_| ArtifactError::InvalidUtf8)?;
        let email = String::from_utf8(self.bytes_u32()?.to_vec())
            .map_err(|_| ArtifactError::InvalidUtf8)?;
        let timestamp = i64::from_be_bytes(self.take(8)?.try_into().expect("eight bytes"));
        Ok(Person {
            name,
            email,
            timestamp,
        })
    }

    fn finish(&self) -> Result<(), ArtifactError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(ArtifactError::TrailingBytes)
        }
    }
}

#[allow(dead_code)]
fn compare_names(left: &str, right: &str) -> Ordering {
    left.as_bytes().cmp(right.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> ArtifactId {
        ArtifactId::from_bytes([byte; 32])
    }

    #[test]
    fn trees_use_sorted_compact_entries() {
        let tree = Tree::new(vec![
            TreeEntry {
                name: "z.txt".into(),
                kind: TreeEntryKind::File,
                target: id(2),
            },
            TreeEntry {
                name: "a.txt".into(),
                kind: TreeEntryKind::Executable,
                target: id(1),
            },
        ])
        .expect("tree");
        assert_eq!(tree.entries()[0].name, "a.txt");
        assert_eq!(
            Tree::from_artifact(tree.artifact().clone())
                .expect("round trip")
                .entries(),
            tree.entries()
        );
    }

    #[test]
    fn revision_round_trips_and_preserves_ordered_parents() {
        let revision = Revision::new(
            vec![id(1), id(2)],
            vec![id(3), id(4)],
            ChangeId::from_bytes([7; 16]),
            Person {
                name: "A".into(),
                email: "a@example.test".into(),
                timestamp: 11,
            },
            Person {
                name: "B".into(),
                email: "b@example.test".into(),
                timestamp: 12,
            },
            "merge".into(),
        )
        .expect("revision");
        let copy = Revision::from_artifact(revision.artifact().clone()).expect("round trip");
        assert_eq!(copy.parents(), &[id(3), id(4)]);
        assert_eq!(copy.trees(), &[id(1), id(2)]);
        assert_eq!(copy.change_id(), ChangeId::from_bytes([7; 16]));
    }
}
