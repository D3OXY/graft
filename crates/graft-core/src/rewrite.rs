use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{artifact::Revision, ArtifactId, Person, RepoStore};

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("revision `{0}` was not found")]
    MissingRevision(ArtifactId),
    #[error("artifact `{0}` is not a revision")]
    NotRevision(ArtifactId),
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
            let parents = child
                .parents()
                .iter()
                .map(|id| mapping.get(id).copied().unwrap_or(*id))
                .collect();
            let rewritten = Revision::new(
                child.trees().to_vec(),
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

fn load_revision(store: &RepoStore, id: ArtifactId) -> Result<Revision, RewriteError> {
    let artifact = store.get(id)?.ok_or(RewriteError::MissingRevision(id))?;
    Revision::from_artifact(artifact).map_err(|_| RewriteError::NotRevision(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangeId, Person, Revision};

    fn id(byte: u8) -> ArtifactId {
        ArtifactId::from_bytes([byte; 32])
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
            vec![id(1)],
            vec![],
            ChangeId::from_bytes([1; 16]),
            person(),
            person(),
            "root".into(),
        )
        .expect("root");
        let root = store.insert(root.artifact()).expect("root");
        let child = Revision::new(
            vec![id(2)],
            vec![root],
            ChangeId::from_bytes([2; 16]),
            person(),
            person(),
            "child".into(),
        )
        .expect("child");
        let child = store.insert(child.artifact()).expect("child");
        let replacement = Revision::new(
            vec![id(9)],
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
