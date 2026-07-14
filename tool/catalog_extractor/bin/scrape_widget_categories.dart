import 'dart:convert';
import 'dart:io';

/// Regenerates `widget_categories.json` — the curated, external data file
/// [widget_filter.dart]'s `categorize()` reads at extraction time.
///
/// ## Why this exists as a separate script, not part of the main pipeline
///
/// docs.flutter.dev's widget-catalog taxonomy (Basics, Layout, Scrolling,
/// Text, ...) has no equivalent signal in the Flutter SDK source — this was
/// verified directly against current SDK source, not assumed (see
/// `widget_filter.dart`'s doc comment for details). Scraping is genuinely
/// the only source for this data. Keeping that scrape here, run manually /
/// periodically and checked in as a JSON artifact, is a different and much
/// safer trade-off than the old design's *inline, per-run* HTML scraping
/// baked into `catalog_extractor.dart`'s critical path:
///
///   - It can fail, go stale, or rot without blocking `dart run
///     bin/extract_catalog.dart` from working at all.
///   - It's reviewable in a diff (`git diff widget_categories.json`) the way
///     a hand-maintained Dart map is, but without needing a code change or
///     a new release to fix.
///   - Nothing about the main extraction pipeline depends on
///     docs.flutter.dev being reachable at extraction time.
///
/// ## Multi-category accumulation
///
/// The previous implementation used `Map.putIfAbsent`, which keeps only the
/// *first* category a widget was seen under and silently discards the
/// rest — this is why widgets known to appear on multiple catalog pages
/// (`Column`/`Row` under both "Basics" and "Layout", for example) were
/// showing up with only one. This script accumulates into a
/// `Map<String, Set<String>>` instead, so a widget seen on N pages ends up
/// with all N categories.
///
/// ## Usage
///
/// ```bash
/// dart run bin/scrape_widget_categories.dart --output widget_categories.json
/// ```
///
/// Re-run this whenever docs.flutter.dev adds a category page or widget,
/// or on a schedule (e.g. a quarterly CI job that opens a PR with the diff)
/// rather than trusting it to stay accurate indefinitely.
void main(List<String> args) async {
  final outputPath = _argValue(args, '--output') ?? 'widget_categories.json';
  final client = HttpClient();

  stdout.writeln('🚀 Discovering category pages from docs.flutter.dev...');

  final indexUri = Uri.parse('https://docs.flutter.dev/ui/widgets');
  final categorySlugs = <String>{};

  try {
    final request = await client.getUrl(indexUri);
    final response = await request.close();
    if (response.statusCode == 200) {
      final html = await response.transform(utf8.decoder).join();
      final slugRegex = RegExp(
        r"""href=["'](?:https?:\/\/docs\.flutter\.dev)?\/ui\/widgets\/([a-z0-9-]+)\/?["']""",
      );
      for (final match in slugRegex.allMatches(html)) {
        final slug = match.group(1)!;
        if (slug != 'index' && slug.isNotEmpty) categorySlugs.add(slug);
      }
    } else {
      stderr.writeln('❌ Index page returned ${response.statusCode}');
      exitCode = 1;
      return;
    }
  } catch (e) {
    stderr.writeln('❌ Failed to fetch index: $e');
    exitCode = 1;
    return;
  }

  stdout.writeln(
    '✅ Discovered ${categorySlugs.length} category pages: '
    '${categorySlugs.join(', ')}\n',
  );

  // widget name -> set of category labels it appears under. A Set here
  // (not a List, not putIfAbsent) is what makes accumulation-without-
  // duplication automatic and correct.
  final Map<String, Set<String>> categoriesByWidget = {};
  final Map<String, int> countPerPage = {};

  for (final slug in categorySlugs) {
    final pageUri = Uri.parse('https://docs.flutter.dev/ui/widgets/$slug');
    try {
      final request = await client.getUrl(pageUri);
      final response = await request.close();
      if (response.statusCode != 200) continue;

      final html = await response.transform(utf8.decoder).join();

      final titleMatch = RegExp(
        r'<title>(.*?)(?:\s+widgets)?\s*\|\s*Flutter<\/title>',
        caseSensitive: false,
      ).firstMatch(html);
      final categoryLabel = titleMatch != null
          ? titleMatch.group(1)!.trim()
          : slug.replaceAll('-', ' ');

      final widgetRegex = RegExp(r'\/([A-Za-z0-9_]+)-class\.html');
      final matches = widgetRegex.allMatches(html);

      var count = 0;
      for (final match in matches) {
        final widgetName = match.group(1)!;
        // .add(), not putIfAbsent — every page a widget appears on
        // contributes its category label, none are discarded.
        (categoriesByWidget[widgetName] ??= <String>{}).add(categoryLabel);
        count++;
      }
      countPerPage[categoryLabel] = count;
      stdout.writeln('   -> $count widget references under "$categoryLabel"');
    } catch (e) {
      stderr.writeln('❌ Failed to fetch "$slug": $e');
    }
  }

  client.close();

  final output = <String, List<String>>{
    for (final entry in categoriesByWidget.entries)
      entry.key: entry.value.toList()..sort(),
  };

  final encoder = JsonEncoder.withIndent('  ');
  await File(outputPath).writeAsString(encoder.convert(output));

  stdout.writeln(
    '\n🎯 Wrote $outputPath — ${output.length} distinct widgets, '
    '${categoriesByWidget.values.fold<int>(0, (sum, s) => sum + s.length)} '
    'total widget-category memberships across ${categorySlugs.length} pages.',
  );
}

String? _argValue(List<String> args, String flag) {
  final i = args.indexOf(flag);
  if (i == -1 || i + 1 >= args.length) return null;
  return args[i + 1];
}
