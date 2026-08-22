use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    artifact::Revision,
    merge::{merge_trees, MergeOutcome},
    ArtifactId, Person, RepoStore,
};

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("revision `{0}` was not found")]
    MissingRevision(ArtifactId),
    #[error("artifact `{0}` is not a revision")]
    NotRevision(ArtifactId),
    #[error("rewriting revision `{0}` produces a tree conflict")]
    Conflict(ArtifactId),
    #[error("merge error: {0}")]
    Merge(#[from] crate::merge::MergeError),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("artifact error: {0}")]
    Artifact(#[from] crate::artifact::ArtifactError),
}

pub fn rebase_revision(
    store: &RepoStore,
    revision_id: ArtifactId,
    new_parent: ArtifactId,
    committer: Person,
) -> Result<ArtifactId, RewriteError> {
    let revision = load_revision(store, revision_id)?;
    let rewritten = Revision::new(
        revision.trees().to_vec(),
        vec![new_parent],
        revision.change_id(),
        revision.author().clone(),
        committer,
        revision.message().to_owned(),
    )?;
    Ok(store.insert(rewritten.artifact())?)
}

pub fn rewrite_descendants(
    store: &RepoStore,
    old_root: ArtifactId,
    new_root: ArtifactId,
    committer: Person,
) -> Result<BTreeMap<ArtifactId, ArtifactId>, RewriteError> {
    let mut children = BTreeMap::<ArtifactId, Vec<ArtifactId>>::new();
    for id in store.ids()? {
        let Some(artifact) = store.get(id)? else {
            continue;
        };
        let Ok(revision) = Revision::from_artifact(artifact) else {
            continue;
        };
        for parent in revision.parents() {
            children.entry(*parent).or_default().push(id);
        }
    }
    let mut mapping = BTreeMap::from([(old_root, new_root)]);
    let mut queue = VecDeque::from([old_root]);
    let mut visited = BTreeSet::new();
    while let Some(parent) = queue.pop_front() {
        if !visited.insert(parent) {
            continue;
        }
        for child_id in children.get(&parent).into_iter().flatten() {
            let child = load_revision(store, *child_id)?;
            let tree = rewritten_tree(store, *child_id, &child, &mapping)?;
            let parents = child
                .parents()
                .iter()
                .map(|id| mapping.get(id).copied().unwrap_or(*id))
                .collect();
            let rewritten = Revision::new(
                tree,
                parents,
                child.change_id(),
                child.author().clone(),
                committer.clone(),
                child.message().to_owned(),
            )?;
            let rewritten_id = store.insert(rewritten.artifact())?;
            mapping.insert(*child_id, rewritten_id);
            queue.push_back(*child_id);
        }
    }
    Ok(mapping)
}

fn rewritten_tree(
    store: &RepoStore,
    child_id: ArtifactId,
    child: &Revision,
    mapping: &BTreeMap<ArtifactId, ArtifactId>,
) -> Result<Vec<ArtifactId>, RewriteError> {
    let Some(original_parent) = child.parents().first().copied() else {
        return Ok(child.trees().to_vec());
    };
    let Some(rewritten_parent) = mapping.get(&original_parent).copied() else {
        return Ok(child.trees().to_vec());
    };
    if original_parent == rewritten_parent || child.trees().len() != 1 {
        return Ok(child.trees().to_vec());
    }
    let original_base_tree = load_revision(store, original_parent)?
        .trees()
        .first()
        .copied();
    let rewritten_parent_tree = load_revision(store, rewritten_parent)?
        .trees()
        .first()
        .copied();
    let child_tree = child.trees().first().copied();
    match merge_trees(store, original_base_tree, rewritten_parent_tree, child_tree)? {
        MergeOutcome::Clean(tree) => Ok(vec![tree]),
        MergeOutcome::Conflict(_) => Err(RewriteError::Conflict(child_id)),
    }
}

fn load_revision(store: &RepoStore, id: ArtifactId) -> Result<Revision, RewriteError> {
    let artifact = store.get(id)?.ok_or(RewriteError::MissingRevision(id))?;
    Revision::from_artifact(artifact).map_err(|_| RewriteError::NotRevision(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tree::{build_tree, FileEntry},
        Blob, ChangeId, Person, Revision, TreeEntryKind,
    };

    fn tree(store: &RepoStore, files: &[(&str, &[u8])]) -> ArtifactId {
        let entries = files
            .iter()
            .map(|(path, bytes)| {
                let blob = Blob::from_bytes(bytes).expect("blob");
                store.insert(blob.artifact()).expect("blob");
                (
                    (*path).to_owned(),
                    FileEntry {
                        kind: TreeEntryKind::File,
                        target: blob.id(),
                    },
                )
            })
            .collect();
        build_tree(store, &entries).expect("tree")
    }
    fn person() -> Person {
        Person {
            name: "A".into(),
            email: "a@x".into(),
            timestamp: 0,
        }
    }

    #[test]
    fn rebase_keeps_change_id_and_rewrites_descendants() {
        let store = RepoStore::memory().expect("store");
        let root = Revision::new(
            vec![tree(&store, &[("root", b"root")])],
            vec![],
            ChangeId::from_bytes([1; 16]),
            person(),
            person(),
            "root".into(),
        )
        .expect("root");
        let root = store.insert(root.artifact()).expect("root");
        let child = Revision::new(
            vec![tree(&store, &[("root", b"root"), ("child", b"child")])],
            vec![root],
            ChangeId::from_bytes([2; 16]),
            person(),
            person(),
            "child".into(),
        )
        .expect("child");
        let child = store.insert(child.artifact()).expect("child");
        let replacement = Revision::new(
            vec![tree(&store, &[("root", b"replacement")])],
            vec![],
            ChangeId::from_bytes([1; 16]),
            person(),
            person(),
            "root".into(),
        )
        .expect("replacement");
        let replacement = store.insert(replacement.artifact()).expect("replacement");
        let mapping = rewrite_descendants(&store, root, replacement, person()).expect("rewrite");
        let rewritten =
            Revision::from_artifact(store.get(mapping[&child]).expect("get").expect("child"))
                .expect("revision");
        assert_eq!(rewritten.change_id(), ChangeId::from_bytes([2; 16]));
        assert_eq!(rewritten.parents(), &[replacement]);
    }
}
