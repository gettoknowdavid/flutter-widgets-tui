PRAGMA foreign_keys = ON;

-- ------------------------------------------------------------
-- widgets: the core catalog entity
-- ------------------------------------------------------------
CREATE TABLE widgets
(
    id                   INETEGER PRIMARY KEY,
    name                 TEXT    NOT NULL UNIQUE,
    top_level            TEXT    NOT NULL,
    design_system        TEXT    NOT NULL DEFAULT 'base' CHECK (
        design_system IN (
                          'cupertino',
                          'material',
                          'base'
            )
        ),
    categories           TEXT    NOT NULL DEFAULT '[]',
    summary              TEXT    NOT NULL,
    overview_markdown    TEXT    NOT NULL,
    is_deprecated        BOOLEAN NOT NULL DEFAULT 0,
    super_chain          TEXT    NOT NULL DEFAULT '[]',
    related_widget_id    INTEGER REFERENCES widgets (id) ON DELETE SET NULL,
    youtube_urls         TEXT    NOT NULL DEFAULT '[]',
    flutter_stable_since TEXT,
    flutter_channel      TEXT    NOT NULL DEFAULT 'stable' CHECK (
        flutter_channel IN (
                            'stable',
                            'beta',
                            'dev',
                            'master'
            )
        ),
    created_at           TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_widget_categories ON widgets (categories);

CREATE INDEX idx_widget_design_system ON widgets (design_system);

CREATE INDEX idx_widget_top_level ON widgets (top_level);

-- ------------------------------------------------------------
-- widgets_fts: external-content FTS5 index over name/category/summary
-- ------------------------------------------------------------
CREATE VIRTUAL TABLE widgets_fts USING fts5
(
    name,
    categories,
    summary,
    content = 'widgets',
    content_rowid = 'id'
);

-- --- FTS5 sync triggers -------------------------------------
-- INSERT: mirror the new row straight into the FTS index.
CREATE TRIGGER widgets_ai
    AFTER
        INSERT
    ON widgets
BEGIN
    INSERT INTO widgets_fts (rowid, name, categories, summary)
    VALUES (new.id, new.name, new.categories, new.summary);
END;

-- DELETE: external-content FTS5 tables require a special
-- 'delete' command row (not a plain DELETE) that tells FTS5
-- which OLD content to remove from its internal index — the
-- content itself already lives in `widgets`, so FTS5 needs to
-- be told explicitly, using the old row's own values, what to
-- retract. Omitting this (e.g. `DELETE FROM widgets_fts WHERE
-- rowid = old.id`) is a common mistake that leaves the FTS
-- b-tree in a corrupted, `integrity-check`-failing state.
CREATE TRIGGER widgets_ad
    AFTER DELETE
    ON widgets
BEGIN
    INSERT INTO widgets_fts(widgets_fts, rowid, name, categories, summary)
    VALUES ('delete',
            old.id,
            old.name,
            old.categories,
            old.summary);
END;

-- UPDATE: the same 'delete' command row for the OLD values,
-- immediately followed by a fresh INSERT of the NEW values.
-- This delete-then-insert pairing (not a raw overwrite) is the
-- documented, correct way to keep an external-content FTS5
-- table synchronized on update.
CREATE TRIGGER widgets_au
    AFTER
        UPDATE
    ON widgets
BEGIN
    INSERT INTO widgets_fts(widgets_fts, rowid, name, categories, summary)
    VALUES ('delete',
            old.id,
            old.name,
            old.categories,
            old.summary);
    INSERT INTO widgets_fts(rowid, name, categories, summary)
    VALUES (new.id, new.name, new.categories, new.summary);
END;

-- ------------------------------------------------------------
-- constructors: parameters stored as JSON TEXT (avoids a
-- separate `parameters` table for what is, per-constructor, a
-- small, order-sensitive, display-only list with no independent
-- query needs of its own).
-- ------------------------------------------------------------
CREATE TABLE constructors
(
    id            INTEGER PRIMARY KEY,
    widget_id     INTEGER NOT NULL REFERENCES widgets (id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    documentation TEXT    NOT NULL DEFAULT '',
    parameters    TEXT    NOT NULL DEFAULT '[]',
    sort_order    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_constructors_widget_id ON constructors (widget_id);

-- ------------------------------------------------------------
-- code_samples
-- ------------------------------------------------------------
CREATE TABLE code_samples
(
    id           INTEGER PRIMARY KEY,
    widget_id    INTEGER NOT NULL REFERENCES widgets (id) ON DELETE CASCADE,
    label        TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    code         TEXT    NOT NULL DEFAULT '',
    example_path TEXT,
    sort_order   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_code_samples_widget_id ON code_samples (widget_id);

-- ------------------------------------------------------------
-- properties: drives BOTH the "properties" pane AND the Dynamic
-- Code Parameter Builder (Epic 3).
-- ------------------------------------------------------------
CREATE TABLE properties
(
    id            INTEGER PRIMARY KEY,
    widget_id     INTEGER NOT NULL REFERENCES widgets (id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    type          TEXT    NOT NULL,
    default_value TEXT,
    description   TEXT    NOT NULL DEFAULT '',
    is_required   INTEGER NOT NULL DEFAULT 0,
    is_static     INTEGER NOT NULL DEFAULT 0,
    is_final      INTEGER NOT NULL DEFAULT 0,
    input_kind    TEXT    NOT NULL DEFAULT 'text' CHECK (
        input_kind IN (
                       'enum',
                       'bool',
                       'text',
                       'number'
            )
        ),
    enum_options  TEXT,
    sort_order    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_properties_widget_id ON properties (widget_id);

-- ------------------------------------------------------------
-- methods: declared + inherited (see member_extractor.dart)
-- ------------------------------------------------------------
CREATE TABLE methods
(
    id           INTEGER PRIMARY KEY,
    widget_id    INTEGER NOT NULL REFERENCES widgets (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    return_type  TEXT    NOT NULL,
    kind         TEXT    NOT NULL CHECK (
        kind IN ('static', 'instance')
        ),
    description  TEXT    NOT NULL DEFAULT '',
    parameters   TEXT    NOT NULL DEFAULT '[]',
    declared_on  TEXT    NOT NULL,
    is_inherited INTEGER NOT NULL DEFAULT 0,
    sort_order   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_methods_widget_id ON methods (widget_id);

-- ------------------------------------------------------------
-- enums / enum_values: full per-value documentation, so
-- `properties.enum_options` entries can be cross-referenced for
-- rich display, rather than being bare strings.
-- ------------------------------------------------------------
CREATE TABLE enums
(
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE,
    documentation TEXT NOT NULL DEFAULT ''
);

CREATE TABLE enum_values
(
    id            INTEGER PRIMARY KEY,
    enum_id       INTEGER NOT NULL REFERENCES enums (id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    documentation TEXT    NOT NULL DEFAULT '',
    sort_order    INTEGER NOT NULL DEFAULT 0,
    UNIQUE (enum_id, name)
);

CREATE INDEX idx_enum_values_enum_id ON enum_values (enum_id);

-- ------------------------------------------------------------
-- catalog_meta: versioning / provenance key-value store
-- ------------------------------------------------------------
CREATE TABLE catalog_meta
(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);