use crate::db::migrations;
use fwt_domain::ports::RepositoryError;
use fwt_domain::widget::{
    CategorySummary, CodeSample, DesignSystem, InputKind, Method, MethodKind, Parameter, Property,
    SearchCorpusEntry, Widget, WidgetId, WidgetSummary,
};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OpenFlags, Row};
use std::path::Path;
use std::sync::Arc;

const POOL_SIZE: u32 = 4;

pub struct SqliteCatalogRepository {
    pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
}

impl SqliteCatalogRepository {
    /// Constructs a repository against `db_path`, migrating first and
    /// only then downgrading to a read-only connection pool.
    pub fn new(db_path: &Path) -> Result<Self, RepositoryError> {
        // Block 1: migrate, using a momentary R/W connection
        {
            let conn = migrations::open_and_migrate(db_path)
                .map_err(|e| RepositoryError::ConnectionFailed(Box::new(e)))?;
            drop(conn);
        }

        // Block 2: construct the read-only pool
        let read_only_flags = OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI;

        let manager = SqliteConnectionManager::file(db_path).with_flags(read_only_flags);

        let pool = r2d2::Pool::builder()
            .max_size(POOL_SIZE)
            .build(manager)
            .map_err(|e| RepositoryError::ConnectionFailed(Box::new(e)))?;

        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    /// Internal helper: checks out a pooled connection inside the
    /// caller's `spawn_blocking` closure. Not `pub` — every trait method
    /// below is the only place this is called, keeping the
    /// `spawn_blocking` boundary consistent everywhere.
    fn checkout(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, RepositoryError> {
        self.pool.get().map_err(|_| RepositoryError::PoolExhausted)
    }
}

// -----------------------------------------------------------------------
// Row -> domain mapping helpers (the ONLY place JSON columns get parsed)
// -----------------------------------------------------------------------

fn parse_json_string_array(
    column: &'static str,
    raw: &str,
) -> Result<Vec<String>, RepositoryError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(raw).map_err(|e| RepositoryError::Serialization {
        column: column.to_string(),
        source: Box::new(e),
    })
}

fn parse_parameters(raw: &str) -> Result<Vec<Parameter>, RepositoryError> {
    #[derive(serde::Deserialize)]
    struct RawParam {
        name: String,
        #[serde(rename = "type")]
        type_name: String,
        #[serde(rename = "is_required")]
        is_required: bool,
        #[serde(rename = "is_named")]
        is_named: bool,
        #[serde(rename = "default_value", default)]
        default_value: String,
    }

    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let parsed: Vec<RawParam> = serde_json::from_str(raw).map_err(|e| RepositoryError::Serialization {
        column: "parameters".to_string(),
        source: Box::new(e),
    })?;

    Ok(parsed
        .into_iter()
        .map(|p| Parameter {
            name: p.name,
            type_name: p.type_name,
            is_required: p.is_required,
            is_named: p.is_named,
            default_value: if p.default_value.is_empty() {
                None
            } else {
                Some(p.default_value)
            },
        })
        .collect())
}

fn row_to_widget(row: &Row) -> rusqlite::Result<(Widget, ())> {
    // Extracted as a fallible closure so widget_from_row (below) can
    // surface WidgetValidationError distinctly from rusqlite::Error —
    // rusqlite's row-mapping closures must return rusqlite::Result, so
    // JSON/validation errors are deferred to the caller via a raw tuple
    // of pre-parsed columns, mapped into domain types just outside the
    // rusqlite query_row/query_map closure.
    unreachable!("see widget_from_row below — this stub exists only for doc clarity")
}

/// Maps one fully-selected `widgets` row into a `Widget`. Called from
/// both `get_widget_by_id` and `get_widget_by_name` — kept as a single
/// function so the two lookups can never drift in which columns they
/// select or how they're mapped.
fn widget_from_row(row: &Row) -> Result<Widget, RepositoryError> {
    let id: i64 = row.get("id").map_err(query_failed)?;
    let name: String = row.get("name").map_err(query_failed)?;
    let top_level: String = row.get("top_level").map_err(query_failed)?;
    let design_system_raw: String = row.get("design_system").map_err(query_failed)?;
    let categories_raw: String = row.get("categories").map_err(query_failed)?;
    let summary: String = row.get("summary").map_err(query_failed)?;
    let overview_markdown: String = row.get("overview_markdown").map_err(query_failed)?;
    let is_deprecated: bool = row.get("is_deprecated").map_err(query_failed)?;
    let super_chain_raw: String = row.get("super_chain").map_err(query_failed)?;
    let related_widget_id: Option<i64> = row.get("related_widget_id").map_err(query_failed)?;
    let youtube_urls_raw: String = row.get("youtube_urls").map_err(query_failed)?;
    let flutter_stable_since: Option<String> =
        row.get("flutter_stable_since").map_err(query_failed)?;
    let flutter_channel: String = row.get("flutter_channel").map_err(query_failed)?;

    let design_system =
        DesignSystem::parse(&design_system_raw).map_err(|e| RepositoryError::Serialization {
            column: "design_system".to_string(),
            source: Box::new(e),
        })?;
    let categories = parse_json_string_array("categories", &categories_raw)?;
    let super_chain = parse_json_string_array("super_chain", &super_chain_raw)?;
    let youtube_urls = parse_json_string_array("youtube_urls", &youtube_urls_raw)?;

    Widget::new(
        WidgetId(id),
        name,
        top_level,
        design_system,
        categories,
        summary,
        overview_markdown,
        is_deprecated,
        super_chain,
        related_widget_id.map(WidgetId),
        youtube_urls,
        flutter_stable_since,
        flutter_channel,
    )
    .map_err(|e| RepositoryError::Serialization {
        column: "widgets(row)".to_string(),
        source: Box::new(e),
    })
}

fn widget_summary_from_row(row: &Row) -> Result<WidgetSummary, RepositoryError> {
    let id: i64 = row.get("id").map_err(query_failed)?;
    let name: String = row.get("name").map_err(query_failed)?;
    let summary: String = row.get("summary").map_err(query_failed)?;
    let design_system_raw: String = row.get("design_system").map_err(query_failed)?;
    let categories_raw: String = row.get("categories").map_err(query_failed)?;

    let design_system =
        DesignSystem::parse(&design_system_raw).map_err(|e| RepositoryError::Serialization {
            column: "design_system".to_string(),
            source: Box::new(e),
        })?;
    let categories = parse_json_string_array("categories", &categories_raw)?;

    Ok(WidgetSummary {
        id: WidgetId(id),
        name,
        summary,
        design_system,
        categories,
    })
}

fn query_failed(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::QueryFailed(Box::new(e))
}

// -----------------------------------------------------------------------
// Trait implementation
// -----------------------------------------------------------------------

#[async_trait::async_trait]
impl fwt_domain::ports::catalog_repository::CatalogRepository for SqliteCatalogRepository {
    async fn get_widget_by_id(&self, id: WidgetId) -> Result<Option<Widget>, RepositoryError> {
        let pool = Arc::clone(&self.pool);

        // Every trait method wraps its synchronous rusqlite work in
        // spawn_blocking — rusqlite is fully synchronous, and calling it
        // directly on an async fn would block whatever Tokio worker
        // thread happens to be running this task, starving the event
        // loop. (Epic 1 Ticket 003's non-blocking guarantee applies here
        // just as much as it did to Command dispatch.)
        tokio::task::spawn_blocking(move || -> Result<Option<Widget>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;

            // Parameterized via `?1` — id is a typed i64, never
            // string-interpolated, so this is injection-safe by
            // construction even before criterion 6's dedicated test.
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, top_level, design_system, categories, summary,
                            overview_markdown, is_deprecated, super_chain,
                            related_widget_id, youtube_urls, flutter_stable_since,
                            flutter_channel
                     FROM widgets WHERE id = ?1",
                )
                .map_err(query_failed)?;

            let mut rows = stmt.query(params![id.get()]).map_err(query_failed)?;

            match rows.next().map_err(query_failed)? {
                Some(row) => Ok(Some(widget_from_row(row)?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn get_widget_by_name(&self, name: &str) -> Result<Option<Widget>, RepositoryError> {
        let pool = Arc::clone(&self.pool);
        let name = name.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<Widget>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, top_level, design_system, categories, summary,
                            overview_markdown, is_deprecated, super_chain,
                            related_widget_id, youtube_urls, flutter_stable_since,
                            flutter_channel
                     FROM widgets WHERE name = ?1",
                )
                .map_err(query_failed)?;
            let mut rows = stmt.query(params![name]).map_err(query_failed)?;
            match rows.next().map_err(query_failed)? {
                Some(row) => Ok(Some(widget_from_row(row)?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn list_categories(&self) -> Result<Vec<CategorySummary>, RepositoryError> {
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || -> Result<Vec<CategorySummary>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;

            // `categories` is a JSON array TEXT column; `json_each` (part
            // of SQLite's JSON1 extension, bundled by default with
            // rusqlite's `bundled` feature) expands each widget's array
            // into one row per category so we can GROUP BY category name
            // for a count — exactly the "· N widgets" the wireframe's
            // catGrid needs.
            let mut stmt = conn
                .prepare(
                    "SELECT je.value AS category, COUNT(*) AS widget_count
                     FROM widgets, json_each(widgets.categories) AS je
                     GROUP BY je.value
                     ORDER BY je.value ASC",
                )
                .map_err(query_failed)?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(CategorySummary {
                        name: row.get("category")?,
                        widget_count: row.get("widget_count")?,
                    })
                })
                .map_err(query_failed)?;

            rows.collect::<Result<Vec<_>, _>>().map_err(query_failed)
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn list_widgets_by_category(
        &self,
        category: &str,
    ) -> Result<Vec<WidgetSummary>, RepositoryError> {
        let pool = Arc::clone(&self.pool);
        let category = category.to_string();

        tokio::task::spawn_blocking(move || -> Result<Vec<WidgetSummary>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;

            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT w.id, w.name, w.summary, w.design_system, w.categories
                     FROM widgets w, json_each(w.categories) AS je
                     WHERE je.value = ?1
                     ORDER BY w.name ASC",
                )
                .map_err(query_failed)?;

            let rows = stmt
                .query_map(params![category], |row| {
                    widget_summary_from_row(row)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                })
                .map_err(query_failed)?;

            let mut stmt2 = conn
                .prepare(
                    "SELECT DISTINCT w.id, w.name, w.summary, w.design_system, w.categories
                     FROM widgets w, json_each(w.categories) AS je
                     WHERE je.value = ?1
                     ORDER BY w.name ASC",
                )
                .map_err(query_failed)?;
            let mut result_rows = stmt2.query(params![category]).map_err(query_failed)?;

            let mut results = Vec::new();
            while let Some(row) = result_rows.next().map_err(query_failed)? {
                results.push(widget_summary_from_row(row)?);
            }
            Ok(results)
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn get_properties(&self, widget_id: WidgetId) -> Result<Vec<Property>, RepositoryError> {
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || -> Result<Vec<Property>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, widget_id, name, type, default_value, description,
                            is_required, is_static, is_final, input_kind, enum_options,
                            sort_order
                     FROM properties WHERE widget_id = ?1 ORDER BY sort_order ASC",
                )
                .map_err(query_failed)?;

            let mut rows = stmt.query(params![widget_id.get()]).map_err(query_failed)?;
            let mut results = Vec::new();

            while let Some(row) = rows.next().map_err(query_failed)? {
                let id: i64 = row.get("id").map_err(query_failed)?;
                let name: String = row.get("name").map_err(query_failed)?;
                let type_name: String = row.get("type").map_err(query_failed)?;
                let default_value: Option<String> =
                    row.get("default_value").map_err(query_failed)?;
                let description: String = row.get("description").map_err(query_failed)?;
                let is_required: bool = row.get("is_required").map_err(query_failed)?;
                let is_static: bool = row.get("is_static").map_err(query_failed)?;
                let is_final: bool = row.get("is_final").map_err(query_failed)?;
                let input_kind_raw: String = row.get("input_kind").map_err(query_failed)?;
                let enum_options_raw: Option<String> =
                    row.get("enum_options").map_err(query_failed)?;

                let input_kind = match input_kind_raw.as_str() {
                    "bool" => InputKind::Bool,
                    "text" => InputKind::Text,
                    "number" => InputKind::Number,
                    "enum" => {
                        let raw = enum_options_raw.unwrap_or_default();
                        InputKind::Enum(parse_json_string_array("enum_options", &raw)?)
                    }
                    other => {
                        return Err(RepositoryError::Serialization {
                            column: "input_kind".to_string(),
                            source: Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("unknown input_kind `{other}`"),
                            )),
                        })
                    }
                };

                let sort_order: i64 = row.get("sort_order").map_err(query_failed)?;

                results.push(
                    Property::new(
                        id,
                        widget_id,
                        name,
                        type_name,
                        default_value,
                        description,
                        is_required,
                        is_static,
                        is_final,
                        input_kind,
                        sort_order,
                    )
                    .map_err(|e| RepositoryError::Serialization {
                        column: "properties(row)".to_string(),
                        source: Box::new(e),
                    })?,
                );
            }

            Ok(results)
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn get_methods(&self, widget_id: WidgetId) -> Result<Vec<Method>, RepositoryError> {
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || -> Result<Vec<Method>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, widget_id, name, return_type, kind, description,
                            parameters, declared_on, is_inherited, sort_order
                     FROM methods WHERE widget_id = ?1 ORDER BY sort_order ASC",
                )
                .map_err(query_failed)?;

            let mut rows = stmt.query(params![widget_id.get()]).map_err(query_failed)?;
            let mut results = Vec::new();

            while let Some(row) = rows.next().map_err(query_failed)? {
                let id: i64 = row.get("id").map_err(query_failed)?;
                let name: String = row.get("name").map_err(query_failed)?;
                let return_type: String = row.get("return_type").map_err(query_failed)?;
                let kind_raw: String = row.get("kind").map_err(query_failed)?;
                let description: String = row.get("description").map_err(query_failed)?;
                let parameters_raw: String = row.get("parameters").map_err(query_failed)?;
                let declared_on: String = row.get("declared_on").map_err(query_failed)?;
                let is_inherited: bool = row.get("is_inherited").map_err(query_failed)?;
                let sort_order: i64 = row.get("sort_order").map_err(query_failed)?;

                let kind =
                    MethodKind::parse(&kind_raw).map_err(|e| RepositoryError::Serialization {
                        column: "kind".to_string(),
                        source: Box::new(e),
                    })?;
                let parameters = parse_parameters(&parameters_raw)?;

                results.push(
                    Method::new(
                        id,
                        widget_id,
                        name,
                        return_type,
                        kind,
                        description,
                        parameters,
                        declared_on,
                        is_inherited,
                        sort_order,
                    )
                    .map_err(|e| RepositoryError::Serialization {
                        column: "methods(row)".to_string(),
                        source: Box::new(e),
                    })?,
                );
            }

            Ok(results)
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn get_code_samples(
        &self,
        widget_id: WidgetId,
    ) -> Result<Vec<CodeSample>, RepositoryError> {
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || -> Result<Vec<CodeSample>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, widget_id, label, kind, code, example_path, sort_order
                     FROM code_samples WHERE widget_id = ?1 ORDER BY sort_order ASC",
                )
                .map_err(query_failed)?;

            let rows = stmt
                .query_map(params![widget_id.get()], |row| {
                    Ok(CodeSample {
                        id: row.get("id")?,
                        widget_id,
                        label: row.get("label")?,
                        kind: row.get("kind")?,
                        code: row.get("code")?,
                        example_path: row.get("example_path")?,
                        sort_order: row.get("sort_order")?,
                    })
                })
                .map_err(query_failed)?;

            rows.collect::<Result<Vec<_>, _>>().map_err(query_failed)
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn search_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<WidgetSummary>, RepositoryError> {
        let pool = Arc::clone(&self.pool);
        let query = query.to_string();

        tokio::task::spawn_blocking(move || -> Result<Vec<WidgetSummary>, RepositoryError> {
            // Empty/whitespace-only queries are not forwarded to FTS5 —
            // an empty MATCH string is either an SQLite error or
            // undefined behavior depending on FTS5 configuration, so
            // short-circuit here rather than trusting the caller always
            // pre-validates (Ticket 006 also short-circuits at the
            // SearchService layer; this is defense in depth, not
            // redundant, since search_fts is itself a public trait
            // method other future callers could invoke directly).
            if query.trim().is_empty() {
                return Ok(Vec::new());
            }

            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;

            // CRITICAL: the query string is ALWAYS bound via `?1`, never
            // interpolated into the SQL text. FTS5's MATCH operand syntax
            // has its own special characters (", *, -, NEAR, AND/OR/NOT). However,
            //  binding it as a parameter means even adversarial input
            // is interpreted purely as FTS5 query syntax against the
            // search index — it can never break out into the surrounding
            // SQL statement the way string concatenation could. A
            // malformed FTS5 query syntax error from adversarial input is
            // caught and turned into an empty result, not propagated as
            // a crash (see the query_failed fallback below).
            let mut stmt = conn
                .prepare(
                    "SELECT w.id, w.name, w.summary, w.design_system, w.categories
                     FROM widgets_fts
                     JOIN widgets w ON w.id = widgets_fts.rowid
                     WHERE widgets_fts MATCH ?1
                     ORDER BY rank
                     LIMIT ?2",
                )
                .map_err(query_failed)?;

            let query_result = stmt.query(params![query, limit as i64]);

            // A syntactically invalid FTS5 MATCH expression (e.g., an
            // unbalanced quote from adversarial/accidental input) is a
            // real possibility with raw user text bound directly as the
            // MATCH operand. Treat that specific failure mode as "no
            // results" rather than propagating a hard error — a search
            // box should never crash on odd punctuation.
            let mut rows = match query_result {
                Ok(rows) => rows,
                Err(_) => return Ok(Vec::new()),
            };

            let mut results = Vec::new();
            while let Some(row) = rows.next().map_err(query_failed)? {
                results.push(widget_summary_from_row(row)?);
            }
            Ok(results)
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }

    async fn load_search_corpus(&self) -> Result<Vec<SearchCorpusEntry>, RepositoryError> {
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || -> Result<Vec<SearchCorpusEntry>, RepositoryError> {
            let conn = pool.get().map_err(|_| RepositoryError::PoolExhausted)?;
            let mut stmt = conn
                .prepare("SELECT id, name, categories, summary FROM widgets ORDER BY id ASC")
                .map_err(query_failed)?;

            let mut rows = stmt.query([]).map_err(query_failed)?;
            let mut results = Vec::new();

            while let Some(row) = rows.next().map_err(query_failed)? {
                let id: i64 = row.get("id").map_err(query_failed)?;
                let name: String = row.get("name").map_err(query_failed)?;
                let categories_raw: String = row.get("categories").map_err(query_failed)?;
                let summary: String = row.get("summary").map_err(query_failed)?;
                let categories = parse_json_string_array("categories", &categories_raw)?;

                results.push(SearchCorpusEntry {
                    id: WidgetId(id),
                    name,
                    categories,
                    summary,
                });
            }
            Ok(results)
        })
        .await
        .map_err(|_| RepositoryError::TaskJoinFailed)?
    }
}
