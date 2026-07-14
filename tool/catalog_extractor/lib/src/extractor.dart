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
/// this class's only job is sequencing them correctly.
///
/// Sequencing here is deliberately **single-pass per barrel file**:
/// classification + full member/doc extraction for a barrel's exports
/// happens immediately, in [AnalyzerSession.forEachBarrelFile]'s callback,
/// rather than in a separate pass over elements gathered from every
/// barrel up front. See `analyzer_session.dart`'s doc comment for why
/// that matters — it's not just a style choice, it's what fixed widgets
/// like `CupertinoAdaptiveTextSelectionToolbar` silently vanishing from
/// the catalog despite being an unambiguous, unambiguously-exported
/// `Widget` subclass.
///
/// The one thing that genuinely can't happen until every barrel has been
/// visited is [ParsedDoc.seeAlsoCandidates] resolution into a real
/// `related_widget_name` (`[GridView]` in some other widget's doc comment
/// is only worth trusting as a cross-reference once `GridView` is known
/// to be something this run actually catalogued) — that's the one
/// intentional second pass left, and it's pure in-memory data lookup
/// against already-extracted [WidgetRecord]s, not a second round of
/// analyzer element access.
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

    // First barrel (in sorted, deterministic order — see
    // AnalyzerSession.discoverBarrelFiles) to define a given name wins,
    // same cross-barrel dedup rule as before, just tracked here now that
    // resolution and extraction happen in the same pass instead of two.
    final processedNames = <String>{};

    final preliminary = <_PreliminaryWidget>[];
    final enumRecords = <EnumRecord>[];
    var uncategorizedCount = 0;
    var deprecatedCount = 0;
    final extractionFailures = <String>[];

    await session.forEachBarrelFile((fileName, namespace) async {
      for (final entry in namespace.definedNames2.entries) {
        final element = entry.value;
        final name = element.name;
        if (name == null || name.startsWith('_')) continue;
        if (!session.belongsToFlutterPackage(element)) continue;
        if (!processedNames.add(name)) continue;

        try {
          if (element is ClassElement && isCatalogableClass(element)) {
            final categorization = categorize(element);
            if (categorization.isUncategorized) uncategorizedCount++;

            final doc = parseDoc(
              element.documentationComment ?? '',
              flutterSrcRoot: flutterSrcRoot,
            );

            final isDeprecated = isElementDeprecated(element);
            if (isDeprecated) deprecatedCount++;

            preliminary.add(
              _PreliminaryWidget(
                name: name,
                topLevel: categorization.designSystem.topLevelLabel,
                designSystem: categorization.designSystem.name,
                categories: categorization.categories,
                summary: doc.summary,
                overviewMarkdown: doc.overviewMarkdown,
                isDeprecated: isDeprecated,
                superChain: element.allSupertypes
                    .map((t) => t.element.name ?? '')
                    .where((n) => n.isNotEmpty)
                    .toList(),
                seeAlsoCandidates: doc.seeAlsoCandidates,
                constructors: extractConstructors(element),
                properties: extractProperties(element),
                methods: extractMethods(element),
                codeSamples: doc.codeSamples,
                youtubeUrls: doc.youtubeUrls,
              ),
            );
          } else if (element is EnumElement) {
            enumRecords.add(
              EnumRecord(
                name: name,
                documentation: cleanShortDoc(
                  element.documentationComment ?? '',
                ),
                values: extractEnumValues(element),
              ),
            );
          }
          // Anything else (mixins, extensions, top-level functions,
          // non-widget classes) is intentionally out of scope for a
          // *widget* catalog — the original widgets_extractor.dart dumped
          // all of these, but nothing downstream (model.dart, the Rust
          // schema) consumes them.
        } catch (e) {
          // A single element failing to extract (an unresolved supertype,
          // an analyzer edge case on one odd constructor signature, etc.)
          // must never take the rest of the run down with it, and must
          // never disappear silently either — both of which were possible
          // before: an uncaught throw here would abort `run()` entirely
          // (crashing the whole extraction over one widget), while any
          // caller-side swallowing would drop the widget with zero trace
          // of why. Recording it and moving on keeps every other widget
          // in `$fileName` intact and gives the end-of-run summary
          // something concrete to point at instead of a silent gap.
          extractionFailures.add('$name (from $fileName): $e');
        }
      }
    }, onProgress: onProgress);

    // Sorted for deterministic output — a seed artifact must not reorder
    // itself between runs/machines just because iteration order over the
    // export namespaces isn't guaranteed.
    preliminary.sort((a, b) => a.name.compareTo(b.name));
    enumRecords.sort((a, b) => a.name.compareTo(b.name));

    onProgress?.call(
      '📦 Classified ${preliminary.length} widgets and '
      '${enumRecords.length} enums.',
    );

    // --- Second pass: resolve See-also candidates into related_widget_name.
    // Pure lookup against `knownWidgetNames` — no analyzer element access,
    // so this can't reintroduce the staleness problem the single-pass
    // restructuring above fixed.
    final knownWidgetNames = preliminary.map((w) => w.name).toSet();
    final widgetRecords = preliminary.map((w) {
      String? relatedGuess;
      for (final candidate in w.seeAlsoCandidates) {
        if (candidate != w.name && knownWidgetNames.contains(candidate)) {
          relatedGuess = candidate;
          break;
        }
      }
      return w.toWidgetRecord(relatedWidgetName: relatedGuess);
    }).toList();

    if (extractionFailures.isNotEmpty) {
      onProgress?.call(
        '⚠️  ${extractionFailures.length} element(s) failed extraction and '
        'were skipped:',
      );
      for (final failure in extractionFailures) {
        onProgress?.call('   - $failure');
      }
    }

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

/// Everything a [WidgetRecord] needs except `relatedWidgetName`, which
/// can't be known until every barrel has been visited (see [CatalogExtractor.run]).
/// Kept as plain data — no analyzer `Element` reference held past the
/// callback that created it.
class _PreliminaryWidget {
  final String name;
  final String topLevel;
  final String designSystem;
  final List<String> categories;
  final String summary;
  final String overviewMarkdown;
  final bool isDeprecated;
  final List<String> superChain;
  final List<String> seeAlsoCandidates;
  final List<ConstructorRecord> constructors;
  final List<PropertyRecord> properties;
  final List<MethodRecord> methods;
  final List<CodeSampleRecord> codeSamples;
  final List<String> youtubeUrls;

  _PreliminaryWidget({
    required this.name,
    required this.topLevel,
    required this.designSystem,
    required this.categories,
    required this.summary,
    required this.overviewMarkdown,
    required this.isDeprecated,
    required this.superChain,
    required this.seeAlsoCandidates,
    required this.constructors,
    required this.properties,
    required this.methods,
    required this.codeSamples,
    required this.youtubeUrls,
  });

  WidgetRecord toWidgetRecord({required String? relatedWidgetName}) {
    return WidgetRecord(
      name: name,
      topLevel: topLevel,
      designSystem: designSystem,
      categories: categories,
      summary: summary,
      overviewMarkdown: overviewMarkdown,
      isDeprecated: isDeprecated,
      superChain: superChain,
      relatedWidgetName: relatedWidgetName,
      constructors: constructors,
      properties: properties,
      methods: methods,
      codeSamples: codeSamples,
      youtubeUrls: youtubeUrls,
    );
  }
}
