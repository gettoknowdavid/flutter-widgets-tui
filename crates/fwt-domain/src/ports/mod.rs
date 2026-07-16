//! Port trait definitions — the seam `fwt-app` depends on and `fwt-infra`
//! implements, per TRD Section 2.1's dependency rule. Nothing in this
//! module or its children performs I/O itself; these are interfaces only.

pub mod catalog_repository;

pub use catalog_repository::{CatalogRepository, RepositoryError};
