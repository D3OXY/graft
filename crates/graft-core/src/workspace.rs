use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    artifact::{ArtifactError, ChangeId, Person, Revision},
    diff::{diff_trees, DiffError, FileChange},
    graph::{GraphError, RevisionGraph},
    merge::{merge_trees, MergeError, MergeOutcome},
    rewrite::{rewrite_descendants, RewriteError},
    tree::{build_tree, flatten_tree, FileEntry, TreeError},
    ArtifactId, RepoStore, TreeEntryKind,
};

const GRAFT_DIR: &str = ".graft";
const STORE_FILE: &str = "store.sqlite";
const STATE_FILE: &str = "state.json";
const OPLOG_FILE: &str = "oplog.jsonl";

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("workspace is already initialized at `{0}`")]
    AlreadyInitialized(PathBuf),
    #[error("workspace is not initialized at `{0}`")]
    NotInitialized(PathBuf),
    #[error("workspace state is invalid: {0}")]
    InvalidState(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON state error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("artifact error: {0}")]
    Artifact(#[from] ArtifactError),
    #[error("tree error: {0}")]
    Tree(#[from] TreeError),
    #[error("diff error: {0}")]
    Diff(#[from] DiffError),
    #[error("merge error: {0}")]
    Merge(#[from] MergeError),
    #[error("rewrite error: {0}")]
    Rewrite(#[from] RewriteError),
    #[error("secrets error: {0}")]
    Secrets(#[from] crate::secrets::SecretError),
    #[error("graph error: {0}")]
    Graph(#[from] GraphError),
    #[error("branch `{0}` does not exist")]
    MissingBranch(String),
    #[error("revision for change `{0}` was not found")]
    MissingChange(String),
    #[error("there is nothing to undo")]
    NothingToUndo,
    #[error("working copy contains a path that cannot be represented")]
    UnrepresentableWorkingCopy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceState {
    pub active_branch: String,
    pub working_change: String,
    pub working_tree: String,
    pub working_parents: Vec<String>,
    pub conflict_trees: Vec<String>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Operation {
    state: WorkspaceState,
    refs: Vec<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceStatus {
    pub changes: Vec<FileChange>,
    pub conflicts: bool,
}

pub struct Workspace {
    root: PathBuf,
    store: RepoStore,
    state: WorkspaceState,
}

impl Workspace {
    pub fn init(path: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = path.as_ref().to_path_buf();
        let graft = root.join(GRAFT_DIR);
        if graft.exists() {
            return Err(WorkspaceError::AlreadyInitialized(root));
        }
        fs::create_dir_all(&graft)?;
        let store = RepoStore::open(graft.join(STORE_FILE))?;
        let empty_tree = crate::Tree::new(Vec::new())?;
        let empty_tree_id = store.insert(empty_tree.artifact())?;
        let state = WorkspaceState {
            active_branch: "main".into(),
            working_change: ChangeId::random().to_string(),
            working_tree: empty_tree_id.to_string(),
            working_parents: Vec::new(),
            conflict_trees: Vec::new(),
            message: String::new(),
        };
        let workspace = Self { root, store, state };
        workspace.save_state()?;
        Ok(workspace)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = path.as_ref().to_path_buf();
        let graft = root.join(GRAFT_DIR);
        if !graft.is_dir() {
            return Err(WorkspaceError::NotInitialized(root));
        }
        let state = serde_json::from_slice(&fs::read(graft.join(STATE_FILE))?)?;
        let store = RepoStore::open(graft.join(STORE_FILE))?;
        let workspace = Self { root, store, state };
        workspace.validate_state()?;
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn store(&self) -> &RepoStore {
        &self.store
    }

    pub fn state(&self) -> &WorkspaceState {
        &self.state
    }

    pub fn status(&self) -> Result<WorkspaceStatus, WorkspaceError> {
        let current = self.snapshot_disk()?;
        let expected = self.working_tree()?;
        let changes = diff_trees(&self.store, Some(expected), Some(current))?;
        Ok(WorkspaceStatus {
            changes,
            conflicts: !self.state.conflict_trees.is_empty(),
        })
    }

    pub fn snap(&mut self, message: impl Into<String>) -> Result<ArtifactId, WorkspaceError> {
        self.mutate(|workspace| {
            let current_tree = workspace.snapshot_disk()?;
            let parents = workspace.parse_ids(&workspace.state.working_parents)?;
            let change_id = workspace.parse_change_id(&workspace.state.working_change)?;
            let person = default_person();
            let revision = Revision::new(
                vec![current_tree],
                parents,
                change_id,
                person.clone(),
                person,
                message.into(),
            )?;
            let revision_id = workspace.store.insert(revision.artifact())?;
            let branch_ref = branch_ref(&workspace.state.active_branch);
            let expected = workspace.store.get_ref(&branch_ref)?;
            workspace.store.cas_refs(&[crate::RefUpdate {
                name: branch_ref,
                expected,
                new: Some(revision_id),
            }])?;
            workspace.state.working_change = ChangeId::random().to_string();
            workspace.state.working_tree = current_tree.to_string();
            workspace.state.working_parents = vec![revision_id.to_string()];
            workspace.state.conflict_trees.clear();
            workspace.state.message.clear();
            Ok(revision_id)
        })
    }

    pub fn describe(&mut self, message: impl Into<String>) -> Result<(), WorkspaceError> {
        self.mutate(|workspace| {
            workspace.snapshot_disk()?;
            workspace.state.message = message.into();
            Ok(())
        })
    }

    pub fn amend(&mut self) -> Result<ArtifactId, WorkspaceError> {
        self.mutate(|workspace| {
            let current_tree = workspace.snapshot_disk()?;
            let old_id = workspace
                .state
                .working_parents
                .first()
                .ok_or_else(|| WorkspaceError::MissingChange("working parent".into()))
                .and_then(|value| {
                    value
                        .parse()
                        .map_err(|_| WorkspaceError::InvalidState("working parent".into()))
                })?;
            let old = workspace.load_revision(old_id)?;
            let person = default_person();
            let amended = Revision::new(
                vec![current_tree],
                old.parents().to_vec(),
                old.change_id(),
                old.author().clone(),
                person,
                if workspace.state.message.is_empty() {
                    old.message().to_owned()
                } else {
                    workspace.state.message.clone()
                },
            )?;
            let amended_id = workspace.store.insert(amended.artifact())?;
            let mapping =
                rewrite_descendants(&workspace.store, old_id, amended_id, default_person())?;
            workspace.update_refs(&mapping)?;
            workspace.state.working_tree = current_tree.to_string();
            workspace.state.working_parents = vec![amended_id.to_string()];
            workspace.state.conflict_trees.clear();
            Ok(amended_id)
        })
    }

    pub fn edit(&mut self, selector: &str) -> Result<(), WorkspaceError> {
        self.mutate(|workspace| {
            let revision_id = workspace.find_change(selector)?;
            let revision = workspace.load_revision(revision_id)?;
            let tree = revision
                .trees()
                .first()
                .copied()
                .ok_or_else(|| WorkspaceError::InvalidState("revision has no tree".into()))?;
            workspace.state.working_change = revision.change_id().to_string();
            workspace.state.working_tree = tree.to_string();
            workspace.state.working_parents =
                revision.parents().iter().map(ToString::to_string).collect();
            workspace.state.conflict_trees.clear();
            workspace.state.message = revision.message().to_owned();
            materialize_tree(&workspace.store, &workspace.root, tree, None)?;
            Ok(())
        })
    }

    pub fn branch(&mut self, name: &str) -> Result<(), WorkspaceError> {
        validate_branch(name)?;
        self.mutate(|workspace| {
            workspace.snapshot_disk()?;
            let current = workspace
                .store
                .get_ref(&branch_ref(&workspace.state.active_branch))?;
            let new_ref = branch_ref(name);
            if workspace.store.get_ref(&new_ref)?.is_some() {
                return Err(WorkspaceError::InvalidState(format!(
                    "branch `{name}` already exists"
                )));
            }
            if let Some(current) = current {
                workspace.store.cas_refs(&[crate::RefUpdate {
                    name: new_ref,
                    expected: None,
                    new: Some(current),
                }])?;
            }
            workspace.state.active_branch = name.to_owned();
            Ok(())
        })
    }

    pub fn switch(&mut self, name: &str) -> Result<(), WorkspaceError> {
        validate_branch(name)?;
        self.mutate(|workspace| {
            workspace.snapshot_disk()?;
            let target = workspace
                .store
                .get_ref(&branch_ref(name))?
                .ok_or_else(|| WorkspaceError::MissingBranch(name.to_owned()))?;
            let revision = workspace.load_revision(target)?;
            let tree = revision.trees().first().copied().ok_or_else(|| {
                WorkspaceError::InvalidState("branch revision has no tree".into())
            })?;
            workspace.state.active_branch = name.to_owned();
            workspace.state.working_change = ChangeId::random().to_string();
            workspace.state.working_tree = tree.to_string();
            workspace.state.working_parents = vec![target.to_string()];
            workspace.state.conflict_trees.clear();
            workspace.state.message.clear();
            materialize_tree(&workspace.store, &workspace.root, tree, None)?;
            Ok(())
        })
    }

    pub fn merge(&mut self, name: &str) -> Result<bool, WorkspaceError> {
        validate_branch(name)?;
        self.mutate(|workspace| {
            let current_tree = workspace.snapshot_disk()?;
            let left = workspace
                .state
                .working_parents
                .first()
                .and_then(|value| value.parse().ok());
            let right = workspace
                .store
                .get_ref(&branch_ref(name))?
                .ok_or_else(|| WorkspaceError::MissingBranch(name.to_owned()))?;
            let right_revision = workspace.load_revision(right)?;
            let right_tree = right_revision.trees().first().copied();
            let left_tree = match left {
                Some(id) => workspace.load_revision(id)?.trees().first().copied(),
                None => Some(current_tree),
            };
            let base = left.and_then(|left| {
                RevisionGraph::new(&workspace.store)
                    .common_ancestors(left, right)
                    .ok()?
                    .into_iter()
                    .next()
            });
            let base_tree =
                base.and_then(|id| workspace.load_revision(id).ok()?.trees().first().copied());
            let outcome = merge_trees(&workspace.store, base_tree, left_tree, right_tree)?;
            workspace.state.working_change = ChangeId::random().to_string();
            workspace.state.working_parents = [left, Some(right)]
                .into_iter()
                .flatten()
                .map(|id| id.to_string())
                .collect();
            workspace.state.message = format!("Merge {name}");
            let conflicted = match outcome {
                MergeOutcome::Clean(tree) => {
                    workspace.state.working_tree = tree.to_string();
                    workspace.state.conflict_trees.clear();
                    materialize_tree(&workspace.store, &workspace.root, tree, None)?;
                    false
                }
                MergeOutcome::Conflict(trees) => {
                    workspace.state.working_tree = trees
                        .first()
                        .copied()
                        .ok_or_else(|| WorkspaceError::InvalidState("empty conflict".into()))?
                        .to_string();
                    workspace.state.conflict_trees =
                        trees.iter().map(ToString::to_string).collect();
                    materialize_tree(&workspace.store, &workspace.root, trees[0], Some(&trees))?;
                    true
                }
            };
            Ok(conflicted)
        })
    }

    pub fn undo(&mut self) -> Result<(), WorkspaceError> {
        let path = self.oplog_path();
        let contents = fs::read_to_string(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                WorkspaceError::NothingToUndo
            } else {
                WorkspaceError::Io(error)
            }
        })?;
        let mut lines = contents.lines().collect::<Vec<_>>();
        let last = lines.pop().ok_or(WorkspaceError::NothingToUndo)?;
        let operation: Operation = serde_json::from_str(last)?;
        let refs = operation
            .refs
            .iter()
            .map(|(name, id)| {
                Ok((
                    name.clone(),
                    id.parse()
                        .map_err(|_| WorkspaceError::InvalidState("oplog ref".into()))?,
                ))
            })
            .collect::<Result<Vec<_>, WorkspaceError>>()?;
        self.store.replace_refs(&refs)?;
        self.state = operation.state;
        self.save_state()?;
        materialize_tree(&self.store, &self.root, self.working_tree()?, None)?;
        let mut file = fs::File::create(path)?;
        if !lines.is_empty() {
            writeln!(file, "{}", lines.join("\n"))?;
        }
        Ok(())
    }

    pub fn rebuild(&self) -> Result<(), WorkspaceError> {
        self.store.rebuild()?;
        Ok(())
    }

    pub fn refs(&self) -> Result<Vec<(String, ArtifactId)>, WorkspaceError> {
        Ok(self.store.refs()?)
    }

    pub fn set_import_head(
        &mut self,
        branch: &str,
        revision_id: ArtifactId,
    ) -> Result<(), WorkspaceError> {
        let revision = self.load_revision(revision_id)?;
        let tree =
            revision.trees().first().copied().ok_or_else(|| {
                WorkspaceError::InvalidState("imported revision has no tree".into())
            })?;
        let reference = branch_ref(branch);
        let expected = self.store.get_ref(&reference)?;
        self.store.cas_refs(&[crate::RefUpdate {
            name: reference,
            expected,
            new: Some(revision_id),
        }])?;
        self.state.active_branch = branch.to_owned();
        self.state.working_change = ChangeId::random().to_string();
        self.state.working_tree = tree.to_string();
        self.state.working_parents = vec![revision_id.to_string()];
        self.state.conflict_trees.clear();
        self.state.message.clear();
        materialize_tree(&self.store, &self.root, tree, None)?;
        self.save_state()?;
        Ok(())
    }

    pub fn log(&self) -> Result<Vec<ArtifactId>, WorkspaceError> {
        let refs = self.store.refs()?;
        let roots = refs
            .iter()
            .filter(|(name, _)| name.starts_with("branches/"))
            .map(|(_, id)| *id)
            .collect::<Vec<_>>();
        Ok(RevisionGraph::new(&self.store).topological(&roots)?)
    }

    fn mutate<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let before = Operation {
            state: self.state.clone(),
            refs: self
                .store
                .refs()?
                .into_iter()
                .map(|(name, id)| (name, id.to_string()))
                .collect(),
        };
        match action(self) {
            Ok(value) => {
                self.save_state()?;
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.oplog_path())?;
                writeln!(file, "{}", serde_json::to_string(&before)?)?;
                Ok(value)
            }
            Err(error) => {
                let refs = before
                    .refs
                    .iter()
                    .map(|(name, id)| {
                        Ok((
                            name.clone(),
                            id.parse()
                                .map_err(|_| WorkspaceError::InvalidState("oplog ref".into()))?,
                        ))
                    })
                    .collect::<Result<Vec<_>, WorkspaceError>>()?;
                self.store.replace_refs(&refs)?;
                self.state = before.state;
                let _ = self.save_state();
                Err(error)
            }
        }
    }

    fn snapshot_disk(&self) -> Result<ArtifactId, WorkspaceError> {
        let mut files = BTreeMap::new();
        collect_files(&self.store, &self.root, &self.root, &mut files)?;
        Ok(build_tree(&self.store, &files)?)
    }

    fn update_refs(
        &self,
        mapping: &BTreeMap<ArtifactId, ArtifactId>,
    ) -> Result<(), WorkspaceError> {
        let refs = self.store.refs()?;
        for (name, target) in refs {
            if let Some(replacement) = mapping.get(&target) {
                self.store.cas_refs(&[crate::RefUpdate {
                    name,
                    expected: Some(target),
                    new: Some(*replacement),
                }])?;
            }
        }
        Ok(())
    }

    fn working_tree(&self) -> Result<ArtifactId, WorkspaceError> {
        self.parse_id(&self.state.working_tree)
    }

    fn parse_id(&self, value: &str) -> Result<ArtifactId, WorkspaceError> {
        value
            .parse()
            .map_err(|_| WorkspaceError::InvalidState(value.to_owned()))
    }

    fn parse_ids(&self, values: &[String]) -> Result<Vec<ArtifactId>, WorkspaceError> {
        values.iter().map(|value| self.parse_id(value)).collect()
    }

    fn parse_change_id(&self, value: &str) -> Result<ChangeId, WorkspaceError> {
        value
            .parse()
            .map_err(|_| WorkspaceError::InvalidState(value.to_owned()))
    }

    fn load_revision(&self, id: ArtifactId) -> Result<Revision, WorkspaceError> {
        let artifact = self
            .store
            .get(id)?
            .ok_or_else(|| WorkspaceError::MissingChange(id.to_string()))?;
        Ok(Revision::from_artifact(artifact)?)
    }

    fn find_change(&self, selector: &str) -> Result<ArtifactId, WorkspaceError> {
        for id in self.store.ids()? {
            let Some(artifact) = self.store.get(id)? else {
                continue;
            };
            let Ok(revision) = Revision::from_artifact(artifact) else {
                continue;
            };
            let change = revision.change_id();
            if change.to_string().starts_with(selector)
                || change.to_letter_string().starts_with(selector)
            {
                return Ok(id);
            }
        }
        Err(WorkspaceError::MissingChange(selector.to_owned()))
    }

    fn validate_state(&self) -> Result<(), WorkspaceError> {
        self.parse_id(&self.state.working_tree)?;
        self.parse_change_id(&self.state.working_change)?;
        self.parse_ids(&self.state.working_parents)?;
        self.parse_ids(&self.state.conflict_trees)?;
        Ok(())
    }

    fn save_state(&self) -> Result<(), WorkspaceError> {
        let path = self.state_path();
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.state)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    fn state_path(&self) -> PathBuf {
        self.root.join(GRAFT_DIR).join(STATE_FILE)
    }
    fn oplog_path(&self) -> PathBuf {
        self.root.join(GRAFT_DIR).join(OPLOG_FILE)
    }
}

fn default_person() -> Person {
    Person {
        name: std::env::var("GRAFT_AUTHOR_NAME").unwrap_or_else(|_| "Graft User".into()),
        email: std::env::var("GRAFT_AUTHOR_EMAIL").unwrap_or_else(|_| "user@graft.local".into()),
        timestamp: 0,
    }
}

fn branch_ref(name: &str) -> String {
    format!("branches/{name}")
}

fn validate_branch(name: &str) -> Result<(), WorkspaceError> {
    if name.is_empty() || name.contains('/') || name.contains('\0') || name == "." || name == ".." {
        return Err(WorkspaceError::InvalidState("invalid branch name".into()));
    }
    Ok(())
}

fn collect_files(
    store: &RepoStore,
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, FileEntry>,
) -> Result<(), WorkspaceError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if path == root.join(GRAFT_DIR) || name == ".env" {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| WorkspaceError::UnrepresentableWorkingCopy)?;
        let relative = relative
            .to_str()
            .ok_or(WorkspaceError::UnrepresentableWorkingCopy)?
            .replace(std::path::MAIN_SEPARATOR, "/");
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_files(store, root, &path, files)?;
            continue;
        }
        let (kind, bytes) = if metadata.file_type().is_symlink() {
            (
                TreeEntryKind::Symlink,
                fs::read_link(&path)?.to_string_lossy().as_bytes().to_vec(),
            )
        } else {
            let executable = metadata.permissions().mode() & 0o111 != 0;
            (
                if executable {
                    TreeEntryKind::Executable
                } else {
                    TreeEntryKind::File
                },
                fs::read(&path)?,
            )
        };
        let blob = crate::Blob::from_bytes(&bytes)?;
        store.insert(blob.artifact())?;
        files.insert(
            relative,
            FileEntry {
                kind,
                target: blob.id(),
            },
        );
    }
    Ok(())
}

fn materialize_tree(
    store: &RepoStore,
    root: &Path,
    tree: ArtifactId,
    conflict_trees: Option<&[ArtifactId]>,
) -> Result<(), WorkspaceError> {
    let files = flatten_tree(store, tree)?;
    let tracked = files.keys().cloned().collect::<BTreeSet<_>>();
    for path in files.keys() {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        let entry = files.get(path).expect("path just iterated");
        let artifact = store
            .get(entry.target)?
            .ok_or(WorkspaceError::UnrepresentableWorkingCopy)?;
        match entry.kind {
            TreeEntryKind::Symlink => {
                let target = String::from_utf8(artifact.payload().to_vec())
                    .map_err(|_| WorkspaceError::UnrepresentableWorkingCopy)?;
                let _ = fs::remove_file(&full);
                std::os::unix::fs::symlink(target, full)?;
            }
            _ => {
                fs::write(&full, artifact.payload())?;
                if entry.kind == TreeEntryKind::Executable {
                    let mut permissions = fs::metadata(&full)?.permissions();
                    permissions.set_mode(0o755);
                    fs::set_permissions(&full, permissions)?;
                }
            }
        }
    }
    if let Some(conflicts) = conflict_trees {
        if conflicts.len() > 1 {
            // The first tree remains the canonical data view. Conflict metadata stays in
            // the workspace state until the next snapshot resolves it.
        }
    }
    let _ = tracked;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("graft-workspace-{suffix}"))
    }

    #[test]
    fn init_snap_branch_switch_and_undo_restore_state() {
        let root = temp_dir();
        fs::create_dir_all(&root).expect("dir");
        let mut workspace = Workspace::init(&root).expect("init");
        fs::write(root.join("hello.txt"), b"hello").expect("file");
        workspace.snap("seed").expect("snap");
        assert_eq!(workspace.refs().expect("refs").len(), 1);
        workspace.branch("dev").expect("branch");
        fs::write(root.join("hello.txt"), b"dev").expect("edit");
        let dev_head = workspace.snap("dev").expect("snap");
        workspace.switch("main").expect("switch");
        assert_eq!(fs::read(root.join("hello.txt")).expect("read"), b"hello");
        workspace.undo().expect("undo");
        assert_eq!(workspace.state().active_branch, "dev");
        assert_eq!(
            workspace.store().get_ref("branches/dev").expect("ref"),
            Some(dev_head)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn env_file_is_not_snapshotted() {
        let root = temp_dir();
        fs::create_dir_all(&root).expect("dir");
        let mut workspace = Workspace::init(&root).expect("init");
        fs::write(root.join(".env"), b"SECRET=hidden").expect("env");
        workspace.snap("empty").expect("snap");
        let status = workspace.status().expect("status");
        assert!(status.changes.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
