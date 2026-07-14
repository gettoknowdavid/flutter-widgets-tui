library;

import 'dart:convert';
import 'dart:io';

import 'package:analyzer/dart/element/element.dart';

/// Class detection + categorization.
///
/// Replaces `catalog_extractor.dart`'s original `docs.flutter.dev` HTML
/// scraping *inline in the extraction pipeline*, per the TRD's principle
/// ("SDK source is the authoritative data source") — but see the important
/// caveat below before assuming that principle covers everything.
///
/// This file no longer gates entries on "is this a `Widget` subclass?".
/// Earlier, [isCatalogableClass] (then named `isWidgetElement`) required
/// `Widget` to appear somewhere in `allSupertypes` before a class was
/// included at all — which meant every class the catalog cared about was
/// only ever as complete as that one check, and a class that was
/// genuinely a widget but hit any analyzer edge case in its supertype
/// resolution would simply vanish from the output with nothing logged to
/// explain why. The scope is intentionally wider now: any real,
/// publicly-exported `package:flutter` class is catalogable, matching
/// what `bin/widgets_extractor.dart` (the original single-pass reference
/// implementation) always did. Non-widget classes (`BuildContext`,
/// `GlobalKey`, controllers, etc.) show up alongside widgets now; nothing
/// downstream needs to distinguish them, and `superChain` on each
/// resulting record still shows exactly what a class does or doesn't
/// extend for anyone who wants to filter client-side.
///
/// Categorization has two genuinely different tiers, with different levels
/// of reliability, and it's important not to conflate them:
///
///  1. [DesignSystemTier] (Material / Cupertino / base) is derived from the
///     declaring library's file path — a Material/Cupertino class is
///     always physically under `src/material/` or `src/cupertino/` in the
///     Flutter SDK. This is 100% reliable, structural, and needs zero
///     curation. It will never silently drift the way scraped HTML can.
///
///  2. `categories` (the docs.flutter.dev widget-catalog taxonomy — Basics,
///     Layout, Scrolling, Text, ...) has **no equivalent structural signal
///     anywhere in the SDK source**. This was verified directly against
///     Flutter's current `widgets/icon.dart`: despite `dartdoc` supporting
///     `{@category ...}` doc-comment directives (see
///     https://github.com/flutter/flutter/issues/10344, opened 2017), the
///     SDK does not actually use them. `list_view.dart`, `basic.dart`, and
///     `text.dart` are flat siblings with no folder or annotation saying
///     "this one is Scrolling." docs.flutter.dev's fine-grained taxonomy is
///     hand-curated content that is published *only* as rendered HTML.
///
///     Given that, the robust fix isn't to fake a source-derived signal —
///     it's to stop pretending this file should own that data at all. The
///     categories are now loaded from an external `widget_categories.json`
///     (see [loadCuratedCategories] / [loadCuratedCategoriesFromFile]),
///     produced by the separate, standalone
///     `bin/scrape_widget_categories.dart` script (checked in next to this
///     file). Updating the taxonomy — adding a missing widget, fixing a
///     miscategorization, tracking a new docs.flutter.dev category page —
///     means re-running that script and committing a new JSON file, never
///     touching this Dart code. That is what "future-proof" actually looks
///     like here, as distinct from a map that needs a PR every time Flutter
///     ships a widget. Non-widget classes simply have no entry in that
///     curated file and fall back to `categories: []`, same as an
///     uncurated widget would.
///
///     `categories` is a `List<String>` (not a single string) because most
///     widgets genuinely belong to more than one docs.flutter.dev page —
///     `Column`/`Row` appear under both "Basics" and "Layout", for example.
///     The scraper accumulates every category a widget appears under
///     instead of keeping only the first (see that script's doc comment
///     for why `Map.putIfAbsent`-style "first one wins" logic is exactly
///     the bug that silently dropped a widget's other categories before).

// ---------------------------------------------------------------------------
// Class detection
// ---------------------------------------------------------------------------

/// Whether [element] belongs in the catalog at all. The only remaining
/// disqualifier is [_hasRealSourceFile] — there is deliberately no
/// `Widget`-subtype check here anymore (see this file's top-of-file doc
/// comment for why).
bool isCatalogableClass(ClassElement element) => _hasRealSourceFile(element);

/// Replacement for the deprecated `LibraryElement.isSynthetic` check.
///
/// analyzer 10.0.1 deprecated `LibraryElement.isSynthetic` in favor of
/// `LibraryElement.isOriginNotExistingFile` — `true` means "this library
/// has no backing source file," which is exactly what `isSynthetic` used
/// to signal for libraries. `isOriginNotExistingFile == true` is the
/// synthetic case, so this helper inverts it. Guards against classes
/// attributed to a library the analyzer synthesized rather than resolved
/// from an actual file on disk (e.g. certain error-recovery / augmentation
/// edge cases) — these are not real SDK classes and would otherwise
/// pollute the catalog with meaningless entries.
///
/// Pinned against `analyzer: 13.3.0` per `pubspec.yaml`. If you bump that
/// version, re-verify this against the new version's changelog before
/// trusting it — same caveat this project already applies to `dartdoc`
/// (see this package's README, "Version pinning" section).
bool _hasRealSourceFile(InterfaceElement element) {
  return !element.library.isOriginNotExistingFile;
}

// ---------------------------------------------------------------------------
// Categorization
// ---------------------------------------------------------------------------

/// The one categorization tier that *is* structurally reliable — derived
/// straight from the declaring library's file path, never curated.
enum DesignSystemTier {
  material,
  cupertino,
  base;

  /// Matches the `top_level` strings the previous scraper-based pipeline
  /// produced ("Design Systems" vs "Base Widgets"), so downstream JSON
  /// consumers (`json_writer.dart`, the Rust `xtask seed-catalog` step)
  /// don't need to change their expectations.
  String get topLevelLabel => this == base ? 'Base Widgets' : 'Design Systems';
}

class Categorization {
  final DesignSystemTier designSystem;

  /// The docs.flutter.dev widget-catalog categories this widget appears
  /// under (e.g. `["Layout", "Basics"]`). Empty — not null — when the
  /// widget isn't present in the curated `widget_categories.json`, which
  /// is expected and fine: it just means `bin/scrape_widget_categories.dart`
  /// hasn't captured it yet (it's still findable via [designSystem] and by
  /// name/summary search; it's not dropped from the catalog).
  final List<String> categories;

  const Categorization({required this.designSystem, required this.categories});

  bool get isUncategorized => categories.isEmpty;

  Map<String, dynamic> toJson() => {
    'top_level': designSystem.topLevelLabel,
    'design_system': designSystem.name,
    'categories': categories,
  };

  @override
  String toString() =>
      'Categorization(designSystem: $designSystem, categories: $categories)';
}

DesignSystemTier _designSystemFor(LibraryElement library) {
  final uri = library.uri.toString();
  if (uri.contains('/src/material/')) return DesignSystemTier.material;
  if (uri.contains('/src/cupertino/')) return DesignSystemTier.cupertino;
  return DesignSystemTier.base;
}

Map<String, List<String>> _curatedCategories = {};

/// Loads a `{ "WidgetName": ["Category A", "Category B"] }` map, replacing
/// whatever was previously loaded. Call once, before [categorize], with the
/// contents of `widget_categories.json` (see
/// [loadCuratedCategoriesFromFile] for the common case of loading straight
/// from disk).
void loadCuratedCategories(Map<String, dynamic> raw) {
  _curatedCategories = {
    for (final entry in raw.entries)
      entry.key: List<String>.from(entry.value as List<dynamic>),
  };
}

/// Convenience wrapper: reads and parses `widget_categories.json` (or
/// whatever path you pass) and calls [loadCuratedCategories] with it.
///
/// Missing file is treated as a soft warning, not a fatal error — the
/// pipeline should still run (every widget just falls back to
/// `categories: []`, keeping `design_system` intact) rather than blocking
/// the whole extraction on a maintenance-time data file being stale or
/// absent.
Future<void> loadCuratedCategoriesFromFile(String path) async {
  final file = File(path);
  if (!file.existsSync()) {
    stderr.writeln(
      '⚠️  Curated categories file not found at "$path". Run '
      'bin/scrape_widget_categories.dart to generate it. Proceeding with '
      'empty `categories` for every widget (design_system tier is '
      'unaffected).',
    );
    return;
  }
  final raw = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
  loadCuratedCategories(raw);
}

Categorization categorize(InterfaceElement element) {
  final designSystem = _designSystemFor(element.library);
  final name = element.name ?? '';
  final categories = _curatedCategories[name] ?? const <String>[];
  return Categorization(designSystem: designSystem, categories: categories);
}