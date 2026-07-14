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

  /// Resolves every barrel file and returns the union of their public
  /// export namespaces, keyed by name and deduplicated so a symbol
  /// re-exported from multiple barrels is only processed once — and,
  /// thanks to [discoverBarrelFiles]'s sort, always resolves to the same
  /// winner across runs and machines.
  ///
  /// Map values are deliberately left untyped (`dynamic`) rather than
  /// annotated with analyzer's element-model base type: that name has
  /// shifted across analyzer versions during the "Element2" migration,
  /// and every consumer of this map immediately narrows via a runtime
  /// `is ClassElement` / `is EnumElement` check anyway (see
  /// `catalog_extractor.dart`), which works identically regardless of the
  /// map's static value type.
  Future<Map<String, dynamic>> resolvePublicExportNamespace({
    void Function(String message)? onProgress,
  }) async {
    final context = _collection.contextFor(flutterPackagePath);
    final session = context.currentSession;
    final result = <String, dynamic>{};

    for (final file in discoverBarrelFiles()) {
      onProgress?.call('Analyzing entry point: ${p.basename(file.path)}');
      final resolved = await session.getResolvedLibrary(file.path);
      if (resolved is! ResolvedLibraryResult) {
        onProgress?.call('  ⚠ could not fully resolve ${file.path}');
        continue;
      }

      final exportNamespace = resolved.element.exportNamespace;
      for (final entry in exportNamespace.definedNames2.entries) {
        final element = entry.value;
        final name = element.name;
        if (name == null || name.startsWith('_')) continue;
        if (!_belongsToFlutterPackage(element)) continue;

        // First barrel (in sorted order) to define a given name wins.
        // Dart itself forbids a class and, say, a top-level function from
        // sharing one name within a single export namespace, so a plain
        // name key (rather than a "kind:name" composite) is sufficient.
        result.putIfAbsent(name, () => element);
      }
    }
    return result;
  }

  bool _belongsToFlutterPackage(dynamic element) {
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
