use std::{collections::BTreeMap, path::Path};

use crate::{ArtifactId, RepoStore, Tree, TreeEntry, TreeEntryKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileEntry {
    pub kind: TreeEntryKind,
    pub target: ArtifactId,
}

#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error("path `{0}` is empty or invalid")]
    InvalidPath(String),
    #[error("path `{0}` collides with a file")]
    PathCollision(String),
    #[error("tree `{0}` was not found")]
    MissingTree(ArtifactId),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("artifact error: {0}")]
    Artifact(#[from] crate::artifact::ArtifactError),
}

#[derive(Default)]
struct Node {
    file: Option<FileEntry>,
    children: BTreeMap<String, Node>,
}

pub fn build_tree(
    store: &RepoStore,
    files: &BTreeMap<String, FileEntry>,
) -> Result<ArtifactId, TreeError> {
    let mut root = Node::default();
    for (path, file) in files {
        let components = split_path(path)?;
        let mut node = &mut root;
        for component in components {
            if node.file.is_some() {
                return Err(TreeError::PathCollision(path.clone()));
            }
            node = node.children.entry(component.to_owned()).or_default();
        }
        if node.file.is_some() || !node.children.is_empty() {
            return Err(TreeError::PathCollision(path.clone()));
        }
        node.file = Some(*file);
    }
    build_node(store, &root)
}

pub fn flatten_tree(
    store: &RepoStore,
    root: ArtifactId,
) -> Result<BTreeMap<String, FileEntry>, TreeError> {
    let mut output = BTreeMap::new();
    walk_tree(store, root, Path::new(""), &mut output)?;
    Ok(output)
}

fn build_node(store: &RepoStore, node: &Node) -> Result<ArtifactId, TreeError> {
    let mut entries = Vec::with_capacity(node.children.len());
    for (name, child) in &node.children {
        let (kind, target) = match (child.file, child.children.is_empty()) {
            (Some(file), true) => (file.kind, file.target),
            (None, false) => (TreeEntryKind::Tree, build_node(store, child)?),
            _ => return Err(TreeError::PathCollision(name.clone())),
        };
        entries.push(TreeEntry {
            name: name.clone(),
            kind,
            target,
        });
    }
    let tree = Tree::new(entries)?;
    store.insert(tree.artifact()).map_err(TreeError::from)
}

fn walk_tree(
    store: &RepoStore,
    id: ArtifactId,
    prefix: &Path,
    output: &mut BTreeMap<String, FileEntry>,
) -> Result<(), TreeError> {
    let artifact = store.get(id)?.ok_or(TreeError::MissingTree(id))?;
    let tree = Tree::from_artifact(artifact)?;
    for entry in tree.entries() {
        let path = prefix.join(&entry.name);
        if entry.kind == TreeEntryKind::Tree {
            walk_tree(store, entry.target, &path, output)?;
        } else {
            let path = path
                .to_str()
                .ok_or_else(|| TreeError::InvalidPath(path.display().to_string()))?;
            output.insert(
                path.to_owned(),
                FileEntry {
                    kind: entry.kind,
                    target: entry.target,
                },
            );
        }
    }
    Ok(())
}

fn split_path(path: &str) -> Result<Vec<&str>, TreeError> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') || path.contains('\0') {
        return Err(TreeError::InvalidPath(path.to_owned()));
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components
        .iter()
        .any(|component| component.is_empty() || *component == "." || *component == "..")
    {
        return Err(TreeError::InvalidPath(path.to_owned()));
    }
    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Blob;

    #[test]
    fn nested_tree_builds_and_flattens() {
        let store = RepoStore::memory().expect("store");
        let one = Blob::from_bytes(b"one").expect("blob");
        let two = Blob::from_bytes(b"two").expect("blob");
        store.insert(one.artifact()).expect("one");
        store.insert(two.artifact()).expect("two");
        let files = BTreeMap::from([
            (
                "src/main.rs".to_owned(),
                FileEntry {
                    kind: TreeEntryKind::Executable,
                    target: one.id(),
                },
            ),
            (
                "README".to_owned(),
                FileEntry {
                    kind: TreeEntryKind::File,
                    target: two.id(),
                },
            ),
        ]);
        let root = build_tree(&store, &files).expect("tree");
        assert_eq!(flatten_tree(&store, root).expect("flatten"), files);
    }
}
