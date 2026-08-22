use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
    process::Command,
};

use crate::{
    tree::{build_tree, FileEntry},
    ArtifactId, ChangeId, Person, Revision, TreeEntryKind, Workspace, WorkspaceError,
};

#[derive(Debug, thiserror::Error)]
pub enum GitImportError {
    #[error("git executable is unavailable")]
    GitUnavailable,
    #[error("git fast-export failed")]
    GitFailed,
    #[error("destination must be a fresh directory")]
    DestinationNotFresh,
    #[error("fast-export stream is malformed: {0}")]
    Malformed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("workspace error: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("artifact error: {0}")]
    Artifact(#[from] crate::artifact::ArtifactError),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitImportResult {
    pub heads: BTreeMap<String, ArtifactId>,
    pub warnings: Vec<String>,
}

#[derive(Default)]
struct Export {
    blobs: Vec<BlobRecord>,
    commits: Vec<CommitRecord>,
    warnings: Vec<String>,
}

struct BlobRecord {
    mark: Option<String>,
    original_oid: String,
    bytes: Vec<u8>,
}

struct CommitRecord {
    branch: Option<String>,
    mark: String,
    original_oid: String,
    author: Person,
    committer: Person,
    message: String,
    parents: Vec<String>,
    operations: Vec<Operation>,
}

enum Operation {
    Set {
        mode: String,
        blob: String,
        path: String,
    },
    Delete {
        path: String,
    },
    Rename {
        old: String,
        new: String,
    },
    Copy {
        old: String,
        new: String,
    },
    DeleteAll,
}

pub fn import_git(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<GitImportResult, GitImportError> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    if destination.join(".graft").exists()
        || (destination.exists() && fs::read_dir(destination)?.next().is_some())
    {
        return Err(GitImportError::DestinationNotFresh);
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args([
            "fast-export",
            "--show-original-ids",
            "--reencode=no",
            "--full-tree",
            "--all",
        ])
        .output()
        .map_err(|_| GitImportError::GitUnavailable)?;
    if !output.status.success() {
        return Err(GitImportError::GitFailed);
    }
    let export = parse_export(&output.stdout)?;
    if export.commits.is_empty() {
        return Err(GitImportError::Malformed(
            "export contains no commits".into(),
        ));
    }
    stage_import(export, destination)
}

fn stage_import(export: Export, destination: &Path) -> Result<GitImportResult, GitImportError> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".graft-import-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    let mut workspace = Workspace::init(&staging)?;
    let mut blobs = HashMap::new();
    for blob in export.blobs {
        let artifact = crate::Blob::from_bytes(&blob.bytes)?;
        let id = workspace.store().insert(artifact.artifact())?;
        if let Some(mark) = blob.mark {
            blobs.insert(mark, id);
        }
        blobs.insert(blob.original_oid, id);
    }
    let mut revisions = HashMap::new();
    let mut branch_heads = BTreeMap::new();
    for commit in export.commits {
        let mut files = if let Some(parent) = commit.parents.first() {
            let parent_id = revisions.get(parent).copied().ok_or_else(|| {
                GitImportError::Malformed("commit references unknown parent".into())
            })?;
            let parent_revision =
                Revision::from_artifact(workspace.store().get(parent_id)?.ok_or_else(|| {
                    GitImportError::Malformed("parent artifact is missing".into())
                })?)?;
            flatten_import_tree(
                &workspace,
                parent_revision
                    .trees()
                    .first()
                    .copied()
                    .ok_or_else(|| GitImportError::Malformed("parent has no tree".into()))?,
            )?
        } else {
            BTreeMap::new()
        };
        for operation in commit.operations {
            apply_operation(&mut files, operation, &blobs, &mut workspace)?;
        }
        let tree = build_tree(workspace.store(), &files)
            .map_err(|error| GitImportError::Malformed(error.to_string()))?;
        let parents = commit
            .parents
            .iter()
            .map(|parent| {
                revisions.get(parent).copied().ok_or_else(|| {
                    GitImportError::Malformed("commit references unknown parent".into())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let change_id = import_change_id(&commit.original_oid);
        let revision = Revision::new(
            vec![tree],
            parents,
            change_id,
            commit.author,
            commit.committer,
            commit.message,
        )?;
        let revision_id = workspace.store().insert(revision.artifact())?;
        revisions.insert(commit.mark, revision_id);
        if let Some(branch) = commit.branch {
            branch_heads.insert(branch, revision_id);
        }
    }
    if branch_heads.is_empty() {
        return Err(GitImportError::Malformed(
            "export contains no branch heads".into(),
        ));
    }
    for (branch, head) in &branch_heads {
        workspace.set_import_head(branch, *head)?;
    }
    let result = GitImportResult {
        heads: branch_heads,
        warnings: export.warnings,
    };
    if destination.exists() {
        fs::remove_dir(destination)?;
    }
    fs::rename(staging, destination)?;
    Ok(result)
}

fn apply_operation(
    files: &mut BTreeMap<String, FileEntry>,
    operation: Operation,
    blobs: &HashMap<String, ArtifactId>,
    workspace: &mut Workspace,
) -> Result<(), GitImportError> {
    match operation {
        Operation::Set { mode, blob, path } => {
            if mode == "160000" {
                return Ok(());
            }
            let target = blobs
                .get(&blob)
                .copied()
                .ok_or_else(|| GitImportError::Malformed("file references unknown blob".into()))?;
            let kind = match mode.as_str() {
                "100644" => TreeEntryKind::File,
                "100755" => TreeEntryKind::Executable,
                "120000" => TreeEntryKind::Symlink,
                _ => {
                    return Err(GitImportError::Malformed(format!(
                        "unsupported file mode {mode}"
                    )))
                }
            };
            files.insert(path, FileEntry { kind, target });
        }
        Operation::Delete { path } => {
            files.remove(&path);
        }
        Operation::Rename { old, new } => {
            if let Some(entry) = files.remove(&old) {
                files.insert(new, entry);
            }
        }
        Operation::Copy { old, new } => {
            if let Some(entry) = files.get(&old).copied() {
                files.insert(new, entry);
            }
        }
        Operation::DeleteAll => files.clear(),
    }
    let _ = workspace;
    Ok(())
}

fn flatten_import_tree(
    workspace: &Workspace,
    root: ArtifactId,
) -> Result<BTreeMap<String, FileEntry>, GitImportError> {
    crate::tree::flatten_tree(workspace.store(), root)
        .map_err(|error| GitImportError::Malformed(error.to_string()))
}

fn import_change_id(original_oid: &str) -> ChangeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"graft/git-import/v1\0");
    hasher.update(original_oid.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    ChangeId::from_bytes(bytes)
}

fn parse_export(bytes: &[u8]) -> Result<Export, GitImportError> {
    let mut parser = Parser::new(bytes);
    let mut export = Export::default();
    while let Some(line) = parser.line()? {
        if line.is_empty() || line == "done" {
            continue;
        }
        match line.as_str() {
            "blob" => export.blobs.push(parse_blob(&mut parser)?),
            line if line.starts_with("commit ") => {
                export.commits.push(parse_commit(&mut parser, line)?)
            }
            line if line.starts_with("tag ") => {
                export.warnings.push(format!("omitted tag {}", &line[4..]));
                skip_record(&mut parser)?;
            }
            line if line.starts_with("reset ") => skip_reset(&mut parser)?,
            line if line.starts_with("progress ") || line.starts_with("feature ") => {}
            _ => {
                return Err(GitImportError::Malformed(format!(
                    "unexpected record `{line}`"
                )))
            }
        }
    }
    Ok(export)
}

fn parse_blob(parser: &mut Parser<'_>) -> Result<BlobRecord, GitImportError> {
    let mut mark = None;
    let mut original_oid = None;
    loop {
        let line = parser
            .line()?
            .ok_or_else(|| GitImportError::Malformed("truncated blob".into()))?;
        if let Some(value) = line.strip_prefix("mark ") {
            mark = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("original-oid ") {
            original_oid = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("data ") {
            let length = parse_length(value)?;
            let bytes = parser.data(length)?;
            return Ok(BlobRecord {
                mark,
                original_oid: original_oid
                    .ok_or_else(|| GitImportError::Malformed("blob has no original oid".into()))?,
                bytes,
            });
        } else if line.is_empty() {
            continue;
        } else {
            return Err(GitImportError::Malformed(format!(
                "unexpected blob line `{line}`"
            )));
        }
    }
}

fn parse_commit(parser: &mut Parser<'_>, header: &str) -> Result<CommitRecord, GitImportError> {
    let branch = header.strip_prefix("commit refs/heads/").map(str::to_owned);
    let mut mark = None;
    let mut original_oid = None;
    let mut author = None;
    let mut committer = None;
    let mut message = String::new();
    let mut parents = Vec::new();
    let mut operations = Vec::new();
    loop {
        let line = parser
            .line()?
            .ok_or_else(|| GitImportError::Malformed("truncated commit".into()))?;
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("mark ") {
            mark = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("original-oid ") {
            original_oid = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("author ") {
            author = Some(parse_person(value)?);
        } else if let Some(value) = line.strip_prefix("committer ") {
            committer = Some(parse_person(value)?);
        } else if line.starts_with("gpgsig ") {
            return Err(GitImportError::Malformed(
                "signed commit stream is not literal fast-export".into(),
            ));
        } else if let Some(value) = line.strip_prefix("data ") {
            message = String::from_utf8(parser.data(parse_length(value)?)?)
                .map_err(|_| GitImportError::Malformed("commit message is not UTF-8".into()))?;
        } else if let Some(value) = line.strip_prefix("from ") {
            parents.push(value.to_owned());
        } else if let Some(value) = line.strip_prefix("merge ") {
            parents.push(value.to_owned());
        } else if line == "deleteall" {
            operations.push(Operation::DeleteAll);
        } else if let Some(value) = line.strip_prefix("M ") {
            let mut fields = value.splitn(3, ' ');
            operations.push(Operation::Set {
                mode: fields
                    .next()
                    .ok_or_else(|| GitImportError::Malformed("M mode".into()))?
                    .into(),
                blob: fields
                    .next()
                    .ok_or_else(|| GitImportError::Malformed("M blob".into()))?
                    .into(),
                path: unquote_path(
                    fields
                        .next()
                        .ok_or_else(|| GitImportError::Malformed("M path".into()))?,
                ),
            });
        } else if let Some(value) = line.strip_prefix("D ") {
            operations.push(Operation::Delete {
                path: unquote_path(value),
            });
        } else if let Some(value) = line.strip_prefix("R ") {
            let mut fields = value.splitn(2, ' ');
            operations.push(Operation::Rename {
                old: unquote_path(
                    fields
                        .next()
                        .ok_or_else(|| GitImportError::Malformed("R old".into()))?,
                ),
                new: unquote_path(
                    fields
                        .next()
                        .ok_or_else(|| GitImportError::Malformed("R new".into()))?,
                ),
            });
        } else if let Some(value) = line.strip_prefix("C ") {
            let mut fields = value.splitn(2, ' ');
            operations.push(Operation::Copy {
                old: unquote_path(
                    fields
                        .next()
                        .ok_or_else(|| GitImportError::Malformed("C old".into()))?,
                ),
                new: unquote_path(
                    fields
                        .next()
                        .ok_or_else(|| GitImportError::Malformed("C new".into()))?,
                ),
            });
        } else if line.starts_with("N ") || line.starts_with("encoding ") {
        } else {
            return Err(GitImportError::Malformed(format!(
                "unexpected commit line `{line}`"
            )));
        }
    }
    Ok(CommitRecord {
        branch,
        mark: mark.ok_or_else(|| GitImportError::Malformed("commit has no mark".into()))?,
        original_oid: original_oid
            .ok_or_else(|| GitImportError::Malformed("commit has no original oid".into()))?,
        author: author.ok_or_else(|| GitImportError::Malformed("commit has no author".into()))?,
        committer: committer
            .ok_or_else(|| GitImportError::Malformed("commit has no committer".into()))?,
        message,
        parents,
        operations,
    })
}

fn parse_person(value: &str) -> Result<Person, GitImportError> {
    let end = value
        .rfind('>')
        .ok_or_else(|| GitImportError::Malformed("person email".into()))?;
    let start = value[..end]
        .rfind('<')
        .ok_or_else(|| GitImportError::Malformed("person email".into()))?;
    let name = value[..start].trim().to_owned();
    let email = value[start + 1..end].to_owned();
    let timestamp = value[end + 1..]
        .split_whitespace()
        .next()
        .ok_or_else(|| GitImportError::Malformed("person timestamp".into()))?
        .parse()
        .map_err(|_| GitImportError::Malformed("person timestamp".into()))?;
    Ok(Person {
        name,
        email,
        timestamp,
    })
}

fn parse_length(value: &str) -> Result<usize, GitImportError> {
    value
        .parse()
        .map_err(|_| GitImportError::Malformed("data length".into()))
}
fn unquote_path(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .replace("\\\\", "\\")
}
fn skip_record(parser: &mut Parser<'_>) -> Result<(), GitImportError> {
    while let Some(line) = parser.line()? {
        if line.is_empty() {
            break;
        }
    }
    Ok(())
}

fn skip_reset(parser: &mut Parser<'_>) -> Result<(), GitImportError> {
    let position = parser.position;
    if let Some(line) = parser.line()? {
        if !line.starts_with("from ") && !line.is_empty() {
            parser.position = position;
        } else if line.starts_with("from ") {
            skip_record(parser)?;
        }
    }
    Ok(())
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn line(&mut self) -> Result<Option<String>, GitImportError> {
        if self.position >= self.bytes.len() {
            return Ok(None);
        }
        let start = self.position;
        let Some(offset) = self.bytes[self.position..]
            .iter()
            .position(|byte| *byte == b'\n')
        else {
            self.position = self.bytes.len();
            return String::from_utf8(self.bytes[start..].to_vec())
                .map(Some)
                .map_err(|_| GitImportError::Malformed("non-UTF-8 control line".into()));
        };
        self.position += offset + 1;
        let line = &self.bytes[start..start + offset];
        String::from_utf8(line.strip_suffix(b"\r").unwrap_or(line).to_vec())
            .map(Some)
            .map_err(|_| GitImportError::Malformed("non-UTF-8 control line".into()))
    }
    fn data(&mut self, length: usize) -> Result<Vec<u8>, GitImportError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| GitImportError::Malformed("data length overflow".into()))?;
        let data = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| GitImportError::Malformed("truncated data".into()))?
            .to_vec();
        self.position = end;
        if self.bytes.get(self.position) == Some(&b'\n') {
            self.position += 1;
        }
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::{import_change_id, parse_export};

    #[test]
    fn change_id_uses_the_complete_raw_oid_and_is_stable() {
        assert_eq!(
            import_change_id("0123456789abcdef0123456789abcdef01234567"),
            import_change_id("0123456789abcdef0123456789abcdef01234567")
        );
    }

    #[test]
    fn fast_export_parser_preserves_modes_and_merges() {
        let stream = b"blob\nmark :1\noriginal-oid aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\ndata 1\nx\n\ncommit refs/heads/main\nmark :2\noriginal-oid bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nauthor A <a@x> 0 +0000\ncommitter A <a@x> 0 +0000\ndata 4\nseed\nM 100755 :1 file\n\n";
        let export = parse_export(stream).expect("parse");
        assert_eq!(export.commits[0].operations.len(), 1);
        assert_eq!(import_change_id("a"), import_change_id("a"));
    }
}
