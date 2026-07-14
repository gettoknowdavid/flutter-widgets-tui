# catalog_extractor

Extracts Flutter widget metadata from a local `flutter/flutter` checkout into
`catalog.json`, ready to be seeded into `catalog.db` (see the Rust project's
`TRD.md` Section 4.2 for the target schema).

## Location in the parent repo

This is a **separate pub package**, living as a sibling to the Rust
workspace — not inside `crates/`, since it has nothing to do with the Rust
dependency graph:

```
flutter-widgets-tui/
├── Cargo.toml                  # Rust workspace (unrelated to this tool)
├── crates/
├── xtask/
└── tool/
    └── catalog_extractor/      # <-- everything below lives here
        ├── pubspec.yaml
        ├── README.md
        ├── bin/
        │   └── extract_catalog.dart
        └── lib/
            ├── catalog_extractor.dart
            └── src/
                ├── model.dart
                ├── widget_filter.dart
                ├── analyzer_session.dart
                ├── dartdoc_session.dart
                ├── doc_comment_parser.dart
                ├── member_extractor.dart
                ├── extractor.dart
                └── json_writer.dart
```

## Setup

1. Clone Flutter at a known tag and precache its bundled Dart SDK:
   ```bash
   git clone --depth 1 --branch stable https://github.com/flutter/flutter.git /tmp/flutter-src
   cd /tmp/flutter-src && bin/flutter precache
   ```
2. From `tool/catalog_extractor/`:
   ```bash
   dart pub get
   ```

## Running

```bash
dart run bin/extract_catalog.dart --flutter-src /tmp/flutter-src --output catalog.json
```

Expect this to take several minutes and a few GB of RAM — it resolves the
*entire* `flutter` package's analyzer + dartdoc graphs, not just widgets.
Progress and warnings print to stdout/stderr as it runs; a summary (widgets
extracted, file/class failures, doc-resolution warnings) prints at the end.

## Version pinning — read before upgrading dartdoc

`package:analyzer`'s element model (`ClassElement`, `ConstructorElement`,
etc.) is a stable, documented public API — safe to upgrade normally.

`package:dartdoc`'s model classes (`PackageGraph`, `Class`, `Library`) are
**not** a published API; they are dartdoc's internal implementation and can
change shape between releases with no deprecation cycle. This project is
pinned to `dartdoc: 8.0.14` in `pubspec.yaml` for exactly this reason. If you
bump that version, re-verify every dartdoc call in
`lib/src/dartdoc_session.dart` and `lib/src/doc_comment_parser.dart` against
the new version's source (`lib/src/model/model.dart`,
`lib/src/package_builder.dart`, `lib/src/dartdoc_options.dart`) before
trusting this tool's output again. `dartdoc` is deliberately used *only* for
what it does uniquely well — resolving `[CrossReferences]` and
`{@category}` tags against the real symbol table — everything else
(constructors, parameters, inherited members) goes through `analyzer`.

## File map

| File                              | Responsibility                                                               |
| --------------------------------- | ---------------------------------------------------------------------------- |
| `bin/extract_catalog.dart`        | CLI entry point; drives the whole run, prints progress/summary               |
| `lib/catalog_extractor.dart`      | Barrel export — import this one file from `bin/`                             |
| `lib/src/model.dart`              | JSON-serializable output types (`WidgetRecord`, `ConstructorRecord`, etc.)   |
| `lib/src/widget_filter.dart`      | Is-this-a-Widget detection, walks `allSupertypes`                            |
| `lib/src/analyzer_session.dart`   | `AnalysisContextCollection` setup + file discovery + resolution              |
| `lib/src/dartdoc_session.dart`    | `PackageGraph` setup for doc resolution (⚠️ unstable API — see warning above) |
| `lib/src/doc_comment_parser.dart` | `{@category}`, `[refs]`, `{@tool dartpad/snippet}` extraction                |
| `lib/src/member_extractor.dart`   | Constructors, parameters, properties, methods (via `InheritanceManager3`)    |
| `lib/src/extractor.dart`          | Combines all of the above into one `WidgetRecord` per widget                 |
| `lib/src/json_writer.dart`        | Writes the final `catalog.json`                                              |

## Output

A flat JSON array of widget objects (not `{ "widgets": [...] }`) — maps 1:1
onto `serde_json::from_reader::<Vec<WidgetJson>>` on the Rust side. See
`lib/src/model.dart`'s `toJson()` methods for the exact schema. Feed this
into a `cargo xtask seed-catalog` step to populate `catalog.db`, resolving
`related_widget_name` → `related_widget_id` and assigning row IDs there —
this tool deliberately never assigns IDs itself.