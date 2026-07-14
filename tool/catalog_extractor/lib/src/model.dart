/// Top-level container written to `catalog.json`.
///
/// NOTE: this is a JSON *object* (`{ "widgets": [...], "enums": [...] }`),
/// not a flat array like the original `catalog_extractor.dart` produced.
/// The `enums` array is new — it exists so `PropertyRecord.enumOptions`
/// (just value names) can be cross-referenced against a value's own
/// documentation on the Rust ingestion side, without re-deriving enum
/// metadata from scratch there.
class CatalogOutput {
  final String generatedAt; // ISO-8601 UTC
  final String?
  flutterVersionTag; // best-effort, from bin/internal/engine.version
  final List<WidgetRecord> widgets;
  final List<EnumRecord> enums;

  CatalogOutput({
    required this.generatedAt,
    required this.flutterVersionTag,
    required this.widgets,
    required this.enums,
  });

  Map<String, dynamic> toJson() => {
    'generatedAt': generatedAt,
    'flutterVersionTag': flutterVersionTag,
    'widgets': widgets.map((w) => w.toJson()).toList(),
    'enums': enums.map((e) => e.toJson()).toList(),
  };
}

class WidgetRecord {
  final String name;

  /// 'Design Systems' | 'Base Widgets' — structural, path-derived, always
  /// present. See `widget_filter.dart`'s `DesignSystemTier.topLevelLabel`.
  final String topLevel;

  /// 'material' | 'cupertino' | 'base' — the same tier as [topLevel], just
  /// the finer-grained enum name rather than the display label. Kept as a
  /// separate field (rather than making callers parse [topLevel]) because
  /// the Rust side's `design_system` column (TRD Section 4.2) wants exactly
  /// these three lowercase values, not the two display-label strings.
  final String designSystem;

  /// The docs.flutter.dev widget-catalog categories this widget appears
  /// under (e.g. `["Layout", "Basics"]`). Replaces the old singular
  /// `subCategory: String` — see `widget_filter.dart`'s doc comment for why
  /// a widget can genuinely belong to more than one, and why this is now
  /// sourced from an external curated JSON file rather than a hardcoded
  /// map. Empty (never null) when uncurated.
  final List<String> categories;

  final String summary;
  final String overviewMarkdown;
  final bool isDeprecated;
  final List<String> superChain;
  final String? relatedWidgetName;
  final List<ConstructorRecord> constructors;
  final List<PropertyRecord> properties;
  final List<MethodRecord> methods;
  final List<CodeSampleRecord> codeSamples;
  final List<String> youtubeUrls;

  WidgetRecord({
    required this.name,
    required this.topLevel,
    required this.designSystem,
    required this.categories,
    required this.summary,
    required this.overviewMarkdown,
    required this.isDeprecated,
    required this.superChain,
    required this.relatedWidgetName,
    required this.constructors,
    required this.properties,
    required this.methods,
    required this.codeSamples,
    required this.youtubeUrls,
  });

  Map<String, dynamic> toJson() => {
    'name': name,
    'top_level': topLevel,
    'design_system': designSystem,
    'categories': categories,
    'summary': summary,
    'overview_markdown': overviewMarkdown,
    'is_deprecated': isDeprecated,
    'super_chain': superChain,
    'related_widget_name': relatedWidgetName,
    'constructors': constructors.map((c) => c.toJson()).toList(),
    'properties': properties.map((p) => p.toJson()).toList(),
    'methods': methods.map((m) => m.toJson()).toList(),
    'code_samples': codeSamples.map((c) => c.toJson()).toList(),
    'youtube_urls': youtubeUrls,
  };
}

class ConstructorRecord {
  final String name; // 'ListView' or 'ListView.builder'
  final String documentation;
  final List<ParameterRecord> parameters;

  ConstructorRecord({
    required this.name,
    required this.documentation,
    required this.parameters,
  });

  Map<String, dynamic> toJson() => {
    'name': name,
    'documentation': documentation,
    'parameters': parameters.map((p) => p.toJson()).toList(),
  };
}

class ParameterRecord {
  final String name;
  final String type;
  final bool isRequired;
  final bool isNamed;
  final String defaultValue; // '' if none

  ParameterRecord({
    required this.name,
    required this.type,
    required this.isRequired,
    required this.isNamed,
    required this.defaultValue,
  });

  Map<String, dynamic> toJson() => {
    'name': name,
    'type': type,
    'is_required': isRequired,
    'is_named': isNamed,
    'default_value': defaultValue,
  };
}

/// Maps directly onto the `properties` table in the Rust TRD's schema
/// (Section 4.2) — `input_kind`/`enum_options` are exactly what the
/// Dynamic Code Parameter Builder feature needs.
class PropertyRecord {
  final String name;
  final String type;
  final String? defaultValue;
  final String description;
  final bool isRequired;
  final bool isStatic;
  final bool isFinal;
  final String inputKind; // 'enum' | 'bool' | 'text' | 'number'
  final List<String>? enumOptions;

  PropertyRecord({
    required this.name,
    required this.type,
    required this.defaultValue,
    required this.description,
    required this.isRequired,
    required this.isStatic,
    required this.isFinal,
    required this.inputKind,
    required this.enumOptions,
  });

  Map<String, dynamic> toJson() => {
    'name': name,
    'type': type,
    'default_value': defaultValue,
    'description': description,
    'is_required': isRequired,
    'is_static': isStatic,
    'is_final': isFinal,
    'input_kind': inputKind,
    'enum_options': enumOptions,
  };
}

class MethodRecord {
  final String name;
  final String returnType;
  final bool isStatic;
  final String kind; // 'static' | 'instance'
  final String description;
  final List<ParameterRecord> parameters;
  final String declaredOn; // owning class name
  final bool isInherited; // false if declared directly on this widget

  MethodRecord({
    required this.name,
    required this.returnType,
    required this.isStatic,
    required this.kind,
    required this.description,
    required this.parameters,
    required this.declaredOn,
    required this.isInherited,
  });

  Map<String, dynamic> toJson() => {
    'name': name,
    'return_type': returnType,
    'is_static': isStatic,
    'kind': kind,
    'description': description,
    'parameters': parameters.map((p) => p.toJson()).toList(),
    'declared_on': declaredOn,
    'is_inherited': isInherited,
  };
}

/// `kind == 'snippet'` -> code came from an inline ```dart fence inside a
/// `{@tool snippet}` block.
/// `kind == 'dartpad'` -> code (if resolved) came from reading the
/// `examples/api/...` file referenced by a `{@tool dartpad}` block's
/// `** See code in ... **` marker. `examplePath` is always set for these
/// even if `code` ended up empty (file not found / --flutter-src too
/// shallow) — the path itself is still useful for a future "open example"
/// affordance.
class CodeSampleRecord {
  final String label;
  final String kind;
  final String code;
  final String? examplePath;

  CodeSampleRecord({
    required this.label,
    required this.kind,
    required this.code,
    required this.examplePath,
  });

  bool get hasCode => code.trim().isNotEmpty;

  Map<String, dynamic> toJson() => {
    'label': label,
    'kind': kind,
    'code': code,
    'example_path': examplePath,
  };
}

class EnumRecord {
  final String name;
  final String documentation;
  final List<EnumValueRecord> values;

  EnumRecord({
    required this.name,
    required this.documentation,
    required this.values,
  });

  Map<String, dynamic> toJson() => {
    'name': name,
    'documentation': documentation,
    'values': values.map((v) => v.toJson()).toList(),
  };
}

class EnumValueRecord {
  final String name;
  final String documentation;

  EnumValueRecord({required this.name, required this.documentation});

  Map<String, dynamic> toJson() => {
    'name': name,
    'documentation': documentation,
  };
}
