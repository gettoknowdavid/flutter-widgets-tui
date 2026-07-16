//! Pure domain types mirroring `catalog.db`'s schema (Ticket 004,
//! `migrations/catalog/V1__initial_schema.sql`). Zero I/O, zero
//! `rusqlite`/JSON-parsing here — `fwt-infra`'s adapter is the only place
//! that ever sees a `rusqlite::Row` or a raw JSON string; everything that
//! crosses into this crate is already a fully typed Rust value.

/// New type around a widget's row id. Deliberately not a bare `i64` —
/// prevents accidentally passing a `FavoriteId`/`ChatSessionId` (both
/// `i64`-backed in later epics) where a `WidgetId` was expected. `Copy`
/// because it's an inexpensive 8-byte value, same as any other id new type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(pub i64);

impl WidgetId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }
    pub fn get(&self) -> i64 {
        self.0
    }
}

impl From<i64> for WidgetId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignSystem {
    Cupertino,
    Material,
    Base,
}

impl DesignSystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            DesignSystem::Cupertino => "cupertino",
            DesignSystem::Material => "material",
            DesignSystem::Base => "base",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, WidgetValidationError> {
        match raw {
            "material" => Ok(DesignSystem::Material),
            "cupertino" => Ok(DesignSystem::Cupertino),
            "base" => Ok(DesignSystem::Base),
            other => Err(WidgetValidationError::UnknownDesignSystem(
                other.to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    Enum(Vec<String>),
    Bool,
    Text,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Static,
    Instance,
}

impl MethodKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MethodKind::Static => "static",
            MethodKind::Instance => "instance",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, WidgetValidationError> {
        match raw {
            "static" => Ok(MethodKind::Static),
            "instance" => Ok(MethodKind::Instance),
            other => Err(WidgetValidationError::UnknownMethodKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub type_name: String,
    pub is_required: bool,
    pub is_named: bool,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub id: i64,
    pub widget_id: WidgetId,
    pub name: String,
    pub type_name: String,
    pub default_value: Option<String>,
    pub description: String,
    pub is_required: bool,
    pub is_static: bool,
    pub is_final: bool,
    pub input_kind: InputKind,
    pub sort_order: i64,
}
impl Property {
    pub fn new(
        id: i64,
        widget_id: WidgetId,
        name: impl Into<String>,
        type_name: impl Into<String>,
        default_value: Option<String>,
        description: impl Into<String>,
        is_required: bool,
        is_static: bool,
        is_final: bool,
        input_kind: InputKind,
        sort_order: i64,
    ) -> Result<Self, WidgetValidationError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(WidgetValidationError::EmptyPropertyName);
        }
        Ok(Self {
            id,
            widget_id,
            name,
            type_name: type_name.into(),
            default_value,
            description: description.into(),
            is_required,
            is_static,
            is_final,
            input_kind,
            sort_order,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method {
    pub id: i64,
    pub widget_id: WidgetId,
    pub name: String,
    pub return_type: String,
    pub kind: MethodKind,
    pub description: String,
    pub parameters: Vec<Parameter>,
    pub declared_on: String,
    pub is_inherited: bool,
    pub sort_order: i64,
}
impl Method {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i64,
        widget_id: WidgetId,
        name: impl Into<String>,
        return_type: impl Into<String>,
        kind: MethodKind,
        description: impl Into<String>,
        parameters: Vec<Parameter>,
        declared_on: impl Into<String>,
        is_inherited: bool,
        sort_order: i64,
    ) -> Result<Self, WidgetValidationError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(WidgetValidationError::EmptyMethodName);
        }
        Ok(Self {
            id,
            widget_id,
            name,
            return_type: return_type.into(),
            kind,
            description: description.into(),
            parameters,
            declared_on: declared_on.into(),
            is_inherited,
            sort_order,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSample {
    pub id: i64,
    pub widget_id: WidgetId,
    pub label: String,
    pub kind: String, // 'snippet' | 'dartpad' — free-form per schema, not CHECK-constrained
    pub code: String,
    pub example_path: Option<String>,
    pub sort_order: i64,
}

/// Construction-time invariant violations. Never panics — every
/// constructor here returns `Result`, consistent with Epic 1 Ticket
/// 004's `update()` never-panics discipline.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WidgetValidationError {
    #[error("widget name cannot be empty")]
    EmptyName,

    #[error("widget summary cannot be empty")]
    EmptySummary,

    #[error("unknown design_system value: `{0}` (expected material/cupertino/base)")]
    UnknownDesignSystem(String),

    #[error("unknown method kind: `{0}` (expected static/instance)")]
    UnknownMethodKind(String),

    #[error("property name cannot be empty")]
    EmptyPropertyName,

    #[error("method name cannot be empty")]
    EmptyMethodName,
}

/// Lightweight projection for list rendering (Search results, Catalog
/// category grid) — deliberately excludes `overview_markdown`/
/// `super_chain`/etc. so rendering a 100-row result list doesn't drag a
/// full Markdown body per row through memory and across the
/// `spawn_blocking` boundary for no reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetSummary {
    pub id: WidgetId,
    pub name: String,
    pub summary: String,
    pub design_system: DesignSystem,
    pub categories: Vec<String>,
}

/// One row of the Catalog tab's category grid — category name plus how many
/// widgets carry it (the wireframe's `"Scrolling · 12 widgets"` style).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategorySummary {
    pub name: String,
    pub widget_count: i64,
}

/// The flat name/categories/summary tuple `SearchService` (Ticket 006)
/// loads once at startup to build its in-memory fuzzy index — the "full
/// corpus" TRD Section 6 step 3 describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCorpusEntry {
    pub id: WidgetId,
    pub name: String,
    pub categories: Vec<String>,
    pub summary: String,
}

/// The full widget record — everything `get_widget_by_id`/
/// `get_widget_by_name` return. Deliberately does NOT eagerly embed
/// `properties`/`methods`/`code_samples` as nested fields: those are
/// separate `CatalogRepository` calls (`get_properties`, etc.), matching
/// the ticket's explicit trait shape and avoiding a giant, always-fully-
/// joined query on every single widget lookup (e.g., resolving
/// `related_widget_id` for a breadcrumb hover shouldn't force-load every
/// property row either).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    pub id: WidgetId,
    pub name: String,
    pub top_level: String,
    pub design_system: DesignSystem,
    pub categories: Vec<String>,
    pub summary: String,
    pub overview_markdown: String,
    pub is_deprecated: bool,
    pub super_chain: Vec<String>,
    pub related_widget_id: Option<WidgetId>,
    pub youtube_urls: Vec<String>,
    pub flutter_stable_since: Option<String>,
    pub flutter_channel: String,
}
impl Widget {
    /// Enforces the same non-empty invariants a `NOT NULL`/non-blank
    /// column implies, but as a typed Rust check rather than trusting
    /// the caller (the infra adapter) got every field right. This is
    /// deliberately a plain constructor, not a builder — `Widget` has no
    /// optional-in-practice fields that would benefit from builder
    /// ergonomics; every field is always known by the time the adapter
    /// maps a fully selected SQL row.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: WidgetId,
        name: impl Into<String>,
        top_level: impl Into<String>,
        design_system: DesignSystem,
        categories: Vec<String>,
        summary: impl Into<String>,
        overview_markdown: impl Into<String>,
        is_deprecated: bool,
        super_chain: Vec<String>,
        related_widget_id: Option<WidgetId>,
        youtube_urls: Vec<String>,
        flutter_stable_since: Option<String>,
        flutter_channel: impl Into<String>,
    ) -> Result<Self, WidgetValidationError> {
        let name = name.into();
        let summary = summary.into();

        if name.trim().is_empty() {
            return Err(WidgetValidationError::EmptyName);
        }
        if summary.trim().is_empty() {
            return Err(WidgetValidationError::EmptySummary);
        }

        Ok(Self {
            id,
            name,
            top_level: top_level.into(),
            design_system,
            categories,
            summary,
            overview_markdown: overview_markdown.into(),
            is_deprecated,
            super_chain,
            related_widget_id,
            youtube_urls,
            flutter_stable_since,
            flutter_channel: flutter_channel.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_widget() -> Result<Widget, WidgetValidationError> {
        Widget::new(
            WidgetId(1),
            "ListView",
            "Base Widgets",
            DesignSystem::Base,
            vec!["Scrolling".to_string()],
            "A scrollable, linear list of widgets.",
            "Full overview...",
            false,
            vec!["Widget".to_string()],
            None,
            vec![],
            Some("1.0.0".to_string()),
            "stable",
        )
    }

    #[test]
    fn valid_widget_constructs_successfully() {
        assert!(valid_widget().is_ok());
    }

    #[test]
    fn empty_name_is_rejected() {
        let result = Widget::new(
            WidgetId(1),
            "",
            "Base Widgets",
            DesignSystem::Base,
            vec![],
            "summary",
            "",
            false,
            vec![],
            None,
            vec![],
            None,
            "stable",
        );
        assert_eq!(result, Err(WidgetValidationError::EmptyName));
    }

    #[test]
    fn whitespace_only_name_is_rejected() {
        let result = Widget::new(
            WidgetId(1),
            "   ",
            "Base Widgets",
            DesignSystem::Base,
            vec![],
            "summary",
            "",
            false,
            vec![],
            None,
            vec![],
            None,
            "stable",
        );
        assert_eq!(result, Err(WidgetValidationError::EmptyName));
    }

    #[test]
    fn empty_summary_is_rejected() {
        let result = Widget::new(
            WidgetId(1),
            "ListView",
            "Base Widgets",
            DesignSystem::Base,
            vec![],
            "",
            "",
            false,
            vec![],
            None,
            vec![],
            None,
            "stable",
        );
        assert_eq!(result, Err(WidgetValidationError::EmptySummary));
    }

    #[test]
    fn design_system_parses_valid_values() {
        assert_eq!(DesignSystem::parse("material"), Ok(DesignSystem::Material));
        assert_eq!(
            DesignSystem::parse("cupertino"),
            Ok(DesignSystem::Cupertino)
        );
        assert_eq!(DesignSystem::parse("base"), Ok(DesignSystem::Base));
    }

    #[test]
    fn design_system_rejects_unknown_value() {
        assert_eq!(
            DesignSystem::parse("flutter_web"),
            Err(WidgetValidationError::UnknownDesignSystem(
                "flutter_web".to_string()
            ))
        );
    }

    #[test]
    fn method_kind_round_trips_through_as_str() {
        for kind in [MethodKind::Static, MethodKind::Instance] {
            assert_eq!(MethodKind::parse(kind.as_str()), Ok(kind));
        }
    }

    #[test]
    fn property_rejects_empty_name() {
        let result = Property::new(
            1,
            WidgetId(1),
            "",
            "bool",
            None,
            "",
            false,
            false,
            false,
            InputKind::Bool,
            0,
        );
        assert_eq!(result, Err(WidgetValidationError::EmptyPropertyName));
    }

    #[test]
    fn widget_id_from_i64_round_trips() {
        let id: WidgetId = 42i64.into();
        assert_eq!(id.get(), 42);
    }
}
