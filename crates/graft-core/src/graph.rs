use std::collections::{BTreeMap, BTreeSet};

use crate::{artifact::Revision, ArtifactId, RepoStore};

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("revision `{0}` was not found")]
    MissingRevision(ArtifactId),
    #[error("artifact `{0}` is not a revision")]
    NotRevision(ArtifactId),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("revision graph contains a cycle at `{0}`")]
    Cycle(ArtifactId),
}

pub struct RevisionGraph<'a> {
    store: &'a RepoStore,
}

impl<'a> RevisionGraph<'a> {
    pub fn new(store: &'a RepoStore) -> Self {
        Self { store }
    }

    pub fn revision(&self, id: ArtifactId) -> Result<Revision, GraphError> {
        let artifact = self.store.get(id)?.ok_or(GraphError::MissingRevision(id))?;
        Revision::from_artifact(artifact).map_err(|_| GraphError::NotRevision(id))
    }

    pub fn ancestors(&self, root: ArtifactId) -> Result<BTreeSet<ArtifactId>, GraphError> {
        let mut found = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        self.visit(root, &mut found, &mut visiting)?;
        Ok(found)
    }

    pub fn is_ancestor(
        &self,
        ancestor: ArtifactId,
        descendant: ArtifactId,
    ) -> Result<bool, GraphError> {
        if ancestor == descendant {
            return Ok(true);
        }
        Ok(self.ancestors(descendant)?.contains(&ancestor))
    }

    pub fn common_ancestors(
        &self,
        left: ArtifactId,
        right: ArtifactId,
    ) -> Result<BTreeSet<ArtifactId>, GraphError> {
        let left_ancestors = self.ancestors(left)?;
        let right_ancestors = self.ancestors(right)?;
        Ok(left_ancestors
            .intersection(&right_ancestors)
            .copied()
            .collect())
    }

    pub fn topological(&self, roots: &[ArtifactId]) -> Result<Vec<ArtifactId>, GraphError> {
        let mut states = BTreeMap::new();
        let mut ordered = Vec::new();
        for root in roots {
            self.visit_topological(*root, &mut states, &mut ordered)?;
        }
        Ok(ordered)
    }

    fn visit(
        &self,
        id: ArtifactId,
        found: &mut BTreeSet<ArtifactId>,
        visiting: &mut BTreeSet<ArtifactId>,
    ) -> Result<(), GraphError> {
        if !found.insert(id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err(GraphError::Cycle(id));
        }
        let revision = self.revision(id)?;
        for parent in revision.parents() {
            self.visit(*parent, found, visiting)?;
        }
        visiting.remove(&id);
        Ok(())
    }

    fn visit_topological(
        &self,
        id: ArtifactId,
        states: &mut BTreeMap<ArtifactId, VisitState>,
        ordered: &mut Vec<ArtifactId>,
    ) -> Result<(), GraphError> {
        match states.get(&id) {
            Some(VisitState::Done) => return Ok(()),
            Some(VisitState::Visiting) => return Err(GraphError::Cycle(id)),
            None => {}
        }
        states.insert(id, VisitState::Visiting);
        let revision = self.revision(id)?;
        for parent in revision.parents() {
            self.visit_topological(*parent, states, ordered)?;
        }
        states.insert(id, VisitState::Done);
        ordered.push(id);
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum VisitState {
    Visiting,
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangeId, Person, Revision};

    fn id(byte: u8) -> ArtifactId {
        ArtifactId::from_bytes([byte; 32])
    }

    fn revision(store: &RepoStore, byte: u8, parents: &[ArtifactId]) -> ArtifactId {
        let revision = Revision::new(
            vec![id(1)],
            parents.to_vec(),
            ChangeId::from_bytes([byte; 16]),
            Person {
                name: "A".into(),
                email: "a@x".into(),
                timestamp: i64::from(byte),
            },
            Person {
                name: "A".into(),
                email: "a@x".into(),
                timestamp: i64::from(byte),
            },
            format!("r{byte}"),
        )
        .expect("revision");
        store.insert(revision.artifact()).expect("insert")
    }

    #[test]
    fn n_ary_ancestry_is_walked_once_and_ordered() {
        let store = RepoStore::memory().expect("store");
        let root = revision(&store, 1, &[]);
        let left = revision(&store, 2, &[root]);
        let right = revision(&store, 3, &[root]);
        let merge = revision(&store, 4, &[left, right]);
        let graph = RevisionGraph::new(&store);
        assert_eq!(graph.ancestors(merge).expect("ancestors").len(), 4);
        assert!(graph.is_ancestor(root, merge).expect("ancestor"));
        assert_eq!(
            graph.topological(&[merge]).expect("topological"),
            vec![root, left, right, merge]
        );
    }
}
