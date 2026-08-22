use std::collections::BTreeMap;

use crate::{
    artifact::Revision,
    tree::{build_tree, flatten_tree, TreeError},
    ArtifactId, Person, RepoStore, TreeEntryKind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergeOutcome {
    Clean(ArtifactId),
    Conflict(Vec<ArtifactId>),
}

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    #[error("revision `{0}` was not found")]
    MissingRevision(ArtifactId),
    #[error("artifact `{0}` is not a revision")]
    NotRevision(ArtifactId),
    #[error("tree error: {0}")]
    Tree(#[from] TreeError),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("artifact error: {0}")]
    Artifact(#[from] crate::artifact::ArtifactError),
}

pub fn merge_revisions(
    store: &RepoStore,
    base: Option<ArtifactId>,
    left: ArtifactId,
    right: ArtifactId,
    committer: Person,
) -> Result<Revision, MergeError> {
    let left_revision = load_revision(store, left)?;
    let right_revision = load_revision(store, right)?;
    let base_revision = base.map(|id| load_revision(store, id)).transpose()?;
    let left_tree = left_revision.trees().first().copied();
    let right_tree = right_revision.trees().first().copied();
    let base_tree = base_revision
        .as_ref()
        .and_then(|revision| revision.trees().first().copied());
    let outcome = merge_trees(store, base_tree, left_tree, right_tree)?;
    let trees = match outcome {
        MergeOutcome::Clean(tree) => vec![tree],
        MergeOutcome::Conflict(trees) => trees,
    };
    Ok(Revision::new(
        trees,
        vec![left, right],
        left_revision.change_id(),
        left_revision.author().clone(),
        committer,
        format!("Merge {}", right_revision.change_id()),
    )?)
}

pub fn merge_trees(
    store: &RepoStore,
    base: Option<ArtifactId>,
    left: Option<ArtifactId>,
    right: Option<ArtifactId>,
) -> Result<MergeOutcome, MergeError> {
    if left == right {
        return left
            .map(MergeOutcome::Clean)
            .ok_or_else(|| MergeError::Tree(TreeError::InvalidPath("merge has no tree".into())));
    }
    if left == base {
        return right
            .map(MergeOutcome::Clean)
            .ok_or_else(|| MergeError::Tree(TreeError::InvalidPath("merge has no tree".into())));
    }
    if right == base {
        return left
            .map(MergeOutcome::Clean)
            .ok_or_else(|| MergeError::Tree(TreeError::InvalidPath("merge has no tree".into())));
    }
    let Some(left) = left else {
        return Ok(MergeOutcome::Clean(right.expect("right exists")));
    };
    let Some(right) = right else {
        return Ok(MergeOutcome::Clean(left));
    };
    let left_files = flatten_tree(store, left)?;
    let right_files = flatten_tree(store, right)?;
    let base_files = match base {
        Some(base) => flatten_tree(store, base)?,
        None => BTreeMap::new(),
    };
    let mut merged = BTreeMap::new();
    let mut conflict = false;
    for path in left_files
        .keys()
        .chain(right_files.keys())
        .chain(base_files.keys())
    {
        if merged.contains_key(path) {
            continue;
        }
        let base_entry = base_files.get(path);
        let left_entry = left_files.get(path);
        let right_entry = right_files.get(path);
        let selected = if left_entry == right_entry {
            left_entry.copied()
        } else if left_entry == base_entry {
            right_entry.copied()
        } else if right_entry == base_entry {
            left_entry.copied()
        } else {
            conflict = true;
            None
        };
        if let Some(entry) = selected {
            merged.insert(path.clone(), entry);
        }
    }
    if conflict {
        return Ok(MergeOutcome::Conflict(simplify_conflict_trees(&[
            left,
            base.unwrap_or(left),
            right,
        ])));
    }
    Ok(MergeOutcome::Clean(build_tree(store, &merged)?))
}

pub fn simplify_conflict_trees(trees: &[ArtifactId]) -> Vec<ArtifactId> {
    let mut simplified = Vec::new();
    for tree in trees {
        if simplified.last() != Some(tree) {
            simplified.push(*tree);
        }
    }
    if simplified.windows(2).all(|pair| pair[0] == pair[1]) {
        simplified.truncate(1);
    }
    simplified
}

pub fn resolve_conflict(trees: &[ArtifactId], resolved: ArtifactId) -> Vec<ArtifactId> {
    let _ = trees;
    vec![resolved]
}

fn load_revision(store: &RepoStore, id: ArtifactId) -> Result<Revision, MergeError> {
    let artifact = store.get(id)?.ok_or(MergeError::MissingRevision(id))?;
    Revision::from_artifact(artifact).map_err(|_| MergeError::NotRevision(id))
}

#[allow(dead_code)]
fn _tree_kind(kind: TreeEntryKind) -> u8 {
    kind as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tree::FileEntry, Blob, ChangeId};

    fn person() -> Person {
        Person {
            name: "A".into(),
            email: "a@x".into(),
            timestamp: 0,
        }
    }

    fn tree(store: &RepoStore, value: &[u8]) -> ArtifactId {
        let blob = Blob::from_bytes(value).expect("blob");
        store.insert(blob.artifact()).expect("blob");
        build_tree(
            store,
            &BTreeMap::from([(
                "f".into(),
                FileEntry {
                    kind: TreeEntryKind::File,
                    target: blob.id(),
                },
            )]),
        )
        .expect("tree")
    }

    #[test]
    fn content_conflict_is_data_and_clean_three_way_merge_is_built() {
        let store = RepoStore::memory().expect("store");
        let base_tree = tree(&store, b"base");
        let left_tree = tree(&store, b"left");
        let right_tree = tree(&store, b"right");
        assert!(matches!(
            merge_trees(&store, Some(base_tree), Some(left_tree), Some(right_tree)).expect("merge"),
            MergeOutcome::Conflict(_)
        ));
        assert_eq!(
            merge_trees(&store, Some(base_tree), Some(left_tree), Some(base_tree)).expect("merge"),
            MergeOutcome::Clean(left_tree)
        );
        let left = Revision::new(
            vec![left_tree],
            vec![],
            ChangeId::from_bytes([1; 16]),
            person(),
            person(),
            "left".into(),
        )
        .expect("left");
        let left = store.insert(left.artifact()).expect("left");
        let right = Revision::new(
            vec![right_tree],
            vec![],
            ChangeId::from_bytes([2; 16]),
            person(),
            person(),
            "right".into(),
        )
        .expect("right");
        let right = store.insert(right.artifact()).expect("right");
        let base = Revision::new(
            vec![base_tree],
            vec![],
            ChangeId::from_bytes([0; 16]),
            person(),
            person(),
            "base".into(),
        )
        .expect("base");
        let base = store.insert(base.artifact()).expect("base");
        let merged = merge_revisions(&store, Some(base), left, right, person()).expect("revision");
        assert!(merged.trees().len() > 1);
    }
}
