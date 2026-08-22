use std::collections::BTreeMap;

use crate::{
    tree::{flatten_tree, FileEntry, TreeError},
    ArtifactId, RepoStore, Revision, TreeEntryKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
    ModeChanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub old: Option<FileEntry>,
    pub new: Option<FileEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("revision `{0}` was not found")]
    MissingRevision(ArtifactId),
    #[error("artifact `{0}` is not a revision")]
    NotRevision(ArtifactId),
    #[error("interdiff revisions have different Change IDs")]
    DifferentChanges,
    #[error("tree error: {0}")]
    Tree(#[from] TreeError),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("artifact error: {0}")]
    Artifact(#[from] crate::artifact::ArtifactError),
}

pub fn diff_trees(
    store: &RepoStore,
    old: Option<ArtifactId>,
    new: Option<ArtifactId>,
) -> Result<Vec<FileChange>, DiffError> {
    let old_files = match old {
        Some(id) => flatten_tree(store, id)?,
        None => BTreeMap::new(),
    };
    let new_files = match new {
        Some(id) => flatten_tree(store, id)?,
        None => BTreeMap::new(),
    };
    Ok(diff_maps(&old_files, &new_files))
}

pub fn diff_revisions(
    store: &RepoStore,
    old: Option<ArtifactId>,
    new: ArtifactId,
) -> Result<Vec<FileChange>, DiffError> {
    let revision = load_revision(store, new)?;
    let old_tree = match old {
        Some(id) => Some(load_revision(store, id)?.trees()[0]),
        None => None,
    };
    diff_trees(store, old_tree, revision.trees().first().copied())
}

pub fn interdiff(
    store: &RepoStore,
    older: ArtifactId,
    newer: ArtifactId,
) -> Result<Vec<FileChange>, DiffError> {
    let older_revision = load_revision(store, older)?;
    let newer_revision = load_revision(store, newer)?;
    if older_revision.change_id() != newer_revision.change_id() {
        return Err(DiffError::DifferentChanges);
    }
    diff_trees(
        store,
        older_revision.trees().first().copied(),
        newer_revision.trees().first().copied(),
    )
}

fn load_revision(store: &RepoStore, id: ArtifactId) -> Result<Revision, DiffError> {
    let artifact = store.get(id)?.ok_or(DiffError::MissingRevision(id))?;
    Revision::from_artifact(artifact).map_err(|_| DiffError::NotRevision(id))
}

fn diff_maps(
    old: &BTreeMap<String, FileEntry>,
    new: &BTreeMap<String, FileEntry>,
) -> Vec<FileChange> {
    let mut changes = Vec::new();
    for path in old.keys().chain(new.keys()) {
        if changes
            .iter()
            .any(|change: &FileChange| change.path == *path)
        {
            continue;
        }
        match (old.get(path), new.get(path)) {
            (None, Some(new)) => changes.push(FileChange {
                path: path.clone(),
                kind: ChangeKind::Added,
                old: None,
                new: Some(*new),
            }),
            (Some(old), None) => changes.push(FileChange {
                path: path.clone(),
                kind: ChangeKind::Removed,
                old: Some(*old),
                new: None,
            }),
            (Some(old), Some(new)) if old.target != new.target => changes.push(FileChange {
                path: path.clone(),
                kind: ChangeKind::Modified,
                old: Some(*old),
                new: Some(*new),
            }),
            (Some(old), Some(new)) if old.kind != new.kind => changes.push(FileChange {
                path: path.clone(),
                kind: ChangeKind::ModeChanged,
                old: Some(*old),
                new: Some(*new),
            }),
            _ => {}
        }
    }
    changes.sort_by(|left, right| left.path.cmp(&right.path));
    changes
}

#[allow(dead_code)]
fn _mode(kind: TreeEntryKind) -> u8 {
    kind as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tree::{build_tree, FileEntry},
        Blob, ChangeId, Person, Revision,
    };

    fn revision(store: &RepoStore, change: u8, tree: ArtifactId) -> ArtifactId {
        let revision = Revision::new(
            vec![tree],
            vec![],
            ChangeId::from_bytes([change; 16]),
            Person {
                name: "A".into(),
                email: "a@x".into(),
                timestamp: 0,
            },
            Person {
                name: "A".into(),
                email: "a@x".into(),
                timestamp: 0,
            },
            "".into(),
        )
        .expect("revision");
        store.insert(revision.artifact()).expect("insert")
    }

    #[test]
    fn interdiff_is_for_one_change_only() {
        let store = RepoStore::memory().expect("store");
        let a = Blob::from_bytes(b"a").expect("a");
        let b = Blob::from_bytes(b"b").expect("b");
        store.insert(a.artifact()).expect("a");
        store.insert(b.artifact()).expect("b");
        let tree_a = build_tree(
            &store,
            &BTreeMap::from([(
                "f".into(),
                FileEntry {
                    kind: TreeEntryKind::File,
                    target: a.id(),
                },
            )]),
        )
        .expect("tree");
        let tree_b = build_tree(
            &store,
            &BTreeMap::from([(
                "f".into(),
                FileEntry {
                    kind: TreeEntryKind::File,
                    target: b.id(),
                },
            )]),
        )
        .expect("tree");
        let old = revision(&store, 1, tree_a);
        let new = Revision::new(
            vec![tree_b],
            vec![old],
            ChangeId::from_bytes([1; 16]),
            Person {
                name: "A".into(),
                email: "a@x".into(),
                timestamp: 1,
            },
            Person {
                name: "A".into(),
                email: "a@x".into(),
                timestamp: 1,
            },
            "".into(),
        )
        .expect("revision");
        let new = store.insert(new.artifact()).expect("new");
        assert_eq!(interdiff(&store, old, new).expect("interdiff").len(), 1);
        let other = revision(&store, 2, tree_b);
        assert!(matches!(
            interdiff(&store, old, other),
            Err(DiffError::DifferentChanges)
        ));
    }
}
