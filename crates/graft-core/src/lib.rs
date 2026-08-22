//! Graft's transport- and UI-independent domain library.

pub mod artifact;
pub mod store;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use artifact::{Artifact, ArtifactId, ArtifactKind, Blob, Revision, Tree, TreeEntry};
pub use store::{RefUpdate, RepoStore};
