use std::{
    io::{Read, Write},
    path::Path,
};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::artifact::{Artifact, ArtifactError, ArtifactId, ArtifactKind, Blob};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("artifact error: {0}")]
    Artifact(#[from] ArtifactError),
    #[error("artifact `{0}` already exists with different bytes")]
    HashCollision(ArtifactId),
    #[error("ref `{0}` points to a missing artifact")]
    MissingArtifact(String),
    #[error("ref batch compare-and-swap failed for `{0}`")]
    StaleRef(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid ref name")]
    InvalidRefName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefUpdate {
    pub name: String,
    pub expected: Option<ArtifactId>,
    pub new: Option<ArtifactId>,
}

pub struct RepoStore {
    connection: Connection,
}

impl RepoStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    pub fn memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> Result<(), StoreError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS artifacts (
                 id BLOB PRIMARY KEY NOT NULL,
                 kind INTEGER NOT NULL,
                 bytes BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS refs (
                 name TEXT PRIMARY KEY NOT NULL,
                 target BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS phantoms (
                 id BLOB PRIMARY KEY NOT NULL
             );
             CREATE TABLE IF NOT EXISTS artifact_index (
                 id BLOB PRIMARY KEY NOT NULL,
                 kind INTEGER NOT NULL,
                 size INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS tree_index (
                 tree_id BLOB NOT NULL,
                 name BLOB NOT NULL,
                 kind INTEGER NOT NULL,
                 target BLOB NOT NULL,
                 PRIMARY KEY (tree_id, name)
             );",
        )?;
        Ok(())
    }

    pub fn insert(&self, artifact: &Artifact) -> Result<ArtifactId, StoreError> {
        let id = artifact.id();
        let existing = self
            .connection
            .query_row(
                "SELECT bytes FROM artifacts WHERE id = ?1",
                params![id.as_slice().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing != artifact.bytes() {
                return Err(StoreError::HashCollision(id));
            }
            return Ok(id);
        }
        self.connection.execute(
            "INSERT INTO artifacts (id, kind, bytes) VALUES (?1, ?2, ?3)",
            params![
                id.as_slice().as_slice(),
                artifact.kind().code(),
                artifact.bytes()
            ],
        )?;
        self.index_artifact(artifact)?;
        Ok(id)
    }

    pub fn insert_blob_reader<R: Read>(&self, mut reader: R) -> Result<ArtifactId, StoreError> {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            let read = reader.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        let blob = Blob::from_bytes(&bytes)?;
        self.insert(blob.artifact())
    }

    pub fn materialize_blob<W: Write>(
        &self,
        id: ArtifactId,
        mut writer: W,
    ) -> Result<bool, StoreError> {
        let Some(artifact) = self.get(id)? else {
            return Ok(false);
        };
        if artifact.kind() != ArtifactKind::Blob {
            return Ok(false);
        }
        writer.write_all(artifact.payload())?;
        Ok(true)
    }

    pub fn get(&self, id: ArtifactId) -> Result<Option<Artifact>, StoreError> {
        let bytes = self
            .connection
            .query_row(
                "SELECT bytes FROM artifacts WHERE id = ?1",
                params![id.as_slice().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        bytes
            .map(|bytes| Artifact::from_bytes(bytes).map_err(StoreError::from))
            .transpose()
    }

    pub fn contains(&self, id: ArtifactId) -> Result<bool, StoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT 1 FROM artifacts WHERE id = ?1",
                params![id.as_slice().as_slice()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn ids(&self) -> Result<Vec<ArtifactId>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM artifacts ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(ArtifactId::from_bytes(bytes))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn refs(&self) -> Result<Vec<(String, ArtifactId)>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT name, target FROM refs ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            let target: Vec<u8> = row.get(1)?;
            let target: [u8; 32] = target
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok((row.get(0)?, ArtifactId::from_bytes(target)))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_ref(&self, name: &str) -> Result<Option<ArtifactId>, StoreError> {
        let target = self
            .connection
            .query_row(
                "SELECT target FROM refs WHERE name = ?1",
                params![name],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        target
            .map(|bytes| {
                bytes
                    .try_into()
                    .map(ArtifactId::from_bytes)
                    .map_err(|_| rusqlite::Error::InvalidQuery)
            })
            .transpose()
            .map_err(StoreError::from)
    }

    pub fn set_ref(&self, name: &str, target: ArtifactId) -> Result<(), StoreError> {
        self.cas_refs(&[RefUpdate {
            name: name.to_owned(),
            expected: self.get_ref(name)?,
            new: Some(target),
        }])
    }

    pub fn cas_refs(&self, updates: &[RefUpdate]) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        validate_updates(&transaction, updates)?;
        for update in updates {
            match update.new {
                Some(target) => {
                    transaction.execute(
                        "INSERT INTO refs (name, target) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET target = excluded.target",
                        params![update.name, target.as_slice().as_slice()],
                    )?;
                }
                None => {
                    transaction
                        .execute("DELETE FROM refs WHERE name = ?1", params![update.name])?;
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn add_phantom(&self, id: ArtifactId) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO phantoms (id) VALUES (?1)",
            params![id.as_slice().as_slice()],
        )?;
        Ok(())
    }

    pub fn phantoms(&self) -> Result<Vec<ArtifactId>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM phantoms ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(ArtifactId::from_bytes(bytes))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn rebuild(&self) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute_batch("DELETE FROM artifact_index; DELETE FROM tree_index;")?;
        let mut statement =
            transaction.prepare("SELECT id, kind, bytes FROM artifacts ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            let id: Vec<u8> = row.get(0)?;
            let id: [u8; 32] = id.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok((
                ArtifactId::from_bytes(id),
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let artifacts = rows.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for (id, kind, bytes) in artifacts {
            let artifact = Artifact::from_bytes(bytes)?;
            if artifact.id() != id || i64::from(artifact.kind().code()) != kind {
                return Err(StoreError::Artifact(ArtifactError::InvalidHeader));
            }
            index_artifact(&transaction, &artifact)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn clear_derived_for_test(&self) -> Result<(), StoreError> {
        self.connection
            .execute_batch("DELETE FROM artifact_index; DELETE FROM tree_index;")?;
        Ok(())
    }

    pub fn derived_count(&self) -> Result<(u64, u64), StoreError> {
        let artifacts =
            self.connection
                .query_row("SELECT COUNT(*) FROM artifact_index", [], |row| row.get(0))?;
        let entries = self
            .connection
            .query_row("SELECT COUNT(*) FROM tree_index", [], |row| row.get(0))?;
        Ok((artifacts, entries))
    }

    pub fn garbage_collect(&self) -> Result<usize, StoreError> {
        let mut keep = std::collections::BTreeSet::new();
        for (_, id) in self.refs()? {
            mark_reachable(self, id, &mut keep)?;
        }
        let mut statement = self.connection.prepare("SELECT id, kind FROM artifacts")?;
        let rows = statement.query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            let id: [u8; 32] = bytes
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok((ArtifactId::from_bytes(id), row.get::<_, u8>(1)?))
        })?;
        let artifacts = rows.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut removed = 0;
        for (id, kind) in artifacts {
            if !keep.contains(&id)
                && !matches!(
                    ArtifactKind::from_code(kind),
                    ArtifactKind::SecretBundle | ArtifactKind::Keyring
                )
            {
                removed += self.connection.execute(
                    "DELETE FROM artifacts WHERE id = ?1",
                    params![id.as_slice().as_slice()],
                )?;
            }
        }
        self.rebuild()?;
        Ok(removed)
    }

    fn index_artifact(&self, artifact: &Artifact) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        index_artifact(&transaction, artifact)?;
        transaction.commit()?;
        Ok(())
    }
}

fn validate_updates(
    transaction: &Transaction<'_>,
    updates: &[RefUpdate],
) -> Result<(), StoreError> {
    for update in updates {
        if update.name.is_empty() || update.name.contains('\0') || update.name.starts_with('/') {
            return Err(StoreError::InvalidRefName);
        }
        let actual = transaction
            .query_row(
                "SELECT target FROM refs WHERE name = ?1",
                params![update.name],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|bytes| {
                bytes
                    .try_into()
                    .map(ArtifactId::from_bytes)
                    .map_err(|_| rusqlite::Error::InvalidQuery)
            })
            .transpose()?;
        if actual != update.expected {
            return Err(StoreError::StaleRef(update.name.clone()));
        }
        if let Some(new) = update.new {
            let exists = transaction
                .query_row(
                    "SELECT 1 FROM artifacts WHERE id = ?1",
                    params![new.as_slice().as_slice()],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::MissingArtifact(update.name.clone()));
            }
        }
    }
    Ok(())
}

fn index_artifact(transaction: &Transaction<'_>, artifact: &Artifact) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT OR REPLACE INTO artifact_index (id, kind, size) VALUES (?1, ?2, ?3)",
        params![
            artifact.id().as_slice().as_slice(),
            artifact.kind().code(),
            i64::try_from(artifact.bytes().len())
                .map_err(|_| StoreError::Artifact(ArtifactError::TooLarge))?
        ],
    )?;
    if let Ok(tree) = crate::artifact::Tree::from_artifact(artifact.clone()) {
        for entry in tree.entries() {
            transaction.execute(
                "INSERT INTO tree_index (tree_id, name, kind, target) VALUES (?1, ?2, ?3, ?4)",
                params![
                    tree.id().as_slice().as_slice(),
                    entry.name.as_bytes(),
                    entry.kind as u8,
                    entry.target.as_slice().as_slice()
                ],
            )?;
        }
    }
    Ok(())
}

fn mark_reachable(
    store: &RepoStore,
    id: ArtifactId,
    keep: &mut std::collections::BTreeSet<ArtifactId>,
) -> Result<(), StoreError> {
    if !keep.insert(id) {
        return Ok(());
    }
    let Some(artifact) = store.get(id)? else {
        return Ok(());
    };
    match artifact.kind() {
        ArtifactKind::Tree => {
            let tree = crate::artifact::Tree::from_artifact(artifact)?;
            for entry in tree.entries() {
                mark_reachable(store, entry.target, keep)?;
            }
        }
        ArtifactKind::Revision => {
            let revision = crate::artifact::Revision::from_artifact(artifact)?;
            for target in revision.trees().iter().chain(revision.parents()) {
                mark_reachable(store, *target, keep)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{TreeEntry, TreeEntryKind};
    use std::io::{self, Read};

    fn id(byte: u8) -> ArtifactId {
        ArtifactId::from_bytes([byte; 32])
    }

    struct Chunked {
        bytes: Vec<u8>,
        position: usize,
        max: usize,
    }

    impl Read for Chunked {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let remaining = self.bytes.len() - self.position;
            if remaining == 0 {
                return Ok(0);
            }
            let length = remaining.min(output.len()).min(self.max);
            output[..length].copy_from_slice(&self.bytes[self.position..self.position + length]);
            self.position += length;
            Ok(length)
        }
    }

    #[test]
    fn duplicate_insert_is_idempotent_and_streams() {
        let store = RepoStore::memory().expect("store");
        let bytes = vec![b'x'; 100_000];
        let id = store
            .insert_blob_reader(Chunked {
                bytes: bytes.clone(),
                position: 0,
                max: 7,
            })
            .expect("insert");
        assert_eq!(
            store
                .insert_blob_reader(Chunked {
                    bytes,
                    position: 0,
                    max: 3
                })
                .expect("duplicate"),
            id
        );
        let mut output = Vec::new();
        assert!(store
            .materialize_blob(id, &mut output)
            .expect("materialize"));
        assert_eq!(output, vec![b'x'; 100_000]);
    }

    #[test]
    fn cas_batch_is_all_or_nothing_and_rebuild_restores_indexes() {
        let store = RepoStore::memory().expect("store");
        let blob = crate::artifact::Blob::from_bytes(b"hello").expect("blob");
        let tree = crate::artifact::Tree::new(vec![TreeEntry {
            name: "a".into(),
            kind: TreeEntryKind::File,
            target: blob.id(),
        }])
        .expect("tree");
        store.insert(blob.artifact()).expect("blob insert");
        store.insert(tree.artifact()).expect("tree insert");
        let result = store.cas_refs(&[
            RefUpdate {
                name: "branches/main".into(),
                expected: None,
                new: Some(tree.id()),
            },
            RefUpdate {
                name: "branches/dev".into(),
                expected: None,
                new: Some(id(9)),
            },
        ]);
        assert!(matches!(result, Err(StoreError::MissingArtifact(_))));
        assert!(store.refs().expect("refs").is_empty());
        assert_eq!(store.derived_count().expect("indexes"), (2, 1));
        store.clear_derived_for_test().expect("clear");
        assert_eq!(store.derived_count().expect("cleared"), (0, 0));
        store.rebuild().expect("rebuild");
        assert_eq!(store.derived_count().expect("rebuilt"), (2, 1));
    }
}
