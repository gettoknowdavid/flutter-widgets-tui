import 'dart:io';

import 'package:analyzer/dart/element/element.dart';
import 'package:path/path.dart' as p;

import 'analyzer_session.dart';
import 'doc_comment_parser.dart';
import 'member_extractor.dart';
import 'model.dart';
import 'widget_filter.dart';

/// Orchestrates the full extraction: resolves the SDK's public export
/// surface via [AnalyzerSession], classifies + categorizes via
/// `widget_filter.dart`, pulls constructors/properties/methods via
/// `member_extractor.dart`, parses doc comments via
/// `doc_comment_parser.dart`, and assembles everything into a
/// [CatalogOutput].
///
/// This is the direct replacement for the original monolithic
/// `bin/catalog_extractor.dart` (the one that regex-scraped
/// docs.flutter.dev *and* regex-scanned SDK source in the same script).
/// Every one of that script's responsibilities has moved to its own file;
/// this class's only job is sequencing them correctly — in particular,
/// running extraction in two passes, because [parseDoc]'s "See also:"
/// related-widget guess needs the *complete* set of widget names before it
/// can decide whether `[GridView]` in some other widget's doc comment is
/// worth trusting as a real cross-reference.
class CatalogExtractor {
  final AnalyzerSession session;

  /// Root of the Flutter SDK checkout (contains `packages/` and `bin/`).
  /// Used for two purposes, both best-effort: resolving `{@tool dartpad}`
  /// `examples/api/...` code samples (see `doc_comment_parser.dart`), and
  /// reading `bin/internal/engine.version` for [CatalogOutput.flutterVersionTag].
  final String? flutterSrcRoot;

  final void Function(String message)? onProgress;

  CatalogExtractor({
    required this.session,
    this.flutterSrcRoot,
    this.onProgress,
  });

  Future<CatalogOutput> run() async {
    onProgress?.call('🔍 Resolving public export namespace...');
    final namespace = await session.resolvePublicExportNamespace(
      onProgress: onProgress,
    );

    // --- Pass 1: classify + collect names -----------------------------
    // Cheap, no doc parsing or member extraction yet — just enough to
    // build the `knownWidgetNames` set that pass 2's parseDoc() calls
    // need for related-widget guessing.
    final widgetElements = <ClassElement>[];
    final enumElements = <EnumElement>[];

    for (final element in namespace.values) {
      if (element is ClassElement && isWidgetElement(element)) {
        widgetElements.add(element);
      } else if (element is EnumElement) {
        enumElements.add(element);
      }
      // Anything else (mixins, extensions, top-level functions, non-widget
      // classes) is intentionally out of scope for a *widget* catalog —
      // the original widgets_extractor.dart dumped all of these, but
      // nothing downstream (model.dart, the Rust schema) consumes them.
    }

    // Sorted for deterministic output — same rationale as
    // AnalyzerSession.discoverBarrelFiles: a seed artifact must not
    // reorder itself between runs/machines just because
    // `Map.values` iteration order isn't guaranteed.
    widgetElements.sort((a, b) => (a.name ?? '').compareTo(b.name ?? ''));
    enumElements.sort((a, b) => (a.name ?? '').compareTo(b.name ?? ''));

    final knownWidgetNames = widgetElements
        .map((e) => e.name)
        .whereType<String>()
        .toSet();

    onProgress?.call(
      '📦 Classified ${widgetElements.length} widgets and '
      '${enumElements.length} enums.',
    );

    // --- Pass 2: full extraction ----------------------------------------
    var uncategorizedCount = 0;
    var deprecatedCount = 0;

    final widgetRecords = <WidgetRecord>[];
    for (final element in widgetElements) {
      final categorization = categorize(element);
      if (categorization.isUncategorized) uncategorizedCount++;

      final doc = parseDoc(
        element.documentationComment ?? '',
        knownWidgetNames: knownWidgetNames,
        flutterSrcRoot: flutterSrcRoot,
      );

      final isDeprecated = isElementDeprecated(element);
      if (isDeprecated) deprecatedCount++;

      widgetRecords.add(
        WidgetRecord(
          name: element.name ?? '',
          topLevel: categorization.designSystem.topLevelLabel,
          designSystem: categorization.designSystem.name,
          categories: categorization.categories,
          summary: doc.summary,
          overviewMarkdown: doc.overviewMarkdown,
          isDeprecated: isDeprecated,
          superChain: element.allSupertypes
              .map((t) => t.element.name ?? '')
              .where((name) => name.isNotEmpty)
              .toList(),
          relatedWidgetName: doc.relatedWidgetNameGuess,
          constructors: extractConstructors(element),
          properties: extractProperties(element),
          methods: extractMethods(element),
          codeSamples: doc.codeSamples,
          youtubeUrls: doc.youtubeUrls,
        ),
      );
    }

    final enumRecords = enumElements
        .map(
          (element) => EnumRecord(
            name: element.name ?? '',
            documentation: cleanShortDoc(element.documentationComment ?? ''),
            values: extractEnumValues(element),
          ),
        )
        .toList();

    onProgress?.call(
      '✅ Extraction complete — '
      '$uncategorizedCount/${widgetRecords.length} widgets have no curated '
      'category yet (run bin/scrape_widget_categories.dart to fill these '
      'in), $deprecatedCount flagged @Deprecated.',
    );

    return CatalogOutput(
      generatedAt: DateTime.now().toUtc().toIso8601String(),
      flutterVersionTag: _readEngineVersion(),
      widgets: widgetRecords,
      enums: enumRecords,
    );
  }

  /// Best-effort Flutter version tag, read straight from the SDK checkout
  /// (TRD Section 4.2's `flutter_sdk_version` catalog_meta row) rather than
  /// hardcoded or passed in by hand. `bin/internal/engine.version` holds
  /// the pinned version string on every Flutter checkout old enough to
  /// matter here. Returns null — never throws — if the checkout layout
  /// doesn't match what's expected; this is metadata, not something worth
  /// failing extraction over.
  String? _readEngineVersion() {
    final root = flutterSrcRoot;
    if (root == null) return null;
    final versionFile = File(p.join(root, 'bin', 'internal', 'engine.version'));
    if (!versionFile.existsSync()) return null;
    final content = versionFile.readAsStringSync().trim();
    return content.isEmpty ? null : content;
  }
}
