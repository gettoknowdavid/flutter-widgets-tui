//! `cargo xtask seed-catalog` — builds a fresh `catalog.db` from the
//! human-authored TOML source in `assets/catalog_seed/`.
//!
//! This is the ONLY code path in the entire project permitted to open
//! `catalog.db` for writing (see ADR-1 / TRD Section 4.1). It is a
//! standalone build-time binary invocation, never part of the shipped
//! `fwt` runtime — do not share connection-construction code between
//! this module and `fwt-infra`'s read-only repository adapter beyond
//! the migration SQL itself.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, Transaction};
use serde::Deserialize;

use crate::migrations::CATALOG_MIGRATIONS;

/// The exact 12 categories from `flutter_widget_catalog_tui.html`'s
/// `catGrid` script — the canonical taxonomy source of truth for MVP
/// seed data. Any seed entry naming a category outside this set fails
/// validation loudly rather than silently introducing a mismatched/
/// duplicated category in the Catalog tab's grid.
const APPROVED_CATEGORIES: &[&str] = &[
    "Accessibility",
    "Animation and motion",
    "Assets, images, and icons",
    "Async",
    "Basics",
    "Input",
    "Interaction models",
    "Layout",
    "Painting and effects",
    "Scrolling",
    "Styling",
    "Text",
];

// =============================================================
// Seed file models (Serde-deserialized directly from TOML)
// =============================================================

#[derive(Debug, Deserialize)]
pub struct SeedWidget {
    pub name: String,
    pub top_level: String,
    pub design_system: String,
    #[serde(default)]
    pub categories: Vec<String>,
    pub summary: String,
    #[serde(default)]
    pub overview_markdown: String,
    #[serde(default)]
    pub is_deprecated: bool,
    #[serde(default)]
    pub super_chain: Vec<String>,
    pub related_widget_name: Option<String>,
    #[serde(default)]
    pub youtube_urls: Vec<String>,
    pub flutter_stable_since: Option<String>,
    #[serde(default = "default_channel")]
    pub flutter_channel: String,
    #[serde(default)]
    pub constructors: Vec<SeedConstructor>,
    #[serde(default)]
    pub properties: Vec<SeedProperty>,
    #[serde(default)]
    pub methods: Vec<SeedMethod>,
    #[serde(default)]
    pub code_samples: Vec<SeedCodeSample>,
}

fn default_channel() -> String {
    "stable".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SeedParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_named: bool,
    #[serde(default)]
    pub default_value: String,
}

#[derive(Debug, Deserialize)]
pub struct SeedConstructor {
    pub name: String,
    #[serde(default)]
    pub documentation: String,
    #[serde(default)]
    pub parameters: Vec<SeedParameter>,
}

#[derive(Debug, Deserialize)]
pub struct SeedProperty {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub default_value: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_static: bool,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default = "default_input_kind")]
    pub input_kind: String,
    pub enum_options: Option<Vec<String>>,
}

fn default_input_kind() -> String {
    "text".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SeedMethod {
    pub name: String,
    pub return_type: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<SeedParameter>,
    pub declared_on: String,
    #[serde(default)]
    pub is_inherited: bool,
}

fn default_kind() -> String {
    "instance".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SeedCodeSample {
    pub label: String,
    #[serde(default = "default_sample_kind")]
    pub kind: String,
    #[serde(default)]
    pub code: String,
    pub example_path: Option<String>,
}

fn default_sample_kind() -> String {
    "snippet".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SeedEnum {
    pub name: String,
    #[serde(default)]
    pub documentation: String,
    #[serde(default)]
    pub values: Vec<SeedEnumValue>,
}

#[derive(Debug, Deserialize)]
pub struct SeedEnumValue {
    pub name: String,
    #[serde(default)]
    pub documentation: String,
}

// =============================================================
// Validation
// =============================================================

/// A precise, file-and-field-identifying validation failure. Every
/// variant carries enough context that a contributor sees exactly
/// which file and which field is wrong without re-reading this module.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SeedValidationError {
    #[error("{file}: `name` must not be empty")]
    EmptyName { file: String },

    #[error("{file}: duplicate widget name `{name}` (already seen in {first_seen_in})")]
    DuplicateWidgetName {
        file: String,
        name: String,
        first_seen_in: String,
    },

    #[error("{file}: `design_system` must be one of 'material'|'cupertino'|'base', got `{value}`")]
    InvalidDesignSystem { file: String, value: String },

    #[error("{file}: category `{category}` is not one of the 12 approved wireframe categories")]
    UnapprovedCategory { file: String, category: String },

    #[error(
        "{file}: property `{property}` has input_kind='enum' but enum_options is missing or empty"
    )]
    MissingEnumOptions { file: String, property: String },

    #[error("{file}: property `{property}` has input_kind != 'enum' but enum_options is set")]
    UnexpectedEnumOptions { file: String, property: String },

    #[error("{file}: related_widget_name `{target}` does not match any seeded widget name")]
    UnresolvedRelatedWidget { file: String, target: String },

    #[error("{file}: enum `{enum_name}` has duplicate value `{value_name}`")]
    DuplicateEnumValue {
        file: String,
        enum_name: String,
        value_name: String,
    },
}

/// Validates a single widget in isolation (fields that don't require
/// knowledge of the rest of the seed set). Cross-widget checks
/// (duplicate names, related_widget_name resolution) happen in
/// [`validate_seed_set`] once every file has been parsed.
fn validate_widget_shape(file: &str, widget: &SeedWidget) -> Result<(), SeedValidationError> {
    if widget.name.trim().is_empty() {
        return Err(SeedValidationError::EmptyName {
            file: file.to_string(),
        });
    }

    if !matches!(
        widget.design_system.as_str(),
        "material" | "cupertino" | "base"
    ) {
        return Err(SeedValidationError::InvalidDesignSystem {
            file: file.to_string(),
            value: widget.design_system.clone(),
        });
    }

    for category in &widget.categories {
        if !APPROVED_CATEGORIES.contains(&category.as_str()) {
            return Err(SeedValidationError::UnapprovedCategory {
                file: file.to_string(),
                category: category.clone(),
            });
        }
    }

    for property in &widget.properties {
        let has_options = property
            .enum_options
            .as_ref()
            .is_some_and(|v| !v.is_empty());
        match (property.input_kind.as_str(), has_options) {
            ("enum", false) => {
                return Err(SeedValidationError::MissingEnumOptions {
                    file: file.to_string(),
                    property: property.name.clone(),
                });
            }
            (kind, true) if kind != "enum" => {
                return Err(SeedValidationError::UnexpectedEnumOptions {
                    file: file.to_string(),
                    property: property.name.clone(),
                });
            }
            _ => {}
        }
    }

    Ok(())
}

fn validate_enum_shape(file: &str, seed_enum: &SeedEnum) -> Result<(), SeedValidationError> {
    let mut seen = std::collections::HashSet::new();
    for value in &seed_enum.values {
        if !seen.insert(value.name.clone()) {
            return Err(SeedValidationError::DuplicateEnumValue {
                file: file.to_string(),
                enum_name: seed_enum.name.clone(),
                value_name: value.name.clone(),
            });
        }
    }
    Ok(())
}

/// Cross-file validation: duplicate widget names and
/// `related_widget_name` resolution against the full seeded set.
/// Called once every widget file has been parsed and per-file-shape
/// validated.
fn validate_seed_set(widgets: &[(String, SeedWidget)]) -> Result<(), SeedValidationError> {
    let mut seen_names: HashMap<&str, &str> = HashMap::new();

    for (file, widget) in widgets {
        if let Some(first_seen_in) = seen_names.get(widget.name.as_str()) {
            return Err(SeedValidationError::DuplicateWidgetName {
                file: file.clone(),
                name: widget.name.clone(),
                first_seen_in: first_seen_in.to_string(),
            });
        }
        seen_names.insert(&widget.name, file);
    }

    let known_names: std::collections::HashSet<&str> =
        widgets.iter().map(|(_, w)| w.name.as_str()).collect();

    for (file, widget) in widgets {
        if let Some(target) = &widget.related_widget_name {
            if !known_names.contains(target.as_str()) {
                return Err(SeedValidationError::UnresolvedRelatedWidget {
                    file: file.clone(),
                    target: target.clone(),
                });
            }
        }
    }

    Ok(())
}

// =============================================================
// File discovery + parsing
// =============================================================

fn load_widgets(seed_dir: &Path) -> Result<Vec<(String, SeedWidget)>> {
    let widgets_dir = seed_dir.join("widgets");
    let mut out = Vec::new();

    let mut paths: Vec<PathBuf> = fs::read_dir(&widgets_dir)
        .with_context(|| format!("reading {}", widgets_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    // Deterministic order — same rationale as AnalyzerSession::discoverBarrelFiles.
    paths.sort();

    for path in paths {
        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let widget: SeedWidget =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        validate_widget_shape(&file_name, &widget)?;
        out.push((file_name, widget));
    }

    Ok(out)
}

fn load_enums(seed_dir: &Path) -> Result<Vec<(String, SeedEnum)>> {
    let enums_dir = seed_dir.join("enums");
    let mut out = Vec::new();

    if !enums_dir.exists() {
        // Enums are additive metadata; an entirely absent enums/
        // directory is a soft warning, not fatal — matches the
        // project's existing "missing optional data" convention
        // (see widget_categories.json's soft-warning precedent).
        eprintln!(
            "⚠️  no {} directory found — proceeding with zero enums",
            enums_dir.display()
        );
        return Ok(out);
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(&enums_dir)
        .with_context(|| format!("reading {}", enums_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();

    for path in paths {
        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let seed_enum: SeedEnum =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        validate_enum_shape(&file_name, &seed_enum)?;
        out.push((file_name, seed_enum));
    }

    Ok(out)
}

// =============================================================
// Two-pass insertion
// =============================================================

/// Serializes `parameters` into the JSON-TEXT shape both
/// `constructors.parameters` and `methods.parameters` expect —
/// a single helper so the two call sites can't silently drift
/// into two different JSON shapes for the same conceptual data.
fn params_json(parameters: &[SeedParameter]) -> String {
    let values: Vec<serde_json::Value> = parameters
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "type": p.type_,
                "is_required": p.is_required,
                "is_named": p.is_named,
                "default_value": p.default_value,
            })
        })
        .collect();
    serde_json::Value::Array(values).to_string()
}

fn insert_enums(tx: &Transaction, enums: &[(String, SeedEnum)]) -> Result<()> {
    for (_file, seed_enum) in enums {
        tx.execute(
            "INSERT INTO enums (name, documentation) VALUES (?1, ?2)",
            params![seed_enum.name, seed_enum.documentation],
        )?;
        let enum_id = tx.last_insert_rowid();

        for (i, value) in seed_enum.values.iter().enumerate() {
            tx.execute(
                "INSERT INTO enum_values (enum_id, name, documentation, sort_order)
                 VALUES (?1, ?2, ?3, ?4)",
                params![enum_id, value.name, value.documentation, i as i64],
            )?;
        }
    }
    Ok(())
}

/// Pass 1: insert every widget with `related_widget_id = NULL`, plus
/// all of its child rows (constructors/properties/methods/code
/// samples). Returns a `widget_name -> database_id` map that Pass 2
/// uses to resolve `related_widget_name` references — this two-pass
/// split is required because a widget's related-widget target may not
/// have been inserted yet (including mutual references, e.g.,
/// ListView <-> GridView, which are valid and NOT a cycle to reject).
fn insert_widgets_pass_one(
    tx: &Transaction,
    widgets: &[(String, SeedWidget)],
) -> Result<HashMap<String, i64>> {
    let mut name_to_id = HashMap::with_capacity(widgets.len());

    for (_file, widget) in widgets {
        let categories_json = serde_json::to_string(&widget.categories)?;
        let super_chain_json = serde_json::to_string(&widget.super_chain)?;
        let youtube_urls_json = serde_json::to_string(&widget.youtube_urls)?;

        tx.execute(
            "INSERT INTO widgets (
                name, top_level, design_system, categories, summary,
                overview_markdown, is_deprecated, super_chain,
                related_widget_id, youtube_urls, flutter_stable_since,
                flutter_channel
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?11)",
            params![
                widget.name,
                widget.top_level,
                widget.design_system,
                categories_json,
                widget.summary,
                widget.overview_markdown,
                widget.is_deprecated as i64,
                super_chain_json,
                youtube_urls_json,
                widget.flutter_stable_since,
                widget.flutter_channel,
            ],
        )?;

        let widget_id = tx.last_insert_rowid();
        name_to_id.insert(widget.name.clone(), widget_id);

        for (i, ctor) in widget.constructors.iter().enumerate() {
            tx.execute(
                "INSERT INTO constructors (widget_id, name, documentation, parameters, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    widget_id,
                    ctor.name,
                    ctor.documentation,
                    params_json(&ctor.parameters),
                    i as i64
                ],
            )?;
        }

        for (i, prop) in widget.properties.iter().enumerate() {
            let enum_options_json = prop
                .enum_options
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            tx.execute(
                "INSERT INTO properties (
                    widget_id, name, type, default_value, description,
                    is_required, is_static, is_final, input_kind,
                    enum_options, sort_order
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    widget_id,
                    prop.name,
                    prop.type_,
                    prop.default_value,
                    prop.description,
                    prop.is_required as i64,
                    prop.is_static as i64,
                    prop.is_final as i64,
                    prop.input_kind,
                    enum_options_json,
                    i as i64
                ],
            )?;
        }

        for (i, method) in widget.methods.iter().enumerate() {
            tx.execute(
                "INSERT INTO methods (
                    widget_id, name, return_type, kind, description,
                    parameters, declared_on, is_inherited, sort_order
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    widget_id,
                    method.name,
                    method.return_type,
                    method.kind,
                    method.description,
                    params_json(&method.parameters),
                    method.declared_on,
                    method.is_inherited as i64,
                    i as i64
                ],
            )?;
        }

        for (i, sample) in widget.code_samples.iter().enumerate() {
            tx.execute(
                "INSERT INTO code_samples (widget_id, label, kind, code, example_path, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    widget_id,
                    sample.label,
                    sample.kind,
                    sample.code,
                    sample.example_path,
                    i as i64
                ],
            )?;
        }
    }

    Ok(name_to_id)
}

/// Pass 2: resolve each widget's `related_widget_name` against the
/// map built in Pass 1, and UPDATE `related_widget_id` accordingly.
/// `validate_seed_set` already guaranteed every reference resolves,
/// so any lookup failure here indicates a bug in this function, not
/// a data-quality issue — hence `.expect()` rather than a soft error.
fn resolve_related_widgets_pass_two(
    tx: &Transaction,
    widgets: &[(String, SeedWidget)],
    name_to_id: &HashMap<String, i64>,
) -> Result<()> {
    for (_file, widget) in widgets {
        if let Some(target_name) = &widget.related_widget_name {
            let widget_id = name_to_id
                .get(widget.name.as_str())
                .expect("widget was inserted in pass one");
            let target_id = name_to_id
                .get(target_name.as_str())
                .expect("validate_seed_set already confirmed related_widget_name resolves");

            tx.execute(
                "UPDATE widgets SET related_widget_id = ?1 WHERE id = ?2",
                params![target_id, widget_id],
            )?;
        }
    }
    Ok(())
}

fn insert_catalog_meta(tx: &Transaction, widget_count: usize, enum_count: usize) -> Result<()> {
    let catalog_version = chrono_date_string();
    let rows = [
        ("schema_version", "1".to_string()),
        ("catalog_version", catalog_version),
        ("widget_count", widget_count.to_string()),
        ("enum_count", enum_count.to_string()),
    ];
    for (key, value) in rows {
        tx.execute(
            "INSERT INTO catalog_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
    }
    Ok(())
}

/// No `chrono`/`time` dependency pulled in just for this one date
/// string — `xtask` is a dev-tool, not the shipped binary, but still
/// no reason to add a dependency for something this trivial.
fn chrono_date_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Coarse YYYY-MM-DD-ish tag is sufficient for catalog_version's
    // purpose (a human-scannable "when was this built" marker) —
    // exact calendar-correctness isn't load-bearing here.
    format!("epoch-{secs}")
}

// =============================================================
// Public entry point
// =============================================================

pub struct SeedCatalogArgs {
    pub seed_dir: PathBuf,
    pub output_path: PathBuf,
    pub force: bool,
}

pub fn run(args: SeedCatalogArgs) -> Result<()> {
    if args.output_path.exists() && !args.force {
        bail!(
            "output file already exists at {} — use --force to overwrite",
            args.output_path.display()
        );
    }

    println!("🚀 Loading seed data from {}", args.seed_dir.display());
    let widgets = load_widgets(&args.seed_dir)?;
    let enums = load_enums(&args.seed_dir)?;

    if widgets.len() < 100 {
        bail!(
            "seed set has only {} widgets; ticket 004 requires >= 100",
            widgets.len()
        );
    }

    println!(
        "✅ Parsed {} widgets and {} enums — validating cross-references...",
        widgets.len(),
        enums.len()
    );
    validate_seed_set(&widgets)?;

    // Build to a temp path first, then atomically rename into place —
    // an accidental double-seed or a mid-run failure must never leave
    // a partially seeded, confusing file at the real output path.
    let tmp_path = args.output_path.with_extension("db.building");
    if tmp_path.exists() {
        fs::remove_file(&tmp_path)?;
    }

    let mut conn =
        Connection::open(&tmp_path).with_context(|| format!("opening {}", tmp_path.display()))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    CATALOG_MIGRATIONS
        .to_latest(&mut conn)
        .context("applying migrations to fresh seed database")?;

    println!("🔨 Inserting into database...");
    let tx = conn.transaction()?;
    insert_enums(&tx, &enums)?;
    let name_to_id = insert_widgets_pass_one(&tx, &widgets)?;
    resolve_related_widgets_pass_two(&tx, &widgets, &name_to_id)?;
    insert_catalog_meta(&tx, widgets.len(), enums.len())?;
    tx.commit()?;

    drop(conn);
    fs::rename(&tmp_path, &args.output_path).with_context(|| {
        format!(
            "renaming {} -> {}",
            tmp_path.display(),
            args.output_path.display()
        )
    })?;

    println!(
        "🎯 Success! Wrote {} widgets and {} enums to {}",
        widgets.len(),
        enums.len(),
        args.output_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_widget(name: &str) -> SeedWidget {
        SeedWidget {
            name: name.to_string(),
            top_level: "Base Widgets".to_string(),
            design_system: "base".to_string(),
            categories: vec!["Scrolling".to_string()],
            summary: "A widget.".to_string(),
            overview_markdown: String::new(),
            is_deprecated: false,
            super_chain: vec![],
            related_widget_name: None,
            youtube_urls: vec![],
            flutter_stable_since: None,
            flutter_channel: "stable".to_string(),
            constructors: vec![],
            properties: vec![],
            methods: vec![],
            code_samples: vec![],
        }
    }

    #[test]
    fn empty_name_is_rejected() {
        let mut w = base_widget("");
        w.name = String::new();
        let err = validate_widget_shape("f.toml", &w).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::EmptyName {
                file: "f.toml".into()
            }
        );
    }

    #[test]
    fn invalid_design_system_is_rejected() {
        let mut w = base_widget("Foo");
        w.design_system = "bogus".to_string();
        let err = validate_widget_shape("f.toml", &w).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::InvalidDesignSystem {
                file: "f.toml".into(),
                value: "bogus".into()
            }
        );
    }

    #[test]
    fn unapproved_category_is_rejected() {
        let mut w = base_widget("Foo");
        w.categories = vec!["Not A Real Category".to_string()];
        let err = validate_widget_shape("f.toml", &w).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::UnapprovedCategory {
                file: "f.toml".into(),
                category: "Not A Real Category".into()
            }
        );
    }

    #[test]
    fn enum_input_kind_without_options_is_rejected() {
        let mut w = base_widget("Foo");
        w.properties = vec![SeedProperty {
            name: "axis".into(),
            type_: "Axis".into(),
            default_value: None,
            description: String::new(),
            is_required: false,
            is_static: false,
            is_final: true,
            input_kind: "enum".into(),
            enum_options: None,
        }];
        let err = validate_widget_shape("f.toml", &w).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::MissingEnumOptions {
                file: "f.toml".into(),
                property: "axis".into()
            }
        );
    }

    #[test]
    fn non_enum_kind_with_options_is_rejected() {
        let mut w = base_widget("Foo");
        w.properties = vec![SeedProperty {
            name: "axis".into(),
            type_: "Axis".into(),
            default_value: None,
            description: String::new(),
            is_required: false,
            is_static: false,
            is_final: true,
            input_kind: "text".into(),
            enum_options: Some(vec!["a".into()]),
        }];
        let err = validate_widget_shape("f.toml", &w).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::UnexpectedEnumOptions {
                file: "f.toml".into(),
                property: "axis".into()
            }
        );
    }

    #[test]
    fn duplicate_widget_names_across_files_are_rejected() {
        let widgets = vec![
            ("a.toml".to_string(), base_widget("ListView")),
            ("b.toml".to_string(), base_widget("ListView")),
        ];
        let err = validate_seed_set(&widgets).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::DuplicateWidgetName {
                file: "b.toml".into(),
                name: "ListView".into(),
                first_seen_in: "a.toml".into(),
            }
        );
    }

    #[test]
    fn unresolved_related_widget_name_is_rejected() {
        let mut w = base_widget("ListView");
        w.related_widget_name = Some("Nonexistent".to_string());
        let widgets = vec![("a.toml".to_string(), w)];
        let err = validate_seed_set(&widgets).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::UnresolvedRelatedWidget {
                file: "a.toml".into(),
                target: "Nonexistent".into(),
            }
        );
    }

    #[test]
    fn mutual_related_widget_references_are_valid() {
        // ListView <-> GridView is a valid mutual reference, NOT a
        // cycle to reject — both names exist in the seeded set.
        let mut lv = base_widget("ListView");
        lv.related_widget_name = Some("GridView".to_string());
        let mut gv = base_widget("GridView");
        gv.related_widget_name = Some("ListView".to_string());

        let widgets = vec![("a.toml".to_string(), lv), ("b.toml".to_string(), gv)];
        assert!(validate_seed_set(&widgets).is_ok());
    }

    #[test]
    fn duplicate_enum_value_is_rejected() {
        let seed_enum = SeedEnum {
            name: "Axis".to_string(),
            documentation: String::new(),
            values: vec![
                SeedEnumValue {
                    name: "horizontal".into(),
                    documentation: String::new(),
                },
                SeedEnumValue {
                    name: "horizontal".into(),
                    documentation: String::new(),
                },
            ],
        };
        let err = validate_enum_shape("axis.toml", &seed_enum).unwrap_err();
        assert_eq!(
            err,
            SeedValidationError::DuplicateEnumValue {
                file: "axis.toml".into(),
                enum_name: "Axis".into(),
                value_name: "horizontal".into(),
            }
        );
    }
}
