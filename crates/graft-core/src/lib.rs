//! Graft's transport- and UI-independent domain library.

pub mod artifact;
pub mod diff;
pub mod git_import;
pub mod graph;
pub mod land;
pub mod merge;
pub mod rewrite;
pub mod secrets;
pub mod store;
pub mod tree;
pub mod workspace;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use artifact::{
    Artifact, ArtifactId, ArtifactKind, Blob, ChangeId, Person, Revision, Tree, TreeEntry,
    TreeEntryKind,
};
pub use land::{
    discover_stack, land_stack, rebase_stack, LandError, RebasedChange, RebasedStack, StackChange,
};
pub use store::{RefUpdate, RepoStore};
pub use workspace::{Workspace, WorkspaceError, WorkspaceState, WorkspaceStatus};
