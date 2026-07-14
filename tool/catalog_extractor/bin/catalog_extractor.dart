import 'dart:io';

import 'package:args/args.dart';
import 'package:catalog_extractor/catalog_extractor.dart';

/// ```bash
/// dart run bin/extract_catalog.dart \
///   --flutter-src /tmp/flutter-src \
///   --output catalog.json \
///   --categories widget_categories.json
/// ```
///
/// Replaces the old `bin/catalog_extractor.dart`, which did its own HTML
/// scraping of docs.flutter.dev *and* its own regex-based SDK source scan
/// in one file. That responsibility is now split across
/// `bin/scrape_widget_categories.dart` (run separately, occasionally, to
/// produce `widget_categories.json` — see that script's doc comment) and
/// this script (run every time you want a fresh `catalog.json`, using
/// `package:analyzer` exclusively — no network access, fully offline,
/// consistent with NFR-1).
Future<void> main(List<String> arguments) async {
  final parser = ArgParser()
    ..addOption(
      'flutter-src',
      help:
          'Path to a Flutter SDK checkout (the directory containing '
          'packages/ and bin/). Clone at a pinned stable tag first — '
          'see README.md "Setup".',
      mandatory: true,
    )
    ..addOption(
      'output',
      abbr: 'o',
      help: 'Path to write catalog.json to.',
      defaultsTo: 'catalog.json',
    )
    ..addOption(
      'categories',
      help:
          'Path to the curated widget_categories.json produced by '
          'bin/scrape_widget_categories.dart. Missing file is a warning, '
          'not a fatal error — every widget just gets categories: [].',
      defaultsTo: 'widget_categories.json',
    )
    ..addFlag(
      'help',
      abbr: 'h',
      negatable: false,
      help: 'Show this usage information.',
    );

  final ArgResults args;
  try {
    args = parser.parse(arguments);
  } on FormatException catch (e) {
    stderr.writeln('❌ ${e.message}\n');
    stdout.writeln(parser.usage);
    exitCode = 64; // EX_USAGE
    return;
  }

  if (args.flag('help')) {
    stdout.writeln(parser.usage);
    return;
  }

  final flutterSrc = args.option('flutter-src')!;
  final outputPath = args.option('output')!;
  final categoriesPath = args.option('categories')!;

  if (!Directory(flutterSrc).existsSync()) {
    stderr.writeln('❌ --flutter-src path does not exist: $flutterSrc');
    exitCode = 1;
    return;
  }

  final stopwatch = Stopwatch()..start();
  stdout.writeln('🚀 Analyzing Flutter SDK at: $flutterSrc');

  await loadCuratedCategoriesFromFile(categoriesPath);

  final session = AnalyzerSession.forFlutterSdk(flutterSrc);
  final extractor = CatalogExtractor(
    session: session,
    flutterSrcRoot: flutterSrc,
    onProgress: (message) => stdout.writeln(message),
  );

  final CatalogOutput output;
  try {
    output = await extractor.run();
  } catch (e, stackTrace) {
    stderr.writeln('❌ Extraction failed: $e');
    stderr.writeln(stackTrace);
    exitCode = 1;
    return;
  }

  await writeCatalogJson(output, outputPath);
  stopwatch.stop();

  stdout.writeln(
    '\n🎯 Success! Wrote ${output.widgets.length} widgets and '
    '${output.enums.length} enums to $outputPath '
    'in ${stopwatch.elapsed.inSeconds}s.',
  );
}
