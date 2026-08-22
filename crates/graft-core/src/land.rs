use crate::{
    artifact::{ChangeId, Revision},
    graph::{GraphError, RevisionGraph},
    merge::{merge_trees, MergeError, MergeOutcome},
    ArtifactId, Person, RefUpdate, RepoStore,
};

#[derive(Debug, thiserror::Error)]
pub enum LandError {
    #[error("revision {0} was not found")]
    MissingRevision(ArtifactId),
    #[error("artifact {0} is not a revision")]
    NotRevision(ArtifactId),
    #[error("source revision is not descended from the target")]
    SourceNotDescended,
    #[error("proposal stack contains a merge revision {0}")]
    NonLinear(ArtifactId),
    #[error("Change {0} rebases with conflicts")]
    Conflict(ChangeId),
    #[error("target ref advanced while landing")]
    TargetAdvanced,
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("artifact error: {0}")]
    Artifact(#[from] crate::artifact::ArtifactError),
    #[error("graph error: {0}")]
    Graph(#[from] GraphError),
    #[error("merge error: {0}")]
    Merge(#[from] MergeError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackChange {
    pub revision_id: ArtifactId,
    pub change_id: ChangeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebasedChange {
    pub original_revision: ArtifactId,
    pub revision: ArtifactId,
    pub change_id: ChangeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebasedStack {
    pub original_tip: ArtifactId,
    pub target: Option<ArtifactId>,
    pub tip: Option<ArtifactId>,
    pub changes: Vec<RebasedChange>,
}

/// Return revisions between an optional target and descendant tip in parent-before-child order.
pub fn discover_stack(
    store: &RepoStore,
    target: Option<ArtifactId>,
    tip: ArtifactId,
) -> Result<Vec<StackChange>, LandError> {
    let graph = RevisionGraph::new(store);
    let excluded = match target {
        Some(target) => {
            let common = graph.common_ancestors(target, tip)?;
            let base = graph
                .topological(&[target])?
                .into_iter()
                .rfind(|id| common.contains(id))
                .ok_or(LandError::SourceNotDescended)?;
            graph.ancestors(base)?
        }
        None => Default::default(),
    };
    Ok(graph
        .topological(&[tip])?
        .into_iter()
        .filter_map(|revision_id| {
            if excluded.contains(&revision_id) {
                return None;
            }
            let artifact = store.get(revision_id).ok().flatten()?;
            let revision = Revision::from_artifact(artifact).ok()?;
            Some(StackChange {
                revision_id,
                change_id: revision.change_id(),
            })
        })
        .collect())
}

/// Rebase a proposal stack onto the current target without moving any refs.
pub fn rebase_stack(
    store: &RepoStore,
    target: Option<ArtifactId>,
    tip: ArtifactId,
    committer: Person,
) -> Result<RebasedStack, LandError> {
    let stack = discover_stack(store, target, tip)?;
    let mut current = target;
    let mut current_tree = target
        .map(|id| load_revision(store, id))
        .transpose()?
        .and_then(|revision| revision.trees().first().copied());
    let mut changes = Vec::with_capacity(stack.len());
    for change in stack {
        let original = load_revision(store, change.revision_id)?;
        if original.parents().len() > 1 {
            return Err(LandError::NonLinear(change.revision_id));
        }
        if original.trees().len() > 1 {
            return Err(LandError::Conflict(change.change_id));
        }
        let original_base_tree = original
            .parents()
            .first()
            .copied()
            .map(|id| load_revision(store, id))
            .transpose()?
            .and_then(|revision| revision.trees().first().copied());
        let right_tree = original.trees().first().copied();
        let tree = match merge_trees(store, original_base_tree, current_tree, right_tree)? {
            MergeOutcome::Clean(tree) => tree,
            MergeOutcome::Conflict(_) => return Err(LandError::Conflict(change.change_id)),
        };
        let rewritten = Revision::new(
            vec![tree],
            current.into_iter().collect(),
            original.change_id(),
            original.author().clone(),
            committer.clone(),
            original.message().to_owned(),
        )?;
        let revision = store.insert(rewritten.artifact())?;
        changes.push(RebasedChange {
            original_revision: change.revision_id,
            revision,
            change_id: change.change_id,
        });
        current = Some(revision);
        current_tree = Some(tree);
    }
    Ok(RebasedStack {
        original_tip: tip,
        target,
        tip: current,
        changes,
    })
}

/// Rebase and atomically advance a target ref. New artifacts are inserted before the CAS.
pub fn land_stack(
    store: &RepoStore,
    target_ref: &str,
    expected_target: Option<ArtifactId>,
    source_tip: ArtifactId,
    committer: Person,
) -> Result<RebasedStack, LandError> {
    if store.get_ref(target_ref)? != expected_target {
        return Err(LandError::TargetAdvanced);
    }
    let plan = rebase_stack(store, expected_target, source_tip, committer)?;
    store.cas_refs(&[RefUpdate {
        name: target_ref.to_owned(),
        expected: expected_target,
        new: plan.tip,
    }])?;
    Ok(plan)
}

fn load_revision(store: &RepoStore, id: ArtifactId) -> Result<Revision, LandError> {
    let artifact = store.get(id)?.ok_or(LandError::MissingRevision(id))?;
    Revision::from_artifact(artifact).map_err(|_| LandError::NotRevision(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Blob, ChangeId, TreeEntryKind};
    use std::collections::BTreeMap;

    fn person() -> Person {
        Person {
            name: "A".into(),
            email: "a@example.test".into(),
            timestamp: 0,
        }
    }

    fn revision(
        store: &RepoStore,
        parent: Option<ArtifactId>,
        value: &[u8],
        change: u8,
    ) -> ArtifactId {
        let blob = Blob::from_bytes(value).expect("blob");
        store.insert(blob.artifact()).expect("blob");
        let tree = crate::tree::build_tree(
            store,
            &BTreeMap::from([(
                format!("file-{change}"),
                crate::tree::FileEntry {
                    kind: TreeEntryKind::File,
                    target: blob.id(),
                },
            )]),
        )
        .expect("tree");
        let revision = Revision::new(
            vec![tree],
            parent.into_iter().collect(),
            ChangeId::from_bytes([change; 16]),
            person(),
            person(),
            format!("change {change}"),
        )
        .expect("revision");
        store.insert(revision.artifact()).expect("revision")
    }

    #[test]
    fn land_rebases_a_stack_and_keeps_change_ids() {
        let store = RepoStore::memory().expect("store");
        let base = revision(&store, None, b"base", 1);
        let target = revision(&store, Some(base), b"target", 2);
        let source = revision(&store, Some(base), b"source", 3);
        store.set_ref("branches/main", target).expect("target");
        let plan =
            land_stack(&store, "branches/main", Some(target), source, person()).expect("land");
        assert_eq!(plan.changes[0].change_id, ChangeId::from_bytes([3; 16]));
        assert_eq!(store.get_ref("branches/main").expect("ref"), plan.tip);
        let landed = load_revision(&store, plan.tip.expect("tip")).expect("revision");
        assert_eq!(landed.parents(), &[target]);
    }
}
