import 'dart:io';

import 'package:analyzer/dart/analysis/analysis_context_collection.dart';
import 'package:analyzer/dart/analysis/results.dart';
import 'package:path/path.dart' as p;

/// Wraps `AnalysisContextCollection` setup + deterministic top-level file
/// discovery + public-export-surface resolution for a Flutter SDK checkout.
///
/// This is the ONLY file that touches `package:analyzer`'s analysis
/// context/session machinery directly — `member_extractor.dart` and
/// `widget_filter.dart` only ever see already-resolved `Element`-family
/// objects, never the context/session plumbing itself. That boundary
/// matters for the same reason the Rust side enforces crate boundaries:
/// if analyzer's context-setup API ever changes shape, exactly one file
/// needs to change.
class AnalyzerSession {
  final String flutterPackagePath;
  final AnalysisContextCollection _collection;

  AnalyzerSession._(this.flutterPackagePath, this._collection);

  factory AnalyzerSession.forFlutterSdk(String flutterSdkPath) {
    final flutterPackagePath = p.normalize(
      p.absolute(p.join(flutterSdkPath, 'packages', 'flutter')),
    );
    final collection = AnalysisContextCollection(
      includedPaths: [flutterPackagePath],
    );
    return AnalyzerSession._(flutterPackagePath, collection);
  }

  /// Top-level `.dart` files directly under `lib/` — the public barrel
  /// files (`material.dart`, `cupertino.dart`, `widgets.dart`, etc.) —
  /// NOT a recursive walk of `lib/src/`, since a file's mere presence
  /// under `src/` says nothing about whether it's actually exported.
  ///
  /// Sorted for deterministic, reproducible output: `Directory.listSync()`
  /// makes no ordering guarantee, and without sorting, a symbol re-exported
  /// from two barrels (e.g. `Container` via both `widgets.dart` and
  /// `material.dart`) could resolve to a "first seen" winner that differs
  /// between two runs on different filesystems/OSes — a seed artifact
  /// should never have that kind of run-to-run nondeterminism.
  List<File> discoverBarrelFiles() {
    final libPath = p.join(flutterPackagePath, 'lib');
    final files =
        Directory(libPath)
            .listSync()
            .whereType<File>()
            .where((f) => f.path.endsWith('.dart'))
            .toList()
          ..sort((a, b) => a.path.compareTo(b.path));
    return files;
  }

  /// Resolves every barrel file **one at a time** and, for each, invokes
  /// [onNamespace] immediately with that barrel's public export namespace
  /// — before moving on to resolve the next (often much larger) barrel.
  ///
  /// This replaces an earlier two-phase design that resolved *every*
  /// barrel first, stashed the raw `Element`s from all of them in one
  /// long-lived `Map`, and only walked/extracted from those elements
  /// afterwards, once every barrel — including `material.dart` and
  /// `widgets.dart`, each of which transitively touches most of the SDK —
  /// had already been resolved. `AnalysisContextCollection` does not
  /// guarantee that every `Element` it has ever handed out stays backed by
  /// a live AST forever; resolving dozens of large, overlapping libraries
  /// into the *same* session before touching any of their members is
  /// exactly the access pattern that risks an early barrel's elements
  /// going stale (empty `allSupertypes`, empty `constructors`, etc.) by
  /// the time something reads them minutes later — which manifests as a
  /// widget that is unambiguously public and unambiguously a `Widget`
  /// subclass (e.g. `CupertinoAdaptiveTextSelectionToolbar`) silently
  /// failing `isWidgetElement` and dropping out of the catalog, with
  /// nothing in the logs to explain why.
  ///
  /// Doing the classify-and-extract work for a barrel's namespace
  /// immediately, inside this loop, means every `Element` this tool ever
  /// inspects is read back essentially as soon as the analyzer produced
  /// it — matching how the original single-pass `widgets_extractor.dart`
  /// extracted immediately per file — while still visiting every barrel
  /// exactly once, in the same deterministic sorted order as before.
  ///
  /// [onNamespace] receives the resolved library's `exportNamespace`
  /// directly; callers are responsible for their own cross-barrel
  /// dedup (first barrel in sorted order should win — see
  /// `CatalogExtractor.run()`), since this method deliberately doesn't
  /// hold any state between barrels itself.
  ///
  /// The namespace parameter is deliberately typed `dynamic` rather than
  /// analyzer's own export-namespace class — same rationale as
  /// [belongsToFlutterPackage]'s `Element` parameter being as loosely
  /// typed as the rest of this file allows: the exact class backing
  /// `LibraryElement.exportNamespace` (and whether the member is called
  /// `definedNames` or `definedNames2`) has shifted across analyzer
  /// versions during the "Element2" migration. Every caller immediately
  /// narrows via `.definedNames2.entries` today; if a future analyzer
  /// upgrade renames that getter, exactly one call site (in
  /// `CatalogExtractor.run()`) needs to change.
  Future<void> forEachBarrelFile(
    Future<void> Function(String fileName, dynamic namespace) onNamespace, {
    void Function(String message)? onProgress,
  }) async {
    final context = _collection.contextFor(flutterPackagePath);
    final session = context.currentSession;

    for (final file in discoverBarrelFiles()) {
      final fileName = p.basename(file.path);
      onProgress?.call('Analyzing entry point: $fileName');
      final resolved = await session.getResolvedLibrary(file.path);
      if (resolved is! ResolvedLibraryResult) {
        onProgress?.call('  ⚠ could not fully resolve $fileName');
        continue;
      }
      await onNamespace(fileName, resolved.element.exportNamespace);
    }
  }

  /// Whether [element] is actually declared in `package:flutter` itself
  /// (as opposed to `dart:ui`, another pub package, or an analyzer-only
  /// synthetic library) — the same three-URI-shape check every export
  /// namespace entry needs before it's trusted as real SDK surface.
  ///
  /// Deliberately `dynamic`, not analyzer's `Element`/`Element2` base
  /// type — same version-agnostic rationale as [forEachBarrelFile]'s
  /// `namespace` parameter.
  bool belongsToFlutterPackage(dynamic element) {
    final uri = element.library?.uri.toString() ?? '';
    if (uri.startsWith('dart:')) return false;
    if (uri.startsWith('package:') && !uri.startsWith('package:flutter/')) {
      return false;
    }
    if (uri.startsWith('file:') && !uri.contains('/packages/flutter/lib/')) {
      return false;
    }
    return true;
  }
}
