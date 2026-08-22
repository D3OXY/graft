//! Graft's transport- and UI-independent domain library.

pub mod artifact;
pub mod diff;
pub mod graph;
pub mod merge;
pub mod rewrite;
pub mod store;
pub mod tree;
pub mod workspace;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use artifact::{
    Artifact, ArtifactId, ArtifactKind, Blob, ChangeId, Person, Revision, Tree, TreeEntry,
    TreeEntryKind,
};
pub use store::{RefUpdate, RepoStore};
