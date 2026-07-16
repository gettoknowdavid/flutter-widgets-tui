//! The `CatalogRepository` port — how the application layer reads
//! `catalog.db`'s content without knowing SQL exists. The concrete
//! `SqliteCatalogRepository` adapter lives in `fwt-infra::db::catalog_repo`
//! (Ticket 005, Phase 3/4); nothing in this file ever imports `rusqlite`.
//!
//! ## Async trait mechanism (decided here, not left implicit)
//! This trait uses `#[async_trait::async_trait]` rather than native
//! `async fn` in traits, specifically because callers need
//! `Arc<dyn CatalogRepository>` (object-safe dynamic dispatch) for
//! dependency injection from `fwt-cli`'s composition root — native
//! `async fn` in traits is not object-safe as of this workspace's Rust
//! edition/toolchain. `async-trait` is a compile-time-only proc-macro
//! dependency (no I/O, no runtime), consistent with this crate's "zero
//! I/O" constraint.

use crate::widget::{
    CategorySummary, CodeSample, Method, Property, SearchCorpusEntry, Widget, WidgetId,
    WidgetSummary,
};

/// Errors any `CatalogRepository` implementation can return.
///
/// `NotFound` is reserved for cases where an *expected-to-exist* record
/// is missing (e.g., a required, `catalog_meta` key) — routine "no row for
/// this id" lookups return `Ok(None)`/`Ok(vec![])`, per the ticket's
/// explicit instruction that absence is not an error.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("expected catalog record not found: {0}")]
    NotFound(String),

    #[error("catalog query failed")]
    QueryFailed(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("failed to parse stored JSON column `{column}`")]
    Serialization {
        column: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },

    #[error("connection pool exhausted")]
    PoolExhausted,

    #[error("failed to establish a catalog.db connection")]
    ConnectionFailed(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("a background task executing this query panicked or was cancelled")]
    TaskJoinFailed,
}

#[async_trait::async_trait]
pub trait CatalogRepository: Send + Sync {
    async fn get_widget_by_id(&self, id: WidgetId) -> Result<Option<Widget>, RepositoryError>;

    async fn get_widget_by_name(&self, name: &str) -> Result<Option<Widget>, RepositoryError>;

    async fn list_categories(&self) -> Result<Vec<CategorySummary>, RepositoryError>;

    async fn list_widgets_by_category(
        &self,
        category: &str,
    ) -> Result<Vec<WidgetSummary>, RepositoryError>;

    async fn get_properties(&self, widget_id: WidgetId) -> Result<Vec<Property>, RepositoryError>;

    async fn get_methods(&self, widget_id: WidgetId) -> Result<Vec<Method>, RepositoryError>;

    async fn get_code_samples(
        &self,
        widget_id: WidgetId,
    ) -> Result<Vec<CodeSample>, RepositoryError>;

    /// The coarse FTS5 pre-filter pass (TRD Section 6, step 1). `limit`
    /// bounds the candidate set handed to Ticket 006's in-memory
    /// fine-ranking stage — callers should pass a generous value (Ticket
    /// 006's "coarse_limit" constant), not the final UI result count.
    async fn search_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<WidgetSummary>, RepositoryError>;

    /// The full name/categories/summary corpus, loaded once at startup
    /// to build Ticket 006's in-memory fuzzy index (TRD Section 6, step
    /// 3) — never called per-keystroke.
    async fn load_search_corpus(&self) -> Result<Vec<SearchCorpusEntry>, RepositoryError>;
}
